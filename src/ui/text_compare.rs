use super::aligned_editor::{
    self, EditorCache, LayoutOptions, FONT_SIZE, GUTTER_WIDTH, ROW_HEIGHT, TEXT_OFFSET,
};
use super::dialogs::{DialogField, DialogHost, SaveChoice};
use super::text_view::{self, RowKind, TextView, FOLD_HEIGHT};
use super::toolbar::{ToolEvent, ToolState};

/// Copy button of a margin row (REA 0xf034e0: margin left + 4, one 14 px button).
const GUTTER_BUTTON_LEFT: f32 = 4.0;
const GUTTER_BUTTON_WIDTH: f32 = 14.0;
use crate::diff::{
    AlignedLine, DiffEngine, DiffResult, DiffStatus, FilterMode, InlineChunk, InlineChunkKind,
};
use crate::document::{Document, Encoding, LineEnding};
use crate::ui::{icons::IconManager, theme::BcTheme};
use egui::*;
use std::{
    ops::Range,
    path::{Path, PathBuf},
    time::Instant,
};

struct SearchState {
    options: crate::search::SearchOptions,
    replacement: String,
    shown: bool,
    replace_mode: bool,
    advanced: bool,
    sides: [bool; 2],
    wrap: bool,
    message: Option<String>,
    last: Option<(bool, Range<usize>, u64)>,
    focus_query: bool,
}
impl Default for SearchState {
    fn default() -> Self {
        Self {
            options: Default::default(),
            replacement: String::new(),
            shown: false,
            replace_mode: false,
            advanced: false,
            sides: [true, true],
            wrap: true,
            message: None,
            last: None,
            focus_query: false,
        }
    }
}

pub enum TextCompareAction {
    None,
    BackToHome,
}

pub struct TextCompareScreen {
    pub left_path: String,
    pub right_path: String,
    pub left: Document,
    pub right: Document,
    pub diff_result: DiffResult,
    pub filter_mode: FilterMode,
    pub selected_line_idx: usize,
    load_time_ms: u128,
    error: Option<String>,
    editing: bool,
    scroll_to_selection: bool,
    undo: Vec<(String, String)>,
    redo: Vec<(String, String)>,
    context_lines: bool,
    line_numbers: bool,
    active_left: bool,
    scroll_offset_y: f32,
    scroll_offset_x: [f32; 2],
    details_offset_x: f32,
    navigation_mode: super::text_chrome::NavigationMode,
    max_line_lengths: [usize; 2],
    editor_cache: [EditorCache; 2],
    revision: u64,
    caret: [(usize, usize); 2],
    revealed: Vec<std::ops::Range<usize>>,
    view_revision: u64,
    previous_filter: FilterMode,
    search: SearchState,
    recent_paths: Vec<String>,
    recent_searches: Vec<String>,
    pending_search_history: Option<String>,
    pending_selection: Option<(bool, Range<usize>)>,
    /// Pane and anchor row of the last line selection made in the margin (Shift+click
    /// extends from it; a click in the text drops it).
    margin_anchor: Option<(bool, usize)>,
    /// Pane of a line selection being dragged: set by a press in the margin, cleared on
    /// release. Only then does the selection follow the pointer by lines.
    margin_selecting: Option<bool>,
    show_details: bool,
    show_thumbnail: bool,
    show_whitespace: bool,
    show_filtered_counts: bool,
    rules: crate::diff::DiffRules,
    rules_enabled: bool,
    ignore_minor: bool,
    #[cfg(test)]
    fold_controls: Vec<(bool, std::ops::Range<usize>, Rect)>,
    #[cfg(test)]
    gutter_arrows: Vec<(bool, usize, Rect, GutterArrow)>,
    #[cfg(test)]
    chrome_controls: Vec<(bool, Rect, Rect, Id)>,
}

/// Gutter button of a text pane. Without a selection the pane shows one
/// "Copy Section" arrow per difference; with a selection it shows a single
/// "Copy Line(s)" arrow on the first selected row (as in the original).
#[derive(Clone, Copy, Debug, PartialEq)]
enum GutterArrow {
    Section,
    Selection,
}

impl GutterArrow {
    fn for_row(index: usize, block_start: bool, selection: Option<&Range<usize>>) -> Option<Self> {
        match selection {
            Some(rows) => (index == rows.start).then_some(Self::Selection),
            None => block_start.then_some(Self::Section),
        }
    }

    /// Hint of the arrow drawn in the `left` pane, which copies to the other side.
    fn hint(self, left: bool) -> &'static str {
        match (self, left) {
            (Self::Section, true) => "Copiar Seção para Direita",
            (Self::Section, false) => "Copiar Seção para Esquerda",
            (Self::Selection, true) => "Copiar Linha(s) para Direita",
            (Self::Selection, false) => "Copiar Linha(s) para Esquerda",
        }
    }
}

impl TextCompareScreen {
    pub fn session_options(&self) -> serde_json::Value {
        serde_json::json!({"filter":format!("{:?}",self.filter_mode),"editing":self.editing,"context":self.context_lines,"line_numbers":self.line_numbers,"details":self.show_details,"thumbnail":self.show_thumbnail,"whitespace":self.show_whitespace,"filtered_counts":self.show_filtered_counts,"ignore_minor":self.ignore_minor,"ignore_case":self.rules.ignore_case,"ignore_whitespace":self.rules.ignore_whitespace,"rules_enabled":self.rules_enabled})
    }
    pub fn apply_session_options(&mut self, value: &serde_json::Value) {
        self.filter_mode = match value["filter"].as_str() {
            Some("Differences") => FilterMode::Differences,
            Some("Same") => FilterMode::Same,
            _ => FilterMode::All,
        };
        for (key, target) in [
            ("editing", &mut self.editing),
            ("context", &mut self.context_lines),
            ("line_numbers", &mut self.line_numbers),
            ("details", &mut self.show_details),
            ("thumbnail", &mut self.show_thumbnail),
            ("whitespace", &mut self.show_whitespace),
            ("filtered_counts", &mut self.show_filtered_counts),
            ("ignore_minor", &mut self.ignore_minor),
            ("ignore_case", &mut self.rules.ignore_case),
            ("ignore_whitespace", &mut self.rules.ignore_whitespace),
            ("rules_enabled", &mut self.rules_enabled),
        ] {
            if let Some(value) = value[key].as_bool() {
                *target = value;
            }
        }
        self.recompare();
    }
    pub fn new() -> Self {
        Self {
            left_path: String::new(),
            right_path: String::new(),
            left: Document::default(),
            right: Document::default(),
            diff_result: DiffEngine::compare("", ""),
            filter_mode: FilterMode::All,
            selected_line_idx: 0,
            load_time_ms: 0,
            error: None,
            editing: true,
            scroll_to_selection: false,
            undo: Vec::new(),
            redo: Vec::new(),
            context_lines: false,
            line_numbers: false,
            active_left: true,
            scroll_offset_y: 0.0,
            scroll_offset_x: [0.0; 2],
            details_offset_x: 0.0,
            navigation_mode: Default::default(),
            max_line_lengths: [0, 0],
            editor_cache: Default::default(),
            revision: 0,
            caret: [(1, 1); 2],
            revealed: Vec::new(),
            view_revision: 0,
            previous_filter: FilterMode::All,
            search: SearchState::default(),
            recent_paths: Vec::new(),
            recent_searches: Vec::new(),
            pending_search_history: None,
            pending_selection: None,
            margin_anchor: None,
            margin_selecting: None,
            show_details: true,
            show_thumbnail: true,
            show_whitespace: false,
            show_filtered_counts: true,
            rules: Default::default(),
            rules_enabled: true,
            ignore_minor: false,
            #[cfg(test)]
            fold_controls: Vec::new(),
            #[cfg(test)]
            gutter_arrows: Vec::new(),
            #[cfg(test)]
            chrome_controls: Vec::new(),
        }
    }

    pub fn load_files(&mut self, left: &str, right: &str) {
        let started = Instant::now();
        match (
            Document::load(Path::new(left)),
            Document::load(Path::new(right)),
        ) {
            (Ok(l), Ok(r)) => {
                self.left = l;
                self.right = r;
                self.left_path = left.to_owned();
                self.right_path = right.to_owned();
                self.undo.clear();
                self.redo.clear();
                self.revealed.clear();
                self.selected_line_idx = 0;
                self.error = None;
                self.compare();
                self.load_time_ms = started.elapsed().as_millis();
            }
            (Err(e), _) | (_, Err(e)) => self.error = Some(e),
        }
    }

    pub fn open_side(&mut self, left: bool, path: PathBuf, dialogs: &mut dyn DialogHost) {
        if let Err(error) = Document::load(&path) {
            self.error = Some(error.clone());
            dialogs.error(&error);
            return;
        }
        if self.confirm_side(left, dialogs) {
            self.commit_open(left, &path);
        }
    }

    fn commit_open(&mut self, left: bool, path: &Path) {
        match Document::load(path) {
            Ok(doc) => {
                if left {
                    self.left = doc;
                    self.left_path = path.to_string_lossy().into();
                } else {
                    self.right = doc;
                    self.right_path = path.to_string_lossy().into();
                }
                self.error = None;
                self.undo.clear();
                self.redo.clear();
                self.revealed.clear();
                self.selected_line_idx = 0;
                self.compare();
            }
            Err(error) => self.error = Some(error),
        }
    }

    fn reopen_encoded(
        &mut self,
        left: bool,
        encoding: Option<Encoding>,
        dialogs: &mut dyn DialogHost,
    ) {
        let path = if left {
            self.left.path.clone()
        } else {
            self.right.path.clone()
        };
        let Some(path) = path else {
            return;
        };
        // Validate the requested decoding before asking to discard any buffer.
        if let Err(error) = Document::load_with_encoding(&path, encoding) {
            self.error = Some(error.clone());
            dialogs.error(&error);
            return;
        }
        if !self.confirm_side(left, dialogs) {
            return;
        }
        // Save may have changed the bytes: decode again after the save decision.
        match Document::load_with_encoding(&path, encoding) {
            Ok(document) => {
                if left {
                    self.left = document;
                } else {
                    self.right = document;
                }
                self.undo.clear();
                self.redo.clear();
                self.revealed.clear();
                self.error = None;
                self.recompare();
            }
            Err(error) => {
                self.error = Some(error.clone());
                dialogs.error(&error);
            }
        }
    }

    fn compare(&mut self) {
        let start = Instant::now();
        let result = DiffEngine::update_rules(
            &self.left.text,
            &self.right.text,
            &self.diff_result,
            self.active_rules(),
        );
        self.accept_comparison(result, start.elapsed().as_millis());
    }

    fn recompare(&mut self) {
        let start = Instant::now();
        let result =
            DiffEngine::compare_rules(&self.left.text, &self.right.text, self.active_rules());
        self.accept_comparison(result, start.elapsed().as_millis());
    }

    fn accept_comparison(&mut self, result: DiffResult, ms: u128) {
        self.diff_result = result;
        self.revision = self.revision.wrapping_add(1);
        self.max_line_lengths = [
            self.left
                .text
                .lines()
                .map(|s| s.chars().count())
                .max()
                .unwrap_or(0),
            self.right
                .text
                .lines()
                .map(|s| s.chars().count())
                .max()
                .unwrap_or(0),
        ];
        self.load_time_ms = ms;
        self.selected_line_idx = self
            .selected_line_idx
            .min(self.diff_result.lines.len().saturating_sub(1));
    }

