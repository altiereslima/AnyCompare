//! BC5-faithful typed table cell engine (`TableVal`  &
//! `TTableMap.GetDiffs` ).
//!
//! Recognizes BC5's five native cell value kinds (`Boolean`, `Currency`,
//! `DateTime` with Date / Time / DateTime subtypes, `Number` with strict
//! 3-digit thousand-separator grouping and arbitrary-precision `BigDecimal`,
//! and `Text`), compares cells with per-column `ColHandling` rules (including
//! absolute or relative `%` numeric tolerance and datetime tolerance in
//! seconds), and provides BC5's typed sort order (`sub_1417B4CB0` +
//! `sub_1417B4D20`).

use crate::table_columns::{Change, DateOrder};
use bigdecimal::BigDecimal;
use chrono::{NaiveDate, NaiveDateTime, NaiveTime, Timelike};
use serde::{Deserialize, Serialize};
use std::{cmp::Ordering, str::FromStr, sync::OnceLock};

/// Native numeric / regional separator setting (`TTableRgnFrame` / `sub_1411B06B0`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum NumberLocale {
    #[default]
    System,
    /// Dot decimal, comma thousands (e.g. `1,234.56`).
    Dot,
    /// Comma decimal, dot thousands (e.g. `1.234,56`).
    Comma,
}

impl NumberLocale {
    pub fn resolved(self) -> Self {
        match self {
            Self::System => system_number_locale(),
            other => other,
        }
    }

    pub fn separators(self) -> (char, char) {
        match self.resolved() {
            Self::Comma => (',', '.'),
            Self::Dot | Self::System => ('.', ','),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::System => "table.locale_auto",
            Self::Dot => "table.decimal.dot",
            Self::Comma => "table.locale_comma",
        }
    }

    pub fn short_label(self) -> &'static str {
        match self.resolved() {
            Self::Comma => "1.234,56",
            Self::Dot | Self::System => "1,234.56",
        }
    }
}

fn system_number_locale() -> NumberLocale {
    static CACHED: OnceLock<NumberLocale> = OnceLock::new();
    *CACHED.get_or_init(|| {
        #[cfg(windows)]
        {
            use windows_sys::Win32::Globalization::{GetLocaleInfoW, LOCALE_SDECIMAL};
            let mut buf = [0u16; 8];
            let count = unsafe {
                GetLocaleInfoW(0x0400, LOCALE_SDECIMAL, buf.as_mut_ptr(), buf.len() as i32)
            };
            if count > 0 && buf[0] == u16::from(b',') {
                return NumberLocale::Comma;
            }
        }
        NumberLocale::Dot
    })
}

pub fn system_currency_symbol() -> &'static str {
    static CACHED: OnceLock<String> = OnceLock::new();
    CACHED
        .get_or_init(|| {
            #[cfg(windows)]
            {
                use windows_sys::Win32::Globalization::{GetLocaleInfoW, LOCALE_SCURRENCY};
                let mut buf = [0u16; 16];
                let count = unsafe {
                    GetLocaleInfoW(0x0400, LOCALE_SCURRENCY, buf.as_mut_ptr(), buf.len() as i32)
                };
                if count > 1 {
                    let s = String::from_utf16_lossy(&buf[..(count as usize - 1)]);
                    let trimmed = s.trim();
                    if !trimmed.is_empty() {
                        return trimmed.to_uppercase();
                    }
                }
            }
            "$".to_owned()
        })
        .as_str()
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegionalSettings {
    pub date_order: DateOrder,
    pub number_locale: NumberLocale,
}

impl RegionalSettings {
    pub fn decimal_and_thousand(self) -> (char, char) {
        self.number_locale.separators()
    }
}

/// Per-column comparison rules mirroring BC5 `TTableColHandling` (`sub_141793DA0`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ColHandling {
    /// `AsText` (`+0x00`): when `false` (`General`, BC5 default), parses typed
    /// cell values; when `true`, forces raw text comparison.
    pub as_text: bool,
    /// `Insig` (`+0x08`): differences in this column are always `Minor` and the
    /// column is excluded from row alignment unless every column is unimportant.
    pub unimportant: bool,
    /// `StrCaseInsig` (`+0x19`): case-only text differences become `Minor`.
    pub ignore_case: bool,
    /// `StrWSInsig` (`+0x1A`): whitespace-only text differences (`' '`, `'\t'`,
    /// `'\r'`, `'\n'`, per `sub_1404FF420`) become `Minor`.
    pub ignore_whitespace: bool,
    /// `NumTolerance` (`+0x10`): numeric / currency tolerance value (string for
    /// arbitrary-precision `BigDecimal`).
    pub num_tolerance: String,
    /// `NumToleranceRelative` (`+0x18`): when `true`, `num_tolerance` is a
    /// percentage of `max(|left|, |right|)` (`(tol / 100) * max(|a|, |b|)`).
    pub num_tolerance_relative: bool,
    /// `DateTimeTolerance` (`+0x04`): datetime tolerance in seconds.
    pub datetime_tolerance_secs: u64,
}

impl Default for ColHandling {
    fn default() -> Self {
        Self {
            as_text: false,
            unimportant: false,
            ignore_case: false,
            ignore_whitespace: false,
            num_tolerance: "0".into(),
            num_tolerance_relative: false,
            datetime_tolerance_secs: 0,
        }
    }
}

