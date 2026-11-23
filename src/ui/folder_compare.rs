use super::{
    folder_ops::{
        self, CompareResultRow, CompareResultsState, CompareRowStatus, ContentCompareMode,
        DosAttributesSpec, FileContentVerdict, FolderOpConfirmed, FolderOpModal,
        FolderStructureMode, RenameMode, SelectionStats, TouchMode,
    },
    toolbar::{ToolEvent, ToolState},
};
use crate::folder_source::{self, EntryMeta};
use crate::history::{RecentSession, SessionKind};
use crate::ui::{icons::IconManager, theme::BcTheme};
use egui::*;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    thread,
    time::{Instant, SystemTime},
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Status {
    Same,
    Different,
    LeftOnly,
    RightOnly,
    Error,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DisplayFilter {
    All,
    Diff,
    NoOrphan,
    DiffNoOrphan,
    Orphan,
    LeftNewer,
    RightNewer,
    LeftNewerOrphan,
    RightNewerOrphan,
    LeftOrphan,
    RightOrphan,
    Same,
}

impl DisplayFilter {
    fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "FavCompareAll" => Self::All,
            "FavDiff" => Self::Diff,
            "FavNoOrphan" => Self::NoOrphan,
            "FavDiffNoOrphan" => Self::DiffNoOrphan,
            "FavOrphan" => Self::Orphan,
            "FavLeftNewer" => Self::LeftNewer,
            "FavRightNewer" => Self::RightNewer,
            "FavLeftNewerOrphan" => Self::LeftNewerOrphan,
            "FavRightNewerOrphan" => Self::RightNewerOrphan,
            "FavLeftOrphan" => Self::LeftOrphan,
            "FavRightOrphan" => Self::RightOrphan,
            "FavSame" => Self::Same,
            _ => return None,
        })
    }

    fn as_name(self) -> &'static str {
        match self {
            Self::All => "FavCompareAll",
            Self::Diff => "FavDiff",
            Self::NoOrphan => "FavNoOrphan",
            Self::DiffNoOrphan => "FavDiffNoOrphan",
            Self::Orphan => "FavOrphan",
            Self::LeftNewer => "FavLeftNewer",
            Self::RightNewer => "FavRightNewer",
            Self::LeftNewerOrphan => "FavLeftNewerOrphan",
            Self::RightNewerOrphan => "FavRightNewerOrphan",
            Self::LeftOrphan => "FavLeftOrphan",
            Self::RightOrphan => "FavRightOrphan",
            Self::Same => "FavSame",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DirHandling {
    ShowAll,
    CompareStructure,
    CompareFilesOnly,
    Flatten,
}

impl DirHandling {
    fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "DirShowAll" => Self::ShowAll,
            "DirCompareStructure" => Self::CompareStructure,
            "DirCompareFilesOnly" => Self::CompareFilesOnly,
            "DirFlatten" => Self::Flatten,
            _ => return None,
        })
    }

    fn as_name(self) -> &'static str {
        match self {
            Self::ShowAll => "DirShowAll",
            Self::CompareStructure => "DirCompareStructure",
            Self::CompareFilesOnly => "DirCompareFilesOnly",
            Self::Flatten => "DirFlatten",
        }
    }
}
/// Default name filter of the original toolbar combo ("Filtros: *.*").
const DEFAULT_FILTER: &str = "*.*";
const ROW_HEIGHT: f32 = 16.0;
const ORPHAN_COLOR: Color32 = Color32::from_rgb(128, 0, 255);
/// Shared geometry for column headings, clipping and values in each pane.
struct FolderColumns {
    name: Rect,
    size: Rect,
    modified: Rect,
}

impl FolderColumns {
    fn new(pane: Rect) -> Self {
        let date_x = pane.right() - 112.0;
        let size_x = date_x - 80.0;
        Self {
            name: Rect::from_min_max(pane.min, pos2(size_x, pane.bottom())),
            size: Rect::from_min_max(pos2(size_x, pane.top()), pos2(date_x, pane.bottom())),
            modified: Rect::from_min_max(pos2(date_x, pane.top()), pane.max),
        }
    }
}

fn grouped_size(bytes: u64) -> String {
    let digits = bytes.to_string();
    let separator = if crate::i18n::language().starts_with("pt") {
        '.'
    } else {
        ','
    };
    let mut formatted = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            formatted.push(separator);
        }
        formatted.push(digit);
    }
    formatted
}

fn volume_label(bytes: u64) -> String {
    if bytes < 1024 {
        return format!("{bytes} bytes");
    }
    let mut value = bytes as f64;
    let mut suffix = "KB";
    for unit in ["KB", "MB", "GB", "TB"] {
        value /= 1024.0;
        suffix = unit;
        if value < 1024.0 {
            break;
        }
    }
    let number = if matches!(suffix, "GB" | "TB") {
        let s = format!("{value:.2}");
        s.strip_suffix(".00").unwrap_or(&s).to_owned()
    } else {
        let s = format!("{value:.1}");
        s.strip_suffix(".0").unwrap_or(&s).to_owned()
    };
    let number = if crate::i18n::language().starts_with("pt") {
        number.replace('.', ",")
    } else {
        number
    };
    format!("{number} {suffix}")
}

fn disk_space(path: &Path) -> Option<(u64, String)> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        let query_dir = if path.is_dir() {
            path
        } else {
            path.parent().unwrap_or(path)
        };
        let path = query_dir.canonicalize().ok()?;
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        let mut available = 0;
        // SAFETY: the path is terminated and the output points to a live u64.
        let success = unsafe {
            windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW(
                wide.as_ptr(),
                &mut available,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        let root = path
            .components()
            .next()?
            .as_os_str()
            .to_string_lossy()
            .replace("\\\\?\\", "");
        (success != 0).then_some((available, format!("{root}\\")))
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        None
    }
}

fn modified_secs(metadata: &Option<EntryMeta>) -> Option<u64> {
    metadata
        .as_ref()?
        .modified()?
        .duration_since(SystemTime::UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs())
}

fn entry_color(entry: &Entry, left: bool, ignore_minor: bool) -> Color32 {
    if is_dir(entry) {
        // Native folder names remain black; the two icon halves describe the sides.
        return Color32::BLACK;
    }
    if entry.ignored || (ignore_minor && entry.is_minor) {
        return Color32::BLACK;
    }
    if entry.content_compared && entry.status == Status::Same {
        // Explicit content comparison overrides quick-test timestamp differences.
        return Color32::BLACK;
    }
    match entry.status {
        Status::LeftOnly | Status::RightOnly => ORPHAN_COLOR,
        Status::Error => Color32::DARK_RED,
        _ => {
            let (own, other) = if left {
                (modified_secs(&entry.left), modified_secs(&entry.right))
            } else {
                (modified_secs(&entry.right), modified_secs(&entry.left))
            };
            match (own, other) {
                (Some(a), Some(b)) if a > b => Color32::RED,
                (Some(a), Some(b)) if a < b => Color32::GRAY,
                _ if entry.status == Status::Different => Color32::RED,
                _ => Color32::BLACK,
            }
        }
    }
}

fn entry_matches_display_filter(entry: &Entry, filter: DisplayFilter, ignore_minor: bool) -> bool {
    let status = if entry.ignored || (ignore_minor && entry.is_minor) {
        Status::Same
    } else {
        entry.status
    };
    let (l_secs, r_secs) = (modified_secs(&entry.left), modified_secs(&entry.right));
    let left_newer = match (l_secs, r_secs) {
        (Some(l), Some(r)) => l > r,
        _ => false,
    };
    let right_newer = match (l_secs, r_secs) {
        (Some(l), Some(r)) => r > l,
        _ => false,
    };
    match filter {
        DisplayFilter::All => true,
        DisplayFilter::Diff => status != Status::Same,
        DisplayFilter::NoOrphan => !matches!(status, Status::LeftOnly | Status::RightOnly),
        DisplayFilter::DiffNoOrphan => matches!(status, Status::Different | Status::Error),
        DisplayFilter::Orphan => matches!(status, Status::LeftOnly | Status::RightOnly),
        DisplayFilter::LeftNewer => status == Status::Different && left_newer,
        DisplayFilter::RightNewer => status == Status::Different && right_newer,
        DisplayFilter::LeftNewerOrphan => {
            (status == Status::Different && left_newer) || status == Status::LeftOnly
        }
        DisplayFilter::RightNewerOrphan => {
            (status == Status::Different && right_newer) || status == Status::RightOnly
        }
        DisplayFilter::LeftOrphan => status == Status::LeftOnly,
        DisplayFilter::RightOrphan => status == Status::RightOnly,
        DisplayFilter::Same => status == Status::Same,
    }
}

fn is_dir(entry: &Entry) -> bool {
    entry
        .left
        .iter()
        .chain(entry.right.iter())
        .any(|metadata| metadata.is_dir())
}

fn path_key(path: &Path) -> String {
    if cfg!(windows) {
        path.to_string_lossy().to_lowercase()
    } else {
        path.to_string_lossy().into_owned()
    }
}

fn name_order(left: &std::ffi::OsStr, right: &std::ffi::OsStr) -> std::cmp::Ordering {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        let left: Vec<u16> = left.encode_wide().chain(Some(0)).collect();
        let right: Vec<u16> = right.encode_wide().chain(Some(0)).collect();
        // StrCmpLogicalW only reads the two terminated UTF-16 buffers. It gives
        // the native logical ordering observed for accents and item2/item10.
        unsafe { windows_sys::Win32::UI::Shell::StrCmpLogicalW(left.as_ptr(), right.as_ptr()) }
            .cmp(&0)
    }
    #[cfg(not(windows))]
    {
        left.cmp(right)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct EntryKey {
    relative: PathBuf,
    directory: bool,
}
/// Directories precede files at each level; a directory's descendants stay
/// together, even when the other side has a file with that directory's name.
fn tree_order(a: &Entry, b: &Entry) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let left: Vec<_> = a.relative.components().collect();
    let right: Vec<_> = b.relative.components().collect();
    for (index, (l, r)) in left.iter().zip(&right).enumerate() {
        let l_name = path_key(Path::new(l.as_os_str()));
        let r_name = path_key(Path::new(r.as_os_str()));
        if l_name != r_name {
            let l_dir = index + 1 < left.len() || is_dir(a);
            let r_dir = index + 1 < right.len() || is_dir(b);
            return r_dir
                .cmp(&l_dir)
                .then_with(|| name_order(l.as_os_str(), r.as_os_str()));
        }
    }
    if left.len() < right.len() {
        if is_dir(a) {
            Ordering::Less
        } else {
            Ordering::Greater
        }
    } else if left.len() > right.len() {
        if is_dir(b) {
            Ordering::Greater
        } else {
            Ordering::Less
        }
    } else {
        is_dir(b)
            .cmp(&is_dir(a))
            .then_with(|| a.relative.cmp(&b.relative))
    }
}
/// File masks separated by ';' with '*' and '?' (case-insensitive, file names only;
/// folders stay visible). Without wildcards the text filters by substring of the path.
fn name_filter_matches(filter: &str, entry: &Entry) -> bool {
    let filter = filter.trim();
    if filter.is_empty() {
        return true;
    }
    if !filter.contains(['*', '?']) {
        return entry
            .relative
            .to_string_lossy()
            .to_lowercase()
            .contains(&filter.to_lowercase());
    }
    if is_dir(entry) {
        return true;
    }
    let name = entry
        .relative
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    filter
        .split(';')
        .map(str::trim)
        .filter(|mask| !mask.is_empty())
        .any(|mask| mask == "*.*" || wildcard(&mask.to_lowercase(), &name))
}

