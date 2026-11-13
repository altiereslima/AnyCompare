//! Home: BC5's twelve simultaneous launchers and separate 64/32 px artwork.
//! Tree drawing retains the measured BC4 controls. BC5 desktop geometry is
//! observed in docs/reference/bc5 and the user's 02/11/2026 screenshots; its
//! resize handler has not been recovered. Do not describe the fit as decompiled.

use super::dialogs::DialogHost;
use crate::history::{RecentSession, SessionKind};
use crate::ui::icons::IconManager;
use egui::*;
use std::collections::BTreeSet;

pub enum HomeAction {
    None,
    OpenTextCompare,
    OpenFolderCompare,
    OpenHexCompare,
    OpenTableCompare,
    OpenVersionCompare,
    OpenRegistryCompare,
    OpenMediaCompare,
    OpenTextEdit,
    OpenRecentSession(RecentSession),
    ForgetRecentSession(RecentSession),
}

/// VCL font scaling of the Home frame: MulDiv(value, 15, 13).
fn sc(value: i32) -> f32 {
    ((value * 15 + 6) / 13) as f32
}

const RIGHT_BG: Color32 = Color32::from_rgb(249, 249, 249);
const TREE_BG: Color32 = Color32::from_rgb(227, 231, 237);
const PANEL_BG: Color32 = Color32::from_rgb(240, 240, 240);
const LINE: Color32 = Color32::from_rgb(160, 160, 160);
const TREE_DOT: Color32 = Color32::from_rgb(109, 109, 109);
const SELECTION_FILL: Color32 = Color32::from_rgb(193, 197, 202);
const SELECTION_BORDER: Color32 = Color32::from_rgb(132, 134, 138);
const BUTTON_FILL: Color32 = Color32::from_rgb(253, 253, 253);
const BUTTON_BORDER: Color32 = Color32::from_rgb(208, 208, 208);
const BUTTON_BORDER_BOTTOM: Color32 = Color32::from_rgb(186, 186, 186);
const DEFAULT_BORDER: Color32 = Color32::from_rgb(0, 120, 212);
const DEFAULT_BORDER_BOTTOM: Color32 = Color32::from_rgb(0, 107, 190);
const DISABLED_TEXT: Color32 = Color32::from_gray(160);
const EDIT_UNDERLINE: Color32 = Color32::from_rgb(131, 131, 131);

/// Menu band of the Home view (25 rows, no toolbar) and the 2-row top-line bevel below it.
const MENU_HEIGHT: f32 = 25.0;
const ROW: f32 = 18.0;
const INDENT: f32 = 19.0;

/// Explorer-theme chevrons at 100%: (dx from the level's centre column, dy from the row top,
/// colour), sampled over the tree background.
const CHEVRON_COLLAPSED: &[(i8, i8, [u8; 3])] = &[
    (-2, 6, [185, 187, 191]),
    (-1, 6, [180, 182, 185]),
    (-2, 7, [180, 182, 185]),
    (-1, 7, [139, 139, 139]),
    (0, 7, [175, 176, 179]),
    (-1, 8, [175, 176, 179]),
    (0, 8, [139, 139, 139]),
    (1, 8, [175, 176, 179]),
    (0, 9, [175, 176, 179]),
    (1, 9, [139, 139, 139]),
    (2, 9, [175, 176, 179]),
    (1, 10, [153, 154, 155]),
    (2, 10, [139, 139, 139]),
    (3, 10, [208, 211, 216]),
    (0, 11, [175, 176, 179]),
    (1, 11, [139, 139, 139]),
    (2, 11, [175, 176, 179]),
    (-1, 12, [175, 176, 179]),
    (0, 12, [139, 139, 139]),
    (1, 12, [175, 176, 179]),
    (-2, 13, [180, 182, 185]),
    (-1, 13, [139, 139, 139]),
    (0, 13, [175, 176, 179]),
    (-2, 14, [185, 187, 191]),
    (-1, 14, [180, 182, 185]),
];
const CHEVRON_EXPANDED: &[(i8, i8, [u8; 3])] = &[
    (-4, 7, [132, 134, 138]),
    (-3, 7, [120, 122, 125]),
    (3, 7, [120, 122, 125]),
    (4, 7, [132, 134, 138]),
    (-4, 8, [120, 122, 125]),
    (-3, 8, [27, 27, 27]),
    (-2, 8, [109, 110, 113]),
    (2, 8, [109, 110, 113]),
    (3, 8, [27, 27, 27]),
    (4, 8, [120, 122, 125]),
    (-3, 9, [109, 110, 113]),
    (-2, 9, [27, 27, 27]),
    (-1, 9, [109, 110, 113]),
    (1, 9, [109, 110, 113]),
    (2, 9, [27, 27, 27]),
    (3, 9, [109, 110, 113]),
    (-2, 10, [109, 110, 113]),
    (-1, 10, [27, 27, 27]),
    (0, 10, [59, 60, 61]),
    (1, 10, [27, 27, 27]),
    (2, 10, [109, 110, 113]),
    (-1, 11, [109, 110, 113]),
    (0, 11, [27, 27, 27]),
    (1, 11, [109, 110, 113]),
    (0, 12, [183, 186, 191]),
];