/// Native value hint preserved by binary/structured readers (XLSX, XLS, XLSB,
/// ODS, DBF) so cross-format comparisons (e.g. XLSX vs PT-BR CSV) retain exact
/// semantic types without depending on display-text regional separators.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CellHint {
    None,
    Bool(bool),
    Number(BigDecimal),
    Date(NaiveDate),
    Time(NaiveTime),
    DateTime(NaiveDateTime),
}

/// BC5 `TTableVal` DateTime subtype (`sub_1417B4C60`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DateTimeKind {
    /// Date only (`0x01`).
    Date,
    /// Time only (`0x10`).
    Time,
    /// Date + time (`0x11`).
    DateTime,
}

/// BC5 `TTableVal` kind tag (`*(u8*)(val + 8)`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValKind {
    Bool = 1,
    Currency = 2,
    DateTime = 3,
    Number = 4,
    Text = 5,
}

/// Parsed semantic cell value mirroring BC5 `TTableVal`.
#[derive(Clone, Debug)]
pub enum TableVal {
    Bool(bool),
    Currency {
        symbol: String,
        value: BigDecimal,
    },
    DateTime {
        kind: DateTimeKind,
        /// Milliseconds since Unix epoch (for `Date`/`DateTime`) or since
        /// midnight `00:00:00.000` (for `Time`).
        millis: i64,
    },
    Number(BigDecimal),
    Text(String),
}

impl TableVal {
    pub fn kind(&self) -> ValKind {
        match self {
            Self::Bool(_) => ValKind::Bool,
            Self::Currency { .. } => ValKind::Currency,
            Self::DateTime { .. } => ValKind::DateTime,
            Self::Number(_) => ValKind::Number,
            Self::Text(_) => ValKind::Text,
        }
    }

    /// BC5 `sub_1417B4CB0`: sort priority across distinct cell value types.
    /// `Time (1) < Number/Currency (2) < Date/DateTime (3) < Text (4) < Bool (5)`.
    fn sort_group(&self) -> u8 {
        match self {
            Self::DateTime {
                kind: DateTimeKind::Time,
                ..
            } => 1,
            Self::Currency { .. } | Self::Number(_) => 2,
            Self::DateTime { .. } => 3,
            Self::Text(_) => 4,
            Self::Bool(_) => 5,
        }
    }

    /// Canonical string representation for line alignment (`sub_1417B46A0` +
    /// `sub_1417A5D30`).
    pub fn canonical_for_align(&self, handling: &ColHandling) -> String {
        match self {
            Self::Bool(b) => if *b { "B:1" } else { "B:0" }.to_owned(),
            Self::Currency { symbol, value } => {
                format!("C:{symbol}:{}", canonical_decimal(value))
            }
            Self::Number(value) => format!("N:{}", canonical_decimal(value)),
            Self::DateTime { kind, millis } => {
                let tag = match kind {
                    DateTimeKind::Date => 'D',
                    DateTimeKind::Time => 'T',
                    DateTimeKind::DateTime => 'X',
                };
                format!("{tag}:{millis}")
            }
            Self::Text(text) => format!(
                "S:{}",
                normalize_text(text, handling.ignore_case, handling.ignore_whitespace)
            ),
        }
    }
}

fn canonical_decimal(value: &BigDecimal) -> String {
    let normalized = value.normalized();
    if normalized == 0 {
        "0".to_owned()
    } else {
        normalized.to_plain_string()
    }
}

/// BC5 `sub_1404FF420` -> `sub_1404FF2A0` (bitmask ):
/// removes ASCII whitespace characters `' '`, `'\t'`, `'\r'`, `'\n'`.
pub fn strip_bc5_whitespace(text: &str) -> String {
    text.chars()
        .filter(|c| !matches!(c, ' ' | '\t' | '\r' | '\n'))
        .collect()
}

pub fn normalize_text(text: &str, ignore_case: bool, ignore_whitespace: bool) -> String {
    let mut s = if ignore_whitespace {
        strip_bc5_whitespace(text)
    } else {
        text.to_owned()
    };
    if ignore_case {
        s = s.to_uppercase();
    }
    s
}

/// Parse a cell using BC5's exact `TTableVal` recognizer (`sub_1417B2F70`),
/// honoring native workbook/DBF `CellHint` when present and `as_text == false`.
pub fn parse_table_val(
    raw: &str,
    hint: Option<&CellHint>,
    regional: RegionalSettings,
    as_text: bool,
) -> TableVal {
    if as_text {
        return TableVal::Text(raw.to_owned());
    }
    if let Some(hint) = hint {
        match hint {
            CellHint::None => {}
            CellHint::Bool(b) => return TableVal::Bool(*b),
            CellHint::Number(dec) => return TableVal::Number(dec.clone()),
            CellHint::Date(d) => {
                return TableVal::DateTime {
                    kind: DateTimeKind::Date,
                    millis: date_to_millis(*d),
                };
            }
            CellHint::Time(t) => {
                return TableVal::DateTime {
                    kind: DateTimeKind::Time,
                    millis: time_to_millis(*t),
                };
            }
            CellHint::DateTime(dt) => {
                return TableVal::DateTime {
                    kind: DateTimeKind::DateTime,
                    millis: datetime_to_millis(*dt),
                };
            }
        }
    }

    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return TableVal::Text(raw.to_owned());
    }

    // 1. Boolean (`kind = 1`): case-insensitive exact TRUE / FALSE.
    if trimmed.eq_ignore_ascii_case("TRUE") {
        return TableVal::Bool(true);
    }
    if trimmed.eq_ignore_ascii_case("FALSE") {
        return TableVal::Bool(false);
    }

    // 2. Number & Currency (`kind = 4` or `kind = 2`).
    let (dec_sep, thou_sep) = regional.decimal_and_thousand();
    if let Some(val) = parse_currency_or_number(trimmed, dec_sep, thou_sep) {
        return val;
    }

    // 3. Date / Time / DateTime (`kind = 3`).
    if let Some((kind, millis)) = parse_datetime_val(trimmed, regional.date_order, dec_sep) {
        return TableVal::DateTime { kind, millis };
    }

    // 4. Text fallback (`kind = 5`).
    TableVal::Text(raw.to_owned())
}