fn wildcard(mask: &str, name: &str) -> bool {
    let (mask, name): (Vec<char>, Vec<char>) = (mask.chars().collect(), name.chars().collect());
    let (mut m, mut n, mut star, mut mark) = (0, 0, None, 0);
    while n < name.len() {
        if m < mask.len() && (mask[m] == '?' || mask[m] == name[n]) {
            m += 1;
            n += 1;
        } else if m < mask.len() && mask[m] == '*' {
            star = Some(m);
            mark = n;
            m += 1;
        } else if let Some(s) = star {
            m = s + 1;
            mark += 1;
            n = mark;
        } else {
            return false;
        }
    }
    mask[m..].iter().all(|&c| c == '*')
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct DirSideSummary {
    bytes: u64,
    has_newer_or_diff: bool,
    has_older: bool,
    has_orphan: bool,
}

impl DirSideSummary {
    /// AnyCompare two-half folder icon colors (`imgFolderTop`, `imgFolderBottom`):
    /// red = newer or differing content, purple = orphan, gray = older, yellow = identical.
    fn icon_halves(self, own_orphan: bool) -> (Color32, Color32) {
        const SAME_YELLOW: Color32 = Color32::from_rgb(192, 160, 0);
        const DIFF_RED: Color32 = Color32::from_rgb(215, 25, 25);
        const OLDER_GRAY: Color32 = Color32::from_gray(140);
        if own_orphan {
            return (ORPHAN_COLOR, ORPHAN_COLOR);
        }
        match (self.has_newer_or_diff, self.has_orphan, self.has_older) {
            (true, true, _) => (DIFF_RED, ORPHAN_COLOR),
            (true, false, true) => (DIFF_RED, OLDER_GRAY),
            (true, false, false) => (DIFF_RED, DIFF_RED),
            (false, true, true) => (ORPHAN_COLOR, OLDER_GRAY),
            (false, true, false) => (ORPHAN_COLOR, ORPHAN_COLOR),
            (false, false, true) => (OLDER_GRAY, OLDER_GRAY),
            (false, false, false) => (SAME_YELLOW, SAME_YELLOW),
        }
    }
}

struct Entry {
    id: EntryKey,
    relative: PathBuf,
    right_relative: PathBuf,
    left: Option<EntryMeta>,
    right: Option<EntryMeta>,
    status: Status,
    is_minor: bool,
    content_compared: bool,
    ignored: bool,
    left_crc32: Option<u32>,
    right_crc32: Option<u32>,
    error: Option<String>,
    dir_summary: [DirSideSummary; 2],
}

struct Scan {
    entries: Vec<Entry>,
    errors: Vec<String>,
}

struct ContentCompareItemUpdate {
    id: EntryKey,
    status: Status,
    is_minor: bool,
    left_crc32: Option<u32>,
    right_crc32: Option<u32>,
    error: Option<String>,
}

struct ContentCompareOutcome {
    updates: Vec<ContentCompareItemUpdate>,
    results: CompareResultsState,
    show_results: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum QuickSyncPreset {
    UpdateLeft,
    UpdateRight,
    UpdateBoth,
    MirrorLeft,
    MirrorRight,
}

#[derive(Clone, Debug)]
enum ContextAction {
    OpenFolder(bool),
    OpenSubfolders(bool),
    SetAsBaseFolders,
    OpenInNewView,
    OpenFileDefault,
    OpenWith(SessionKind),
    OpenWithAssocApp,
    CompareContents,
    CopyToSide(bool),
    MoveToSide(bool),
    CopyToFolder(bool),
    MoveToFolder(bool),
    Delete,
    Rename,
    Attributes,
    Touch,
    Exclude,
    ClearExclusions,
    NewFolder,
    CopyFilename,
    ToggleIgnored,
    RefreshSelection,
    QuickSync(QuickSyncPreset),
    ExplorerSelect,
    ExplorerOpenFolder,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct FolderCompareCriteria {
    compare_size: bool,
    compare_timestamp: bool,
    timestamp_tolerance_secs: u64,
    ignore_dst: bool,
    ignore_timezone: bool,
    compare_filename_case: bool,
    align_different_extensions: bool,
    align_unicode_normalization: bool,
    compare_attr_archive: bool,
    compare_attr_system: bool,
    compare_attr_hidden: bool,
    compare_attr_readonly: bool,
    compare_unix_permissions: bool,
    compare_unix_owner: bool,
    compare_unix_group: bool,
    compare_content: bool,
    content_mode: ContentCompareMode,
    compare_versions: bool,
    skip_if_quick_same: bool,
    override_quick_results: bool,
    follow_symlinks: bool,
}

impl Default for FolderCompareCriteria {
    fn default() -> Self {
        Self {
            compare_size: true,
            compare_timestamp: true,
            timestamp_tolerance_secs: 2,
            ignore_dst: false,
            ignore_timezone: false,
            compare_filename_case: false,
            align_different_extensions: false,
            align_unicode_normalization: true,
            compare_attr_archive: false,
            compare_attr_system: false,
            compare_attr_hidden: false,
            compare_attr_readonly: false,
            compare_unix_permissions: false,
            compare_unix_owner: false,
            compare_unix_group: false,
            compare_content: true,
            content_mode: ContentCompareMode::Binary,
            compare_versions: false,
            skip_if_quick_same: false,
            override_quick_results: true,
            follow_symlinks: false,
        }
    }
}

pub enum FolderAction {
    None,
    Home,
    OpenFiles(PathBuf, PathBuf),
}
pub struct FolderCompareScreen {
    pub left_path: String,
    pub right_path: String,
    description: String,
    entries: Vec<Entry>,
    errors: Vec<String>,
    filter: String,
    display_filter: DisplayFilter,
    dir_handling: DirHandling,
    ignore_minor: bool,
    compare_content: bool,
    content_mode: ContentCompareMode,
    criteria: FolderCompareCriteria,
    archives_as_folders: bool,
    exclude_protected_os: bool,
    min_size_kb: u64,
    show_folder_sizes: bool,
    auto_expand_subfolders: bool,
    rules_scope: super::session_settings::FolderScopeChoice,
    show_compare_results: bool,
    dont_show_compare_dialog: bool,
    dont_show_copy_dialog: bool,
    dont_show_move_dialog: bool,
    last_touch_time: Option<SystemTime>,
    exclude_patterns: Vec<String>,
    ignored_paths: BTreeSet<String>,
    pending: Option<mpsc::Receiver<Scan>>,
    pending_content: Option<mpsc::Receiver<ContentCompareOutcome>>,
    cancel: Arc<AtomicBool>,
    loaded_left: PathBuf,
    loaded_right: PathBuf,
    log: Vec<String>,
    selected: BTreeSet<EntryKey>,
    anchor_selected: Option<EntryKey>,
    active_left: bool,
    show_log: bool,
    show_toolbar: bool,
    collapsed: BTreeSet<String>,
    tree_roots: (PathBuf, PathBuf),
    recent_paths: Vec<String>,
    root_history: Vec<(String, String)>,
    root_cursor: usize,
    free_space: [Option<(u64, String)>; 2],
    scroll_to_selected: bool,
    find_text: String,
    selection_focus: bool,
    suppress_filters: bool,
    modal: Option<FolderOpModal>,
}

impl FolderCompareScreen {
    fn show_tree(&self) -> bool {
        self.dir_handling != DirHandling::Flatten
    }

    pub fn set_compare_files_only(&mut self, files_only: bool) {
        self.dir_handling = if files_only {
            DirHandling::CompareFilesOnly
        } else {
            DirHandling::CompareStructure
        };
    }

    #[cfg(test)]
    pub fn is_compare_files_only(&self) -> bool {
        self.dir_handling == DirHandling::CompareFilesOnly
    }

    pub fn select_newer(&mut self, left: bool, right: bool) {
        self.selected = self
            .visible_indices()
            .into_iter()
            .map(|i| &self.entries[i])
            .filter(|e| {
                !is_dir(e)
                    && ((left
                        && entry_matches_display_filter(
                            e,
                            DisplayFilter::LeftNewer,
                            self.ignore_minor,
                        ))
                        || (right
                            && entry_matches_display_filter(
                                e,
                                DisplayFilter::RightNewer,
                                self.ignore_minor,
                            )))
            })
            .map(|e| e.id.clone())
            .collect();
    }

    pub fn select_orphans(&mut self, left: bool, right: bool) {
        self.selected = self
            .visible_indices()
            .into_iter()
            .map(|i| &self.entries[i])
            .filter(|e| {
                (left && e.status == Status::LeftOnly)
                    || (right && e.status == Status::RightOnly)
            })
            .map(|e| e.id.clone())
            .collect();
    }

    #[cfg(test)]
    pub fn selected_count(&self) -> usize {
        self.selected.len()
    }

    pub fn full_refresh(&mut self) {
        self.tree_roots = Default::default();
        self.start_scan();
    }

    pub fn set_selected_as_base_side(&mut self, other_side: bool) {
        let Some(entry) = self
            .entries
            .iter()
            .find(|e| self.selected.contains(&e.id) && is_dir(e))
        else {
            return;
        };
        let from_left = self.active_left;
        let to_left = if other_side { !from_left } else { from_left };
        let resolved = if from_left {
            entry
                .left
                .as_ref()
                .and_then(|_| crate::file_ops::resolve(&self.loaded_left, &entry.relative).ok())
        } else {
            entry
                .right
                .as_ref()
                .and_then(|_| {
                    crate::file_ops::resolve(&self.loaded_right, &entry.right_relative).ok()
                })
        };
        if let Some(p) = resolved {
            if to_left {
                self.left_path = p.to_string_lossy().into_owned();
            } else {
                self.right_path = p.to_string_lossy().into_owned();
            }
            self.start_scan();
        }
    }

    fn open_compare_info_modal(&self, ctx: &Context) {
        let same = self
            .entries
            .iter()
            .filter(|e| !is_dir(e) && e.status == Status::Same)
            .count();
        let diff = self
            .entries
            .iter()
            .filter(|e| !is_dir(e) && e.status == Status::Different)
            .count();
        let left_orphans = self
            .entries
            .iter()
            .filter(|e| !is_dir(e) && e.status == Status::LeftOnly)
            .count();
        let right_orphans = self
            .entries
            .iter()
            .filter(|e| !is_dir(e) && e.status == Status::RightOnly)
            .count();
        let rows = vec![
            (crate::i18n::tr("folder.side_left"), self.left_path.clone()),
            (crate::i18n::tr("folder.side_right"), self.right_path.clone()),
            (
                crate::i18n::tr("common.same"),
                same.to_string(),
            ),
            (
                crate::i18n::tr("common.differences"),
                diff.to_string(),
            ),
            (
                crate::i18n::tr("folder.orphans"),
                format!("{} / {}", left_orphans, right_orphans),
            ),
            (
                crate::i18n::tr("common.errors"),
                self.errors.len().to_string(),
            ),
        ];
        super::info_modals::open_info(ctx, crate::i18n::tr("folder.compare_info"), rows);
    }

    fn current_criteria(&self) -> FolderCompareCriteria {
        let mut c = self.criteria.clone();
        c.compare_content = self.compare_content;
        c.content_mode = self.content_mode;
        c
    }

    pub fn rules_draft(&self) -> super::session_settings::FolderRulesDraft {
        use super::session_settings::FolderHandlingChoice;
        let dir_handling = match self.dir_handling {
            DirHandling::ShowAll => FolderHandlingChoice::ShowAll,
            DirHandling::CompareStructure => FolderHandlingChoice::CompareStructure,
            DirHandling::CompareFilesOnly => FolderHandlingChoice::CompareFilesOnly,
            DirHandling::Flatten => FolderHandlingChoice::Flatten,
        };
        super::session_settings::FolderRulesDraft {
            left_path: self.left_path.clone(),
            right_path: self.right_path.clone(),
            description: self.description.clone(),
            compare_size: self.criteria.compare_size,
            compare_timestamp: self.criteria.compare_timestamp,
            timestamp_tolerance_secs: self.criteria.timestamp_tolerance_secs,
            ignore_dst: self.criteria.ignore_dst,
            ignore_timezone: self.criteria.ignore_timezone,
            compare_filename_case: self.criteria.compare_filename_case,
            align_different_extensions: self.criteria.align_different_extensions,
            align_unicode_normalization: self.criteria.align_unicode_normalization,
            compare_attr_archive: self.criteria.compare_attr_archive,
            compare_attr_system: self.criteria.compare_attr_system,
            compare_attr_hidden: self.criteria.compare_attr_hidden,
            compare_attr_readonly: self.criteria.compare_attr_readonly,
            compare_unix_permissions: self.criteria.compare_unix_permissions,
            compare_unix_owner: self.criteria.compare_unix_owner,
            compare_unix_group: self.criteria.compare_unix_group,
            compare_content: self.compare_content,
            content_mode: self.content_mode,
            compare_versions: self.criteria.compare_versions,
            skip_if_quick_same: self.criteria.skip_if_quick_same,
            override_quick_results: self.criteria.override_quick_results,
            dir_handling,
            archives_as_folders: self.archives_as_folders,
            follow_symlinks: self.criteria.follow_symlinks,
            filter: self.filter.clone(),
            exclude_patterns: self.exclude_patterns.join("; "),
            suppress_filters: self.suppress_filters,
            only_diffs: self.display_filter == DisplayFilter::Diff,
            only_same: self.display_filter == DisplayFilter::Same,
            exclude_protected_os: self.exclude_protected_os,
            min_size_kb: self.min_size_kb,
            show_log: self.show_log,
            show_folder_sizes: self.show_folder_sizes,
            auto_expand_subfolders: self.auto_expand_subfolders,
            scope: self.rules_scope,
        }
    }

    pub fn apply_rules(&mut self, draft: super::session_settings::FolderRulesDraft) {
        use super::session_settings::FolderHandlingChoice;
        let prev_criteria = self.current_criteria();
        let paths_changed =
            self.left_path != draft.left_path || self.right_path != draft.right_path;
        self.left_path = draft.left_path;
        self.right_path = draft.right_path;
        self.description = draft.description;
        self.compare_content = draft.compare_content;
        self.content_mode = draft.content_mode;
        self.criteria = FolderCompareCriteria {
            compare_size: draft.compare_size,
            compare_timestamp: draft.compare_timestamp,
            timestamp_tolerance_secs: draft.timestamp_tolerance_secs,
            ignore_dst: draft.ignore_dst,
            ignore_timezone: draft.ignore_timezone,
            compare_filename_case: draft.compare_filename_case,
            align_different_extensions: draft.align_different_extensions,
            align_unicode_normalization: draft.align_unicode_normalization,
            compare_attr_archive: draft.compare_attr_archive,
            compare_attr_system: draft.compare_attr_system,
            compare_attr_hidden: draft.compare_attr_hidden,
            compare_attr_readonly: draft.compare_attr_readonly,
            compare_unix_permissions: draft.compare_unix_permissions,
            compare_unix_owner: draft.compare_unix_owner,
            compare_unix_group: draft.compare_unix_group,
            compare_content: draft.compare_content,
            content_mode: draft.content_mode,
            compare_versions: draft.compare_versions,
            skip_if_quick_same: draft.skip_if_quick_same,
            override_quick_results: draft.override_quick_results,
            follow_symlinks: draft.follow_symlinks,
        };
        self.dir_handling = match draft.dir_handling {
            FolderHandlingChoice::ShowAll => DirHandling::ShowAll,
            FolderHandlingChoice::CompareStructure => DirHandling::CompareStructure,
            FolderHandlingChoice::CompareFilesOnly => DirHandling::CompareFilesOnly,
            FolderHandlingChoice::Flatten => DirHandling::Flatten,
        };
        self.archives_as_folders = draft.archives_as_folders;
        self.filter = draft.filter;
        self.exclude_patterns = draft
            .exclude_patterns
            .split(';')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .collect();
        self.suppress_filters = draft.suppress_filters;
        self.exclude_protected_os = draft.exclude_protected_os;
        self.min_size_kb = draft.min_size_kb;
        self.show_log = draft.show_log;
        self.show_folder_sizes = draft.show_folder_sizes;
        self.auto_expand_subfolders = draft.auto_expand_subfolders;
        if self.auto_expand_subfolders {
            self.collapsed.clear();
        }
        self.rules_scope = draft.scope;
        if draft.only_diffs {
            self.display_filter = DisplayFilter::Diff;
        } else if draft.only_same {
            self.display_filter = DisplayFilter::Same;
        } else if matches!(self.display_filter, DisplayFilter::Diff | DisplayFilter::Same) {
            self.display_filter = DisplayFilter::All;
        }
        let needs_rescan = paths_changed || prev_criteria != self.current_criteria();
        if needs_rescan && !self.left_path.is_empty() && !self.right_path.is_empty() {
            self.start_scan();
        }
    }

    pub fn sync_preferences(&self, prefs: &mut crate::i18n::AppPreferences) {
        prefs.folder_compare_content = self.compare_content;
        prefs.folder_show_tree = self.show_tree();
        prefs.folder_show_log = self.show_log;
    }

    pub fn apply_preferences(&mut self, prefs: &crate::i18n::AppPreferences) {
        let content_changed = self.compare_content != prefs.folder_compare_content;
        self.compare_content = prefs.folder_compare_content;
        self.dir_handling = if prefs.folder_show_tree {
            if self.dir_handling == DirHandling::Flatten {
                DirHandling::CompareStructure
            } else {
                self.dir_handling
            }
        } else {
            DirHandling::Flatten
        };
        self.show_log = prefs.folder_show_log;
        if content_changed && !self.left_path.is_empty() && !self.right_path.is_empty() {
            self.start_scan();
        }
    }

    pub fn session_options(&self) -> serde_json::Value {
        serde_json::json!({
            "filter": self.filter,
            "only_diffs": self.display_filter == DisplayFilter::Diff,
            "only_same": self.display_filter == DisplayFilter::Same,
            "display_filter": self.display_filter.as_name(),
            "dir_handling": self.dir_handling.as_name(),
            "ignore_minor": self.ignore_minor,
            "content": self.compare_content,
            "content_mode": self.content_mode.as_name(),
            "tree": self.show_tree(),
            "log": self.show_log,
            "exclude_patterns": self.exclude_patterns,
        })
    }

    pub fn apply_session_options(&mut self, value: &serde_json::Value) {
        if let Some(v) = value["content"].as_bool() {
            self.compare_content = v;
        }
        if let Some(m) = value["content_mode"]
            .as_str()
            .and_then(ContentCompareMode::from_name)
        {
            self.content_mode = m;
        }
        if let Some(v) = value["log"].as_bool() {
            self.show_log = v;
        }
        if let Some(v) = value["ignore_minor"].as_bool() {
            self.ignore_minor = v;
        }
        if let Some(arr) = value["exclude_patterns"].as_array() {
            self.exclude_patterns = arr
                .iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect();
        }
        if let Some(name) = value["display_filter"]
            .as_str()
            .and_then(DisplayFilter::from_name)
        {
            self.display_filter = name;
        } else if value["only_diffs"].as_bool() == Some(true) {
            self.display_filter = DisplayFilter::Diff;
        } else if value["only_same"].as_bool() == Some(true) {
            self.display_filter = DisplayFilter::Same;
        } else if value["only_diffs"].is_boolean() || value["only_same"].is_boolean() {
            self.display_filter = DisplayFilter::All;
        }
        if let Some(name) = value["dir_handling"]
            .as_str()
            .and_then(DirHandling::from_name)
        {
            self.dir_handling = name;
        } else if let Some(tree) = value["tree"].as_bool() {
            self.dir_handling = if tree {
                DirHandling::CompareStructure
            } else {
                DirHandling::Flatten
            };
        }
        if let Some(filter) = value["filter"].as_str() {
            self.filter = filter.into();
        }
        self.start_scan();
    }

    pub fn new() -> Self {
        Self {
            left_path: String::new(),
            right_path: String::new(),
            description: String::new(),
            entries: Vec::new(),
            errors: Vec::new(),
            filter: DEFAULT_FILTER.into(),
            display_filter: DisplayFilter::All,
            dir_handling: DirHandling::CompareStructure,
            ignore_minor: false,
            compare_content: true,
            content_mode: ContentCompareMode::Binary,
            criteria: FolderCompareCriteria::default(),
            archives_as_folders: false,
            exclude_protected_os: false,
            min_size_kb: 0,
            show_folder_sizes: false,
            auto_expand_subfolders: false,
            rules_scope: super::session_settings::FolderScopeChoice::ThisViewOnly,
            show_compare_results: true,
            dont_show_compare_dialog: false,
            dont_show_copy_dialog: false,
            dont_show_move_dialog: false,
            last_touch_time: None,
            exclude_patterns: Vec::new(),
            ignored_paths: BTreeSet::new(),
            pending: None,
            pending_content: None,
            cancel: Arc::new(AtomicBool::new(false)),
            loaded_left: PathBuf::new(),
            loaded_right: PathBuf::new(),
            log: Vec::new(),
            selected: BTreeSet::new(),
            anchor_selected: None,
            active_left: true,
            show_log: true,
            show_toolbar: true,
            collapsed: BTreeSet::new(),
            tree_roots: Default::default(),
            recent_paths: Vec::new(),
            root_history: Vec::new(),
            root_cursor: 0,
            free_space: Default::default(),
            scroll_to_selected: false,
            find_text: String::new(),
            selection_focus: true,
            suppress_filters: false,
            modal: None,
        }
    }
    /// A directory drop updates the comparison instead of creating two
    /// left-only sessions. Missing sides are filled before replacing a side.
    pub fn drop_folders(&mut self, folders: &[PathBuf], target_left: Option<bool>) {
        match folders {
            [left, right] => {
                self.left_path = left.to_string_lossy().into_owned();
                self.right_path = right.to_string_lossy().into_owned();
            }
            [folder] => {
                let left = match (self.left_path.is_empty(), self.right_path.is_empty()) {
                    (true, false) => true,
                    (false, true) => false,
                    _ => target_left.unwrap_or(self.active_left),
                };
                self.active_left = left;
                if left {
                    self.left_path = folder.to_string_lossy().into_owned();
                } else {
                    self.right_path = folder.to_string_lossy().into_owned();
                }
            }
            _ => return,
        }
        if self.left_path.is_empty() || self.right_path.is_empty() {
            self.clear_scan();
            self.errors.clear();
        } else {
            self.start_scan();
        }
    }

    fn clear_scan(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        self.pending = None;
        self.pending_content = None;
        self.entries.clear();
        self.selected.clear();
        self.anchor_selected = None;
        self.loaded_left = PathBuf::new();
        self.loaded_right = PathBuf::new();
    }
    #[cfg(test)]
    pub(crate) fn comparison_results(&mut self) -> Vec<(PathBuf, &'static str)> {
        if let Some(receiver) = self.pending.take() {
            let scan = receiver
                .recv_timeout(std::time::Duration::from_secs(5))
                .unwrap();
            self.accept_scan(scan);
        }
        assert!(self.errors.is_empty(), "{:?}", self.errors);
        self.entries
            .iter()
            .map(|entry| {
                (
                    entry.relative.clone(),
                    match entry.status {
                        Status::Same => "same",
                        Status::Different => "different",
                        Status::LeftOnly => "left-only",
                        Status::RightOnly => "right-only",
                        Status::Error => "error",
                    },
                )
            })
            .collect()
    }

    pub fn is_scanning(&self) -> bool {
        self.pending.is_some() || self.pending_content.is_some()
    }

    pub fn load_folders(&mut self, left: &Path, right: &Path) {
        self.left_path = left.to_string_lossy().into();
        self.right_path = right.to_string_lossy().into();
        self.start_scan();
    }

    fn accept_scan(&mut self, mut scan: Scan) {
        let roots = (self.loaded_left.clone(), self.loaded_right.clone());
        let mut ignored_applied = false;
        if !self.ignored_paths.is_empty() {
            for entry in &mut scan.entries {
                if self.ignored_paths.contains(&path_key(&entry.relative)) {
                    entry.ignored = true;
                    ignored_applied = true;
                }
            }
            if ignored_applied {
                recompute_dir_summaries(&mut scan.entries);
            }
        }
        if roots != self.tree_roots {
            self.collapsed = if self.auto_expand_subfolders {
                BTreeSet::new()
            } else {
                scan.entries
                    .iter()
                    .filter(|entry| is_dir(entry))
                    .map(|entry| path_key(&entry.relative))
                    .collect()
            };
            if let Some(entry) = scan.entries.first() {
                self.selected.insert(entry.id.clone());
                self.anchor_selected = Some(entry.id.clone());
            }
            self.tree_roots = roots;
        }
        self.selection_focus = true;
        self.entries = scan.entries;
        self.errors = scan.errors;
        self.pending = None;
    }

    fn start_scan(&mut self) {
        self.clear_scan();
        let left = PathBuf::from(&self.left_path);
        let right = PathBuf::from(&self.right_path);
        if !folder_source::is_folder_like(&left) || !folder_source::is_folder_like(&right) {
            self.errors = vec!["folder.select_two_existing_folders_to_compare".into()];
            return;
        }
        self.free_space = [disk_space(&left), disk_space(&right)];
        let pair = (self.left_path.clone(), self.right_path.clone());
        if self.root_history.get(self.root_cursor) != Some(&pair) {
            self.root_history.truncate(self.root_cursor + 1);
            self.root_history.push(pair);
            self.root_cursor = self.root_history.len() - 1;
        }
        self.loaded_left = left.clone();
        self.loaded_right = right.clone();
        for path in [&self.left_path, &self.right_path] {
            if !self.recent_paths.contains(path) {
                self.recent_paths.push(path.clone());
            }
        }
        if self.log.is_empty() {
            let now = chrono::Local::now().format("%d/%m/%Y %H:%M:%S").to_string();
            let host = std::env::var("COMPUTERNAME").unwrap_or_default();
            let user = std::env::var("USERNAME").unwrap_or_default();
            self.log.push(crate::i18n::tr_fmt(
                "status.user",
                &[("", &now), ("", &host), ("", &user)],
            ));
        }
        let now = chrono::Local::now().format("%d/%m/%Y %H:%M:%S").to_string();
        self.log.push(crate::i18n::tr_fmt(
            "status.load_comparison",
            &[
                ("", &now),
                ("", &left.display().to_string()),
                ("", &right.display().to_string()),
            ],
        ));
        self.cancel = Arc::new(AtomicBool::new(false));
        let cancel = self.cancel.clone();
        let criteria = self.current_criteria();
        let (sender, receiver) = mpsc::channel();
        self.pending = Some(receiver);
        self.errors.clear();
        thread::spawn(move || {
            let _ = sender.send(scan_folders_with_criteria(&left, &right, &criteria, &cancel));
        });
    }

    fn is_excluded(&self, entry: &Entry) -> bool {
        if self.suppress_filters {
            return false;
        }
        let name_norm = entry
            .relative
            .file_name()
            .map(|n| n.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        if self.exclude_protected_os
            && matches!(
                name_norm.as_str(),
                "desktop.ini"
                    | "thumbs.db"
                    | ".ds_store"
                    | "ntuser.dat"
                    | "$recycle.bin"
                    | "system volume information"
            )
        {
            return true;
        }
        if self.min_size_kb > 0 && !is_dir(entry) {
            let min_bytes = self.min_size_kb.saturating_mul(1024);
            let max_side = entry
                .left
                .as_ref()
                .map_or(0, EntryMeta::len)
                .max(entry.right.as_ref().map_or(0, EntryMeta::len));
            if max_side < min_bytes {
                return true;
            }
        }
        if self.exclude_patterns.is_empty() {
            return false;
        }
        let rel_norm = entry
            .relative
            .to_string_lossy()
            .replace('\\', "/")
            .to_lowercase();
        self.exclude_patterns.iter().any(|pat| {
            let pat_norm = pat.replace('\\', "/").to_lowercase();
            if pat_norm.contains('/') {
                rel_norm == pat_norm || rel_norm.starts_with(&format!("{pat_norm}/"))
            } else {
                name_norm == pat_norm
                    || wildcard(&pat_norm, &name_norm)
                    || entry.relative.ancestors().skip(1).any(|anc| {
                        anc.file_name().is_some_and(|n| {
                            let an = n.to_string_lossy().to_lowercase();
                            an == pat_norm || wildcard(&pat_norm, &an)
                        })
                    })
            }
        })
    }

    fn visible_indices(&self) -> Vec<usize> {
        let mut matching: Vec<bool> = self
            .entries
            .iter()
            .map(|entry| {
                if self.is_excluded(entry) {
                    return false;
                }
                if is_dir(entry) {
                    match self.dir_handling {
                        DirHandling::Flatten => false,
                        DirHandling::ShowAll => true,
                        DirHandling::CompareFilesOnly => false,
                        DirHandling::CompareStructure => {
                            entry_matches_display_filter(entry, self.display_filter, self.ignore_minor)
                                && (self.suppress_filters || name_filter_matches(&self.filter, entry))
                        }
                    }
                } else {
                    entry_matches_display_filter(entry, self.display_filter, self.ignore_minor)
                        && (self.suppress_filters || name_filter_matches(&self.filter, entry))
                }
            })
            .collect();
        let show_tree = self.show_tree();
        if show_tree {
            // Retain folder context for matching descendants: a mixed folder
            // must still be shown when the user selects "Same" or "Compare Files Only".
            let mut ancestors = BTreeSet::new();
            for (entry, matched) in self.entries.iter().zip(&matching) {
                if *matched {
                    for parent in entry
                        .relative
                        .ancestors()
                        .skip(1)
                        .filter(|p| !p.as_os_str().is_empty())
                    {
                        ancestors.insert(path_key(parent));
                    }
                }
            }
            for (entry, matched) in self.entries.iter().zip(&mut matching) {
                if is_dir(entry) && ancestors.contains(&path_key(&entry.relative)) {
                    *matched = true;
                }
            }
        }
        self.entries
            .iter()
            .zip(matching)
            .enumerate()
            .filter_map(|(index, (entry, matched))| {
                (matched
                    && (!show_tree
                        || !entry
                            .relative
                            .ancestors()
                            .skip(1)
                            .any(|parent| self.collapsed.contains(&path_key(parent)))))
                .then_some(index)
            })
            .collect()
    }

    fn paths(&self, left: bool) -> Result<Vec<PathBuf>, String> {
        let root = if left {
            &self.loaded_left
        } else {
            &self.loaded_right
        };
        let mut roots: Vec<PathBuf> = Vec::new();
        let mut paths = Vec::new();
        for entry in self
            .entries
            .iter()
            .filter(|entry| self.selected.contains(&entry.id))
        {
            let (relative, exists) = if left {
                (&entry.relative, entry.left.is_some())
            } else {
                (&entry.right_relative, entry.right.is_some())
            };
            if !exists || roots.iter().any(|parent| relative.starts_with(parent)) {
                continue;
            }
            paths.push(crate::file_ops::resolve(root, relative)?);
            roots.push(relative.clone());
        }
        Ok(paths)
    }

    fn copy_selected(&mut self, to_right: bool, dialogs: &mut dyn super::dialogs::DialogHost) {
        if self.pending.is_some() {
            return;
        }
        let (source, target) = if to_right {
            (&self.loaded_left, &self.loaded_right)
        } else {
            (&self.loaded_right, &self.loaded_left)
        };
        if !folder_source::is_writable(target) {
            dialogs.error(&crate::i18n::tr("folder.archive_read_only"));
            return;
        }
        if folder_source::is_snapshot_path(source) {
            dialogs.error(&crate::i18n::tr("folder.snapshot_no_content"));
            return;
        }
        let selected: Vec<_> = self
            .entries
            .iter()
            .filter(|entry| {
                self.selected.contains(&entry.id)
                    && if to_right {
                        entry.left.is_some()
                    } else {
                        entry.right.is_some()
                    }
            })
            .map(|entry| {
                if to_right {
                    entry.relative.clone()
                } else {
                    entry.right_relative.clone()
                }
            })
            .collect();
        if selected.is_empty() {
            return;
        }
        if !folder_source::is_writable(source) {
            // Extracting selected archive entries into a real writable folder.
            let mut extract_items: Vec<(PathBuf, bool, PathBuf)> = Vec::new();
            let mut overwrite_count = 0usize;
            for entry in &self.entries {
                let Some(meta) = (if to_right { &entry.left } else { &entry.right }) else {
                    continue;
                };
                let rel = if to_right {
                    &entry.relative
                } else {
                    &entry.right_relative
                };
                if selected
                    .iter()
                    .any(|sel| rel == sel || rel.starts_with(sel))
                {
                    let dst = match crate::file_ops::resolve(target, rel) {
                        Ok(p) => p,
                        Err(e) => {
                            dialogs.error(&e);
                            return;
                        }
                    };
                    if !meta.is_dir() && dst.is_file() {
                        overwrite_count += 1;
                    }
                    extract_items.push((rel.clone(), meta.is_dir(), dst));
                }
            }
            if extract_items.is_empty() {
                return;
            }
            let preview = extract_items
                .iter()
                .take(12)
                .map(|(rel, _, dst)| format!("{} → {}", rel.display(), dst.display()))
                .collect::<Vec<_>>()
                .join("\n");
            if !dialogs.confirm(&crate::i18n::tr_fmt(
                "dialog.copy_items_files_will_be_overwritten",
                &[
                    ("", &extract_items.len().to_string()),
                    ("", &overwrite_count.to_string()),
                    ("preview", &preview),
                ],
            )) {
                return;
            }
            self.cancel.store(true, Ordering::Relaxed);
            self.cancel = Arc::new(AtomicBool::new(false));
            let cancel = self.cancel.clone();
            let src_root = source.clone();
            let left = self.loaded_left.clone();
            let right = self.loaded_right.clone();
            let content = self.compare_content;
            let (sender, receiver) = mpsc::channel();
            self.pending = Some(receiver);
            self.log.push(crate::i18n::tr_fmt(
                "status.copy_selection_items_to",
                &[
                    ("", &extract_items.len().to_string()),
                    ("", &target.display().to_string()),
                ],
            ));
            thread::spawn(move || {
                let mut extract_errors = Vec::new();
                for (rel, is_dir_entry, dst) in extract_items {
                    if cancel.load(Ordering::Relaxed) {
                        break;
                    }
                    if is_dir_entry {
                        if let Err(e) = fs::create_dir_all(&dst) {
                            extract_errors.push(format!("{}: {e}", dst.display()));
                        }
                    } else {
                        if let Some(parent) = dst.parent() {
                            let _ = fs::create_dir_all(parent);
                        }
                        match folder_source::read_entry_bytes(&src_root, &rel) {
                            Ok(bytes) => {
                                if let Err(e) = crate::document::atomic_write(&dst, &bytes) {
                                    extract_errors.push(e);
                                }
                            }
                            Err(e) => extract_errors.push(e),
                        }
                    }
                }
                let mut scan = scan_folders(&left, &right, content, &cancel);
                scan.errors.extend(extract_errors);
                let _ = sender.send(scan);
            });
            return;
        }
        let plan = match crate::file_ops::CopyPlan::build(source, target, &selected) {
            Ok(plan) => plan,
            Err(error) => {
                dialogs.error(&error);
                return;
            }
        };
        let preview = plan
            .items
            .iter()
            .take(12)
            .map(|(source, target)| format!("{} → {}", source.display(), target.display()))
            .collect::<Vec<_>>()
            .join("\n");
        if !dialogs.confirm(&crate::i18n::tr_fmt(
            "dialog.copy_items_files_will_be_overwritten",
            &[
                ("", &plan.items.len().to_string()),
                ("", &plan.overwrite_count.to_string()),
                ("preview", &preview),
            ],
        )) {
            return;
        }
        self.cancel.store(true, Ordering::Relaxed);
        self.cancel = Arc::new(AtomicBool::new(false));
        let cancel = self.cancel.clone();
        let left = self.loaded_left.clone();
        let right = self.loaded_right.clone();
        let content = self.compare_content;
        let (sender, receiver) = mpsc::channel();
        self.pending = Some(receiver);
        self.log.push(crate::i18n::tr_fmt(
            "status.copy_selection_items_to",
            &[
                ("", &plan.items.len().to_string()),
                ("", &target.display().to_string()),
            ],
        ));
        thread::spawn(move || {
            let result = plan.execute(&cancel);
            let mut scan = scan_folders(&left, &right, content, &cancel);
            if let Err(error) = result {
                scan.errors.push(error);
            }
            let _ = sender.send(scan);
        });
    }

    #[cfg(test)]
    fn rename_selected(&mut self, dialogs: &mut dyn super::dialogs::DialogHost) {
        let root = if self.active_left {
            &self.loaded_left
        } else {
            &self.loaded_right
        };
        if !folder_source::is_writable(root) {
            dialogs.error(&crate::i18n::tr("folder.archive_read_only"));
            return;
        }
        let paths = match self.paths(self.active_left) {
            Ok(paths) => paths,
            Err(error) => {
                dialogs.error(&error);
                return;
            }
        };
        if paths.len() != 1 {
            return;
        }
        let source = &paths[0];
        let initial = source.file_name().unwrap().to_string_lossy();
        if let Some(name) = dialogs.input("common.rename", "folder.new_name", &initial) {
            let result = (|| -> Result<(), String> {
                let name = crate::file_ops::leaf(&name)?;
                let target = source.parent().unwrap().join(name);
                if target.exists() {
                    return Err(crate::i18n::tr(
                        "folder.an_item_with_this_name_already_exists",
                    ));
                }
                fs::rename(source, target).map_err(|e| e.to_string())
            })();
            if let Err(error) = result {
                dialogs.error(&error);
            } else {
                self.start_scan();
            }
        }
    }

    #[cfg(test)]
    fn delete_selected(&mut self, dialogs: &mut dyn super::dialogs::DialogHost) {
        let root = if self.active_left {
            &self.loaded_left
        } else {
            &self.loaded_right
        };
        if !folder_source::is_writable(root) {
            dialogs.error(&crate::i18n::tr("folder.archive_read_only"));
            return;
        }
        let paths = match self.paths(self.active_left) {
            Ok(paths) => paths,
            Err(error) => {
                dialogs.error(&error);
                return;
            }
        };
        if paths.is_empty() {
            return;
        }
        let preview = paths
            .iter()
            .take(12)
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join("\n");
        if dialogs.confirm(&crate::i18n::tr_fmt(
            "dialog.send_items_to_the_recycle_bin",
            &[("", &paths.len().to_string()), ("preview", &preview)],
        )) {
            match dialogs.recycle(&paths) {
                Ok(()) => self.start_scan(),
                Err(error) => dialogs.error(&error),
            }
        }
    }

    fn navigate(&mut self, forward: bool, query: Option<&str>) {
        let visible: Vec<_> = self
            .visible_indices()
            .into_iter()
            .map(|index| &self.entries[index])
            .collect();
        let current = self.selected.iter().next();
        let index = current.and_then(|path| visible.iter().position(|entry| &entry.id == path));
        let mut order: Vec<_> = (0..visible.len()).collect();
        if !forward {
            order.reverse();
        }
        let next = order.into_iter().find(|&i| {
            let after = index.is_none_or(|index| if forward { i > index } else { i < index });
            after
                && if let Some(query) = query {
                    visible[i]
                        .relative
                        .to_string_lossy()
                        .to_lowercase()
                        .contains(&query.to_lowercase())
                } else {
                    visible[i].status != Status::Same
                }
        });
        if let Some(index) = next {
            let path = visible[index].id.clone();
            self.selected.clear();
            self.selected.insert(path);
            self.scroll_to_selected = true;
        }
    }

    fn options(&mut self, dialogs: &mut dyn super::dialogs::DialogHost) {
        use super::dialogs::DialogField;
        let mut fields = [
            DialogField::Check(
                "folder.compare_file_contents".into(),
                self.compare_content,
            ),
            DialogField::Check("common.show_folder_structure".into(), self.show_tree()),
            DialogField::Check("common.show_log".into(), self.show_log),
            crate::i18n::language_field(),
        ];
        if dialogs.fields("folder.folder_options", &mut fields) {
            let previous = self.compare_content;
            if let DialogField::Check(_, value) = fields[0] {
                self.compare_content = value;
            }
            if let DialogField::Check(_, value) = fields[1] {
                self.dir_handling = if value {
                    if self.dir_handling == DirHandling::Flatten {
                        DirHandling::CompareStructure
                    } else {
                        self.dir_handling
                    }
                } else {
                    DirHandling::Flatten
                };
            }
            if let DialogField::Check(_, value) = fields[2] {
                self.show_log = value;
            }
            if previous != self.compare_content {
                self.start_scan();
            }
            crate::i18n::apply_language_field(fields.last().unwrap(), dialogs);
        }
    }

    fn report(&self, dialogs: &mut dyn super::dialogs::DialogHost) {
        let describe = |metadata: &Option<EntryMeta>| {
            metadata
                .as_ref()
                .map(|m| {
                    if m.is_dir() {
                        "common.folder".into()
                    } else {
                        format!("{} bytes", m.len())
                    }
                })
                .unwrap_or_default()
        };
        let rows = self
            .entries
            .iter()
            .map(|entry| crate::report::Row {
                key: entry.relative.display().to_string(),
                left: describe(&entry.left),
                right: describe(&entry.right),
                different: entry.status != Status::Same,
            })
            .collect::<Vec<_>>();
        crate::report::export(
            "session.folder_compare",
            &self.left_path,
            &self.right_path,
            &rows,
            &[
                PathBuf::from(&self.left_path),
                PathBuf::from(&self.right_path),
            ],
            dialogs,
        );
    }

    /// Collects indices of all entries covered by the current selection, including
    /// all descendants of selected directories (even when those directories are collapsed).
    fn selected_subtree_indices(&self, include_hidden: bool) -> Vec<usize> {
        let selected_dirs: Vec<PathBuf> = self
            .entries
            .iter()
            .filter(|e| self.selected.contains(&e.id) && is_dir(e))
            .map(|e| e.relative.clone())
            .collect();
        self.entries
            .iter()
            .enumerate()
            .filter_map(|(idx, e)| {
                let in_sel = self.selected.contains(&e.id)
                    || selected_dirs.iter().any(|d| e.relative.starts_with(d) && e.relative != *d);
                if !in_sel {
                    return None;
                }
                if !include_hidden && self.is_excluded(e) {
                    return None;
                }
                if !include_hidden
                    && !is_dir(e)
                    && !(entry_matches_display_filter(e, self.display_filter, self.ignore_minor)
                        && (self.suppress_filters || name_filter_matches(&self.filter, e)))
                {
                    return None;
                }
                Some(idx)
            })
            .collect()
    }

    fn selection_stats(&self) -> SelectionStats {
        let mut stats = SelectionStats::default();
        let selected_dirs: Vec<PathBuf> = self
            .entries
            .iter()
            .filter(|e| self.selected.contains(&e.id) && is_dir(e))
            .map(|e| e.relative.clone())
            .collect();
        for e in &self.entries {
            let direct = self.selected.contains(&e.id);
            let descendant =
                selected_dirs.iter().any(|d| e.relative.starts_with(d) && e.relative != *d);
            if !direct && !descendant {
                continue;
            }
            let hidden = self.is_excluded(e)
                || (!is_dir(e)
                    && !(entry_matches_display_filter(e, self.display_filter, self.ignore_minor)
                        && (self.suppress_filters || name_filter_matches(&self.filter, e))));
            if hidden {
                stats.hidden_items += 1;
                continue;
            }
            if let Some(m) = &e.left {
                if m.is_dir() {
                    stats.left_dirs += 1;
                } else {
                    stats.left_files += 1;
                    stats.left_bytes = stats.left_bytes.saturating_add(m.len());
                }
            }
            if let Some(m) = &e.right {
                if m.is_dir() {
                    stats.right_dirs += 1;
                } else {
                    stats.right_files += 1;
                    stats.right_bytes = stats.right_bytes.saturating_add(m.len());
                }
            }
        }
        stats
    }

    /// Visual-audit helper: waits for the initial scan, selects all visible items,
    /// and runs binary content comparison synchronously to present `TOpCompareResultsDialog`.
    pub fn capture_run_binary_compare_results(&mut self, expand_sample: bool) {
        if let Some(rx) = self.pending.take() {
            if let Ok(scan) = rx.recv_timeout(std::time::Duration::from_secs(10)) {
                self.accept_scan(scan);
            }
        }
        self.selected = self
            .visible_indices()
            .into_iter()
            .map(|idx| self.entries[idx].id.clone())
            .collect();
        self.start_content_compare(ContentCompareMode::Binary, true, false);
        if let Some(rx) = self.pending_content.take() {
            if let Ok(outcome) = rx.recv_timeout(std::time::Duration::from_secs(10)) {
                self.accept_content_compare(outcome);
            }
        }
        if expand_sample {
            if let Some(FolderOpModal::CompareResults(ref mut res)) = self.modal {
                // Expand the first non-empty categories to verify child icons & dotted tree lines.
                let counts = [
                    res.different,
                    res.minor,
                    res.same,
                    res.left_orphans,
                    res.right_orphans,
                    res.errors,
                ];
                for (i, &c) in counts.iter().enumerate() {
                    if c > 0 {
                        res.expanded[i] = true;
                        if res.selected.is_none() {
                            res.selected = Some((i, None));
                        }
                    }
                }
            }
        }
    }

    fn open_compare_contents_modal(&mut self) {
        if self.is_scanning() || self.selected.is_empty() {
            return;
        }
        if self.dont_show_compare_dialog {
            self.start_content_compare(self.content_mode, self.show_compare_results, false);
            return;
        }
        self.modal = Some(FolderOpModal::CompareContents {
            stats: self.selection_stats(),
            mode: self.content_mode,
            show_results: self.show_compare_results,
            dont_show_again: self.dont_show_compare_dialog,
            include_hidden: false,
        });
    }

    fn start_content_compare(
        &mut self,
        mode: ContentCompareMode,
        show_results: bool,
        include_hidden: bool,
    ) {
        if self.is_scanning() {
            return;
        }
        self.content_mode = mode;
        self.show_compare_results = show_results;
        let indices = self.selected_subtree_indices(include_hidden);
        struct CompareJob {
            id: EntryKey,
            rel: PathBuf,
            right_rel: PathBuf,
            left: Option<EntryMeta>,
            right: Option<EntryMeta>,
        }
        let jobs: Vec<CompareJob> = indices
            .into_iter()
            .filter_map(|idx| {
                let e = &self.entries[idx];
                if is_dir(e) {
                    return None;
                }
                Some(CompareJob {
                    id: e.id.clone(),
                    rel: e.relative.clone(),
                    right_rel: e.right_relative.clone(),
                    left: e.left.clone(),
                    right: e.right.clone(),
                })
            })
            .collect();
        self.cancel = Arc::new(AtomicBool::new(false));
        let cancel = self.cancel.clone();
        let left_root = self.loaded_left.clone();
        let right_root = self.loaded_right.clone();
        let (tx, rx) = mpsc::channel();
        self.pending_content = Some(rx);
        let now = chrono::Local::now().format("%d/%m/%Y %H:%M:%S").to_string();
        self.log.push(format!(
            "{now} {} ({})",
            crate::i18n::tr("folder.op.compare_title"),
            crate::i18n::tr(mode.label_key())
        ));
        thread::spawn(move || {
            let started = Instant::now();
            let job_count = jobs.len();
            let worker_count = std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(4)
                .clamp(1, 16)
                .min(job_count.max(1));
            let next_idx = std::sync::atomic::AtomicUsize::new(0);
            type CompareSlot = (CompareResultRow, Option<ContentCompareItemUpdate>);
            let slots: Vec<std::sync::Mutex<Option<CompareSlot>>> =
                (0..job_count).map(|_| std::sync::Mutex::new(None)).collect();

            std::thread::scope(|scope| {
                for _ in 0..worker_count {
                    scope.spawn(|| loop {
                        if cancel.load(Ordering::Relaxed) {
                            break;
                        }
                        let i = next_idx.fetch_add(1, Ordering::Relaxed);
                        if i >= job_count {
                            break;
                        }
                        let job = &jobs[i];
                        let out = match (&job.left, &job.right) {
                            (Some(l_meta), Some(r_meta)) if l_meta.is_file() && r_meta.is_file() => {
                                match folder_ops::compare_file_pair(
                                    &left_root,
                                    &job.rel,
                                    l_meta,
                                    &right_root,
                                    &job.right_rel,
                                    r_meta,
                                    mode,
                                    &cancel,
                                ) {
                                    FileContentVerdict::Same {
                                        detail,
                                        left_crc,
                                        right_crc,
                                    } => (
                                        CompareResultRow {
                                            relative: job.rel.clone(),
                                            right_relative: job.right_rel.clone(),
                                            status: CompareRowStatus::Same,
                                            detail,
                                        },
                                        Some(ContentCompareItemUpdate {
                                            id: job.id.clone(),
                                            status: Status::Same,
                                            is_minor: false,
                                            left_crc32: left_crc,
                                            right_crc32: right_crc,
                                            error: None,
                                        }),
                                    ),
                                    FileContentVerdict::Minor { detail } => (
                                        CompareResultRow {
                                            relative: job.rel.clone(),
                                            right_relative: job.right_rel.clone(),
                                            status: CompareRowStatus::Minor,
                                            detail,
                                        },
                                        Some(ContentCompareItemUpdate {
                                            id: job.id.clone(),
                                            status: Status::Different,
                                            is_minor: true,
                                            left_crc32: l_meta.crc32,
                                            right_crc32: r_meta.crc32,
                                            error: None,
                                        }),
                                    ),
                                    FileContentVerdict::Different {
                                        detail,
                                        left_crc,
                                        right_crc,
                                    } => (
                                        CompareResultRow {
                                            relative: job.rel.clone(),
                                            right_relative: job.right_rel.clone(),
                                            status: CompareRowStatus::Different,
                                            detail,
                                        },
                                        Some(ContentCompareItemUpdate {
                                            id: job.id.clone(),
                                            status: Status::Different,
                                            is_minor: false,
                                            left_crc32: left_crc,
                                            right_crc32: right_crc,
                                            error: None,
                                        }),
                                    ),
                                    FileContentVerdict::Error(err) => (
                                        CompareResultRow {
                                            relative: job.rel.clone(),
                                            right_relative: job.right_rel.clone(),
                                            status: CompareRowStatus::Error,
                                            detail: err.clone(),
                                        },
                                        Some(ContentCompareItemUpdate {
                                            id: job.id.clone(),
                                            status: Status::Error,
                                            is_minor: false,
                                            left_crc32: l_meta.crc32,
                                            right_crc32: r_meta.crc32,
                                            error: Some(err),
                                        }),
                                    ),
                                }
                            }
                            (Some(_), None) => (
                                CompareResultRow {
                                    relative: job.rel.clone(),
                                    right_relative: PathBuf::new(),
                                    status: CompareRowStatus::LeftOrphan,
                                    detail: String::new(),
                                },
                                None,
                            ),
                            _ => (
                                CompareResultRow {
                                    relative: job.right_rel.clone(),
                                    right_relative: job.right_rel.clone(),
                                    status: CompareRowStatus::RightOrphan,
                                    detail: String::new(),
                                },
                                None,
                            ),
                        };
                        if let Ok(mut guard) = slots[i].lock() {
                            *guard = Some(out);
                        }
                    });
                }
            });

            let mut updates = Vec::with_capacity(job_count);
            let mut rows = Vec::with_capacity(job_count);
            let (mut same, mut minor, mut different, mut left_orphans, mut right_orphans, mut errors) =
                (0, 0, 0, 0, 0, 0);
            for slot in slots {
                if let Some((row, upd)) = slot.into_inner().ok().flatten() {
                    match row.status {
                        CompareRowStatus::Same => same += 1,
                        CompareRowStatus::Minor => minor += 1,
                        CompareRowStatus::Different => different += 1,
                        CompareRowStatus::LeftOrphan => left_orphans += 1,
                        CompareRowStatus::RightOrphan => right_orphans += 1,
                        CompareRowStatus::Error => errors += 1,
                    }
                    rows.push(row);
                    if let Some(u) = upd {
                        updates.push(u);
                    }
                }
            }
            let elapsed_ms = started.elapsed().as_millis();
            let _ = tx.send(ContentCompareOutcome {
                updates,
                results: CompareResultsState {
                    mode,
                    elapsed_ms,
                    same,
                    minor,
                    different,
                    left_orphans,
                    right_orphans,
                    errors,
                    rows,
                    expanded: [false; 6],
                    selected: None,
                },
                show_results,
            });
        });
    }

    fn accept_content_compare(&mut self, outcome: ContentCompareOutcome) {
        self.pending_content = None;
        let mut by_id: BTreeMap<EntryKey, ContentCompareItemUpdate> = outcome
            .updates
            .into_iter()
            .map(|u| (u.id.clone(), u))
            .collect();
        for entry in &mut self.entries {
            if let Some(upd) = by_id.remove(&entry.id) {
                entry.status = upd.status;
                entry.is_minor = upd.is_minor;
                entry.content_compared = true;
                entry.left_crc32 = upd.left_crc32;
                entry.right_crc32 = upd.right_crc32;
                entry.error = upd.error;
            }
        }
        recompute_dir_summaries(&mut self.entries);
        let now = chrono::Local::now().format("%d/%m/%Y %H:%M:%S").to_string();
        self.log
            .push(format!("{now} {}", outcome.results.completion_log_line()));
        if outcome.show_results {
            self.modal = Some(FolderOpModal::CompareResults(outcome.results));
        }
    }

    fn count_side_overwrites(&self) -> usize {
        self.selected_subtree_indices(false)
            .into_iter()
            .filter(|&idx| {
                let e = &self.entries[idx];
                !is_dir(e) && e.left.is_some() && e.right.is_some()
            })
            .count()
    }

    fn open_copy_side_modal(&mut self, to_right: bool, dialogs: &mut dyn super::dialogs::DialogHost) {
        if self.is_scanning() || self.selected.is_empty() {
            return;
        }
        let target = if to_right {
            &self.loaded_right
        } else {
            &self.loaded_left
        };
        let source = if to_right {
            &self.loaded_left
        } else {
            &self.loaded_right
        };
        if !folder_source::is_writable(target) {
            dialogs.error(&crate::i18n::tr("folder.archive_read_only"));
            return;
        }
        if folder_source::is_snapshot_path(source) {
            dialogs.error(&crate::i18n::tr("folder.snapshot_no_content"));
            return;
        }
        if self.dont_show_copy_dialog {
            self.execute_copy_to_side_confirmed(to_right, dialogs);
            return;
        }
        self.modal = Some(FolderOpModal::CopyToSide {
            stats: self.selection_stats(),
            to_right,
            overwrite_count: self.count_side_overwrites(),
            dont_show_again: self.dont_show_copy_dialog,
        });
    }

    fn open_move_side_modal(&mut self, to_right: bool, dialogs: &mut dyn super::dialogs::DialogHost) {
        if self.is_scanning() || self.selected.is_empty() {
            return;
        }
        if !folder_source::is_writable(&self.loaded_left)
            || !folder_source::is_writable(&self.loaded_right)
        {
            dialogs.error(&crate::i18n::tr("folder.archive_read_only"));
            return;
        }
        if self.dont_show_move_dialog {
            self.execute_move_to_side_confirmed(to_right, dialogs);
            return;
        }
        self.modal = Some(FolderOpModal::MoveToSide {
            stats: self.selection_stats(),
            to_right,
            overwrite_count: self.count_side_overwrites(),
            dont_show_again: self.dont_show_move_dialog,
        });
    }

    fn execute_copy_to_side_confirmed(
        &mut self,
        to_right: bool,
        dialogs: &mut dyn super::dialogs::DialogHost,
    ) {
        if self.is_scanning() {
            return;
        }
        let (source, target) = if to_right {
            (&self.loaded_left, &self.loaded_right)
        } else {
            (&self.loaded_right, &self.loaded_left)
        };
        if !folder_source::is_writable(target) {
            dialogs.error(&crate::i18n::tr("folder.archive_read_only"));
            return;
        }
        if folder_source::is_snapshot_path(source) {
            dialogs.error(&crate::i18n::tr("folder.snapshot_no_content"));
            return;
        }
        let selected: Vec<_> = self
            .entries
            .iter()
            .filter(|entry| {
                self.selected.contains(&entry.id)
                    && if to_right {
                        entry.left.is_some()
                    } else {
                        entry.right.is_some()
                    }
            })
            .map(|entry| {
                if to_right {
                    entry.relative.clone()
                } else {
                    entry.right_relative.clone()
                }
            })
            .collect();
        if selected.is_empty() {
            return;
        }
        if !folder_source::is_writable(source) {
            let mut extract_items: Vec<(PathBuf, bool, PathBuf)> = Vec::new();
            for entry in &self.entries {
                let Some(meta) = (if to_right { &entry.left } else { &entry.right }) else {
                    continue;
                };
                let rel = if to_right {
                    &entry.relative
                } else {
                    &entry.right_relative
                };
                if selected.iter().any(|sel| rel == sel || rel.starts_with(sel)) {
                    match crate::file_ops::resolve(target, rel) {
                        Ok(dst) => extract_items.push((rel.clone(), meta.is_dir(), dst)),
                        Err(e) => {
                            dialogs.error(&e);
                            return;
                        }
                    }
                }
            }
            if extract_items.is_empty() {
                return;
            }
            self.cancel = Arc::new(AtomicBool::new(false));
            let cancel = self.cancel.clone();
            let src_root = source.clone();
            let left = self.loaded_left.clone();
            let right = self.loaded_right.clone();
            let content = self.compare_content;
            let content_mode = self.content_mode;
            let (sender, receiver) = mpsc::channel();
            self.pending = Some(receiver);
            self.log.push(crate::i18n::tr_fmt(
                "status.copy_selection_items_to",
                &[
                    ("", &extract_items.len().to_string()),
                    ("", &target.display().to_string()),
                ],
            ));
            thread::spawn(move || {
                let mut extract_errors = Vec::new();
                for (rel, is_dir_entry, dst) in extract_items {
                    if cancel.load(Ordering::Relaxed) {
                        break;
                    }
                    if is_dir_entry {
                        if let Err(e) = fs::create_dir_all(&dst) {
                            extract_errors.push(format!("{}: {e}", dst.display()));
                        }
                    } else {
                        if let Some(parent) = dst.parent() {
                            let _ = fs::create_dir_all(parent);
                        }
                        match folder_source::read_entry_bytes(&src_root, &rel) {
                            Ok(bytes) => {
                                if let Err(e) = crate::document::atomic_write(&dst, &bytes) {
                                    extract_errors.push(e);
                                }
                            }
                            Err(e) => extract_errors.push(e),
                        }
                    }
                }
                let mut scan =
                    scan_folders_with_mode(&left, &right, content, content_mode, &cancel);
                scan.errors.extend(extract_errors);
                let _ = sender.send(scan);
            });
            return;
        }
        let plan = match crate::file_ops::CopyPlan::build(source, target, &selected) {
            Ok(plan) => plan,
            Err(error) => {
                dialogs.error(&error);
                return;
            }
        };
        self.cancel = Arc::new(AtomicBool::new(false));
        let cancel = self.cancel.clone();
        let left = self.loaded_left.clone();
        let right = self.loaded_right.clone();
        let content = self.compare_content;
        let content_mode = self.content_mode;
        let (sender, receiver) = mpsc::channel();
        self.pending = Some(receiver);
        self.log.push(crate::i18n::tr_fmt(
            "status.copy_selection_items_to",
            &[
                ("", &plan.items.len().to_string()),
                ("", &target.display().to_string()),
            ],
        ));
        thread::spawn(move || {
            let result = plan.execute(&cancel);
            let mut scan = scan_folders_with_mode(&left, &right, content, content_mode, &cancel);
            if let Err(error) = result {
                scan.errors.push(error);
            }
            let _ = sender.send(scan);
        });
    }

    fn execute_move_to_side_confirmed(
        &mut self,
        to_right: bool,
        dialogs: &mut dyn super::dialogs::DialogHost,
    ) {
        if self.is_scanning() {
            return;
        }
        let (source, target) = if to_right {
            (&self.loaded_left, &self.loaded_right)
        } else {
            (&self.loaded_right, &self.loaded_left)
        };
        if !folder_source::is_writable(source) || !folder_source::is_writable(target) {
            dialogs.error(&crate::i18n::tr("folder.archive_read_only"));
            return;
        }
        let selected: Vec<_> = self
            .entries
            .iter()
            .filter(|entry| {
                self.selected.contains(&entry.id)
                    && if to_right {
                        entry.left.is_some()
                    } else {
                        entry.right.is_some()
                    }
            })
            .map(|entry| {
                if to_right {
                    entry.relative.clone()
                } else {
                    entry.right_relative.clone()
                }
            })
            .collect();
        if selected.is_empty() {
            return;
        }
        let plan = match crate::file_ops::CopyPlan::build(source, target, &selected) {
            Ok(plan) => plan,
            Err(error) => {
                dialogs.error(&error);
                return;
            }
        };
        let source_paths = match self.paths(to_right) {
            Ok(p) => p,
            Err(e) => {
                dialogs.error(&e);
                return;
            }
        };
        self.cancel = Arc::new(AtomicBool::new(false));
        let cancel = self.cancel.clone();
        let left = self.loaded_left.clone();
        let right = self.loaded_right.clone();
        let content = self.compare_content;
        let content_mode = self.content_mode;
        let (sender, receiver) = mpsc::channel();
        self.pending = Some(receiver);
        thread::spawn(move || {
            let mut op_errors = Vec::new();
            match plan.execute(&cancel) {
                Ok(_) => {
                    for p in source_paths {
                        let r = if p.is_dir() {
                            fs::remove_dir_all(&p)
                        } else {
                            fs::remove_file(&p)
                        };
                        if let Err(e) = r {
                            op_errors.push(format!("{}: {e}", p.display()));
                        }
                    }
                }
                Err(e) => op_errors.push(e),
            }
            let mut scan = scan_folders_with_mode(&left, &right, content, content_mode, &cancel);
            scan.errors.extend(op_errors);
            let _ = sender.send(scan);
        });
    }

    fn open_copy_to_folder_modal(
        &mut self,
        source_left: bool,
        is_move: bool,
        dialogs: &mut dyn super::dialogs::DialogHost,
    ) {
        if self.is_scanning() || self.selected.is_empty() {
            return;
        }
        let source = if source_left {
            &self.loaded_left
        } else {
            &self.loaded_right
        };
        if folder_source::is_snapshot_path(source) {
            dialogs.error(&crate::i18n::tr("folder.snapshot_no_content"));
            return;
        }
        if is_move && !folder_source::is_writable(source) {
            dialogs.error(&crate::i18n::tr("folder.archive_read_only"));
            return;
        }
        let default_dir = source
            .parent()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        let stats = self.selection_stats();
        self.modal = Some(if is_move {
            FolderOpModal::MoveToFolder {
                stats,
                source_left,
                target_dir: default_dir,
                structure: FolderStructureMode::KeepRelative,
            }
        } else {
            FolderOpModal::CopyToFolder {
                stats,
                source_left,
                target_dir: default_dir,
                structure: FolderStructureMode::KeepRelative,
            }
        });
    }

    fn execute_external_transfer(
        &mut self,
        source_left: bool,
        target_dir: PathBuf,
        structure: FolderStructureMode,
        is_move: bool,
        dialogs: &mut dyn super::dialogs::DialogHost,
    ) {
        let source_root = if source_left {
            self.loaded_left.clone()
        } else {
            self.loaded_right.clone()
        };
        if let Err(e) = fs::create_dir_all(&target_dir) {
            dialogs.error(&format!("{}: {e}", target_dir.display()));
            return;
        }
        let items: Vec<(PathBuf, bool)> = self
            .selected_subtree_indices(false)
            .into_iter()
            .filter_map(|idx| {
                let e = &self.entries[idx];
                let meta = if source_left { &e.left } else { &e.right };
                let meta = meta.as_ref()?;
                let rel = if source_left {
                    e.relative.clone()
                } else {
                    e.right_relative.clone()
                };
                Some((rel, meta.is_dir()))
            })
            .collect();
        let source_roots_to_remove = if is_move {
            self.paths(source_left).unwrap_or_default()
        } else {
            Vec::new()
        };
        let mut errors = Vec::new();
        for (rel, is_dir_entry) in &items {
            let dst = folder_ops::resolve_external_target(&target_dir, &source_root, rel, structure);
            if *is_dir_entry {
                if structure != FolderStructureMode::Flatten {
                    if let Err(e) = fs::create_dir_all(&dst) {
                        errors.push(format!("{}: {e}", dst.display()));
                    }
                }
            } else {
                if let Some(parent) = dst.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                let copy_res = if folder_source::is_writable(&source_root) {
                    crate::file_ops::resolve(&source_root, rel)
                        .and_then(|src| fs::copy(&src, &dst).map(|_| ()).map_err(|e| e.to_string()))
                } else {
                    folder_source::read_entry_bytes(&source_root, rel)
                        .and_then(|bytes| crate::document::atomic_write(&dst, &bytes))
                };
                if let Err(e) = copy_res {
                    errors.push(format!("{}: {e}", dst.display()));
                }
            }
        }
        if is_move && errors.is_empty() {
            for p in source_roots_to_remove {
                let r = if p.is_dir() {
                    fs::remove_dir_all(&p)
                } else {
                    fs::remove_file(&p)
                };
                if let Err(e) = r {
                    errors.push(format!("{}: {e}", p.display()));
                }
            }
        }
        if !errors.is_empty() {
            dialogs.error(&errors.join("\n"));
        }
        if is_move {
            self.start_scan();
        }
    }

    fn open_delete_modal(&mut self, dialogs: &mut dyn super::dialogs::DialogHost) {
        if self.is_scanning() || self.selected.is_empty() {
            return;
        }
        let left_w = folder_source::is_writable(&self.loaded_left);
        let right_w = folder_source::is_writable(&self.loaded_right);
        if !left_w && !right_w {
            dialogs.error(&crate::i18n::tr("folder.archive_read_only"));
            return;
        }
        let stats = self.selection_stats();
        self.modal = Some(FolderOpModal::Delete {
            stats,
            delete_left: self.active_left && left_w && (stats.left_files + stats.left_dirs > 0),
            delete_right: !self.active_left && right_w && (stats.right_files + stats.right_dirs > 0),
            use_recycle_bin: true,
        });
    }

    fn execute_delete(
        &mut self,
        delete_left: bool,
        delete_right: bool,
        use_recycle_bin: bool,
        dialogs: &mut dyn super::dialogs::DialogHost,
    ) {
        let mut all_paths = Vec::new();
        for (do_side, is_left) in [(delete_left, true), (delete_right, false)] {
            if !do_side {
                continue;
            }
            let root = if is_left {
                &self.loaded_left
            } else {
                &self.loaded_right
            };
            if !folder_source::is_writable(root) {
                dialogs.error(&crate::i18n::tr("folder.archive_read_only"));
                return;
            }
            match self.paths(is_left) {
                Ok(p) => all_paths.extend(p),
                Err(e) => {
                    dialogs.error(&e);
                    return;
                }
            }
        }
        if all_paths.is_empty() {
            return;
        }
        if use_recycle_bin {
            if let Err(e) = dialogs.recycle(&all_paths) {
                dialogs.error(&e);
                return;
            }
        } else {
            let mut errs = Vec::new();
            for p in &all_paths {
                let r = if p.is_dir() {
                    fs::remove_dir_all(p)
                } else {
                    fs::remove_file(p)
                };
                if let Err(e) = r {
                    errs.push(format!("{}: {e}", p.display()));
                }
            }
            if !errs.is_empty() {
                dialogs.error(&errs.join("\n"));
            }
        }
        self.start_scan();
    }

    fn open_rename_modal(&mut self, dialogs: &mut dyn super::dialogs::DialogHost) {
        if self.is_scanning() || self.selected.is_empty() {
            return;
        }
        let root = if self.active_left {
            &self.loaded_left
        } else {
            &self.loaded_right
        };
        if !folder_source::is_writable(root) {
            dialogs.error(&crate::i18n::tr("folder.archive_read_only"));
            return;
        }
        let paths = match self.paths(self.active_left) {
            Ok(p) => p,
            Err(e) => {
                dialogs.error(&e);
                return;
            }
        };
        if paths.is_empty() {
            return;
        }
        let sample_names: Vec<String> = paths
            .iter()
            .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
            .collect();
        let single_item = sample_names.len() == 1;
        let (old_mask, new_mask) = if single_item {
            (sample_names[0].clone(), sample_names[0].clone())
        } else {
            ("*.*".to_owned(), "*.*".to_owned())
        };
        self.modal = Some(FolderOpModal::Rename {
            old_mask,
            new_mask,
            mode: RenameMode::Dos,
            sample_names,
            single_item,
        });
    }

    fn execute_rename(
        &mut self,
        old_mask: &str,
        new_mask: &str,
        mode: RenameMode,
        dialogs: &mut dyn super::dialogs::DialogHost,
    ) {
        let paths = match self.paths(self.active_left) {
            Ok(p) => p,
            Err(e) => {
                dialogs.error(&e);
                return;
            }
        };
        if paths.is_empty() {
            return;
        }
        let single = paths.len() == 1;
        let mut errs = Vec::new();
        let mut renamed_any = false;
        for src in paths {
            let Some(orig_name) = src.file_name().map(|n| n.to_string_lossy().into_owned()) else {
                continue;
            };
            let next_name = if single {
                new_mask.trim().to_owned()
            } else {
                match folder_ops::apply_rename_mask(&orig_name, old_mask, new_mask, mode) {
                    Ok(n) => n,
                    Err(e) => {
                        errs.push(e);
                        break;
                    }
                }
            };
            if next_name == orig_name || next_name.is_empty() {
                continue;
            }
            let res = (|| -> Result<(), String> {
                let leaf = crate::file_ops::leaf(&next_name)?;
                let dst = src.parent().unwrap().join(leaf);
                if dst.exists() {
                    return Err(crate::i18n::tr(
                        "folder.an_item_with_this_name_already_exists",
                    ));
                }
                fs::rename(&src, &dst).map_err(|e| format!("{}: {e}", src.display()))
            })();
            match res {
                Ok(()) => renamed_any = true,
                Err(e) => errs.push(e),
            }
        }
        if !errs.is_empty() {
            dialogs.error(&errs.join("\n"));
        }
        if renamed_any {
            self.start_scan();
        }
    }

    fn open_attributes_modal(&mut self, dialogs: &mut dyn super::dialogs::DialogHost) {
        if self.is_scanning() || self.selected.is_empty() {
            return;
        }
        let root = if self.active_left {
            &self.loaded_left
        } else {
            &self.loaded_right
        };
        if !folder_source::is_writable(root) {
            dialogs.error(&crate::i18n::tr("folder.archive_read_only"));
            return;
        }
        let paths = self.paths(self.active_left).unwrap_or_default();
        let spec = folder_ops::inspect_dos_attributes(&paths);
        self.modal = Some(FolderOpModal::Attributes {
            stats: self.selection_stats(),
            left_side: self.active_left,
            spec,
        });
    }

    fn execute_attributes(
        &mut self,
        left_side: bool,
        spec: DosAttributesSpec,
        dialogs: &mut dyn super::dialogs::DialogHost,
    ) {
        let root = if left_side {
            &self.loaded_left
        } else {
            &self.loaded_right
        };
        if !folder_source::is_writable(root) {
            dialogs.error(&crate::i18n::tr("folder.archive_read_only"));
            return;
        }
        let targets: Vec<PathBuf> = if spec.recursive {
            self.selected_subtree_indices(false)
                .into_iter()
                .filter_map(|idx| {
                    let e = &self.entries[idx];
                    let rel = if left_side {
                        e.left.as_ref().map(|_| &e.relative)
                    } else {
                        e.right.as_ref().map(|_| &e.right_relative)
                    }?;
                    crate::file_ops::resolve(root, rel).ok()
                })
                .collect()
        } else {
            self.paths(left_side).unwrap_or_default()
        };
        let mut errs = Vec::new();
        for p in &targets {
            if let Err(e) = folder_ops::apply_dos_attributes(p, spec) {
                errs.push(e);
            }
        }
        if !errs.is_empty() {
            dialogs.error(&errs.join("\n"));
        } else {
            self.start_scan();
        }
    }

    fn open_touch_modal(&mut self, dialogs: &mut dyn super::dialogs::DialogHost) {
        if self.is_scanning() || self.selected.is_empty() {
            return;
        }
        let root = if self.active_left {
            &self.loaded_left
        } else {
            &self.loaded_right
        };
        if !folder_source::is_writable(root) {
            dialogs.error(&crate::i18n::tr("folder.archive_read_only"));
            return;
        }
        let can_copy_other = self.selected_subtree_indices(false).into_iter().any(|idx| {
            let e = &self.entries[idx];
            e.left.is_some() && e.right.is_some()
        });
        let initial = self.last_touch_time.unwrap_or_else(SystemTime::now);
        self.modal = Some(FolderOpModal::Touch {
            stats: self.selection_stats(),
            left_side: self.active_left,
            mode: if can_copy_other {
                TouchMode::CopyOtherSide
            } else {
                TouchMode::SetDateTime
            },
            datetime_text: folder_ops::format_datetime(initial),
            recursive: true,
            can_copy_other,
        });
    }

    fn execute_touch(
        &mut self,
        left_side: bool,
        mode: TouchMode,
        datetime_text: &str,
        recursive: bool,
        dialogs: &mut dyn super::dialogs::DialogHost,
    ) {
        let root = if left_side {
            &self.loaded_left
        } else {
            &self.loaded_right
        };
        if !folder_source::is_writable(root) {
            dialogs.error(&crate::i18n::tr("folder.archive_read_only"));
            return;
        }
        let custom_time = match mode {
            TouchMode::SetDateTime => match folder_ops::parse_touch_datetime(datetime_text) {
                Ok(t) => {
                    self.last_touch_time = Some(t);
                    Some(t)
                }
                Err(e) => {
                    dialogs.error(&e);
                    return;
                }
            },
            TouchMode::CopyOtherSide => None,
        };
        let indices: Vec<usize> = if recursive {
            self.selected_subtree_indices(false)
        } else {
            self.entries
                .iter()
                .enumerate()
                .filter_map(|(i, e)| self.selected.contains(&e.id).then_some(i))
                .collect()
        };
        let mut errs = Vec::new();
        for idx in indices {
            let e = &self.entries[idx];
            let (own_rel, other_meta) = if left_side {
                if e.left.is_none() {
                    continue;
                }
                (&e.relative, &e.right)
            } else {
                if e.right.is_none() {
                    continue;
                }
                (&e.right_relative, &e.left)
            };
            let target_time = match mode {
                TouchMode::SetDateTime => custom_time,
                TouchMode::CopyOtherSide => other_meta.as_ref().and_then(EntryMeta::modified),
            };
            let Some(time) = target_time else {
                continue;
            };
            match crate::file_ops::resolve(root, own_rel) {
                Ok(p) => {
                    if let Err(err) = folder_ops::apply_touch_time(&p, time) {
                        errs.push(err);
                    }
                }
                Err(err) => errs.push(err),
            }
        }
        if !errs.is_empty() {
            dialogs.error(&errs.join("\n"));
        }
        self.start_scan();
    }

    fn open_exclude_modal(&mut self) {
        if self.selected.is_empty() {
            return;
        }
        let mut names = Vec::new();
        let mut relative_paths = Vec::new();
        for e in self
            .entries
            .iter()
            .filter(|e| self.selected.contains(&e.id))
        {
            if let Some(n) = e.relative.file_name().map(|s| s.to_string_lossy().into_owned()) {
                if !names.contains(&n) {
                    names.push(n);
                }
            }
            let rel = e.relative.to_string_lossy().replace('\\', "/");
            if !relative_paths.contains(&rel) {
                relative_paths.push(rel);
            }
        }
        if !names.is_empty() {
            self.modal = Some(FolderOpModal::Exclude {
                names,
                relative_paths,
                only_this_folder: false,
            });
        }
    }

    fn open_new_folder_modal(&mut self, dialogs: &mut dyn super::dialogs::DialogHost) {
        if self.is_scanning() {
            return;
        }
        let left_w = folder_source::is_writable(&self.loaded_left);
        let right_w = folder_source::is_writable(&self.loaded_right);
        if !left_w && !right_w {
            dialogs.error(&crate::i18n::tr("folder.archive_read_only"));
            return;
        }
        self.modal = Some(FolderOpModal::NewFolder {
            name: String::new(),
            create_left: self.active_left && left_w,
            create_right: !self.active_left && right_w,
            left_writable: left_w,
            right_writable: right_w,
        });
    }

    fn execute_new_folder(
        &mut self,
        name: &str,
        create_left: bool,
        create_right: bool,
        dialogs: &mut dyn super::dialogs::DialogHost,
    ) {
        let leaf = match crate::file_ops::leaf(name) {
            Ok(l) => l,
            Err(e) => {
                dialogs.error(&e);
                return;
            }
        };
        // If a single directory is selected, create inside it; otherwise at base root.
        let parent_rel: Option<PathBuf> = (self.selected.len() == 1)
            .then(|| {
                self.entries
                    .iter()
                    .find(|e| self.selected.contains(&e.id) && is_dir(e))
                    .map(|e| e.relative.clone())
            })
            .flatten();
        let mut errs = Vec::new();
        for (do_side, root) in [
            (create_left, &self.loaded_left),
            (create_right, &self.loaded_right),
        ] {
            if !do_side || !folder_source::is_writable(root) {
                continue;
            }
            let rel = match &parent_rel {
                Some(p) => p.join(leaf),
                None => PathBuf::from(leaf),
            };
            match crate::file_ops::resolve(root, &rel) {
                Ok(p) => {
                    if let Err(e) = fs::create_dir_all(&p) {
                        errs.push(format!("{}: {e}", p.display()));
                    }
                }
                Err(e) => errs.push(e),
            }
        }
        if !errs.is_empty() {
            dialogs.error(&errs.join("\n"));
        }
        self.start_scan();
    }

    fn toggle_ignored(&mut self) {
        let indices = self.selected_subtree_indices(false);
        if indices.is_empty() {
            return;
        }
        let all_ignored = indices.iter().all(|&i| self.entries[i].ignored);
        let new_state = !all_ignored;
        for idx in indices {
            self.entries[idx].ignored = new_state;
            let key = path_key(&self.entries[idx].relative);
            if new_state {
                self.ignored_paths.insert(key);
            } else {
                self.ignored_paths.remove(&key);
            }
        }
        recompute_dir_summaries(&mut self.entries);
    }

    fn refresh_selection(&mut self) {
        if self.is_scanning() {
            return;
        }
        if !folder_source::is_writable(&self.loaded_left)
            || !folder_source::is_writable(&self.loaded_right)
        {
            self.start_scan();
            return;
        }
        let indices = self.selected_subtree_indices(true);
        if indices.is_empty() {
            return;
        }
        let cancel = AtomicBool::new(false);
        for idx in indices {
            let e = &mut self.entries[idx];
            e.left = crate::file_ops::resolve(&self.loaded_left, &e.relative)
                .ok()
                .and_then(|p| fs::metadata(p).ok())
                .map(|m| EntryMeta::from_fs(&m));
            e.right = crate::file_ops::resolve(&self.loaded_right, &e.right_relative)
                .ok()
                .and_then(|p| fs::metadata(p).ok())
                .map(|m| EntryMeta::from_fs(&m));
            e.error = None;
            e.is_minor = false;
            e.content_compared = false;
            e.status = match (&e.left, &e.right) {
                (None, None) => Status::Same,
                (Some(_), None) => Status::LeftOnly,
                (None, Some(_)) => Status::RightOnly,
                (Some(l), Some(r)) if l.is_dir() && r.is_dir() => Status::Same,
                (Some(l), Some(r)) if l.is_file() && r.is_file() => {
                    if self.compare_content {
                        match folder_ops::compare_file_pair(
                            &self.loaded_left,
                            &e.relative,
                            l,
                            &self.loaded_right,
                            &e.right_relative,
                            r,
                            self.content_mode,
                            &cancel,
                        ) {
                            FileContentVerdict::Same {
                                left_crc,
                                right_crc,
                                ..
                            } => {
                                e.content_compared = true;
                                e.left_crc32 = left_crc;
                                e.right_crc32 = right_crc;
                                Status::Same
                            }
                            FileContentVerdict::Minor { .. } => {
                                e.content_compared = true;
                                e.is_minor = true;
                                Status::Different
                            }
                            FileContentVerdict::Different {
                                left_crc,
                                right_crc,
                                ..
                            } => {
                                e.content_compared = true;
                                e.left_crc32 = left_crc;
                                e.right_crc32 = right_crc;
                                Status::Different
                            }
                            FileContentVerdict::Error(err) => {
                                e.error = Some(err);
                                Status::Error
                            }
                        }
                    } else if l.len() != r.len() {
                        Status::Different
                    } else {
                        let lt = l.modified().unwrap_or(SystemTime::UNIX_EPOCH);
                        let rt = r.modified().unwrap_or(SystemTime::UNIX_EPOCH);
                        if lt == rt {
                            Status::Same
                        } else {
                            let diff_secs = lt
                                .duration_since(rt)
                                .or_else(|_| rt.duration_since(lt))
                                .map_or(u64::MAX, |d| d.as_secs());
                            if diff_secs <= 2 || diff_secs == 3600 {
                                e.is_minor = true;
                            }
                            Status::Different
                        }
                    }
                }
                _ => Status::Different,
            };
        }
        self.entries
            .retain(|e| e.left.is_some() || e.right.is_some());
        recompute_dir_summaries(&mut self.entries);
    }

    fn open_selected_folders(&mut self, open: bool, recursive: bool) {
        let selected_dirs: Vec<PathBuf> = self
            .entries
            .iter()
            .filter(|e| self.selected.contains(&e.id) && is_dir(e))
            .map(|e| e.relative.clone())
            .collect();
        for entry in &self.entries {
            if !is_dir(entry) {
                continue;
            }
            let matches = if recursive {
                selected_dirs
                    .iter()
                    .any(|d| entry.relative == *d || entry.relative.starts_with(d))
            } else {
                self.selected.contains(&entry.id)
            };
            if matches {
                let key = path_key(&entry.relative);
                if open {
                    self.collapsed.remove(&key);
                } else {
                    self.collapsed.insert(key);
                }
            }
        }
    }

    fn set_selected_as_base_folders(&mut self) {
        let Some(entry) = self
            .entries
            .iter()
            .find(|e| self.selected.contains(&e.id) && is_dir(e))
        else {
            return;
        };
        if !folder_source::is_writable(&self.loaded_left)
            || !folder_source::is_writable(&self.loaded_right)
        {
            return;
        }
        let next_left = entry
            .left
            .as_ref()
            .and_then(|_| crate::file_ops::resolve(&self.loaded_left, &entry.relative).ok());
        let next_right = entry
            .right
            .as_ref()
            .and_then(|_| crate::file_ops::resolve(&self.loaded_right, &entry.right_relative).ok());
        if let Some(l) = next_left {
            self.left_path = l.to_string_lossy().into_owned();
        }
        if let Some(r) = next_right {
            self.right_path = r.to_string_lossy().into_owned();
        }
        self.start_scan();
    }

    fn first_selected_file_pair(
        &self,
        dialogs: &mut dyn super::dialogs::DialogHost,
    ) -> Option<(PathBuf, PathBuf)> {
        let entry = self
            .entries
            .iter()
            .find(|e| self.selected.contains(&e.id) && !is_dir(e))?;
        let l = entry
            .left
            .as_ref()
            .filter(|m| m.is_file())
            .and_then(|_| {
                folder_source::materialize_file(&self.loaded_left, &entry.relative)
                    .map_err(|err| dialogs.error(&err))
                    .ok()
            });
        let r = entry
            .right
            .as_ref()
            .filter(|m| m.is_file())
            .and_then(|_| {
                folder_source::materialize_file(&self.loaded_right, &entry.right_relative)
                    .map_err(|err| dialogs.error(&err))
                    .ok()
            });
        match (l, r) {
            (Some(lp), Some(rp)) => Some((lp, rp)),
            (Some(lp), None) => Some((lp, PathBuf::new())),
            (None, Some(rp)) => Some((PathBuf::new(), rp)),
            (None, None) => None,
        }
    }

    fn quick_sync_selection(
        &mut self,
        preset: QuickSyncPreset,
        dialogs: &mut dyn super::dialogs::DialogHost,
    ) {
        if self.is_scanning() {
            return;
        }
        if !folder_source::is_writable(&self.loaded_left)
            || !folder_source::is_writable(&self.loaded_right)
        {
            dialogs.error(&crate::i18n::tr("folder.archive_read_only"));
            return;
        }
        let indices = self.selected_subtree_indices(false);
        let mut copy_l_to_r = Vec::new();
        let mut copy_r_to_l = Vec::new();
        let mut del_left = Vec::new();
        let mut del_right = Vec::new();
        for idx in indices {
            let e = &self.entries[idx];
            if is_dir(e) || e.ignored || e.status == Status::Same {
                continue;
            }
            let (lt, rt) = (modified_secs(&e.left), modified_secs(&e.right));
            let l_newer = matches!((lt, rt), (Some(l), Some(r)) if l > r);
            let r_newer = matches!((lt, rt), (Some(l), Some(r)) if r > l);
            match preset {
                QuickSyncPreset::UpdateRight => {
                    if e.status == Status::LeftOnly || (e.status == Status::Different && l_newer) {
                        copy_l_to_r.push(e.relative.clone());
                    }
                }
                QuickSyncPreset::UpdateLeft => {
                    if e.status == Status::RightOnly || (e.status == Status::Different && r_newer) {
                        copy_r_to_l.push(e.right_relative.clone());
                    }
                }
                QuickSyncPreset::UpdateBoth => {
                    if e.status == Status::LeftOnly || (e.status == Status::Different && l_newer) {
                        copy_l_to_r.push(e.relative.clone());
                    } else if e.status == Status::RightOnly
                        || (e.status == Status::Different && r_newer)
                    {
                        copy_r_to_l.push(e.right_relative.clone());
                    }
                }
                QuickSyncPreset::MirrorRight => {
                    if e.status == Status::LeftOnly || e.status == Status::Different {
                        copy_l_to_r.push(e.relative.clone());
                    } else if e.status == Status::RightOnly {
                        if let Ok(p) = crate::file_ops::resolve(&self.loaded_right, &e.right_relative) {
                            del_right.push(p);
                        }
                    }
                }
                QuickSyncPreset::MirrorLeft => {
                    if e.status == Status::RightOnly || e.status == Status::Different {
                        copy_r_to_l.push(e.right_relative.clone());
                    } else if e.status == Status::LeftOnly {
                        if let Ok(p) = crate::file_ops::resolve(&self.loaded_left, &e.relative) {
                            del_left.push(p);
                        }
                    }
                }
            }
        }
        let total = copy_l_to_r.len() + copy_r_to_l.len() + del_left.len() + del_right.len();
        if total == 0 {
            dialogs.info(&crate::i18n::tr("sync.there_are_no_pending_changes_to_sync"));
            return;
        }
        let msg = format!(
            "{}\n\n→ {}: {} | ← {}: {} | ×: {}",
            crate::i18n::tr("sync.run_sync_now"),
            crate::i18n::tr("common.right"),
            copy_l_to_r.len(),
            crate::i18n::tr("common.left"),
            copy_r_to_l.len(),
            del_left.len() + del_right.len(),
        );
        if !dialogs.confirm(&msg) {
            return;
        }
        let cancel = AtomicBool::new(false);
        let mut errs = Vec::new();
        if !copy_l_to_r.is_empty() {
            match crate::file_ops::CopyPlan::build(&self.loaded_left, &self.loaded_right, &copy_l_to_r)
                .and_then(|p| p.execute(&cancel))
            {
                Ok(_) => {}
                Err(e) => errs.push(e),
            }
        }
        if !copy_r_to_l.is_empty() {
            match crate::file_ops::CopyPlan::build(&self.loaded_right, &self.loaded_left, &copy_r_to_l)
                .and_then(|p| p.execute(&cancel))
            {
                Ok(_) => {}
                Err(e) => errs.push(e),
            }
        }
        let to_del: Vec<PathBuf> = del_left.into_iter().chain(del_right).collect();
        if !to_del.is_empty() {
            if let Err(e) = dialogs.recycle(&to_del) {
                errs.push(e);
            }
        }
        if !errs.is_empty() {
            dialogs.error(&errs.join("\n"));
        }
        self.start_scan();
    }

    fn handle_context_action(
        &mut self,
        ctx: &Context,
        cmd: ContextAction,
        dialogs: &mut dyn super::dialogs::DialogHost,
        action: &mut FolderAction,
    ) {
        match cmd {
            ContextAction::OpenFolder(open) => self.open_selected_folders(open, false),
            ContextAction::OpenSubfolders(open) => self.open_selected_folders(open, true),
            ContextAction::SetAsBaseFolders => self.set_selected_as_base_folders(),
            ContextAction::OpenInNewView => {
                if let Some(dir_entry) = self
                    .entries
                    .iter()
                    .find(|e| self.selected.contains(&e.id) && is_dir(e))
                {
                    let l = dir_entry
                        .left
                        .as_ref()
                        .and_then(|_| crate::file_ops::resolve(&self.loaded_left, &dir_entry.relative).ok())
                        .unwrap_or_default();
                    let r = dir_entry
                        .right
                        .as_ref()
                        .and_then(|_| {
                            crate::file_ops::resolve(&self.loaded_right, &dir_entry.right_relative).ok()
                        })
                        .unwrap_or_default();
                    if !l.as_os_str().is_empty() || !r.as_os_str().is_empty() {
                        super::session_menu::request(
                            ctx,
                            super::session_menu::Request::Recent(RecentSession::new(
                                SessionKind::Folder,
                                l,
                                r,
                            )),
                        );
                    }
                } else if let Some((l, r)) = self.first_selected_file_pair(dialogs) {
                    if !l.as_os_str().is_empty() && !r.as_os_str().is_empty() {
                        *action = FolderAction::OpenFiles(l, r);
                    } else {
                        let single = if !l.as_os_str().is_empty() { l } else { r };
                        super::session_menu::request(
                            ctx,
                            super::session_menu::Request::EditText(single),
                        );
                    }
                }
            }
            ContextAction::OpenFileDefault => {
                if let Some((l, r)) = self.first_selected_file_pair(dialogs) {
                    if !l.as_os_str().is_empty() && !r.as_os_str().is_empty() {
                        *action = FolderAction::OpenFiles(l, r);
                    } else {
                        let single = if !l.as_os_str().is_empty() { l } else { r };
                        super::session_menu::request(
                            ctx,
                            super::session_menu::Request::EditText(single),
                        );
                    }
                }
            }
            ContextAction::OpenWith(kind) => {
                if kind == SessionKind::TextEdit {
                    if let Ok(paths) = self.paths(self.active_left) {
                        if let Some(first) = paths.into_iter().find(|p| p.is_file()) {
                            super::session_menu::request(
                                ctx,
                                super::session_menu::Request::EditText(first),
                            );
                        }
                    }
                } else if let Some((l, r)) = self.first_selected_file_pair(dialogs) {
                    super::session_menu::request(
                        ctx,
                        super::session_menu::Request::Recent(RecentSession::new(kind, l, r)),
                    );
                }
            }
            ContextAction::OpenWithAssocApp => {
                if let Ok(paths) = self.paths(self.active_left) {
                    for p in paths {
                        if let Err(err) = dialogs.open_path(&p, false) {
                            dialogs.error(&err);
                        }
                    }
                }
            }
            ContextAction::CompareContents => self.open_compare_contents_modal(),
            ContextAction::CopyToSide(to_right) => self.open_copy_side_modal(to_right, dialogs),
            ContextAction::MoveToSide(to_right) => self.open_move_side_modal(to_right, dialogs),
            ContextAction::CopyToFolder(src_left) => {
                self.open_copy_to_folder_modal(src_left, false, dialogs);
            }
            ContextAction::MoveToFolder(src_left) => {
                self.open_copy_to_folder_modal(src_left, true, dialogs);
            }
            ContextAction::Delete => self.open_delete_modal(dialogs),
            ContextAction::Rename => self.open_rename_modal(dialogs),
            ContextAction::Attributes => self.open_attributes_modal(dialogs),
            ContextAction::Touch => self.open_touch_modal(dialogs),
            ContextAction::Exclude => self.open_exclude_modal(),
            ContextAction::ClearExclusions => self.exclude_patterns.clear(),
            ContextAction::NewFolder => self.open_new_folder_modal(dialogs),
            ContextAction::CopyFilename => {
                if let Ok(paths) = self.paths(self.active_left) {
                    ctx.copy_text(
                        paths
                            .iter()
                            .map(|p| p.display().to_string())
                            .collect::<Vec<_>>()
                            .join("\n"),
                    );
                }
            }
            ContextAction::ToggleIgnored => self.toggle_ignored(),
            ContextAction::RefreshSelection => self.refresh_selection(),
            ContextAction::QuickSync(preset) => self.quick_sync_selection(preset, dialogs),
            ContextAction::ExplorerSelect => {
                if let Ok(paths) = self.paths(self.active_left) {
                    for p in paths {
                        if let Err(err) = dialogs.open_path(&p, true) {
                            dialogs.error(&err);
                        }
                    }
                }
            }
            ContextAction::ExplorerOpenFolder => {
                let target = if self.active_left {
                    &self.loaded_left
                } else {
                    &self.loaded_right
                };
                let folder = self
                    .paths(self.active_left)
                    .ok()
                    .and_then(|mut ps| ps.pop())
                    .map(|p| {
                        if p.is_dir() {
                            p
                        } else {
                            p.parent().unwrap_or(target).to_path_buf()
                        }
                    })
                    .unwrap_or_else(|| target.clone());
                if let Err(err) = dialogs.open_path(&folder, false) {
                    dialogs.error(&err);
                }
            }
        }
    }

    fn handle_modal_confirmed(
        &mut self,
        ctx: &Context,
        confirmed: FolderOpConfirmed,
        dialogs: &mut dyn super::dialogs::DialogHost,
        action: &mut FolderAction,
    ) {
        match confirmed {
            FolderOpConfirmed::CompareContents {
                mode,
                show_results,
                dont_show_again,
                include_hidden,
            } => {
                self.dont_show_compare_dialog = dont_show_again;
                self.start_content_compare(mode, show_results, include_hidden);
            }
            FolderOpConfirmed::CopyToSide {
                to_right,
                dont_show_again,
            } => {
                self.dont_show_copy_dialog = dont_show_again;
                self.execute_copy_to_side_confirmed(to_right, dialogs);
            }
            FolderOpConfirmed::MoveToSide {
                to_right,
                dont_show_again,
            } => {
                self.dont_show_move_dialog = dont_show_again;
                self.execute_move_to_side_confirmed(to_right, dialogs);
            }
            FolderOpConfirmed::CopyToFolder {
                source_left,
                target_dir,
                structure,
            } => {
                self.execute_external_transfer(source_left, target_dir, structure, false, dialogs);
            }
            FolderOpConfirmed::MoveToFolder {
                source_left,
                target_dir,
                structure,
            } => {
                self.execute_external_transfer(source_left, target_dir, structure, true, dialogs);
            }
            FolderOpConfirmed::Delete {
                delete_left,
                delete_right,
                use_recycle_bin,
            } => {
                self.execute_delete(delete_left, delete_right, use_recycle_bin, dialogs);
            }
            FolderOpConfirmed::Rename {
                old_mask,
                new_mask,
                mode,
            } => {
                self.execute_rename(&old_mask, &new_mask, mode, dialogs);
            }
            FolderOpConfirmed::Attributes { left_side, spec } => {
                self.execute_attributes(left_side, spec, dialogs);
            }
            FolderOpConfirmed::Touch {
                left_side,
                mode,
                datetime_text,
                recursive,
            } => {
                self.execute_touch(left_side, mode, &datetime_text, recursive, dialogs);
            }
            FolderOpConfirmed::Exclude { patterns } => {
                for pat in patterns {
                    if !self.exclude_patterns.contains(&pat) {
                        self.exclude_patterns.push(pat);
                    }
                }
            }
            FolderOpConfirmed::NewFolder {
                name,
                create_left,
                create_right,
            } => {
                self.execute_new_folder(&name, create_left, create_right, dialogs);
            }
            FolderOpConfirmed::OpenResultRow {
                left_rel,
                right_rel,
                status,
            } => match status {
                CompareRowStatus::LeftOrphan => {
                    match folder_source::materialize_file(&self.loaded_left, &left_rel) {
                        Ok(l_path) => super::session_menu::request(
                            ctx,
                            super::session_menu::Request::EditText(l_path),
                        ),
                        Err(err) => dialogs.error(&err),
                    }
                }
                CompareRowStatus::RightOrphan => {
                    match folder_source::materialize_file(&self.loaded_right, &right_rel) {
                        Ok(r_path) => super::session_menu::request(
                            ctx,
                            super::session_menu::Request::EditText(r_path),
                        ),
                        Err(err) => dialogs.error(&err),
                    }
                }
                _ => match (
                    folder_source::materialize_file(&self.loaded_left, &left_rel),
                    folder_source::materialize_file(&self.loaded_right, &right_rel),
                ) {
                    (Ok(l_path), Ok(r_path)) => {
                        *action = FolderAction::OpenFiles(l_path, r_path);
                    }
                    (Err(err), _) | (_, Err(err)) => dialogs.error(&err),
                },
            },
        }
    }

    pub fn ui(
        &mut self,
        ctx: &Context,
        ui: &mut Ui,
        icons: &mut IconManager,
        dialogs: &mut dyn super::dialogs::DialogHost,
    ) -> FolderAction {
        let mut action = FolderAction::None;
        let busy = self.is_scanning();
        let can_operate = !busy && !self.selected.is_empty();
        let modal_open = self.modal.is_some();
        let typing = ctx.wants_keyboard_input();

        if !modal_open && !typing {
            if !busy && ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::A)) {
                self.selection_focus = true;
                self.selected = self
                    .visible_indices()
                    .into_iter()
                    .map(|index| self.entries[index].id.clone())
                    .collect();
            }
            if can_operate {
                if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::R)) {
                    self.open_copy_side_modal(true, dialogs);
                }
                if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::L)) {
                    self.open_copy_side_modal(false, dialogs);
                }
                if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::F2)) {
                    self.open_rename_modal(dialogs);
                }
                if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Delete)) {
                    self.open_delete_modal(dialogs);
                }
                if ctx.input_mut(|i| i.consume_key(Modifiers::SHIFT, Key::F5)) {
                    self.refresh_selection();
                }
                if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::C)) {
                    if let Ok(paths) = self.paths(self.active_left) {
                        ctx.copy_text(
                            paths
                                .iter()
                                .map(|p| p.display().to_string())
                                .collect::<Vec<_>>()
                                .join("\n"),
                        );
                    }
                }
            }
            if !busy && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Insert)) {
                self.open_new_folder_modal(dialogs);
            }
            if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::N)) {
                self.navigate(true, None);
            }
            if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::P)) {
                self.navigate(false, None);
            }
            if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::F)) {
                if let Some(query) =
                    dialogs.input("folder.find_filename", "folder.name_or_substring", &self.find_text)
                {
                    self.find_text = query;
                    let query = self.find_text.clone();
                    self.navigate(true, Some(&query));
                }
            }
            let previous = ctx.input_mut(|i| i.consume_key(Modifiers::SHIFT, Key::F3));
            if previous || ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::F3)) {
                let query = self.find_text.clone();
                self.navigate(!previous, Some(&query));
            }
            if ctx.input_mut(|input| {
                input.consume_key(Modifiers::CTRL, Key::F5)
                    || input.consume_key(Modifiers::NONE, Key::F5)
            }) {
                self.start_scan();
            }
        }
        if let Some(receiver) = &self.pending {
            match receiver.try_recv() {
                Ok(scan) => {
                    self.accept_scan(scan);
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.pending = None;
                    self.errors.push("folder.the_comparison_was_interrupted".into());
                }
                Err(mpsc::TryRecvError::Empty) => {
                    ctx.request_repaint_after(std::time::Duration::from_millis(100))
                }
            }
        }
        if let Some(receiver) = &self.pending_content {
            match receiver.try_recv() {
                Ok(outcome) => {
                    self.accept_content_compare(outcome);
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.pending_content = None;
                }
                Err(mpsc::TryRecvError::Empty) => {
                    ctx.request_repaint_after(std::time::Duration::from_millis(80))
                }
            }
        }
        TopBottomPanel::top("folder_menu")
            .exact_height(super::chrome::MENU_BAND_HEIGHT)
            .show_separator_line(false)
            .frame(Frame::none())
            .show_inside(ui, |ui| {
                super::chrome::menu_background(ui);
                super::chrome::menu_bar(ui, |ui| {
                    super::chrome::menu_button(ui, "menu.session", |ui| {
                        super::session_menu::prefix(
                            ui,
                            icons,
                            !self.left_path.is_empty() && !self.right_path.is_empty(),
                        );
                        if super::chrome::menu_item(ui, icons, "menu.session.settings", true)
                            .clicked()
                        {
                            if dialogs.persist_settings() {
                                super::session_settings::request(ui.ctx());
                            } else {
                                self.options(dialogs);
                            }
                            ui.close_menu();
                        }
                        super::session_menu::close(ui, icons);
                        ui.separator();
                        if super::chrome::menu_item(ui, icons, "folder.report", true)
                            .clicked()
                        {
                            self.report(dialogs);
                            ui.close_menu();
                        }
                        if super::chrome::menu_item(ui, icons, "folder.compare_info", true)
                            .clicked()
                        {
                            if dialogs.persist_settings() {
                                self.open_compare_info_modal(ctx);
                            } else {
                                dialogs.info(&crate::i18n::tr_fmt(
                                    "status.left_right_items_errors",
                                    &[
                                        ("", &self.left_path),
                                        ("", &self.right_path),
                                        ("", &self.entries.len().to_string()),
                                        ("", &self.errors.len().to_string()),
                                    ],
                                ));
                            }
                            ui.close_menu();
                        }
                        let has_both =
                            !self.left_path.is_empty() && !self.right_path.is_empty();
                        if super::chrome::menu_item(
                            ui,
                            icons,
                            "folder.sync_base_folders",
                            has_both,
                        )
                        .clicked()
                        {
                            super::session_menu::request(
                                ui.ctx(),
                                super::session_menu::Request::SyncBaseFolders(
                                    PathBuf::from(&self.left_path),
                                    PathBuf::from(&self.right_path),
                                ),
                            );
                            ui.close_menu();
                        }
                        if super::chrome::menu_item(
                            ui,
                            icons,
                            "folder.merge_base_folders",
                            has_both,
                        )
                        .clicked()
                        {
                            super::session_menu::request(
                                ui.ctx(),
                                super::session_menu::Request::MergeBaseFolders(
                                    PathBuf::from(&self.left_path),
                                    PathBuf::from(&self.right_path),
                                ),
                            );
                            ui.close_menu();
                        }
                        if super::chrome::menu_item(ui, icons, "session.home", true).clicked() {
                            action = FolderAction::Home;
                            ui.close_menu();
                        }
                        if super::chrome::menu_item(ui, icons, "menu.edit.swap_sides", true).clicked() {
                            std::mem::swap(&mut self.left_path, &mut self.right_path);
                            self.start_scan();
                            ui.close_menu();
                        }
                        super::session_menu::suffix(
                            ui,
                            icons,
                            !self.left_path.is_empty() && !self.right_path.is_empty(),
                        );
                    });
                    super::chrome::menu_button(ui, "menu.actions", |ui| {
                        let any_ignored = self
                            .entries
                            .iter()
                            .any(|e| self.selected.contains(&e.id) && e.ignored);
                        if super::chrome::menu_item(
                            ui,
                            icons,
                            "folder.ctx.compare_contents_ellipsis",
                            can_operate,
                        )
                        .clicked()
                        {
                            self.open_compare_contents_modal();
                            ui.close_menu();
                        }
                        ui.separator();
                        for (label, right) in [
                            ("menu.actions.copy_to_right  Ctrl+R", true),
                            ("menu.actions.copy_to_left  Ctrl+L", false),
                        ] {
                            if super::chrome::menu_item(ui, icons, label, can_operate).clicked() {
                                self.open_copy_side_modal(right, dialogs);
                                ui.close_menu();
                            }
                        }
                        if super::chrome::menu_item(
                            ui,
                            icons,
                            "folder.ctx.move_to_side_ellipsis",
                            can_operate,
                        )
                        .clicked()
                        {
                            self.open_move_side_modal(self.active_left, dialogs);
                            ui.close_menu();
                        }
                        if super::chrome::menu_item(
                            ui,
                            icons,
                            "folder.ctx.copy_to_folder_ellipsis",
                            can_operate,
                        )
                        .clicked()
                        {
                            self.open_copy_to_folder_modal(self.active_left, false, dialogs);
                            ui.close_menu();
                        }
                        if super::chrome::menu_item(
                            ui,
                            icons,
                            "folder.ctx.move_to_folder_ellipsis",
                            can_operate,
                        )
                        .clicked()
                        {
                            self.open_copy_to_folder_modal(self.active_left, true, dialogs);
                            ui.close_menu();
                        }
                        if super::chrome::menu_item(
                            ui,
                            icons,
                            "folder.ctx.delete_ellipsis  Delete",
                            can_operate,
                        )
                        .clicked()
                        {
                            self.open_delete_modal(dialogs);
                            ui.close_menu();
                        }
                        if super::chrome::menu_item(
                            ui,
                            icons,
                            "folder.rename_ellipsis  F2",
                            can_operate,
                        )
                        .clicked()
                        {
                            self.open_rename_modal(dialogs);
                            ui.close_menu();
                        }
                        if super::chrome::menu_item(
                            ui,
                            icons,
                            "folder.ctx.attributes_ellipsis",
                            can_operate,
                        )
                        .clicked()
                        {
                            self.open_attributes_modal(dialogs);
                            ui.close_menu();
                        }
                        if super::chrome::menu_item(
                            ui,
                            icons,
                            "folder.ctx.touch_ellipsis",
                            can_operate,
                        )
                        .clicked()
                        {
                            self.open_touch_modal(dialogs);
                            ui.close_menu();
                        }
                        if super::chrome::menu_item(ui, icons, "folder.ctx.exclude", can_operate)
                            .clicked()
                        {
                            self.open_exclude_modal();
                            ui.close_menu();
                        }
                        if super::chrome::menu_item(
                            ui,
                            icons,
                            "folder.new_folder_ellipsis  Insert",
                            !busy,
                        )
                        .clicked()
                        {
                            self.open_new_folder_modal(dialogs);
                            ui.close_menu();
                        }
                        if super::chrome::menu_check_item(
                            ui,
                            icons,
                            "folder.ctx.ignored",
                            any_ignored,
                            can_operate,
                        )
                        .clicked()
                        {
                            self.toggle_ignored();
                            ui.close_menu();
                        }
                        if super::chrome::menu_item(
                            ui,
                            icons,
                            "folder.ctx.refresh_selection  Shift+F5",
                            can_operate,
                        )
                        .clicked()
                        {
                            self.refresh_selection();
                            ui.close_menu();
                        }
                        let has_selected_dir = self
                            .entries
                            .iter()
                            .any(|e| self.selected.contains(&e.id) && is_dir(e));
                        if super::chrome::menu_item(
                            ui,
                            icons,
                            "folder.set_as_base_folder",
                            has_selected_dir,
                        )
                        .clicked()
                        {
                            self.set_selected_as_base_side(false);
                            ui.close_menu();
                        }
                        if super::chrome::menu_item(
                            ui,
                            icons,
                            "folder.set_base_other_side",
                            has_selected_dir,
                        )
                        .clicked()
                        {
                            self.set_selected_as_base_side(true);
                            ui.close_menu();
                        }
                        ui.separator();
                        super::chrome::submenu_item(ui, icons, "folder.ctx.synchronize", |ui, icons| {
                            for (label, preset) in [
                                ("sync.preset.update_left", QuickSyncPreset::UpdateLeft),
                                ("sync.preset.update_right", QuickSyncPreset::UpdateRight),
                                ("sync.preset.update_both", QuickSyncPreset::UpdateBoth),
                                ("sync.preset.mirror_left", QuickSyncPreset::MirrorLeft),
                                ("sync.preset.mirror_right", QuickSyncPreset::MirrorRight),
                            ] {
                                if super::chrome::menu_item(ui, icons, label, can_operate).clicked()
                                {
                                    self.quick_sync_selection(preset, dialogs);
                                    ui.close_menu();
                                }
                            }
                        });
                        ui.separator();
                        if super::chrome::menu_item(ui, icons, "menu.actions.compare_refresh", true)
                            .clicked()
                        {
                            self.start_scan();
                            ui.close_menu();
                        }
                        if super::chrome::menu_item(ui, icons, "common.stop", busy).clicked() {
                            self.cancel.store(true, Ordering::Relaxed);
                            ui.close_menu();
                        }
                        if super::chrome::menu_item(ui, icons, "folder.expand_all", true).clicked() {
                            self.collapsed.clear();
                            ui.close_menu();
                        }
                        if super::chrome::menu_item(ui, icons, "folder.collapse_all", true).clicked() {
                            self.collapsed = self
                                .entries
                                .iter()
                                .filter(|e| {
                                    e.left
                                        .as_ref()
                                        .or(e.right.as_ref())
                                        .is_some_and(|m| m.is_dir())
                                })
                                .map(|e| path_key(&e.relative))
                                .collect();
                            ui.close_menu();
                        }
                        if super::chrome::menu_item(ui, icons, "folder.open_in_explorer", can_operate)
                            .clicked()
                        {
                            match self.paths(self.active_left) {
                                Ok(paths) => {
                                    for path in paths {
                                        if let Err(error) = dialogs.open_path(&path, true) {
                                            dialogs.error(&error);
                                        }
                                    }
                                }
                                Err(error) => dialogs.error(&error),
                            }
                            ui.close_menu();
                        }
                    });
                    super::chrome::menu_button(ui, "menu.edit", |ui| {
                        if super::chrome::menu_item(ui, icons, "menu.edit.select_all  Ctrl+A", true)
                            .clicked()
                        {
                            self.selected = self
                                .visible_indices()
                                .into_iter()
                                .map(|index| &self.entries[index])
                                .map(|e| e.id.clone())
                                .collect();
                            ui.close_menu();
                        }
                        if super::chrome::menu_item(ui, icons, "folder.invert_selection", true).clicked()
                        {
                            self.selected = self
                                .visible_indices()
                                .into_iter()
                                .map(|index| &self.entries[index])
                                .filter(|e| !self.selected.contains(&e.id))
                                .map(|e| e.id.clone())
                                .collect();
                            ui.close_menu();
                        }
                        super::chrome::submenu_item(ui, icons, "folder.select_newer", |ui, icons| {
                            for (lbl, l, r) in [
                                ("folder.side_left", true, false),
                                ("folder.side_right", false, true),
                                ("folder.side_both", true, true),
                            ] {
                                if super::chrome::menu_item(ui, icons, lbl, true).clicked() {
                                    self.select_newer(l, r);
                                    ui.close_menu();
                                }
                            }
                        });
                        super::chrome::submenu_item(ui, icons, "folder.select_orphans", |ui, icons| {
                            for (lbl, l, r) in [
                                ("folder.side_left", true, false),
                                ("folder.side_right", false, true),
                                ("folder.side_both", true, true),
                            ] {
                                if super::chrome::menu_item(ui, icons, lbl, true).clicked() {
                                    self.select_orphans(l, r);
                                    ui.close_menu();
                                }
                            }
                        });
                        ui.separator();
                        if super::chrome::menu_item(ui, icons, "menu.actions.compare_refresh  F5", true)
                            .clicked()
                        {
                            self.start_scan();
                            ui.close_menu();
                        }
                        if super::chrome::menu_item(ui, icons, "folder.full_refresh  Ctrl+F5", true)
                            .clicked()
                        {
                            self.full_refresh();
                            ui.close_menu();
                        }
                        if super::chrome::menu_item(
                            ui,
                            icons,
                            "folder.copy_filenames  Ctrl+C",
                            can_operate,
                        )
                        .clicked()
                        {
                            if let Ok(paths) = self.paths(self.active_left) {
                                ctx.copy_text(
                                    paths
                                        .iter()
                                        .map(|p| p.display().to_string())
                                        .collect::<Vec<_>>()
                                        .join("\n"),
                                );
                            }
                            ui.close_menu();
                        }
                    });
                    super::chrome::menu_button(ui, "menu.search", |ui| {
                        if super::chrome::menu_item(ui, icons, "folder.find_name  Ctrl+F", true)
                            .clicked()
                        {
                            if let Some(query) =
                                dialogs.input("folder.find_filename", "folder.name_or_substring", &self.find_text)
                            {
                                self.find_text = query;
                                let query = self.find_text.clone();
                                self.navigate(true, Some(&query));
                            }
                            ui.close_menu();
                        }
                        for (label, forward) in [
                            ("folder.find_next_filename  F3", true),
                            ("folder.find_prev_filename  Shift+F3", false),
                        ] {
                            if super::chrome::menu_item(ui, icons, label, true).clicked() {
                                let query = self.find_text.clone();
                                self.navigate(forward, Some(&query));
                                ui.close_menu();
                            }
                        }
                        for (label, forward) in [
                            ("menu.search.next_diff  Ctrl+N", true),
                            ("menu.search.prev_diff  Ctrl+P", false),
                        ] {
                            if super::chrome::menu_item(ui, icons, label, true).clicked() {
                                self.navigate(forward, None);
                                ui.close_menu();
                            }
                        }
                        ui.label(crate::i18n::tr("folder.filter_name"));
                        ui.text_edit_singleline(&mut self.filter);
                    });
                    super::chrome::menu_button(ui, "menu.view", |ui| {
                        for (label, mode) in [
                            ("common.all", DisplayFilter::All),
                            ("common.differences", DisplayFilter::Diff),
                            ("common.same", DisplayFilter::Same),
                            ("toolbar.show_orphans", DisplayFilter::Orphan),
                            ("toolbar.show_no_orphans", DisplayFilter::NoOrphan),
                            ("toolbar.show_differences_but_no_orphans", DisplayFilter::DiffNoOrphan),
                            ("toolbar.show_left_newer", DisplayFilter::LeftNewer),
                            ("toolbar.show_right_newer", DisplayFilter::RightNewer),
                            ("toolbar.show_left_orphans", DisplayFilter::LeftOrphan),
                            ("toolbar.show_right_orphans", DisplayFilter::RightOrphan),
                        ] {
                            if super::chrome::menu_radio_item(
                                ui,
                                icons,
                                label,
                                self.display_filter == mode,
                                true,
                            )
                            .clicked()
                            {
                                self.display_filter = mode;
                                ui.close_menu();
                            }
                        }
                        ui.separator();
                        if super::chrome::menu_check_item(
                            ui,
                            icons,
                            "view.ignore_unimportant",
                            self.ignore_minor,
                            true,
                        )
                        .clicked()
                        {
                            self.ignore_minor = !self.ignore_minor;
                            ui.close_menu();
                        }
                        ui.separator();
                        if super::chrome::menu_check_item(
                            ui,
                            icons,
                            "folder.show_tree",
                            self.show_tree(),
                            true,
                        )
                        .clicked()
                        {
                            self.dir_handling = if self.show_tree() {
                                DirHandling::Flatten
                            } else {
                                DirHandling::CompareStructure
                            };
                            ui.close_menu();
                        }
                        if super::chrome::menu_check_item(
                            ui,
                            icons,
                            "folder.compare_files_only",
                            self.dir_handling == DirHandling::CompareFilesOnly,
                            true,
                        )
                        .clicked()
                        {
                            self.set_compare_files_only(
                                self.dir_handling != DirHandling::CompareFilesOnly,
                            );
                            ui.close_menu();
                        }
                        if super::chrome::menu_check_item(ui, icons, "folder.log", self.show_log, true)
                            .clicked()
                        {
                            self.show_log = !self.show_log;
                            ui.close_menu();
                        }
                        if super::chrome::menu_check_item(
                            ui,
                            icons,
                            "folder.compare_contents",
                            self.compare_content,
                            true,
                        )
                        .clicked()
                        {
                            self.compare_content = !self.compare_content;
                            if !self.left_path.is_empty() && !self.right_path.is_empty() {
                                self.start_scan();
                            }
                            ui.close_menu();
                        }
                        ui.separator();
                        if super::chrome::menu_item(ui, icons, "folder.legend", true).clicked() {
                            super::info_modals::open_legend(ui.ctx());
                            ui.close_menu();
                        }
                        if super::chrome::menu_check_item(
                            ui,
                            icons,
                            "view.toolbar",
                            self.show_toolbar,
                            true,
                        )
                        .clicked()
                        {
                            self.show_toolbar = !self.show_toolbar;
                            ui.close_menu();
                        }
                    });
                    super::chrome::menu_button(ui, "menu.tools", |ui| {
                        if super::chrome::menu_item(ui, icons, "menu.tools.options", true).clicked() {
                            if dialogs.persist_settings() {
                                super::options_dialog::request(
                                    ui.ctx(),
                                    super::options_dialog::OptionsCategory::FolderCompare,
                                );
                            } else {
                                self.options(dialogs);
                            }
                            ui.close_menu();
                        }
                        for label in [
                            "menu.tools.file_formats",
                            "menu.tools.profiles",
                            "menu.tools.source_control",
                        ] {
                            super::chrome::menu_item(ui, icons, label, false)
                                .on_disabled_hover_text(crate::i18n::tr(
                                    "common.not_available",
                                ));
                        }
                        super::session_menu::settings_tools(ui, icons, dialogs);
                        ui.separator();
                        let snap_source = if self.active_left {
                            &self.loaded_left
                        } else {
                            &self.loaded_right
                        };
                        let snap_source = if snap_source.as_os_str().is_empty() {
                            None
                        } else {
                            Some(snap_source.as_path())
                        };
                        super::session_menu::utility_tools(ui, icons, dialogs, snap_source, None);
                    });
                    super::chrome::menu_button(ui, "menu.help", |ui| {
                        if super::chrome::menu_item(ui, icons, "folder.legend", true).clicked() {
                            super::info_modals::open_legend(ui.ctx());
                            ui.close_menu();
                        }
                        if super::chrome::menu_item(ui, icons, "menu.help.about", true)
                            .clicked()
                        {
                            super::info_modals::open_about(ui.ctx());
                            ui.close_menu();
                        }
                    });
                });
            });
        super::session_tabs::show(ui, icons);
        let (display_filter, dir_handling, ignore_minor, pending) = (
            self.display_filter,
            self.dir_handling,
            self.ignore_minor,
            self.pending.is_some(),
        );
        let show_tree = self.show_tree();
        let suppress = self.suppress_filters;
        let tool_state = move |name: &str| -> ToolState {
            if let Some(mode) = DisplayFilter::from_name(name) {
                return ToolState::checked(display_filter == mode);
            }
            if let Some(mode) = DirHandling::from_name(name) {
                return ToolState::checked(dir_handling == mode);
            }
            match name {
                "InsigIgnored" => ToolState::checked(ignore_minor),
                "SuppressFilters" => ToolState::checked(suppress),
                "CopySide" => ToolState::enabled(can_operate),
                "SessionsCombo" => ToolState::enabled(true),
                "ExpandAll" | "CollapseAll" => ToolState::enabled(show_tree),
                "Refresh" => ToolState::enabled(!pending),
                "Stop" => ToolState::enabled(pending),
                "SessionHome" | "Rules" | "SelectAll" | "SelectAllFiles" | "Swap"
                | "NameFilter" => ToolState::enabled(true),
                _ => ToolState::DISABLED,
            }
        };
        let items = super::toolbar_spec::FOLDER_COMPARE;
        let mut tool_event = None;
        if self.show_toolbar {
            let toolbar_height =
                super::toolbar::height(ui, items, ui.available_width(), &tool_state);
            let filter = &mut self.filter;
            TopBottomPanel::top("folder_toolbar")
                .exact_height(toolbar_height)
                .show_separator_line(false)
                .frame(Frame::none())
                .show_inside(ui, |ui| {
                    tool_event = super::toolbar::show(
                        ui,
                        icons,
                        items,
                        self.active_left,
                        &tool_state,
                        &mut |ui, _item, rect| {
                            let mut child = ui.new_child(UiBuilder::new().max_rect(rect));
                            super::history_combo::show_sized(
                                &mut child,
                                "folder_name_filter",
                                filter,
                                &[DEFAULT_FILTER.to_owned()],
                                rect.size(),
                                "",
                            );
                        },
                    );
                });
        }
        match tool_event {
            Some(ToolEvent::Click("SessionHome")) => action = FolderAction::Home,
            Some(ToolEvent::Click(name)) if DisplayFilter::from_name(name).is_some() => {
                if let Some(mode) = DisplayFilter::from_name(name) {
                    self.display_filter = mode;
                }
            }
            Some(ToolEvent::Click(name)) if DirHandling::from_name(name).is_some() => {
                if let Some(mode) = DirHandling::from_name(name) {
                    self.dir_handling = mode;
                }
            }
            Some(ToolEvent::Click("InsigIgnored")) => {
                self.ignore_minor = !self.ignore_minor;
            }
            Some(ToolEvent::Click("Rules")) => {
                if dialogs.persist_settings() {
                    super::session_settings::request(ctx);
                } else {
                    self.options(dialogs);
                }
            }
            Some(ToolEvent::Click("CopySide")) => self.copy_selected(self.active_left, dialogs),
            Some(ToolEvent::Click("ExpandAll")) => self.collapsed.clear(),
            Some(ToolEvent::Click("CollapseAll")) => {
                self.collapsed = self
                    .entries
                    .iter()
                    .filter(|e| is_dir(e))
                    .map(|e| path_key(&e.relative))
                    .collect();
            }
            Some(ToolEvent::Click(name @ ("SelectAll" | "SelectAllFiles"))) => {
                self.selected = self
                    .visible_indices()
                    .into_iter()
                    .map(|index| &self.entries[index])
                    .filter(|e| name == "SelectAll" || !is_dir(e))
                    .map(|e| e.id.clone())
                    .collect();
            }
            Some(ToolEvent::Click("Refresh")) => self.start_scan(),
            Some(ToolEvent::Click("Swap")) => {
                std::mem::swap(&mut self.left_path, &mut self.right_path);
                self.start_scan();
            }
            Some(ToolEvent::Click("Stop")) => self.cancel.store(true, Ordering::Relaxed),
            Some(ToolEvent::Click("NameFilter")) => {
                if let Some(mask) = dialogs.input("common.filters", "common.filters_colon", &self.filter) {
                    self.filter = mask;
                }
            }
            Some(ToolEvent::Click("SuppressFilters")) => {
                self.suppress_filters = !self.suppress_filters
            }
            _ => {}
        }
        TopBottomPanel::top("folder_paths")
            .frame(
                Frame::none()
                    .fill(BcTheme::WINDOW_BG)
                    .inner_margin(Margin::symmetric(1.0, 2.0)),
            )
            .show_separator_line(false)
            .show_inside(ui, |ui| {
                ui.spacing_mut().item_spacing = vec2(6.0, 0.0);
                let mut up_both = false;
                let mut navigation = None;
                let mut rescan = false;
                let mut changed = false;
                ui.columns(2, |columns| {
                    for (index, column) in columns.iter_mut().enumerate() {
                        column.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 1.0;
                            ui.spacing_mut().button_padding = vec2(1.0, 1.0);
                            let text = if index == 0 {
                                &mut self.left_path
                            } else {
                                &mut self.right_path
                            };
                            let width = (ui.available_width()
                                - if index == 0 { 76.0 } else { 95.0 })
                            .max(45.0);
                            let output = super::history_combo::show(
                                ui,
                                ("folder_path", index),
                                text,
                                &self.recent_paths,
                                width,
                                "folder.enter_path_here",
                            );
                            if output.response.has_focus() {
                                self.active_left = index == 0;
                                self.selection_focus = false;
                            }
                            changed |= output.response.changed();
                            if let Some(path) = output.selected {
                                *text = path;
                                rescan = true;
                            }
                            rescan |= output.submitted;
                            let mut buttons = vec![
                                (
                                    "imgAccept",
                                    "folder.load_comparison",
                                    !text.is_empty()
                                        && Path::new(text)
                                            != if index == 0 {
                                                &self.loaded_left
                                            } else {
                                                &self.loaded_right
                                            },
                                ),
                                ("imgBrowse", "folder.choose_folder", true),
                                (
                                    "imgUpOneLevel",
                                    "folder.up_one_level",
                                    Path::new(text).parent().is_some_and(|p| p.is_dir()),
                                ),
                            ];
                            if index == 0 {
                                buttons.push((
                                    "imgUpOneLevelAll",
                                    "folder.up_one_level_on_both_sides",
                                    true,
                                ));
                            } else {
                                buttons.push(("imgSpecsBack", "folder.back", self.root_cursor > 0));
                                buttons.push((
                                    "imgSpecsForward",
                                    "folder.forward",
                                    self.root_cursor + 1 < self.root_history.len(),
                                ));
                            }
                            for (name, hint, enabled) in buttons {
                                if let Some(icon) =
                                    icons.get_icon_with_state(ui.ctx(), name, enabled)
                                {
                                    if ui
                                        .add_enabled(
                                            enabled,
                                            ImageButton::new((icon.id(), vec2(16.0, 16.0)))
                                                .frame(false),
                                        )
                                        .on_hover_text(crate::i18n::tr(hint))
                                        .clicked()
                                    {
                                        match name {
                                            "imgBrowse" => {
                                                if let Some(path) =
                                                    dialogs.folder(index == 0, Path::new(text))
                                                {
                                                    *text = path.to_string_lossy().into_owned();
                                                    rescan = true;
                                                }
                                            }
                                            "imgUpOneLevel" => {
                                                if let Some(parent) = Path::new(text).parent() {
                                                    *text = parent.to_string_lossy().into_owned();
                                                    rescan = true;
                                                }
                                            }
                                            "imgUpOneLevelAll" => up_both = true,
                                            "imgSpecsBack" => {
                                                navigation = Some(self.root_cursor - 1)
                                            }
                                            "imgSpecsForward" => {
                                                navigation = Some(self.root_cursor + 1)
                                            }
                                            _ => rescan = true,
                                        }
                                    }
                                }
                            }
                        });
                    }
                });
                if let Some(cursor) = navigation {
                    self.root_cursor = cursor;
                    (self.left_path, self.right_path) = self.root_history[cursor].clone();
                    rescan = true;
                }
                if up_both {
                    for path in [&mut self.left_path, &mut self.right_path] {
                        if let Some(parent) = Path::new(path).parent().filter(|p| p.is_dir()) {
                            *path = parent.to_string_lossy().into_owned();
                        }
                    }
                    rescan = true;
                }
                if changed {
                    self.clear_scan();
                    self.errors.clear();
                }
                if rescan && !self.left_path.is_empty() && !self.right_path.is_empty() {
                    self.start_scan();
                }
                for error in self.errors.iter().take(3) {
                    ui.colored_label(Color32::DARK_RED, crate::i18n::tr(error));
                }
            });
        TopBottomPanel::bottom("folder_status")
            .exact_height(20.0)
            .frame(Frame::none().fill(BcTheme::WINDOW_BG))
            .show_inside(ui, |ui| {
                let visible = self.visible_indices();
                let rect = ui.max_rect();
                for side in 0..2 {
                    let sel_files: Vec<_> = if self.selected.len() > 1 {
                        self.entries
                            .iter()
                            .filter(|entry| self.selected.contains(&entry.id))
                            .filter_map(|entry| {
                                let metadata = if side == 0 { &entry.left } else { &entry.right };
                                metadata.as_ref().filter(|m| m.is_file())
                            })
                            .collect()
                    } else {
                        Vec::new()
                    };
                    let text = if !sel_files.is_empty() {
                        let bytes: u64 = sel_files.iter().map(|m| m.len()).sum();
                        crate::i18n::tr_fmt(
                            "status.files_selected",
                            &[
                                ("count", &grouped_size(sel_files.len() as u64)),
                                ("size", &volume_label(bytes)),
                            ],
                        )
                    } else {
                        let files: Vec<_> = visible
                            .iter()
                            .filter_map(|&index| {
                                let entry = &self.entries[index];
                                let metadata = if side == 0 { &entry.left } else { &entry.right };
                                metadata.as_ref().filter(|m| m.is_file())
                            })
                            .collect();
                        let bytes: u64 = files.iter().map(|m| m.len()).sum();
                        crate::i18n::tr_fmt(
                            "status.file_s",
                            &[
                                ("", &grouped_size(files.len() as u64)),
                                ("", &volume_label(bytes)),
                            ],
                        )
                    };
                    let x = rect.left() + side as f32 * rect.width() / 2.0;
                    let end = x + rect.width() / 2.0;
                    let free_x = end - 180.0;
                    ui.painter().text(
                        pos2(x + 3.0, rect.center().y),
                        Align2::LEFT_CENTER,
                        text,
                        FontId::proportional(12.0),
                        Color32::BLACK,
                    );
                    if let Some((bytes, drive)) = &self.free_space[side] {
                        let number = format!("{:.1}", *bytes as f64 / 1_073_741_824.0);
                        let number = if crate::i18n::language().starts_with("pt") {
                            number.replace('.', ",")
                        } else {
                            number
                        };
                        ui.painter().text(
                            pos2(free_x + 4.0, rect.center().y),
                            Align2::LEFT_CENTER,
                            crate::i18n::tr_fmt(
                                "status.gb_free_on",
                                &[("number", &number), ("drive", drive)],
                            ),
                            FontId::proportional(12.0),
                            Color32::BLACK,
                        );
                    }
                    for divider in [x, free_x] {
                        ui.painter().vline(
                            divider,
                            rect.y_range(),
                            Stroke::new(1.0_f32, Color32::from_gray(210)),
                        );
                    }
                }
                // Classic Win32 / BC5 6-dot status bar resize grip at bottom-right corner
                for row_g in 0..3 {
                    for col_g in 0..=row_g {
                        let gx = (rect.right() - 4.0 - (col_g as f32) * 3.0).round();
                        let gy = (rect.bottom() - 4.0 - ((2 - row_g) as f32) * 3.0).round();
                        ui.painter().rect_filled(
                            Rect::from_min_size(pos2(gx + 1.0, gy + 1.0), vec2(2.0, 2.0)),
                            0.0,
                            Color32::from_rgba_unmultiplied(255, 255, 255, 200),
                        );
                        ui.painter().rect_filled(
                            Rect::from_min_size(pos2(gx, gy), vec2(2.0, 2.0)),
                            0.0,
                            Color32::from_gray(152),
                        );
                    }
                }
                ui.allocate_space(rect.size());
            });
        if self.show_log {
            TopBottomPanel::bottom("folder_log")
                .resizable(true)
                .default_height(90.0)
                .min_height(44.0)
                .max_height(260.0)
                .frame(Frame::none().fill(BcTheme::WINDOW_BG).inner_margin(Margin {
                    left: 24.0,
                    right: 2.0,
                    top: 2.0,
                    bottom: 2.0,
                }))
                .show_inside(ui, |ui| {
                    let panel = ui.max_rect();
                    ui.spacing_mut().item_spacing.y = 0.0;
                    // Exact BC5 bluish Log ListBox background measured from BC5 capture: RGB(227, 231, 237).
                    ui.painter()
                        .rect_filled(panel, 0.0, Color32::from_rgb(227, 231, 237));
                    ui.painter().rect_stroke(
                        panel,
                        0.0,
                        Stroke::new(1.0_f32, Color32::from_gray(172)),
                    );
                    let menu_rect =
                        Rect::from_min_size(panel.left_top() - vec2(22.0, 0.0), vec2(20.0, 20.0));
                    let mut buttons = ui.new_child(
                        UiBuilder::new()
                            .max_rect(Rect::from_min_size(menu_rect.min, vec2(20.0, 44.0))),
                    );
                    if let Some(icon) = icons.get_icon(ui.ctx(), "imgOptions") {
                        buttons.menu_image_button(
                            Image::new((icon.id(), vec2(16.0, 16.0))),
                            |ui| {
                                if super::chrome::menu_item(ui, icons, "folder.hide_log", true)
                                    .clicked()
                                {
                                    self.show_log = false;
                                    ui.close_menu();
                                }
                            },
                        );
                    }
                    if let Some(icon) = icons.get_icon(ui.ctx(), "imgSave") {
                        if buttons
                            .add(ImageButton::new((icon.id(), vec2(16.0, 16.0))).frame(false))
                            .on_hover_text(crate::i18n::tr("folder.save_log"))
                            .clicked()
                        {
                            if let Some(path) = dialogs.report_path("txt") {
                                if let Err(error) = fs::write(path, self.log.join("\r\n")) {
                                    dialogs.error(&error.to_string());
                                }
                            }
                        }
                    }
                    ui.style_mut().spacing.scroll = egui::style::ScrollStyle::solid();
                    let log_len = self.log.len();
                    ScrollArea::vertical()
                        .stick_to_bottom(true)
                        .scroll_bar_visibility(
                            egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded,
                        )
                        .show(ui, |ui| {
                            for (idx, line) in self.log.iter().enumerate() {
                                let (rect, _) = ui.allocate_exact_size(
                                    vec2(ui.available_width(), 17.0),
                                    Sense::hover(),
                                );
                                if idx + 1 == log_len {
                                    ui.painter().rect_filled(
                                        rect.shrink2(vec2(1.0, 0.0)),
                                        0.0,
                                        Color32::from_rgb(221, 225, 230),
                                    );
                                    ui.painter().rect_stroke(
                                        rect.shrink2(vec2(1.0, 0.5)),
                                        0.0,
                                        Stroke::new(1.0_f32, Color32::from_rgb(188, 194, 204)),
                                    );
                                }
                                ui.painter().with_clip_rect(rect).text(
                                    rect.left_center() + vec2(4.0, 0.0),
                                    Align2::LEFT_CENTER,
                                    crate::i18n::tr(line),
                                    FontId::proportional(12.0),
                                    Color32::BLACK,
                                );
                            }
                        });
                });
        }
        let mut deferred_ctx: Option<ContextAction> = None;
        CentralPanel::default()
            .frame(
                Frame::none()
                    .fill(BcTheme::PANEL_BG)
                    .stroke(Stroke::new(1.0_f32, Color32::from_gray(180))),
            )
            .show_inside(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                let visible: Vec<&Entry> = self
                    .visible_indices()
                    .into_iter()
                    .map(|index| &self.entries[index])
                    .collect();
                // Pre-compute selection properties for the right-click context menu.
                let sel_has_dirs = self
                    .entries
                    .iter()
                    .any(|e| self.selected.contains(&e.id) && is_dir(e));
                let sel_has_collapsed_dir = self.entries.iter().any(|e| {
                    self.selected.contains(&e.id)
                        && is_dir(e)
                        && self.collapsed.contains(&path_key(&e.relative))
                });
                let sel_has_files = self
                    .entries
                    .iter()
                    .any(|e| self.selected.contains(&e.id) && !is_dir(e));
                let sel_all_ignored = !self.selected.is_empty()
                    && self
                        .entries
                        .iter()
                        .filter(|e| self.selected.contains(&e.id))
                        .all(|e| e.ignored);
                let sel_single_dir = self.selected.len() == 1 && sel_has_dirs;
                let left_writable = folder_source::is_writable(&self.loaded_left);
                let right_writable = folder_source::is_writable(&self.loaded_right);
                let excl_count = self.exclude_patterns.len();
                ui.spacing_mut().scroll.bar_width = 17.0;
                ui.spacing_mut().scroll.bar_inner_margin = 0.0;
                let table_width =
                    (ui.available_width() - ui.spacing().scroll.allocated_width()).max(650.0);
                const CENTER_HALF: f32 = 9.0;
                let panes = |rect: Rect| {
                    let mid = (rect.left() + table_width / 2.0).floor();
                    [
                        Rect::from_min_max(rect.min, pos2(mid - CENTER_HALF, rect.bottom())),
                        Rect::from_min_max(pos2(mid + CENTER_HALF, rect.top()), rect.max),
                    ]
                };
                let center_rect = |rect: Rect| {
                    let mid = (rect.left() + table_width / 2.0).floor();
                    Rect::from_min_max(
                        pos2(mid - CENTER_HALF, rect.top()),
                        pos2(mid + CENTER_HALF, rect.bottom()),
                    )
                };
                let (header, _) = ui.allocate_exact_size(vec2(table_width, 22.0), Sense::hover());
                let border = Stroke::new(1.0_f32, Color32::from_gray(208));
                ui.painter()
                    .rect_filled(header, 0.0, Color32::from_rgb(246, 247, 249));
                let hdr_center = center_rect(header);
                ui.painter()
                    .rect_filled(hdr_center, 0.0, Color32::from_rgb(238, 240, 243));
                ui.painter()
                    .line_segment([hdr_center.left_top(), hdr_center.left_bottom()], border);
                ui.painter()
                    .line_segment([hdr_center.right_top(), hdr_center.right_bottom()], border);
                for pane in panes(header) {
                    let columns = FolderColumns::new(pane);
                    for (cell, label) in [
                        (columns.name, "common.name"),
                        (columns.size, "common.size"),
                        (columns.modified, "status.modified"),
                    ] {
                        ui.painter()
                            .with_clip_rect(cell.shrink2(vec2(3.0, 0.0)))
                            .text(
                                cell.left_center() + vec2(6.0, 0.0),
                                Align2::LEFT_CENTER,
                                crate::i18n::tr(label),
                                FontId::proportional(12.0),
                                Color32::BLACK,
                            );
                        ui.painter()
                            .line_segment([cell.right_top(), cell.right_bottom()], border);
                    }
                    // Subtle sort indicator tucked neatly to the right of the "Nome" caption.
                    let c = pos2(columns.name.left() + 46.0, columns.name.center().y - 1.5);
                    let arrow_stroke = Stroke::new(1.1_f32, Color32::from_gray(115));
                    ui.painter()
                        .line_segment([c + vec2(-3.0, 2.5), c], arrow_stroke);
                    ui.painter()
                        .line_segment([c, c + vec2(3.0, 2.5)], arrow_stroke);
                }
                ui.painter()
                    .line_segment([header.left_bottom(), header.right_bottom()], border);
                let mut scroll = ScrollArea::vertical()
                    .id_salt("folder_rows")
                    .scroll_bar_visibility(scroll_area::ScrollBarVisibility::AlwaysVisible)
                    .auto_shrink([false; 2]);
                if self.scroll_to_selected {
                    if let Some(index) = visible.iter().position(|e| self.selected.contains(&e.id))
                    {
                        scroll = scroll.vertical_scroll_offset(index as f32 * ROW_HEIGHT);
                    }
                    self.scroll_to_selected = false;
                }
                let table_top = ui.cursor().top();
                let body_area = ui.max_rect();
                let bg_center = Rect::from_min_max(
                    pos2(
                        (body_area.left() + table_width / 2.0).floor() - CENTER_HALF,
                        table_top,
                    ),
                    pos2(
                        (body_area.left() + table_width / 2.0).floor() + CENTER_HALF,
                        body_area.bottom(),
                    ),
                );
                ui.painter()
                    .rect_filled(bg_center, 0.0, Color32::from_rgb(245, 246, 248));
                let show_tree = self.dir_handling != DirHandling::Flatten;
                let ignore_minor = self.ignore_minor;
                scroll.show_rows(ui, ROW_HEIGHT, visible.len(), |ui, range| {
                    for index in range {
                        let entry = visible[index];
                        let is_selected = self.selected.contains(&entry.id);
                        let color = if is_selected {
                            if self.selection_focus {
                                Color32::from_rgb(176, 211, 244)
                            } else {
                                Color32::from_rgb(211, 226, 240)
                            }
                        } else if index % 2 == 0 {
                            Color32::WHITE
                        } else {
                            Color32::from_rgb(248, 250, 249)
                        };
                        let (rect, response) =
                            ui.allocate_exact_size(vec2(table_width, ROW_HEIGHT), Sense::click());
                        let mid_col = center_rect(rect);
                        let center_fill = if is_selected {
                            color
                        } else {
                            Color32::from_rgb(245, 246, 248)
                        };
                        ui.painter().rect_filled(mid_col, 0.0, center_fill);
                        ui.painter().line_segment(
                            [mid_col.left_top(), mid_col.left_bottom()],
                            Stroke::new(1.0_f32, Color32::from_gray(225)),
                        );
                        ui.painter().line_segment(
                            [mid_col.right_top(), mid_col.right_bottom()],
                            Stroke::new(1.0_f32, Color32::from_gray(225)),
                        );
                        // BC5 center status indicator (`✓`, `≠`, `=`, `>`, `<`).
                        let center_icon = if entry.ignored {
                            Some("imgIgnored")
                        } else {
                            match entry.status {
                                Status::Same => Some("imgContentSame"),
                                Status::Different | Status::Error => {
                                    if is_dir(entry) {
                                        let l_new = entry.dir_summary[0].has_newer_or_diff;
                                        let r_new = entry.dir_summary[1].has_newer_or_diff;
                                        match (l_new, r_new) {
                                            (true, false) => Some("imgContentArrowRight"),
                                            (false, true) => Some("imgContentArrowLeft"),
                                            _ => Some("imgContentDifferent"),
                                        }
                                    } else {
                                        let l_mod = entry.left.as_ref().and_then(|m| m.modified());
                                        let r_mod = entry.right.as_ref().and_then(|m| m.modified());
                                        match (l_mod, r_mod) {
                                            (Some(l), Some(r)) if l > r => {
                                                Some("imgContentArrowRight")
                                            }
                                            (Some(l), Some(r)) if r > l => {
                                                Some("imgContentArrowLeft")
                                            }
                                            _ => Some("imgContentDifferent"),
                                        }
                                    }
                                }
                                Status::LeftOnly | Status::RightOnly => None,
                            }
                        };
                        if let Some(icon_name) = center_icon {
                            if let Some(tex) = icons.get_icon(ui.ctx(), icon_name) {
                                ui.painter().image(
                                    tex.id(),
                                    Rect::from_center_size(mid_col.center(), vec2(14.0, 14.0)),
                                    Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)),
                                    Color32::WHITE,
                                );
                            }
                        }
                        for (side, pane) in panes(rect).into_iter().enumerate() {
                            let columns = FolderColumns::new(pane);
                            let painter =
                                ui.painter().with_clip_rect(pane.intersect(ui.clip_rect()));
                            painter.rect_filled(pane, 0.0, color);
                            let metadata = if side == 0 { &entry.left } else { &entry.right };
                            if let Some(metadata) = metadata {
                                let relative = if side == 0 {
                                    &entry.relative
                                } else {
                                    &entry.right_relative
                                };
                                let depth = if show_tree {
                                    relative.components().count().saturating_sub(1) as f32 * 16.0
                                } else {
                                    0.0
                                };
                                let chevron_slot = if show_tree { 14.0 } else { 2.0 };
                                let icon_x = pane.left() + depth + chevron_slot;
                                let text_color = entry_color(entry, side == 0, ignore_minor);
                                if metadata.is_dir() {
                                    let opened =
                                        !self.collapsed.contains(&path_key(&entry.relative));
                                    if show_tree {
                                        let cx = (pane.left() + depth + 7.0).floor() + 0.5;
                                        let cy = pane.center().y.floor() + 0.5;
                                        let c_stroke = Stroke::new(
                                            1.2_f32,
                                            if opened {
                                                Color32::from_gray(55)
                                            } else {
                                                Color32::from_gray(115)
                                            },
                                        );
                                        let pts = if opened {
                                            [
                                                pos2(cx - 3.0, cy - 1.5),
                                                pos2(cx, cy + 1.5),
                                                pos2(cx + 3.0, cy - 1.5),
                                            ]
                                        } else {
                                            [
                                                pos2(cx - 1.5, cy - 3.0),
                                                pos2(cx + 1.5, cy),
                                                pos2(cx - 1.5, cy + 3.0),
                                            ]
                                        };
                                        painter.line_segment([pts[0], pts[1]], c_stroke);
                                        painter.line_segment([pts[1], pts[2]], c_stroke);
                                    }
                                    let own_orphan = matches!(
                                        entry.status,
                                        Status::LeftOnly | Status::RightOnly
                                    );
                                    let (top_color, bottom_color) =
                                        entry.dir_summary[side].icon_halves(own_orphan);
                                    let layers = if opened {
                                        [
                                            ("imgFolderTop1", top_color),
                                            ("imgFolderBottom1", bottom_color),
                                        ]
                                    } else {
                                        [
                                            ("imgFolderTop0", top_color),
                                            ("imgFolderBottom0", bottom_color),
                                        ]
                                    };
                                    for (name, layer_color) in layers {
                                        if let Some(icon) =
                                            icons.folder_model(ui.ctx(), name, layer_color)
                                        {
                                            painter.image(
                                                icon.id(),
                                                Rect::from_min_size(
                                                    pos2(icon_x, pane.top() + 1.0),
                                                    vec2(16.0, 16.0),
                                                ),
                                                Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)),
                                                Color32::WHITE,
                                            );
                                        }
                                    }
                                } else if relative.extension().is_some_and(|ext| {
                                    ext.eq_ignore_ascii_case("chm")
                                        || folder_source::is_archive_path(relative)
                                }) {
                                    for name in
                                        ["imgFolderTop0", "imgFolderBottom0", "imgFolderZip"]
                                    {
                                        if let Some(icon) =
                                            icons.folder_model(ui.ctx(), name, Color32::GRAY)
                                        {
                                            painter.image(
                                                icon.id(),
                                                Rect::from_min_size(
                                                    pos2(icon_x, pane.top() + 1.0),
                                                    vec2(16.0, 16.0),
                                                ),
                                                Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)),
                                                Color32::WHITE,
                                            );
                                        }
                                    }
                                } else {
                                    // Crisp 12x14 page/document icon tinted with the BC5 status color.
                                    let doc = Rect::from_min_size(
                                        pos2(icon_x + 2.0, pane.top() + 2.0),
                                        vec2(11.0, 13.0),
                                    );
                                    let outline = if text_color == Color32::BLACK {
                                        Color32::from_gray(110)
                                    } else {
                                        text_color
                                    };
                                    let fold = 3.5;
                                    let page_pts = vec![
                                        doc.left_top(),
                                        pos2(doc.right() - fold, doc.top()),
                                        pos2(doc.right(), doc.top() + fold),
                                        doc.right_bottom(),
                                        doc.left_bottom(),
                                    ];
                                    painter.add(Shape::convex_polygon(
                                        page_pts,
                                        Color32::WHITE,
                                        Stroke::new(1.0_f32, outline),
                                    ));
                                    painter.line_segment(
                                        [
                                            pos2(doc.right() - fold, doc.top()),
                                            pos2(doc.right() - fold, doc.top() + fold),
                                        ],
                                        Stroke::new(1.0_f32, outline),
                                    );
                                    painter.line_segment(
                                        [
                                            pos2(doc.right() - fold, doc.top() + fold),
                                            pos2(doc.right(), doc.top() + fold),
                                        ],
                                        Stroke::new(1.0_f32, outline),
                                    );
                                    let line_color = if text_color == Color32::BLACK {
                                        Color32::from_gray(155)
                                    } else {
                                        text_color
                                    };
                                    for (dy, rw) in [(6.0, 6.0), (8.5, 6.5), (11.0, 4.5)] {
                                        painter.line_segment(
                                            [
                                                pos2(doc.left() + 2.0, doc.top() + dy),
                                                pos2(doc.left() + 2.0 + rw, doc.top() + dy),
                                            ],
                                            Stroke::new(1.0_f32, line_color),
                                        );
                                    }
                                }
                                let name = if show_tree {
                                    relative.file_name().map(Path::new).unwrap_or(relative)
                                } else {
                                    relative
                                };
                                painter
                                    .with_clip_rect(columns.name.intersect(ui.clip_rect()))
                                    .text(
                                        pos2(icon_x + 19.0, pane.center().y),
                                        Align2::LEFT_CENTER,
                                        name.display().to_string(),
                                        FontId::proportional(12.0),
                                        text_color,
                                    );
                                let size_display = if metadata.is_file() {
                                    Some((metadata.len(), text_color))
                                } else if metadata.is_dir() && self.show_folder_sizes {
                                    Some((entry.dir_summary[side].bytes, Color32::GRAY))
                                } else {
                                    None
                                };
                                if let Some((bytes, size_color)) = size_display {
                                    painter
                                        .with_clip_rect(columns.size.intersect(ui.clip_rect()))
                                        .text(
                                            columns.size.right_center() - vec2(4.0, 0.0),
                                            Align2::RIGHT_CENTER,
                                            grouped_size(bytes),
                                            FontId::proportional(12.0),
                                            size_color,
                                        );
                                }
                                let date = metadata
                                    .modified()
                                    .map(|t| {
                                        let date: chrono::DateTime<chrono::Local> = t.into();
                                        date.format("%d/%m/%Y %H:%M:%S").to_string()
                                    })
                                    .unwrap_or_default();
                                painter
                                    .with_clip_rect(columns.modified.intersect(ui.clip_rect()))
                                    .text(
                                        columns.modified.right_center() - vec2(4.0, 0.0),
                                        Align2::RIGHT_CENTER,
                                        date,
                                        FontId::proportional(12.0),
                                        if metadata.is_dir() {
                                            Color32::GRAY
                                        } else {
                                            text_color
                                        },
                                    );
                            }
                        }
                        if is_selected {
                            let outline = rect.shrink(0.5);
                            ui.painter().rect_stroke(
                                outline,
                                0.0,
                                Stroke::new(1.0_f32, Color32::from_rgb(122, 175, 224)),
                            );
                        }
                        if index == 0 {
                            ui.ctx().data_mut(|d| {
                                d.insert_temp(Id::new("capture_menu_rect").with("folder_first_row"), rect);
                            });
                        }
                        if response.clicked() {
                            self.selection_focus = true;
                            self.active_left = response
                                .interact_pointer_pos()
                                .is_none_or(|pos| pos.x < rect.center().x);
                            let modifiers = ui.input(|i| i.modifiers);
                            if modifiers.shift {
                                let anchor_idx = self
                                    .anchor_selected
                                    .as_ref()
                                    .and_then(|id| visible.iter().position(|e| &e.id == id))
                                    .unwrap_or(index);
                                let (lo, hi) = if anchor_idx <= index {
                                    (anchor_idx, index)
                                } else {
                                    (index, anchor_idx)
                                };
                                if !modifiers.ctrl {
                                    self.selected.clear();
                                }
                                for item in &visible[lo..=hi] {
                                    self.selected.insert(item.id.clone());
                                }
                            } else if modifiers.ctrl {
                                if !self.selected.insert(entry.id.clone()) {
                                    self.selected.remove(&entry.id);
                                }
                                self.anchor_selected = Some(entry.id.clone());
                            } else {
                                self.selected.clear();
                                self.selected.insert(entry.id.clone());
                                self.anchor_selected = Some(entry.id.clone());
                            }
                        }
                        if response.secondary_clicked() {
                            self.selection_focus = true;
                            self.active_left = response
                                .interact_pointer_pos()
                                .is_none_or(|pos| pos.x < rect.center().x);
                            // Preserve existing multi-selection (e.g. after Ctrl+A) when
                            // right-clicking an already-selected item — standard BC5 / Windows UX.
                            if !self.selected.contains(&entry.id) {
                                self.selected.clear();
                                self.selected.insert(entry.id.clone());
                                self.anchor_selected = Some(entry.id.clone());
                            }
                        }
                        let active_left_now = self.active_left;
                        let active_writable = if active_left_now {
                            left_writable
                        } else {
                            right_writable
                        };
                        let other_writable = if active_left_now {
                            right_writable
                        } else {
                            left_writable
                        };
                        let ctx_has_dirs = sel_has_dirs || is_dir(entry);
                        let ctx_has_collapsed = sel_has_collapsed_dir
                            || (is_dir(entry) && self.collapsed.contains(&path_key(&entry.relative)));
                        let ctx_has_files = sel_has_files || !is_dir(entry);
                        response.context_menu(|ui| {
                            ui.set_min_width(225.0);
                            ui.spacing_mut().item_spacing.y = 0.0;
                            ui.spacing_mut().button_padding = vec2(4.0, 2.0);
                            ui.spacing_mut().interact_size.y = 20.0;
                            if ctx_has_dirs {
                                let open_label = if ctx_has_collapsed {
                                    "folder.ctx.open_folder"
                                } else {
                                    "folder.ctx.close_folder"
                                };
                                if super::chrome::menu_item_default(ui, icons, open_label, true)
                                    .clicked()
                                {
                                    deferred_ctx =
                                        Some(ContextAction::OpenFolder(ctx_has_collapsed));
                                    ui.close_menu();
                                }
                                if super::chrome::menu_item(
                                    ui,
                                    icons,
                                    "folder.ctx.open_subfolders",
                                    true,
                                )
                                .clicked()
                                {
                                    deferred_ctx = Some(ContextAction::OpenSubfolders(true));
                                    ui.close_menu();
                                }
                                if super::chrome::menu_item(
                                    ui,
                                    icons,
                                    "folder.ctx.close_subfolders",
                                    true,
                                )
                                .clicked()
                                {
                                    deferred_ctx = Some(ContextAction::OpenSubfolders(false));
                                    ui.close_menu();
                                }
                                if super::chrome::menu_item(
                                    ui,
                                    icons,
                                    "folder.ctx.set_base_folders",
                                    sel_single_dir && left_writable && right_writable,
                                )
                                .clicked()
                                {
                                    deferred_ctx = Some(ContextAction::SetAsBaseFolders);
                                    ui.close_menu();
                                }
                                if super::chrome::menu_item(
                                    ui,
                                    icons,
                                    "folder.ctx.open_in_new_view",
                                    !busy,
                                )
                                .clicked()
                                {
                                    deferred_ctx = Some(ContextAction::OpenInNewView);
                                    ui.close_menu();
                                }
                            } else if super::chrome::menu_item_default(
                                ui,
                                icons,
                                "folder.ctx.open_file",
                                !busy,
                            )
                            .clicked()
                            {
                                deferred_ctx = Some(ContextAction::OpenFileDefault);
                                ui.close_menu();
                            }
                            super::chrome::submenu_item(
                                ui,
                                icons,
                                "menu.actions.open_with",
                                |ui, icons| {
                                    for (lbl, kind) in [
                                        ("session.text_compare", SessionKind::Text),
                                        ("session.table_compare", SessionKind::Table),
                                        ("session.hex_compare", SessionKind::Hex),
                                        ("session.picture_compare", SessionKind::Picture),
                                        ("session.version_compare", SessionKind::Version),
                                        ("session.media_compare", SessionKind::Media),
                                        ("session.registry_compare", SessionKind::Registry),
                                        ("session.text_edit", SessionKind::TextEdit),
                                    ] {
                                        if super::chrome::menu_item(
                                            ui,
                                            icons,
                                            lbl,
                                            !busy && ctx_has_files,
                                        )
                                        .clicked()
                                        {
                                            deferred_ctx = Some(ContextAction::OpenWith(kind));
                                            ui.close_menu();
                                        }
                                    }
                                    ui.separator();
                                    if super::chrome::menu_item(
                                        ui,
                                        icons,
                                        "folder.ctx.associated_app",
                                        active_writable,
                                    )
                                    .clicked()
                                    {
                                        deferred_ctx = Some(ContextAction::OpenWithAssocApp);
                                        ui.close_menu();
                                    }
                                },
                            );
                            ui.separator();
                            if super::chrome::menu_item(
                                ui,
                                icons,
                                "folder.ctx.compare_contents_ellipsis",
                                !busy,
                            )
                            .clicked()
                            {
                                deferred_ctx = Some(ContextAction::CompareContents);
                                ui.close_menu();
                            }
                            if super::chrome::menu_item(
                                ui,
                                icons,
                                "folder.ctx.copy_to_side_ellipsis",
                                !busy && other_writable,
                            )
                            .clicked()
                            {
                                deferred_ctx = Some(ContextAction::CopyToSide(active_left_now));
                                ui.close_menu();
                            }
                            if super::chrome::menu_item(
                                ui,
                                icons,
                                "folder.ctx.move_to_side_ellipsis",
                                !busy && left_writable && right_writable,
                            )
                            .clicked()
                            {
                                deferred_ctx = Some(ContextAction::MoveToSide(active_left_now));
                                ui.close_menu();
                            }
                            if super::chrome::menu_item(
                                ui,
                                icons,
                                "folder.ctx.copy_to_folder_ellipsis",
                                !busy,
                            )
                            .clicked()
                            {
                                deferred_ctx = Some(ContextAction::CopyToFolder(active_left_now));
                                ui.close_menu();
                            }
                            if super::chrome::menu_item(
                                ui,
                                icons,
                                "folder.ctx.move_to_folder_ellipsis",
                                !busy && active_writable,
                            )
                            .clicked()
                            {
                                deferred_ctx = Some(ContextAction::MoveToFolder(active_left_now));
                                ui.close_menu();
                            }
                            if super::chrome::menu_item(
                                ui,
                                icons,
                                "folder.ctx.delete_ellipsis  Del",
                                !busy && (left_writable || right_writable),
                            )
                            .clicked()
                            {
                                deferred_ctx = Some(ContextAction::Delete);
                                ui.close_menu();
                            }
                            if super::chrome::menu_item(
                                ui,
                                icons,
                                "folder.rename_ellipsis  F2",
                                !busy && active_writable,
                            )
                            .clicked()
                            {
                                deferred_ctx = Some(ContextAction::Rename);
                                ui.close_menu();
                            }
                            if super::chrome::menu_item(
                                ui,
                                icons,
                                "folder.ctx.attributes_ellipsis",
                                !busy && active_writable,
                            )
                            .clicked()
                            {
                                deferred_ctx = Some(ContextAction::Attributes);
                                ui.close_menu();
                            }
                            if super::chrome::menu_item(
                                ui,
                                icons,
                                "folder.ctx.touch_ellipsis",
                                !busy && active_writable,
                            )
                            .clicked()
                            {
                                deferred_ctx = Some(ContextAction::Touch);
                                ui.close_menu();
                            }
                            if super::chrome::menu_item(ui, icons, "folder.ctx.exclude", !busy)
                                .clicked()
                            {
                                deferred_ctx = Some(ContextAction::Exclude);
                                ui.close_menu();
                            }
                            if excl_count > 0
                                && ui
                                    .button(crate::i18n::tr_fmt(
                                        "folder.op.clear_exclusions",
                                        &[("count", &excl_count.to_string())],
                                    ))
                                    .clicked()
                            {
                                deferred_ctx = Some(ContextAction::ClearExclusions);
                                ui.close_menu();
                            }
                            if super::chrome::menu_item(
                                ui,
                                icons,
                                "folder.new_folder_ellipsis  Ins",
                                !busy && (left_writable || right_writable),
                            )
                            .clicked()
                            {
                                deferred_ctx = Some(ContextAction::NewFolder);
                                ui.close_menu();
                            }
                            if super::chrome::menu_item(
                                ui,
                                icons,
                                "folder.ctx.copy_filename",
                                true,
                            )
                            .clicked()
                            {
                                deferred_ctx = Some(ContextAction::CopyFilename);
                                ui.close_menu();
                            }
                            if super::chrome::menu_check_item(
                                ui,
                                icons,
                                "folder.ctx.ignored",
                                sel_all_ignored,
                                !busy,
                            )
                            .clicked()
                            {
                                deferred_ctx = Some(ContextAction::ToggleIgnored);
                                ui.close_menu();
                            }
                            if super::chrome::menu_item(
                                ui,
                                icons,
                                "folder.ctx.refresh_selection  Shift+F5",
                                !busy,
                            )
                            .clicked()
                            {
                                deferred_ctx = Some(ContextAction::RefreshSelection);
                                ui.close_menu();
                            }
                            ui.separator();
                            super::chrome::submenu_item(
                                ui,
                                icons,
                                "folder.ctx.synchronize",
                                |ui, icons| {
                                    let sync_ok = !busy && left_writable && right_writable;
                                    for (lbl, preset) in [
                                        ("sync.preset.update_left", QuickSyncPreset::UpdateLeft),
                                        ("sync.preset.update_right", QuickSyncPreset::UpdateRight),
                                        ("sync.preset.update_both", QuickSyncPreset::UpdateBoth),
                                        ("sync.preset.mirror_left", QuickSyncPreset::MirrorLeft),
                                        ("sync.preset.mirror_right", QuickSyncPreset::MirrorRight),
                                    ] {
                                        if super::chrome::menu_item(ui, icons, lbl, sync_ok)
                                            .clicked()
                                        {
                                            deferred_ctx = Some(ContextAction::QuickSync(preset));
                                            ui.close_menu();
                                        }
                                    }
                                },
                            );
                            ui.separator();
                            super::chrome::submenu_item(ui, icons, "table.explorer", |ui, icons| {
                                if super::chrome::menu_item(
                                    ui,
                                    icons,
                                    "version.select_in_explorer",
                                    active_writable,
                                )
                                .clicked()
                                {
                                    deferred_ctx = Some(ContextAction::ExplorerSelect);
                                    ui.close_menu();
                                }
                                if super::chrome::menu_item(
                                    ui,
                                    icons,
                                    "version.open_folder",
                                    active_writable,
                                )
                                .clicked()
                                {
                                    deferred_ctx = Some(ContextAction::ExplorerOpenFolder);
                                    ui.close_menu();
                                }
                                if super::chrome::menu_item(
                                    ui,
                                    icons,
                                    "folder.ctx.associated_app",
                                    active_writable,
                                )
                                .clicked()
                                {
                                    deferred_ctx = Some(ContextAction::OpenWithAssocApp);
                                    ui.close_menu();
                                }
                            });
                        });
                        if let Some(error) = &entry.error {
                            response.clone().on_hover_text(crate::i18n::tr(error));
                        }
                        let icon_clicked = response.clicked()
                            && response.interact_pointer_pos().is_some_and(|pos| {
                                let depth = if show_tree {
                                    entry.relative.components().count().saturating_sub(1) as f32
                                        * 16.0
                                } else {
                                    0.0
                                };
                                panes(rect).into_iter().any(|pane| {
                                    pane.contains(pos)
                                        && pos.x >= pane.left() + depth
                                        && pos.x <= pane.left() + depth + 32.0
                                })
                            });
                        if is_dir(entry)
                            && (response.double_clicked() || icon_clicked)
                            && !self.collapsed.insert(path_key(&entry.relative))
                        {
                            self.collapsed.remove(&path_key(&entry.relative));
                        }
                        if response.double_clicked()
                            && entry.left.as_ref().is_some_and(|m| m.is_file())
                            && entry.right.as_ref().is_some_and(|m| m.is_file())
                        {
                            match (
                                folder_source::materialize_file(&self.loaded_left, &entry.relative),
                                folder_source::materialize_file(
                                    &self.loaded_right,
                                    &entry.right_relative,
                                ),
                            ) {
                                (Ok(l_path), Ok(r_path)) => {
                                    action = FolderAction::OpenFiles(l_path, r_path);
                                }
                                (Err(err), _) | (_, Err(err)) => {
                                    dialogs.error(&err);
                                }
                            }
                        }
                    }
                });
                // Keep the center status rail visible below the final row.
                let rail_stroke = Stroke::new(1.0_f32, Color32::from_gray(215));
                ui.painter()
                    .line_segment([bg_center.left_top(), bg_center.left_bottom()], rail_stroke);
                ui.painter().line_segment(
                    [bg_center.right_top(), bg_center.right_bottom()],
                    rail_stroke,
                );
            });
        if let Some(cmd) = deferred_ctx {
            self.handle_context_action(ctx, cmd, dialogs, &mut action);
        }
        if let Some(mut modal) = self.modal.take() {
            match modal.show(ctx, icons, dialogs) {
                Ok(Some(confirmed)) => {
                    let keep_open = matches!(confirmed, FolderOpConfirmed::OpenResultRow { .. });
                    self.handle_modal_confirmed(ctx, confirmed, dialogs, &mut action);
                    if keep_open {
                        self.modal = Some(modal);
                    }
                }
                Ok(None) => self.modal = Some(modal),
                Err(()) => {}
            }
        }
        action
    }
}