/// Magnifier of the Windows 11 search edit (alpha of colour (102,102,102)), sampled at 100%
/// over white; drawn 3 px right and 4 px down from the edit's corner.
const MAGNIFIER: [[u8; 14]; 13] = [
    [0, 0, 0, 0, 0, 0, 137, 223, 223, 213, 137, 0, 0, 0],
    [0, 0, 0, 0, 27, 233, 255, 242, 200, 250, 255, 200, 0, 0],
    [0, 0, 0, 0, 223, 250, 117, 0, 0, 27, 137, 255, 170, 0],
    [0, 0, 0, 50, 255, 185, 0, 0, 0, 0, 0, 223, 250, 0],
    [0, 0, 0, 95, 255, 50, 0, 0, 0, 0, 0, 137, 255, 73],
    [0, 0, 0, 95, 255, 50, 0, 0, 0, 0, 0, 137, 255, 73],
    [0, 0, 0, 50, 255, 185, 0, 0, 0, 0, 0, 223, 250, 0],
    [0, 0, 0, 0, 220, 252, 168, 0, 0, 27, 153, 255, 170, 0],
    [0, 0, 0, 73, 250, 255, 255, 250, 213, 250, 255, 200, 0, 0],
    [0, 0, 73, 250, 255, 233, 148, 213, 223, 200, 95, 0, 0, 0],
    [0, 73, 250, 255, 233, 27, 0, 0, 0, 0, 0, 0, 0, 0],
    [0, 137, 255, 233, 27, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    [0, 27, 137, 27, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
];

#[derive(Clone, Copy)]
struct SessionType {
    small: &'static str,
    big: &'static str,
    label: &'static str,
    kind: Option<SessionKind>,
}

/// BC5 primary sessions: six large icons, all visible on the Home.
const PRIMARY: [SessionType; 6] = [
    SessionType {
        small: "imgDirCompareSession",
        big: "imgHomeDirCompareSession",
        label: "Comparar Pastas",
        kind: Some(SessionKind::Folder),
    },
    SessionType {
        small: "imgDirMergeSession",
        big: "imgHomeDirMergeSession",
        label: "Combinar Pastas",
        kind: None,
    },
    SessionType {
        small: "imgDirSyncSession",
        big: "imgHomeDirSyncSession",
        label: "Sincronizar Pastas",
        kind: None,
    },
    SessionType {
        small: "imgTextCompareSession",
        big: "imgHomeTextCompareSession",
        label: "Comparar Textos",
        kind: Some(SessionKind::Text),
    },
    SessionType {
        small: "imgTextMergeSession",
        big: "imgHomeTextMergeSession",
        label: "Mesclar Texto",
        kind: None,
    },
    SessionType {
        small: "imgTextEditSession",
        big: "imgHomeTextEditSession",
        label: "Editar texto",
        kind: Some(SessionKind::TextEdit),
    },
];

/// BC5 secondary sessions use the original 32 px artwork.
const SECONDARY: [SessionType; 6] = [
    SessionType {
        small: "imgHexCompareSession",
        big: "imgHomeHexCompareSession",
        label: "Comparar Hex",
        kind: Some(SessionKind::Hex),
    },
    SessionType {
        small: "imgMP3CompareSession",
        big: "imgHomeMediaCompareSession",
        label: "Comparar Mídia",
        kind: Some(SessionKind::Media),
    },
    SessionType {
        small: "imgPixCompareSession",
        big: "imgHomePixCompareSession",
        label: "Comparar Imagens",
        kind: None,
    },
    SessionType {
        small: "imgRegCompareSession",
        big: "imgHomeRegCompareSession",
        label: "Comparar Registro",
        kind: Some(SessionKind::Registry),
    },
    SessionType {
        small: "imgTableCompareSession",
        big: "imgHomeTableCompareSession",
        label: "Comparar Tabelas",
        kind: Some(SessionKind::Table),
    },
    SessionType {
        small: "imgVerCompareSession",
        big: "imgHomeVerCompareSession",
        label: "Comparar Versões",
        kind: Some(SessionKind::Version),
    },
];

/// Unique tree key of a recent session: kind and both full paths (labels repeat when only
/// the folders differ).
fn session_key(session: &RecentSession) -> String {
    format!(
        "session/{:?}|{}|{}",
        session.kind,
        session.left.display(),
        session.right.display()
    )
}

fn open_action(kind: SessionKind) -> HomeAction {
    match kind {
        SessionKind::Text => HomeAction::OpenTextCompare,
        SessionKind::Folder => HomeAction::OpenFolderCompare,
        SessionKind::Hex => HomeAction::OpenHexCompare,
        SessionKind::Table => HomeAction::OpenTableCompare,
        SessionKind::Version => HomeAction::OpenVersionCompare,
        SessionKind::Registry => HomeAction::OpenRegistryCompare,
        SessionKind::Media => HomeAction::OpenMediaCompare,
        SessionKind::TextEdit => HomeAction::OpenTextEdit,
    }
}

fn kind_icon(kind: SessionKind) -> &'static str {
    match kind {
        SessionKind::Text => "imgTextCompareSession",
        SessionKind::Folder => "imgDirCompareSession",
        SessionKind::Hex => "imgHexCompareSession",
        SessionKind::Table => "imgTableCompareSession",
        SessionKind::Version => "imgVerCompareSession",
        SessionKind::Registry => "imgRegCompareSession",
        SessionKind::Media => "imgMP3CompareSession",
        SessionKind::TextEdit => "imgTextEditSession",
    }
}

/// Auto-saved date groups of the original (BCompare.tr strings).
fn group_label(group: u8) -> String {
    match group {
        0 => "Hoje".into(),
        1 => "Ontem".into(),
        2..=6 => format!("{group} dias atrás"),
        _ => "Mais do que 6 dias atrás".into(),
    }
}

fn group_of(saved: i64) -> u8 {
    use chrono::{Local, TimeZone};
    let today = Local::now().date_naive();
    let Some(date) = Local.timestamp_opt(saved, 0).single() else {
        return 7;
    };
    (today - date.date_naive()).num_days().clamp(0, 7) as u8
}

#[derive(Clone, Copy, PartialEq)]
enum Node {
    New,
    NewType(SessionType),
    Auto,
    Group(u8),
    Session(usize),
}

impl PartialEq for SessionType {
    fn eq(&self, other: &Self) -> bool {
        self.label == other.label
    }
}

struct Row {
    node: Node,
    key: String,
    level: usize,
    icon: &'static str,
    overlay: bool,
    label: String,
    has_children: bool,
    expanded: bool,
    last: bool,
    /// For each ancestor level: whether that ancestor was the last of its siblings.
    ancestors_last: Vec<bool>,
}

#[derive(Clone)]
pub struct HomeScreen {
    pub search_query: String,
    pub recent: Vec<RecentSession>,
    pub notice: Option<String>,
    selected: Option<String>,
    expanded: BTreeSet<String>,
    expansion_initialized: bool,
}

impl HomeScreen {
    pub fn new() -> Self {
        Self {
            search_query: String::new(),
            recent: Vec::new(),
            notice: None,
            selected: None,
            expanded: BTreeSet::new(),
            expansion_initialized: false,
        }
    }

    fn rows(&mut self) -> Vec<Row> {
        if !self.expansion_initialized && !self.recent.is_empty() {
            // The most recent date group opens with the auto-saved folder.
            self.expanded.insert("auto".into());
            let first = group_of(self.recent[0].saved);
            self.expanded.insert(format!("auto/{first}"));
            self.expansion_initialized = true;
        }
        let query = self.search_query.to_lowercase();
        let matching: Vec<usize> = (0..self.recent.len())
            .filter(|&i| self.recent[i].label().to_lowercase().contains(&query))
            .collect();
        let mut groups: Vec<(u8, Vec<usize>)> = Vec::new();
        for i in matching {
            let group = group_of(self.recent[i].saved);
            match groups.iter_mut().find(|(g, _)| *g == group) {
                Some((_, items)) => items.push(i),
                None => groups.push((group, vec![i])),
            }
        }
        groups.sort_by_key(|(g, _)| *g);
        let mut rows = Vec::new();
        let new_open = self.expanded.contains("new");
        rows.push(Row {
            node: Node::New,
            key: "new".into(),
            level: 0,
            icon: "imgSessionDefaultFolder",
            overlay: false,
            label: crate::i18n::tr("Novo"),
            has_children: true,
            expanded: new_open,
            last: false,
            ancestors_last: vec![],
        });
        if new_open {
            // Preserve the existing session-picker contents. Standalone Text
            // Edit is a Home launcher, not an implemented comparison session.
            let types: Vec<SessionType> = PRIMARY
                .iter()
                .chain(SECONDARY.iter())
                .filter(|t| t.label != "Editar texto")
                .copied()
                .collect();
            for (n, t) in types.iter().enumerate() {
                rows.push(Row {
                    node: Node::NewType(*t),
                    key: format!("new/{}", t.label),
                    level: 1,
                    icon: t.small,
                    overlay: false,
                    label: crate::i18n::tr(t.label),
                    has_children: false,
                    expanded: false,
                    last: n + 1 == types.len(),
                    ancestors_last: vec![false],
                });
            }
        }
        let auto_open = self.expanded.contains("auto");
        rows.push(Row {
            node: Node::Auto,
            key: "auto".into(),
            level: 0,
            icon: "imgSessionFolder",
            overlay: false,
            label: crate::i18n::tr("Salvo automaticamente"),
            has_children: !groups.is_empty(),
            expanded: auto_open,
            last: true,
            ancestors_last: vec![],
        });
        if auto_open {
            for (g, (group, items)) in groups.iter().enumerate() {
                let key = format!("auto/{group}");
                let open = self.expanded.contains(&key);
                let last_group = g + 1 == groups.len();
                rows.push(Row {
                    node: Node::Group(*group),
                    key,
                    level: 1,
                    icon: "imgSessionFolder",
                    overlay: false,
                    label: crate::i18n::tr(&group_label(*group)),
                    has_children: true,
                    expanded: open,
                    last: last_group,
                    ancestors_last: vec![true],
                });
                if open {
                    for (n, &i) in items.iter().enumerate() {
                        let session = &self.recent[i];
                        rows.push(Row {
                            node: Node::Session(i),
                            key: session_key(session),
                            level: 2,
                            icon: kind_icon(session.kind),
                            overlay: true,
                            label: session.label(),
                            has_children: false,
                            expanded: false,
                            last: n + 1 == items.len(),
                            ancestors_last: vec![true, last_group],
                        });
                    }
                }
            }
        }
        rows
    }

    fn dropdown_rows(&mut self) -> Vec<Row> {
        let mut rows = self.rows();
        if self.recent.is_empty() {
            rows.retain(|row| !matches!(row.node, Node::Auto));
            for row in &mut rows {
                if matches!(row.node, Node::New) {
                    row.last = true;
                }
                if let Some(last) = row.ancestors_last.first_mut() {
                    *last = true;
                }
            }
        }
        for row in &mut rows {
            if let Node::NewType(kind) = row.node {
                row.label = crate::i18n::tr(match kind.label {
                    "Comparar Pastas" => "Comparar Pastas (F)",
                    "Combinar Pastas" => "Combinar Pasta (L)",
                    label => label,
                });
            }
        }
        rows
    }

    /// Native TUiComboTree uses all nodes for width, visible nodes for height.
    /// SessionsCombo overrides DropDownCount to 30 (REA runtime, 30/10/2026).
    /// Width includes the border; the caller supplies the one-pixel frame.
    pub fn session_dropdown_size(&mut self, ui: &Ui, minimum_width: f32) -> Vec2 {
        let visible = self.dropdown_rows().len().max(1);
        let mut all = self.clone();
        all.expanded.insert("new".into());
        all.expanded.insert("auto".into());
        for record in &all.recent {
            all.expanded
                .insert(format!("auto/{}", group_of(record.saved)));
        }
        let width = all
            .dropdown_rows()
            .iter()
            .fold(minimum_width, |width, row| {
                width.max(
                    INDENT * (row.level + 2) as f32
                        + 16.0
                        + text_width(ui, &row.label, 12.0)
                        + 17.0,
                )
            });
        // An egui popup lives within the app viewport; unlike a native HWND it
        // cannot extend onto the desktop beyond the window's client bounds.
        let screen = ui.ctx().screen_rect();
        let fitting_rows = ((screen.height() - 2.0) / ROW).floor().max(1.0) as usize;
        vec2(
            width.ceil().min(screen.width()),
            visible.min(30).min(fitting_rows) as f32 * ROW + 2.0,
        )
    }

    /// Selects the n-th recent session and opens its folders (used by `--home-select`).
    pub fn select_recent(&mut self, index: usize) {
        if let Some(session) = self.recent.get(index) {
            self.expanded.insert("auto".into());
            self.expanded
                .insert(format!("auto/{}", group_of(session.saved)));
            self.expansion_initialized = true;
            self.selected = Some(session_key(session));
        }
    }

    fn selected_session(&self) -> Option<&RecentSession> {
        let key = self.selected.as_ref()?;
        self.recent.iter().find(|s| &session_key(s) == key)
    }

    pub fn ui(
        &mut self,
        _ctx: &Context,
        ui: &mut Ui,
        icons: &mut IconManager,
        dialogs: &mut dyn DialogHost,
    ) -> HomeAction {
        let mut action = HomeAction::None;
        TopBottomPanel::top("home_menu")
            .exact_height(MENU_HEIGHT + 2.0)
            .show_separator_line(false)
            .frame(Frame::none())
            .show_inside(ui, |ui| {
                let band = ui.max_rect();
                // BC5 flat menu, then the two-pixel TopLineBevel.
                ui.painter().rect_filled(band, 0.0, Color32::WHITE);
                let y = band.top() + MENU_HEIGHT;
                ui.painter().rect_filled(
                    Rect::from_min_max(pos2(band.left(), y), pos2(band.right(), y + 1.0)),
                    0.0,
                    LINE,
                );
                ui.painter().rect_filled(
                    Rect::from_min_max(pos2(band.left(), y + 1.0), pos2(band.right(), y + 2.0)),
                    0.0,
                    Color32::WHITE,
                );
                let mut bar = ui.new_child(UiBuilder::new().max_rect(Rect::from_min_size(
                    band.min,
                    vec2(band.width(), MENU_HEIGHT),
                )));
                self.menu(&mut bar, icons, dialogs, &mut action);
            });

        super::session_tabs::show(ui, icons);
        SidePanel::left("home_sessions")
            .exact_width((ui.available_width() * 0.20).floor())
            .resizable(false)
            .show_separator_line(false)
            .frame(Frame::none().fill(TREE_BG))
            .show_inside(ui, |ui| self.sessions_panel(ui, icons, &mut action));

        CentralPanel::default()
            .frame(Frame::none().fill(RIGHT_BG))
            .show_inside(ui, |ui| self.right_panel(ui, icons, &mut action));

        action
    }

    fn menu(
        &mut self,
        ui: &mut Ui,
        icons: &mut IconManager,
        dialogs: &mut dyn DialogHost,
        action: &mut HomeAction,
    ) {
        super::chrome::menu_bar(ui, |ui| {
            super::chrome::menu_button(ui, "Sessão", |ui| {
                super::session_menu::prefix(ui, icons, false);
                if super::chrome::menu_item(ui, icons, "Comparar Textos", true).clicked() {
                    *action = HomeAction::OpenTextCompare;
                    ui.close_menu();
                }
                if super::chrome::menu_item(ui, icons, "Comparar Pastas", true).clicked() {
                    *action = HomeAction::OpenFolderCompare;
                    ui.close_menu();
                }
                if super::chrome::menu_item(ui, icons, "Comparar Hexadecimal", true).clicked() {
                    *action = HomeAction::OpenHexCompare;
                    ui.close_menu();
                }
                ui.separator();
                super::session_menu::suffix(ui, icons, false);
            });
            super::chrome::menu_button(ui, "Visualizar", |ui| {
                if super::chrome::menu_item(ui, icons, "Expandir Tudo", true).clicked() {
                    self.expanded.insert("new".into());
                    self.expanded.insert("auto".into());
                    for record in &self.recent {
                        self.expanded
                            .insert(format!("auto/{}", group_of(record.saved)));
                    }
                    ui.close_menu();
                }
                if super::chrome::menu_item(ui, icons, "Recolher Tudo", true).clicked() {
                    self.expanded.clear();
                    self.expansion_initialized = true;
                    ui.close_menu();
                }
            });
            super::chrome::menu_button(ui, "Ferramentas", |ui| {
                if super::chrome::menu_item(ui, icons, "Opções...", true).clicked() {
                    crate::i18n::configure(dialogs);
                    ui.close_menu();
                }
                if super::chrome::menu_item(ui, icons, "Limpar filtro de sessões", true).clicked()
                {
                    self.search_query.clear();
                    ui.close_menu();
                }
                for label in [
                    "Formatos de Arquivo...",
                    "Perfis...",
                    "Integração de Controle de Origem...",
                ] {
                    super::chrome::menu_item(ui, icons, label, false).on_disabled_hover_text(
                        crate::i18n::tr("Esta função ainda não está disponível"),
                    );
                }
                super::session_menu::settings_tools(ui, icons, dialogs);
                ui.separator();
                for label in [
                    "Salvar Instantâneo...",
                    "Editar Arquivo de Texto",
                    "Visualizar Correção...",
                ] {
                    super::chrome::menu_item(ui, icons, label, false).on_disabled_hover_text(
                        crate::i18n::tr("Esta função ainda não está disponível"),
                    );
                }
            });
            super::chrome::menu_button(ui, "Ajuda (H)", |ui| {
                if super::chrome::menu_item(ui, icons, "Sobre o AnyCompare...", true).clicked() {
                    dialogs.about();
                    ui.close_menu();
                }
            });
        });
    }

    fn session_tree(
        &mut self,
        ui: &mut Ui,
        icons: &mut IconManager,
        tree_rect: Rect,
        action: &mut HomeAction,
        activate_on_click: bool,
    ) {
        let rows = if activate_on_click {
            self.dropdown_rows()
        } else {
            self.rows()
        };
        let mut tree_ui = ui.new_child(UiBuilder::new().max_rect(tree_rect));
        tree_ui.set_clip_rect(tree_rect);
        let content_width = rows
            .iter()
            .map(|row| {
                22.0 + INDENT * row.level as f32 + 20.0 + text_width(ui, &row.label, 12.0) + 6.0
            })
            .fold(tree_rect.width(), f32::max);
        let mut toggle = None;
        let mut select = None;
        let mut open = None;
        let mut keyboard_row = None;
        if activate_on_click {
            let current = rows
                .iter()
                .position(|row| self.selected.as_deref() == Some(row.key.as_str()));
            for key in [
                Key::ArrowDown,
                Key::ArrowUp,
                Key::ArrowRight,
                Key::ArrowLeft,
                Key::Enter,
            ] {
                if ui.input_mut(|input| input.consume_key(Modifiers::NONE, key)) {
                    let next = match key {
                        Key::ArrowDown => current
                            .map_or(0, |current| (current + 1).min(rows.len().saturating_sub(1))),
                        Key::ArrowUp => current.unwrap_or(0).saturating_sub(1),
                        _ => current.unwrap_or(0),
                    };
                    if let Some(row) = rows.get(next) {
                        select = Some(row.key.clone());
                        keyboard_row = Some(next);
                        match key {
                            Key::ArrowRight if row.has_children && !row.expanded => {
                                toggle = Some(row.key.clone())
                            }
                            Key::ArrowLeft if row.has_children && row.expanded => {
                                toggle = Some(row.key.clone())
                            }
                            Key::Enter if row.has_children => toggle = Some(row.key.clone()),
                            Key::Enter => open = Some(row.node),
                            _ => {}
                        }
                    }
                }
            }
        }
        ScrollArea::both()
            .id_salt("home_tree")
            .auto_shrink([false; 2])
            .show(&mut tree_ui, |ui| {
                let (area, _) = ui.allocate_exact_size(
                    vec2(content_width, rows.len() as f32 * ROW),
                    Sense::hover(),
                );
                let painter = ui.painter().clone();
                for (index, row) in rows.iter().enumerate() {
                    let top = area.top() + index as f32 * ROW;
                    let left = area.left();
                    let cx = left + 10.0 + INDENT * row.level as f32;
                    let icon_x = left + 22.0 + INDENT * row.level as f32;
                    let row_rect = Rect::from_min_size(
                        pos2(left, top),
                        vec2(area.width().max(tree_rect.width()), ROW),
                    );
                    if keyboard_row == Some(index) {
                        ui.scroll_to_rect(row_rect, None);
                    }
                    let response = ui.interact(
                        row_rect,
                        ui.id().with(("home_row", &row.key)),
                        Sense::click(),
                    );
                    let selected_key = self
                        .selected
                        .as_deref()
                        .or(activate_on_click.then_some("new"));
                    let selected = selected_key == Some(row.key.as_str());
                    if selected && activate_on_click {
                        let sel = Rect::from_min_max(
                            pos2(icon_x - 1.0, top),
                            pos2(
                                (icon_x + 24.0 + text_width(ui, &row.label, 12.0))
                                    .min(tree_rect.right()),
                                top + ROW,
                            ),
                        );
                        painter.rect_filled(sel, 1.0, Color32::from_rgb(153, 209, 255));
                        painter.rect_filled(sel.shrink(1.0), 0.0, Color32::from_rgb(204, 232, 255));
                    } else if selected {
                        let sel = Rect::from_min_max(
                            pos2(icon_x - 3.0, top),
                            pos2(tree_rect.right() - 2.0, top + ROW),
                        );
                        painter.rect_filled(sel, 0.0, SELECTION_BORDER);
                        painter.rect_filled(sel.shrink(1.0), 0.0, SELECTION_FILL);
                    }
                    paint_tree_lines(&painter, row, cx, icon_x, top, selected);
                    if row.has_children {
                        let glyph = if row.expanded {
                            CHEVRON_EXPANDED
                        } else {
                            CHEVRON_COLLAPSED
                        };
                        for &(dx, dy, [r, g, b]) in glyph {
                            painter.rect_filled(
                                Rect::from_min_size(
                                    pos2(cx + f32::from(dx), top + f32::from(dy)),
                                    vec2(1.0, 1.0),
                                ),
                                0.0,
                                Color32::from_rgb(r, g, b),
                            );
                        }
                    }
                    for image in [Some(row.icon), row.overlay.then_some("imgPlus")]
                        .into_iter()
                        .flatten()
                    {
                        if let Some(texture) = icons.get_icon(ui.ctx(), image) {
                            painter.image(
                                texture.id(),
                                Rect::from_min_size(pos2(icon_x, top + 1.0), vec2(16.0, 16.0)),
                                Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)),
                                Color32::WHITE,
                            );
                        }
                    }
                    let enabled = !matches!(row.node, Node::NewType(t) if t.kind.is_none());
                    paint_text(
                        &painter,
                        pos2(icon_x + 20.0, top + 1.0),
                        &row.label,
                        12.0,
                        if enabled {
                            Color32::BLACK
                        } else {
                            DISABLED_TEXT
                        },
                    );
                    if activate_on_click && matches!(row.node, Node::New) {
                        ui.ctx().data_mut(|data| {
                            data.insert_temp(
                                Id::new("capture_menu_rect").with("Sessões Novo"),
                                Rect::from_min_size(pos2(cx - 8.0, top), vec2(16.0, ROW)),
                            )
                        });
                    }
                    let on_chevron = response
                        .interact_pointer_pos()
                        .is_some_and(|p| (p.x - cx).abs() <= 8.0);
                    if response.clicked() {
                        if row.has_children && on_chevron {
                            toggle = Some(row.key.clone());
                        } else {
                            select = Some(row.key.clone());
                        }
                    }
                    if response.double_clicked()
                        || activate_on_click && response.clicked() && !row.has_children && enabled
                    {
                        if row.has_children {
                            toggle = Some(row.key.clone());
                        } else {
                            open = Some(row.node);
                        }
                    }
                }
            });
        if let Some(key) = toggle {
            if !self.expanded.remove(&key) {
                self.expanded.insert(key);
            }
        }
        if let Some(key) = select {
            self.selected = Some(key);
        }
        match open {
            Some(Node::Session(i)) => {
                if let Some(session) = self.recent.get(i) {
                    *action = HomeAction::OpenRecentSession(session.clone());
                }
            }
            Some(Node::NewType(t)) => {
                if let Some(kind) = t.kind {
                    *action = open_action(kind);
                }
            }
            _ => {}
        }
    }

    /// Session combo reuses the Home tree, without the Home's header/buttons.
    pub fn session_dropdown(
        &mut self,
        ui: &mut Ui,
        icons: &mut IconManager,
        size: Vec2,
    ) -> HomeAction {
        let mut action = HomeAction::None;
        let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
        ui.painter().rect_filled(rect, 0.0, Color32::WHITE);
        self.session_tree(ui, icons, rect, &mut action, true);
        action
    }

    fn sessions_panel(&mut self, ui: &mut Ui, icons: &mut IconManager, action: &mut HomeAction) {
        let panel = ui.max_rect();
        let painter = ui.painter().clone();
        let tree_top = panel.top() + sc(24);
        // Bottom area: max(button 24, edit 23) + MulDiv(8) -> 35 rows at 100%.
        let bottom_h = 35.0;
        let tree_bottom = panel.bottom() - bottom_h - 1.0;
        // Header (SessionsPanelPaint): gradient down to the tree top, line under it.
        super::chrome::gradient(
            ui,
            Rect::from_min_max(panel.min, pos2(panel.right(), tree_top)),
            Color32::from_gray(240),
            Color32::from_gray(224),
        );
        paint_text(
            &painter,
            pos2(panel.left() + sc(8), panel.top() + sc(2)),
            &crate::i18n::tr("Sessões"),
            13.0,
            Color32::BLACK,
        );
        hline(&painter, panel.left(), panel.right(), tree_top - 1.0, LINE);
        // Bottom area.
        let bottom = Rect::from_min_max(pos2(panel.left(), tree_bottom + 1.0), panel.max);
        painter.rect_filled(bottom, 0.0, PANEL_BG);
        hline(&painter, panel.left(), panel.right(), tree_bottom, LINE);
        // Right edge (the splitter's dark side).
        painter.rect_filled(
            Rect::from_min_max(pos2(panel.right() - 1.0, panel.top()), panel.max),
            0.0,
            LINE,
        );

        // Tree.
        let tree_rect = Rect::from_min_max(
            pos2(panel.left(), tree_top),
            pos2(panel.right() - 1.0, tree_bottom),
        );
        self.session_tree(ui, icons, tree_rect, action, false);

        // Bottom: New (+), Delete (-) and the search edit (DFM SessionsNewBtn,
        // SessionsDeleteBtn, SessionsSearchEdit), measured at 100%.
        let top = bottom.top() + 4.0;
        let new_rect = Rect::from_min_size(pos2(panel.left() + 3.0, top), vec2(25.0, 24.0));
        let delete_rect = Rect::from_min_size(pos2(panel.left() + 30.0, top), vec2(25.0, 24.0));
        let new_response = win_button(ui, new_rect, None, Some("imgListAdd"), icons, false, true)
            .on_hover_text(crate::i18n::tr("Novo"));
        let popup = ui.id().with("home_new_popup");
        if new_response.clicked() {
            ui.memory_mut(|m| m.toggle_popup(popup));
        }
        popup::popup_below_widget(
            ui,
            popup,
            &new_response,
            PopupCloseBehavior::CloseOnClick,
            |ui| {
                ui.set_min_width(170.0);
                for t in PRIMARY
                    .iter()
                    .chain(SECONDARY.iter())
                    .filter(|t| t.label != "Editar texto")
                {
                    if super::chrome::menu_item(ui, icons, t.label, t.kind.is_some()).clicked() {
                        if let Some(kind) = t.kind {
                            *action = open_action(kind);
                        }
                    }
                }
            },
        );
        let deletable = self.selected_session().cloned();
        if win_button(
            ui,
            delete_rect,
            None,
            Some("imgListDelete"),
            icons,
            false,
            deletable.is_some(),
        )
        .on_hover_text(crate::i18n::tr("Excluir"))
        .clicked()
        {
            if let Some(session) = deletable {
                self.selected = None;
                *action = HomeAction::ForgetRecentSession(session);
            }
        }
        let edit = Rect::from_min_max(
            pos2(panel.left() + 65.0, bottom.top() + 5.0),
            pos2(panel.right() - 6.0, bottom.top() + 28.0),
        );
        search_edit(ui, edit, &mut self.search_query);
    }

    fn right_panel(&mut self, ui: &mut Ui, icons: &mut IconManager, action: &mut HomeAction) {
        // The 5 px splitter after the sessions panel is painted with this panel's colour.
        let full = ui.max_rect();
        let panel = Rect::from_min_max(pos2(full.left() + 5.0, full.top()), full.max);
        let painter = ui.painter().clone();
        let session = self.selected_session().cloned();
        let spec_h = if session.is_some() { sc(119) } else { 0.0 };
        if let Some(session) = &session {
            if let Some(texture) = icons.get_icon(ui.ctx(), kind_icon(session.kind)) {
                painter.image(
                    texture.id(),
                    Rect::from_min_size(panel.min + vec2(sc(4), sc(4)), vec2(16.0, 16.0)),
                    Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)),
                    Color32::WHITE,
                );
            }
            paint_text(
                &painter,
                panel.min + vec2(sc(24), sc(4)),
                &session.label(),
                15.0,
                Color32::BLACK,
            );
            for (n, path) in [&session.left, &session.right].into_iter().enumerate() {
                paint_text(
                    &painter,
                    panel.min + vec2(sc(4) + 7.0, [sc(24), sc(44)][n]),
                    &path.to_string_lossy(),
                    12.0,
                    Color32::BLACK,
                );
            }
            let top = panel.top() + sc(88) + 1.0;
            let open_rect = Rect::from_min_size(pos2(panel.left() + 7.0, top), vec2(82.0, 24.0));
            let edit_rect = Rect::from_min_size(pos2(panel.left() + 99.0, top), vec2(82.0, 24.0));
            if win_button(ui, open_rect, Some("Abrir"), None, icons, true, true).clicked() {
                *action = HomeAction::OpenRecentSession(session.clone());
            }
            // The session settings editor (ManagePanel) is not reconstructed yet.
            win_button(ui, edit_rect, Some("Editar"), None, icons, false, false)
                .on_disabled_hover_text(crate::i18n::tr("Esta função ainda não está disponível"));
        }
        if let Some(notice) = &self.notice {
            paint_text(
                &painter,
                panel.min + vec2(sc(4), spec_h + 4.0),
                &crate::i18n::tr(notice),
                12.0,
                Color32::DARK_RED,
            );
        }

        // BC5 displays both families simultaneously. Keep the selected-session
        // description independent from the grid, so changing history does not
        // move the session launchers or hide a family behind a page arrow.
        let area = Rect::from_min_max(panel.min, panel.max - vec2(0.0, sc(16)));
        let grid = HomeGrid::new(area);
        let tip = crate::i18n::tr("Arraste pastas ou arquivos para o ícone da sessão");
        let desc = crate::i18n::tr("ou clique no ícone da sessão para começar:");
        for (text, size, y) in [
            (&tip, 13.0, grid.top - 46.0),
            (&desc, 12.0, grid.top - 24.0),
        ] {
            if y >= panel.top() + spec_h {
                paint_text(
                    &painter,
                    pos2(
                        (area.center().x - text_width(ui, text, size) / 2.0).floor(),
                        y,
                    ),
                    text,
                    size,
                    Color32::BLACK,
                );
            }
        }
        for (index, item) in PRIMARY.iter().chain(SECONDARY.iter()).enumerate() {
            let (cell, large) = grid.cell(index);
            let available = item.kind.is_some();
            let response = ui
                .interact(
                    cell,
                    ui.id().with(("home_task", item.label)),
                    if available {
                        Sense::click()
                    } else {
                        Sense::hover()
                    },
                )
                .on_hover_text(crate::i18n::tr(if available {
                    item.label
                } else {
                    "Esta sessão ainda não está disponível"
                }));
            let image_size = if large { 64.0 } else { 32.0 };
            let image_top = cell.top() + 17.0;
            if let Some(texture) = icons.home_icon(ui.ctx(), item.big, large, available) {
                painter.image(
                    texture.id(),
                    Rect::from_min_size(
                        pos2((cell.center().x - image_size / 2.0).floor(), image_top),
                        vec2(image_size, image_size),
                    ),
                    Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)),
                    Color32::WHITE,
                );
            }
            let caption = crate::i18n::tr(item.label);
            paint_text(
                &painter,
                pos2(
                    (cell.center().x - text_width(ui, &caption, 13.0) / 2.0).floor(),
                    image_top + image_size + 1.0,
                ),
                &caption,
                13.0,
                if available {
                    Color32::BLACK
                } else {
                    DISABLED_TEXT
                },
            );
            if response.clicked() {
                if let Some(kind) = item.kind {
                    *action = open_action(kind);
                }
            }
        }
    }
}

