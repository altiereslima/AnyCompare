//! Logical records, workbook readers and stable key alignment for Table Compare.
//! Delimited files retain their Document so edits use the normal encoding/EOL and
//! external-change safeguards. Workbooks are read-only; never rewrite a workbook
//! as CSV or lose its styles, formulas, macros or other sheets.
use crate::{
    diff::align::{self, AlignConfig, AlignMethod},
    document::Document,
    table_columns::{self, Alignment, Change, Column},
    table_formats::{self, FirstLineKind, SideFormat},
    table_val::{
        cmp_table_vals, format_workbook_float, parse_iso_duration_time, parse_table_val, CellHint,
        ColHandling, NumberLocale, RegionalSettings, TableVal,
    },
};
use bigdecimal::BigDecimal;
use calamine::{open_workbook_auto, Data, Reader, SheetType};
use chrono::{NaiveDate, NaiveTime, Timelike};
use serde::{Deserialize, Serialize};
use std::{cmp::Ordering, collections::BTreeMap, ops::Range, path::Path};

pub fn is_workbook(path: &Path) -> bool {
    matches!(
        extension(path).as_str(),
        "xlsx" | "xls" | "xlsb" | "xlsm" | "xlam" | "ods" | "xla"
    )
}
pub fn is_dbf(path: &Path) -> bool {
    table_formats::is_dbf(path)
}
pub fn supported(path: &Path) -> bool {
    is_workbook(path)
        || is_dbf(path)
        || matches!(
            extension(path).as_str(),
            "csv" | "tsv" | "tab" | "psv" | "ssv" | "dsv"
        )
        || (is_html(path)
            && Document::load(path).is_ok_and(|doc| crate::html_tables::read(&doc.text).is_ok()))
}
pub fn is_html(path: &Path) -> bool {
    matches!(extension(path).as_str(), "html" | "htm")
}
fn extension(path: &Path) -> String {
    path.extension()
        .unwrap_or_default()
        .to_string_lossy()
        .to_ascii_lowercase()
}

#[derive(Clone, Debug)]
pub struct Row {
    pub cells: Vec<String>,
    pub formulas: Vec<String>,
    pub hints: Vec<CellHint>,
    pub line: usize,
    pub(crate) span: Range<usize>,
}
#[derive(Clone, Debug, Default)]
pub struct Table {
    pub rows: Vec<Row>,
    pub columns: usize,
    /// Absolute, zero-based workbook column of the first used cell.
    pub column_origin: usize,
}

#[derive(Clone, Debug, Default)]
pub struct Side {
    pub path: String,
    pub document: Option<Document>,
    pub table: Table,
    pub sheets: Vec<String>,
    pub sheet: usize,
    pub delimiter: u8,
    pub quote: Option<u8>,
    pub format: SideFormat,
    pub missing_sheet: bool,
    pub metadata: String,
    pub binary_hash: Option<[u8; 32]>,
}

