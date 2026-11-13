use super::{
    chrome,
    dialogs::{DialogField, DialogHost},
    history_combo,
    icons::IconManager,
    session_menu, session_tabs,
    theme::BcTheme,
    toolbar::{self, ToolEvent, ToolState},
    toolbar_spec,
};
use crate::{
    i18n::tr,
    pe_version::{Image, Node},
    table::Filter,
    table_columns::Change,
};
use egui::*;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

mod property_context;

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
struct Options {
    filter: Filter,
    ignore_minor: bool,
    ignored: Vec<String>,
    closed: BTreeSet<String>,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            filter: Filter::All,
            ignore_minor: false,
            ignored: vec!["additional/sha256".into()],
            closed: BTreeSet::from(["additional".into()]),
        }
    }
}
struct Row {
    key: String,
    depth: usize,
    names: [Option<String>; 2],
    values: [Option<String>; 2],
    group: bool,
    change: Change,
}
fn flatten<'a>(
    nodes: &'a [Node],
    prefix: &str,
    depth: usize,
    result: &mut Vec<(String, usize, &'a Node)>,
) {
    for node in nodes {
        let key = if prefix.is_empty() {
            node.key.clone()
        } else {
            format!("{prefix}/{}", node.key)
        };
        result.push((key.clone(), depth, node));
        flatten(&node.children, &key, depth + 1, result);
    }
}
pub struct VersionCompareScreen {
    kind: crate::history::SessionKind,
    pub left_path: String,
    pub right_path: String,
    left: Option<Image>,
    right: Option<Image>,
    rows: Vec<Row>,
    options: Options,
    selected: usize,
    active_side: usize,
    jump: bool,
    error: Option<String>,
}
impl VersionCompareScreen {
    pub fn record_paths(&self) -> Option<(PathBuf, PathBuf)> {
        Some((
            self.left.as_ref()?.path.clone()?,
            self.right.as_ref()?.path.clone()?,
        ))
    }
    pub fn set_filter(&mut self, filter: Filter) {
        self.options.filter = filter;
    }
    pub fn new() -> Self {
        Self {
            kind: crate::history::SessionKind::Version,
            left_path: String::new(),
            right_path: String::new(),
            left: None,
            right: None,
            rows: Vec::new(),
            options: Options::default(),
            selected: 0,
            active_side: 0,
            jump: false,
            error: None,
        }
    }
    pub fn session_options(&self) -> serde_json::Value {
        serde_json::to_value(&self.options).unwrap_or_default()
    }
    pub fn for_kind(kind: crate::history::SessionKind) -> Self {
        let mut screen = Self::new();
        screen.kind = kind;
        if kind != crate::history::SessionKind::Version {
            screen.options.ignored = vec!["technical".into()];
            screen.options.closed = BTreeSet::from(["technical".into()]);
        }
        screen
    }
    pub fn kind(&self) -> crate::history::SessionKind {
        self.kind
    }
    fn tools(&self) -> &'static [super::toolbar::ToolItem] {
        match self.kind {
            crate::history::SessionKind::Registry => toolbar_spec::REGISTRY_COMPARE,
            crate::history::SessionKind::Media => toolbar_spec::MP3_COMPARE,
            _ => toolbar_spec::VERSION_COMPARE,
        }
    }
    pub fn apply_session_options(&mut self, value: &serde_json::Value) {
        if let Ok(options) = serde_json::from_value(value.clone()) {
            self.options = options;
            self.compare();
        }
    }
    pub fn load_files(&mut self, left: &Path, right: &Path) -> Result<(), String> {
        let a = crate::property_sources::load(self.kind, left)?;
        let b = crate::property_sources::load(self.kind, right)?;
        self.left = Some(a);
        self.right = Some(b);
        self.left_path = left.to_string_lossy().into_owned();
        self.right_path = right.to_string_lossy().into_owned();
        self.error = None;
        self.compare();
        Ok(())
    }
    pub fn set_error(&mut self, error: String) {
        self.error = Some(error);
    }
    pub fn refresh_source(&mut self, path: &Path, dialogs: &mut dyn DialogHost) {
        for side in [true, false] {
            let source = if side { &self.left } else { &self.right };
            if source
                .as_ref()
                .and_then(|s| s.path.as_deref())
                .is_some_and(|source| crate::document::same_file(source, path))
            {
                self.open_side(side, path, dialogs);
            }
        }
    }
    pub fn open_side(&mut self, left: bool, path: &Path, dialogs: &mut dyn DialogHost) {
        match crate::property_sources::load(self.kind, path) {
            Ok(image) => {
                if left {
                    self.left = Some(image);
                    self.left_path = path.to_string_lossy().into_owned();
                } else {
                    self.right = Some(image);
                    self.right_path = path.to_string_lossy().into_owned();
                }
                self.error = None;
                self.compare();
            }
            Err(error) => dialogs.error(&error),
        }
    }
    fn compare(&mut self) {
        let mut a = Vec::new();
        let mut b = Vec::new();
        if let Some(image) = &self.left {
            flatten(&image.nodes, "", 0, &mut a);
        }
        if let Some(image) = &self.right {
            flatten(&image.nodes, "", 0, &mut b);
        }
        let right: BTreeMap<_, _> = b
            .iter()
            .map(|(key, depth, n)| (key.as_str(), (*depth, *n)))
            .collect();
        let left: BTreeSet<_> = a.iter().map(|(k, _, _)| k.as_str()).collect();
        let mut rows = Vec::new();
        for (key, depth, n) in &a {
            let other = right.get(key.as_str()).map(|(_, n)| *n);
            let change = if other.is_some_and(|other| n.same(other)) {
                Change::Same
            } else if self
                .options
                .ignored
                .iter()
                .any(|ignored| key == ignored || key.starts_with(&format!("{ignored}/")))
            {
                Change::Minor
            } else {
                Change::Important
            };
            rows.push(Row {
                key: key.clone(),
                depth: *depth,
                names: [Some(n.name.clone()), other.map(|n| n.name.clone())],
                values: [Some(n.value.clone()), other.map(|n| n.value.clone())],
                group: !n.children.is_empty(),
                change,
            });
        }
        for (key, depth, n) in &b {
            if !left.contains(key.as_str()) {
                rows.push(Row {
                    key: key.clone(),
                    depth: *depth,
                    names: [None, Some(n.name.clone())],
                    values: [None, Some(n.value.clone())],
                    group: !n.children.is_empty(),
                    change: Change::Important,
                });
            }
        }
        for i in (0..rows.len()).rev() {
            if rows[i].group {
                let prefix = format!("{}/", rows[i].key);
                rows[i].change = rows
                    .iter()
                    .filter(|r| r.key.starts_with(&prefix))
                    .map(|r| r.change)
                    .max()
                    .unwrap_or(rows[i].change);
            }
        }
        self.rows = rows;
        self.selected = self.selected.min(self.rows.len().saturating_sub(1));
    }
    fn visible(&self) -> Vec<usize> {
        self.rows
            .iter()
            .enumerate()
            .filter_map(|(i, row)| {
                let changed = if self.options.ignore_minor {
                    row.change == Change::Important
                } else {
                    row.change != Change::Same
                };
                let filter = match self.options.filter {
                    Filter::All => true,
                    Filter::Differences => changed,
                    Filter::Same => !changed,
                };
                (filter
                    && !self
                        .options
                        .closed
                        .iter()
                        .any(|key| row.key.starts_with(&format!("{key}/"))))
                .then_some(i)
            })
            .collect()
    }
    fn navigate(&mut self, forward: bool) {
        let different: Vec<_> = self
            .rows
            .iter()
            .enumerate()
            .filter_map(|(i, row)| {
                (!row.group
                    && if self.options.ignore_minor {
                        row.change == Change::Important
                    } else {
                        row.change != Change::Same
                    })
                .then_some(i)
            })
            .collect();
        if different.is_empty() {
            return;
        }
        let index = if forward {
            different
                .iter()
                .copied()
                .find(|&i| i > self.selected)
                .unwrap_or(different[0])
        } else {
            different
                .iter()
                .rev()
                .copied()
                .find(|&i| i < self.selected)
                .unwrap_or(*different.last().unwrap())
        };
        self.selected = index;
        self.jump = true;
        let key = self.rows[index].key.clone();
        self.options
            .closed
            .retain(|closed| !key.starts_with(&format!("{closed}/")));
    }
    fn rules(&mut self, dialogs: &mut dyn DialogHost) {
        if self.kind != crate::history::SessionKind::Version {
            let keys: Vec<_> = self
                .rows
                .iter()
                .filter(|r| !r.group)
                .map(|r| {
                    (
                        r.key.clone(),
                        r.names[0]
                            .as_ref()
                            .or(r.names[1].as_ref())
                            .cloned()
                            .unwrap_or_default(),
                    )
                })
                .collect();
            let mut fields: Vec<_> = keys
                .iter()
                .map(|(key, name)| {
                    DialogField::Check(
                        property_name(self.kind, key, name),
                        !self.options.ignored.iter().any(|ignored| {
                            key == ignored || key.starts_with(&format!("{ignored}/"))
                        }),
                    )
                })
                .collect();
            if dialogs.fields("Regras de comparação", &mut fields) {
                self.options.ignored.clear();
                for ((key, _), field) in keys.into_iter().zip(fields) {
                    self.options.ignored.retain(|k| k != &key);
                    if matches!(field, DialogField::Check(_, false)) {
                        self.options.ignored.push(key);
                    }
                }
                self.compare();
            }
            return;
        }
        let keys = [
            ("code/data", "Comparar código e dados"),
            ("pe/timestamp", "Comparar data de criação"),
            ("pe/checksum", "Comparar soma de verificação"),
            ("pe/base", "Comparar base de imagem"),
            ("additional/sha256", "Comparar demais bytes do cabeçalho"),
        ];
        let mut fields: Vec<_> = keys
            .iter()
            .map(|(key, label)| {
                DialogField::Check(tr(label), !self.options.ignored.iter().any(|k| k == key))
            })
            .collect();
        if dialogs.fields("Regras de comparação", &mut fields) {
            for ((key, _), field) in keys.into_iter().zip(fields) {
                self.options.ignored.retain(|k| k != key);
                if matches!(field, DialogField::Check(_, false)) {
                    self.options.ignored.push(key.into());
                }
            }
            self.compare();
        }
    }
    fn pick(&mut self, left: bool, dialogs: &mut dyn DialogHost) {
        let path = PathBuf::from(if left {
            &self.left_path
        } else {
            &self.right_path
        });
        if let Some(path) = dialogs.file(false, left, Some(&path)) {
            self.open_side(left, &path, dialogs);
        }
    }
    pub fn ui(
        &mut self,
        ctx: &Context,
        ui: &mut Ui,
        icons: &mut IconManager,
        dialogs: &mut dyn DialogHost,
    ) -> bool {
        let mut home = false;
        ui.spacing_mut().item_spacing = Vec2::ZERO;
        TopBottomPanel::top("version_menu")
            .exact_height(chrome::MENU_BAND_HEIGHT)
            .show_separator_line(false)
            .frame(Frame::none())
            .show_inside(ui, |ui| {
                if self.kind == crate::history::SessionKind::Version {
                    chrome::menu_background(ui);
                } else {
                    ui.painter().rect_filled(ui.max_rect(), 0.0, Color32::WHITE);
                }
                chrome::menu_bar(ui, |ui| {
                    chrome::menu_button(ui, "Sessão", |ui| {
                        session_menu::prefix(ui, icons, true);
                        session_menu::close(ui, icons);
                        session_menu::suffix(
                            ui,
                            icons,
                            self.left.is_some() && self.right.is_some(),
                        );
                    });
                    chrome::menu_button(ui, "Arquivo (F)", |ui| {
                        for (label, left) in [
                            ("Abrir arquivo esquerdo...", true),
                            ("Abrir arquivo direito...", false),
                        ] {
                            if chrome::menu_item(ui, icons, label, true).clicked() {
                                self.pick(left, dialogs);
                                ui.close_menu();
                            }
                        }
                        if chrome::menu_item(
                            ui,
                            icons,
                            "Recarregar Arquivos  F5",
                            self.left.is_some() && self.right.is_some(),
                        )
                        .clicked()
                        {
                            self.reload(dialogs);
                            ui.close_menu();
                        }
                    });
                    chrome::menu_button(ui, "Editar", |ui| {
                        if chrome::menu_item(
                            ui,
                            icons,
                            "Copiar valor  Ctrl+C",
                            !self.rows.is_empty(),
                        )
                        .clicked()
                        {
                            self.copy(ctx);
                            ui.close_menu();
                        }
                    });
                    chrome::menu_button(ui, "Pesquisar", |ui| {
                        for (label, next) in
                            [("Próxima Diferença", true), ("Diferença Anterior", false)]
                        {
                            if chrome::menu_item(ui, icons, label, true).clicked() {
                                self.navigate(next);
                                ui.close_menu();
                            }
                        }
                    });
                    chrome::menu_button(ui, "Visualizar", |ui| {
                        for (label, filter) in [
                            ("Todos", Filter::All),
                            ("Diferenças", Filter::Differences),
                            ("Iguais", Filter::Same),
                        ] {
                            if ui
                                .selectable_label(self.options.filter == filter, tr(label))
                                .clicked()
                            {
                                self.options.filter = filter;
                                ui.close_menu();
                            }
                        }
                        if ui.button(tr("Expandir Todos")).clicked() {
                            self.options.closed.clear();
                            ui.close_menu();
                        }
                        if ui.button(tr("Recolher Todos")).clicked() {
                            self.options.closed = self
                                .rows
                                .iter()
                                .filter(|r| r.group)
                                .map(|r| r.key.clone())
                                .collect();
                            ui.close_menu();
                        }
                    });
                    chrome::menu_button(ui, "Ferramentas", |ui| {
                        if chrome::menu_item(ui, icons, "Regras de comparação...", true).clicked()
                        {
                            self.rules(dialogs);
                            ui.close_menu();
                        }
                        if chrome::menu_item(ui, icons, "Opções...", true).clicked() {
                            crate::i18n::configure(dialogs);
                            ui.close_menu();
                        }
                        session_menu::settings_tools(ui, icons, dialogs);
                    });
                    chrome::menu_button(ui, "Ajuda (H)", |ui| {
                        if chrome::menu_item(ui, icons, "Sobre...", true).clicked() {
                            dialogs.about();
                            ui.close_menu();
                        }
                    });
                });
            });
        session_tabs::show(ui, icons);
        let state = |name: &str| match name {
            "FavCompareAll" => ToolState::checked(self.options.filter == Filter::All),
            "FavDiff" => ToolState::checked(self.options.filter == Filter::Differences),
            "FavSame" => ToolState::checked(self.options.filter == Filter::Same),
            "InsigIgnored" => ToolState::checked(self.options.ignore_minor),
            "SessionHome" | "SessionsCombo" | "Rules" | "Swap" => ToolState::enabled(true),
            "Reload" => ToolState::enabled(self.left.is_some() && self.right.is_some()),
            "DiffItemNext" | "DiffItemPrev" => {
                ToolState::enabled(self.rows.iter().any(|r| r.change != Change::Same))
            }
            _ => ToolState::DISABLED,
        };
        let height = toolbar::height(ui, self.tools(), ui.available_width(), &state);
        let mut event = None;
        TopBottomPanel::top("version_toolbar")
            .exact_height(height)
            .show_separator_line(false)
            .frame(Frame::none())
            .show_inside(ui, |ui| {
                if self.kind != crate::history::SessionKind::Version {
                    ui.data_mut(|d| d.insert_temp(Id::new("bc5_flat_toolbar"), true));
                }
                event = toolbar::show(ui, icons, self.tools(), true, &state, &mut |_, _, _| {});
                ui.data_mut(|d| d.remove::<bool>(Id::new("bc5_flat_toolbar")));
            });
        if let Some(ToolEvent::Click(command)) = event {
            match command {
                "SessionHome" => home = true,
                "FavCompareAll" => self.options.filter = Filter::All,
                "FavDiff" => self.options.filter = Filter::Differences,
                "FavSame" => self.options.filter = Filter::Same,
                "InsigIgnored" => self.options.ignore_minor = !self.options.ignore_minor,
                "Rules" => self.rules(dialogs),
                "DiffItemNext" => self.navigate(true),
                "DiffItemPrev" => self.navigate(false),
                "Reload" => self.reload(dialogs),
                "Swap" => {
                    std::mem::swap(&mut self.left, &mut self.right);
                    std::mem::swap(&mut self.left_path, &mut self.right_path);
                    self.compare();
                }
                _ => {}
            }
        }
        if !ctx.wants_keyboard_input() && !super::session_picker::is_open(ctx) {
            if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::F5)) {
                self.reload(dialogs);
            }
            if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::C)) {
                self.copy(ctx);
            }
            if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::O)) {
                self.pick(self.active_side == 0, dialogs);
            }
            if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::F7)) {
                self.navigate(true);
            }
        }
        ui.horizontal(|ui| {
            let width = (ui.available_width() - 6.0) / 2.0;
            for left in [true, false] {
                ui.allocate_ui_with_layout(
                    vec2(width, 23.0),
                    Layout::left_to_right(Align::Center),
                    |ui| {
                        ui.set_min_width(width);
                        ui.set_max_width(width);
                        ui.set_clip_rect(ui.max_rect().intersect(ui.clip_rect()));
                        let path = if left {
                            &mut self.left_path
                        } else {
                            &mut self.right_path
                        };
                        let response = history_combo::show(
                            ui,
                            ("version_path", left),
                            path,
                            &[],
                            (width - 25.0).max(30.0),
                            "Entre o nome do arquivo aqui",
                        );
                        let typed = path.clone();
                        if let Some(path) = response.selected {
                            self.open_side(left, Path::new(&path), dialogs);
                        } else if response.submitted {
                            self.open_side(left, Path::new(&typed), dialogs);
                        }
                        if let Some(icon) = icons.get_icon(ui.ctx(), "imgBrowse") {
                            if ui
                                .add(ImageButton::new((icon.id(), Vec2::splat(16.0))).frame(false))
                                .clicked()
                            {
                                self.pick(left, dialogs);
                            }
                        }
                    },
                );
                if left {
                    ui.add_space(6.0);
                }
            }
        });
        ui.horizontal(|ui| {
            let width = (ui.available_width() - 6.0) / 2.0;
            for image in [&self.left, &self.right] {
                ui.add_sized(
                    [width, 22.0],
                    Label::new(image.as_ref().map_or(String::new(), Image::metadata_label))
                        .wrap_mode(TextWrapMode::Truncate),
                );
                ui.add_space(6.0);
            }
        });
        if let Some(error) = &self.error {
            ui.colored_label(Color32::RED, error);
        }
        let visible = self.visible();
        let width = (ui.available_width() - 6.0) / 2.0;
        ui.horizontal(|ui| {
            for _ in 0..2 {
                let (rect, _) = ui.allocate_exact_size(vec2(width, 22.0), Sense::hover());
                ui.painter().rect_filled(rect, 0.0, Color32::WHITE);
                paint(
                    ui,
                    Rect::from_min_size(rect.min, vec2(210.0, 22.0)),
                    &tr("Nome"),
                    Color32::BLACK,
                );
                paint(
                    ui,
                    Rect::from_min_max(rect.min + vec2(210.0, 0.0), rect.max),
                    &tr("Valor"),
                    Color32::BLACK,
                );
                ui.add_space(6.0);
            }
        });
        let height = (ui.available_height() - 24.0).max(40.0);
        let background = Rect::from_min_size(ui.cursor().min, vec2(width * 2.0 + 6.0, height));
        ui.painter().rect_filled(background, 0.0, Color32::WHITE);
        ui.painter().line_segment(
            [
                background.min + vec2(width + 3.0, 0.0),
                background.min + vec2(width + 3.0, height),
            ],
            Stroke::new(1.0_f32, Color32::from_gray(160)),
        );

        let mut toggle = None;
        let mut context_command = None;
        let first_leaf = self.rows.iter().position(|row| !row.group);
        ScrollArea::both()
            .id_salt("version_tree")
            .auto_shrink([false, false])
            .max_height(height)
            .show_rows(ui, 16.0, visible.len(), |ui, range| {
                for visual in range {
                    let index = visible[visual];
                    let row = &self.rows[index];
                    let (rect, response) =
                        ui.allocate_exact_size(vec2(width * 2.0 + 6.0, 16.0), Sense::hover());
                    if index == self.selected && self.jump {
                        response.scroll_to_me(Some(Align::Center));
                    }
                    for side in 0..2 {
                        let pane = Rect::from_min_size(
                            rect.min + vec2(side as f32 * (width + 6.0), 0.0),
                            vec2(width, 16.0),
                        );
                        let cell = ui.interact(
                            pane,
                            Id::new("property_row").with((index, side)),
                            Sense::click(),
                        );
                        if cell.clicked() || cell.secondary_clicked() {
                            self.selected = index;
                            self.active_side = side;
                            if cell.clicked() && row.group {
                                toggle = Some(row.key.clone());
                            }
                        }
                        #[cfg(test)]
                        ui.data_mut(|d| {
                            d.insert_temp(Id::new("property_test_row").with((index, side)), pane)
                        });
                        if first_leaf == Some(index) {
                            let label = if side == 0 {
                                "Menu de propriedades"
                            } else {
                                "Menu de propriedades direito"
                            };
                            ui.data_mut(|d| {
                                d.insert_temp(Id::new("capture_menu_rect").with(label), pane)
                            });
                        }
                        cell.context_menu(|ui| {
                            context_command = property_context::menu(
                                ui,
                                icons,
                                row,
                                property_context::Target {
                                    kind: self.kind,
                                    side,
                                    important: !self.options.ignored.iter().any(|ignored| {
                                        row.key == *ignored
                                            || row.key.starts_with(&format!("{ignored}/"))
                                    }),
                                    loaded: if side == 0 {
                                        self.left.is_some()
                                    } else {
                                        self.right.is_some()
                                    },
                                },
                            )
                            .map(|command| (index, side, command));
                        });
                        let bg = if index == self.selected {
                            Color32::from_rgb(211, 228, 239)
                        } else if visual % 2 == 0 {
                            Color32::from_gray(248)
                        } else {
                            Color32::WHITE
                        };
                        ui.painter().rect_filled(pane, 0.0, bg);
                        let color = match row.change {
                            Change::Same => Color32::BLACK,
                            Change::Minor if self.options.ignore_minor => Color32::BLACK,
                            Change::Minor => BcTheme::MINOR_TEXT,
                            Change::Important => BcTheme::DIFF_TEXT,
                        };
                        let name = row.names[side]
                            .as_ref()
                            .map(|name| {
                                format!(
                                    "{}{}",
                                    if row.group {
                                        if self.options.closed.contains(&row.key) {
                                            "▸ "
                                        } else {
                                            "▾ "
                                        }
                                    } else {
                                        ""
                                    },
                                    property_name(self.kind, &row.key, name)
                                )
                            })
                            .unwrap_or_default();
                        let indent = row.depth as f32 * 18.0;
                        paint(
                            ui,
                            Rect::from_min_max(
                                pane.min + vec2(indent, 0.0),
                                pane.min + vec2(209.0, 16.0),
                            ),
                            &name,
                            color,
                        );
                        paint(
                            ui,
                            Rect::from_min_max(pane.min + vec2(210.0, 0.0), pane.max),
                            &property_value(
                                self.kind,
                                &row.key,
                                row.values[side].as_deref().unwrap_or(""),
                            ),
                            color,
                        );
                        ui.painter().line_segment(
                            [pane.left_bottom(), pane.right_bottom()],
                            Stroke::new(1.0_f32, Color32::from_gray(230)),
                        );
                    }
                }
            });
        self.jump = false;
        if let Some((index, side, command)) = context_command {
            self.selected = index;
            self.active_side = side;
            self.context_command(command, ctx, dialogs);
        }
        if let Some(key) = toggle {
            if !self.options.closed.remove(&key) {
                self.options.closed.insert(key);
            }
        }
        let changed = self
            .rows
            .iter()
            .filter(|r| {
                !r.group
                    && if self.options.ignore_minor {
                        r.change == Change::Important
                    } else {
                        r.change != Change::Same
                    }
            })
            .count();
        ui.separator();
        ui.label(
            if self
                .left
                .as_ref()
                .zip(self.right.as_ref())
                .is_some_and(|(a, b)| a.sha256 == b.sha256)
            {
                tr("Mesmo binário")
            } else if changed == 0 {
                tr("Iguais")
            } else {
                format!("{changed} {}", tr("itens diferentes"))
            },
        );
        home
    }
    fn reload(&mut self, dialogs: &mut dyn DialogHost) {
        let a = PathBuf::from(&self.left_path);
        let b = PathBuf::from(&self.right_path);
        if let Err(error) = self.load_files(&a, &b) {
            dialogs.error(&error);
        }
    }
    fn copy(&self, ctx: &Context) {
        if let Some(row) = self.rows.get(self.selected) {
            if let Some(value) = &row.values[self.active_side] {
                ctx.copy_text(property_value(self.kind, &row.key, value));
            }
        }
    }
}
// Stored field names are locale-independent keys. Registry identifiers are user
// data and must never be run through the application's translation catalog.
fn property_name(kind: crate::history::SessionKind, key: &str, name: &str) -> String {
    if kind == crate::history::SessionKind::Registry && !key.ends_with("/value:") {
        name.to_owned()
    } else {
        tr(name)
    }
}
fn property_value(kind: crate::history::SessionKind, key: &str, value: &str) -> String {
    if kind == crate::history::SessionKind::Media && key == "audio/data" {
        if let Some(number) = value
            .strip_suffix(" bytes")
            .and_then(|n| n.parse::<usize>().ok())
        {
            return format!("{} bytes", crate::property_sources::display_number(number));
        }
    }
    value.to_owned()
}