    pub fn save_side(&mut self, left: bool, save_as: bool, dialogs: &mut dyn DialogHost) -> bool {
        let doc = if left {
            &mut self.left
        } else {
            &mut self.right
        };
        let path = if save_as || doc.path.is_none() {
            dialogs.file(true, left, doc.path.as_deref())
        } else {
            doc.path.clone()
        };
        if let Some(path) = path {
            if doc.encoding == Encoding::Windows1252
                && encoding_rs::WINDOWS_1252.encode(&doc.text).2
            {
                // TFILECODEPAGEERRORDIALOG has a warning, encoding picker and
                // Save/Cancel. Never accept lossy replacement characters.
                let encodings: Vec<_> = Encoding::ALL
                    .into_iter()
                    .filter(|e| *e != Encoding::Windows1252)
                    .collect();
                let mut fields = [
                    DialogField::Notice("Este arquivo contém caracteres Unicode que seriam perdidos com a codificação atual. Selecione uma codificação para preservar o texto.".into()),
                    DialogField::Choice("Codificação:".into(), 0, encodings.iter().map(|e| e.label().to_owned()).collect()),
                ];
                if !dialogs.fields_with_accept("Erro de codificação", &mut fields, "Salvar") {
                    return false;
                }
                if let DialogField::Choice(_, index, _) = fields[1] {
                    doc.encoding = encodings[index];
                }
            }
            match doc.save(&path) {
                Ok(()) => {
                    if left {
                        self.left_path = path.to_string_lossy().into();
                    } else {
                        self.right_path = path.to_string_lossy().into();
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
        let document = if left { &self.left } else { &self.right };
        if !document.dirty() {
            return true;
        }
        match dialogs.confirm_save(left, document.path.as_deref()) {
            SaveChoice::Save => self.save_side(left, false, dialogs),
            SaveChoice::Discard => true,
            SaveChoice::Cancel => false,
        }
    }

    pub fn confirm_saved(&mut self, dialogs: &mut dyn DialogHost) -> bool {
        self.confirm_side(true, dialogs) && self.confirm_side(false, dialogs)
    }

    fn remember_edit(&mut self) {
        self.redo.clear();
        // Whole-buffer history is bounded; the native text widgets also retain edit undo.
        if self.undo.len() >= 20 {
            self.undo.remove(0);
        }
        self.undo
            .push((self.left.text.clone(), self.right.text.clone()));
    }

    /// Copy the section of the current line (gutter arrow, and the command
    /// fallback when nothing is selected).
    pub fn copy_difference(&mut self, to_right: bool) {
        let text = DiffEngine::merge_block(
            &self.left.text,
            &self.right.text,
            &self.diff_result,
            self.selected_line_idx,
            to_right,
        );
        self.apply_copy(text, to_right);
    }

    /// CopyLeft/CopyRight/CopySide: "Copies selected lines or current section".
    /// The active pane's selection defines the aligned rows in either direction.
    fn copy_selected(&mut self, ctx: &Context, to_right: bool) {
        let selection = self.selected_bytes(ctx, self.active_left);
        let Some(rows) = self.selected_rows(self.active_left, selection) else {
            self.copy_difference(to_right);
            return;
        };
        self.copy_rows(rows, to_right);
    }

    /// Copy whole aligned rows to the other side (selection arrow and commands).
    fn copy_rows(&mut self, rows: Range<usize>, to_right: bool) {
        let text = DiffEngine::merge_range(
            &self.left.text,
            &self.right.text,
            &self.diff_result,
            rows,
            to_right,
        );
        self.apply_copy(text, to_right);
    }

    /// Aligned rows covered by a byte selection of one pane. A partially
    /// selected line counts whole; a selection ending right after `\n` stops at
    /// that line. Selecting the whole document also covers the other side's
    /// orphan rows before the first and after the last line.
    fn selected_rows(&self, left: bool, selection: Range<usize>) -> Option<Range<usize>> {
        if selection.is_empty() {
            return None;
        }
        let text = if left {
            &self.left.text
        } else {
            &self.right.text
        };
        let lines = &self.diff_result.lines;
        if selection == (0..text.len()) {
            return (!lines.is_empty()).then_some(0..lines.len());
        }
        let newlines = |end: usize| text[..end].bytes().filter(|&b| b == b'\n').count();
        let first = newlines(selection.start) + 1;
        let last = newlines(selection.end) + usize::from(!text[..selection.end].ends_with('\n'));
        let number = |line: &AlignedLine| {
            if left {
                line.left_line_no
            } else {
                line.right_line_no
            }
        };
        let start = lines.iter().position(|line| number(line) == Some(first))?;
        let end = lines.iter().rposition(|line| number(line) == Some(last))? + 1;
        Some(start..end)
    }

    /// Select a whole pane without moving the view. egui's own Ctrl+A puts the
    /// caret at the end and scrolls there, which on long files slides the view
    /// down to the last line.
    fn select_all(&mut self, ctx: &Context, left: bool) {
        let Some(id) = self.editor_cache[usize::from(!left)].widget_id else {
            return;
        };
        let text = if left {
            &self.left.text
        } else {
            &self.right.text
        };
        let mut state = TextEdit::load_state(ctx, id).unwrap_or_default();
        // Set before the editor runs, so it sees no selection change to follow.
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::two(
                egui::text::CCursor::new(0),
                egui::text::CCursor::new(text.chars().count()),
            )));
        state.store(ctx, id);
        ctx.memory_mut(|memory| memory.request_focus(id));
        self.active_left = left;
    }

    fn has_selection(&self, ctx: &Context, left: bool) -> bool {
        self.editor_cache[usize::from(!left)]
            .widget_id
            .and_then(|id| TextEdit::load_state(ctx, id))
            .and_then(|state| state.cursor.char_range())
            .is_some_and(|range| range.primary.index != range.secondary.index)
    }

    fn copy_line(&mut self, to_right: bool) {
        let text = DiffEngine::merge_range(
            &self.left.text,
            &self.right.text,
            &self.diff_result,
            self.selected_line_idx..self.selected_line_idx + 1,
            to_right,
        );
        self.apply_copy(text, to_right);
    }

    fn apply_copy(&mut self, text: Option<String>, to_right: bool) {
        let Some(text) = text else {
            return;
        };
        let target = if to_right {
            &self.right.text
        } else {
            &self.left.text
        };
        if *target == text {
            return;
        }
        self.remember_edit();
        if to_right {
            self.right.text = text;
        } else {
            self.left.text = text;
        }
        self.compare();
    }

    fn transform_active(&mut self, transform: impl FnOnce(&str) -> String) {
        if !self.editing {
            return;
        }
        let text = if self.active_left {
            &self.left.text
        } else {
            &self.right.text
        };
        let next = transform(text);
        if next == *text {
            return;
        }
        self.remember_edit();
        if self.active_left {
            self.left.text = next;
        } else {
            self.right.text = next;
        }
        self.compare();
    }

    fn select_section(&mut self) {
        let Some(range) = self
            .diff_result
            .sections
            .iter()
            .find(|r| r.contains(&self.selected_line_idx))
            .cloned()
        else {
            return;
        };
        let number = |line: &AlignedLine| {
            if self.active_left {
                line.left_line_no
            } else {
                line.right_line_no
            }
        };
        let start = self.diff_result.lines[..range.start]
            .iter()
            .filter(|line| number(line).is_some())
            .count();
        let count = self.diff_result.lines[range]
            .iter()
            .filter(|line| number(line).is_some())
            .count();
        let text = if self.active_left {
            &self.left.text
        } else {
            &self.right.text
        };
        let a = text.split_inclusive('\n').take(start).map(str::len).sum();
        let b = text
            .split_inclusive('\n')
            .take(start + count)
            .map(str::len)
            .sum();
        self.select_bytes(self.active_left, a..b);
    }

    fn edit_lines(&mut self, ctx: &Context, operation: u8) {
        if !self.editing {
            return;
        }
        let selection = self.selected_bytes(ctx, self.active_left);
        let text = if self.active_left {
            &self.left.text
        } else {
            &self.right.text
        };
        let start = text[..selection.start]
            .rfind('\n')
            .map(|i| i + 1)
            .unwrap_or(0);
        let end = if selection.end > selection.start && text[..selection.end].ends_with('\n') {
            selection.end
        } else {
            text[selection.end..]
                .find('\n')
                .map(|i| selection.end + i + 1)
                .unwrap_or(text.len())
        };
        let selected = text[start..end].to_owned();
        self.transform_active(|text| {
            let mut result = text.to_owned();
            match operation {
                0 => {
                    result.replace_range(start..end, "");
                }
                1 => result.insert(start, '\n'),
                2 => {
                    if end == text.len() && !text.ends_with('\n') {
                        result.push('\n');
                    } else {
                        result.insert(end, '\n');
                    }
                }
                3 => {
                    let replacement = selected
                        .split_inclusive('\n')
                        .map(|line| format!("    {line}"))
                        .collect::<String>();
                    result.replace_range(start..end, &replacement);
                }
                4 => {
                    let replacement = selected
                        .split_inclusive('\n')
                        .map(|line| {
                            if let Some(line) = line.strip_prefix('\t') {
                                line
                            } else {
                                &line[line.bytes().take(4).take_while(|&b| b == b' ').count()..]
                            }
                        })
                        .collect::<String>();
                    result.replace_range(start..end, &replacement);
                }
                _ => unreachable!(),
            }
            result
        });
    }

    fn undo_merge(&mut self) {
        if let Some((left, right)) = self.undo.pop() {
            self.redo
                .push((self.left.text.clone(), self.right.text.clone()));
            self.left.text = left;
            self.right.text = right;
            self.compare();
        }
    }

    fn redo_edit(&mut self) {
        if let Some((left, right)) = self.redo.pop() {
            if self.undo.len() >= 20 {
                self.undo.remove(0);
            }
            self.undo
                .push((self.left.text.clone(), self.right.text.clone()));
            self.left.text = left;
            self.right.text = right;
            self.compare();
        }
    }

    fn reload(&mut self, dialogs: &mut dyn DialogHost) {
        if !self.confirm_saved(dialogs) {
            return;
        }
        let left = self.left.path.clone();
        let right = self.right.path.clone();
        if let Some(path) = left {
            self.commit_open(true, &path);
        }
        if let Some(path) = right {
            self.commit_open(false, &path);
        }
    }

    fn swap_sides(&mut self) {
        std::mem::swap(&mut self.left, &mut self.right);
        std::mem::swap(&mut self.left_path, &mut self.right_path);
        self.undo.clear();
        self.redo.clear();
        self.revealed.clear();
        self.compare();
    }

    fn next_diff(&mut self, forward: bool) {
        let sections = &self.diff_result.sections;
        let next = if forward {
            sections
                .iter()
                .find(|section| section.start > self.selected_line_idx)
        } else {
            sections
                .iter()
                .rev()
                .find(|section| section.start < self.selected_line_idx)
        };
        if let Some(section) = next {
            self.selected_line_idx = section.start;
            self.scroll_to_selection = true;
        }
    }

    fn selected_bytes(&self, ctx: &Context, left: bool) -> Range<usize> {
        let text = if left {
            &self.left.text
        } else {
            &self.right.text
        };
        let cursor = self.editor_cache[usize::from(!left)]
            .widget_id
            .and_then(|id| TextEdit::load_state(ctx, id))
            .and_then(|state| state.cursor.char_range());
        let byte = |index| {
            text.char_indices()
                .nth(index)
                .map(|(i, _)| i)
                .unwrap_or(text.len())
        };
        cursor
            .map(|r| {
                let a = byte(r.primary.index);
                let b = byte(r.secondary.index);
                a.min(b)..a.max(b)
            })
            .unwrap_or(0..0)
    }

    fn select_bytes(&mut self, left: bool, range: Range<usize>) {
        let text = if left {
            &self.left.text
        } else {
            &self.right.text
        };
        let number = text[..range.start].bytes().filter(|&b| b == b'\n').count() + 1;
        let chars = text[..range.start].chars().count()..text[..range.end].chars().count();
        if let Some(index) = self.diff_result.lines.iter().position(|line| {
            (if left {
                line.left_line_no
            } else {
                line.right_line_no
            }) == Some(number)
        }) {
            if self.previous_filter != self.filter_mode {
                self.revealed.clear();
                self.previous_filter = self.filter_mode;
            }
            self.selected_line_idx = index;
            self.revealed.push(index..index + 1);
            self.view_revision = self.view_revision.wrapping_add(1);
        }
        self.active_left = left;
        self.scroll_to_selection = true;
        self.pending_selection = Some((left, chars));
    }

    fn find(&mut self, ctx: &Context, forward: bool) {
        let active = self.active_left;
        let cursor = self.selected_bytes(ctx, active);
        let from = if forward { cursor.end } else { cursor.start };
        let mut candidates = Vec::new();
        let mut wrapped = Vec::new();
        for side in [active, !active] {
            if !self.search.sides[usize::from(!side)] {
                continue;
            }
            let text = if side {
                &self.left.text
            } else {
                &self.right.text
            };
            let ranges = match self.search.options.ranges(text) {
                Ok(ranges) => ranges,
                Err(error) => {
                    self.search.message = Some(error);
                    return;
                }
            };
            let iter: Box<dyn Iterator<Item = Range<usize>>> = if forward {
                Box::new(ranges.into_iter())
            } else {
                Box::new(ranges.into_iter().rev())
            };
            for range in iter {
                if self
                    .search
                    .last
                    .as_ref()
                    .is_some_and(|(last_side, last, revision)| {
                        *last_side == side
                            && *revision == self.revision
                            && *last == range
                            && range.is_empty()
                    })
                {
                    continue;
                }
                if side != active
                    || if forward {
                        range.start >= from
                    } else {
                        range.end <= from
                    }
                {
                    candidates.push((side, range));
                } else {
                    wrapped.push((side, range));
                }
            }
        }
        if self.search.wrap {
            candidates.extend(wrapped);
        }
        if !self.search.options.query.is_empty() {
            self.pending_search_history = Some(self.search.options.query.clone());
        }
        if let Some((side, range)) = candidates.into_iter().next() {
            self.search.last = Some((side, range.clone(), self.revision));
            self.select_bytes(side, range);
            self.search.message = None;
        } else {
            self.search.message = Some("Texto não encontrado.".into());
        }
    }

    fn replace_found(&mut self, ctx: &Context) {
        if !self.editing {
            return;
        }
        let selection = self.selected_bytes(ctx, self.active_left);
        let valid = self
            .search
            .last
            .as_ref()
            .is_some_and(|(side, range, revision)| {
                *side == self.active_left && *revision == self.revision && *range == selection
            });
        if !valid {
            self.find(ctx, true);
            return;
        }
        let text = if self.active_left {
            &self.left.text
        } else {
            &self.right.text
        };
        match self
            .search
            .options
            .replacement(&text[selection.clone()], &self.search.replacement)
        {
            Ok(replacement) => {
                self.remember_edit();
                let text = if self.active_left {
                    &mut self.left.text
                } else {
                    &mut self.right.text
                };
                text.replace_range(selection.clone(), &replacement);
                self.compare();
                self.search.last = None;
                let end = selection.start + replacement.len();
                self.select_bytes(self.active_left, end..end);
            }
            Err(error) => self.search.message = Some(error),
        }
    }

    fn replace_all(&mut self) {
        if !self.editing {
            return;
        }
        let mut next = [None, None];
        let mut count = 0;
        for (side, result) in next.iter_mut().enumerate() {
            if !self.search.sides[side] {
                continue;
            }
            let text = if side == 0 {
                &self.left.text
            } else {
                &self.right.text
            };
            match self
                .search
                .options
                .replace_all(text, &self.search.replacement)
            {
                Ok((text, n)) => {
                    *result = Some(text);
                    count += n;
                }
                Err(error) => {
                    self.search.message = Some(error);
                    return;
                }
            }
        }
        if count > 0 {
            self.remember_edit();
            if let Some(text) = next[0].take() {
                self.left.text = text;
            }
            if let Some(text) = next[1].take() {
                self.right.text = text;
            }
            self.compare();
            self.search.last = None;
        }
        self.search.message = Some(format!("{count} substituições."));
    }

    fn go_to(&mut self, dialogs: &mut dyn DialogHost) {
        let current = self.caret[usize::from(!self.active_left)];
        if let Some(value) = dialogs.input(
            "Ir para",
            "Linha[:coluna]:",
            &format!("{}:{}", current.0, current.1),
        ) {
            let parts: Vec<_> = value.trim().split(':').collect();
            let line = parts[0].parse::<usize>();
            let column = parts.get(1).unwrap_or(&"1").parse::<usize>();
            let text = if self.active_left {
                &self.left.text
            } else {
                &self.right.text
            };
            if let (Ok(line), Ok(column)) = (line, column) {
                if line > 0 && column > 0 && parts.len() <= 2 {
                    if let Some((start, content)) = text
                        .split_inclusive('\n')
                        .scan(0, |offset, content| {
                            let start = *offset;
                            *offset += content.len();
                            Some((start, content))
                        })
                        .nth(line - 1)
                    {
                        let content = content.trim_end_matches('\n');
                        let byte = content
                            .char_indices()
                            .nth(column - 1)
                            .map(|(i, _)| i)
                            .unwrap_or(content.len());
                        self.select_bytes(self.active_left, start + byte..start + byte);
                        return;
                    }
                }
            }
            dialogs.error("Informe uma linha existente e uma coluna maior que zero.");
        }
    }

    fn options_dialog(&mut self, dialogs: &mut dyn DialogHost) {
        let mut fields = [
            DialogField::Check("Permitir edição de texto".into(), self.editing),
            DialogField::Check("Mostrar números de linhas".into(), self.line_numbers),
            DialogField::Check("Mostrar contexto das diferenças".into(), self.context_lines),
            DialogField::Check("Mostrar miniatura".into(), self.show_thumbnail),
            DialogField::Check("Mostrar detalhes de texto".into(), self.show_details),
            DialogField::Check("Mostrar espaços e tabs".into(), self.show_whitespace),
            crate::i18n::language_field(),
        ];
        if dialogs.fields("Opções de texto", &mut fields) {
            let mut values = fields.iter().map(|field| {
                if let DialogField::Check(_, v) = field {
                    *v
                } else {
                    false
                }
            });
            self.editing = values.next().unwrap();
            self.line_numbers = values.next().unwrap();
            self.context_lines = values.next().unwrap();
            self.show_thumbnail = values.next().unwrap();
            self.show_details = values.next().unwrap();
            self.show_whitespace = values.next().unwrap();
            crate::i18n::apply_language_field(fields.last().unwrap(), dialogs);
        }
    }
    fn report(&self, dialogs: &mut dyn DialogHost) {
        let rows = self
            .diff_result
            .lines
            .iter()
            .map(|line| crate::report::Row {
                key: format!(
                    "{} / {}",
                    line.left_line_no.map(|n| n.to_string()).unwrap_or_default(),
                    line.right_line_no
                        .map(|n| n.to_string())
                        .unwrap_or_default()
                ),
                left: line.left_text.clone(),
                right: line.right_text.clone(),
                different: line.status != DiffStatus::Same,
            })
            .collect::<Vec<_>>();
        let inputs = self
            .left
            .path
            .iter()
            .chain(self.right.path.iter())
            .cloned()
            .collect::<Vec<_>>();
        crate::report::export(
            "Comparar Textos",
            &self.left_path,
            &self.right_path,
            &rows,
            &inputs,
            dialogs,
        );
    }

    fn format_dialog(&mut self, dialogs: &mut dyn DialogHost) {
        use crate::document::Encoding;
        let document = if self.active_left {
            &mut self.left
        } else {
            &mut self.right
        };
        let encodings = Encoding::ALL;
        let index = encodings
            .iter()
            .position(|&e| e == document.encoding)
            .unwrap();
        let mut fields = [
            DialogField::Choice(
                "Codificação:".into(),
                index,
                encodings
                    .map(Encoding::label)
                    .into_iter()
                    .map(str::to_owned)
                    .collect(),
            ),
            DialogField::Choice(
                "Terminador de linha:".into(),
                LineEnding::ALL
                    .iter()
                    .position(|&ending| ending == document.line_ending)
                    .unwrap(),
                LineEnding::ALL
                    .map(|ending| ending.label().to_owned())
                    .into(),
            ),
        ];
        if dialogs.fields("Formato do arquivo", &mut fields) {
            if let DialogField::Choice(_, index, _) = fields[0] {
                document.encoding = encodings[index];
            }
            if let DialogField::Choice(_, index, _) = fields[1] {
                document.line_ending = LineEnding::ALL[index];
            }
        }
    }

    fn active_rules(&self) -> crate::diff::DiffRules {
        if self.ignore_minor {
            crate::diff::DiffRules {
                ignore_case: true,
                ignore_whitespace: true,
            }
        } else if self.rules_enabled {
            self.rules
        } else {
            Default::default()
        }
    }

    fn comparison_info(&self, dialogs: &mut dyn DialogHost) {
        dialogs.info(&format!(
            "Esquerda: {}\nDireita: {}\n\n{} seções diferentes\n{} linhas iguais",
            self.left_path,
            self.right_path,
            self.diff_result.sections.len(),
            self.diff_result.same_count
        ));
    }

    fn rules_dialog(&mut self, dialogs: &mut dyn DialogHost) {
        let mut fields = [
            DialogField::Check(
                "Ignorar maiúsculas/minúsculas".into(),
                self.rules.ignore_case,
            ),
            DialogField::Check(
                "Ignorar espaços em branco".into(),
                self.rules.ignore_whitespace,
            ),
        ];
        if dialogs.fields("Regras de comparação", &mut fields) {
            if let DialogField::Check(_, value) = fields[0] {
                self.rules.ignore_case = value;
            }
            if let DialogField::Check(_, value) = fields[1] {
                self.rules.ignore_whitespace = value;
            }
            self.rules_enabled = true;
            self.ignore_minor = false;
            self.recompare();
        }
    }

    fn search_panel(&mut self, ctx: &Context, ui: &mut Ui, icons: &mut IconManager) {
        if !self.search.shown {
            return;
        }
        TopBottomPanel::top("text_find")
            .frame(
                Frame::none()
                    .fill(Color32::from_rgb(214, 224, 237))
                    .inner_margin(Margin::symmetric(8.0, 4.0)),
            )
            .show_inside(ui, |ui| {
                let mut forward = false;
                let mut backward = false;
                let mut replace = false;
                let mut all = false;
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    ui.spacing_mut().interact_size.y = 23.0;
                    if let Some(texture) = icons.get_icon(ctx, "imgSearch") {
                        ui.image((texture.id(), vec2(16.0, 16.0)));
                    }
                    let width = (ui.available_width() - 300.0).max(60.0);
                    let history = super::history_combo::show(
                        ui,
                        "find_query",
                        &mut self.search.options.query,
                        &self.recent_searches,
                        width,
                        "Localizar",
                    );
                    if let Some(query) = history.selected {
                        self.search.options.query = query;
                        forward = true;
                    }
                    forward |= history.submitted;
                    let response = history.response;
                    if self.search.focus_query {
                        response.request_focus();
                        if let Some(mut state) = TextEdit::load_state(ctx, response.id) {
                            state
                                .cursor
                                .set_char_range(Some(egui::text::CCursorRange::two(
                                    egui::text::CCursor::new(0),
                                    egui::text::CCursor::new(
                                        self.search.options.query.chars().count(),
                                    ),
                                )));
                            state.store(ctx, response.id);
                        }
                        self.search.focus_query = false;
                    }
                    forward |=
                        response.has_focus() && ui.input(|input| input.key_pressed(Key::Enter));
                    let regex_menu = egui::menu::menu_custom_button(
                        ui,
                        Button::new("").min_size(vec2(23.0, 23.0)),
                        |ui| {
                            for (label, expression) in [
                                ("Tabulação (\\t)", "\\t"),
                                ("Qualquer caractere (.)", "."),
                                ("Caractere alfanumérico (\\w)", "\\w"),
                                ("Caractere não alfanumérico (\\W)", "\\W"),
                                ("Dígito (\\d)", "\\d"),
                                ("Caractere não numérico (\\D)", "\\D"),
                                ("Espaço em branco (\\s)", "\\s"),
                                ("Caractere no conjunto ([])", "[]"),
                                ("Caractere fora do conjunto ([^])", "[^]"),
                                ("Início da linha (^)", "^"),
                                ("Fim da linha ($)", "$"),
                                ("Grupo de captura (())", "()"),
                                ("Ou (|)", "|"),
                                ("Zero ou mais (*)", "*"),
                                ("Um ou mais (+)", "+"),
                                ("Zero ou um (?)", "?"),
                            ] {
                                if ui.button(crate::i18n::tr(label)).clicked() {
                                    insert_search_expression(
                                        ctx,
                                        response.id,
                                        &mut self.search.options.query,
                                        expression,
                                    );
                                    self.search.options.regular_expression = true;
                                    self.search.focus_query = true;
                                    ui.close_menu();
                                }
                            }
                        },
                    )
                    .response
                    .on_hover_text(crate::i18n::tr("Inserir elemento de expressão regular"));
                    paint_search_chevron(ui, regex_menu.rect, false);
                    for (value, icon, hint) in [
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
                        if let Some(texture) = icons.get_icon(ctx, icon) {
                            if ui
                                .add(
                                    Button::image((texture.id(), vec2(16.0, 16.0)))
                                        .selected(*value)
                                        .min_size(vec2(23.0, 23.0)),
                                )
                                .on_hover_text(crate::i18n::tr(hint))
                                .clicked()
                            {
                                *value = !*value;
                            }
                        }
                    }
                    backward |= ui
                        .add(Button::new(crate::i18n::tr("Anterior")).min_size(vec2(61.0, 23.0)))
                        .clicked();
                    forward |= ui
                        .add(Button::new(crate::i18n::tr("Próximo")).min_size(vec2(61.0, 23.0)))
                        .clicked();
                    let advanced = ui
                        .add(Button::new("").min_size(vec2(23.0, 23.0)))
                        .on_hover_text(crate::i18n::tr("Mais opções de pesquisa"));
                    paint_search_chevron(ui, advanced.rect, self.search.advanced);
                    if advanced.clicked() {
                        self.search.advanced = !self.search.advanced;
                    }
                });
                if self.search.replace_mode {
                    ui.horizontal(|ui| {
                        ui.label(crate::i18n::tr("Substituir:"));
                        ui.add(
                            TextEdit::singleline(&mut self.search.replacement).desired_width(280.0),
                        );
                        replace = ui
                            .add_enabled(self.editing, Button::new(crate::i18n::tr("Substituir")))
                            .clicked();
                        all = ui
                            .add_enabled(
                                self.editing,
                                Button::new(crate::i18n::tr("Substituir tudo")),
                            )
                            .clicked();
                    });
                }
                if self.search.advanced || self.search.replace_mode {
                    ui.horizontal_wrapped(|ui| {
                        ui.checkbox(&mut self.search.sides[0], crate::i18n::tr("Esquerda"));
                        ui.checkbox(&mut self.search.sides[1], crate::i18n::tr("Direita"));
                        ui.checkbox(&mut self.search.replace_mode, crate::i18n::tr("Substituir"));
                        if ui.button(crate::i18n::tr("Fechar pesquisa")).clicked() {
                            self.search.shown = false;
                        }
                    });
                }
                if let Some(message) = &self.search.message {
                    ui.label(crate::i18n::tr(message));
                }
                if forward || backward {
                    self.find(ctx, forward);
                }
                if replace {
                    self.replace_found(ctx);
                }
                if all {
                    self.replace_all();
                }
            });
    }

    pub fn set_editable(&mut self, editable: bool) {
        self.editing = editable;
    }

    pub fn show_search(&mut self, query: String) {
        self.search.options.query = query;
        self.search.shown = true;
        self.search.replace_mode = false;
        self.search.focus_query = true;
    }

    pub fn set_recent_history(&mut self, history: &crate::history::History) {
        self.recent_paths = history.text_paths();
        for document in [&self.left, &self.right] {
            if let Some(path) = &document.path {
                let path = path.to_string_lossy().into_owned();
                if !self.recent_paths.contains(&path) {
                    self.recent_paths.push(path);
                }
            }
        }
        self.recent_searches = history.searches.clone();
    }

    pub fn take_recent_search(&mut self) -> Option<String> {
        self.pending_search_history.take()
    }

