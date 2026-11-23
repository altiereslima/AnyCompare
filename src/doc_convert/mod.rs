//! Built-in document-to-text converters (PDF, DOCX, DOC, RTF, ODT, PPTX, ODP, EPUB).
//!
//! Mirrors AnyCompare's automatic read-only text conversion for non-plain-text
//! documents ("Documentos PDF · Convertido", "Documentos MS Word · Convertido", etc.).
//! Original binary files on disk are never overwritten by saving a converted buffer.

mod office;
mod pdf;
mod rtf;

use std::path::Path;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConvertedDocument {
    pub text: String,
    pub format_label: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DocKind {
    Pdf,
    Docx,
    Doc,
    Rtf,
    Odt,
    Pptx,
    Odp,
    Epub,
}

impl DocKind {
    fn from_path(path: &Path) -> Option<Self> {
        let ext = path.extension()?.to_string_lossy().to_ascii_lowercase();
        match ext.as_str() {
            "pdf" => Some(Self::Pdf),
            "docx" | "docm" | "dotx" | "dotm" => Some(Self::Docx),
            "doc" | "dot" => Some(Self::Doc),
            "rtf" => Some(Self::Rtf),
            "odt" | "ott" => Some(Self::Odt),
            "pptx" | "pptm" | "ppsx" | "potx" => Some(Self::Pptx),
            "odp" | "otp" => Some(Self::Odp),
            "epub" => Some(Self::Epub),
            _ => None,
        }
    }

    fn format_label(self) -> &'static str {
        match self {
            Self::Pdf => "doc.format.pdf",
            Self::Docx | Self::Doc => "doc.format.word",
            Self::Rtf => "doc.format.rtf",
            Self::Odt => "doc.format.odt",
            Self::Pptx => "doc.format.pptx",
            Self::Odp => "doc.format.odp",
            Self::Epub => "doc.format.epub",
        }
    }
}

pub fn is_convertible(path: &Path) -> bool {
    DocKind::from_path(path).is_some()
}

#[allow(dead_code)]
pub fn format_label_for_path(path: &Path) -> Option<&'static str> {
    DocKind::from_path(path).map(DocKind::format_label)
}

pub fn convert_bytes(path: &Path, bytes: &[u8]) -> Result<ConvertedDocument, String> {
    let kind = DocKind::from_path(path)
        .ok_or_else(|| format!("Formato não conversível: {}", path.display()))?;
    let raw_text = match kind {
        DocKind::Pdf => pdf::extract_text(bytes)?,
        DocKind::Docx => office::extract_docx(bytes)?,
        DocKind::Doc => office::extract_doc(bytes)?,
        DocKind::Rtf => rtf::extract_text(bytes)?,
        DocKind::Odt => office::extract_odt(bytes)?,
        DocKind::Pptx => office::extract_pptx(bytes)?,
        DocKind::Odp => office::extract_odp(bytes)?,
        DocKind::Epub => office::extract_epub(bytes)?,
    };
    Ok(ConvertedDocument {
        text: clean_converted_text(&raw_text),
        format_label: kind.format_label(),
    })
}