fn parse_currency_or_number(text: &str, dec_sep: char, thou_sep: char) -> Option<TableVal> {
    let mut s = text;
    let mut sign: i8 = 0; // 0 = unset, 1 = +, -1 = -

    // Accounting negative parentheses: `(123.45)` or `($123.45)`.
    if s.starts_with('(') && s.ends_with(')') && s.len() >= 2 {
        sign = -1;
        s = s[1..s.len() - 1].trim();
        if s.is_empty() {
            return None;
        }
    }

    // Optional leading sign before currency.
    if let Some(rest) = s.strip_prefix('+') {
        if sign != 0 {
            return None;
        }
        sign = 1;
        s = rest.trim_start();
    } else if let Some(rest) = s.strip_prefix('-') {
        if sign != 0 {
            return None;
        }
        sign = -1;
        s = rest.trim_start();
    }

    // Currency prefix: system currency + BC5 built-ins `$`, `€`, `£`.
    let currency = match_currency_prefix(s);
    if let Some((sym, rest)) = &currency {
        let _ = sym;
        s = rest.trim_start();
        // Optional sign after currency symbol (`$-100`, `R$ -50`).
        if let Some(after) = s.strip_prefix('+') {
            if sign != 0 {
                return None;
            }
            sign = 1;
            s = after.trim_start();
        } else if let Some(after) = s.strip_prefix('-') {
            if sign != 0 {
                return None;
            }
            sign = -1;
            s = after.trim_start();
        }
    }

    let num = parse_strict_decimal(s, dec_sep, thou_sep, sign < 0)?;
    if let Some((symbol, _)) = currency {
        Some(TableVal::Currency { symbol, value: num })
    } else {
        Some(TableVal::Number(num))
    }
}

fn match_currency_prefix(s: &str) -> Option<(String, &str)> {
    let sys = system_currency_symbol();
    // Order candidates longest-first so e.g. `R$` matches before `$`.
    let mut candidates = [sys, "$", "€", "£"];
    candidates.sort_by_key(|c| std::cmp::Reverse(c.len()));

    let upper = s.to_uppercase();
    for cand in candidates {
        if cand.is_empty() {
            continue;
        }
        if upper.starts_with(cand) {
            // Slice original `s` by matching character count (safe for UTF-8).
            let char_count = cand.chars().count();
            let byte_end = s
                .char_indices()
                .nth(char_count)
                .map_or(s.len(), |(idx, _)| idx);
            return Some((cand.to_owned(), &s[byte_end..]));
        }
    }
    None
}

/// BC5 strict numeric grammar (`sub_1417B2F70` lines 226..395):
/// - Up to 64 significant digits (rejects absurdly long inputs)
/// - Strict 3-digit grouping around `thou_sep` (`digits <= 3` before first
///   separator, and `digits - last_sep == 3` between consecutive separators
///   and before decimal/exponent/end)
/// - Supports leading decimal separator (`.5` / `,5`) and scientific `E`/`e`
///   exponent within `[-10_000, 10_000]`.
pub fn parse_strict_decimal(
    s: &str,
    dec_sep: char,
    thou_sep: char,
    negative: bool,
) -> Option<BigDecimal> {
    if s.is_empty() || s.len() > 4096 {
        return None;
    }

    let mut chars = s.chars().peekable();
    let mut normalized = String::with_capacity(s.len() + 2);
    if negative {
        normalized.push('-');
    }

    let mut int_digits: usize = 0;
    let mut total_sig_digits: usize = 0;
    let mut seen_decimal = false;
    let mut last_thou_digit_pos: Option<usize> = None;
    let mut has_any_digit = false;

    while let Some(&ch) = chars.peek() {
        if ch.is_ascii_digit() {
            chars.next();
            has_any_digit = true;
            if !seen_decimal {
                int_digits += 1;
            }
            if !(total_sig_digits == 0 && ch == '0' && !seen_decimal) {
                total_sig_digits += 1;
                if total_sig_digits > 64 {
                    return None;
                }
            }
            normalized.push(ch);
        } else if ch == thou_sep && thou_sep != dec_sep && !seen_decimal {
            // Strict BC5 thousand grouping:
            // Must have 1..=3 digits before the first separator, and exactly 3
            // digits since the previous separator.
            match last_thou_digit_pos {
                None => {
                    if int_digits == 0 || int_digits > 3 {
                        return None;
                    }
                }
                Some(prev) => {
                    if int_digits - prev != 3 {
                        return None;
                    }
                }
            }
            last_thou_digit_pos = Some(int_digits);
            chars.next();
            // Must be followed by a digit.
            if !chars.peek().is_some_and(char::is_ascii_digit) {
                return None;
            }
        } else if ch == dec_sep && !seen_decimal {
            if let Some(prev) = last_thou_digit_pos {
                if int_digits - prev != 3 {
                    return None;
                }
            }
            seen_decimal = true;
            chars.next();
            // BC5 requires a digit immediately after decimal separator if there
            // were no integer digits yet (`.5` valid, `.` invalid).
            if int_digits == 0 && !chars.peek().is_some_and(char::is_ascii_digit) {
                return None;
            }
            if int_digits == 0 {
                normalized.push('0');
            }
            normalized.push('.');
        } else {
            break;
        }
    }

    if !has_any_digit {
        return None;
    }
    if !seen_decimal {
        if let Some(prev) = last_thou_digit_pos {
            if int_digits - prev != 3 {
                return None;
            }
        }
    }

    // Optional scientific notation exponent `e` / `E`.
    if matches!(chars.peek(), Some('e' | 'E')) {
        chars.next();
        normalized.push('e');
        let mut exp_neg = false;
        if let Some(&sign_ch @ ('+' | '-')) = chars.peek() {
            exp_neg = sign_ch == '-';
            normalized.push(sign_ch);
            chars.next();
        }
        let mut exp_digits = String::new();
        while let Some(&d) = chars.peek() {
            if d.is_ascii_digit() {
                exp_digits.push(d);
                chars.next();
            } else {
                break;
            }
        }
        if exp_digits.is_empty() {
            return None;
        }
        let exp_val = exp_digits.parse::<i32>().ok()?;
        let signed_exp = if exp_neg { -exp_val } else { exp_val };
        if signed_exp.unsigned_abs() > 10_000 {
            return None;
        }
        normalized.push_str(&exp_digits);
    }

    // Must have consumed the entire trimmed token.
    if chars.peek().is_some() {
        return None;
    }

    // Strip trailing bare decimal point if any (e.g. `12.`).
    if normalized.ends_with('.') {
        normalized.push('0');
    }

    BigDecimal::from_str(&normalized).ok()
}

