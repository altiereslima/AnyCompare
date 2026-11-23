//! Read local HTML tables as independent sheets. No scripts or remote requests.
use crate::table::{Row, Table};
use scraper::{ElementRef, Html, Selector};
use std::collections::BTreeMap;

pub struct Sheet {
    pub name: String,
    pub table: Table,
}
fn text(element: ElementRef<'_>, output: &mut String) {
    for child in element.children() {
        if let Some(element) = ElementRef::wrap(child) {
            match element.value().name() {
                "script" | "style" | "table" => {}
                "br" => output.push('\n'),
                "p" | "div" => {
                    if !output.is_empty() && !output.ends_with('\n') {
                        output.push('\n');
                    }
                    text(element, output);
                }
                _ => text(element, output),
            }
        } else if let scraper::Node::Text(value) = child.value() {
            output.push_str(value);
        }
    }
}
fn span(cell: ElementRef<'_>, attribute: &str) -> Result<usize, String> {
    let n = cell
        .value()
        .attr(attribute)
        .unwrap_or("1")
        .parse::<usize>()
        .map_err(|_| format!("{attribute} inválido."))?;
    if n == 0 || n > 16384 {
        return Err(format!("{attribute} fora do limite suportado (1–16384)."));
    }
    Ok(n)
}
pub fn read(source: &str) -> Result<Vec<Sheet>, String> {
    let html = Html::parse_document(source);
    let table_selector = Selector::parse("table").unwrap();
    let row_selector = Selector::parse("tr").unwrap();
    let mut result = Vec::new();
    for table in html.select(&table_selector) {
        let caption = table
            .children()
            .filter_map(ElementRef::wrap)
            .find(|e| e.value().name() == "caption")
            .map(|c| c.text().collect::<String>());
        let name = caption
            .filter(|s| !s.trim().is_empty())
            .or_else(|| table.value().attr("id").map(str::to_owned))
            .unwrap_or_else(|| format!("Table {}", result.len() + 1));
        let mut rows = Vec::new();
        let mut columns = 0;
        let mut pending = BTreeMap::<usize, (usize, String)>::new();
        for row in table.select(&row_selector).filter(|r| {
            r.ancestors()
                .filter_map(ElementRef::wrap)
                .find(|e| e.value().name() == "table")
                .is_some_and(|e| e.id() == table.id())
        }) {
            let mut cells = Vec::new();
            let mut occupied = Vec::new();
            for (&column, (remaining, value)) in &mut pending {
                cells.resize(cells.len().max(column + 1), String::new());
                occupied.resize(cells.len(), false);
                cells[column] = value.clone();
                occupied[column] = true;
                *remaining -= 1;
            }
            pending.retain(|_, (remaining, _)| *remaining > 0);
            let mut column = 0;
            for cell in row
                .children()
                .filter_map(ElementRef::wrap)
                .filter(|e| matches!(e.value().name(), "td" | "th"))
            {
                while occupied.get(column) == Some(&true) {
                    column += 1;
                }
                let width = span(cell, "colspan")?;
                let height = span(cell, "rowspan")?;
                if column + width > 16384 {
                    return Err("Tabela HTML excede 16384 colunas.".into());
                }
                let mut value = String::new();
                text(cell, &mut value);
                let value = value
                    .split('\n')
                    .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
                    .collect::<Vec<_>>()
                    .join("\n")
                    .trim()
                    .to_owned();
                cells.resize(cells.len().max(column + width), String::new());
                occupied.resize(cells.len(), false);
                for col in column..column + width {
                    cells[col] = value.clone();
                    occupied[col] = true;
                    if height > 1 {
                        pending.insert(col, (height - 1, value.clone()));
                    }
                }
                column += width;
            }
            columns = columns.max(cells.len());
            rows.push(Row {
                cells,
                formulas: Vec::new(),
                hints: Vec::new(),
                line: rows.len() + 1,
                span: 0..0,
            });
        }
        if !rows.is_empty() {
            result.push(Sheet {
                name: name.trim().to_owned(),
                table: Table {
                    rows,
                    columns,
                    column_origin: 0,
                },
            });
        }
    }
    if result.is_empty() {
        return Err("Nenhuma tabela HTML foi encontrada no arquivo.".into());
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn multiple_tables_spans_entities_and_line_breaks() {
        let html = r#"<table id="one"><tr><th>ID</th><th>Value</th></tr><tr><td rowspan="2">a</td><td>ação &amp; texto<br>next</td></tr><tr><td>b</td></tr></table><table><caption>Second</caption><tr><td colspan="2">merged</td></tr></table>"#;
        let sheets = read(html).unwrap();
        assert_eq!(sheets.len(), 2);
        assert_eq!(sheets[0].table.rows[1].cells, ["a", "ação & texto\nnext"]);
        assert_eq!(sheets[0].table.rows[2].cells, ["a", "b"]);
        assert_eq!(sheets[1].name, "Second");
        assert_eq!(sheets[1].table.rows[0].cells, ["merged", "merged"]);
    }
    #[test]
    fn nested_tables_are_separate_and_extreme_spans_are_rejected() {
        let tables = read("<table><tr><td>Outer<table><tr><td>Inner</table></table>").unwrap();
        assert_eq!(tables.len(), 2);
        assert_eq!(tables[0].table.rows.len(), 1);
        assert_eq!(tables[0].table.rows[0].cells, ["Outer"]);
        assert!(read("<table><tr><td colspan='999999999'>bad</table>").is_err());
        assert!(read("<p>No tables<script>ignored()</script>").is_err());
    }
}