impl Side {
    pub fn from_text(text: String) -> Result<Self, String> {
        let (delimiter, quote) =
            table_formats::detect_delimiter_and_qualifier(&text, Path::new("clipboard.csv"));
        let format = SideFormat::default();
        let table = table_formats::parse_with_format(&text, delimiter, &format, quote)?;
        let mut doc = Document::default();
        doc.text = text;
        Ok(Self {
            document: Some(doc),
            table,
            delimiter,
            quote,
            format,
            ..Self::default()
        })
    }
    pub fn load(path: &Path, delimiter: Option<u8>, sheet: usize) -> Result<Self, String> {
        Self::load_with_format(path, delimiter, sheet, &SideFormat::default())
    }
    pub fn load_with_format(
        path: &Path,
        delimiter: Option<u8>,
        sheet: usize,
        format: &SideFormat,
    ) -> Result<Self, String> {
        if is_dbf(path) {
            let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
            let table = table_formats::parse_dbf(&bytes)?;
            return Ok(Self {
                path: path.to_string_lossy().into_owned(),
                document: None,
                table,
                sheets: Vec::new(),
                sheet: 0,
                delimiter: b',',
                quote: Some(b'"'),
                format: format.clone(),
                missing_sheet: false,
                metadata: file_metadata(path),
                binary_hash: file_hash(path),
            });
        }
        if is_html(path) {
            let doc = Document::load(path)?;
            let sheets = crate::html_tables::read(&doc.text)?;
            let names = sheets.iter().map(|s| s.name.clone()).collect();
            let selected = sheets
                .into_iter()
                .nth(sheet)
                .ok_or("Tabela HTML inexistente.")?;
            return Ok(Self {
                path: path.to_string_lossy().into_owned(),
                table: selected.table,
                sheets: names,
                sheet,
                delimiter: b',',
                quote: Some(b'"'),
                format: format.clone(),
                missing_sheet: false,
                metadata: doc.metadata_label().unwrap_or_default(),
                binary_hash: file_hash(path),
                ..Default::default()
            });
        }
        if is_workbook(path) {
            let mut book =
                open_workbook_auto(path).map_err(|e| format!("{}: {e}", path.display()))?;
            let sheets: Vec<_> = book
                .sheets_metadata()
                .iter()
                .filter(|sheet| sheet.typ == SheetType::WorkSheet)
                .map(|sheet| sheet.name.clone())
                .collect();
            let name = sheets
                .get(sheet)
                .ok_or("Planilha inexistente ou arquivo sem planilhas.")?;
            let range = book.worksheet_range(name).map_err(|e| e.to_string())?;
            let formulas = book.worksheet_formula(name).map_err(|e| e.to_string())?;
            let origin = range.start().unwrap_or((0, 0));
            let rows = range
                .rows()
                .enumerate()
                .map(|(index, cells)| {
                    let mut text_cells = Vec::with_capacity(cells.len());
                    let mut hints = Vec::with_capacity(cells.len());
                    for cell in cells {
                        let (txt, hint) = cell_text_and_hint(cell);
                        text_cells.push(txt);
                        hints.push(hint);
                    }
                    Row {
                        cells: text_cells,
                        formulas: (0..cells.len())
                            .map(|col| {
                                formulas
                                    .get_value((origin.0 + index as u32, origin.1 + col as u32))
                                    .cloned()
                                    .unwrap_or_default()
                            })
                            .collect(),
                        hints,
                        line: origin.0 as usize + index + 1,
                        span: 0..0,
                    }
                })
                .collect();
            Ok(Self {
                path: path.to_string_lossy().into_owned(),
                document: None,
                table: Table {
                    rows,
                    columns: range.width(),
                    column_origin: origin.1 as usize,
                },
                sheets,
                sheet,
                delimiter: b',',
                quote: Some(b'"'),
                format: format.clone(),
                missing_sheet: false,
                metadata: file_metadata(path),
                binary_hash: file_hash(path),
            })
        } else {
            let document = Document::load(path)?;
            let (detected_delim, detected_quote) =
                table_formats::detect_delimiter_and_qualifier(&document.text, path);
            let delimiter = delimiter.unwrap_or(detected_delim);
            let table = table_formats::parse_with_format(
                &document.text,
                delimiter,
                format,
                detected_quote,
            )?;
            Ok(Self {
                path: path.to_string_lossy().into_owned(),
                document: Some(document),
                table,
                delimiter,
                quote: detected_quote,
                format: format.clone(),
                ..Self::default()
            })
        }
    }
    pub fn dirty(&self) -> bool {
        self.document.as_ref().is_some_and(Document::dirty)
    }
    pub fn editable(&self) -> bool {
        self.document.is_some()
    }
    pub fn same_saved_bytes(&self, other: &Self) -> bool {
        if let (Some(a), Some(b)) = (&self.document, &other.document) {
            a.same_saved_bytes(b)
        } else {
            !self.dirty()
                && !other.dirty()
                && self.binary_hash.is_some()
                && self.binary_hash == other.binary_hash
        }
    }
    pub fn metadata_label(&self) -> String {
        self.document
            .as_ref()
            .and_then(Document::metadata_label)
            .unwrap_or_else(|| self.metadata.clone())
    }
    pub fn reparse(&mut self) -> Result<(), String> {
        if let Some(doc) = &self.document {
            self.table = table_formats::parse_with_format(
                &doc.text,
                self.delimiter,
                &self.format,
                self.quote,
            )?;
        }
        Ok(())
    }
    pub fn edit_cell(&mut self, row: usize, column: usize, value: &str) -> Result<(), String> {
        let record = self.table.rows.get(row).ok_or("Linha inexistente.")?;
        let mut cells = record.cells.clone();
        cells.resize(cells.len().max(column + 1), String::new());
        cells[column] = value.to_owned();
        self.replace_row(Some(row), &cells)
    }
    pub fn replace_row(&mut self, row: Option<usize>, cells: &[String]) -> Result<(), String> {
        let doc = self
            .document
            .as_mut()
            .ok_or("A planilha está em modo somente leitura.")?;
        let text =
            table_formats::serialize_with_format(cells, self.delimiter, &self.format, self.quote)?;
        if let Some(row) = row {
            let span = self
                .table
                .rows
                .get(row)
                .ok_or("Linha inexistente.")?
                .span
                .clone();
            doc.text.replace_range(span, &text);
        } else {
            if !doc.text.is_empty() && !doc.text.ends_with('\n') {
                doc.text.push('\n');
            }
            doc.text.push_str(&text);
            doc.text.push('\n');
        }
        self.reparse()
    }
    pub fn insert_row(&mut self, before: Option<usize>, cells: &[String]) -> Result<(), String> {
        let Some(before) = before else {
            return self.replace_row(None, cells);
        };
        let offset = self
            .table
            .rows
            .get(before)
            .ok_or("Linha inexistente.")?
            .span
            .start;
        let text =
            table_formats::serialize_with_format(cells, self.delimiter, &self.format, self.quote)?
                + "\n";
        self.document
            .as_mut()
            .ok_or("A planilha está em modo somente leitura.")?
            .text
            .insert_str(offset, &text);
        self.reparse()
    }
    pub fn apply_record_changes(
        &mut self,
        rows: std::collections::BTreeMap<usize, Option<Vec<String>>>,
        insertions: std::collections::BTreeMap<usize, Vec<Vec<String>>>,
    ) -> Result<(), String> {
        let doc = self
            .document
            .as_mut()
            .ok_or("A planilha está em modo somente leitura.")?;
        let mut edits = Vec::new();
        for (row, cells) in rows {
            let record = self.table.rows.get(row).ok_or("Linha inexistente.")?;
            let mut span = record.span.clone();
            let replacement = if let Some(cells) = cells {
                if cells == record.cells {
                    continue;
                }
                table_formats::serialize_with_format(
                    &cells,
                    self.delimiter,
                    &self.format,
                    self.quote,
                )?
            } else {
                if doc.text.as_bytes().get(span.end) == Some(&b'\n') {
                    span.end += 1;
                }
                String::new()
            };
            edits.push((span, replacement));
        }
        for (row, records) in insertions {
            let offset = if row == self.table.rows.len() {
                doc.text.len()
            } else {
                self.table
                    .rows
                    .get(row)
                    .ok_or("Linha inexistente.")?
                    .span
                    .start
            };
            let mut inserted = String::new();
            if offset == doc.text.len() && offset > 0 && !doc.text.ends_with('\n') {
                inserted.push('\n');
            }
            for cells in records {
                inserted.push_str(&table_formats::serialize_with_format(
                    &cells,
                    self.delimiter,
                    &self.format,
                    self.quote,
                )?);
                inserted.push('\n');
            }
            edits.push((offset..offset, inserted));
        }
        if edits.is_empty() {
            return Ok(());
        }
        edits.sort_by_key(|(span, _)| (span.start, span.end));
        let mut text = String::with_capacity(doc.text.len());
        let mut offset = 0;
        for (span, replacement) in edits {
            text.push_str(
                doc.text
                    .get(offset..span.start)
                    .ok_or("Alterações de linhas sobrepostas.")?,
            );
            text.push_str(&replacement);
            offset = span.end;
        }
        text.push_str(&doc.text[offset..]);
        let table =
            table_formats::parse_with_format(&text, self.delimiter, &self.format, self.quote)?;
        doc.text = text;
        self.table = table;
        Ok(())
    }

    /// Change only record spans, retaining blank physical lines, record EOLs,
    /// document encoding and all other metadata until the normal Save path.
    pub fn change_column(&mut self, column: usize, insert: bool) -> Result<(), String> {
        if self.document.is_none() {
            return Err("A planilha está em modo somente leitura.".into());
        }
        let mut replacements = Vec::new();
        for row in &self.table.rows {
            let mut cells = row.cells.clone();
            if insert {
                cells.resize(cells.len().max(column), String::new());
                cells.insert(column, String::new());
            } else if column < cells.len() {
                cells.remove(column);
                if cells.is_empty() {
                    cells.push(String::new());
                }
            } else {
                continue;
            }
            replacements.push((
                row.span.clone(),
                table_formats::serialize_with_format(
                    &cells,
                    self.delimiter,
                    &self.format,
                    self.quote,
                )?,
            ));
        }
        let doc = self.document.as_mut().unwrap();
        for (span, text) in replacements.into_iter().rev() {
            doc.text.replace_range(span, &text);
        }
        self.reparse()
    }
}

fn file_hash(path: &Path) -> Option<[u8; 32]> {
    use sha2::Digest;
    std::fs::read(path)
        .ok()
        .map(|bytes| sha2::Sha256::digest(bytes).into())
}
fn file_metadata(path: &Path) -> String {
    let Ok(m) = std::fs::metadata(path) else {
        return String::new();
    };
    let date = m
        .modified()
        .ok()
        .map(|t| {
            chrono::DateTime::<chrono::Local>::from(t)
                .format("%d/%m/%Y %H:%M:%S")
                .to_string()
        })
        .unwrap_or_default();
    let digits = m.len().to_string();
    let size: String = digits
        .chars()
        .enumerate()
        .flat_map(|(i, c)| {
            if i > 0 && (digits.len() - i).is_multiple_of(3) {
                vec!['.', c]
            } else {
                vec![c]
            }
        })
        .collect();
    format!("{date}   {size} bytes")
}

