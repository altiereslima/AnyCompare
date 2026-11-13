#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod binary_diff;
mod diff;
mod document;
mod file_ops;
mod history;
mod html_tables;
mod i18n;
mod pe_version;
mod property_sources;
mod report;
mod resources;
mod search;
mod table;
mod table_columns;
mod table_sheets;
mod ui;
mod workspace;

use eframe::egui;
use history::{History, RecentSession, SessionKind};
use std::path::PathBuf;
use ui::{
    folder_compare::{FolderAction, FolderCompareScreen},
    hex_compare::HexCompareScreen,
    home::{HomeAction, HomeScreen},
    icons::IconManager,
    table_compare::TableCompareScreen,
    text_compare::{TextCompareAction, TextCompareScreen},
    text_edit::TextEditScreen,
    version_compare::VersionCompareScreen,
};

enum Session {
    Text(Box<TextCompareScreen>),
    Folder(Box<FolderCompareScreen>),
    Hex(Box<HexCompareScreen>),
    Table(Box<TableCompareScreen>),
    Version(Box<VersionCompareScreen>),
    Properties(Box<VersionCompareScreen>),
    TextEdit(Box<TextEditScreen>),
}

impl Session {
    fn saved(&self) -> Option<workspace::SavedSession> {
        Some(workspace::SavedSession {
            files: self.record()?,
            options: match self {
                Self::Text(s) => s.session_options(),
                Self::Folder(s) => s.session_options(),
                Self::Hex(s) => s.session_options(),
                Self::Table(s) => s.session_options(),
                Self::Version(s) | Self::Properties(s) => s.session_options(),
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
            SessionKind::Hex => Self::Hex(Box::new(HexCompareScreen::new())),
            SessionKind::Table => Self::Table(Box::new(TableCompareScreen::new())),
            SessionKind::TextEdit => Self::TextEdit(Box::new(TextEditScreen::new())),
            SessionKind::Version => Self::Version(Box::new(VersionCompareScreen::new())),
            SessionKind::Registry | SessionKind::Media => {
                Self::Properties(Box::new(VersionCompareScreen::for_kind(kind)))
            }
        }
    }

    fn dirty(&self) -> bool {
        match self {
            Self::Text(s) => s.left.dirty() || s.right.dirty(),
            Self::Hex(s) => s.dirty(),
            Self::Table(s) => s.dirty(),
            Self::TextEdit(s) => s.document.dirty(),
            Self::Folder(_) | Self::Version(_) | Self::Properties(_) => false,
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
            (Self::Table(s), SessionKind::Table) => s.is_empty(),
            (Self::Properties(s), kind) if s.kind() == kind => {
                s.left_path.is_empty() && s.right_path.is_empty()
            }
            (Self::Version(s), SessionKind::Version) => {
                s.left_path.is_empty() && s.right_path.is_empty()
            }
            (Self::Hex(s), SessionKind::Hex) => s.left_path.is_empty() && s.right_path.is_empty(),
            _ => false,
        }
    }

    fn record(&self) -> Option<RecentSession> {
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
            Self::Hex(s) if !s.left_path.is_empty() && !s.right_path.is_empty() => (
                SessionKind::Hex,
                PathBuf::from(&s.left_path),
                PathBuf::from(&s.right_path),
            ),
            _ => return None,
        };
        Some(RecentSession::new(kind, left, right))
    }

    fn title(&self) -> String {
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
            .unwrap_or_else(|| match self {
                Self::Text(_) => crate::i18n::tr("Comparar Textos"),
                Self::Folder(_) => crate::i18n::tr("Comparar Pastas"),
                Self::Hex(_) => crate::i18n::tr("Comparar Hexadecimal"),
                Self::Table(_) => crate::i18n::tr("Comparar Tabelas"),
                Self::TextEdit(_) => crate::i18n::tr("Editor de Texto"),
                Self::Version(_) => crate::i18n::tr("Comparar Versões"),
                Self::Properties(s) => crate::i18n::tr(if s.kind() == SessionKind::Media {
                    "Comparar Mídia"
                } else {
                    "Comparar Registro"
                }),
            });
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
    kind: Option<SessionKind>,
    capture: Option<PathBuf>,
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
    capture_sheet: Option<usize>,
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
                Some("--text") => options.kind = Some(SessionKind::Text),
                Some("--folder") => options.kind = Some(SessionKind::Folder),
                Some("--hex") => options.kind = Some(SessionKind::Hex),
                Some("--table") => options.kind = Some(SessionKind::Table),
                Some("--version") => options.kind = Some(SessionKind::Version),
                Some("--registry") => options.kind = Some(SessionKind::Registry),
                Some("--media") => options.kind = Some(SessionKind::Media),
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
                Some("--capture-sheet") => {
                    options.capture_sheet =
                        args.next().and_then(|s| s.to_string_lossy().parse().ok());
                }
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
        if paths.len() == 2 {
            options.pair = Some((paths.remove(0), paths.remove(0)));
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
    capture_frames: usize,
    capture_menu: Option<String>,
    capture_submenu: Option<String>,
    capture_drop: Vec<PathBuf>,
    capture_drop_separate: bool,
    /// Title last sent to the window. Every viewport command requests another
    /// frame, so sending it unconditionally repainted the app forever.
    window_title: String,
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
            capture_frames: 0,
            capture_menu: startup.capture_menu,
            capture_submenu: startup.capture_submenu,
            capture_drop: startup.capture_drop,
            capture_drop_separate: startup.capture_drop_separate,
            window_title: String::new(),
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
            app.open_pair(RecentSession::new(kind, left, right));
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
        if app.capture.is_some() {
            if let (Some(index), Some(Session::Table(screen))) = (
                startup.capture_sheet,
                app.active.map(|i| &mut app.sessions[i]),
            ) {
                if let Err(error) = screen.capture_sheet(index) {
                    screen.error = Some(error);
                }
            }
        }
        app
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
                if screen.left.path.is_none() {
                    // A folder result can be binary or use an unsupported text encoding.
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
            Session::Table(screen) => {
                if let Err(error) = screen.load_files(&record.left, &record.right) {
                    screen.error = Some(error);
                }
            }
            Session::Folder(screen) => screen.load_folders(&record.left, &record.right),
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

    fn handle_dropped_input(
        &mut self,
        ctx: &egui::Context,
        dialogs: &mut dyn ui::dialogs::DialogHost,
    ) {
        let center_x = ctx.screen_rect().center().x;
        let (paths, side) = ctx.input_mut(|input| {
            let paths = std::mem::take(&mut input.raw.dropped_files)
                .into_iter()
                .filter_map(|file| file.path)
                .collect::<Vec<_>>();
            let side = input.pointer.latest_pos().map(|pos| pos.x < center_x);
            (paths, side)
        });
        if !paths.is_empty() {
            ctx.memory_mut(|memory| memory.close_popup());
        }
        self.handle_dropped_paths(&paths, side, dialogs);
    }

    fn handle_dropped_paths(
        &mut self,
        paths: &[PathBuf],
        target_left: Option<bool>,
        dialogs: &mut dyn ui::dialogs::DialogHost,
    ) {
        if paths.is_empty() {
            return;
        }
        if paths.len() > 2 {
            dialogs.error("Solte no máximo duas pastas ou dois arquivos para comparar.");
            return;
        }
        if let Some(path) = paths.iter().find(|path| !path.exists()) {
            dialogs.error(&format!(
                "{}\n{}",
                i18n::tr("Não foi possível acessar o item solto."),
                path.display()
            ));
            return;
        }
        if paths.iter().all(|path| path.is_dir()) {
            let active = self
                .active
                .filter(|&index| matches!(self.sessions[index], Session::Folder(_)));
            let reusable = active.or_else(|| self.sessions.iter().position(|session| {
                matches!(session, Session::Folder(screen) if screen.left_path.is_empty() || screen.right_path.is_empty())
            }));
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
            dialogs.error("Compare duas pastas ou dois arquivos; não misture os dois tipos.");
        } else if let [left, right] = paths {
            self.open_pair(RecentSession::new(
                SessionKind::for_paths(left, right),
                left.clone(),
                right.clone(),
            ));
        } else if let Some(path) = paths.first() {
            let property_kind = SessionKind::for_paths(path, path);
            if matches!(property_kind, SessionKind::Registry | SessionKind::Media) {
                let active=self.active.filter(|&i|matches!(&self.sessions[i],Session::Properties(s) if s.kind()==property_kind));
                if let Some(i) = active {
                    self.active = Some(i);
                } else {
                    self.open_session(property_kind);
                }
                if let Some(Session::Properties(screen)) =
                    self.active.map(|i| &mut self.sessions[i])
                {
                    let left = target_left.unwrap_or(screen.left_path.is_empty());
                    screen.open_side(left, path, dialogs);
                }
                return;
            }
            if pe_version::supported(path) {
                let active=self.active.filter(|&i|matches!(&self.sessions[i],Session::Version(s) if s.left_path.is_empty() || s.right_path.is_empty()));
                if let Some(i) = active {
                    self.active = Some(i);
                } else {
                    self.open_session(SessionKind::Version);
                }
                if let Some(Session::Version(screen)) = self.active.map(|i| &mut self.sessions[i]) {
                    let left = target_left.unwrap_or(screen.left_path.is_empty());
                    screen.open_side(left, path, dialogs);
                }
                return;
            }
            if table::supported(path) {
                let active = self.active.filter(|&i| matches!(&self.sessions[i], Session::Table(s) if s.left.path.is_empty() || s.right.path.is_empty()));
                if let Some(i) = active {
                    self.active = Some(i);
                } else {
                    self.open_session(SessionKind::Table);
                }
                if let Some(Session::Table(screen)) = self.active.map(|i| &mut self.sessions[i]) {
                    let side = target_left.unwrap_or(screen.left.path.is_empty());
                    screen.open_side(side, path, dialogs);
                }
                return;
            }
            self.open_session(SessionKind::Text);
            if let Some(Session::Text(screen)) = self.active.map(|index| &mut self.sessions[index])
            {
                screen.open_side(true, path.clone(), dialogs);
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
                            "Carregar Espaço de Trabalho"
                        } else {
                            "Abrir Sessão"
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
                            return Err(
                                "O arquivo contém um espaço de trabalho, não uma única sessão."
                                    .into(),
                            );
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
                        return Err(
                            "Abra os dois arquivos ou pastas antes de salvar a sessão.".into()
                        );
                    };
                    let current = self.session_paths[index].as_deref();
                    let path = if save_as || current.is_none() {
                        let name = current
                            .and_then(std::path::Path::file_name)
                            .map(|s| s.to_string_lossy().into_owned())
                            .unwrap_or_else(|| "comparison.acsession.json".into());
                        dialogs.named_file("Salvar Sessão Como...", true, current, &name)
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
                        return Err("Nenhuma sessão com dois caminhos foi aberta.".into());
                    }
                    if let Some(path) = dialogs.named_file(
                        "Salvar Espaço de Trabalho Como...",
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
        let cell_edit = self.capture_menu.as_deref() == Some("Editar célula");
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
                        button: if label == "Menu da célula"
                            || label.starts_with("Menu de propriedades")
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
        self.handle_dropped_input(ctx, &mut dialogs);

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
                || ctx.input_mut(|input| input.consume_key(egui::Modifiers::CTRL, egui::Key::Tab))
            {
                let current = self.active.unwrap_or(0);
                self.active = Some(if backwards {
                    (current + self.sessions.len() - 1) % self.sessions.len()
                } else {
                    (current + 1) % self.sessions.len()
                });
            }
        }
        let close = if ctx.input_mut(|input| input.consume_key(egui::Modifiers::CTRL, egui::Key::W))
        {
            self.active
        } else {
            None
        };
        let title = self
            .active
            .map(|i| self.sessions[i].title())
            .unwrap_or_else(|| "Página Inicial".into());
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
                        Session::Hex(_) => "imgHexCompareSession",
                        Session::Table(_) => "imgTableCompareSession",
                        Session::Version(_) => "imgVerCompareSession",
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
                        Session::Version(screen) | Session::Properties(screen) => {
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
            HomeAction::OpenHexCompare => self.open_session(SessionKind::Hex),
            HomeAction::OpenTableCompare => self.open_session(SessionKind::Table),
            HomeAction::OpenVersionCompare => self.open_session(SessionKind::Version),
            HomeAction::OpenRegistryCompare => self.open_session(SessionKind::Registry),
            HomeAction::OpenMediaCompare => self.open_session(SessionKind::Media),
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
        if let Some(request) = ui::session_menu::take(ctx) {
            self.session_request(request, &mut dialogs);
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
                } else if self.capture_menu.as_deref() == Some("Editar célula") {
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
                    ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot);
                }
            }
            ctx.request_repaint_after(std::time::Duration::from_millis(25));
        }
    }
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
        image::load_from_memory(include_bytes!("../assets/reference/ico/MAINICON.png"))
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
        let root =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("vendor/bc5-prototypes/TESTES_REGISTRY");
        let a = root.join("02_diferencas_esquerda.reg");
        let b = root.join("02_diferencas_direita.reg");
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
            capture_frames: 0,
            capture_menu: None,
            capture_submenu: None,
            capture_drop: Vec::new(),
            capture_drop_separate: false,
            window_title: String::new(),
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
        std::fs::copy(root.join("tests/fixtures/bc5/pe32.bin"), &a).unwrap();
        std::fs::copy(root.join("tests/fixtures/bc5/pe64.bin"), &b).unwrap();
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
            for kind in 0..4 {
                let ctx = egui::Context::default();
                ui::theme::apply(&ctx);
                ctx.set_pixels_per_point(scale);
                let mut home = HomeScreen::new();
                let mut text = TextCompareScreen::new();
                let fixtures =
                    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
                if kind == 1 {
                    text.load_files(
                        &fixtures.join("left/example.txt").to_string_lossy(),
                        &fixtures.join("right/example.txt").to_string_lossy(),
                    );
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
                        _ => {
                            hex.ui(ctx, ui, &mut icons, &mut dialogs);
                        }
                    });
                });
            }
        }
    }
}
