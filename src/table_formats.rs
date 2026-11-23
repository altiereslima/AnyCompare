//! BC5-faithful table format detection (`TableDetect`), delimited / fixed-width
//! record parser (`SsLineParse.pas` `sub_14051ED30`), and native dBase (`.dbf`)
//! table reader.

use crate::{
    table::{Row, Table},
    table_val::{format_workbook_float, parse_table_val, CellHint, RegionalSettings, ValKind},
};
use bigdecimal::BigDecimal;
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::Path, str::FromStr};

/// Exact 10 candidate delimiters from BC5 `asc_141F58158` (`TTableFmtDetector`).
pub const BC5_CANDIDATE_DELIMITERS: [u8; 10] = *b",\t;\\|/^~`=";

/// Text qualifier option mirroring BC5 `TTableFmtFrame`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Qualifier {
    #[default]
    Auto,
    None,
    Char(u8),
}

impl Qualifier {
    pub fn resolve(self, detected: Option<u8>) -> Option<u8> {
        match self {
            Self::Auto => Some(detected.unwrap_or(b'"')),
            Self::None => None,
            Self::Char(c) => Some(c),
        }
    }
}

/// First-line interpretation mirroring BC5 `TTableFormatTypeFrame.FirstKindEdit`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum FirstLineKind {
    Detect,
    ColumnNames,
    #[default]
    CellData,
}

/// Per-side parsing & regional settings mirroring BC5 `TTableFmtFrame` + `TTableRgnFrame`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SideFormat {
    /// Non-empty means Fixed-Width mode (`FxdWidthsEdit`), widths in chars.
    pub fixed_widths: Vec<usize>,
    /// Text qualifier (`"`, `'`, custom, or `None`).
    pub qualifier: Qualifier,
    /// `DlmIgnoreMultipleDelimsEdit`: treat consecutive delimiters as one.
    pub consecutive_delimiters_as_one: bool,
    /// `DlmTrimWSEdit`: treat surrounding whitespace as part of delimiter.
    pub trim_delimiter_whitespace: bool,
    /// Optional per-side regional override (`TableContent + 280`).
    pub regional: Option<RegionalSettings>,
}

impl SideFormat {
    pub fn is_fixed_width(&self) -> bool {
        !self.fixed_widths.is_empty()
    }

    pub fn widths_string(&self) -> String {
        self.fixed_widths
            .iter()
            .map(usize::to_string)
            .collect::<Vec<_>>()
            .join(";")
    }

    pub fn parse_widths(input: &str) -> Result<Vec<usize>, String> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Ok(Vec::new());
        }
        let mut widths = Vec::new();
        for part in trimmed.split([';', ',']) {
            let item = part.trim();
            if item.is_empty() {
                continue;
            }
            let width = item
                .parse::<usize>()
                .ok()
                .filter(|&w| w > 0 && w <= 4096)
                .ok_or_else(|| {
                    "Largura de coluna inválida (use inteiros positivos separados por ';')."
                        .to_owned()
                })?;
            widths.push(width);
        }
        Ok(widths)
    }
}

pub fn is_dbf(path: &Path) -> bool {
    path.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("dbf"))
}

