//! Single-file editor. This is deliberately a separate session, not a Text
//! Compare with an empty or duplicate second pane.
use super::{
    chrome,
    dialogs::{DialogField, DialogHost, SaveChoice},
    history_combo,
    icons::IconManager,
    session_menu, session_tabs,
    toolbar::{self, ToolEvent, ToolItem, ToolKind, ToolState},
};
use crate::{
    document::{Document, Encoding, LineEnding},
    i18n::tr,
};
use egui::*;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

mod navigation;
mod search;

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct Options {
    wrap: bool,
}

#[derive(Clone)]
struct EditPoint {
    text: String,
    cursor: egui::text::CCursorRange,
}

#[derive(Clone, Copy)]
enum Action {
    Open,
    Save(bool),
    Reload,
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    Delete,
    All,
    Find(bool),
    FindNext(bool),
    Replace,
    ReplaceAll,
    GoTo,
    NextEdit(bool),
    ToggleBookmark(usize),
    GoToBookmark(usize),
    ClearBookmarks,
}

pub struct TextEditScreen {
    pub document: Document,
    path: String,
    pub read_only: bool,
    options: Options,
    editor_id: Id,
    focus: bool,
    saved_source: Option<PathBuf>,
    undo_flags: (bool, bool),
    undo: Vec<EditPoint>,
    redo: Vec<EditPoint>,
    search: search::State,
    scroll_to_cursor: bool,
    navigation: navigation::Navigation,
}
impl TextEditScreen {
    pub fn new() -> Self {
        Self {
            document: Document::default(),
            path: String::new(),
            read_only: false,
            options: Options::default(),
            editor_id: Id::NULL,
            focus: true,
            saved_source: None,
            undo_flags: (false, false),
            undo: Vec::new(),
            redo: Vec::new(),
            search: search::State::default(),
            scroll_to_cursor: false,
            navigation: Default::default(),
        }
    }
    pub fn load(&mut self, path: &Path) -> Result<(), String> {
        let doc = Document::load(path)?;
        self.document = doc;
        self.path = path.to_string_lossy().into_owned();
        self.focus = true;
        self.undo_flags = (false, false);
        self.undo.clear();
        self.redo.clear();
        self.navigation = Default::default();
        self.search.last = None;
        Ok(())
    }
    pub fn session_options(&self) -> serde_json::Value {
        serde_json::to_value(&self.options).unwrap_or_default()
    }
    pub fn take_saved_source(&mut self) -> Option<PathBuf> {
        self.saved_source.take()
    }
    pub fn apply_session_options(&mut self, value: &serde_json::Value) {
        if let Ok(options) = serde_json::from_value(value.clone()) {
            self.options = options;
        }
    }
    pub fn confirm_saved(&mut self, dialogs: &mut dyn DialogHost) -> bool {
        if !self.document.dirty() {
            return true;
        }
        match dialogs.confirm_save_file(self.document.path.as_deref()) {
            SaveChoice::Save => self.save(false, dialogs),
            SaveChoice::Discard => true,
            SaveChoice::Cancel => false,
        }
    }
    pub fn save(&mut self, save_as: bool, dialogs: &mut dyn DialogHost) -> bool {
        if self.read_only {
            return false;
        }
        let path = if save_as || self.document.path.is_none() {
            let name = self
                .document
                .path
                .as_ref()
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            dialogs.named_file(
                "Salvar Arquivo Como...",
                true,
                self.document.path.as_deref(),
                &name,
            )
        } else {
            self.document.path.clone()
        };
        let Some(path) = path else {
            return false;
        };
        // Change a candidate only: cancelling an encoding warning must preserve
        // the buffer, encoding and dirty state of the editing session.
        let mut candidate = self.document.clone();
        if candidate.encoding == Encoding::Windows1252
            && encoding_rs::WINDOWS_1252.encode(&candidate.text).2
        {
            let encodings: Vec<_> = Encoding::ALL
                .into_iter()
                .filter(|e| *e != Encoding::Windows1252)
                .collect();
            let mut fields = [DialogField::Notice(tr("Este arquivo contém caracteres Unicode que seriam perdidos com a codificação atual. Selecione uma codificação para preservar o texto.")), DialogField::Choice(tr("Codificação:"), 0, encodings.iter().map(|e| e.label().to_owned()).collect())];
            if !dialogs.fields_with_accept("Erro de codificação", &mut fields, "Salvar") {
                return false;
            }
            if let DialogField::Choice(_, index, _) = fields[1] {
                candidate.encoding = encodings[index.min(encodings.len() - 1)];
            }
        }
        match candidate.save(&path) {
            Ok(()) => {
                self.document = candidate;
                self.path = path.to_string_lossy().into_owned();
                self.saved_source = Some(path);
                self.navigation.invalidate_edits();
                true
            }
            Err(error) => {
                dialogs.error(&error);
                false
            }
        }
    }
    fn edit_point(&self, ctx: &Context) -> EditPoint {
        EditPoint {
            text: self.document.text.clone(),
            cursor: TextEdit::load_state(ctx, self.editor_id)
                .and_then(|s| s.cursor.char_range())
                .unwrap_or_default(),
        }
    }
    fn remember_change(&mut self, before: EditPoint) {
        if before.text == self.document.text {
            return;
        }
        self.navigation
            .text_changed(&before.text, &self.document.text);
        self.undo.push(before);
        let mut bytes: usize = self.undo.iter().map(|p| p.text.len()).sum();
        while self.undo.len() > 1 && (self.undo.len() > 50 || bytes > 32 * 1024 * 1024) {
            bytes -= self.undo.remove(0).text.len();
        }
        self.redo.clear();
        self.undo_flags = (!self.undo.is_empty(), false);
    }
    fn run(&mut self, action: Action, ctx: &Context, dialogs: &mut dyn DialogHost) {
        match action {
            Action::Find(replace) => self.show_search(ctx, replace),
            Action::FindNext(forward) => self.find(ctx, forward),
            Action::Replace => self.replace_found(ctx),
            Action::ReplaceAll => self.replace_all(ctx),
            Action::GoTo => self.go_to(ctx, dialogs),
            Action::NextEdit(forward) => {
                if let Some(line) = self.edited_targets(ctx)[usize::from(!forward)] {
                    self.jump_to_line(ctx, line);
                }
            }
            Action::ToggleBookmark(number) => {
                let line = self.cursor_line(ctx);
                let mark = &mut self.navigation.bookmarks[number];
                *mark = (*mark != Some(line)).then_some(line);
                self.focus = true;
            }
            Action::GoToBookmark(number) => {
                if let Some(line) = self.navigation.bookmarks[number] {
                    self.jump_to_line(ctx, line);
                }
            }
            Action::ClearBookmarks => {
                self.navigation.bookmarks.fill(None);
                self.focus = true;
            }
            Action::Save(save_as) => {
                self.save(save_as, dialogs);
            }
            Action::Open => {
                if self.confirm_saved(dialogs) {
                    if let Some(path) = dialogs.named_file(
                        "Abrir Arquivo...",
                        false,
                        self.document.path.as_deref(),
                        "",
                    ) {
                        if let Err(e) = self.load(&path) {
                            dialogs.error(&e);
                        } else {
                            self.reset_state(ctx);
                        }
                    }
                }
            }
            Action::Reload => {
                if self.confirm_saved(dialogs) {
                    if let Some(path) = self.document.path.clone() {
                        if let Err(e) = self.load(&path) {
                            dialogs.error(&e);
                        } else {
                            self.reset_state(ctx);
                        }
                    }
                }
            }
            Action::Undo | Action::Redo => {
                if self.read_only {
                    return;
                }
                let current = self.edit_point(ctx);
                let next = if matches!(action, Action::Undo) {
                    self.undo.pop()
                } else {
                    self.redo.pop()
                };
                if let Some(next) = next {
                    self.navigation.text_changed(&current.text, &next.text);
                    if matches!(action, Action::Undo) {
                        self.redo.push(current);
                    } else {
                        self.undo.push(current);
                    }
                    self.document.text = next.text;
                    let mut state = TextEdit::load_state(ctx, self.editor_id).unwrap_or_default();
                    state.cursor.set_char_range(Some(next.cursor));
                    state.clear_undoer();
                    state.store(ctx, self.editor_id);
                    self.search.last = None;
                    self.focus = true;
                    self.scroll_to_cursor = true;
                }
                self.undo_flags = (!self.undo.is_empty(), !self.redo.is_empty());
            }
            action => {
                let before = if matches!(action, Action::Cut | Action::Delete | Action::Paste)
                    && !self.read_only
                {
                    Some(self.edit_point(ctx))
                } else {
                    None
                };
                let mut state = TextEdit::load_state(ctx, self.editor_id).unwrap_or_default();
                let cursors = state.cursor.char_range().unwrap_or_default().sorted();
                let range = cursors[0].index..cursors[1].index;
                let offset = |n| {
                    self.document
                        .text
                        .char_indices()
                        .nth(n)
                        .map_or(self.document.text.len(), |(i, _)| i)
                };
                let mut bytes = offset(range.start)..offset(range.end);
                if matches!(action, Action::Delete)
                    && bytes.is_empty()
                    && bytes.end < self.document.text.len()
                {
                    bytes.end += self.document.text[bytes.start..]
                        .chars()
                        .next()
                        .unwrap()
                        .len_utf8();
                }
                match action {
                    Action::All => {
                        state
                            .cursor
                            .set_char_range(Some(egui::text::CCursorRange::two(
                                egui::text::CCursor::new(0),
                                egui::text::CCursor::new(self.document.text.chars().count()),
                            )));
                    }
                    Action::Copy => ctx.copy_text(self.document.text[bytes].to_owned()),
                    Action::Cut | Action::Delete if !self.read_only => {
                        self.search.last = None;
                        if matches!(action, Action::Cut) {
                            ctx.copy_text(self.document.text[bytes.clone()].to_owned());
                        }
                        self.document.text.replace_range(bytes, "");
                        state
                            .cursor
                            .set_char_range(Some(egui::text::CCursorRange::one(
                                egui::text::CCursor::new(range.start),
                            )));
                    }
                    Action::Paste if !self.read_only => {
                        self.search.last = None;
                        if let Ok(text) = arboard::Clipboard::new().and_then(|mut c| c.get_text()) {
                            let text = text.replace("\r\n", "\n").replace('\r', "\n");
                            self.document.text.replace_range(bytes, &text);
                            state
                                .cursor
                                .set_char_range(Some(egui::text::CCursorRange::one(
                                    egui::text::CCursor::new(range.start + text.chars().count()),
                                )));
                        }
                    }
                    _ => {}
                }
                state.store(ctx, self.editor_id);
                if let Some(before) = before {
                    self.remember_change(before);
                }
                self.focus = true;
            }
        }
    }
    fn reset_state(&self, ctx: &Context) {
        egui::text_edit::TextEditState::default().store(ctx, self.editor_id);
    }
    pub fn ui(
        &mut self,
        ctx: &Context,
        ui: &mut Ui,
        icons: &mut IconManager,
        dialogs: &mut dyn DialogHost,
    ) -> bool {
        self.editor_id = ui.id().with("single_file_editor");
        // Find/Go To move the cursor but are not edits. Use a text-only history
        // for document Undo; the search fields retain their own widget Undo.
        if ctx.memory(|m| m.has_focus(self.editor_id)) {
            for (modifiers, key, command) in [
                (Modifiers::CTRL | Modifiers::SHIFT, Key::Z, Action::Redo),
                (Modifiers::CTRL, Key::Y, Action::Redo),
                (Modifiers::CTRL, Key::Z, Action::Undo),
            ] {
                if ctx.input_mut(|i| i.consume_key(modifiers, key)) {
                    self.run(command, ctx, dialogs);
                }
            }
        }
        if !super::session_picker::is_open(ctx) && !ctx.is_context_menu_open() {
            for (modifiers, key, command) in [
                (Modifiers::CTRL, Key::F, Action::Find(false)),
                (Modifiers::CTRL, Key::H, Action::Find(true)),
                (Modifiers::CTRL, Key::G, Action::GoTo),
                (Modifiers::SHIFT, Key::F3, Action::FindNext(false)),
                (Modifiers::NONE, Key::F3, Action::FindNext(true)),
            ] {
                if ctx.input_mut(|i| i.consume_key(modifiers, key)) {
                    self.run(command, ctx, dialogs);
                }
            }
            if self.search.shown && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
                self.search.shown = false;
                self.focus = true;
            }
        }
        let (undo, redo) = self.undo_flags;
        let mut action = None;
        let mut home = false;
        ui.spacing_mut().item_spacing = Vec2::ZERO;
        TopBottomPanel::top("text_edit_menu")
            .exact_height(chrome::MENU_BAND_HEIGHT)
            .frame(Frame::none())
            .show_inside(ui, |ui| {
                ui.painter().rect_filled(ui.max_rect(), 0.0, Color32::WHITE);
                chrome::menu_bar(ui, |ui| {
                    chrome::menu_button(ui, "Sessão", |ui| {
                        session_menu::prefix(ui, icons, true);
                        session_menu::close(ui, icons);
                    });
                    chrome::menu_button(ui, "Arquivo (F)", |ui| {
                        for (label, enabled, command) in [
                            ("Abrir Arquivo...  Ctrl+O", true, Action::Open),
                            (
                                "Salvar Arquivo  Ctrl+S",
                                !self.read_only && self.document.dirty(),
                                Action::Save(false),
                            ),
                            (
                                "Salvar Arquivo Como...  F12",
                                !self.read_only,
                                Action::Save(true),
                            ),
                            (
                                "Recarregar  F5",
                                self.document.path.is_some(),
                                Action::Reload,
                            ),
                        ] {
                            if chrome::menu_item(ui, icons, label, enabled).clicked() {
                                action = Some(command);
                                ui.close_menu();
                            }
                        }
                    });
                    chrome::menu_button(ui, "Editar", |ui| {
                        for (label, enabled, command) in [
                            ("Desfazer  Ctrl+Z", !self.read_only && undo, Action::Undo),
                            ("Refazer  Ctrl+Y", !self.read_only && redo, Action::Redo),
                            ("Recortar  Ctrl+X", !self.read_only, Action::Cut),
                            ("Copiar  Ctrl+C", true, Action::Copy),
                            ("Colar  Ctrl+V", !self.read_only, Action::Paste),
                            ("Excluir", !self.read_only, Action::Delete),
                            ("Selecionar tudo  Ctrl+A", true, Action::All),
                        ] {
                            if chrome::menu_item(ui, icons, label, enabled).clicked() {
                                action = Some(command);
                                ui.close_menu();
                            }
                        }
                    });
                    chrome::menu_button(ui, "Pesquisar", |ui| {
                        let targets = self.edited_targets(ctx);
                        for (label, enabled, command) in [
                            ("Localizar...  Ctrl+F", true, Action::Find(false)),
                            ("Substituir...  Ctrl+H", !self.read_only, Action::Find(true)),
                            ("Localizar próximo  F3", true, Action::FindNext(true)),
                            (
                                "Localizar anterior  Shift+F3",
                                true,
                                Action::FindNext(false),
                            ),
                            ("Ir para...  Ctrl+G", true, Action::GoTo),
                            (
                                "Próxima edição",
                                targets[0].is_some(),
                                Action::NextEdit(true),
                            ),
                            (
                                "Edição anterior",
                                targets[1].is_some(),
                                Action::NextEdit(false),
                            ),
                        ] {
                            if chrome::menu_item(ui, icons, label, enabled).clicked() {
                                action = Some(command);
                                ui.close_menu();
                            }
                        }
                        ui.separator();
                        let line = self.cursor_line(ctx);
                        chrome::menu_button(ui, "Alternar marcador", |ui| {
                            for (number, mark) in self.navigation.bookmarks.iter().enumerate() {
                                if ui
                                    .selectable_label(*mark == Some(line), number.to_string())
                                    .clicked()
                                {
                                    action = Some(Action::ToggleBookmark(number));
                                    ui.close_menu();
                                }
                            }
                        });
                        chrome::menu_button(ui, "Ir para marcador", |ui| {
                            for (number, mark) in self.navigation.bookmarks.iter().enumerate() {
                                if ui
                                    .add_enabled(mark.is_some(), Button::new(number.to_string()))
                                    .clicked()
                                {
                                    action = Some(Action::GoToBookmark(number));
                                    ui.close_menu();
                                }
                            }
                        });
                        if chrome::menu_item(
                            ui,
                            icons,
                            "Limpar marcadores",
                            self.navigation.bookmarks.iter().any(Option::is_some),
                        )
                        .clicked()
                        {
                            action = Some(Action::ClearBookmarks);
                            ui.close_menu();
                        }
                    });
                    chrome::menu_button(ui, "Visualizar", |ui| {
                        if ui
                            .checkbox(&mut self.options.wrap, tr("Quebra automática de linha"))
                            .clicked()
                        {
                            ui.close_menu();
                        }
                    });
                    chrome::menu_button(ui, "Ferramentas", |ui| {
                        if chrome::menu_item(ui, icons, "Opções...", true).clicked() {
                            crate::i18n::configure(dialogs);
                            ui.close_menu();
                        }
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
            "Undo" => ToolState::enabled(!self.read_only && undo),
            "Redo" => ToolState::enabled(!self.read_only && redo),
            "Cut" | "Paste" | "Delete" => ToolState::enabled(!self.read_only),
            "Syntax" | "Font" => ToolState::DISABLED,
            _ => ToolState::enabled(true),
        };
        let height = toolbar::height(ui, TOOLS, ui.available_width(), &state);
        TopBottomPanel::top("text_edit_toolbar")
            .exact_height(height)
            .frame(Frame::none())
            .show_inside(ui, |ui| {
                ui.data_mut(|d| d.insert_temp(Id::new("bc5_flat_toolbar"), true));
                if let Some(ToolEvent::Click(name)) =
                    toolbar::show(ui, icons, TOOLS, true, &state, &mut |_, _, _| {})
                {
                    match name {
                        "SessionHome" => home = true,
                        "Undo" => action = Some(Action::Undo),
                        "Redo" => action = Some(Action::Redo),
                        "Cut" => action = Some(Action::Cut),
                        "Copy" => action = Some(Action::Copy),
                        "Paste" => action = Some(Action::Paste),
                        "Delete" => action = Some(Action::Delete),
                        _ => {}
                    }
                }
                ui.data_mut(|d| d.remove::<bool>(Id::new("bc5_flat_toolbar")));
            });
        // Open the strip in the same frame as the menu/shortcut command.
        if matches!(action, Some(Action::Find(_))) {
            self.run(action.take().unwrap(), ctx, dialogs);
        }
        self.search_panel(ctx, ui, icons, dialogs);
        ui.horizontal(|ui| {
            let response = history_combo::show(
                ui,
                "text_edit_path",
                &mut self.path,
                &[],
                (ui.available_width() - 60.0).max(80.0),
                "Entre o nome do arquivo aqui",
            );
            if response.submitted || response.selected.is_some() {
                if self.confirm_saved(dialogs) {
                    let path = PathBuf::from(&self.path);
                    if let Err(e) = self.load(&path) {
                        dialogs.error(&e);
                    } else {
                        self.reset_state(ctx);
                    }
                }
                self.path = self
                    .document
                    .path
                    .as_ref()
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_default();
            }
            for (icon, enabled, command) in [
                ("imgBrowse", true, Action::Open),
                (
                    "imgSave",
                    !self.read_only && self.document.dirty(),
                    Action::Save(false),
                ),
            ] {
                if let Some(image) = icons.get_icon_with_state(ctx, icon, enabled) {
                    let r = ui.add_enabled(
                        enabled,
                        ImageButton::new((image.id(), Vec2::splat(16.0))).frame(false),
                    );
                    #[cfg(test)]
                    ui.data_mut(|d| {
                        d.insert_temp(Id::new("text_edit_save_enabled").with(icon), enabled)
                    });
                    if r.clicked() {
                        action = Some(command);
                    }
                }
            }
        });
        ui.add_enabled_ui(!self.read_only, |ui| {
            ui.horizontal(|ui| {
                ui.label(self.document.metadata_label().unwrap_or_default());
                ComboBox::from_id_salt("text_edit_encoding")
                    .selected_text(tr(self.document.encoding_label()))
                    .show_ui(ui, |ui| {
                        for encoding in Encoding::ALL {
                            ui.selectable_value(
                                &mut self.document.encoding,
                                encoding,
                                tr(encoding.label()),
                            );
                        }
                    });
                ComboBox::from_id_salt("text_edit_eol")
                    .selected_text(tr(self.document.line_ending.label()))
                    .show_ui(ui, |ui| {
                        for ending in LineEnding::ALL {
                            ui.selectable_value(
                                &mut self.document.line_ending,
                                ending,
                                tr(ending.label()),
                            );
                        }
                    });
            });
        });
        if !super::session_picker::is_open(ctx) {
            for (modifiers, key, command) in [
                (Modifiers::CTRL, Key::S, Action::Save(false)),
                (Modifiers::NONE, Key::F12, Action::Save(true)),
                (Modifiers::CTRL, Key::O, Action::Open),
                (Modifiers::NONE, Key::F5, Action::Reload),
            ] {
                if ctx.input_mut(|i| i.consume_key(modifiers, key)) {
                    action = Some(command);
                }
            }
        }
        if let Some(command) = action {
            self.run(command, ctx, dialogs);
        }
        if let Some(path) = self.take_saved_source() {
            session_menu::request(ctx, session_menu::Request::SourceSaved(path));
        }
        if self.read_only && ctx.memory(|m| m.has_focus(self.editor_id)) {
            // Read-only input must preserve the selection as well as the bytes.
            ctx.input_mut(|input| {
                input.events.retain(|event| {
                    !matches!(
                        event,
                        Event::Text(_) | Event::Paste(_) | Event::Cut | Event::Ime(_)
                    )
                })
            });
        }
        let height = (ui.available_height() - 22.0).max(40.0);
        let wrap = self.options.wrap;
        let mut layouter = |ui: &Ui, text: &str, width: f32| {
            ui.fonts(|f| {
                f.layout(
                    text.to_owned(),
                    FontId::monospace(13.0),
                    Color32::BLACK,
                    if wrap { width } else { f32::INFINITY },
                )
            })
        };
        let mut editor_action = None;
        let scroll = ScrollArea::both()
            .id_salt("single_file_scroll")
            .auto_shrink([false, false])
            .max_height(height)
            .show(ui, |ui| {
                let edit_input = !self.read_only
                    && ctx.memory(|m| m.has_focus(self.editor_id))
                    && ctx.input(|i| {
                        i.events.iter().any(|e| {
                            matches!(
                                e,
                                Event::Text(_)
                                    | Event::Paste(_)
                                    | Event::Cut
                                    | Event::Ime(_)
                                    | Event::Key { pressed: true, .. }
                            )
                        })
                    });
                let before = edit_input.then(|| self.edit_point(ctx));
                let previous_selection =
                    TextEdit::load_state(ctx, self.editor_id).and_then(|s| s.cursor.char_range());
                let preserve_selection = ctx.is_context_menu_open()
                    || self.search.shown && !ctx.memory(|m| m.has_focus(self.editor_id))
                    || ctx.input(|i| {
                        i.pointer.button_down(PointerButton::Secondary)
                            || i.pointer.button_released(PointerButton::Secondary)
                    });
                let mut read_only_text = self.document.text.as_str();
                let editor = if self.read_only {
                    TextEdit::multiline(&mut read_only_text)
                } else {
                    TextEdit::multiline(&mut self.document.text)
                };
                let mut output = editor
                    .id(self.editor_id)
                    .font(FontId::monospace(13.0))
                    .frame(false)
                    .margin(vec2(36.0, 2.0))
                    .desired_width(ui.available_width())
                    .desired_rows(1)
                    .layouter(&mut layouter)
                    .show(ui);
                if preserve_selection {
                    if let Some(selection) = previous_selection {
                        output.state.cursor.set_char_range(Some(selection));
                        output.state.clone().store(ctx, self.editor_id);
                    }
                }
                if output.response.changed() {
                    self.search.last = None;
                    if let Some(before) = before {
                        self.remember_change(before);
                    }
                }
                // Keep only the document's text-change history, rather than
                // accumulating a second history of cursor navigation.
                output.state.clear_undoer();
                output.state.clone().store(ctx, self.editor_id);
                self.draw_bookmarks(ui, &output);
                if self.scroll_to_cursor {
                    if let Some(cursor) = output.state.cursor.char_range() {
                        let rect = output
                            .galley
                            .pos_from_ccursor(cursor.primary)
                            .translate(output.galley_pos.to_vec2());
                        ui.scroll_to_rect(rect.expand(4.0), Some(Align::Center));
                    }
                    self.scroll_to_cursor = false;
                }
                output.response.context_menu(|ui| {
                    for (label, enabled, command) in [
                        (
                            "Desfazer  Ctrl+Z",
                            !self.read_only && self.undo_flags.0,
                            Action::Undo,
                        ),
                        (
                            "Refazer  Ctrl+Y",
                            !self.read_only && self.undo_flags.1,
                            Action::Redo,
                        ),
                        ("Recortar  Ctrl+X", !self.read_only, Action::Cut),
                        ("Copiar  Ctrl+C", true, Action::Copy),
                        ("Colar  Ctrl+V", !self.read_only, Action::Paste),
                        ("Excluir", !self.read_only, Action::Delete),
                        ("Selecionar tudo  Ctrl+A", true, Action::All),
                    ] {
                        let response = chrome::menu_item(ui, icons, label, enabled);
                        #[cfg(test)]
                        ui.data_mut(|d| {
                            d.insert_temp(
                                Id::new("text_edit_context").with(label),
                                (response.rect, enabled),
                            )
                        });
                        if response.clicked() {
                            editor_action = Some(command);
                            ui.close_menu();
                        }
                    }
                });
                ui.painter().rect_filled(
                    Rect::from_min_size(
                        output.response.rect.min,
                        vec2(28.0, output.response.rect.height()),
                    ),
                    0.0,
                    Color32::from_gray(240),
                );
                if self.focus {
                    output.response.request_focus();
                    self.focus = false;
                }
                #[cfg(test)]
                ui.data_mut(|d| {
                    d.insert_temp(Id::new("text_edit_rect"), output.response.rect);
                    d.insert_temp(Id::new("text_edit_rows"), output.galley.rows.len());
                    if let Some(cursor) = output.state.cursor.char_range() {
                        d.insert_temp(
                            Id::new("text_edit_caret_rect"),
                            output
                                .galley
                                .pos_from_ccursor(cursor.primary)
                                .translate(output.galley_pos.to_vec2()),
                        );
                    }
                });
            });
        #[cfg(test)]
        ctx.data_mut(|d| {
            d.insert_temp(Id::new("text_edit_scroll_offset"), scroll.state.offset);
            d.insert_temp(Id::new("text_edit_viewport"), scroll.inner_rect);
        });
        #[cfg(not(test))]
        let _ = scroll;
        if let Some(command) = editor_action {
            self.run(command, ctx, dialogs);
            ctx.request_repaint();
        }
        ui.separator();
        ui.horizontal(|ui| {
            let cursor = TextEdit::load_state(ctx, self.editor_id)
                .and_then(|s| s.cursor.char_range())
                .map_or(0, |c| c.primary.index);
            let byte = self
                .document
                .text
                .char_indices()
                .nth(cursor)
                .map_or(self.document.text.len(), |(i, _)| i);
            let line = self.document.text[..byte]
                .bytes()
                .filter(|b| *b == b'\n')
                .count()
                + 1;
            let start = self.document.text[..byte].rfind('\n').map_or(0, |i| i + 1);
            ui.label(format!(
                "{line}:{}",
                self.document.text[start..byte].chars().count() + 1
            ));
            ui.separator();
            ui.label(if self.read_only {
                tr("Editar desativado")
            } else if self.document.dirty() {
                tr("Modificado")
            } else {
                tr("Inserir")
            });
        });
        home
    }
}

const fn tool(name: &'static str, image: &'static str, caption: &'static str) -> ToolItem {
    ToolItem {
        name,
        image,
        caption,
        short: caption,
        hint: caption,
        kind: ToolKind::Button,
        children: &[],
    }
}
const TOOLS: &[ToolItem] = &[
    tool("SessionHome", "imgSessionHome", "Página Inicial"),
    ToolItem {
        kind: ToolKind::Tree,
        ..tool("SessionsCombo", "imgSessionsCombo", "Sessões")
    },
    ToolItem::SEPARATOR,
    tool("Undo", "imgUndo", "Desfazer"),
    tool("Redo", "imgRedo", "Refazer"),
    ToolItem::SEPARATOR,
    tool("Cut", "imgCut", "Recortar"),
    tool("Copy", "imgCopy", "Copiar"),
    tool("Paste", "imgPaste", "Colar"),
    tool("Delete", "imgDelete", "Excluir"),
    ToolItem::SEPARATOR,
    tool("Syntax", "imgSyntax", "Sintaxe"),
    tool("Font", "imgFont", "Fonte"),
];

#[cfg(test)]
mod tests;