    pub fn ui(
        &mut self,
        ctx: &Context,
        ui: &mut Ui,
        icons: &mut IconManager,
        dialogs: &mut dyn DialogHost,
    ) -> TextCompareAction {
        let mut action = TextCompareAction::None;
        let mut reopen = None;
        // Normalize pasted physical terminators before the native edit widget
        // handles them; cursor offsets and comparison stay in the same LF buffer.
        ctx.input_mut(|input| {
            for event in &mut input.events {
                if let Event::Paste(text) = event {
                    if text.contains('\r') {
                        *text = text.replace("\r\n", "\n").replace('\r', "\n");
                    }
                }
            }
        });
        let mut clipboard_command = None;
        let mut select_all = false;
        if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::F)) {
            self.search.shown = true;
            self.search.replace_mode = false;
            self.search.focus_query = true;
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::H)) {
            self.search.shown = true;
            self.search.replace_mode = true;
            self.search.focus_query = true;
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::G)) {
            self.go_to(dialogs);
        }
        let previous_match = ctx.input_mut(|i| i.consume_key(Modifiers::SHIFT, Key::F3));
        if previous_match || ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::F3)) {
            if self.search.options.query.is_empty() {
                self.search.shown = true;
                self.search.focus_query = true;
            } else {
                self.find(ctx, !previous_match);
            }
        }
        if self.search.shown && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
            self.search.shown = false;
        }
        if let Some(left) = [true, false].into_iter().find(|&left| {
            self.editor_cache[usize::from(!left)]
                .widget_id
                .is_some_and(|id| ctx.memory(|memory| memory.has_focus(id)))
        }) {
            if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::A)) {
                self.select_all(ctx, left);
            }
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL | Modifiers::SHIFT, Key::R)) {
            self.copy_line(true);
        } else if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::R)) {
            self.copy_selected(ctx, true);
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL | Modifiers::SHIFT, Key::L)) {
            self.copy_line(false);
        } else if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::L)) {
            self.copy_selected(ctx, false);
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::N)) {
            self.next_diff(true);
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::P)) {
            self.next_diff(false);
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::D)) {
            self.select_section();
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::I)) {
            self.comparison_info(dialogs);
        }
        for (key, operation) in [(Key::I, 3), (Key::U, 4)] {
            if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL | Modifiers::SHIFT, key)) {
                self.edit_lines(ctx, operation);
            }
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL | Modifiers::SHIFT, Key::Enter)) {
            self.edit_lines(ctx, 1);
        } else if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::Enter)) {
            self.edit_lines(ctx, 2);
        }
        let editor_focus = ctx.memory(|memory| {
            memory.focused().is_none()
                || self
                    .editor_cache
                    .iter()
                    .any(|cache| cache.widget_id.is_some_and(|id| memory.has_focus(id)))
        });
        if editor_focus {
            if ctx.input_mut(|i| {
                i.consume_key(Modifiers::CTRL | Modifiers::SHIFT, Key::Z)
                    || i.consume_key(Modifiers::CTRL, Key::Y)
            }) {
                self.redo_edit();
            } else if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::Z)) {
                self.undo_merge();
            }
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::F2)) {
            self.editing = !self.editing;
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::F5)) {
            self.recompare();
        } else if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::F5)) {
            self.reload(dialogs);
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::F12)) {
            self.save_side(self.active_left, true, dialogs);
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::O)) {
            if let Some(path) = dialogs.file(
                false,
                self.active_left,
                if self.active_left {
                    self.left.path.as_deref()
                } else {
                    self.right.path.as_deref()
                },
            ) {
                self.open_side(self.active_left, path, dialogs);
            }
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::S))
            && if self.active_left {
                self.left.dirty()
            } else {
                self.right.dirty()
            }
        {
            self.save_side(self.active_left, false, dialogs);
        }

        TopBottomPanel::top("text_menus")
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
                        if super::chrome::menu_item(ui, icons, "Configurações da Sessão...", true)
                            .clicked()
                        {
                            self.rules_dialog(dialogs);
                            ui.close_menu();
                        }
                        super::chrome::menu_item(ui, icons, "Bloqueado", false)
                            .on_disabled_hover_text(crate::i18n::tr(
                                "Esta função ainda não está disponível",
                            ));

                        if super::chrome::menu_item(ui, icons, "Limpar sessão", true).clicked() {
                            if self.confirm_saved(dialogs) {
                                self.left = Document::default();
                                self.right = Document::default();
                                self.left_path.clear();
                                self.right_path.clear();
                                self.undo.clear();
                                self.redo.clear();
                                self.revealed.clear();
                                self.search.last = None;
                                self.compare();
                            }
                            ui.close_menu();
                        }
                        super::session_menu::close(ui, icons);
                        ui.separator();
                        if super::chrome::menu_item(ui, icons, "Inverter lados", true).clicked() {
                            self.swap_sides();
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
                        ui.separator();
                        if super::chrome::menu_item(ui, icons, "Relatório de texto...", true)
                            .clicked()
                        {
                            self.report(dialogs);
                            ui.close_menu();
                        }
                        if super::chrome::menu_item(
                            ui,
                            icons,
                            "Informações da comparação  Ctrl+I",
                            true,
                        )
                        .clicked()
                        {
                            self.comparison_info(dialogs);
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
                            let current = if self.active_left {
                                self.left.path.as_deref()
                            } else {
                                self.right.path.as_deref()
                            };
                            if let Some(path) = dialogs.file(false, self.active_left, current) {
                                self.open_side(self.active_left, path, dialogs);
                            }
                            ui.close_menu();
                        }
                        let dirty = if self.active_left {
                            self.left.dirty()
                        } else {
                            self.right.dirty()
                        };
                        if super::chrome::menu_item(ui, icons, "Salvar arquivo  Ctrl+S", dirty)
                            .clicked()
                        {
                            self.save_side(self.active_left, false, dialogs);
                            ui.close_menu();
                        }
                        if super::chrome::menu_item(ui, icons, "Salvar arquivo como...  F12", true)
                            .clicked()
                        {
                            self.save_side(self.active_left, true, dialogs);
                            ui.close_menu();
                        }
                        ui.separator();
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
                        super::chrome::menu_button(ui, "Linhas", |ui| {
                            if super::chrome::menu_item(
                                ui,
                                icons,
                                "Copiar linha para a Direita  Ctrl+Shift+R",
                                true,
                            )
                            .clicked()
                            {
                                self.copy_line(true);
                                ui.close_menu();
                            }
                            if super::chrome::menu_item(
                                ui,
                                icons,
                                "Copiar linha para a Esquerda  Ctrl+Shift+L",
                                true,
                            )
                            .clicked()
                            {
                                self.copy_line(false);
                                ui.close_menu();
                            }
                            if super::chrome::menu_item(ui, icons, "Selecionar seção  Ctrl+D", true)
                                .clicked()
                            {
                                self.select_section();
                                ui.close_menu();
                            }
                            for (label, operation) in [
                                ("Excluir linhas", 0),
                                ("Inserir linha antes  Ctrl+Shift+Enter", 1),
                                ("Inserir linha depois  Ctrl+Enter", 2),
                                ("Aumentar recuo  Ctrl+Shift+I", 3),
                                ("Diminuir recuo  Ctrl+Shift+U", 4),
                            ] {
                                if super::chrome::menu_item(ui, icons, label, self.editing)
                                    .clicked()
                                {
                                    self.edit_lines(ctx, operation);
                                    ui.close_menu();
                                }
                            }
                        });
                        super::chrome::menu_button(ui, "Converter arquivo", |ui| {
                            if super::chrome::menu_item(
                                ui,
                                icons,
                                "Remover espaços ao fim das linhas",
                                self.editing,
                            )
                            .clicked()
                            {
                                self.transform_active(|text| {
                                    text.split_inclusive('\n')
                                        .map(|line| {
                                            format!(
                                                "{}{}",
                                                line.trim_end_matches([' ', '\t', '\n']),
                                                if line.ends_with('\n') { "\n" } else { "" }
                                            )
                                        })
                                        .collect()
                                });
                                ui.close_menu();
                            }
                            if super::chrome::menu_item(
                                ui,
                                icons,
                                "Tabs para espaços (4)",
                                self.editing,
                            )
                            .clicked()
                            {
                                self.transform_active(|text| text.replace('\t', "    "));
                                ui.close_menu();
                            }
                            if super::chrome::menu_item(
                                ui,
                                icons,
                                "Espaços iniciais para tabs (4)",
                                self.editing,
                            )
                            .clicked()
                            {
                                self.transform_active(|text| {
                                    text.split_inclusive('\n')
                                        .map(|line| {
                                            let n = line.bytes().take_while(|&b| b == b' ').count();
                                            format!(
                                                "{}{}{}",
                                                "\t".repeat(n / 4),
                                                " ".repeat(n % 4),
                                                &line[n..]
                                            )
                                        })
                                        .collect()
                                });
                                ui.close_menu();
                            }
                        });
                        let selected = self.has_selection(ctx, self.active_left);
                        for (label, enabled, command) in [
                            (
                                "Recortar  Ctrl+X",
                                self.editing && selected,
                                ViewportCommand::RequestCut,
                            ),
                            ("Copiar  Ctrl+C", selected, ViewportCommand::RequestCopy),
                            ("Colar  Ctrl+V", self.editing, ViewportCommand::RequestPaste),
                        ] {
                            if super::chrome::menu_item(ui, icons, label, enabled).clicked() {
                                clipboard_command = Some(command);
                                ui.close_menu();
                            }
                        }
                        if super::chrome::menu_item(ui, icons, "Selecionar tudo  Ctrl+A", true)
                            .clicked()
                        {
                            select_all = true;
                            ui.close_menu();
                        }
                        ui.separator();
                        if super::chrome::menu_item(
                            ui,
                            icons,
                            "Copiar diferença para a Direita  Ctrl+R",
                            true,
                        )
                        .clicked()
                        {
                            self.copy_selected(ctx, true);
                            ui.close_menu();
                        }
                        if super::chrome::menu_item(
                            ui,
                            icons,
                            "Copiar diferença para a Esquerda  Ctrl+L",
                            true,
                        )
                        .clicked()
                        {
                            self.copy_selected(ctx, false);
                            ui.close_menu();
                        }
                        if super::chrome::menu_item(
                            ui,
                            icons,
                            "Desfazer  Ctrl+Z",
                            !self.undo.is_empty(),
                        )
                        .clicked()
                        {
                            self.undo_merge();
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
                            self.redo_edit();
                            ui.close_menu();
                        }
                    });
                    super::chrome::menu_button(ui, "Pesquisar", |ui| {
                        if super::chrome::menu_item(ui, icons, "Localizar...  Ctrl+F", true)
                            .clicked()
                        {
                            self.search.shown = true;
                            self.search.replace_mode = false;
                            self.search.focus_query = true;
                            ui.close_menu();
                        }
                        if super::chrome::menu_item(ui, icons, "Localizar próximo  F3", true)
                            .clicked()
                        {
                            self.find(ctx, true);
                            ui.close_menu();
                        }
                        if super::chrome::menu_item(ui, icons, "Localizar anterior  Shift+F3", true)
                            .clicked()
                        {
                            self.find(ctx, false);
                            ui.close_menu();
                        }
                        if super::chrome::menu_item(ui, icons, "Substituir...  Ctrl+H", true)
                            .clicked()
                        {
                            self.search.shown = true;
                            self.search.replace_mode = true;
                            self.search.focus_query = true;
                            ui.close_menu();
                        }
                        if super::chrome::menu_item(ui, icons, "Ir para...  Ctrl+G", true).clicked()
                        {
                            self.go_to(dialogs);
                            ui.close_menu();
                        }
                        ui.separator();
                        if super::chrome::menu_item(ui, icons, "Próxima diferença  Ctrl+N", true)
                            .clicked()
                        {
                            self.next_diff(true);
                            ui.close_menu();
                        }
                        if super::chrome::menu_item(ui, icons, "Diferença anterior  Ctrl+P", true)
                            .clicked()
                        {
                            self.next_diff(false);
                            ui.close_menu();
                        }
                    });
                    super::chrome::menu_button(ui, "Visualizar", |ui| {
                        ui.radio_value(
                            &mut self.filter_mode,
                            FilterMode::All,
                            crate::i18n::tr("Todos"),
                        );
                        ui.radio_value(
                            &mut self.filter_mode,
                            FilterMode::Differences,
                            crate::i18n::tr("Diferenças"),
                        );
                        ui.radio_value(
                            &mut self.filter_mode,
                            FilterMode::Same,
                            crate::i18n::tr("Iguais"),
                        );
                        ui.checkbox(&mut self.editing, crate::i18n::tr("Editar textos"));
                        ui.checkbox(
                            &mut self.line_numbers,
                            crate::i18n::tr("Números das linhas"),
                        );
                        ui.checkbox(
                            &mut self.context_lines,
                            crate::i18n::tr("Contexto das diferenças"),
                        );
                        ui.checkbox(&mut self.show_details, crate::i18n::tr("Detalhes de texto"));
                        ui.checkbox(&mut self.show_thumbnail, crate::i18n::tr("Miniatura"));
                        ui.checkbox(
                            &mut self.show_whitespace,
                            crate::i18n::tr("Espaços em branco visíveis"),
                        );
                    });
                    super::chrome::menu_button(ui, "Ferramentas", |ui| {
                        if super::chrome::menu_item(ui, icons, "Opções...", true).clicked() {
                            self.options_dialog(dialogs);
                            ui.close_menu();
                        }
                        if super::chrome::menu_item(ui, icons, "Formato do arquivo...", true)
                            .clicked()
                        {
                            self.format_dialog(dialogs);
                            ui.close_menu();
                        }
                        if super::chrome::menu_item(ui, icons, "Regras de comparação...", true)
                            .clicked()
                        {
                            self.rules_dialog(dialogs);
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

        let filter_mode = self.filter_mode;
        let context_lines = self.context_lines;
        let ignore_minor = self.ignore_minor;
        let editing = self.editing;
        let can_copy = self.has_selection(ctx, self.active_left)
            || self
                .diff_result
                .lines
                .get(self.selected_line_idx)
                .is_some_and(|line| line.status != DiffStatus::Same);
        let has_next = self
            .diff_result
            .sections
            .iter()
            .any(|section| section.start > self.selected_line_idx);
        let has_prev = self
            .diff_result
            .sections
            .iter()
            .any(|section| section.start < self.selected_line_idx);
        super::session_tabs::show(ui, icons);
        let tool_state = move |name: &str| -> ToolState {
            match name {
                "FavCompareAll" => ToolState::checked(filter_mode == FilterMode::All),
                "FavDiff" => ToolState::checked(filter_mode == FilterMode::Differences),
                "FavSame" => ToolState::checked(filter_mode == FilterMode::Same),
                "ShowContext" => ToolState::checked(context_lines),
                "InsigIgnored" => ToolState::checked(ignore_minor),
                "FullEdit" => ToolState::checked(editing),
                "SessionsCombo" => ToolState::enabled(true),
                "CopySide" => ToolState::enabled(can_copy),
                "DiffSectionNext" => ToolState::enabled(has_next),
                "DiffSectionPrev" => ToolState::enabled(has_prev),
                "SessionHome" | "Rules" | "FileFormat" | "Swap" | "Reload" => {
                    ToolState::enabled(true)
                }
                _ => ToolState::DISABLED,
            }
        };
        let items = super::toolbar_spec::TEXT_COMPARE;
        let toolbar_height = super::toolbar::height(ui, items, ui.available_width(), &tool_state);
        let mut tool_event = None;
        TopBottomPanel::top("text_toolbar")
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
                    &mut |_, _, _| {},
                );
            });
        match tool_event {
            Some(ToolEvent::Click("SessionHome")) => action = TextCompareAction::BackToHome,
            Some(ToolEvent::Click("FavCompareAll")) => self.filter_mode = FilterMode::All,
            Some(ToolEvent::Click("FavDiff")) => self.filter_mode = FilterMode::Differences,
            Some(ToolEvent::Click("FavSame")) => self.filter_mode = FilterMode::Same,
            Some(ToolEvent::Click("ShowContext")) => {
                self.context_lines = !self.context_lines;
                self.filter_mode = FilterMode::Differences;
            }
            Some(ToolEvent::Click("InsigIgnored")) => {
                self.ignore_minor = !self.ignore_minor;
                self.recompare();
            }
            Some(ToolEvent::Click("Rules")) => self.rules_dialog(dialogs),
            Some(ToolEvent::Click("FileFormat") | ToolEvent::Arrow("FileFormat")) => {
                self.format_dialog(dialogs)
            }
            Some(ToolEvent::Click("CopySide")) => self.copy_selected(ctx, self.active_left),
            Some(ToolEvent::Click("FullEdit")) => self.editing = !self.editing,
            Some(ToolEvent::Click("DiffSectionNext")) => self.next_diff(true),
            Some(ToolEvent::Click("DiffSectionPrev")) => self.next_diff(false),
            Some(ToolEvent::Click("Swap")) => self.swap_sides(),
            Some(ToolEvent::Click("Reload")) => self.reload(dialogs),
            _ => {}
        }

        TopBottomPanel::bottom("text_status")
            .exact_height(super::text_chrome::WINDOW_STATUS_HEIGHT)
            .show_separator_line(false)
            .frame(Frame::none().fill(BcTheme::WINDOW_BG))
            .show_inside(ui, |ui| self.window_status(ui, icons));

        self.search_panel(ctx, ui, icons);
        TopBottomPanel::top("text_paths")
            .show_separator_line(false)
            .frame(Frame::none().inner_margin(Margin {
                left: 45.0,
                right: 2.0,
                top: 2.0,
                bottom: 3.0,
            }))
            .show_inside(ui, |ui| {
                ui.columns(2, |columns| {
                    for (index, column) in columns.iter_mut().enumerate() {
                        let left = index == 0;
                        column.horizontal(|ui| {
                            let path = if left {
                                &mut self.left_path
                            } else {
                                &mut self.right_path
                            };
                            let title = if left {
                                "Arquivo esquerdo"
                            } else {
                                "Arquivo direito"
                            };
                            let width = (ui.available_width() - 90.0).max(30.0);
                            let history = super::history_combo::show(
                                ui,
                                ("text_path", left),
                                path,
                                &self.recent_paths,
                                width,
                                title,
                            );
                            let response = history.response;
                            if response.has_focus() {
                                self.active_left = left;
                            }
                            let typed = path.clone();
                            if let Some(path) = history.selected {
                                self.open_side(left, PathBuf::from(path), dialogs);
                            } else if history.submitted && !typed.is_empty() {
                                self.open_side(left, PathBuf::from(typed), dialogs);
                            }
                            let browse = icons.get_icon(ctx, "imgBrowse");
                            if browse.is_some_and(|texture| {
                                ui.add(
                                    ImageButton::new((texture.id(), vec2(16.0, 16.0))).frame(false),
                                )
                                .on_hover_text(crate::i18n::tr(title))
                                .clicked()
                            }) {
                                if let Some(path) = dialogs.file(
                                    false,
                                    left,
                                    if left {
                                        self.left.path.as_deref()
                                    } else {
                                        self.right.path.as_deref()
                                    },
                                ) {
                                    self.open_side(left, path, dialogs);
                                }
                            }
                            let dirty = if left {
                                self.left.dirty()
                            } else {
                                self.right.dirty()
                            };
                            if let Some(texture) = icons.get_icon_with_state(ctx, "imgSave", dirty)
                            {
                                if ui
                                    .add_enabled(
                                        dirty,
                                        ImageButton::new((texture.id(), vec2(16.0, 16.0)))
                                            .frame(false),
                                    )
                                    .on_hover_text(crate::i18n::tr("Salvar arquivo"))
                                    .clicked()
                                {
                                    self.save_side(left, false, dialogs);
                                }
                            }
                        });
                        let document = if left {
                            &mut self.left
                        } else {
                            &mut self.right
                        };
                        column.horizontal(|ui| {
                            if let Some(metadata) = document.metadata_label() {
                                ui.add(
                                    Label::new(RichText::new(metadata).size(12.0))
                                        .wrap_mode(TextWrapMode::Extend),
                                );
                            }
                            ui.label(crate::i18n::tr("Todo o Resto"));
                            ComboBox::from_id_salt(("encoding", left))
                                .selected_text(document.encoding_label())
                                .width(64.0)
                                .show_ui(ui, |ui| {
                                    for encoding in Encoding::ALL {
                                        ui.selectable_value(
                                            &mut document.encoding,
                                            encoding,
                                            crate::i18n::tr(encoding.label()),
                                        );
                                    }
                                    if document.path.is_some() {
                                        ui.separator();
                                        ui.menu_button(
                                            crate::i18n::tr("Reabrir com codificação"),
                                            |ui| {
                                                if ui
                                                    .button(crate::i18n::tr(
                                                        "Detectar automaticamente",
                                                    ))
                                                    .clicked()
                                                {
                                                    reopen = Some((left, None));
                                                    ui.close_menu();
                                                }
                                                for encoding in Encoding::ALL {
                                                    if ui
                                                        .button(crate::i18n::tr(encoding.label()))
                                                        .clicked()
                                                    {
                                                        reopen = Some((left, Some(encoding)));
                                                        ui.close_menu();
                                                    }
                                                }
                                            },
                                        );
                                    }
                                });
                            ComboBox::from_id_salt(("line_ending", left))
                                .selected_text(crate::i18n::tr(document.line_ending.label()))
                                .width(40.0)
                                .show_ui(ui, |ui| {
                                    for ending in LineEnding::ALL {
                                        ui.selectable_value(
                                            &mut document.line_ending,
                                            ending,
                                            crate::i18n::tr(ending.label()),
                                        );
                                    }
                                });
                        });
                    }
                });
                if let Some(message) = self.error.clone() {
                    ui.horizontal_wrapped(|ui| {
                        ui.colored_label(
                            Color32::from_rgb(175, 25, 25),
                            crate::i18n::tr(&(message)),
                        );
                        if ui.small_button(crate::i18n::tr("Fechar")).clicked() {
                            self.error = None;
                        }
                    });
                }
            });

        if let Some((left, encoding)) = reopen {
            self.reopen_encoded(left, encoding, dialogs);
        }
        if self.show_thumbnail {
            SidePanel::left("text_thumbnail")
                .exact_width(44.0)
                .resizable(false)
                .show_separator_line(false)
                .frame(Frame::none().fill(BcTheme::WINDOW_BG))
                .show_inside(ui, |ui| self.thumbnail(ui));
        }
        if self.show_details {
            TopBottomPanel::bottom("text_details")
                .exact_height(super::text_chrome::DETAILS_HEIGHT)
                .show_separator_line(false)
                .frame(Frame::none().fill(BcTheme::WINDOW_BG))
                .show_inside(ui, |ui| self.character_details(ui));
        }
        CentralPanel::default()
            .frame(Frame::none().fill(BcTheme::WINDOW_BG))
            .show_inside(ui, |ui| {
                if select_all {
                    self.select_all(ctx, self.active_left);
                }
                if let Some(command) = clipboard_command {
                    if let Some(id) = self.editor_cache[usize::from(!self.active_left)].widget_id {
                        ui.memory_mut(|memory| memory.request_focus(id));
                    }
                    ctx.send_viewport_cmd(command);
                }
                self.diff_view(ui, icons);
            });

        action
    }

    fn window_status(&self, ui: &mut Ui, icons: &mut IconManager) {
        let rect = ui.max_rect();
        let painter = ui.painter();
        painter.rect_filled(
            Rect::from_min_size(rect.min, vec2(rect.width(), 1.0)),
            0.0,
            Color32::from_gray(215),
        );
        let mut x = rect.left();
        let important = self.diff_result.lines.iter().any(|line| {
            matches!(
                line.status,
                DiffStatus::Different | DiffStatus::LeftOrphan | DiffStatus::RightOrphan
            )
        });
        let result = if self.left.same_saved_bytes(&self.right) {
            crate::i18n::tr("Mesmo binário")
        } else if important {
            crate::i18n::tr(&format!(
                "{} seções diferentes",
                self.diff_result.sections.len()
            ))
        } else if self
            .diff_result
            .lines
            .iter()
            .any(|line| line.status == DiffStatus::Unimportant)
        {
            crate::i18n::tr("Diferenças não-importantes")
        } else if self.left.text != self.right.text
            && self.active_rules() != crate::diff::DiffRules::default()
        {
            crate::i18n::tr("Diferenças ignoradas")
        } else {
            crate::i18n::tr("Iguais")
        };
        let mut first = Rect::from_min_max(
            pos2(x, rect.top()),
            pos2((x + 225.0).min(rect.right()), rect.bottom()),
        );
        if let Some(icon) = icons.get_icon(
            ui.ctx(),
            if important {
                "status_diff"
            } else if self.left.same_saved_bytes(&self.right) {
                "imgContentSameBinary"
            } else if !self.diff_result.sections.is_empty() {
                "imgContentSimilar"
            } else {
                "status_same"
            },
        ) {
            let size = icon.size_vec2() / ui.ctx().pixels_per_point();
            let at = painter.round_pos_to_pixels(pos2(x, rect.center().y - size.y / 2.0));
            painter.image(
                icon.id(),
                Rect::from_min_size(at, size),
                Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)),
                Color32::WHITE,
            );
            first.min.x += size.x;
        }
        super::text_chrome::label(ui, first, &result);
        x += 225.0;
        let row_state = self
            .diff_result
            .lines
            .get(self.selected_line_idx)
            .map(|line| match line.status {
                DiffStatus::Same => "Iguais",
                DiffStatus::Different => "Diferença Importante",
                DiffStatus::Unimportant => "Diferença Não-importante",
                DiffStatus::LeftOrphan => "Órfão Esquerdo Importante",
                DiffStatus::RightOrphan => "Órfão Direito Importante",
            })
            .unwrap_or("");
        for (width, value) in [
            (300.0, crate::i18n::tr(row_state)),
            (81.0, crate::i18n::tr("Inserir")),
            (
                rect.width(),
                format!(
                    "{} {}",
                    crate::i18n::tr("Tempo de carregamento:"),
                    load_time_label(self.load_time_ms)
                ),
            ),
        ] {
            if x >= rect.right() {
                break;
            }
            super::text_chrome::separator(ui, x, rect.top() + 2.0, rect.height() - 3.0);
            let field = Rect::from_min_max(
                pos2(x + 1.0, rect.top()),
                pos2((x + width).min(rect.right()), rect.bottom()),
            );
            super::text_chrome::label(ui, field, &value);
            x += width;
        }
    }

    fn character_details(&mut self, ui: &mut Ui) {
        let rect = ui.max_rect();
        ui.painter().rect_filled(rect, 0.0, Color32::from_gray(160));
        let Some(line) = self.diff_result.lines.get(self.selected_line_idx) else {
            return;
        };
        let (left, right) = DiffEngine::compute_character_diff(&line.left_text, &line.right_text);
        let galleys: Vec<_> = [true, false]
            .into_iter()
            .zip([left, right])
            .map(|(side, chunks)| {
                let document = if side { &self.left } else { &self.right };
                let number = if side {
                    line.left_line_no
                } else {
                    line.right_line_no
                };
                let eol = number
                    .and_then(|n| document.text.split_inclusive('\n').nth(n - 1))
                    .is_some_and(|s| s.ends_with('\n'));
                ui.fonts(|fonts| fonts.layout_job(detail_job(&chunks, eol)))
            })
            .collect();
        let text_width = galleys.iter().map(|g| g.size().x).fold(0.0_f32, f32::max);
        let viewport = (rect.width() - 17.0).max(1.0);
        self.details_offset_x = self
            .details_offset_x
            .clamp(0.0, (text_width - viewport).max(0.0));
        for (side, galley) in galleys.into_iter().enumerate() {
            let row = Rect::from_min_size(
                rect.min + vec2(1.0, 1.0 + 17.0 * side as f32),
                vec2((rect.width() - 2.0).max(0.0), 16.0),
            );
            ui.painter().rect_filled(row, 0.0, BcTheme::WINDOW_BG);
            let text = Rect::from_min_max(row.min + vec2(15.0, 0.0), row.max);
            if (if side == 0 {
                line.left_line_no
            } else {
                line.right_line_no
            })
            .is_some()
            {
                let bg = match line.status {
                    DiffStatus::Same => Color32::WHITE,
                    DiffStatus::Unimportant => BcTheme::MINOR_BG,
                    _ => BcTheme::DIFF_BG,
                };
                let background = Rect::from_min_size(
                    text.min,
                    vec2(
                        (galley.size().x - self.details_offset_x).clamp(0.0, text.width()),
                        16.0,
                    ),
                );
                ui.painter().rect_filled(background, 0.0, bg);
                let p = ui.painter().with_clip_rect(text.intersect(ui.clip_rect()));
                p.galley(
                    text.min - vec2(self.details_offset_x, 0.0),
                    galley,
                    Color32::BLACK,
                );
            } else {
                draw_hatching(ui.painter(), text);
            }
            super::text_chrome::detail_arrow(
                ui,
                Rect::from_min_size(row.min, vec2(14.0, 16.0)),
                side == 0,
            );
        }
        let bar = Rect::from_min_max(rect.min + vec2(1.0, 35.0), rect.max - vec2(1.0, 1.0));
        super::text_chrome::scroll_bar(
            ui,
            ui.id().with("detail_scroll"),
            bar,
            super::text_chrome::Axis::HorizontalWithArrows,
            text_width,
            viewport,
            &mut self.details_offset_x,
        );
        for y in [rect.top() + 34.0, rect.bottom() - 1.0] {
            ui.painter().rect_filled(
                Rect::from_min_size(pos2(rect.left() + 1.0, y), vec2(rect.width() - 2.0, 1.0)),
                0.0,
                Color32::WHITE,
            );
        }
    }

    fn thumbnail(&mut self, ui: &mut Ui) {
        let (rect, response) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
        let strip = Rect::from_min_max(
            rect.min + vec2(8.0, 2.0),
            pos2(rect.left() + 31.0, rect.bottom() - 2.0),
        );
        let count = self.diff_result.lines.len().max(1);
        ui.painter().rect_filled(strip, 0.0, Color32::WHITE);
        for (index, line) in self.diff_result.lines.iter().enumerate() {
            let color = match line.status {
                DiffStatus::Same => Color32::WHITE,
                DiffStatus::Different => Color32::from_rgb(252, 100, 100),
                DiffStatus::Unimportant => Color32::from_rgb(120, 120, 255),
                _ => BcTheme::THUMBNAIL_ORPHAN,
            };
            let y = strip.top() + index as f32 / count as f32 * strip.height();
            let row = Rect::from_min_size(
                pos2(strip.left(), y),
                vec2(strip.width(), (strip.height() / count as f32).max(1.0)),
            );
            ui.painter().rect_filled(row, 0.0, color);
        }
        if response.clicked() || response.dragged() {
            if let Some(pos) = response.interact_pointer_pos() {
                self.selected_line_idx = (((pos.y - strip.top()) / strip.height()).clamp(0.0, 1.0)
                    * count as f32) as usize;
                self.selected_line_idx = self.selected_line_idx.min(count - 1);
                self.scroll_to_selection = true;
            }
        }
    }

    fn diff_view(&mut self, ui: &mut Ui, icons: &mut IconManager) {
        let height = ui.available_height().max(1.0);
        let row_height = ROW_HEIGHT;
        if self.previous_filter != self.filter_mode {
            self.revealed.clear();
            self.view_revision = self.view_revision.wrapping_add(1);
            self.previous_filter = self.filter_mode;
        }
        let view = TextView::new(
            &self.diff_result,
            self.filter_mode,
            self.context_lines,
            &self.revealed,
        );
        let visible = &view.visible;
        #[cfg(test)]
        self.fold_controls.clear();
        #[cfg(test)]
        self.gutter_arrows.clear();
        #[cfg(test)]
        self.chrome_controls.clear();
        if !visible.is_empty() && !visible.contains(&self.selected_line_idx) {
            self.selected_line_idx = visible
                .iter()
                .copied()
                .find(|&index| index >= self.selected_line_idx)
                .unwrap_or_else(|| *visible.last().unwrap());
            ui.ctx().request_repaint();
        }
        let before = self
            .editing
            .then(|| (self.left.text.clone(), self.right.text.clone()));
        let mut edited = false;
        if self.scroll_to_selection {
            if let Some(y) = view.line_y.get(self.selected_line_idx).copied().flatten() {
                self.scroll_offset_y = (y - height / 2.0).max(0.0);
            }
            {
                let side = usize::from(!self.active_left);
                if let Some(id) = self.editor_cache[side].widget_id {
                    let number = self
                        .diff_result
                        .lines
                        .iter()
                        .skip(self.selected_line_idx)
                        .find_map(|line| {
                            if self.active_left {
                                line.left_line_no
                            } else {
                                line.right_line_no
                            }
                        });
                    let text = if self.active_left {
                        &self.left.text
                    } else {
                        &self.right.text
                    };
                    let index = number
                        .map(|n| {
                            text.split_inclusive('\n')
                                .take(n - 1)
                                .map(|line| line.chars().count())
                                .sum()
                        })
                        .unwrap_or_else(|| text.chars().count());
                    if let Some(mut state) = TextEdit::load_state(ui.ctx(), id) {
                        state
                            .cursor
                            .set_char_range(Some(egui::text::CCursorRange::one(
                                egui::text::CCursor::new(index),
                            )));
                        state.store(ui.ctx(), id);
                        ui.memory_mut(|memory| memory.request_focus(id));
                    }
                }
            }
            self.scroll_to_selection = false;
        }
        if let Some((left, range)) = self.pending_selection.take() {
            if let Some(id) = self.editor_cache[usize::from(!left)].widget_id {
                if let Some(mut state) = TextEdit::load_state(ui.ctx(), id) {
                    state
                        .cursor
                        .set_char_range(Some(egui::text::CCursorRange::two(
                            egui::text::CCursor::new(range.start),
                            egui::text::CCursor::new(range.end),
                        )));
                    state.store(ui.ctx(), id);
                    ui.memory_mut(|memory| memory.request_focus(id));
                }
            }
        }
        // Whole physical pixels, so the gutter and its icons of the right pane
        // stay on the pixel grid instead of being blended.
        let ppp = ui.ctx().pixels_per_point();
        // The frame starts one pixel under the thumbnail's right edge. The
        // native splitter is 3 px; its left edge is at half the main panel,
        // rather than half the space remaining after subtracting the splitter.
        let frame_rect = Rect::from_min_max(ui.cursor().min - vec2(1.0, 0.0), ui.max_rect().max);
        let first_width = ((frame_rect.width() / 2.0 * ppp).floor() / ppp).max(1.0);
        let pane_widths = [
            first_width,
            (frame_rect.width() - first_width - super::text_chrome::SPLITTER_WIDTH).max(1.0),
        ];
        let mut panes = ui.new_child(UiBuilder::new().max_rect(frame_rect));
        // The thumbnail and editor share this edge. The central panel's
        // inherited clip would otherwise cut off the left frame's one pixel.
        let clip = panes.clip_rect();
        panes.set_clip_rect(Rect::from_min_max(
            pos2(frame_rect.left(), clip.top()),
            clip.max,
        ));
        let mut copy = None;
        let mut copy_rows: Option<(bool, Range<usize>)> = None;
        let mut margin_target: Option<(bool, usize)> = None;
        let (primary_pressed, primary_down, shift) = ui.input(|i| {
            (
                i.pointer.primary_pressed(),
                i.pointer.primary_down(),
                i.modifiers.shift,
            )
        });
        let pointer = ui.input(|i| i.pointer.interact_pos());
        let selection_rows = [true, false].map(|left| {
            let selection = self.selected_bytes(ui.ctx(), left);
            self.selected_rows(left, selection)
        });
        let mut toggle_fold = None;
        panes.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = super::text_chrome::SPLITTER_WIDTH;
            for left in [true, false] {
                let side = if left { 0 } else { 1 };
                let pane_width = pane_widths[side];
                let selection_arrow = icons.gutter_selection_arrow(ui.ctx(), left);
                let arrow = icons.get_icon(
                    ui.ctx(),
                    if left {
                        "imgGutterCopySectionRight"
                    } else {
                        "imgGutterCopySectionLeft"
                    },
                );
                ui.allocate_ui_with_layout(
                    vec2(pane_width, height),
                    Layout::top_down(Align::Min),
                    |ui| {
                        ui.set_min_width(pane_width);
                        ui.spacing_mut().item_spacing.y = 0.0;
                        let pane_top = ui.cursor().min;
                        let pane_rect = Rect::from_min_size(pane_top, vec2(pane_width, height));
                        let editor_height =
                            (height - super::text_chrome::STATUS_HEIGHT - 1.0).max(1.0);
                        let editor_width =
                            (pane_width - super::text_chrome::BAR_WIDTH - 2.0).max(1.0);
                        // Reserve the non-scrolling top/left border explicitly.
                        ui.add_space(1.0);
                        let editor = ui.scope_builder(
                            UiBuilder::new().max_rect(Rect::from_min_size(
                                pane_top + vec2(1.0, 1.0),
                                vec2(editor_width, editor_height),
                            )),
                            |ui| {
                                let char_width = ui.fonts(|fonts| {
                                    fonts.glyph_width(&FontId::monospace(FONT_SIZE), 'M')
                                });
                                let content_width =
                                    (self.max_line_lengths[side] as f32 * char_width + TEXT_OFFSET)
                                        .max(editor_width);
                                ScrollArea::both()
                                    .id_salt(("aligned_pane", left))
                                    .auto_shrink([false; 2])
                                    .scroll_bar_visibility(
                                        egui::scroll_area::ScrollBarVisibility::AlwaysHidden,
                                    )
                                    .max_height(editor_height)
                                    .min_scrolled_height(editor_height)
                                    .max_width(editor_width)
                                    .horizontal_scroll_offset(self.scroll_offset_x[side])
                                    .vertical_scroll_offset(self.scroll_offset_y)
                                    .show_viewport(ui, |ui, viewport| {
                                        ui.set_min_width(content_width);
                                        let origin = ui.cursor().min;
                                        let content_height = (view.height
                                            + if self.editing { row_height } else { 0.0 })
                                        .max(row_height);
                                        ui.allocate_space(vec2(content_width, content_height));
                                        if visible.is_empty() {
                                            let empty = Rect::from_min_size(
                                                origin + vec2(TEXT_OFFSET, 0.0),
                                                vec2(content_width - TEXT_OFFSET, row_height),
                                            );
                                            draw_hatching(ui.painter(), empty);
                                        }
                                        let mut folds = Vec::new();
                                        for view_row in view.rows.iter().filter(|row| {
                                            row.top + row.height() >= viewport.top()
                                                && row.top <= viewport.bottom()
                                        }) {
                                            if let RowKind::Fold { range, expanded } =
                                                &view_row.kind
                                            {
                                                folds.push((
                                                    Rect::from_min_size(
                                                        origin
                                                            + vec2(viewport.left(), view_row.top),
                                                        vec2(viewport.width(), FOLD_HEIGHT),
                                                    ),
                                                    range.clone(),
                                                    *expanded,
                                                ));
                                                continue;
                                            }
                                            let RowKind::Line(index) = view_row.kind else {
                                                unreachable!()
                                            };
                                            let line = &self.diff_result.lines[index];
                                            let block_start = line.status != DiffStatus::Same
                                                && (index == 0
                                                    || self.diff_result.lines[index - 1].status
                                                        == DiffStatus::Same);
                                            let rect = Rect::from_min_size(
                                                origin + vec2(0.0, view_row.top),
                                                vec2(content_width, row_height),
                                            );
                                            let hit_rect = Rect::from_min_max(
                                                pos2(origin.x + viewport.left(), rect.top()),
                                                pos2(
                                                    origin.x + viewport.left() + GUTTER_WIDTH,
                                                    rect.bottom(),
                                                ),
                                            );
                                            let selection = selection_rows[side].as_ref();
                                            let gutter_arrow =
                                                GutterArrow::for_row(index, block_start, selection);
                                            // UiEditorWindow.pas (REA 18/10/2026): the margin is hit type 3
                                            // (line selection: a click selects the whole line, a drag or
                                            // Shift+click extends by lines); only the button rectangle of a
                                            // row that has a button is type 4 (0xf034e0: 4 px in, 14 px wide,
                                            // full row height) and copies.
                                            let margin = ui.interact(
                                                hit_rect,
                                                ui.id().with(("diff_margin", left, index)),
                                                Sense::click_and_drag(),
                                            );
                                            let button_rect = Rect::from_min_max(
                                                pos2(
                                                    hit_rect.left() + GUTTER_BUTTON_LEFT,
                                                    rect.top(),
                                                ),
                                                pos2(
                                                    hit_rect.left()
                                                        + GUTTER_BUTTON_LEFT
                                                        + GUTTER_BUTTON_WIDTH,
                                                    rect.bottom(),
                                                ),
                                            );
                                            let button = gutter_arrow.map(|kind| {
                                                #[cfg(test)]
                                                self.gutter_arrows.push((
                                                    left,
                                                    index,
                                                    button_rect,
                                                    kind,
                                                ));
                                                ui.interact(
                                                    button_rect,
                                                    ui.id().with(("diff_row", left, index)),
                                                    Sense::click(),
                                                )
                                                .on_hover_text(crate::i18n::tr(kind.hint(left)))
                                            });
                                            let on_button = button
                                                .as_ref()
                                                .is_some_and(|b| b.contains_pointer());
                                            if button.as_ref().is_some_and(|b| b.clicked()) {
                                                self.selected_line_idx = index;
                                                self.active_left = left;
                                                match gutter_arrow {
                                                    Some(GutterArrow::Section) => copy = Some(left),
                                                    Some(GutterArrow::Selection) => {
                                                        copy_rows = selection
                                                            .map(|rows| (left, rows.clone()));
                                                    }
                                                    None => {}
                                                }
                                            }
                                            let pointer_in_row = pointer.is_some_and(|p| {
                                                p.y >= rect.top() && p.y < rect.bottom()
                                            });
                                            if primary_pressed
                                                && margin.contains_pointer()
                                                && !on_button
                                            {
                                                let anchor = match self.margin_anchor {
                                                    Some((side, anchor))
                                                        if shift && side == left =>
                                                    {
                                                        anchor
                                                    }
                                                    _ if shift && self.active_left == left => {
                                                        self.selected_line_idx
                                                    }
                                                    _ => index,
                                                };
                                                self.margin_anchor = Some((left, anchor));
                                                self.margin_selecting = Some(left);
                                                margin_target = Some((left, index));
                                            } else if primary_down
                                                && self.margin_selecting == Some(left)
                                                && pointer_in_row
                                            {
                                                margin_target = Some((left, index));
                                            } else if primary_pressed
                                                && pointer_in_row
                                                && pointer.is_some_and(|p| {
                                                    p.x >= rect.left() + GUTTER_WIDTH
                                                        && p.x < rect.right()
                                                })
                                            {
                                                // A press in the text is an ordinary caret/character
                                                // selection; the next Shift+click in the margin starts
                                                // from the current line again.
                                                self.margin_anchor = None;
                                            }
                                            let response = button.unwrap_or(margin);
                                            let line_arrow = match gutter_arrow {
                                                Some(GutterArrow::Section) => arrow.as_ref(),
                                                Some(GutterArrow::Selection) => {
                                                    selection_arrow.as_ref()
                                                }
                                                None => None,
                                            };
                                            render_diff_line(
                                                ui,
                                                rect,
                                                line,
                                                LineAppearance {
                                                    left,
                                                    selected: self.selected_line_idx == index,
                                                    line_numbers: self.line_numbers,
                                                    active: self.active_left == left,
                                                    arrow: line_arrow,
                                                    hot: line_arrow.is_some()
                                                        && on_button
                                                        && response.hovered(),
                                                    paint_text: false,
                                                    gutter_left: origin.x + viewport.left(),
                                                },
                                            );
                                        }
                                        {
                                            let options = LayoutOptions {
                                                left,
                                                active: self.active_left == left,
                                                filter: self.filter_mode,
                                                context: self.context_lines,
                                                revision: self.revision,
                                                view_revision: self.view_revision,
                                                paint_range: [
                                                    ((viewport.top() / 128.0).floor() * 128.0
                                                        - 128.0)
                                                        as i32,
                                                    ((viewport.bottom() / 128.0).ceil() * 128.0
                                                        + 128.0)
                                                        as i32,
                                                ],
                                                unchanged: true,
                                                rules: self.active_rules(),
                                            };
                                            let (text, other) = if left {
                                                (&mut self.left.text, &self.right.text)
                                            } else {
                                                (&mut self.right.text, &self.left.text)
                                            };
                                            let cache = &mut self.editor_cache[side];
                                            let layout = cache.layout(
                                                ui,
                                                text,
                                                other,
                                                &self.diff_result,
                                                options,
                                                &self.revealed,
                                            );
                                            let initial_text = text.clone();
                                            let editor_origin =
                                                origin + vec2(TEXT_OFFSET, layout.leading_y);
                                            let editor_rect = Rect::from_min_size(
                                                editor_origin,
                                                vec2(
                                                    content_width - TEXT_OFFSET,
                                                    layout.galley.size().y.max(row_height),
                                                ),
                                            );
                                            ui.allocate_new_ui(
                                                UiBuilder::new().max_rect(editor_rect),
                                                |ui| {
                                                    let clip = ui.clip_rect();
                                                    ui.set_clip_rect(Rect::from_min_max(
                                                        pos2(
                                                            clip.left().max(
                                                                origin.x
                                                                    + viewport.left()
                                                                    + TEXT_OFFSET,
                                                            ),
                                                            clip.top(),
                                                        ),
                                                        clip.max,
                                                    ));
                                                    ui.visuals_mut().text_cursor.stroke.width = 0.0;
                                                    ui.visuals_mut().text_cursor.preview = false;
                                                    let mut layouter =
                                                        |ui: &Ui, text: &str, _: f32| {
                                                            cache
                                                                .layout(
                                                                    ui,
                                                                    text,
                                                                    other,
                                                                    &self.diff_result,
                                                                    LayoutOptions {
                                                                        unchanged: text
                                                                            == initial_text,
                                                                        ..options
                                                                    },
                                                                    &self.revealed,
                                                                )
                                                                .galley
                                                        };
                                                    let backwards = ui.input(|i| {
                                                        i.key_pressed(Key::ArrowUp)
                                                            || i.key_pressed(Key::ArrowLeft)
                                                    });
                                                    let mut read_only;
                                                    let editor = if self.editing {
                                                        TextEdit::multiline(text)
                                                    } else {
                                                        read_only = text.as_str();
                                                        TextEdit::multiline(&mut read_only)
                                                    };
                                                    let mut output = editor
                                                        .id_salt(("aligned_editor", left))
                                                        .font(FontId::monospace(FONT_SIZE))
                                                        .margin(Vec2::ZERO)
                                                        .frame(false)
                                                        .desired_width(content_width - TEXT_OFFSET)
                                                        .desired_rows(1)
                                                        .layouter(&mut layouter)
                                                        .show(ui);
                                                    cache.widget_id = Some(output.response.id);
                                                    cache.widget_rect = Some(output.response.rect);
                                                    if self.show_whitespace {
                                                        let painter = ui
                                                            .painter()
                                                            .with_clip_rect(output.text_clip_rect);
                                                        for row in output.galley.rows.iter().filter(
                                                            |row| {
                                                                row.rect
                                                                    .translate(
                                                                        output.galley_pos.to_vec2(),
                                                                    )
                                                                    .intersects(
                                                                        output.text_clip_rect,
                                                                    )
                                                            },
                                                        ) {
                                                            for glyph in &row.glyphs {
                                                                let marker = match glyph.chr {
                                                                    ' ' => Some("·"),
                                                                    '\t' => Some("→"),
                                                                    _ => None,
                                                                };
                                                                if let Some(marker) = marker {
                                                                    painter.text(
                                                                        output.galley_pos
                                                                            + glyph.pos.to_vec2(),
                                                                        Align2::LEFT_BOTTOM,
                                                                        marker,
                                                                        FontId::monospace(
                                                                            FONT_SIZE,
                                                                        ),
                                                                        Color32::from_gray(130),
                                                                    );
                                                                }
                                                            }
                                                        }
                                                    }
                                                    edited |= output.response.changed();
                                                    if output.response.has_focus() {
                                                        self.active_left = left;
                                                        if let Some(mut range) = output.cursor_range
                                                        {
                                                            // Keep a select-all intact even when the
                                                            // last line is filtered out.
                                                            let whole =
                                                                range.secondary.ccursor.index == 0
                                                                    && range.primary.ccursor.index
                                                                        == output
                                                                            .galley
                                                                            .end()
                                                                            .ccursor
                                                                            .index;
                                                            if self.filter_mode != FilterMode::All
                                                                && !output.response.changed()
                                                                && !whole
                                                            {
                                                                if let Some(cursor) =
                                                                    aligned_editor::visible_cursor(
                                                                        &output.galley,
                                                                        range.primary,
                                                                        &self.diff_result,
                                                                        left,
                                                                        visible,
                                                                        backwards,
                                                                    )
                                                                {
                                                                    let empty = range.is_empty();
                                                                    range.primary = cursor;
                                                                    if empty {
                                                                        range.secondary = cursor;
                                                                    }
                                                                    output
                                                                        .state
                                                                        .cursor
                                                                        .set_char_range(Some(
                                                                            range
                                                                                .as_ccursor_range(),
                                                                        ));
                                                                    output.state.clone().store(
                                                                        ui.ctx(),
                                                                        output.response.id,
                                                                    );
                                                                    ui.ctx().request_repaint();
                                                                }
                                                            }
                                                            let caret = (
                                                                range.primary.rcursor.row + 1,
                                                                range.primary.rcursor.column + 1,
                                                            );
                                                            if self.caret[side] != caret
                                                                || output.response.changed()
                                                                || output.response.clicked()
                                                            {
                                                                self.caret[side] = caret;
                                                                if let Some(index) = self
                                                                    .diff_result
                                                                    .lines
                                                                    .iter()
                                                                    .position(|line| {
                                                                        (if left {
                                                                            line.left_line_no
                                                                        } else {
                                                                            line.right_line_no
                                                                        }) == Some(caret.0)
                                                                    })
                                                                {
                                                                    self.selected_line_idx = index;
                                                                }
                                                            }
                                                            if self.editing && range.is_empty() {
                                                                let rect = output
                                                                    .galley
                                                                    .pos_from_cursor(&range.primary)
                                                                    .translate(
                                                                        output.galley_pos.to_vec2(),
                                                                    );
                                                                ui.painter()
                                                                    .with_clip_rect(
                                                                        output.text_clip_rect,
                                                                    )
                                                                    .line_segment(
                                                                        [
                                                                            rect.left_top(),
                                                                            rect.left_top()
                                                                                + vec2(
                                                                                    0.0, row_height,
                                                                                ),
                                                                        ],
                                                                        Stroke::new(
                                                                            1.0_f32,
                                                                            Color32::BLACK,
                                                                        ),
                                                                    );
                                                            }
                                                        }
                                                    }
                                                },
                                            );
                                        }
                                        for (rect, range, expanded) in folds {
                                            draw_filtered_gap(
                                                ui,
                                                rect,
                                                range.len(),
                                                expanded,
                                                self.show_filtered_counts,
                                            );
                                            let hit = rect.intersect(ui.clip_rect());
                                            let response = ui.interact(
                                                hit,
                                                ui.id().with((
                                                    "fold",
                                                    left,
                                                    range.start,
                                                    range.end,
                                                )),
                                                Sense::click(),
                                            );
                                            response.context_menu(|ui| {
                                                for (label, enabled) in [
                                                    ("Recolher intervalo filtrado", expanded),
                                                    ("Expandir intervalo filtrado", !expanded),
                                                ] {
                                                    if super::chrome::menu_item(
                                                        ui, icons, label, enabled,
                                                    )
                                                    .clicked()
                                                    {
                                                        toggle_fold = Some(range.clone());
                                                        ui.close_menu();
                                                    }
                                                }
                                                ui.separator();
                                                ui.checkbox(
                                                    &mut self.show_filtered_counts,
                                                    crate::i18n::tr(
                                                        "Mostrar contagem de linhas filtradas",
                                                    ),
                                                );
                                            });
                                            #[cfg(test)]
                                            self.fold_controls.push((left, range.clone(), hit));
                                            if response
                                                .on_hover_text(crate::i18n::tr(
                                                    &(format!(
                                                        "{} {} linhas",
                                                        if expanded {
                                                            "Recolher"
                                                        } else {
                                                            "Expandir"
                                                        },
                                                        range.len()
                                                    )),
                                                ))
                                                .clicked()
                                            {
                                                toggle_fold = Some(range);
                                            }
                                        }
                                        // Section brackets belong to the margin, not
                                        // the text. Draw after row fills and keep them
                                        // fixed while the text scrolls horizontally.
                                        if !self.line_numbers {
                                            draw_section_brackets(
                                                ui,
                                                origin,
                                                viewport,
                                                &view,
                                                &self.diff_result,
                                                self.active_left == left,
                                            );
                                        }
                                    })
                            },
                        );
                        let output = editor.inner;
                        self.scroll_offset_x[side] = output.state.offset.x;
                        let status_rect = Rect::from_min_size(
                            pane_top + vec2(0.0, editor_height + 1.0),
                            vec2(editor_width + 1.0, super::text_chrome::STATUS_HEIGHT),
                        );
                        let row = self.diff_result.lines.get(self.selected_line_idx);
                        let number = row.and_then(|line| {
                            if left {
                                line.left_line_no
                            } else {
                                line.right_line_no
                            }
                        });
                        let position = number
                            .map(|number| format!("{number}: {}", self.caret[side].1))
                            .unwrap_or_default();
                        let format = if number.is_some() {
                            crate::i18n::tr("Texto padrão")
                        } else {
                            String::new()
                        };
                        let dirty = if left {
                            self.left.dirty()
                        } else {
                            self.right.dirty()
                        };
                        let horizontal = super::text_chrome::pane_status(
                            ui,
                            status_rect,
                            &position,
                            &format,
                            dirty,
                        );
                        super::text_chrome::scroll_bar(
                            ui,
                            ui.id().with(("horizontal", left)),
                            horizontal,
                            super::text_chrome::Axis::Horizontal,
                            output.content_size.x,
                            output.inner_rect.width(),
                            &mut self.scroll_offset_x[side],
                        );
                        let vertical = Rect::from_min_size(
                            pane_top + vec2(editor_width + 2.0, 1.0),
                            vec2(
                                super::text_chrome::BAR_WIDTH - 1.0,
                                (editor_height - 54.0).max(0.0),
                            ),
                        );
                        let mut offset_y = output.state.offset.y;
                        super::text_chrome::scroll_bar(
                            ui,
                            ui.id().with(("vertical", left)),
                            vertical,
                            super::text_chrome::Axis::Vertical,
                            output.content_size.y,
                            output.inner_rect.height(),
                            &mut offset_y,
                        );
                        let mut nav = Rect::from_min_size(
                            pane_top
                                + vec2(editor_width + 2.0, (editor_height - 54.0).max(0.0) + 1.0),
                            vec2(super::text_chrome::BAR_WIDTH - 1.0, 54.0),
                        );
                        // The right-hand navigation strip includes the inner
                        // bevel; the left strip leaves it visible. Measured on
                        // text-loaded.png (right edge stops at y=694).
                        if !left {
                            nav.min -= vec2(1.0, 1.0);
                        }
                        #[cfg(test)]
                        self.chrome_controls
                            .push((left, vertical, horizontal, output.id));
                        if let Some(choice) = super::text_chrome::navigation(
                            ui,
                            icons,
                            nav,
                            &mut self.navigation_mode,
                        ) {
                            self.active_left = left;
                            match self.navigation_mode {
                                super::text_chrome::NavigationMode::Sections => {
                                    self.next_diff(choice > 0)
                                }
                                super::text_chrome::NavigationMode::Find => {
                                    self.search.shown = true;
                                    if self.search.options.query.is_empty() {
                                        self.search.focus_query = true;
                                    } else {
                                        self.find(ui.ctx(), choice > 0);
                                    }
                                }
                                super::text_chrome::NavigationMode::Differences => {
                                    let next = if choice > 0 {
                                        self.diff_result
                                            .lines
                                            .iter()
                                            .enumerate()
                                            .find(|(i, l)| {
                                                *i > self.selected_line_idx
                                                    && l.status != DiffStatus::Same
                                            })
                                            .map(|(i, _)| i)
                                    } else {
                                        self.diff_result
                                            .lines
                                            .iter()
                                            .enumerate()
                                            .rev()
                                            .find(|(i, l)| {
                                                *i < self.selected_line_idx
                                                    && l.status != DiffStatus::Same
                                            })
                                            .map(|(i, _)| i)
                                    };
                                    if let Some(i) = next {
                                        self.selected_line_idx = i;
                                        self.scroll_to_selection = true;
                                    }
                                }
                            }
                        }
                        if (offset_y - output.state.offset.y).abs() > 0.1 {
                            self.scroll_offset_y = offset_y;
                        }
                        let mut state = output.state;
                        state.offset.x = self.scroll_offset_x[side];
                        state.offset.y = offset_y;
                        state.store(ui.ctx(), output.id);
                        let editor_focused = self.editor_cache[side]
                            .widget_id
                            .is_some_and(|id| ui.memory(|memory| memory.has_focus(id)));
                        if (editor_focused
                            || output.inner_rect.contains(
                                ui.input(|i| i.pointer.hover_pos().unwrap_or(Pos2::ZERO)),
                            ))
                            && (self.scroll_offset_y - offset_y).abs() > 0.1
                        {
                            self.scroll_offset_y = offset_y;
                            ui.ctx().request_repaint();
                        }
                        let mut viewport_edge = output.inner_rect;
                        if !left {
                            viewport_edge.max.y = nav.top();
                        }
                        super::text_chrome::editor_edges(ui, pane_rect, viewport_edge);
                    },
                );
            }
        });
        if let Some(range) = toggle_fold {
            text_view::toggle(&mut self.revealed, range);
            self.view_revision = self.view_revision.wrapping_add(1);
            ui.ctx().request_repaint();
        }
        if edited {
            if let Some(before) = before {
                if before.0 != self.left.text || before.1 != self.right.text {
                    if self.undo.len() >= 20 {
                        self.undo.remove(0);
                    }
                    self.undo.push(before);
                    self.redo.clear();
                    let side = usize::from(!self.active_left);
                    if let Some((result, ms)) =
                        self.editor_cache[side].take_comparison(&self.left.text, &self.right.text)
                    {
                        self.accept_comparison(result, ms);
                    } else {
                        self.compare();
                    }
                    let physical = self.caret[usize::from(!self.active_left)].0;
                    if let Some(index) = self.diff_result.lines.iter().position(|line| {
                        (if self.active_left {
                            line.left_line_no
                        } else {
                            line.right_line_no
                        }) == Some(physical)
                    }) {
                        self.selected_line_idx = index;
                    }
                    ui.ctx().request_repaint();
                }
            }
        }
        if let Some((left, row)) = margin_target {
            if let Some((_, anchor)) = self.margin_anchor {
                if let Some(chars) = self.line_chars(left, anchor.min(row)..anchor.max(row) + 1) {
                    self.pending_selection = Some((left, chars));
                }
                self.selected_line_idx = row;
                self.active_left = left;
                ui.ctx().request_repaint();
            }
        }
        if !primary_down {
            // Releasing the button ends the line drag; the anchor stays for Shift+click.
            self.margin_selecting = None;
        }
        if let Some(to_right) = copy {
            self.copy_difference(to_right);
        }
        if let Some((to_right, rows)) = copy_rows {
            self.copy_rows(rows, to_right);
        }
    }

    /// Character range of the pane lines shown in a range of aligned rows, with their
    /// line ends (a margin selection takes whole lines). Rows without a line on this side
    /// are skipped; `None` when the range has no line of this pane.
    fn line_chars(&self, left: bool, rows: Range<usize>) -> Option<Range<usize>> {
        let number = |line: &AlignedLine| {
            if left {
                line.left_line_no
            } else {
                line.right_line_no
            }
        };
        let lines = &self.diff_result.lines[rows.start.min(self.diff_result.lines.len())
            ..rows.end.min(self.diff_result.lines.len())];
        let first = lines.iter().find_map(number)?;
        let last = lines.iter().rev().find_map(number)?;
        let text = if left {
            &self.left.text
        } else {
            &self.right.text
        };
        let mut start = 0;
        let mut end = 0;
        for (n, line) in text.split_inclusive('\n').enumerate() {
            let count = line.chars().count();
            if n + 1 < first {
                start += count;
            }
            if n < last {
                end += count;
            } else {
                break;
            }
        }
        Some(start..end)
    }
}