pub(crate) fn clean_converted_text(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut blank_run = 0usize;
    let bytes = input.as_bytes();
    let mut pos = 0usize;
    let mut line = String::with_capacity(128);
    while pos <= bytes.len() {
        let mut end = pos;
        while end < bytes.len() && bytes[end] != b'\n' && bytes[end] != b'\r' {
            end += 1;
        }
        let raw_line = &input[pos..end];
        let next_pos = if end < bytes.len() {
            if bytes[end] == b'\r' && bytes.get(end + 1) == Some(&b'\n') {
                end + 2
            } else {
                end + 1
            }
        } else {
            bytes.len() + 1
        };
        line.clear();
        for ch in raw_line.chars() {
            match ch {
                '\0' | '\u{00AD}' | '\u{FEFF}' => {}
                '\u{00A0}' | '\u{2007}' | '\u{202F}' => line.push(' '),
                '\u{FB00}' => line.push_str("ff"),
                '\u{FB01}' => line.push_str("fi"),
                '\u{FB02}' => line.push_str("fl"),
                '\u{FB03}' => line.push_str("ffi"),
                '\u{FB04}' => line.push_str("ffl"),
                c if c.is_control() && c != '\t' => {}
                c => line.push(c),
            }
        }
        let trimmed = line.trim_end();
        if trimmed.is_empty() {
            blank_run += 1;
            if !out.is_empty() && blank_run <= 1 {
                out.push('\n');
            }
        } else {
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(trimmed);
            blank_run = 0;
        }
        pos = next_pos;
    }
    while out.ends_with('\n') {
        out.pop();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn make_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut cursor = std::io::Cursor::new(Vec::new());
        {
            let mut zw = zip::ZipWriter::new(&mut cursor);
            let opts = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated);
            for (name, data) in entries {
                zw.start_file(*name, opts).unwrap();
                zw.write_all(data).unwrap();
            }
            zw.finish().unwrap();
        }
        cursor.into_inner()
    }

    #[test]
    fn converts_rtf_with_portuguese_accents_and_unicode() {
        let rtf = br#"{\rtf1\ansi\ansicpg1252{\fonttbl{\f0 Calibri;}}{\colortbl;\red0\green0\blue0;}
{\*\generator AnyCompare;}
Ol\'e1 Mundo! A\u231?\'e3o e cora\'e7\'e3o.\par
Segunda linha\tab com tab.\line Terceira linha.}"#;
        let conv = convert_bytes(Path::new("teste.rtf"), rtf).unwrap();
        assert_eq!(conv.format_label, "doc.format.rtf");
        assert_eq!(crate::i18n::translate("pt-BR", conv.format_label), "Documentos RTF");
        assert!(conv.text.contains("Olá Mundo! Ação e coração."), "got: {}", conv.text);
        assert!(conv.text.contains("Segunda linha\tcom tab."), "got: {}", conv.text);
        assert!(conv.text.contains("Terceira linha."), "got: {}", conv.text);
        assert!(!conv.text.contains("Calibri"));
        assert!(!conv.text.contains("AnyCompare"));
    }

    #[test]
    fn converts_docx_paragraphs_tables_and_accents() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p><w:r><w:t>Título em Português</w:t></w:r></w:p>
    <w:p><w:r><w:t>Linha um</w:t><w:br/><w:t>Linha dois</w:t></w:r></w:p>
    <w:tbl><w:tr>
      <w:tc><w:p><w:r><w:t>Col A</w:t></w:r></w:p></w:tc>
      <w:tc><w:p><w:r><w:t>Col B</w:t></w:r></w:p></w:tc>
    </w:tr></w:tbl>
  </w:body>