fn cell_text_and_hint(cell: &Data) -> (String, CellHint) {
    match cell {
        Data::Int(v) => (v.to_string(), CellHint::Number(BigDecimal::from(*v))),
        Data::Float(v) => {
            let (txt, dec) = format_workbook_float(*v);
            (txt, dec.map_or(CellHint::None, CellHint::Number))
        }
        Data::Bool(b) => (
            if *b { "TRUE" } else { "FALSE" }.to_owned(),
            CellHint::Bool(*b),
        ),
        Data::DateTime(dt) => {
            if dt.is_duration() {
                if let Some(dur) = dt.as_duration() {
                    let total_ms = dur.num_milliseconds();
                    if (0..86_400_000).contains(&total_ms) {
                        let secs = (total_ms / 1000) as u32;
                        let ms = (total_ms % 1000) as u32;
                        if let Some(t) =
                            NaiveTime::from_num_seconds_from_midnight_opt(secs, ms * 1_000_000)
                        {
                            let txt = if ms == 0 {
                                t.format("%H:%M:%S").to_string()
                            } else {
                                t.format("%H:%M:%S%.3f").to_string()
                            };
                            return (txt, CellHint::Time(t));
                        }
                    }
                }
            }
            let (y, m, d, h, min, s, ms) = dt.to_ymd_hms_milli();
            let serial = dt.as_f64();
            // Pure Excel time serial (`0.0 <= serial < 1.0`) has no real calendar date.
            if (0.0..1.0).contains(&serial) {
                if let Some(t) = NaiveTime::from_hms_milli_opt(
                    u32::from(h),
                    u32::from(min),
                    u32::from(s),
                    u32::from(ms),
                ) {
                    let txt = if ms == 0 {
                        t.format("%H:%M:%S").to_string()
                    } else {
                        t.format("%H:%M:%S%.3f").to_string()
                    };
                    return (txt, CellHint::Time(t));
                }
            }
            if let Some(date) = NaiveDate::from_ymd_opt(i32::from(y), u32::from(m), u32::from(d)) {
                if h == 0 && min == 0 && s == 0 && ms == 0 {
                    return (date.format("%Y-%m-%d").to_string(), CellHint::Date(date));
                }
                if let Some(time) = NaiveTime::from_hms_milli_opt(
                    u32::from(h),
                    u32::from(min),
                    u32::from(s),
                    u32::from(ms),
                ) {
                    let ndt = date.and_time(time);
                    let txt = if ms == 0 {
                        ndt.format("%Y-%m-%d %H:%M:%S").to_string()
                    } else {
                        ndt.format("%Y-%m-%d %H:%M:%S%.3f").to_string()
                    };
                    return (txt, CellHint::DateTime(ndt));
                }
            }
            (cell.to_string(), CellHint::None)
        }
        Data::DateTimeIso(s) => {
            let trimmed = s.trim();
            if let Some((d_part, t_part)) = trimmed.split_once('T') {
                let clean_t = t_part.trim_end_matches(['Z', 'z']);
                if let (Ok(d), Ok(t)) = (
                    NaiveDate::parse_from_str(d_part, "%Y-%m-%d"),
                    NaiveTime::parse_from_str(clean_t, "%H:%M:%S%.f")
                        .or_else(|_| NaiveTime::parse_from_str(clean_t, "%H:%M")),
                ) {
                    let ndt = d.and_time(t);
                    return (format!("{d_part} {clean_t}"), CellHint::DateTime(ndt));
                }
            } else if let Ok(d) = NaiveDate::parse_from_str(trimmed, "%Y-%m-%d") {
                return (trimmed.to_owned(), CellHint::Date(d));
            }
            (trimmed.to_owned(), CellHint::None)
        }
        Data::DurationIso(s) => {
            if let Some(t) = parse_iso_duration_time(s) {
                let txt = if t.nanosecond() == 0 {
                    t.format("%H:%M:%S").to_string()
                } else {
                    t.format("%H:%M:%S%.3f").to_string()
                };
                return (txt, CellHint::Time(t));
            }
            (s.clone(), CellHint::None)
        }
        Data::String(s) => (s.clone(), CellHint::None),
        Data::Error(e) => (e.to_string(), CellHint::None),
        Data::Empty => (String::new(), CellHint::None),
    }
}

pub fn parse(text: &str, delimiter: u8) -> Result<Table, String> {
    table_formats::parse_with_format(text, delimiter, &SideFormat::default(), Some(b'"'))
}

pub fn serialize_record(cells: &[String], delimiter: u8) -> Result<String, String> {
    table_formats::serialize_with_format(cells, delimiter, &SideFormat::default(), Some(b'"'))
}

#[cfg(test)]
pub fn detect_delimiter(text: &str, path: &Path) -> u8 {
    table_formats::detect_delimiter_and_qualifier(text, path).0
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Filter {
    All,
    Differences,
    Same,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Options {
    pub header: bool,
    pub first_line: FirstLineKind,
    pub column_alignment: Alignment,
    pub extra_keys: Vec<usize>,
    pub unimportant_columns: Vec<usize>,
    pub ignore_minor: bool,
    /// Legacy flag kept for saved sessions / tests: setting `numeric: true` with
    /// `decimal_comma: false` selects dot-decimal numeric mode.
    pub numeric: bool,
    /// BC5 `ColAsText`: when `false` (`General`, BC5 default), typed values are
    /// recognized automatically; when `true`, all columns default to raw text.
    pub as_text: bool,
    pub tolerance: String,
    /// BC5 `ColNumToleranceRelative`: numeric tolerance is `%` of `max(|a|, |b|)`.
    pub num_tolerance_relative: bool,
    /// BC5 `ColDateTimeTolerance`: datetime tolerance in seconds.
    pub datetime_tolerance_secs: u64,
    pub decimal_comma: bool,
    pub number_locale: NumberLocale,
    pub widths: Vec<f32>,
    pub sort: Option<(usize, bool)>,
    pub hide_same_columns: bool,
    pub key: Option<usize>,
    pub ignore_case: bool,
    /// BC5 `StrWSInsig`: ignores whitespace characters (`' '`, `'\t'`, `'\r'`, `'\n'`).
    pub trim: bool,
    pub formulas: bool,
    pub filter: Filter,
    pub delimiters: [Option<u8>; 2],
    pub formats: [SideFormat; 2],
    pub sheets: [usize; 2],
    pub missing_sheets: [bool; 2],
    pub sheet_alignment: Alignment,
    pub sheet_pair_explicit: bool,
    pub date_order: table_columns::DateOrder,
    pub align_sort: bool,
    pub align_method: AlignMethod,
    pub col_handlings: BTreeMap<usize, ColHandling>,
    pub manual_rows: Vec<[usize; 2]>,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            header: false,
            first_line: FirstLineKind::CellData,
            column_alignment: Alignment::Position,
            extra_keys: Vec::new(),
            unimportant_columns: Vec::new(),
            ignore_minor: false,
            numeric: false,
            as_text: false,
            tolerance: "0".into(),
            num_tolerance_relative: false,
            datetime_tolerance_secs: 0,
            decimal_comma: false,
            number_locale: NumberLocale::System,
            widths: Vec::new(),
            sort: None,
            hide_same_columns: false,
            key: None,
            ignore_case: false,
            trim: false,
            formulas: false,
            filter: Filter::All,
            delimiters: [None; 2],
            formats: Default::default(),
            sheets: [0; 2],
            missing_sheets: [false; 2],
            sheet_alignment: Alignment::LeftNames,
            sheet_pair_explicit: false,
            date_order: table_columns::DateOrder::System,
            align_sort: false,
            align_method: AlignMethod::Patience,
            col_handlings: BTreeMap::new(),
            manual_rows: Vec::new(),
        }
    }
}
impl Options {
    pub fn default_regional(&self) -> RegionalSettings {
        let number_locale = if self.number_locale != NumberLocale::System {
            self.number_locale
        } else if self.decimal_comma {
            NumberLocale::Comma
        } else if self.numeric {
            // Preserve legacy callers that set `numeric: true` without
            // setting `decimal_comma` (US dot decimal).
            NumberLocale::Dot
        } else {
            NumberLocale::System
        };
        RegionalSettings {
            date_order: self.date_order,
            number_locale,
        }
    }