struct LineAppearance<'a> {
    left: bool,
    selected: bool,
    line_numbers: bool,
    active: bool,
    arrow: Option<&'a TextureHandle>,
    hot: bool,
    paint_text: bool,
    gutter_left: f32,
}

fn paint_search_chevron(ui: &Ui, rect: Rect, expanded: bool) {
    let center = rect.center();
    let direction = if expanded { -1.0 } else { 1.0 };
    ui.painter().add(Shape::line(
        vec![
            center + vec2(-3.0, -1.5 * direction),
            center + vec2(0.0, 1.5 * direction),
            center + vec2(3.0, -1.5 * direction),
        ],
        Stroke::new(1.5_f32, Color32::from_gray(60)),
    ));
}

fn insert_search_expression(ctx: &Context, id: Id, text: &mut String, expression: &str) {
    let mut state = TextEdit::load_state(ctx, id).unwrap_or_default();
    let range = state
        .cursor
        .char_range()
        .map(|range| {
            range.primary.index.min(range.secondary.index)
                ..range.primary.index.max(range.secondary.index)
        })
        .unwrap_or_else(|| text.chars().count()..text.chars().count());
    let byte_at = |index| {
        text.char_indices()
            .nth(index)
            .map_or(text.len(), |(byte, _)| byte)
    };
    let bytes = byte_at(range.start)..byte_at(range.end);
    text.replace_range(bytes, expression);
    state
        .cursor
        .set_char_range(Some(egui::text::CCursorRange::one(
            egui::text::CCursor::new(range.start + expression.chars().count()),
        )));
    state.store(ctx, id);
}

