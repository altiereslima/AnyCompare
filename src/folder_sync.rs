//! AnyCompare Folder Sync planner and safe executor.
//!
//! Supports all five BC5 synchronization presets:
//! - `UpdateLeft`, `UpdateRight`, `UpdateBoth`
//! - `MirrorLeft`, `MirrorRight`

use crate::file_ops::{atomic_copy_file, matches_name_filter, resolve, same_file_contents};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};
use walkdir::WalkDir;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SyncPreset {
    UpdateRight,
    UpdateLeft,
    UpdateBoth,
    MirrorRight,
    MirrorLeft,
}

impl SyncPreset {
    pub const ALL: [Self; 5] = [
        Self::UpdateRight,
        Self::UpdateLeft,
        Self::UpdateBoth,
        Self::MirrorRight,
        Self::MirrorLeft,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::UpdateRight => "sync.preset.update_right",
            Self::UpdateLeft => "sync.preset.update_left",
            Self::UpdateBoth => "sync.preset.update_both",
            Self::MirrorRight => "sync.preset.mirror_right",
            Self::MirrorLeft => "sync.preset.mirror_left",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::UpdateRight => {
                "sync.copies_newer_and_orphan_files_from_left_to"
            }
            Self::UpdateLeft => {
                "sync.copies_newer_and_orphan_files_from_right_to"
            }
            Self::UpdateBoth => {
                "sync.copies_newer_and_orphan_files_in_both"
            }
            Self::MirrorRight => {
                "sync.makes_the_right_folder_identical_to_the_left"
            }
            Self::MirrorLeft => {
                "sync.makes_the_left_folder_identical_to_the_right"
            }
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Self::UpdateRight => "imgUpdateRight",
            Self::UpdateLeft => "imgUpdateLeft",
            Self::UpdateBoth => "imgDirSyncSession",
            Self::MirrorRight => "imgMirrorRight",
            Self::MirrorLeft => "imgMirrorLeft",
        }
    }

    pub fn is_mirror(self) -> bool {
        matches!(self, Self::MirrorLeft | Self::MirrorRight)
    }

    pub fn swapped(self) -> Self {
        match self {
            Self::UpdateRight => Self::UpdateLeft,
            Self::UpdateLeft => Self::UpdateRight,
            Self::UpdateBoth => Self::UpdateBoth,
            Self::MirrorRight => Self::MirrorLeft,
            Self::MirrorLeft => Self::MirrorRight,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyncOptions {
    pub preset: SyncPreset,
    pub compare_content: bool,
    pub timestamp_tolerance_secs: u64,
    pub include_empty_dirs: bool,
    pub name_filter: String,
    pub suppress_filters: bool,
}

impl Default for SyncOptions {
    fn default() -> Self {
        Self {
            preset: SyncPreset::UpdateRight,
            compare_content: true,
            timestamp_tolerance_secs: 2,
            include_empty_dirs: true,
            name_filter: "*.*".into(),
            suppress_filters: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SyncEntryStatus {
    Same,
    LeftNewer,
    RightNewer,
    DifferentSameTime,
    LeftOnly,
    RightOnly,
    TypeMismatch,
    Error,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SyncAction {
    None,
    CopyRight,
    CopyLeft,
    DeleteLeft,
    DeleteRight,
    Conflict,
    Skip,
}

impl SyncAction {
    pub fn icon(self) -> &'static str {
        match self {
            Self::None => "imgContentSame",
            Self::CopyRight => "imgSyncCopyRight",
            Self::CopyLeft => "imgSyncCopyLeft",
            Self::DeleteLeft | Self::DeleteRight => "imgSyncDelete",
            Self::Conflict => "imgContentConflict",
            Self::Skip => "imgSyncCancel",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::None => "status.equal",
            Self::CopyRight => "menu.actions.copy_right_short",
            Self::CopyLeft => "menu.actions.copy_left_short",
            Self::DeleteLeft => "sync.delete_left",
            Self::DeleteRight => "sync.delete_right",
            Self::Conflict => "common.conflict",
            Self::Skip => "common.ignore",
        }
    }
}

#[derive(Clone, Debug)]
pub struct SideMeta {
    pub is_dir: bool,
    pub size: u64,
    pub modified_secs: Option<u64>,
}

impl SideMeta {
    fn from_metadata(m: &fs::Metadata) -> Self {
        let modified_secs = m
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs());
        Self {
            is_dir: m.is_dir(),
            size: if m.is_dir() { 0 } else { m.len() },
            modified_secs,
        }
    }
}

#[derive(Clone, Debug)]
pub struct SyncEntry {
    pub relative: PathBuf,
    pub depth: usize,
    pub is_dir: bool,
    pub left: Option<SideMeta>,
    pub right: Option<SideMeta>,
    pub status: SyncEntryStatus,
    pub action: SyncAction,
    pub error: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SyncSummary {
    pub copy_right: usize,
    pub copy_left: usize,
    pub delete_left: usize,
    pub delete_right: usize,
    pub conflicts: usize,
    pub unchanged: usize,
    pub skipped: usize,
    pub errors: usize,
}

impl SyncSummary {
    pub fn total_operations(self) -> usize {
        self.copy_right + self.copy_left + self.delete_left + self.delete_right
    }
}

#[derive(Clone, Debug, Default)]
pub struct SyncExecReport {
    pub copied_right: usize,
    pub copied_left: usize,
    pub deleted_left: usize,
    pub deleted_right: usize,
    pub skipped_conflicts: usize,
    pub errors: Vec<String>,
    pub log: Vec<String>,
    pub cancelled: bool,
}

fn path_key(path: &Path) -> String {
    if cfg!(windows) {
        path.to_string_lossy().to_lowercase()
    } else {
        path.to_string_lossy().into_owned()
    }
}

fn is_reparse(meta: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        meta.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        meta.file_type().is_symlink()
    }
}

fn scan_side(
    root: &Path,
    cancel: &AtomicBool,
    errors: &mut Vec<(PathBuf, String)>,
) -> Result<BTreeMap<String, (PathBuf, SideMeta)>, String> {
    if !root.is_dir() {
        return Err(crate::i18n::tr_fmt(
            "folder.not_found",
            &[("path", &root.display().to_string())],
        ));
    }
    let mut map = BTreeMap::new();
    for entry in WalkDir::new(root).follow_links(false).min_depth(1) {
        if cancel.load(Ordering::Relaxed) {
            return Err(crate::i18n::tr("common.canceled"));
        }
        match entry {
            Ok(e) => {
                let Ok(rel) = e.path().strip_prefix(root) else {
                    continue;
                };
                match fs::symlink_metadata(e.path()) {
                    Ok(meta) => {
                        if is_reparse(&meta) {
                            continue;
                        }
                        map.insert(path_key(rel), (rel.to_path_buf(), SideMeta::from_metadata(&meta)));
                    }
                    Err(err) => errors.push((rel.to_path_buf(), err.to_string())),
                }
            }
            Err(err) => {
                let rel = err
                    .path()
                    .and_then(|p| p.strip_prefix(root).ok())
                    .map(Path::to_path_buf)
                    .unwrap_or_default();
                errors.push((rel, err.to_string()));
            }
        }
    }
    Ok(map)
}

pub fn default_action(
    preset: SyncPreset,
    status: SyncEntryStatus,
    is_dir: bool,
    include_empty_dirs: bool,
) -> SyncAction {
    if is_dir && !include_empty_dirs {
        return match status {
            SyncEntryStatus::LeftOnly if preset == SyncPreset::MirrorLeft => SyncAction::DeleteLeft,
            SyncEntryStatus::RightOnly if preset == SyncPreset::MirrorRight => SyncAction::DeleteRight,
            SyncEntryStatus::TypeMismatch => SyncAction::Conflict,
            _ => SyncAction::None,
        };
    }
    match status {
        SyncEntryStatus::Same => SyncAction::None,
        SyncEntryStatus::Error | SyncEntryStatus::TypeMismatch => SyncAction::Conflict,
        SyncEntryStatus::LeftOnly => match preset {
            SyncPreset::UpdateRight | SyncPreset::UpdateBoth | SyncPreset::MirrorRight => {
                SyncAction::CopyRight
            }
            SyncPreset::UpdateLeft => SyncAction::None,
            SyncPreset::MirrorLeft => SyncAction::DeleteLeft,
        },
        SyncEntryStatus::RightOnly => match preset {
            SyncPreset::UpdateLeft | SyncPreset::UpdateBoth | SyncPreset::MirrorLeft => {
                SyncAction::CopyLeft
            }
            SyncPreset::UpdateRight => SyncAction::None,
            SyncPreset::MirrorRight => SyncAction::DeleteRight,
        },
        SyncEntryStatus::LeftNewer => match preset {
            SyncPreset::UpdateRight | SyncPreset::UpdateBoth | SyncPreset::MirrorRight => {
                SyncAction::CopyRight
            }
            SyncPreset::UpdateLeft => SyncAction::None,
            SyncPreset::MirrorLeft => SyncAction::CopyLeft,
        },
        SyncEntryStatus::RightNewer => match preset {
            SyncPreset::UpdateLeft | SyncPreset::UpdateBoth | SyncPreset::MirrorLeft => {
                SyncAction::CopyLeft
            }
            SyncPreset::UpdateRight => SyncAction::None,
            SyncPreset::MirrorRight => SyncAction::CopyRight,
        },
        SyncEntryStatus::DifferentSameTime => match preset {
            SyncPreset::UpdateRight | SyncPreset::MirrorRight => SyncAction::CopyRight,
            SyncPreset::UpdateLeft | SyncPreset::MirrorLeft => SyncAction::CopyLeft,
            SyncPreset::UpdateBoth => SyncAction::Conflict,
        },
    }
}

pub fn build_sync_plan(
    left_root: &Path,
    right_root: &Path,
    options: &SyncOptions,
    cancel: &AtomicBool,
) -> Result<Vec<SyncEntry>, String> {
    let mut scan_errors = Vec::new();
    let left_map = scan_side(left_root, cancel, &mut scan_errors)?;
    let right_map = scan_side(right_root, cancel, &mut scan_errors)?;

    let mut all_keys: BTreeMap<String, PathBuf> = BTreeMap::new();
    for (k, (rel, _)) in &left_map {
        all_keys.insert(k.clone(), rel.clone());
    }
    for (k, (rel, _)) in &right_map {
        all_keys.entry(k.clone()).or_insert_with(|| rel.clone());
    }

    let mut entries = Vec::with_capacity(all_keys.len());
    for (k, rel) in all_keys {
        if cancel.load(Ordering::Relaxed) {
            return Err(crate::i18n::tr("common.canceled"));
        }
        let l = left_map.get(&k).map(|(_, m)| m.clone());
        let r = right_map.get(&k).map(|(_, m)| m.clone());
        let is_dir = l.as_ref().is_some_and(|m| m.is_dir) || r.as_ref().is_some_and(|m| m.is_dir);

        if !is_dir && !options.suppress_filters && !matches_name_filter(&rel, &options.name_filter) {
            continue;
        }

        let depth = rel.components().count().saturating_sub(1);
        let (status, error) = match (&l, &r) {
            (Some(_), None) => (SyncEntryStatus::LeftOnly, None),
            (None, Some(_)) => (SyncEntryStatus::RightOnly, None),
            (Some(lm), Some(rm)) => {
                if lm.is_dir != rm.is_dir {
                    (SyncEntryStatus::TypeMismatch, None)
                } else if lm.is_dir {
                    (SyncEntryStatus::Same, None)
                } else {
                    let same_bytes = if lm.size == rm.size && options.compare_content {
                        match same_file_contents(&left_root.join(&rel), &right_root.join(&rel), cancel) {
                            Ok(eq) => Some(eq),
                            Err(err) if err == crate::i18n::tr("common.canceled") => return Err(err),
                            Err(err) => {
                                entries.push(SyncEntry {
                                    relative: rel,
                                    depth,
                                    is_dir: false,
                                    left: l,
                                    right: r,
                                    status: SyncEntryStatus::Error,
                                    action: SyncAction::Conflict,
                                    error: Some(err),
                                });
                                continue;
                            }
                        }
                    } else if lm.size == rm.size {
                        match (lm.modified_secs, rm.modified_secs) {
                            (Some(lt), Some(rt))
                                if lt.abs_diff(rt) <= options.timestamp_tolerance_secs =>
                            {
                                Some(true)
                            }
                            _ => Some(false),
                        }
                    } else {
                        Some(false)
                    };

                    if same_bytes == Some(true) {
                        (SyncEntryStatus::Same, None)
                    } else {
                        match (lm.modified_secs, rm.modified_secs) {
                            (Some(lt), Some(rt)) => {
                                if lt > rt.saturating_add(options.timestamp_tolerance_secs) {
                                    (SyncEntryStatus::LeftNewer, None)
                                } else if rt > lt.saturating_add(options.timestamp_tolerance_secs) {
                                    (SyncEntryStatus::RightNewer, None)
                                } else {
                                    (SyncEntryStatus::DifferentSameTime, None)
                                }
                            }
                            _ => (SyncEntryStatus::DifferentSameTime, None),
                        }
                    }
                }
            }
            (None, None) => continue,
        };

        let action = default_action(options.preset, status, is_dir, options.include_empty_dirs);
        entries.push(SyncEntry {
            relative: rel,
            depth,
            is_dir,
            left: l,
            right: r,
            status,
            action,
            error,
        });
    }

    // Sort tree order: folders before files within the same parent, natural order.
    entries.sort_by(|a, b| sort_relative(&a.relative, a.is_dir, &b.relative, b.is_dir));
    Ok(entries)
}

fn sort_relative(a: &Path, a_dir: bool, b: &Path, b_dir: bool) -> std::cmp::Ordering {
    let ac: Vec<String> = a
        .components()
        .map(|c| c.as_os_str().to_string_lossy().to_lowercase())
        .collect();
    let bc: Vec<String> = b
        .components()
        .map(|c| c.as_os_str().to_string_lossy().to_lowercase())
        .collect();
    let common = ac.len().min(bc.len());
    for i in 0..common {
        let a_is_folder = if i + 1 < ac.len() { true } else { a_dir };
        let b_is_folder = if i + 1 < bc.len() { true } else { b_dir };
        if ac[i] != bc[i] {
            if a_is_folder != b_is_folder {
                return b_is_folder.cmp(&a_is_folder);
            }
            return ac[i].cmp(&bc[i]);
        }
    }
    ac.len().cmp(&bc.len())
}

pub fn recompute_actions(entries: &mut [SyncEntry], options: &SyncOptions) {
    for entry in entries {
        entry.action = default_action(
            options.preset,
            entry.status,
            entry.is_dir,
            options.include_empty_dirs,
        );
    }
}

pub fn summarize(entries: &[SyncEntry]) -> SyncSummary {
    let mut s = SyncSummary::default();
    for e in entries {
        if e.status == SyncEntryStatus::Error {
            s.errors += 1;
        }
        match e.action {
            SyncAction::CopyRight => s.copy_right += 1,
            SyncAction::CopyLeft => s.copy_left += 1,
            SyncAction::DeleteLeft => s.delete_left += 1,
            SyncAction::DeleteRight => s.delete_right += 1,
            SyncAction::Conflict => s.conflicts += 1,
            SyncAction::None => s.unchanged += 1,
            SyncAction::Skip => s.skipped += 1,
        }
    }
    s
}

pub fn cycle_entry_action(entry: &mut SyncEntry) {
    let mut choices = Vec::new();
    if entry.left.is_some() {
        choices.push(SyncAction::CopyRight);
    }
    if entry.right.is_some() {
        choices.push(SyncAction::CopyLeft);
    }
    if entry.left.is_some() && entry.right.is_none() {
        choices.push(SyncAction::DeleteLeft);
    }
    if entry.right.is_some() && entry.left.is_none() {
        choices.push(SyncAction::DeleteRight);
    }
    choices.push(SyncAction::Skip);
    if let Some(pos) = choices.iter().position(|c| *c == entry.action) {
        entry.action = choices[(pos + 1) % choices.len()];
    } else if let Some(&first) = choices.first() {
        entry.action = first;
    }
}

fn check_non_overlapping(left_root: &Path, right_root: &Path) -> Result<(), String> {
    let l = left_root
        .canonicalize()
        .map_err(|e| e.to_string())?
        .to_string_lossy()
        .to_lowercase();
    let r = right_root
        .canonicalize()
        .map_err(|e| e.to_string())?
        .to_string_lossy()
        .to_lowercase();
    let sep = std::path::MAIN_SEPARATOR_STR;
    if l == r || l.starts_with(&(r.clone() + sep)) || r.starts_with(&(l + sep)) {
        return Err(crate::i18n::tr("error.source_and_destination_overlap"));
    }
    Ok(())
}

/// Execute all planned sync operations safely (create dirs & copy files first,
/// then delete files, then delete empty directories deepest-first).
pub fn execute_sync(
    left_root: &Path,
    right_root: &Path,
    entries: &[SyncEntry],
    cancel: &AtomicBool,
) -> Result<SyncExecReport, String> {
    check_non_overlapping(left_root, right_root)?;
    let mut report = SyncExecReport::default();

    // Pass 1: Directories and file copies (shallowest first so parent folders exist).
    for entry in entries {
        if cancel.load(Ordering::Relaxed) {
            report.cancelled = true;
            report.log.push(crate::i18n::tr("sync.canceled_log"));
            return Ok(report);
        }
        match entry.action {
            SyncAction::CopyRight => {
                let res = (|| -> Result<(), String> {
                    let src = resolve(left_root, &entry.relative)?;
                    let dst = resolve(right_root, &entry.relative)?;
                    if entry.is_dir {
                        fs::create_dir_all(&dst).map_err(|e| e.to_string())?;
                    } else {
                        atomic_copy_file(&src, &dst, cancel)?;
                    }
                    Ok(())
                })();
                match res {
                    Ok(()) => {
                        report.copied_right += 1;
                        report
                            .log
                            .push(format!("-> {}", entry.relative.display()));
                    }
                    Err(err) => report
                        .errors
                        .push(format!("-> {}: {err}", entry.relative.display())),
                }
            }
            SyncAction::CopyLeft => {
                let res = (|| -> Result<(), String> {
                    let src = resolve(right_root, &entry.relative)?;
                    let dst = resolve(left_root, &entry.relative)?;
                    if entry.is_dir {
                        fs::create_dir_all(&dst).map_err(|e| e.to_string())?;
                    } else {
                        atomic_copy_file(&src, &dst, cancel)?;
                    }
                    Ok(())
                })();
                match res {
                    Ok(()) => {
                        report.copied_left += 1;
                        report
                            .log
                            .push(format!("<- {}", entry.relative.display()));
                    }
                    Err(err) => report
                        .errors
                        .push(format!("<- {}: {err}", entry.relative.display())),
                }
            }
            SyncAction::Conflict => {
                report.skipped_conflicts += 1;
            }
            _ => {}
        }
    }

    // Pass 2: Deletions (files before dirs, deepest path first so parent dirs become empty).
    let mut deletions: Vec<&SyncEntry> = entries
        .iter()
        .filter(|e| matches!(e.action, SyncAction::DeleteLeft | SyncAction::DeleteRight))
        .collect();
    deletions.sort_by(|a, b| {
        a.is_dir
            .cmp(&b.is_dir)
            .then_with(|| b.depth.cmp(&a.depth))
    });

    for entry in deletions {
        if cancel.load(Ordering::Relaxed) {
            report.cancelled = true;
            report.log.push(crate::i18n::tr("sync.canceled_log"));
            return Ok(report);
        }
        let is_left = entry.action == SyncAction::DeleteLeft;
        let root = if is_left { left_root } else { right_root };
        let res = (|| -> Result<(), String> {
            let target = resolve(root, &entry.relative)?;
            if !target.exists() {
                return Ok(());
            }
            if target.is_dir() {
                fs::remove_dir(&target).map_err(|e| e.to_string())?;
            } else {
                fs::remove_file(&target).map_err(|e| e.to_string())?;
            }
            Ok(())
        })();
        match res {
            Ok(()) => {
                if is_left {
                    report.deleted_left += 1;
                    report
                        .log
                        .push(format!("X [L] {}", entry.relative.display()));
                } else {
                    report.deleted_right += 1;
                    report
                        .log
                        .push(format!("X [R] {}", entry.relative.display()));
                }
            }
            Err(err) => report
                .errors
                .push(format!("X {}: {err}", entry.relative.display())),
        }
    }

    Ok(report)
}

pub fn format_time(secs: Option<u64>) -> String {
    let Some(secs) = secs else {
        return String::new();
    };
    let dt = chrono::DateTime::<chrono::Utc>::from_timestamp(secs as i64, 0)
        .map(|d| d.with_timezone(&chrono::Local));
    dt.map(|d| d.format("%d/%m/%Y %H:%M").to_string())
        .unwrap_or_default()
}

pub fn timestamp_now_hms() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    chrono::DateTime::<chrono::Utc>::from_timestamp(secs, 0)
        .map(|d| d.with_timezone(&chrono::Local).format("%H:%M:%S").to_string())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn temp_pair(tag: &str) -> (PathBuf, PathBuf, PathBuf) {
        let root = std::env::temp_dir().join(format!(
            "anycompare-sync-{tag}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let left = root.join("left");
        let right = root.join("right");
        fs::create_dir_all(&left).unwrap();
        fs::create_dir_all(&right).unwrap();
        (root, left, right)
    }

    fn write_with_age(path: &Path, content: &[u8], age_secs: u64) {
        if let Some(p) = path.parent() {
            fs::create_dir_all(p).unwrap();
        }
        fs::write(path, content).unwrap();
        let t = SystemTime::now() - Duration::from_secs(age_secs);
        let f = fs::OpenOptions::new().write(true).open(path).unwrap();
        f.set_times(fs::FileTimes::new().set_modified(t)).unwrap();
    }

    #[test]
    fn update_right_and_mirror_right_handle_newer_older_orphans_and_deletions() {
        let (root, left, right) = temp_pair("mirror");
        write_with_age(&left.join("same.txt"), b"hello", 100);
        write_with_age(&right.join("same.txt"), b"hello", 100);
        write_with_age(&left.join("newer_l.txt"), b"v2-left", 10);
        write_with_age(&right.join("newer_l.txt"), b"v1-right", 200);
        write_with_age(&left.join("only_l.txt"), b"orphan-l", 50);
        write_with_age(&right.join("only_r.txt"), b"orphan-r", 50);

        let cancel = AtomicBool::new(false);
        let opts = SyncOptions {
            preset: SyncPreset::UpdateRight,
            ..Default::default()
        };
        let plan = build_sync_plan(&left, &right, &opts, &cancel).unwrap();
        let sum = summarize(&plan);
        assert_eq!(sum.copy_right, 2); // newer_l + only_l
        assert_eq!(sum.delete_right, 0); // UpdateRight never deletes
        assert_eq!(sum.unchanged, 2); // same + only_r

        // Switch to MirrorRight -> deletes only_r too!
        let mut mirror_plan = plan.clone();
        let mirror_opts = SyncOptions {
            preset: SyncPreset::MirrorRight,
            ..opts
        };
        recompute_actions(&mut mirror_plan, &mirror_opts);
        let msum = summarize(&mirror_plan);
        assert_eq!(msum.copy_right, 2);
        assert_eq!(msum.delete_right, 1);

        let report = execute_sync(&left, &right, &mirror_plan, &cancel).unwrap();
        assert!(report.errors.is_empty());
        assert_eq!(report.copied_right, 2);
        assert_eq!(report.deleted_right, 1);
        assert_eq!(fs::read_to_string(right.join("newer_l.txt")).unwrap(), "v2-left");
        assert_eq!(fs::read_to_string(right.join("only_l.txt")).unwrap(), "orphan-l");
        assert!(!right.join("only_r.txt").exists());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn update_both_flags_same_timestamp_different_content_as_conflict() {
        let (root, left, right) = temp_pair("both");
        write_with_age(&left.join("both.txt"), b"version A", 60);
        write_with_age(&right.join("both.txt"), b"version B", 60);
        write_with_age(&left.join("l_only.txt"), b"from left", 60);
        write_with_age(&right.join("r_only.txt"), b"from right", 60);

        let cancel = AtomicBool::new(false);
        let opts = SyncOptions {
            preset: SyncPreset::UpdateBoth,
            ..Default::default()
        };
        let plan = build_sync_plan(&left, &right, &opts, &cancel).unwrap();
        let sum = summarize(&plan);
        assert_eq!(sum.conflicts, 1);
        assert_eq!(sum.copy_right, 1);
        assert_eq!(sum.copy_left, 1);

        let report = execute_sync(&left, &right, &plan, &cancel).unwrap();
        assert_eq!(report.skipped_conflicts, 1);
        assert_eq!(report.copied_right, 1);
        assert_eq!(report.copied_left, 1);
        // Conflict file remains untouched on both sides.
        assert_eq!(fs::read_to_string(left.join("both.txt")).unwrap(), "version A");
        assert_eq!(fs::read_to_string(right.join("both.txt")).unwrap(), "version B");
        assert_eq!(fs::read_to_string(right.join("l_only.txt")).unwrap(), "from left");
        assert_eq!(fs::read_to_string(left.join("r_only.txt")).unwrap(), "from right");

        let _ = fs::remove_dir_all(root);
    }
}
