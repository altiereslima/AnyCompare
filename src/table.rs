//! Logical records, workbook readers and stable key alignment for Table Compare.
//! Delimited files retain their Document so edits use the normal encoding/EOL and
//! external-change safeguards. Workbooks are read-only; never rewrite a workbook
//! as CSV or lose its styles, formulas, macros or other sheets.
use crate::{
    document::Document,
    table_columns::{self, Alignment, Change, Column},
};
use calamine::{open_workbook_auto, Data, DataType, Reader, SheetType};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, ops::Range, path::Path};

pub fn is_workbook(path: &Path) -> bool {
    matches!(
        extension(path).as_str(),
        "xlsx" | "xls" | "xlsb" | "xlsm" | "xlam" | "ods" | "xla"
    )
}
pub fn supported(path: &Path) -> bool {
    is_workbook(path)
        || matches!(
            extension(path).as_str(),
            "csv" | "tsv" | "tab" | "psv" | "ssv"
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
    pub missing_sheet: bool,
    pub metadata: String,
    pub binary_hash: Option<[u8; 32]>,
}

impl Side {
    pub fn from_text(text: String) -> Result<Self, String> {
        let delimiter = detect_delimiter(&text, Path::new("clipboard.csv"));
        let table = parse(&text, delimiter)?;
        let mut doc = Document::default();
        doc.text = text;
        Ok(Self {
            document: Some(doc),
            table,
            delimiter,
            ..Self::default()
        })
    }
    pub fn load(path: &Path, delimiter: Option<u8>, sheet: usize) -> Result<Self, String> {
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
                .map(|(index, cells)| Row {
                    cells: cells.iter().map(cell_text).collect(),
                    formulas: (0..cells.len())
                        .map(|col| {
                            formulas
                                .get_value((origin.0 + index as u32, origin.1 + col as u32))
                                .cloned()
                                .unwrap_or_default()
                        })
                        .collect(),
                    line: origin.0 as usize + index + 1,
                    span: 0..0,
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
                missing_sheet: false,
                metadata: file_metadata(path),
                binary_hash: file_hash(path),
            })
        } else {
            let document = Document::load(path)?;
            let delimiter = delimiter.unwrap_or_else(|| detect_delimiter(&document.text, path));
            let table = parse(&document.text, delimiter)?;
            Ok(Self {
                path: path.to_string_lossy().into_owned(),
                document: Some(document),
                table,
                delimiter,
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
            self.table = parse(&doc.text, self.delimiter)?;
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
        let text = serialize_record(cells, self.delimiter)?;
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
        let text = serialize_record(cells, self.delimiter)? + "\n";
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
                serialize_record(&cells, self.delimiter)?
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
                inserted.push_str(&serialize_record(&cells, self.delimiter)?);
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
        let table = parse(&text, self.delimiter)?;
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
            replacements.push((row.span.clone(), serialize_record(&cells, self.delimiter)?));
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

fn cell_text(cell: &Data) -> String {
    match cell {
        Data::DateTime(_) | Data::DateTimeIso(_) => cell
            .as_datetime()
            .map(|v| v.to_string())
            .unwrap_or_else(|| cell.to_string()),
        _ => cell.to_string(),
    }
}

pub fn parse(text: &str, delimiter: u8) -> Result<Table, String> {
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(delimiter)
        .has_headers(false)
        .flexible(true)
        .from_reader(text.as_bytes());
    let mut record = csv::StringRecord::new();
    let mut rows = Vec::new();
    let mut columns = 0;
    while reader.read_record(&mut record).map_err(|e| e.to_string())? {
        let position = record
            .position()
            .ok_or("Posição do registro indisponível.")?;
        let mut start = position.byte() as usize;
        // csv ignores blank physical lines before a record. Keep them outside
        // its editable span, so changing one record cannot remove blank lines.
        while text.as_bytes().get(start) == Some(&b'\n') {
            start += 1;
        }
        let mut end = reader.position().byte() as usize;
        if text.as_bytes().get(end.wrapping_sub(1)) == Some(&b'\n') {
            end -= 1;
        }
        columns = columns.max(record.len());
        rows.push(Row {
            cells: record.iter().map(str::to_owned).collect(),
            formulas: Vec::new(),
            line: position.line() as usize
                + text[position.byte() as usize..start]
                    .bytes()
                    .filter(|b| *b == b'\n')
                    .count(),
            span: start..end,
        });
    }
    Ok(Table {
        rows,
        columns,
        column_origin: 0,
    })
}

pub fn serialize_record(cells: &[String], delimiter: u8) -> Result<String, String> {
    let mut writer = csv::WriterBuilder::new()
        .delimiter(delimiter)
        .from_writer(Vec::new());
    writer.write_record(cells).map_err(|e| e.to_string())?;
    let mut bytes = writer.into_inner().map_err(|e| e.to_string())?;
    if bytes.last() == Some(&b'\n') {
        bytes.pop();
    }
    String::from_utf8(bytes).map_err(|e| e.to_string())
}

pub fn detect_delimiter(text: &str, path: &Path) -> u8 {
    match extension(path).as_str() {
        "tsv" | "tab" => return b'\t',
        "psv" => return b'|',
        "ssv" => return b';',
        _ => {}
    }
    // Consistent logical record widths, not comma counts in quoted values.
    let mut best = (b',', (0, 0));
    for delimiter in *b",;\t|" {
        let mut reader = csv::ReaderBuilder::new()
            .delimiter(delimiter)
            .has_headers(false)
            .flexible(true)
            .from_reader(text.as_bytes());
        let widths: Vec<_> = reader
            .records()
            .take(32)
            .filter_map(Result::ok)
            .map(|r| r.len())
            .collect();
        let mut counts = BTreeMap::<usize, usize>::new();
        for width in widths {
            if width > 1 {
                *counts.entry(width).or_default() += 1;
            }
        }
        let score = counts
            .into_iter()
            .map(|(width, count)| (count, width))
            .max()
            .unwrap_or((0, 0));
        if score > best.1 {
            best = (delimiter, score);
        }
    }
    best.0
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
    pub column_alignment: Alignment,
    pub extra_keys: Vec<usize>,
    pub unimportant_columns: Vec<usize>,
    pub ignore_minor: bool,
    pub numeric: bool,
    pub tolerance: String,
    pub decimal_comma: bool,
    pub widths: Vec<f32>,
    pub sort: Option<(usize, bool)>,
    pub hide_same_columns: bool,
    pub key: Option<usize>,
    pub ignore_case: bool,
    pub trim: bool,
    pub formulas: bool,
    pub filter: Filter,
    pub delimiters: [Option<u8>; 2],
    pub sheets: [usize; 2],
    pub missing_sheets: [bool; 2],
    pub sheet_alignment: Alignment,
    pub sheet_pair_explicit: bool,
    pub date_order: table_columns::DateOrder,
    pub manual_rows: Vec<[usize; 2]>,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            header: false,
            column_alignment: Alignment::Position,
            extra_keys: Vec::new(),
            unimportant_columns: Vec::new(),
            ignore_minor: false,
            numeric: false,
            tolerance: "0".into(),
            decimal_comma: false,
            widths: Vec::new(),
            sort: None,
            hide_same_columns: false,
            key: None,
            ignore_case: false,
            trim: false,
            formulas: false,
            filter: Filter::All,
            delimiters: [None; 2],
            sheets: [0; 2],
            missing_sheets: [false; 2],
            sheet_alignment: Alignment::LeftNames,
            sheet_pair_explicit: false,
            date_order: table_columns::DateOrder::System,
            manual_rows: Vec::new(),
        }
    }
}
impl Options {
    fn normalized(&self, text: &str) -> String {
        let text = if self.trim { text.trim() } else { text };
        if let Some(date) = table_columns::date(text, self.date_order) {
            return date.format("%Y-%m-%d %H:%M:%S%.f").to_string();
        }
        if self.ignore_case {
            text.to_lowercase()
        } else {
            text.to_owned()
        }
    }
    pub fn equal(&self, left: Option<&str>, right: Option<&str>) -> bool {
        match (left, right) {
            (Some(a), Some(b)) => self.normalized(a) == self.normalized(b),
            (None, None) => true,
            _ => false,
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
}
#[derive(Clone, Debug)]
pub struct Pair {
    pub left: Option<usize>,
    pub right: Option<usize>,
    pub different: bool,
    pub important: bool,
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
        let mut groups = BTreeMap::<Vec<(bool, String)>, (Vec<usize>, Vec<usize>)>::new();
        for (is_left, table) in [(true, left), (false, right)] {
            for (index, row) in table.rows.iter().enumerate().skip(start) {
                let key = keys
                    .iter()
                    .map(|&k| {
                        let value = columns[k].side(is_left).and_then(|c| row.cells.get(c));
                        (
                            value.is_none(),
                            options.normalized(value.map_or("", String::as_str)),
                        )
                    })
                    .collect();
                let group = groups.entry(key).or_default();
                if is_left {
                    group.0.push(index);
                } else {
                    group.1.push(index);
                }
            }
        }
        for (a, b) in groups.into_values() {
            for i in 0..a.len().max(b.len()) {
                pairs.push((a.get(i).copied(), b.get(i).copied()));
            }
        }
    } else {
        // No keys means source order, not an implicit sort on column A.
        // Anchor equal records before pairing changed runs, so inserting a row
        // does not make every following record appear changed. Missing and empty
        // cells remain distinct in the comparison even when used as an anchor.
        let signatures = |table: &Table, is_left| {
            table
                .rows
                .iter()
                .skip(start)
                .map(|row| {
                    columns
                        .iter()
                        .map(|c| {
                            c.side(is_left)
                                .and_then(|c| options.value(row, c))
                                .map(|v| {
                                    options
                                        .normalized(&table_columns::display(v, options.date_order))
                                })
                        })
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>()
        };
        let a = signatures(left, true);
        let b = signatures(right, false);
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(200);
        for op in
            similar::capture_diff_slices_deadline(similar::Algorithm::Myers, &a, &b, Some(deadline))
        {
            let (_, old, new) = op.as_tag_tuple();
            for i in 0..old.len().max(new.len()) {
                pairs.push((
                    (i < old.len()).then_some(start + old.start + i),
                    (i < new.len()).then_some(start + new.start + i),
                ));
            }
        }
    }
    pairs
        .into_iter()
        .map(|(a, b)| {
            let change = match (a, b) {
                (Some(a), Some(b)) => columns
                    .iter()
                    .enumerate()
                    .map(|(i, c)| {
                        table_columns::change(
                            options,
                            i,
                            c.left.and_then(|c| options.value(&left.rows[a], c)),
                            c.right.and_then(|c| options.value(&right.rows[b], c)),
                        )
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
    #[ignore = "manual, read-only verification of the user's translation tables"]
    fn profile_user_tables() {
        for name in ["Global.csv", "Missions.csv"] {
            let start = std::time::Instant::now();
            let path = |dir: &str| {
                Path::new("C:/Projetos/Into the Breach")
                    .join(dir)
                    .join("scripts/localization")
                    .join(name)
            };
            let a = Side::load(&path("Jogo"), None, 0).unwrap();
            let b = Side::load(&path("Jogo_diff"), None, 0).unwrap();
            assert_eq!(a.table.columns, 12);
            assert_eq!(b.table.columns, 12);
            let pairs = align(&a.table, &b.table, &Options::default());
            assert_eq!(pairs.len(), a.table.rows.len());
            assert!(pairs.iter().all(|p| !p.different));
            println!(
                "{}",
                serde_json::json!({"file": name, "records": a.table.rows.len(), "columns": a.table.columns, "aligned_rows": pairs.len(), "different": 0, "load_align_ms": start.elapsed().as_secs_f64() * 1000.0})
            );
        }
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
}
