use crate::binary_diff::BinaryDiff;
use crate::ui::{
    dialogs::{DialogField, DialogHost, SaveChoice},
    icons::IconManager,
    theme::BcTheme,
};
use crate::{diff::FilterMode, document::BinaryDocument};
use egui::*;
use std::{
    ops::Range,
    path::{Path, PathBuf},
};

struct Edit {
    left: bool,
    start: usize,
    before: Vec<u8>,
    after: Vec<u8>,
}
struct RowCache {
    revision: u64,
    filter: FilterMode,
    bytes_per_row: usize,
    rows: std::sync::Arc<[usize]>,
}

/// Use identical geometry for the painted cells and their pointer targets.
struct HexRowLayout {
    rect: Rect,
    pane: f32,
    bytes_per_row: usize,
    char_width: f32,
}
impl HexRowLayout {
    fn pane_left(&self, left: bool) -> f32 {
        self.rect.left() + if left { 0.0 } else { self.pane }
    }
    fn cells(&self, left: bool, column: usize) -> (Rect, Rect) {
        let x = self.pane_left(left);
        (
            Rect::from_min_size(
                pos2(x + 76.0 + column as f32 * 23.0, self.rect.top()),
                vec2(22.0, self.rect.height()),
            ),
            Rect::from_min_size(
                pos2(
                    x + 82.0 + self.bytes_per_row as f32 * 23.0 + column as f32 * self.char_width,
                    self.rect.top(),
                ),
                vec2(self.char_width, self.rect.height()),
            ),
        )
    }
}

pub struct HexCompareScreen {
    pub left_path: String,
    pub right_path: String,
    left: BinaryDocument,
    right: BinaryDocument,
    alignment: BinaryDiff,
    difference_count: usize,
    selected: usize,
    selection_end: usize,
    active_left: bool,
    editing: bool,
    overwrite: bool,
    filter: FilterMode,
    bytes_per_row: usize,
    show_ascii: bool,
    hex_addresses: bool,
    little_endian: bool,
    scroll_to_selected: bool,
    error: Option<String>,
    undo: Vec<Edit>,
    redo: Vec<Edit>,
    find_bytes: Vec<u8>,
    paste_requested: bool,
    revision: u64,
    row_cache: Option<RowCache>,
    last_difference: Option<usize>,
    drag_anchor: Option<(bool, usize)>,
    #[cfg(test)]
    cells: Vec<(bool, usize, Rect, Option<Rect>)>,
    #[cfg(test)]
    viewport: Option<(Rect, Id)>,
}
fn parse_hex(value: &str) -> Result<Vec<u8>, String> {
    let value: String = value.chars().filter(|c| !c.is_whitespace()).collect();
    if !value.is_ascii() || !value.len().is_multiple_of(2) {
        return Err("Informe pares de dígitos hexadecimais, como 41 FF 00.".into());
    }
    value
        .as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|part| {
            u8::from_str_radix(std::str::from_utf8(part).unwrap(), 16)
                .map_err(|_| "Dígito hexadecimal inválido.".into())
        })
        .collect()
}
fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(" ")
}

