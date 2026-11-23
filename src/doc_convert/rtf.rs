//! Pure-Rust Rich Text Format (RTF) to plain text converter.

use encoding_rs::Encoding as RsEncoding;

#[derive(Clone, Copy)]
struct GroupState {
    uc: usize,
    skip: bool,
}

pub fn extract_text(bytes: &[u8]) -> Result<String, String> {
    let trimmed = bytes
        .iter()
        .position(|b| !b.is_ascii_whitespace())
        .map_or(bytes, |i| &bytes[i..]);
    if !trimmed.starts_with(b"{\\rtf") {
        return Err("Arquivo RTF inválido (cabeçalho {\\rtf ausente).".into());
    }

    let mut encoding: &'static RsEncoding = encoding_rs::WINDOWS_1252;
    let mut stack = vec![GroupState { uc: 1, skip: false }];
    let mut out = String::with_capacity(bytes.len() / 2);
    let mut hex_buf: Vec<u8> = Vec::new();
    let mut i = 0usize;
    let len = trimmed.len();
    let mut ignorable_next = false;

    let flush_hex = |buf: &mut Vec<u8>, enc: &'static RsEncoding, out: &mut String| {
        if !buf.is_empty() {
            let (decoded, _, _) = enc.decode(buf);
            out.push_str(&decoded);
            buf.clear();
        }
    };

    while i < len {
        let state = *stack.last().unwrap_or(&GroupState { uc: 1, skip: false });
        let b = trimmed[i];
        match b {
            b'{' => {
                flush_hex(&mut hex_buf, encoding, &mut out);
                stack.push(state);
                ignorable_next = false;
                i += 1;
            }
            b'}' => {
                flush_hex(&mut hex_buf, encoding, &mut out);
                if stack.len() > 1 {
                    stack.pop();
                }
                ignorable_next = false;
                i += 1;
            }
            b'\r' | b'\n' => {
                i += 1;
            }
            b'\\' => {
                i += 1;
                if i >= len {
                    break;
                }
                let next = trimmed[i];
                if next == b'\'' {
                    i += 1;
                    if i + 1 < len {
                        if let (Some(hi), Some(lo)) = (hex_val(trimmed[i]), hex_val(trimmed[i + 1]))
                        {
                            if !state.skip {
                                hex_buf.push((hi << 4) | lo);
                            }
                            i += 2;
                        }
                    }
                    continue;
                }
                flush_hex(&mut hex_buf, encoding, &mut out);
                if next == b'*' {
                    ignorable_next = true;
                    if let Some(top) = stack.last_mut() {
                        top.skip = true;
                    }
                    i += 1;
                    continue;
                }
                if !next.is_ascii_alphabetic() {
                    i += 1;
                    if !state.skip {
                        match next {
                            b'\\' | b'{' | b'}' => out.push(next as char),
                            b'~' => out.push(' '),
                            b'_' => out.push('-'),
                            b'\r' | b'\n' => out.push('\n'),
                            _ => {}
                        }
                    }
                    continue;
                }

                let word_start = i;
                while i < len && trimmed[i].is_ascii_alphabetic() {
                    i += 1;
                }
                let word = std::str::from_utf8(&trimmed[word_start..i]).unwrap_or("");

                let mut neg = false;
                let mut has_param = false;
                let mut param: i32 = 0;
                if i < len && trimmed[i] == b'-' {
                    neg = true;
                    i += 1;
                }
                let num_start = i;
                while i < len && trimmed[i].is_ascii_digit() {
                    param = param
                        .saturating_mul(10)
                        .saturating_add((trimmed[i] - b'0') as i32);
                    i += 1;
                }
                if i > num_start {
                    has_param = true;
                    if neg {
                        param = -param;
                    }
                }
                if i < len && trimmed[i] == b' ' {
                    i += 1;
                }

                if word == "ansicpg" && has_param {
                    encoding = codepage_to_encoding(param);
                    continue;
                }
                if word == "uc" && has_param && param >= 0 {
                    if let Some(top) = stack.last_mut() {
                        top.uc = (param as usize).min(16);
                    }
                    continue;
                }
                if is_skip_destination(word) || ignorable_next {
                    if let Some(top) = stack.last_mut() {
                        top.skip = true;
                    }
                    ignorable_next = false;
                    continue;
                }
                if state.skip {
                    continue;
                }

                match word {
                    "par" | "line" | "row" | "sect" | "page" => out.push('\n'),
                    "tab" | "cell" => out.push('\t'),
                    "emdash" => out.push('\u{2014}'),
                    "endash" => out.push('\u{2013}'),
                    "lquote" => out.push('\u{2018}'),
                    "rquote" => out.push('\u{2019}'),
                    "ldblquote" => out.push('\u{201C}'),
                    "rdblquote" => out.push('\u{201D}'),
                    "bullet" => out.push('\u{2022}'),
                    "u" if has_param => {
                        let code = if param < 0 {
                            (param + 65536) as u32
                        } else {
                            param as u32
                        };
                        if let Some(ch) = char::from_u32(code) {
                            out.push(ch);
                        }
                        // Skip the next `uc` ANSI fallback representations.
                        let mut skipped = 0usize;
                        while skipped < state.uc && i < len {
                            if trimmed[i] == b'\\' && i + 3 < len && trimmed[i + 1] == b'\'' {
                                i += 4;
                                skipped += 1;
                            } else if trimmed[i] == b'{' || trimmed[i] == b'}' {
                                break;
                            } else {
                                i += 1;
                                skipped += 1;
                            }
                        }
                    }
                    _ => {}
                }
            }
            _ => {
                if !state.skip {
                    if b < 0x80 {
                        flush_hex(&mut hex_buf, encoding, &mut out);
                        out.push(b as char);
                    } else {
                        hex_buf.push(b);
                    }
                }
                i += 1;
            }
        }
    }
    flush_hex(&mut hex_buf, encoding, &mut out);
    Ok(out)
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn codepage_to_encoding(cp: i32) -> &'static RsEncoding {
    match cp {
        1250 => encoding_rs::WINDOWS_1250,
        1251 => encoding_rs::WINDOWS_1251,
        1252 => encoding_rs::WINDOWS_1252,
        1253 => encoding_rs::WINDOWS_1253,
        1254 => encoding_rs::WINDOWS_1254,
        1255 => encoding_rs::WINDOWS_1255,
        1256 => encoding_rs::WINDOWS_1256,
        1257 => encoding_rs::WINDOWS_1257,
        1258 => encoding_rs::WINDOWS_1258,
        932 => encoding_rs::SHIFT_JIS,
        936 => encoding_rs::GBK,
        949 => encoding_rs::EUC_KR,
        950 => encoding_rs::BIG5,
        874 => encoding_rs::WINDOWS_874,
        65001 => encoding_rs::UTF_8,
        _ => encoding_rs::WINDOWS_1252,
    }
}

fn is_skip_destination(word: &str) -> bool {
    matches!(
        word,
        "fonttbl"
            | "colortbl"
            | "stylesheet"
            | "info"
            | "pict"
            | "object"
            | "objdata"
            | "datafield"
            | "fldinst"
            | "header"
            | "headerl"
            | "headerr"
            | "headerf"
            | "footer"
            | "footerl"
            | "footerr"
            | "footerf"
            | "listtable"
            | "listoverridetable"
            | "revtbl"
            | "rsidtbl"
            | "generator"
            | "themedata"
            | "colorschememapping"
            | "latentstyles"
            | "datastore"
            | "xmlnstbl"
            | "mmathPr"
            | "pnseclvl"
            | "bkmkstart"
            | "bkmkend"
            | "xe"
            | "tc"
    )
}
