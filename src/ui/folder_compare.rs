use super::toolbar::{ToolEvent, ToolState};
use crate::ui::{icons::IconManager, theme::BcTheme};
use egui::*;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{BufReader, Read},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    thread,
    time::SystemTime,
};
use walkdir::WalkDir;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Status {
    Same,
    Different,
    LeftOnly,
    RightOnly,
    Error,
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
        let size_x = date_x - 68.0;
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
    let number = format!("{value:.1}");
    let number = number.strip_suffix(".0").unwrap_or(&number);
    let number = if crate::i18n::language().starts_with("pt") {
        number.replace('.', ",")
    } else {
        number.to_owned()
    };
    format!("{number} {suffix}")
}

fn disk_space(path: &Path) -> Option<(u64, String)> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        let path = path.canonicalize().ok()?;
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

fn entry_color(entry: &Entry, left: bool) -> Color32 {
    if is_dir(entry) {
        // Native folder names remain black; the two icon halves describe the sides.
        return Color32::BLACK;
    }
    match entry.status {
        Status::LeftOnly | Status::RightOnly => ORPHAN_COLOR,
        Status::Error => Color32::DARK_RED,
        _ => {
            let time = |metadata: &Option<fs::Metadata>| {
                metadata
                    .as_ref()?
                    .modified()
                    .ok()?
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .ok()
                    .map(|d| d.as_secs())
            };
            let (own, other) = if left {
                (time(&entry.left), time(&entry.right))
            } else {
                (time(&entry.right), time(&entry.left))
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

struct Entry {
    id: EntryKey,
    relative: PathBuf,
    right_relative: PathBuf,
    left: Option<fs::Metadata>,
    right: Option<fs::Metadata>,
    status: Status,
    error: Option<String>,
}

struct Scan {
    entries: Vec<Entry>,
    errors: Vec<String>,
}
pub enum FolderAction {
    None,
    Home,
    OpenFiles(PathBuf, PathBuf),
}
pub struct FolderCompareScreen {
    pub left_path: String,
    pub right_path: String,
    entries: Vec<Entry>,
    errors: Vec<String>,
    filter: String,
    only_diffs: bool,
    only_same: bool,
    compare_content: bool,
    pending: Option<mpsc::Receiver<Scan>>,
    cancel: Arc<AtomicBool>,
    loaded_left: PathBuf,
    loaded_right: PathBuf,
    log: Vec<String>,
    selected: BTreeSet<EntryKey>,
    active_left: bool,
    show_log: bool,
    tree: bool,
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
}

impl FolderCompareScreen {
    pub fn session_options(&self) -> serde_json::Value {
        serde_json::json!({"filter":self.filter,"only_diffs":self.only_diffs,"only_same":self.only_same,"content":self.compare_content,"tree":self.tree,"log":self.show_log})
    }

    pub fn apply_session_options(&mut self, value: &serde_json::Value) {
        for (key, target) in [
            ("only_diffs", &mut self.only_diffs),
            ("only_same", &mut self.only_same),
            ("content", &mut self.compare_content),
            ("tree", &mut self.tree),
            ("log", &mut self.show_log),
        ] {
            if let Some(value) = value[key].as_bool() {
                *target = value;
            }
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
            entries: Vec::new(),
            errors: Vec::new(),
            filter: DEFAULT_FILTER.into(),
            only_diffs: false,
            only_same: false,
            compare_content: true,
            pending: None,
            cancel: Arc::new(AtomicBool::new(false)),
            loaded_left: PathBuf::new(),
            loaded_right: PathBuf::new(),
            log: Vec::new(),
            selected: BTreeSet::new(),
            active_left: true,
            show_log: true,
            tree: true,
            collapsed: BTreeSet::new(),
            tree_roots: Default::default(),
            recent_paths: Vec::new(),
            root_history: Vec::new(),
            root_cursor: 0,
            free_space: Default::default(),
            scroll_to_selected: false,
            find_text: String::new(),
            selection_focus: false,
            suppress_filters: false,
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
        self.entries.clear();
        self.selected.clear();
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
        self.pending.is_some()
    }

    pub fn load_folders(&mut self, left: &Path, right: &Path) {
        self.left_path = left.to_string_lossy().into();
        self.right_path = right.to_string_lossy().into();
        self.start_scan();
    }

    fn accept_scan(&mut self, scan: Scan) {
        let roots = (self.loaded_left.clone(), self.loaded_right.clone());
        if roots != self.tree_roots {
            self.collapsed = scan
                .entries
                .iter()
                .filter(|entry| is_dir(entry))
                .map(|entry| path_key(&entry.relative))
                .collect();
            if let Some(entry) = scan.entries.first() {
                self.selected.insert(entry.id.clone());
            }
            self.tree_roots = roots;
        }
        self.entries = scan.entries;
        self.errors = scan.errors;
        self.pending = None;
    }

    fn start_scan(&mut self) {
        self.clear_scan();
        let left = PathBuf::from(&self.left_path);
        let right = PathBuf::from(&self.right_path);
        if !left.is_dir() || !right.is_dir() {
            self.errors = vec!["Escolha duas pastas existentes para comparar.".into()];
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
            self.log.push(format!(
                "{} Usuário: {}\\{}",
                chrono::Local::now().format("%d/%m/%Y %H:%M:%S"),
                std::env::var("COMPUTERNAME").unwrap_or_default(),
                std::env::var("USERNAME").unwrap_or_default(),
            ));
        }
        self.log.push(format!(
            "{} Carregar comparação: {} <-> {}",
            chrono::Local::now().format("%d/%m/%Y %H:%M:%S"),
            left.display(),
            right.display()
        ));
        self.cancel = Arc::new(AtomicBool::new(false));
        let cancel = self.cancel.clone();
        let content = self.compare_content;
        let (sender, receiver) = mpsc::channel();
        self.pending = Some(receiver);
        self.errors.clear();
        thread::spawn(move || {
            let _ = sender.send(scan_folders(&left, &right, content, &cancel));
        });
    }

    fn visible_indices(&self) -> Vec<usize> {
        let mut matching: Vec<bool> = self
            .entries
            .iter()
            .map(|entry| {
                (!self.only_diffs || entry.status != Status::Same)
                    && (!self.only_same || entry.status == Status::Same)
                    && (self.suppress_filters || name_filter_matches(&self.filter, entry))
            })
            .collect();
        if self.tree {
            // Retain folder context for matching descendants: a mixed folder
            // must still be shown when the user selects "Same".
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
                    && (!self.tree
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
        let preview = plan
            .items
            .iter()
            .take(12)
            .map(|(source, target)| format!("{} → {}", source.display(), target.display()))
            .collect::<Vec<_>>()
            .join("\n");
        if !dialogs.confirm(&format!(
            "Copiar {} itens?\n{} arquivos serão substituídos.\n\n{preview}",
            plan.items.len(),
            plan.overwrite_count
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
        self.log.push(format!(
            "Copiar seleção: {} itens para {}",
            plan.items.len(),
            target.display()
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

    fn rename_selected(&mut self, dialogs: &mut dyn super::dialogs::DialogHost) {
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
        if let Some(name) = dialogs.input("Renomear", "Novo nome:", &initial) {
            let result = (|| -> Result<(), String> {
                let name = crate::file_ops::leaf(&name)?;
                let target = source.parent().unwrap().join(name);
                if target.exists() {
                    return Err("Já existe um item com esse nome.".into());
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

    fn delete_selected(&mut self, dialogs: &mut dyn super::dialogs::DialogHost) {
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
        if dialogs.confirm(&format!(
            "Enviar {} itens à Lixeira?\n\n{preview}",
            paths.len()
        )) {
            match dialogs.recycle(&paths) {
                Ok(()) => self.start_scan(),
                Err(error) => dialogs.error(&error),
            }
        }
    }

    fn new_folder(&mut self, dialogs: &mut dyn super::dialogs::DialogHost) {
        let root = if self.active_left {
            &self.loaded_left
        } else {
            &self.loaded_right
        };
        if !root.is_dir() {
            return;
        }
        if let Some(name) = dialogs.input("Nova pasta", "Nome da pasta:", "") {
            let result = (|| -> Result<(), String> {
                let name = crate::file_ops::leaf(&name)?;
                let path = crate::file_ops::resolve(root, name)?;
                fs::create_dir(path).map_err(|e| e.to_string())
            })();
            if let Err(error) = result {
                dialogs.error(&error);
            } else {
                self.start_scan();
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
                "Comparar conteúdo dos arquivos".into(),
                self.compare_content,
            ),
            DialogField::Check("Mostrar estrutura de pastas".into(), self.tree),
            DialogField::Check("Mostrar log".into(), self.show_log),
            crate::i18n::language_field(),
        ];
        if dialogs.fields("Opções de pastas", &mut fields) {
            let previous = self.compare_content;
            if let DialogField::Check(_, value) = fields[0] {
                self.compare_content = value;
            }
            if let DialogField::Check(_, value) = fields[1] {
                self.tree = value;
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
        let describe = |metadata: &Option<fs::Metadata>| {
            metadata
                .as_ref()
                .map(|m| {
                    if m.is_dir() {
                        "Pasta".into()
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
            "Comparar Pastas",
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

    pub fn ui(
        &mut self,
        ctx: &Context,
        ui: &mut Ui,
        icons: &mut IconManager,
        dialogs: &mut dyn super::dialogs::DialogHost,
    ) -> FolderAction {
        let mut action = FolderAction::None;
        let can_operate = self.pending.is_none() && !self.selected.is_empty();
        if can_operate && self.selection_focus {
            if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::R)) {
                self.copy_selected(true, dialogs);
            }
            if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::L)) {
                self.copy_selected(false, dialogs);
            }
            if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::F2)) {
                self.rename_selected(dialogs);
            }
            if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Delete)) {
                self.delete_selected(dialogs);
            }
            if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Insert)) {
                self.new_folder(dialogs);
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
            if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::A)) {
                self.selected = self
                    .visible_indices()
                    .into_iter()
                    .map(|index| &self.entries[index])
                    .map(|e| e.id.clone())
                    .collect();
            }
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::N)) {
            self.navigate(true, None);
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::P)) {
            self.navigate(false, None);
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::F)) {
            if let Some(query) = dialogs.input("Localizar nome", "Nome ou trecho:", &self.find_text)
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
        if let Some(receiver) = &self.pending {
            match receiver.try_recv() {
                Ok(scan) => {
                    self.accept_scan(scan);
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.pending = None;
                    self.errors.push("A comparação foi interrompida.".into());
                }
                Err(mpsc::TryRecvError::Empty) => {
                    ctx.request_repaint_after(std::time::Duration::from_millis(100))
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
                    super::chrome::menu_button(ui, "Sessão", |ui| {
                        super::session_menu::prefix(
                            ui,
                            icons,
                            !self.left_path.is_empty() && !self.right_path.is_empty(),
                        );
                        super::session_menu::close(ui, icons);
                        ui.separator();
                        if super::chrome::menu_item(ui, icons, "Relatório de pastas...", true)
                            .clicked()
                        {
                            self.report(dialogs);
                            ui.close_menu();
                        }
                        if super::chrome::menu_item(ui, icons, "Informações da comparação", true)
                            .clicked()
                        {
                            dialogs.info(&format!(
                                "Esquerda: {}\nDireita: {}\n\n{} itens, {} erros",
                                self.left_path,
                                self.right_path,
                                self.entries.len(),
                                self.errors.len()
                            ));
                            ui.close_menu();
                        }
                        if super::chrome::menu_item(ui, icons, "Página Inicial", true).clicked() {
                            action = FolderAction::Home;
                            ui.close_menu();
                        }
                        if super::chrome::menu_item(ui, icons, "Inverter lados", true).clicked() {
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
                    super::chrome::menu_button(ui, "Ações", |ui| {
                        for (label, right) in [
                            ("Copiar para a Direita  Ctrl+R", true),
                            ("Copiar para a Esquerda  Ctrl+L", false),
                        ] {
                            if super::chrome::menu_item(ui, icons, label, can_operate).clicked() {
                                self.copy_selected(right, dialogs);
                                ui.close_menu();
                            }
                        }
                        if super::chrome::menu_item(
                            ui,
                            icons,
                            "Renomear...  F2",
                            can_operate && self.selected.len() == 1,
                        )
                        .clicked()
                        {
                            self.rename_selected(dialogs);
                            ui.close_menu();
                        }
                        if super::chrome::menu_item(
                            ui,
                            icons,
                            "Excluir (Lixeira)  Delete",
                            can_operate,
                        )
                        .clicked()
                        {
                            self.delete_selected(dialogs);
                            ui.close_menu();
                        }
                        if super::chrome::menu_item(
                            ui,
                            icons,
                            "Nova pasta...  Insert",
                            self.pending.is_none(),
                        )
                        .clicked()
                        {
                            self.new_folder(dialogs);
                            ui.close_menu();
                        }
                        ui.separator();
                        if super::chrome::menu_item(ui, icons, "Comparar / Atualizar", true)
                            .clicked()
                        {
                            self.start_scan();
                            ui.close_menu();
                        }
                        if super::chrome::menu_item(ui, icons, "Parar", self.pending.is_some())
                            .clicked()
                        {
                            self.cancel.store(true, Ordering::Relaxed);
                            ui.close_menu();
                        }
                        if super::chrome::menu_item(ui, icons, "Expandir tudo", true).clicked() {
                            self.collapsed.clear();
                            ui.close_menu();
                        }
                        if super::chrome::menu_item(ui, icons, "Recolher tudo", true).clicked() {
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
                        if super::chrome::menu_item(ui, icons, "Abrir no Explorer", can_operate)
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
                    super::chrome::menu_button(ui, "Editar", |ui| {
                        if super::chrome::menu_item(ui, icons, "Selecionar tudo  Ctrl+A", true)
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
                        if super::chrome::menu_item(ui, icons, "Inverter seleção", true).clicked()
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
                        if super::chrome::menu_item(
                            ui,
                            icons,
                            "Copiar nomes de arquivos  Ctrl+C",
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
                    super::chrome::menu_button(ui, "Pesquisar", |ui| {
                        if super::chrome::menu_item(ui, icons, "Localizar nome...  Ctrl+F", true)
                            .clicked()
                        {
                            if let Some(query) =
                                dialogs.input("Localizar nome", "Nome ou trecho:", &self.find_text)
                            {
                                self.find_text = query;
                                let query = self.find_text.clone();
                                self.navigate(true, Some(&query));
                            }
                            ui.close_menu();
                        }
                        for (label, forward) in [
                            ("Localizar próximo  F3", true),
                            ("Localizar anterior  Shift+F3", false),
                        ] {
                            if super::chrome::menu_item(ui, icons, label, true).clicked() {
                                let query = self.find_text.clone();
                                self.navigate(forward, Some(&query));
                                ui.close_menu();
                            }
                        }
                        for (label, forward) in [
                            ("Próxima diferença  Ctrl+N", true),
                            ("Diferença anterior  Ctrl+P", false),
                        ] {
                            if super::chrome::menu_item(ui, icons, label, true).clicked() {
                                self.navigate(forward, None);
                                ui.close_menu();
                            }
                        }
                        ui.label(crate::i18n::tr("Filtrar nome"));
                        ui.text_edit_singleline(&mut self.filter);
                    });
                    super::chrome::menu_button(ui, "Visualizar", |ui| {
                        ui.checkbox(&mut self.tree, crate::i18n::tr("Estrutura de pastas"));
                        ui.checkbox(&mut self.show_log, crate::i18n::tr("Log"));
                        if ui
                            .checkbox(&mut self.only_diffs, crate::i18n::tr("Somente diferenças"))
                            .changed()
                            && self.only_diffs
                        {
                            self.only_same = false;
                        }
                        if ui
                            .checkbox(
                                &mut self.compare_content,
                                crate::i18n::tr("Comparar conteúdo"),
                            )
                            .changed()
                            && !self.left_path.is_empty()
                            && !self.right_path.is_empty()
                        {
                            self.start_scan();
                        }
                    });
                    super::chrome::menu_button(ui, "Ferramentas", |ui| {
                        if super::chrome::menu_item(ui, icons, "Opções...", true).clicked() {
                            self.options(dialogs);
                            ui.close_menu();
                        }
                        for label in [
                            "Formatos de Arquivo...",
                            "Perfis...",
                            "Integração de Controle de Origem...",
                        ] {
                            super::chrome::menu_item(ui, icons, label, false)
                                .on_disabled_hover_text(crate::i18n::tr(
                                    "Esta função ainda não está disponível",
                                ));
                        }
                        super::session_menu::settings_tools(ui, icons, dialogs);
                        ui.separator();
                        for label in [
                            "Salvar Instantâneo...",
                            "Editar Arquivo de Texto",
                            "Visualizar Correção...",
                        ] {
                            super::chrome::menu_item(ui, icons, label, false)
                                .on_disabled_hover_text(crate::i18n::tr(
                                    "Esta função ainda não está disponível",
                                ));
                        }
                    });
                    super::chrome::menu_button(ui, "Ajuda (H)", |ui| {
                        ui.label(crate::i18n::tr(
                            "Duplo clique em um par de arquivos para comparar.",
                        ));
                        if super::chrome::menu_item(ui, icons, "Sobre o AnyCompare...", true)
                            .clicked()
                        {
                            dialogs.about();
                            ui.close_menu();
                        }
                    });
                });
            });
        super::session_tabs::show(ui, icons);
        let (only_diffs, only_same, tree, pending) = (
            self.only_diffs,
            self.only_same,
            self.tree,
            self.pending.is_some(),
        );
        let suppress = self.suppress_filters;
        let tool_state = move |name: &str| -> ToolState {
            match name {
                "FavCompareAll" => ToolState::checked(!only_diffs && !only_same),
                "FavDiff" => ToolState::checked(only_diffs),
                "FavSame" => ToolState::checked(only_same),
                "DirCompareStructure" => ToolState::checked(tree),
                "DirFlatten" => ToolState::checked(!tree),
                "SuppressFilters" => ToolState::checked(suppress),
                "CopySide" => ToolState::enabled(can_operate),
                "SessionsCombo" => ToolState::enabled(true),
                "ExpandAll" | "CollapseAll" => ToolState::enabled(tree),
                "Refresh" => ToolState::enabled(!pending),
                "Stop" => ToolState::enabled(pending),
                "SessionHome" | "Rules" | "SelectAll" | "SelectAllFiles" | "Swap"
                | "NameFilter" => ToolState::enabled(true),
                // Not implemented yet: the other display filters, DirShowAll,
                // DirCompareFilesOnly and InsigIgnored stay disabled.
                _ => ToolState::DISABLED,
            }
        };
        let items = super::toolbar_spec::FOLDER_COMPARE;
        let toolbar_height = super::toolbar::height(ui, items, ui.available_width(), &tool_state);
        let mut tool_event = None;
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
        match tool_event {
            Some(ToolEvent::Click("SessionHome")) => action = FolderAction::Home,
            Some(ToolEvent::Click("FavCompareAll")) => {
                self.only_diffs = false;
                self.only_same = false;
            }
            Some(ToolEvent::Click("FavDiff")) => {
                self.only_diffs = true;
                self.only_same = false;
            }
            Some(ToolEvent::Click("FavSame")) => {
                self.only_same = true;
                self.only_diffs = false;
            }
            Some(ToolEvent::Click("DirCompareStructure")) => self.tree = true,
            Some(ToolEvent::Click("DirFlatten")) => self.tree = false,
            Some(ToolEvent::Click("Rules")) => self.options(dialogs),
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
                if let Some(mask) = dialogs.input("Filtros", "Filtros:", &self.filter) {
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
                                "Entre o caminho aqui",
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
                                    "Carregar comparação",
                                    !text.is_empty()
                                        && Path::new(text)
                                            != if index == 0 {
                                                &self.loaded_left
                                            } else {
                                                &self.loaded_right
                                            },
                                ),
                                ("imgBrowse", "Escolher pasta", true),
                                (
                                    "imgUpOneLevel",
                                    "Pasta superior",
                                    Path::new(text).parent().is_some_and(|p| p.is_dir()),
                                ),
                            ];
                            if index == 0 {
                                buttons.push((
                                    "imgUpOneLevelAll",
                                    "Pastas superiores em ambos os lados",
                                    true,
                                ));
                            } else {
                                buttons.push(("imgSpecsBack", "Voltar", self.root_cursor > 0));
                                buttons.push((
                                    "imgSpecsForward",
                                    "Avançar",
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
                    let files: Vec<_> = visible
                        .iter()
                        .filter_map(|&index| {
                            let entry = &self.entries[index];
                            let metadata = if side == 0 { &entry.left } else { &entry.right };
                            metadata.as_ref().filter(|m| m.is_file())
                        })
                        .collect();
                    let bytes: u64 = files.iter().map(|m| m.len()).sum();
                    let text = crate::i18n::tr(&format!(
                        "{} arquivo(s), {}",
                        files.len(),
                        volume_label(bytes)
                    ));
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
                            crate::i18n::tr(&format!("{number} GB livres em {drive}")),
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
                ui.allocate_space(rect.size());
            });
        if self.show_log {
            TopBottomPanel::bottom("folder_log")
                .exact_height(100.0)
                .frame(
                    Frame::none()
                        .fill(Color32::from_rgb(227, 231, 237))
                        .inner_margin(Margin {
                            left: 24.0,
                            right: 2.0,
                            top: 2.0,
                            bottom: 2.0,
                        }),
                )
                .show_inside(ui, |ui| {
                    let panel = ui.max_rect();
                    ui.spacing_mut().item_spacing.y = 0.0;
                    let menu_rect =
                        Rect::from_min_size(panel.left_top() - vec2(23.0, 0.0), vec2(21.0, 20.0));
                    let mut buttons = ui.new_child(
                        UiBuilder::new()
                            .max_rect(Rect::from_min_size(menu_rect.min, vec2(21.0, 44.0))),
                    );
                    if let Some(icon) = icons.get_icon(ui.ctx(), "imgOptions") {
                        buttons.menu_image_button(
                            Image::new((icon.id(), vec2(16.0, 16.0))),
                            |ui| {
                                if ui.button(crate::i18n::tr("Ocultar log")).clicked() {
                                    self.show_log = false;
                                    ui.close_menu();
                                }
                            },
                        );
                    }
                    if let Some(icon) = icons.get_icon(ui.ctx(), "imgSave") {
                        if buttons
                            .add(ImageButton::new((icon.id(), vec2(16.0, 16.0))).frame(false))
                            .on_hover_text(crate::i18n::tr("Salvar log"))
                            .clicked()
                        {
                            if let Some(path) = dialogs.report_path("txt") {
                                if let Err(error) = fs::write(path, self.log.join("\r\n")) {
                                    dialogs.error(&error.to_string());
                                }
                            }
                        }
                    }
                    ScrollArea::vertical().stick_to_bottom(true).show(ui, |ui| {
                        for line in &self.log {
                            let (rect, _) = ui.allocate_exact_size(
                                vec2(ui.available_width(), 18.0),
                                Sense::hover(),
                            );
                            ui.painter().rect_stroke(
                                rect,
                                0.0,
                                Stroke::new(1.0_f32, Color32::from_gray(190)),
                            );
                            ui.painter().with_clip_rect(rect).text(
                                rect.left_center() + vec2(3.0, 0.0),
                                Align2::LEFT_CENTER,
                                crate::i18n::tr(line),
                                FontId::proportional(12.0),
                                Color32::BLACK,
                            );
                        }
                    });
                });
        }
        CentralPanel::default()
            .frame(
                Frame::none()
                    .fill(BcTheme::PANEL_BG)
                    .stroke(Stroke::new(1.0_f32, Color32::from_gray(160))),
            )
            .show_inside(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                let visible: Vec<&Entry> = self
                    .visible_indices()
                    .into_iter()
                    .map(|index| &self.entries[index])
                    .collect();
                ui.spacing_mut().scroll.bar_width = 17.0;
                ui.spacing_mut().scroll.bar_inner_margin = 0.0;
                let table_width =
                    (ui.available_width() - ui.spacing().scroll.allocated_width()).max(650.0);
                let panes = |rect: Rect| {
                    [
                        Rect::from_min_max(
                            rect.min,
                            pos2(rect.left() + table_width / 2.0 - 16.0, rect.bottom()),
                        ),
                        Rect::from_min_max(
                            pos2(rect.left() + table_width / 2.0 + 16.0, rect.top()),
                            rect.max,
                        ),
                    ]
                };
                let (header, _) = ui.allocate_exact_size(vec2(table_width, 22.0), Sense::hover());
                let border = Stroke::new(1.0_f32, Color32::from_gray(200));
                for pane in panes(header) {
                    let columns = FolderColumns::new(pane);
                    for (cell, label) in [
                        (columns.name, "Nome"),
                        (columns.size, "Tamanho"),
                        (columns.modified, "Modificado"),
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
                    // The default tree ordering is ascending by name.
                    let c = columns.name.center_top() + vec2(0.0, 3.0);
                    ui.painter().line_segment(
                        [c + vec2(-3.0, 2.0), c],
                        Stroke::new(1.0_f32, Color32::GRAY),
                    );
                    ui.painter()
                        .line_segment([c, c + vec2(3.0, 2.0)], Stroke::new(1.0_f32, Color32::GRAY));
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
                scroll.show_rows(ui, ROW_HEIGHT, visible.len(), |ui, range| {
                    for index in range {
                        let entry = visible[index];
                        let color = if self.selected.contains(&entry.id) {
                            if self.selection_focus {
                                Color32::from_rgb(168, 205, 241)
                            } else {
                                Color32::from_rgb(212, 226, 238)
                            }
                        } else if index % 2 == 0 {
                            Color32::WHITE
                        } else {
                            Color32::from_rgb(248, 250, 249)
                        };
                        let (rect, response) =
                            ui.allocate_exact_size(vec2(table_width, ROW_HEIGHT), Sense::click());
                        for (side, pane) in panes(rect).into_iter().enumerate() {
                            let columns = FolderColumns::new(pane);
                            let painter =
                                ui.painter().with_clip_rect(pane.intersect(ui.clip_rect()));
                            painter.rect_filled(pane, 0.0, color);
                            if self.selected.contains(&entry.id) {
                                let outline = pane.shrink(0.5);
                                painter.extend(Shape::dotted_line(
                                    &[
                                        outline.left_top(),
                                        outline.right_top(),
                                        outline.right_bottom(),
                                        outline.left_bottom(),
                                        outline.left_top(),
                                    ],
                                    Color32::from_gray(80),
                                    2.0,
                                    0.5,
                                ));
                            }
                            let metadata = if side == 0 { &entry.left } else { &entry.right };
                            if let Some(metadata) = metadata {
                                let relative = if side == 0 {
                                    &entry.relative
                                } else {
                                    &entry.right_relative
                                };
                                let depth = if self.tree {
                                    relative.components().count().saturating_sub(1) as f32 * 16.0
                                } else {
                                    0.0
                                };
                                let icon_x = pane.left() + depth + 2.0;
                                let text_color = entry_color(entry, side == 0);
                                if metadata.is_dir() {
                                    let icon_color = if matches!(
                                        entry.status,
                                        Status::LeftOnly | Status::RightOnly
                                    ) {
                                        ORPHAN_COLOR
                                    } else {
                                        Color32::from_rgb(192, 160, 0)
                                    };
                                    let opened =
                                        !self.collapsed.contains(&path_key(&entry.relative));
                                    for name in if opened {
                                        ["imgFolderTop1", "imgFolderBottom1"]
                                    } else {
                                        ["imgFolderTop0", "imgFolderBottom0"]
                                    } {
                                        if let Some(icon) =
                                            icons.folder_model(ui.ctx(), name, icon_color)
                                        {
                                            painter.image(
                                                icon.id(),
                                                Rect::from_min_size(
                                                    pos2(icon_x, pane.top()),
                                                    vec2(16.0, 16.0),
                                                ),
                                                Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)),
                                                Color32::WHITE,
                                            );
                                        }
                                    }
                                } else if relative
                                    .extension()
                                    .is_some_and(|ext| ext.eq_ignore_ascii_case("chm"))
                                {
                                    for name in
                                        ["imgFolderTop0", "imgFolderBottom0", "imgFolderZip"]
                                    {
                                        if let Some(icon) =
                                            icons.folder_model(ui.ctx(), name, Color32::GRAY)
                                        {
                                            painter.image(
                                                icon.id(),
                                                Rect::from_min_size(
                                                    pos2(icon_x, pane.top()),
                                                    vec2(16.0, 16.0),
                                                ),
                                                Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)),
                                                Color32::WHITE,
                                            );
                                        }
                                    }
                                } else {
                                    // Native file indicators are 5 x 6 pixels, colored per side.
                                    painter.rect_filled(
                                        Rect::from_min_size(
                                            pos2(icon_x + 3.0, pane.top() + 5.0),
                                            vec2(5.0, 6.0),
                                        ),
                                        0.0,
                                        text_color,
                                    );
                                }
                                let name = if self.tree {
                                    relative.file_name().map(Path::new).unwrap_or(relative)
                                } else {
                                    relative
                                };
                                painter
                                    .with_clip_rect(columns.name.intersect(ui.clip_rect()))
                                    .text(
                                        pos2(icon_x + 18.0, pane.center().y),
                                        Align2::LEFT_CENTER,
                                        name.display().to_string(),
                                        FontId::proportional(12.0),
                                        text_color,
                                    );
                                if metadata.is_file() {
                                    painter
                                        .with_clip_rect(columns.size.intersect(ui.clip_rect()))
                                        .text(
                                            columns.size.right_center() - vec2(4.0, 0.0),
                                            Align2::RIGHT_CENTER,
                                            grouped_size(metadata.len()),
                                            FontId::proportional(12.0),
                                            text_color,
                                        );
                                }
                                let date = metadata
                                    .modified()
                                    .ok()
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
                        if response.clicked() {
                            self.selection_focus = true;
                            self.active_left = response
                                .interact_pointer_pos()
                                .is_none_or(|pos| pos.x < rect.center().x);
                            if !ui.input(|i| i.modifiers.ctrl) {
                                self.selected.clear();
                            }
                            if !self.selected.insert(entry.id.clone()) {
                                self.selected.remove(&entry.id);
                            }
                        }
                        if let Some(error) = &entry.error {
                            response.clone().on_hover_text(crate::i18n::tr(error));
                        }
                        let icon_clicked = response.clicked()
                            && response.interact_pointer_pos().is_some_and(|pos| {
                                panes(rect).into_iter().any(|pane| {
                                    pane.contains(pos)
                                        && pos.x
                                            < pane.left()
                                                + entry.relative.components().count() as f32 * 16.0
                                                + 3.0
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
                            action = FolderAction::OpenFiles(
                                self.loaded_left.join(&entry.relative),
                                self.loaded_right.join(&entry.right_relative),
                            );
                        }
                    }
                });
                // Keep the pane separator visible below the final row as in the native list.
                let x = ui.max_rect().left() + table_width / 2.0;
                ui.painter().line_segment(
                    [pos2(x, table_top), pos2(x, ui.max_rect().bottom())],
                    Stroke::new(1.0_f32, Color32::from_gray(160)),
                );
            });
        action
    }
}

impl Drop for FolderCompareScreen {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

fn snapshot(
    root: &Path,
    cancel: &AtomicBool,
    errors: &mut Vec<String>,
) -> BTreeMap<PathBuf, fs::Metadata> {
    let mut entries = BTreeMap::new();
    for result in WalkDir::new(root).follow_links(false).min_depth(1) {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        match result {
            Ok(entry) => match entry.metadata() {
                Ok(metadata) => {
                    if let Ok(relative) = entry.path().strip_prefix(root) {
                        entries.insert(relative.to_owned(), metadata);
                    }
                }
                Err(error) => errors.push(error.to_string()),
            },
            Err(error) => errors.push(error.to_string()),
        }
    }
    entries
}

fn same_content(left: &Path, right: &Path, cancel: &AtomicBool) -> std::io::Result<bool> {
    let mut left = BufReader::new(fs::File::open(left)?);
    let mut right = BufReader::new(fs::File::open(right)?);
    let mut a = [0_u8; 64 * 1024];
    let mut b = [0_u8; 64 * 1024];
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Interrupted,
                "Comparação cancelada",
            ));
        }
        let n = read_block(&mut left, &mut a)?;
        if read_block(&mut right, &mut b)? != n || a[..n] != b[..n] {
            return Ok(false);
        }
        if n == 0 {
            return Ok(true);
        }
    }
}

fn read_block(reader: &mut impl Read, buffer: &mut [u8]) -> std::io::Result<usize> {
    let mut filled = 0;
    while filled < buffer.len() {
        match reader.read(&mut buffer[filled..]) {
            Ok(0) => break,
            Ok(count) => filled += count,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        }
    }
    Ok(filled)
}

fn scan_folders(left: &Path, right: &Path, content: bool, cancel: &AtomicBool) -> Scan {
    let mut errors = Vec::new();
    let left_entries = snapshot(left, cancel, &mut errors);
    let right_snapshot = snapshot(right, cancel, &mut errors);
    let mut right_entries: BTreeMap<(String, bool), Vec<(PathBuf, fs::Metadata)>> = BTreeMap::new();
    for (path, metadata) in right_snapshot {
        right_entries
            .entry((path_key(&path), metadata.is_dir()))
            .or_default()
            .push((path, metadata));
    }
    let mut entries = Vec::new();
    for (relative, l) in left_entries {
        if cancel.load(Ordering::Relaxed) {
            errors.push("Comparação cancelada; resultados parciais.".into());
            break;
        }
        let right_entry = right_entries
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
        let right_relative = right_entry
            .as_ref()
            .map(|(path, _)| path.clone())
            .unwrap_or_else(|| relative.clone());
        let r = right_entry.map(|(_, metadata)| metadata);
        let mut error = None;
        let status = match &r {
            None => Status::LeftOnly,
            Some(r) if l.is_dir() && r.is_dir() => Status::Same,
            Some(r) if l.is_file() && r.is_file() => {
                if l.len() != r.len() {
                    Status::Different
                } else if content {
                    match same_content(&left.join(&relative), &right.join(&right_relative), cancel)
                    {
                        Ok(true) => Status::Same,
                        Ok(false) => Status::Different,
                        Err(e) => {
                            let message = format!("{}: {e}", relative.display());
                            errors.push(message.clone());
                            error = Some(message);
                            Status::Error
                        }
                    }
                } else if l.modified().unwrap_or(SystemTime::UNIX_EPOCH)
                    == r.modified().unwrap_or(SystemTime::UNIX_EPOCH)
                {
                    Status::Same
                } else {
                    Status::Different
                }
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
            error,
        });
    }
    if !cancel.load(Ordering::Relaxed) {
        entries.extend(
            right_entries
                .into_values()
                .flatten()
                .map(|(relative, right)| Entry {
                    id: EntryKey {
                        relative: relative.clone(),
                        directory: right.is_dir(),
                    },
                    right_relative: relative.clone(),
                    relative,
                    left: None,
                    right: Some(right),
                    status: Status::RightOnly,
                    error: None,
                }),
        );
    }
    entries.sort_by(tree_order);
    let mut changed_parents = BTreeSet::new();
    for entry in entries.iter().filter(|entry| entry.status != Status::Same) {
        for parent in entry
            .relative
            .ancestors()
            .skip(1)
            .filter(|p| !p.as_os_str().is_empty())
        {
            changed_parents.insert(path_key(parent));
        }
    }
    for entry in &mut entries {
        if entry.status == Status::Same
            && entry.left.as_ref().is_some_and(|m| m.is_dir())
            && changed_parents.contains(&path_key(&entry.relative))
        {
            entry.status = Status::Different;
        }
    }
    Scan { entries, errors }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[cfg(windows)]
    fn native_folder_reports_match_pairs_status_and_tree_order_in_both_directions() {
        let corpus = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("docs/reference/controlled-folder-replay-2026-10-30");
        let cases: serde_json::Value =
            serde_json::from_slice(&fs::read(corpus.join("expected.json")).unwrap()).unwrap();
        let mut mismatches = Vec::new();
        for case in cases.as_array().unwrap() {
            let name = case["name"].as_str().unwrap();
            let case_root = corpus.join(name.trim_end_matches("-reverse"));
            let (left, right) = if name.ends_with("-reverse") {
                ("right", "left")
            } else {
                ("left", "right")
            };
            let scan = scan_folders(
                &case_root.join(left),
                &case_root.join(right),
                true,
                &AtomicBool::new(false),
            );
            assert!(
                scan.errors.is_empty(),
                "{}: {:?}",
                case["name"],
                scan.errors
            );
            let actual: Vec<_> = scan.entries.iter().map(|entry| serde_json::json!({
                "kind": if is_dir(entry) { "directory" } else { "file" },
                "left": entry.left.as_ref().map(|_| entry.relative.to_string_lossy().replace('\\', "/")),
                "right": entry.right.as_ref().map(|_| entry.right_relative.to_string_lossy().replace('\\', "/")),
                "status": match entry.status { Status::Same=>"same",Status::Different=>"different",Status::LeftOnly=>"left-only",Status::RightOnly=>"right-only",Status::Error=>"error" }
            })).collect();
            if serde_json::json!(actual) != case["entries"] {
                mismatches.push(serde_json::json!({"case":case["name"],"actual":actual,"expected":case["entries"]}));
            }
            let mut screen = FolderCompareScreen::new();
            screen.entries = scan.entries;
            for mode in ["matches", "differences"] {
                screen.only_same = mode == "matches";
                screen.only_diffs = mode == "differences";
                let project = |row: &serde_json::Value| {
                    serde_json::json!({
                        "kind": row["kind"], "left": row["left"], "right": row["right"]
                    })
                };
                let visible: Vec<_> = screen
                    .visible_indices()
                    .into_iter()
                    .map(|index| project(&actual[index]))
                    .collect();
                let expected: Vec<_> = case["filtered"][mode]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(project)
                    .collect();
                if visible != expected {
                    mismatches.push(serde_json::json!({"case":case["name"],"mode":mode,"actual":visible,"expected":expected}));
                }
            }
        }
        assert!(
            mismatches.is_empty(),
            "Native folder divergences: {}",
            serde_json::to_string_pretty(&mismatches).unwrap()
        );
    }
    #[test]
    fn new_folder_pair_starts_collapsed_and_refresh_keeps_expansion() {
        let corpus = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("docs/reference/controlled-folder-replay-2026-10-30");
        let mut screen = FolderCompareScreen::new();
        let nested = corpus.join("nested");
        screen.load_folders(&nested.join("left"), &nested.join("right"));
        screen.comparison_results();
        assert!(screen
            .visible_indices()
            .iter()
            .all(|&index| screen.entries[index].relative.components().count() == 1));
        assert_eq!(screen.selected.len(), 1);
        screen.collapsed.clear();
        screen.start_scan();
        screen.comparison_results();
        assert_eq!(screen.visible_indices().len(), screen.entries.len());
        let next = corpus.join("case-parent");
        screen.load_folders(&next.join("left"), &next.join("right"));
        screen.comparison_results();
        assert_eq!(screen.visible_indices().len(), 1);
        assert_eq!(screen.root_history.len(), 2);
    }

    #[test]
    fn type_conflict_rows_have_independent_selection_and_cannot_overwrite_a_directory() {
        let corpus = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("docs/reference/controlled-folder-replay-2026-10-30/file-directory");
        let mut screen = FolderCompareScreen::new();
        screen.load_folders(&corpus.join("left"), &corpus.join("right"));
        screen.comparison_results();
        let file = screen
            .entries
            .iter()
            .find(|entry| entry.relative == Path::new("conflict") && !is_dir(entry))
            .unwrap()
            .id
            .clone();
        let folder = screen
            .entries
            .iter()
            .find(|entry| entry.relative == Path::new("conflict") && is_dir(entry))
            .unwrap()
            .id
            .clone();
        assert_ne!(file, folder);
        screen.selected.clear();
        screen.selected.insert(file);
        assert_eq!(
            screen.paths(true).unwrap(),
            vec![corpus.join("left/conflict").canonicalize().unwrap()]
        );
        assert!(screen.paths(false).unwrap().is_empty());
        let mut dialogs = super::super::dialogs::TestDialogs::default();
        screen.copy_selected(true, &mut dialogs);
        assert_eq!(dialogs.errors.len(), 1);
        assert!(corpus.join("right/conflict").is_dir());
        assert_eq!(fs::read(corpus.join("left/conflict")).unwrap(), b"file");
        screen.selected.insert(folder);
        assert_eq!(
            screen.paths(false).unwrap(),
            vec![corpus.join("right/conflict").canonicalize().unwrap()]
        );
    }
    #[test]
    fn folded_folder_context_and_right_hand_names_preserve_the_original_spelling() {
        let corpus = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("docs/reference/controlled-folder-replay-2026-10-30/case-parent");
        let mut screen = FolderCompareScreen::new();
        screen.load_folders(&corpus.join("left"), &corpus.join("right"));
        screen.comparison_results();
        screen.collapsed.insert(path_key(Path::new("Data")));
        assert_eq!(screen.visible_indices().len(), 1);
        screen.collapsed.clear();
        let ctx = Context::default();
        super::super::theme::apply(&ctx);
        let mut icons = IconManager::new();
        let mut dialogs = super::super::dialogs::TestDialogs::default();
        let output = ctx.run(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1150.0, 720.0))),
                ..Default::default()
            },
            |ctx| {
                CentralPanel::default().show(ctx, |ui| {
                    screen.ui(ctx, ui, &mut icons, &mut dialogs);
                });
            },
        );
        let captions: Vec<_> = output
            .shapes
            .iter()
            .filter_map(|shape| {
                if let Shape::Text(text) = &shape.shape {
                    Some(text.galley.text().to_owned())
                } else {
                    None
                }
            })
            .collect();
        for caption in ["Data", "data", "equal.txt", "EQUAL.TXT"] {
            assert!(
                captions.iter().any(|actual| actual == caption),
                "missing {caption}: {captions:?}"
            );
        }
    }
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
            ["Escolha duas pastas existentes para comparar."]
        );
    }
    #[test]
    fn block_reader_accepts_short_reads_before_eof() {
        struct ShortReader(std::io::Cursor<Vec<u8>>);
        impl Read for ShortReader {
            fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
                let count = buffer.len().min(3);
                self.0.read(&mut buffer[..count])
            }
        }
        let mut reader = ShortReader(std::io::Cursor::new(b"0123456789abcdef".to_vec()));
        let mut buffer = [0; 10];
        assert_eq!(read_block(&mut reader, &mut buffer).unwrap(), 10);
        assert_eq!(&buffer, b"0123456789");
        assert_eq!(read_block(&mut reader, &mut buffer).unwrap(), 6);
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
}
