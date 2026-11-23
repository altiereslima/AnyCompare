//! Pure-Rust Office & eBook text converters:
//! - DOCX / DOCM / DOTX (`word/document.xml`, footnotes, endnotes)
//! - ODT / OTT / ODP (`content.xml`)
//! - PPTX / PPTM / PPSX (`ppt/slides/slideN.xml`)
//! - EPUB (`META-INF/container.xml` -> OPF spine -> XHTML)
//! - Legacy binary DOC (OLE2 / CFBF Compound File + Word FIB/CLX piece table)

use quick_xml::{events::Event, Reader};
use std::collections::HashMap;
use std::io::{Cursor, Read};
use zip::ZipArchive;

pub fn extract_docx(bytes: &[u8]) -> Result<String, String> {
    let mut zip = ZipArchive::new(Cursor::new(bytes))
        .map_err(|e| format!("Arquivo DOCX inválido: {e}"))?;
    let mut parts = Vec::new();
    for name in ["word/document.xml", "word/footnotes.xml", "word/endnotes.xml"] {
        if let Some(xml) = read_zip_string(&mut zip, name)? {
            let text = parse_docx_xml(&xml)?;
            if !text.trim().is_empty() {
                parts.push(text);
            }
        }
    }
    if parts.is_empty() {
        return Err("Arquivo DOCX não contém word/document.xml legível.".into());
    }
    Ok(parts.join("\n\n"))
}

fn parse_docx_xml(xml: &str) -> Result<String, String> {
    let mut reader = Reader::from_str(xml);
    let mut out = String::new();
    let mut in_text = false;
    let mut in_deleted = false;
    let mut in_cell = false;
    let mut cell_has_para = false;
    let mut first_cell_in_row = true;

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => match local_name(e.name().as_ref()) {
                "t" | "delText" => {
                    if !in_deleted {
                        in_text = true;
                    }
                }
                "del" | "instrText" => in_deleted = true,
                "tr" => {
                    first_cell_in_row = true;
                }
                "tc" => {
                    if !first_cell_in_row {
                        out.push('\t');
                    }
                    first_cell_in_row = false;
                    in_cell = true;
                    cell_has_para = false;
                }
                "p" if in_cell => {
                    if cell_has_para {
                        out.push(' ');
                    }
                    cell_has_para = true;
                }
                _ => {}
            },
            Ok(Event::Empty(e)) => {
                if !in_deleted {
                    match local_name(e.name().as_ref()) {
                        "tab" => out.push('\t'),
                        "br" | "cr" => out.push('\n'),
                        "p" if !in_cell => out.push('\n'),
                        _ => {}
                    }
                }
            }
            Ok(Event::End(e)) => match local_name(e.name().as_ref()) {
                "t" | "delText" => in_text = false,
                "del" | "instrText" => in_deleted = false,
                "p" => {
                    if !in_cell {
                        out.push('\n');
                    }
                }
                "tc" => in_cell = false,
                "tr" => out.push('\n'),
                _ => {}
            },
            Ok(Event::Text(e)) if in_text && !in_deleted => {
                out.push_str(&unescape_xml(e.as_ref()));
            }
            Ok(Event::GeneralRef(e)) if in_text && !in_deleted => {
                push_entity(&mut out, e.as_ref());
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(format!("Erro ao ler XML do DOCX: {e}")),
            _ => {}
        }
    }
    Ok(out)
}

pub fn extract_odt(bytes: &[u8]) -> Result<String, String> {
    extract_odf(bytes, false)
}

pub fn extract_odp(bytes: &[u8]) -> Result<String, String> {
    extract_odf(bytes, true)
}