#[cfg(test)]
mod locale_tests {
    use super::*;
    use crate::history::SessionKind;

    #[test]
    #[ignore = "Locale changes require their own test process"]
    fn locale_switch_properties_hidden() {
        crate::i18n::initialize(Some("en"));
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("vendor/bc5-prototypes/RUST_APP/crates/bc5-media/tests/fixtures/A.mp3");
        let mut screen = VersionCompareScreen::for_kind(SessionKind::Media);
        screen.load_files(&fixture, &fixture).unwrap();
        let data = screen.rows.iter().find(|r| r.key == "audio/data").unwrap();
        assert_eq!(
            data.names[0].as_deref(),
            Some("Dados"),
            "reader must preserve a language-independent key"
        );
        assert_eq!(
            property_name(screen.kind, &data.key, data.names[0].as_deref().unwrap()),
            "Data"
        );
        assert_eq!(
            property_value(screen.kind, &data.key, data.values[0].as_deref().unwrap()),
            "2,689 bytes"
        );
        // Switch the already loaded session. Never reopen or reread a personal file.
        crate::i18n::initialize(Some("pt-BR"));
        assert_eq!(
            property_name(screen.kind, &data.key, data.names[0].as_deref().unwrap()),
            "Dados"
        );
        assert_eq!(
            property_value(screen.kind, &data.key, data.values[0].as_deref().unwrap()),
            "2.689 bytes"
        );
        crate::i18n::initialize(Some("en"));
        for literal in ["Dados", "Áudio", "Título", "(Padrão)"] {
            assert_eq!(
                property_name(
                    SessionKind::Registry,
                    "hkey_current_user/software/test/value:nome",
                    literal
                ),
                literal
            );
        }
        assert_eq!(
            property_name(
                SessionKind::Registry,
                "hkey_current_user/software/test/value:",
                "(Padrão)"
            ),
            "(Default)"
        );
        crate::i18n::initialize(Some("pt-BR"));
    }
}
fn paint(ui: &Ui, rect: Rect, text: &str, color: Color32) {
    if rect.width() <= 0.0 {
        return;
    }
    ui.painter()
        .with_clip_rect(rect.intersect(ui.clip_rect()))
        .text(
            rect.left_center() + vec2(4.0, 0.0),
            Align2::LEFT_CENTER,
            text,
            FontId::proportional(12.0),
            color,
        );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::dialogs::TestDialogs;
    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/bc5")
            .join(name)
    }
    #[test]
    fn version_tree_compares_values_and_preserves_previous_files_after_a_load_error() {
        let mut screen = VersionCompareScreen::new();
        screen
            .load_files(&fixture("pe32.bin"), &fixture("pe32-changed.bin"))
            .unwrap();
        let changes: Vec<_> = screen
            .rows
            .iter()
            .filter(|r| !r.group && r.change == Change::Important)
            .collect();
        assert_eq!(changes.len(), 1);
        assert!(changes[0].key.contains("FileDescription"));
        assert_eq!(
            screen
                .rows
                .iter()
                .find(|r| r.key == "code/data")
                .unwrap()
                .change,
            Change::Same
        );
        screen.navigate(true);
        assert!(screen.rows[screen.selected].key.contains("FileDescription"));
        assert!(screen.jump);
        let left = screen.left_path.clone();
        let count = screen.rows.len();
        let mut dialogs = TestDialogs::default();
        screen.open_side(true, &fixture("missing.dll"), &mut dialogs);
        assert_eq!(screen.left_path, left);
        assert_eq!(screen.rows.len(), count);
        assert_eq!(dialogs.errors.len(), 1);
        let saved = screen.session_options();
        let mut restored = VersionCompareScreen::new();
        restored.apply_session_options(&saved);
        assert_eq!(restored.options.ignored, screen.options.ignored);
    }
    #[test]
    fn version_tree_renders_at_small_and_large_sizes() {
        for size in [
            vec2(700.0, 450.0),
            vec2(1150.0, 720.0),
            vec2(1910.0, 1000.0),
        ] {
            let mut screen = VersionCompareScreen::new();
            screen
                .load_files(&fixture("pe32.bin"), &fixture("pe64.bin"))
                .unwrap();
            let ctx = Context::default();
            super::super::theme::apply(&ctx);
            let mut icons = IconManager::new();
            let mut dialogs = TestDialogs::default();
            for _ in 0..2 {
                let _ = ctx.run(
                    RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
                        ..Default::default()
                    },
                    |ctx| {
                        CentralPanel::default().show(ctx, |ui| {
                            screen.ui(ctx, ui, &mut icons, &mut dialogs);
                        });
                    },
                );
            }
            assert!(dialogs.errors.is_empty());
        }
    }
}