/// Six columns in the supplied BC5 desktop capture; wrap each band on
/// narrower windows without pagination. Geometry remains independent of history.
struct HomeGrid {
    left: f32,
    top: f32,
    columns: usize,
    primary_rows: usize,
}
impl HomeGrid {
    fn new(area: Rect) -> Self {
        let columns = ((area.width() / 138.0).floor() as usize).clamp(1, 6);
        let primary_rows = 6_usize.div_ceil(columns);
        let total = primary_rows as f32 * 115.0 + 28.0 + primary_rows as f32 * 81.0;
        // Placement fitted to the two native desktop captures, not a recovered
        // BC5 resize handler. Keep this limitation in the parity report.
        let top = (area.top() + area.height() / 6.0 + 120.0)
            .min(area.bottom() - total)
            .max(area.top());
        Self {
            left: (area.center().x - columns as f32 * 69.0).floor(),
            top: top.floor(),
            columns,
            primary_rows,
        }
    }
    fn cell(&self, index: usize) -> (Rect, bool) {
        let large = index < 6;
        let local = index % 6;
        let row = local / self.columns;
        let top = self.top
            + if large {
                row as f32 * 115.0
            } else {
                self.primary_rows as f32 * 115.0 + 28.0 + row as f32 * 81.0
            };
        (
            Rect::from_min_size(
                pos2(self.left + (local % self.columns) as f32 * 138.0, top),
                vec2(138.0, if large { 115.0 } else { 81.0 }),
            ),
            large,
        )
    }
}