/// BC5 `TTableFmtDetector.Calc` (`sub_1417B5D60`):
/// 1. Chooses quote qualifier (`"` vs `'`) by frequency in the sampled prefix.
/// 2. Splits up to 205 logical records honoring quoted newlines.
/// 3. Skips 0 / 1 / 4 leading rows (`<5` / `<20` / `>=20`) and samples up to
///    201 records across all 10 BC5 candidate delimiters (`asc_141F58158`).
/// 4. Prefers the highest mean delimiter count with `variance_buckets <= 2`,
///    falling back to `mean > 0.99` with minimum `variance_buckets`.
pub fn detect_delimiter_and_qualifier(text: &str, path: &Path) -> (u8, Option<u8>) {
    let ext = path
        .extension()
        .unwrap_or_default()
        .to_string_lossy()
        .to_ascii_lowercase();
    let quote = detect_quote_char(text);
    match ext.as_str() {
        "tsv" | "tab" => return (b'\t', quote),
        "psv" => return (b'|', quote),
        "ssv" => return (b';', quote),
        _ => {}
    }

    let records = sample_logical_records(text, quote, 205);
    if records.is_empty() {
        return (b',', quote);
    }

    let skip = if records.len() < 5 {
        0
    } else if records.len() < 20 {
        1
    } else {
        4
    };
    let sample = &records[skip..records.len().min(skip + 201)];
    if sample.is_empty() {
        return (b',', quote);
    }

    let mut stats = Vec::with_capacity(BC5_CANDIDATE_DELIMITERS.len());
    for &delim in &BC5_CANDIDATE_DELIMITERS {
        let mut total = 0usize;
        let mut hist = BTreeMap::<usize, usize>::new();
        for rec in sample {
            let count = count_unquoted_delimiter(rec, delim, quote);
            total += count;
            if count > 0 {
                *hist.entry(count).or_default() += 1;
            }
        }
        let mean = total as f64 / sample.len() as f64;
        // BC5 `sub_1417B5D60` lines 126..142: number of distinct non-zero
        // delimiter-count buckets occurring more than once.
        let variance_buckets = hist.values().filter(|&&freq| freq > 1).count();
        let non_zero_rows = hist.values().sum::<usize>();
        stats.push((delim, mean, variance_buckets, non_zero_rows));
    }

    // Pass 1: consistent column counts (`variance_buckets <= 2`, `mean > 0`),
    // preferring the highest mean (with a small edge for multi-row consistency).
    let mut best_consistent: Option<(u8, f64, usize)> = None;
    for &(delim, mean, variance, non_zero_rows) in &stats {
        if mean > 0.0 && variance <= 2 && (sample.len() == 1 || non_zero_rows >= 2 || mean >= 1.0) {
            let replace = match best_consistent {
                None => true,
                Some((_, best_mean, best_non_zero)) => {
                    // Prefer delimiters present on at least as many rows when
                    // comparing weak single-row artifacts against real columns.
                    if non_zero_rows > 0 && best_non_zero == 0 {
                        true
                    } else {
                        mean > best_mean
                    }
                }
            };
            if replace {
                best_consistent = Some((delim, mean, non_zero_rows));
            }
        }
    }
    if let Some((delim, _, _)) = best_consistent {
        return (delim, quote);
    }

    // Pass 2: fallback for irregular tables (`mean > 0.99`, minimum variance).
    let mut best_fallback: Option<(u8, f64, usize)> = None;
    for &(delim, mean, variance, _) in &stats {
        if mean > 0.99 {
            let replace = match best_fallback {
                None => true,
                Some((_, best_mean, best_var)) => {
                    variance < best_var || (variance == best_var && mean > best_mean)
                }
            };
            if replace {
                best_fallback = Some((delim, mean, variance));
            }
        }
    }
    if let Some((delim, _, _)) = best_fallback {
        return (delim, quote);
    }

    (b',', quote)
}

fn detect_quote_char(text: &str) -> Option<u8> {
    let prefix_len = text.len().min(65_536);
    let prefix = &text.as_bytes()[..prefix_len];
    let mut double_quotes = 0usize;
    let mut single_quotes = 0usize;
    for &b in prefix {
        if b == b'"' {
            double_quotes += 1;
        } else if b == b'\'' {
            single_quotes += 1;
        }
    }
    if single_quotes >= 2 && single_quotes > double_quotes && double_quotes == 0 {
        // Avoid misclassifying English apostrophes (`don't`, `it's`) as text
        // qualifiers: require at least one quote positioned at field boundary.
        let mut boundary_single = 0usize;
        for (i, &b) in prefix.iter().enumerate() {
            if b == b'\'' {
                let prev_bound =
                    i == 0 || matches!(prefix[i - 1], b',' | b';' | b'\t' | b'|' | b'\n' | b'\r');
                let next_bound = i + 1 == prefix.len()
                    || matches!(prefix[i + 1], b',' | b';' | b'\t' | b'|' | b'\n' | b'\r');
                if prev_bound || next_bound {
                    boundary_single += 1;
                }
            }
        }
        if boundary_single >= 2 {
            return Some(b'\'');
        }
    }
    Some(b'"')
}