impl Drop for FolderCompareScreen {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

fn scan_folders(left: &Path, right: &Path, content: bool, cancel: &AtomicBool) -> Scan {
    scan_folders_with_mode(left, right, content, ContentCompareMode::Binary, cancel)
}

#[cfg(test)]
pub(crate) fn bench_scan_folders(left: &Path, right: &Path, content: bool) -> usize {
    let cancel = AtomicBool::new(false);
    scan_folders_with_mode(left, right, content, ContentCompareMode::Binary, &cancel)
        .entries
        .len()
}

fn scan_folders_with_mode(
    left: &Path,
    right: &Path,
    content: bool,
    content_mode: ContentCompareMode,
    cancel: &AtomicBool,
) -> Scan {
    let criteria = FolderCompareCriteria {
        compare_content: content,
        content_mode,
        ..FolderCompareCriteria::default()
    };
    scan_folders_with_criteria(left, right, &criteria, cancel)
}

fn stem_key(path: &Path) -> Option<String> {
    let stem = path.file_stem()?.to_string_lossy().to_lowercase();
    let parent = path
        .parent()
        .map(path_key)
        .unwrap_or_default();
    if parent.is_empty() {
        Some(stem)
    } else {
        Some(format!("{parent}/{stem}"))
    }
}

fn evaluate_quick_tests(
    relative: &Path,
    right_relative: &Path,
    l: &EntryMeta,
    r: &EntryMeta,
    criteria: &FolderCompareCriteria,
) -> (Status, bool) {
    let mut hard_diff = false;
    let mut minor_diff = false;

    if criteria.compare_size && l.len() != r.len() {
        hard_diff = true;
    }
    if criteria.compare_timestamp {
        let lt = l.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        let rt = r.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        if lt != rt {
            let diff_secs = lt
                .duration_since(rt)
                .or_else(|_| rt.duration_since(lt))
                .map_or(u64::MAX, |d| d.as_secs());
            let tol = criteria.timestamp_tolerance_secs;
            let dst_match = criteria.ignore_dst && diff_secs.abs_diff(3600) <= tol;
            let tz_rem = diff_secs % 3600;
            let tz_match = criteria.ignore_timezone
                && diff_secs <= 86_400
                && (tz_rem <= tol || 3600 - tz_rem <= tol);
            if dst_match || tz_match {
                // Treated as matching timestamp
            } else if diff_secs <= tol {
                minor_diff = true;
            } else {
                hard_diff = true;
            }
        }
    }
    if criteria.compare_filename_case && relative != right_relative {
        hard_diff = true;
    }
    let mut attr_mask = 0u32;
    if criteria.compare_attr_readonly {
        attr_mask |= 0x01;
    }
    if criteria.compare_attr_hidden {
        attr_mask |= 0x02;
    }
    if criteria.compare_attr_system {
        attr_mask |= 0x04;
    }
    if criteria.compare_attr_archive {
        attr_mask |= 0x20;
    }
    if attr_mask != 0 && (l.dos_attrs & attr_mask) != (r.dos_attrs & attr_mask) {
        hard_diff = true;
    }
    if criteria.compare_unix_permissions && l.unix_mode != r.unix_mode {
        hard_diff = true;
    }
    if criteria.compare_unix_owner && l.unix_uid != r.unix_uid {
        hard_diff = true;
    }
    if criteria.compare_unix_group && l.unix_gid != r.unix_gid {
        hard_diff = true;
    }

    if hard_diff {
        (Status::Different, false)
    } else if minor_diff {
        (Status::Different, true)
    } else {
        (Status::Same, false)
    }
}

fn pe_versions_match(left_full: &Path, right_full: &Path) -> Option<bool> {
    if !crate::pe_version::supported(left_full)
        || !crate::pe_version::supported(right_full)
    {
        return None;
    }
    let l_img = crate::pe_version::Image::load(left_full).ok()?;
    let r_img = crate::pe_version::Image::load(right_full).ok()?;
    fn nodes_equal(a: &[crate::pe_version::Node], b: &[crate::pe_version::Node]) -> bool {
        if a.len() != b.len() {
            return false;
        }
        a.iter()
            .zip(b)
            .all(|(x, y)| x.key == y.key && x.same(y) && nodes_equal(&x.children, &y.children))
    }
    Some(nodes_equal(&l_img.nodes, &r_img.nodes))
}

fn scan_folders_with_criteria(
    left: &Path,
    right: &Path,
    criteria: &FolderCompareCriteria,
    cancel: &AtomicBool,
) -> Scan {
    let follow = criteria.follow_symlinks;
    let ((left_entries, mut errors), (right_snapshot, right_errors)) = std::thread::scope(|scope| {
        let hr = scope.spawn(|| {
            let mut r_errs = Vec::new();
            let snap = folder_source::scan_source_with_options(right, follow, cancel, &mut r_errs);
            (snap, r_errs)
        });
        let mut l_errs = Vec::new();
        let l_snap = folder_source::scan_source_with_options(left, follow, cancel, &mut l_errs);
        ((l_snap, l_errs), hr.join().unwrap())
    });
    errors.extend(right_errors);
    let mut right_entries: BTreeMap<(String, bool), Vec<(PathBuf, EntryMeta)>> = BTreeMap::new();
    for (path, metadata) in right_snapshot {
        right_entries
            .entry((path_key(&path), metadata.is_dir()))
            .or_default()
            .push((path, metadata));
    }
    let mut entries = Vec::with_capacity(left_entries.len() + right_entries.len());
    let mut content_indices = Vec::new();
    for (relative, l) in left_entries {
        if cancel.load(Ordering::Relaxed) {
            errors.push("folder.comparison_canceled_partial_results".into());
            break;
        }
        let mut right_entry = right_entries
            .get_mut(&(path_key(&relative), l.is_dir()))
            .and_then(|candidates| {
                if candidates.is_empty() {
                    return None;
                }
                let index = candidates
                    .iter()
                    .position(|(path, _)| *path == relative)
                    .unwrap_or(0);
                Some(candidates.remove(index))
            });
        if right_entry.is_none() && criteria.align_different_extensions && l.is_file() {
            if let Some(target_stem) = stem_key(&relative) {
                let matching_key = right_entries
                    .iter()
                    .find(|((_, is_dir_flag), cands)| {
                        !*is_dir_flag
                            && !cands.is_empty()
                            && stem_key(&cands[0].0).as_deref() == Some(target_stem.as_str())
                    })
                    .map(|(k, _)| k.clone());
                if let Some(k) = matching_key {
                    if let Some(cands) = right_entries.get_mut(&k) {
                        if !cands.is_empty() {
                            right_entry = Some(cands.remove(0));
                        }
                    }
                }
            }
        }
        let right_relative = right_entry
            .as_ref()
            .map(|(path, _)| path.clone())
            .unwrap_or_else(|| relative.clone());
        let r = right_entry.map(|(_, metadata)| metadata);
        let mut is_minor = false;
        let mut content_compared = false;
        let left_crc32 = l.crc32;
        let right_crc32 = r.as_ref().and_then(|m| m.crc32);
        let status = match &r {
            None => Status::LeftOnly,
            Some(r) if l.is_dir() && r.is_dir() => Status::Same,
            Some(r) if l.is_file() && r.is_file() => {
                let (quick_status, quick_minor) =
                    evaluate_quick_tests(&relative, &right_relative, &l, r, criteria);
                is_minor = quick_minor;
                let mut st = quick_status;
                if criteria.compare_versions && left.is_dir() && right.is_dir() {
                    if let Some(same_ver) =
                        pe_versions_match(&left.join(&relative), &right.join(&right_relative))
                    {
                        if same_ver {
                            if criteria.override_quick_results {
                                st = Status::Same;
                                is_minor = false;
                            }
                        } else {
                            st = Status::Different;
                            is_minor = false;
                        }
                    }
                }
                if criteria.compare_content {
                    let skip = criteria.skip_if_quick_same
                        && quick_status == Status::Same
                        && !quick_minor;
                    if !skip {
                        content_compared = true;
                        content_indices.push(entries.len());
                    }
                }
                st
            }
            Some(_) => Status::Different,
        };
        entries.push(Entry {
            id: EntryKey {
                relative: relative.clone(),
                directory: l.is_dir(),
            },
            relative,
            right_relative,
            left: Some(l),
            right: r,
            status,
            is_minor,
            content_compared,
            ignored: false,
            left_crc32,
            right_crc32,
            error: None,
            dir_summary: [DirSideSummary::default(); 2],
        });
    }
    if criteria.compare_content && !content_indices.is_empty() && !cancel.load(Ordering::Relaxed) {
        let total = content_indices.len();
        let workers = std::thread::available_parallelism()
            .map(|p| p.get())
            .unwrap_or(4)
            .clamp(1, 16)
            .min(total);
        let next_job = std::sync::atomic::AtomicUsize::new(0);
        let verdicts: Vec<std::sync::Mutex<Option<FileContentVerdict>>> =
            (0..total).map(|_| std::sync::Mutex::new(None)).collect();
        let entries_ref = &entries;
        let idx_ref = &content_indices;
        let verdicts_ref = &verdicts;
        let content_mode = criteria.content_mode;
        std::thread::scope(|scope| {
            for _ in 0..workers {
                scope.spawn(|| loop {
                    if cancel.load(Ordering::Relaxed) {
                        break;
                    }
                    let slot = next_job.fetch_add(1, Ordering::Relaxed);
                    if slot >= total {
                        break;
                    }
                    let e = &entries_ref[idx_ref[slot]];
                    let (Some(l), Some(r)) = (&e.left, &e.right) else {
                        continue;
                    };
                    let v = folder_ops::compare_file_pair(
                        left,
                        &e.relative,
                        l,
                        right,
                        &e.right_relative,
                        r,
                        content_mode,
                        cancel,
                    );
                    if let Ok(mut guard) = verdicts_ref[slot].lock() {
                        *guard = Some(v);
                    }
                });
            }
        });
        for (slot, entry_idx) in content_indices.into_iter().enumerate() {
            if let Some(verdict) = verdicts[slot].lock().ok().and_then(|mut g| g.take()) {
                let e = &mut entries[entry_idx];
                match verdict {
                    FileContentVerdict::Same {
                        left_crc,
                        right_crc,
                        ..
                    } => {
                        e.left_crc32 = left_crc.or(e.left_crc32);
                        e.right_crc32 = right_crc.or(e.right_crc32);
                        if criteria.override_quick_results {
                            e.status = Status::Same;
                            e.is_minor = false;
                        }
                    }
                    FileContentVerdict::Minor { .. } => {
                        if criteria.override_quick_results || e.status == Status::Same {
                            e.is_minor = true;
                            e.status = Status::Different;
                        }
                    }
                    FileContentVerdict::Different {
                        left_crc,
                        right_crc,
                        ..
                    } => {
                        e.left_crc32 = left_crc.or(e.left_crc32);
                        e.right_crc32 = right_crc.or(e.right_crc32);
                        e.status = Status::Different;
                        e.is_minor = false;
                    }
                    FileContentVerdict::Error(err) => {
                        let message = format!("{}: {err}", e.relative.display());
                        errors.push(message.clone());
                        e.error = Some(message);
                        e.status = Status::Error;
                    }
                }
            }
        }
    }
    if !cancel.load(Ordering::Relaxed) {
        entries.extend(
            right_entries
                .into_values()
                .flatten()
                .map(|(relative, right)| {
                    let right_crc32 = right.crc32;
                    Entry {
                        id: EntryKey {
                            relative: relative.clone(),
                            directory: right.is_dir(),
                        },
                        right_relative: relative.clone(),
                        relative,
                        left: None,
                        right: Some(right),
                        status: Status::RightOnly,
                        is_minor: false,
                        content_compared: false,
                        ignored: false,
                        left_crc32: None,
                        right_crc32,
                        error: None,
                        dir_summary: [DirSideSummary::default(); 2],
                    }
                }),
        );
    }
    entries.sort_by(tree_order);
    recompute_dir_summaries(&mut entries);
    Scan { entries, errors }
}

fn recompute_dir_summaries(entries: &mut [Entry]) {
    let mut changed_parents = BTreeSet::new();
    let mut dir_summaries: BTreeMap<String, [DirSideSummary; 2]> = BTreeMap::new();
    for entry in entries.iter() {
        let effective_diff = !entry.ignored && entry.status != Status::Same;
        if effective_diff {
            for parent in entry
                .relative
                .ancestors()
                .skip(1)
                .filter(|p| !p.as_os_str().is_empty())
            {
                changed_parents.insert(path_key(parent));
            }
        }
        let is_file_entry = !is_dir(entry);
        let left_bytes = entry
            .left
            .as_ref()
            .filter(|m| m.is_file())
            .map_or(0, EntryMeta::len);
        let right_bytes = entry
            .right
            .as_ref()
            .filter(|m| m.is_file())
            .map_or(0, EntryMeta::len);
        let left_color = if is_file_entry {
            entry_color(entry, true, false)
        } else {
            Color32::BLACK
        };
        let right_color = if is_file_entry {
            entry_color(entry, false, false)
        } else {
            Color32::BLACK
        };
        for parent in entry
            .relative
            .ancestors()
            .skip(1)
            .filter(|p| !p.as_os_str().is_empty())
        {
            let summary = dir_summaries.entry(path_key(parent)).or_default();
            summary[0].bytes = summary[0].bytes.saturating_add(left_bytes);
            summary[1].bytes = summary[1].bytes.saturating_add(right_bytes);
            if entry.left.is_some() && !entry.ignored {
                if entry.status == Status::LeftOnly {
                    summary[0].has_orphan = true;
                } else if is_file_entry {
                    if left_color == Color32::RED || left_color == Color32::DARK_RED {
                        summary[0].has_newer_or_diff = true;
                    } else if left_color == Color32::GRAY {
                        summary[0].has_older = true;
                    }
                }
            }
            if entry.right.is_some() && !entry.ignored {
                if entry.status == Status::RightOnly {
                    summary[1].has_orphan = true;
                } else if is_file_entry {
                    if right_color == Color32::RED || right_color == Color32::DARK_RED {
                        summary[1].has_newer_or_diff = true;
                    } else if right_color == Color32::GRAY {
                        summary[1].has_older = true;
                    }
                }
            }
        }
    }
    for entry in entries.iter_mut() {
        if is_dir(entry) {
            let key = path_key(&entry.relative);
            entry.dir_summary = dir_summaries.get(&key).copied().unwrap_or_default();
            if entry.left.as_ref().is_some_and(|m| m.is_dir())
                && entry.right.as_ref().is_some_and(|m| m.is_dir())
            {
                entry.status = if !entry.ignored && changed_parents.contains(&key) {
                    Status::Different
                } else {
                    Status::Same
                };
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(windows)]
    fn locked_file_is_reported_as_an_error_instead_of_a_match() {
        use std::os::windows::fs::OpenOptionsExt;
        let root =
            std::env::temp_dir().join(format!("anycompare-folder-locked-{}", std::process::id()));
        let (left, right) = (root.join("left"), root.join("right"));
        fs::create_dir_all(&left).unwrap();
        fs::create_dir_all(&right).unwrap();
        fs::write(left.join("locked.bin"), b"same").unwrap();
        fs::write(right.join("locked.bin"), b"same").unwrap();
        let locked = fs::OpenOptions::new()
            .read(true)
            .share_mode(6)
            .open(right.join("locked.bin"))
            .unwrap();
        let scan = scan_folders(&left, &right, true, &AtomicBool::new(false));
        assert!(!scan.errors.is_empty());
        assert!(scan
            .entries
            .iter()
            .any(|entry| entry.status == Status::Error && entry.error.is_some()));
        drop(locked);
        for file in [left.join("locked.bin"), right.join("locked.bin")] {
            fs::remove_file(file).unwrap();
        }
        fs::remove_dir(left).unwrap();
        fs::remove_dir(right).unwrap();
        fs::remove_dir(root).unwrap();
    }
    #[test]
    fn folder_commands_confirm_copy_reject_bad_rename_and_keep_cancelled_delete() {
        let root = std::env::temp_dir().join(format!(
            "anycompare-folder-menu-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let left = root.join("left");
        let right = root.join("right");
        fs::create_dir_all(&left).unwrap();
        fs::create_dir(&right).unwrap();
        fs::write(left.join("item"), "esquerdo").unwrap();
        fs::write(right.join("item"), "direito").unwrap();
        let mut screen = FolderCompareScreen::new();
        screen.left_path = left.display().to_string();
        screen.right_path = right.display().to_string();
        screen.loaded_left = left.clone();
        screen.loaded_right = right.clone();
        screen.entries = scan_folders(&left, &right, true, &AtomicBool::new(false)).entries;
        screen.selected.insert(EntryKey {
            relative: "item".into(),
            directory: false,
        });
        let mut dialogs = super::super::dialogs::TestDialogs::default();
        dialogs.confirmations.push_back(false);
        screen.copy_selected(true, &mut dialogs);
        assert!(screen.pending.is_none());
        assert_eq!(fs::read_to_string(right.join("item")).unwrap(), "direito");
        dialogs.confirmations.push_back(true);
        screen.copy_selected(true, &mut dialogs);
        let scan = screen
            .pending
            .take()
            .unwrap()
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        assert!(scan.errors.is_empty());
        assert_eq!(
            fs::read(left.join("item")).unwrap(),
            fs::read(right.join("item")).unwrap()
        );
        screen.entries = scan.entries;
        dialogs.inputs.push_back(Some("../bad".into()));
        screen.rename_selected(&mut dialogs);
        assert_eq!(dialogs.errors.len(), 1);
        assert!(left.join("item").is_file());
        dialogs.confirmations.push_back(false);
        screen.delete_selected(&mut dialogs);
        assert!(dialogs.recycled.is_empty());
        dialogs.confirmations.push_back(true);
        screen.delete_selected(&mut dialogs);
        assert_eq!(dialogs.recycled.len(), 1);
        // Native recycle is mocked here; no real system Trash or user files are touched.
        screen.cancel.store(true, Ordering::Relaxed);
        if let Some(receiver) = screen.pending.take() {
            let _ = receiver.recv_timeout(std::time::Duration::from_secs(5));
        }
        fs::remove_file(left.join("item")).unwrap();
        fs::remove_file(right.join("item")).unwrap();
        fs::remove_dir(left).unwrap();
        fs::remove_dir(right).unwrap();
        fs::remove_dir(root).unwrap();
    }
    #[test]
    fn invalid_rescan_cancels_previous_scan_and_drops_stale_results() {
        let mut screen = FolderCompareScreen::new();
        let (sender, receiver) = mpsc::channel();
        screen.pending = Some(receiver);
        let old_cancel = screen.cancel.clone();
        screen.left_path = String::new();
        screen.right_path = String::new();
        screen.start_scan();
        assert!(old_cancel.load(Ordering::Relaxed));
        assert!(screen.pending.is_none());
        assert!(screen.loaded_left.as_os_str().is_empty());
        assert!(screen.loaded_right.as_os_str().is_empty());
        assert!(sender
            .send(Scan {
                entries: Vec::new(),
                errors: Vec::new()
            })
            .is_err());
        assert_eq!(
            screen.errors,
            ["folder.select_two_existing_folders_to_compare"]
        );
    }
    #[test]
    fn block_reader_accepts_short_reads_before_eof() {
        use std::io::Read;
        struct ShortReader(std::io::Cursor<Vec<u8>>);
        impl Read for ShortReader {
            fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
                let count = buffer.len().min(3);
                self.0.read(&mut buffer[..count])
            }
        }
        let mut reader = ShortReader(std::io::Cursor::new(b"0123456789abcdef".to_vec()));
        let mut buffer = [0; 10];
        assert_eq!(folder_source::read_block(&mut reader, &mut buffer).unwrap(), 10);
        assert_eq!(&buffer, b"0123456789");
        assert_eq!(folder_source::read_block(&mut reader, &mut buffer).unwrap(), 6);
        assert_eq!(&buffer[..6], b"abcdef");
    }
    #[test]
    fn content_comparison_detects_equal_size_changes_and_orphans() {
        let root =
            std::env::temp_dir().join(format!("anycompare-folder-test-{}", std::process::id()));
        let left = root.join("left");
        let right = root.join("right");
        fs::create_dir_all(&left).unwrap();
        fs::create_dir_all(&right).unwrap();
        fs::write(left.join("same"), b"equal").unwrap();
        fs::write(right.join("same"), b"equal").unwrap();
        fs::write(left.join("changed"), b"left").unwrap();
        fs::write(right.join("changed"), b"diff").unwrap();
        fs::write(left.join("orphan"), b"only left").unwrap();
        let scan = scan_folders(&left, &right, true, &AtomicBool::new(false));
        assert!(scan.errors.is_empty());
        assert_eq!(
            scan.entries
                .iter()
                .map(|entry| entry.status)
                .collect::<Vec<_>>(),
            vec![Status::Different, Status::LeftOnly, Status::Same]
        );
        // Remove only these explicitly named test files/directories; no recursive cleanup.
        for file in [
            left.join("same"),
            right.join("same"),
            left.join("changed"),
            right.join("changed"),
            left.join("orphan"),
        ] {
            fs::remove_file(file).unwrap();
        }
        fs::remove_dir(left).unwrap();
        fs::remove_dir(right).unwrap();
        fs::remove_dir(root).unwrap();
    }

    #[test]
    fn folder_icon_summary_propagates_red_difference_and_aggregated_bytes() {
        let root = std::env::temp_dir().join(format!(
            "anycompare-dir-summary-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let left_sub = root.join("left/pkg");
        let right_sub = root.join("right/pkg");
        fs::create_dir_all(&left_sub).unwrap();
        fs::create_dir_all(&right_sub).unwrap();
        fs::write(left_sub.join("data.bin"), b"12345678").unwrap();
        fs::write(right_sub.join("data.bin"), b"1234567890").unwrap();
        let scan = scan_folders(
            &root.join("left"),
            &root.join("right"),
            true,
            &AtomicBool::new(false),
        );
        let top_dir = scan
            .entries
            .iter()
            .find(|e| is_dir(e) && e.relative == Path::new("pkg"))
            .expect("pkg directory");
        assert_eq!(top_dir.status, Status::Different);
        assert_eq!(top_dir.dir_summary[0].bytes, 8);
        assert_eq!(top_dir.dir_summary[1].bytes, 10);
        let (top_left, _) = top_dir.dir_summary[0].icon_halves(false);
        let (top_right, _) = top_dir.dir_summary[1].icon_halves(false);
        let yellow = Color32::from_rgb(192, 160, 0);
        assert!(
            top_left != yellow || top_right != yellow,
            "differing nested folder must show red/gray/purple rather than plain yellow"
        );
        fs::remove_file(left_sub.join("data.bin")).unwrap();
        fs::remove_file(right_sub.join("data.bin")).unwrap();
        fs::remove_dir(&left_sub).unwrap();
        fs::remove_dir(&right_sub).unwrap();
        fs::remove_dir(root.join("left")).unwrap();
        fs::remove_dir(root.join("right")).unwrap();
        fs::remove_dir(&root).unwrap();
    }

    #[test]
    fn all_twelve_display_filters_four_dir_handlings_minor_and_archive_extract_work() {
        use std::io::Write;
        use std::time::Duration;
        let root = std::env::temp_dir().join(format!(
            "anycompare-folder-filters-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let left = root.join("left");
        let right = root.join("right");
        fs::create_dir_all(left.join("sub")).unwrap();
        fs::create_dir_all(right.join("sub")).unwrap();
        fs::create_dir_all(left.join("empty_only_left")).unwrap();

        let t_old = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000);
        let t_new = t_old + Duration::from_secs(300);
        let t_minor = t_old + Duration::from_secs(1);

        let write_with_time = |p: PathBuf, data: &[u8], mtime: SystemTime| {
            let f = fs::File::create(&p).unwrap();
            (&f).write_all(data).unwrap();
            let _ = f.set_modified(mtime);
        };
        write_with_time(left.join("same.txt"), b"same", t_old);
        write_with_time(right.join("same.txt"), b"same", t_old);
        write_with_time(left.join("sub/left_newer.txt"), b"L_NEW", t_new);
        write_with_time(right.join("sub/left_newer.txt"), b"R_OLD", t_old);
        write_with_time(left.join("sub/right_newer.txt"), b"L_OLD", t_old);
        write_with_time(right.join("sub/right_newer.txt"), b"R_NEW", t_new);
        write_with_time(left.join("minor.txt"), b"1234", t_old);
        write_with_time(right.join("minor.txt"), b"1234", t_minor);
        write_with_time(left.join("orphan_l.txt"), b"L", t_old);
        write_with_time(right.join("orphan_r.txt"), b"R", t_old);

        let mut screen = FolderCompareScreen::new();
        screen.compare_content = false; // timestamp mode exercises InsigIgnored 2s tolerance
        screen.load_folders(&left, &right);
        screen.comparison_results();
        screen.collapsed.clear();

        let visible_files = |s: &FolderCompareScreen| -> Vec<String> {
            s.visible_indices()
                .into_iter()
                .map(|i| &s.entries[i])
                .filter(|e| !is_dir(e))
                .map(|e| e.relative.to_string_lossy().replace('\\', "/"))
                .collect()
        };

        // Check all 12 DisplayFilter modes
        screen.display_filter = DisplayFilter::All;
        assert_eq!(visible_files(&screen).len(), 6);
        screen.display_filter = DisplayFilter::Same;
        assert_eq!(visible_files(&screen), vec!["same.txt"]);
        screen.display_filter = DisplayFilter::Orphan;
        assert_eq!(visible_files(&screen), vec!["orphan_l.txt", "orphan_r.txt"]);
        screen.display_filter = DisplayFilter::LeftOrphan;
        assert_eq!(visible_files(&screen), vec!["orphan_l.txt"]);
        screen.display_filter = DisplayFilter::RightOrphan;
        assert_eq!(visible_files(&screen), vec!["orphan_r.txt"]);
        screen.display_filter = DisplayFilter::NoOrphan;
        assert_eq!(visible_files(&screen).len(), 4);
        screen.display_filter = DisplayFilter::DiffNoOrphan;
        assert_eq!(visible_files(&screen).len(), 3);
        screen.display_filter = DisplayFilter::LeftNewer;
        assert_eq!(visible_files(&screen), vec!["sub/left_newer.txt"]);
        screen.display_filter = DisplayFilter::RightNewer;
        assert_eq!(visible_files(&screen), vec!["sub/right_newer.txt", "minor.txt"]);
        screen.display_filter = DisplayFilter::LeftNewerOrphan;
        assert_eq!(
            visible_files(&screen),
            vec!["sub/left_newer.txt", "orphan_l.txt"]
        );
        screen.display_filter = DisplayFilter::RightNewerOrphan;
        assert_eq!(
            visible_files(&screen),
            vec!["sub/right_newer.txt", "minor.txt", "orphan_r.txt"]
        );

        // InsigIgnored treats 1-second timestamp diff as Same!
        screen.ignore_minor = true;
        screen.display_filter = DisplayFilter::Same;
        assert_eq!(visible_files(&screen), vec!["minor.txt", "same.txt"]);
        screen.display_filter = DisplayFilter::Diff;
        assert!(!visible_files(&screen).contains(&"minor.txt".to_string()));

        // Check all 4 DirHandling modes
        screen.ignore_minor = false;
        screen.display_filter = DisplayFilter::Same;
        let visible_dirs = |s: &FolderCompareScreen| -> Vec<String> {
            s.visible_indices()
                .into_iter()
                .map(|i| &s.entries[i])
                .filter(|e| is_dir(e))
                .map(|e| e.relative.to_string_lossy().replace('\\', "/"))
                .collect()
        };
        screen.dir_handling = DirHandling::ShowAll;
        assert!(visible_dirs(&screen).contains(&"empty_only_left".to_string()));
        screen.dir_handling = DirHandling::CompareFilesOnly;
        assert!(visible_dirs(&screen).is_empty()); // same.txt is at root, no subdir has same files
        screen.dir_handling = DirHandling::Flatten;
        assert!(visible_dirs(&screen).is_empty());
        assert_eq!(visible_files(&screen), vec!["same.txt"]);
        screen.dir_handling = DirHandling::CompareStructure;
        assert!(!visible_dirs(&screen).contains(&"empty_only_left".to_string()));

        // Verify archive-to-folder copy extracts files & read-only guard blocks folder-to-archive write
        let zip_path = root.join("src.zip");
        {
            let f = fs::File::create(&zip_path).unwrap();
            let mut zw = zip::ZipWriter::new(f);
            let opts = zip::write::SimpleFileOptions::default();
            zw.start_file("from_zip.txt", opts).unwrap();
            zw.write_all(b"extracted payload").unwrap();
            zw.finish().unwrap();
        }
        let target_dir = root.join("extract_target");
        fs::create_dir_all(&target_dir).unwrap();
        let mut zscreen = FolderCompareScreen::new();
        zscreen.load_folders(&zip_path, &target_dir);
        zscreen.comparison_results();
        zscreen.selected.clear();
        zscreen.selected.insert(EntryKey {
            relative: PathBuf::from("from_zip.txt"),
            directory: false,
        });
        let mut dialogs = super::super::dialogs::TestDialogs::default();
        // Writing INTO the ZIP must fail with read-only guard!
        zscreen.copy_selected(false, &mut dialogs);
        assert_eq!(dialogs.errors.len(), 1);
        // Extracting FROM the ZIP into target_dir must succeed!
        dialogs.confirmations.push_back(true);
        zscreen.copy_selected(true, &mut dialogs);
        let scan = zscreen
            .pending
            .take()
            .unwrap()
            .recv_timeout(Duration::from_secs(5))
            .unwrap();
        assert!(scan.errors.is_empty(), "{:?}", scan.errors);
        assert_eq!(
            fs::read(target_dir.join("from_zip.txt")).unwrap(),
            b"extracted payload"
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn ctrl_a_collapsed_selection_stats_compare_contents_modes_and_context_ops_work() {
        use std::io::Write;
        use std::time::Duration;
        let root = std::env::temp_dir().join(format!(
            "anycompare-folder-ctx-ops-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let left = root.join("left");
        let right = root.join("right");
        fs::create_dir_all(left.join("pkg/sub")).unwrap();
        fs::create_dir_all(right.join("pkg/sub")).unwrap();

        let t_old = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000);
        let t_new = t_old + Duration::from_secs(600);
        let write_t = |p: PathBuf, data: &[u8], mtime: SystemTime| {
            let f = fs::File::create(&p).unwrap();
            (&f).write_all(data).unwrap();
            let _ = f.set_modified(mtime);
        };
        // 3 files inside collapsed folder `pkg`:
        // 1. same bytes, different timestamp -> Same under Binary/CRC/Rules!
        write_t(left.join("pkg/a.bin"), b"0123456789", t_new);
        write_t(right.join("pkg/a.bin"), b"0123456789", t_old);
        // 2. CRLF vs LF -> Different under Binary/CRC, Same under RulesBased!
        write_t(left.join("pkg/b.txt"), b"line1\r\nline2\r\n", t_old);
        write_t(right.join("pkg/b.txt"), b"line1\nline2\n", t_old);
        // 3. nested file inside sub -> genuinely different bytes
        write_t(left.join("pkg/sub/c.txt"), b"alpha", t_old);
        write_t(right.join("pkg/sub/c.txt"), b"omega", t_old);

        let mut screen = FolderCompareScreen::new();
        screen.compare_content = false; // start with quick timestamp+size comparison
        screen.load_folders(&left, &right);
        screen.comparison_results();

        // Simulate Ctrl+A immediately after load (without clicking a row first!)
        let ctx = Context::default();
        super::super::theme::apply(&ctx);
        let mut icons = IconManager::new();
        let mut dialogs = super::super::dialogs::TestDialogs::default();
        let _ = ctx.run(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1150.0, 720.0))),
                events: vec![Event::Key {
                    key: Key::A,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers::CTRL,
                }],
                ..Default::default()
            },
            |ctx| {
                CentralPanel::default().show(ctx, |ui| {
                    screen.ui(ctx, ui, &mut icons, &mut dialogs);
                });
            },
        );
        // `pkg` is collapsed at root; Ctrl+A selects it and selection_stats counts ALL 3 files + 2 folders!
        assert!(screen.selected.contains(&EntryKey {
            relative: PathBuf::from("pkg"),
            directory: true,
        }));
        let stats = screen.selection_stats();
        assert_eq!(stats.left_files, 3);
        assert_eq!(stats.right_files, 3);
        assert_eq!(stats.left_dirs, 2); // pkg + pkg/sub
        assert_eq!(stats.left_bytes, 10 + 14 + 5);

        // Run Compare Contents in Binary mode: a.bin becomes Same (overrides timestamp diff!), b.txt & c.txt Different.
        screen.start_content_compare(ContentCompareMode::Binary, true, false);
        let outcome = screen
            .pending_content
            .take()
            .unwrap()
            .recv_timeout(Duration::from_secs(5))
            .unwrap();
        assert_eq!(outcome.results.same, 1);
        assert_eq!(outcome.results.different, 2);
        screen.accept_content_compare(outcome);
        assert!(matches!(screen.modal, Some(FolderOpModal::CompareResults(_))));
        screen.modal = None;

        let status_of = |s: &FolderCompareScreen, p: &str| {
            s.entries
                .iter()
                .find(|e| e.relative == Path::new(p))
                .unwrap()
                .status
        };
        assert_eq!(status_of(&screen, "pkg/a.bin"), Status::Same);
        assert_eq!(status_of(&screen, "pkg/b.txt"), Status::Different);

        // Run Compare Contents in RulesBased mode: b.txt (CRLF vs LF) ALSO becomes Same!
        screen.start_content_compare(ContentCompareMode::RulesBased, false, false);
        let outcome = screen
            .pending_content
            .take()
            .unwrap()
            .recv_timeout(Duration::from_secs(5))
            .unwrap();
        assert_eq!(outcome.results.same, 2);
        assert_eq!(outcome.results.different, 1);
        screen.accept_content_compare(outcome);
        assert_eq!(status_of(&screen, "pkg/b.txt"), Status::Same);
        assert_eq!(status_of(&screen, "pkg/sub/c.txt"), Status::Different);

        // Toggle Ignorado on selection marks all entries ignored and turns `pkg` back to Same!
        screen.toggle_ignored();
        assert!(screen.entries.iter().all(|e| e.ignored));
        assert_eq!(status_of(&screen, "pkg"), Status::Same);
        screen.toggle_ignored(); // un-ignore
        assert_eq!(status_of(&screen, "pkg"), Status::Different);

        // Open Subfolders / Close Subfolders
        screen.open_selected_folders(true, true);
        assert!(screen.collapsed.is_empty());
        screen.open_selected_folders(false, true);
        assert!(screen.collapsed.contains("pkg"));
        assert!(screen.collapsed.contains("pkg\\sub") || screen.collapsed.contains("pkg/sub"));

        // Exclude from session hides matching files
        screen.exclude_patterns.push("c.txt".into());
        assert!(!screen
            .entries
            .iter()
            .enumerate()
            .filter(|(i, _)| screen.visible_indices().contains(i))
            .any(|(_, e)| e.relative.ends_with("c.txt")));
        screen.exclude_patterns.clear();

        // Copy to Folder (Flatten vs KeepRelative)
        let ext_rel = root.join("ext_rel");
        let ext_flat = root.join("ext_flat");
        screen.execute_external_transfer(
            true,
            ext_rel.clone(),
            FolderStructureMode::KeepRelative,
            false,
            &mut dialogs,
        );
        assert!(ext_rel.join("pkg/sub/c.txt").is_file());
        screen.execute_external_transfer(
            true,
            ext_flat.clone(),
            FolderStructureMode::Flatten,
            false,
            &mut dialogs,
        );
        assert!(ext_flat.join("c.txt").is_file());

        // Touch (Copy timestamps from other side)
        screen.execute_touch(true, TouchMode::CopyOtherSide, "", true, &mut dialogs);
        if let Some(rx) = screen.pending.take() {
            let scan = rx.recv_timeout(Duration::from_secs(5)).unwrap();
            screen.accept_scan(scan);
        }
        assert!(dialogs.errors.is_empty(), "{:?}", dialogs.errors);

        // Set as Base Folders navigates into `pkg`
        screen.selected.clear();
        screen.selected.insert(EntryKey {
            relative: PathBuf::from("pkg"),
            directory: true,
        });
        screen.set_selected_as_base_folders();
        screen.comparison_results();
        assert!(screen.entries.iter().any(|e| e.relative == Path::new("a.bin")));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn folder_compare_criteria_dst_timezone_size_and_content_overrides_work() {
        use std::time::Duration;
        let root = std::env::temp_dir().join(format!(
            "anycompare_criteria_test_{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        let left = root.join("left");
        let right = root.join("right");
        fs::create_dir_all(&left).unwrap();
        fs::create_dir_all(&right).unwrap();

        let t0 = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000);
        let t_dst = t0 + Duration::from_secs(3600);
        let t_tz = t0 + Duration::from_secs(3 * 3600);
        let write_ts = |p: PathBuf, data: &[u8], ts: SystemTime| {
            fs::write(&p, data).unwrap();
            let f = fs::File::options().write(true).open(&p).unwrap();
            let _ = f.set_modified(ts);
        };

        write_ts(left.join("dst.txt"), b"same", t0);
        write_ts(right.join("dst.txt"), b"same", t_dst);
        write_ts(left.join("tz.txt"), b"same", t0);
        write_ts(right.join("tz.txt"), b"same", t_tz);
        write_ts(left.join("size.txt"), b"short", t0);
        write_ts(right.join("size.txt"), b"much-longer", t0);
        write_ts(left.join("ext.txt"), b"hello", t0);
        write_ts(right.join("ext.md"), b"hello", t0);

        let cancel = AtomicBool::new(false);
        let base = FolderCompareCriteria {
            compare_content: false,
            ..FolderCompareCriteria::default()
        };

        // Default quick tests: dst, tz, size all differ; ext.txt is LeftOnly
        let s0 = scan_folders_with_criteria(&left, &right, &base, &cancel);
        let st = |scan: &Scan, name: &str| {
            scan.entries
                .iter()
                .find(|e| e.relative == Path::new(name))
                .map(|e| e.status)
        };
        assert_eq!(st(&s0, "dst.txt"), Some(Status::Different));
        assert_eq!(st(&s0, "tz.txt"), Some(Status::Different));
        assert_eq!(st(&s0, "size.txt"), Some(Status::Different));
        assert_eq!(st(&s0, "ext.txt"), Some(Status::LeftOnly));

        // Enable ignore_dst + ignore_timezone + align_different_extensions + compare_size=false
        let c1 = FolderCompareCriteria {
            compare_size: false,
            ignore_dst: true,
            ignore_timezone: true,
            align_different_extensions: true,
            ..base.clone()
        };
        let s1 = scan_folders_with_criteria(&left, &right, &c1, &cancel);
        assert_eq!(st(&s1, "dst.txt"), Some(Status::Same));
        assert_eq!(st(&s1, "tz.txt"), Some(Status::Same));
        assert_eq!(st(&s1, "size.txt"), Some(Status::Same));
        assert_eq!(st(&s1, "ext.txt"), Some(Status::Same));

        // Content compare with override_quick_results = false preserves timestamp diff
        let c2 = FolderCompareCriteria {
            compare_content: true,
            override_quick_results: false,
            ..base.clone()
        };
        let s2 = scan_folders_with_criteria(&left, &right, &c2, &cancel);
        assert_eq!(st(&s2, "dst.txt"), Some(Status::Different));

        // Content compare with override_quick_results = true overrides timestamp diff to Same
        let c3 = FolderCompareCriteria {
            compare_content: true,
            override_quick_results: true,
            ..base
        };
        let s3 = scan_folders_with_criteria(&left, &right, &c3, &cancel);
        assert_eq!(st(&s3, "dst.txt"), Some(Status::Same));

        let _ = fs::remove_dir_all(&root);
    }
}