fn hline(painter: &Painter, left: f32, right: f32, y: f32, color: Color32) {
    painter.rect_filled(
        Rect::from_min_max(pos2(left, y), pos2(right, y + 1.0)),
        0.0,
        color,
    );
}

fn dot(painter: &Painter, x: f32, y: f32) {
    painter.rect_filled(
        Rect::from_min_size(pos2(x, y), vec2(1.0, 1.0)),
        0.0,
        TREE_DOT,
    );
}

/// Dotted tree lines (TVS_HASLINES | TVS_LINESATROOT): one dot every 2 px counted from
/// each line's start (horizontal: from the level's centre column + 1; vertical: from the row
/// top), hidden under the chevron box (centre ± 8 px, rows 1..17) and under the selection.
fn paint_tree_lines(painter: &Painter, row: &Row, cx: f32, icon_x: f32, top: f32, selected: bool) {
    let line_x = cx + 1.0;
    let mid = top + 10.0;
    let in_box = |x: f32, y: f32| {
        row.has_children && (x - (cx + 0.5)).abs() <= 8.0 && y >= top + 1.0 && y < top + ROW
    };
    // Horizontal stub to the icon.
    let end = if selected { icon_x - 4.0 } else { icon_x - 1.0 };
    let mut x = line_x;
    while x <= end {
        if !in_box(x, mid) {
            dot(painter, x, mid);
        }
        x += 2.0;
    }
    // Own vertical: from the row top (from the middle for the very first root) to the
    // middle when last, else through the row.
    let start = if row.level == 0 && row.key == "new" {
        mid
    } else {
        top
    };
    let stop = if row.last { mid } else { top + ROW - 1.0 };
    let mut y = start;
    while y <= stop {
        if !in_box(line_x, y) {
            dot(painter, line_x, y);
        }
        y += 2.0;
    }
    // Ancestors that still have siblings below.
    for (level, &last) in row.ancestors_last.iter().enumerate() {
        if last {
            continue;
        }
        let ax = cx - INDENT * (row.ancestors_last.len() - level) as f32 + 1.0;
        let mut y = top;
        while y < top + ROW {
            dot(painter, ax, y);
            y += 2.0;
        }
    }
}