/// Loose tolerance parser that accepts optional leading sign and respects the
/// configured decimal/thousand separators.
pub fn parse_tolerance(text: &str, regional: RegionalSettings) -> Option<BigDecimal> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Some(BigDecimal::from(0));
    }
    let (dec_sep, thou_sep) = regional.decimal_and_thousand();
    if trimmed.starts_with('-') {
        return None;
    }
    let clean = trimmed.strip_prefix('+').unwrap_or(trimmed).trim();
    parse_strict_decimal(clean, dec_sep, thou_sep, false)
}

fn date_to_millis(d: NaiveDate) -> i64 {
    d.and_hms_opt(0, 0, 0)
        .map_or(0, |dt| dt.and_utc().timestamp_millis())
}

fn time_to_millis(t: NaiveTime) -> i64 {
    i64::from(t.num_seconds_from_midnight()) * 1_000 + i64::from(t.nanosecond() / 1_000_000)
}

fn datetime_to_millis(dt: NaiveDateTime) -> i64 {
    dt.and_utc().timestamp_millis()
}

fn parse_month_name(token: &str) -> Option<u32> {
    match token.to_ascii_lowercase().as_str() {
        "jan" | "january" | "janeiro" => Some(1),
        "feb" | "february" | "fev" | "fevereiro" => Some(2),
        "mar" | "march" | "março" | "marco" => Some(3),
        "apr" | "april" | "abr" | "abril" => Some(4),
        "may" | "mai" | "maio" => Some(5),
        "jun" | "june" | "junho" => Some(6),
        "jul" | "july" | "julho" => Some(7),
        "aug" | "august" | "ago" | "agosto" => Some(8),
        "sep" | "sept" | "september" | "set" | "setembro" => Some(9),
        "oct" | "october" | "out" | "outubro" => Some(10),
        "nov" | "november" | "novembro" => Some(11),
        "dec" | "december" | "dez" | "dezembro" => Some(12),
        _ => None,
    }
}

fn parse_time_part(raw: &str, dec_sep: char) -> Option<NaiveTime> {
    let mut s = raw.trim();
    if s.is_empty() || s.len() > 24 {
        return None;
    }
    // Optional trailing 'Z' (ISO UTC marker).
    if let Some(stripped) = s.strip_suffix(['Z', 'z']) {
        s = stripped.trim_end();
    }

    // Optional AM/PM (prefix or suffix, as in BC5 `sub_1417B2D50`).
    let mut ampm: Option<bool> = None; // Some(true) = PM, Some(false) = AM
    let upper = s.to_ascii_uppercase();
    if let Some(rest) = upper.strip_suffix("AM") {
        ampm = Some(false);
        s = s[..rest.len()].trim_end();
    } else if let Some(rest) = upper.strip_suffix("PM") {
        ampm = Some(true);
        s = s[..rest.len()].trim_end();
    } else if let Some(rest) = upper.strip_prefix("AM") {
        ampm = Some(false);
        s = s[s.len() - rest.len()..].trim_start();
    } else if let Some(rest) = upper.strip_prefix("PM") {
        ampm = Some(true);
        s = s[s.len() - rest.len()..].trim_start();
    }

    let parts: Vec<&str> = s.split(':').collect();
    if !(2..=3).contains(&parts.len()) {
        return None;
    }
    let mut hour: u32 = parts[0].trim().parse().ok()?;
    let min: u32 = parts[1].trim().parse().ok()?;
    if parts[0].is_empty() || parts[1].is_empty() || min > 59 {
        return None;
    }
    let (sec, millis) = if parts.len() == 3 {
        let sec_str = parts[2].trim();
        let split_pos = sec_str.find(['.', dec_sep]);
        if let Some(pos) = split_pos {
            let whole: u32 = sec_str[..pos].parse().ok()?;
            let frac = &sec_str[pos + 1..];
            if frac.is_empty() || !frac.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            // Convert fractional seconds to milliseconds (first 3 digits, right-padded).
            let mut ms_val = 0u32;
            let mut scale = 100u32;
            for b in frac.bytes().take(3) {
                ms_val += u32::from(b - b'0') * scale;
                scale /= 10;
            }
            (whole, ms_val)
        } else {
            (sec_str.parse::<u32>().ok()?, 0)
        }
    } else {
        (0, 0)
    };
    if sec > 59 {
        return None;
    }

    if let Some(is_pm) = ampm {
        if hour == 0 || hour > 12 {
            return None;
        }
        if hour == 12 {
            hour = 0;
        }
        if is_pm {
            hour += 12;
        }
    } else if hour > 23 {
        return None;
    }

    NaiveTime::from_hms_milli_opt(hour, min, sec, millis)
}

