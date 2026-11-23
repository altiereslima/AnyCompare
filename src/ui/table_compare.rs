mod table_actions;
mod table_grid;
mod table_selection;
use super::{
    chrome,
    dialogs::{DialogField, DialogHost, SaveChoice},
    history_combo,
    icons::IconManager,
    session_menu, session_tabs,
    theme::BcTheme,
    toolbar::{self, ToolEvent, ToolState},
    toolbar_spec,
};
use crate::{
    diff::AlignMethod,
    document::{self, Encoding},
    i18n::tr,
    table::{self, Filter, Options, Pair, Side},
    table_columns::{self, Alignment, Change, Column},
    table_formats::{self, FirstLineKind, Qualifier, SideFormat},
    table_val::{self, CellHint, ColHandling, NumberLocale, RegionalSettings},
};
use egui::*;
use std::path::{Path, PathBuf};
use table_grid::column_letter;

const ROW_HEIGHT: f32 = 18.0;
const COLUMN_WIDTH: f32 = 260.0;
const NUMBER_WIDTH: f32 = 54.0;
// BC5 UI Automation: client-relative thumbnail 43 px, pane divider 3 px.
const OVERVIEW_WIDTH: f32 = 43.0;
use table_selection::{CellGesture, Selection};
type CellHit = (bool, usize, usize, CellGesture);
type InlinePair = (Vec<crate::diff::InlineChunk>, Vec<crate::diff::InlineChunk>);
#[derive(Clone, Copy)]
enum HeaderAction {
    Resize(usize, f32),
    Sort(usize),
    Key(usize),
    Unimportant(usize),
    Configure(usize),
}
fn header_action_id() -> Id {
    Id::new("table_header_action")
}

struct CellEdit {
    left: bool,
    row: usize,
    column: usize,
    value: String,
    focus: bool,
}

pub struct TableCompareScreen {
    pub left: Side,
    pub right: Side,
    pub error: Option<String>,
    pub read_only: bool,
    options: Options,
    pairs: Vec<Pair>,
    columns: Vec<Column>,
    column_changes: Vec<Change>,
    sheet_summaries: Vec<crate::table_sheets::Summary>,
    summary_signature: String,
    visible: Vec<usize>,
    paths: [String; 2],
    active_left: bool,
    selected: usize,
    column: usize,
    selection: Selection,
    scroll_y: f32,
    scroll_x: [f32; 2],
    detail_x: f32,
    detail_height: f32,
    auto_widths: Vec<f32>,
    load_time: f64,
    inline_cache: std::cell::RefCell<std::collections::HashMap<(usize, usize), InlinePair>>,
    jump: bool,
    editor: Option<CellEdit>,
    align_pending: Option<(bool, usize)>,
    undo: Vec<[Option<String>; 2]>,
    redo: Vec<[Option<String>; 2]>,
    search: String,
    show_search: bool,
    search_notice: bool,
    show_row_numbers: bool,
    show_toolbar: bool,
    show_file_info: bool,
    over_under: bool,
    font_size: f32,
}

impl TableCompareScreen {
    pub fn new() -> Self {
        Self {
            left: Side::default(),
            right: Side::default(),
            error: None,
            read_only: false,
            options: Options::default(),
            pairs: Vec::new(),
            columns: Vec::new(),
            column_changes: Vec::new(),
            sheet_summaries: Vec::new(),
            summary_signature: String::new(),
            visible: Vec::new(),
            paths: Default::default(),
            active_left: true,
            selected: 0,
            column: 0,
            selection: Selection::default(),
            scroll_y: 0.0,
            scroll_x: [0.0; 2],
            detail_x: 0.0,
            detail_height: 82.0,
            auto_widths: Vec::new(),
            load_time: 0.0,
            inline_cache: Default::default(),
            jump: false,
            editor: None,
            align_pending: None,
            undo: Vec::new(),
            redo: Vec::new(),
            search: String::new(),
            show_search: false,
            search_notice: false,
            show_row_numbers: true,
            show_toolbar: true,
            show_file_info: true,
            over_under: false,
            font_size: 12.0,
        }
    }

    pub fn resize_columns_to_fit(&mut self) {
        self.auto_widths.clear();
    }

    pub fn navigate_diff_cell(&mut self, forward: bool) {
        self.navigate(forward);
    }

    #[cfg(test)]
    pub fn selected_pair_index(&self) -> usize {
        self.selected
    }

    pub fn report(&self, dialogs: &mut dyn DialogHost) {
        let mut rows = Vec::with_capacity(self.pairs.len());
        for (idx, pair) in self.pairs.iter().enumerate() {
            let l_cells = pair
                .left
                .and_then(|r| self.left.table.rows.get(r))
                .map(|r| r.cells.join(" | "))
                .unwrap_or_default();
            let r_cells = pair
                .right
                .and_then(|r| self.right.table.rows.get(r))
                .map(|r| r.cells.join(" | "))
                .unwrap_or_default();
            rows.push(crate::report::Row {
                key: (idx + 1).to_string(),
                left: l_cells,
                right: r_cells,
                different: pair.different,
            });
        }
        let mut inputs = Vec::new();
        if !self.left.path.is_empty() {
            inputs.push(PathBuf::from(&self.left.path));
        }
        if !self.right.path.is_empty() {
            inputs.push(PathBuf::from(&self.right.path));
        }
        crate::report::export(
            "table.report",
            &self.left.path,
            &self.right.path,
            &rows,
            &inputs,
            dialogs,
        );
    }

