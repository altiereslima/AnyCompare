#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod binary_diff;
mod diff;
mod doc_convert;
mod document;
mod file_ops;
mod folder_merge;
mod folder_source;
mod folder_sync;
mod history;
mod html_tables;
mod i18n;
mod merge3;
mod pe_version;
#[cfg(test)]
mod perf_bench;
mod picture;
mod property_sources;
mod report;
mod resources;
mod search;
mod table;
mod table_columns;
mod table_formats;
mod table_sheets;
mod table_val;
mod ui;
mod workspace;

use eframe::egui;
use history::{History, RecentSession, SessionKind};
use std::path::PathBuf;
use ui::{
    folder_compare::{FolderAction, FolderCompareScreen},
    folder_merge::{FolderMergeScreen, FolderMergeUiAction},
    folder_sync::{FolderSyncAction, FolderSyncScreen},
    hex_compare::HexCompareScreen,
    home::{HomeAction, HomeScreen},
    icons::IconManager,
    picture_compare::PictureCompareScreen,
    table_compare::TableCompareScreen,
    text_compare::{TextCompareAction, TextCompareScreen},
    text_edit::TextEditScreen,
    text_merge::{TextMergeAction, TextMergeScreen},
    version_compare::VersionCompareScreen,
};

enum Session {
    Text(Box<TextCompareScreen>),
    Folder(Box<FolderCompareScreen>),
    FolderSync(Box<FolderSyncScreen>),
    TextMerge(Box<TextMergeScreen>),
    FolderMerge(Box<FolderMergeScreen>),
    Hex(Box<HexCompareScreen>),
    Table(Box<TableCompareScreen>),
    Version(Box<VersionCompareScreen>),
    Properties(Box<VersionCompareScreen>),
    Picture(Box<PictureCompareScreen>),
    TextEdit(Box<TextEditScreen>),
}

impl Session {
    fn saved(&self) -> Option<workspace::SavedSession> {
        Some(workspace::SavedSession {
            files: self.record()?,
            options: match self {
                Self::Text(s) => s.session_options(),
                Self::Folder(s) => s.session_options(),
                Self::FolderSync(s) => s.saved_options(),
                Self::TextMerge(s) => s.saved_options(),
                Self::FolderMerge(s) => s.saved_options(),
                Self::Hex(s) => s.session_options(),
                Self::Table(s) => s.session_options(),
                Self::Version(s) | Self::Properties(s) => s.session_options(),
                Self::Picture(s) => s.session_options(),
                Self::TextEdit(s) => s.session_options(),
            },
        })
    }
    fn from_saved(saved: &workspace::SavedSession) -> Result<Self, String> {
        let files = &saved.files;
        let mut session = Self::new(files.kind);
        match &mut session {
            Self::TextEdit(screen) => {
                screen.load(&files.left)?;
                screen.apply_session_options(&saved.options);
            }
            Self::Version(screen) | Self::Properties(screen) => {
                screen.load_files(&files.left, &files.right)?;
                screen.apply_session_options(&saved.options);
            }
            Self::Picture(screen) => {
                screen.load_files(&files.left, &files.right)?;
                screen.apply_session_options(&saved.options);
            }
            Self::Table(screen) => {
                screen.apply_session_options(&saved.options);
                screen.load_files(&files.left, &files.right)?;
            }
            Self::Text(screen) => {
                screen.left = document::Document::load(&files.left)?;
                screen.right = document::Document::load(&files.right)?;
                screen.left_path = files.left.to_string_lossy().into_owned();
                screen.right_path = files.right.to_string_lossy().into_owned();
                screen.apply_session_options(&saved.options);
            }
            Self::Folder(screen) => {
                screen.load_folders(&files.left, &files.right);
                screen.apply_session_options(&saved.options);
            }
            Self::FolderSync(screen) => {
                screen.apply_saved_options(&saved.options);
                screen.load_folders(&files.left, &files.right);
            }
            Self::TextMerge(screen) => {
                screen.apply_saved_options(&saved.options);
                screen.load_files_silent(
                    &files.left,
                    files.center.as_deref(),
                    &files.right,
                    files.output.as_deref(),
                )?;
            }
            Self::FolderMerge(screen) => {
                screen.apply_saved_options(&saved.options);
                screen.load_folders(
                    &files.left,
                    files.center.as_deref(),
                    &files.right,
                    files.output.as_deref(),
                );
            }
            Self::Hex(screen) => {
                screen.load_files(&files.left, &files.right);
                if let Some(error) = screen.load_error() {
                    return Err(error.to_owned());
                }
                screen.apply_session_options(&saved.options);
            }
        }
        Ok(session)
    }
    fn confirm_saved(&mut self, dialogs: &mut dyn ui::dialogs::DialogHost) -> bool {
        match self {
            Self::Text(screen) => screen.confirm_saved(dialogs),
            Self::TextMerge(screen) => screen.confirm_saved(dialogs),
            Self::Hex(screen) => screen.confirm_saved(dialogs),
            Self::Table(screen) => screen.confirm_saved(dialogs),
            Self::TextEdit(screen) => screen.confirm_saved(dialogs),
            _ => true,
        }
    }
    fn new(kind: SessionKind) -> Self {
        match kind {
            SessionKind::Text => Self::Text(Box::new(TextCompareScreen::new())),
            SessionKind::Folder => Self::Folder(Box::new(FolderCompareScreen::new())),
            SessionKind::FolderSync => Self::FolderSync(Box::default()),
            SessionKind::TextMerge => Self::TextMerge(Box::default()),
            SessionKind::FolderMerge => Self::FolderMerge(Box::default()),
            SessionKind::Hex => Self::Hex(Box::new(HexCompareScreen::new())),
            SessionKind::Table => Self::Table(Box::new(TableCompareScreen::new())),
            SessionKind::TextEdit => Self::TextEdit(Box::new(TextEditScreen::new())),
            SessionKind::Version => Self::Version(Box::new(VersionCompareScreen::new())),
            SessionKind::Picture => Self::Picture(Box::new(PictureCompareScreen::new())),
            SessionKind::Registry | SessionKind::Media => {
                Self::Properties(Box::new(VersionCompareScreen::for_kind(kind)))
            }
        }
    }

    fn dirty(&self) -> bool {
        match self {
            Self::Text(s) => s.left.dirty() || s.right.dirty(),
            Self::TextMerge(s) => s.dirty,
            Self::Hex(s) => s.dirty(),
            Self::Table(s) => s.dirty(),
            Self::TextEdit(s) => s.document.dirty(),
            Self::Folder(_)
            | Self::FolderSync(_)
            | Self::FolderMerge(_)
            | Self::Version(_)
            | Self::Properties(_)
            | Self::Picture(_) => false,
        }
    }

    fn side_state(&self) -> Option<(bool, bool, bool)> {
        match self {
            Self::Text(s) => Some((
                s.is_side_empty(true),
                s.is_side_empty(false),
                s.active_left(),
            )),
            Self::Folder(s) => Some((s.left_path.is_empty(), s.right_path.is_empty(), true)),
            Self::FolderSync(s) => Some((
                s.left_path.is_empty(),
                s.right_path.is_empty(),
                s.active_left,
            )),
            Self::TextMerge(s) => Some((s.left_path.is_empty(), s.right_path.is_empty(), true)),
            Self::FolderMerge(s) => Some((s.left_path.is_empty(), s.right_path.is_empty(), true)),
            Self::Hex(s) => Some((
                s.is_side_empty(true),
                s.is_side_empty(false),
                s.active_left(),
            )),
            Self::Table(s) => Some((
                s.is_side_empty(true),
                s.is_side_empty(false),
                s.active_left(),
            )),
            Self::Version(s) | Self::Properties(s) => Some((
                s.is_side_empty(true),
                s.is_side_empty(false),
                s.active_left(),
            )),
            Self::Picture(s) => Some((
                s.is_side_empty(true),
                s.is_side_empty(false),
                s.active_left(),
            )),
            Self::TextEdit(_) => None,
        }
    }

    fn kind(&self) -> SessionKind {
        match self {
            Self::Text(_) => SessionKind::Text,
            Self::Folder(_) => SessionKind::Folder,
            Self::FolderSync(_) => SessionKind::FolderSync,
            Self::TextMerge(_) => SessionKind::TextMerge,
            Self::FolderMerge(_) => SessionKind::FolderMerge,
            Self::Hex(_) => SessionKind::Hex,
            Self::Table(_) => SessionKind::Table,
            Self::Version(_) => SessionKind::Version,
            Self::Properties(s) => s.kind(),
            Self::Picture(_) => SessionKind::Picture,
            Self::TextEdit(_) => SessionKind::TextEdit,
        }
    }

    fn is_empty(&self, kind: SessionKind) -> bool {
        match (self, kind) {
            (Self::TextEdit(s), SessionKind::TextEdit) => {
                !s.document.dirty() && s.document.path.is_none() && s.document.text.is_empty()
            }
            (Self::Text(s), SessionKind::Text) => {
                !self.dirty()
                    && s.left.path.is_none()
                    && s.right.path.is_none()
                    && s.left.text.is_empty()
                    && s.right.text.is_empty()
                    && s.left_path.is_empty()
                    && s.right_path.is_empty()
            }
            (Self::Folder(s), SessionKind::Folder) => {
                s.left_path.is_empty() && s.right_path.is_empty()
            }
            (Self::FolderSync(s), SessionKind::FolderSync) => {
                s.left_path.is_empty() && s.right_path.is_empty()
            }
            (Self::TextMerge(s), SessionKind::TextMerge) => {
                !s.dirty && s.left_path.is_empty() && s.right_path.is_empty()
            }
            (Self::FolderMerge(s), SessionKind::FolderMerge) => {
                s.left_path.is_empty() && s.right_path.is_empty()
            }
            (Self::Table(s), SessionKind::Table) => s.is_empty(),
            (Self::Properties(s), kind) if s.kind() == kind => {
                s.left_path.is_empty() && s.right_path.is_empty()
            }
            (Self::Version(s), SessionKind::Version) => {
                s.left_path.is_empty() && s.right_path.is_empty()
            }
            (Self::Picture(s), SessionKind::Picture) => s.is_empty(),
            (Self::Hex(s), SessionKind::Hex) => s.left_path.is_empty() && s.right_path.is_empty(),
            _ => false,
        }
    }

    fn record(&self) -> Option<RecentSession> {
        match self {
            Self::TextMerge(s) if s.has_pair() => {
                return Some(RecentSession::new_merge(
                    SessionKind::TextMerge,
                    PathBuf::from(s.left_path.trim()),
                    (!s.center_path.trim().is_empty()).then(|| PathBuf::from(s.center_path.trim())),
                    PathBuf::from(s.right_path.trim()),
                    (!s.output_path.trim().is_empty()).then(|| PathBuf::from(s.output_path.trim())),
                ));
            }
            Self::FolderMerge(s) if s.has_pair() => {
                return Some(RecentSession::new_merge(
                    SessionKind::FolderMerge,
                    PathBuf::from(s.left_path.trim()),
                    (!s.center_path.trim().is_empty()).then(|| PathBuf::from(s.center_path.trim())),
                    PathBuf::from(s.right_path.trim()),
                    (!s.output_path.trim().is_empty()).then(|| PathBuf::from(s.output_path.trim())),
                ));
            }
            _ => {}
        }
        let (kind, left, right) = match self {
            Self::TextEdit(s) => (
                SessionKind::TextEdit,
                s.document.path.clone()?,
                s.document.path.clone()?,
            ),
            Self::Text(s) => (
                SessionKind::Text,
                s.left.path.clone()?,
                s.right.path.clone()?,
            ),
            Self::Version(s) | Self::Properties(s) => {
                let (left, right) = s.record_paths()?;
                (s.kind(), left, right)
            }
            Self::Picture(s) => {
                let (left, right) = s.record_paths()?;
                (SessionKind::Picture, left, right)
            }
            Self::Table(s) if !s.left.path.is_empty() && !s.right.path.is_empty() => (
                SessionKind::Table,
                PathBuf::from(&s.left.path),
                PathBuf::from(&s.right.path),
            ),
            Self::Folder(s) if !s.left_path.is_empty() && !s.right_path.is_empty() => (
                SessionKind::Folder,
                PathBuf::from(&s.left_path),
                PathBuf::from(&s.right_path),
            ),
            Self::FolderSync(s) if s.has_pair() => (
                SessionKind::FolderSync,
                PathBuf::from(s.left_path.trim()),
                PathBuf::from(s.right_path.trim()),
            ),
            Self::Hex(s) if !s.left_path.is_empty() && !s.right_path.is_empty() => (
                SessionKind::Hex,
                PathBuf::from(&s.left_path),
                PathBuf::from(&s.right_path),
            ),
            _ => return None,
        };
        Some(RecentSession::new(kind, left, right))
    }