fn parse_date_part(raw: &str, order: DateOrder) -> Option<NaiveDate> {
    let s = raw.trim();
    if !(6..=32).contains(&s.len()) {
        return None;
    }

    // Standard numeric or named-month date separated by '/', '-', or '.'.
    let sep = if s.contains('/') {
        '/'
    } else if s.contains('-') {
        '-'
    } else if s.contains('.') {
        '.'
    } else {
        // Check space/comma month-name dates like `Jan 15, 2024` or `15 Jan 2024`.
        return parse_named_month_space_date(s, order);
    };

    let parts: Vec<&str> = s.split(sep).map(str::trim).collect();
    if parts.len() != 3 || parts.iter().any(|p| p.is_empty()) {
        return None;
    }

    // Named month in slot 0 or slot 1 (e.g. `15-Jan-2024` or `Jan-15-2024`).
    if let Some(month) = parse_month_name(parts[1]) {
        let a: u32 = parts[0].parse().ok()?;
        let b: u32 = parts[2].parse().ok()?;
        return if parts[0].len() == 4 {
            NaiveDate::from_ymd_opt(a as i32, month, b)
        } else if parts[2].len() == 4 {
            NaiveDate::from_ymd_opt(b as i32, month, a)
        } else {
            None
        };
    }
    if let Some(month) = parse_month_name(parts[0]) {
        let day: u32 = parts[1].parse().ok()?;
        let year: u32 = parts[2].parse().ok()?;
        if parts[2].len() == 4 {
            return NaiveDate::from_ymd_opt(year as i32, month, day);
        }
        return None;
    }

    if parts.iter().any(|p| !p.bytes().all(|b| b.is_ascii_digit())) {
        return None;
    }
    let n0: u32 = parts[0].parse().ok()?;
    let n1: u32 = parts[1].parse().ok()?;
    let n2: u32 = parts[2].parse().ok()?;

    // Four-digit years only so ambiguous codes like `01/02/24` never collide.
    let (y, m, d) = if parts[0].len() == 4 {
        (n0, n1, n2)
    } else if parts[2].len() == 4 {
        match order.resolved() {
            DateOrder::Mdy => (n2, n0, n1),
            DateOrder::Ymd => return None,
            DateOrder::Dmy | DateOrder::System => (n2, n1, n0),
        }
    } else {
        return None;
    };
    NaiveDate::from_ymd_opt(y as i32, m, d)
}

fn parse_named_month_space_date(s: &str, order: DateOrder) -> Option<NaiveDate> {
    let cleaned = s.replace(',', " ");
    let parts: Vec<&str> = cleaned.split_whitespace().collect();
    if parts.len() != 3 {
        return None;
    }
    let _ = order;
    if let Some(month) = parse_month_name(parts[0]) {
        let day: u32 = parts[1].parse().ok()?;
        let year: u32 = parts[2].parse().ok()?;
        if parts[2].len() == 4 {
            return NaiveDate::from_ymd_opt(year as i32, month, day);
        }
    } else if let Some(month) = parse_month_name(parts[1]) {
        let day: u32 = parts[0].parse().ok()?;
        let year: u32 = parts[2].parse().ok()?;
        if parts[2].len() == 4 {
            return NaiveDate::from_ymd_opt(year as i32, month, day);
        }
    }
    None
}

fn parse_datetime_val(
    trimmed: &str,
    order: DateOrder,
    dec_sep: char,
) -> Option<(DateTimeKind, i64)> {
    // Try pure time first (`14:30`, `14:30:05.250`, `2:30 PM`).
    if trimmed.contains(':') && !trimmed.contains('/') && !trimmed.contains('-') {
        if let Some(time) = parse_time_part(trimmed, dec_sep) {
            return Some((DateTimeKind::Time, time_to_millis(time)));
        }
    }

    // Try splitting date and time on 'T' or space (taking care with named-month dates).
    if let Some((d_part, t_part)) = trimmed.split_once('T') {
        if let (Some(d), Some(t)) = (
            parse_date_part(d_part, order),
            parse_time_part(t_part, dec_sep),
        ) {
            return Some((DateTimeKind::DateTime, datetime_to_millis(d.and_time(t))));
        }
    }

    // Space-separated Date + Time: find first token containing ':'.
    if trimmed.contains(':') {
        for (idx, ch) in trimmed.char_indices() {
            if ch == ' ' {
                let d_part = trimmed[..idx].trim();
                let t_part = trimmed[idx + 1..].trim();
                if let (Some(d), Some(t)) = (
                    parse_date_part(d_part, order),
                    parse_time_part(t_part, dec_sep),
                ) {
                    return Some((DateTimeKind::DateTime, datetime_to_millis(d.and_time(t))));
                }
            }
        }
    }

    // Pure date.
    if let Some(d) = parse_date_part(trimmed, order) {
        return Some((DateTimeKind::Date, date_to_millis(d)));
    }

    None
}