    pub fn side_regional(&self, left: bool) -> RegionalSettings {
        let idx = usize::from(!left);
        self.formats[idx]
            .regional
            .unwrap_or_else(|| self.default_regional())
    }

    pub fn default_col_handling(&self) -> ColHandling {
        ColHandling {
            as_text: self.as_text,
            unimportant: false,
            ignore_case: self.ignore_case,
            ignore_whitespace: self.trim,
            num_tolerance: self.tolerance.clone(),
            num_tolerance_relative: self.num_tolerance_relative,
            datetime_tolerance_secs: self.datetime_tolerance_secs,
        }
    }

    pub fn col_handling(&self, column: usize) -> ColHandling {
        if let Some(custom) = self.col_handlings.get(&column) {
            let mut h = custom.clone();
            if self.unimportant_columns.contains(&column) {
                h.unimportant = true;
            }
            h
        } else {
            let mut h = self.default_col_handling();
            h.unimportant = self.unimportant_columns.contains(&column);
            h
        }
    }

    pub fn is_key(&self, column: usize) -> bool {
        self.key == Some(column) || self.extra_keys.contains(&column)
    }

    pub fn set_key(&mut self, column: usize, enabled: bool) {
        if enabled {
            if !self.is_key(column) {
                if self.key.is_none() {
                    self.key = Some(column);
                } else {
                    self.extra_keys.push(column);
                }
            }
        } else if self.key == Some(column) {
            self.key = self.extra_keys.first().copied();
            if !self.extra_keys.is_empty() {
                self.extra_keys.remove(0);
            }
        } else {
            self.extra_keys.retain(|&c| c != column);
        }
    }

    pub fn value<'a>(&self, row: &'a Row, column: usize) -> Option<&'a str> {
        if self.formulas {
            if let Some(formula) = row.formulas.get(column).filter(|s| !s.is_empty()) {
                return Some(formula);
            }
        }
        // A short CSV record still has an empty field in an existing table
        // column. A missing comparison column is represented by Column::None.
        Some(row.cells.get(column).map_or("", String::as_str))
    }

    pub fn cell<'a>(&self, row: &'a Row, column: usize) -> (Option<&'a str>, Option<&'a CellHint>) {
        if self.formulas {
            if let Some(formula) = row.formulas.get(column).filter(|s| !s.is_empty()) {
                return (Some(formula), None);
            }
        }
        let text = row.cells.get(column).map_or("", String::as_str);
        let hint = row.hints.get(column).filter(|h| **h != CellHint::None);
        (Some(text), hint)
    }
}
#[derive(Clone, Debug)]
pub struct Pair {
    pub left: Option<usize>,
    pub right: Option<usize>,
    pub different: bool,
    pub important: bool,
}

fn eval_cell_val(
    row: &Row,
    phys_col: Option<usize>,
    options: &Options,
    regional: RegionalSettings,
    handling: &ColHandling,
) -> Option<TableVal> {
    let col = phys_col?;
    let (raw, hint) = options.cell(row, col);
    Some(parse_table_val(
        raw.unwrap_or(""),
        hint,
        regional,
        handling.as_text,
    ))
}

fn cmp_opt_val(a: Option<&TableVal>, b: Option<&TableVal>, handling: &ColHandling) -> Ordering {
    match (a, b) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Less,
        (Some(_), None) => Ordering::Greater,
        (Some(x), Some(y)) => cmp_table_vals(x, y, handling),
    }
}

fn cmp_val_tuples(
    a: &[Option<TableVal>],
    b: &[Option<TableVal>],
    handlings: &[ColHandling],
) -> Ordering {
    for ((va, vb), h) in a.iter().zip(b.iter()).zip(handlings.iter()) {
        let ord = cmp_opt_val(va.as_ref(), vb.as_ref(), h);
        if ord != Ordering::Equal {
            return ord;
        }
    }
    Ordering::Equal
}

fn build_row_signature(
    row: &Row,
    is_left: bool,
    align_cols: &[usize],
    columns: &[Column],
    handlings: &[ColHandling],
    options: &Options,
    regional: RegionalSettings,
) -> String {
    let mut sig = String::new();
    for (idx, &col_idx) in align_cols.iter().enumerate() {
        if idx > 0 {
            sig.push('\x1F');
        }
        let phys = columns[col_idx].side(is_left);
        let h = &handlings[col_idx];
        if let Some(val) = eval_cell_val(row, phys, options, regional, h) {
            sig.push_str(&val.canonical_for_align(h));
        } else {
            // Distinct sentinel for missing comparison column vs empty cell.
            sig.push('\x1E');
        }
    }
    sig
}

struct AlignSubsetCtx<'a> {
    left: &'a Table,
    right: &'a Table,
    align_cols: &'a [usize],
    columns: &'a [Column],
    handlings: &'a [ColHandling],
    options: &'a Options,
}

