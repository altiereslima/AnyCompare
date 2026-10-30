use std::{
    fs,
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

pub fn resolve(root: &Path, relative: &Path) -> Result<PathBuf, String> {
    if relative.as_os_str().is_empty()
        || relative
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err("Nome relativo inválido.".into());
    }
    let mut path = root.canonicalize().map_err(|e| e.to_string())?;
    if !path.is_dir() {
        return Err("A pasta base não existe.".into());
    }
    for component in relative.components() {
        path.push(component.as_os_str());
        if let Ok(metadata) = fs::symlink_metadata(&path) {
            #[cfg(windows)]
            let reparse = {
                use std::os::windows::fs::MetadataExt;
                metadata.file_attributes() & 0x400 != 0
            };
            #[cfg(not(windows))]
            let reparse = metadata.file_type().is_symlink();
            if reparse {
                return Err(format!(
                    "Links/junções não são seguidos: {}",
                    path.display()
                ));
            }
        }
    }
    Ok(path)
}

pub fn leaf(name: &str) -> Result<&Path, String> {
    let path = Path::new(name);
    if name.trim().is_empty()
        || path.components().count() != 1
        || !matches!(path.components().next(), Some(Component::Normal(_)))
        || name.contains(['\\', '/'])
    {
        Err("Informe somente o nome, sem caminho.".into())
    } else {
        Ok(path)
    }
}

#[derive(Clone)]
pub struct CopyPlan {
    pub items: Vec<(PathBuf, PathBuf)>,
    pub overwrite_count: usize,
}