fn extract_odf(bytes: &[u8], presentation: bool) -> Result<String, String> {
    let mut zip = ZipArchive::new(Cursor::new(bytes))
        .map_err(|e| format!("Arquivo OpenDocument inválido: {e}"))?;
    let xml = read_zip_string(&mut zip, "content.xml")?
        .ok_or_else(|| "Arquivo OpenDocument não contém content.xml.".to_string())?;

    let mut reader = Reader::from_str(&xml);
    let mut out = String::new();
    let mut text_depth = 0usize;
    let mut in_cell = false;
    let mut first_cell = true;
    let mut slide_no = 0usize;

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => match local_name(e.name().as_ref()) {
                "page" if presentation => {
                    slide_no += 1;
                    if !out.is_empty() {
                        out.push('\n');
                    }
                    out.push_str(&format!("--- Slide {slide_no} ---\n"));
                }
                "p" | "h" => {
                    text_depth += 1;
                }
                "table-row" => {
                    first_cell = true;
                }
                "table-cell" => {
                    if !first_cell {
                        out.push('\t');
                    }
                    first_cell = false;
                    in_cell = true;
                }
                "tab" if text_depth > 0 => out.push('\t'),
                "line-break" if text_depth > 0 => out.push('\n'),
                "s" if text_depth > 0 => {
                    let count = attr_usize(&e, "c").unwrap_or(1).min(64);
                    for _ in 0..count {
                        out.push(' ');
                    }
                }
                _ => {}
            },
            Ok(Event::Empty(e)) if text_depth > 0 => match local_name(e.name().as_ref()) {
                "tab" => out.push('\t'),
                "line-break" => out.push('\n'),
                "s" => {
                    let count = attr_usize(&e, "c").unwrap_or(1).min(64);
                    for _ in 0..count {
                        out.push(' ');
                    }
                }
                _ => {}
            },
            Ok(Event::End(e)) => match local_name(e.name().as_ref()) {
                "p" | "h" => {
                    text_depth = text_depth.saturating_sub(1);
                    if !in_cell {
                        out.push('\n');
                    }
                }
                "table-cell" => in_cell = false,
                "table-row" => out.push('\n'),
                _ => {}
            },
            Ok(Event::Text(e)) if text_depth > 0 => {
                out.push_str(&unescape_xml(e.as_ref()));
            }
            Ok(Event::GeneralRef(e)) if text_depth > 0 => {
                push_entity(&mut out, e.as_ref());
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(format!("Erro ao ler XML OpenDocument: {e}")),
            _ => {}
        }
    }
    Ok(out)
}

pub fn extract_pptx(bytes: &[u8]) -> Result<String, String> {
    let mut zip = ZipArchive::new(Cursor::new(bytes))
        .map_err(|e| format!("Arquivo PPTX inválido: {e}"))?;
    let mut slides: Vec<(u32, String)> = Vec::new();
    for i in 0..zip.len() {
        let entry = zip
            .by_index(i)
            .map_err(|e| format!("Erro no ZIP PPTX: {e}"))?;
        let Ok(name_cow) = entry.name() else {
            continue;
        };
        let name = name_cow.into_owned();
        if let Some(rest) = name.strip_prefix("ppt/slides/slide") {
            if let Some(num_str) = rest.strip_suffix(".xml") {
                if let Ok(num) = num_str.parse::<u32>() {
                    slides.push((num, name));
                }
            }
        }
    }
    slides.sort_by_key(|(n, _)| *n);
    if slides.is_empty() {
        return Err("Arquivo PPTX não contém slides legíveis.".into());
    }
    let mut out = String::new();
    for (num, name) in slides {
        let Some(xml) = read_zip_string(&mut zip, &name)? else {
            continue;
        };
        let slide_text = parse_pptx_slide(&xml)?;
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(&format!("--- Slide {num} ---\n"));
        out.push_str(&slide_text);
        if !slide_text.ends_with('\n') {
            out.push('\n');
        }
    }
    Ok(out)
}