/// At 100% the original has a 1 px gray-105 stem at gutter + 9, from
/// the second row to 3 px above the section bottom, with a 4 px foot.
/// Single-line sections only have an arrow. Coordinates are in the aligned
/// view, so inserted/deleted rows and filtered bands preserve section bounds.
fn draw_section_brackets(
    ui: &Ui,
    origin: Pos2,
    viewport: Rect,
    view: &TextView,
    diff: &DiffResult,
    active: bool,
) {
    let left = origin.x + viewport.left();
    let clip = ui.clip_rect().intersect(Rect::from_min_max(
        pos2(left, origin.y + viewport.top()),
        pos2(left + GUTTER_WIDTH, origin.y + viewport.bottom()),
    ));
    let painter = ui.painter().with_clip_rect(clip);
    let color = Color32::from_gray(if active { 105 } else { 132 });
    for section in &diff.sections {
        if section.len() < 2 {
            continue;
        }
        let (Some(first), Some(last)) = (view.line_y[section.start], view.line_y[section.end - 1])
        else {
            continue;
        };
        let top = origin.y + first + ROW_HEIGHT;
        let bottom = origin.y + last + ROW_HEIGHT - 3.0;
        if bottom < clip.top() || top >= clip.bottom() {
            continue;
        }
        for rect in [
            Rect::from_min_max(pos2(left + 9.0, top), pos2(left + 10.0, bottom + 1.0)),
            Rect::from_min_size(pos2(left + 9.0, bottom), vec2(4.0, 1.0)),
        ] {
            painter.rect_filled(painter.round_rect_to_pixels(rect), 0.0, color);
        }
    }
}