    fn single_side_name(&self) -> Option<String> {
        let path_str = match self {
            Self::Text(s) => {
                if !s.left_path.is_empty() {
                    Some(s.left_path.as_str())
                } else if !s.right_path.is_empty() {
                    Some(s.right_path.as_str())
                } else {
                    None
                }
            }
            Self::Folder(s) => {
                if !s.left_path.is_empty() {
                    Some(s.left_path.as_str())
                } else if !s.right_path.is_empty() {
                    Some(s.right_path.as_str())
                } else {
                    None
                }
            }
            Self::FolderSync(s) => {
                if !s.left_path.is_empty() {
                    Some(s.left_path.as_str())
                } else if !s.right_path.is_empty() {
                    Some(s.right_path.as_str())
                } else {
                    None
                }
            }
            Self::TextMerge(s) => {
                if !s.left_path.is_empty() {
                    Some(s.left_path.as_str())
                } else if !s.right_path.is_empty() {
                    Some(s.right_path.as_str())
                } else {
                    None
                }
            }
            Self::FolderMerge(s) => {
                if !s.left_path.is_empty() {
                    Some(s.left_path.as_str())
                } else if !s.right_path.is_empty() {
                    Some(s.right_path.as_str())
                } else {
                    None
                }
            }
            Self::Hex(s) => {
                if !s.left_path.is_empty() {
                    Some(s.left_path.as_str())
                } else if !s.right_path.is_empty() {
                    Some(s.right_path.as_str())
                } else {
                    None
                }
            }
            Self::Table(s) => {
                if !s.left.path.is_empty() {
                    Some(s.left.path.as_str())
                } else if !s.right.path.is_empty() {
                    Some(s.right.path.as_str())
                } else {
                    None
                }
            }
            Self::Picture(s) => {
                if !s.left_path.is_empty() {
                    Some(s.left_path.as_str())
                } else if !s.right_path.is_empty() {
                    Some(s.right_path.as_str())
                } else {
                    None
                }
            }
            Self::Version(s) | Self::Properties(s) => {
                if !s.left_path.is_empty() {
                    Some(s.left_path.as_str())
                } else if !s.right_path.is_empty() {
                    Some(s.right_path.as_str())
                } else {
                    None
                }
            }
            Self::TextEdit(_) => None,
        }?;
        let p = std::path::Path::new(path_str);
        Some(
            p.file_name()
                .unwrap_or(p.as_os_str())
                .to_string_lossy()
                .into_owned(),
        )
    }

    fn title(&self) -> String {
        let kind_label = match self {
            Self::Text(_) => crate::i18n::tr("session.text_compare"),
            Self::Folder(_) => crate::i18n::tr("session.folder_compare"),
            Self::FolderSync(_) => crate::i18n::tr("session.folder_sync"),
            Self::TextMerge(_) => crate::i18n::tr("session.text_merge"),
            Self::FolderMerge(_) => crate::i18n::tr("session.folder_merge"),
            Self::Hex(_) => crate::i18n::tr("session.hex_compare"),
            Self::Table(_) => crate::i18n::tr("session.table_compare"),
            Self::TextEdit(_) => crate::i18n::tr("session.text_edit"),
            Self::Version(_) => crate::i18n::tr("session.version_compare"),
            Self::Picture(_) => crate::i18n::tr("session.picture_compare"),
            Self::Properties(s) => crate::i18n::tr(if s.kind() == SessionKind::Media {
                "session.media_compare"
            } else {
                "session.registry_compare"
            }),
        };
        let title = self
            .record()
            .map(|r| {
                if r.kind != SessionKind::Folder && r.left.file_name() == r.right.file_name() {
                    r.left
                        .file_name()
                        .unwrap_or(r.left.as_os_str())
                        .to_string_lossy()
                        .into_owned()
                } else {
                    r.label()
                }
            })
            .or_else(|| {
                self.single_side_name()
                    .map(|name| format!("{name} - {kind_label}"))
            })
            .unwrap_or(kind_label);
        if self.dirty() {
            format!("* {title}")
        } else {
            title
        }
    }
}

#[derive(Default)]
struct Startup {
    pair: Option<(PathBuf, PathBuf)>,
    center: Option<PathBuf>,
    output: Option<PathBuf>,
    single: Option<PathBuf>,
    kind: Option<SessionKind>,
    capture: Option<PathBuf>,
    dump_layout: Option<PathBuf>,
    size: Option<[f32; 2]>,
    read_only: bool,
    differences: bool,
    capture_modified: bool,
    capture_tabs: bool,
    language: Option<String>,
    capture_menu: Option<String>,
    capture_submenu: Option<String>,
    capture_drop: Vec<PathBuf>,
    capture_drop_separate: bool,
    capture_find: Option<String>,
    capture_compare_results: bool,
    capture_compare_results_expanded: bool,
    capture_sheet: Option<usize>,
    capture_options: Option<String>,
    capture_rules: bool,
    home_select: Option<usize>,
}

impl Startup {
    fn from_args() -> Self {
        let mut options = Self::default();
        let mut paths = Vec::new();
        let mut args = std::env::args_os().skip(1);
        while let Some(arg) = args.next() {
            match arg.to_str() {
                Some("--capture") => options.capture = args.next().map(PathBuf::from),
                Some("--dump-layout") => options.dump_layout = args.next().map(PathBuf::from),
                Some("--text") => options.kind = Some(SessionKind::Text),
                Some("--folder") => options.kind = Some(SessionKind::Folder),
                Some("--sync") => options.kind = Some(SessionKind::FolderSync),
                Some("--text-merge") => options.kind = Some(SessionKind::TextMerge),
                Some("--folder-merge") => options.kind = Some(SessionKind::FolderMerge),
                Some("--center") => options.center = args.next().map(PathBuf::from),
                Some("--output") => options.output = args.next().map(PathBuf::from),
                Some("--hex") => options.kind = Some(SessionKind::Hex),
                Some("--table") => options.kind = Some(SessionKind::Table),
                Some("--version") => options.kind = Some(SessionKind::Version),
                Some("--registry") => options.kind = Some(SessionKind::Registry),
                Some("--media") => options.kind = Some(SessionKind::Media),
                Some("--picture") => options.kind = Some(SessionKind::Picture),
                Some("--edit") => options.kind = Some(SessionKind::TextEdit),
                Some("--read-only") => options.read_only = true,
                Some("--differences") => options.differences = true,
                Some("--capture-modified") => options.capture_modified = true,
                Some("--capture-tabs") => options.capture_tabs = true,
                Some("--language") => {
                    options.language = args.next().map(|arg| arg.to_string_lossy().into_owned())
                }
                Some("--capture-menu") => {
                    options.capture_menu = args.next().map(|arg| arg.to_string_lossy().into_owned())
                }
                Some("--capture-drop") => {
                    if let (Some(left), Some(right)) = (args.next(), args.next()) {
                        options.capture_drop = vec![left.into(), right.into()];
                    }
                }
                Some("--capture-drop-separate") => options.capture_drop_separate = true,
                Some("--capture-submenu") => {
                    options.capture_submenu =
                        args.next().map(|arg| arg.to_string_lossy().into_owned())
                }
                Some("--home-select") => {
                    options.home_select = args
                        .next()
                        .and_then(|arg| arg.to_string_lossy().parse::<usize>().ok())
                }
                Some("--capture-find") => {
                    options.capture_find = args.next().map(|arg| arg.to_string_lossy().into_owned())
                }
                Some("--capture-compare-results") => options.capture_compare_results = true,
                Some("--capture-compare-results-expanded") => {
                    options.capture_compare_results = true;
                    options.capture_compare_results_expanded = true;
                }
                Some("--capture-sheet") => {
                    options.capture_sheet =
                        args.next().and_then(|s| s.to_string_lossy().parse().ok());
                }
                Some("--capture-options") => {
                    options.capture_options =
                        args.next().map(|arg| arg.to_string_lossy().into_owned());
                }
                Some("--capture-rules") => options.capture_rules = true,
                Some("--size") => {
                    if let (Some(width), Some(height)) = (args.next(), args.next()) {
                        if let (Ok(width), Ok(height)) = (
                            width.to_string_lossy().parse::<f32>(),
                            height.to_string_lossy().parse::<f32>(),
                        ) {
                            if width.is_finite() && height.is_finite() {
                                options.size = Some([width.max(700.0), height.max(450.0)]);
                            }
                        }
                    }
                }
                _ => paths.push(PathBuf::from(arg)),
            }
        }
        if options.kind == Some(SessionKind::TextEdit) && paths.len() == 1 {
            paths.push(paths[0].clone());
        }
        if paths.len() == 3 {
            let left = paths.remove(0);
            let center = paths.remove(0);
            let right = paths.remove(0);
            options.pair = Some((left, right));
            if options.center.is_none() {
                options.center = Some(center);
            }
            if options.kind.is_none() {
                if options
                    .pair
                    .as_ref()
                    .is_some_and(|(l, r)| l.is_dir() && r.is_dir())
                {
                    options.kind = Some(SessionKind::FolderMerge);
                } else {
                    options.kind = Some(SessionKind::TextMerge);
                }
            }
        } else if paths.len() == 2 {
            options.pair = Some((paths.remove(0), paths.remove(0)));
        } else if paths.len() == 1 {
            options.single = Some(paths.remove(0));
        }
        options
    }
}

struct AnyCompareApp {
    home: HomeScreen,
    sessions: Vec<Session>,
    session_paths: Vec<Option<PathBuf>>,
    active: Option<usize>,
    icons: IconManager,
    history: History,
    history_valid: bool,
    pending_close: Option<usize>,
    pending_exit: bool,
    allow_exit: bool,
    capture: Option<PathBuf>,
    dump_layout: Option<PathBuf>,
    capture_frames: usize,
    capture_menu: Option<String>,
    capture_submenu: Option<String>,
    capture_drop: Vec<PathBuf>,
    capture_drop_separate: bool,
    /// Title last sent to the window. Every viewport command requests another
    /// frame, so sending it unconditionally repainted the app forever.
    window_title: String,
    options_dialog: ui::options_dialog::OptionsDialog,
    session_settings: ui::session_settings::SessionSettingsDialog,
}

impl AnyCompareApp {
    fn new(cc: &eframe::CreationContext<'_>, startup: Startup) -> Self {
        ui::theme::apply(&cc.egui_ctx);
        let (history, error) = match History::load() {
            Ok(history) => (history, None),
            Err(error) => (History::default(), Some(error)),
        };
        let mut home = HomeScreen::new();
        home.recent = history.entries.clone();
        home.notice = error.clone();
        if let Some(index) = startup.home_select {
            // Reproducible captures of the Home with a recent session selected.
            home.select_recent(index);
        }
        let mut app = Self {
            home,
            sessions: Vec::new(),
            session_paths: Vec::new(),
            active: None,
            icons: IconManager::new(),
            history,
            history_valid: error.is_none(),
            pending_close: None,
            pending_exit: false,
            allow_exit: false,
            capture: startup.capture,
            dump_layout: startup.dump_layout,
            capture_frames: 0,
            capture_menu: startup.capture_menu,
            capture_submenu: startup.capture_submenu,
            capture_drop: startup.capture_drop,
            capture_drop_separate: startup.capture_drop_separate,
            window_title: String::new(),
            options_dialog: ui::options_dialog::OptionsDialog::default(),
            session_settings: ui::session_settings::SessionSettingsDialog::default(),
        };
        if let Some((left, right)) = startup.pair {
            let kind = startup
                .kind
                .unwrap_or_else(|| SessionKind::for_paths(&left, &right));
            if startup.capture_tabs && app.capture.is_some() && kind != SessionKind::Folder {
                if let (Some(left), Some(right)) = (left.parent(), right.parent()) {
                    app.open_pair(RecentSession::new(
                        SessionKind::Folder,
                        left.to_path_buf(),
                        right.to_path_buf(),
                    ));
                }
            }
            app.open_pair(RecentSession::new_merge(
                kind,
                left,
                startup.center,
                right,
                startup.output,
            ));
            if app.capture.is_some() {
                if let Some(index) = app.active {
                    match &mut app.sessions[index] {
                        Session::FolderSync(screen) => {
                            screen.stop_pending();
                            let _ = screen.scan_sync();
                        }
                        Session::FolderMerge(screen) => {
                            screen.stop_pending();
                            let _ = screen.scan_sync();
                        }
                        _ => {}
                    }
                }
            }
        } else if let Some(single) = startup.single {
            let kind = startup.kind.unwrap_or_else(|| {
                if single.is_dir() {
                    SessionKind::Folder
                } else {
                    SessionKind::for_paths(&single, &single)
                }
            });
            app.open_session(kind);
            if let Some(index) = app.active {
                let mut dialogs = ui::dialogs::NativeDialogs::default();
                match &mut app.sessions[index] {
                    Session::Text(screen) => screen.open_side(true, single, &mut dialogs),
                    Session::Folder(screen) => {
                        screen.drop_folders(std::slice::from_ref(&single), Some(true))
                    }
                    Session::FolderSync(screen) => {
                        screen.left_path = single.to_string_lossy().into_owned();
                    }
                    Session::TextMerge(screen) => {
                        screen.left_path = single.to_string_lossy().into_owned();
                        screen.reload(&mut dialogs);
                    }
                    Session::FolderMerge(screen) => {
                        screen.left_path = single.to_string_lossy().into_owned();
                    }
                    Session::Hex(screen) => screen.open_side(true, single, &mut dialogs),
                    Session::Table(screen) => screen.open_side(true, &single, &mut dialogs),
                    Session::Picture(screen) => screen.open_side(true, &single, &mut dialogs),
                    Session::Version(screen) | Session::Properties(screen) => {
                        screen.open_side(true, &single, &mut dialogs)
                    }
                    Session::TextEdit(screen) => {
                        let _ = screen.load(&single);
                    }
                }
            }
        } else if let Some(kind) = startup.kind {
            app.open_session(kind);
        }
        if startup.read_only || startup.differences {
            if let Some(index) = app.active {
                if let Session::TextEdit(screen) = &mut app.sessions[index] {
                    screen.read_only = startup.read_only;
                }
                if let Session::Table(screen) = &mut app.sessions[index] {
                    screen.read_only = startup.read_only;
                    if startup.differences {
                        screen.set_filter(table::Filter::Differences);
                    }
                }
                if let Session::Version(screen) = &mut app.sessions[index] {
                    if startup.differences {
                        screen.set_filter(table::Filter::Differences);
                    }
                }
                if let Session::Text(screen) = &mut app.sessions[index] {
                    if startup.read_only {
                        screen.set_editable(false);
                    }
                    if startup.differences {
                        screen.filter_mode = diff::FilterMode::Differences;
                    }
                }
            }
        }
        if app.capture.is_some() {
            if let (Some(query), Some(session)) = (
                startup.capture_find,
                app.active.map(|index| &mut app.sessions[index]),
            ) {
                match session {
                    Session::Text(screen) => screen.show_search(query),
                    Session::Table(screen) => screen.show_search(query),
                    _ => {}
                }
            }
        }
        // Only the renderer validation mode can prepare an unsaved buffer.
        // The ordinary application never changes input files on startup.
        if startup.capture_modified && app.capture.is_some() {
            if let Some(Session::Text(screen)) = app.active.map(|index| &mut app.sessions[index]) {
                if let Some(section) = screen.diff_result.sections.first() {
                    screen.selected_line_idx = section.start;
                    screen.copy_difference(false);
                }
            }
        }
        if startup.capture_compare_results && app.capture.is_some() {
            if let Some(Session::Folder(screen)) = app.active.map(|index| &mut app.sessions[index])
            {
                screen.capture_run_binary_compare_results(startup.capture_compare_results_expanded);
            }
        }
        if app.capture.is_some() {
            if let (Some(index), Some(Session::Table(screen))) = (
                startup.capture_sheet,
                app.active.map(|i| &mut app.sessions[i]),
            ) {
                if let Err(error) = screen.capture_sheet(index) {
                    screen.error = Some(error);
                }
            }
            if let Some(cat_str) = startup.capture_options.as_deref() {
                let category = ui::options_dialog::OptionsCategory::from_cli(cat_str);
                let current = app.current_preferences();
                app.options_dialog.open(category, current);
            }
            if startup.capture_rules {
                if let Some(rules) = app.active_session_rules() {
                    app.session_settings.open(rules);
                }
            }
        }
        app
    }

