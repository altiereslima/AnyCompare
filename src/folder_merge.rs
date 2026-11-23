//! AnyCompare 3-Way Folder Merge planner and safe executor.
//!
//! Compares `left` and `right` directories against a common `center` (base)
//! directory, classifies every relative path into Unchanged / LeftChanged /
//! RightChanged / BothSame / Conflict, and merges approved items into `output`.

use crate::file_ops::{atomic_copy_file, matches_name_filter, resolve, same_file_contents};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::UNIX_EPOCH,
};
use walkdir::WalkDir;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FolderMergeStatus {
    Unchanged,
    LeftChanged,
    RightChanged,
    BothSame,
    Conflict,
    Error,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FolderMergeAction {
    None,
    TakeLeft,
    TakeRight,
    TakeCenter,
    DeleteOutput,
    Conflict,
    Skip,
}

impl FolderMergeAction {
    pub fn label(self) -> &'static str {
        match self {
            Self::None => "status.equal",
            Self::TakeLeft => "merge.take_left",
            Self::TakeRight => "merge.take_right",
            Self::TakeCenter => "merge.take_center",
            Self::DeleteOutput => "merge.delete_from_output",
            Self::Conflict => "common.conflict",
            Self::Skip => "common.ignore",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Self::None => "imgContentSame",
            Self::TakeLeft => "imgContentMergeableLeft",
            Self::TakeRight => "imgContentMergeableRight",
            Self::TakeCenter => "imgShowCenter",
            Self::DeleteOutput => "imgContentDelete",
            Self::Conflict => "imgContentConflict",
            Self::Skip => "imgSyncCancel",
        }
    }
}

#[derive(Clone, Debug)]
pub struct FolderSideMeta {
    pub is_dir: bool,
    pub size: u64,
    pub modified_secs: Option<u64>,
}