fn draw_filtered_gap(ui: &Ui, rect: Rect, count: usize, expanded: bool, show_count: bool) {
    let painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
    painter.rect_filled(rect, 0.0, BcTheme::WINDOW_BG);
    let stroke = Stroke::new(1.0_f32, Color32::from_gray(128));
    let start = rect.left() + TEXT_OFFSET - 2.0;
    painter.line_segment(
        [
            pos2(start, rect.center().y),
            pos2(rect.right(), rect.center().y),
        ],
        Stroke::new(1.0_f32, Color32::from_gray(160)),
    );
    // Keep the centered border inside the 9 px band, including its half-pixel
    // stroke. A box starting at band.top() loses its upper border to clipping.
    let button = Rect::from_center_size(pos2(rect.left() + 7.0, rect.center().y), vec2(7.0, 7.0));
    painter.rect_filled(button, 0.0, BcTheme::WINDOW_BG);
    painter.rect_stroke(button, 0.0, stroke);
    painter.line_segment(
        [
            button.center() - vec2(2.0, 0.0),
            button.center() + vec2(2.0, 0.0),
        ],
        stroke,
    );
    if !expanded {
        painter.line_segment(
            [
                button.center() - vec2(0.0, 2.0),
                button.center() + vec2(0.0, 2.0),
            ],
            stroke,
        );
    }
    if !show_count {
        return;
    }
    let clipped = rect.intersect(ui.clip_rect());
    let center = pos2(
        (start.max(clipped.left()) + clipped.right()) / 2.0,
        rect.center().y,
    );
    let text = format!("{count} {}", crate::i18n::tr("LINHAS FILTRADAS"));
    let galley = ui.fonts(|fonts| {
        fonts.layout_no_wrap(text, FontId::proportional(7.0), Color32::from_gray(105))
    });
    let label = Rect::from_center_size(center, galley.size() + vec2(6.0, 0.0));
    painter.rect_filled(label, 0.0, BcTheme::WINDOW_BG);
    painter.galley(
        label.left_top() + vec2(3.0, 0.0),
        galley,
        Color32::from_gray(105),
    );
}

fn render_diff_line(ui: &Ui, rect: Rect, line: &AlignedLine, appearance: LineAppearance<'_>) {
    let LineAppearance {
        left,
        selected,
        line_numbers,
        active,
        arrow,
        hot,
        paint_text,
        gutter_left,
    } = appearance;
    let line_no = if left {
        line.left_line_no
    } else {
        line.right_line_no
    };
    let chunks = if left {
        &line.left_inline
    } else {
        &line.right_inline
    };
    let color = if line_no.is_none() {
        BcTheme::WINDOW_BG
    } else {
        match line.status {
            DiffStatus::Same => {
                if active {
                    Color32::WHITE
                } else {
                    Color32::from_rgb(252, 252, 252)
                }
            }
            DiffStatus::Different => {
                if active {
                    BcTheme::DIFF_BG
                } else {
                    Color32::from_rgb(252, 230, 230)
                }
            }
            DiffStatus::Unimportant => BcTheme::MINOR_BG,
            _ => BcTheme::ORPHAN_BG,
        }
    };
    let painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
    painter.rect_filled(rect, 0.0, color);
    // No line at the gutter/text boundary; the lowered frame surrounds the
    // whole editor and the section brackets are drawn after all rows.
    let gutter = painter.round_rect_to_pixels(Rect::from_min_max(
        pos2(gutter_left, rect.top()),
        pos2(gutter_left + GUTTER_WIDTH, rect.bottom()),
    ));
    painter.rect_filled(gutter, 0.0, BcTheme::GUTTER_BG);
    if selected && active {
        painter.rect_stroke(
            Rect::from_min_max(pos2(gutter.right(), rect.top()), rect.max),
            0.0,
            Stroke::new(1.0_f32, Color32::from_gray(105)),
        );
    }
    if line_numbers {
        if let Some(number) = line_no {
            painter.text(
                pos2(gutter.right() - 4.0, rect.center().y),
                Align2::RIGHT_CENTER,
                number.to_string(),
                FontId::monospace(12.0),
                BcTheme::GUTTER_TEXT,
            );
        }
    } else if let Some(arrow) = arrow {
        // Button image (14x10 at 100%) 4 px into the gutter, drawn at its own
        // pixel size on the physical grid; under the mouse a light-blue frame
        // of the row height minus one surrounds it (sampled at 100%).
        let size = arrow.size_vec2() / painter.ctx().pixels_per_point();
        let min =
            painter.round_pos_to_pixels(pos2(gutter.left() + 4.0, rect.center().y - size.y / 2.0));
        if hot {
            let frame = painter.round_rect_to_pixels(Rect::from_min_size(
                pos2(min.x, rect.top() + 1.0),
                vec2(size.x, rect.height() - 1.0),
            ));
            painter.rect_filled(frame, 0.0, BcTheme::GUTTER_HOT_BORDER);
            painter.rect_filled(frame.shrink(1.0), 0.0, BcTheme::GUTTER_HOT_FILL);
        }
        painter.image(
            arrow.id(),
            Rect::from_min_size(min, size),
            Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)),
            Color32::WHITE,
        );
    }
    if line_no.is_none() {
        draw_hatching(
            &painter,
            Rect::from_min_max(pos2(gutter.right(), rect.top()), rect.max),
        );
    }
    if paint_text {
        let galley = ui.fonts(|fonts| fonts.layout_job(inline_job(chunks, active)));
        painter.galley(
            pos2(rect.left() + TEXT_OFFSET, rect.top()),
            galley,
            BcTheme::TEXT_NORMAL,
        );
    }
}

fn inline_job(chunks: &[InlineChunk], _active: bool) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    for chunk in chunks {
        let changed = chunk.kind == InlineChunkKind::Diff;
        job.append(
            &chunk.text,
            0.0,
            TextFormat {
                font_id: FontId::monospace(FONT_SIZE),
                color: match chunk.kind {
                    InlineChunkKind::Diff => BcTheme::DIFF_TEXT,
                    InlineChunkKind::Unimportant => BcTheme::MINOR_TEXT,
                    InlineChunkKind::Same => Color32::BLACK,
                },
                background: if changed {
                    BcTheme::DIFF_INLINE_BG
                } else {
                    Color32::TRANSPARENT
                },
                ..Default::default()
            },
        );
    }
    job
}

fn detail_job(chunks: &[InlineChunk], end_of_line: bool) -> egui::text::LayoutJob {
    let visible: Vec<_> = chunks
        .iter()
        .map(|chunk| InlineChunk {
            text: chunk
                .text
                .chars()
                .map(|c| match c {
                    ' ' => '·',
                    '\t' => '→',
                    c => c,
                })
                .collect(),
            kind: chunk.kind.clone(),
        })
        .collect();
    let mut job = inline_job(&visible, true);
    if end_of_line {
        job.append(
            "¤",
            0.0,
            TextFormat {
                font_id: FontId::monospace(FONT_SIZE),
                color: Color32::from_gray(160),
                ..Default::default()
            },
        );
    }
    job
}

fn load_time_label(milliseconds: u128) -> String {
    let mut number = format!("{:.2}", milliseconds as f64 / 1000.0);
    while number.ends_with('0') {
        number.pop();
    }
    if number.ends_with('.') {
        number.pop();
    }
    if crate::i18n::language().starts_with("pt") {
        number = number.replace('.', ",");
    }
    format!("{number} {}", crate::i18n::tr("segundos"))
}

fn draw_hatching(painter: &Painter, rect: Rect) {
    let painter = painter.with_clip_rect(rect.intersect(painter.clip_rect()));
    painter.rect_filled(rect, 0.0, Color32::WHITE);
    let mut x = rect.left() - rect.height();
    while x < rect.right() {
        for direction in [-1.0, 1.0] {
            painter.line_segment(
                [
                    pos2(x, rect.top()),
                    pos2(x + rect.height() * direction, rect.bottom()),
                ],
                Stroke::new(0.5_f32, Color32::from_rgb(218, 218, 218)),
            );
        }
        x += 6.0;
    }
}

#[cfg(test)]
mod tests {
    use super::super::dialogs::{SaveChoice, TestDialogs};
    use super::*;

    #[test]
    fn detail_whitespace_and_end_markers_do_not_change_the_document() {
        let chunks = vec![InlineChunk {
            text: "a b\tc".into(),
            kind: InlineChunkKind::Diff,
        }];
        let job = detail_job(&chunks, true);
        assert_eq!(job.text, "a·b→c¤");
        assert_eq!(chunks[0].text, "a b\tc");
        assert_eq!(detail_job(&chunks, false).text, "a·b→c");
        assert_eq!(job.sections[0].format.color, BcTheme::DIFF_TEXT);
    }

    #[test]
    fn pane_scrollbars_move_real_viewports_keep_gutters_fixed_and_preserve_buffers() {
        let ctx = Context::default();
        super::super::theme::apply(&ctx);
        let mut icons = IconManager::new();
        let mut screen = TextCompareScreen::new();
        screen.left.text = (0..200)
            .map(|i| format!("{i} {}\n", "texto ".repeat(100)))
            .collect();
        screen.right.text = screen.left.text.replace("10 texto", "10 novo");
        screen.editing = false;
        screen.compare();
        let before = (screen.left.text.clone(), screen.right.text.clone());
        render_frame_timed(&ctx, &mut screen, &mut icons, vec![]);
        let controls = screen.chrome_controls.clone();
        assert_eq!(controls.len(), 2);
        assert!(controls[0].1.right() < controls[1].1.left());
        let click = |screen: &mut TextCompareScreen, icons: &mut IconManager, pos: Pos2| {
            for pressed in [true, false] {
                render_frame_timed(
                    &ctx,
                    screen,
                    icons,
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
            render_frame_timed(&ctx, screen, icons, vec![]);
        };
        let vertical = controls[0].1;
        click(
            &mut screen,
            &mut icons,
            pos2(
                vertical.center().x,
                vertical.top() + vertical.height() * 0.65,
            ),
        );
        assert!(screen.scroll_offset_y > 100.0, "vertical thumb must scroll");
        let offsets: Vec<_> = screen
            .chrome_controls
            .iter()
            .map(|c| egui::scroll_area::State::load(&ctx, c.3).unwrap().offset.y)
            .collect();
        assert_eq!(offsets[0], offsets[1]);
        let horizontal = screen.chrome_controls[0].2;
        assert!(horizontal.width() > 15.0);
        click(
            &mut screen,
            &mut icons,
            pos2(horizontal.right() - 3.0, horizontal.center().y),
        );
        assert!(screen.scroll_offset_x[0] > 10.0);
        assert_eq!(screen.scroll_offset_x[1], 0.0);
        assert_eq!((screen.left.text, screen.right.text), before);
        assert_eq!(screen.chrome_controls[0].1, vertical);
    }

    fn disposable_pair(tag: &str) -> (TextCompareScreen, PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "anycompare-review-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&dir).unwrap();
        let left = dir.join("left.txt");
        let right = dir.join("right.txt");
        std::fs::write(&left, "original left\n").unwrap();
        std::fs::write(&right, "original right\n").unwrap();
        let mut screen = TextCompareScreen::new();
        screen.load_files(left.to_str().unwrap(), right.to_str().unwrap());
        (screen, dir)
    }

    fn remove_pair(dir: &Path) {
        for name in ["left.txt", "right.txt"] {
            std::fs::remove_file(dir.join(name)).unwrap();
        }
        std::fs::remove_dir(dir).unwrap();
    }

    #[test]
    fn encoding_reopen_cancel_and_loss_warning_preserve_buffers_and_bytes() {
        let (mut screen, dir) = disposable_pair("encoding-ui");
        let path = dir.join("left.txt");
        std::fs::write(&path, "ação").unwrap();
        screen.load_files(
            path.to_str().unwrap(),
            dir.join("right.txt").to_str().unwrap(),
        );
        screen.left.text.push('!');
        let mut dialogs = TestDialogs {
            choices: [SaveChoice::Cancel].into(),
            ..Default::default()
        };
        screen.reopen_encoded(true, Some(Encoding::Windows1252), &mut dialogs);
        assert_eq!(screen.left.text, "ação!");
        assert!(screen.left.dirty());
        dialogs.choices.push_back(SaveChoice::Discard);
        screen.reopen_encoded(true, Some(Encoding::Windows1252), &mut dialogs);
        assert_eq!(screen.left.text, "aÃ§Ã£o");
        assert!(!screen.left.dirty());
        screen.left.text = "ação🙂".into();
        dialogs.field_answers.push_back(None);
        assert!(!screen.save_side(true, false, &mut dialogs));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "ação");
        assert_eq!(screen.left.text, "ação🙂");
        assert!(screen.left.dirty());
        assert_eq!(screen.left.encoding, Encoding::Windows1252);
        dialogs.field_answers.push_back(Some(vec![
            DialogField::Notice(String::new()),
            DialogField::Choice(String::new(), 0, vec![]),
        ]));
        assert!(screen.save_side(true, false, &mut dialogs));
        assert_eq!(screen.left.encoding, Encoding::Utf8);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "ação🙂");
        assert!(!screen.left.dirty());
        assert!(dialogs.errors.is_empty());
        remove_pair(&dir);
    }

    #[test]
    fn save_state_tracks_edit_copy_undo_redo_encoding_and_successful_save() {
        let (mut screen, dir) = disposable_pair("save-state");
        assert!(!screen.left.dirty() && !screen.right.dirty());
        screen.selected_line_idx = 0;
        screen.copy_difference(true);
        assert!(!screen.left.dirty() && screen.right.dirty());
        screen.undo_merge();
        assert!(!screen.right.dirty());
        screen.redo_edit();
        assert!(screen.right.dirty());
        let mut dialogs = TestDialogs::default();
        assert!(screen.save_side(false, false, &mut dialogs));
        assert!(!screen.right.dirty());
        assert_eq!(
            std::fs::read_to_string(dir.join("right.txt")).unwrap(),
            "original left\n"
        );
        screen.right.encoding = crate::document::Encoding::Utf8Bom;
        assert!(screen.right.dirty());
        assert!(screen.save_side(false, false, &mut dialogs));
        assert!(!screen.right.dirty());
        screen.right.line_ending = LineEnding::CrLf;
        assert!(screen.right.dirty());
        assert!(dialogs.prompts.is_empty() && dialogs.errors.is_empty());
        remove_pair(&dir);
    }

    #[test]
    fn cancel_after_discard_answer_keeps_both_unsaved_documents_and_disk() {
        let (mut screen, dir) = disposable_pair("cancel-close");
        screen.left.text = "edited left".into();
        screen.right.text = "edited right".into();
        let mut dialogs = TestDialogs {
            choices: [SaveChoice::Discard, SaveChoice::Cancel].into(),
            ..Default::default()
        };
        assert!(!screen.confirm_saved(&mut dialogs));
        assert_eq!(dialogs.prompts, vec![true, false]);
        assert!(screen.left.dirty() && screen.right.dirty());
        assert_eq!(screen.left.text, "edited left");
        assert_eq!(screen.right.text, "edited right");
        assert_eq!(
            std::fs::read_to_string(dir.join("left.txt")).unwrap(),
            "original left\n"
        );
        assert_eq!(
            std::fs::read_to_string(dir.join("right.txt")).unwrap(),
            "original right\n"
        );
        remove_pair(&dir);
    }

    #[test]
    fn close_save_answers_save_each_file_before_allowing_close() {
        let (mut screen, dir) = disposable_pair("save-close");
        screen.left.text = "edited left".into();
        screen.right.text = "edited right".into();
        let mut dialogs = TestDialogs {
            choices: [SaveChoice::Save, SaveChoice::Save].into(),
            ..Default::default()
        };
        assert!(screen.confirm_saved(&mut dialogs));
        assert!(!screen.left.dirty() && !screen.right.dirty());
        assert_eq!(
            std::fs::read_to_string(dir.join("left.txt")).unwrap(),
            "edited left"
        );
        assert_eq!(
            std::fs::read_to_string(dir.join("right.txt")).unwrap(),
            "edited right"
        );
        remove_pair(&dir);
    }

    #[test]
    fn failed_save_or_canceled_save_as_prevents_close_and_preserves_edits() {
        let (mut screen, dir) = disposable_pair("failed-save");
        screen.left.text = "unsaved".into();
        std::fs::write(dir.join("left.txt"), "external change").unwrap();
        let mut dialogs = TestDialogs {
            choices: [SaveChoice::Save].into(),
            ..Default::default()
        };
        assert!(!screen.confirm_saved(&mut dialogs));
        assert_eq!(dialogs.errors.len(), 1);
        assert_eq!(screen.left.text, "unsaved");
        assert!(screen.left.dirty());
        assert_eq!(
            std::fs::read_to_string(dir.join("left.txt")).unwrap(),
            "external change"
        );
        remove_pair(&dir);
        let mut new = TextCompareScreen::new();
        new.left.text = "new text".into();
        let mut dialogs = TestDialogs {
            choices: [SaveChoice::Save].into(),
            files: [None].into(),
            ..Default::default()
        };
        assert!(!new.confirm_saved(&mut dialogs));
        assert!(new.left.dirty());
        assert_eq!(new.left.text, "new text");
    }

    #[test]
    fn canceled_open_keeps_document_and_ctrl_s_saves_active_pane() {
        let (mut screen, dir) = disposable_pair("open-save-key");
        screen.left.text = "edited".into();
        let mut dialogs = TestDialogs {
            choices: [SaveChoice::Cancel].into(),
            ..Default::default()
        };
        screen.open_side(true, dir.join("right.txt"), &mut dialogs);
        assert_eq!(screen.left.text, "edited");
        assert_eq!(
            screen.left.path.as_deref(),
            Some(dir.join("left.txt").as_path())
        );
        screen.compare();
        let ctx = Context::default();
        let mut icons = IconManager::new();
        render_frame(
            &ctx,
            &mut screen,
            &mut icons,
            vec![Event::Key {
                key: Key::S,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::CTRL,
            }],
        );
        assert!(!screen.left.dirty());
        assert_eq!(
            std::fs::read_to_string(dir.join("left.txt")).unwrap(),
            "edited"
        );
        remove_pair(&dir);
    }

    #[test]
    #[ignore = "manual replay with the user's read-only sample documents"]
    fn profile_edit_latency() {
        let ctx = Context::default();
        super::super::theme::apply(&ctx);
        let mut icons = IconManager::new();
        let mut screen = TextCompareScreen::new();
        screen.load_files(
            "C:/Users/Altieres/Desktop/resumo.txt",
            "C:/Users/Altieres/Desktop/resumo 1.txt",
        );
        assert!(screen.error.is_none());
        screen.filter_mode = FilterMode::Differences;
        for _ in 0..3 {
            render_frame(&ctx, &mut screen, &mut icons, vec![]);
        }
        focus_at(
            &ctx,
            &screen,
            true,
            screen
                .left
                .text
                .split_inclusive('\n')
                .take(9)
                .map(|s| s.chars().count())
                .sum(),
        );
        render_frame(&ctx, &mut screen, &mut icons, vec![]);
        let mut frames = Vec::new();
        for _ in 0..12 {
            let start = Instant::now();
            render_frame(&ctx, &mut screen, &mut icons, vec![Event::Text("x".into())]);
            frames.push(start.elapsed().as_secs_f64() * 1000.0);
        }
        println!(
            "{}",
            serde_json::json!({"version": env!("CARGO_PKG_VERSION"), "metric": "egui input/layout frame, no GPU submission",
            "file_bytes": [screen.left.text.len(), screen.right.text.len()], "keypresses": frames.len(),
            "mean_ms": frames.iter().sum::<f64>() / frames.len() as f64, "max_ms": frames.iter().copied().fold(0.0_f64, f64::max), "frames_ms": frames})
        );
    }

