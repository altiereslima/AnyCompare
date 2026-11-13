//! Embedded find/replace strip and single-file cursor navigation.
use super::*;
use crate::search::SearchOptions;
use std::ops::Range;

#[derive(Clone)]
pub(super) struct Hit {
    range: Range<usize>,
    options: SearchOptions,
}
pub(super) struct State {
    pub shown: bool,
    replace_mode: bool,
    focus_query: bool,
    pub options: SearchOptions,
    pub replacement: String,
    pub wrap: bool,
    pub last: Option<Hit>,
    pub message: Option<String>,
    pub recent: Vec<String>,
    pub pending_history: Option<String>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            shown: false,
            replace_mode: false,
            focus_query: false,
            options: SearchOptions::default(),
            replacement: String::new(),
            wrap: true,
            last: None,
            message: None,
            recent: Vec::new(),
            pending_history: None,
        }
    }
}

impl TextEditScreen {
    pub fn set_recent_searches(&mut self, history: &[String]) {
        if self.search.recent != history {
            self.search.recent = history.to_vec();
        }
    }
    pub fn take_recent_search(&mut self) -> Option<String> {
        self.search.pending_history.take()
    }
    pub(super) fn selected_bytes(&self, ctx: &Context) -> Range<usize> {
        let cursors = TextEdit::load_state(ctx, self.editor_id)
            .and_then(|s| s.cursor.char_range())
            .unwrap_or_default()
            .sorted();
        let byte = |n| {
            self.document
                .text
                .char_indices()
                .nth(n)
                .map_or(self.document.text.len(), |(byte, _)| byte)
        };
        byte(cursors[0].index)..byte(cursors[1].index)
    }
    pub(super) fn select_bytes(&mut self, ctx: &Context, range: Range<usize>) {
        let start = self.document.text[..range.start].chars().count();
        let end = start + self.document.text[range].chars().count();
        let mut state = TextEdit::load_state(ctx, self.editor_id).unwrap_or_default();
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::two(
                egui::text::CCursor::new(start),
                egui::text::CCursor::new(end),
            )));
        state.store(ctx, self.editor_id);
        self.focus = true;
        self.scroll_to_cursor = true;
    }
    pub(super) fn show_search(&mut self, ctx: &Context, replace: bool) {
        let selection = self.selected_bytes(ctx);
        let text = &self.document.text[selection];
        if !text.is_empty() && !text.contains('\n') {
            self.search.options.query = text.to_owned();
        }
        self.search.shown = true;
        self.search.replace_mode = replace;
        self.search.focus_query = true;
        self.search.message = None;
        self.focus = false;
    }
    pub(super) fn find(&mut self, ctx: &Context, forward: bool) {
        if self.search.options.query.is_empty() {
            self.show_search(ctx, false);
            return;
        }
        let ranges = match self.search.options.ranges(&self.document.text) {
            Ok(ranges) => ranges,
            Err(error) => {
                self.search.message = Some(local_error(error));
                return;
            }
        };
        self.search.pending_history = Some(self.search.options.query.clone());
        let selection = self.selected_bytes(ctx);
        let skip = self
            .search
            .last
            .as_ref()
            .filter(|h| {
                h.options == self.search.options && h.range == selection && h.range.is_empty()
            })
            .map(|h| h.range.start);
        let eligible = |range: &&Range<usize>| !(range.is_empty() && skip == Some(range.start));
        let found = if forward {
            ranges
                .iter()
                .filter(eligible)
                .find(|r| r.start >= selection.end)
                .or_else(|| {
                    self.search
                        .wrap
                        .then(|| ranges.iter().find(eligible))
                        .flatten()
                })
        } else {
            ranges
                .iter()
                .rev()
                .filter(eligible)
                .find(|r| r.end <= selection.start)
                .or_else(|| {
                    self.search
                        .wrap
                        .then(|| ranges.iter().rev().find(eligible))
                        .flatten()
                })
        }
        .cloned();
        if let Some(range) = found {
            self.select_bytes(ctx, range.clone());
            self.search.last = Some(Hit {
                range,
                options: self.search.options.clone(),
            });
            self.search.message = None;
        } else {
            self.search.last = None;
            self.search.message = Some(tr("Texto não encontrado."));
        }
    }
    /// One explicit replacement is one undo point; creating a new edit after
    /// Undo clears stale Redo states rather than restoring a previous document.
    fn apply_replacement(&mut self, ctx: &Context, text: String, cursor: usize) {
        if self.read_only {
            return;
        }
        let mut state = TextEdit::load_state(ctx, self.editor_id).unwrap_or_default();
        if self.document.text != text {
            let before = self.edit_point(ctx);
            self.document.text = text;
            state
                .cursor
                .set_char_range(Some(egui::text::CCursorRange::one(
                    egui::text::CCursor::new(cursor),
                )));
            self.remember_change(before);
            state.clear_undoer();
        } else {
            state
                .cursor
                .set_char_range(Some(egui::text::CCursorRange::one(
                    egui::text::CCursor::new(cursor),
                )));
        }
        state.store(ctx, self.editor_id);
        self.search.last = None;
        self.focus = true;
        self.scroll_to_cursor = true;
    }
    pub(super) fn replace_found(&mut self, ctx: &Context) {
        if self.read_only {
            return;
        }
        let range = self.selected_bytes(ctx);
        let valid = self
            .search
            .last
            .as_ref()
            .is_some_and(|hit| hit.range == range && hit.options == self.search.options);
        if !valid {
            self.find(ctx, true);
            return;
        }
        let ranges = match self.search.options.ranges(&self.document.text) {
            Ok(r) => r,
            Err(e) => {
                self.search.message = Some(local_error(e));
                return;
            }
        };
        if !ranges.contains(&range) {
            self.search.last = None;
            self.find(ctx, true);
            return;
        }
        match self
            .search
            .options
            .replacement(&self.document.text[range.clone()], &self.search.replacement)
        {
            Ok(value) => {
                let cursor =
                    self.document.text[..range.start].chars().count() + value.chars().count();
                let end = range.start + value.len();
                let mut text = self.document.text.clone();
                text.replace_range(range.clone(), &value);
                self.apply_replacement(ctx, text, cursor);
                if range.is_empty() {
                    self.search.last = Some(Hit {
                        range: end..end,
                        options: self.search.options.clone(),
                    });
                }
                self.find(ctx, true);
            }
            Err(error) => self.search.message = Some(local_error(error)),
        }
    }
    pub(super) fn replace_all(&mut self, ctx: &Context) {
        if self.read_only {
            return;
        }
        match self
            .search
            .options
            .replace_all(&self.document.text, &self.search.replacement)
        {
            Ok((text, count)) => {
                self.search.pending_history = Some(self.search.options.query.clone());
                let cursor = TextEdit::load_state(ctx, self.editor_id)
                    .and_then(|s| s.cursor.char_range())
                    .map_or(0, |s| s.primary.index)
                    .min(text.chars().count());
                if count > 0 {
                    self.apply_replacement(ctx, text, cursor);
                }
                self.search.message = Some(format!("{count} {}", tr("substituições")));
            }
            Err(error) => self.search.message = Some(local_error(error)),
        }
    }
    pub(super) fn go_to(&mut self, ctx: &Context, dialogs: &mut dyn DialogHost) {
        let cursor = self.selected_bytes(ctx).start;
        let line = self.document.text[..cursor]
            .bytes()
            .filter(|b| *b == b'\n')
            .count()
            + 1;
        let line_start = self.document.text[..cursor]
            .rfind('\n')
            .map_or(0, |i| i + 1);
        let column = self.document.text[line_start..cursor].chars().count() + 1;
        let Some(value) = dialogs.input("Ir para", "Linha[:coluna]:", &format!("{line}:{column}"))
        else {
            return;
        };
        let parts: Vec<_> = value.trim().split(':').collect();
        let line = parts[0].parse::<usize>();
        let column = parts.get(1).copied().unwrap_or("1").parse::<usize>();
        if let (Ok(line), Ok(column)) = (line, column) {
            if line > 0 && column > 0 && parts.len() <= 2 {
                let mut start = 0;
                for (index, content) in self.document.text.split('\n').enumerate() {
                    if index + 1 == line {
                        let offset = content
                            .char_indices()
                            .nth(column - 1)
                            .map_or(content.len(), |(i, _)| i);
                        self.select_bytes(ctx, start + offset..start + offset);
                        return;
                    }
                    start += content.len() + 1;
                }
            }
        }
        dialogs.error(&tr("Linha ou coluna inválida."));
    }
    pub(super) fn search_panel(
        &mut self,
        ctx: &Context,
        ui: &mut Ui,
        icons: &mut IconManager,
        dialogs: &mut dyn DialogHost,
    ) {
        if !self.search.shown {
            return;
        }
        let previous_options = self.search.options.clone();
        let mut command = None;
        TopBottomPanel::top("single_file_find")
            .frame(
                Frame::none()
                    .fill(Color32::from_rgb(214, 224, 237))
                    .inner_margin(Margin::symmetric(8.0, 4.0)),
            )
            .show_inside(ui, |ui| {
                ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
                ui.horizontal(|ui| {
                    if let Some(image) = icons.get_icon(ctx, "imgSearch") {
                        ui.image((image.id(), vec2(16.0, 16.0)));
                    }
                    let response = history_combo::show(
                        ui,
                        "single_file_query",
                        &mut self.search.options.query,
                        &self.search.recent,
                        (ui.available_width() - 282.0).max(60.0),
                        "Localizar",
                    );
                    if self.search.focus_query {
                        response.response.request_focus();
                        if let Some(mut state) = TextEdit::load_state(ctx, response.response.id) {
                            state
                                .cursor
                                .set_char_range(Some(egui::text::CCursorRange::two(
                                    egui::text::CCursor::new(0),
                                    egui::text::CCursor::new(
                                        self.search.options.query.chars().count(),
                                    ),
                                )));
                            state.store(ctx, response.response.id);
                        }
                        self.search.focus_query = false;
                    }
                    if response.submitted || response.selected.is_some() {
                        command = Some(Action::FindNext(true));
                    }
                    for (value, image, hint) in [
                        (
                            &mut self.search.options.regular_expression,
                            "imgFindIsRegExp",
                            "Expressão regular",
                        ),
                        (
                            &mut self.search.options.case_sensitive,
                            "imgFindCaseSensitive",
                            "Maiúsculas/minúsculas",
                        ),
                        (
                            &mut self.search.options.whole_word,
                            "imgFindWholeWords",
                            "Palavra inteira",
                        ),
                        (&mut self.search.wrap, "imgFindWrap", "Voltar ao início"),
                    ] {
                        if let Some(image) = icons.get_icon(ctx, image) {
                            if ui
                                .add(
                                    Button::image((image.id(), vec2(16.0, 16.0)))
                                        .selected(*value)
                                        .min_size(vec2(23.0, 23.0)),
                                )
                                .on_hover_text(tr(hint))
                                .clicked()
                            {
                                *value = !*value;
                            }
                        }
                    }
                    for (label, forward) in [("Anterior", false), ("Próximo", true)] {
                        if ui
                            .add(Button::new(tr(label)).min_size(vec2(61.0, 23.0)))
                            .clicked()
                        {
                            command = Some(Action::FindNext(forward));
                        }
                    }
                    if ui
                        .button("×")
                        .on_hover_text(tr("Fechar pesquisa"))
                        .clicked()
                    {
                        self.search.shown = false;
                        self.focus = true;
                    }
                    #[cfg(test)]
                    ui.data_mut(|d| {
                        d.insert_temp(Id::new("text_edit_query_rect"), response.response.rect)
                    });
                });
                if self.search.replace_mode {
                    ui.horizontal(|ui| {
                        ui.label(tr("Substituir:"));
                        let _response = ui.add(
                            TextEdit::singleline(&mut self.search.replacement)
                                .desired_width((ui.available_width() - 270.0).max(60.0)),
                        );
                        #[cfg(test)]
                        ui.data_mut(|d| {
                            d.insert_temp(Id::new("text_edit_replace_rect"), _response.rect)
                        });
                        for (label, action) in [
                            ("Substituir", Action::Replace),
                            ("Substituir tudo", Action::ReplaceAll),
                        ] {
                            let response = ui.add_enabled(!self.read_only, Button::new(tr(label)));
                            #[cfg(test)]
                            ui.data_mut(|d| {
                                d.insert_temp(
                                    Id::new("text_edit_find_action").with(label),
                                    (response.rect, !self.read_only),
                                )
                            });
                            if response.clicked() {
                                command = Some(action);
                            }
                        }
                    });
                }
                if let Some(message) = &self.search.message {
                    ui.label(message);
                }
            });
        if self.search.options != previous_options {
            self.search.last = None;
            self.search.message = None;
        }
        if let Some(command) = command {
            self.run(command, ctx, dialogs);
        }
    }
}
fn local_error(error: String) -> String {
    error.strip_prefix("Expressão inválida: ").map_or_else(
        || tr(&error),
        |detail| format!("{} {detail}", tr("Expressão inválida:")),
    )
}
