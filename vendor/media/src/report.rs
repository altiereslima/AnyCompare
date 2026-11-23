//! Media Compare reports: summary and side-by-side, with display filters.
use crate::{FieldComparison, FieldStatus, MediaComparison, MediaError};
use serde::{Deserialize, Serialize};
use std::fmt::Write;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReportLayout {
    Summary,
    SideBySide,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReportDisplay {
    All,
    Mismatches,
    Matches,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ReportOptions {
    pub layout: ReportLayout,
    pub display: ReportDisplay,
    pub ignore_unimportant: bool,
}
impl Default for ReportOptions {
    fn default() -> Self {
        Self {
            layout: ReportLayout::SideBySide,
            display: ReportDisplay::All,
            ignore_unimportant: false,
        }
    }
}

fn visible(f: &FieldComparison, options: ReportOptions) -> bool {
    let difference = match f.status {
        FieldStatus::Same => false,
        FieldStatus::ImportantDifference => true,
        FieldStatus::UnimportantDifference => !options.ignore_unimportant,
    };
    match options.display {
        ReportDisplay::All => true,
        ReportDisplay::Mismatches => difference,
        ReportDisplay::Matches => !difference,
    }
}
fn fmt_values(xs: &[String]) -> String {
    if xs.is_empty() {
        "—".to_owned()
    } else {
        xs.join("; ")
    }
}
fn marker(status: FieldStatus) -> &'static str {
    match status {
        FieldStatus::Same => "=",
        FieldStatus::ImportantDifference => "!",
        FieldStatus::UnimportantDifference => "~",
    }
}
fn effective_summary(c: &MediaComparison, options: ReportOptions) -> &'static str {
    if c.important_differences > 0 {
        "important_differences"
    } else if c.unimportant_differences > 0 && !options.ignore_unimportant {
        "unimportant_differences"
    } else {
        "same"
    }
}

pub fn render_text_with_options(c: &MediaComparison, options: ReportOptions) -> String {
    let mut s = String::new();
    let _ = writeln!(
        s,
        "MEDIA COMPARE\nEsquerda: {}\nDireita: {}",
        c.left.path, c.right.path
    );
    let _ = writeln!(
        s,
        "Resultado: {}\nIguais: {} | Importantes: {} | Secundárias: {}",
        effective_summary(c, options),
        c.same,
        c.important_differences,
        c.unimportant_differences
    );
    if options.layout == ReportLayout::Summary {
        return s;
    }
    s.push('\n');
    for f in c.fields.iter().filter(|f| visible(f, options)) {
        let _ = writeln!(
            s,
            "{} {}\n  E: {}\n  D: {}",
            marker(f.status),
            f.key,
            fmt_values(&f.left),
            fmt_values(&f.right)
        );
    }
    s
}
fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

pub fn render_html_with_options(c: &MediaComparison, options: ReportOptions) -> String {
    let mut s = String::from("<!doctype html><html lang=\"pt-BR\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"><title>Media Compare</title><style>body{font:14px system-ui,sans-serif;margin:2rem;max-width:1100px;color:#1e2530;background:#fafbfd}h1{font-size:1.6rem}table{border-collapse:collapse;width:100%;table-layout:fixed}th,td{padding:.7rem;border:1px solid #d9e0e9;word-break:break-word;vertical-align:top}thead{background:#e7ebf1}tr.important{background:#ffdbdb}tr.unimportant{background:#fff5d6}tr.same{background:#f5f7fa}.stats{margin:1rem 0;color:#4b5563}code{font-size:12px}@media(prefers-color-scheme:dark){body{background:#161b24;color:#e0e7ee}thead{background:#323e4d}tr.same{background:#252e3a}tr.important{background:#583337}tr.unimportant{background:#4a4431}th,td{border-color:#556272}.stats{color:#bbc9d5}}</style></head><body><h1>Comparação de mídia</h1>");
    let _ = write!(
        s,
        "<p><strong>Esquerda:</strong> {}<br><strong>Direita:</strong> {}</p>",
        escape(&c.left.path),
        escape(&c.right.path)
    );
    let _ = write!(
        s,
        "<p class=\"stats\">{} · {} iguais · {} diferenças importantes · {} secundárias</p>",
        escape(effective_summary(c, options)),
        c.same,
        c.important_differences,
        c.unimportant_differences
    );
    if options.layout == ReportLayout::Summary {
        s.push_str("</body></html>");
        return s;
    }
    s.push_str(
        "<table><thead><tr><th>Campo</th><th>Esquerda</th><th>Direita</th></tr></thead><tbody>",
    );
    for f in c.fields.iter().filter(|f| visible(f, options)) {
        let css = match f.status {
            FieldStatus::Same => "same",
            FieldStatus::ImportantDifference => "important",
            FieldStatus::UnimportantDifference => "unimportant",
        };
        let _ = write!(
            s,
            "<tr class=\"{}\"><th scope=\"row\">{}</th><td>{}</td><td>{}</td></tr>",
            css,
            escape(&f.key),
            escape(&fmt_values(&f.left)),
            escape(&fmt_values(&f.right))
        );
    }
    s.push_str("</tbody></table></body></html>");
    s
}
pub fn render_text(c: &MediaComparison) -> String {
    render_text_with_options(c, ReportOptions::default())
}
pub fn render_html(c: &MediaComparison) -> String {
    render_html_with_options(c, ReportOptions::default())
}
pub fn to_json(c: &MediaComparison) -> Result<String, MediaError> {
    Ok(serde_json::to_string_pretty(c)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{compare_media, MediaInfo, MediaRules};
    use std::collections::BTreeMap;
    fn media() -> MediaInfo {
        MediaInfo {
            path: "<unsafe>".into(),
            file_size: 0,
            file_type: "Mpeg".into(),
            fields: BTreeMap::new(),
            raw_tags: vec![],
            tag_sources: vec![],
            pictures: vec![],
            warnings: vec![],
        }
    }
    #[test]
    fn html_escapes_untrusted_tags() {
        let mut l = media();
        l.add_field("tag.title", "<script>alert(1)</script>");
        let c = compare_media(&l, &l, &MediaRules::default());
        let html = render_html(&c);
        assert!(html.contains("&lt;script&gt;"));
        assert!(!html.contains("<script>"));
    }
    #[test]
    fn difference_filter_and_summary_are_independent() {
        let mut l = media();
        let mut r = media();
        l.add_field("tag.artist", "A");
        r.add_field("tag.artist", "B");
        l.add_field("tag.title", "T");
        r.add_field("tag.title", "T");
        let c = compare_media(&l, &r, &MediaRules::default());
        let options = ReportOptions {
            layout: ReportLayout::SideBySide,
            display: ReportDisplay::Mismatches,
            ignore_unimportant: false,
        };
        let txt = render_text_with_options(&c, options);
        assert!(txt.contains("tag.artist"));
        assert!(!txt.contains("tag.title"));
        let short = render_html_with_options(
            &c,
            ReportOptions {
                layout: ReportLayout::Summary,
                ..options
            },
        );
        assert!(!short.contains("<table>"));
        assert!(short.contains("&lt;unsafe&gt;"));
    }
}