fn within_numeric_tolerance(
    a: &BigDecimal,
    b: &BigDecimal,
    tol: &BigDecimal,
    relative: bool,
) -> bool {
    if *tol <= 0 {
        return false;
    }
    let diff = (a - b).abs();
    if relative {
        // BC5 `TTableMap.GetDiffs` lines 281-295:
        // effective_tol = (tol / 100) * max(|a|, |b|)
        let max_abs = a.abs().max(b.abs());
        let effective = (tol / BigDecimal::from(100)) * max_abs;
        diff <= effective
    } else {
        diff <= *tol
    }
}

/// Cell operand passed to `compare_cells`.
#[derive(Clone, Copy)]
pub struct CellInput<'a> {
    pub raw: Option<&'a str>,
    pub hint: Option<&'a CellHint>,
    pub regional: RegionalSettings,
}

/// Compare two cells identically to BC5 `TTableMap.GetDiffs` ().
pub fn compare_cells(
    left: CellInput<'_>,
    right: CellInput<'_>,
    handling: &ColHandling,
    tolerance_regional: RegionalSettings,
) -> Change {
    if left.raw == right.raw && left.hint == right.hint {
        return Change::Same;
    }
    if handling.unimportant {
        if left.raw == right.raw {
            return Change::Same;
        }
        return Change::Minor;
    }
    if matches!((left.raw, right.raw), (None, Some("")) | (Some(""), None)) {
        return Change::Minor;
    }
    let (Some(a_raw), Some(b_raw)) = (left.raw, right.raw) else {
        return Change::Important;
    };
    if a_raw == b_raw && left.hint.is_none() && right.hint.is_none() {
        return Change::Same;
    }

    let a_val = parse_table_val(a_raw, left.hint, left.regional, handling.as_text);
    let b_val = parse_table_val(b_raw, right.hint, right.regional, handling.as_text);

    match (&a_val, &b_val) {
        (TableVal::Bool(a), TableVal::Bool(b)) => {
            if a == b {
                Change::Same
            } else {
                Change::Important
            }
        }
        (
            TableVal::Currency {
                symbol: sym_a,
                value: val_a,
            },
            TableVal::Currency {
                symbol: sym_b,
                value: val_b,
            },
        ) if sym_a == sym_b => {
            if val_a == val_b {
                Change::Same
            } else if parse_tolerance(&handling.num_tolerance, tolerance_regional).is_some_and(
                |tol| within_numeric_tolerance(val_a, val_b, &tol, handling.num_tolerance_relative),
            ) {
                Change::Minor
            } else {
                Change::Important
            }
        }
        (TableVal::Number(val_a), TableVal::Number(val_b)) => {
            if val_a == val_b {
                Change::Same
            } else if parse_tolerance(&handling.num_tolerance, tolerance_regional).is_some_and(
                |tol| within_numeric_tolerance(val_a, val_b, &tol, handling.num_tolerance_relative),
            ) {
                Change::Minor
            } else {
                Change::Important
            }
        }
        (
            TableVal::DateTime {
                kind: kind_a,
                millis: ms_a,
            },
            TableVal::DateTime {
                kind: kind_b,
                millis: ms_b,
            },
        ) if kind_a == kind_b => {
            let diff_ms = ms_a.abs_diff(*ms_b);
            if diff_ms == 0 {
                Change::Same
            } else if handling.datetime_tolerance_secs > 0
                && diff_ms <= handling.datetime_tolerance_secs.saturating_mul(1_000)
            {
                Change::Minor
            } else {
                Change::Important
            }
        }
        _ => {
            // Text comparison or type-mismatch fallback (`GetDiffs` lines 213..221).
            if a_raw == b_raw {
                Change::Same
            } else if (handling.ignore_case || handling.ignore_whitespace)
                && normalize_text(a_raw, handling.ignore_case, handling.ignore_whitespace)
                    == normalize_text(b_raw, handling.ignore_case, handling.ignore_whitespace)
            {
                Change::Minor
            } else {
                Change::Important
            }
        }
    }
}

/// Typed sort comparator mirroring BC5 `sub_1417B4D20` + `sub_1417B4CB0`.
pub fn cmp_table_vals(a: &TableVal, b: &TableVal, handling: &ColHandling) -> Ordering {
    let group_cmp = a.sort_group().cmp(&b.sort_group());
    if group_cmp != Ordering::Equal {
        return group_cmp;
    }
    match (a, b) {
        (TableVal::Bool(x), TableVal::Bool(y)) => x.cmp(y),
        (TableVal::DateTime { millis: x, .. }, TableVal::DateTime { millis: y, .. }) => x.cmp(y),
        (TableVal::Number(x), TableVal::Number(y)) => x.cmp(y),
        (
            TableVal::Currency {
                symbol: sx,
                value: vx,
            },
            TableVal::Currency {
                symbol: sy,
                value: vy,
            },
        ) => vx.cmp(vy).then_with(|| sx.cmp(sy)),
        (TableVal::Currency { value: vx, .. }, TableVal::Number(vy))
        | (TableVal::Number(vx), TableVal::Currency { value: vy, .. }) => vx.cmp(vy),
        (TableVal::Text(x), TableVal::Text(y)) => {
            if handling.ignore_case || handling.ignore_whitespace {
                normalize_text(x, handling.ignore_case, handling.ignore_whitespace).cmp(
                    &normalize_text(y, handling.ignore_case, handling.ignore_whitespace),
                )
            } else {
                x.cmp(y)
            }
        }
        _ => Ordering::Equal,
    }
}

