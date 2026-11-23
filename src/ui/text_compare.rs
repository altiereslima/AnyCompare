use super::aligned_editor::{
    self, EditorCache, LayoutOptions, GUTTER_WIDTH, ROW_HEIGHT, TEXT_OFFSET,
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

type ViewKey = (u64, u64, FilterMode, bool, Vec<std::ops::Range<usize>>);

/// Byte offset of character `index` (the end when past it), skipping whole
/// blocks by counting their non-continuation bytes: every frame converts the
/// editor's character cursors, which on a long file sit megabytes in.
pub(crate) fn char_to_byte(text: &str, index: usize) -> usize {
    const BLOCK: usize = 4096;
    let bytes = text.as_bytes();
    let (mut at, mut seen) = (0, 0);
    while at + BLOCK <= bytes.len() {
        // Extend the block to the next character boundary.
        let mut end = at + BLOCK;
        while end < bytes.len() && bytes[end] & 0xC0 == 0x80 {
            end += 1;
        }
        let count = bytes[at..end].iter().filter(|&&b| b & 0xC0 != 0x80).count();
        if seen + count > index {
            break;
        }
        seen += count;
        at = end;
    }
    text[at..]
        .char_indices()
        .nth(index - seen)
        .map_or(text.len(), |(i, _)| at + i)
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
    undo: Vec<(std::sync::Arc<str>, std::sync::Arc<str>)>,
    redo: Vec<(std::sync::Arc<str>, std::sync::Arc<str>)>,
    context_lines: bool,
    line_numbers: bool,
    word_wrap: bool,
    wrap_cache: aligned_editor::wrap::WrapCache,
    reflow_caret: Option<bool>,
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
    /// Aligned view of the current comparison, rebuilt only when its inputs change.
    view_cache: Option<(ViewKey, std::sync::Arc<TextView>)>,
    /// Bumped whenever `view_cache` is rebuilt; identifies the editor geometry.
    view_generation: u64,
    thumbnail_cache: Option<((u64, usize, u32), Vec<ThumbCell>)>,
    /// Height of the editors' viewport in the last frame (overview marker).
    viewport_height: f32,
    /// Horizontal split ratio between left and right panes (0.5 by default).
    split_ratio: f32,
    /// Buffers at the end of the last edited frame (tagged with `revision`),
    /// the base of the next undo step.
    edit_base: Option<(u64, std::sync::Arc<str>, std::sync::Arc<str>)>,
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
    show_toolbar: bool,
    show_file_info: bool,
    show_ruler: bool,
    show_hex_details: bool,
    show_alignment_details: bool,
    syntax_highlighting: bool,
    over_under: bool,
    display_font_size: f32,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ThumbCell {
    Same,
    Minor,
    Diff,
    LeftOrphanMinor,
    LeftOrphanDiff,
    RightOrphanMinor,
    RightOrphanDiff,
}

impl ThumbCell {
    fn from_line(line: &crate::diff::AlignedLine) -> Self {
        match line.status {
            DiffStatus::Same => Self::Same,
            DiffStatus::Unimportant => Self::Minor,
            DiffStatus::Different => Self::Diff,
            DiffStatus::LeftOrphan => {
                if line.left_text.trim().is_empty() {
                    Self::LeftOrphanMinor
                } else {
                    Self::LeftOrphanDiff
                }
            }
            DiffStatus::RightOrphan => {
                if line.right_text.trim().is_empty() {
                    Self::RightOrphanMinor
                } else {
                    Self::RightOrphanDiff
                }
            }
        }
    }

    fn rank(self) -> u8 {
        match self {
            Self::Same => 0,
            Self::Minor => 1,
            Self::LeftOrphanMinor | Self::RightOrphanMinor => 2,
            Self::LeftOrphanDiff | Self::RightOrphanDiff => 3,
            Self::Diff => 4,
        }
    }
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
            (Self::Section, true) => "text.copy_section_right",
            (Self::Section, false) => "text.copy_section_left",
            (Self::Selection, true) => "text.copy_lines_right",
            (Self::Selection, false) => "text.copy_lines_left",
        }
    }
}