    fn current_preferences(&self) -> i18n::AppPreferences {
        let mut prefs = i18n::preferences();
        if let Some(session) = self.active.map(|i| &self.sessions[i]) {
            match session {
                Session::Folder(s) => s.sync_preferences(&mut prefs),
                Session::Text(s) => s.sync_preferences(&mut prefs),
                Session::Table(s) => s.sync_preferences(&mut prefs),
                Session::Picture(s) => s.sync_preferences(&mut prefs),
                Session::Hex(s) => s.sync_preferences(&mut prefs),
                Session::FolderSync(_)
                | Session::TextMerge(_)
                | Session::FolderMerge(_)
                | Session::Version(_)
                | Session::Properties(_)
                | Session::TextEdit(_) => {}
            }
        }
        prefs
    }

    fn apply_preferences(&mut self, prefs: &i18n::AppPreferences) {
        if let Some(session) = self.active.map(|i| &mut self.sessions[i]) {
            match session {
                Session::Folder(s) => s.apply_preferences(prefs),
                Session::Text(s) => s.apply_preferences(prefs),
                Session::TextMerge(s) => s.set_editor_font_size(prefs.editor_font_size as f32),
                Session::Table(s) => s.apply_preferences(prefs),
                Session::Picture(s) => s.apply_preferences(prefs),
                Session::Hex(s) => s.apply_preferences(prefs),
                Session::FolderSync(_)
                | Session::FolderMerge(_)
                | Session::Version(_)
                | Session::Properties(_)
                | Session::TextEdit(_) => {}
            }
        }
    }

    fn active_session_rules(&self) -> Option<ui::session_settings::SessionRules> {
        match self.active.map(|i| &self.sessions[i])? {
            Session::Folder(s) => Some(ui::session_settings::SessionRules::Folder(s.rules_draft())),
            Session::Text(s) => Some(ui::session_settings::SessionRules::Text(s.rules_draft())),
            Session::Picture(s) => {
                Some(ui::session_settings::SessionRules::Picture(s.rules_draft()))
            }
            _ => None,
        }
    }

    fn apply_session_rules(&mut self, rules: ui::session_settings::SessionRules) {
        let Some(session) = self.active.map(|i| &mut self.sessions[i]) else {
            return;
        };
        match (session, rules) {
            (Session::Folder(s), ui::session_settings::SessionRules::Folder(d)) => s.apply_rules(d),
            (Session::Text(s), ui::session_settings::SessionRules::Text(d)) => s.apply_rules(d),
            (Session::Picture(s), ui::session_settings::SessionRules::Picture(d)) => {
                s.apply_rules(d)
            }
            _ => {}
        }
    }

    fn open_session(&mut self, kind: SessionKind) {
        self.active = Some(self.sessions.len());
        self.sessions.push(Session::new(kind));
        self.session_paths.push(None);
    }

    fn open_pair(&mut self, record: RecentSession) {
        let record = record.resolve_legacy_format();
        if let Some(index) = self
            .sessions
            .iter()
            .position(|session| session.record().as_ref() == Some(&record))
        {
            self.active = Some(index);
            return;
        }
        let empty = self
            .sessions
            .iter()
            .position(|session| session.is_empty(record.kind));
        let mut session = Session::new(record.kind);
        match &mut session {
            Session::TextEdit(screen) => {
                if screen.load(&record.left).is_err() {
                    return;
                }
            }
            Session::Text(screen) => {
                screen.load_files(
                    &record.left.to_string_lossy(),
                    &record.right.to_string_lossy(),
                );
                if screen.left.path.is_none() || screen.right.path.is_none() {
                    // A folder or dropped pair can contain binary data or an unsupported encoding.
                    let mut hex = HexCompareScreen::new();
                    hex.load_files(&record.left, &record.right);
                    session = Session::Hex(Box::new(hex));
                }
            }
            Session::Version(screen) | Session::Properties(screen) => {
                if let Err(error) = screen.load_files(&record.left, &record.right) {
                    screen.set_error(error);
                }
            }
            Session::Picture(screen) => {
                if let Err(error) = screen.load_files(&record.left, &record.right) {
                    screen.set_error(error);
                }
            }
            Session::Table(screen) => {
                if let Err(error) = screen.load_files(&record.left, &record.right) {
                    screen.error = Some(error);
                }
            }
            Session::Folder(screen) => screen.load_folders(&record.left, &record.right),
            Session::FolderSync(screen) => screen.load_folders(&record.left, &record.right),
            Session::TextMerge(screen) => {
                let _ = screen.load_files_silent(
                    &record.left,
                    record.center.as_deref(),
                    &record.right,
                    record.output.as_deref(),
                );
            }
            Session::FolderMerge(screen) => screen.load_folders(
                &record.left,
                record.center.as_deref(),
                &record.right,
                record.output.as_deref(),
            ),
            Session::Hex(screen) => screen.load_files(&record.left, &record.right),
        }
        if let Some(index) = empty {
            self.sessions[index] = session;
            self.session_paths[index] = None;
            self.active = Some(index);
        } else {
            self.active = Some(self.sessions.len());
            self.sessions.push(session);
            self.session_paths.push(None);
        }
    }

    #[cfg(test)]
    fn handle_dropped_input(
        &mut self,
        ctx: &egui::Context,
        dialogs: &mut dyn ui::dialogs::DialogHost,
    ) {
        self.handle_dropped_input_with_frame(ctx, None, dialogs);
    }

    fn handle_dropped_input_with_frame(
        &mut self,
        ctx: &egui::Context,
        frame: Option<&eframe::Frame>,
        dialogs: &mut dyn ui::dialogs::DialogHost,
    ) {
        let paths = ctx.input_mut(|input| {
            std::mem::take(&mut input.raw.dropped_files)
                .into_iter()
                .filter_map(|file| file.path)
                .collect::<Vec<_>>()
        });
        if paths.is_empty() {
            return;
        }
        ctx.memory_mut(|memory| memory.close_popup());
        let (pos, side) = self.drop_cursor_location(ctx, frame);
        self.handle_dropped_paths_at(&paths, side, pos, dialogs);
    }

    /// Resolve real cursor coordinates and side (`Some(true)` = left, `Some(false)` = right)
    /// during external drag-and-drop. On Windows, winit's `IDropTarget` ignores `POINTL`
    /// and never emits `CursorMoved`, leaving `egui`'s `latest_pos()` stale at the last
    /// pre-drag position; querying Win32 `GetCursorPos` + `ScreenToClient` fixes that.
    fn drop_cursor_location(
        &self,
        ctx: &egui::Context,
        frame: Option<&eframe::Frame>,
    ) -> (Option<egui::Pos2>, Option<bool>) {
        if self.capture.is_none() {
            if let Some(frame) = frame {
                if let Some((pos, left)) = native_cursor_location(ctx, frame) {
                    return (Some(pos), Some(left));
                }
            }
        }
        let center_x = ctx.screen_rect().center().x;
        let pos = ctx.input(|input| input.pointer.latest_pos());
        let side = pos.map(|p| p.x < center_x);
        (pos, side)
    }

    #[cfg(test)]
    fn handle_dropped_paths(
        &mut self,
        paths: &[PathBuf],
        target_left: Option<bool>,
        dialogs: &mut dyn ui::dialogs::DialogHost,
    ) {
        self.handle_dropped_paths_at(paths, target_left, None, dialogs);
    }

    fn handle_dropped_paths_at(
        &mut self,
        paths: &[PathBuf],
        target_left: Option<bool>,
        drop_pos: Option<egui::Pos2>,
        dialogs: &mut dyn ui::dialogs::DialogHost,
    ) {
        if paths.is_empty() {
            return;
        }
        if paths.len() > 2 {
            dialogs.error("app.drop_at_most_two_folders_or_two_files_to");
            return;
        }
        if let Some(path) = paths.iter().find(|path| !path.exists()) {
            dialogs.error(&format!(
                "{}\n{}",
                i18n::tr("app.the_dropped_item_could_not_be_accessed"),
                path.display()
            ));
            return;
        }
        let home_kind = if self.active.is_none() {
            drop_pos.and_then(|pos| self.home.drop_target_at(pos).map(|(_, kind)| kind))
        } else {
            None
        };
        if paths
            .iter()
            .all(|path| crate::folder_source::is_folder_like(path))
            && !(paths.iter().all(|path| path.is_file())
                && matches!(home_kind, Some(SessionKind::Hex)))
        {
            let all_dirs = paths.iter().all(|path| path.is_dir());
            let folder_kind = match home_kind {
                Some(SessionKind::Folder) => SessionKind::Folder,
                Some(SessionKind::FolderSync | SessionKind::FolderMerge) if all_dirs => {
                    home_kind.unwrap()
                }
                Some(SessionKind::FolderSync | SessionKind::FolderMerge) => {
                    dialogs.error("error.drop.real_folders_only");
                    return;
                }
                Some(_) => {
                    dialogs.error("app.this_session_compares_files_drop_files_here");
                    return;
                }
                None => match self.active.map(|i| self.sessions[i].kind()) {
                    Some(k @ (SessionKind::FolderSync | SessionKind::FolderMerge)) if all_dirs => k,
                    _ => SessionKind::Folder,
                },
            };
            if folder_kind == SessionKind::FolderSync {
                if let [left, right, ..] = paths {
                    self.open_pair(RecentSession::new(
                        SessionKind::FolderSync,
                        left.clone(),
                        right.clone(),
                    ));
                } else if let Some(one) = paths.first() {
                    let index = self
                        .sessions
                        .iter()
                        .position(|s| {
                            matches!(s, Session::FolderSync(sc) if sc.left_path.is_empty() || sc.right_path.is_empty())
                        })
                        .unwrap_or_else(|| {
                            self.open_session(SessionKind::FolderSync);
                            self.active.unwrap()
                        });
                    self.active = Some(index);
                    if let Session::FolderSync(sc) = &mut self.sessions[index] {
                        if target_left == Some(false) || (!sc.left_path.is_empty() && sc.right_path.is_empty()) {
                            sc.right_path = one.to_string_lossy().into_owned();
                        } else {
                            sc.left_path = one.to_string_lossy().into_owned();
                        }
                        if sc.has_pair() {
                            sc.start_scan();
                        }
                    }
                }
                return;
            }
            if folder_kind == SessionKind::FolderMerge {
                match paths {
                    [left, center, right, ..] => {
                        self.open_pair(RecentSession::new_merge(
                            SessionKind::FolderMerge,
                            left.clone(),
                            Some(center.clone()),
                            right.clone(),
                            None,
                        ));
                    }
                    [left, right] => {
                        self.open_pair(RecentSession::new(
                            SessionKind::FolderMerge,
                            left.clone(),
                            right.clone(),
                        ));
                    }
                    [one] => {
                        let index = self
                            .sessions
                            .iter()
                            .position(|s| {
                                matches!(s, Session::FolderMerge(sc) if sc.left_path.is_empty() || sc.right_path.is_empty())
                            })
                            .unwrap_or_else(|| {
                                self.open_session(SessionKind::FolderMerge);
                                self.active.unwrap()
                            });
                        self.active = Some(index);
                        if let Session::FolderMerge(sc) = &mut self.sessions[index] {
                            if sc.left_path.is_empty() {
                                sc.left_path = one.to_string_lossy().into_owned();
                            } else if sc.right_path.is_empty() {
                                sc.right_path = one.to_string_lossy().into_owned();
                            } else {
                                sc.center_path = one.to_string_lossy().into_owned();
                            }
                            if sc.has_pair() {
                                sc.start_scan();
                            }
                        }
                    }
                    [] => {}
                }
                return;
            }
            let active = self
                .active
                .filter(|&index| matches!(self.sessions[index], Session::Folder(_)));
            let reusable = active.or_else(|| {
                self.sessions.iter().position(|session| {
                    matches!(
                        session,
                        Session::Folder(screen)
                            if screen.left_path.is_empty() || screen.right_path.is_empty()
                    )
                })
            });
            let index = reusable.unwrap_or_else(|| {
                self.open_session(SessionKind::Folder);
                self.active.unwrap()
            });
            self.active = Some(index);
            if let Session::Folder(screen) = &mut self.sessions[index] {
                screen.drop_folders(paths, target_left);
                self.session_paths[index] = None;
            }
        } else if paths.iter().any(|path| path.is_dir()) {
            dialogs.error("app.compare_two_folders_or_two_files_do_not_mix");
        } else if let [left, center, right, ..] = paths {
            self.open_pair(RecentSession::new_merge(
                SessionKind::TextMerge,
                left.clone(),
                Some(center.clone()),
                right.clone(),
                None,
            ));
        } else if let [left, right] = paths {
            self.handle_dropped_file_pair(left, right, home_kind, dialogs);
        } else if let Some(path) = paths.first() {
            self.handle_dropped_single_file(path, target_left, home_kind, dialogs);
        }
    }