fn sample_logical_records(text: &str, quote: Option<u8>, max_records: usize) -> Vec<&str> {
    let bytes = text.as_bytes();
    let mut records = Vec::new();
    let mut start = 0usize;
    let mut in_quotes = false;
    let mut i = 0usize;
    while i < bytes.len() && records.len() < max_records {
        let b = bytes[i];
        if Some(b) == quote {
            if in_quotes && i + 1 < bytes.len() && bytes[i + 1] == b {
                i += 2;
                continue;
            }
            in_quotes = !in_quotes;
            i += 1;
            continue;
        }
        if !in_quotes && b == b'\n' {
            let end = if i > start && bytes[i - 1] == b'\r' {
                i - 1
            } else {
                i
            };
            if end > start {
                records.push(&text[start..end]);
            }
            start = i + 1;
        }
        i += 1;
    }
    if records.len() < max_records && start < bytes.len() {
        let end = if bytes.last() == Some(&b'\r') {
            bytes.len() - 1
        } else {
            bytes.len()
        };
        if end > start {
            records.push(&text[start..end]);
        }
    }
    records
}

fn count_unquoted_delimiter(record: &str, delim: u8, quote: Option<u8>) -> usize {
    let bytes = record.as_bytes();
    let mut in_quotes = false;
    let mut count = 0usize;
    let mut i = 0usize;
    while i < bytes.len() {
        let b = bytes[i];
        if Some(b) == quote {
            if in_quotes && i + 1 < bytes.len() && bytes[i + 1] == b {
                i += 2;
                continue;
            }
            in_quotes = !in_quotes;
        } else if !in_quotes && b == delim {
            count += 1;
        }
        i += 1;
    }
    count
}

/// BC5 `TTableHasColNamesDetector.Calc` ():
/// Samples rows `1..=128` to build a per-column bitmask of `ValKind` values.
/// Row `0` votes `+header` when a cell is `Text` while data rows have non-text,
/// or `+data` when row `0` is non-text matching the exact exclusive data type.
pub fn detect_has_header(table: &Table, regional: RegionalSettings) -> bool {
    if table.rows.len() < 2 {
        return false;
    }
    let mut col_masks: Vec<u8> = Vec::new();
    for row in table.rows.iter().skip(1).take(128) {
        if col_masks.len() < row.cells.len() {
            col_masks.resize(row.cells.len(), 0);
        }
        for (col, cell) in row.cells.iter().enumerate() {
            if cell.is_empty() {
                continue;
            }
            let hint = row.hints.get(col);
            let kind = parse_table_val(cell, hint, regional, false).kind() as u8;
            col_masks[col] |= 1u8 << kind;
        }
    }

    let first = &table.rows[0];
    let limit = first.cells.len().min(col_masks.len());
    let mut header_votes = 0usize;
    let mut data_votes = 0usize;
    for (col, &mask) in col_masks.iter().enumerate().take(limit) {
        let cell = &first.cells[col];
        if cell.is_empty() {
            continue;
        }
        let hint = first.hints.get(col);
        let kind = parse_table_val(cell, hint, regional, false).kind();
        if kind == ValKind::Text {
            // `0xDF` clears bit 5 (`ValKind::Text = 5`): true if any non-text
            // type appeared in rows 1..N.
            if (mask & !(1u8 << (ValKind::Text as u8))) != 0 {
                header_votes += 1;
            }
        } else if mask == (1u8 << (kind as u8)) {
            data_votes += 1;
        }
    }
    header_votes > data_votes
}