</w:document>"#;
        let bytes = make_zip(&[("word/document.xml", xml.as_bytes())]);
        let conv = convert_bytes(Path::new("relatorio.docx"), &bytes).unwrap();
        assert_eq!(conv.format_label, "doc.format.word");
        assert!(conv.text.contains("Título em Português"), "got: {}", conv.text);
        assert!(conv.text.contains("Linha um\nLinha dois"), "got: {}", conv.text);
        assert!(conv.text.contains("Col A\tCol B"), "got: {}", conv.text);
    }

    #[test]
    fn converts_odt_pptx_odp_and_epub() {
        let odt_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0">
  <office:body><office:text>
    <text:h>Capítulo 1</text:h>
    <text:p>Texto<text:s text:c="2"/>com espaços e acentuação.</text:p>
  </office:text></office:body>
</office:document-content>"#;
        let odt = make_zip(&[("content.xml", odt_xml.as_bytes())]);
        let conv = convert_bytes(Path::new("nota.odt"), &odt).unwrap();
        assert_eq!(conv.format_label, "doc.format.odt");
        assert!(conv.text.contains("Capítulo 1"));
        assert!(conv.text.contains("Texto  com espaços e acentuação."));

        let s1 = br#"<p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><p:cSld><p:spTree><p:sp><p:txBody><a:p><a:r><a:t>Slide Um</a:t></a:r></a:p></p:txBody></p:sp></p:spTree></p:cSld></p:sld>"#;
        let s2 = br#"<p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><p:cSld><p:spTree><p:sp><p:txBody><a:p><a:r><a:t>Slide Dois</a:t></a:r></a:p></p:txBody></p:sp></p:spTree></p:cSld></p:sld>"#;
        let pptx = make_zip(&[
            ("ppt/slides/slide2.xml", s2),
            ("ppt/slides/slide1.xml", s1),
        ]);
        let conv = convert_bytes(Path::new("deck.pptx"), &pptx).unwrap();
        assert_eq!(conv.format_label, "doc.format.pptx");
        let p1 = conv.text.find("Slide Um").unwrap();
        let p2 = conv.text.find("Slide Dois").unwrap();
        assert!(p1 < p2, "slides must be ordered numerically: {}", conv.text);

        let container = br#"<?xml version="1.0"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#;
        let opf = br#"<?xml version="1.0"?><package xmlns="http://www.idpf.org/2007/opf"><manifest><item id="c1" href="ch1.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="c1"/></spine></package>"#;
        let ch1 = "<html><body><h1>Prólogo</h1><p>Era uma vez em Português.</p></body></html>".as_bytes();
        let epub = make_zip(&[
            ("META-INF/container.xml", container),
            ("OEBPS/content.opf", opf),
            ("OEBPS/ch1.xhtml", ch1),
        ]);
        let conv = convert_bytes(Path::new("livro.epub"), &epub).unwrap();
        assert_eq!(conv.format_label, "doc.format.epub");
        assert!(conv.text.contains("Prólogo"));
        assert!(conv.text.contains("Era uma vez em Português."));
    }

    #[test]
    fn converts_minimal_pdf_with_flatedecode_and_tounicode() {
        let cmap = b"/CIDInit /ProcSet findresource begin 12 dict begin begincmap\n\
1 begincodespacerange <00> <FF> endcodespacerange\n\
3 beginbfchar\n<01> <00E9>\n<02> <00E3>\n<03> <00660069>\nendbfchar\nendcmap end end";
        let content = b"BT /F1 12 Tf 72 720 Td (Caf\x01 e a\xe7\x02o \x03m) Tj 0 -18 Td (Segunda linha) Tj ET";
        let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(content).unwrap();
        let zcontent = encoder.finish().unwrap();

        let mut pdf_bytes = Vec::new();
        pdf_bytes.extend_from_slice(b"%PDF-1.4\n");
        let mut offsets = Vec::new();
        let objs: Vec<Vec<u8>> = vec![
            b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n".to_vec(),
            b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n".to_vec(),
            b"3 0 obj\n<< /Type /Page /Parent 2 0 R /Resources << /Font << /F1 4 0 R >> >> /Contents 6 0 R >>\nendobj\n".to_vec(),
            b"4 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding /ToUnicode 5 0 R >>\nendobj\n".to_vec(),
            [
                format!("5 0 obj\n<< /Length {} >>\nstream\n", cmap.len()).into_bytes(),
                cmap.to_vec(),
                b"\nendstream\nendobj\n".to_vec(),
            ].concat(),
            [
                format!("6 0 obj\n<< /Filter /FlateDecode /Length {} >>\nstream\n", zcontent.len()).into_bytes(),
                zcontent,
                b"\nendstream\nendobj\n".to_vec(),
            ].concat(),
        ];
        for o in &objs {
            offsets.push(pdf_bytes.len());
            pdf_bytes.extend_from_slice(o);
        }
        let xref_pos = pdf_bytes.len();
        pdf_bytes.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
        for off in offsets {
            pdf_bytes.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes());
        }
        pdf_bytes.extend_from_slice(
            format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref_pos}\n%%EOF\n", objs.len() + 1).as_bytes(),
        );

        let conv = convert_bytes(Path::new("sample.pdf"), &pdf_bytes).unwrap();
        assert_eq!(conv.format_label, "doc.format.pdf");
        assert!(conv.text.contains("Café e ação fim"), "got: {:?}", conv.text);
        assert!(conv.text.contains("Segunda linha"), "got: {:?}", conv.text);
    }
}
