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
    document::{self, Encoding},
    i18n::tr,
    table::{self, Filter, Options, Pair, Side},
    table_columns::{self, Alignment, Change, Column},
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
        }
    }
    pub fn dirty(&self) -> bool {
        self.left.dirty() || self.right.dirty() || self.editor_changed()
    }
    pub fn is_empty(&self) -> bool {
        self.left.path.is_empty() && self.right.path.is_empty() && !self.dirty()
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
        let a = Side::load(left, self.options.delimiters[0], self.options.sheets[0])?;
        let b = Side::load(right, self.options.delimiters[1], self.options.sheets[1])?;
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
    pub fn open_side(&mut self, left: bool, path: &Path, dialogs: &mut dyn DialogHost) {
        let i = usize::from(!left);
        match Side::load(path, self.options.delimiters[i], 0) {
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
                        (Some(a), Some(b)) => table_columns::change(
                            &self.options,
                            column,
                            self.value(true, &self.left.table.rows[a], column),
                            self.value(false, &self.right.table.rows[b], column),
                        ),
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
            let value = |index: usize| {
                let p = &self.pairs[index];
                p.left
                    .and_then(|r| self.value(true, &self.left.table.rows[r], column))
                    .or_else(|| {
                        p.right
                            .and_then(|r| self.value(false, &self.right.table.rows[r], column))
                    })
                    .unwrap_or("")
                    .to_owned()
            };
            let mut ordered = self.visible.clone();
            ordered.sort_by(|&a, &b| {
                let order = value(a).cmp(&value(b));
                if ascending {
                    order
                } else {
                    order.reverse()
                }
            });
            self.visible = ordered;
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
        if table::is_workbook(&path) || table::is_html(&path) {
            dialogs.error("Escolha uma extensão de texto: CSV, TSV, TAB, SSV ou PSV. Salvar como Excel perderia a estrutura da planilha.");
            return false;
        }
        if !self.side(!left).path.is_empty()
            && document::same_file(&path, Path::new(&self.side(!left).path))
        {
            dialogs.error("O destino é o arquivo do outro lado da comparação.");
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
                    "Escolha uma codificação que preserve todos os caracteres.",
                )),
                DialogField::Choice(
                    tr("Codificação:"),
                    0,
                    encodings.iter().map(|e| e.label().to_owned()).collect(),
                ),
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
        let mut keys = vec![tr("Posição da linha")];
        let columns = self.columns.len();
        keys.extend((0..columns).map(|i| self.heading(true, i)));
        let mut fields = [
            DialogField::Check(
                tr("Primeira linha contém nomes das colunas"),
                self.options.header,
            ),
            DialogField::Choice(
                tr("Alinhar por:"),
                self.options.key.map_or(0, |k| k + 1).min(columns),
                keys,
            ),
            DialogField::Check(
                tr("Ignorar maiúsculas/minúsculas"),
                self.options.ignore_case,
            ),
            DialogField::Check(tr("Ignorar espaços nas extremidades"), self.options.trim),
            DialogField::Check(
                tr("Comparar fórmulas em vez dos valores armazenados"),
                self.options.formulas,
            ),
            DialogField::Choice(
                tr("Alinhamento das colunas:"),
                match self.options.column_alignment {
                    Alignment::Position => 0,
                    Alignment::LeftNames => 1,
                    Alignment::RightNames => 2,
                },
                ["Posição", "Nomes do lado esquerdo", "Nomes do lado direito"]
                    .map(tr)
                    .to_vec(),
            ),
            DialogField::Text(
                tr("Chaves adicionais (colunas, separadas por vírgula):"),
                self.options
                    .extra_keys
                    .iter()
                    .map(|c| (c + 1).to_string())
                    .collect::<Vec<_>>()
                    .join(","),
            ),
            DialogField::Check(tr("Comparação numérica exata"), self.options.numeric),
            DialogField::Text(tr("Tolerância numérica:"), self.options.tolerance.clone()),
            DialogField::Check(
                tr("Vírgula decimal; ponto para milhares"),
                self.options.decimal_comma,
            ),
            DialogField::Text(
                tr("Colunas não importantes:"),
                self.options
                    .unimportant_columns
                    .iter()
                    .map(|c| (c + 1).to_string())
                    .collect::<Vec<_>>()
                    .join(","),
            ),
        ];
        if dialogs.fields("Regras de comparação", &mut fields) {
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
                        .ok_or_else(|| tr("Número de coluna inválido."))?;
                    if !result.contains(&(n - 1)) {
                        result.push(n - 1);
                    }
                }
                Ok(result)
            };
            let (extra, unimportant) = match (list(&fields[6]), list(&fields[10])) {
                (Ok(a), Ok(b)) => (a, b),
                (Err(e), _) | (_, Err(e)) => {
                    dialogs.error(&e);
                    return;
                }
            };
            let decimal_comma = matches!(fields[9], DialogField::Check(_, true));
            let tolerance = if let DialogField::Text(_, ref text) = fields[8] {
                text.clone()
            } else {
                "0".into()
            };
            if table_columns::decimal(&tolerance, decimal_comma).is_none_or(|t| t < 0) {
                dialogs.error("Tolerância numérica inválida.");
                return;
            }
            if let DialogField::Check(_, v) = fields[0] {
                self.options.header = v;
            }
            if let DialogField::Choice(_, v, _) = fields[1] {
                self.options.key = v.checked_sub(1);
            }
            if let DialogField::Check(_, v) = fields[2] {
                self.options.ignore_case = v;
            }
            if let DialogField::Check(_, v) = fields[3] {
                self.options.trim = v;
            }
            if let DialogField::Check(_, v) = fields[4] {
                self.options.formulas = v;
            }
            if let DialogField::Choice(_, v, _) = fields[5] {
                self.options.column_alignment = match v {
                    1 => Alignment::LeftNames,
                    2 => Alignment::RightNames,
                    _ => Alignment::Position,
                };
            }
            self.options.extra_keys = if self.options.key.is_some() {
                extra
            } else {
                Vec::new()
            };
            self.options.unimportant_columns = unimportant;
            self.options.numeric = matches!(fields[7], DialogField::Check(_, true));
            self.options.tolerance = tolerance;
            self.options.decimal_comma = decimal_comma;
            self.rebuild();
            self.scroll_y = 0.0;
        }
    }
    fn format(&mut self, dialogs: &mut dyn DialogHost) {
        if !self.commit_editor(dialogs) {
            return;
        }
        let names = [
            "Automático",
            "Vírgula (,)",
            "Ponto e vírgula (;)",
            "Tabulação",
            "Barra vertical (|)",
        ]
        .map(tr)
        .to_vec();
        let delimiters = [None, Some(b','), Some(b';'), Some(b'\t'), Some(b'|')];
        let mut fields: Vec<_> = [true, false]
            .into_iter()
            .map(|left| {
                let i = usize::from(!left);
                DialogField::Choice(
                    tr(if left {
                        "Separador esquerdo:"
                    } else {
                        "Separador direito:"
                    }),
                    delimiters
                        .iter()
                        .position(|d| *d == self.options.delimiters[i])
                        .unwrap_or(0),
                    names.clone(),
                )
            })
            .collect();
        if !dialogs.fields("Formato do arquivo", &mut fields) {
            return;
        }
        let mut sides = [self.left.clone(), self.right.clone()];
        for (i, side) in sides.iter_mut().enumerate() {
            if let DialogField::Choice(_, index, _) = fields[i] {
                let delimiter = delimiters[index];
                if let Some(doc) = &side.document {
                    side.delimiter = delimiter.unwrap_or_else(|| {
                        table::detect_delimiter(&doc.text, Path::new(&side.path))
                    });
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
            .exact_height(25.0)
            .show_separator_line(false)
            .frame(Frame::none())
            .show_inside(ui, |ui| {
                ui.painter().rect_filled(ui.max_rect(), 0.0, Color32::WHITE);
                chrome::menu_bar(ui, |ui| {
                    chrome::menu_button(ui, "Sessão", |ui| {
                        session_menu::prefix(ui, icons, true);
                        if chrome::menu_item(ui, icons, "Configurações da Sessão...", true)
                            .clicked()
                        {
                            self.rules(dialogs);
                            ui.close_menu();
                        }
                        session_menu::close(ui, icons);
                        session_menu::suffix(
                            ui,
                            icons,
                            !self.left.path.is_empty() && !self.right.path.is_empty(),
                        );
                    });
                    chrome::menu_button(ui, "Arquivo (F)", |ui| {
                        if chrome::menu_item(ui, icons, "Abrir arquivo...  Ctrl+O", true).clicked()
                        {
                            self.pick_file(self.active_left, dialogs);
                            ui.close_menu();
                        }
                        if chrome::menu_item(
                            ui,
                            icons,
                            "Salvar arquivo  Ctrl+S",
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
                            "Salvar como...",
                            self.side(self.active_left).editable() && !self.read_only,
                        )
                        .clicked()
                        {
                            self.save_side(self.active_left, true, dialogs);
                            ui.close_menu();
                        }
                        ui.separator();
                        if chrome::menu_item(ui, icons, "Inverter Lados", true).clicked() {
                            self.swap(dialogs);
                            ui.close_menu();
                        }
                        if chrome::menu_item(
                            ui,
                            icons,
                            "Recarregar Arquivos  F5",
                            !self.left.path.is_empty() || !self.right.path.is_empty(),
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
                            "Desfazer  Ctrl+Z",
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
                            "Refazer  Ctrl+Y",
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
                            "Editar célula  F2",
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
                            "Copiar linha para o outro lado",
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
                            "Copiar célula para o outro lado",
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
                            "Selecionar tudo  Ctrl+A",
                            !self.visible.is_empty(),
                        )
                        .clicked()
                        {
                            self.select_all_visible();
                            ui.close_menu();
                        }
                    });
                    chrome::menu_button(ui, "Pesquisar", |ui| {
                        if chrome::menu_item(ui, icons, "Localizar...  Ctrl+F", true).clicked() {
                            self.show_search = true;
                            ui.close_menu();
                        }
                        if chrome::menu_item(ui, icons, "Próxima Diferença", true).clicked() {
                            self.navigate(true);
                            ui.close_menu();
                        }
                        if chrome::menu_item(ui, icons, "Diferença Anterior", true).clicked() {
                            self.navigate(false);
                            ui.close_menu();
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
                                self.set_filter(filter);
                                ui.close_menu();
                            }
                        }
                        if ui
                            .checkbox(
                                &mut self.options.hide_same_columns,
                                tr("Ocultar colunas iguais"),
                            )
                            .changed()
                        {
                            ui.close_menu();
                        }
                    });
                    chrome::menu_button(ui, "Ferramentas", |ui| {
                        if chrome::menu_item(ui, icons, "Opções...", true).clicked() {
                            crate::i18n::configure(dialogs);
                            ui.close_menu();
                        }
                        if chrome::menu_item(ui, icons, "Formatos de Arquivo...", true).clicked() {
                            self.format(dialogs);
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
        let height = toolbar::height(
            ui,
            toolbar_spec::TABLE_COMPARE,
            ui.available_width(),
            &state,
        );
        let mut event = None;
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
                ui.label(tr("Localizar:"));
                let response = ui.add_sized(
                    [ui.available_width().min(450.0), 22.0],
                    TextEdit::singleline(&mut self.search),
                );
                if response.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                    self.find(true);
                }
                if ui.button(tr("Anterior")).clicked() {
                    self.find(false);
                }
                if ui.button(tr("Próximo")).clicked() {
                    self.find(true);
                }
                if ui.button("×").clicked() {
                    self.show_search = false;
                }
                if self.search_notice {
                    ui.label(tr("Texto não encontrado"));
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
                        if self.options.key == Some(column) {
                            self.options.key = self.options.extra_keys.first().copied();
                            if !self.options.extra_keys.is_empty() {
                                self.options.extra_keys.remove(0);
                            }
                        } else if self.options.extra_keys.contains(&column) {
                            self.options.extra_keys.retain(|&c| c != column);
                        } else if self.options.key.is_none() {
                            self.options.key = Some(column);
                        } else {
                            self.options.extra_keys.push(column);
                        }
                        self.rebuild();
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
            return Err("Há alterações não salvas.".into());
        }
        let pair = self
            .sheet_pairs()
            .get(index)
            .copied()
            .ok_or("Planilha inexistente.")?;
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
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/bc5");
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
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/bc5");
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
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/bc5");
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