fn align_index_subsets(
    ctx: &AlignSubsetCtx<'_>,
    left_rows: &[usize],
    right_rows: &[usize],
    method: AlignMethod,
    use_closeness: bool,
) -> Vec<(Option<usize>, Option<usize>)> {
    if left_rows.is_empty() {
        return right_rows.iter().map(|&r| (None, Some(r))).collect();
    }
    if right_rows.is_empty() {
        return left_rows.iter().map(|&l| (Some(l), None)).collect();
    }
    let left_reg = ctx.options.side_regional(true);
    let right_reg = ctx.options.side_regional(false);
    let sigs_l: Vec<String> = left_rows
        .iter()
        .map(|&r| {
            build_row_signature(
                &ctx.left.rows[r],
                true,
                ctx.align_cols,
                ctx.columns,
                ctx.handlings,
                ctx.options,
                left_reg,
            )
        })
        .collect();
    let sigs_r: Vec<String> = right_rows
        .iter()
        .map(|&r| {
            build_row_signature(
                &ctx.right.rows[r],
                false,
                ctx.align_cols,
                ctx.columns,
                ctx.handlings,
                ctx.options,
                right_reg,
            )
        })
        .collect();
    let refs_l: Vec<&str> = sigs_l.iter().map(String::as_str).collect();
    let refs_r: Vec<&str> = sigs_r.iter().map(String::as_str).collect();
    let aligned = align::align_lines(
        &refs_l,
        &refs_r,
        AlignConfig {
            method,
            skew_tolerance: 2000,
            use_closeness,
            never_align_diffs: false,
            ignore_case: false,
            ignore_whitespace: false,
        },
    );
    aligned
        .into_iter()
        .map(|(li, ri)| (li.map(|i| left_rows[i]), ri.map(|j| right_rows[j])))
        .collect()
}