    fn open_info_modal(&self, ctx: &Context) {
        let diff_rows = self.pairs.iter().filter(|p| p.different).count();
        let same_rows = self.pairs.len().saturating_sub(diff_rows);
        let rows = vec![
            (tr("folder.side_left"), self.left.path.clone()),
            (tr("folder.side_right"), self.right.path.clone()),
            (tr("common.same"), same_rows.to_string()),
            (tr("common.differences"), diff_rows.to_string()),
        ];
        super::info_modals::open_info(ctx, tr("table.compare_info"), rows);
    }
    pub fn dirty(&self) -> bool {
        self.left.dirty() || self.right.dirty() || self.editor_changed()
    }
    pub fn is_empty(&self) -> bool {
        self.left.path.is_empty() && self.right.path.is_empty() && !self.dirty()
    }
    pub fn active_left(&self) -> bool {
        self.active_left
    }
    pub fn sync_preferences(&self, prefs: &mut crate::i18n::AppPreferences) {
        prefs.table_has_header = self.options.header;
        prefs.table_sort_by_key = self.options.align_sort;
        prefs.table_ignore_minor = self.options.ignore_minor;
        prefs.table_hide_same_columns = self.options.hide_same_columns;
    }
    pub fn apply_preferences(&mut self, prefs: &crate::i18n::AppPreferences) {
        let rebuild = self.options.header != prefs.table_has_header
            || self.options.align_sort != prefs.table_sort_by_key
            || self.options.ignore_minor != prefs.table_ignore_minor;
        self.options.header = prefs.table_has_header;
        self.options.align_sort = prefs.table_sort_by_key;
        self.options.ignore_minor = prefs.table_ignore_minor;
        self.options.hide_same_columns = prefs.table_hide_same_columns;
        if rebuild {
            self.rebuild();
        }
    }
    pub fn is_side_empty(&self, left: bool) -> bool {
        if left {
            self.left.path.is_empty() && !self.left.dirty()
        } else {
            self.right.path.is_empty() && !self.right.dirty()
        }
    }
    pub fn session_options(&self) -> serde_json::Value {
        serde_json::to_value(&self.options).unwrap_or_default()
    }
    pub fn apply_session_options(&mut self, options: &serde_json::Value) {
        if let Ok(options) = serde_json::from_value(options.clone()) {
            self.options = options;
            self.options.sheet_pair_explicit = true;
        }
        self.rebuild();
    }
    pub fn load_files(&mut self, left: &Path, right: &Path) -> Result<(), String> {
        let started = std::time::Instant::now();
        let delims = self.options.delimiters;
        let sheets = self.options.sheets;
        let formats = &self.options.formats;
        let (res_a, res_b) = std::thread::scope(|scope| {
            let hb = scope.spawn(|| Side::load_with_format(right, delims[1], sheets[1], &formats[1]));
            let a = Side::load_with_format(left, delims[0], sheets[0], &formats[0]);
            (a, hb.join().unwrap())
        });
        let a = res_a?;
        let b = res_b?;
        let a = if self.options.missing_sheets[0] {
            crate::table_sheets::load(&a, None)?
        } else {
            a
        };
        let b = if self.options.missing_sheets[1] {
            crate::table_sheets::load(&b, None)?
        } else {
            b
        };
        self.left = a;
        self.right = b;
        if !self.options.sheet_pair_explicit
            && (!self.left.sheets.is_empty() || !self.right.sheets.is_empty())
        {
            if let Some(pair) = self.sheet_pairs().first() {
                let a = crate::table_sheets::load(&self.left, pair.left)?;
                let b = crate::table_sheets::load(&self.right, pair.right)?;
                self.left = a;
                self.right = b;
                self.options.sheets = [self.left.sheet, self.right.sheet];
                self.options.missing_sheets = [self.left.missing_sheet, self.right.missing_sheet];
            }
        }
        self.apply_first_line_detection();
        self.paths = [self.left.path.clone(), self.right.path.clone()];
        self.undo.clear();
        self.redo.clear();
        self.editor = None;
        self.error = None;
        self.selected = 0;
        self.scroll_y = 0.0;
        self.scroll_x = [0.0; 2];
        self.rebuild();
        self.load_time = started.elapsed().as_secs_f64();
        Ok(())
    }
    fn apply_first_line_detection(&mut self) {
        if self.options.first_line == FirstLineKind::Detect {
            let left_has = table_formats::detect_has_header(
                &self.left.table,
                self.options.side_regional(true),
            );
            let right_has = table_formats::detect_has_header(
                &self.right.table,
                self.options.side_regional(false),
            );
            self.options.header = left_has || right_has;
        }
    }
    pub fn open_side(&mut self, left: bool, path: &Path, dialogs: &mut dyn DialogHost) {
        let i = usize::from(!left);
        match Side::load_with_format(
            path,
            self.options.delimiters[i],
            0,
            &self.options.formats[i],
        ) {
            Ok(side) => {
                if !self.commit_editor(dialogs) || !self.confirm_side(left, dialogs) {
                    return;
                }
                if left {
                    self.left = side;
                } else {
                    self.right = side;
                }
                self.paths[i] = path.to_string_lossy().into_owned();
                self.options.sheets[i] = 0;
                self.options.missing_sheets[i] = false;
                self.apply_first_line_detection();
                self.undo.clear();
                self.redo.clear();
                self.error = None;
                self.selected = 0;
                self.rebuild();
            }
            Err(error) => {
                self.error = Some(error.clone());
                dialogs.error(&error);
            }
        }
    }
    pub fn set_filter(&mut self, filter: Filter) {
        self.options.filter = filter;
        self.rebuild();
    }
    pub fn show_search(&mut self, query: String) {
        self.show_search = true;
        self.search = query;
        self.find(true);
    }
    fn rebuild(&mut self) {
        self.selection = Selection::default();
        self.inline_cache.get_mut().clear();
        let row = self.selected_row(self.active_left);
        self.columns = table_columns::columns(&self.left.table, &self.right.table, &self.options);
        self.auto_widths = self
            .columns
            .iter()
            .map(|column| {
                [true, false]
                    .into_iter()
                    .filter_map(|left| {
                        column.side(left).map(|c| {
                            self.side(left)
                                .table
                                .rows
                                .iter()
                                .take(256)
                                .filter_map(|r| r.cells.get(c))
                                .map(|s| s.chars().count().min(180))
                                .max()
                                .unwrap_or(0)
                        })
                    })
                    .max()
                    .map_or(COLUMN_WIDTH, |n| {
                        (n as f32 * 7.0 + 16.0).clamp(48.0, 1100.0)
                    })
            })
            .collect();
        self.column = self.column.min(self.columns.len().saturating_sub(1));
        self.pairs = table::align_columns(
            &self.left.table,
            &self.right.table,
            &self.options,
            &self.columns,
        );
        self.column_changes = (0..self.columns.len())
            .map(|column| {
                self.pairs
                    .iter()
                    .map(|p| match (p.left, p.right) {
                        (Some(a), Some(b)) => {
                            let (lv, lh) = self.cell(true, &self.left.table.rows[a], column);
                            let (rv, rh) = self.cell(false, &self.right.table.rows[b], column);
                            table_columns::change_with_hints(&self.options, column, lv, lh, rv, rh)
                        }
                        _ => Change::Important,
                    })
                    .max()
                    .unwrap_or(Change::Same)
            })
            .collect();
        self.visible = self
            .pairs
            .iter()
            .enumerate()
            .filter_map(|(index, pair)| {
                (match self.options.filter {
                    Filter::All => true,
                    Filter::Differences => {
                        if self.options.ignore_minor {
                            pair.important
                        } else {
                            pair.different
                        }
                    }
                    Filter::Same => {
                        if self.options.ignore_minor {
                            !pair.important
                        } else {
                            !pair.different
                        }
                    }
                })
                .then_some(index)
            })
            .collect();
        if let Some((column, ascending)) = self.options.sort {
            let handling = self.options.col_handling(column);
            let left_reg = self.options.side_regional(true);
            let right_reg = self.options.side_regional(false);
            let mut keyed: Vec<(usize, table_val::TableVal)> = self
                .visible
                .iter()
                .map(|&index| {
                    let p = &self.pairs[index];
                    let val = if let Some(r) = p.left {
                        let (txt, hint) = self.cell(true, &self.left.table.rows[r], column);
                        table_val::parse_table_val(
                            txt.unwrap_or(""),
                            hint,
                            left_reg,
                            handling.as_text,
                        )
                    } else if let Some(r) = p.right {
                        let (txt, hint) = self.cell(false, &self.right.table.rows[r], column);
                        table_val::parse_table_val(
                            txt.unwrap_or(""),
                            hint,
                            right_reg,
                            handling.as_text,
                        )
                    } else {
                        table_val::TableVal::Text(String::new())
                    };
                    (index, val)
                })
                .collect();
            keyed.sort_by(|a, b| {
                let order = table_val::cmp_table_vals(&a.1, &b.1, &handling);
                let ordered = if ascending { order } else { order.reverse() };
                ordered.then_with(|| a.0.cmp(&b.0))
            });
            self.visible = keyed.into_iter().map(|(index, _)| index).collect();
        }
        self.selected = row
            .and_then(|row| {
                self.pairs
                    .iter()
                    .position(|p| (if self.active_left { p.left } else { p.right }) == Some(row))
            })
            .unwrap_or(self.selected)
            .min(self.pairs.len().saturating_sub(1));
        self.scroll_y = self
            .scroll_y
            .min(self.visible.len().saturating_sub(1) as f32 * ROW_HEIGHT);
        if !self.left.sheets.is_empty() || !self.right.sheets.is_empty() {
            let mut rules = self.options.clone();
            rules.filter = Filter::All;
            rules.sort = None;
            rules.widths.clear();
            rules.hide_same_columns = false;
            rules.sheets = [0; 2];
            rules.missing_sheets = [false; 2];
            rules.sheet_pair_explicit = false;
            let signature = format!(
                "{:?}{:?}{:?}{:?}{}",
                self.left.path,
                self.right.path,
                self.left.binary_hash,
                self.right.binary_hash,
                serde_json::to_string(&rules).unwrap_or_default()
            );
            if signature != self.summary_signature {
                self.sheet_summaries = crate::table_sheets::compare(
                    &self.left,
                    &self.right,
                    &rules,
                    &self.sheet_pairs(),
                );
                self.summary_signature = signature;
            }
        } else {
            self.sheet_summaries.clear();
            self.summary_signature.clear();
        }
    }
    fn side(&self, left: bool) -> &Side {
        if left {
            &self.left
        } else {
            &self.right
        }
    }
    fn side_mut(&mut self, left: bool) -> &mut Side {
        if left {
            &mut self.left
        } else {
            &mut self.right
        }
    }
    fn physical_column(&self, left: bool, column: usize) -> Option<usize> {
        self.columns.get(column).and_then(|c| c.side(left))
    }
    fn column_visible(&self, column: usize) -> bool {
        column < self.columns.len()
            && (!self.options.hide_same_columns
                || self.column_changes.get(column).is_some_and(|change| {
                    *change == Change::Important
                        || (!self.options.ignore_minor && *change == Change::Minor)
                }))
    }
    fn column_layout(&self) -> Vec<(usize, f32, f32)> {
        let mut x = NUMBER_WIDTH;
        let mut result = Vec::new();
        for column in 0..self.columns.len() {
            if !self.column_visible(column) {
                continue;
            }
            let width = self
                .options
                .widths
                .get(column)
                .copied()
                .filter(|v| v.is_finite())
                .unwrap_or_else(|| {
                    self.auto_widths
                        .get(column)
                        .copied()
                        .unwrap_or(COLUMN_WIDTH)
                })
                .clamp(40.0, 1200.0);
            result.push((column, x, width));
            x += width;
        }
        result
    }
    fn copy_cell(&mut self, dialogs: &mut dyn DialogHost) {
        if !self.commit_editor(dialogs) || !self.can_copy_cell() {
            return;
        }
        let left = self.active_left;
        let mut rows = std::collections::BTreeMap::new();
        for (pair, column) in self.selected_cells() {
            let (Some(source), Some(target), Some(_), Some(target_col)) = (
                self.row_at(left, pair),
                self.row_at(!left, pair),
                self.physical_column(left, column),
                self.physical_column(!left, column),
            ) else {
                continue;
            };
            let value = self
                .value(left, &self.side(left).table.rows[source], column)
                .unwrap_or("");
            let cells = rows
                .entry(target)
                .or_insert_with(|| Some(self.side(!left).table.rows[target].cells.clone()))
                .as_mut()
                .unwrap();
            cells.resize(cells.len().max(target_col + 1), String::new());
            cells[target_col] = value.to_owned();
        }
        self.commit_records(!left, rows, Default::default(), false, dialogs);
    }
    fn value<'a>(&self, left: bool, row: &'a table::Row, column: usize) -> Option<&'a str> {
        self.physical_column(left, column)
            .and_then(|c| self.options.value(row, c))
    }
    fn cell<'a>(
        &self,
        left: bool,
        row: &'a table::Row,
        column: usize,
    ) -> (Option<&'a str>, Option<&'a CellHint>) {
        self.physical_column(left, column)
            .map_or((None, None), |c| self.options.cell(row, c))
    }
    fn selected_row(&self, left: bool) -> Option<usize> {
        self.pairs
            .get(self.selected)
            .and_then(|p| if left { p.left } else { p.right })
    }
    fn snapshot(&self) -> [Option<String>; 2] {
        [&self.left, &self.right].map(|s| s.document.as_ref().map(|d| d.text.clone()))
    }
    fn remember_edit(&mut self) {
        if self.undo.len() >= 20 {
            self.undo.remove(0);
        }
        self.undo.push(self.snapshot());
        self.redo.clear();
    }
    fn restore(&mut self, redo: bool) {
        let state = if redo {
            self.redo.pop()
        } else {
            self.undo.pop()
        };
        if let Some(state) = state {
            let current = self.snapshot();
            if redo {
                self.undo.push(current);
            } else {
                self.redo.push(current);
            }
            for (side, text) in [&mut self.left, &mut self.right].into_iter().zip(state) {
                if let (Some(doc), Some(text)) = (&mut side.document, text) {
                    doc.text = text;
                    let _ = side.reparse();
                }
            }
            self.editor = None;
            self.rebuild();
        }
    }
    fn editor_changed(&self) -> bool {
        self.editor.as_ref().is_some_and(|e| {
            self.side(e.left)
                .table
                .rows
                .get(e.row)
                .and_then(|r| r.cells.get(e.column))
                .map(String::as_str)
                .unwrap_or("")
                != e.value
        })
    }
    fn commit_editor(&mut self, dialogs: &mut dyn DialogHost) -> bool {
        if !self.editor_changed() {
            self.editor = None;
            return true;
        }
        if self.read_only {
            return false;
        }
        self.remember_edit();
        let edit = self.editor.take().unwrap();
        match self
            .side_mut(edit.left)
            .edit_cell(edit.row, edit.column, &edit.value)
        {
            Ok(()) => {
                self.rebuild();
                true
            }
            Err(error) => {
                self.editor = Some(edit);
                dialogs.error(&error);
                false
            }
        }
    }
    fn begin_edit(&mut self, dialogs: &mut dyn DialogHost) {
        let left = self.active_left;
        if self.read_only || !self.side(left).editable() || !self.commit_editor(dialogs) {
            return;
        }
        if let (Some(row), Some(column)) = (
            self.selected_row(left),
            self.physical_column(left, self.column),
        ) {
            let value = self.side(left).table.rows[row]
                .cells
                .get(column)
                .cloned()
                .unwrap_or_default();
            self.editor = Some(CellEdit {
                left,
                row,
                column,
                value,
                focus: true,
            });
        }
    }
    fn copy_row(&mut self, dialogs: &mut dyn DialogHost) {
        if !self.commit_editor(dialogs) || !self.can_copy() {
            return;
        }
        let left = self.active_left;
        let source = &self.side(left).table;
        let target = &self.side(!left).table;
        let empty = target.rows.is_empty();
        let mut rows = std::collections::BTreeMap::new();
        let mut inserts = std::collections::BTreeMap::<usize, Vec<Vec<String>>>::new();
        if empty && self.options.header {
            if let Some(header) = source.rows.first() {
                inserts.entry(0).or_default().push(header.cells.clone());
            }
        }
        let mut next_row = target.rows.len();
        let next_targets = self
            .pairs
            .iter()
            .rev()
            .map(|pair| {
                let next = next_row;
                if let Some(row) = if left { pair.right } else { pair.left } {
                    next_row = row;
                }
                next
            })
            .collect::<Vec<_>>();
        for pair in self.selected_pairs() {
            let Some(from) = self.row_at(left, pair) else {
                continue;
            };
            let to = self.row_at(!left, pair);
            let source_cells = &source.rows[from].cells;
            let cells = if empty {
                source_cells.clone()
            } else {
                let mut cells = to
                    .map(|row| target.rows[row].cells.clone())
                    .unwrap_or_else(|| vec![String::new(); target.columns]);
                for column in &self.columns {
                    if let (Some(value), Some(to)) = (
                        column
                            .side(left)
                            .and_then(|column| source_cells.get(column)),
                        column.side(!left),
                    ) {
                        cells.resize(cells.len().max(to + 1), String::new());
                        cells[to] = value.clone();
                    }
                }
                cells
            };
            if let Some(row) = to {
                rows.insert(row, Some(cells));
            } else {
                let before = next_targets[self.pairs.len() - pair - 1];
                inserts.entry(before).or_default().push(cells);
            }
        }
        let structural = !inserts.is_empty();
        self.commit_records(!left, rows, inserts, structural, dialogs);
    }
    fn save_side(&mut self, left: bool, save_as: bool, dialogs: &mut dyn DialogHost) -> bool {
        if !self.commit_editor(dialogs) || self.read_only {
            return false;
        }
        let Some(doc) = &self.side(left).document else {
            return false;
        };
        let path = if save_as || doc.path.is_none() {
            dialogs.file(true, left, doc.path.as_deref())
        } else {
            doc.path.clone()
        };
        let Some(path) = path else {
            return false;
        };
        if table::is_workbook(&path) || table::is_html(&path) || table::is_dbf(&path) {
            dialogs.error("error.table.text_ext_required");
            return false;
        }
        if !self.side(!left).path.is_empty()
            && document::same_file(&path, Path::new(&self.side(!left).path))
        {
            dialogs.error("error.table.target_is_other_side");
            return false;
        }
        let doc = self.side_mut(left).document.as_mut().unwrap();
        if doc.encoding == Encoding::Windows1252 && encoding_rs::WINDOWS_1252.encode(&doc.text).2 {
            let encodings: Vec<_> = Encoding::ALL
                .into_iter()
                .filter(|e| *e != Encoding::Windows1252)
                .collect();
            let mut fields = [
                DialogField::Notice(tr(
                    "table.choose_an_encoding_that_preserves_all",
                )),
                DialogField::Choice(
                    tr("common.encoding"),
                    0,
                    encodings.iter().map(|e| e.label().to_owned()).collect(),
                ),
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
                let i = usize::from(!left);
                self.paths[i] = path.to_string_lossy().into_owned();
                self.side_mut(left).path = path.to_string_lossy().into_owned();
                true
            }
            Err(error) => {
                dialogs.error(&error);
                false
            }
        }
    }
    fn confirm_side(&mut self, left: bool, dialogs: &mut dyn DialogHost) -> bool {
        if !self.side(left).dirty() {
            return true;
        }
        match dialogs.confirm_save(
            left,
            self.side(left)
                .document
                .as_ref()
                .and_then(|d| d.path.as_deref()),
        ) {
            SaveChoice::Save => self.save_side(left, false, dialogs),
            SaveChoice::Discard => true,
            SaveChoice::Cancel => false,
        }
    }
    pub fn confirm_saved(&mut self, dialogs: &mut dyn DialogHost) -> bool {
        self.commit_editor(dialogs)
            && self.confirm_side(true, dialogs)
            && self.confirm_side(false, dialogs)
    }
    fn navigate(&mut self, forward: bool) {
        if self.visible.is_empty() {
            return;
        }
        let start = self
            .visible
            .iter()
            .position(|&i| i == self.selected)
            .unwrap_or(0);
        for delta in 1..=self.visible.len() {
            let position = if forward {
                (start + delta) % self.visible.len()
            } else {
                (start + self.visible.len() - delta % self.visible.len()) % self.visible.len()
            };
            let index = self.visible[position];
            if if self.options.ignore_minor {
                self.pairs[index].important
            } else {
                self.pairs[index].different
            } {
                self.selection = Selection::default();
                self.selected = index;
                self.jump = true;
                break;
            }
        }
    }
    fn find(&mut self, forward: bool) {
        self.search_notice = false;
        if self.search.is_empty() {
            return;
        }
        let query = self.search.to_lowercase();
        // Search all cells in aligned order, including columns outside the viewport.
        let mut hits = Vec::new();
        for &index in &self.visible {
            let pair = &self.pairs[index];
            for (left, row) in [(true, pair.left), (false, pair.right)] {
                if let Some(row) = row {
                    for col in 0..self.columns.len() {
                        let Some(value) = self.value(left, &self.side(left).table.rows[row], col)
                        else {
                            continue;
                        };
                        if value.to_lowercase().contains(&query) {
                            hits.push((index, left, col));
                        }
                    }
                }
            }
        }
        let current = hits
            .iter()
            .position(|&h| h == (self.selected, self.active_left, self.column));
        let next = if forward {
            current.map_or(0, |i| (i + 1) % hits.len().max(1))
        } else {
            current.map_or(hits.len().saturating_sub(1), |i| {
                (i + hits.len() - 1) % hits.len()
            })
        };
        if let Some(&(index, left, column)) = hits.get(next) {
            self.selection = Selection::default();
            self.selected = index;
            self.active_left = left;
            self.column = column;
            self.options.hide_same_columns = false;
            self.jump = true;
        } else {
            self.search_notice = true;
        }
    }
    fn rules(&mut self, dialogs: &mut dyn DialogHost) {
        if !self.commit_editor(dialogs) {
            return;
        }
        let mut keys = vec![tr("table.row_position")];
        let columns = self.columns.len();
        keys.extend((0..columns).map(|i| self.heading(true, i)));
        let first_line_idx = match self.options.first_line {
            FirstLineKind::Detect => 0,
            FirstLineKind::ColumnNames => 1,
            FirstLineKind::CellData => usize::from(self.options.header),
        };
        let locale_idx = match self.options.number_locale {
            NumberLocale::System => {
                if self.options.decimal_comma {
                    2
                } else {
                    0
                }
            }
            NumberLocale::Dot => 1,
            NumberLocale::Comma => 2,
        };
        let mut fields = [
            DialogField::Choice(
                tr("table.first_line_contains"),
                if self.options.first_line == FirstLineKind::Detect {
                    0
                } else if self.options.header {
                    1
                } else {
                    2
                },
                [
                    "doc.detect_auto",
                    "table.column_names",
                    "table.cell_data",
                ]
                .map(tr)
                .to_vec(),
            ),
            DialogField::Choice(
                tr("table.align_by"),
                self.options.key.map_or(0, |k| k + 1).min(columns),
                keys,
            ),
            DialogField::Text(
                tr("table.additional_keys_comma_separated_columns"),
                self.options
                    .extra_keys
                    .iter()
                    .map(|c| (c + 1).to_string())
                    .collect::<Vec<_>>()
                    .join(","),
            ),
            DialogField::Check(
                tr("table.sort_lines_before_aligning"),
                self.options.align_sort,
            ),
            DialogField::Choice(
                tr("table.alignment_algorithm"),
                self.options.align_method.index(),
                AlignMethod::ALL.iter().map(|m| tr(m.label())).collect(),
            ),
            DialogField::Choice(
                tr("table.column_alignment"),
                match self.options.column_alignment {
                    Alignment::Position => 0,
                    Alignment::LeftNames => 1,
                    Alignment::RightNames => 2,
                },
                ["common.position", "table.left_names", "table.right_names"]
                    .map(tr)
                    .to_vec(),
            ),
            DialogField::Choice(
                tr("table.default_column_type"),
                usize::from(self.options.as_text),
                ["table.type_general", "common.text"]
                    .map(tr)
                    .to_vec(),
            ),
            DialogField::Choice(
                tr("table.regional_numeric_format"),
                locale_idx,
                [
                    NumberLocale::System.label(),
                    NumberLocale::Dot.label(),
                    NumberLocale::Comma.label(),
                ]
                .map(tr)
                .to_vec(),
            ),
            DialogField::Text(tr("table.numeric_tolerance"), self.options.tolerance.clone()),
            DialogField::Choice(
                tr("table.numeric_tolerance_type"),
                usize::from(self.options.num_tolerance_relative),
                ["table.tolerance.absolute", "table.tolerance.relative"]
                    .map(tr)
                    .to_vec(),
            ),
            DialogField::Text(
                tr("table.timestamp_tolerance_seconds"),
                self.options.datetime_tolerance_secs.to_string(),
            ),
            DialogField::Check(
                tr("common.ignore_character_case"),
                self.options.ignore_case,
            ),
            DialogField::Check(tr("common.ignore_whitespace"), self.options.trim),
            DialogField::Check(
                tr("table.compare_formulas_instead_of_stored_values"),
                self.options.formulas,
            ),
            DialogField::Text(
                tr("table.unimportant_columns"),
                self.options
                    .unimportant_columns
                    .iter()
                    .map(|c| (c + 1).to_string())
                    .collect::<Vec<_>>()
                    .join(","),
            ),
        ];
        let _ = first_line_idx;
        if dialogs.fields("common.comparison_rules", &mut fields) {
            let list = |field: &DialogField| -> Result<Vec<usize>, String> {
                let DialogField::Text(_, text) = field else {
                    return Ok(Vec::new());
                };
                let mut result = Vec::new();
                for item in text.split(',').map(str::trim).filter(|s| !s.is_empty()) {
                    let n = item
                        .parse::<usize>()
                        .ok()
                        .filter(|&n| n > 0 && n <= columns)
                        .ok_or_else(|| tr("table.invalid_column_number"))?;
                    if !result.contains(&(n - 1)) {
                        result.push(n - 1);
                    }
                }
                Ok(result)
            };
            let (extra, unimportant) = match (list(&fields[2]), list(&fields[14])) {
                (Ok(a), Ok(b)) => (a, b),
                (Err(e), _) | (_, Err(e)) => {
                    dialogs.error(&e);
                    return;
                }
            };
            let number_locale = match fields[7] {
                DialogField::Choice(_, 1, _) => NumberLocale::Dot,
                DialogField::Choice(_, 2, _) => NumberLocale::Comma,
                _ => NumberLocale::System,
            };
            let tol_regional = RegionalSettings {
                date_order: self.options.date_order,
                number_locale,
            };
            let tolerance = if let DialogField::Text(_, ref text) = fields[8] {
                text.clone()
            } else {
                "0".into()
            };
            if table_val::parse_tolerance(&tolerance, tol_regional).is_none() {
                dialogs.error("table.invalid_numeric_tolerance");
                return;
            }
            let datetime_tolerance_secs = if let DialogField::Text(_, ref text) = fields[10] {
                match text.trim().parse::<u64>() {
                    Ok(v) => v,
                    Err(_) => {
                        dialogs.error("error.table.invalid_datetime_tol");
                        return;
                    }
                }
            } else {
                0
            };
            if let DialogField::Choice(_, v, _) = fields[0] {
                match v {
                    0 => {
                        self.options.first_line = FirstLineKind::Detect;
                        self.apply_first_line_detection();
                    }
                    1 => {
                        self.options.first_line = FirstLineKind::ColumnNames;
                        self.options.header = true;
                    }
                    _ => {
                        self.options.first_line = FirstLineKind::CellData;
                        self.options.header = false;
                    }
                }
            }
            if let DialogField::Choice(_, v, _) = fields[1] {
                self.options.key = v.checked_sub(1);
            }
            if let DialogField::Check(_, v) = fields[3] {
                self.options.align_sort = v;
            }
            if let DialogField::Choice(_, v, _) = fields[4] {
                self.options.align_method = AlignMethod::from_index(v);
            }
            if let DialogField::Choice(_, v, _) = fields[5] {
                self.options.column_alignment = match v {
                    1 => Alignment::LeftNames,
                    2 => Alignment::RightNames,
                    _ => Alignment::Position,
                };
            }
            if let DialogField::Choice(_, v, _) = fields[6] {
                self.options.as_text = v == 1;
            }
            self.options.number_locale = number_locale;
            self.options.decimal_comma = number_locale == NumberLocale::Comma;
            self.options.numeric = !self.options.as_text;
            self.options.tolerance = tolerance;
            if let DialogField::Choice(_, v, _) = fields[9] {
                self.options.num_tolerance_relative = v == 1;
            }
            self.options.datetime_tolerance_secs = datetime_tolerance_secs;
            if let DialogField::Check(_, v) = fields[11] {
                self.options.ignore_case = v;
            }
            if let DialogField::Check(_, v) = fields[12] {
                self.options.trim = v;
            }
            if let DialogField::Check(_, v) = fields[13] {
                self.options.formulas = v;
            }
            self.options.extra_keys = if self.options.key.is_some() {
                extra
            } else {
                Vec::new()
            };
            self.options.unimportant_columns = unimportant;
            self.rebuild();
            self.scroll_y = 0.0;
        }
    }

    /// BC5 `TTableColHandlingDialog` (`TTABLECOLHANDLINGDIALOG.json`):
    /// per-column Key, Use Default, General/Text, Unimportant, Case, Whitespace,
    /// Date/Time tolerance (s/min/h/d) and Numeric tolerance (abs/%).
    fn configure_column(&mut self, column: usize, dialogs: &mut dyn DialogHost) {
        if !self.commit_editor(dialogs) || column >= self.columns.len() {
            return;
        }
        let is_key = self.options.is_key(column);
        let use_default = !self.options.col_handlings.contains_key(&column);
        let h = self.options.col_handling(column);
        let (dt_val, dt_unit) = if h.datetime_tolerance_secs > 0
            && h.datetime_tolerance_secs.is_multiple_of(86_400)
        {
            (h.datetime_tolerance_secs / 86_400, 3)
        } else if h.datetime_tolerance_secs > 0 && h.datetime_tolerance_secs.is_multiple_of(3_600) {
            (h.datetime_tolerance_secs / 3_600, 2)
        } else if h.datetime_tolerance_secs > 0 && h.datetime_tolerance_secs.is_multiple_of(60) {
            (h.datetime_tolerance_secs / 60, 1)
        } else {
            (h.datetime_tolerance_secs, 0)
        };
        let mut fields = [
            DialogField::Check(tr("table.key_column"), is_key),
            DialogField::Check(tr("table.use_session_default"), use_default),
            DialogField::Choice(
                tr("table.type"),
                usize::from(h.as_text),
                ["common.general", "common.text"].map(tr).to_vec(),
            ),
            DialogField::Check(tr("table.unimportant"), h.unimportant),
            DialogField::Check(
                tr("table.ignore_case_abc_abc"),
                h.ignore_case,
            ),
            DialogField::Check(tr("common.ignore_whitespace"), h.ignore_whitespace),
            DialogField::Text(tr("table.date_time_tolerance"), dt_val.to_string()),
            DialogField::Choice(
                tr("table.date_time_unit"),
                dt_unit,
                ["common.seconds", "common.minutes", "common.hours", "common.days"]
                    .map(tr)
                    .to_vec(),
            ),
            DialogField::Text(tr("table.numeric_tolerance"), h.num_tolerance.clone()),
            DialogField::Choice(
                tr("table.numeric_tolerance_type"),
                usize::from(h.num_tolerance_relative),
                ["table.tolerance.absolute", "table.tolerance.relative"]
                    .map(tr)
                    .to_vec(),
            ),
        ];
        let title = format!("{} — {}", tr("table.column"), self.heading(true, column));
        if !dialogs.fields(&title, &mut fields) {
            return;
        }
        let next_key = matches!(fields[0], DialogField::Check(_, true));
        let next_default = matches!(fields[1], DialogField::Check(_, true));
        let next_unimportant = matches!(fields[3], DialogField::Check(_, true));
        let next_tol = if let DialogField::Text(_, ref s) = fields[8] {
            s.clone()
        } else {
            "0".into()
        };
        if !next_default
            && table_val::parse_tolerance(&next_tol, self.options.default_regional()).is_none()
        {
            dialogs.error("table.invalid_numeric_tolerance");
            return;
        }
        let dt_num = if let DialogField::Text(_, ref s) = fields[6] {
            match s.trim().parse::<u64>() {
                Ok(v) => v,
                Err(_) => {
                    dialogs.error("error.table.invalid_datetime_tol");
                    return;
                }
            }
        } else {
            0
        };
        let mult = match fields[7] {
            DialogField::Choice(_, 1, _) => 60,
            DialogField::Choice(_, 2, _) => 3_600,
            DialogField::Choice(_, 3, _) => 86_400,
            _ => 1,
        };
        self.options.set_key(column, next_key);
        if next_default {
            self.options.col_handlings.remove(&column);
            if next_unimportant {
                if !self.options.unimportant_columns.contains(&column) {
                    self.options.unimportant_columns.push(column);
                }
            } else {
                self.options.unimportant_columns.retain(|&c| c != column);
            }
        } else {
            self.options.unimportant_columns.retain(|&c| c != column);
            self.options.col_handlings.insert(
                column,
                ColHandling {
                    as_text: matches!(fields[2], DialogField::Choice(_, 1, _)),
                    unimportant: next_unimportant,
                    ignore_case: matches!(fields[4], DialogField::Check(_, true)),
                    ignore_whitespace: matches!(fields[5], DialogField::Check(_, true)),
                    num_tolerance: next_tol,
                    num_tolerance_relative: matches!(fields[9], DialogField::Choice(_, 1, _)),
                    datetime_tolerance_secs: dt_num.saturating_mul(mult),
                },
            );
        }
        self.rebuild();
    }

    fn format(&mut self, dialogs: &mut dyn DialogHost) {
        if !self.commit_editor(dialogs) {
            return;
        }
        let names = [
            "app.language.auto",
            "table.comma",
            "table.semicolon",
            "table.tab",
            "table.pipe",
            "table.delim.space_symbol",
            "table.delim.backslash",
            "table.delim.slash",
            "table.delim.caret",
            "table.delim.tilde",
            "table.delim.backtick",
            "table.delim.equals",
        ]
        .map(tr)
        .to_vec();
        let delimiters = [
            None,
            Some(b','),
            Some(b';'),
            Some(b'\t'),
            Some(b'|'),
            Some(b' '),
            Some(b'\\'),
            Some(b'/'),
            Some(b'^'),
            Some(b'~'),
            Some(b'`'),
            Some(b'='),
        ];
        let qualifier_names = [
            "app.language.auto",
            "table.delim.double_quote",
            "table.delim.apostrophe",
            "common.none",
        ]
        .map(tr)
        .to_vec();
        let qualifiers = [
            Qualifier::Auto,
            Qualifier::Char(b'"'),
            Qualifier::Char(b'\''),
            Qualifier::None,
        ];
        let regional_names = [
            "table.session_default",
            "table.locale_auto",
            "table.decimal.dot",
            "table.locale_comma",
        ]
        .map(tr)
        .to_vec();

        let side_regional_idx = |fmt: &SideFormat| match fmt.regional {
            None => 0,
            Some(r) => match r.number_locale {
                NumberLocale::System => 1,
                NumberLocale::Dot => 2,
                NumberLocale::Comma => 3,
            },
        };
        let mut fields = vec![
            DialogField::Choice(
                tr("table.left_delimiter"),
                delimiters
                    .iter()
                    .position(|d| *d == self.options.delimiters[0])
                    .unwrap_or(0),
                names.clone(),
            ),
            DialogField::Choice(
                tr("table.right_delimiter"),
                delimiters
                    .iter()
                    .position(|d| *d == self.options.delimiters[1])
                    .unwrap_or(0),
                names,
            ),
            DialogField::Choice(
                tr("table.left_text_qualifier"),
                qualifiers
                    .iter()
                    .position(|q| *q == self.options.formats[0].qualifier)
                    .unwrap_or(0),
                qualifier_names.clone(),
            ),
            DialogField::Choice(
                tr("table.right_text_qualifier"),
                qualifiers
                    .iter()
                    .position(|q| *q == self.options.formats[1].qualifier)
                    .unwrap_or(0),
                qualifier_names,
            ),
            DialogField::Check(
                tr("table.treat_consecutive_delimiters_as_one"),
                self.options.formats[0].consecutive_delimiters_as_one
                    || self.options.formats[1].consecutive_delimiters_as_one,
            ),
            DialogField::Check(
                tr("table.ignore_spaces_around_delimiter"),
                self.options.formats[0].trim_delimiter_whitespace
                    || self.options.formats[1].trim_delimiter_whitespace,
            ),
            DialogField::Text(
                tr("table.left_fixed_widths_e_g_10_20_12_empty"),
                self.options.formats[0].widths_string(),
            ),
            DialogField::Text(
                tr("table.right_fixed_widths_e_g_10_20_12_empty"),
                self.options.formats[1].widths_string(),
            ),
            DialogField::Choice(
                tr("table.left_numeric_format"),
                side_regional_idx(&self.options.formats[0]),
                regional_names.clone(),
            ),
            DialogField::Choice(
                tr("table.right_numeric_format"),
                side_regional_idx(&self.options.formats[1]),
                regional_names,
            ),
        ];
        if !dialogs.fields("table.file_format", &mut fields) {
            return;
        }
        let widths_l = match &fields[6] {
            DialogField::Text(_, s) => match SideFormat::parse_widths(s) {
                Ok(w) => w,
                Err(e) => {
                    dialogs.error(&e);
                    return;
                }
            },
            _ => Vec::new(),
        };
        let widths_r = match &fields[7] {
            DialogField::Text(_, s) => match SideFormat::parse_widths(s) {
                Ok(w) => w,
                Err(e) => {
                    dialogs.error(&e);
                    return;
                }
            },
            _ => Vec::new(),
        };
        let consecutive = matches!(fields[4], DialogField::Check(_, true));
        let trim_ws = matches!(fields[5], DialogField::Check(_, true));
        let parse_regional = |field: &DialogField, date_order| match field {
            DialogField::Choice(_, 1, _) => Some(RegionalSettings {
                date_order,
                number_locale: NumberLocale::System,
            }),
            DialogField::Choice(_, 2, _) => Some(RegionalSettings {
                date_order,
                number_locale: NumberLocale::Dot,
            }),
            DialogField::Choice(_, 3, _) => Some(RegionalSettings {
                date_order,
                number_locale: NumberLocale::Comma,
            }),
            _ => None,
        };
        let mut next_formats = self.options.formats.clone();
        next_formats[0].fixed_widths = widths_l;
        next_formats[1].fixed_widths = widths_r;
        for i in 0..2 {
            if let DialogField::Choice(_, q_idx, _) = fields[2 + i] {
                next_formats[i].qualifier = qualifiers[q_idx];
            }
            next_formats[i].consecutive_delimiters_as_one = consecutive;
            next_formats[i].trim_delimiter_whitespace = trim_ws;
            next_formats[i].regional = parse_regional(&fields[8 + i], self.options.date_order);
        }

        let mut sides = [self.left.clone(), self.right.clone()];
        for (i, side) in sides.iter_mut().enumerate() {
            if let DialogField::Choice(_, index, _) = fields[i] {
                let delimiter = delimiters[index];
                side.format = next_formats[i].clone();
                if let Some(doc) = &side.document {
                    let (det_delim, det_quote) = table_formats::detect_delimiter_and_qualifier(
                        &doc.text,
                        Path::new(&side.path),
                    );
                    side.delimiter = delimiter.unwrap_or(det_delim);
                    side.quote = det_quote;
                    if let Err(error) = side.reparse() {
                        dialogs.error(&error);
                        return;
                    }
                }
            }
        }
        [self.left, self.right] = sides;
        for (i, field) in fields.iter().enumerate().take(2) {
            if let DialogField::Choice(_, index, _) = *field {
                self.options.delimiters[i] = delimiters[index];
            }
        }
        self.options.formats = next_formats;
        self.apply_first_line_detection();
        self.rebuild();
    }
    fn reload(&mut self, dialogs: &mut dyn DialogHost) {
        if !self.confirm_saved(dialogs) {
            return;
        }
        if !self.left.path.is_empty() && !self.right.path.is_empty() {
            let paths = [
                PathBuf::from(&self.left.path),
                PathBuf::from(&self.right.path),
            ];
            if let Err(error) = self.load_files(&paths[0], &paths[1]) {
                dialogs.error(&error);
            }
        } else if !self.side(self.active_left).path.is_empty() {
            self.open_side(
                self.active_left,
                &PathBuf::from(&self.side(self.active_left).path),
                dialogs,
            );
        }
    }
    fn heading(&self, left: bool, column: usize) -> String {
        let Some(column) = self.physical_column(left, column) else {
            return "—".into();
        };
        if self.options.header {
            if let Some(value) = self
                .side(left)
                .table
                .rows
                .first()
                .and_then(|r| r.cells.get(column))
                .filter(|v| !v.is_empty())
            {
                return value.clone();
            }
        }
        column_letter(column + self.side(left).table.column_origin)
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
        // Native dialogs and the shared session tree keep the same modality as
        // the other comparison screens. Unsupported commands stay unavailable.
        TopBottomPanel::top("table_menus")
            .exact_height(chrome::MENU_BAND_HEIGHT)
            .show_separator_line(false)
            .frame(Frame::none())
            .show_inside(ui, |ui| {
                chrome::menu_background(ui);
                chrome::menu_bar(ui, |ui| {
                    chrome::menu_button(ui, "menu.session", |ui| {
                        session_menu::prefix(ui, icons, true);
                        if chrome::menu_item(ui, icons, "menu.session.settings", true)
                            .clicked()
                        {
                            self.rules(dialogs);
                            ui.close_menu();
                        }
                        session_menu::close(ui, icons);
                        ui.separator();
                        if chrome::menu_item(ui, icons, "table.report", true).clicked() {
                            self.report(dialogs);
                            ui.close_menu();
                        }
                        if chrome::menu_item(ui, icons, "table.compare_info", true).clicked() {
                            self.open_info_modal(ctx);
                            ui.close_menu();
                        }
                        session_menu::suffix(
                            ui,
                            icons,
                            !self.left.path.is_empty() && !self.right.path.is_empty(),
                        );
                    });
                    chrome::menu_button(ui, "menu.file", |ui| {
                        if chrome::menu_item(ui, icons, "menu.file.open_ellipsis  Ctrl+O", true).clicked()
                        {
                            self.pick_file(self.active_left, dialogs);
                            ui.close_menu();
                        }
                        if chrome::menu_item(
                            ui,
                            icons,
                            "menu.file.save  Ctrl+S",
                            self.side(self.active_left).dirty() || self.editor_changed(),
                        )
                        .clicked()
                        {
                            self.save_side(self.active_left, false, dialogs);
                            ui.close_menu();
                        }
                        if chrome::menu_item(
                            ui,
                            icons,
                            "menu.file.save_as_short",
                            self.side(self.active_left).editable() && !self.read_only,
                        )
                        .clicked()
                        {
                            self.save_side(self.active_left, true, dialogs);
                            ui.close_menu();
                        }
                        ui.separator();
                        if chrome::menu_item(ui, icons, "toolbar.swap_sides", true).clicked() {
                            self.swap(dialogs);
                            ui.close_menu();
                        }
                        if chrome::menu_item(
                            ui,
                            icons,
                            "menu.file.reload_files  F5",
                            !self.left.path.is_empty() || !self.right.path.is_empty(),
                        )
                        .clicked()
                        {
                            self.reload(dialogs);
                            ui.close_menu();
                        }
                    });
                    chrome::menu_button(ui, "menu.edit", |ui| {
                        if chrome::menu_item(
                            ui,
                            icons,
                            "common.undo  Ctrl+Z",
                            !self.undo.is_empty() && !self.read_only,
                        )
                        .clicked()
                        {
                            self.restore(false);
                            ui.close_menu();
                        }
                        if chrome::menu_item(
                            ui,
                            icons,
                            "common.redo  Ctrl+Y",
                            !self.redo.is_empty() && !self.read_only,
                        )
                        .clicked()
                        {
                            self.restore(true);
                            ui.close_menu();
                        }
                        ui.separator();
                        if chrome::menu_item(
                            ui,
                            icons,
                            "table.edit_cell  F2",
                            self.selected_row(self.active_left).is_some()
                                && self.side(self.active_left).editable()
                                && !self.read_only,
                        )
                        .clicked()
                        {
                            self.begin_edit(dialogs);
                            ui.close_menu();
                        }
                        if chrome::menu_item(
                            ui,
                            icons,
                            "table.copy_row_other",
                            self.can_copy(),
                        )
                        .clicked()
                        {
                            self.copy_row(dialogs);
                            ui.close_menu();
                        }
                        if chrome::menu_item(
                            ui,
                            icons,
                            "table.copy_cell_other",
                            self.can_copy_cell(),
                        )
                        .clicked()
                        {
                            self.copy_cell(dialogs);
                            ui.close_menu();
                        }
                        if chrome::menu_item(
                            ui,
                            icons,
                            "table.copy_cells_right",
                            self.can_copy_cell(),
                        )
                        .clicked()
                        {
                            self.active_left = true;
                            self.copy_cell(dialogs);
                            ui.close_menu();
                        }
                        if chrome::menu_item(
                            ui,
                            icons,
                            "table.copy_cells_left",
                            self.can_copy_cell(),
                        )
                        .clicked()
                        {
                            self.active_left = false;
                            self.copy_cell(dialogs);
                            ui.close_menu();
                        }
                        if chrome::menu_item(
                            ui,
                            icons,
                            "menu.edit.select_all  Ctrl+A",
                            !self.visible.is_empty(),
                        )
                        .clicked()
                        {
                            self.select_all_visible();
                            ui.close_menu();
                        }
                    });
                    chrome::menu_button(ui, "menu.search", |ui| {
                        if chrome::menu_item(ui, icons, "menu.search.find  Ctrl+F", true).clicked() {
                            self.show_search = true;
                            ui.close_menu();
                        }
                        if chrome::menu_item(ui, icons, "table.next_diff_cell", true).clicked() {
                            self.navigate_diff_cell(true);
                            ui.close_menu();
                        }
                        if chrome::menu_item(ui, icons, "table.prev_diff_cell", true).clicked() {
                            self.navigate_diff_cell(false);
                            ui.close_menu();
                        }
                        if chrome::menu_item(ui, icons, "toolbar.next_diff", true).clicked() {
                            self.navigate(true);
                            ui.close_menu();
                        }
                        if chrome::menu_item(ui, icons, "toolbar.prev_diff", true).clicked() {
                            self.navigate(false);
                            ui.close_menu();
                        }
                    });
                    chrome::menu_button(ui, "menu.view", |ui| {
                        for (label, filter) in [
                            ("common.all", Filter::All),
                            ("common.differences", Filter::Differences),
                            ("common.same", Filter::Same),
                        ] {
                            if chrome::menu_radio_item(
                                ui,
                                icons,
                                label,
                                self.options.filter == filter,
                                true,
                            )
                            .clicked()
                            {
                                self.set_filter(filter);
                                ui.close_menu();
                            }
                        }
                        ui.separator();
                        if chrome::menu_check_item(
                            ui,
                            icons,
                            "table.ignore_unimportant",
                            self.options.ignore_minor,
                            true,
                        )
                        .clicked()
                        {
                            self.options.ignore_minor = !self.options.ignore_minor;
                            self.rebuild();
                            ui.close_menu();
                        }
                        if chrome::menu_check_item(
                            ui,
                            icons,
                            "options.hide_same_columns",
                            self.options.hide_same_columns,
                            true,
                        )
                        .clicked()
                        {
                            self.options.hide_same_columns = !self.options.hide_same_columns;
                            ui.close_menu();
                        }
                        if chrome::menu_check_item(
                            ui,
                            icons,
                            "table.row_numbers",
                            self.show_row_numbers,
                            true,
                        )
                        .clicked()
                        {
                            self.show_row_numbers = !self.show_row_numbers;
                            ui.close_menu();
                        }
                        if chrome::menu_item(ui, icons, "table.resize_columns_to_fit", true)
                            .clicked()
                        {
                            self.resize_columns_to_fit();
                            ui.close_menu();
                        }
                        ui.separator();
                        if chrome::menu_item(
                            ui,
                            icons,
                            "view.increase_font_size  Ctrl++",
                            self.font_size < 24.0,
                        )
                        .clicked()
                        {
                            self.font_size = (self.font_size + 1.0).clamp(9.0, 24.0);
                            ui.close_menu();
                        }
                        if chrome::menu_item(
                            ui,
                            icons,
                            "view.decrease_font_size  Ctrl+-",
                            self.font_size > 9.0,
                        )
                        .clicked()
                        {
                            self.font_size = (self.font_size - 1.0).clamp(9.0, 24.0);
                            ui.close_menu();
                        }
                        if chrome::menu_item(
                            ui,
                            icons,
                            "view.reset_font_size  Ctrl+0",
                            (self.font_size - 12.0).abs() > 0.1,
                        )
                        .clicked()
                        {
                            self.font_size = 12.0;
                            ui.close_menu();
                        }
                        ui.separator();
                        if chrome::menu_radio_item(
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
                        if chrome::menu_radio_item(
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
                        if chrome::menu_check_item(
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
                        if chrome::menu_check_item(
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
                    chrome::menu_button(ui, "menu.tools", |ui| {
                        if chrome::menu_item(ui, icons, "menu.tools.options", true).clicked() {
                            if dialogs.persist_settings() {
                                super::options_dialog::request(
                                    ui.ctx(),
                                    super::options_dialog::OptionsCategory::TableCompare,
                                );
                            } else {
                                crate::i18n::configure(dialogs);
                            }
                            ui.close_menu();
                        }
                        if chrome::menu_item(ui, icons, "menu.tools.file_formats", true).clicked() {
                            self.format(dialogs);
                            ui.close_menu();
                        }
                        session_menu::settings_tools(ui, icons, dialogs);
                    });
                    chrome::menu_button(ui, "menu.help", |ui| {
                        if chrome::menu_item(ui, icons, "menu.help.about", true).clicked() {
                            super::info_modals::open_about(ui.ctx());
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
            "SessionHome" | "SessionsCombo" | "Rules" | "FileFormat" | "Swap" => {
                ToolState::enabled(true)
            }
            "CopySide" => ToolState::enabled(self.can_copy()),
            "DiffItemNext" | "DiffItemPrev" => ToolState::enabled(self.visible.iter().any(|&i| {
                if self.options.ignore_minor {
                    self.pairs[i].important
                } else {
                    self.pairs[i].different
                }
            })),
            "Reload" => {
                ToolState::enabled(!self.left.path.is_empty() || !self.right.path.is_empty())
            }
            _ => ToolState::DISABLED,
        };
        let mut event = None;
        if self.show_toolbar {
            let height = toolbar::height(
                ui,
                toolbar_spec::TABLE_COMPARE,
                ui.available_width(),
                &state,
            );
            TopBottomPanel::top("table_toolbar")
                .exact_height(height + 3.0)
                .show_separator_line(false)
                .frame(Frame::none())
                .show_inside(ui, |ui| {
                    #[cfg(test)]
                    ui.ctx().data_mut(|data| {
                        data.insert_temp(Id::new("table_test_toolbar"), ui.max_rect())
                    });
                    ui.data_mut(|d| d.insert_temp(Id::new("bc5_flat_toolbar"), true));
                    event = toolbar::show(
                        ui,
                        icons,
                        toolbar_spec::TABLE_COMPARE,
                        self.active_left,
                        &state,
                        &mut |_, _, _| {},
                    );
                    ui.data_mut(|d| d.remove::<bool>(Id::new("bc5_flat_toolbar")));
                });
        }
        if let Some(event) = event {
            match event {
                ToolEvent::Click("SessionHome") => home = true,
                ToolEvent::Click("FavCompareAll") => self.set_filter(Filter::All),
                ToolEvent::Click("FavDiff") => self.set_filter(Filter::Differences),
                ToolEvent::Click("FavSame") => self.set_filter(Filter::Same),
                ToolEvent::Click("InsigIgnored") => {
                    self.options.ignore_minor = !self.options.ignore_minor;
                    self.rebuild();
                }
                ToolEvent::Click("Rules") => self.rules(dialogs),
                ToolEvent::Click("FileFormat") | ToolEvent::Arrow("FileFormat") => {
                    self.format(dialogs)
                }
                ToolEvent::Click("CopySide") => self.copy_row(dialogs),
                ToolEvent::Click("DiffItemNext") => self.navigate(true),
                ToolEvent::Click("DiffItemPrev") => self.navigate(false),
                ToolEvent::Click("Swap") => self.swap(dialogs),
                ToolEvent::Click("Reload") => self.reload(dialogs),
                _ => {}
            }
        }
        if !super::session_picker::is_open(ctx)
            && ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::S))
        {
            self.save_side(self.active_left, false, dialogs);
        }
        if !ctx.wants_keyboard_input()
            && !super::session_picker::is_open(ctx)
            && !ctx.is_context_menu_open()
        {
            self.cell_shortcuts(ctx, dialogs);
            if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::O)) {
                self.pick_file(self.active_left, dialogs);
            }
            if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::F)) {
                self.show_search = true;
            }
            if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::F2)) {
                self.begin_edit(dialogs);
            }
            if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::F5)) {
                self.reload(dialogs);
            }
            if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::F3)) {
                self.find(true);
            }
            if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::Z)) && !self.read_only {
                self.restore(false);
            }
            if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::Y)) && !self.read_only {
                self.restore(true);
            }
            for (key, delta, vertical) in [
                (Key::ArrowDown, 1, true),
                (Key::ArrowUp, -1, true),
                (Key::ArrowRight, 1, false),
                (Key::ArrowLeft, -1, false),
            ] {
                let extend = ctx.input_mut(|i| i.consume_key(Modifiers::SHIFT, key));
                if extend || ctx.input_mut(|i| i.consume_key(Modifiers::NONE, key)) {
                    self.move_focus(delta, vertical, extend);
                }
            }
        }
        if self.show_search {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                ui.label(tr("hex.find"));
                let response = ui.add_sized(
                    [ui.available_width().min(450.0), 22.0],
                    TextEdit::singleline(&mut self.search),
                );
                if response.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                    self.find(true);
                }
                if ui.button(tr("common.previous")).clicked() {
                    self.find(false);
                }
                if ui.button(tr("common.next")).clicked() {
                    self.find(true);
                }
                if ui.button("×").clicked() {
                    self.show_search = false;
                }
                if self.search_notice {
                    ui.label(tr("table.text_not_found"));
                }
            });
        }
        let overview_top = ui.cursor().min;
        self.path_bar(ui, icons, dialogs);
        self.metadata_bar(ui, dialogs);
        if let Some(error) = &self.error {
            ui.colored_label(Color32::RED, error);
        }
        let sheets_height = if self.left.sheets.is_empty() && self.right.sheets.is_empty() {
            0.0
        } else {
            26.0
        };
        let details_height = self.detail_height;
        let height = (ui.available_height() - details_height - sheets_height - 20.0).max(50.0);
        if self.jump {
            if let Some(position) = self.visible.iter().position(|&i| i == self.selected) {
                let top = position as f32 * ROW_HEIGHT;
                let viewport = (height - 40.0).max(ROW_HEIGHT);
                if top < self.scroll_y {
                    self.scroll_y = top;
                } else if top + ROW_HEIGHT > self.scroll_y + viewport {
                    self.scroll_y = top + ROW_HEIGHT - viewport;
                }
            }
        }
        let (area, _) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
        #[cfg(test)]
        ctx.data_mut(|d| d.insert_temp(Id::new("table_test_grid"), area));
        let mut clicked = self.drag_selection(ctx, area);
        let mut next_scroll = self.scroll_y;
        ctx.data_mut(|d| d.remove::<Rect>(table_actions::editor_rect_id()));
        for (i, rect) in table_grid::pane_rects(area).into_iter().enumerate() {
            let left = i == 0;
            ui.painter().rect_filled(rect, 0.0, Color32::WHITE);
            table_grid::overlay(ui, UiBuilder::new().max_rect(rect), |ui| {
                ui.set_clip_rect(rect);
                let (hit, scroll, hovered) = self.pane(ui, left, height, icons);
                if let Some(hit) = hit {
                    clicked = Some(hit);
                }
                if hovered {
                    next_scroll = scroll;
                }
            });
        }
        let header_action = ctx.data_mut(|d| {
            let action = d.get_temp::<HeaderAction>(header_action_id());
            d.remove::<HeaderAction>(header_action_id());
            action
        });
        if let Some(action) = header_action {
            if self.commit_editor(dialogs) {
                match action {
                    HeaderAction::Resize(column, width) => {
                        if self.options.widths.len() < self.columns.len() {
                            self.options.widths =
                                self.column_layout().iter().map(|(_, _, w)| *w).collect();
                            self.options.widths.resize(self.columns.len(), COLUMN_WIDTH);
                        }
                        self.options.widths[column] = width;
                    }
                    HeaderAction::Sort(column) => {
                        let ascending = self
                            .options
                            .sort
                            .is_none_or(|(old, asc)| old != column || !asc);
                        self.options.sort = Some((column, ascending));
                        self.rebuild();
                    }
                    HeaderAction::Unimportant(column) => {
                        if self.options.unimportant_columns.contains(&column) {
                            self.options.unimportant_columns.retain(|&c| c != column);
                        } else {
                            self.options.unimportant_columns.push(column);
                        }
                        self.rebuild();
                    }
                    HeaderAction::Key(column) => {
                        let currently_key = self.options.is_key(column);
                        self.options.set_key(column, !currently_key);
                        self.rebuild();
                    }
                    HeaderAction::Configure(column) => {
                        self.configure_column(column, dialogs);
                    }
                }
                ctx.request_repaint();
            }
        }
        if (next_scroll - self.scroll_y).abs() > 0.1 {
            self.scroll_y = next_scroll;
            ctx.request_repaint();
        }
        self.jump = false;
        if let Some((left, index, column, gesture)) = clicked {
            let physical = self
                .pairs
                .get(index)
                .and_then(|p| if left { p.left } else { p.right });
            if self.commit_editor(dialogs) {
                let pair = physical
                    .and_then(|row| {
                        self.pairs.iter().position(|p| {
                            if left {
                                p.left == Some(row)
                            } else {
                                p.right == Some(row)
                            }
                        })
                    })
                    .unwrap_or(index.min(self.pairs.len().saturating_sub(1)));
                self.select_cell(left, pair, column, gesture);
                ctx.request_repaint();
                if let Some((origin_left, origin)) = self.align_pending {
                    if origin_left != left {
                        if let Some(row) = physical {
                            self.align_rows(
                                if left { [row, origin] } else { [origin, row] },
                                dialogs,
                            );
                        }
                    }
                } else if gesture == CellGesture::Edit {
                    self.begin_edit(dialogs);
                }
            }
        }
        if let Some(request) = ctx.data_mut(|d| {
            let request = d.get_temp::<table_actions::CellRequest>(table_actions::request_id());
            d.remove::<table_actions::CellRequest>(table_actions::request_id());
            request
        }) {
            if self.commit_editor(dialogs) {
                self.select_cell(
                    request.left,
                    request.pair,
                    request.column,
                    CellGesture::Context,
                );
                self.run_cell_command(ctx, request.command, dialogs);
            }
        }
        if !ctx.input(|i| i.focused && i.pointer.primary_down()) {
            self.selection.dragging = false;
        }
        self.inline_editor(ui, dialogs);
        self.details(ui);
        let overview_area = Rect::from_min_max(
            overview_top + vec2(4.0, 7.0),
            pos2(
                overview_top.x + OVERVIEW_WIDTH - 4.0,
                ui.cursor().min.y - 7.0,
            ),
        );
        self.overview(ui, overview_area, (height - 45.0).max(ROW_HEIGHT));
        self.sheet_tabs(ui, dialogs);
        self.window_status(ui, icons);
        home
    }
    fn can_copy(&self) -> bool {
        self.can_copy_rows_from(self.active_left, None)
    }

    fn can_copy_rows_from(&self, left: bool, cell: Option<(usize, usize)>) -> bool {
        !self.read_only
            && self.side(!left).editable()
            && self
                .cells_for_menu(left, cell)
                .any(|(pair, _)| self.row_at(left, pair).is_some())
            && (self.side(!left).table.rows.is_empty()
                || self
                    .columns
                    .iter()
                    .all(|c| c.side(left).is_none() || c.side(!left).is_some()))
    }

    fn can_copy_cell(&self) -> bool {
        self.can_copy_cells_from(self.active_left, None)
    }

    fn can_copy_cells_from(&self, left: bool, cell: Option<(usize, usize)>) -> bool {
        !self.read_only
            && self.side(!left).editable()
            && self.cells_for_menu(left, cell).any(|(pair, column)| {
                self.row_at(left, pair).is_some()
                    && self.row_at(!left, pair).is_some()
                    && self.physical_column(left, column).is_some()
                    && self.physical_column(!left, column).is_some()
            })
    }
    fn pick_file(&mut self, left: bool, dialogs: &mut dyn DialogHost) {
        let current = PathBuf::from(&self.side(left).path);
        if let Some(path) = dialogs.file(false, left, Some(&current)) {
            self.open_side(left, &path, dialogs);
        }
    }
    fn swap(&mut self, dialogs: &mut dyn DialogHost) {
        if !self.commit_editor(dialogs) {
            return;
        }
        std::mem::swap(&mut self.left, &mut self.right);
        self.paths.swap(0, 1);
        self.align_pending = None;
        for anchor in &mut self.options.manual_rows {
            anchor.swap(0, 1);
        }
        self.options.sheets.swap(0, 1);
        self.options.missing_sheets.swap(0, 1);
        self.options.delimiters.swap(0, 1);
        self.options.formats.swap(0, 1);
        self.options.column_alignment = match self.options.column_alignment {
            Alignment::LeftNames => Alignment::RightNames,
            Alignment::RightNames => Alignment::LeftNames,
            Alignment::Position => Alignment::Position,
        };
        for pair in &mut self.pairs {
            std::mem::swap(&mut pair.left, &mut pair.right);
        }
        self.active_left = !self.active_left;
        for state in self.undo.iter_mut().chain(&mut self.redo) {
            state.swap(0, 1);
        }
        self.rebuild();
    }
    fn change_sheet(&mut self, left: bool, sheet: usize, dialogs: &mut dyn DialogHost) {
        if !self.commit_editor(dialogs) {
            return;
        }
        match Side::load(Path::new(&self.side(left).path), None, sheet) {
            Ok(side) => {
                *self.side_mut(left) = side;
                self.options.sheets[usize::from(!left)] = sheet;
                self.options.missing_sheets[usize::from(!left)] = false;
                self.options.sheet_pair_explicit = true;
                self.options.manual_rows.clear();
                self.align_pending = None;
                self.rebuild();
                self.scroll_y = 0.0;
            }
            Err(error) => dialogs.error(&error),
        }
    }
    fn select_sheet_pair(&mut self, pair: Column, dialogs: &mut dyn DialogHost) {
        if !self.confirm_saved(dialogs) {
            return;
        }
        if let Err(error) = self.load_sheet_pair(pair) {
            dialogs.error(&error);
        }
    }
    pub fn capture_sheet(&mut self, index: usize) -> Result<(), String> {
        if self.dirty() {
            return Err(tr("table.unsaved_changes"));
        }
        let pair = self
            .sheet_pairs()
            .get(index)
            .copied()
            .ok_or_else(|| tr("table.error.sheet_not_found"))?;
        self.load_sheet_pair(pair)
    }
    fn load_sheet_pair(&mut self, pair: Column) -> Result<(), String> {
        let result = (|| -> Result<_, String> {
            Ok((
                crate::table_sheets::load(&self.left, pair.left)?,
                crate::table_sheets::load(&self.right, pair.right)?,
            ))
        })();
        match result {
            Ok((a, b)) => {
                self.left = a;
                self.right = b;
                self.options.sheets = [self.left.sheet, self.right.sheet];
                self.options.missing_sheets = [self.left.missing_sheet, self.right.missing_sheet];
                self.options.sheet_pair_explicit = true;
                self.options.manual_rows.clear();
                self.align_pending = None;
                self.scroll_y = 0.0;
                self.selected = 0;
                self.undo.clear();
                self.redo.clear();
                self.rebuild();
            }
            Err(error) => return Err(error),
        }
        Ok(())
    }
    fn sheet_pairs(&self) -> Vec<Column> {
        table_columns::sheets_aligned(
            &self.left.sheets,
            &self.right.sheets,
            self.options.sheet_alignment,
        )
    }
}