impl HexCompareScreen {
    pub fn session_options(&self) -> serde_json::Value {
        serde_json::json!({"filter":format!("{:?}",self.filter),"editing":self.editing,"overwrite":self.overwrite,"ascii":self.show_ascii,"hex_addresses":self.hex_addresses,"little_endian":self.little_endian,"bytes_per_row":self.bytes_per_row})
    }
    pub fn apply_session_options(&mut self, value: &serde_json::Value) {
        for (key, target) in [
            ("editing", &mut self.editing),
            ("overwrite", &mut self.overwrite),
            ("ascii", &mut self.show_ascii),
            ("hex_addresses", &mut self.hex_addresses),
            ("little_endian", &mut self.little_endian),
        ] {
            if let Some(value) = value[key].as_bool() {
                *target = value;
            }
        }
        if let Some(rows) = value["bytes_per_row"].as_u64() {
            if [8, 16, 32].contains(&rows) {
                self.bytes_per_row = rows as usize;
            }
        }
        self.filter = match value["filter"].as_str() {
            Some("Differences") => FilterMode::Differences,
            Some("Same") => FilterMode::Same,
            _ => FilterMode::All,
        };
    }
    pub fn new() -> Self {
        Self {
            left_path: String::new(),
            right_path: String::new(),
            left: Default::default(),
            right: Default::default(),
            alignment: BinaryDiff::default(),
            difference_count: 0,
            selected: 0,
            selection_end: 1,
            active_left: true,
            editing: false,
            overwrite: true,
            filter: FilterMode::All,
            bytes_per_row: 16,
            show_ascii: true,
            hex_addresses: true,
            little_endian: true,
            scroll_to_selected: false,
            error: None,
            undo: Vec::new(),
            redo: Vec::new(),
            find_bytes: Vec::new(),
            paste_requested: false,
            revision: 0,
            row_cache: None,
            last_difference: None,
            drag_anchor: None,
            #[cfg(test)]
            cells: Vec::new(),
            #[cfg(test)]
            viewport: None,
        }
    }
    fn doc(&self, left: bool) -> &BinaryDocument {
        if left {
            &self.left
        } else {
            &self.right
        }
    }
    fn doc_mut(&mut self, left: bool) -> &mut BinaryDocument {
        if left {
            &mut self.left
        } else {
            &mut self.right
        }
    }
    pub fn dirty(&self) -> bool {
        self.left.dirty() || self.right.dirty()
    }
    pub fn load_error(&self) -> Option<&str> {
        self.error.as_deref()
    }
    pub fn load_files(&mut self, left: &Path, right: &Path) {
        match (BinaryDocument::load(left), BinaryDocument::load(right)) {
            (Ok(a), Ok(b)) => {
                self.left = a;
                self.right = b;
                self.left_path = left.display().to_string();
                self.right_path = right.display().to_string();
                self.undo.clear();
                self.redo.clear();
                self.recompare();
                self.error = None;
            }
            (Err(error), _) | (_, Err(error)) => self.error = Some(error),
        }
    }
    fn recompare(&mut self) {
        self.revision = self.revision.wrapping_add(1);
        self.alignment = BinaryDiff::compare(&self.left.bytes, &self.right.bytes);
        self.last_difference = self.alignment.last_difference;
        self.difference_count = self.alignment.difference_count();
        self.selected = self.selected.min(self.alignment.len);
        self.selection_end = self
            .selection_end
            .min(self.alignment.len)
            .max(self.selected);
    }
    fn row_map(&mut self, count: usize) -> Option<std::sync::Arc<[usize]>> {
        if self.filter == FilterMode::All {
            return None;
        }
        if let Some(cache) = &self.row_cache {
            if cache.revision == self.revision
                && cache.filter == self.filter
                && cache.bytes_per_row == self.bytes_per_row
            {
                return Some(cache.rows.clone());
            }
        }
        let rows: std::sync::Arc<[usize]> = (0..count)
            .filter(|&row| {
                let range = row * self.bytes_per_row
                    ..((row + 1) * self.bytes_per_row).min(self.alignment.len);
                range.into_iter().any(|offset| {
                    !self
                        .alignment
                        .changed(offset, &self.left.bytes, &self.right.bytes)
                        == (self.filter == FilterMode::Same)
                })
            })
            .collect::<Vec<_>>()
            .into();
        self.row_cache = Some(RowCache {
            revision: self.revision,
            filter: self.filter,
            bytes_per_row: self.bytes_per_row,
            rows: rows.clone(),
        });
        Some(rows)
    }
    fn select(&mut self, index: usize, end: usize) {
        self.selected = index;
        self.selection_end = end;
        self.scroll_to_selected = true;
    }
    fn selected_range(&self, left: bool) -> Range<usize> {
        self.alignment
            .range(left, self.selected..self.selection_end)
    }
    fn select_physical(&mut self, left: bool, range: Range<usize>) {
        self.select(
            self.alignment.index(left, range.start),
            self.alignment.index(left, range.end),
        );
    }
    fn next_diff(&mut self, forward: bool) {
        let count = self.alignment.len;
        let next = if forward {
            (self.selected.saturating_add(1)..count).find(|&i| {
                self.alignment
                    .changed(i, &self.left.bytes, &self.right.bytes)
            })
        } else {
            (0..self.selected).rev().find(|&i| {
                self.alignment
                    .changed(i, &self.left.bytes, &self.right.bytes)
            })
        };
        if let Some(i) = next {
            self.select(i, i + 1);
        }
    }
    fn change(&mut self, left: bool, range: Range<usize>, bytes: Vec<u8>) {
        let before = self.doc(left).bytes[range.clone()].to_vec();
        if before == bytes {
            return;
        }
        self.undo.push(Edit {
            left,
            start: range.start,
            before,
            after: bytes.clone(),
        });
        self.redo.clear();
        while self.undo.len() > 1
            && self
                .undo
                .iter()
                .map(|e| e.before.len() + e.after.len())
                .sum::<usize>()
                > 64 * 1024 * 1024
        {
            self.undo.remove(0);
        }
        self.doc_mut(left)
            .bytes
            .splice(range.clone(), bytes.iter().copied());
        self.recompare();
        self.select_physical(left, range.start..range.start + bytes.len());
    }
    fn undo(&mut self, redo: bool) {
        let edit = if redo {
            self.redo.pop()
        } else {
            self.undo.pop()
        };
        if let Some(edit) = edit {
            let (old, new) = if redo {
                (&edit.before, &edit.after)
            } else {
                (&edit.after, &edit.before)
            };
            self.doc_mut(edit.left)
                .bytes
                .splice(edit.start..edit.start + old.len(), new.iter().copied());
            self.active_left = edit.left;
            self.recompare();
            self.select_physical(edit.left, edit.start..edit.start + new.len());
            if redo {
                self.undo.push(edit);
            } else {
                self.redo.push(edit);
            }
        }
    }
    fn copy_side(&mut self, to_right: bool) {
        let source_left = to_right;
        let range = self.selected_range(source_left);
        let bytes = self.doc(source_left).bytes[range].to_vec();
        let target = self.selected_range(!source_left);
        self.change(!source_left, target, bytes);
    }
    fn save_side(&mut self, left: bool, save_as: bool, dialogs: &mut dyn DialogHost) -> bool {
        let path = if save_as || self.doc(left).path.is_none() {
            dialogs.file(true, left, self.doc(left).path.as_deref())
        } else {
            self.doc(left).path.clone()
        };
        if let Some(path) = path {
            match self.doc_mut(left).save(&path) {
                Ok(()) => {
                    if left {
                        self.left_path = path.display().to_string();
                    } else {
                        self.right_path = path.display().to_string();
                    }
                    self.error = None;
                    return true;
                }
                Err(error) => {
                    self.error = Some(error.clone());
                    dialogs.error(&error);
                }
            }
        }
        false
    }
    fn confirm_side(&mut self, left: bool, dialogs: &mut dyn DialogHost) -> bool {
        if !self.doc(left).dirty() {
            return true;
        }
        match dialogs.confirm_save(left, self.doc(left).path.as_deref()) {
            SaveChoice::Save => self.save_side(left, false, dialogs),
            SaveChoice::Discard => true,
            SaveChoice::Cancel => false,
        }
    }
    pub fn confirm_saved(&mut self, dialogs: &mut dyn DialogHost) -> bool {
        self.confirm_side(true, dialogs) && self.confirm_side(false, dialogs)
    }
    fn open_side(&mut self, left: bool, path: PathBuf, dialogs: &mut dyn DialogHost) {
        let next = match BinaryDocument::load(&path) {
            Ok(next) => next,
            Err(error) => {
                dialogs.error(&error);
                return;
            }
        };
        if self.confirm_side(left, dialogs) {
            *self.doc_mut(left) = next;
            if left {
                self.left_path = path.display().to_string();
            } else {
                self.right_path = path.display().to_string();
            }
            self.undo.clear();
            self.redo.clear();
            self.recompare();
        }
    }
    fn reload(&mut self, dialogs: &mut dyn DialogHost) {
        if !self.confirm_saved(dialogs) {
            return;
        }
        for left in [true, false] {
            if let Some(path) = self.doc(left).path.clone() {
                match BinaryDocument::load(&path) {
                    Ok(next) => *self.doc_mut(left) = next,
                    Err(error) => {
                        dialogs.error(&error);
                        return;
                    }
                }
            }
        }
        self.undo.clear();
        self.redo.clear();
        self.recompare();
    }
    fn swap(&mut self) {
        std::mem::swap(&mut self.left, &mut self.right);
        std::mem::swap(&mut self.left_path, &mut self.right_path);
        self.undo.clear();
        self.redo.clear();
        self.active_left = !self.active_left;
        self.recompare();
    }
    fn edit(&mut self, dialogs: &mut dyn DialogHost) {
        if !self.editing {
            return;
        }
        let range = self.selected_range(self.active_left);
        let initial = hex(&self.doc(self.active_left).bytes[range.clone()]);
        if let Some(value) = dialogs.input("Editar bytes", "Bytes hexadecimais:", &initial) {
            match parse_hex(&value) {
                Ok(bytes) => self.change(self.active_left, range, bytes),
                Err(error) => dialogs.error(&error),
            }
        }
    }
    fn go_to(&mut self, dialogs: &mut dyn DialogHost) {
        if let Some(value) = dialogs.input(
            "Ir para",
            "Posição decimal ou 0x...:",
            &format!("0x{:X}", self.selected_range(self.active_left).start),
        ) {
            let value = value.trim();
            let number = if let Some(value) = value
                .strip_prefix("0x")
                .or_else(|| value.strip_prefix("0X"))
            {
                usize::from_str_radix(value, 16)
            } else {
                value.parse()
            };
            match number {
                Ok(index) if index <= self.doc(self.active_left).bytes.len() => {
                    self.select_physical(self.active_left, index..index.saturating_add(1))
                }
                _ => dialogs.error("Posição fora do arquivo ou inválida."),
            }
        }
    }
    fn find(&mut self, dialogs: &mut dyn DialogHost, ask: bool, forward: bool) {
        if ask {
            let mut fields = [
                DialogField::Text("Localizar:".into(), hex(&self.find_bytes)),
                DialogField::Choice(
                    "Formato:".into(),
                    0,
                    vec!["Hexadecimal".into(), "Texto UTF-8".into()],
                ),
            ];
            if !dialogs.fields("Localizar bytes", &mut fields) {
                return;
            }
            if let (DialogField::Text(_, value), DialogField::Choice(_, format, _)) =
                (&fields[0], &fields[1])
            {
                self.find_bytes = if *format == 0 {
                    match parse_hex(value) {
                        Ok(bytes) => bytes,
                        Err(error) => {
                            dialogs.error(&error);
                            return;
                        }
                    }
                } else {
                    value.as_bytes().to_vec()
                };
            }
        }
        if self.find_bytes.is_empty() {
            return;
        }
        let matches: Vec<_> = self
            .doc(self.active_left)
            .bytes
            .windows(self.find_bytes.len())
            .enumerate()
            .filter(|(_, bytes)| *bytes == self.find_bytes)
            .map(|(i, _)| i)
            .collect();
        let next = if forward {
            matches
                .iter()
                .find(|&&i| i > self.selected_range(self.active_left).start)
                .or(matches.first())
        } else {
            matches
                .iter()
                .rev()
                .find(|&&i| i < self.selected_range(self.active_left).start)
                .or(matches.last())
        };
        if let Some(&index) = next {
            self.select_physical(self.active_left, index..index + self.find_bytes.len());
        } else {
            dialogs.error("Bytes não encontrados.");
        }
    }
    fn options(&mut self, dialogs: &mut dyn DialogHost) {
        let rows = [8, 16, 32];
        let mut fields = [
            DialogField::Check("Permitir edição de bytes".into(), self.editing),
            DialogField::Check("Mostrar caracteres ASCII".into(), self.show_ascii),
            DialogField::Check("Endereços em hexadecimal".into(), self.hex_addresses),
            DialogField::Check("Valores Little Endian".into(), self.little_endian),
            DialogField::Choice(
                "Bytes por linha:".into(),
                rows.iter().position(|&n| n == self.bytes_per_row).unwrap(),
                vec!["8".into(), "16".into(), "32".into()],
            ),
            crate::i18n::language_field(),
        ];
        if dialogs.fields("Layout hexadecimal", &mut fields) {
            if let DialogField::Check(_, v) = fields[0] {
                self.editing = v;
            }
            if let DialogField::Check(_, v) = fields[1] {
                self.show_ascii = v;
            }
            if let DialogField::Check(_, v) = fields[2] {
                self.hex_addresses = v;
            }
            if let DialogField::Check(_, v) = fields[3] {
                self.little_endian = v;
            }
            if let DialogField::Choice(_, v, _) = fields[4] {
                self.bytes_per_row = rows[v];
            }
            crate::i18n::apply_language_field(fields.last().unwrap(), dialogs);
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
        if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::N)) {
            self.next_diff(true);
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::P)) {
            self.next_diff(false);
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::G)) {
            self.go_to(dialogs);
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::F)) {
            self.find(dialogs, true, true);
        }
        let previous = ctx.input_mut(|i| i.consume_key(Modifiers::SHIFT, Key::F3));
        if previous || ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::F3)) {
            self.find(dialogs, false, !previous);
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::F5)) {
            self.recompare();
        } else if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::F5)) {
            self.reload(dialogs);
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::F2)) {
            self.editing = !self.editing;
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Insert)) {
            self.overwrite = !self.overwrite;
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::S))
            && self.doc(self.active_left).dirty()
        {
            self.save_side(self.active_left, false, dialogs);
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::F12)) {
            self.save_side(self.active_left, true, dialogs);
        }
        let bytes_focused = self.paste_requested || ctx.memory(|memory| memory.focused().is_none());
        if bytes_focused {
            if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::A)) {
                self.select(0, self.doc(self.active_left).bytes.len());
            }
            if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::R)) {
                self.copy_side(true);
            }
            if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::L)) {
                self.copy_side(false);
            }
            if ctx.input_mut(|i| {
                i.consume_key(Modifiers::CTRL | Modifiers::SHIFT, Key::Z)
                    || i.consume_key(Modifiers::CTRL, Key::Y)
            }) {
                self.undo(true);
            } else if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::Z)) {
                self.undo(false);
            }
            let events = ctx.input_mut(|i| {
                let mut owned = Vec::new();
                i.events.retain(|event| {
                    if matches!(event, Event::Copy | Event::Cut | Event::Paste(_)) {
                        owned.push(event.clone());
                        false
                    } else {
                        true
                    }
                });
                owned
            });
            for event in events {
                match event {
                    Event::Copy => ctx
                        .copy_text(hex(&self.doc(self.active_left).bytes
                            [self.selected_range(self.active_left)])),
                    Event::Cut if self.editing => {
                        let range = self.selected_range(self.active_left);
                        ctx.copy_text(hex(&self.doc(self.active_left).bytes[range.clone()]));
                        self.change(self.active_left, range, Vec::new());
                    }
                    Event::Paste(value) if self.editing => {
                        self.paste_requested = false;
                        match parse_hex(&value) {
                            Ok(bytes) => {
                                self.paste_requested = false;
                                let mut range = self.selected_range(self.active_left);
                                if self.overwrite {
                                    range.end = (range.start + bytes.len())
                                        .min(self.doc(self.active_left).bytes.len());
                                }
                                self.change(self.active_left, range, bytes);
                            }
                            Err(error) => dialogs.error(&error),
                        }
                    }
                    _ => {}
                }
            }
            if self.editing && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Delete)) {
                self.change(
                    self.active_left,
                    self.selected_range(self.active_left),
                    Vec::new(),
                );
            }
        }
        let mut open = false;
        TopBottomPanel::top("hex_menus")
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
                            self.left.path.is_some() && self.right.path.is_some(),
                        );
                        super::session_menu::close(ui, icons);
                        ui.separator();

                        if super::chrome::menu_item(ui, icons, "Página Inicial", true).clicked() {
                            home = true;
                            ui.close_menu();
                        }
                        if super::chrome::menu_item(ui, icons, "Inverter lados", true).clicked() {
                            self.swap();
                            ui.close_menu();
                        }

                        super::session_menu::suffix(
                            ui,
                            icons,
                            self.left.path.is_some() && self.right.path.is_some(),
                        );
                    });
                    super::chrome::menu_button(ui, "Arquivo (F)", |ui| {
                        if super::chrome::menu_item(ui, icons, "Abrir arquivo...  Ctrl+O", true)
                            .clicked()
                        {
                            open = true;
                            ui.close_menu();
                        }
                        if super::chrome::menu_item(
                            ui,
                            icons,
                            "Salvar arquivo  Ctrl+S",
                            self.doc(self.active_left).dirty(),
                        )
                        .clicked()
                        {
                            self.save_side(self.active_left, false, dialogs);
                            ui.close_menu();
                        }
                        if super::chrome::menu_item(ui, icons, "Salvar como...  F12", true)
                            .clicked()
                        {
                            self.save_side(self.active_left, true, dialogs);
                            ui.close_menu();
                        }
                        if super::chrome::menu_item(ui, icons, "Recarregar  F5", true).clicked() {
                            self.reload(dialogs);
                            ui.close_menu();
                        }
                        if super::chrome::menu_item(ui, icons, "Recomparar  Ctrl+F5", true)
                            .clicked()
                        {
                            self.recompare();
                            ui.close_menu();
                        }
                    });
                    super::chrome::menu_button(ui, "Editar", |ui| {
                        if super::chrome::menu_item(
                            ui,
                            icons,
                            "Desfazer  Ctrl+Z",
                            !self.undo.is_empty(),
                        )
                        .clicked()
                        {
                            self.undo(false);
                            ui.close_menu();
                        }
                        if super::chrome::menu_item(
                            ui,
                            icons,
                            "Refazer  Ctrl+Y",
                            !self.redo.is_empty(),
                        )
                        .clicked()
                        {
                            self.undo(true);
                            ui.close_menu();
                        }
                        if super::chrome::menu_item(ui, icons, "Copiar bytes  Ctrl+C", true)
                            .clicked()
                        {
                            ctx.copy_text(hex(&self.doc(self.active_left).bytes
                                [self.selected_range(self.active_left)]));
                            ui.close_menu();
                        }
                        if super::chrome::menu_item(ui, icons, "Colar bytes  Ctrl+V", self.editing)
                            .clicked()
                        {
                            ctx.memory_mut(|m| {
                                if let Some(id) = m.focused() {
                                    m.surrender_focus(id);
                                }
                            });
                            ctx.send_viewport_cmd(ViewportCommand::RequestPaste);
                            self.paste_requested = true;
                            ui.close_menu();
                        }
                        if super::chrome::menu_item(ui, icons, "Selecionar tudo  Ctrl+A", true)
                            .clicked()
                        {
                            self.select(0, self.doc(self.active_left).bytes.len());
                            ui.close_menu();
                        }
                        for (label, right) in [
                            ("Copiar para a Direita  Ctrl+R", true),
                            ("Copiar para a Esquerda  Ctrl+L", false),
                        ] {
                            if super::chrome::menu_item(ui, icons, label, true).clicked() {
                                self.copy_side(right);
                                ui.close_menu();
                            }
                        }
                        if super::chrome::menu_item(ui, icons, "Editar bytes...", self.editing)
                            .clicked()
                        {
                            self.edit(dialogs);
                            ui.close_menu();
                        }
                        if super::chrome::menu_item(
                            ui,
                            icons,
                            "Excluir bytes  Delete",
                            self.editing,
                        )
                        .clicked()
                        {
                            self.change(
                                self.active_left,
                                self.selected_range(self.active_left),
                                Vec::new(),
                            );
                            ui.close_menu();
                        }
                    });
                    super::chrome::menu_button(ui, "Pesquisar", |ui| {
                        if super::chrome::menu_item(ui, icons, "Localizar...  Ctrl+F", true)
                            .clicked()
                        {
                            self.find(dialogs, true, true);
                            ui.close_menu();
                        }
                        for (label, forward) in [
                            ("Localizar próximo  F3", true),
                            ("Localizar anterior  Shift+F3", false),
                        ] {
                            if super::chrome::menu_item(ui, icons, label, true).clicked() {
                                self.find(dialogs, false, forward);
                                ui.close_menu();
                            }
                        }
                        for (label, forward) in [
                            ("Próxima diferença  Ctrl+N", true),
                            ("Diferença anterior  Ctrl+P", false),
                        ] {
                            if super::chrome::menu_item(ui, icons, label, true).clicked() {
                                self.next_diff(forward);
                                ui.close_menu();
                            }
                        }
                        if super::chrome::menu_item(ui, icons, "Ir para...  Ctrl+G", true).clicked()
                        {
                            self.go_to(dialogs);
                            ui.close_menu();
                        }
                    });
                    super::chrome::menu_button(ui, "Visualizar", |ui| {
                        ui.radio_value(&mut self.filter, FilterMode::All, crate::i18n::tr("Todos"));
                        ui.radio_value(
                            &mut self.filter,
                            FilterMode::Differences,
                            crate::i18n::tr("Diferenças"),
                        );
                        ui.radio_value(
                            &mut self.filter,
                            FilterMode::Same,
                            crate::i18n::tr("Iguais"),
                        );
                        ui.checkbox(&mut self.editing, crate::i18n::tr("Editar bytes"));
                        ui.checkbox(&mut self.show_ascii, crate::i18n::tr("Caracteres ASCII"));
                        ui.checkbox(
                            &mut self.hex_addresses,
                            crate::i18n::tr("Endereços hexadecimais"),
                        );
                        ui.checkbox(&mut self.little_endian, crate::i18n::tr("Little Endian"));
                        ui.checkbox(&mut self.overwrite, crate::i18n::tr("Sobrescrever"));
                        if super::chrome::menu_item(ui, icons, "Layout...", true).clicked() {
                            self.options(dialogs);
                            ui.close_menu();
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
        if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::O)) {
            open = true;
        }
        if open {
            if let Some(path) = dialogs.file(
                false,
                self.active_left,
                self.doc(self.active_left).path.as_deref(),
            ) {
                self.open_side(self.active_left, path, dialogs);
            }
        }
        use super::toolbar::{ToolEvent, ToolState};
        let can_copy = self.doc(self.active_left).bytes[self.selected_range(self.active_left)]
            != self.doc(!self.active_left).bytes[self.selected_range(!self.active_left)];
        let state = |name: &str| match name {
            "FavCompareAll" => ToolState::checked(self.filter == FilterMode::All),
            "FavDiff" => ToolState::checked(self.filter == FilterMode::Differences),
            "FavSame" => ToolState::checked(self.filter == FilterMode::Same),
            "CopySide" => ToolState::enabled(can_copy),
            "DiffItemNext" => {
                ToolState::enabled(self.last_difference.is_some_and(|i| i > self.selected))
            }
            "DiffItemPrev" => ToolState::enabled(
                self.alignment
                    .first_difference
                    .is_some_and(|i| i < self.selected),
            ),
            "SessionHome" | "Swap" | "Reload" => ToolState::enabled(true),
            "SessionsCombo" => ToolState::enabled(true),
            // These native actions do not yet have their native implementation.
            // Keep them visible and disabled instead of routing to another action.
            _ => ToolState::DISABLED,
        };
        let items = super::toolbar_spec::HEX_COMPARE;
        let height = super::toolbar::height(ui, items, ui.available_width(), &state);
        let mut event = None;
        TopBottomPanel::top("hex_toolbar")
            .exact_height(height)
            .show_separator_line(false)
            .frame(Frame::none())
            .show_inside(ui, |ui| {
                event = super::toolbar::show(
                    ui,
                    icons,
                    items,
                    self.active_left,
                    &state,
                    &mut |_, _, _| {},
                );
            });
        match event {
            Some(ToolEvent::Click("SessionHome")) => home = true,
            Some(ToolEvent::Click("FavCompareAll")) => self.filter = FilterMode::All,
            Some(ToolEvent::Click("FavDiff")) => self.filter = FilterMode::Differences,
            Some(ToolEvent::Click("FavSame")) => self.filter = FilterMode::Same,
            Some(ToolEvent::Click("CopySide")) => self.copy_side(self.active_left),
            Some(ToolEvent::Click("DiffItemNext")) => self.next_diff(true),
            Some(ToolEvent::Click("DiffItemPrev")) => self.next_diff(false),
            Some(ToolEvent::Click("Swap")) => self.swap(),
            Some(ToolEvent::Click("Reload")) => self.reload(dialogs),
            _ => {}
        }
        TopBottomPanel::top("hex_paths").show_inside(ui, |ui| {
            ui.columns(2, |columns| {
                for (side, column) in columns.iter_mut().enumerate() {
                    let left = side == 0;
                    let mut typed = None;
                    let mut browse = false;
                    let mut save = false;
                    column.horizontal(|ui| {
                        let path = if left {
                            &mut self.left_path
                        } else {
                            &mut self.right_path
                        };
                        let response = ui.add(
                            TextEdit::singleline(path)
                                .desired_width((ui.available_width() - 65.0).max(30.0))
                                .hint_text(crate::i18n::tr("Entre o nome do arquivo aqui")),
                        );
                        if response.has_focus() {
                            self.active_left = left;
                        }
                        if response.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                            typed = Some(PathBuf::from(path.clone()));
                        }
                        if let Some(icon) = icons.get_icon(ctx, "imgBrowse") {
                            browse = ui
                                .add(ImageButton::new((icon.id(), vec2(16.0, 16.0))).frame(false))
                                .clicked();
                        }
                        let dirty = self.doc(left).dirty();
                        if let Some(icon) = icons.get_icon_with_state(ctx, "imgSave", dirty) {
                            save = ui
                                .add_enabled(
                                    dirty,
                                    ImageButton::new((icon.id(), vec2(16.0, 16.0))).frame(false),
                                )
                                .clicked();
                        }
                    });
                    if browse {
                        typed = dialogs.file(false, left, self.doc(left).path.as_deref());
                    }
                    if let Some(path) = typed {
                        self.open_side(left, path, dialogs);
                    }
                    if save {
                        self.save_side(left, false, dialogs);
                    }
                    column.label(crate::i18n::tr(
                        &(format!("{} bytes", self.doc(left).bytes.len())),
                    ));
                }
            });
            if let Some(error) = &self.error {
                ui.colored_label(Color32::DARK_RED, crate::i18n::tr(error));
            }
        });
        TopBottomPanel::bottom("hex_status")
            .exact_height(24.0)
            .show_inside(ui, |ui| {
                let bytes = &self.doc(self.active_left).bytes;
                let selected_offset = self.selected_range(self.active_left).start;
                let mut value = [0; 8];
                let end = (selected_offset + 8).min(bytes.len());
                if selected_offset < bytes.len() {
                    let source = &bytes[selected_offset..end];
                    if self.little_endian {
                        value[..source.len()].copy_from_slice(source);
                    } else {
                        value[8 - source.len()..].copy_from_slice(source);
                    }
                }
                let value = if self.little_endian {
                    u64::from_le_bytes(value)
                } else {
                    u64::from_be_bytes(value)
                };
                let status = crate::i18n::tr(
                    &(format!(
                        "{} / {} bytes · {} diferentes · Posição 0x{:X} · {} · Valor: {value}",
                        self.left.bytes.len(),
                        self.right.bytes.len(),
                        self.difference_count,
                        selected_offset,
                        crate::i18n::tr(if self.editing {
                            "Editar"
                        } else {
                            "Somente leitura"
                        })
                    )),
                );
                if self.alignment.limited {
                    ui.colored_label(
                        Color32::DARK_RED,
                        format!(
                            "{} · {status}",
                            crate::i18n::tr("Alinhamento limitado pelo tempo")
                        ),
                    );
                } else {
                    ui.label(status);
                }
            });
        let mut edit = false;
        #[cfg(test)]
        self.cells.clear();
        CentralPanel::default()
            .frame(Frame::none().fill(Color32::WHITE))
            .show_inside(ui, |ui| {
                let all_rows = self.alignment.len.div_ceil(self.bytes_per_row);
                let rows = self.row_map(all_rows);
                ui.spacing_mut().item_spacing.y = 0.0;
                let scroll = ScrollArea::both()
                    .auto_shrink([false; 2])
                    .animated(false)
                    .drag_to_scroll(false)
                    .id_salt("hex_rows");
                let reveal = if self.scroll_to_selected {
                    let index = rows
                        .as_ref()
                        .map(|rows| {
                            rows.iter()
                                .position(|&row| row == self.selected / self.bytes_per_row)
                        })
                        .unwrap_or(Some(self.selected / self.bytes_per_row));
                    self.scroll_to_selected = false;
                    index
                } else {
                    None
                };
                let (pointer, pressed, down, shift) = ui.input(|input| {
                    (
                        input.pointer.interact_pos(),
                        input.pointer.primary_pressed(),
                        input.pointer.primary_down(),
                        input.modifiers.shift,
                    )
                });
                let output = scroll.show_rows(
                    ui,
                    20.0,
                    rows.as_ref().map_or(all_rows, |rows| rows.len()),
                    |ui, range| {
                        if let Some(index) = reveal {
                            // Reveal only as far as necessary. Forcing the
                            // selected row to the top made every click jump.
                            let top =
                                ui.cursor().min.y - range.start as f32 * 20.0 + index as f32 * 20.0;
                            let target = Rect::from_min_size(
                                pos2(ui.clip_rect().center().x, top),
                                vec2(1.0, 20.0),
                            );
                            ui.scroll_to_rect(target, None);
                        }
                        let char_width =
                            ui.fonts(|fonts| fonts.glyph_width(&FontId::monospace(12.0), 'M'));
                        let pane = (76.0
                            + self.bytes_per_row as f32 * 23.0
                            + if self.show_ascii {
                                self.bytes_per_row as f32 * char_width + 20.0
                            } else {
                                0.0
                            })
                        .max(ui.available_width() / 2.0);
                        for visible in range {
                            let row = rows.as_ref().map_or(visible, |rows| rows[visible]);
                            let (rect, response) = ui.allocate_exact_size(
                                vec2(pane * 2.0, 20.0),
                                Sense::click_and_drag(),
                            );
                            let layout = HexRowLayout {
                                rect,
                                pane,
                                bytes_per_row: self.bytes_per_row,
                                char_width,
                            };
                            // Hexadecimal and character cells have different
                            // advances. Both hit testing and painting use these
                            // same cell rectangles; addresses, padding and EOF
                            // are not byte cells.
                            let hit = pointer
                                .filter(|&pos| rect.intersect(ui.clip_rect()).contains(pos))
                                .and_then(|pos| {
                                    for left in [true, false] {
                                        for column in 0..self.bytes_per_row {
                                            let offset = row * self.bytes_per_row + column;
                                            if self.alignment.offset(left, offset).is_none() {
                                                continue;
                                            }
                                            let (hex, ascii) = layout.cells(left, column);
                                            if hex.contains(pos)
                                                || self.show_ascii && ascii.contains(pos)
                                            {
                                                return Some((left, offset));
                                            }
                                        }
                                    }
                                    None
                                });
                            if hit.is_some() {
                                response.clone().on_hover_cursor(CursorIcon::Text);
                            }
                            if let Some((left, offset)) = hit {
                                if pressed && response.contains_pointer() {
                                    let anchor = if shift && self.active_left == left {
                                        self.selected
                                    } else {
                                        offset
                                    };
                                    self.drag_anchor = Some((left, anchor));
                                    self.active_left = left;
                                    ctx.memory_mut(|memory| {
                                        if let Some(id) = memory.focused() {
                                            memory.surrender_focus(id);
                                        }
                                    });
                                }
                                if down {
                                    if let Some((side, anchor)) =
                                        self.drag_anchor.filter(|(side, _)| *side == left)
                                    {
                                        self.active_left = side;
                                        self.selected = anchor.min(offset);
                                        self.selection_end = anchor.max(offset) + 1;
                                        self.scroll_to_selected = false;
                                    }
                                }
                                edit |= response.double_clicked() && self.editing;
                            }
                            for side in [0, 1] {
                                let bytes = if side == 0 {
                                    &self.left.bytes
                                } else {
                                    &self.right.bytes
                                };
                                let x = layout.pane_left(side == 0);
                                let painter =
                                    ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
                                let address = self.alignment.range(
                                    side == 0,
                                    row * self.bytes_per_row
                                        ..((row + 1) * self.bytes_per_row).min(self.alignment.len),
                                );
                                painter.text(
                                    pos2(x + 4.0, rect.center().y),
                                    Align2::LEFT_CENTER,
                                    if address.is_empty() {
                                        String::new()
                                    } else if self.hex_addresses {
                                        format!("{:08X}", address.start)
                                    } else {
                                        format!("{:08}", address.start)
                                    },
                                    FontId::monospace(12.0),
                                    BcTheme::GUTTER_TEXT,
                                );
                                for column in 0..self.bytes_per_row {
                                    let offset = row * self.bytes_per_row + column;
                                    let (hex, ascii) = layout.cells(side == 0, column);
                                    let position = hex.min;
                                    let changed = self.alignment.changed(
                                        offset,
                                        &self.left.bytes,
                                        &self.right.bytes,
                                    );
                                    if changed {
                                        painter.rect_filled(hex, 0.0, BcTheme::DIFF_BG);
                                    }
                                    if offset >= self.selected
                                        && offset < self.selection_end
                                        && self.active_left == (side == 0)
                                    {
                                        painter.rect_stroke(
                                            hex,
                                            0.0,
                                            Stroke::new(1.0_f32, Color32::from_rgb(70, 145, 215)),
                                        );
                                    }
                                    if let Some(byte) = self
                                        .alignment
                                        .offset(side == 0, offset)
                                        .and_then(|physical| bytes.get(physical))
                                    {
                                        #[cfg(test)]
                                        self.cells.push((
                                            side == 0,
                                            offset,
                                            hex,
                                            self.show_ascii.then_some(ascii),
                                        ));
                                        painter.text(
                                            position + vec2(0.0, 10.0),
                                            Align2::LEFT_CENTER,
                                            format!("{byte:02X}"),
                                            FontId::monospace(12.0),
                                            if changed {
                                                BcTheme::DIFF_TEXT
                                            } else {
                                                BcTheme::TEXT_NORMAL
                                            },
                                        );
                                        if self.show_ascii {
                                            if changed {
                                                painter.rect_filled(ascii, 0.0, BcTheme::DIFF_BG);
                                            }
                                            if offset >= self.selected
                                                && offset < self.selection_end
                                                && self.active_left == (side == 0)
                                            {
                                                painter.rect_filled(
                                                    ascii,
                                                    0.0,
                                                    Color32::from_rgb(204, 232, 255),
                                                );
                                            }
                                            painter.text(
                                                ascii.left_center(),
                                                Align2::LEFT_CENTER,
                                                if byte.is_ascii_graphic() || *byte == b' ' {
                                                    *byte as char
                                                } else {
                                                    '.'
                                                },
                                                FontId::monospace(12.0),
                                                if changed {
                                                    BcTheme::DIFF_TEXT
                                                } else {
                                                    BcTheme::TEXT_NORMAL
                                                },
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    },
                );
                #[cfg(test)]
                {
                    self.viewport = Some((output.inner_rect, output.id));
                }
                #[cfg(not(test))]
                let _ = output;
                if !down {
                    self.drag_anchor = None;
                }
            });
        if edit {
            self.edit(dialogs);
        }
        home
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pointer_frame(
        ctx: &Context,
        screen: &mut HexCompareScreen,
        icons: &mut IconManager,
        time: &mut f64,
        events: Vec<Event>,
    ) -> FullOutput {
        *time += if events
            .iter()
            .any(|event| matches!(event, Event::PointerButton { pressed: true, .. }))
        {
            1.0
        } else {
            0.016
        };
        ctx.run(
            RawInput {
                time: Some(*time),
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(3400.0, 700.0))),
                events,
                ..Default::default()
            },
            |ctx| {
                CentralPanel::default().show(ctx, |ui| {
                    screen.ui(
                        ctx,
                        ui,
                        icons,
                        &mut super::super::dialogs::TestDialogs::default(),
                    );
                });
            },
        )
    }

    fn click_cell(
        ctx: &Context,
        screen: &mut HexCompareScreen,
        icons: &mut IconManager,
        time: &mut f64,
        pos: Pos2,
    ) {
        for pressed in [true, false] {
            pointer_frame(
                ctx,
                screen,
                icons,
                time,
                vec![
                    Event::PointerMoved(pos),
                    Event::PointerButton {
                        pos,
                        button: PointerButton::Primary,
                        pressed,
                        modifiers: Modifiers::NONE,
                    },
                ],
            );
        }
        pointer_frame(ctx, screen, icons, time, vec![]);
    }

    #[test]
    fn mouse_targets_every_hex_and_character_cell_at_each_row_width_and_dpi() {
        for scale in [1.0, 1.5] {
            for count in [8, 16, 32] {
                let ctx = Context::default();
                super::super::theme::apply(&ctx);
                ctx.set_pixels_per_point(scale);
                let mut icons = IconManager::new();
                let mut screen = HexCompareScreen::new();
                screen.bytes_per_row = count;
                screen.left.bytes = (0..count * 3 + 3).map(|i| i as u8).collect();
                screen.right.bytes = screen.left.bytes.clone();
                screen.recompare();
                let before = (screen.left.bytes.clone(), screen.right.bytes.clone());
                let mut time = 0.0;
                for _ in 0..2 {
                    pointer_frame(&ctx, &mut screen, &mut icons, &mut time, vec![]);
                }
                for left in [true, false] {
                    for column in 0..count {
                        let cell = screen
                            .cells
                            .iter()
                            .find(|cell| cell.0 == left && cell.1 == column)
                            .copied()
                            .unwrap();
                        for rect in [cell.2, cell.3.unwrap()] {
                            let hover = pointer_frame(
                                &ctx,
                                &mut screen,
                                &mut icons,
                                &mut time,
                                vec![Event::PointerMoved(rect.center())],
                            );
                            assert_eq!(hover.platform_output.cursor_icon, CursorIcon::Text);
                            click_cell(&ctx, &mut screen, &mut icons, &mut time, rect.center());
                            assert_eq!(
                                (screen.active_left, screen.selected, screen.selection_end),
                                (left, column, column + 1),
                                "scale {scale}, width {count}, column {column}, rect {rect:?}"
                            );
                        }
                    }
                }
                assert_eq!((screen.left.bytes, screen.right.bytes), before);
            }
        }
    }

    #[test]
    fn clicking_and_dragging_visible_bytes_preserves_the_scrolled_viewport() {
        let ctx = Context::default();
        super::super::theme::apply(&ctx);
        let mut screen = HexCompareScreen::new();
        let mut icons = IconManager::new();
        screen.left.bytes = (0..4800).map(|i| i as u8).collect();
        screen.right.bytes = screen.left.bytes.clone();
        screen.recompare();
        let before = screen.left.bytes.clone();
        let mut time = 0.0;
        for _ in 0..2 {
            pointer_frame(&ctx, &mut screen, &mut icons, &mut time, vec![]);
        }
        let (_, id) = screen.viewport.unwrap();
        let mut state = egui::scroll_area::State::load(&ctx, id).unwrap();
        state.offset.y = 1800.0;
        state.store(&ctx, id);
        for _ in 0..2 {
            pointer_frame(&ctx, &mut screen, &mut icons, &mut time, vec![]);
        }
        let (viewport, id) = screen.viewport.unwrap();
        let offset = egui::scroll_area::State::load(&ctx, id).unwrap().offset;
        assert!(offset.y > 1000.0);
        for left in [true, false] {
            let cell = screen
                .cells
                .iter()
                .find(|cell| {
                    cell.0 == left && cell.1 % 16 == 3 && cell.2.center().y > viewport.top() + 140.0
                })
                .copied()
                .unwrap();
            let end = screen
                .cells
                .iter()
                .find(|end| end.0 == left && end.1 == cell.1 + 21)
                .copied()
                .unwrap();
            for ascii in [false, true] {
                let start_pos = if ascii {
                    cell.3.unwrap().center()
                } else {
                    cell.2.center()
                };
                let end_pos = if ascii {
                    end.3.unwrap().center()
                } else {
                    end.2.center()
                };
                click_cell(&ctx, &mut screen, &mut icons, &mut time, start_pos);
                assert_eq!(screen.selected_range(left), cell.1..cell.1 + 1);
                assert_eq!(
                    egui::scroll_area::State::load(&ctx, id).unwrap().offset,
                    offset,
                    "click must not move a visible row"
                );
                pointer_frame(
                    &ctx,
                    &mut screen,
                    &mut icons,
                    &mut time,
                    vec![
                        Event::PointerMoved(start_pos),
                        Event::PointerButton {
                            pos: start_pos,
                            button: PointerButton::Primary,
                            pressed: true,
                            modifiers: Modifiers::NONE,
                        },
                    ],
                );
                pointer_frame(
                    &ctx,
                    &mut screen,
                    &mut icons,
                    &mut time,
                    vec![Event::PointerMoved(end_pos)],
                );
                pointer_frame(
                    &ctx,
                    &mut screen,
                    &mut icons,
                    &mut time,
                    vec![Event::PointerButton {
                        pos: end_pos,
                        button: PointerButton::Primary,
                        pressed: false,
                        modifiers: Modifiers::NONE,
                    }],
                );
                pointer_frame(&ctx, &mut screen, &mut icons, &mut time, vec![]);
                assert_eq!(screen.selected_range(left), cell.1..end.1 + 1);
                assert_eq!(
                    egui::scroll_area::State::load(&ctx, id).unwrap().offset,
                    offset,
                    "drag must preserve the viewport"
                );
            }
        }
        assert_eq!(screen.left.bytes, before);
        assert_eq!(screen.right.bytes, before);
        // Explicit navigation still brings an off-screen byte into view.
        screen.select(4500, 4501);
        for _ in 0..4 {
            pointer_frame(&ctx, &mut screen, &mut icons, &mut time, vec![]);
        }
        assert!(egui::scroll_area::State::load(&ctx, id).unwrap().offset.y > offset.y);
        let (viewport, _) = screen.viewport.unwrap();
        assert!(
            screen
                .cells
                .iter()
                .any(|cell| cell.1 == 4500 && viewport.contains(cell.2.center())),
            "offset {:?}, viewport {viewport:?}, visible {:?}",
            egui::scroll_area::State::load(&ctx, id).unwrap().offset,
            screen
                .cells
                .iter()
                .filter(|cell| cell.0)
                .map(|cell| cell.1)
                .min()
                .zip(
                    screen
                        .cells
                        .iter()
                        .filter(|cell| cell.0)
                        .map(|cell| cell.1)
                        .max()
                )
        );
    }

    #[test]
    fn filtered_character_click_edits_the_same_byte_and_empty_cells_do_not_select() {
        let ctx = Context::default();
        super::super::theme::apply(&ctx);
        let mut screen = HexCompareScreen::new();
        let mut icons = IconManager::new();
        screen.left.bytes = (0..35).collect();
        screen.right.bytes = screen.left.bytes.clone();
        screen.right.bytes[21] = 200;
        screen.filter = FilterMode::Differences;
        screen.recompare();
        let mut time = 0.0;
        for _ in 0..2 {
            pointer_frame(&ctx, &mut screen, &mut icons, &mut time, vec![]);
        }
        let cell = screen
            .cells
            .iter()
            .find(|cell| !cell.0 && cell.1 == 21)
            .copied()
            .unwrap();
        click_cell(
            &ctx,
            &mut screen,
            &mut icons,
            &mut time,
            cell.3.unwrap().center(),
        );
        assert_eq!(screen.selected_range(false), 21..22);
        screen.editing = true;
        let mut dialogs = super::super::dialogs::TestDialogs::default();
        dialogs.inputs.push_back(Some("AB".into()));
        screen.edit(&mut dialogs);
        assert_eq!(&screen.right.bytes[20..23], &[20, 0xAB, 22]);
        screen.undo(false);
        assert_eq!(screen.right.bytes[21], 200);
        screen.undo(true);
        screen.copy_side(false);
        assert_eq!(screen.left.bytes[21], 0xAB);
        screen.undo(false);
        assert_eq!(screen.left.bytes[21], 21);

        screen.filter = FilterMode::All;
        screen.select(0, 1);
        pointer_frame(&ctx, &mut screen, &mut icons, &mut time, vec![]);
        let tail = screen
            .cells
            .iter()
            .find(|cell| cell.0 && cell.1 == 34)
            .copied()
            .unwrap();
        click_cell(&ctx, &mut screen, &mut icons, &mut time, tail.2.center());
        for pos in [
            tail.2.center() + vec2(23.0, 0.0),
            tail.3.unwrap().center() + vec2(tail.3.unwrap().width(), 0.0),
            pos2(tail.2.left() - 2.0 * 23.0 - 40.0, tail.2.center().y),
        ] {
            click_cell(&ctx, &mut screen, &mut icons, &mut time, pos);
            assert_eq!(screen.selected_range(true), 34..35, "empty target {pos:?}");
        }
        screen.show_ascii = false;
        pointer_frame(&ctx, &mut screen, &mut icons, &mut time, vec![]);
        assert!(screen.cells.iter().all(|cell| cell.3.is_none()));
    }

    #[test]
    fn aligned_pointer_copy_edit_undo_and_save_use_physical_file_offsets() {
        let root = std::env::temp_dir().join(format!(
            "anycompare-hex-aligned-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&root).unwrap();
        let a = root.join("left.bin");
        let b = root.join("right.bin");
        let base: Vec<u8> = (0..64).collect();
        let mut inserted = base.clone();
        inserted.insert(17, 255);
        std::fs::write(&a, &base).unwrap();
        std::fs::write(&b, &inserted).unwrap();
        let ctx = Context::default();
        super::super::theme::apply(&ctx);
        let mut screen = HexCompareScreen::new();
        let mut icons = IconManager::new();
        screen.load_files(&a, &b);
        assert_eq!(screen.difference_count, 1);
        let mut time = 0.0;
        for _ in 0..2 {
            pointer_frame(&ctx, &mut screen, &mut icons, &mut time, vec![]);
        }
        let cell = screen
            .cells
            .iter()
            .find(|cell| !cell.0 && cell.1 == 18)
            .copied()
            .unwrap();
        click_cell(
            &ctx,
            &mut screen,
            &mut icons,
            &mut time,
            cell.3.unwrap().center(),
        );
        assert_eq!(screen.selected_range(false), 18..19);
        assert_eq!(screen.selected_range(true), 17..18);
        screen.editing = true;
        let mut dialogs = super::super::dialogs::TestDialogs::default();
        dialogs.inputs.push_back(Some("AB".into()));
        screen.edit(&mut dialogs);
        assert_eq!(&screen.right.bytes[17..20], &[255, 0xAB, 18]);
        assert_eq!(&screen.left.bytes, &base);
        assert!(screen.save_side(false, false, &mut dialogs));
        let mut expected = inserted.clone();
        expected[18] = 0xAB;
        assert_eq!(std::fs::read(&b).unwrap(), expected);
        assert_eq!(std::fs::read(&a).unwrap(), base);
        screen.undo(false);
        assert_eq!(screen.right.bytes, inserted);
        screen.select(17, 18);
        screen.copy_side(false);
        assert_eq!(screen.left.bytes, inserted);
        assert_eq!(screen.difference_count, 0);
        screen.undo(false);
        assert_eq!(screen.left.bytes, base);
        screen.select(17, 18);
        screen.copy_side(true);
        assert_eq!(screen.right.bytes, base);
        screen.undo(false);
        assert_eq!(screen.right.bytes, inserted);
        std::fs::remove_file(a).unwrap();
        std::fs::remove_file(b).unwrap();
        std::fs::remove_dir(root).unwrap();
    }

    #[test]
    fn generated_toolbar_navigates_and_copies_the_active_byte() {
        let ctx = Context::default();
        super::super::theme::apply(&ctx);
        let mut screen = HexCompareScreen::new();
        let mut icons = IconManager::new();
        screen.left.bytes = b"ABCD".to_vec();
        screen.right.bytes = b"XBCY".to_vec();
        screen.recompare();
        let mut time = 0.0;
        for _ in 0..2 {
            pointer_frame(&ctx, &mut screen, &mut icons, &mut time, vec![]);
        }
        for (caption, index) in [
            ("Próxima Diferença", 3),
            ("Diferença Anterior", 0),
            ("Copiar", 0),
        ] {
            let frame = pointer_frame(&ctx, &mut screen, &mut icons, &mut time, vec![]);
            let pos = frame
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    Shape::Text(text) if text.galley.text() == crate::i18n::tr(caption) => {
                        Some(text.pos + text.galley.size() / 2.0)
                    }
                    _ => None,
                })
                .unwrap_or_else(|| panic!("toolbar caption missing: {caption}"));
            click_cell(&ctx, &mut screen, &mut icons, &mut time, pos);
            assert_eq!(screen.selected, index, "{caption}");
        }
        assert_eq!(screen.right.bytes, b"ABCY");
        screen.undo(false);
        assert_eq!(screen.right.bytes, b"XBCY");
    }

    #[test]
    fn byte_edits_copy_undo_save_cancel_and_disk_conflict() {
        let root = std::env::temp_dir().join(format!(
            "anycompare-hex-menu-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&root).unwrap();
        let a = root.join("a.bin");
        let b = root.join("b.bin");
        std::fs::write(&a, [0, 1, 2, 3, 4]).unwrap();
        std::fs::write(&b, [0, 9, 2]).unwrap();
        let mut screen = HexCompareScreen::new();
        screen.load_files(&a, &b);
        screen.select(1, 2);
        screen.copy_side(true);
        assert_eq!(screen.right.bytes, [0, 1, 2]);
        assert!(screen.right.dirty());
        screen.undo(false);
        assert_eq!(screen.right.bytes, [0, 9, 2]);
        screen.undo(true);
        assert_eq!(screen.right.bytes, [0, 1, 2]);
        let mut dialogs = super::super::dialogs::TestDialogs::default();
        dialogs.choices.push_back(SaveChoice::Cancel);
        assert!(!screen.confirm_saved(&mut dialogs));
        dialogs.choices.push_back(SaveChoice::Save);
        assert!(screen.confirm_saved(&mut dialogs));
        assert_eq!(std::fs::read(&b).unwrap(), [0, 1, 2]);
        screen.editing = true;
        screen.active_left = true;
        screen.select(0, 1);
        dialogs.inputs.push_back(Some("FF 00".into()));
        screen.edit(&mut dialogs);
        assert_eq!(screen.left.bytes, [255, 0, 1, 2, 3, 4]);
        let before = screen.left.bytes.clone();
        dialogs.inputs.push_back(Some("ZZ".into()));
        screen.edit(&mut dialogs);
        assert_eq!(screen.left.bytes, before);
        assert_eq!(dialogs.errors.len(), 1);
        std::fs::write(&a, [7, 8]).unwrap();
        dialogs.choices.push_back(SaveChoice::Save);
        assert!(!screen.confirm_saved(&mut dialogs));
        assert_eq!(std::fs::read(&a).unwrap(), [7, 8]);
        assert!(screen.left.dirty());
        std::fs::remove_file(a).unwrap();
        std::fs::remove_file(b).unwrap();
        std::fs::remove_dir(root).unwrap();
    }
}