    fn handle_dropped_file_pair(
        &mut self,
        left: &PathBuf,
        right: &PathBuf,
        home_kind: Option<SessionKind>,
        dialogs: &mut dyn ui::dialogs::DialogHost,
    ) {
        if let Some(kind) = home_kind {
            if matches!(
                kind,
                SessionKind::Folder | SessionKind::FolderSync | SessionKind::FolderMerge
            ) {
                dialogs.error("app.this_session_compares_folders_drop_folders");
                return;
            }
            if kind == SessionKind::TextEdit {
                for path in [left, right] {
                    match document::Document::load(path) {
                        Ok(_) => self.open_pair(RecentSession::new(
                            SessionKind::TextEdit,
                            path.clone(),
                            path.clone(),
                        )),
                        Err(error) => dialogs.error(&error),
                    }
                }
                return;
            }
            self.open_pair(RecentSession::new(kind, left.clone(), right.clone()));
            return;
        }
        let detected = SessionKind::for_paths(left, right);
        if let Some(index) = self.active {
            if !self.sessions[index].dirty() {
                match &mut self.sessions[index] {
                    Session::Hex(screen) => {
                        screen.load_files(left, right);
                        self.session_paths[index] = None;
                        return;
                    }
                    Session::Text(screen) if detected == SessionKind::Text => {
                        screen.load_files(&left.to_string_lossy(), &right.to_string_lossy());
                        if screen.left.path.is_none() || screen.right.path.is_none() {
                            let mut hex = HexCompareScreen::new();
                            hex.load_files(left, right);
                            self.sessions[index] = Session::Hex(Box::new(hex));
                        }
                        self.session_paths[index] = None;
                        return;
                    }
                    Session::Table(screen) if detected == SessionKind::Table => {
                        if let Err(error) = screen.load_files(left, right) {
                            screen.error = Some(error);
                        }
                        self.session_paths[index] = None;
                        return;
                    }
                    Session::Version(screen) if detected == SessionKind::Version => {
                        if let Err(error) = screen.load_files(left, right) {
                            screen.set_error(error);
                        }
                        self.session_paths[index] = None;
                        return;
                    }
                    Session::Properties(screen) if detected == screen.kind() => {
                        if let Err(error) = screen.load_files(left, right) {
                            screen.set_error(error);
                        }
                        self.session_paths[index] = None;
                        return;
                    }
                    Session::Picture(screen) if detected == SessionKind::Picture => {
                        if let Err(error) = screen.load_files(left, right) {
                            screen.set_error(error);
                        }
                        self.session_paths[index] = None;
                        return;
                    }
                    _ => {}
                }
            }
        }
        self.open_pair(RecentSession::new(detected, left.clone(), right.clone()));
    }

    fn handle_dropped_single_file(
        &mut self,
        path: &PathBuf,
        target_left: Option<bool>,
        home_kind: Option<SessionKind>,
        dialogs: &mut dyn ui::dialogs::DialogHost,
    ) {
        if let Some(kind) = home_kind {
            if matches!(
                kind,
                SessionKind::Folder | SessionKind::FolderSync | SessionKind::FolderMerge
            ) {
                dialogs.error("app.this_session_compares_folders_drop_folders");
                return;
            }
            if kind == SessionKind::TextEdit {
                match document::Document::load(path) {
                    Ok(_) => self.open_pair(RecentSession::new(
                        SessionKind::TextEdit,
                        path.clone(),
                        path.clone(),
                    )),
                    Err(error) => dialogs.error(&error),
                }
                return;
            }
            let index = self
                .sessions
                .iter()
                .position(|s| s.is_empty(kind))
                .unwrap_or_else(|| {
                    self.open_session(kind);
                    self.active.unwrap()
                });
            self.active = Some(index);
            self.drop_file_into_session(index, path, None, target_left, dialogs);
            return;
        }

        // First, try to load into the currently active session if it accepts this file.
        if let Some(index) = self.active {
            let detected = SessionKind::for_paths(path, path);
            match &mut self.sessions[index] {
                Session::TextEdit(screen) => {
                    if screen.confirm_saved(dialogs) {
                        if let Err(error) = screen.load(path) {
                            dialogs.error(&error);
                        } else {
                            self.session_paths[index] = None;
                        }
                    }
                    return;
                }
                Session::Hex(_) => {
                    self.drop_file_into_session(index, path, None, target_left, dialogs);
                    return;
                }
                Session::Table(_) if table::supported(path) => {
                    self.drop_file_into_session(index, path, None, target_left, dialogs);
                    return;
                }
                Session::Version(_) if pe_version::supported(path) => {
                    self.drop_file_into_session(index, path, None, target_left, dialogs);
                    return;
                }
                Session::Properties(screen) if detected == screen.kind() => {
                    self.drop_file_into_session(index, path, None, target_left, dialogs);
                    return;
                }
                Session::Picture(_) if picture::is_picture_path(path) => {
                    self.drop_file_into_session(index, path, None, target_left, dialogs);
                    return;
                }
                Session::Text(screen) => {
                    let both_empty = screen.is_side_empty(true) && screen.is_side_empty(false);
                    let specialized = matches!(
                        detected,
                        SessionKind::Registry
                            | SessionKind::Media
                            | SessionKind::Picture
                            | SessionKind::Version
                            | SessionKind::Table
                    );
                    if !both_empty || !specialized {
                        if let Ok(doc) = document::Document::load(path) {
                            self.drop_file_into_session(
                                index,
                                path,
                                Some(doc),
                                target_left,
                                dialogs,
                            );
                            return;
                        }
                    }
                }
                _ => {}
            }
        }

        // Otherwise route by file type, reusing a session with an open slot before
        // opening a new tab (filling the missing right/left side first).
        let detected = SessionKind::for_paths(path, path);
        let (kind, loaded_text) = if matches!(
            detected,
            SessionKind::Registry
                | SessionKind::Media
                | SessionKind::Picture
                | SessionKind::Version
                | SessionKind::Table
        ) {
            (detected, None)
        } else {
            match document::Document::load(path) {
                Ok(doc) => (SessionKind::Text, Some(doc)),
                Err(_) => (SessionKind::Hex, None),
            }
        };

        let index = self
            .find_reusable_single_drop_session(kind)
            .unwrap_or_else(|| {
                self.open_session(kind);
                self.active.unwrap()
            });
        self.active = Some(index);
        self.drop_file_into_session(index, path, loaded_text, target_left, dialogs);
    }

    fn find_reusable_single_drop_session(&self, kind: SessionKind) -> Option<usize> {
        // 1) Prefer a session of this kind that already has one side filled and one empty
        //    (the user is dropping the second file of a pair).
        if let Some(index) = self.sessions.iter().position(|session| {
            session.kind() == kind
                && session
                    .side_state()
                    .is_some_and(|(left_empty, right_empty, _)| left_empty ^ right_empty)
        }) {
            return Some(index);
        }
        // 2) Next, prefer an empty session of this kind.
        self.sessions
            .iter()
            .position(|session| session.is_empty(kind))
    }

    fn drop_file_into_session(
        &mut self,
        index: usize,
        path: &PathBuf,
        loaded_text: Option<document::Document>,
        target_left: Option<bool>,
        dialogs: &mut dyn ui::dialogs::DialogHost,
    ) {
        let Some((left_empty, right_empty, active_left)) = self.sessions[index].side_state() else {
            return;
        };
        let side = choose_drop_side(left_empty, right_empty, target_left, active_left);
        match &mut self.sessions[index] {
            Session::Text(screen) => {
                if let Some(doc) = loaded_text {
                    screen.open_loaded_side(side, doc, path.clone(), dialogs);
                } else {
                    screen.open_side(side, path.clone(), dialogs);
                }
            }
            Session::Hex(screen) => screen.open_side(side, path.clone(), dialogs),
            Session::Table(screen) => screen.open_side(side, path, dialogs),
            Session::Version(screen) | Session::Properties(screen) => {
                screen.open_side(side, path, dialogs);
            }
            Session::Picture(screen) => {
                screen.open_side(side, path, dialogs);
            }
            Session::Folder(screen) => {
                screen.drop_folders(std::slice::from_ref(path), Some(side));
            }
            Session::TextMerge(screen) => {
                if side {
                    screen.left_path = path.to_string_lossy().into_owned();
                } else {
                    screen.right_path = path.to_string_lossy().into_owned();
                }
                screen.reload(dialogs);
            }
            Session::FolderSync(_) | Session::FolderMerge(_) | Session::TextEdit(_) => return,
        }
        self.session_paths[index] = None;
    }

