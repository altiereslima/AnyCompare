#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod binary_diff;
mod diff;
mod document;
mod file_ops;
mod history;
mod i18n;
mod report;
mod search;
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
    text_compare::{TextCompareAction, TextCompareScreen},
};

enum Session {
    Text(Box<TextCompareScreen>),
    Folder(Box<FolderCompareScreen>),
    Hex(Box<HexCompareScreen>),
}

impl Session {
    fn saved(&self) -> Option<workspace::SavedSession> {
        Some(workspace::SavedSession {
            files: self.record()?,
            options: match self {
                Self::Text(s) => s.session_options(),
                Self::Folder(s) => s.session_options(),
                Self::Hex(s) => s.session_options(),
            },
        })
    }
    fn from_saved(saved: &workspace::SavedSession) -> Result<Self, String> {
        let files = &saved.files;
        let mut session = Self::new(files.kind);
        match &mut session {
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
            _ => true,
        }
    }
    fn new(kind: SessionKind) -> Self {
        match kind {
            SessionKind::Text => Self::Text(Box::new(TextCompareScreen::new())),
            SessionKind::Folder => Self::Folder(Box::new(FolderCompareScreen::new())),
            SessionKind::Hex => Self::Hex(Box::new(HexCompareScreen::new())),
        }
    }

    fn dirty(&self) -> bool {
        match self {
            Self::Text(s) => s.left.dirty() || s.right.dirty(),
            Self::Hex(s) => s.dirty(),
            Self::Folder(_) => false,
        }
    }

    fn is_empty(&self, kind: SessionKind) -> bool {
        match (self, kind) {
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
            (Self::Hex(s), SessionKind::Hex) => s.left_path.is_empty() && s.right_path.is_empty(),
            _ => false,
        }
    }

    fn record(&self) -> Option<RecentSession> {
        let (kind, left, right) = match self {
            Self::Text(s) => (
                SessionKind::Text,
                s.left.path.clone()?,
                s.right.path.clone()?,
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
        };
        if let Some((left, right)) = startup.pair {
            let kind = startup.kind.unwrap_or_else(|| {
                if left.is_dir() && right.is_dir() {
                    SessionKind::Folder
                } else {
                    SessionKind::Text
                }
            });
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
            if let (Some(query), Some(Session::Text(screen))) = (
                startup.capture_find,
                app.active.map(|index| &mut app.sessions[index]),
            ) {
                screen.show_search(query);
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
        app
    }

    fn open_session(&mut self, kind: SessionKind) {
        self.active = Some(self.sessions.len());
        self.sessions.push(Session::new(kind));
        self.session_paths.push(None);
    }

    fn open_pair(&mut self, record: RecentSession) {
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
                SessionKind::Text,
                left.clone(),
                right.clone(),
            ));
        } else if let Some(path) = paths.first() {
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
        if self.capture.is_some() && [4, 5, 8, 9].contains(&self.capture_frames) {
            let label = if self.capture_frames <= 5 {
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
                        button: egui::PointerButton::Primary,
                        pressed: self.capture_frames == 4 || self.capture_frames == 8,
                        modifiers: Default::default(),
                    });
                }
            }
        }
    }
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
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
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(format!(
            "{title} - AnyCompare"
        )));
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
                        Session::Folder(_) => "imgDirCompareSession",
                        Session::Hex(_) => "imgHexCompareSession",
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
            self.open_pair(RecentSession::new(SessionKind::Text, left, right));
        }
        if let Some(request) = ui::session_menu::take(ctx) {
            self.session_request(request, &mut dialogs);
        }
        if self.capture.is_none() && self.history_valid {
            let mut history_changed = false;
            if let Some(Session::Text(screen)) = self.active.map(|index| &mut self.sessions[index])
            {
                if let Some(query) = screen.take_recent_search() {
                    history_changed |= self.history.remember_search(query);
                }
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
                self.remove_session(index);
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

fn main() -> eframe::Result<()> {
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
        }
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