impl TextCompareScreen {
    pub fn session_options(&self) -> serde_json::Value {
        serde_json::json!({"filter":format!("{:?}",self.filter_mode),"editing":self.editing,"word_wrap":self.word_wrap,"context":self.context_lines,"line_numbers":self.line_numbers,"details":self.show_details,"thumbnail":self.show_thumbnail,"whitespace":self.show_whitespace,"filtered_counts":self.show_filtered_counts,"ignore_minor":self.ignore_minor,"ignore_case":self.rules.ignore_case,"ignore_whitespace":self.rules.ignore_whitespace,"rules_enabled":self.rules_enabled})
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
            ("word_wrap", &mut self.word_wrap),
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
            word_wrap: false,
            wrap_cache: Default::default(),
            reflow_caret: None,
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
            view_cache: None,
            view_generation: 0,
            thumbnail_cache: None,
            viewport_height: 0.0,
            split_ratio: 0.5,
            edit_base: None,
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
            show_toolbar: true,
            show_file_info: true,
            show_ruler: false,
            show_hex_details: false,
            show_alignment_details: false,
            syntax_highlighting: true,
            over_under: false,
            display_font_size: aligned_editor::FONT_SIZE,
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

    pub fn active_left(&self) -> bool {
        self.active_left
    }

    pub fn pane_editable(&self, left: bool) -> bool {
        let converted = if left {
            self.left.is_converted()
        } else {
            self.right.is_converted()
        };
        self.editing && !converted
    }

    pub fn rules_draft(&self) -> super::session_settings::TextRulesDraft {
        super::session_settings::TextRulesDraft {
            ignore_case: self.rules.ignore_case,
            ignore_whitespace: self.rules.ignore_whitespace,
            align_method: self.rules.align_method,
            use_closeness: self.rules.use_closeness,
            never_align_diffs: self.rules.never_align_diffs,
            skew_tolerance: self.rules.skew_tolerance,
            editing: self.editing,
            line_numbers: self.line_numbers,
            context_lines: self.context_lines,
            show_details: self.show_details,
            show_thumbnail: self.show_thumbnail,
            show_whitespace: self.show_whitespace,
            word_wrap: self.word_wrap,
        }
    }

    pub fn apply_rules(&mut self, draft: super::session_settings::TextRulesDraft) {
        let diff_changed = self.rules.ignore_case != draft.ignore_case
            || self.rules.ignore_whitespace != draft.ignore_whitespace
            || self.rules.align_method != draft.align_method
            || self.rules.use_closeness != draft.use_closeness
            || self.rules.never_align_diffs != draft.never_align_diffs
            || self.rules.skew_tolerance != draft.skew_tolerance
            || self.ignore_minor;
        self.rules.ignore_case = draft.ignore_case;
        self.rules.ignore_whitespace = draft.ignore_whitespace;
        self.rules.align_method = draft.align_method;
        self.rules.use_closeness = draft.use_closeness;
        self.rules.never_align_diffs = draft.never_align_diffs;
        self.rules.skew_tolerance = draft.skew_tolerance.clamp(1, 50_000);
        self.rules_enabled = true;
        self.ignore_minor = false;
        self.editing = draft.editing;
        self.line_numbers = draft.line_numbers;
        self.context_lines = draft.context_lines;
        self.show_details = draft.show_details;
        self.show_thumbnail = draft.show_thumbnail;
        self.show_whitespace = draft.show_whitespace;
        self.word_wrap = draft.word_wrap;
        if diff_changed {
            self.recompare();
        }
    }

    pub fn sync_preferences(&self, prefs: &mut crate::i18n::AppPreferences) {
        prefs.text_allow_edit = self.editing;
        prefs.text_line_numbers = self.line_numbers;
        prefs.text_show_context = self.context_lines;
        prefs.text_show_details = self.show_details;
        prefs.text_show_thumbnail = self.show_thumbnail;
        prefs.text_show_whitespace = self.show_whitespace;
        prefs.text_word_wrap = self.word_wrap;
    }

    pub fn apply_preferences(&mut self, prefs: &crate::i18n::AppPreferences) {
        self.editing = prefs.text_allow_edit;
        self.line_numbers = prefs.text_line_numbers;
        self.context_lines = prefs.text_show_context;
        self.show_details = prefs.text_show_details;
        self.show_thumbnail = prefs.text_show_thumbnail;
        self.show_whitespace = prefs.text_show_whitespace;
        self.word_wrap = prefs.text_word_wrap;
    }

    pub fn is_side_empty(&self, left: bool) -> bool {
        if left {
            self.left.path.is_none() && self.left.text.is_empty() && self.left_path.is_empty()
        } else {
            self.right.path.is_none() && self.right.text.is_empty() && self.right_path.is_empty()
        }
    }

    pub fn load_files(&mut self, left: &str, right: &str) {
        let started = Instant::now();
        let (res_l, res_r) = std::thread::scope(|scope| {
            let hr = scope.spawn(|| Document::load(Path::new(right)));
            let l = Document::load(Path::new(left));
            (l, hr.join().unwrap())
        });
        match (res_l, res_r) {
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
        let doc = match Document::load(&path) {
            Ok(doc) => doc,
            Err(error) => {
                self.error = Some(error.clone());
                dialogs.error(&error);
                return;
            }
        };
        self.open_loaded_side(left, doc, path, dialogs);
    }

    pub fn open_loaded_side(
        &mut self,
        left: bool,
        doc: Document,
        path: PathBuf,
        dialogs: &mut dyn DialogHost,
    ) {
        if self.confirm_side(left, dialogs) {
            self.commit_document(left, doc, &path);
        }
    }

    fn commit_open(&mut self, left: bool, path: &Path) {
        match Document::load(path) {
            Ok(doc) => self.commit_document(left, doc, path),
            Err(error) => self.error = Some(error),
        }
    }

    fn commit_document(&mut self, left: bool, doc: Document, path: &Path) {
        if left {
            self.left = doc;
            self.left_path = path.to_string_lossy().into();
        } else {
            self.right = doc;
            self.right_path = path.to_string_lossy().into();
        }
        self.active_left = left;
        self.error = None;
        self.undo.clear();
        self.redo.clear();
        self.revealed.clear();
        self.selected_line_idx = 0;
        self.compare();
    }

    pub fn open_clipboard_side(&mut self, left: bool, dialogs: &mut dyn DialogHost) {
        let Some(text) = super::chrome::read_system_clipboard_text() else {
            return;
        };
        if !self.confirm_side(left, dialogs) {
            return;
        }
        let doc = Document::from_clipboard_text(text);
        let label = crate::i18n::tr("file.open_clipboard");
        if left {
            self.left = doc;
            self.left_path = label;
        } else {
            self.right = doc;
            self.right_path = label;
        }
        self.active_left = left;
        self.error = None;
        self.undo.clear();
        self.redo.clear();
        self.revealed.clear();
        self.selected_line_idx = 0;
        self.recompare();
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
        self.accept_comparison_side(result, ms, None);
    }

    fn refresh_max_line_length(&mut self, changed_side: Option<bool>) {
        // Longest line in characters, one pass over the bytes per changed side.
        fn longest(text: &str) -> usize {
            // Same result as `lines().map(|l| l.chars().count()).max()`, which
            // drops a carriage return only right before the line feed.
            let (mut best, mut current, mut previous) = (0, 0, 0u8);
            for &byte in text.as_bytes() {
                if byte == b'\n' {
                    best = best.max(current - usize::from(previous == b'\r'));
                    current = 0;
                } else if byte & 0xC0 != 0x80 {
                    current += 1;
                }
                previous = byte;
            }
            best.max(current)
        }
        let (left, right) = (&self.left.text, &self.right.text);
        match changed_side {
            Some(true) => self.max_line_lengths[0] = longest(left),
            Some(false) => self.max_line_lengths[1] = longest(right),
            None => {
                self.max_line_lengths = if left.len() + right.len() > 1 << 20 {
                    std::thread::scope(|scope| {
                        let other = scope.spawn(|| longest(right));
                        [longest(left), other.join().unwrap()]
                    })
                } else {
                    [longest(left), longest(right)]
                };
            }
        }
    }

    fn accept_comparison_side(&mut self, result: DiffResult, ms: u128, changed_side: Option<bool>) {
        let previous = std::mem::replace(&mut self.diff_result, result);
        // Releasing tens of thousands of shared lines takes milliseconds; it
        // does not need to delay the frame that shows the edit.
        if previous.lines.len() > 10_000 {
            std::thread::spawn(move || drop(previous));
        }
        self.revision = self.revision.wrapping_add(1);
        self.refresh_max_line_length(changed_side);
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
                    DialogField::Notice("text.this_file_contains_unicode_characters_that".into()),
                    DialogField::Choice("common.encoding".into(), 0, encodings.iter().map(|e| e.label().to_owned()).collect()),
                ];
                if !dialogs.fields_with_accept("common.encoding_error", &mut fields, "common.save") {
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
        let left = match &self.edit_base {
            Some((_, l, _)) if l.as_ref() == self.left.text.as_str() => l.clone(),
            _ => std::sync::Arc::from(self.left.text.as_str()),
        };
        let right = match &self.edit_base {
            Some((_, _, r)) if r.as_ref() == self.right.text.as_str() => r.clone(),
            _ => std::sync::Arc::from(self.right.text.as_str()),
        };
        self.undo.push((left, right));
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
        if !self.pane_editable(!to_right) {
            return;
        }
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
        let started = Instant::now();
        let rules = self.active_rules();
        if to_right {
            let old = std::mem::replace(&mut self.right.text, text);
            DiffEngine::update_edit_rules_in_place(
                &old,
                &self.right.text,
                &self.left.text,
                false,
                &mut self.diff_result,
                rules,
            );
        } else {
            let old = std::mem::replace(&mut self.left.text, text);
            DiffEngine::update_edit_rules_in_place(
                &old,
                &self.left.text,
                &self.right.text,
                true,
                &mut self.diff_result,
                rules,
            );
        }
        let ms = started.elapsed().as_millis();
        self.revision = self.revision.wrapping_add(1);
        self.refresh_max_line_length(Some(!to_right));
        self.load_time_ms = ms;
        self.selected_line_idx = self
            .selected_line_idx
            .min(self.diff_result.lines.len().saturating_sub(1));
    }

    fn transform_active(&mut self, transform: impl FnOnce(&str) -> String) {
        if !self.pane_editable(self.active_left) {
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
        if !self.pane_editable(self.active_left) {
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
            self.redo.push((
                std::sync::Arc::from(self.left.text.as_str()),
                std::sync::Arc::from(self.right.text.as_str()),
            ));
            self.left.text = left.as_ref().to_owned();
            self.right.text = right.as_ref().to_owned();
            self.compare();
        }
    }

    fn redo_edit(&mut self) {
        if let Some((left, right)) = self.redo.pop() {
            if self.undo.len() >= 20 {
                self.undo.remove(0);
            }
            self.undo.push((
                std::sync::Arc::from(self.left.text.as_str()),
                std::sync::Arc::from(self.right.text.as_str()),
            ));
            self.left.text = left.as_ref().to_owned();
            self.right.text = right.as_ref().to_owned();
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
        self.max_line_lengths.swap(0, 1);
        self.active_left = !self.active_left;
        self.undo.clear();
        self.redo.clear();
        self.revealed.clear();
        self.diff_result.swap_sides();
        self.revision = self.revision.wrapping_add(1);
        self.load_time_ms = 0;
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
        let byte = |index| char_to_byte(text, index);
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
            self.search.message = Some("text.text_not_found".into());
        }
    }

    fn replace_found(&mut self, ctx: &Context) {
        if !self.pane_editable(self.active_left) {
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
        if !self.editing || (self.left.is_converted() && self.right.is_converted()) {
            return;
        }
        let mut next = [None, None];
        let mut count = 0;
        for (side, result) in next.iter_mut().enumerate() {
            if !self.search.sides[side] || !self.pane_editable(side == 0) {
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
        self.search.message = Some(crate::i18n::tr_fmt(
            "status.replacements",
            &[("count", &count.to_string())],
        ));
    }

    fn go_to(&mut self, dialogs: &mut dyn DialogHost) {
        let current = self.caret[usize::from(!self.active_left)];
        if let Some(value) = dialogs.input(
            "common.go_to",
            "text.line_column",
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
            dialogs.error("text.enter_an_existing_line_and_a_column_greater");
        }
    }

    fn options_dialog(&mut self, dialogs: &mut dyn DialogHost) {
        let mut fields = [
            DialogField::Check("common.allow_text_editing".into(), self.editing),
            DialogField::Check("common.show_line_numbers".into(), self.line_numbers),
            DialogField::Check("common.show_difference_context".into(), self.context_lines),
            DialogField::Check("common.show_thumbnail".into(), self.show_thumbnail),
            DialogField::Check("common.show_text_details".into(), self.show_details),
            DialogField::Check("common.show_spaces_and_tabs".into(), self.show_whitespace),
            crate::i18n::language_field(),
        ];
        if dialogs.fields("text.text_options", &mut fields) {
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
                left: line.left_text.to_string(),
                right: line.right_text.to_string(),
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
            "session.text_compare",
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
                "common.encoding".into(),
                index,
                encodings
                    .map(Encoding::label)
                    .into_iter()
                    .map(str::to_owned)
                    .collect(),
            ),
            DialogField::Choice(
                "text.line_endings".into(),
                LineEnding::ALL
                    .iter()
                    .position(|&ending| ending == document.line_ending)
                    .unwrap(),
                LineEnding::ALL
                    .map(|ending| ending.label().to_owned())
                    .into(),
            ),
        ];
        if dialogs.fields("table.file_format", &mut fields) {
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
                ..self.rules
            }
        } else if self.rules_enabled {
            self.rules
        } else {
            Default::default()
        }
    }

    pub fn set_display_font_size(&mut self, px: f32) {
        let clamped = px.clamp(9.0, 24.0);
        if (self.display_font_size - clamped).abs() > f32::EPSILON {
            self.display_font_size = clamped;
            aligned_editor::set_active_font_size(clamped);
            self.editor_cache = Default::default();
            self.wrap_cache.clear();
            self.view_cache = None;
            self.view_revision = self.view_revision.wrapping_add(1);
        }
    }

    #[cfg(test)]
    pub fn display_font_size(&self) -> f32 {
        self.display_font_size
    }

    #[cfg(test)]
    pub fn set_over_under(&mut self, over_under: bool) {
        self.over_under = over_under;
    }

    #[cfg(test)]
    pub fn is_over_under(&self) -> bool {
        self.over_under
    }

    #[cfg(test)]
    pub fn set_show_ruler(&mut self, show: bool) {
        self.show_ruler = show;
    }

    #[cfg(test)]
    pub fn set_show_hex_details(&mut self, show: bool) {
        self.show_hex_details = show;
    }

    pub fn convert_line_endings(&mut self, ending: LineEnding) {
        let doc = if self.active_left {
            &mut self.left
        } else {
            &mut self.right
        };
        if doc.line_ending != ending {
            doc.line_ending = ending;
            self.revision = self.revision.wrapping_add(1);
        }
    }

    fn compare_selection_to_clipboard(&self, ctx: &Context) {
        let active_line: &str = self
            .diff_result
            .lines
            .get(self.selected_line_idx)
            .map_or("", |l| {
                if self.active_left {
                    l.left_text.as_ref()
                } else {
                    l.right_text.as_ref()
                }
            });
        let range = self.selected_bytes(ctx, self.active_left);
        let text = if self.active_left {
            &self.left.text
        } else {
            &self.right.text
        };
        let selected_text = text
            .get(range)
            .filter(|s| !s.is_empty())
            .unwrap_or(active_line);
        let clip_text = arboard::Clipboard::new()
            .and_then(|mut cb| cb.get_text())
            .unwrap_or_default();
        let same = selected_text == clip_text;
        let rows = vec![
            (
                crate::i18n::tr("common.status"),
                crate::i18n::tr(if same {
                    "common.same"
                } else {
                    "common.differences"
                }),
            ),
            (
                crate::i18n::tr(if self.active_left {
                    "folder.side_left"
                } else {
                    "folder.side_right"
                }),
                selected_text.chars().take(160).collect::<String>(),
            ),
            (
                crate::i18n::tr("menu.file.open_clipboard"),
                clip_text.chars().take(160).collect::<String>(),
            ),
        ];
        super::info_modals::open_info(
            ctx,
            crate::i18n::tr("text.compare_selection_to_clipboard"),
            rows,
        );
    }

    fn open_compare_info_modal(&self, ctx: &Context) {
        let alignment = crate::i18n::tr(self.active_rules().align_method.label());
        let left_lines = self.left.text.lines().count();
        let right_lines = self.right.text.lines().count();
        let diff_lines = self
            .diff_result
            .lines
            .iter()
            .filter(|l| l.status != DiffStatus::Same)
            .count();
        let rows = vec![
            (crate::i18n::tr("folder.side_left"), self.left_path.clone()),
            (crate::i18n::tr("folder.side_right"), self.right_path.clone()),
            (
                crate::i18n::tr("options.alignment_method"),
                alignment,
            ),
            (
                crate::i18n::tr("text.line_endings"),
                format!(
                    "{} / {}",
                    crate::i18n::tr(self.left.line_ending.label()),
                    crate::i18n::tr(self.right.line_ending.label()),
                ),
            ),
            (
                crate::i18n::tr("text.lines"),
                format!("{} / {}", left_lines, right_lines),
            ),
            (
                crate::i18n::tr("common.same"),
                self.diff_result.same_count.to_string(),
            ),
            (
                crate::i18n::tr("common.differences"),
                format!(
                    "{} ({})",
                    self.diff_result.sections.len(),
                    diff_lines
                ),
            ),
        ];
        super::info_modals::open_info(ctx, crate::i18n::tr("text.compare_info"), rows);
    }

    fn comparison_info(&self, dialogs: &mut dyn DialogHost) {
        let alignment = crate::i18n::tr(self.active_rules().align_method.label());
        dialogs.info(&crate::i18n::tr_fmt(
            "text.info_summary_aligned",
            &[
                ("left", &self.left_path),
                ("right", &self.right_path),
                ("alignment", &alignment),
                ("sections", &self.diff_result.sections.len().to_string()),
                ("matching", &self.diff_result.same_count.to_string()),
            ],
        ));
    }

    fn rules_dialog(&mut self, dialogs: &mut dyn DialogHost) {
        use crate::diff::AlignMethod;
        let mut fields = [
            DialogField::Check(
                "common.ignore_character_case".into(),
                self.rules.ignore_case,
            ),
            DialogField::Check(
                "common.ignore_whitespace".into(),
                self.rules.ignore_whitespace,
            ),
            DialogField::Choice(
                "options.alignment_method".into(),
                self.rules.align_method.index(),
                AlignMethod::ALL.map(|m| m.label().to_owned()).into(),
            ),
            DialogField::Check(
                "options.align_similar_lines_closeness".into(),
                self.rules.use_closeness,
            ),
            DialogField::Check(
                "options.never_align_mismatches".into(),
                self.rules.never_align_diffs,
            ),
            DialogField::Text(
                "options.skew_tolerance_lines".into(),
                self.rules.skew_tolerance.to_string(),
            ),
        ];
        if dialogs.fields("common.comparison_rules", &mut fields) {
            if let DialogField::Check(_, value) = fields[0] {
                self.rules.ignore_case = value;
            }
            if let DialogField::Check(_, value) = fields[1] {
                self.rules.ignore_whitespace = value;
            }
            if let DialogField::Choice(_, index, _) = fields[2] {
                self.rules.align_method = AlignMethod::from_index(index);
            }
            if let DialogField::Check(_, value) = fields[3] {
                self.rules.use_closeness = value;
            }
            if let DialogField::Check(_, value) = fields[4] {
                self.rules.never_align_diffs = value;
            }
            if let DialogField::Text(_, ref text) = fields[5] {
                if let Ok(value) = text.trim().parse::<usize>() {
                    self.rules.skew_tolerance = value.clamp(1, 50_000);
                }
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
                        "common.find",
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
                                ("text.regex.tab_char", "\\t"),
                                ("text.any_character", "."),
                                ("text.any_alphanumeric_character_w", "\\w"),
                                ("text.any_non_alphanumeric_character_w", "\\W"),
                                ("text.any_numeric_character_d", "\\d"),
                                ("text.any_non_numeric_character_d", "\\D"),
                                ("text.any_whitespace_character_s", "\\s"),
                                ("text.character_in_set", "[]"),
                                ("text.character_not_in_set", "[^]"),
                                ("text.beginning_of_line", "^"),
                                ("text.end_of_line", "$"),
                                ("text.tagged_expression", "()"),
                                ("text.or", "|"),
                                ("text.n_0_or_more_matches", "*"),
                                ("text.n_1_or_more_matches_plus", "+"),
                                ("text.n_0_or_1_matches", "?"),
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
                    .on_hover_text(crate::i18n::tr("text.insert_regular_expression_element"));
                    paint_search_chevron(ui, regex_menu.rect, false);
                    for (value, icon, hint) in [
                        (
                            &mut self.search.options.regular_expression,
                            "imgFindIsRegExp",
                            "text.regular_expression",
                        ),
                        (
                            &mut self.search.options.case_sensitive,
                            "imgFindCaseSensitive",
                            "text.match_case",
                        ),
                        (
                            &mut self.search.options.whole_word,
                            "imgFindWholeWords",
                            "text.whole_word",
                        ),
                        (&mut self.search.wrap, "imgFindWrap", "text.wrap_search"),
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
                        .add(Button::new(crate::i18n::tr("common.previous")).min_size(vec2(61.0, 23.0)))
                        .clicked();
                    forward |= ui
                        .add(Button::new(crate::i18n::tr("common.next")).min_size(vec2(61.0, 23.0)))
                        .clicked();
                    let advanced = ui
                        .add(Button::new("").min_size(vec2(23.0, 23.0)))
                        .on_hover_text(crate::i18n::tr("text.more_search_options"));
                    paint_search_chevron(ui, advanced.rect, self.search.advanced);
                    if advanced.clicked() {
                        self.search.advanced = !self.search.advanced;
                    }
                });
                if self.search.replace_mode {
                    ui.horizontal(|ui| {
                        ui.label(crate::i18n::tr("text.replace"));
                        ui.add(
                            TextEdit::singleline(&mut self.search.replacement).desired_width(280.0),
                        );
                        replace = ui
                            .add_enabled(self.editing, Button::new(crate::i18n::tr("common.replace")))
                            .clicked();
                        all = ui
                            .add_enabled(
                                self.editing,
                                Button::new(crate::i18n::tr("text.replace_all")),
                            )
                            .clicked();
                    });
                }
                if self.search.advanced || self.search.replace_mode {
                    ui.horizontal_wrapped(|ui| {
                        ui.checkbox(&mut self.search.sides[0], crate::i18n::tr("common.left"));
                        ui.checkbox(&mut self.search.sides[1], crate::i18n::tr("common.right"));
                        ui.checkbox(&mut self.search.replace_mode, crate::i18n::tr("common.replace"));
                        if ui.button(crate::i18n::tr("text.close_search")).clicked() {
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
        aligned_editor::set_active_font_size(self.display_font_size);
        if ctx.input_mut(|i| {
            i.consume_key(Modifiers::CTRL, Key::Plus)
                || i.consume_key(Modifiers::CTRL, Key::Equals)
        }) {
            self.set_display_font_size(self.display_font_size + 1.0);
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::Minus)) {
            self.set_display_font_size(self.display_font_size - 1.0);
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::Num0)) {
            self.set_display_font_size(aligned_editor::FONT_SIZE);
        }
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
            if dialogs.persist_settings() {
                self.open_compare_info_modal(ctx);
            } else {
                self.comparison_info(dialogs);
            }
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
        if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL | Modifiers::SHIFT, Key::V)) {
            self.open_clipboard_side(self.active_left, dialogs);
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
                    super::chrome::menu_button(ui, "menu.session", |ui| {
                        super::session_menu::prefix(
                            ui,
                            icons,
                            self.left.path.is_some() && self.right.path.is_some(),
                        );
                        if super::chrome::menu_item(ui, icons, "menu.session.settings", true)
                            .clicked()
                        {
                            if dialogs.persist_settings() {
                                super::session_settings::request(ui.ctx());
                            } else {
                                self.rules_dialog(dialogs);
                            }
                            ui.close_menu();
                        }
                        super::chrome::menu_item(ui, icons, "menu.session.locked", false)
                            .on_disabled_hover_text(crate::i18n::tr(
                                "common.not_available",
                            ));

                        if super::chrome::menu_item(ui, icons, "sync.clear_session", true).clicked() {
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
                        if super::chrome::menu_item(ui, icons, "menu.edit.swap_sides", true).clicked() {
                            self.swap_sides();
                            ui.close_menu();
                        }
                        if super::chrome::menu_item(ui, icons, "common.reload  F5", true).clicked() {
                            self.reload(dialogs);
                            ui.close_menu();
                        }
                        if super::chrome::menu_item(ui, icons, "menu.actions.recompare  Ctrl+F5", true)
                            .clicked()
                        {
                            self.recompare();
                            ui.close_menu();
                        }
                        ui.separator();
                        if super::chrome::menu_item(ui, icons, "text.report", true)
                            .clicked()
                        {
                            self.report(dialogs);
                            ui.close_menu();
                        }
                        if super::chrome::menu_item(
                            ui,
                            icons,
                            "text.compare_info  Ctrl+I",
                            true,
                        )
                        .clicked()
                        {
                            if dialogs.persist_settings() {
                                self.open_compare_info_modal(ctx);
                            } else {
                                self.comparison_info(dialogs);
                            }
                            ui.close_menu();
                        }
                        super::session_menu::suffix(
                            ui,
                            icons,
                            self.left.path.is_some() && self.right.path.is_some(),
                        );
                    });
                    super::chrome::menu_button(ui, "menu.file", |ui| {
                        if super::chrome::menu_item(ui, icons, "menu.file.open_ellipsis  Ctrl+O", true)
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
                        if super::chrome::menu_item(
                            ui,
                            icons,
                            "file.open_clipboard  Shift+Ctrl+V",
                            true,
                        )
                        .clicked()
                        {
                            self.open_clipboard_side(self.active_left, dialogs);
                            ui.close_menu();
                        }
                        let dirty = if self.active_left {
                            self.left.dirty()
                        } else {
                            self.right.dirty()
                        };
                        if super::chrome::menu_item(ui, icons, "menu.file.save  Ctrl+S", dirty)
                            .clicked()
                        {
                            self.save_side(self.active_left, false, dialogs);
                            ui.close_menu();
                        }
                        if super::chrome::menu_item(ui, icons, "menu.file.save_as  F12", true)
                            .clicked()
                        {
                            self.save_side(self.active_left, true, dialogs);
                            ui.close_menu();
                        }
                        ui.separator();
                        if super::chrome::menu_item(ui, icons, "common.reload  F5", true).clicked() {
                            self.reload(dialogs);
                            ui.close_menu();
                        }
                        if super::chrome::menu_item(ui, icons, "menu.actions.recompare  Ctrl+F5", true)
                            .clicked()
                        {
                            self.recompare();
                            ui.close_menu();
                        }
                    });
                    super::chrome::menu_button(ui, "menu.edit", |ui| {
                        super::chrome::submenu_item(ui, icons, "text.lines", |ui, icons| {
                            if super::chrome::menu_item(
                                ui,
                                icons,
                                "text.copy_line_right  Ctrl+Shift+R",
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
                                "text.copy_line_left  Ctrl+Shift+L",
                                true,
                            )
                            .clicked()
                            {
                                self.copy_line(false);
                                ui.close_menu();
                            }
                            if super::chrome::menu_item(ui, icons, "text.select_section  Ctrl+D", true)
                                .clicked()
                            {
                                self.select_section();
                                ui.close_menu();
                            }
                            for (label, operation) in [
                                ("text.delete_line", 0),
                                ("text.insert_line_before  Ctrl+Shift+Enter", 1),
                                ("text.insert_line_after  Ctrl+Enter", 2),
                                ("text.indent  Ctrl+Shift+I", 3),
                                ("text.unindent  Ctrl+Shift+U", 4),
                            ] {
                                if super::chrome::menu_item(ui, icons, label, self.editing)
                                    .clicked()
                                {
                                    self.edit_lines(ctx, operation);
                                    ui.close_menu();
                                }
                            }
                        });
                        super::chrome::submenu_item(ui, icons, "text.convert_file", |ui, icons| {
                            if super::chrome::menu_item(
                                ui,
                                icons,
                                "text.trim_trailing_whitespace",
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
                                "text.tabs_to_spaces",
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
                                "text.leading_spaces_to_tabs",
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
                            ui.separator();
                            super::chrome::submenu_item(
                                ui,
                                icons,
                                "text.line_endings_menu",
                                |ui, icons| {
                                    let active_ending = if self.active_left {
                                        self.left.line_ending
                                    } else {
                                        self.right.line_ending
                                    };
                                    for (label, ending) in [
                                        ("text.to_windows", LineEnding::CrLf),
                                        ("text.to_unix", LineEnding::Lf),
                                        ("text.to_mac", LineEnding::Cr),
                                    ] {
                                        if super::chrome::menu_radio_item(
                                            ui,
                                            icons,
                                            label,
                                            active_ending == ending,
                                            self.editing,
                                        )
                                        .clicked()
                                        {
                                            self.convert_line_endings(ending);
                                            ui.close_menu();
                                        }
                                    }
                                },
                            );
                        });
                        if super::chrome::menu_item(
                            ui,
                            icons,
                            "text.compare_selection_to_clipboard",
                            true,
                        )
                        .clicked()
                        {
                            self.compare_selection_to_clipboard(ctx);
                            ui.close_menu();
                        }
                        let selected = self.has_selection(ctx, self.active_left);
                        for (label, enabled, command) in [
                            (
                                "common.cut  Ctrl+X",
                                self.editing && selected,
                                ViewportCommand::RequestCut,
                            ),
                            ("common.copy  Ctrl+C", selected, ViewportCommand::RequestCopy),
                            ("common.paste  Ctrl+V", self.editing, ViewportCommand::RequestPaste),
                        ] {
                            if super::chrome::menu_item(ui, icons, label, enabled).clicked() {
                                clipboard_command = Some(command);
                                ui.close_menu();
                            }
                        }
                        if super::chrome::menu_item(ui, icons, "menu.edit.select_all  Ctrl+A", true)
                            .clicked()
                        {
                            select_all = true;
                            ui.close_menu();
                        }
                        ui.separator();
                        if super::chrome::menu_item(
                            ui,
                            icons,
                            "text.copy_diff_right  Ctrl+R",
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
                            "text.copy_diff_left  Ctrl+L",
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
                            "common.undo  Ctrl+Z",
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
                            "common.redo  Ctrl+Y",
                            !self.redo.is_empty(),
                        )
                        .clicked()
                        {
                            self.redo_edit();
                            ui.close_menu();
                        }
                    });
                    super::chrome::menu_button(ui, "menu.search", |ui| {
                        if super::chrome::menu_item(ui, icons, "menu.search.find  Ctrl+F", true)
                            .clicked()
                        {
                            self.search.shown = true;
                            self.search.replace_mode = false;
                            self.search.focus_query = true;
                            ui.close_menu();
                        }
                        if super::chrome::menu_item(ui, icons, "menu.search.find_next  F3", true)
                            .clicked()
                        {
                            self.find(ctx, true);
                            ui.close_menu();
                        }
                        if super::chrome::menu_item(ui, icons, "menu.search.find_prev  Shift+F3", true)
                            .clicked()
                        {
                            self.find(ctx, false);
                            ui.close_menu();
                        }
                        if super::chrome::menu_item(ui, icons, "menu.search.replace  Ctrl+H", true)
                            .clicked()
                        {
                            self.search.shown = true;
                            self.search.replace_mode = true;
                            self.search.focus_query = true;
                            ui.close_menu();
                        }
                        if super::chrome::menu_item(ui, icons, "menu.search.goto  Ctrl+G", true).clicked()
                        {
                            self.go_to(dialogs);
                            ui.close_menu();
                        }
                        ui.separator();
                        if super::chrome::menu_item(ui, icons, "menu.search.next_diff  Ctrl+N", true)
                            .clicked()
                        {
                            self.next_diff(true);
                            ui.close_menu();
                        }
                        if super::chrome::menu_item(ui, icons, "menu.search.prev_diff  Ctrl+P", true)
                            .clicked()
                        {
                            self.next_diff(false);
                            ui.close_menu();
                        }
                    });
                    super::chrome::menu_button(ui, "menu.view", |ui| {
                        for (label, mode) in [
                            ("common.all", FilterMode::All),
                            ("common.differences", FilterMode::Differences),
                            ("common.same", FilterMode::Same),
                        ] {
                            if super::chrome::menu_radio_item(
                                ui,
                                icons,
                                label,
                                self.filter_mode == mode,
                                true,
                            )
                            .clicked()
                            {
                                self.filter_mode = mode;
                                ui.close_menu();
                            }
                        }
                        ui.separator();
                        if super::chrome::menu_check_item(
                            ui,
                            icons,
                            "text.edit_texts",
                            self.editing,
                            true,
                        )
                        .clicked()
                        {
                            self.editing = !self.editing;
                            ui.close_menu();
                        }
                        let wrap = super::chrome::menu_check_item(
                            ui,
                            icons,
                            "text.word_wrap",
                            self.word_wrap,
                            true,
                        );
                        #[cfg(test)]
                        ui.data_mut(|d| {
                            d.insert_temp(Id::new("text_compare_wrap_toggle"), wrap.rect)
                        });
                        if wrap.clicked() {
                            self.word_wrap = !self.word_wrap;
                            self.scroll_offset_x = [0.0; 2];
                            // Preserve the actual caret/selection. Request a
                            // scroll to its new visual position after reflow.
                            self.reflow_caret = Some(self.active_left);
                            ui.close_menu();
                        }
                        if super::chrome::menu_check_item(
                            ui,
                            icons,
                            "text.line_numbers",
                            self.line_numbers,
                            true,
                        )
                        .clicked()
                        {
                            self.line_numbers = !self.line_numbers;
                            ui.close_menu();
                        }
                        if super::chrome::menu_check_item(
                            ui,
                            icons,
                            "text.show_context",
                            self.context_lines,
                            true,
                        )
                        .clicked()
                        {
                            self.context_lines = !self.context_lines;
                            ui.close_menu();
                        }
                        if super::chrome::menu_check_item(
                            ui,
                            icons,
                            "text.show_details",
                            self.show_details,
                            true,
                        )
                        .clicked()
                        {
                            self.show_details = !self.show_details;
                            ui.close_menu();
                        }
                        if super::chrome::menu_check_item(
                            ui,
                            icons,
                            "text.thumbnail",
                            self.show_thumbnail,
                            true,
                        )
                        .clicked()
                        {
                            self.show_thumbnail = !self.show_thumbnail;
                            ui.close_menu();
                        }
                        if super::chrome::menu_check_item(
                            ui,
                            icons,
                            "text.visible_whitespace",
                            self.show_whitespace,
                            true,
                        )
                        .clicked()
                        {
                            self.show_whitespace = !self.show_whitespace;
                            ui.close_menu();
                        }
                        if super::chrome::menu_check_item(
                            ui,
                            icons,
                            "text.syntax_highlighting",
                            self.syntax_highlighting,
                            true,
                        )
                        .clicked()
                        {
                            self.syntax_highlighting = !self.syntax_highlighting;
                            ui.close_menu();
                        }
                        ui.separator();
                        if super::chrome::menu_item(
                            ui,
                            icons,
                            "view.increase_font_size  Ctrl++",
                            self.display_font_size < 24.0,
                        )
                        .clicked()
                        {
                            self.set_display_font_size(self.display_font_size + 1.0);
                            ui.close_menu();
                        }
                        if super::chrome::menu_item(
                            ui,
                            icons,
                            "view.decrease_font_size  Ctrl+-",
                            self.display_font_size > 9.0,
                        )
                        .clicked()
                        {
                            self.set_display_font_size(self.display_font_size - 1.0);
                            ui.close_menu();
                        }
                        if super::chrome::menu_item(
                            ui,
                            icons,
                            "view.reset_font_size  Ctrl+0",
                            (self.display_font_size - aligned_editor::FONT_SIZE).abs() > 0.1,
                        )
                        .clicked()
                        {
                            self.set_display_font_size(aligned_editor::FONT_SIZE);
                            ui.close_menu();
                        }
                        ui.separator();
                        if super::chrome::menu_radio_item(
                            ui,
                            icons,
                            "view.side_by_side_layout",
                            !self.over_under,
                            true,
                        )
                        .clicked()
                        {
                            self.over_under = false;
                            ui.close_menu();
                        }
                        if super::chrome::menu_radio_item(
                            ui,
                            icons,
                            "view.over_under_layout",
                            self.over_under,
                            true,
                        )
                        .clicked()
                        {
                            self.over_under = true;
                            ui.close_menu();
                        }
                        ui.separator();
                        if super::chrome::menu_check_item(
                            ui,
                            icons,
                            "text.hex_details",
                            self.show_hex_details,
                            true,
                        )
                        .clicked()
                        {
                            self.show_hex_details = !self.show_hex_details;
                            ui.close_menu();
                        }
                        if super::chrome::menu_check_item(
                            ui,
                            icons,
                            "text.alignment_details",
                            self.show_alignment_details,
                            true,
                        )
                        .clicked()
                        {
                            self.show_alignment_details = !self.show_alignment_details;
                            ui.close_menu();
                        }
                        if super::chrome::menu_check_item(
                            ui,
                            icons,
                            "text.ruler",
                            self.show_ruler,
                            true,
                        )
                        .clicked()
                        {
                            self.show_ruler = !self.show_ruler;
                            ui.close_menu();
                        }
                        if super::chrome::menu_check_item(
                            ui,
                            icons,
                            "view.file_info",
                            self.show_file_info,
                            true,
                        )
                        .clicked()
                        {
                            self.show_file_info = !self.show_file_info;
                            ui.close_menu();
                        }
                        if super::chrome::menu_check_item(
                            ui,
                            icons,
                            "view.toolbar",
                            self.show_toolbar,
                            true,
                        )
                        .clicked()
                        {
                            self.show_toolbar = !self.show_toolbar;
                            ui.close_menu();
                        }
                    });
                    super::chrome::menu_button(ui, "menu.tools", |ui| {
                        if super::chrome::menu_item(ui, icons, "menu.tools.options", true).clicked() {
                            if dialogs.persist_settings() {
                                super::options_dialog::request(
                                    ui.ctx(),
                                    super::options_dialog::OptionsCategory::TextCompare,
                                );
                            } else {
                                self.options_dialog(dialogs);
                            }
                            ui.close_menu();
                        }
                        if super::chrome::menu_item(ui, icons, "text.file_format_ellipsis", true)
                            .clicked()
                        {
                            self.format_dialog(dialogs);
                            ui.close_menu();
                        }
                        if super::chrome::menu_item(ui, icons, "menu.session.rules_ellipsis", true)
                            .clicked()
                        {
                            if dialogs.persist_settings() {
                                super::session_settings::request(ui.ctx());
                            } else {
                                self.rules_dialog(dialogs);
                            }
                            ui.close_menu();
                        }

                        for label in [
                            "menu.tools.file_formats",
                            "menu.tools.profiles",
                            "menu.tools.source_control",
                        ] {
                            super::chrome::menu_item(ui, icons, label, false)
                                .on_disabled_hover_text(crate::i18n::tr(
                                    "common.not_available",
                                ));
                        }
                        super::session_menu::settings_tools(ui, icons, dialogs);
                        ui.separator();
                        let active_doc_path = if self.active_left {
                            self.left.path.as_deref()
                        } else {
                            self.right.path.as_deref()
                        };
                        super::session_menu::utility_tools(
                            ui,
                            icons,
                            dialogs,
                            active_doc_path.and_then(|p| p.parent()),
                            active_doc_path,
                        );
                    });
                    super::chrome::menu_button(ui, "menu.help", |ui| {
                        if super::chrome::menu_item(ui, icons, "menu.help.about", true)
                            .clicked()
                        {
                            super::info_modals::open_about(ui.ctx());
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
        let mut tool_event = None;
        if self.show_toolbar {
            let toolbar_height =
                super::toolbar::height(ui, items, ui.available_width(), &tool_state);
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
        }
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
            Some(ToolEvent::Click("Rules")) => {
                if dialogs.persist_settings() {
                    super::session_settings::request(ctx);
                } else {
                    self.rules_dialog(dialogs);
                }
            }
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
        // The original's overview column starts right under the toolbar, beside
        // the path fields (measured 13/11/2026), so it is laid out before them.
        if self.show_thumbnail {
            SidePanel::left("text_thumbnail")
                .exact_width(44.0)
                .resizable(false)
                .show_separator_line(false)
                .frame(Frame::none().fill(BcTheme::WINDOW_BG))
                .show_inside(ui, |ui| self.thumbnail(ui));
        }
        let paths_left = if self.show_thumbnail { -1.0 } else { 45.0 };
        TopBottomPanel::top("text_paths")
            .show_separator_line(false)
            .frame(Frame::none().inner_margin(Margin {
                left: paths_left,
                right: 2.0,
                top: 2.0,
                bottom: 3.0,
            }))
            .show_inside(ui, |ui| {
                // White line right under the toolbar, across the window (original).
                let inner = ui.max_rect();
                let clip = ui.clip_rect();
                ui.set_clip_rect(Rect::from_min_max(
                    pos2(clip.left().min(inner.left()), clip.top()),
                    clip.max,
                ));
                ui.painter().rect_filled(
                    Rect::from_min_max(
                        pos2(inner.left() - paths_left, inner.top() - 2.0),
                        pos2(inner.right() + 2.0, inner.top() - 1.0),
                    ),
                    0.0,
                    Color32::WHITE,
                );
                let total_w = ui.available_width().max(2.0);
                let splitter_w = super::text_chrome::SPLITTER_WIDTH;
                let left_col_w = (total_w * self.split_ratio)
                    .clamp(80.0, (total_w - 80.0 - splitter_w).max(80.0));
                let right_col_w = (total_w - left_col_w - splitter_w).max(80.0);
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = splitter_w;
                    let col_h = if self.show_file_info { 46.0 } else { 24.0 };
                    for (left, col_w) in [(true, left_col_w), (false, right_col_w)] {
                        ui.allocate_ui_with_layout(
                            vec2(col_w, col_h),
                            Layout::top_down(Align::Min),
                            |column| {
                                column.set_min_width(col_w);
                                column.set_max_width(col_w);
                                column.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = 2.0;
                                    let path = if left {
                                        &mut self.left_path
                                    } else {
                                        &mut self.right_path
                                    };
                                    let title = if left {
                                        "table.left_file"
                                    } else {
                                        "table.right_file"
                                    };
                                    let width = (ui.available_width() - 70.0).max(30.0);
                                    let history = super::history_combo::show_active(
                                        ui,
                                        ("text_path", left),
                                        path,
                                        &self.recent_paths,
                                        width,
                                        title,
                                        left == self.active_left,
                                    );
                                    let response = history.response;
                                    if response.has_focus() {
                                        self.active_left = left;
                                    }
                                    let typed = path.clone();
                                    if let Some(path) = history.selected {
                                        self.open_side(left, PathBuf::from(path), dialogs);
                                    } else if history.submitted && !typed.is_empty() {
                                        self.open_side(left, PathBuf::from(typed.clone()), dialogs);
                                    }
                                    let doc_path = if left {
                                        self.left.path.clone()
                                    } else {
                                        self.right.path.clone()
                                    };
                                    let pending_load = !typed.trim().is_empty()
                                        && doc_path.as_deref().map(|p| p.to_string_lossy())
                                            != Some(std::borrow::Cow::Borrowed(typed.as_str()))
                                        && typed != crate::i18n::tr("file.open_clipboard");
                                    let (_, browse_act) = super::chrome::file_browse_buttons(
                                        ui,
                                        icons,
                                        ("text_browse", left),
                                        pending_load,
                                        false,
                                        false,
                                        true,
                                        doc_path.is_some(),
                                    );
                                    match browse_act {
                                        Some(super::chrome::BrowseMenuAction::LoadTyped) => {
                                            if !typed.trim().is_empty() {
                                                self.open_side(left, PathBuf::from(typed), dialogs);
                                            }
                                        }
                                        Some(
                                            super::chrome::BrowseMenuAction::OpenFileSystem
                                            | super::chrome::BrowseMenuAction::OpenProfile,
                                        ) => {
                                            if let Some(path) =
                                                dialogs.file(false, left, doc_path.as_deref())
                                            {
                                                self.open_side(left, path, dialogs);
                                            }
                                        }
                                        Some(super::chrome::BrowseMenuAction::OpenClipboard) => {
                                            self.open_clipboard_side(left, dialogs);
                                        }
                                        Some(super::chrome::BrowseMenuAction::OpenWithTextEdit) => {
                                            if let Some(p) = doc_path.clone() {
                                                super::session_menu::request(
                                                    ctx,
                                                    super::session_menu::Request::EditText(p),
                                                );
                                            }
                                        }
                                        Some(super::chrome::BrowseMenuAction::OpenWithAssociated) => {
                                            if let Some(p) = doc_path.clone() {
                                                if let Err(err) = dialogs.open_path(&p, false) {
                                                    dialogs.error(&err);
                                                }
                                            }
                                        }
                                        None => {}
                                    }
                                    let dirty = if left {
                                        self.left.dirty()
                                    } else {
                                        self.right.dirty()
                                    };
                                    if let Some(texture) =
                                        icons.get_icon_with_state(ctx, "imgSave", dirty)
                                    {
                                        if ui
                                            .add_enabled(
                                                dirty,
                                                ImageButton::new((texture.id(), vec2(16.0, 16.0)))
                                                    .frame(false),
                                            )
                                            .on_hover_text(crate::i18n::tr("menu.file.save"))
                                            .clicked()
                                        {
                                            self.save_side(left, false, dialogs);
                                        }
                                    }
                                });
                                if self.show_file_info {
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
                                    ui.label(crate::i18n::tr(document.format_label()));
                                    if document.is_converted() {
                                        ui.label(
                                            RichText::new(crate::i18n::tr("doc.converted"))
                                                .size(12.0)
                                                .color(Color32::from_rgb(56, 98, 152)),
                                        );
                                    } else {
                                        ComboBox::from_id_salt(("encoding", left))
                                            .selected_text(crate::i18n::tr(
                                                document.encoding_label(),
                                            ))
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
                                                        crate::i18n::tr("doc.reopen_encoding"),
                                                        |ui| {
                                                            if ui
                                                                .button(crate::i18n::tr(
                                                                    "doc.detect_auto",
                                                                ))
                                                                .clicked()
                                                            {
                                                                reopen = Some((left, None));
                                                                ui.close_menu();
                                                            }
                                                            for encoding in Encoding::ALL {
                                                                if ui
                                                                    .button(crate::i18n::tr(
                                                                        encoding.label(),
                                                                    ))
                                                                    .clicked()
                                                                {
                                                                    reopen = Some((
                                                                        left,
                                                                        Some(encoding),
                                                                    ));
                                                                    ui.close_menu();
                                                                }
                                                            }
                                                        },
                                                    );
                                                }
                                            });
                                        ComboBox::from_id_salt(("line_ending", left))
                                            .selected_text(crate::i18n::tr(
                                                document.line_ending.label(),
                                            ))
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
                                    }
                                });
                                }
                            },
                        );
                    }
                });
                if let Some(message) = self.error.clone() {
                    ui.horizontal_wrapped(|ui| {
                        ui.colored_label(
                            Color32::from_rgb(175, 25, 25),
                            crate::i18n::tr(&(message)),
                        );
                        if ui.small_button(crate::i18n::tr("common.close")).clicked() {
                            self.error = None;
                        }
                    });
                }
            });

        if let Some((left, encoding)) = reopen {
            self.reopen_encoded(left, encoding, dialogs);
        }
        if self.show_details {
            TopBottomPanel::bottom("text_details")
                .exact_height(super::text_chrome::DETAILS_HEIGHT)
                .show_separator_line(false)
                .frame(Frame::none().fill(BcTheme::WINDOW_BG))
                .show_inside(ui, |ui| self.character_details(ui));
        }
        if self.show_hex_details {
            TopBottomPanel::bottom("text_hex_details")
                .exact_height(46.0)
                .show_separator_line(true)
                .frame(Frame::none().fill(Color32::WHITE).inner_margin(Margin::symmetric(8.0, 4.0)))
                .show_inside(ui, |ui| self.hex_details_strip(ui));
        }
        if self.show_alignment_details {
            TopBottomPanel::bottom("text_align_details")
                .exact_height(26.0)
                .show_separator_line(true)
                .frame(Frame::none().fill(Color32::from_rgb(246, 248, 252)).inner_margin(Margin::symmetric(8.0, 4.0)))
                .show_inside(ui, |ui| self.alignment_details_strip(ui));
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
                self.diff_view(ui, icons, dialogs);
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
        let single_side_only = self.is_side_empty(true) ^ self.is_side_empty(false);
        let important = self.diff_result.lines.iter().any(|line| {
            matches!(
                line.status,
                DiffStatus::Different | DiffStatus::LeftOrphan | DiffStatus::RightOrphan
            )
        });
        let result = if single_side_only {
            String::new()
        } else if self.left.same_saved_bytes(&self.right) {
            crate::i18n::tr("common.binary_same")
        } else if important {
            crate::i18n::tr_fmt(
                "status.difference_sections",
                &[("", &self.diff_result.sections.len().to_string())],
            )
        } else if self
            .diff_result
            .lines
            .iter()
            .any(|line| line.status == DiffStatus::Unimportant)
        {
            crate::i18n::tr("table.unimportant_differences")
        } else if self.left.text != self.right.text
            && self.active_rules() != crate::diff::DiffRules::default()
        {
            crate::i18n::tr("text.ignored_differences")
        } else {
            crate::i18n::tr("common.same")
        };
        let mut first = Rect::from_min_max(
            pos2(x, rect.top()),
            pos2((x + 225.0).min(rect.right()), rect.bottom()),
        );
        if !single_side_only {
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
        }
        x += 225.0;
        let row_state = self
            .diff_result
            .lines
            .get(self.selected_line_idx)
            .map(|line| match line.status {
                DiffStatus::Same => "common.same",
                DiffStatus::Different => "table.important_difference",
                DiffStatus::Unimportant => "table.unimportant_difference",
                DiffStatus::LeftOrphan => "text.important_left_orphan",
                DiffStatus::RightOrphan => "text.important_right_orphan",
            })
            .unwrap_or("");
        for (width, value) in [
            (300.0, crate::i18n::tr(row_state)),
            (81.0, crate::i18n::tr("common.insert")),
            (
                rect.width(),
                format!(
                    "{} {}",
                    crate::i18n::tr("table.load_time"),
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
        super::chrome::status_bar_grip(ui.painter(), rect);
    }

    fn character_details(&mut self, ui: &mut Ui) {
        let mut rect = ui.max_rect();
        if self.show_thumbnail {
            rect.min.x -= 1.0;
            let clip = ui.clip_rect();
            ui.set_clip_rect(Rect::from_min_max(pos2(rect.left(), clip.top()), clip.max));
        }
        ui.painter().rect_filled(rect, 0.0, Color32::from_gray(160));
        let Some(line) = self.diff_result.lines.get(self.selected_line_idx) else {
            return;
        };
        let galleys: Vec<_> = [true, false]
            .into_iter()
            .zip([line.left_inline(), line.right_inline()])
            .map(|(side, chunks)| {
                let document = if side { &self.left } else { &self.right };
                let number = if side {
                    line.left_line_no
                } else {
                    line.right_line_no
                };
                // Every line but an unterminated last one ends with a newline
                // (no scan from the start of the buffer on every frame).
                let last = self.diff_result.lines.iter().rev().find_map(|line| {
                    if side {
                        line.left_line_no
                    } else {
                        line.right_line_no
                    }
                });
                let eol = number.is_some_and(|n| Some(n) != last || document.text.ends_with('\n'));
                ui.fonts(|fonts| fonts.layout_job(detail_job(chunks, eol)))
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
            ui.painter().rect_filled(row, 0.0, Color32::WHITE);
            let text = Rect::from_min_max(row.min + vec2(15.0, 0.0), row.max);
            if (if side == 0 {
                line.left_line_no
            } else {
                line.right_line_no
            })
            .is_some()
            {
                let side_text = if side == 0 {
                    line.left_text.as_ref()
                } else {
                    line.right_text.as_ref()
                };
                let bg = match line.status {
                    DiffStatus::Same => Color32::WHITE,
                    DiffStatus::Unimportant => BcTheme::MINOR_BG,
                    DiffStatus::LeftOrphan | DiffStatus::RightOrphan
                        if side_text.trim().is_empty() =>
                    {
                        BcTheme::MINOR_BG
                    }
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

    /// Overview column. Geometry measured on the original at 100% (13/11/2026,
    /// fresh portable profile, 57,108-line pair), in points from the column's
    /// top-left: a white line under the toolbar; a sunken frame (grey 160 top and
    /// left at 9/10, white right and bottom); a 20 px colour strip at x 13..33
    /// from y 11 to 11 px above the bottom; the visible-area marker outlined in
    /// grey 128 from x 6 to 36, at least 3 px tall, inside which differences are
    /// drawn brighter (255,65,65; equal lines 252 → 255); and a blue
    /// (0,120,215) arrow at x 1..5 on the current line.
    /// The frames right of the column (paths, panes, details) start one pixel
    /// under its right edge, as in the original.
    fn thumbnail(&mut self, ui: &mut Ui) {
        let (rect, response) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
        let painter = ui.painter();
        let at =
            |x: f32, y: f32, w: f32, h: f32| Rect::from_min_size(rect.min + vec2(x, y), vec2(w, h));
        let bottom = rect.height();
        painter.rect_filled(at(0.0, 0.0, rect.width(), 1.0), 0.0, Color32::WHITE);
        let strip = Rect::from_min_max(
            rect.min + vec2(13.0, 11.0),
            pos2(rect.left() + 33.0, rect.bottom() - 10.0),
        );
        let shadow = Color32::from_gray(160);
        painter.rect_filled(at(9.0, 10.0, 24.0, 1.0), 0.0, shadow);
        painter.rect_filled(at(9.0, 10.0, 1.0, bottom - 20.0), 0.0, shadow);
        painter.rect_filled(at(33.0, 10.0, 1.0, bottom - 19.0), 0.0, Color32::WHITE);
        painter.rect_filled(at(9.0, bottom - 10.0, 25.0, 1.0), 0.0, Color32::WHITE);
        // Neutral gray background for the empty area below EOF (measured on BC5: rgb(232,232,232)).
        painter.rect_filled(strip, 0.0, Color32::from_gray(232));
        let count = self.diff_result.lines.len();
        if count == 0 {
            return;
        }
        // BC5 never stretches a short file across empty viewport height: each line takes
        // at most 1 logical pixel until the file reaches the full height of the strip.
        let content_h = (count as f32).min(strip.height()).max(1.0);
        let content_strip = Rect::from_min_size(strip.min, vec2(strip.width(), content_h));
        painter.rect_filled(content_strip, 0.0, Color32::from_gray(244));
        let ppp = ui.ctx().pixels_per_point();
        let pixels = (content_h * ppp).round().max(1.0) as usize;
        let key = (self.revision, pixels, ppp.to_bits());
        if self.thumbnail_cache.as_ref().map(|(k, _)| *k) != Some(key) {
            let lines = &self.diff_result.lines;
            let mut cells = Vec::with_capacity(pixels);
            for pixel in 0..pixels {
                let (a, b) = (
                    pixel as f64 / pixels as f64 * count as f64,
                    (pixel + 1) as f64 / pixels as f64 * count as f64,
                );
                let range = if b - a <= 1.0 {
                    let center = ((a + b) / 2.0) as usize;
                    center..center + 1
                } else {
                    a.floor() as usize..(b.ceil() as usize)
                };
                cells.push(
                    lines
                        .get(range.start.min(lines.len())..range.end.min(lines.len()))
                        .unwrap_or_default()
                        .iter()
                        .map(ThumbCell::from_line)
                        .max_by_key(|cell| cell.rank())
                        .unwrap_or(ThumbCell::Same),
                );
            }
            self.thumbnail_cache = Some((key, cells));
        }
        // Visible area, as a fraction of the aligned view, projected onto `content_h`.
        let (first, last) = match &self.view_cache {
            Some((_, view)) if view.height > 0.0 => (
                (self.scroll_offset_y / view.height).clamp(0.0, 1.0),
                ((self.scroll_offset_y + self.viewport_height) / view.height).clamp(0.0, 1.0),
            ),
            _ => (0.0, 1.0),
        };
        let marker_top = (strip.top() - 1.0 + (first * content_h).floor()).round();
        let marker_bottom = (strip.top() + (last * content_h).ceil())
            .max(marker_top + 2.0)
            .round();
        let cells = &self.thumbnail_cache.as_ref().unwrap().1;
        let mid_x = strip.center().x;
        let mut mesh = egui::Mesh::default();
        let mut run = 0;
        while run < cells.len() {
            let y = strip.top() + run as f32 / ppp;
            let inside = y > marker_top && y < marker_bottom;
            let cell = cells[run];
            let mut end = run + 1;
            while end < cells.len() && cells[end] == cell && {
                let y = strip.top() + end as f32 / ppp;
                (y > marker_top && y < marker_bottom) == inside
            } {
                end += 1;
            }
            let y1 = strip.top() + end as f32 / ppp;
            let red = if inside {
                Color32::from_rgb(255, 62, 62)
            } else {
                Color32::from_rgb(248, 108, 108)
            };
            let minor = if inside {
                Color32::from_rgb(95, 85, 228)
            } else {
                Color32::from_rgb(154, 134, 214)
            };
            let empty_half = if inside {
                Color32::WHITE
            } else {
                Color32::from_gray(244)
            };
            match cell {
                ThumbCell::Same => {
                    if inside {
                        mesh.add_colored_rect(
                            Rect::from_min_max(pos2(strip.left(), y), pos2(strip.right(), y1)),
                            Color32::WHITE,
                        );
                    }
                }
                ThumbCell::Diff => {
                    mesh.add_colored_rect(
                        Rect::from_min_max(pos2(strip.left(), y), pos2(strip.right(), y1)),
                        red,
                    );
                }
                ThumbCell::Minor => {
                    mesh.add_colored_rect(
                        Rect::from_min_max(pos2(strip.left(), y), pos2(strip.right(), y1)),
                        minor,
                    );
                }
                ThumbCell::LeftOrphanDiff | ThumbCell::LeftOrphanMinor => {
                    let left_col = if cell == ThumbCell::LeftOrphanDiff {
                        red
                    } else {
                        minor
                    };
                    mesh.add_colored_rect(
                        Rect::from_min_max(pos2(strip.left(), y), pos2(mid_x, y1)),
                        left_col,
                    );
                    mesh.add_colored_rect(
                        Rect::from_min_max(pos2(mid_x, y), pos2(strip.right(), y1)),
                        empty_half,
                    );
                }
                ThumbCell::RightOrphanDiff | ThumbCell::RightOrphanMinor => {
                    let right_col = if cell == ThumbCell::RightOrphanDiff {
                        red
                    } else {
                        minor
                    };
                    mesh.add_colored_rect(
                        Rect::from_min_max(pos2(strip.left(), y), pos2(mid_x, y1)),
                        empty_half,
                    );
                    mesh.add_colored_rect(
                        Rect::from_min_max(pos2(mid_x, y), pos2(strip.right(), y1)),
                        right_col,
                    );
                }
            }
            run = end;
        }
        painter.add(mesh);
        let marker = Color32::from_gray(128);
        let (left, right) = (rect.left() + 6.0, rect.left() + 37.0);
        for y in [marker_top, marker_bottom] {
            painter.rect_filled(
                Rect::from_min_max(pos2(left, y), pos2(right, y + 1.0)),
                0.0,
                marker,
            );
        }
        for x in [left, right - 1.0] {
            painter.rect_filled(
                Rect::from_min_max(pos2(x, marker_top), pos2(x + 1.0, marker_bottom + 1.0)),
                0.0,
                marker,
            );
        }
        // Current line arrow: rows of 1, 2, 3, 4, 3, 2 and 1 px, the widest on the line.
        let selected = self.selected_line_idx.min(count - 1) as f32 / count as f32;
        let center = (strip.top() + (selected * content_h).floor()).round();
        for (row, width) in [1.0, 2.0, 3.0, 4.0, 3.0, 2.0, 1.0].into_iter().enumerate() {
            painter.rect_filled(
                Rect::from_min_size(
                    pos2(rect.left() + 1.0, center - 3.0 + row as f32),
                    vec2(width, 1.0),
                ),
                0.0,
                Color32::from_rgb(0, 120, 215),
            );
        }
        if response.clicked() || response.dragged() {
            if let Some(pos) = response.interact_pointer_pos() {
                self.selected_line_idx =
                    (((pos.y - strip.top()) / content_h).clamp(0.0, 1.0) * count as f32) as usize;
                self.selected_line_idx = self.selected_line_idx.min(count - 1);
                self.scroll_to_selection = true;
            }
        }
    }

    fn hex_details_strip(&self, ui: &mut Ui) {
        let line = self.diff_result.lines.get(self.selected_line_idx);
        let left_bytes = line.map(|l| l.left_text.as_bytes()).unwrap_or(&[]);
        let right_bytes = line.map(|l| l.right_text.as_bytes()).unwrap_or(&[]);
        let mono = FontId::monospace(11.0);
        let max_bytes = 24usize;
        for (tag, bytes, other) in [
            ("L", left_bytes, right_bytes),
            ("R", right_bytes, left_bytes),
        ] {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                ui.label(
                    RichText::new(format!("{tag}:"))
                        .font(mono.clone())
                        .strong()
                        .color(Color32::from_gray(80)),
                );
                for (i, &b) in bytes.iter().take(max_bytes).enumerate() {
                    let diff = other.get(i).copied() != Some(b);
                    ui.label(
                        RichText::new(format!("{b:02X}"))
                            .font(mono.clone())
                            .color(if diff {
                                Color32::from_rgb(205, 25, 25)
                            } else {
                                Color32::from_gray(40)
                            }),
                    );
                }
                if bytes.len() > max_bytes {
                    ui.label(
                        RichText::new(format!("… ({} B)", bytes.len()))
                            .font(mono.clone())
                            .color(Color32::from_gray(110)),
                    );
                }
            });
        }
    }

    fn alignment_details_strip(&self, ui: &mut Ui) {
        let line = self.diff_result.lines.get(self.selected_line_idx);
        let method = crate::i18n::tr(self.active_rules().align_method.label());
        let text = if let Some(l) = line {
            let lno = l
                .left_line_no
                .map(|n| n.to_string())
                .unwrap_or_else(|| "-".into());
            let rno = l
                .right_line_no
                .map(|n| n.to_string())
                .unwrap_or_else(|| "-".into());
            let status = crate::i18n::tr(match l.status {
                DiffStatus::Same => "common.same",
                DiffStatus::Different => "table.important_difference",
                DiffStatus::Unimportant => "table.unimportant_difference",
                DiffStatus::LeftOrphan => "text.important_left_orphan",
                DiffStatus::RightOrphan => "text.important_right_orphan",
            });
            format!(
                "{}: {}  |  L: {}  R: {}  |  {}",
                crate::i18n::tr("text.alignment_details"),
                method,
                lno,
                rno,
                status
            )
        } else {
            format!("{}: {}", crate::i18n::tr("text.alignment_details"), method)
        };
        ui.label(RichText::new(text).size(11.5).color(Color32::from_gray(45)));
    }

    fn paint_column_ruler(ui: &Ui, rect: Rect, scroll_x: f32, char_width: f32) {
        let p = ui.painter().with_clip_rect(rect);
        p.rect_filled(rect, 0.0, Color32::from_rgb(245, 245, 245));
        p.hline(
            rect.x_range(),
            rect.bottom() - 0.5,
            Stroke::new(1.0_f32, Color32::from_gray(195)),
        );
        let cw = char_width.max(4.0);
        let text_start = rect.left() + TEXT_OFFSET;
        let first_col = ((scroll_x / cw).floor() as usize).saturating_sub(1);
        let max_cols = ((rect.width() / cw).ceil() as usize) + 4;
        let tick_stroke = Stroke::new(1.0_f32, Color32::from_gray(135));
        for col in first_col..first_col + max_cols {
            let x = text_start + col as f32 * cw - scroll_x;
            if x < text_start || x > rect.right() {
                continue;
            }
            let one_based = col + 1;
            let tick_h = if one_based % 10 == 0 {
                7.0
            } else if one_based % 5 == 0 {
                5.0
            } else {
                3.0
            };
            p.line_segment(
                [pos2(x, rect.bottom() - tick_h), pos2(x, rect.bottom() - 1.0)],
                tick_stroke,
            );
            if one_based == 1 || one_based % 10 == 0 {
                p.text(
                    pos2(x + 1.0, rect.top() + 1.0),
                    Align2::LEFT_TOP,
                    one_based.to_string(),
                    FontId::monospace(9.5),
                    Color32::from_gray(75),
                );
            }
        }
    }

    fn diff_view(
        &mut self,
        ui: &mut Ui,
        icons: &mut IconManager,
        dialogs: &mut dyn super::dialogs::DialogHost,
    ) {
        let height = ui.available_height().max(1.0);
        let row_height = ROW_HEIGHT;
        if self.previous_filter != self.filter_mode {
            self.revealed.clear();
            self.view_revision = self.view_revision.wrapping_add(1);
            self.previous_filter = self.filter_mode;
        }
        // Whole physical pixels, so the gutter and its icons of the right pane
        // stay on the pixel grid instead of being blended.
        let ppp = ui.ctx().pixels_per_point();
        // The frame starts one pixel under the thumbnail's right edge. The
        // native splitter is 3 px; its left edge is at half the main panel,
        // rather than half the space remaining after subtracting the splitter.
        let frame_rect = Rect::from_min_max(ui.cursor().min - vec2(1.0, 0.0), ui.max_rect().max);
        let (pane_widths, pane_heights) = if self.over_under {
            let min_h = 80.0_f32.min(height * 0.4);
            let max_h = (height - min_h - super::text_chrome::SPLITTER_WIDTH).max(1.0);
            let first_h = ((height * self.split_ratio * ppp).floor() / ppp)
                .clamp(min_h.max(1.0), max_h);
            (
                [frame_rect.width(), frame_rect.width()],
                [
                    first_h,
                    (height - first_h - super::text_chrome::SPLITTER_WIDTH).max(1.0),
                ],
            )
        } else {
            let min_pane = 80.0_f32.min(frame_rect.width() * 0.4);
            let max_first =
                (frame_rect.width() - min_pane - super::text_chrome::SPLITTER_WIDTH).max(1.0);
            let first_width = ((frame_rect.width() * self.split_ratio * ppp).floor() / ppp)
                .clamp(min_pane.max(1.0), max_first);
            (
                [
                    first_width,
                    (frame_rect.width() - first_width - super::text_chrome::SPLITTER_WIDTH)
                        .max(1.0),
                ],
                [height, height],
            )
        };
        let wrap_widths = self.word_wrap.then(|| {
            pane_widths
                .map(|width| (width - super::text_chrome::BAR_WIDTH - 2.0 - TEXT_OFFSET).max(1.0))
        });
        let view: std::sync::Arc<TextView> = if let Some(widths) = wrap_widths {
            self.scroll_offset_x = [0.0; 2];
            let geometry = self
                .wrap_cache
                .get(ui, &self.diff_result, self.revision, widths);
            std::sync::Arc::new(
                TextView::new(
                    &self.diff_result,
                    self.filter_mode,
                    self.context_lines,
                    &self.revealed,
                )
                .with_heights(&geometry.heights),
            )
        } else {
            self.wrap_cache.clear();
            let key: ViewKey = (
                self.revision,
                self.view_revision,
                self.filter_mode,
                self.context_lines,
                self.revealed.clone(),
            );
            match &self.view_cache {
                Some((cached, view)) if *cached == key => view.clone(),
                _ => {
                    let view = std::sync::Arc::new(TextView::new(
                        &self.diff_result,
                        self.filter_mode,
                        self.context_lines,
                        &self.revealed,
                    ));
                    self.view_generation = self.view_generation.wrapping_add(1);
                    self.view_cache = Some((key, view.clone()));
                    view
                }
            }
        };
        let geometry_revision = if wrap_widths.is_some() {
            self.view_revision
        } else {
            self.view_generation
        };
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
        // The undo base is refreshed only when `revision` changes (load, merge,
        // undo...); a plain scroll frame checks one u64 instead of memcmp'ing 2 x N bytes.
        if self.editing {
            let stale = self
                .edit_base
                .as_ref()
                .is_none_or(|(rev, _, _)| *rev != self.revision);
            if stale {
                let prev = self.edit_base.take();
                let left = match &prev {
                    Some((_, l, _)) if l.as_ref() == self.left.text.as_str() => l.clone(),
                    _ => std::sync::Arc::from(self.left.text.as_str()),
                };
                let right = match &prev {
                    Some((_, _, r)) if r.as_ref() == self.right.text.as_str() => r.clone(),
                    _ => std::sync::Arc::from(self.right.text.as_str()),
                };
                self.edit_base = Some((self.revision, left, right));
            }
        } else {
            self.edit_base = None;
        }
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
        let reflow = self.reflow_caret.take();
        if let Some(left) = reflow {
            if let Some(id) = self.editor_cache[usize::from(!left)].widget_id {
                ui.memory_mut(|memory| memory.request_focus(id));
            }
        }
        let scroll_to_match = self
            .pending_selection
            .as_ref()
            .map(|(left, _)| *left)
            .or(reflow);
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
        #[derive(Clone, Copy)]
        enum TextPaneCtx {
            CopyDiff(bool),
            CopyLine(bool),
            Cut,
            Copy,
            Paste,
            DeleteLines,
            SelectAll,
            SelectSection,
            Find,
            NextDiff,
            PrevDiff,
            OpenFile,
            OpenWith(crate::history::SessionKind),
            AssocApp,
            Explore(bool),
        }
        let mut toggle_fold = None;
        let mut pane_ctx: Option<(bool, TextPaneCtx)> = None;
        let left_has_sel = self.has_selection(ui.ctx(), true);
        let right_has_sel = self.has_selection(ui.ctx(), false);
        let left_editable = self.pane_editable(true);
        let right_editable = self.pane_editable(false);
        let left_has_path = self.left.path.is_some();
        let right_has_path = self.right.path.is_some();
        let both_have_paths = left_has_path && right_has_path;
        let panes_layout = if self.over_under {
            Layout::top_down(Align::Min)
        } else {
            Layout::left_to_right(Align::Min)
        };
        panes.with_layout(panes_layout, |ui| {
            ui.spacing_mut().item_spacing = Vec2::ZERO;
            for left in [true, false] {
                let side = if left { 0 } else { 1 };
                let pane_width = pane_widths[side];
                let pane_height = pane_heights[side];
                let ruler_h = if self.show_ruler { 16.0 } else { 0.0 };
                let editor_top_y = 1.0 + ruler_h;
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
                    vec2(pane_width, pane_height),
                    Layout::top_down(Align::Min),
                    |ui| {
                        ui.set_min_width(pane_width);
                        ui.spacing_mut().item_spacing.y = 0.0;
                        let pane_top = ui.cursor().min;
                        let pane_rect =
                            Rect::from_min_size(pane_top, vec2(pane_width, pane_height));
                        let editor_height = (pane_height
                            - super::text_chrome::STATUS_HEIGHT
                            - editor_top_y)
                            .max(1.0);
                        self.viewport_height = editor_height;
                        let editor_width =
                            (pane_width - super::text_chrome::BAR_WIDTH - 2.0).max(1.0);
                        // Reserve the non-scrolling top/left border explicitly.
                        ui.add_space(editor_top_y);
                        if self.show_ruler {
                            let cw = ui.fonts(|fonts| {
                                fonts.glyph_width(&aligned_editor::editor_font(), 'M')
                            });
                            let ruler_rect = Rect::from_min_size(
                                pane_top + vec2(1.0, 1.0),
                                vec2(editor_width, ruler_h),
                            );
                            Self::paint_column_ruler(ui, ruler_rect, self.scroll_offset_x[side], cw);
                        }
                        let editor = ui.scope_builder(
                            UiBuilder::new().max_rect(Rect::from_min_size(
                                pane_top + vec2(1.0, editor_top_y),
                                vec2(editor_width, editor_height),
                            )),
                            |ui| {
                                let char_width = ui.fonts(|fonts| {
                                    fonts.glyph_width(&aligned_editor::editor_font(), 'M')
                                });
                                let content_width = if self.word_wrap {
                                    editor_width.max(TEXT_OFFSET + 1.0)
                                } else {
                                    (self.max_line_lengths[side] as f32 * char_width + TEXT_OFFSET)
                                        .max(editor_width)
                                };
                                ScrollArea::new([!self.word_wrap, true])
                                    .id_salt(("aligned_pane", left))
                                    // Offsets are synchronized explicitly between panes.
                                    // An internal scroll animation fights that synchronization.
                                    .animated(false)
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
                                            let side_has_section_lines = block_start
                                                && self.diff_result.lines[index..]
                                                    .iter()
                                                    .take_while(|l| l.status != DiffStatus::Same)
                                                    .any(|l| {
                                                        if left {
                                                            l.left_line_no.is_some()
                                                        } else {
                                                            l.right_line_no.is_some()
                                                        }
                                                    });
                                            let rect = Rect::from_min_size(
                                                origin + vec2(0.0, view_row.top),
                                                vec2(content_width, view_row.height()),
                                            );
                                            let hit_rect = Rect::from_min_max(
                                                pos2(origin.x + viewport.left(), rect.top()),
                                                pos2(
                                                    origin.x + viewport.left() + GUTTER_WIDTH,
                                                    rect.bottom(),
                                                ),
                                            );
                                            let selection = selection_rows[side].as_ref();
                                            let target_editable = self.pane_editable(!left);
                                            let gutter_arrow = if target_editable {
                                                GutterArrow::for_row(
                                                    index,
                                                    side_has_section_lines,
                                                    selection,
                                                )
                                            } else {
                                                None
                                            };
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
                                                    rect.top() + row_height,
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
                                                view_revision: geometry_revision,
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
                                                wrap_widths,
                                            };
                                            let pane_editable = self.pane_editable(left);
                                            let (text, other) = if left {
                                                (&mut self.left.text, &self.right.text)
                                            } else {
                                                (&mut self.right.text, &self.left.text)
                                            };
                                            let cache = &mut self.editor_cache[side];
                                            // Keep no galley reference across the editor: an edit
                                            // then updates the cached rows in place.
                                            let (leading_y, galley_height) = {
                                                let layout = cache.layout(
                                                    ui,
                                                    text,
                                                    other,
                                                    &self.diff_result,
                                                    options,
                                                    &self.revealed,
                                                    Some(&view),
                                                );
                                                (layout.leading_y, layout.galley.size().y)
                                            };
                                            let mut layout_calls = 0;
                                            let editor_origin =
                                                origin + vec2(TEXT_OFFSET, leading_y);
                                            let editor_rect = Rect::from_min_size(
                                                editor_origin,
                                                vec2(
                                                    content_width - TEXT_OFFSET,
                                                    galley_height.max(row_height),
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
                                                    // TextEdit lays out the unchanged buffer first and
                                                    // again only after each event that mutated it.
                                                    let mut layouter =
                                                        |ui: &Ui, text: &str, _: f32| {
                                                            layout_calls += 1;
                                                            cache
                                                                .layout(
                                                                    ui,
                                                                    text,
                                                                    other,
                                                                    &self.diff_result,
                                                                    LayoutOptions {
                                                                        unchanged: layout_calls
                                                                            == 1,
                                                                        ..options
                                                                    },
                                                                    &self.revealed,
                                                                    Some(&view),
                                                                )
                                                                .galley
                                                        };
                                                    let backwards = ui.input(|i| {
                                                        i.key_pressed(Key::ArrowUp)
                                                            || i.key_pressed(Key::ArrowLeft)
                                                    });
                                                    let mut read_only;
                                                    let editor = if pane_editable {
                                                        TextEdit::multiline(text)
                                                    } else {
                                                        read_only = text.as_str();
                                                        TextEdit::multiline(&mut read_only)
                                                    };
                                                    let mut output = editor
                                                        .id_salt(("aligned_editor", left))
                                                        .font(aligned_editor::editor_font())
                                                        .margin(Vec2::ZERO)
                                                        .frame(false)
                                                        .desired_width(content_width - TEXT_OFFSET)
                                                        .desired_rows(1)
                                                        .layouter(&mut layouter)
                                                        .disable_undoer()
                                                        .show(ui);
                                                    if cache.paints_rows() {
                                                        let (diff, painted_view) =
                                                            cache.painted(&self.diff_result, &view);
                                                        aligned_editor::paint_rows(
                                                            ui,
                                                            ui.painter(),
                                                            output.galley_pos
                                                                - vec2(0.0, leading_y),
                                                            diff,
                                                            painted_view,
                                                            left,
                                                            self.show_whitespace,
                                                        );
                                                    }
                                                    cache.widget_id = Some(output.response.id);
                                                    cache.widget_rect = Some(output.response.rect);
                                                    if left {
                                                        ui.ctx().data_mut(|d| {
                                                            d.insert_temp(
                                                                Id::new("capture_menu_rect")
                                                                    .with("text_pane_left"),
                                                                output.response.rect,
                                                            );
                                                        });
                                                    }
                                                    if output.response.secondary_clicked() {
                                                        self.active_left = left;
                                                    }
                                                    let this_editable = if left {
                                                        left_editable
                                                    } else {
                                                        right_editable
                                                    };
                                                    let other_editable = if left {
                                                        right_editable
                                                    } else {
                                                        left_editable
                                                    };
                                                    let this_sel = if left {
                                                        left_has_sel
                                                    } else {
                                                        right_has_sel
                                                    };
                                                    let this_path = if left {
                                                        left_has_path
                                                    } else {
                                                        right_has_path
                                                    };
                                                    output.response.context_menu(|ui| {
                                                        ui.set_min_width(220.0);
                                                        ui.spacing_mut().item_spacing.y = 0.0;
                                                        ui.spacing_mut().button_padding =
                                                            vec2(4.0, 2.0);
                                                        ui.spacing_mut().interact_size.y = 20.0;
                                                        let (copy_diff_lbl, copy_line_lbl) = if left
                                                        {
                                                            (
                                                                "text.copy_diff_right  Ctrl+R",
                                                                "text.copy_line_right  Ctrl+Shift+R",
                                                            )
                                                        } else {
                                                            (
                                                                "text.copy_diff_left  Ctrl+L",
                                                                "text.copy_line_left  Ctrl+Shift+L",
                                                            )
                                                        };
                                                        if super::chrome::menu_item(
                                                            ui,
                                                            icons,
                                                            copy_diff_lbl,
                                                            other_editable,
                                                        )
                                                        .clicked()
                                                        {
                                                            pane_ctx = Some((
                                                                left,
                                                                TextPaneCtx::CopyDiff(left),
                                                            ));
                                                            ui.close_menu();
                                                        }
                                                        if super::chrome::menu_item(
                                                            ui,
                                                            icons,
                                                            copy_line_lbl,
                                                            other_editable,
                                                        )
                                                        .clicked()
                                                        {
                                                            pane_ctx = Some((
                                                                left,
                                                                TextPaneCtx::CopyLine(left),
                                                            ));
                                                            ui.close_menu();
                                                        }
                                                        ui.separator();
                                                        if super::chrome::menu_item(
                                                            ui,
                                                            icons,
                                                            "common.cut  Ctrl+X",
                                                            this_editable && this_sel,
                                                        )
                                                        .clicked()
                                                        {
                                                            pane_ctx =
                                                                Some((left, TextPaneCtx::Cut));
                                                            ui.close_menu();
                                                        }
                                                        if super::chrome::menu_item(
                                                            ui,
                                                            icons,
                                                            "common.copy  Ctrl+C",
                                                            this_sel,
                                                        )
                                                        .clicked()
                                                        {
                                                            pane_ctx =
                                                                Some((left, TextPaneCtx::Copy));
                                                            ui.close_menu();
                                                        }
                                                        if super::chrome::menu_item(
                                                            ui,
                                                            icons,
                                                            "common.paste  Ctrl+V",
                                                            this_editable,
                                                        )
                                                        .clicked()
                                                        {
                                                            pane_ctx =
                                                                Some((left, TextPaneCtx::Paste));
                                                            ui.close_menu();
                                                        }
                                                        if super::chrome::menu_item(
                                                            ui,
                                                            icons,
                                                            "text.delete_lines",
                                                            this_editable,
                                                        )
                                                        .clicked()
                                                        {
                                                            pane_ctx = Some((
                                                                left,
                                                                TextPaneCtx::DeleteLines,
                                                            ));
                                                            ui.close_menu();
                                                        }
                                                        if super::chrome::menu_item(
                                                            ui,
                                                            icons,
                                                            "menu.edit.select_all  Ctrl+A",
                                                            true,
                                                        )
                                                        .clicked()
                                                        {
                                                            pane_ctx = Some((
                                                                left,
                                                                TextPaneCtx::SelectAll,
                                                            ));
                                                            ui.close_menu();
                                                        }
                                                        if super::chrome::menu_item(
                                                            ui,
                                                            icons,
                                                            "text.ctx.select_section  Ctrl+D",
                                                            true,
                                                        )
                                                        .clicked()
                                                        {
                                                            pane_ctx = Some((
                                                                left,
                                                                TextPaneCtx::SelectSection,
                                                            ));
                                                            ui.close_menu();
                                                        }
                                                        ui.separator();
                                                        if super::chrome::menu_item(
                                                            ui,
                                                            icons,
                                                            "menu.search.find  Ctrl+F",
                                                            true,
                                                        )
                                                        .clicked()
                                                        {
                                                            pane_ctx =
                                                                Some((left, TextPaneCtx::Find));
                                                            ui.close_menu();
                                                        }
                                                        if super::chrome::menu_item(
                                                            ui,
                                                            icons,
                                                            "menu.search.next_diff  Ctrl+N",
                                                            true,
                                                        )
                                                        .clicked()
                                                        {
                                                            pane_ctx =
                                                                Some((left, TextPaneCtx::NextDiff));
                                                            ui.close_menu();
                                                        }
                                                        if super::chrome::menu_item(
                                                            ui,
                                                            icons,
                                                            "menu.search.prev_diff  Ctrl+P",
                                                            true,
                                                        )
                                                        .clicked()
                                                        {
                                                            pane_ctx =
                                                                Some((left, TextPaneCtx::PrevDiff));
                                                            ui.close_menu();
                                                        }
                                                        ui.separator();
                                                        if super::chrome::menu_item(
                                                            ui,
                                                            icons,
                                                            "menu.file.open_ellipsis  Ctrl+O",
                                                            true,
                                                        )
                                                        .clicked()
                                                        {
                                                            pane_ctx =
                                                                Some((left, TextPaneCtx::OpenFile));
                                                            ui.close_menu();
                                                        }
                                                        super::chrome::submenu_item(
                                                            ui,
                                                            icons,
                                                            "menu.actions.open_with",
                                                            |ui, icons| {
                                                                use crate::history::SessionKind;
                                                                for (lbl, kind, ok) in [
                                                                    (
                                                                        "session.hex_compare",
                                                                        SessionKind::Hex,
                                                                        both_have_paths,
                                                                    ),
                                                                    (
                                                                        "session.table_compare",
                                                                        SessionKind::Table,
                                                                        both_have_paths,
                                                                    ),
                                                                    (
                                                                        "session.version_compare",
                                                                        SessionKind::Version,
                                                                        both_have_paths,
                                                                    ),
                                                                    (
                                                                        "session.text_edit",
                                                                        SessionKind::TextEdit,
                                                                        this_path,
                                                                    ),
                                                                ] {
                                                                    if super::chrome::menu_item(
                                                                        ui, icons, lbl, ok,
                                                                    )
                                                                    .clicked()
                                                                    {
                                                                        pane_ctx = Some((
                                                                            left,
                                                                            TextPaneCtx::OpenWith(
                                                                                kind,
                                                                            ),
                                                                        ));
                                                                        ui.close_menu();
                                                                    }
                                                                }
                                                                ui.separator();
                                                                if super::chrome::menu_item(
                                                                    ui,
                                                                    icons,
                                                                    "folder.ctx.associated_app",
                                                                    this_path,
                                                                )
                                                                .clicked()
                                                                {
                                                                    pane_ctx = Some((
                                                                        left,
                                                                        TextPaneCtx::AssocApp,
                                                                    ));
                                                                    ui.close_menu();
                                                                }
                                                            },
                                                        );
                                                        super::chrome::submenu_item(
                                                            ui,
                                                            icons,
                                                            "table.explorer",
                                                            |ui, icons| {
                                                                if super::chrome::menu_item(
                                                                    ui,
                                                                    icons,
                                                                    "version.select_in_explorer",
                                                                    this_path,
                                                                )
                                                                .clicked()
                                                                {
                                                                    pane_ctx = Some((
                                                                        left,
                                                                        TextPaneCtx::Explore(true),
                                                                    ));
                                                                    ui.close_menu();
                                                                }
                                                                if super::chrome::menu_item(
                                                                    ui,
                                                                    icons,
                                                                    "version.open_folder",
                                                                    this_path,
                                                                )
                                                                .clicked()
                                                                {
                                                                    pane_ctx = Some((
                                                                        left,
                                                                        TextPaneCtx::Explore(false),
                                                                    ));
                                                                    ui.close_menu();
                                                                }
                                                            },
                                                        );
                                                    });
                                                    if self.show_whitespace && !cache.paints_rows()
                                                    {
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
                                                                        aligned_editor::editor_font(
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
                                                            if scroll_to_match == Some(left) {
                                                                ui.scroll_to_rect(
                                                                    output
                                                                        .galley
                                                                        .pos_from_cursor(
                                                                            &range.primary,
                                                                        )
                                                                        .translate(
                                                                            output
                                                                                .galley_pos
                                                                                .to_vec2(),
                                                                        ),
                                                                    Some(Align::Center),
                                                                );
                                                            }
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
                                                                range.primary.pcursor.paragraph + 1,
                                                                range.primary.pcursor.offset + 1,
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
                                                    ("text.collapse_filtered_gap", expanded),
                                                    ("text.expand_filtered_gap", !expanded),
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
                                                        "text.show_filtered_line_counts",
                                                    ),
                                                );
                                            });
                                            #[cfg(test)]
                                            self.fold_controls.push((left, range.clone(), hit));
                                            let fold_verb = crate::i18n::tr(if expanded {
                                                "toolbar.collapse"
                                            } else {
                                                "toolbar.expand"
                                            });
                                            if response
                                                .on_hover_text(crate::i18n::tr_fmt(
                                                    "status.lines",
                                                    &[("", &fold_verb), ("", &range.len().to_string())],
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
                            pane_top + vec2(0.0, editor_top_y + editor_height),
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
                            crate::i18n::tr("doc.format.plain_text")
                        } else {
                            String::new()
                        };
                        let dirty = if left {
                            self.left.dirty()
                        } else {
                            self.right.dirty()
                        };
                        let state = if !self.pane_editable(left) {
                            crate::i18n::tr("common.editing_disabled")
                        } else if dirty {
                            crate::i18n::tr("status.modified")
                        } else {
                            String::new()
                        };
                        let horizontal = super::text_chrome::pane_status_with_state(
                            ui,
                            status_rect,
                            &position,
                            &format,
                            &state,
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
                            pane_top + vec2(editor_width + 2.0, editor_top_y),
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
                                + vec2(
                                    editor_width + 2.0,
                                    editor_top_y + (editor_height - 54.0).max(0.0),
                                ),
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
                if left {
                    if self.over_under {
                        let (split_rect, split_resp) = ui.allocate_exact_size(
                            vec2(frame_rect.width(), super::text_chrome::SPLITTER_WIDTH),
                            Sense::click_and_drag(),
                        );
                        if split_resp.hovered() || split_resp.dragged() {
                            ui.ctx().set_cursor_icon(CursorIcon::ResizeVertical);
                            ui.painter()
                                .rect_filled(split_rect, 0.0, Color32::from_gray(205));
                        }
                        if split_resp.dragged() && height > 20.0 {
                            let delta = split_resp.drag_delta().y;
                            self.split_ratio = ((self.split_ratio * height + delta) / height)
                                .clamp(0.15, 0.85);
                        }
                        if split_resp.double_clicked() {
                            self.split_ratio = 0.5;
                        }
                    } else {
                        let (split_rect, split_resp) = ui.allocate_exact_size(
                            vec2(super::text_chrome::SPLITTER_WIDTH, height),
                            Sense::click_and_drag(),
                        );
                        if split_resp.hovered() || split_resp.dragged() {
                            ui.ctx().set_cursor_icon(CursorIcon::ResizeHorizontal);
                            ui.painter()
                                .rect_filled(split_rect, 0.0, Color32::from_gray(205));
                        }
                        if split_resp.dragged() && frame_rect.width() > 20.0 {
                            let delta = split_resp.drag_delta().x;
                            self.split_ratio = ((self.split_ratio * frame_rect.width() + delta)
                                / frame_rect.width())
                            .clamp(0.15, 0.85);
                        }
                        if split_resp.double_clicked() {
                            self.split_ratio = 0.5;
                        }
                    }
                }
            }
        });
        if let Some(range) = toggle_fold {
            text_view::toggle(&mut self.revealed, range);
            self.view_revision = self.view_revision.wrapping_add(1);
            ui.ctx().request_repaint();
        }
        if let Some((side_left, cmd)) = pane_ctx {
            self.active_left = side_left;
            let ctx = ui.ctx();
            match cmd {
                TextPaneCtx::CopyDiff(to_right) => self.copy_selected(ctx, to_right),
                TextPaneCtx::CopyLine(to_right) => self.copy_line(to_right),
                TextPaneCtx::Cut => {
                    if let Some(id) = self.editor_cache[usize::from(!side_left)].widget_id {
                        ctx.memory_mut(|m| m.request_focus(id));
                    }
                    ctx.send_viewport_cmd(ViewportCommand::RequestCut);
                }
                TextPaneCtx::Copy => {
                    if let Some(id) = self.editor_cache[usize::from(!side_left)].widget_id {
                        ctx.memory_mut(|m| m.request_focus(id));
                    }
                    ctx.send_viewport_cmd(ViewportCommand::RequestCopy);
                }
                TextPaneCtx::Paste => {
                    if let Some(id) = self.editor_cache[usize::from(!side_left)].widget_id {
                        ctx.memory_mut(|m| m.request_focus(id));
                    }
                    ctx.send_viewport_cmd(ViewportCommand::RequestPaste);
                }
                TextPaneCtx::DeleteLines => self.edit_lines(ctx, 0),
                TextPaneCtx::SelectAll => self.select_all(ctx, side_left),
                TextPaneCtx::SelectSection => self.select_section(),
                TextPaneCtx::Find => {
                    self.search.shown = true;
                    self.search.replace_mode = false;
                    self.search.focus_query = true;
                }
                TextPaneCtx::NextDiff => self.next_diff(true),
                TextPaneCtx::PrevDiff => self.next_diff(false),
                TextPaneCtx::OpenFile => {
                    let cur = if side_left {
                        self.left.path.as_deref()
                    } else {
                        self.right.path.as_deref()
                    };
                    if let Some(path) = dialogs.file(false, side_left, cur) {
                        self.open_side(side_left, path, dialogs);
                    }
                }
                TextPaneCtx::OpenWith(kind) => {
                    if kind == crate::history::SessionKind::TextEdit {
                        let p = if side_left {
                            self.left.path.clone()
                        } else {
                            self.right.path.clone()
                        };
                        if let Some(path) = p {
                            super::session_menu::request(
                                ctx,
                                super::session_menu::Request::EditText(path),
                            );
                        }
                    } else {
                        super::session_menu::request(
                            ctx,
                            super::session_menu::Request::CompareUsing(kind),
                        );
                    }
                }
                TextPaneCtx::AssocApp => {
                    let p = if side_left {
                        self.left.path.clone()
                    } else {
                        self.right.path.clone()
                    };
                    if let Some(path) = p {
                        if let Err(err) = dialogs.open_path(&path, false) {
                            dialogs.error(&err);
                        }
                    }
                }
                TextPaneCtx::Explore(select) => {
                    let p = if side_left {
                        self.left.path.clone()
                    } else {
                        self.right.path.clone()
                    };
                    if let Some(path) = p {
                        let target = if select {
                            path
                        } else {
                            path.parent().unwrap_or(&path).to_path_buf()
                        };
                        if let Err(err) = dialogs.open_path(&target, select) {
                            dialogs.error(&err);
                        }
                    }
                }
            }
        }
        if edited {
            if let Some((_, left_before, right_before)) = self.edit_base.take() {
                let left_changed = left_before.as_ref() != self.left.text.as_str();
                let right_changed = right_before.as_ref() != self.right.text.as_str();
                if left_changed || right_changed {
                    if self.undo.len() >= 20 {
                        self.undo.remove(0);
                    }
                    let next_left = if left_changed {
                        std::sync::Arc::from(self.left.text.as_str())
                    } else {
                        left_before.clone()
                    };
                    let next_right = if right_changed {
                        std::sync::Arc::from(self.right.text.as_str())
                    } else {
                        right_before.clone()
                    };
                    self.undo.push((left_before, right_before));
                    self.redo.clear();
                    let side = usize::from(!self.active_left);
                    if let Some((result, ms, view)) =
                        self.editor_cache[side].take_comparison(&self.left.text, &self.right.text)
                    {
                        self.accept_comparison_side(result, ms, Some(self.active_left));
                        // The editor already built the aligned view of this comparison.
                        if !self.word_wrap {
                            self.view_cache = Some((
                                (
                                    self.revision,
                                    self.view_revision,
                                    self.filter_mode,
                                    self.context_lines,
                                    self.revealed.clone(),
                                ),
                                std::sync::Arc::new(view),
                            ));
                            self.view_generation = self.view_generation.wrapping_add(1);
                        }
                    } else {
                        self.compare();
                    }
                    self.edit_base = Some((self.revision, next_left, next_right));
                    let physical = self.caret[usize::from(!self.active_left)].0;
                    if let Some(index) = crate::diff::find_line_no(
                        &self.diff_result.lines,
                        self.active_left,
                        physical,
                    ) {
                        self.selected_line_idx = index;
                    }
                    ui.ctx().request_repaint();
                } else {
                    self.edit_base = Some((self.revision, left_before, right_before));
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
    let byte_at = |index| char_to_byte(text, index);
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
        let bottom = origin.y + last + view.line_heights[section.end - 1] - 3.0;
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
    let text = format!("{count} {}", crate::i18n::tr("text.filtered_lines"));
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
    let chunks = line.inline_chunks(left);
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
            DiffStatus::LeftOrphan | DiffStatus::RightOrphan => {
                let side_text = if left {
                    line.left_text.as_ref()
                } else {
                    line.right_text.as_ref()
                };
                if side_text.trim().is_empty() {
                    BcTheme::MINOR_BG
                } else if active {
                    BcTheme::DIFF_BG
                } else {
                    Color32::from_rgb(252, 230, 230)
                }
            }
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
                pos2(gutter.right() - 4.0, rect.top() + ROW_HEIGHT / 2.0),
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
        let min = painter.round_pos_to_pixels(pos2(
            gutter.left() + 4.0,
            rect.top() + ROW_HEIGHT / 2.0 - size.y / 2.0,
        ));
        if hot {
            let frame = painter.round_rect_to_pixels(Rect::from_min_size(
                pos2(min.x, rect.top() + 1.0),
                vec2(size.x, ROW_HEIGHT - 1.0),
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
                font_id: aligned_editor::editor_font(),
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
                font_id: aligned_editor::editor_font(),
                color: Color32::from_gray(160),
                ..Default::default()
            },
        );
    }
    job
}

fn load_time_label(milliseconds: u128) -> String {
    let mut number = format!("{:.2}", milliseconds as f64 / 1000.0);
    if crate::i18n::language().starts_with("pt") {
        number = number.replace('.', ",");
    }
    format!("{number} {}", crate::i18n::tr("common.seconds"))
}

/// BC5 orphan empty-slot pattern: single-direction 45° diagonal lines (`/`, bottom-left
/// to top-right), matching Windows GDI `HS_BDIAGONAL` (8 px pitch, crisp light gray on
/// white) and globally phase-locked (`x + y = k * step`) so adjacent rows join seamlessly.
fn draw_hatching(painter: &Painter, rect: Rect) {
    let painter = painter.with_clip_rect(rect.intersect(painter.clip_rect()));
    painter.rect_filled(rect, 0.0, Color32::WHITE);
    let step = 8.0_f32;
    let stroke = Stroke::new(1.0_f32, Color32::from_gray(208));
    let s_min = rect.left() + rect.top();
    let s_max = rect.right() + rect.bottom();
    let mut s = (s_min / step).floor() * step;
    while s <= s_max {
        painter.line_segment(
            [
                pos2(s - rect.bottom(), rect.bottom()),
                pos2(s - rect.top(), rect.top()),
            ],
            stroke,
        );
        s += step;
    }
}

#[cfg(test)]
mod tests {
    use super::super::dialogs::{SaveChoice, TestDialogs};
    use super::*;
    include!("text_compare/wrap_tests.rs");

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
        super::super::theme::apply(&ctx);
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
    fn char_to_byte_matches_char_indices_across_blocks() {
        let text: String = (0..3000)
            .map(|i| {
                if i % 3 == 0 {
                    "ação🙂
"
                } else {
                    "abc"
                }
            })
            .collect();
        for index in (0..text.chars().count() + 3).step_by(7) {
            let expected = text
                .char_indices()
                .nth(index)
                .map_or(text.len(), |(i, _)| i);
            assert_eq!(char_to_byte(&text, index), expected, "{index}");
        }
    }

    #[test]
    #[ignore = "manual replay with the user's read-only sample documents"]
    fn profile_edit_latency() {
        let ctx = Context::default();
        super::super::theme::apply(&ctx);
        let mut icons = IconManager::new();
        let mut screen = TextCompareScreen::new();
        screen.load_files(
            "C:/Test/sample_left.txt",
            "C:/Test/sample_right.txt",
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

    /// Manual benchmark with a large text pair: open (read + compare + first frame),
    /// wheel scrolling down/up and typing. Set ANYCOMPARE_BENCH_LEFT and ANYCOMPARE_BENCH_RIGHT.
    #[test]
    #[ignore = "manual benchmark requiring ANYCOMPARE_BENCH_LEFT and ANYCOMPARE_BENCH_RIGHT"]
    fn profile_large_pair() {
        let left = std::env::var("ANYCOMPARE_BENCH_LEFT")
            .expect("Set ANYCOMPARE_BENCH_LEFT to run this benchmark");
        let right = std::env::var("ANYCOMPARE_BENCH_RIGHT")
            .expect("Set ANYCOMPARE_BENCH_RIGHT to run this benchmark");
        let ms = |start: Instant| start.elapsed().as_secs_f64() * 1000.0;
        let stats = |frames: &[f64]| {
            let mut sorted = frames.to_vec();
            sorted.sort_by(f64::total_cmp);
            serde_json::json!({"count": frames.len(),
                "mean_ms": frames.iter().sum::<f64>() / frames.len().max(1) as f64,
                "p95_ms": sorted.get(sorted.len() * 95 / 100).copied().unwrap_or(0.0),
                "max_ms": sorted.last().copied().unwrap_or(0.0)})
        };
        let ctx = Context::default();
        super::super::theme::apply(&ctx);
        let mut icons = IconManager::new();
        let mut screen = TextCompareScreen::new();
        let open = Instant::now();
        screen.load_files(&left, &right);
        let load_ms = ms(open);
        assert!(screen.error.is_none(), "{:?}", screen.error);
        render_frame(&ctx, &mut screen, &mut icons, vec![]);
        let open_ms = ms(open);
        for _ in 0..2 {
            render_frame(&ctx, &mut screen, &mut icons, vec![]);
        }
        let over = Event::PointerMoved(pos2(300.0, 400.0));
        let wheel = |dy: f32| Event::MouseWheel {
            unit: egui::MouseWheelUnit::Line,
            delta: vec2(0.0, dy),
            modifiers: Modifiers::NONE,
        };
        render_frame(&ctx, &mut screen, &mut icons, vec![over.clone()]);
        let mut down = Vec::new();
        let mut tessellate = Vec::new();
        for _ in 0..60 {
            let start = Instant::now();
            let output = render_frame(
                &ctx,
                &mut screen,
                &mut icons,
                vec![over.clone(), wheel(-3.0)],
            );
            down.push(ms(start));
            let start = Instant::now();
            let shape_count = output.shapes.len();
            let primitives = ctx.tessellate(output.shapes, output.pixels_per_point);
            tessellate.push(ms(start));
            if tessellate.len() == 1 {
                let vertices: usize = primitives
                    .iter()
                    .map(|p| match &p.primitive {
                        egui::epaint::Primitive::Mesh(m) => m.vertices.len(),
                        _ => 0,
                    })
                    .sum();
                println!(
                    "shapes {shape_count} primitives {} vertices {vertices}",
                    primitives.len()
                );
            }
            assert!(!primitives.is_empty());
        }
        let scrolled = screen.scroll_offset_y;
        let mut up = Vec::new();
        for _ in 0..60 {
            let start = Instant::now();
            render_frame(
                &ctx,
                &mut screen,
                &mut icons,
                vec![over.clone(), wheel(3.0)],
            );
            up.push(ms(start));
        }
        // Jump to the middle and to the end (scrollbar drag / Ctrl+End cost).
        let mut jumps = Vec::new();
        for line in [
            screen.diff_result.lines.len() / 2,
            screen.diff_result.lines.len() - 1,
            0,
        ] {
            screen.selected_line_idx = line;
            screen.scroll_to_selection = true;
            let start = Instant::now();
            render_frame(&ctx, &mut screen, &mut icons, vec![]);
            jumps.push(ms(start));
        }
        render_frame(&ctx, &mut screen, &mut icons, vec![]);
        focus_at(&ctx, &screen, true, 3);
        render_frame(&ctx, &mut screen, &mut icons, vec![]);
        let mut typing = Vec::new();
        for _ in 0..20 {
            let start = Instant::now();
            render_frame(&ctx, &mut screen, &mut icons, vec![Event::Text("x".into())]);
            typing.push(ms(start));
        }
        assert!(
            screen.left.text.contains("xxxxxxxxxx"),
            "typing must reach the buffer"
        );
        println!(
            "{}",
            serde_json::json!({"version": env!("CARGO_PKG_VERSION"),
                "metric": "egui input/layout frame, no GPU submission",
                "lines": [screen.left.text.lines().count(), screen.right.text.lines().count()],
                "aligned_rows": screen.diff_result.lines.len(),
                "load_compare_ms": load_ms, "open_first_frame_ms": open_ms,
                "scrolled_px": scrolled, "scroll_down": stats(&down), "tessellate": stats(&tessellate), "scroll_up": stats(&up),
                "jumps_ms": jumps, "typing": stats(&typing)})
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
            DialogField::Choice(
                "".into(),
                crate::diff::AlignMethod::default().index(),
                vec![],
            ),
            DialogField::Check("".into(), true),
            DialogField::Check("".into(), false),
            DialogField::Text("".into(), "2000".into()),
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
            .any(|line| &*line.left_text == "🙂fim"));
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
                super::super::theme::apply(&ctx);
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
        super::super::theme::apply(&ctx);
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
        super::super::theme::apply(&ctx);
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
                "C:/Test/sample_left.txt",
                "C:/Test/sample_right.txt",
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
        assert_eq!(&*screen.diff_result.lines[caret].right_text, "meio");
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
        assert_eq!(GutterArrow::Section.hint(true), "text.copy_section_right");
        assert_eq!(GutterArrow::Section.hint(false), "text.copy_section_left");
        assert_eq!(GutterArrow::Selection.hint(true), "text.copy_lines_right");
        assert_eq!(GutterArrow::Selection.hint(false), "text.copy_lines_left");
        assert_eq!(
            crate::i18n::translate("pt-BR", GutterArrow::Section.hint(true)),
            "Copiar seção para a direita"
        );
        assert_eq!(
            crate::i18n::translate("pt-BR", GutterArrow::Section.hint(false)),
            "Copiar seção para a esquerda"
        );
        assert_eq!(
            crate::i18n::translate("pt-BR", GutterArrow::Selection.hint(true)),
            "Copiar linha(s) para a direita"
        );
        assert_eq!(
            crate::i18n::translate("pt-BR", GutterArrow::Selection.hint(false)),
            "Copiar linha(s) para a esquerda"
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