    fn paint_drop_overlay(&self, ctx: &egui::Context, frame: &eframe::Frame) {
        if self.capture.is_some() {
            return;
        }
        let hovered_count = ctx.input(|i| i.raw.hovered_files.len());
        if hovered_count == 0 {
            return;
        }
        // Poll cursor position while files are dragged over the window since winit's
        // Windows `IDropTarget::DragOver` does not emit `CursorMoved`.
        ctx.request_repaint_after(std::time::Duration::from_millis(32));
        let (pos, target_left) = self.drop_cursor_location(ctx, Some(frame));
        let screen = ctx.screen_rect();
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("drop_target_overlay"),
        ));
        let accent = egui::Color32::from_rgb(0, 120, 212);

        if hovered_count > 2 {
            let warn = egui::Color32::from_rgb(196, 43, 28);
            painter.rect_stroke(screen.shrink(2.0), 0.0, egui::Stroke::new(2.0_f32, warn));
            return;
        }

        if self.active.is_none() {
            // On Home, highlight only the hovered launcher icon (BC5-style), never tint the window.
            if let Some((rect, _)) = pos.and_then(|p| self.home.drop_target_at(p)) {
                painter.rect(
                    rect,
                    2.0,
                    egui::Color32::from_rgba_unmultiplied(0, 120, 212, 22),
                    egui::Stroke::new(1.0_f32, accent),
                );
            }
            return;
        }

        // Inside a 2-pane comparison, outline only the target pane for single-item drops (no blue wash).
        if hovered_count == 1 {
            if let Some((left_empty, right_empty, active_left)) =
                self.active.and_then(|i| self.sessions[i].side_state())
            {
                let side_left = choose_drop_side(left_empty, right_empty, target_left, active_left);
                let mid = screen.center().x;
                let half = if side_left {
                    egui::Rect::from_min_max(screen.min, egui::pos2(mid, screen.max.y))
                } else {
                    egui::Rect::from_min_max(egui::pos2(mid, screen.min.y), screen.max)
                };
                painter.rect_stroke(half.shrink(2.0), 0.0, egui::Stroke::new(2.0_f32, accent));
            }
        }
    }

    fn remove_session(&mut self, index: usize) {
        self.sessions.remove(index);
        self.session_paths.remove(index);
        self.active = match self.active {
            Some(active) if active == index => {
                (!self.sessions.is_empty()).then(|| active.min(self.sessions.len() - 1))
            }
            Some(active) if active > index => Some(active - 1),
            other => other,
        };
    }

    fn session_request(
        &mut self,
        request: ui::session_menu::Request,
        dialogs: &mut dyn ui::dialogs::DialogHost,
    ) {
        use ui::session_menu::Request;
        let result = (|| -> Result<(), String> {
            match request {
                Request::EditText(path) => {
                    // Read before adding a tab so a failed decode cannot create
                    // a misleading blank editor or route to Registry/Hex again.
                    let _ = document::Document::load(&path)?;
                    self.open_pair(RecentSession::new(
                        SessionKind::TextEdit,
                        path.clone(),
                        path,
                    ));
                }
                Request::SourceSaved(path) => {
                    self.refresh_property_sources(&path, dialogs);
                }
                Request::New(kind) => self.open_session(kind),
                Request::Recent(record) => self.open_pair(record),
                Request::NewWindow => {
                    std::process::Command::new(std::env::current_exe().map_err(|e| e.to_string())?)
                        .args(["--language", &i18n::language()])
                        .spawn()
                        .map_err(|e| e.to_string())?;
                }
                Request::Close => self.pending_close = self.active,
                Request::Open | Request::LoadWorkspace => {
                    let workspace = matches!(request, Request::LoadWorkspace);
                    let Some(path) = dialogs.named_file(
                        if workspace {
                            "menu.session.load_workspace"
                        } else {
                            "app.open_session"
                        },
                        false,
                        None,
                        "",
                    ) else {
                        return Ok(());
                    };
                    let saved = workspace::Workspace::load(&path)?;
                    let mut sessions = saved
                        .sessions
                        .iter()
                        .map(Session::from_saved)
                        .collect::<Result<Vec<_>, _>>()?;
                    if workspace {
                        if self.sessions.iter_mut().all(|s| s.confirm_saved(dialogs)) {
                            self.sessions = sessions;
                            self.session_paths = vec![None; self.sessions.len()];
                            self.active = saved.active;
                        }
                    } else {
                        if sessions.len() != 1 {
                            return Err(crate::i18n::tr(
                                "app.the_file_contains_a_workspace_instead_of_a",
                            ));
                        }
                        self.active = Some(self.sessions.len());
                        self.sessions.push(sessions.remove(0));
                        self.session_paths.push(Some(path));
                    }
                }
                Request::Save(save_as) => {
                    let Some(index) = self.active else {
                        return Ok(());
                    };
                    let Some(saved) = self.sessions[index].saved() else {
                        return Err(crate::i18n::tr(
                            "app.open_both_files_or_folders_before_saving_the",
                        ));
                    };
                    let current = self.session_paths[index].as_deref();
                    let path = if save_as || current.is_none() {
                        let name = current
                            .and_then(std::path::Path::file_name)
                            .map(|s| s.to_string_lossy().into_owned())
                            .unwrap_or_else(|| "comparison.acsession.json".into());
                        dialogs.named_file("menu.session.save_as", true, current, &name)
                    } else {
                        current.map(std::path::Path::to_owned)
                    };
                    if let Some(path) = path {
                        workspace::Workspace::new(vec![saved], Some(0))
                            .save(&path, &self.protected_paths())?;
                        self.session_paths[index] = Some(path);
                    }
                }
                Request::SaveWorkspace => {
                    let mut sessions = Vec::new();
                    let mut active = None;
                    for (index, session) in self.sessions.iter().enumerate() {
                        if let Some(saved) = session.saved() {
                            if self.active == Some(index) {
                                active = Some(sessions.len());
                            }
                            sessions.push(saved);
                        }
                    }
                    if sessions.is_empty() {
                        return Err(crate::i18n::tr("app.no_session_with_two_paths_is_open"));
                    }
                    if let Some(path) = dialogs.named_file(
                        "menu.session.save_workspace_as",
                        true,
                        None,
                        "workspace.acworkspace.json",
                    ) {
                        workspace::Workspace::new(sessions, active)
                            .save(&path, &self.protected_paths())?;
                    }
                }
                Request::CompareUsing(kind) => {
                    if let Some(mut files) = self.active.and_then(|i| self.sessions[i].record()) {
                        files.kind = kind;
                        self.open_pair(files);
                    }
                }
                Request::MergeFiles => {
                    if let Some(files) = self.active.and_then(|i| self.sessions[i].record()) {
                        if files.left.is_dir() && files.right.is_dir() {
                            self.open_pair(RecentSession::new(
                                SessionKind::FolderMerge,
                                files.left,
                                files.right,
                            ));
                        } else {
                            self.open_pair(RecentSession::new(
                                SessionKind::TextMerge,
                                files.left,
                                files.right,
                            ));
                        }
                    } else {
                        self.open_session(SessionKind::TextMerge);
                    }
                }
                Request::ManageWorkspaces => {
                    let count = self.sessions.len();
                    dialogs.info(&format!(
                        "{}: {}",
                        crate::i18n::tr("menu.session.manage_workspaces").trim_end_matches("..."),
                        count
                    ));
                }
                Request::SyncBaseFolders(left, right) => {
                    self.open_pair(RecentSession::new(SessionKind::FolderSync, left, right));
                }
                Request::MergeBaseFolders(left, right) => {
                    self.open_pair(RecentSession::new(SessionKind::FolderMerge, left, right));
                }
                Request::CompareBaseFolders(left, right) => {
                    self.open_pair(RecentSession::new(SessionKind::Folder, left, right));
                }
                Request::ParentFolders => {
                    if let Some(files) = self.active.and_then(|i| self.sessions[i].record()) {
                        if let (Some(left), Some(right)) =
                            (files.left.parent(), files.right.parent())
                        {
                            self.open_pair(RecentSession::new(
                                SessionKind::Folder,
                                left.into(),
                                right.into(),
                            ));
                        }
                    }
                }
            }
            Ok(())
        })();
        if let Err(error) = result {
            dialogs.error(&error);
        }
    }
    fn protected_paths(&self) -> Vec<PathBuf> {
        self.sessions
            .iter()
            .filter_map(Session::record)
            .flat_map(|r| [r.left, r.right])
            .collect()
    }
    fn refresh_property_sources(
        &mut self,
        path: &std::path::Path,
        dialogs: &mut dyn ui::dialogs::DialogHost,
    ) {
        for session in &mut self.sessions {
            if let Session::Version(screen) | Session::Properties(screen) = session {
                screen.refresh_source(path, dialogs);
            }
        }
    }
}

impl eframe::App for AnyCompareApp {
    fn raw_input_hook(&mut self, ctx: &egui::Context, input: &mut egui::RawInput) {
        ui::session_picker::route_input(ctx, input);
        // Captures model an active application window even when the validation
        // process is launched hidden. This makes focus/selection states explicit
        // and reproducible without activating another user's desktop window.
        if self.capture.is_some() {
            input.focused = true;
        }
        if self.capture.is_some() && !self.capture_drop.is_empty() {
            let paths: &[PathBuf] = if self.capture_drop_separate {
                match self.capture_frames {
                    2 => &self.capture_drop[..1],
                    4 => &self.capture_drop[1..],
                    _ => &[],
                }
            } else if self.capture_frames == 2 {
                &self.capture_drop
            } else {
                &[]
            };
            input
                .dropped_files
                .extend(paths.iter().map(|path| egui::DroppedFile {
                    path: Some(path.clone()),
                    ..Default::default()
                }));
        }
        // Controlled clicks target this application's menu only, in capture mode.
        let cell_edit = matches!(
            self.capture_menu.as_deref(),
            Some("table.edit_cell" | "Editar célula")
        );
        if self.capture.is_some()
            && ([4, 5, 8, 9].contains(&self.capture_frames)
                || cell_edit && [6, 7].contains(&self.capture_frames))
        {
            let label =
                if self.capture_frames <= 5 || cell_edit && [6, 7].contains(&self.capture_frames) {
                    &self.capture_menu
                } else {
                    &self.capture_submenu
                };
            if let Some(label) = label {
                if let Some(rect) = ctx.data(|data| {
                    data.get_temp::<egui::Rect>(egui::Id::new("capture_menu_rect").with(label))
                }) {
                    input.events.push(egui::Event::PointerMoved(rect.center()));
                    input.events.push(egui::Event::PointerButton {
                        pos: rect.center(),
                        button: if matches!(
                            label.as_str(),
                            "table.cell_menu"
                                | "Menu da célula"
                                | "folder_first_row"
                                | "text_pane_left"
                                | "hex_first_row"
                                | "pix_pane_left"
                        ) || label.starts_with("Menu de propriedades")
                        {
                            egui::PointerButton::Secondary
                        } else {
                            egui::PointerButton::Primary
                        },
                        pressed: [4, 6, 8].contains(&self.capture_frames),
                        modifiers: Default::default(),
                    });
                }
            }
        }
    }
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        ui::audit::set_enabled(ctx, self.dump_layout.is_some());
        ui::audit::begin_frame(ctx);
        // The original scrolls its text panes by the Windows "lines per notch"
        // setting (3 by default) of 16 px rows; egui's default is 40 px.
        let line_scroll = match self.active.map(|index| &self.sessions[index]) {
            Some(Session::Text(_)) => wheel_scroll_lines() as f32 * ui::aligned_editor::ROW_HEIGHT,
            _ => 40.0,
        };
        ctx.options_mut(|options| options.line_scroll_speed = line_scroll);
        let mut dialogs = ui::dialogs::NativeDialogs::parented(frame);
        if ctx.input(|i| i.viewport().close_requested())
            && !self.allow_exit
            && self.sessions.iter().any(Session::dirty)
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.pending_exit = true;
        }
        self.handle_dropped_input_with_frame(ctx, Some(frame), &mut dialogs);

        let modal_open = self.options_dialog.is_open() || self.session_settings.is_open();
        if !modal_open {
            if ctx.input_mut(|input| input.consume_key(egui::Modifiers::CTRL, egui::Key::Q)) {
                self.pending_exit = true;
            }
            if ctx.input_mut(|input| input.consume_key(egui::Modifiers::ALT, egui::Key::Home)) {
                self.active = None;
            }
            if ctx.input_mut(|input| input.consume_key(egui::Modifiers::CTRL, egui::Key::T)) {
                self.open_session(SessionKind::Text);
            }
            for (key, command) in [
                (egui::Key::O, ui::session_menu::Request::Open),
                (egui::Key::S, ui::session_menu::Request::Save(false)),
            ] {
                if ctx.input_mut(|input| {
                    input.consume_key(egui::Modifiers::CTRL | egui::Modifiers::SHIFT, key)
                }) {
                    self.session_request(command, &mut dialogs);
                }
            }
            if !self.sessions.is_empty() {
                let backwards = ctx.input_mut(|input| {
                    input.consume_key(
                        egui::Modifiers::CTRL | egui::Modifiers::SHIFT,
                        egui::Key::Tab,
                    )
                });
                if backwards
                    || ctx
                        .input_mut(|input| input.consume_key(egui::Modifiers::CTRL, egui::Key::Tab))
                {
                    let current = self.active.unwrap_or(0);
                    self.active = Some(if backwards {
                        (current + self.sessions.len() - 1) % self.sessions.len()
                    } else {
                        (current + 1) % self.sessions.len()
                    });
                }
            }
        }
        let close = if !modal_open
            && ctx.input_mut(|input| input.consume_key(egui::Modifiers::CTRL, egui::Key::W))
        {
            self.active
        } else {
            None
        };
        let title = self
            .active
            .map(|i| self.sessions[i].title())
            .unwrap_or_else(|| i18n::tr("session.home"));
        let title = format!("{title} - AnyCompare");
        if title != self.window_title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.window_title = title;
        }
        if let Some(index) = close {
            if self.sessions[index].dirty() {
                self.pending_close = Some(index);
            } else {
                self.remove_session(index);
            }
        }

        ui::session_tabs::set(
            ctx,
            self.sessions
                .iter()
                .map(|session| ui::session_tabs::Tab {
                    title: session.title(),
                    icon: match session {
                        Session::Text(_) => "imgTextCompareSession",
                        Session::TextEdit(_) => "imgTextEditSession",
                        Session::Folder(_) => "imgDirCompareSession",
                        Session::FolderSync(_) => "imgDirSyncSession",
                        Session::TextMerge(_) => "imgTextMergeSession",
                        Session::FolderMerge(_) => "imgDirMergeSession",
                        Session::Hex(_) => "imgHexCompareSession",
                        Session::Table(_) => "imgTableCompareSession",
                        Session::Version(_) => "imgVerCompareSession",
                        Session::Picture(_) => "imgPixCompareSession",
                        Session::Properties(s) => {
                            if s.kind() == SessionKind::Media {
                                "imgMP3CompareSession"
                            } else {
                                "imgRegCompareSession"
                            }
                        }
                    },
                })
                .collect(),
            self.active,
        );
        let mut home_action = HomeAction::None;
        ui::session_picker::set_history(ctx, &self.history.entries);
        let mut open_files = None;
        let mut open_merge_files: Option<(PathBuf, Option<PathBuf>, PathBuf, Option<PathBuf>)> =
            None;
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(ui::theme::BcTheme::WINDOW_BG))
            .show(ctx, |ui| {
                if let Some(index) = self.active {
                    ui.push_id(index, |ui| match &mut self.sessions[index] {
                        Session::TextEdit(screen) => {
                            screen.set_recent_searches(&self.history.searches);
                            if screen.ui(ctx, ui, &mut self.icons, &mut dialogs) {
                                self.active = None;
                            }
                        }
                        Session::Text(screen) => {
                            screen.set_recent_history(&self.history);
                            if matches!(
                                screen.ui(ctx, ui, &mut self.icons, &mut dialogs),
                                TextCompareAction::BackToHome
                            ) {
                                self.active = None;
                            }
                        }
                        Session::Folder(screen) => {
                            match screen.ui(ctx, ui, &mut self.icons, &mut dialogs) {
                                FolderAction::Home => self.active = None,
                                FolderAction::OpenFiles(left, right) => {
                                    open_files = Some((left, right))
                                }
                                FolderAction::None => {}
                            }
                        }
                        Session::FolderSync(screen) => {
                            match screen.ui(ctx, ui, &mut self.icons, &mut dialogs) {
                                FolderSyncAction::Home => self.active = None,
                                FolderSyncAction::CompareFiles(left, right) => {
                                    open_files = Some((left, right))
                                }
                                FolderSyncAction::None => {}
                            }
                        }
                        Session::TextMerge(screen) => {
                            if matches!(
                                screen.ui(ctx, ui, &mut self.icons, &mut dialogs),
                                TextMergeAction::Home
                            ) {
                                self.active = None;
                            }
                        }
                        Session::FolderMerge(screen) => {
                            match screen.ui(ctx, ui, &mut self.icons, &mut dialogs) {
                                FolderMergeUiAction::Home => self.active = None,
                                FolderMergeUiAction::OpenTextMerge {
                                    left,
                                    center,
                                    right,
                                    output,
                                } => {
                                    open_merge_files = Some((left, center, right, output));
                                }
                                FolderMergeUiAction::None => {}
                            }
                        }
                        Session::Version(screen) | Session::Properties(screen) => {
                            if screen.ui(ctx, ui, &mut self.icons, &mut dialogs) {
                                self.active = None;
                            }
                        }
                        Session::Picture(screen) => {
                            if screen.ui(ctx, ui, &mut self.icons, &mut dialogs) {
                                self.active = None;
                            }
                        }
                        Session::Table(screen) => {
                            if screen.ui(ctx, ui, &mut self.icons, &mut dialogs) {
                                self.active = None;
                            }
                        }
                        Session::Hex(screen) => {
                            if screen.ui(ctx, ui, &mut self.icons, &mut dialogs) {
                                self.active = None;
                            }
                        }
                    });
                } else {
                    home_action = self.home.ui(ctx, ui, &mut self.icons, &mut dialogs);
                }
            });
        self.paint_drop_overlay(ctx, frame);
        if let Some(category) = ui::options_dialog::take_request(ctx) {
            let current = self.current_preferences();
            self.options_dialog.open(category, current);
        }
        if ui::session_settings::take_request(ctx) {
            if let Some(rules) = self.active_session_rules() {
                self.session_settings.open(rules);
            }
        }
        if let Some(prefs) = self.options_dialog.show(ctx, &mut self.icons, &mut dialogs) {
            self.apply_preferences(&prefs);
        }
        if let Some(rules) = self.session_settings.show(ctx, &mut self.icons) {
            self.apply_session_rules(rules);
        }
        if let Some(action) = ui::session_tabs::take_action(ctx) {
            ctx.memory_mut(|memory| memory.close_popup());
            match action {
                ui::session_tabs::Action::Activate(index) => self.active = Some(index),
                ui::session_tabs::Action::Close(index) => self.pending_close = Some(index),
                ui::session_tabs::Action::CloseHome => {
                    self.active = self.sessions.len().checked_sub(1)
                }
            }
        }
        match home_action {
            HomeAction::OpenTextCompare => self.open_session(SessionKind::Text),
            HomeAction::OpenFolderCompare => self.open_session(SessionKind::Folder),
            HomeAction::OpenFolderSync => self.open_session(SessionKind::FolderSync),
            HomeAction::OpenTextMerge => self.open_session(SessionKind::TextMerge),
            HomeAction::OpenFolderMerge => self.open_session(SessionKind::FolderMerge),
            HomeAction::OpenHexCompare => self.open_session(SessionKind::Hex),
            HomeAction::OpenTableCompare => self.open_session(SessionKind::Table),
            HomeAction::OpenVersionCompare => self.open_session(SessionKind::Version),
            HomeAction::OpenRegistryCompare => self.open_session(SessionKind::Registry),
            HomeAction::OpenMediaCompare => self.open_session(SessionKind::Media),
            HomeAction::OpenPictureCompare => self.open_session(SessionKind::Picture),
            HomeAction::OpenTextEdit => self.open_session(SessionKind::TextEdit),
            HomeAction::OpenRecentSession(record) => self.open_pair(record),
            HomeAction::ForgetRecentSession(record) => {
                // The "-" button of the sessions panel; never during a capture run.
                if self.capture.is_none() && self.history_valid && self.history.forget(&record) {
                    self.home.recent = self.history.entries.clone();
                    if let Err(error) = self.history.save() {
                        self.home.notice = Some(error);
                    }
                }
            }
            HomeAction::None => {}
        }
        if let Some((left, right)) = open_files {
            self.open_pair(RecentSession::new(
                SessionKind::for_paths(&left, &right),
                left,
                right,
            ));
        }
        if let Some((left, center, right, output)) = open_merge_files {
            self.open_pair(RecentSession::new_merge(
                SessionKind::TextMerge,
                left,
                center,
                right,
                output,
            ));
        }
        if let Some(request) = ui::session_menu::take(ctx) {
            if matches!(request, ui::session_menu::Request::ManageWorkspaces) {
                let mut rows: Vec<(String, String)> = self
                    .sessions
                    .iter()
                    .enumerate()
                    .map(|(idx, s)| {
                        let summary = s
                            .record()
                            .map(|r| {
                                format!("{} <-> {}", r.left.display(), r.right.display())
                            })
                            .unwrap_or_else(|| s.title());
                        (format!("#{}", idx + 1), summary)
                    })
                    .collect();
                if rows.is_empty() {
                    rows.push((
                        crate::i18n::tr("session.home"),
                        crate::i18n::tr("app.no_session_with_two_paths_is_open"),
                    ));
                }
                ui::info_modals::open_info(
                    ctx,
                    crate::i18n::tr("menu.session.manage_workspaces")
                        .trim_end_matches("..."),
                    rows,
                );
            } else {
                self.session_request(request, &mut dialogs);
            }
        }
        if self.capture.is_none() && self.history_valid {
            let mut history_changed = false;
            let query = match self.active.map(|index| &mut self.sessions[index]) {
                Some(Session::Text(screen)) => screen.take_recent_search(),
                Some(Session::TextEdit(screen)) => screen.take_recent_search(),
                _ => None,
            };
            if let Some(query) = query {
                history_changed |= self.history.remember_search(query);
            }
            if let Some(record) = self.active.and_then(|i| self.sessions[i].record()) {
                history_changed |= self.history.remember(record);
            }
            if history_changed {
                self.home.recent = self.history.entries.clone();
                if let Err(error) = self.history.save() {
                    self.home.notice = Some(error);
                }
            }
        }

        if self.pending_exit {
            self.pending_exit = false;
            if self
                .sessions
                .iter_mut()
                .all(|session| session.confirm_saved(&mut dialogs))
            {
                self.allow_exit = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        } else if let Some(index) = self.pending_close.take() {
            if self.sessions[index].confirm_saved(&mut dialogs) {
                let saved = if let Session::TextEdit(screen) = &mut self.sessions[index] {
                    screen.take_saved_source()
                } else {
                    None
                };
                self.remove_session(index);
                if let Some(path) = saved {
                    self.refresh_property_sources(&path, &mut dialogs);
                }
            }
        }

        ui::info_modals::show(ctx, &mut self.icons);

        // Developer capture uses the application's renderer, never OS input automation.
        if let Some(path) = &self.capture {
            self.capture_frames += 1;
            let screenshot = ctx.input(|input| {
                input.events.iter().find_map(|event| match event {
                    egui::Event::Screenshot { image, .. } => Some(image.clone()),
                    _ => None,
                })
            });
            if let Some(screenshot) = screenshot {
                let rgba: Vec<u8> = screenshot
                    .pixels
                    .iter()
                    .flat_map(|p| p.to_array())
                    .collect();
                if let Some(parent) = path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                match image::save_buffer(
                    path,
                    &rgba,
                    screenshot.width() as u32,
                    screenshot.height() as u32,
                    image::ColorType::Rgba8,
                ) {
                    Ok(()) => println!("Saved application capture to {}", path.display()),
                    Err(error) => {
                        eprintln!("Capture failed: {error}");
                        std::process::exit(1);
                    }
                }
                self.allow_exit = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            } else if self.capture_frames > 240 {
                eprintln!("Application capture timed out");
                std::process::exit(1);
            } else if self.capture_frames
                >= if self.capture_submenu.is_some() {
                    12
                } else if matches!(
                    self.capture_menu.as_deref(),
                    Some("table.edit_cell" | "Editar célula")
                ) {
                    10
                } else {
                    8
                }
            {
                for label in [&self.capture_menu, &self.capture_submenu]
                    .into_iter()
                    .flatten()
                {
                    let found = ctx.data(|data| {
                        data.get_temp::<egui::Rect>(egui::Id::new("capture_menu_rect").with(label))
                            .is_some()
                    });
                    if !found {
                        eprintln!("Capture menu target was not rendered: {label}");
                        std::process::exit(1);
                    }
                }
                let waiting = self.active.is_some_and(|index| {
                    matches!(&self.sessions[index], Session::Folder(screen) if screen.is_scanning())
                });
                if !waiting {
                    if let Some(layout_path) = &self.dump_layout {
                        let snap = ui::audit::snapshot(ctx);
                        if let Some(parent) = layout_path.parent() {
                            let _ = std::fs::create_dir_all(parent);
                        }
                        if let Ok(json) = serde_json::to_string_pretty(&snap) {
                            let _ = std::fs::write(layout_path, json);
                        }
                    }
                    ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot);
                }
            }
            ctx.request_repaint_after(std::time::Duration::from_millis(25));
        }
    }
}