/// Parse text using either Fixed-Width columns (`FxdPanel`), standard RFC-4180
/// `csv`, or BC5's full `SsLineParse.pas` (`sub_14051ED30`) tokenizer for custom
/// qualifiers, consecutive-as-one delimiters, and delimiter whitespace trimming.
pub fn parse_with_format(
    text: &str,
    delimiter: u8,
    format: &SideFormat,
    detected_quote: Option<u8>,
) -> Result<Table, String> {
    if format.is_fixed_width() {
        return Ok(parse_fixed_width(text, &format.fixed_widths));
    }

    let quote = format.qualifier.resolve(detected_quote);
    if !format.consecutive_delimiters_as_one
        && !format.trim_delimiter_whitespace
        && quote == Some(b'"')
    {
        return parse_csv_fast(text, delimiter, Some(b'"'));
    }
    if !format.consecutive_delimiters_as_one && !format.trim_delimiter_whitespace {
        return parse_csv_fast(text, delimiter, quote);
    }

    Ok(parse_bc5_delimited(
        text,
        delimiter,
        quote,
        format.consecutive_delimiters_as_one,
        format.trim_delimiter_whitespace,
    ))
}

fn parse_csv_fast(text: &str, delimiter: u8, quote: Option<u8>) -> Result<Table, String> {
    let mut builder = csv::ReaderBuilder::new();
    builder
        .delimiter(delimiter)
        .has_headers(false)
        .flexible(true);
    match quote {
        Some(q) => {
            builder.quoting(true).quote(q);
        }
        None => {
            builder.quoting(false);
        }
    }
    let mut reader = builder.from_reader(text.as_bytes());
    let mut record = csv::StringRecord::new();
    let mut rows = Vec::new();
    let mut columns = 0;
    while reader.read_record(&mut record).map_err(|e| e.to_string())? {
        let position = record
            .position()
            .ok_or("Posição do registro indisponível.")?;
        let mut start = position.byte() as usize;
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
            hints: Vec::new(),
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

/// Fixed-width parser mirroring `SsLineParse.pas` (`sub_14051ED30` lines 40-54).
pub fn parse_fixed_width(text: &str, widths: &[usize]) -> Table {
    let mut rows = Vec::new();
    let mut offset = 0usize;
    let mut max_cols = 0usize;

    for (line_no, raw_line) in (1usize..).zip(text.split('\n')) {
        let line_len = raw_line.len();
        let clean = raw_line.strip_suffix('\r').unwrap_or(raw_line);
        let span_end = offset + clean.len();
        if !clean.is_empty() {
            let mut cells = Vec::with_capacity(widths.len() + 1);
            let mut chars = clean.chars();
            for &w in widths {
                let field: String = chars.by_ref().take(w).collect();
                cells.push(field.trim_end().to_owned());
            }
            let remainder: String = chars.collect();
            if !remainder.trim_end().is_empty() {
                cells.push(remainder.trim_end().to_owned());
            }
            max_cols = max_cols.max(cells.len());
            rows.push(Row {
                cells,
                formulas: Vec::new(),
                hints: Vec::new(),
                line: line_no,
                span: offset..span_end,
            });
        }
        offset += line_len + 1;
    }

    Table {
        rows,
        columns: max_cols,
        column_origin: 0,
    }
}

/// Full BC5 delimited tokenizer (`SsLineParse.pas` `sub_14051ED30`) supporting
/// `consecutive_delimiters_as_one`, `trim_delimiter_whitespace`, arbitrary
/// qualifiers (`"`, `'`, custom, none), and multiline quoted cells.
fn parse_bc5_delimited(
    text: &str,
    delimiter: u8,
    quote: Option<u8>,
    consecutive_as_one: bool,
    trim_ws: bool,
) -> Table {
    let bytes = text.as_bytes();
    let mut rows = Vec::new();
    let mut columns = 0usize;
    let mut i = 0usize;
    let mut line_no = 1usize;

    while i < bytes.len() {
        // Skip empty physical lines between records.
        while i < bytes.len() && bytes[i] == b'\n' {
            i += 1;
            line_no += 1;
        }
        if i >= bytes.len() {
            break;
        }
        let rec_start = i;
        let rec_line = line_no;
        let mut cells = Vec::new();
        let mut in_quotes = false;

        // Locate record end honoring quoted newlines.
        let mut j = i;
        while j < bytes.len() {
            let b = bytes[j];
            if Some(b) == quote {
                if in_quotes && j + 1 < bytes.len() && bytes[j + 1] == b {
                    j += 2;
                    continue;
                }
                in_quotes = !in_quotes;
                j += 1;
                continue;
            }
            if !in_quotes && b == b'\n' {
                break;
            }
            if b == b'\n' {
                line_no += 1;
            }
            j += 1;
        }
        let rec_end = if j > rec_start && bytes[j - 1] == b'\r' {
            j - 1
        } else {
            j
        };
        let record_str = &text[rec_start..rec_end];

        // Tokenize the logical record using BC5 rules.
        let rbytes = record_str.as_bytes();
        let mut pos = 0usize;
        let mut trailing_delim = false;
        while pos < rbytes.len() {
            if trim_ws {
                while pos < rbytes.len()
                    && rbytes[pos] != delimiter
                    && matches!(rbytes[pos], b' ' | b'\t')
                {
                    pos += 1;
                }
            }
            if pos >= rbytes.len() {
                break;
            }
            let mut field = String::new();
            if Some(rbytes[pos]) == quote {
                let q = rbytes[pos];
                pos += 1;
                let mut seg_start = pos;
                while pos < rbytes.len() {
                    if rbytes[pos] == q {
                        field.push_str(&record_str[seg_start..pos]);
                        if pos + 1 < rbytes.len() && rbytes[pos + 1] == q {
                            field.push(q as char);
                            pos += 2;
                            seg_start = pos;
                        } else {
                            pos += 1;
                            seg_start = pos;
                            break;
                        }
                    } else {
                        pos += 1;
                    }
                }
                if seg_start < pos && pos <= rbytes.len() {
                    field.push_str(&record_str[seg_start..pos]);
                }
                // Consume anything until next delimiter.
                let tail_start = pos;
                while pos < rbytes.len() && rbytes[pos] != delimiter {
                    pos += 1;
                }
                let tail = &record_str[tail_start..pos];
                let tail = if trim_ws { tail.trim() } else { tail };
                if !tail.is_empty() {
                    field.push_str(tail);
                }
            } else {
                let start = pos;
                while pos < rbytes.len() && rbytes[pos] != delimiter {
                    pos += 1;
                }
                let slice = &record_str[start..pos];
                let slice = if trim_ws {
                    slice.trim_end_matches([' ', '\t'])
                } else {
                    slice
                };
                field.push_str(slice);
            }
            cells.push(field);

            if pos < rbytes.len() && rbytes[pos] == delimiter {
                pos += 1;
                if consecutive_as_one {
                    while pos < rbytes.len() && rbytes[pos] == delimiter {
                        pos += 1;
                    }
                    trailing_delim = false;
                } else {
                    trailing_delim = true;
                }
            } else {
                trailing_delim = false;
            }
        }
        if trailing_delim {
            cells.push(String::new());
        }

        columns = columns.max(cells.len());
        rows.push(Row {
            cells,
            formulas: Vec::new(),
            hints: Vec::new(),
            line: rec_line,
            span: rec_start..rec_end,
        });

        i = j;
        if i < bytes.len() && bytes[i] == b'\n' {
            i += 1;
            line_no += 1;
        }
    }

    Table {
        rows,
        columns,
        column_origin: 0,
    }
}

/// Serialize a record back to text respecting fixed widths or delimiter/qualifier.
pub fn serialize_with_format(
    cells: &[String],
    delimiter: u8,
    format: &SideFormat,
    detected_quote: Option<u8>,
) -> Result<String, String> {
    if format.is_fixed_width() {
        let mut out = String::new();
        for (idx, cell) in cells.iter().enumerate() {
            if let Some(&w) = format.fixed_widths.get(idx) {
                let count = cell.chars().count();
                if count >= w {
                    out.extend(cell.chars().take(w));
                } else {
                    out.push_str(cell);
                    for _ in count..w {
                        out.push(' ');
                    }
                }
            } else {
                out.push_str(cell);
            }
        }
        return Ok(out);
    }

    let quote = format.qualifier.resolve(detected_quote);
    let mut builder = csv::WriterBuilder::new();
    builder.delimiter(delimiter);
    match quote {
        Some(q) => {
            builder.quote(q).quote_style(csv::QuoteStyle::Necessary);
        }
        None => {
            builder.quote_style(csv::QuoteStyle::Never);
        }
    }
    let mut writer = builder.from_writer(Vec::new());
    writer.write_record(cells).map_err(|e| e.to_string())?;
    let mut bytes = writer.into_inner().map_err(|e| e.to_string())?;
    if bytes.last() == Some(&b'\n') {
        bytes.pop();
    }
    if bytes.last() == Some(&b'\r') {
        bytes.pop();
    }
    String::from_utf8(bytes).map_err(|e| e.to_string())
}

/// Native dBase III / IV / V / Visual FoxPro (`.dbf`) table reader matching BC5's
/// DBF table support (: aDbfApplication`).
pub fn parse_dbf(bytes: &[u8]) -> Result<Table, String> {
    if bytes.len() < 32 {
        return Err("Arquivo DBF inválido ou truncado.".into());
    }
    let version = bytes[0];
    if !matches!(
        version,
        0x02 | 0x03
            | 0x04
            | 0x05
            | 0x30
            | 0x31
            | 0x32
            | 0x43
            | 0x63
            | 0x83
            | 0x8B
            | 0xCB
            | 0xF5
            | 0xFB
    ) {
        return Err(format!("Versão DBF não suportada (0x{version:02X})."));
    }
    let num_records = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]) as usize;
    let header_len = u16::from_le_bytes([bytes[8], bytes[9]]) as usize;
    let record_len = u16::from_le_bytes([bytes[10], bytes[11]]) as usize;
    if header_len < 33 || header_len > bytes.len() || record_len == 0 {
        return Err("Cabeçalho DBF inválido.".into());
    }

    struct FieldDef {
        name: String,
        kind: u8,
        len: usize,
        decimals: u8,
    }

    let mut fields = Vec::new();
    let mut pos = 32usize;
    while pos + 32 <= header_len {
        if bytes[pos] == 0x0D {
            break;
        }
        let name_bytes = &bytes[pos..pos + 11];
        let name_end = name_bytes.iter().position(|&b| b == 0).unwrap_or(11);
        let name = encoding_rs::WINDOWS_1252
            .decode(&name_bytes[..name_end])
            .0
            .trim()
            .to_owned();
        let kind = bytes[pos + 11];
        let len = usize::from(bytes[pos + 16]);
        let decimals = bytes[pos + 17];
        if len > 0 {
            fields.push(FieldDef {
                name,
                kind,
                len,
                decimals,
            });
        }
        pos += 32;
    }
    if fields.is_empty() {
        return Err("Nenhum campo encontrado no arquivo DBF.".into());
    }

    let col_count = fields.len();
    let mut rows = Vec::with_capacity(num_records.min(1_000_000) + 1);
    rows.push(Row {
        cells: fields.iter().map(|f| f.name.clone()).collect(),
        formulas: Vec::new(),
        hints: vec![CellHint::None; col_count],
        line: 1,
        span: 0..0,
    });

    let mut offset = header_len;
    for rec_idx in 0..num_records {
        if offset >= bytes.len() || bytes[offset] == 0x1A {
            break;
        }
        if offset + record_len > bytes.len() {
            break;
        }
        let rec = &bytes[offset..offset + record_len];
        offset += record_len;
        // Skip deleted records (`*` = 0x2A).
        if rec[0] == b'*' {
            continue;
        }
        let mut cells = Vec::with_capacity(col_count);
        let mut hints = Vec::with_capacity(col_count);
        let mut fpos = 1usize;
        for f in &fields {
            let end = (fpos + f.len).min(rec.len());
            let raw = &rec[fpos..end];
            fpos = end;
            let (text, hint) = decode_dbf_field(raw, f.kind, f.decimals);
            cells.push(text);
            hints.push(hint);
        }
        rows.push(Row {
            cells,
            formulas: Vec::new(),
            hints,
            line: rec_idx + 2,
            span: 0..0,
        });
    }

    Ok(Table {
        rows,
        columns: col_count,
        column_origin: 0,
    })
}