fn cell(ui: &Ui, rect: Rect, text: &str, color: Color32, background: Color32) {
    if !rect.intersects(ui.clip_rect()) {
        return;
    }
    let painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
    painter.rect_filled(rect, 0.0, background);
    let font = table_grid::font();
    // Avoid tessellating an entire multi-kilobyte translation for a 260px cell.
    let text: String = text
        .chars()
        .take(((rect.width() / 6.0).ceil() as usize + 4).min(512))
        .map(|c| if c == '\n' || c == '\r' { '↵' } else { c })
        .collect();
    painter.text(
        rect.left_center() + vec2(3.0, 0.0),
        Align2::LEFT_CENTER,
        text,
        font,
        color,
    );
    painter.line_segment(
        [rect.left_bottom(), rect.right_bottom()],
        Stroke::new(1.0_f32, Color32::from_gray(170)),
    );
    painter.line_segment(
        [rect.right_top(), rect.right_bottom()],
        Stroke::new(1.0_f32, Color32::from_gray(170)),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::dialogs::TestDialogs;
    fn pair() -> TableCompareScreen {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/tables");
        let mut screen = TableCompareScreen::new();
        screen.options.header = true;
        screen.options.key = Some(0);
        screen
            .load_files(&root.join("left.csv"), &root.join("right.csv"))
            .unwrap();
        screen
    }
    #[test]
    fn aligned_column_edits_copy_and_swap_use_physical_target_columns() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/pe");
        let mut screen = TableCompareScreen::new();
        screen.options.header = true;
        screen.options.key = Some(0);
        screen
            .load_files(
                &root.join("columns-left.csv"),
                &root.join("columns-right.csv"),
            )
            .unwrap();
        screen.options.column_alignment = Alignment::LeftNames;
        screen.rebuild();
        assert!(screen.pairs.iter().all(|p| !p.different));
        assert_eq!(screen.physical_column(false, 0), Some(1));
        screen.column = 1;
        let mut dialogs = TestDialogs::default();
        screen.begin_edit(&mut dialogs);
        screen.editor.as_mut().unwrap().value = "Edited".into();
        assert!(screen.commit_editor(&mut dialogs));
        screen.copy_cell(&mut dialogs);
        assert_eq!(screen.right.table.rows[1].cells, ["Olá", "a", "Edited"]);
        screen.column = 2;
        screen.copy_row(&mut dialogs);
        assert_eq!(screen.right.table.rows[1].cells, ["Olá", "a", "Edited"]);
        screen.swap(&mut dialogs);
        assert_eq!(screen.options.column_alignment, Alignment::RightNames);
        assert_eq!(screen.physical_column(true, 0), Some(1));
        assert!(screen.pairs.iter().all(|p| !p.different));
        assert!(dialogs.errors.is_empty());
    }
    #[test]
    fn composite_keys_tolerance_minor_filter_and_html_sheets_are_real_comparisons() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/pe");
        let mut screen = TableCompareScreen::new();
        screen.options.header = true;
        screen.options.key = Some(0);
        screen
            .load_files(
                &root.join("composite-left.csv"),
                &root.join("composite-right.csv"),
            )
            .unwrap();
        screen.options.extra_keys = vec![1];
        screen.rebuild();
        assert_eq!(screen.pairs.iter().filter(|p| p.different).count(), 1);
        screen
            .load_files(
                &root.join("numbers-left.csv"),
                &root.join("numbers-right.csv"),
            )
            .unwrap();
        screen.options.extra_keys.clear();
        screen.options.numeric = true;
        screen.options.tolerance = "0.00000000000000000001".into();
        screen.options.unimportant_columns = vec![2];
        screen.rebuild();
        assert_eq!(screen.pairs.iter().filter(|p| p.important).count(), 1);
        screen.options.ignore_minor = true;
        screen.set_filter(Filter::Differences);
        assert_eq!(screen.visible.len(), 1);
        screen.options = Options::default();
        screen
            .load_files(
                &root.join("multi-left.html"),
                &root.join("multi-right.html"),
            )
            .unwrap();
        let sheets = table_columns::sheets(&screen.left.sheets, &screen.right.sheets);
        assert_eq!(sheets.len(), 3);
        assert_eq!(
            sheets[0],
            Column {
                left: Some(0),
                right: Some(1)
            }
        );
        let summary =
            crate::table_sheets::compare(&screen.left, &screen.right, &screen.options, &sheets);
        assert_eq!(
            summary.iter().map(|s| s.important).collect::<Vec<_>>(),
            [0, 1, 2]
        );
        let mut dialogs = TestDialogs::default();
        screen.select_sheet_pair(sheets[2], &mut dialogs);
        assert!(screen.left.missing_sheet);
        assert!(screen
            .right
            .table
            .rows
            .iter()
            .any(|r| r.cells.contains(&"orphan".into())));
        let options = screen.session_options();
        let mut restored = TableCompareScreen::new();
        restored.apply_session_options(&options);
        restored
            .load_files(
                &root.join("multi-left.html"),
                &root.join("multi-right.html"),
            )
            .unwrap();
        assert!(restored.left.missing_sheet);
        assert_eq!(restored.right.sheet, 2);
        screen.select_sheet_pair(sheets[0], &mut dialogs);
        assert!(!screen.left.missing_sheet);
        assert!(screen.pairs.iter().all(|p| !p.different));
    }
    #[test]
    fn edit_undo_redo_copy_and_cancel_close_are_real_document_operations() {
        let mut screen = pair();
        let original = screen.left.document.as_ref().unwrap().text.clone();
        let mut dialogs = TestDialogs::default();
        screen.column = 1;
        screen.begin_edit(&mut dialogs);
        screen.editor.as_mut().unwrap().value = "edited\nwith multiple lines".into();
        assert!(screen.dirty());
        dialogs.choices.push_back(SaveChoice::Cancel);
        assert!(!screen.confirm_saved(&mut dialogs));
        assert_eq!(
            screen.left.table.rows[2].cells[1],
            "edited\nwith multiple lines"
        );
        screen.restore(false);
        assert_eq!(screen.left.document.as_ref().unwrap().text, original);
        assert!(!screen.dirty());
        screen.restore(true);
        assert!(screen.dirty());
        screen.copy_row(&mut dialogs);
        assert_eq!(
            screen.right.table.rows[1].cells[1],
            "edited\nwith multiple lines"
        );
        assert!(screen.right.dirty());
        let left = screen.left.document.as_ref().unwrap().text.clone();
        screen.open_side(true, Path::new("missing.xlsx"), &mut dialogs);
        assert_eq!(screen.left.document.as_ref().unwrap().text, left);
        assert_eq!(dialogs.errors.len(), 1);
    }
    #[test]
    fn workbook_changes_and_session_options_survive_restore() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/tables");
        let mut screen = pair();
        let mut dialogs = TestDialogs::default();
        screen.open_side(true, &root.join("left.xlsx"), &mut dialogs);
        screen.change_sheet(true, 1, &mut dialogs);
        assert_eq!(screen.left.table.rows[1].cells[1], "Other worksheet");
        screen.begin_edit(&mut dialogs);
        assert!(screen.editor.is_none());
        screen.options.key = None;
        screen.options.header = false;
        let saved = screen.session_options();
        let mut restored = TableCompareScreen::new();
        restored.apply_session_options(&saved);
        restored
            .load_files(&root.join("left.xlsx"), &root.join("right.csv"))
            .unwrap();
        assert_eq!(restored.left.sheet, 1);
        assert_eq!(restored.options.key, None);
        assert!(!restored.options.header);
        assert!(!restored.save_side(true, false, &mut dialogs));
        assert!(dialogs.errors.is_empty());
    }
    #[test]
    fn filters_navigation_and_search_include_offscreen_columns() {
        let mut screen = pair();
        screen.set_filter(Filter::Differences);
        assert_eq!(screen.visible.len(), 4);
        screen.set_filter(Filter::Same);
        assert_eq!(screen.visible.len(), 1);
        screen.set_filter(Filter::All);
        screen.navigate(true);
        assert!(screen.pairs[screen.selected].different);
        screen.search = "coração".into();
        screen.find(true);
        assert_eq!(screen.column, 2);
        assert!(screen.jump);
        screen.search = "no such string".into();
        screen.find(false);
        assert!(screen.search_notice);
    }
    #[test]
    fn copying_to_an_empty_csv_includes_the_header_and_preserves_the_data_row() {
        let mut screen = pair();
        screen.right = Side {
            document: Some(crate::document::Document::default()),
            delimiter: b',',
            ..Side::default()
        };
        screen.rebuild();
        let source = screen.selected_row(true).unwrap();
        let expected = screen.left.table.rows[source].cells.clone();
        let mut dialogs = TestDialogs::default();
        screen.copy_row(&mut dialogs);
        assert_eq!(screen.right.table.rows.len(), 2);
        assert_eq!(screen.right.table.rows[0].cells[0], "ID");
        assert_eq!(screen.right.table.rows[1].cells, expected);
        assert!(dialogs.errors.is_empty());
    }
    #[test]
    fn saves_only_changed_side_with_conflict_and_alias_protection() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join(".codex/table-tests/saving");
        std::fs::create_dir_all(&root).unwrap();
        let a = root.join("a.csv");
        let b = root.join("b.csv");
        let original = b"ID,Text\r\na,original\r\n";
        std::fs::write(&a, original).unwrap();
        std::fs::write(&b, original).unwrap();
        let mut screen = TableCompareScreen::new();
        screen.load_files(&a, &b).unwrap();
        let mut dialogs = TestDialogs::default();
        screen.left.edit_cell(1, 1, "changed").unwrap();
        screen.rebuild();
        dialogs.files.push_back(Some(b.clone()));
        assert!(!screen.save_side(true, true, &mut dialogs));
        assert_eq!(std::fs::read(&b).unwrap(), original);
        std::fs::write(&a, b"external edit").unwrap();
        assert!(!screen.save_side(true, false, &mut dialogs));
        assert_eq!(std::fs::read(&a).unwrap(), b"external edit");
        assert_eq!(dialogs.errors.len(), 2);
    }
    #[test]
    fn table_grid_renders_without_expanding_the_window_at_supported_sizes() {
        for size in [[700.0, 450.0], [1150.0, 720.0], [1600.0, 1000.0]] {
            let ctx = Context::default();
            super::super::theme::apply(&ctx);
            let mut screen = pair();
            let mut icons = IconManager::new();
            let mut dialogs = TestDialogs::default();
            for editing in [false, true] {
                if editing {
                    screen.column = 1;
                    screen.begin_edit(&mut dialogs);
                }
                let _ = ctx.run(
                    RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size.into())),
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
    #[test]
    fn clicking_a_visible_cell_selects_that_cell_without_scrolling() {
        let ctx = Context::default();
        super::super::theme::apply(&ctx);
        let mut screen = pair();
        let mut icons = IconManager::new();
        let mut dialogs = TestDialogs::default();
        let mut render = |events: Vec<Event>, screen: &mut TableCompareScreen| {
            let _ = ctx.run(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1150.0, 720.0))),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    CentralPanel::default().show(ctx, |ui| {
                        screen.ui(ctx, ui, &mut icons, &mut dialogs);
                    });
                },
            );
        };
        render(Vec::new(), &mut screen);
        render(Vec::new(), &mut screen);
        let toolbar = ctx
            .data(|data| data.get_temp::<Rect>(Id::new("table_test_toolbar")))
            .unwrap();
        let path = ctx
            .data(|data| data.get_temp::<Rect>(Id::new("table_test_path").with(true)))
            .unwrap();
        assert!(
            path.top() >= toolbar.bottom(),
            "paths must be below the entire toolbar band"
        );
        let rect = ctx
            .data(|data| {
                data.get_temp::<Rect>(Id::new("table_test_cell").with((false, 1usize, 1usize)))
            })
            .unwrap();
        assert!(rect.is_positive());
        let pos = rect.center();
        let before = screen.scroll_y;
        render(
            vec![
                Event::PointerMoved(pos),
                Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed: true,
                    modifiers: Modifiers::NONE,
                },
            ],
            &mut screen,
        );
        render(
            vec![Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::NONE,
            }],
            &mut screen,
        );
        assert!(!screen.active_left);
        assert_eq!((screen.selected, screen.column), (1, 1));
        assert_eq!(screen.scroll_y, before);
    }
    #[test]
    fn bc5_path_controls_match_measured_native_rectangles_and_details_stay_below_grid() {
        // Craft 0.3.0 UIA, BC5.2.7: client 1443 wide; menu 25, dock 42;
        // left spec 642, load 23, split browse 34, one trailing pixel.
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/tables");
        let mut screen = TableCompareScreen::new();
        screen
            .load_files(&root.join("left.xlsx"), &root.join("right.xlsx"))
            .unwrap();
        let ctx = Context::default();
        super::super::theme::apply(&ctx);
        let mut icons = IconManager::new();
        let mut dialogs = TestDialogs::default();
        for _ in 0..2 {
            let _ = ctx.run(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1443.0, 832.0))),
                    ..Default::default()
                },
                |ctx| {
                    CentralPanel::default()
                        .frame(Frame::none())
                        .show(ctx, |ui| {
                            screen.ui(ctx, ui, &mut icons, &mut dialogs);
                        });
                },
            );
        }
        for (left, x, width) in [(true, 43.0, 642.0), (false, 746.0, 639.0)] {
            let rects = ctx
                .data(|d| d.get_temp::<[Rect; 4]>(Id::new("table_test_path_controls").with(left)))
                .unwrap();
            assert_eq!(
                rects[0],
                Rect::from_min_size(pos2(x, 67.0), vec2(width, 23.0))
            );
            assert_eq!(
                rects[1],
                Rect::from_min_size(pos2(x + width, 67.0), vec2(23.0, 23.0))
            );
            assert_eq!(
                rects[2],
                Rect::from_min_size(pos2(x + width + 23.0, 67.0), vec2(23.0, 23.0))
            );
            assert_eq!(
                rects[3],
                Rect::from_min_size(pos2(x + width + 46.0, 67.0), vec2(11.0, 23.0))
            );
        }
        let grid = ctx
            .data(|d| d.get_temp::<Rect>(Id::new("table_test_grid")))
            .unwrap();
        let details = ctx
            .data(|d| d.get_temp::<Rect>(Id::new("table_test_details")))
            .unwrap();
        assert!(details.top() >= grid.bottom());
        assert!(details.bottom() < 832.0 - 19.0);
        assert!(!screen.dirty());
    }
    #[test]
    fn bottom_sheet_tabs_switch_both_files_and_keep_orphan_html_tables_explicit() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/pe");
        let mut screen = TableCompareScreen::new();
        screen
            .load_files(
                &root.join("multi-left.html"),
                &root.join("multi-right.html"),
            )
            .unwrap();
        assert_eq!((screen.left.sheet, screen.right.sheet), (0, 1));
        assert!(screen.pairs.iter().all(|p| !p.different));
        let ctx = Context::default();
        super::super::theme::apply(&ctx);
        let mut icons = IconManager::new();
        let mut dialogs = TestDialogs::default();
        let mut render = |events, screen: &mut TableCompareScreen| {
            let _ = ctx.run(
                RawInput {
                    events,
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1150.0, 720.0))),
                    ..Default::default()
                },
                |ctx| {
                    CentralPanel::default()
                        .frame(Frame::none())
                        .show(ctx, |ui| {
                            screen.ui(ctx, ui, &mut icons, &mut dialogs);
                        });
                },
            );
        };
        render(Vec::new(), &mut screen);
        render(Vec::new(), &mut screen);
        let tab = ctx
            .data(|d| d.get_temp::<Rect>(Id::new("table_test_sheet").with(2usize)))
            .unwrap();
        let pos = tab.center();
        for pressed in [true, false] {
            render(
                vec![
                    Event::PointerMoved(pos),
                    Event::PointerButton {
                        pos,
                        button: PointerButton::Primary,
                        pressed,
                        modifiers: Modifiers::NONE,
                    },
                ],
                &mut screen,
            );
        }
        assert!(screen.left.missing_sheet);
        assert_eq!(screen.right.sheet, 2);
        assert_eq!(screen.pairs.iter().filter(|p| p.important).count(), 2);
        assert!(screen.session_options()["sheet_pair_explicit"]
            .as_bool()
            .unwrap());
        assert!(!screen.dirty());
    }
    #[test]
    fn finding_a_long_cell_keeps_visible_column_a_instead_of_scrolling_it_away() {
        let mut screen = TableCompareScreen::new();
        let mut doc = document::Document::default();
        doc.text = format!("ID,English\nkey,needle {}\n", "x".repeat(500));
        let side = Side {
            table: table::parse(&doc.text, b',').unwrap(),
            document: Some(doc),
            ..Default::default()
        };
        screen.left = side.clone();
        screen.right = side;
        screen.rebuild();
        screen.show_search("needle".into());
        let ctx = Context::default();
        super::super::theme::apply(&ctx);
        let mut icons = IconManager::new();
        let mut dialogs = TestDialogs::default();
        let _ = ctx.run(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1150.0, 720.0))),
                ..Default::default()
            },
            |ctx| {
                CentralPanel::default()
                    .frame(Frame::none())
                    .show(ctx, |ui| {
                        screen.ui(ctx, ui, &mut icons, &mut dialogs);
                    });
            },
        );
        assert_eq!(screen.column, 1);
        assert_eq!(screen.scroll_x, [0.0, 0.0]);
    }
}