/// Choose which side of a 2-pane session receives a single dropped item:
/// always fill a missing side first (never overwriting the only loaded side),
/// and use the drop cursor side when both sides are empty or both are filled.
fn choose_drop_side(
    left_empty: bool,
    right_empty: bool,
    target_left: Option<bool>,
    active_left: bool,
) -> bool {
    match (left_empty, right_empty) {
        (true, false) => true,
        (false, true) => false,
        (true, true) => target_left.unwrap_or(true),
        (false, false) => target_left.unwrap_or(active_left),
    }
}

/// Real client-area cursor coordinates in egui logical points and `(x < width / 2)`.
fn native_cursor_location(
    ctx: &egui::Context,
    frame: &eframe::Frame,
) -> Option<(egui::Pos2, bool)> {
    #[cfg(windows)]
    {
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        use windows_sys::Win32::Foundation::{POINT, RECT};
        use windows_sys::Win32::Graphics::Gdi::ScreenToClient;
        use windows_sys::Win32::UI::WindowsAndMessaging::{GetClientRect, GetCursorPos};

        let handle = frame.window_handle().ok()?;
        let RawWindowHandle::Win32(win32) = handle.as_raw() else {
            return None;
        };
        let hwnd = win32.hwnd.get() as *mut std::ffi::c_void;
        let mut pt = POINT { x: 0, y: 0 };
        let mut rect = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        // SAFETY: Win32 reads cursor screen coordinates and converts them to `hwnd` client coords.
        let ok = unsafe {
            GetCursorPos(&mut pt) != 0
                && ScreenToClient(hwnd, &mut pt) != 0
                && GetClientRect(hwnd, &mut rect) != 0
        };
        if ok
            && rect.right > 0
            && rect.bottom > 0
            && pt.x >= 0
            && pt.y >= 0
            && pt.x <= rect.right
            && pt.y <= rect.bottom
        {
            let scale = ctx.pixels_per_point().max(0.5);
            let pos = egui::pos2(pt.x as f32 / scale, pt.y as f32 / scale);
            return Some((pos, pt.x < rect.right / 2));
        }
    }
    #[cfg(not(windows))]
    let _ = (ctx, frame);
    None
}

/// Windows "lines to scroll per wheel notch" (SPI_GETWHEELSCROLLLINES), 3 when
/// unavailable or set to page scrolling.
fn wheel_scroll_lines() -> u32 {
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            SystemParametersInfoW, SPI_GETWHEELSCROLLLINES,
        };
        let mut lines: u32 = 3;
        // SAFETY: SPI_GETWHEELSCROLLLINES writes one UINT into `lines`.
        let ok = unsafe {
            SystemParametersInfoW(
                SPI_GETWHEELSCROLLLINES,
                0,
                (&mut lines as *mut u32).cast(),
                0,
            )
        };
        if ok != 0 && (1..=100).contains(&lines) {
            return lines;
        }
    }
    3
}