fn parse_pptx_slide(xml: &str) -> Result<String, String> {
    let mut reader = Reader::from_str(xml);
    let mut out = String::new();
    let mut in_text = false;
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                if local_name(e.name().as_ref()) == "t" {
                    in_text = true;
                }
            }
            Ok(Event::Empty(e)) => match local_name(e.name().as_ref()) {
                "br" => out.push('\n'),
                "tab" => out.push('\t'),
                _ => {}
            },
            Ok(Event::End(e)) => match local_name(e.name().as_ref()) {
                "t" => in_text = false,
                "p" => out.push('\n'),
                _ => {}
            },
            Ok(Event::Text(e)) if in_text => {
                out.push_str(&unescape_xml(e.as_ref()));
            }
            Ok(Event::GeneralRef(e)) if in_text => {
                push_entity(&mut out, e.as_ref());
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(format!("Erro ao ler slide PPTX: {e}")),
            _ => {}
        }
    }
    Ok(out)
}

pub fn extract_epub(bytes: &[u8]) -> Result<String, String> {
    let mut zip = ZipArchive::new(Cursor::new(bytes))
        .map_err(|e| format!("Arquivo EPUB inválido: {e}"))?;
    let opf_path = read_zip_string(&mut zip, "META-INF/container.xml")?
        .and_then(|xml| find_opf_path(&xml));

    let mut chapter_paths = Vec::new();
    if let Some(opf) = opf_path {
        if let Some(opf_xml) = read_zip_string(&mut zip, &opf)? {
            let base_dir = opf.rsplit_once('/').map_or("", |(d, _)| d);
            chapter_paths = parse_opf_spine(&opf_xml, base_dir);
        }
    }
    if chapter_paths.is_empty() {
        for i in 0..zip.len() {
            if let Ok(entry) = zip.by_index(i) {
                if let Ok(name_cow) = entry.name() {
                    let n = name_cow.into_owned();
                    let low = n.to_ascii_lowercase();
                    if low.ends_with(".xhtml") || low.ends_with(".html") || low.ends_with(".htm") {
                        chapter_paths.push(n);
                    }
                }
            }
        }
        chapter_paths.sort();
    }
    if chapter_paths.is_empty() {
        return Err("Arquivo EPUB não contém capítulos legíveis.".into());
    }

    let mut sections = Vec::new();
    for path in chapter_paths {
        let Some(html) = read_zip_string(&mut zip, &path)? else {
            continue;
        };
        let text = html_to_plain_text(&html);
        if !text.trim().is_empty() {
            sections.push(text);
        }
    }
    Ok(sections.join("\n\n"))
}

fn find_opf_path(container_xml: &str) -> Option<String> {
    let mut reader = Reader::from_str(container_xml);
    loop {
        match reader.read_event() {
            Ok(Event::Start(e) | Event::Empty(e))
                if local_name(e.name().as_ref()) == "rootfile" =>
            {
                if let Some(val) = attr_string(&e, "full-path") {
                    return Some(val);
                }
            }
            Ok(Event::Eof) | Err(_) => return None,
            _ => {}
        }
    }
}