fn font(size: f32) -> FontId {
    FontId::proportional(size)
}

fn text_width(ui: &Ui, text: &str, size: f32) -> f32 {
    ui.fonts(|f| f.layout_no_wrap(text.to_owned(), font(size), Color32::BLACK))
        .size()
        .x
        .round()
}

fn paint_text(painter: &Painter, pos: Pos2, text: &str, size: f32, color: Color32) {
    let galley = painter.layout_no_wrap(text.to_owned(), font(size), color);
    painter.galley(painter.round_pos_to_pixels(pos), galley, color);
}

/// Windows 11 push button as drawn by the original (100%): 4 px rounded corners, fill
/// (253,253,253), border (208) with a (186) bottom edge; the default button has a blue
/// border (0,120,212) / (0,107,190).
fn win_button(
    ui: &mut Ui,
    rect: Rect,
    caption: Option<&str>,
    image: Option<&str>,
    icons: &mut IconManager,
    default: bool,
    enabled: bool,
) -> Response {
    let id = ui
        .id()
        .with(("win_button", rect.min.x as i32, rect.min.y as i32));
    let response = ui.interact(
        rect,
        id,
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    let painter = ui.painter();
    let (border, bottom) = if default && enabled {
        (DEFAULT_BORDER, DEFAULT_BORDER_BOTTOM)
    } else {
        (BUTTON_BORDER, BUTTON_BORDER_BOTTOM)
    };
    let fill = if enabled && response.is_pointer_button_down_on() {
        Color32::from_rgb(245, 245, 245)
    } else if enabled && response.hovered() {
        Color32::from_rgb(249, 249, 249)
    } else {
        BUTTON_FILL
    };
    let rect = painter.round_rect_to_pixels(rect);
    painter.rect_filled(rect, 4.0, bottom);
    painter.rect_filled(
        Rect::from_min_max(rect.min, pos2(rect.right(), rect.bottom() - 1.0)),
        4.0,
        border,
    );
    painter.rect_filled(rect.shrink(1.0), 3.0, fill);
    if let Some(name) = image {
        if let Some(texture) = icons.get_icon_with_state(ui.ctx(), name, enabled) {
            let icon = Rect::from_min_size(
                pos2(
                    rect.left() + ((rect.width() - 16.0) / 2.0).round(),
                    rect.top() + ((rect.height() - 16.0) / 2.0).round(),
                ),
                vec2(16.0, 16.0),
            );
            painter.image(
                texture.id(),
                icon,
                Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)),
                Color32::WHITE,
            );
        }
    }
    if let Some(caption) = caption {
        let caption = crate::i18n::tr(caption);
        let w = text_width(ui, &caption, 12.0);
        paint_text(
            ui.painter(),
            pos2(
                rect.left() + ((rect.width() - w) / 2.0).round(),
                rect.top() + 4.0,
            ),
            &caption,
            12.0,
            if enabled {
                Color32::BLACK
            } else {
                DISABLED_TEXT
            },
        );
    }
    response
}