/// Display formatted date/datetime according to `DateOrder`, preserving pure
/// text or forced `as_text` columns untouched.
pub fn display_cell<'a>(
    raw: &'a str,
    hint: Option<&CellHint>,
    order: DateOrder,
    as_text: bool,
) -> std::borrow::Cow<'a, str> {
    if as_text {
        return std::borrow::Cow::Borrowed(raw);
    }
    let resolved = order.resolved();
    let date_fmt = match resolved {
        DateOrder::Mdy => "%m/%d/%Y",
        DateOrder::Ymd => "%Y/%m/%d",
        DateOrder::Dmy | DateOrder::System => "%d/%m/%Y",
    };

    if let Some(hint) = hint {
        match hint {
            CellHint::Date(d) => return std::borrow::Cow::Owned(d.format(date_fmt).to_string()),
            CellHint::DateTime(dt) => {
                let mut out = dt.date().format(date_fmt).to_string();
                if dt.nanosecond() == 0 {
                    out.push_str(&dt.time().format(" %H:%M:%S").to_string());
                } else {
                    out.push_str(&dt.time().format(" %H:%M:%S%.3f").to_string());
                }
                return std::borrow::Cow::Owned(out);
            }
            CellHint::Time(t) => {
                let out = if t.nanosecond() == 0 {
                    t.format("%H:%M:%S").to_string()
                } else {
                    t.format("%H:%M:%S%.3f").to_string()
                };
                return std::borrow::Cow::Owned(out);
            }
            _ => return std::borrow::Cow::Borrowed(raw),
        }
    }

    // Legacy / text cell date normalization (preserves `table_columns::display` contract).
    if let Some(dt) = crate::table_columns::date(raw, order) {
        let mut value = dt.format(date_fmt).to_string();
        if raw.contains('T') || raw.contains(' ') {
            value.push_str(&dt.format(" %H:%M:%S%.f").to_string());
        }
        return std::borrow::Cow::Owned(value);
    }

    std::borrow::Cow::Borrowed(raw)
}

/// Clean Excel/ODS IEEE-754 float noise (e.g. `0.30000000000000004` -> `"0.3"`,
/// `4.0` -> `"4"`), matching Excel's 15-significant-digit precision.
pub fn format_workbook_float(v: f64) -> (String, Option<BigDecimal>) {
    if !v.is_finite() {
        return (v.to_string(), None);
    }
    if v == 0.0 {
        return ("0".to_owned(), Some(BigDecimal::from(0)));
    }
    // Check exact integer in safe f64 range first.
    if v.fract() == 0.0 && v.abs() < 1e15 {
        let i = v as i64;
        return (i.to_string(), Some(BigDecimal::from(i)));
    }
    // Format to 15 significant digits via scientific notation, then normalize to
    // plain decimal when compact (matches Excel's 15-digit rounding).
    let sci = format!("{v:.14e}");
    let text = if let Ok(dec) = BigDecimal::from_str(&sci) {
        let norm = dec.normalized();
        let plain = norm.to_plain_string();
        if plain.len() <= 24 {
            plain
        } else {
            v.to_string()
        }
    } else {
        v.to_string()
    };
    let dec = BigDecimal::from_str(&text).ok();
    (text, dec)
}