impl CopyPlan {
    pub fn build(
        source_root: &Path,
        target_root: &Path,
        selected: &[PathBuf],
    ) -> Result<Self, String> {
        let mut plan = Self {
            items: Vec::new(),
            overwrite_count: 0,
        };
        let mut selected = selected.to_vec();
        selected.sort();
        let mut roots: Vec<PathBuf> = Vec::new();
        for relative in selected {
            if roots.iter().any(|root| relative.starts_with(root)) {
                continue;
            }
            let source = resolve(source_root, &relative)?;
            let target = resolve(target_root, &relative)?;
            let normalized = |path: &Path| {
                let value = path.to_string_lossy().into_owned();
                if cfg!(windows) {
                    value.to_lowercase()
                } else {
                    value
                }
            };
            let source_name = normalized(&source);
            let target_name = normalized(&target);
            if source_name == target_name
                || target_name.starts_with(&(source_name + std::path::MAIN_SEPARATOR_STR))
            {
                return Err("Origem e destino se sobrepõem.".into());
            }
            if source.is_file() {
                plan.push(source, target)?;
            } else if source.is_dir() {
                plan.push(source.clone(), target.clone())?;
                for entry in walkdir::WalkDir::new(&source)
                    .follow_links(false)
                    .min_depth(1)
                {
                    let entry = entry.map_err(|e| e.to_string())?;
                    let child = relative.join(
                        entry
                            .path()
                            .strip_prefix(&source)
                            .map_err(|e| e.to_string())?,
                    );
                    plan.push(resolve(source_root, &child)?, resolve(target_root, &child)?)?;
                }
            } else {
                return Err(format!("Origem não encontrada: {}", source.display()));
            }
            roots.push(relative);
        }
        Ok(plan)
    }
    fn push(&mut self, source: PathBuf, target: PathBuf) -> Result<(), String> {
        if target.exists() && source.is_dir() != target.is_dir() {
            return Err(format!("Arquivo/pasta incompatíveis: {}", target.display()));
        }
        if target.is_file() {
            self.overwrite_count += 1;
        }
        self.items.push((source, target));
        Ok(())
    }
    pub fn execute(&self, cancel: &AtomicBool) -> Result<usize, String> {
        let mut copied = 0;
        for (source, target) in &self.items {
            if cancel.load(Ordering::Relaxed) {
                return Err(format!("Operação cancelada após {copied} arquivos."));
            }
            if source.is_dir() {
                fs::create_dir_all(target).map_err(|e| e.to_string())?;
                continue;
            }
            let parent = target.parent().ok_or("Destino inválido")?;
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            let stamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let temporary = parent.join(format!(
                ".anycompare-copy-{}-{stamp}.tmp",
                std::process::id()
            ));
            let result = (|| -> Result<(), String> {
                let mut input = fs::File::open(source).map_err(|e| e.to_string())?;
                let mut output = fs::OpenOptions::new()
                    .create_new(true)
                    .write(true)
                    .open(&temporary)
                    .map_err(|e| e.to_string())?;
                let mut bytes = [0; 64 * 1024];
                loop {
                    if cancel.load(Ordering::Relaxed) {
                        return Err("Cópia cancelada.".into());
                    }
                    let n = input.read(&mut bytes).map_err(|e| e.to_string())?;
                    if n == 0 {
                        break;
                    }
                    output.write_all(&bytes[..n]).map_err(|e| e.to_string())?;
                }
                if let Ok(modified) = input.metadata().and_then(|m| m.modified()) {
                    output
                        .set_times(fs::FileTimes::new().set_modified(modified))
                        .map_err(|e| e.to_string())?;
                }
                output.sync_all().map_err(|e| e.to_string())?;
                drop(output);
                fs::rename(&temporary, target).map_err(|e| e.to_string())?;
                Ok(())
            })();
            if result.is_err() {
                let _ = fs::remove_file(&temporary);
            }
            result?;
            copied += 1;
        }
        Ok(copied)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nested_directory_copy_preserves_bytes_times_and_empty_folders_without_duplicate_children() {
        let root = std::env::temp_dir().join(format!(
            "anycompare-nested-copy-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let (left, right) = (root.join("left"), root.join("right"));
        for path in [
            left.join("ação/deep"),
            left.join("ação/empty"),
            right.join("ação/deep"),
        ] {
            fs::create_dir_all(path).unwrap();
        }
        fs::write(left.join("ação/deep/item.bin"), b"\0utf8-raw-\xff").unwrap();
        fs::write(right.join("ação/deep/item.bin"), b"previous").unwrap();
        let plan =
            CopyPlan::build(&left, &right, &["ação".into(), "ação/deep/item.bin".into()]).unwrap();
        assert_eq!(plan.overwrite_count, 1);
        assert_eq!(plan.execute(&AtomicBool::new(false)).unwrap(), 1);
        assert_eq!(
            fs::read(left.join("ação/deep/item.bin")).unwrap(),
            fs::read(right.join("ação/deep/item.bin")).unwrap()
        );
        assert!(right.join("ação/empty").is_dir());
        assert_eq!(
            fs::metadata(left.join("ação/deep/item.bin"))
                .unwrap()
                .modified()
                .unwrap(),
            fs::metadata(right.join("ação/deep/item.bin"))
                .unwrap()
                .modified()
                .unwrap()
        );
        for base in [&left, &right] {
            fs::remove_file(base.join("ação/deep/item.bin")).unwrap();
            fs::remove_dir(base.join("ação/deep")).unwrap();
            fs::remove_dir(base.join("ação/empty")).unwrap();
            fs::remove_dir(base.join("ação")).unwrap();
        }
        fs::remove_dir(left).unwrap();
        fs::remove_dir(right).unwrap();
        fs::remove_dir(root).unwrap();
    }

    #[test]
    fn copy_plan_preserves_orphans_overwrites_and_cancels_without_truncating_targets() {
        let root = std::env::temp_dir().join(format!(
            "anycompare-ops-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let left = root.join("left");
        let right = root.join("right");
        fs::create_dir_all(&left).unwrap();
        fs::create_dir(&right).unwrap();
        fs::write(left.join("item"), "ação🦀").unwrap();
        fs::write(right.join("item"), "anterior").unwrap();
        fs::write(left.join("orphan"), "novo").unwrap();
        let plan = CopyPlan::build(&left, &right, &["item".into(), "orphan".into()]).unwrap();
        assert_eq!(plan.overwrite_count, 1);
        assert!(plan.execute(&AtomicBool::new(true)).is_err());
        assert_eq!(fs::read_to_string(right.join("item")).unwrap(), "anterior");
        assert_eq!(plan.execute(&AtomicBool::new(false)).unwrap(), 2);
        assert_eq!(
            fs::read(right.join("item")).unwrap(),
            fs::read(left.join("item")).unwrap()
        );
        assert_eq!(fs::read_to_string(right.join("orphan")).unwrap(), "novo");
        assert!(resolve(&left, Path::new("../outside")).is_err());
        assert!(leaf("../x").is_err());
        assert!(CopyPlan::build(&left, &left, &["item".into()]).is_err());
        for file in [
            left.join("item"),
            left.join("orphan"),
            right.join("item"),
            right.join("orphan"),
        ] {
            fs::remove_file(file).unwrap();
        }
        fs::remove_dir(left).unwrap();
        fs::remove_dir(right).unwrap();
        fs::remove_dir(root).unwrap();
    }
}