#[cfg(test)]
pub fn align(left: &Table, right: &Table, options: &Options) -> Vec<Pair> {
    let columns = table_columns::columns(left, right, options);
    align_columns(left, right, options, &columns)
}
pub fn align_columns(
    left: &Table,
    right: &Table,
    options: &Options,
    columns: &[Column],
) -> Vec<Pair> {
    let start = usize::from(options.header);
    let mut pairs = Vec::new();
    let keys: Vec<_> = options
        .key
        .into_iter()
        .chain(options.extra_keys.iter().copied())
        .filter(|&k| k < columns.len())
        .collect();
    let handlings: Vec<ColHandling> = (0..columns.len())
        .map(|col| options.col_handling(col))
        .collect();
    // BC5 `sub_1417A5F10`: align using important columns only, falling back to
    // all columns when every column is marked unimportant.
    let mut align_cols: Vec<usize> = (0..columns.len())
        .filter(|&col| !handlings[col].unimportant)
        .collect();
    if align_cols.is_empty() {
        align_cols = (0..columns.len()).collect();
    }

    if !options.manual_rows.is_empty() {
        let mut anchors = options.manual_rows.clone();
        anchors.sort_unstable();
        let mut from = [start, start];
        let mut automatic = options.clone();
        automatic.manual_rows.clear();
        automatic.header = false;
        for anchor in anchors
            .into_iter()
            .chain(std::iter::once([left.rows.len(), right.rows.len()]))
        {
            if anchor[0] < from[0]
                || anchor[1] < from[1]
                || anchor[0] > left.rows.len()
                || anchor[1] > right.rows.len()
            {
                continue;
            }
            let a = Table {
                rows: left.rows[from[0]..anchor[0]].to_vec(),
                columns: left.columns,
                column_origin: left.column_origin,
            };
            let b = Table {
                rows: right.rows[from[1]..anchor[1]].to_vec(),
                columns: right.columns,
                column_origin: right.column_origin,
            };
            pairs.extend(
                align_columns(&a, &b, &automatic, columns)
                    .into_iter()
                    .map(|p| (p.left.map(|i| i + from[0]), p.right.map(|i| i + from[1]))),
            );
            if anchor[0] < left.rows.len() && anchor[1] < right.rows.len() {
                pairs.push((Some(anchor[0]), Some(anchor[1])));
            }
            from = [anchor[0] + 1, anchor[1] + 1];
        }
    } else if !keys.is_empty() {
        // BC5 `TTableMapSortJob` + `sub_1417A3E20`: sort both sides by typed
        // `TTableVal` keys (`sub_1417B4D20`) with original-row tiebreak, then
        // linear merge-join. Duplicate-key blocks anchor identical rows first.
        let key_handlings: Vec<ColHandling> = keys.iter().map(|&k| handlings[k].clone()).collect();
        let left_reg = options.side_regional(true);
        let right_reg = options.side_regional(false);

        let build_sorted = |table: &Table, is_left: bool, reg: RegionalSettings| {
            let mut keyed: Vec<(usize, Vec<Option<TableVal>>)> = (start..table.rows.len())
                .map(|row_idx| {
                    let vals = keys
                        .iter()
                        .zip(&key_handlings)
                        .map(|(&k, h)| {
                            eval_cell_val(
                                &table.rows[row_idx],
                                columns[k].side(is_left),
                                options,
                                reg,
                                h,
                            )
                        })
                        .collect();
                    (row_idx, vals)
                })
                .collect();
            keyed.sort_by(|a, b| {
                cmp_val_tuples(&a.1, &b.1, &key_handlings).then_with(|| a.0.cmp(&b.0))
            });
            keyed
        };

        let left_sorted = build_sorted(left, true, left_reg);
        let right_sorted = build_sorted(right, false, right_reg);

        let mut i = 0usize;
        let mut j = 0usize;
        while i < left_sorted.len() && j < right_sorted.len() {
            match cmp_val_tuples(&left_sorted[i].1, &right_sorted[j].1, &key_handlings) {
                Ordering::Less => {
                    pairs.push((Some(left_sorted[i].0), None));
                    i += 1;
                }
                Ordering::Greater => {
                    pairs.push((None, Some(right_sorted[j].0)));
                    j += 1;
                }
                Ordering::Equal => {
                    let mut i_end = i + 1;
                    while i_end < left_sorted.len()
                        && cmp_val_tuples(&left_sorted[i].1, &left_sorted[i_end].1, &key_handlings)
                            == Ordering::Equal
                    {
                        i_end += 1;
                    }
                    let mut j_end = j + 1;
                    while j_end < right_sorted.len()
                        && cmp_val_tuples(
                            &right_sorted[j].1,
                            &right_sorted[j_end].1,
                            &key_handlings,
                        ) == Ordering::Equal
                    {
                        j_end += 1;
                    }
                    if i_end - i == 1 && j_end - j == 1 {
                        pairs.push((Some(left_sorted[i].0), Some(right_sorted[j].0)));
                    } else {
                        let l_rows: Vec<usize> =
                            left_sorted[i..i_end].iter().map(|e| e.0).collect();
                        let r_rows: Vec<usize> =
                            right_sorted[j..j_end].iter().map(|e| e.0).collect();
                        // Anchor identical records within duplicate-key groups
                        // and pair changed gaps 1-to-1 (`use_closeness = false`),
                        // preserving BC5 key-group compactness.
                        let ctx = AlignSubsetCtx {
                            left,
                            right,
                            align_cols: &align_cols,
                            columns,
                            handlings: &handlings,
                            options,
                        };
                        pairs.extend(align_index_subsets(
                            &ctx,
                            &l_rows,
                            &r_rows,
                            AlignMethod::Alternate,
                            false,
                        ));
                    }
                    i = i_end;
                    j = j_end;
                }
            }
        }
        while i < left_sorted.len() {
            pairs.push((Some(left_sorted[i].0), None));
            i += 1;
        }
        while j < right_sorted.len() {
            pairs.push((None, Some(right_sorted[j].0)));
            j += 1;
        }
    } else {
        // Unsorted or `AlignSort`: BC5 `TTableMapAlignLines` ()
        // using our BC5 line alignment engine (`align`, zero 200ms timeout).
        let mut left_rows: Vec<usize> = (start..left.rows.len()).collect();
        let mut right_rows: Vec<usize> = (start..right.rows.len()).collect();

        if options.align_sort {
            // BC5 `sub_1417A4B80`: sort rows by important columns using typed `TTableVal`.
            let sort_handlings: Vec<ColHandling> =
                align_cols.iter().map(|&c| handlings[c].clone()).collect();
            let sort_side = |rows: &mut Vec<usize>, table: &Table, is_left: bool| {
                let reg = options.side_regional(is_left);
                let mut keyed: Vec<(usize, Vec<Option<TableVal>>)> = rows
                    .iter()
                    .map(|&r| {
                        let vals = align_cols
                            .iter()
                            .zip(&sort_handlings)
                            .map(|(&c, h)| {
                                eval_cell_val(
                                    &table.rows[r],
                                    columns[c].side(is_left),
                                    options,
                                    reg,
                                    h,
                                )
                            })
                            .collect();
                        (r, vals)
                    })
                    .collect();
                keyed.sort_by(|a, b| {
                    cmp_val_tuples(&a.1, &b.1, &sort_handlings).then_with(|| a.0.cmp(&b.0))
                });
                *rows = keyed.into_iter().map(|(r, _)| r).collect();
            };
            sort_side(&mut left_rows, left, true);
            sort_side(&mut right_rows, right, false);
        }

        let ctx = AlignSubsetCtx {
            left,
            right,
            align_cols: &align_cols,
            columns,
            handlings: &handlings,
            options,
        };
        pairs = align_index_subsets(&ctx, &left_rows, &right_rows, options.align_method, true);
    }

    pairs
        .into_iter()
        .map(|(a, b)| {
            let change = match (a, b) {
                (Some(a), Some(b)) => columns
                    .iter()
                    .enumerate()
                    .map(|(i, c)| {
                        let (lv, lh) = c
                            .left
                            .map_or((None, None), |col| options.cell(&left.rows[a], col));
                        let (rv, rh) = c
                            .right
                            .map_or((None, None), |col| options.cell(&right.rows[b], col));
                        table_columns::change_with_hints(options, i, lv, lh, rv, rh)
                    })
                    .max()
                    .unwrap_or(Change::Same),
                _ => Change::Important,
            };
            Pair {
                left: a,
                right: b,
                different: change != Change::Same,
                important: change == Change::Important,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_defaults_preserve_header_source_order_and_align_inserted_records() {
        let a = parse(
            "ID,English\nz,last alphabetically\nb,\"first line\nsecond line\"\na,end\n",
            b',',
        )
        .unwrap();
        let b = parse("ID,English\nz,last alphabetically\nnew,inserted\nb,\"first line\nsecond line\"\na,end\n", b',').unwrap();
        let options = Options::default();
        assert!(!options.header);
        assert!(options.key.is_none());
        let pairs = align(&a, &b, &options);
        assert_eq!(
            pairs.iter().map(|p| (p.left, p.right)).collect::<Vec<_>>(),
            [
                (Some(0), Some(0)),
                (Some(1), Some(1)),
                (None, Some(2)),
                (Some(2), Some(3)),
                (Some(3), Some(4))
            ]
        );
        assert_eq!(pairs.iter().filter(|p| p.important).count(), 1);
    }
    #[test]
    fn quoted_cell_newlines_do_not_make_record_separators_mixed_and_insertion_preserves_eol() {
        use crate::document::LineEnding;
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join(".codex/table-tests/record-eol");
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("records.csv");
        let original = b"ID,English\r\nz,\"First line\nsecond line\"\r\na,last\r\n";
        std::fs::write(&path, original).unwrap();
        let mut side = Side::load(&path, None, 0).unwrap();
        let doc = side.document.as_ref().unwrap();
        assert_eq!(doc.line_ending, LineEnding::Mixed);
        assert_eq!(doc.record_line_ending(b','), LineEnding::CrLf);
        side.insert_row(Some(2), &["new".into(), "added".into()])
            .unwrap();
        side.document.as_mut().unwrap().save(&path).unwrap();
        let output = std::fs::read(&path).unwrap();
        assert!(output
            .windows(b"First line\nsecond line".len())
            .any(|w| w == b"First line\nsecond line"));
        assert!(output.ends_with(b"new,added\r\na,last\r\n"));
    }
    fn fixture(name: &str) -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/tables")
            .join(name)
    }
    #[test]
    fn reads_delimited_pairs_and_workbook_values() {
        for extension in ["csv", "tsv", "tab", "ssv", "psv", "ods"] {
            let a = Side::load(&fixture(&format!("left.{extension}")), None, 0).unwrap();
            let b = Side::load(&fixture(&format!("right.{extension}")), None, 0).unwrap();
            assert_eq!(a.table.columns, 4, "{extension}");
            assert_eq!(
                a.table.rows[1].cells[1], "A \"quoted\", value",
                "{extension}"
            );
            assert_eq!(
                a.table.rows[2].cells[1], "First line\nsecond line",
                "{extension}"
            );
            let aligned = align(
                &a.table,
                &b.table,
                &Options {
                    header: true,
                    key: Some(0),
                    ..Options::default()
                },
            );
            assert_eq!(aligned.len(), 5, "{extension}");
            assert_eq!(
                aligned.iter().filter(|p| p.different).count(),
                4,
                "{extension}"
            );
        }
        let a = Side::load(&fixture("left.xlsx"), None, 0).unwrap();
        assert_eq!(a.sheets, ["Translations", "Other"]);
        assert_eq!(a.table.rows[2].cells[1], "First line\nsecond line");
        assert_eq!(a.table.rows[6].cells[1], "4");
        assert_eq!(a.table.rows[6].formulas[1], "1+3");
        let b = Side::load(&fixture("left.xlsx"), None, 1).unwrap();
        assert_eq!(b.table.rows[1].cells[1], "Other worksheet");
        assert!(Side::load(&fixture("left.xlsx"), None, 100).is_err());
    }
    #[test]
    fn reads_real_legacy_and_binary_excel_workbooks() {
        for extension in ["xlsx", "xls", "xlsb", "ods"] {
            let a = Side::load(&fixture(&format!("any_sheets.{extension}")), None, 0).unwrap();
            assert!(a.sheets.len() >= 3, "{extension}");
            assert!(!a.table.rows.is_empty(), "{extension}");
            assert!(
                a.table
                    .rows
                    .iter()
                    .any(|r| r.cells.iter().any(|v| !v.is_empty())),
                "{extension}"
            );
            assert!(!a.editable());
            let mut copy = a.clone();
            assert!(copy.edit_cell(0, 0, "cannot write").is_err());
        }
    }
    #[test]
    fn formula_mode_is_explicit_and_does_not_recalculate() {
        let a = Side::load(&fixture("left.xlsx"), None, 0).unwrap();
        let b = Side::load(&fixture("right.xlsx"), None, 0).unwrap();
        let mut options = Options::default();
        let values = align(&a.table, &b.table, &options);
        assert!(!values.iter().find(|p| p.left == Some(6)).unwrap().different);
        options.formulas = true;
        assert!(
            align(&a.table, &b.table, &options)
                .iter()
                .find(|p| p.left == Some(6))
                .unwrap()
                .different
        );
    }
    #[test]
    fn cell_edit_preserves_other_records_and_encoding_on_save() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join(".codex/table-tests/encoding");
        std::fs::create_dir_all(&root).unwrap();
        let original = "ID;Text\r\na;\"ação\"\r\n\r\nb;\"untouched\"\n";
        for (index, encoding) in [
            EncodingCase::Utf8Bom,
            EncodingCase::Utf16,
            EncodingCase::Ansi,
        ]
        .into_iter()
        .enumerate()
        {
            let path = root.join(format!("case-{index}.csv"));
            let bytes = match encoding {
                EncodingCase::Utf8Bom => [b"\xef\xbb\xbf".as_slice(), original.as_bytes()].concat(),
                EncodingCase::Utf16 => [
                    vec![0xff, 0xfe],
                    original.encode_utf16().flat_map(u16::to_le_bytes).collect(),
                ]
                .concat(),
                EncodingCase::Ansi => encoding_rs::WINDOWS_1252.encode(original).0.into_owned(),
            };
            std::fs::write(&path, &bytes).unwrap();
            let mut side = Side::load(&path, None, 0).unwrap();
            side.edit_cell(1, 1, "coração; novo\ntexto").unwrap();
            assert!(side.dirty());
            assert!(side
                .document
                .as_ref()
                .unwrap()
                .text
                .ends_with("\n\nb;\"untouched\"\n"));
            side.document.as_mut().unwrap().save(&path).unwrap();
            assert!(!side.dirty());
            let loaded = Side::load(&path, None, 0).unwrap();
            assert_eq!(loaded.table.rows[1].cells[1], "coração; novo\ntexto");
            assert_eq!(
                loaded.document.as_ref().unwrap().encoding,
                side.document.as_ref().unwrap().encoding
            );
            let output = std::fs::read(&path).unwrap();
            assert_ne!(output, bytes);
            if matches!(encoding, EncodingCase::Utf8Bom) {
                assert!(output.starts_with(b"\xef\xbb\xbf"));
            }
            if matches!(encoding, EncodingCase::Utf16) {
                assert!(output.starts_with(&[0xff, 0xfe]));
            }
        }
    }
    enum EncodingCase {
        Utf8Bom,
        Utf16,
        Ansi,
    }
    #[test]
    fn logical_records_and_empty_cells() {
        let table = parse("id,text,end\na,\"olá,\"\"mundo\"\"\nnext\",\nb,,x\n", b',').unwrap();
        assert_eq!(table.rows.len(), 3);
        assert_eq!(table.rows[1].cells, ["a", "olá,\"mundo\"\nnext", ""]);
        assert_eq!(table.rows[2].line, 4);
    }
    #[test]
    fn reordered_duplicate_keys_and_missing_columns() {
        let a = parse("id,text\nb,2\na,1\na,3\nc,\n", b',').unwrap();
        let b = parse("id,text\na,1\nb,2\na,4\nc\n", b',').unwrap();
        let pairs = align(
            &a,
            &b,
            &Options {
                header: true,
                key: Some(0),
                ..Options::default()
            },
        );
        assert_eq!(pairs.len(), 4);
        assert_eq!(
            (pairs[0].left, pairs[0].right, pairs[0].different),
            (Some(2), Some(1), false)
        );
        assert_eq!(pairs.iter().filter(|p| p.different).count(), 1);
    }
    #[test]
    fn detects_delimiters_outside_quotes() {
        for (text, delimiter) in [
            ("id;text\na;\"1,2,3\"\nb;\"4,5,6\"", b';'),
            ("id\ttext\na\t\"a\tb\"", b'\t'),
            ("id,text\na,\"1;2;3\"", b','),
        ] {
            assert_eq!(detect_delimiter(text, Path::new("input.csv")), delimiter);
        }
    }
    #[test]
    fn row_serialization_round_trips_multiline_and_quotes() {
        let cells = vec!["ação".into(), "a;\"b\"\nc".into(), "".into()];
        let text = serialize_record(&cells, b';').unwrap();
        assert_eq!(parse(&text, b';').unwrap().rows[0].cells, cells);
    }

    #[test]
    fn cross_format_fixtures_match_across_csv_tsv_ssv_psv_ods_and_xlsx() {
        let csv_left = Side::load(&fixture("left.csv"), None, 0).unwrap();
        for ext in ["tsv", "tab", "ssv", "psv", "ods"] {
            let other = Side::load(&fixture(&format!("left.{ext}")), None, 0).unwrap();
            let options = Options {
                header: true,
                key: Some(0),
                number_locale: NumberLocale::Dot,
                ..Options::default()
            };
            let pairs = align(&csv_left.table, &other.table, &options);
            assert_eq!(
                pairs.iter().filter(|p| p.different).count(),
                0,
                "left.csv vs left.{ext} must have 0 differences"
            );
        }
    }

    #[test]
    fn cross_regional_and_cross_type_cells_match_bc5_semantics() {
        // Left: US CSV (dot decimal, MDY, single quotes, accounting currency)
        // Right: PT-BR CSV (comma decimal, DMY, double quotes, standard minus)
        let left_csv = "ID,Amount,Cost,Active,Date,Time,Note\n\
                        2,'1,234.50',($100.00),true,02/01/2024,2:30 PM,hello world\n\
                        10,'0.30',$50.00,FALSE,12/25/2024,09:05:10,keep\n";
        let right_csv = "ID;Amount;Cost;Active;Date;Time;Note\n\
                         10;0,3;$50;false;25/12/2024;09:05:40;changed note\n\
                         2;1.234,5;-$100;TRUE;01/02/2024;14:30:00;  hello   world \n";

        let left_fmt = SideFormat {
            regional: Some(RegionalSettings {
                date_order: table_columns::DateOrder::Mdy,
                number_locale: NumberLocale::Dot,
            }),
            ..Default::default()
        };
        let right_fmt = SideFormat {
            regional: Some(RegionalSettings {
                date_order: table_columns::DateOrder::Dmy,
                number_locale: NumberLocale::Comma,
            }),
            ..Default::default()
        };

        let (dl, ql) = table_formats::detect_delimiter_and_qualifier(left_csv, Path::new("a.csv"));
        let (dr, qr) = table_formats::detect_delimiter_and_qualifier(right_csv, Path::new("b.csv"));
        assert_eq!((dl, ql), (b',', Some(b'\'')));
        assert_eq!((dr, qr), (b';', Some(b'"')));

        let a = table_formats::parse_with_format(left_csv, dl, &left_fmt, ql).unwrap();
        let b = table_formats::parse_with_format(right_csv, dr, &right_fmt, qr).unwrap();

        let mut options = Options {
            header: true,
            key: Some(0),
            datetime_tolerance_secs: 45,
            trim: true,
            formats: [left_fmt, right_fmt],
            ..Options::default()
        };
        // Mark Note (col 6) unimportant via per-column ColHandling
        options.col_handlings.insert(
            6,
            ColHandling {
                unimportant: true,
                ..Default::default()
            },
        );

        let pairs = align(&a, &b, &options);
        assert_eq!(pairs.len(), 2);
        // Typed numeric key sort: ID=2 MUST come before ID=10 (unlike string sort!)
        assert_eq!(a.rows[pairs[0].left.unwrap()].cells[0], "2");
        assert_eq!(a.rows[pairs[1].left.unwrap()].cells[0], "10");
        // No important differences on either row!
        assert_eq!(pairs.iter().filter(|p| p.important).count(), 0);
        // Row 0 has minor whitespace diff in Note; row 1 has minor 30s time diff & unimportant Note.
        assert_eq!(pairs.iter().filter(|p| p.different).count(), 2);
    }

    #[test]
    fn per_column_as_text_relative_tolerance_unimportant_alignment_and_align_sort() {
        let a = parse(
            "Code,Price,Note\n001,100.00,old a\n002,200.00,old b\n003,300.00,old c\n",
            b',',
        )
        .unwrap();
        let b = parse(
            "Code,Price,Note\n003,300.00,new c\n001,104.00,new a\n002,220.00,new b\n",
            b',',
        )
        .unwrap();

        let mut options = Options {
            header: true,
            align_sort: true,
            number_locale: NumberLocale::Dot,
            unimportant_columns: vec![2],
            ..Options::default()
        };
        // Col 0 (`Code`) forced to Text so `001` != `1`, Col 1 (`Price`) has 5% relative tolerance
        options.col_handlings.insert(
            0,
            ColHandling {
                as_text: true,
                ..Default::default()
            },
        );
        options.col_handlings.insert(
            1,
            ColHandling {
                num_tolerance: "5".into(),
                num_tolerance_relative: true,
                ..Default::default()
            },
        );

        let pairs = align(&a, &b, &options);
        assert_eq!(pairs.len(), 3);
        // All 3 codes aligned despite scrambled order & completely changed Note column!
        assert_eq!(a.rows[pairs[0].left.unwrap()].cells[0], "001");
        assert_eq!(b.rows[pairs[0].right.unwrap()].cells[0], "001");
        assert!(!pairs[0].important); // 100 vs 104 within 5% -> Minor
        assert!(pairs[0].different);
        assert!(pairs[1].important); // 200 vs 220 exceeds 5% -> Important
        assert!(!pairs[2].important); // 300 vs 300, only unimportant Note changed -> Minor

        // Confirm `as_text` keeps `001` != `1`
        assert_eq!(
            table_columns::change(&options, 0, Some("001"), Some("1")),
            Change::Important
        );
    }

    #[test]
    fn workbook_hints_and_dbf_compare_cleanly_against_csv_and_fixed_width() {
        // Synthetic DBF with C, N, L, D compared against Fixed-Width text & CSV
        let mut dbf = Vec::new();
        let header_len: u16 = 32 + 4 * 32 + 1;
        let record_len: u16 = 1 + 8 + 8 + 1 + 8;
        dbf.push(0x03);
        dbf.extend_from_slice(&[24, 5, 10]);
        dbf.extend_from_slice(&2u32.to_le_bytes());
        dbf.extend_from_slice(&header_len.to_le_bytes());
        dbf.extend_from_slice(&record_len.to_le_bytes());
        dbf.resize(32, 0);
        for (name, kind, len) in [
            ("ITEM", b'C', 8u8),
            ("VAL", b'N', 8),
            ("OK", b'L', 1),
            ("DT", b'D', 8),
        ] {
            let mut desc = [0u8; 32];
            desc[..name.len()].copy_from_slice(name.as_bytes());
            desc[11] = kind;
            desc[16] = len;
            dbf.extend_from_slice(&desc);
        }
        dbf.push(0x0D);
        // Record 1
        dbf.push(b' ');
        dbf.extend_from_slice(b"Widget  ");
        dbf.extend_from_slice(b"   12.50");
        dbf.push(b'T');
        dbf.extend_from_slice(b"20240510");
        // Record 2
        dbf.push(b' ');
        dbf.extend_from_slice(b"Gadget  ");
        dbf.extend_from_slice(b"  100.00");
        dbf.push(b'F');
        dbf.extend_from_slice(b"20240511");
        dbf.push(0x1A);

        let dbf_table = table_formats::parse_dbf(&dbf).unwrap();

        let fixed_text = "ITEM    VAL     OK    DT        \n\
                          Widget  12,5    true  10/05/2024\n\
                          Gadget  100     false 11/05/2024\n";
        let fixed_fmt = SideFormat {
            fixed_widths: vec![8, 8, 6, 10],
            regional: Some(RegionalSettings {
                date_order: table_columns::DateOrder::Dmy,
                number_locale: NumberLocale::Comma,
            }),
            ..Default::default()
        };
        let fixed_table =
            table_formats::parse_with_format(fixed_text, b',', &fixed_fmt, None).unwrap();

        let options = Options {
            header: true,
            formats: [SideFormat::default(), fixed_fmt],
            ..Options::default()
        };
        let pairs = align(&dbf_table, &fixed_table, &options);
        assert_eq!(pairs.len(), 2);
        assert!(
            pairs.iter().all(|p| !p.different),
            "DBF native typed values must match PT-BR Fixed-Width text with 0 diffs"
        );
    }

    #[test]
    fn large_table_aligns_fast_without_200ms_myers_timeout() {
        let mut left_txt = String::with_capacity(400_000);
        let mut right_txt = String::with_capacity(400_000);
        left_txt.push_str("ID,Name,Value,Tag\n");
        right_txt.push_str("ID,Name,Value,Tag\n");
        for i in 0..10_000usize {
            left_txt.push_str(&format!("{i},Item-{i},{i}.50,alpha\n"));
            if i == 2500 {
                right_txt.push_str("999999,Inserted,0.00,extra\n");
            }
            if i == 7500 {
                right_txt.push_str(&format!("{i},Item-{i}-edited,{i}.50,alpha\n"));
            } else {
                right_txt.push_str(&format!("{i},Item-{i},{i}.50,alpha\n"));
            }
        }
        let a = parse(&left_txt, b',').unwrap();
        let b = parse(&right_txt, b',').unwrap();
        let options = Options {
            header: true,
            number_locale: NumberLocale::Dot,
            ..Options::default()
        };
        let started = std::time::Instant::now();
        let pairs = align(&a, &b, &options);
        let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
        assert_eq!(pairs.len(), 10_001);
        assert_eq!(pairs.iter().filter(|p| p.important).count(), 2);
        assert!(
            elapsed_ms < 2000.0,
            "10k-row alignment should stay fast, took {elapsed_ms:.1} ms"
        );
    }
}