/// Parse ODS ISO-8601 duration (`PT14H30M00S`, `PT09H05M12.5S`) into a `NaiveTime`.
pub fn parse_iso_duration_time(iso: &str) -> Option<NaiveTime> {
    let s = iso.trim();
    let rest = s.strip_prefix("PT").or_else(|| s.strip_prefix("pt"))?;
    let mut num = String::new();
    let mut h = 0u32;
    let mut m = 0u32;
    let mut sec = 0u32;
    let mut ms = 0u32;
    let mut any = false;
    for ch in rest.chars() {
        if ch.is_ascii_digit() || ch == '.' || ch == ',' {
            num.push(if ch == ',' { '.' } else { ch });
        } else {
            any = true;
            match ch.to_ascii_uppercase() {
                'H' => h = num.parse().ok()?,
                'M' => m = num.parse().ok()?,
                'S' => {
                    if let Some((w, f)) = num.split_once('.') {
                        sec = w.parse().ok()?;
                        let mut scale = 100u32;
                        for b in f.bytes().take(3) {
                            if !b.is_ascii_digit() {
                                return None;
                            }
                            ms += u32::from(b - b'0') * scale;
                            scale /= 10;
                        }
                    } else {
                        sec = num.parse().ok()?;
                    }
                }
                _ => return None,
            }
            num.clear();
        }
    }
    if !any || !num.is_empty() || h > 23 || m > 59 || sec > 59 {
        return None;
    }
    NaiveTime::from_hms_milli_opt(h, m, sec, ms)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn us() -> RegionalSettings {
        RegionalSettings {
            date_order: DateOrder::Mdy,
            number_locale: NumberLocale::Dot,
        }
    }

    fn br() -> RegionalSettings {
        RegionalSettings {
            date_order: DateOrder::Dmy,
            number_locale: NumberLocale::Comma,
        }
    }

    fn cmp_str(
        a: &str,
        ra: RegionalSettings,
        b: &str,
        rb: RegionalSettings,
        h: &ColHandling,
        tr: RegionalSettings,
    ) -> Change {
        compare_cells(
            CellInput {
                raw: Some(a),
                hint: None,
                regional: ra,
            },
            CellInput {
                raw: Some(b),
                hint: None,
                regional: rb,
            },
            h,
            tr,
        )
    }

    #[test]
    fn parses_bc5_booleans_currencies_numbers_dates_times_and_strict_thousands() {
        assert!(matches!(
            parse_table_val(" true ", None, us(), false),
            TableVal::Bool(true)
        ));
        assert!(matches!(
            parse_table_val("FALSE", None, br(), false),
            TableVal::Bool(false)
        ));

        let def = ColHandling::default();
        // US thousands grouping: valid vs invalid
        assert_eq!(
            cmp_str("1,234.50", us(), "1234.5", us(), &def, us()),
            Change::Same
        );
        // `1,23` fails strict 3-digit grouping in US -> Text != Number
        assert_eq!(
            cmp_str("1,23", us(), "123", us(), &def, us()),
            Change::Important
        );
        // PT-BR strict 3-digit grouping: `1.00` fails (only 2 digits after `.`),
        // matching the native BC5 Windows PT-BR probe!
        assert_eq!(
            cmp_str("1.00", br(), "1", br(), &def, br()),
            Change::Important
        );
        assert_eq!(cmp_str("1,00", br(), "1", br(), &def, br()), Change::Same);
        // Cross-regional: US CSV `1,234.50` vs PT-BR CSV `1.234,5`!
        assert_eq!(
            cmp_str("1,234.50", us(), "1.234,5", br(), &def, us()),
            Change::Same
        );
        // Accounting negative parentheses & currency
        assert_eq!(
            cmp_str("($1,000.00)", us(), "-$1000", us(), &def, us()),
            Change::Same
        );
        // Different currencies fall back to text (Important)
        assert_eq!(
            cmp_str("$100", us(), "€100", us(), &def, us()),
            Change::Important
        );
    }

    #[test]
    fn supports_relative_percent_tolerance_and_datetime_tolerance() {
        let pct_handling = ColHandling {
            num_tolerance: "5".into(),
            num_tolerance_relative: true,
            ..Default::default()
        };
        assert_eq!(
            cmp_str("100", us(), "104.9", us(), &pct_handling, us()),
            Change::Minor
        );
        assert_eq!(
            cmp_str("100", us(), "110", us(), &pct_handling, us()),
            Change::Important
        );

        // Datetime seconds tolerance & Time-only parsing
        let dt_handling = ColHandling {
            datetime_tolerance_secs: 60,
            ..Default::default()
        };
        assert_eq!(
            cmp_str(
                "2024-05-10 14:30:00",
                us(),
                "10/05/2024 14:30:45",
                br(),
                &dt_handling,
                us()
            ),
            Change::Minor
        );
        assert_eq!(
            cmp_str("2:30 PM", us(), "14:30:30", br(), &dt_handling, us()),
            Change::Minor
        );
        // Date-only vs DateTime falls back to Text (BC5 subtype check)
        assert_eq!(
            cmp_str(
                "2024-05-10",
                us(),
                "2024-05-10 00:00:00",
                us(),
                &dt_handling,
                us()
            ),
            Change::Important
        );
    }

    #[test]
    fn bc5_whitespace_rule_strips_all_internal_and_edge_whitespace() {
        let h = ColHandling {
            ignore_whitespace: true,
            ..Default::default()
        };
        assert_eq!(
            cmp_str("hello world", us(), "  hello   world \t", us(), &h, us()),
            Change::Minor
        );
    }

    #[test]
    fn typed_sort_matches_bc5_group_and_numeric_order() {
        let h = ColHandling::default();
        let a = parse_table_val("2", None, us(), false);
        let b = parse_table_val("10", None, us(), false);
        assert_eq!(cmp_table_vals(&a, &b, &h), Ordering::Less);

        let t = parse_table_val("09:00", None, us(), false);
        let d = parse_table_val("2024-01-01", None, us(), false);
        let s = parse_table_val("abc", None, us(), false);
        let bl = parse_table_val("TRUE", None, us(), false);
        // Time(1) < Number(2) < Date(3) < Text(4) < Bool(5)
        assert_eq!(cmp_table_vals(&t, &a, &h), Ordering::Less);
        assert_eq!(cmp_table_vals(&a, &d, &h), Ordering::Less);
        assert_eq!(cmp_table_vals(&d, &s, &h), Ordering::Less);
        assert_eq!(cmp_table_vals(&s, &bl, &h), Ordering::Less);
    }

    #[test]
    fn cleans_workbook_floats_and_ods_iso_durations() {
        let (txt, dec) = format_workbook_float(0.1 + 0.2);
        assert_eq!(txt, "0.3");
        assert_eq!(dec, BigDecimal::from_str("0.3").ok());
        let (txt2, _) = format_workbook_float(4.0);
        assert_eq!(txt2, "4");
        assert_eq!(
            parse_iso_duration_time("PT14H30M05.250S"),
            NaiveTime::from_hms_milli_opt(14, 30, 5, 250)
        );
    }
}