/// Windows 11 search edit of the sessions panel: white, light border, dark underline and the
/// magnifier glyph at the left.
fn search_edit(ui: &mut Ui, rect: Rect, text: &mut String) {
    let painter = ui.painter();
    let rect = painter.round_rect_to_pixels(rect);
    painter.rect_filled(rect, 4.0, Color32::from_gray(236));
    painter.rect_filled(rect.shrink(1.0), 3.0, Color32::WHITE);
    hline(
        painter,
        rect.left() + 2.0,
        rect.right() - 2.0,
        rect.bottom() - 1.0,
        EDIT_UNDERLINE,
    );
    for (dy, row) in MAGNIFIER.iter().enumerate() {
        for (dx, &alpha) in row.iter().enumerate() {
            if alpha > 0 {
                painter.rect_filled(
                    Rect::from_min_size(
                        pos2(rect.left() + 3.0 + dx as f32, rect.top() + 4.0 + dy as f32),
                        vec2(1.0, 1.0),
                    ),
                    0.0,
                    Color32::from_rgba_unmultiplied(102, 102, 102, alpha),
                );
            }
        }
    }
    let inner = Rect::from_min_max(
        pos2(rect.left() + 24.0, rect.top() + 2.0),
        pos2(rect.right() - 4.0, rect.bottom() - 2.0),
    );
    let mut child = ui.new_child(
        UiBuilder::new()
            .max_rect(inner)
            .layout(Layout::left_to_right(Align::Center)),
    );
    child.add(
        TextEdit::singleline(text)
            .id(Id::new("home_session_search"))
            .frame(false)
            .desired_width(inner.width())
            .margin(vec2(0.0, 0.0)),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bc5_home_shows_all_twelve_types_without_pagination() {
        assert_eq!(PRIMARY.len() + SECONDARY.len(), 12);
        assert_eq!(PRIMARY[5].label, "Editar texto");
        assert_eq!(SECONDARY[0].kind, Some(SessionKind::Hex));
        assert_eq!(SECONDARY[4].kind, Some(SessionKind::Table));
        assert_eq!(
            PRIMARY
                .iter()
                .chain(SECONDARY.iter())
                .filter(|t| t.kind.is_some())
                .count(),
            8
        );
        for width in [700.0, 1153.0, 1919.0] {
            let area = Rect::from_min_max(
                pos2((width * 0.2_f32).floor() + 5.0, 27.0),
                pos2(width, 1000.0),
            );
            let grid = HomeGrid::new(area);
            for index in 0..12 {
                let (cell, large) = grid.cell(index);
                assert!(
                    area.contains_rect(cell),
                    "type {index} at width {width}: {cell:?}"
                );
                assert_eq!(large, index < 6);
                for previous in 0..index {
                    assert!(!cell
                        .shrink(0.1)
                        .intersects(grid.cell(previous).0.shrink(0.1)));
                }
            }
        }
    }

    #[test]
    fn auto_saved_groups_use_the_original_names() {
        assert_eq!(group_label(0), "Hoje");
        assert_eq!(group_label(1), "Ontem");
        assert_eq!(group_label(3), "3 dias atrás");
        assert_eq!(group_label(7), "Mais do que 6 dias atrás");
        assert_eq!(group_of(crate::history::now()), 0);
        assert_eq!(group_of(crate::history::now() - 30 * 86_400), 7);
    }
}