impl FolderSideMeta {
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
pub struct FolderMergeEntry {
    pub relative: PathBuf,
    pub depth: usize,
    pub is_dir: bool,
    pub left: Option<FolderSideMeta>,
    pub center: Option<FolderSideMeta>,
    pub right: Option<FolderSideMeta>,
    pub status: FolderMergeStatus,
    pub action: FolderMergeAction,
    pub error: Option<String>,
}

#[derive(Clone, Debug)]
pub struct FolderMergeOptions {
    pub compare_content: bool,
    pub timestamp_tolerance_secs: u64,
    pub name_filter: String,
    pub suppress_filters: bool,
}

impl Default for FolderMergeOptions {
    fn default() -> Self {
        Self {
            compare_content: true,
            timestamp_tolerance_secs: 2,
            name_filter: "*.*".into(),
            suppress_filters: false,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FolderMergeSummary {
    pub unchanged: usize,
    pub left_changes: usize,
    pub right_changes: usize,
    pub both_same: usize,
    pub conflicts: usize,
    pub mergeable: usize,
    pub errors: usize,
}

#[derive(Clone, Debug, Default)]
pub struct FolderMergeExecReport {
    pub copied_from_left: usize,
    pub copied_from_right: usize,
    pub copied_from_center: usize,
    pub deleted_output: usize,
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

fn scan_dir_optional(
    root: Option<&Path>,
    cancel: &AtomicBool,
) -> Result<BTreeMap<String, (PathBuf, FolderSideMeta)>, String> {
    let Some(root) = root else {
        return Ok(BTreeMap::new());
    };
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
        if let Ok(e) = entry {
            let Ok(rel) = e.path().strip_prefix(root) else {
                continue;
            };
            if let Ok(meta) = fs::symlink_metadata(e.path()) {
                if is_reparse(&meta) {
                    continue;
                }
                map.insert(
                    path_key(rel),
                    (rel.to_path_buf(), FolderSideMeta::from_metadata(&meta)),
                );
            }
        }
    }
    Ok(map)
}

fn sides_equal(
    root_a: Option<&Path>,
    meta_a: Option<&FolderSideMeta>,
    root_b: Option<&Path>,
    meta_b: Option<&FolderSideMeta>,
    rel: &Path,
    opts: &FolderMergeOptions,
    cancel: &AtomicBool,
) -> Result<bool, String> {
    match (meta_a, meta_b) {
        (None, None) => Ok(true),
        (Some(_), None) | (None, Some(_)) => Ok(false),
        (Some(a), Some(b)) => {
            if a.is_dir != b.is_dir {
                return Ok(false);
            }
            if a.is_dir {
                return Ok(true);
            }
            if a.size != b.size {
                return Ok(false);
            }
            if opts.compare_content {
                if let (Some(ra), Some(rb)) = (root_a, root_b) {
                    return same_file_contents(&ra.join(rel), &rb.join(rel), cancel);
                }
            }
            match (a.modified_secs, b.modified_secs) {
                (Some(ta), Some(tb)) => Ok(ta.abs_diff(tb) <= opts.timestamp_tolerance_secs),
                _ => Ok(true),
            }
        }
    }
}

pub fn build_folder_merge_plan(
    left_root: &Path,
    center_root: Option<&Path>,
    right_root: &Path,
    options: &FolderMergeOptions,
    cancel: &AtomicBool,
) -> Result<Vec<FolderMergeEntry>, String> {
    let left_map = scan_dir_optional(Some(left_root), cancel)?;
    let center_map = scan_dir_optional(center_root, cancel)?;
    let right_map = scan_dir_optional(Some(right_root), cancel)?;

    let mut all_keys: BTreeMap<String, PathBuf> = BTreeMap::new();
    for map in [&left_map, &center_map, &right_map] {
        for (k, (rel, _)) in map {
            all_keys.entry(k.clone()).or_insert_with(|| rel.clone());
        }
    }

    let mut entries = Vec::with_capacity(all_keys.len());
    for (k, rel) in all_keys {
        if cancel.load(Ordering::Relaxed) {
            return Err(crate::i18n::tr("common.canceled"));
        }
        let l = left_map.get(&k).map(|(_, m)| m.clone());
        let c = center_map.get(&k).map(|(_, m)| m.clone());
        let r = right_map.get(&k).map(|(_, m)| m.clone());
        let is_dir = l.as_ref().is_some_and(|m| m.is_dir)
            || c.as_ref().is_some_and(|m| m.is_dir)
            || r.as_ref().is_some_and(|m| m.is_dir);

        if !is_dir && !options.suppress_filters && !matches_name_filter(&rel, &options.name_filter) {
            continue;
        }
        let depth = rel.components().count().saturating_sub(1);

        let (status, action, error) = if center_root.is_some() {
            let l_eq_c = sides_equal(
                Some(left_root),
                l.as_ref(),
                center_root,
                c.as_ref(),
                &rel,
                options,
                cancel,
            );
            let r_eq_c = sides_equal(
                Some(right_root),
                r.as_ref(),
                center_root,
                c.as_ref(),
                &rel,
                options,
                cancel,
            );
            let l_eq_r = sides_equal(
                Some(left_root),
                l.as_ref(),
                Some(right_root),
                r.as_ref(),
                &rel,
                options,
                cancel,
            );
            match (l_eq_c, r_eq_c, l_eq_r) {
                (Ok(lc), Ok(rc), Ok(lr)) => {
                    if lc && rc {
                        (FolderMergeStatus::Unchanged, FolderMergeAction::None, None)
                    } else if !lc && rc {
                        let act = if l.is_some() {
                            FolderMergeAction::TakeLeft
                        } else {
                            FolderMergeAction::DeleteOutput
                        };
                        (FolderMergeStatus::LeftChanged, act, None)
                    } else if lc && !rc {
                        let act = if r.is_some() {
                            FolderMergeAction::TakeRight
                        } else {
                            FolderMergeAction::DeleteOutput
                        };
                        (FolderMergeStatus::RightChanged, act, None)
                    } else if lr {
                        let act = if l.is_some() {
                            FolderMergeAction::TakeLeft
                        } else {
                            FolderMergeAction::DeleteOutput
                        };
                        (FolderMergeStatus::BothSame, act, None)
                    } else {
                        (
                            FolderMergeStatus::Conflict,
                            FolderMergeAction::Conflict,
                            None,
                        )
                    }
                }
                (Err(e), _, _) | (_, Err(e), _) | (_, _, Err(e)) => {
                    if e == crate::i18n::tr("common.canceled") {
                        return Err(e);
                    }
                    (
                        FolderMergeStatus::Error,
                        FolderMergeAction::Conflict,
                        Some(e),
                    )
                }
            }
        } else {
            // 2-way merge without center: equal = unchanged, orphan = take existing side, diff = conflict.
            match (&l, &r) {
                (Some(_), None) => (
                    FolderMergeStatus::LeftChanged,
                    FolderMergeAction::TakeLeft,
                    None,
                ),
                (None, Some(_)) => (
                    FolderMergeStatus::RightChanged,
                    FolderMergeAction::TakeRight,
                    None,
                ),
                (Some(_), Some(_)) => match sides_equal(
                    Some(left_root),
                    l.as_ref(),
                    Some(right_root),
                    r.as_ref(),
                    &rel,
                    options,
                    cancel,
                ) {
                    Ok(true) => (FolderMergeStatus::Unchanged, FolderMergeAction::None, None),
                    Ok(false) => (
                        FolderMergeStatus::Conflict,
                        FolderMergeAction::Conflict,
                        None,
                    ),
                    Err(e) if e == crate::i18n::tr("common.canceled") => return Err(e),
                    Err(e) => (
                        FolderMergeStatus::Error,
                        FolderMergeAction::Conflict,
                        Some(e),
                    ),
                },
                (None, None) => continue,
            }
        };

        entries.push(FolderMergeEntry {
            relative: rel,
            depth,
            is_dir,
            left: l,
            center: c,
            right: r,
            status,
            action,
            error,
        });
    }

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

pub fn summarize(entries: &[FolderMergeEntry]) -> FolderMergeSummary {
    let mut s = FolderMergeSummary::default();
    for e in entries {
        match e.status {
            FolderMergeStatus::Unchanged => s.unchanged += 1,
            FolderMergeStatus::LeftChanged => s.left_changes += 1,
            FolderMergeStatus::RightChanged => s.right_changes += 1,
            FolderMergeStatus::BothSame => s.both_same += 1,
            FolderMergeStatus::Conflict => s.conflicts += 1,
            FolderMergeStatus::Error => s.errors += 1,
        }
        if matches!(
            e.action,
            FolderMergeAction::TakeLeft
                | FolderMergeAction::TakeRight
                | FolderMergeAction::TakeCenter
                | FolderMergeAction::DeleteOutput
        ) {
            s.mergeable += 1;
        }
    }
    s
}

/// Resolve a path to a canonical comparison key even when the leaf directory
/// (such as a fresh `output_root`) does not exist on disk yet.
fn canonicalize_allowing_missing(path: &Path) -> Result<PathBuf, String> {
    let abs = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|e| e.to_string())?
            .join(path)
    };
    // Lexically normalize `.` and `..` before walking up to an existing ancestor,
    // so paths like `C:\Left\new_sub\..` resolve to `C:\Left`.
    let mut normalized = PathBuf::new();
    for comp in abs.components() {
        match comp {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                if !normalized.pop() {
                    normalized.push(comp.as_os_str());
                }
            }
            other => normalized.push(other.as_os_str()),
        }
    }

    let mut existing = normalized.as_path();
    let mut missing_tail: Vec<&std::ffi::OsStr> = Vec::new();
    while !existing.exists() {
        let Some(name) = existing.file_name() else {
            break;
        };
        missing_tail.push(name);
        let Some(parent) = existing.parent() else {
            break;
        };
        existing = parent;
    }

    let mut base = if existing.exists() {
        existing.canonicalize().map_err(|e| e.to_string())?
    } else {
        existing.to_path_buf()
    };
    for name in missing_tail.into_iter().rev() {
        base.push(name);
    }
    Ok(base)
}

fn normalized_cmp_key(path: &Path) -> String {
    let mut s = path.to_string_lossy().into_owned();
    let sep = std::path::MAIN_SEPARATOR;
    while s.len() > 1 && s.ends_with(sep) && !s.ends_with(&format!(":{sep}")) {
        s.pop();
    }
    if cfg!(windows) {
        s.to_lowercase()
    } else {
        s
    }
}

fn is_strict_descendant(child_key: &str, parent_key: &str) -> bool {
    if child_key == parent_key {
        return false;
    }
    let sep = std::path::MAIN_SEPARATOR;
    if parent_key.ends_with(sep) {
        child_key.starts_with(parent_key)
    } else {
        child_key.starts_with(parent_key)
            && child_key.as_bytes().get(parent_key.len()) == Some(&(sep as u8))
    }
}

/// Validates that `output_root` is never nested inside an input folder and never
/// contains an input folder (matching AnyCompare, which forbids nested
/// source/destination trees while allowing an explicit in-place merge onto an
/// exact base folder).
pub fn check_merge_output_safety(
    left_root: &Path,
    center_root: Option<&Path>,
    right_root: &Path,
    output_root: &Path,
) -> Result<(), String> {
    if output_root.is_file() {
        return Err(crate::i18n::tr("error.invalid_destination"));
    }
    let out_canon = canonicalize_allowing_missing(output_root)?;
    let out_key = normalized_cmp_key(&out_canon);

    for input in [Some(left_root), center_root, Some(right_root)]
        .into_iter()
        .flatten()
    {
        let in_canon = canonicalize_allowing_missing(input)?;
        let in_key = normalized_cmp_key(&in_canon);
        if is_strict_descendant(&out_key, &in_key) || is_strict_descendant(&in_key, &out_key) {
            return Err(crate::i18n::tr("error.source_and_destination_overlap"));
        }
    }
    Ok(())
}

/// Returns the translation key of the input side (`merge.left`, `merge.center`,
/// or `merge.right`) when `output_root` resolves to the exact same folder as one
/// of the merge inputs (in-place merge, supported by AnyCompare).
pub fn output_matches_input_side(
    left_root: &Path,
    center_root: Option<&Path>,
    right_root: &Path,
    output_root: &Path,
) -> Option<&'static str> {
    let out_key = normalized_cmp_key(&canonicalize_allowing_missing(output_root).ok()?);
    for (side_key, root) in [
        ("merge.left", Some(left_root)),
        ("merge.center", center_root),
        ("merge.right", Some(right_root)),
    ] {
        if let Some(root) = root {
            if let Ok(in_canon) = canonicalize_allowing_missing(root) {
                if normalized_cmp_key(&in_canon) == out_key {
                    return Some(side_key);
                }
            }
        }
    }
    None
}

/// Merge entries into `output_root` (copies unchanged base/left files too when `include_unchanged`
/// is true so `output_root` receives a complete merged directory tree).
///
/// Follows AnyCompare's safe execution model:
/// 1. Rejects any nested overlap between `output_root` and `left`/`center`/`right`
///    before creating `output_root` or touching any file.
/// 2. Pass 1: creates directories and atomically copies files (shallowest-first).
/// 3. Pass 2: deletes files first, then removes only empty directories deepest-first
///    (`fs::remove_dir`, never blind `fs::remove_dir_all`).
pub fn execute_folder_merge(
    left_root: &Path,
    center_root: Option<&Path>,
    right_root: &Path,
    output_root: &Path,
    entries: &[FolderMergeEntry],
    include_unchanged: bool,
    cancel: &AtomicBool,
) -> Result<FolderMergeExecReport, String> {
    check_merge_output_safety(left_root, center_root, right_root, output_root)?;
    fs::create_dir_all(output_root).map_err(|e| e.to_string())?;
    let mut report = FolderMergeExecReport::default();

    // Pass 1: Directories and file copies (shallowest-first so parent folders exist).
    let mut deletions: Vec<&FolderMergeEntry> = Vec::new();
    for entry in entries {
        if cancel.load(Ordering::Relaxed) {
            report.cancelled = true;
            report.log.push("Mesclagem cancelada.".into());
            return Ok(report);
        }
        let effective = if entry.action == FolderMergeAction::None && include_unchanged {
            if entry.left.is_some() {
                FolderMergeAction::TakeLeft
            } else if entry.right.is_some() {
                FolderMergeAction::TakeRight
            } else if entry.center.is_some() {
                FolderMergeAction::TakeCenter
            } else {
                FolderMergeAction::None
            }
        } else {
            entry.action
        };

        match effective {
            FolderMergeAction::TakeLeft => {
                if let Err(err) =
                    copy_entry(left_root, output_root, &entry.relative, entry.is_dir, cancel)
                {
                    report
                        .errors
                        .push(format!("[L] {}: {err}", entry.relative.display()));
                } else {
                    report.copied_from_left += 1;
                    report
                        .log
                        .push(format!("L -> {}", entry.relative.display()));
                }
            }
            FolderMergeAction::TakeRight => {
                if let Err(err) =
                    copy_entry(right_root, output_root, &entry.relative, entry.is_dir, cancel)
                {
                    report
                        .errors
                        .push(format!("[R] {}: {err}", entry.relative.display()));
                } else {
                    report.copied_from_right += 1;
                    report
                        .log
                        .push(format!("R -> {}", entry.relative.display()));
                }
            }
            FolderMergeAction::TakeCenter => {
                if let Some(c_root) = center_root {
                    if let Err(err) =
                        copy_entry(c_root, output_root, &entry.relative, entry.is_dir, cancel)
                    {
                        report
                            .errors
                            .push(format!("[C] {}: {err}", entry.relative.display()));
                    } else {
                        report.copied_from_center += 1;
                        report
                            .log
                            .push(format!("C -> {}", entry.relative.display()));
                    }
                }
            }
            FolderMergeAction::DeleteOutput => {
                deletions.push(entry);
            }
            FolderMergeAction::Conflict => {
                report.skipped_conflicts += 1;
            }
            FolderMergeAction::None | FolderMergeAction::Skip => {}
        }
    }

    // Pass 2: Deletions (files before dirs, deepest path first so parent dirs become empty;
    // uses `fs::remove_dir` instead of `remove_dir_all` so non-empty folders with skipped,
    // filtered, or conflicted files are never silently destroyed).
    deletions.sort_by(|a, b| {
        a.is_dir
            .cmp(&b.is_dir)
            .then_with(|| b.depth.cmp(&a.depth))
    });
    for entry in deletions {
        if cancel.load(Ordering::Relaxed) {
            report.cancelled = true;
            report.log.push("Mesclagem cancelada.".into());
            return Ok(report);
        }
        let res = (|| -> Result<(), String> {
            let target = resolve(output_root, &entry.relative)?;
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
                report.deleted_output += 1;
                report.log.push(format!("X {}", entry.relative.display()));
            }
            Err(err) => report
                .errors
                .push(format!("X {}: {err}", entry.relative.display())),
        }
    }

    Ok(report)
}

fn copy_entry(
    src_root: &Path,
    dst_root: &Path,
    relative: &Path,
    is_dir: bool,
    cancel: &AtomicBool,
) -> Result<(), String> {
    let src = resolve(src_root, relative)?;
    let dst = resolve(dst_root, relative)?;
    if let (Ok(sc), Ok(dc)) = (src.canonicalize(), dst.canonicalize()) {
        if sc == dc {
            return Ok(());
        }
    }
    if is_dir {
        fs::create_dir_all(&dst).map_err(|e| e.to_string())
    } else {
        atomic_copy_file(&src, &dst, cancel)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::SystemTime;

    #[test]
    fn three_way_folder_merge_classifies_and_writes_clean_output() {
        let root = std::env::temp_dir().join(format!(
            "anycompare-fmerge-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let left = root.join("left");
        let center = root.join("center");
        let right = root.join("right");
        let output = root.join("output");
        for d in [&left, &center, &right] {
            fs::create_dir_all(d).unwrap();
        }
        // 1. unchanged
        for d in [&left, &center, &right] {
            fs::write(d.join("same.txt"), "unchanged").unwrap();
        }
        // 2. changed only on left
        fs::write(center.join("left_mod.txt"), "base").unwrap();
        fs::write(left.join("left_mod.txt"), "from left").unwrap();
        fs::write(right.join("left_mod.txt"), "base").unwrap();
        // 3. changed only on right
        fs::write(center.join("right_mod.txt"), "base").unwrap();
        fs::write(left.join("right_mod.txt"), "base").unwrap();
        fs::write(right.join("right_mod.txt"), "from right").unwrap();
        // 4. deleted on left -> delete from output
        fs::write(center.join("removed.txt"), "old").unwrap();
        fs::write(right.join("removed.txt"), "old").unwrap();
        // 5. conflict (modified differently on both sides)
        fs::write(center.join("conflict.txt"), "base").unwrap();
        fs::write(left.join("conflict.txt"), "left version").unwrap();
        fs::write(right.join("conflict.txt"), "right version").unwrap();

        let cancel = AtomicBool::new(false);
        let plan = build_folder_merge_plan(
            &left,
            Some(&center),
            &right,
            &FolderMergeOptions::default(),
            &cancel,
        )
        .unwrap();
        let s = summarize(&plan);
        assert_eq!(s.unchanged, 1);
        assert_eq!(s.left_changes, 2); // left_mod + removed
        assert_eq!(s.right_changes, 1);
        assert_eq!(s.conflicts, 1);

        let report = execute_folder_merge(
            &left,
            Some(&center),
            &right,
            &output,
            &plan,
            true,
            &cancel,
        )
        .unwrap();
        assert!(report.errors.is_empty());
        assert_eq!(report.skipped_conflicts, 1);
        assert_eq!(fs::read_to_string(output.join("same.txt")).unwrap(), "unchanged");
        assert_eq!(fs::read_to_string(output.join("left_mod.txt")).unwrap(), "from left");
        assert_eq!(fs::read_to_string(output.join("right_mod.txt")).unwrap(), "from right");
        assert!(!output.join("removed.txt").exists());
        assert!(!output.join("conflict.txt").exists());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn nested_output_is_rejected_before_creating_dir_while_bc5_in_place_and_safe_delete_work() {
        let root = std::env::temp_dir().join(format!(
            "anycompare-fmerge-safety-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let left = root.join("left");
        let center = root.join("center");
        let right = root.join("right");
        for d in [&left, &center, &right] {
            fs::create_dir_all(d.join("sub")).unwrap();
        }
        // Unchanged on left, deleted on right -> DeleteOutput for sub/old.txt and sub
        fs::write(center.join("sub/old.txt"), "base").unwrap();
        fs::write(left.join("sub/old.txt"), "base").unwrap();
        // Extra local file inside left/sub (not in center or right) -> must NOT be wiped by remove_dir_all
        fs::write(left.join("sub/keep_local.txt"), "precious").unwrap();
        // Modified only on right -> TakeRight
        fs::write(center.join("updated.txt"), "v1").unwrap();
        fs::write(left.join("updated.txt"), "v1").unwrap();
        fs::write(right.join("updated.txt"), "v2").unwrap();
        // Remove sub on right so right deleted both sub/old.txt and sub/
        fs::remove_dir_all(right.join("sub")).unwrap();

        let cancel = AtomicBool::new(false);
        let plan = build_folder_merge_plan(
            &left,
            Some(&center),
            &right,
            &FolderMergeOptions::default(),
            &cancel,
        )
        .unwrap();

        // 1. Nested non-existing output inside left, center, or right must fail BEFORE create_dir_all
        for nested in [
            left.join("nested_out"),
            center.join("deep").join("nested_out"),
            right.join("nested_out"),
            root.clone(), // ancestor containing left/center/right
        ] {
            let err = execute_folder_merge(
                &left,
                Some(&center),
                &right,
                &nested,
                &plan,
                true,
                &cancel,
            )
            .expect_err("nested or ancestor output must be rejected");
            assert!(!err.is_empty());
            if nested != root {
                assert!(
                    !nested.exists(),
                    "must not create nested output directory before rejecting: {}",
                    nested.display()
                );
            }
        }

        // 2. Exact in-place merge onto left (BC5 behavior): detected by output_matches_input_side,
        //    applies right's change & deletion, and preserves untracked/filtered files inside sub/
        //    because DeleteOutput uses ordered remove_dir instead of blind remove_dir_all.
        assert_eq!(
            output_matches_input_side(&left, Some(&center), &right, &left),
            Some("merge.left")
        );
        // Simulate a filtered/untracked file in left/sub by adding one after planning:
        fs::write(left.join("sub/filtered_extra.tmp"), "untracked").unwrap();
        // Mark `sub` as DeleteOutput to verify non-empty directory is NOT wiped recursively:
        let mut plan_with_dir_del = plan.clone();
        if let Some(sub_dir) = plan_with_dir_del
            .iter_mut()
            .find(|e| e.is_dir && e.relative == Path::new("sub"))
        {
            sub_dir.action = FolderMergeAction::DeleteOutput;
        }
        let report = execute_folder_merge(
            &left,
            Some(&center),
            &right,
            &left,
            &plan_with_dir_del,
            true,
            &cancel,
        )
        .unwrap();
        assert_eq!(fs::read_to_string(left.join("updated.txt")).unwrap(), "v2");
        assert!(!left.join("sub/old.txt").exists());
        assert!(
            left.join("sub/keep_local.txt").exists(),
            "local file in sub/ must survive"
        );
        assert!(
            left.join("sub/filtered_extra.tmp").exists(),
            "untracked file in non-empty sub/ must never be wiped by remove_dir_all"
        );
        assert!(
            !report.errors.is_empty(),
            "non-empty dir removal should report cleanly instead of destroying contents"
        );

        // 3. Missing source file must report an error (not silently succeed when both src and dst are absent)
        let ghost_out = root.join("ghost_out");
        let ghost_entry = FolderMergeEntry {
            relative: PathBuf::from("missing_source.txt"),
            depth: 0,
            is_dir: false,
            left: Some(FolderSideMeta {
                is_dir: false,
                size: 10,
                modified_secs: None,
            }),
            center: None,
            right: None,
            status: FolderMergeStatus::LeftChanged,
            action: FolderMergeAction::TakeLeft,
            error: None,
        };
        let ghost_report = execute_folder_merge(
            &left,
            Some(&center),
            &right,
            &ghost_out,
            &[ghost_entry],
            false,
            &cancel,
        )
        .unwrap();
        assert_eq!(ghost_report.copied_from_left, 0);
        assert_eq!(
            ghost_report.errors.len(),
            1,
            "missing source file must be reported as an error"
        );

        let _ = fs::remove_dir_all(root);
    }
}