    fn render_frame(
        ctx: &Context,
        screen: &mut TextCompareScreen,
        icons: &mut IconManager,
        events: Vec<Event>,
    ) -> FullOutput {
        let input = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(960.0, 700.0))),
            events,
            ..Default::default()
        };
        ctx.run(input, |ctx| {
            CentralPanel::default().show(ctx, |ui| {
                screen.ui(
                    ctx,
                    ui,
                    icons,
                    &mut super::super::dialogs::TestDialogs::default(),
                );
            });
        })
    }

    fn focus_at(ctx: &Context, screen: &TextCompareScreen, left: bool, index: usize) {
        let id = screen.editor_cache[usize::from(!left)].widget_id.unwrap();
        ctx.memory_mut(|memory| memory.request_focus(id));
        let mut state = TextEdit::load_state(ctx, id).unwrap();
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::one(
                egui::text::CCursor::new(index),
            )));
        state.store(ctx, id);
    }

    #[test]
    fn rules_options_format_and_line_copy_keep_real_buffers_and_dirty_state() {
        let ctx = Context::default();
        super::super::theme::apply(&ctx);
        let mut screen = TextCompareScreen::new();
        let mut icons = IconManager::new();
        screen.left.text = "AÇÃO  teste\nigual\nesquerda\nsegunda\n".into();
        screen.right.text = "ação teste\nigual\ndireita\noutra\n".into();
        screen.compare();
        let original = (screen.left.text.clone(), screen.right.text.clone());
        let mut dialogs = super::super::dialogs::TestDialogs::default();
        dialogs.field_answers.push_back(Some(vec![
            DialogField::Check("".into(), true),
            DialogField::Check("".into(), true),
        ]));
        screen.rules_dialog(&mut dialogs);
        assert_eq!(screen.diff_result.lines[0].status, DiffStatus::Same);
        assert_eq!(
            (screen.left.text.clone(), screen.right.text.clone()),
            original
        );
        screen.selected_line_idx = 2;
        screen.copy_line(true);
        assert!(screen.right.text.contains("esquerda\noutra"));
        screen.undo_merge();
        assert_eq!(screen.right.text, original.1);
        render_frame(&ctx, &mut screen, &mut icons, vec![]);
        focus_at(&ctx, &screen, true, 0);
        screen.edit_lines(&ctx, 3);
        assert!(screen.left.text.starts_with("    AÇÃO"));
        screen.undo_merge();
        screen.active_left = true;
        dialogs.field_answers.push_back(Some(vec![
            DialogField::Choice("".into(), 2, vec![]),
            DialogField::Choice("".into(), 1, vec![]),
        ]));
        screen.format_dialog(&mut dialogs);
        assert_eq!(screen.left.encoding, crate::document::Encoding::Utf16Le);
        assert_eq!(screen.left.line_ending, LineEnding::Lf);
        let before = screen.left.text.clone();
        render_frame(&ctx, &mut screen, &mut icons, vec![Event::Text("X".into())]);
        assert_ne!(screen.left.text, before);
        assert_eq!(
            screen.diff_result,
            DiffEngine::compare_rules(&screen.left.text, &screen.right.text, screen.active_rules())
        );
    }

    #[test]
    fn find_replace_and_go_to_reveal_hidden_unicode_lines_and_preserve_undo() {
        let ctx = Context::default();
        super::super::theme::apply(&ctx);
        let mut icons = IconManager::new();
        let mut screen = TextCompareScreen::new();
        screen.left.text = "cabeçalho\nAÇÃO\nigual\nantigo\nfim\n".into();
        screen.right.text = "cabeçalho\nAÇÃO\nigual\nnovo\nfim\n".into();
        screen.compare();
        screen.filter_mode = FilterMode::Differences;
        render_frame(&ctx, &mut screen, &mut icons, vec![]);
        screen.search.options.query = "ação".into();
        screen.search.sides = [true, false];
        focus_at(&ctx, &screen, true, 0);
        screen.find(&ctx, true);
        assert_eq!(screen.selected_line_idx, 1);
        render_frame(&ctx, &mut screen, &mut icons, vec![]);
        let selection = screen.selected_bytes(&ctx, true);
        assert_eq!(&screen.left.text[selection], "AÇÃO");
        screen.search.replacement = "Texto 🦀".into();
        screen.replace_found(&ctx);
        assert!(screen.left.text.contains("Texto 🦀"));
        screen.undo_merge();
        assert!(screen.left.text.contains("AÇÃO"));
        screen.search.sides = [true, true];
        screen.search.options.regular_expression = true;
        screen.search.options.query = "(AÇÃO)".into();
        screen.search.replacement = "${1}!".into();
        screen.replace_all();
        assert!(screen.left.text.contains("AÇÃO!"));
        assert!(screen.right.text.contains("AÇÃO!"));
        let before = (screen.left.text.clone(), screen.right.text.clone());
        screen.search.options.query = "[".into();
        screen.replace_all();
        assert_eq!(
            (screen.left.text.clone(), screen.right.text.clone()),
            before
        );
        let mut dialogs = super::super::dialogs::TestDialogs::default();
        dialogs.inputs.push_back(Some("2:3".into()));
        screen.go_to(&mut dialogs);
        render_frame(&ctx, &mut screen, &mut icons, vec![]);
        let cursor = screen.selected_bytes(&ctx, true);
        assert_eq!(cursor.start, "cabeçalho\nAÇ".len());
        dialogs.inputs.push_back(Some("0".into()));
        screen.go_to(&mut dialogs);
        assert_eq!(dialogs.errors.len(), 1);
    }

    #[test]
    fn navigation_and_undo_redo_shortcuts_operate_on_the_comparison() {
        let ctx = Context::default();
        super::super::theme::apply(&ctx);
        let mut icons = IconManager::new();
        let mut screen = TextCompareScreen::new();
        screen.left.text = "início\num\nigual\ndois\nfim\n".into();
        screen.right.text = "início\none\nigual\ntwo\nfim\n".into();
        screen.compare();
        assert_eq!(screen.diff_result.sections.len(), 2);
        screen.selected_line_idx = screen.diff_result.sections[1].start;
        let key = |key| Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers {
                ctrl: true,
                command: true,
                ..Modifiers::NONE
            },
        };
        render_frame(&ctx, &mut screen, &mut icons, vec![key(Key::P)]);
        assert_eq!(
            screen.selected_line_idx,
            screen.diff_result.sections[0].start
        );
        render_frame(&ctx, &mut screen, &mut icons, vec![key(Key::N)]);
        assert_eq!(
            screen.selected_line_idx,
            screen.diff_result.sections[1].start
        );
        let original = screen.right.text.clone();
        screen.copy_difference(true);
        let copied = screen.right.text.clone();
        assert_ne!(copied, original);
        focus_at(&ctx, &screen, true, 0);
        render_frame(&ctx, &mut screen, &mut icons, vec![key(Key::Z)]);
        assert_eq!(screen.right.text, original);
        render_frame(&ctx, &mut screen, &mut icons, vec![key(Key::Y)]);
        assert_eq!(screen.right.text, copied);
    }

    #[test]
    fn read_only_pane_selects_and_copies_but_rejects_typing_cut_and_paste() {
        let ctx = Context::default();
        super::super::theme::apply(&ctx);
        let mut icons = IconManager::new();
        let mut screen = TextCompareScreen::new();
        screen.left.text = "ação\ntexto diferente\nfim\n".into();
        screen.right.text = "ação\noutro texto\nfim\n".into();
        screen.compare();
        screen.set_editable(false);
        render_frame(&ctx, &mut screen, &mut icons, vec![]);
        focus_at(&ctx, &screen, true, 0);
        let original = screen.left.text.clone();
        render_frame(
            &ctx,
            &mut screen,
            &mut icons,
            vec![Event::Key {
                key: Key::A,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers {
                    ctrl: true,
                    command: true,
                    ..Modifiers::NONE
                },
            }],
        );
        let output = render_frame(&ctx, &mut screen, &mut icons, vec![Event::Copy]);
        assert_eq!(output.platform_output.copied_text, original);
        render_frame(
            &ctx,
            &mut screen,
            &mut icons,
            vec![
                Event::Text("não inserir".into()),
                Event::Cut,
                Event::Paste("não colar".into()),
            ],
        );
        assert_eq!(screen.left.text, original);
        assert_eq!(screen.diff_result.sections.len(), 1);
        screen.set_editable(true);
        render_frame(
            &ctx,
            &mut screen,
            &mut icons,
            vec![Event::Text("editável".into())],
        );
        assert_ne!(screen.left.text, original);
        screen.undo_merge();
        assert_eq!(screen.left.text, original);
    }

    #[test]
    fn native_aligned_editor_types_pastes_undoes_and_saves_without_phantom_lines() {
        let ctx = Context::default();
        super::super::theme::apply(&ctx);
        let mut icons = IconManager::new();
        let mut screen = TextCompareScreen::new();
        screen.left.text = "ação\nfim\n".into();
        screen.right.text = "ação\ninserido\nfim\n".into();
        screen.compare();
        render_frame(&ctx, &mut screen, &mut icons, vec![]);
        // The visual gap after the first line must never become a buffer newline.
        focus_at(&ctx, &screen, true, "ação\n".chars().count());
        render_frame(
            &ctx,
            &mut screen,
            &mut icons,
            vec![Event::Text("🙂".into())],
        );
        assert_eq!(screen.left.text, "ação\n🙂fim\n");
        assert!(screen
            .diff_result
            .lines
            .iter()
            .any(|line| line.left_text == "🙂fim"));
        focus_at(&ctx, &screen, true, screen.left.text.chars().count());
        render_frame(
            &ctx,
            &mut screen,
            &mut icons,
            vec![Event::Paste("última\nlinha".into())],
        );
        assert_eq!(screen.left.text, "ação\n🙂fim\núltima\nlinha");
        render_frame(
            &ctx,
            &mut screen,
            &mut icons,
            vec![Event::Key {
                key: Key::Z,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::CTRL,
            }],
        );
        assert_eq!(screen.left.text, "ação\n🙂fim\n");
        // Toggle editability without replacing the comparison or document.
        screen.editing = false;
        render_frame(&ctx, &mut screen, &mut icons, vec![]);
        screen.editing = true;
        render_frame(&ctx, &mut screen, &mut icons, vec![]);
        assert_eq!(screen.left.text, "ação\n🙂fim\n");
        let path = std::env::temp_dir().join(format!(
            "anycompare-aligned-edit-{}.txt",
            std::process::id()
        ));
        screen.left.line_ending = LineEnding::Lf;
        screen.left.save(&path).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "ação\n🙂fim\n");
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn native_editor_preserves_leading_gaps_empty_buffers_and_filtered_text() {
        for (left, right) in [
            ("fim", "novo\nfim"),
            ("", "novo\n"),
            ("a\n\nfim\n", "a\nnovo\n\nfim\n"),
        ] {
            for filter in [FilterMode::All, FilterMode::Differences, FilterMode::Same] {
                let ctx = Context::default();
                let mut icons = IconManager::new();
                let mut screen = TextCompareScreen::new();
                screen.left.text = left.into();
                screen.right.text = right.into();
                screen.filter_mode = filter;
                screen.compare();
                render_frame(&ctx, &mut screen, &mut icons, vec![]);
                focus_at(&ctx, &screen, true, left.chars().count());
                render_frame(&ctx, &mut screen, &mut icons, vec![Event::Text("ç".into())]);
                assert_eq!(screen.left.text, format!("{left}ç"));
                assert_eq!(screen.right.text, right);
            }
        }
    }

    #[test]
    fn gap_controls_expand_both_sides_and_allow_editing_real_text() {
        let ctx = Context::default();
        let mut icons = IconManager::new();
        let mut screen = TextCompareScreen::new();
        screen.left.text = "header\nname\nold\ntail\n".into();
        screen.right.text = "header\nname\nnew\ntail\n".into();
        screen.filter_mode = FilterMode::Differences;
        screen.compare();
        render_frame(&ctx, &mut screen, &mut icons, vec![]);
        let left = screen
            .fold_controls
            .iter()
            .find(|(left, range, _)| *left && *range == (0..2))
            .unwrap()
            .2;
        let right = screen
            .fold_controls
            .iter()
            .find(|(left, range, _)| !left && *range == (0..2))
            .unwrap()
            .2;
        assert_eq!(left.top(), right.top());
        let pos = left.left_center() + vec2(6.0, 0.0);
        for pressed in [true, false] {
            render_frame(
                &ctx,
                &mut screen,
                &mut icons,
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
        assert_eq!(screen.revealed, vec![0..2]);
        assert_eq!(screen.left.text, "header\nname\nold\ntail\n");
        render_frame(&ctx, &mut screen, &mut icons, vec![]);
        focus_at(&ctx, &screen, true, 0);
        render_frame(&ctx, &mut screen, &mut icons, vec![Event::Text("ç".into())]);
        assert_eq!(screen.left.text, "çheader\nname\nold\ntail\n");
        assert_eq!(screen.right.text, "header\nname\nnew\ntail\n");
        screen.undo_merge();
        render_frame(&ctx, &mut screen, &mut icons, vec![]);
        let right = screen
            .fold_controls
            .iter()
            .find(|(left, range, _)| !left && *range == (0..2))
            .unwrap()
            .2;
        let pos = right.left_center() + vec2(6.0, 0.0);
        for pressed in [true, false] {
            render_frame(
                &ctx,
                &mut screen,
                &mut icons,
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
        assert!(screen.revealed.is_empty());
    }

    #[test]
    fn filtered_editor_navigation_skips_hidden_lines_and_edits_recompute_sections() {
        let ctx = Context::default();
        let mut icons = IconManager::new();
        let mut screen = TextCompareScreen::new();
        screen.left.text = "a\nb\nsame\nd\n".into();
        screen.right.text = "a\nB\nsame\nD\n".into();
        screen.filter_mode = FilterMode::Differences;
        screen.compare();
        assert_eq!(screen.diff_result.sections.len(), 2);
        render_frame(&ctx, &mut screen, &mut icons, vec![]);
        focus_at(&ctx, &screen, true, 2);
        render_frame(&ctx, &mut screen, &mut icons, vec![]);
        render_frame(
            &ctx,
            &mut screen,
            &mut icons,
            vec![Event::Key {
                key: Key::ArrowDown,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            }],
        );
        assert_eq!(screen.caret[0].0, 4);
        render_frame(
            &ctx,
            &mut screen,
            &mut icons,
            vec![Event::Key {
                key: Key::ArrowUp,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            }],
        );
        assert_eq!(screen.caret[0].0, 2);
        let id = screen.editor_cache[0].widget_id.unwrap();
        let mut state = TextEdit::load_state(&ctx, id).unwrap();
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::two(
                egui::text::CCursor::new(2),
                egui::text::CCursor::new(3),
            )));
        state.store(&ctx, id);
        render_frame(&ctx, &mut screen, &mut icons, vec![Event::Text("B".into())]);
        assert_eq!(screen.left.text, "a\nB\nsame\nd\n");
        assert_eq!(screen.diff_result.sections.len(), 1);
        assert_eq!(screen.diff_result.lines[1].status, DiffStatus::Same);
    }

    #[test]
    fn failed_load_keeps_existing_text_and_reports_error() {
        let mut screen = TextCompareScreen::new();
        screen.left.text = "unsaved".into();
        screen.load_files(
            "Z:/nonexistent-anycompare-left.txt",
            "Z:/nonexistent-anycompare-right.txt",
        );
        assert_eq!(screen.left.text, "unsaved");
        assert!(screen.error.is_some());
    }

    #[test]
    fn merge_and_undo_change_real_document_buffers() {
        let mut screen = TextCompareScreen::new();
        screen.left.text = "a\nnovo\nz\n".into();
        screen.right.text = "a\nantigo\nz\n".into();
        screen.compare();
        screen.selected_line_idx = 1;
        screen.copy_difference(true);
        assert_eq!(screen.left.text, screen.right.text);
        assert!(screen.right.dirty());
        screen.undo_merge();
        assert_eq!(screen.right.text, "a\nantigo\nz\n");
    }

    fn ctrl(key: Key) -> Event {
        Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers {
                ctrl: true,
                command: true,
                ..Modifiers::NONE
            },
        }
    }

    /// Focus one pane and select `chars` (character indices) in it.
    fn select_chars(
        ctx: &Context,
        screen: &mut TextCompareScreen,
        icons: &mut IconManager,
        left: bool,
        chars: Range<usize>,
    ) {
        focus_at(ctx, screen, left, chars.start);
        let id = screen.editor_cache[usize::from(!left)].widget_id.unwrap();
        let mut state = TextEdit::load_state(ctx, id).unwrap();
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::two(
                egui::text::CCursor::new(chars.start),
                egui::text::CCursor::new(chars.end),
            )));
        state.store(ctx, id);
        render_frame(ctx, screen, icons, vec![]);
        assert_eq!(screen.active_left, left);
    }

    fn chars(text: &str) -> usize {
        text.chars().count()
    }

    /// Ctrl+A in the focused pane selects it all and leaves both panes where
    /// they were (egui alone scrolls to the caret at the end of the text).
    fn select_all_in_place(
        ctx: &Context,
        screen: &mut TextCompareScreen,
        icons: &mut IconManager,
        left: bool,
    ) {
        let offset = screen.scroll_offset_y;
        render_frame(ctx, screen, icons, vec![ctrl(Key::A)]);
        for _ in 0..10 {
            render_frame(ctx, screen, icons, vec![]);
            assert_eq!(screen.scroll_offset_y, offset);
        }
        let len = if left {
            screen.left.text.len()
        } else {
            screen.right.text.len()
        };
        assert_eq!(screen.selected_bytes(ctx, left), 0..len);
        assert_eq!(screen.active_left, left);
    }

    #[test]
    fn select_all_keeps_the_view_where_it_is() {
        let ctx = Context::default();
        super::super::theme::apply(&ctx);
        let mut icons = IconManager::new();
        let mut screen = TextCompareScreen::new();
        screen.left.text = (1..=200).map(|n| format!("linha {n}\n")).collect();
        screen.right.text = (1..=200)
            .map(|n| format!("linha {}\n", if n % 50 == 0 { n * 10 } else { n }))
            .collect();
        screen.compare();
        render_frame(&ctx, &mut screen, &mut icons, vec![]);
        focus_at(&ctx, &screen, false, 3);
        render_frame(&ctx, &mut screen, &mut icons, vec![]);
        select_all_in_place(&ctx, &mut screen, &mut icons, false);
        // Also from the middle of the document, in the other pane.
        screen.selected_line_idx = 100;
        screen.scroll_to_selection = true;
        render_frame(&ctx, &mut screen, &mut icons, vec![]);
        focus_at(&ctx, &screen, true, chars(&"linha 1\n".repeat(100)));
        render_frame(&ctx, &mut screen, &mut icons, vec![]);
        assert!(screen.scroll_offset_y > 0.0);
        select_all_in_place(&ctx, &mut screen, &mut icons, true);
        // Edit > Select All uses the same selection for the active pane.
        focus_at(&ctx, &screen, false, 3);
        render_frame(&ctx, &mut screen, &mut icons, vec![]);
        let offset = screen.scroll_offset_y;
        screen.select_all(&ctx, false);
        for _ in 0..10 {
            render_frame(&ctx, &mut screen, &mut icons, vec![]);
        }
        assert_eq!(screen.scroll_offset_y, offset);
        assert_eq!(
            screen.selected_bytes(&ctx, false),
            0..screen.right.text.len()
        );
        // A filtered-out last line without a final newline keeps the selection whole.
        screen.left.text = "a\nb\nc\nd\ne\nf".into();
        screen.right.text = "A\nb\nc\nd\ne\nf".into();
        screen.filter_mode = FilterMode::Differences;
        screen.compare();
        render_frame(&ctx, &mut screen, &mut icons, vec![]);
        focus_at(&ctx, &screen, false, 0);
        render_frame(&ctx, &mut screen, &mut icons, vec![]);
        select_all_in_place(&ctx, &mut screen, &mut icons, false);
    }

    #[test]
    #[ignore = "manual replay with the user's read-only sample documents"]
    fn select_all_and_copy_on_user_samples() {
        for filter in [FilterMode::All, FilterMode::Differences] {
            let ctx = Context::default();
            super::super::theme::apply(&ctx);
            let mut icons = IconManager::new();
            let mut screen = TextCompareScreen::new();
            screen.load_files(
                "C:/Users/Altieres/Desktop/resumo.txt",
                "C:/Users/Altieres/Desktop/resumo 1.txt",
            );
            assert!(screen.error.is_none());
            screen.filter_mode = filter;
            render_frame(&ctx, &mut screen, &mut icons, vec![]);
            focus_at(&ctx, &screen, false, 3);
            render_frame(&ctx, &mut screen, &mut icons, vec![]);
            select_all_in_place(&ctx, &mut screen, &mut icons, false);
            println!(
                "{filter:?}: offset {} after Ctrl+A, {} sections",
                screen.scroll_offset_y,
                screen.diff_result.sections.len()
            );
            // Copy to Left in memory only; nothing is ever saved here.
            let right = screen.right.text.clone();
            render_frame(&ctx, &mut screen, &mut icons, vec![ctrl(Key::L)]);
            assert_eq!(screen.left.text, right);
            assert!(screen.diff_result.sections.is_empty());
            screen.undo_merge();
            assert_ne!(screen.left.text, right);
        }
    }

    #[test]
    fn copy_to_left_after_select_all_on_right_copies_whole_document() {
        let ctx = Context::default();
        super::super::theme::apply(&ctx);
        let mut icons = IconManager::new();
        let mut screen = TextCompareScreen::new();
        // Left-only rows before the first and after the last right line.
        screen.left.text = "só esquerda\nigual\nantigo\nmeio\nfim\nsobra\n".into();
        screen.right.text = "igual\nnovo\nmeio\nfim\ninserida\n".into();
        screen.compare();
        assert_eq!(screen.diff_result.lines[0].right_line_no, None);
        render_frame(&ctx, &mut screen, &mut icons, vec![]);
        focus_at(&ctx, &screen, false, chars("igual\nnovo\n"));
        render_frame(&ctx, &mut screen, &mut icons, vec![]);
        // The caret sits on an equal line: only the selection can enable Copy.
        let caret = screen.selected_line_idx;
        assert_eq!(screen.diff_result.lines[caret].right_text, "meio");
        assert_eq!(screen.diff_result.lines[caret].status, DiffStatus::Same);
        assert!(!screen.has_selection(&ctx, false));
        render_frame(&ctx, &mut screen, &mut icons, vec![ctrl(Key::A)]);
        assert!(screen.has_selection(&ctx, false));
        assert_eq!(
            screen.selected_bytes(&ctx, false),
            0..screen.right.text.len()
        );
        let (left, right) = (screen.left.text.clone(), screen.right.text.clone());
        render_frame(&ctx, &mut screen, &mut icons, vec![ctrl(Key::L)]);
        assert_eq!(screen.left.text, right);
        assert_eq!(screen.right.text, right);
        assert!(screen.diff_result.sections.is_empty());
        screen.undo_merge();
        assert_eq!(screen.left.text, left);
        // The toolbar's Copy (other side) takes the same path.
        screen.copy_selected(&ctx, screen.active_left);
        assert_eq!(screen.left.text, right);
        // Also without a final newline and with an empty line.
        screen.right.text = "x\n\ny".into();
        screen.compare();
        select_chars(&ctx, &mut screen, &mut icons, false, 0..chars("x\n\ny"));
        screen.copy_selected(&ctx, false);
        assert_eq!(screen.left.text, "x\n\ny");
    }

    #[test]
    fn copy_uses_partially_selected_lines_of_the_active_pane() {
        let ctx = Context::default();
        super::super::theme::apply(&ctx);
        let mut icons = IconManager::new();
        let mut screen = TextCompareScreen::new();
        let left = "um\ndois\ntrês\nquatro\ncinco\n";
        let right = "um\ndois!\ninserida\ntrês\nquatro!\ncinco\n";
        screen.left.text = left.into();
        screen.right.text = right.into();
        screen.compare();
        let rows: Vec<_> = screen
            .diff_result
            .lines
            .iter()
            .map(|line| (line.left_line_no, line.right_line_no))
            .collect();
        assert_eq!(
            rows,
            [
                (Some(1), Some(1)),
                (Some(2), Some(2)),
                (None, Some(3)),
                (Some(3), Some(4)),
                (Some(4), Some(5)),
                (Some(5), Some(6)),
            ]
        );
        render_frame(&ctx, &mut screen, &mut icons, vec![]);
        // From inside "dois!" to inside "três": partial lines count whole and
        // the gap row between them is copied too; "quatro" stays.
        let start = chars("um\ndo");
        let end = chars("um\ndois!\ninserida\ntr");
        select_chars(&ctx, &mut screen, &mut icons, false, start..end);
        render_frame(&ctx, &mut screen, &mut icons, vec![ctrl(Key::L)]);
        assert_eq!(
            screen.left.text,
            "um\ndois!\ninserida\ntrês\nquatro\ncinco\n"
        );
        assert_eq!(screen.right.text, right);
        screen.undo_merge();
        assert_eq!(screen.left.text, left);
        // A selection ending right after a newline does not take the next line.
        let end = chars("um\ndois!\ninserida\ntrês\n");
        select_chars(&ctx, &mut screen, &mut icons, false, start..end);
        render_frame(&ctx, &mut screen, &mut icons, vec![ctrl(Key::L)]);
        assert_eq!(
            screen.left.text,
            "um\ndois!\ninserida\ntrês\nquatro\ncinco\n"
        );
        screen.undo_merge();
        // The left pane's selection defines the rows in both directions.
        let quatro = chars("um\ndois\ntrês\nqu");
        select_chars(&ctx, &mut screen, &mut icons, true, quatro..quatro + 2);
        render_frame(&ctx, &mut screen, &mut icons, vec![ctrl(Key::L)]);
        assert_eq!(screen.left.text, "um\ndois\ntrês\nquatro!\ncinco\n");
        screen.undo_merge();
        render_frame(&ctx, &mut screen, &mut icons, vec![ctrl(Key::R)]);
        assert_eq!(
            screen.right.text,
            "um\ndois!\ninserida\ntrês\nquatro\ncinco\n"
        );
        assert_eq!(screen.left.text, left);
        screen.undo_merge();
        // Selecting only equal lines leaves the buffers and the undo history alone.
        let undo = screen.undo.len();
        select_chars(&ctx, &mut screen, &mut icons, true, 0..2);
        render_frame(&ctx, &mut screen, &mut icons, vec![ctrl(Key::L)]);
        assert_eq!(
            (screen.left.text.as_str(), screen.right.text.as_str()),
            (left, right)
        );
        assert_eq!(screen.undo.len(), undo);
    }

    #[test]
    fn copy_without_selection_still_copies_the_current_section() {
        let ctx = Context::default();
        super::super::theme::apply(&ctx);
        let mut icons = IconManager::new();
        let mut screen = TextCompareScreen::new();
        let left = "a\nx\nmeio\ny\nfim\n";
        let right = "a\nX\nmeio\nY\nfim\n";
        screen.left.text = left.into();
        screen.right.text = right.into();
        screen.compare();
        assert_eq!(screen.diff_result.sections.len(), 2);
        render_frame(&ctx, &mut screen, &mut icons, vec![]);
        // An empty selection (caret only) on the second section.
        let caret = chars("a\nX\nmeio\nY");
        select_chars(&ctx, &mut screen, &mut icons, false, caret..caret);
        assert_eq!(
            screen.selected_line_idx,
            screen.diff_result.sections[1].start
        );
        assert!(!screen.has_selection(&ctx, false));
        render_frame(&ctx, &mut screen, &mut icons, vec![ctrl(Key::L)]);
        assert_eq!(screen.left.text, "a\nx\nmeio\nY\nfim\n");
        screen.undo_merge();
        assert_eq!(screen.left.text, left);
        // Caret on an equal line without selection: nothing to copy.
        select_chars(&ctx, &mut screen, &mut icons, false, 0..0);
        let undo = screen.undo.len();
        render_frame(&ctx, &mut screen, &mut icons, vec![ctrl(Key::L)]);
        assert_eq!(screen.left.text, left);
        assert_eq!(screen.undo.len(), undo);
    }

    fn click_gutter(
        ctx: &Context,
        screen: &mut TextCompareScreen,
        icons: &mut IconManager,
        hit: Rect,
    ) {
        // The space right of the arrow, where users usually click.
        let pos = pos2(hit.right() - 3.0, hit.center().y);
        for pressed in [true, false] {
            render_frame(
                ctx,
                screen,
                icons,
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
    }

    fn pointer_frames(
        ctx: &Context,
        screen: &mut TextCompareScreen,
        icons: &mut IconManager,
        steps: &[(Pos2, Option<bool>)],
        modifiers: Modifiers,
    ) {
        for &(pos, button) in steps {
            let mut events = vec![Event::PointerMoved(pos)];
            if let Some(pressed) = button {
                events.push(Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers,
                });
            }
            render_frame_timed(ctx, screen, icons, events);
        }
        // One more frame applies the pending selection.
        render_frame_timed(ctx, screen, icons, vec![]);
    }

    /// Separate presses by 1 s, but release within 16 ms: a real click must not
    /// be turned into either a double-click or a long press by the test clock.
    fn render_frame_timed(
        ctx: &Context,
        screen: &mut TextCompareScreen,
        icons: &mut IconManager,
        events: Vec<Event>,
    ) {
        thread_local!(static CLOCK: std::cell::Cell<f64> = const { std::cell::Cell::new(100.0) });
        let time = CLOCK.with(|clock| {
            let pressed = events
                .iter()
                .any(|event| matches!(event, Event::PointerButton { pressed: true, .. }));
            clock.set(clock.get() + if pressed { 1.0 } else { 0.016 });
            clock.get()
        });
        let input = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(960.0, 700.0))),
            events,
            time: Some(time),
            ..Default::default()
        };
        let _ = ctx.run(input, |ctx| {
            CentralPanel::default().show(ctx, |ui| {
                screen.ui(
                    ctx,
                    ui,
                    icons,
                    &mut super::super::dialogs::TestDialogs::default(),
                );
            });
        });
    }

    #[test]
    fn margin_click_selects_whole_lines_and_the_arrow_copies_them() {
        let ctx = Context::default();
        super::super::theme::apply(&ctx);
        let mut icons = IconManager::new();
        let mut screen = TextCompareScreen::new();
        let left = "a\nx1\nx2\nfim\n";
        let right = "a\nX1\nX2\nfim\n";
        screen.left.text = left.into();
        screen.right.text = right.into();
        screen.compare();
        render_frame(&ctx, &mut screen, &mut icons, vec![]);
        let button = screen
            .gutter_arrows
            .iter()
            .find(|(left, ..)| !left)
            .unwrap()
            .2;
        // The user's spot: right of the arrow, still in the margin, one row below.
        let row1 = pos2(button.right() + 8.0, button.center().y);
        let row2 = row1 + vec2(0.0, ROW_HEIGHT);
        pointer_frames(
            &ctx,
            &mut screen,
            &mut icons,
            &[(row2, Some(true)), (row2, Some(false))],
            Modifiers::NONE,
        );
        let x2 = chars("a\nX1\n");
        assert_eq!(screen.selected_bytes(&ctx, false), x2..x2 + 3);
        assert_eq!(screen.left.text, left, "a margin click must not copy");
        // The selection arrow now sits on that row and copies only that line.
        let arrow = screen
            .gutter_arrows
            .iter()
            .find(|(left, ..)| !left)
            .map(|(_, index, rect, kind)| (*index, *rect, *kind))
            .unwrap();
        assert_eq!((arrow.0, arrow.2), (2, GutterArrow::Selection));
        click_gutter(&ctx, &mut screen, &mut icons, arrow.1);
        assert_eq!(screen.left.text, "a\nx1\nX2\nfim\n");
        screen.undo_merge();
        // Dragging in the margin extends the selection by whole lines.
        render_frame(&ctx, &mut screen, &mut icons, vec![]);
        pointer_frames(
            &ctx,
            &mut screen,
            &mut icons,
            &[(row1, Some(true)), (row2, None), (row2, Some(false))],
            Modifiers::NONE,
        );
        let x1 = chars("a\n");
        assert_eq!(screen.selected_bytes(&ctx, false), x1..x1 + 6);
    }

    #[test]
    fn text_clicks_and_drags_after_a_margin_selection_are_not_stuck_in_line_mode() {
        let ctx = Context::default();
        super::super::theme::apply(&ctx);
        let mut icons = IconManager::new();
        let mut screen = TextCompareScreen::new();
        let left = "a\nx1\nx2\nfim do arquivo\n";
        let right = "a\nX1\nX2\nfim do arquivo\n";
        screen.left.text = left.into();
        screen.right.text = right.into();
        screen.compare();
        render_frame(&ctx, &mut screen, &mut icons, vec![]);
        let button = screen
            .gutter_arrows
            .iter()
            .find(|(left, ..)| !left)
            .unwrap()
            .2;
        let row1 = pos2(button.right() + 8.0, button.center().y);
        pointer_frames(
            &ctx,
            &mut screen,
            &mut icons,
            &[(row1, Some(true)), (row1, Some(false))],
            Modifiers::NONE,
        );
        let x1 = chars("a\n");
        assert_eq!(screen.selected_bytes(&ctx, false), x1..x1 + 3);
        // A click in the text two rows below drops the selection: no line mode left.
        let pane_left = button.left() - GUTTER_BUTTON_LEFT;
        let text_row3 = pos2(pane_left + GUTTER_WIDTH + 30.0, row1.y + 2.0 * ROW_HEIGHT);
        pointer_frames(
            &ctx,
            &mut screen,
            &mut icons,
            &[(text_row3, Some(true)), (text_row3, Some(false))],
            Modifiers::NONE,
        );
        assert!(
            screen.selected_bytes(&ctx, false).is_empty(),
            "click in the text must clear the margin selection, got {:?}",
            screen.selected_bytes(&ctx, false)
        );
        // Moving the mouse afterwards (no button) must not select anything.
        pointer_frames(
            &ctx,
            &mut screen,
            &mut icons,
            &[(row1, None), (text_row3, None)],
            Modifiers::NONE,
        );
        assert!(screen.selected_bytes(&ctx, false).is_empty());
        // A drag inside the text selects characters of that line, not whole lines.
        let start = text_row3;
        let end = start + vec2(30.0, 0.0);
        pointer_frames(
            &ctx,
            &mut screen,
            &mut icons,
            &[(start, Some(true)), (end, None), (end, Some(false))],
            Modifiers::NONE,
        );
        let fim = chars("a\nX1\nX2\n");
        let selected = screen.selected_bytes(&ctx, false);
        assert!(
            !selected.is_empty() && selected.start > fim && selected.end < fim + 14,
            "character selection inside the line expected, got {selected:?}"
        );
    }

    #[test]
    fn gutter_arrow_copies_only_the_selected_line_or_the_whole_section() {
        let ctx = Context::default();
        super::super::theme::apply(&ctx);
        let mut icons = IconManager::new();
        let mut screen = TextCompareScreen::new();
        let left = "a\nx1\nx2\nfim\n";
        let right = "a\nX1\nX2\nfim\n";
        screen.left.text = left.into();
        screen.right.text = right.into();
        screen.compare();
        assert_eq!(screen.diff_result.sections.len(), 1);
        render_frame(&ctx, &mut screen, &mut icons, vec![]);
        let arrows = |screen: &TextCompareScreen, side: bool| {
            screen
                .gutter_arrows
                .iter()
                .filter(|(left, ..)| *left == side)
                .map(|(_, index, _, kind)| (*index, *kind))
                .collect::<Vec<_>>()
        };
        // Without selection: one "Copy Section" arrow per difference in each pane.
        assert_eq!(arrows(&screen, true), [(1, GutterArrow::Section)]);
        assert_eq!(arrows(&screen, false), [(1, GutterArrow::Section)]);
        // One line selected on the right: a single "Copy Line(s)" arrow on it.
        let start = chars("a\nX1\nX");
        select_chars(&ctx, &mut screen, &mut icons, false, start..start + 1);
        render_frame(&ctx, &mut screen, &mut icons, vec![]);
        assert_eq!(arrows(&screen, false), [(2, GutterArrow::Selection)]);
        assert_eq!(arrows(&screen, true), [(1, GutterArrow::Section)]);
        let hit = screen
            .gutter_arrows
            .iter()
            .find(|(left, ..)| !left)
            .unwrap()
            .2;
        // Only the arrow frame is the button (REA 0xf034e0: 14 px).
        assert_eq!(hit.width(), 14.0);
        click_gutter(&ctx, &mut screen, &mut icons, hit);
        assert_eq!(screen.left.text, "a\nx1\nX2\nfim\n");
        assert_eq!(screen.right.text, right);
        screen.undo_merge();
        assert_eq!(screen.left.text, left);
        // No selection again: the arrow copies the whole section.
        select_chars(&ctx, &mut screen, &mut icons, false, 0..0);
        render_frame(&ctx, &mut screen, &mut icons, vec![]);
        assert_eq!(arrows(&screen, false), [(1, GutterArrow::Section)]);
        let hit = screen
            .gutter_arrows
            .iter()
            .find(|(left, ..)| !left)
            .unwrap()
            .2;
        click_gutter(&ctx, &mut screen, &mut icons, hit);
        assert_eq!(screen.left.text, right);
    }

    #[test]
    fn gutter_arrow_hints_match_the_original_resource_strings() {
        assert_eq!(GutterArrow::Section.hint(true), "Copiar Seção para Direita");
        assert_eq!(
            GutterArrow::Section.hint(false),
            "Copiar Seção para Esquerda"
        );
        assert_eq!(
            GutterArrow::Selection.hint(true),
            "Copiar Linha(s) para Direita"
        );
        assert_eq!(
            GutterArrow::Selection.hint(false),
            "Copiar Linha(s) para Esquerda"
        );
    }

    #[test]
    fn copying_selected_lines_preserves_crlf_encoding_and_untouched_bytes() {
        let dir = std::env::temp_dir().join(format!(
            "anycompare-copy-selection-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&dir).unwrap();
        let left_path = dir.join("left.txt");
        let right_path = dir.join("right.txt");
        let mut left_bytes = vec![0xef, 0xbb, 0xbf];
        left_bytes.extend("ação\r\nantigo 1\r\nantigo 2\r\nigual\r\nfim\r\n".as_bytes());
        let right_bytes = "ação\r\nnovo 1\r\nnovo 2\r\nigual\r\nFIM\r\n".as_bytes();
        std::fs::write(&left_path, &left_bytes).unwrap();
        std::fs::write(&right_path, right_bytes).unwrap();
        let ctx = Context::default();
        super::super::theme::apply(&ctx);
        let mut icons = IconManager::new();
        let mut screen = TextCompareScreen::new();
        screen.load_files(left_path.to_str().unwrap(), right_path.to_str().unwrap());
        assert_eq!(screen.left.line_ending, LineEnding::CrLf);
        render_frame(&ctx, &mut screen, &mut icons, vec![]);
        // Only the second changed line of the first section ("novo 2").
        let start = chars("ação\nnovo 1\nno");
        select_chars(&ctx, &mut screen, &mut icons, false, start..start + 1);
        render_frame(&ctx, &mut screen, &mut icons, vec![ctrl(Key::L)]);
        assert_eq!(screen.left.text, "ação\nantigo 1\nnovo 2\nigual\nfim\n");
        let mut dialogs = TestDialogs::default();
        assert!(screen.save_side(true, false, &mut dialogs));
        let mut expected = vec![0xef, 0xbb, 0xbf];
        expected.extend("ação\r\nantigo 1\r\nnovo 2\r\nigual\r\nfim\r\n".as_bytes());
        assert_eq!(std::fs::read(&left_path).unwrap(), expected);
        // Whole right document to the left: same lines and CRLF, left keeps its BOM.
        render_frame(&ctx, &mut screen, &mut icons, vec![]);
        focus_at(&ctx, &screen, false, 0);
        render_frame(&ctx, &mut screen, &mut icons, vec![]);
        render_frame(&ctx, &mut screen, &mut icons, vec![ctrl(Key::A)]);
        render_frame(&ctx, &mut screen, &mut icons, vec![ctrl(Key::L)]);
        assert!(screen.save_side(true, false, &mut dialogs));
        let mut expected = vec![0xef, 0xbb, 0xbf];
        expected.extend(right_bytes);
        assert_eq!(std::fs::read(&left_path).unwrap(), expected);
        assert_eq!(std::fs::read(&right_path).unwrap(), right_bytes);
        assert!(dialogs.errors.is_empty());
        remove_pair(&dir);
    }
}