fn main() -> eframe::Result<()> {
    if std::env::args_os().any(|arg| arg == "--verify-resources") {
        let report = format!("{}\n", resources::summary());
        let _ = std::io::Write::write_all(&mut std::io::stdout(), report.as_bytes());
        return Ok(());
    }
    let startup = Startup::from_args();
    i18n::initialize(startup.language.as_deref());
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("AnyCompare")
        .with_inner_size(startup.size.unwrap_or([1150.0, 720.0]))
        .with_min_inner_size([700.0, 450.0]);
    if let Ok(icon) =
        image::load_from_memory(include_bytes!("../assets/icons/app.png"))
    {
        let icon = icon.to_rgba8();
        viewport = viewport.with_icon(egui::IconData {
            width: icon.width(),
            height: icon.height(),
            rgba: icon.into_raw(),
        });
    }
    let native_options = eframe::NativeOptions {
        viewport,
        // Present as soon as a frame is ready: the desktop compositor (DWM) already
        // prevents tearing in a window, and waiting for vertical sync added up to a
        // whole refresh interval to every scroll step and keystroke.
        vsync: false,
        // eframe dithers by default, scattering 239/240 pixels over flat colours;
        // the original paints exact colours.
        dithering: false,
        // OpenGL measured faster than wgpu on this machine (13/11/2026: 12 vs 17 ms
        // per wheel step). wgpu stays built in for other systems (DirectX 12 or
        // Vulkan, Vulkan on Linux, Metal on macOS): ANYCOMPARE_RENDERER=wgpu.
        renderer: if std::env::var_os("ANYCOMPARE_RENDERER").as_deref()
            == Some(std::ffi::OsStr::new("wgpu"))
        {
            eframe::Renderer::Wgpu
        } else {
            eframe::Renderer::Glow
        },
        ..Default::default()
    };
    eframe::run_native(
        "AnyCompare",
        native_options,
        Box::new(move |cc| Ok(Box::new(AnyCompareApp::new(cc, startup)))),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_text_editor_is_a_single_file_tab_reuses_dirty_buffer_and_restores_workspace() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/registry");
        let a = root.join("left.reg");
        let b = root.join("right.reg");
        let mut app = drop_app();
        app.open_pair(RecentSession::new(SessionKind::Registry, a.clone(), b));
        let record = app.sessions[0].record();
        let mut dialogs = ui::dialogs::TestDialogs::default();
        app.session_request(ui::session_menu::Request::EditText(a.clone()), &mut dialogs);
        assert_eq!(app.sessions.len(), 2);
        assert_eq!(app.active, Some(1));
        let Session::TextEdit(editor) = &mut app.sessions[1] else {
            panic!("not the independent editor");
        };
        assert_eq!(editor.document.path.as_ref(), Some(&a));
        assert!(!editor.document.dirty());
        editor
            .document
            .text
            .push_str("\n; preserve unsaved buffer\n");
        app.session_request(ui::session_menu::Request::EditText(a.clone()), &mut dialogs);
        assert_eq!(app.sessions.len(), 2);
        assert!(app.sessions[1].dirty());
        assert_eq!(app.sessions[0].record(), record);
        let saved = app.sessions[1].saved().unwrap();
        assert_eq!(saved.files.kind, SessionKind::TextEdit);
        assert!(!saved.files.label().contains("<-->"));
        assert!(matches!(
            Session::from_saved(&saved).unwrap(),
            Session::TextEdit(_)
        ));
        assert!(dialogs.opened.is_empty());
        assert!(dialogs.errors.is_empty());
    }

    #[test]
    fn identical_file_names_share_a_compact_tab_title() {
        let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        let mut screen = TextCompareScreen::new();
        screen.load_files(
            &fixtures.join("left/example.txt").to_string_lossy(),
            &fixtures.join("right/example.txt").to_string_lossy(),
        );
        let mut session = Session::Text(Box::new(screen));
        assert_eq!(session.title(), "example.txt");
        if let Session::Text(screen) = &mut session {
            screen.left.text.push('x');
        }
        assert_eq!(session.title(), "* example.txt");
    }

    #[test]
    fn closing_the_active_tab_selects_its_neighbour() {
        let mut app = drop_app();
        app.open_session(SessionKind::Folder);
        app.open_session(SessionKind::Text);
        app.open_session(SessionKind::Hex);
        app.active = Some(1);
        app.remove_session(1);
        assert_eq!(app.active, Some(1));
        assert!(matches!(app.sessions[1], Session::Hex(_)));
        app.remove_session(1);
        assert_eq!(app.active, Some(0));
        app.remove_session(0);
        assert_eq!(app.active, None);
    }

    fn drop_app() -> AnyCompareApp {
        AnyCompareApp {
            home: HomeScreen::new(),
            sessions: Vec::new(),
            session_paths: Vec::new(),
            active: None,
            icons: IconManager::new(),
            history: History::default(),
            history_valid: false,
            pending_close: None,
            pending_exit: false,
            allow_exit: false,
            capture: None,
            dump_layout: None,
            capture_frames: 0,
            capture_menu: None,
            capture_submenu: None,
            capture_drop: Vec::new(),
            capture_drop_separate: false,
            window_title: String::new(),
            options_dialog: ui::options_dialog::OptionsDialog::default(),
            session_settings: ui::session_settings::SessionSettingsDialog::default(),
        }
    }

    #[test]
    fn table_file_drops_select_the_table_module_and_keep_other_buffers() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/tables");
        let mut app = drop_app();
        let mut dialogs = ui::dialogs::TestDialogs::default();
        app.open_session(SessionKind::Text);
        if let Session::Text(screen) = &mut app.sessions[0] {
            screen.left.text = "unsaved".into();
        }
        let pair = [root.join("left.csv"), root.join("right.xlsx")];
        app.handle_dropped_paths(&pair, None, &mut dialogs);
        assert!(matches!(app.sessions[1], Session::Table(_)));
        assert_eq!(app.sessions[1].record().unwrap().kind, SessionKind::Table);
        app.handle_dropped_paths(&pair, None, &mut dialogs);
        assert_eq!(app.sessions.len(), 2);
        app.open_session(SessionKind::Table);
        app.handle_dropped_paths(&[root.join("left.ods")], Some(true), &mut dialogs);
        app.handle_dropped_paths(&[root.join("right.ods")], Some(false), &mut dialogs);
        assert_eq!(app.sessions.len(), 3);
        assert!(app.sessions[2].record().is_some());
        assert!(app.sessions[0].dirty());
        assert!(dialogs.errors.is_empty());
        // Explicit Text and Hex commands must retain the user's chosen module.
        app.open_pair(RecentSession::new(
            SessionKind::Text,
            root.join("left.csv"),
            root.join("right.csv"),
        ));
        assert!(matches!(
            app.sessions[app.active.unwrap()],
            Session::Text(_)
        ));
    }
    #[test]
    fn legacy_csv_history_opens_tables_without_replacing_a_dirty_text_tab() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/tables");
        let record:RecentSession=serde_json::from_value(serde_json::json!({"kind":"Text","left":root.join("left.csv"),"right":root.join("right.csv"),"saved":10})).unwrap();
        let mut app = drop_app();
        app.open_session(SessionKind::Text);
        if let Session::Text(screen) = &mut app.sessions[0] {
            screen.left.text = "unsaved editor".into();
        }
        app.open_pair(record.clone());
        assert!(matches!(app.sessions[1], Session::Table(_)));
        assert!(app.sessions[0].dirty());
        app.open_pair(record);
        assert_eq!(app.sessions.len(), 2);
        assert_eq!(app.active, Some(1));
    }
    #[test]
    fn pe_files_open_in_versions_and_explicit_hex_stays_hex() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let out = root.join(".codex/version-routing-tests");
        std::fs::create_dir_all(&out).unwrap();
        let a = out.join("a.dll");
        let b = out.join("b.exe");
        std::fs::copy(root.join("tests/fixtures/pe/pe32.bin"), &a).unwrap();
        std::fs::copy(root.join("tests/fixtures/pe/pe64.bin"), &b).unwrap();
        assert_eq!(SessionKind::for_paths(&a, &b), SessionKind::Version);
        let mut app = drop_app();
        let mut dialogs = ui::dialogs::TestDialogs::default();
        app.handle_dropped_paths(&[a.clone(), b.clone()], None, &mut dialogs);
        assert!(matches!(app.sessions[0], Session::Version(_)));
        app.open_pair(RecentSession::new(SessionKind::Hex, a.clone(), b));
        assert!(matches!(app.sessions[app.active.unwrap()], Session::Hex(_)));
        app.open_session(SessionKind::Version);
        app.handle_dropped_paths(std::slice::from_ref(&a), Some(true), &mut dialogs);
        app.handle_dropped_paths(std::slice::from_ref(&a), Some(false), &mut dialogs);
        assert!(app.sessions[app.active.unwrap()].record().is_some());
        assert!(dialogs.errors.is_empty());
    }

    fn assert_folder_result(
        app: &mut AnyCompareApp,
        left: &std::path::Path,
        right: &std::path::Path,
        same: bool,
    ) {
        let Session::Folder(screen) = &mut app.sessions[app.active.unwrap()] else {
            panic!("directory drops must open Folder Compare");
        };
        assert_eq!(PathBuf::from(&screen.left_path), left);
        assert_eq!(PathBuf::from(&screen.right_path), right);
        assert_eq!(
            screen.comparison_results(),
            vec![(
                "example.txt".into(),
                if same { "same" } else { "different" }
            )]
        );
    }

    #[test]
    fn two_directories_dropped_together_compare_their_contents_and_reuse_the_active_folder() {
        let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        let (left, right) = (fixtures.join("left"), fixtures.join("right"));
        let mut app = drop_app();
        let mut dialogs = ui::dialogs::TestDialogs::default();
        app.handle_dropped_paths(&[left.clone(), right.clone()], None, &mut dialogs);
        assert_eq!(app.sessions.len(), 1);
        assert_folder_result(&mut app, &left, &right, false);
        app.handle_dropped_paths(&[left.clone(), left.clone()], None, &mut dialogs);
        assert_eq!(app.sessions.len(), 1);
        assert_folder_result(&mut app, &left, &left, true);
        assert!(dialogs.errors.is_empty());
    }

    #[test]
    fn directories_dropped_in_separate_frames_fill_the_missing_side_and_keep_unsaved_text() {
        let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        let (left, right) = (fixtures.join("left"), fixtures.join("right"));
        let mut app = drop_app();
        app.open_session(SessionKind::Text);
        if let Session::Text(screen) = &mut app.sessions[0] {
            screen.left.text = "unsaved translation".into();
        }
        let mut dialogs = ui::dialogs::TestDialogs::default();
        app.handle_dropped_paths(std::slice::from_ref(&left), None, &mut dialogs);
        let folder = app.active;
        assert_eq!(app.sessions.len(), 2);
        app.handle_dropped_paths(std::slice::from_ref(&right), None, &mut dialogs);
        assert_eq!(app.active, folder);
        assert_eq!(app.sessions.len(), 2);
        assert_folder_result(&mut app, &left, &right, false);
        let Session::Text(screen) = &app.sessions[0] else {
            panic!("text session replaced");
        };
        assert_eq!(screen.left.text, "unsaved translation");
        assert!(screen.left.dirty());
        app.handle_dropped_paths(std::slice::from_ref(&left), Some(false), &mut dialogs);
        assert_folder_result(&mut app, &left, &left, true);
        app.handle_dropped_paths(std::slice::from_ref(&right), Some(true), &mut dialogs);
        assert_folder_result(&mut app, &right, &left, false);
    }

    #[test]
    fn raw_directory_drop_events_are_consumed_once_across_frames() {
        let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        let (left, right) = (fixtures.join("left"), fixtures.join("right"));
        for separate in [false, true] {
            let ctx = egui::Context::default();
            let mut app = drop_app();
            let mut dialogs = ui::dialogs::TestDialogs::default();
            let batches = if separate {
                vec![vec![left.clone()], vec![right.clone()]]
            } else {
                vec![vec![left.clone(), right.clone()]]
            };
            for paths in batches {
                let input = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(700.0, 450.0),
                    )),
                    events: vec![egui::Event::PointerMoved(egui::pos2(80.0, 80.0))],
                    dropped_files: paths
                        .into_iter()
                        .map(|path| egui::DroppedFile {
                            path: Some(path),
                            ..Default::default()
                        })
                        .collect(),
                    ..Default::default()
                };
                let _ = ctx.run(input, |ctx| {
                    app.handle_dropped_input(ctx, &mut dialogs);
                    assert!(ctx.input(|input| input.raw.dropped_files.is_empty()));
                    app.handle_dropped_input(ctx, &mut dialogs);
                });
            }
            assert_eq!(app.sessions.len(), 1);
            assert_folder_result(&mut app, &left, &right, false);
        }
    }

    #[test]
    fn invalid_directory_drop_does_not_replace_the_comparison() {
        let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        let left = fixtures.join("left");
        let right = fixtures.join("right");
        let mut app = drop_app();
        let mut dialogs = ui::dialogs::TestDialogs::default();
        app.handle_dropped_paths(&[left.clone(), right.clone()], None, &mut dialogs);
        app.handle_dropped_paths(
            &[left.clone(), right.clone(), left.clone()],
            None,
            &mut dialogs,
        );
        app.handle_dropped_paths(
            &[left.clone(), right.join("example.txt")],
            None,
            &mut dialogs,
        );
        assert_eq!(app.sessions.len(), 1);
        assert_eq!(dialogs.errors.len(), 2);
        assert_folder_result(&mut app, &left, &right, false);
    }

    #[test]
    fn text_files_dropped_one_by_one_fill_right_side_without_overwriting_left() {
        let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        let left = fixtures.join("left/example.txt");
        let right = fixtures.join("right/example.txt");
        let mut app = drop_app();
        let mut dialogs = ui::dialogs::TestDialogs::default();

        // Drop first file -> opens Text Compare on the left.
        app.handle_dropped_paths(std::slice::from_ref(&left), Some(true), &mut dialogs);
        assert_eq!(app.sessions.len(), 1);
        let Session::Text(screen) = &app.sessions[0] else {
            panic!("first text drop must open Text Compare");
        };
        assert_eq!(screen.left.path.as_deref(), Some(left.as_path()));
        assert!(screen.right.path.is_none());

        // Drop second file even when cursor hint says left -> must fill the empty right side
        // in the SAME tab instead of overwriting the left side or creating a second tab!
        app.handle_dropped_paths(std::slice::from_ref(&right), Some(true), &mut dialogs);
        assert_eq!(app.sessions.len(), 1);
        let Session::Text(screen) = &app.sessions[0] else {
            panic!("second text drop must stay in Text Compare");
        };
        assert_eq!(screen.left.path.as_deref(), Some(left.as_path()));
        assert_eq!(screen.right.path.as_deref(), Some(right.as_path()));
        assert!(screen.diff_result.diff_count > 0);

        // With both sides filled, dropping on the right replaces ONLY right; on the left ONLY left.
        app.handle_dropped_paths(std::slice::from_ref(&left), Some(false), &mut dialogs);
        let Session::Text(screen) = &app.sessions[0] else {
            unreachable!();
        };
        assert_eq!(screen.left.path.as_deref(), Some(left.as_path()));
        assert_eq!(screen.right.path.as_deref(), Some(left.as_path()));
        assert_eq!(screen.diff_result.diff_count, 0);

        app.handle_dropped_paths(std::slice::from_ref(&right), Some(true), &mut dialogs);
        let Session::Text(screen) = &app.sessions[0] else {
            unreachable!();
        };
        assert_eq!(screen.left.path.as_deref(), Some(right.as_path()));
        assert_eq!(screen.right.path.as_deref(), Some(left.as_path()));
        assert!(screen.diff_result.diff_count > 0);
        assert!(dialogs.errors.is_empty());
    }

    #[test]
    fn two_text_files_dropped_together_compare_and_reuse_clean_tab() {
        let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        let left = fixtures.join("left/example.txt");
        let right = fixtures.join("right/example.txt");
        let mut app = drop_app();
        let mut dialogs = ui::dialogs::TestDialogs::default();

        app.handle_dropped_paths(&[left.clone(), right.clone()], None, &mut dialogs);
        assert_eq!(app.sessions.len(), 1);
        assert!(matches!(app.sessions[0], Session::Text(_)));

        // Dropping a new pair onto a clean active Text tab reuses it.
        app.handle_dropped_paths(&[right.clone(), left.clone()], None, &mut dialogs);
        assert_eq!(app.sessions.len(), 1);
        let Session::Text(screen) = &app.sessions[0] else {
            unreachable!();
        };
        assert_eq!(screen.left.path.as_deref(), Some(right.as_path()));
        assert_eq!(screen.right.path.as_deref(), Some(left.as_path()));

        // Dirty active Text tab is preserved when a new pair is dropped.
        if let Session::Text(screen) = &mut app.sessions[0] {
            screen.left.text.push_str("\nunsaved");
        }
        app.handle_dropped_paths(&[left.clone(), right.clone()], None, &mut dialogs);
        assert_eq!(app.sessions.len(), 2);
        assert!(app.sessions[0].dirty());
        assert!(dialogs.errors.is_empty());
    }

    #[test]
    fn binary_and_hex_drops_route_to_hex_and_preserve_hex_session() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let bin_a = root.join("tests/fixtures/pe/pe32.bin");
        let bin_b = root.join("tests/fixtures/pe/pe64.bin");
        let txt_a = root.join("tests/fixtures/left/example.txt");
        let txt_b = root.join("tests/fixtures/right/example.txt");
        let mut app = drop_app();
        let mut dialogs = ui::dialogs::TestDialogs::default();

        // Single binary file auto-routes to Hex, and second binary file fills right side.
        app.handle_dropped_paths(std::slice::from_ref(&bin_a), Some(true), &mut dialogs);
        assert_eq!(app.sessions.len(), 1);
        assert!(matches!(app.sessions[0], Session::Hex(_)));
        app.handle_dropped_paths(std::slice::from_ref(&bin_b), Some(true), &mut dialogs);
        assert_eq!(app.sessions.len(), 1);
        let Session::Hex(screen) = &app.sessions[0] else {
            unreachable!();
        };
        assert_eq!(PathBuf::from(&screen.left_path), bin_a);
        assert_eq!(PathBuf::from(&screen.right_path), bin_b);

        // Dropping two text files onto an active clean Hex tab stays in Hex.
        app.handle_dropped_paths(&[txt_a.clone(), txt_b.clone()], None, &mut dialogs);
        assert_eq!(app.sessions.len(), 1);
        assert!(matches!(app.sessions[0], Session::Hex(_)));
        assert!(dialogs.errors.is_empty());
    }

    #[test]
    fn home_icon_drop_targets_respect_chosen_session_kind() {
        let ctx = egui::Context::default();
        ui::theme::apply(&ctx);
        let mut app = drop_app();
        let mut dialogs = ui::dialogs::TestDialogs::default();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1150.0, 720.0),
            )),
            ..Default::default()
        };
        let _ = ctx.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                app.home.ui(ctx, ui, &mut app.icons, &mut dialogs);
            });
        });

        let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        let left = fixtures.join("left/example.txt");
        let right = fixtures.join("right/example.txt");

        let hex_pos = app
            .home
            .drop_target_rect(SessionKind::Hex)
            .expect("Home Hex icon rect")
            .center();
        app.handle_dropped_paths_at(
            &[left.clone(), right.clone()],
            None,
            Some(hex_pos),
            &mut dialogs,
        );
        assert!(matches!(app.sessions[0], Session::Hex(_)));

        app.active = None;
        let edit_pos = app
            .home
            .drop_target_rect(SessionKind::TextEdit)
            .expect("Home TextEdit icon rect")
            .center();
        app.handle_dropped_paths_at(
            std::slice::from_ref(&left),
            None,
            Some(edit_pos),
            &mut dialogs,
        );
        assert!(matches!(app.sessions[1], Session::TextEdit(_)));
        assert!(dialogs.errors.is_empty());
    }

    #[test]
    fn picture_files_drop_together_and_one_by_one_into_picture_compare() {
        let icons = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/home_icons/100/HomeImages");
        let a = icons.join("imgHomePixCompareSession.png");
        let b = icons.join("imgHomeTextCompareSession.png");
        assert_eq!(SessionKind::for_paths(&a, &b), SessionKind::Picture);

        let mut app = drop_app();
        let mut dialogs = ui::dialogs::TestDialogs::default();
        // Drop two images together -> opens Picture Compare.
        app.handle_dropped_paths(&[a.clone(), b.clone()], None, &mut dialogs);
        assert_eq!(app.sessions.len(), 1);
        assert!(matches!(app.sessions[0], Session::Picture(_)));
        assert_eq!(app.sessions[0].record().unwrap().kind, SessionKind::Picture);

        // Open fresh Picture Compare and drop one image at a time (even with left hint) -> fills right!
        app.open_session(SessionKind::Picture);
        app.handle_dropped_paths(std::slice::from_ref(&a), Some(true), &mut dialogs);
        app.handle_dropped_paths(std::slice::from_ref(&b), Some(true), &mut dialogs);
        assert_eq!(app.sessions.len(), 2);
        let Session::Picture(screen) = &app.sessions[1] else {
            unreachable!();
        };
        assert_eq!(PathBuf::from(&screen.left_path), a);
        assert_eq!(PathBuf::from(&screen.right_path), b);
        assert!(dialogs.errors.is_empty());
    }

    #[test]
    fn screens_render_at_supported_window_sizes() {
        for (size, scale) in [
            ([1150.0, 720.0], 1.0),
            ([700.0, 450.0], 1.0),
            ([1600.0, 1000.0], 1.0),
            ([960.0, 1000.0], 1.0),
            ([1150.0, 720.0], 1.25),
            ([1150.0, 720.0], 1.5),
            ([1150.0, 720.0], 2.0),
        ] {
            for kind in 0..7 {
                let ctx = egui::Context::default();
                ui::theme::apply(&ctx);
                ctx.set_pixels_per_point(scale);
                let mut home = HomeScreen::new();
                let mut text = TextCompareScreen::new();
                let mut folder_sync = FolderSyncScreen::new();
                let mut text_merge = TextMergeScreen::new();
                let mut folder_merge = FolderMergeScreen::new();
                let fixtures =
                    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
                if kind == 1 {
                    text.load_files(
                        &fixtures.join("left/example.txt").to_string_lossy(),
                        &fixtures.join("right/example.txt").to_string_lossy(),
                    );
                } else if kind == 4 {
                    folder_sync.left_path = fixtures.join("left").to_string_lossy().into_owned();
                    folder_sync.right_path = fixtures.join("right").to_string_lossy().into_owned();
                    let _ = folder_sync.scan_sync();
                } else if kind == 5 {
                    let _ = text_merge.load_files_silent(
                        &fixtures.join("left/example.txt"),
                        None,
                        &fixtures.join("right/example.txt"),
                        None,
                    );
                } else if kind == 6 {
                    folder_merge.left_path = fixtures.join("left").to_string_lossy().into_owned();
                    folder_merge.right_path = fixtures.join("right").to_string_lossy().into_owned();
                    let _ = folder_merge.scan_sync();
                }
                let mut folder = FolderCompareScreen::new();
                let mut hex = HexCompareScreen::new();
                let mut icons = IconManager::new();
                let mut dialogs = ui::dialogs::TestDialogs::default();
                let input = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size.into())),
                    ..Default::default()
                };
                let _ = ctx.run(input, |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| match kind {
                        0 => {
                            home.ui(ctx, ui, &mut icons, &mut dialogs);
                        }
                        1 => {
                            text.ui(ctx, ui, &mut icons, &mut dialogs);
                        }
                        2 => {
                            folder.ui(ctx, ui, &mut icons, &mut dialogs);
                        }
                        3 => {
                            hex.ui(ctx, ui, &mut icons, &mut dialogs);
                        }
                        4 => {
                            folder_sync.ui(ctx, ui, &mut icons, &mut dialogs);
                        }
                        5 => {
                            text_merge.ui(ctx, ui, &mut icons, &mut dialogs);
                        }
                        _ => {
                            folder_merge.ui(ctx, ui, &mut icons, &mut dialogs);
                        }
                    });
                });
            }
        }
    }

    #[test]
    fn manual_features_end_to_end() {
        let ctx = egui::Context::default();
        ui::theme::apply(&ctx);
        let mut icons = IconManager::new();
        let mut dialogs = ui::dialogs::TestDialogs::default();
        let fixtures =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");

        // 1. TextCompare: Over-Under layout, Ruler, Hex Details, Display Font zoom, Line Endings
        let mut text = TextCompareScreen::new();
        text.load_files(
            &fixtures.join("left/example.txt").to_string_lossy(),
            &fixtures.join("right/example.txt").to_string_lossy(),
        );
        assert!(!text.is_over_under());
        text.set_over_under(true);
        assert!(text.is_over_under());
        text.set_show_ruler(true);
        text.set_show_hex_details(true);
        text.set_display_font_size(15.0);
        assert_eq!(text.display_font_size(), 15.0);
        text.convert_line_endings(crate::document::LineEnding::CrLf);
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                text.ui(ctx, ui, &mut icons, &mut dialogs);
            });
        });
        text.set_display_font_size(13.0);

        // 2. TextEdit: Delete Line & Convert File transform
        let mut edit = ui::text_edit::TextEditScreen::new();
        edit.load(&fixtures.join("left/example.txt")).unwrap();
        let initial_lines = edit.document.text.lines().count();
        edit.delete_current_line(&ctx);
        assert_eq!(edit.document.text.lines().count(), initial_lines - 1);
        edit.transform_document(&ctx, |s| format!("    {s}"));
        assert!(edit.document.text.starts_with("    "));

        // 3. FolderCompare: Compare Files Only, Select Newer / Orphans, Full Refresh
        let tmp = std::env::temp_dir().join("anycompare_manual_features_test");
        let _ = std::fs::remove_dir_all(&tmp);
        let dir_l = tmp.join("left");
        let dir_r = tmp.join("right");
        std::fs::create_dir_all(&dir_l).unwrap();
        std::fs::create_dir_all(&dir_r).unwrap();
        std::fs::write(dir_l.join("orphan_left.txt"), "only left\n").unwrap();
        std::fs::write(dir_l.join("shared.txt"), "v1\n").unwrap();
        std::fs::write(dir_r.join("shared.txt"), "v2_longer\n").unwrap();

        let mut folder = FolderCompareScreen::new();
        folder.load_folders(&dir_l, &dir_r);
        let _ = folder.comparison_results();
        assert!(!folder.is_compare_files_only());
        folder.set_compare_files_only(true);
        assert!(folder.is_compare_files_only());
        folder.select_orphans(true, true);
        let orphan_sel = folder.selected_count();
        assert_eq!(orphan_sel, 1);
        folder.select_newer(true, true);
        folder.set_selected_as_base_side(false);
        folder.full_refresh();
        let _ = folder.comparison_results();

        // 4. HexCompare: Big Endian toggle & report
        let mut hex = HexCompareScreen::new();
        assert!(!hex.is_big_endian());
        hex.set_big_endian(true);
        assert!(hex.is_big_endian());

        // 5. PictureCompare: Reset Difference Offset & Blend Toggle
        let mut pic = ui::picture_compare::PictureCompareScreen::new();
        pic.set_difference_offset(12, -7);
        assert_eq!(pic.difference_offset(), (12, -7));
        pic.reset_difference_offset();
        assert_eq!(pic.difference_offset(), (0, 0));
        assert!(!pic.is_blend_mode());
        pic.toggle_blend();
        assert!(pic.is_blend_mode());
        pic.toggle_blend();
        assert!(!pic.is_blend_mode());

        // 6. TableCompare: Resize Columns to Fit & Next/Prev Diff Cell
        let mut table = ui::table_compare::TableCompareScreen::new();
        table
            .load_files(
                &fixtures.join("tables/left.csv"),
                &fixtures.join("tables/right.csv"),
            )
            .unwrap();
        table.resize_columns_to_fit();
        table.navigate_diff_cell(true);
        let _ = table.selected_pair_index();

        // 7. Version/Registry/Media: Item Is Important & Copy Key Name
        let reg_l = tmp.join("left.reg");
        let reg_r = tmp.join("right.reg");
        std::fs::write(
            &reg_l,
            "Windows Registry Editor Version 5.00\r\n\r\n[HKEY_CURRENT_USER\\Software\\AC]\r\n\"A\"=\"1\"\r\n",
        )
        .unwrap();
        std::fs::write(
            &reg_r,
            "Windows Registry Editor Version 5.00\r\n\r\n[HKEY_CURRENT_USER\\Software\\AC]\r\n\"A\"=\"2\"\r\n",
        )
        .unwrap();
        let mut reg = ui::version_compare::VersionCompareScreen::for_kind(
            crate::history::SessionKind::Registry,
        );
        reg.load_files(&reg_l, &reg_r).unwrap();
        let imp_before = reg.is_selected_important();
        reg.toggle_selected_important();
        assert_eq!(reg.is_selected_important(), !imp_before);
        reg.copy_key_name(&ctx);
        let _ = std::fs::remove_dir_all(&tmp);

        // 8. Info / Legend / About native Win11 modals
        ui::info_modals::open_legend(&ctx);
        assert_eq!(
            ui::info_modals::active(&ctx),
            Some(ui::info_modals::ActiveModal::Legend)
        );
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            ui::info_modals::show(ctx, &mut icons);
        });
        ui::info_modals::open_about(&ctx);
        assert_eq!(
            ui::info_modals::active(&ctx),
            Some(ui::info_modals::ActiveModal::About)
        );
        ui::info_modals::close(&ctx);
        assert_eq!(ui::info_modals::active(&ctx), None);
    }

    include!("all_formats_tests.rs");
}