fn decode_dbf_field(raw: &[u8], kind: u8, _decimals: u8) -> (String, CellHint) {
    match kind {
        b'I' if raw.len() == 4 => {
            let v = i32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]);
            (v.to_string(), CellHint::Number(BigDecimal::from(v)))
        }
        b'Y' if raw.len() == 8 => {
            let v = i64::from_le_bytes(raw.try_into().unwrap_or([0; 8]));
            let dec = BigDecimal::new(v.into(), 4);
            (dec.normalized().to_plain_string(), CellHint::Number(dec))
        }
        b'B' if raw.len() == 8 => {
            let v = f64::from_le_bytes(raw.try_into().unwrap_or([0; 8]));
            let (txt, dec) = format_workbook_float(v);
            (txt, dec.map_or(CellHint::None, CellHint::Number))
        }
        b'D' => {
            let s = String::from_utf8_lossy(raw).trim().to_owned();
            if s.len() == 8 && s.bytes().all(|b| b.is_ascii_digit()) {
                if let (Ok(y), Ok(m), Ok(d)) = (
                    s[0..4].parse::<i32>(),
                    s[4..6].parse::<u32>(),
                    s[6..8].parse::<u32>(),
                ) {
                    if let Some(date) = NaiveDate::from_ymd_opt(y, m, d) {
                        return (date.format("%Y-%m-%d").to_string(), CellHint::Date(date));
                    }
                }
            }
            (s, CellHint::None)
        }
        b'L' => {
            let ch = raw.iter().copied().find(|b| !b.is_ascii_whitespace());
            match ch {
                Some(b'T' | b't' | b'Y' | b'y') => ("TRUE".to_owned(), CellHint::Bool(true)),
                Some(b'F' | b'f' | b'N' | b'n') => ("FALSE".to_owned(), CellHint::Bool(false)),
                _ => (String::new(), CellHint::None),
            }
        }
        b'N' | b'F' => {
            let s = String::from_utf8_lossy(raw).trim().to_owned();
            let hint = BigDecimal::from_str(&s)
                .ok()
                .map_or(CellHint::None, CellHint::Number);
            (s, hint)
        }
        _ => {
            let decoded = match std::str::from_utf8(raw) {
                Ok(s) => s.trim_end_matches([' ', '\0']).to_owned(),
                Err(_) => encoding_rs::WINDOWS_1252
                    .decode(raw)
                    .0
                    .trim_end_matches([' ', '\0'])
                    .to_owned(),
            };
            (decoded, CellHint::None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::table_columns::DateOrder;
    use crate::table_val::NumberLocale;

    #[test]
    fn detects_all_bc5_delimiters_and_single_quote_qualifier() {
        for (delim, sample) in [
            (b',', "id,name,val\n1,\"a,b\",10\n2,c,20\n"),
            (b';', "id;name;val\n1;\"a,b\";10\n2;c;20\n"),
            (b'\t', "id\tname\tval\n1\ta\t10\n2\tb\t20\n"),
            (b'|', "id|name|val\n1|a|10\n2|b|20\n"),
            (b'^', "id^name^val\n1^a^10\n2^b^20\n"),
            (b'~', "id~name~val\n1~a~10\n2~b~20\n"),
            (b'=', "k=v=extra\n1=a=10\n2=b=20\n"),
        ] {
            let (found, _) = detect_delimiter_and_qualifier(sample, Path::new("data.txt"));
            assert_eq!(found, delim, "sample={sample:?}");
        }

        let single_quoted = "id,name,val\n1,'hello, world',10\n2,'foo, bar',20\n";
        let (delim, quote) = detect_delimiter_and_qualifier(single_quoted, Path::new("data.csv"));
        assert_eq!(delim, b',');
        assert_eq!(quote, Some(b'\''));
    }

    #[test]
    fn parses_fixed_width_and_consecutive_space_delimiters() {
        let fixed_text = "ID   Name      Amount\n001  Alice     100.50\n002  Bob       250.00\n";
        let fmt = SideFormat {
            fixed_widths: vec![5, 10, 6],
            ..Default::default()
        };
        let table = parse_with_format(fixed_text, b',', &fmt, None).unwrap();
        assert_eq!(table.rows.len(), 3);
        assert_eq!(table.rows[1].cells, ["001", "Alice", "100.50"]);
        assert_eq!(table.rows[2].cells, ["002", "Bob", "250.00"]);

        let spaced = "ID   Name    Score\n1    Alice   98.5\n2    Bob     87.0\n";
        let fmt_space = SideFormat {
            consecutive_delimiters_as_one: true,
            ..Default::default()
        };
        let table_sp = parse_with_format(spaced, b' ', &fmt_space, None).unwrap();
        assert_eq!(table_sp.rows[1].cells, ["1", "Alice", "98.5"]);
    }

    #[test]
    fn detects_headers_like_bc5_table_has_col_names_detector() {
        let us = RegionalSettings {
            date_order: DateOrder::Mdy,
            number_locale: NumberLocale::Dot,
        };
        let with_header = parse_with_format(
            "ID,Price,Active,Date\n1,10.50,TRUE,2024-01-01\n2,20.00,FALSE,2024-01-02\n",
            b',',
            &SideFormat::default(),
            Some(b'"'),
        )
        .unwrap();
        assert!(detect_has_header(&with_header, us));

        let no_header = parse_with_format(
            "1,10.50,TRUE,2024-01-01\n2,20.00,FALSE,2024-01-02\n3,30.00,TRUE,2024-01-03\n",
            b',',
            &SideFormat::default(),
            Some(b'"'),
        )
        .unwrap();
        assert!(!detect_has_header(&no_header, us));
    }

    #[test]
    fn parses_valid_dbase3_dbf_records_with_typed_hints() {
        // Minimal valid dBase III file: 3 fields (NAME C(6), QTY N(4), OK L(1), DT D(8)), 2 records (1 active, 1 deleted).
        let mut dbf = Vec::new();
        let header_len: u16 = 32 + 4 * 32 + 1;
        let record_len: u16 = 1 + 6 + 4 + 1 + 8;
        dbf.push(0x03); // dBase III
        dbf.extend_from_slice(&[24, 5, 10]); // YY MM DD
        dbf.extend_from_slice(&2u32.to_le_bytes()); // 2 records
        dbf.extend_from_slice(&header_len.to_le_bytes());
        dbf.extend_from_slice(&record_len.to_le_bytes());
        dbf.resize(32, 0);

        let push_field = |buf: &mut Vec<u8>, name: &str, kind: u8, len: u8| {
            let mut desc = [0u8; 32];
            desc[..name.len()].copy_from_slice(name.as_bytes());
            desc[11] = kind;
            desc[16] = len;
            buf.extend_from_slice(&desc);
        };
        push_field(&mut dbf, "NAME", b'C', 6);
        push_field(&mut dbf, "QTY", b'N', 4);
        push_field(&mut dbf, "OK", b'L', 1);
        push_field(&mut dbf, "DT", b'D', 8);
        dbf.push(0x0D); // header terminator

        // Active record 1
        dbf.push(b' ');
        dbf.extend_from_slice(b"Alpha ");
        dbf.extend_from_slice(b"  42");
        dbf.push(b'T');
        dbf.extend_from_slice(b"20240510");

        // Deleted record 2 (must be skipped)
        dbf.push(b'*');
        dbf.extend_from_slice(b"Ghost ");
        dbf.extend_from_slice(b"  99");
        dbf.push(b'F');
        dbf.extend_from_slice(b"20240511");

        dbf.push(0x1A); // EOF

        let table = parse_dbf(&dbf).unwrap();
        assert_eq!(table.columns, 4);
        assert_eq!(table.rows.len(), 2);
        assert_eq!(table.rows[0].cells, ["NAME", "QTY", "OK", "DT"]);
        assert_eq!(table.rows[1].cells, ["Alpha", "42", "TRUE", "2024-05-10"]);
        assert_eq!(
            table.rows[1].hints[1],
            CellHint::Number(BigDecimal::from(42))
        );
        assert_eq!(table.rows[1].hints[2], CellHint::Bool(true));
        assert_eq!(
            table.rows[1].hints[3],
            CellHint::Date(NaiveDate::from_ymd_opt(2024, 5, 10).unwrap())
        );
    }
}
