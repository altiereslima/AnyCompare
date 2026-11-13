use crate::{
    table::{self, Options, Side, Table},
    table_columns::{self, Column},
};

#[derive(Clone, Debug)]
pub struct Summary {
    pub rows: usize,
    pub important: usize,
    pub minor: usize,
    pub error: Option<String>,
}

pub fn load(side: &Side, sheet: Option<usize>) -> Result<Side, String> {
    let Some(sheet) = sheet else {
        return Ok(Side {
            table: Table::default(),
            document: None,
            missing_sheet: true,
            ..side.clone()
        });
    };
    if sheet == side.sheet && !side.missing_sheet {
        return Ok(side.clone());
    }
    Side::load(
        std::path::Path::new(&side.path),
        side.document.as_ref().map(|_| side.delimiter),
        sheet,
    )
}
pub fn compare(left: &Side, right: &Side, options: &Options, sheets: &[Column]) -> Vec<Summary> {
    sheets
        .iter()
        .map(|sheet| {
            let result = (|| -> Result<Summary, String> {
                let a = load(left, sheet.left)?;
                let b = load(right, sheet.right)?;
                let columns = table_columns::columns(&a.table, &b.table, options);
                let pairs = table::align_columns(&a.table, &b.table, options, &columns);
                Ok(Summary {
                    rows: pairs.len(),
                    important: pairs.iter().filter(|p| p.important).count(),
                    minor: pairs.iter().filter(|p| p.different && !p.important).count(),
                    error: None,
                })
            })();
            result.unwrap_or_else(|error| Summary {
                rows: 0,
                important: 0,
                minor: 0,
                error: Some(error),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    #[test]
    fn compares_every_sheet_and_keeps_orphan_sheets_explicit() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/tables");
        let a = Side::load(&root.join("left.xlsx"), None, 0).unwrap();
        let b = Side::load(&root.join("right.xlsx"), None, 0).unwrap();
        let pairs = table_columns::sheets(&a.sheets, &b.sheets);
        let results = compare(&a, &b, &Options::default(), &pairs);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].important, 4);
        assert_eq!(results[1].important, 0);
        assert!(results.iter().all(|r| r.error.is_none()));
        let missing = load(&b, None).unwrap();
        assert!(missing.missing_sheet);
        assert!(missing.table.rows.is_empty());
        assert_eq!(missing.path, b.path);
        let restored = load(&missing, Some(0)).unwrap();
        assert!(!restored.missing_sheet);
        assert!(!restored.table.rows.is_empty());
        let names = |names: &[&str]| names.iter().map(|n| n.to_string()).collect::<Vec<_>>();
        assert_eq!(
            table_columns::sheets(&names(&["A", "B", "A"]), &names(&["B", "A", "C", "A"])),
            vec![
                Column {
                    left: Some(0),
                    right: Some(1)
                },
                Column {
                    left: Some(1),
                    right: Some(0)
                },
                Column {
                    left: Some(2),
                    right: Some(3)
                },
                Column {
                    left: None,
                    right: Some(2)
                }
            ]
        );
    }
}
