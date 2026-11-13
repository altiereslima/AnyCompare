//! BC5 comparison columns: physical column numbers are never display indexes.
use crate::table::{Options, Table};
use bigdecimal::BigDecimal;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum DateOrder {
    #[default]
    System,
    Dmy,
    Mdy,
    Ymd,
}
impl DateOrder {
    pub fn resolved(self) -> Self {
        if self != Self::System {
            return self;
        }
        use windows_sys::Win32::Globalization::{GetLocaleInfoW, LOCALE_IDATE};
        let mut value = [0u16; 4];
        let count =
            unsafe { GetLocaleInfoW(0x0400, LOCALE_IDATE, value.as_mut_ptr(), value.len() as i32) };
        if count > 0 {
            match value[0] {
                48 => Self::Mdy,
                50 => Self::Ymd,
                _ => Self::Dmy,
            }
        } else {
            Self::Dmy
        }
    }
    pub fn label(self) -> &'static str {
        match self.resolved() {
            Self::Mdy => "MDA",
            Self::Ymd => "AMD",
            _ => "DMA",
        }
    }
}
pub fn date(text: &str, order: DateOrder) -> Option<chrono::NaiveDateTime> {
    let text = text.trim();
    if !(8..=40).contains(&text.len()) {
        return None;
    }
    let (date, time) = text
        .split_once(['T', ' '])
        .map_or((text, None), |(a, b)| (a, Some(b)));
    let parts: Vec<_> = date.split(['/', '-', '.']).collect();
    if parts.len() != 3
        || parts
            .iter()
            .any(|s| s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()))
    {
        return None;
    }
    let nums = parts
        .iter()
        .map(|s| s.parse::<u32>().ok())
        .collect::<Option<Vec<_>>>()?;
    // Four-digit years only: do not invent a pivot for ambiguous short years.
    let (y, m, d) = if parts[0].len() == 4 {
        (nums[0], nums[1], nums[2])
    } else if parts[2].len() == 4 {
        match order.resolved() {
            DateOrder::Mdy => (nums[2], nums[0], nums[1]),
            DateOrder::Ymd => return None,
            _ => (nums[2], nums[1], nums[0]),
        }
    } else {
        return None;
    };
    let date = chrono::NaiveDate::from_ymd_opt(y as i32, m, d)?;
    let time = if let Some(t) = time {
        chrono::NaiveTime::parse_from_str(t, "%H:%M:%S%.f")
            .or_else(|_| chrono::NaiveTime::parse_from_str(t, "%H:%M"))
            .ok()?
    } else {
        chrono::NaiveTime::from_hms_opt(0, 0, 0)?
    };
    Some(date.and_time(time))
}
pub fn display(text: &str, order: DateOrder) -> std::borrow::Cow<'_, str> {
    let Some(date) = date(text, order) else {
        return std::borrow::Cow::Borrowed(text);
    };
    let format = match order.resolved() {
        DateOrder::Mdy => "%m/%d/%Y",
        DateOrder::Ymd => "%Y/%m/%d",
        _ => "%d/%m/%Y",
    };
    let mut value = date.format(format).to_string();
    if text.contains('T') || text.contains(' ') {
        value.push_str(&date.format(" %H:%M:%S%.f").to_string());
    }
    std::borrow::Cow::Owned(value)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Alignment {
    #[default]
    Position,
    LeftNames,
    RightNames,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Column {
    pub left: Option<usize>,
    pub right: Option<usize>,
}
impl Column {
    pub fn side(self, left: bool) -> Option<usize> {
        if left {
            self.left
        } else {
            self.right
        }
    }
}
fn name(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect()
}
fn heading(table: &Table, column: usize) -> &str {
    table
        .rows
        .first()
        .and_then(|r| r.cells.get(column))
        .map_or("", String::as_str)
}
pub fn columns(left: &Table, right: &Table, options: &Options) -> Vec<Column> {
    if options.column_alignment == Alignment::Position || !options.header {
        return (0..left.columns.max(right.columns))
            .map(|i| Column {
                left: (i < left.columns).then_some(i),
                right: (i < right.columns).then_some(i),
            })
            .collect();
    }
    let reversed = options.column_alignment == Alignment::RightNames;
    let (a, b) = if reversed {
        (right, left)
    } else {
        (left, right)
    };
    let mut names = BTreeMap::<String, VecDeque<usize>>::new();
    for i in 0..b.columns {
        names.entry(name(heading(b, i))).or_default().push_back(i);
    }
    let mut used = vec![false; b.columns];
    let mut result = Vec::new();
    for i in 0..a.columns {
        let j = names
            .get_mut(&name(heading(a, i)))
            .and_then(VecDeque::pop_front);
        if let Some(j) = j {
            used[j] = true;
        }
        result.push(Column {
            left: Some(i),
            right: j,
        });
    }
    for (j, used) in used.into_iter().enumerate() {
        if !used {
            result.push(Column {
                left: None,
                right: Some(j),
            });
        }
    }
    if reversed {
        for col in &mut result {
            std::mem::swap(&mut col.left, &mut col.right);
        }
    }
    result
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Change {
    Same,
    Minor,
    Important,
}

/// Bound the decimal exponent before arbitrary-precision arithmetic. Never use
/// f64: identifiers/amounts beyond 2^53 must not collapse to the same number.
pub fn decimal(text: &str, comma: bool) -> Option<BigDecimal> {
    let text = text.trim();
    if text.len() > 4096 {
        return None;
    }
    let normalized = if comma {
        text.replace('.', "").replace(',', ".")
    } else {
        text.to_owned()
    };
    if let Some((_, exponent)) = normalized.split_once(['e', 'E']) {
        if exponent.parse::<i32>().ok()?.unsigned_abs() > 10000 {
            return None;
        }
    }
    normalized.parse().ok()
}
pub fn change(options: &Options, column: usize, a: Option<&str>, b: Option<&str>) -> Change {
    if a == b {
        return Change::Same;
    }
    if options.unimportant_columns.contains(&column) {
        return Change::Minor;
    }
    if matches!((a, b), (None, Some("")) | (Some(""), None)) {
        return Change::Minor;
    }
    let (Some(a), Some(b)) = (a, b) else {
        return Change::Important;
    };
    if let (Some(a), Some(b)) = (date(a, options.date_order), date(b, options.date_order)) {
        return if a == b {
            Change::Same
        } else {
            Change::Important
        };
    }
    if options.equal(Some(a), Some(b)) {
        return Change::Minor;
    }
    if options.numeric {
        if let (Some(a), Some(b), Some(tolerance)) = (
            decimal(a, options.decimal_comma),
            decimal(b, options.decimal_comma),
            decimal(&options.tolerance, options.decimal_comma),
        ) {
            if tolerance >= 0 && (a - b).abs() <= tolerance {
                return Change::Minor;
            }
        }
    }
    Change::Important
}

/// Sheet correspondence is stable and retains duplicate names. Unlike column
/// names, sheet names are matched exactly; the BC5.2 whitespace/case change is
/// documented for columns only.
pub fn sheets(left: &[String], right: &[String]) -> Vec<Column> {
    if left.is_empty() || right.is_empty() {
        return (0..left.len().max(1).max(right.len()))
            .map(|i| Column {
                left: (i < left.len().max(1)).then_some(i),
                right: (i < right.len().max(1)).then_some(i),
            })
            .collect();
    }
    let mut right_names = BTreeMap::<&str, VecDeque<usize>>::new();
    for (index, name) in right.iter().enumerate() {
        right_names.entry(name).or_default().push_back(index);
    }
    let mut result = Vec::new();
    let mut used = vec![false; right.len()];
    for (index, name) in left.iter().enumerate() {
        let matching = right_names
            .get_mut(name.as_str())
            .and_then(VecDeque::pop_front);
        if let Some(index) = matching {
            used[index] = true;
        }
        result.push(Column {
            left: Some(index),
            right: matching,
        });
    }
    for (index, used) in used.into_iter().enumerate() {
        if !used {
            result.push(Column {
                left: None,
                right: Some(index),
            });
        }
    }
    result
}

pub fn sheets_aligned(left: &[String], right: &[String], alignment: Alignment) -> Vec<Column> {
    match alignment {
        Alignment::Position => (0..left.len().max(1).max(right.len().max(1)))
            .map(|i| Column {
                left: (i < left.len().max(1)).then_some(i),
                right: (i < right.len().max(1)).then_some(i),
            })
            .collect(),
        Alignment::LeftNames => sheets(left, right),
        Alignment::RightNames => sheets(right, left)
            .into_iter()
            .map(|c| Column {
                left: c.right,
                right: c.left,
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::table::parse;
    #[test]
    fn column_names_ignore_case_whitespace_and_preserve_duplicate_occurrences() {
        let a = parse(" ID ,English,Text,Text,OnlyLeft\nx,a,1,2,3", b',').unwrap();
        let b = parse("english,id,Text,OnlyRight,Text\na,x,1,4,2", b',').unwrap();
        let options = Options {
            header: true,
            column_alignment: Alignment::LeftNames,
            ..Default::default()
        };
        let cols = columns(&a, &b, &options);
        assert_eq!(
            cols.iter().map(|c| c.right).collect::<Vec<_>>(),
            [Some(1), Some(0), Some(2), Some(4), None, Some(3)]
        );
        let reverse = columns(
            &a,
            &b,
            &Options {
                column_alignment: Alignment::RightNames,
                ..options
            },
        );
        assert_eq!(
            reverse[0],
            Column {
                left: Some(1),
                right: Some(0)
            }
        );
    }
    #[test]
    fn decimal_tolerance_never_loses_large_integer_or_fraction_precision() {
        let mut options = Options {
            numeric: true,
            tolerance: "0.00000000000000000001".into(),
            ..Default::default()
        };
        assert_eq!(
            change(
                &options,
                0,
                Some("9007199254740992"),
                Some("9007199254740993")
            ),
            Change::Important
        );
        assert_eq!(
            change(
                &options,
                0,
                Some("0.12345678901234567890"),
                Some("0.12345678901234567891")
            ),
            Change::Minor
        );
        assert_eq!(
            change(
                &options,
                0,
                Some("0.12345678901234567890"),
                Some("0.12345678901234567892")
            ),
            Change::Important
        );
        options.decimal_comma = true;
        options.tolerance = "0,01".into();
        assert_eq!(
            change(&options, 0, Some("1.234,56"), Some("1234,565")),
            Change::Minor
        );
        assert!(decimal("1e999999999", false).is_none());
    }
    #[test]
    fn native_regional_probe_preserves_numeric_precision_case_and_date_interpretation() {
        let mut options = Options {
            date_order: DateOrder::Dmy,
            ..Default::default()
        };
        for (a, b, expected) in [
            ("1.00", "1", Change::Important),
            ("9007199254740993", "9007199254740992", Change::Important),
            ("Test", "test", Change::Important),
            ("hello world", "hello  world", Change::Important),
            ("01/02/2024", "1/2/2024", Change::Same),
            ("02/01/2024", "2024-01-02", Change::Same),
        ] {
            assert_eq!(change(&options, 0, Some(a), Some(b)), expected, "{a} / {b}");
        }
        assert_eq!(display("1/2/2024", DateOrder::Dmy), "01/02/2024");
        options.date_order = DateOrder::Mdy;
        assert_eq!(
            change(&options, 0, Some("02/01/2024"), Some("2024-01-02")),
            Change::Important
        );
        assert!(date("30/02/2024", DateOrder::Dmy).is_none());
        assert!(date("01/02/24", DateOrder::Dmy).is_none());
    }
}