fn parse_opf_spine(opf_xml: &str, base_dir: &str) -> Vec<String> {
    let mut reader = Reader::from_str(opf_xml);
    let mut manifest: HashMap<String, String> = HashMap::new();
    let mut spine: Vec<String> = Vec::new();
    loop {
        match reader.read_event() {
            Ok(Event::Start(e) | Event::Empty(e)) => match local_name(e.name().as_ref()) {
                "item" => {
                    if let (Some(id), Some(href)) =
                        (attr_string(&e, "id"), attr_string(&e, "href"))
                    {
                        manifest.insert(id, href);
                    }
                }
                "itemref" => {
                    if let Some(idref) = attr_string(&e, "idref") {
                        spine.push(idref);
                    }
                }
                _ => {}
            },
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    spine
        .into_iter()
        .filter_map(|id| manifest.get(&id).cloned())
        .map(|href| {
            let decoded = percent_decode(&href);
            if base_dir.is_empty() {
                decoded
            } else {
                format!("{base_dir}/{decoded}")
            }
        })
        .collect()
}

fn html_to_plain_text(html: &str) -> String {
    use scraper::{ElementRef, Html, Node};
    let doc = Html::parse_document(html);
    let mut out = String::new();
    fn walk_el(el: ElementRef<'_>, out: &mut String, skip: bool) {
        let tag = el.value().name().to_ascii_lowercase();
        let next_skip = skip || matches!(tag.as_str(), "script" | "style" | "head" | "svg");
        if !next_skip && tag == "br" {
            out.push('\n');
            return;
        }
        let block = matches!(
            tag.as_str(),
            "p" | "div"
                | "h1"
                | "h2"
                | "h3"
                | "h4"
                | "h5"
                | "h6"
                | "li"
                | "tr"
                | "blockquote"
                | "section"
                | "article"
                | "header"
                | "footer"
                | "hr"
        );
        if !next_skip && block && !out.is_empty() && !out.ends_with('\n') {
            out.push('\n');
        }
        for child in el.children() {
            if let Some(child_el) = ElementRef::wrap(child) {
                walk_el(child_el, out, next_skip);
            } else if !next_skip {
                if let Node::Text(t) = child.value() {
                    let mut prev_ws = out.ends_with(' ') || out.ends_with('\n') || out.is_empty();
                    for ch in t.chars() {
                        if ch.is_whitespace() {
                            if !prev_ws {
                                out.push(' ');
                                prev_ws = true;
                            }
                        } else {
                            out.push(ch);
                            prev_ws = false;
                        }
                    }
                }
            }
        }
        if !next_skip && block && !out.ends_with('\n') {
            out.push('\n');
        }
        if !next_skip && matches!(tag.as_str(), "td" | "th") {
            out.push('\t');
        }
    }
    walk_el(doc.root_element(), &mut out, false);
    out
}

/// Legacy `.doc` converter: supports RTF-disguised-as-`.doc`, HTML/XML-disguised-as-`.doc`,
/// and true Microsoft Word 97-2003 OLE2/CFBF binary documents (WordDocument + 1Table/0Table CLX piece table).
pub fn extract_doc(bytes: &[u8]) -> Result<String, String> {
    let trimmed = bytes
        .iter()
        .position(|b| !b.is_ascii_whitespace())
        .map_or(bytes, |i| &bytes[i..]);
    if trimmed.starts_with(b"{\\rtf") {
        return super::rtf::extract_text(bytes);
    }
    if trimmed.starts_with(b"PK\x03\x04") {
        return extract_docx(bytes);
    }
    if trimmed.starts_with(b"<html")
        || trimmed.starts_with(b"<HTML")
        || trimmed.starts_with(b"<!DOCTYPE")
    {
        let text = String::from_utf8_lossy(bytes);
        return Ok(html_to_plain_text(&text));
    }
    if bytes.len() >= 512 && bytes[..8] == [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1] {
        return extract_ole2_word(bytes);
    }
    Err("Arquivo DOC inválido ou em formato binário não suportado.".into())
}

fn extract_ole2_word(bytes: &[u8]) -> Result<String, String> {
    let streams = parse_cfbf_streams(bytes)?;
    let word_doc = streams
        .get("worddocument")
        .ok_or_else(|| "Documento OLE2 não contém fluxo WordDocument.".to_string())?;
    if word_doc.len() < 0x1FC {
        return Err("Fluxo WordDocument truncado.".into());
    }
    let flags = u16::from_le_bytes([word_doc[0x0A], word_doc[0x0B]]);
    if flags & 0x0100 != 0 {
        return Err("Este arquivo Word (.doc) está protegido por senha.".into());
    }
    let table_name = if flags & 0x0200 != 0 {
        "1table"
    } else {
        "0table"
    };
    let table = streams.get(table_name);
    let fc_clx = read_u32(word_doc, 0x01A2).unwrap_or(0) as usize;
    let lcb_clx = read_u32(word_doc, 0x01A6).unwrap_or(0) as usize;

    if let Some(tbl) = table {
        if let Some(clx) = tbl.get(fc_clx..fc_clx.saturating_add(lcb_clx)) {
            if let Some(text) = parse_word_clx(word_doc, clx) {
                if !text.trim().is_empty() {
                    return Ok(text);
                }
            }
        }
    }

    // Fallback: decode main text span from FIB fcMin..fcMac if CLX is absent (Word 6/95).
    let fc_min = read_u32(word_doc, 0x0018).unwrap_or(0) as usize;
    let fc_mac = read_u32(word_doc, 0x001C).unwrap_or(0) as usize;
    if fc_mac > fc_min && fc_mac <= word_doc.len() {
        let (decoded, _, _) = encoding_rs::WINDOWS_1252.decode(&word_doc[fc_min..fc_mac]);
        return Ok(normalize_word_controls(&decoded));
    }
    Err("Não foi possível extrair texto do arquivo Word (.doc).".into())
}

fn parse_word_clx(word_doc: &[u8], clx: &[u8]) -> Option<String> {
    let mut pos = 0usize;
    while pos < clx.len() {
        match clx[pos] {
            0x01 => {
                if pos + 3 > clx.len() {
                    return None;
                }
                let cb = u16::from_le_bytes([clx[pos + 1], clx[pos + 2]]) as usize;
                pos = pos.checked_add(3 + cb)?;
            }
            0x02 => {
                if pos + 5 > clx.len() {
                    return None;
                }
                let lcb = read_u32(clx, pos + 1)? as usize;
                let plc = clx.get(pos + 5..pos + 5 + lcb)?;
                if plc.len() < 12 || (plc.len() - 4) % 12 != 0 {
                    return None;
                }
                let n = (plc.len() - 4) / 12;
                let mut out = String::new();
                for i in 0..n {
                    let cp_start = read_u32(plc, i * 4)? as usize;
                    let cp_end = read_u32(plc, (i + 1) * 4)? as usize;
                    if cp_end <= cp_start {
                        continue;
                    }
                    let chars = cp_end - cp_start;
                    let pcd_off = (n + 1) * 4 + i * 8;
                    let fc_raw = read_u32(plc, pcd_off + 2)?;
                    let compressed = (fc_raw & 0x4000_0000) != 0;
                    let fc = (fc_raw & 0x3FFF_FFFF) as usize;
                    if compressed {
                        let byte_off = fc / 2;
                        if let Some(slice) = word_doc.get(byte_off..byte_off.saturating_add(chars))
                        {
                            let (s, _, _) = encoding_rs::WINDOWS_1252.decode(slice);
                            out.push_str(&s);
                        }
                    } else {
                        let byte_len = chars.saturating_mul(2);
                        if let Some(slice) = word_doc.get(fc..fc.saturating_add(byte_len)) {
                            let units: Vec<u16> = slice
                                .as_chunks::<2>()
                                .0
                                .iter()
                                .map(|c| u16::from_le_bytes(*c))
                                .collect();
                            out.push_str(&String::from_utf16_lossy(&units));
                        }
                    }
                }
                return Some(normalize_word_controls(&out));
            }
            _ => return None,
        }
    }
    None
}

fn normalize_word_controls(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut in_field_code = false;
    for ch in raw.chars() {
        match ch {
            '\u{0013}' => in_field_code = true,
            '\u{0014}' => in_field_code = false,
            '\u{0015}' => in_field_code = false,
            _ if in_field_code => {}
            '\r' | '\u{000B}' | '\u{000C}' => out.push('\n'),
            '\u{0007}' => out.push('\t'),
            '\t' => out.push('\t'),
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    out
}

/// Minimal read-only OLE2 / CFBF Compound File parser returning lowercase stream names -> bytes.
fn parse_cfbf_streams(bytes: &[u8]) -> Result<HashMap<String, Vec<u8>>, String> {
    if bytes.len() < 512 {
        return Err("Cabeçalho OLE2 muito curto.".into());
    }
    let sector_shift = u16::from_le_bytes([bytes[30], bytes[31]]);
    if sector_shift != 9 && sector_shift != 12 {
        return Err("Tamanho de setor OLE2 não suportado.".into());
    }
    let sector_size = 1usize << sector_shift;
    let mini_shift = u16::from_le_bytes([bytes[32], bytes[33]]);
    let mini_size = 1usize << mini_shift.min(9);
    let first_dir_sector = read_u32(bytes, 48).unwrap_or(0xFFFF_FFFE);
    let mini_cutoff = read_u32(bytes, 56).unwrap_or(4096) as usize;
    let first_minifat_sector = read_u32(bytes, 60).unwrap_or(0xFFFF_FFFE);
    let first_difat_sector = read_u32(bytes, 68).unwrap_or(0xFFFF_FFFE);

    let sector_slice = |sec: u32| -> Option<&[u8]> {
        let off = (sec as usize).checked_add(1)?.checked_mul(sector_size)?;
        bytes.get(off..off.checked_add(sector_size)?)
    };

    let mut difat_sectors: Vec<u32> = Vec::new();
    for i in 0..109 {
        let sec = read_u32(bytes, 76 + i * 4).unwrap_or(0xFFFF_FFFF);
        if sec < 0xFFFF_FFFA {
            difat_sectors.push(sec);
        }
    }
    let mut difat_cur = first_difat_sector;
    let mut guard = 0usize;
    while difat_cur < 0xFFFF_FFFA && guard < 4096 {
        guard += 1;
        let Some(slice) = sector_slice(difat_cur) else {
            break;
        };
        let entries = sector_size / 4 - 1;
        for i in 0..entries {
            let sec = read_u32(slice, i * 4).unwrap_or(0xFFFF_FFFF);
            if sec < 0xFFFF_FFFA {
                difat_sectors.push(sec);
            }
        }
        difat_cur = read_u32(slice, entries * 4).unwrap_or(0xFFFF_FFFE);
    }

    let mut fat: Vec<u32> = Vec::with_capacity(difat_sectors.len() * (sector_size / 4));
    for sec in difat_sectors {
        if let Some(slice) = sector_slice(sec) {
            for chunk in slice.as_chunks::<4>().0 {
                fat.push(u32::from_le_bytes(*chunk));
            }
        }
    }

    let read_chain = |start: u32, max_len: usize| -> Vec<u8> {
        let mut out = Vec::new();
        let mut cur = start;
        let mut steps = 0usize;
        while cur < 0xFFFF_FFFA && steps < 131_072 && out.len() < max_len {
            steps += 1;
            let Some(slice) = sector_slice(cur) else {
                break;
            };
            out.extend_from_slice(slice);
            cur = fat.get(cur as usize).copied().unwrap_or(0xFFFF_FFFE);
        }
        if out.len() > max_len {
            out.truncate(max_len);
        }
        out
    };

    let dir_bytes = read_chain(first_dir_sector, 16 * 1024 * 1024);
    let minifat_bytes = read_chain(first_minifat_sector, 8 * 1024 * 1024);
    let minifat: Vec<u32> = minifat_bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|c| u32::from_le_bytes(*c))
        .collect();

    let mut mini_stream: Vec<u8> = Vec::new();
    let mut entries = Vec::new();
    for entry in dir_bytes.as_chunks::<128>().0 {
        let obj_type = entry[66];
        let name_len = (u16::from_le_bytes([entry[64], entry[65]]) as usize).min(64);
        if name_len < 2 {
            continue;
        }
        let units: Vec<u16> = entry[..name_len - 2]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_le_bytes(*c))
            .collect();
        let name = String::from_utf16_lossy(&units).to_ascii_lowercase();
        let start_sec = read_u32(entry, 116).unwrap_or(0xFFFF_FFFE);
        let size = read_u32(entry, 120).unwrap_or(0) as usize;
        if obj_type == 5 {
            mini_stream = read_chain(start_sec, size.min(64 * 1024 * 1024));
        } else if obj_type == 2 {
            entries.push((name, start_sec, size));
        }
    }

    let mut result = HashMap::new();
    for (name, start_sec, size) in entries {
        if size == 0 || size > 64 * 1024 * 1024 {
            continue;
        }
        let data = if size < mini_cutoff && !mini_stream.is_empty() {
            let mut out = Vec::with_capacity(size);
            let mut cur = start_sec;
            let mut steps = 0usize;
            while cur < 0xFFFF_FFFA && steps < 131_072 && out.len() < size {
                steps += 1;
                let off = (cur as usize).saturating_mul(mini_size);
                let Some(slice) = mini_stream.get(off..off.saturating_add(mini_size)) else {
                    break;
                };
                out.extend_from_slice(slice);
                cur = minifat.get(cur as usize).copied().unwrap_or(0xFFFF_FFFE);
            }
            out.truncate(size);
            out
        } else {
            read_chain(start_sec, size)
        };
        result.insert(name, data);
    }
    Ok(result)
}

fn read_u32(bytes: &[u8], off: usize) -> Option<u32> {
    let slice = bytes.get(off..off + 4)?;
    Some(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

fn read_zip_string(zip: &mut ZipArchive<Cursor<&[u8]>>, name: &str) -> Result<Option<String>, String> {
    match zip.by_name(name) {
        Ok(mut file) => {
            let mut buf = Vec::new();
            file.read_to_end(&mut buf)
                .map_err(|e| format!("Erro ao extrair {name}: {e}"))?;
            Ok(Some(String::from_utf8_lossy(&buf).into_owned()))
        }
        Err(zip::result::ZipError::FileNotFound) => Ok(None),
        Err(e) => Err(format!("Erro ao abrir {name}: {e}")),
    }
}

fn local_name(full: &str) -> &str {
    match full.rfind(':') {
        Some(pos) => &full[pos + 1..],
        None => full,
    }
}

fn attr_string(e: &quick_xml::events::BytesStart<'_>, name: &str) -> Option<String> {
    for attr in e.attributes().flatten() {
        if local_name(attr.key.as_ref()) == name {
            return Some(unescape_xml(attr.value.as_ref()));
        }
    }
    None
}

fn attr_usize(e: &quick_xml::events::BytesStart<'_>, name: &str) -> Option<usize> {
    attr_string(e, name)?.parse().ok()
}

fn unescape_xml(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '&' {
            let mut entity = String::new();
            let mut closed = false;
            for ec in chars.by_ref() {
                if ec == ';' {
                    closed = true;
                    break;
                }
                entity.push(ec);
                if entity.len() > 12 {
                    break;
                }
            }
            if closed {
                push_entity(&mut out, &entity);
            } else {
                out.push('&');
                out.push_str(&entity);
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn push_entity(out: &mut String, name: &str) {
    match name {
        "amp" => out.push('&'),
        "lt" => out.push('<'),
        "gt" => out.push('>'),
        "quot" => out.push('"'),
        "apos" => out.push('\''),
        "nbsp" => out.push(' '),
        s if s.starts_with("#x") || s.starts_with("#X") => {
            if let Ok(code) = u32::from_str_radix(&s[2..], 16) {
                if let Some(ch) = char::from_u32(code) {
                    out.push(ch);
                }
            }
        }
        s if s.starts_with('#') => {
            if let Ok(code) = s[1..].parse::<u32>() {
                if let Some(ch) = char::from_u32(code) {
                    out.push(ch);
                }
            }
        }
        _ => {}
    }
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(v) = u8::from_str_radix(
                std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or(""),
                16,
            ) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}
