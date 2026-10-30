use crate::{
    document::atomic_write,
    ui::dialogs::{DialogField, DialogHost},
};
use std::path::PathBuf;

pub struct Row {
    pub key: String,
    pub left: String,
    pub right: String,
    pub different: bool,
}
fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
pub fn html(title: &str, left: &str, right: &str, rows: &[Row], only_differences: bool) -> String {
    let title = crate::i18n::tr(title);
    let language = crate::i18n::language();
    let position = crate::i18n::tr("Posição");
    let mut out=format!("<!doctype html><html lang=\"{language}\"><meta charset=\"utf-8\"><title>{}</title><style>body{{font:13px Tahoma,Arial}}table{{border-collapse:collapse;width:100%;table-layout:fixed}}th,td{{border:1px solid #ddd;padding:3px;vertical-align:top;overflow-wrap:anywhere;white-space:pre-wrap}}td{{font:12px monospace}}.diff{{background:#ffe3e3}}.key{{width:8%}}</style><h1>{}</h1><table><tr><th class=\"key\">{position}</th><th>{}</th><th>{}</th></tr>",escape(&title),escape(&title),escape(left),escape(right));
    for row in rows.iter().filter(|row| !only_differences || row.different) {
        out.push_str(&format!(
            "<tr{}><td>{}</td><td>{}</td><td>{}</td></tr>",
            if row.different { " class=\"diff\"" } else { "" },
            escape(&row.key),
            escape(&row.left),
            escape(&row.right)
        ));
    }
    out.push_str("</table></html>");
    out
}
pub fn export(
    title: &str,
    left: &str,
    right: &str,
    rows: &[Row],
    inputs: &[PathBuf],
    dialogs: &mut dyn DialogHost,
) {
    let mut fields = [
        DialogField::Choice("Formato:".into(), 0, vec!["HTML".into(), "Texto".into()]),
        DialogField::Check("Somente diferenças".into(), false),
    ];
    if !dialogs.fields("Relatório", &mut fields) {
        return;
    }
    let title = crate::i18n::tr(title);
    let text = matches!(fields[0], DialogField::Choice(_, 1, _));
    let only = matches!(fields[1], DialogField::Check(_, true));
    if let Some(mut path) = dialogs.report_path(if text { "txt" } else { "html" }) {
        if path.extension().is_none() {
            path.set_extension(if text { "txt" } else { "html" });
        }
        let canonical = |path: &std::path::Path| {
            path.canonicalize()
                .unwrap_or_else(|_| path.to_owned())
                .to_string_lossy()
                .to_lowercase()
        };
        if inputs.iter().any(|input| {
            canonical(input) == canonical(&path)
                || input.is_dir()
                    && path
                        .canonicalize()
                        .is_ok_and(|p| input.canonicalize().is_ok_and(|root| p.starts_with(root)))
        }) {
            dialogs.error("O relatório não pode substituir um arquivo comparado.");
            return;
        }
        let output = if text {
            let mut output =
                crate::i18n::tr(&format!("{title}\nEsquerda: {left}\nDireita: {right}\n\n"));
            for row in rows.iter().filter(|row| !only || row.different) {
                output.push_str(&format!("{}\n< {}\n> {}\n\n", row.key, row.left, row.right));
            }
            output
        } else {
            html(&title, left, right, rows, only)
        };
        match atomic_write(&path, output.as_bytes()) {
            Ok(()) => dialogs.info(&format!("Relatório salvo em:\n{}", path.display())),
            Err(error) => dialogs.error(&error),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reports_escape_content_filter_rows_and_reject_input_overwrite() {
        let rows = [
            Row {
                key: "1".into(),
                left: "<script>alert(1)</script>".into(),
                right: "ação & teste".into(),
                different: true,
            },
            Row {
                key: "2".into(),
                left: "igual".into(),
                right: "igual".into(),
                different: false,
            },
        ];
        let output = html("Título", "a", "b", &rows, true);
        assert!(!output.contains("<script>"));
        assert!(output.contains("&lt;script&gt;"));
        assert!(!output.contains("igual"));
        let mut dialogs = crate::ui::dialogs::TestDialogs::default();
        dialogs.field_answers.push_back(Some(vec![
            DialogField::Choice("".into(), 0, vec![]),
            DialogField::Check("".into(), true),
        ]));
        let input = std::env::temp_dir().join("anycompare-report-source.html");
        dialogs.files.push_back(Some(input.clone()));
        export("Título", "a", "b", &rows, &[input], &mut dialogs);
        assert_eq!(dialogs.errors.len(), 1);
    }
}
