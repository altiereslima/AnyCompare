//! Pure-Rust PDF 1.0–2.0 text extractor.
//!
//! Supports classic `xref` tables, PDF 1.5+ `/XRef` cross-reference streams with
//! PNG/TIFF predictors, compressed `/ObjStm` object streams, Flate/ASCIIHex/ASCII85/
//! LZW/RunLength stream filters, standard empty-user-password RC4 decryption,
//! `/ToUnicode` CMaps (1-byte, 2-byte, ligatures, surrogate pairs), WinAnsi/MacRoman/
//! Standard/PDFDoc encodings + `/Differences` + Adobe Glyph List, Form XObjects (`Do`),
//! and full 2D CTM/Tm text-layout reconstruction.

use flate2::read::{DeflateDecoder, ZlibDecoder};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::Read;
use std::rc::Rc;

const MAX_PAGES: usize = 10_000;
const MAX_FORM_DEPTH: usize = 8;
const MAX_DECOMPRESSED_STREAM: usize = 64 * 1024 * 1024;

pub fn extract_text(bytes: &[u8]) -> Result<String, String> {
    if !bytes.windows(5).take(1024).any(|w| w == b"%PDF-") {
        return Err("Arquivo PDF inválido (assinatura %PDF- não encontrada).".into());
    }
    let mut doc = PdfDoc::parse(bytes)?;
    doc.extract_all_text()
}

// ============================================================================
// PDF Value Model
// ============================================================================

#[derive(Clone, Debug)]
enum PdfVal {
    Null,
    Bool,
    Int(i64),
    Real(f64),
    Name(String),
    Str(Vec<u8>),
    Array(Vec<PdfVal>),
    Dict(BTreeMap<String, PdfVal>),
    Stream {
        dict: BTreeMap<String, PdfVal>,
        data: Vec<u8>,
    },
    Ref(u32, u16),
}

impl PdfVal {
    fn as_i64(&self) -> Option<i64> {
        match *self {
            Self::Int(v) => Some(v),
            Self::Real(v) => Some(v as i64),
            _ => None,
        }
    }

    fn as_f64(&self) -> Option<f64> {
        match *self {
            Self::Int(v) => Some(v as f64),
            Self::Real(v) => Some(v),
            _ => None,
        }
    }

    fn as_name(&self) -> Option<&str> {
        match self {
            Self::Name(s) => Some(s.as_str()),
            _ => None,
        }
    }

    fn as_str_bytes(&self) -> Option<&[u8]> {
        match self {
            Self::Str(b) => Some(b.as_slice()),
            _ => None,
        }
    }

    fn as_array(&self) -> Option<&[PdfVal]> {
        match self {
            Self::Array(a) => Some(a.as_slice()),
            _ => None,
        }
    }

    fn as_dict(&self) -> Option<&BTreeMap<String, PdfVal>> {
        match self {
            Self::Dict(d) | Self::Stream { dict: d, .. } => Some(d),
            _ => None,
        }
    }

    fn as_ref(&self) -> Option<(u32, u16)> {
        match *self {
            Self::Ref(o, g) => Some((o, g)),
            _ => None,
        }
    }
}

// ============================================================================
// Lexer & Object Parser
// ============================================================================

#[derive(Clone)]
struct Lexer<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Lexer<'a> {
    fn new(data: &'a [u8], pos: usize) -> Self {
        Self { data, pos }
    }

    fn skip_ws(&mut self) {
        while self.pos < self.data.len() {
            let b = self.data[self.pos];
            if is_pdf_ws(b) {
                self.pos += 1;
            } else if b == b'%' {
                while self.pos < self.data.len()
                    && self.data[self.pos] != b'\n'
                    && self.data[self.pos] != b'\r'
                {
                    self.pos += 1;
                }
            } else {
                break;
            }
        }
    }

    fn peek(&mut self) -> Option<u8> {
        self.skip_ws();
        self.data.get(self.pos).copied()
    }

    fn parse_val(&mut self) -> Option<PdfVal> {
        self.parse_val_depth(0)
    }

    fn parse_val_depth(&mut self, depth: usize) -> Option<PdfVal> {
        if depth > 48 {
            return None;
        }
        let b = self.peek()?;
        match b {
            b'<' if self.data.get(self.pos + 1) == Some(&b'<') => {
                self.pos += 2;
                let mut dict = BTreeMap::new();
                loop {
                    self.skip_ws();
                    if self.pos + 1 < self.data.len()
                        && self.data[self.pos] == b'>'
                        && self.data[self.pos + 1] == b'>'
                    {
                        self.pos += 2;
                        break;
                    }
                    if self.pos >= self.data.len() {
                        break;
                    }
                    let key = match self.parse_val_depth(depth + 1) {
                        Some(PdfVal::Name(k)) => k,
                        Some(_) => continue,
                        None => break,
                    };
                    let val = self.parse_val_depth(depth + 1).unwrap_or(PdfVal::Null);
                    dict.insert(key, val);
                }
                Some(PdfVal::Dict(dict))
            }
            b'<' => Some(PdfVal::Str(self.parse_hex_string())),
            b'(' => Some(PdfVal::Str(self.parse_literal_string())),
            b'[' => {
                self.pos += 1;
                let mut arr = Vec::new();
                loop {
                    self.skip_ws();
                    if self.pos >= self.data.len() {
                        break;
                    }
                    if self.data[self.pos] == b']' {
                        self.pos += 1;
                        break;
                    }
                    match self.parse_val_depth(depth + 1) {
                        Some(v) => arr.push(v),
                        None => break,
                    }
                }
                Some(PdfVal::Array(arr))
            }
            b'/' => Some(PdfVal::Name(self.parse_name())),
            b']' | b'>' => None,
            _ => {
                let start = self.pos;
                let tok = self.read_token();
                match tok {
                    b"true" | b"false" => Some(PdfVal::Bool),
                    b"null" => Some(PdfVal::Null),
                    _ => {
                        if let Some(n) = parse_int(tok) {
                            // Check for indirect reference: `int int R`
                            let saved = self.pos;
                            let next1 = self.read_token_ws();
                            if let Some(gen) = parse_int(next1) {
                                let next2 = self.read_token_ws();
                                if next2 == b"R" && n >= 0 && gen >= 0 {
                                    return Some(PdfVal::Ref(n as u32, gen as u16));
                                }
                            }
                            self.pos = saved;
                            return Some(PdfVal::Int(n));
                        }
                        if let Some(f) = parse_real(tok) {
                            return Some(PdfVal::Real(f));
                        }
                        self.pos = start + tok.len().max(1);
                        Some(PdfVal::Name(String::from_utf8_lossy(tok).into_owned()))
                    }
                }
            }
        }
    }

    fn read_token_ws(&mut self) -> &'a [u8] {
        self.skip_ws();
        self.read_token()
    }

    fn read_token(&mut self) -> &'a [u8] {
        let start = self.pos;
        while self.pos < self.data.len() {
            let b = self.data[self.pos];
            if is_pdf_ws(b) || is_pdf_delim(b) {
                break;
            }
            self.pos += 1;
        }
        &self.data[start..self.pos]
    }

    fn parse_name(&mut self) -> String {
        self.pos += 1; // skip '/'
        let mut out = Vec::new();
        while self.pos < self.data.len() {
            let b = self.data[self.pos];
            if is_pdf_ws(b) || is_pdf_delim(b) {
                break;
            }
            if b == b'#' && self.pos + 2 < self.data.len() {
                if let (Some(h), Some(l)) = (
                    hex_digit(self.data[self.pos + 1]),
                    hex_digit(self.data[self.pos + 2]),
                ) {
                    out.push((h << 4) | l);
                    self.pos += 3;
                    continue;
                }
            }
            out.push(b);
            self.pos += 1;
        }
        String::from_utf8_lossy(&out).into_owned()
    }

    fn parse_hex_string(&mut self) -> Vec<u8> {
        self.pos += 1; // skip '<'
        let mut out = Vec::new();
        let mut hi: Option<u8> = None;
        while self.pos < self.data.len() {
            let b = self.data[self.pos];
            self.pos += 1;
            if b == b'>' {
                break;
            }
            if let Some(d) = hex_digit(b) {
                if let Some(h) = hi.take() {
                    out.push((h << 4) | d);
                } else {
                    hi = Some(d);
                }
            }
        }
        if let Some(h) = hi {
            out.push(h << 4);
        }
        out
    }

    fn parse_literal_string(&mut self) -> Vec<u8> {
        self.pos += 1; // skip '('
        let mut out = Vec::new();
        let mut parens = 1usize;
        while self.pos < self.data.len() && parens > 0 {
            let b = self.data[self.pos];
            self.pos += 1;
            match b {
                b'(' => {
                    parens += 1;
                    out.push(b'(');
                }
                b')' => {
                    parens -= 1;
                    if parens > 0 {
                        out.push(b')');
                    }
                }
                b'\\' if self.pos < self.data.len() => {
                    let esc = self.data[self.pos];
                    self.pos += 1;
                    match esc {
                        b'n' => out.push(b'\n'),
                        b'r' => out.push(b'\r'),
                        b't' => out.push(b'\t'),
                        b'b' => out.push(0x08),
                        b'f' => out.push(0x0C),
                        b'(' | b')' | b'\\' => out.push(esc),
                        b'\r' => {
                            if self.data.get(self.pos) == Some(&b'\n') {
                                self.pos += 1;
                            }
                        }
                        b'\n' => {}
                        b'0'..=b'7' => {
                            let mut val = (esc - b'0') as u32;
                            for _ in 0..2 {
                                if let Some(&d) = self.data.get(self.pos) {
                                    if (b'0'..=b'7').contains(&d) {
                                        val = (val << 3) | ((d - b'0') as u32);
                                        self.pos += 1;
                                        continue;
                                    }
                                }
                                break;
                            }
                            out.push((val & 0xFF) as u8);
                        }
                        other => out.push(other),
                    }
                }
                b'\r' => {
                    if self.data.get(self.pos) == Some(&b'\n') {
                        self.pos += 1;
                    }
                    out.push(b'\n');
                }
                other => out.push(other),
            }
        }
        out
    }
}

fn is_pdf_ws(b: u8) -> bool {
    matches!(b, 0 | 9 | 10 | 12 | 13 | 32)
}

fn is_pdf_delim(b: u8) -> bool {
    matches!(
        b,
        b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%'
    )
}

fn hex_digit(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn parse_int(tok: &[u8]) -> Option<i64> {
    if tok.is_empty() {
        return None;
    }
    std::str::from_utf8(tok).ok()?.parse::<i64>().ok()
}

fn parse_real(tok: &[u8]) -> Option<f64> {
    if tok.is_empty() || !tok.iter().any(|b| b.is_ascii_digit()) {
        return None;
    }
    std::str::from_utf8(tok).ok()?.parse::<f64>().ok()
}

// ============================================================================
// XRef, Object Streams, Encryption, and Stream Filters
// ============================================================================

#[derive(Clone, Copy, Debug)]
enum XRefEntry {
    Offset(usize),
    Compressed { obj_stm: u32 },
}

#[derive(Clone)]
struct Rc4Crypt {
    file_key: Vec<u8>,
}

impl Rc4Crypt {
    fn decrypt(&self, obj_num: u32, gen_num: u16, data: &[u8]) -> Vec<u8> {
        let mut input = Vec::with_capacity(self.file_key.len() + 5);
        input.extend_from_slice(&self.file_key);
        input.extend_from_slice(&obj_num.to_le_bytes()[..3]);
        input.extend_from_slice(&gen_num.to_le_bytes());
        let hash = md5(&input);
        let key_len = (self.file_key.len() + 5).min(16);
        rc4(&hash[..key_len], data)
    }
}

struct PdfDoc<'a> {
    data: &'a [u8],
    xref: HashMap<u32, XRefEntry>,
    trailer: BTreeMap<String, PdfVal>,
    objstm_cache: HashMap<u32, HashMap<u32, PdfVal>>,
    obj_cache: HashMap<u32, PdfVal>,
    font_cache: HashMap<u32, Rc<PdfFont>>,
    crypt: Option<Rc4Crypt>,
}

impl<'a> PdfDoc<'a> {
    fn parse(data: &'a [u8]) -> Result<Self, String> {
        let mut doc = Self {
            data,
            xref: HashMap::new(),
            trailer: BTreeMap::new(),
            objstm_cache: HashMap::new(),
            obj_cache: HashMap::new(),
            font_cache: HashMap::new(),
            crypt: None,
        };

        let mut loaded_xref = false;
        let mut visited_xrefs = HashSet::new();
        for start in find_startxref_offsets(data) {
            let mut next = Some(start);
            while let Some(off) = next {
                if !visited_xrefs.insert(off) || visited_xrefs.len() > 64 {
                    break;
                }
                if let Some(prev) = doc.parse_xref_at(off) {
                    loaded_xref = true;
                    next = prev;
                } else {
                    break;
                }
            }
            if loaded_xref && doc.trailer.contains_key("Root") {
                break;
            }
        }

        if !loaded_xref || !doc.trailer.contains_key("Root") {
            doc.scan_objects_fallback();
        }

        // Handle /Encrypt if present (support standard RC4 with empty user password).
        if let Some(enc_val) = doc.trailer.get("Encrypt").cloned() {
            let enc_obj = doc.resolve(&enc_val);
            if let Some(enc_dict) = enc_obj.as_dict() {
                let crypt = build_empty_password_crypt(enc_dict, &doc.trailer)?;
                doc.crypt = Some(crypt);
                doc.obj_cache.clear();
                doc.objstm_cache.clear();
            }
        }

        Ok(doc)
    }

    fn parse_xref_at(&mut self, offset: usize) -> Option<Option<usize>> {
        if offset >= self.data.len() {
            return None;
        }
        let mut lex = Lexer::new(self.data, offset);
        lex.skip_ws();
        if self.data[lex.pos..].starts_with(b"xref") {
            lex.pos += 4;
            loop {
                lex.skip_ws();
                if self.data[lex.pos..].starts_with(b"trailer") {
                    lex.pos += 7;
                    if let Some(PdfVal::Dict(dict)) = lex.parse_val() {
                        let prev = dict
                            .get("Prev")
                            .and_then(PdfVal::as_i64)
                            .filter(|&p| p > 0)
                            .map(|p| p as usize);
                        let xref_stm = dict
                            .get("XRefStm")
                            .and_then(PdfVal::as_i64)
                            .filter(|&p| p > 0)
                            .map(|p| p as usize);
                        for (k, v) in dict {
                            self.trailer.entry(k).or_insert(v);
                        }
                        if let Some(stm_off) = xref_stm {
                            let _ = self.parse_xref_at(stm_off);
                        }
                        return Some(prev);
                    }
                    return Some(None);
                }
                let first = parse_int(lex.read_token_ws())? as u32;
                let count = parse_int(lex.read_token_ws())? as u32;
                for i in 0..count {
                    let off_tok = lex.read_token_ws();
                    let _gen_tok = lex.read_token_ws();
                    let flag_tok = lex.read_token_ws();
                    if flag_tok == b"n" {
                        if let Some(obj_off) = parse_int(off_tok) {
                            if obj_off > 0 {
                                self.xref
                                    .entry(first + i)
                                    .or_insert(XRefEntry::Offset(obj_off as usize));
                            }
                        }
                    }
                }
            }
        } else {
            // PDF 1.5+ Cross-Reference Stream (`N G obj << /Type /XRef ... >> stream ...`)
            let (_obj_num, _gen_num, val) = self.parse_indirect_at(offset, false)?;
            let PdfVal::Stream { dict, data } = val else {
                return None;
            };
            if dict.get("Type").and_then(PdfVal::as_name) != Some("XRef") {
                return None;
            }
            let decoded = decode_stream_bytes(&dict, &data).ok()?;
            let w_arr = dict.get("W").and_then(PdfVal::as_array)?;
            if w_arr.len() != 3 {
                return None;
            }
            let w0 = w_arr[0].as_i64()?.max(0) as usize;
            let w1 = w_arr[1].as_i64()?.max(0) as usize;
            let w2 = w_arr[2].as_i64()?.max(0) as usize;
            let stride = w0 + w1 + w2;
            if stride == 0 {
                return None;
            }
            let size = dict.get("Size").and_then(PdfVal::as_i64).unwrap_or(0).max(0) as u32;
            let mut ranges: Vec<(u32, u32)> = Vec::new();
            if let Some(idx) = dict.get("Index").and_then(PdfVal::as_array) {
                for pair in idx.as_chunks::<2>().0 {
                    if let (Some(a), Some(b)) = (pair[0].as_i64(), pair[1].as_i64()) {
                        if a >= 0 && b > 0 {
                            ranges.push((a as u32, b as u32));
                        }
                    }
                }
            } else {
                ranges.push((0, size));
            }
            let mut cursor = 0usize;
            for (start_obj, count) in ranges {
                for i in 0..count {
                    if cursor + stride > decoded.len() {
                        break;
                    }
                    let f0 = if w0 == 0 {
                        1
                    } else {
                        read_be_uint(&decoded[cursor..cursor + w0])
                    };
                    let f1 = read_be_uint(&decoded[cursor + w0..cursor + w0 + w1]);
                    cursor += stride;
                    let obj_id = start_obj + i;
                    match f0 {
                        1 if f1 > 0 => {
                            self.xref
                                .entry(obj_id)
                                .or_insert(XRefEntry::Offset(f1 as usize));
                        }
                        2 => {
                            self.xref
                                .entry(obj_id)
                                .or_insert(XRefEntry::Compressed { obj_stm: f1 as u32 });
                        }
                        _ => {}
                    }
                }
            }
            let prev = dict
                .get("Prev")
                .and_then(PdfVal::as_i64)
                .filter(|&p| p > 0)
                .map(|p| p as usize);
            for (k, v) in dict {
                self.trailer.entry(k).or_insert(v);
            }
            Some(prev)
        }
    }

    fn scan_objects_fallback(&mut self) {
        let mut pos = 0usize;
        let len = self.data.len();
        while pos < len {
            if self.data[pos].is_ascii_digit()
                && (pos == 0 || is_pdf_ws(self.data[pos - 1]))
            {
                let mut lex = Lexer::new(self.data, pos);
                let n = parse_int(lex.read_token());
                let g = parse_int(lex.read_token_ws());
                let kw = lex.read_token_ws();
                if kw == b"obj" {
                    if let (Some(obj_num), Some(_)) = (n, g) {
                        if obj_num > 0 {
                            self.xref
                                .entry(obj_num as u32)
                                .or_insert(XRefEntry::Offset(pos));
                        }
                    }
                }
            } else if self.data[pos..].starts_with(b"trailer")
                && (pos == 0 || is_pdf_ws(self.data[pos - 1]))
            {
                let mut lex = Lexer::new(self.data, pos + 7);
                if let Some(PdfVal::Dict(dict)) = lex.parse_val() {
                    for (k, v) in dict {
                        self.trailer.entry(k).or_insert(v);
                    }
                }
            }
            pos += 1;
        }
        // If trailer still lacks /Root, locate the /Catalog object directly.
        if !self.trailer.contains_key("Root") {
            let ids: Vec<u32> = self.xref.keys().copied().collect();
            for id in ids {
                if let Some(PdfVal::Dict(d)) = self.get_obj(id) {
                    if d.get("Type").and_then(PdfVal::as_name) == Some("Catalog") {
                        self.trailer.insert("Root".into(), PdfVal::Ref(id, 0));
                        break;
                    }
                }
            }
        }
    }

    fn parse_indirect_at(&self, offset: usize, allow_decrypt: bool) -> Option<(u32, u16, PdfVal)> {
        let mut lex = Lexer::new(self.data, offset);
        let obj_num = parse_int(lex.read_token_ws())? as u32;
        let gen_num = parse_int(lex.read_token_ws())? as u16;
        if lex.read_token_ws() != b"obj" {
            return None;
        }
        let mut val = lex.parse_val()?;
        if lex.read_token_ws() == b"stream" {
            // Stream data starts right after \r\n or \n.
            if self.data.get(lex.pos) == Some(&b'\r') {
                lex.pos += 1;
            }
            if self.data.get(lex.pos) == Some(&b'\n') {
                lex.pos += 1;
            }
            let stream_start = lex.pos;
            let dict = match val {
                PdfVal::Dict(d) => d,
                _ => BTreeMap::new(),
            };
            let length = dict
                .get("Length")
                .and_then(|v| match v {
                    PdfVal::Int(n) if *n >= 0 => Some(*n as usize),
                    PdfVal::Ref(o, _) => self.resolve_int_no_cache(*o),
                    _ => None,
                });
            let stream_end = if let Some(l) = length {
                if stream_start + l <= self.data.len() {
                    stream_start + l
                } else {
                    find_endstream(self.data, stream_start)
                }
            } else {
                find_endstream(self.data, stream_start)
            };
            let mut raw = self.data[stream_start..stream_end].to_vec();
            if allow_decrypt {
                if let Some(crypt) = &self.crypt {
                    raw = crypt.decrypt(obj_num, gen_num, &raw);
                }
            }
            val = PdfVal::Stream { dict, data: raw };
        } else if allow_decrypt {
            if let Some(crypt) = &self.crypt {
                decrypt_val(&mut val, crypt, obj_num, gen_num);
            }
        }
        Some((obj_num, gen_num, val))
    }

    fn resolve_int_no_cache(&self, obj_num: u32) -> Option<usize> {
        let XRefEntry::Offset(off) = self.xref.get(&obj_num).copied()? else {
            return None;
        };
        let mut lex = Lexer::new(self.data, off);
        let _ = lex.read_token_ws();
        let _ = lex.read_token_ws();
        if lex.read_token_ws() != b"obj" {
            return None;
        }
        lex.parse_val()?.as_i64().filter(|&n| n >= 0).map(|n| n as usize)
    }

    fn get_obj(&mut self, obj_num: u32) -> Option<PdfVal> {
        if let Some(cached) = self.obj_cache.get(&obj_num) {
            return Some(cached.clone());
        }
        let entry = self.xref.get(&obj_num).copied()?;
        let val = match entry {
            XRefEntry::Offset(off) => self.parse_indirect_at(off, true)?.2,
            XRefEntry::Compressed { obj_stm, .. } => {
                if !self.objstm_cache.contains_key(&obj_stm) {
                    let parsed = self.load_objstm(obj_stm).unwrap_or_default();
                    self.objstm_cache.insert(obj_stm, parsed);
                }
                self.objstm_cache
                    .get(&obj_stm)
                    .and_then(|m| m.get(&obj_num))
                    .cloned()?
            }
        };
        self.obj_cache.insert(obj_num, val.clone());
        Some(val)
    }

    fn load_objstm(&mut self, obj_stm: u32) -> Option<HashMap<u32, PdfVal>> {
        let XRefEntry::Offset(off) = self.xref.get(&obj_stm).copied()? else {
            return None;
        };
        let (_, _, PdfVal::Stream { dict, data }) = self.parse_indirect_at(off, true)? else {
            return None;
        };
        let n = dict.get("N").and_then(PdfVal::as_i64)?.max(0) as usize;
        let first = dict.get("First").and_then(PdfVal::as_i64)?.max(0) as usize;
        let decoded = decode_stream_bytes(&dict, &data).ok()?;
        let mut header_lex = Lexer::new(&decoded, 0);
        let mut pairs = Vec::with_capacity(n.min(4096));
        for _ in 0..n {
            let num = parse_int(header_lex.read_token_ws())? as u32;
            let rel_off = parse_int(header_lex.read_token_ws())? as usize;
            pairs.push((num, rel_off));
        }
        let mut map = HashMap::with_capacity(pairs.len());
        for (num, rel_off) in pairs {
            let start = first.saturating_add(rel_off);
            if start < decoded.len() {
                let mut val_lex = Lexer::new(&decoded, start);
                if let Some(val) = val_lex.parse_val() {
                    map.insert(num, val);
                }
            }
        }
        Some(map)
    }

    fn resolve(&mut self, val: &PdfVal) -> PdfVal {
        let mut cur = val.clone();
        for _ in 0..8 {
            if let PdfVal::Ref(num, _) = cur {
                if let Some(next) = self.get_obj(num) {
                    cur = next;
                } else {
                    return PdfVal::Null;
                }
            } else {
                break;
            }
        }
        cur
    }

    fn resolve_stream_bytes(&mut self, val: &PdfVal) -> Option<Vec<u8>> {
        let obj = self.resolve(val);
        let PdfVal::Stream { dict, data } = obj else {
            return None;
        };
        decode_stream_bytes(&dict, &data).ok()
    }

    // ========================================================================
    // Page Collection & Text Extraction
    // ========================================================================

    fn extract_all_text(&mut self) -> Result<String, String> {
        let root_ref = self
            .trailer
            .get("Root")
            .cloned()
            .ok_or_else(|| "Catálogo PDF (/Root) não encontrado.".to_string())?;
        let root = self.resolve(&root_ref);
        let pages_ref = root
            .as_dict()
            .and_then(|d| d.get("Pages"))
            .cloned()
            .ok_or_else(|| "Árvore de páginas PDF (/Pages) não encontrada.".to_string())?;

        let mut pages = Vec::new();
        let mut visited = HashSet::new();
        self.collect_pages(&pages_ref, &BTreeMap::new(), &mut pages, &mut visited, 0);
        if pages.is_empty() {
            return Err("Nenhuma página encontrada no arquivo PDF.".into());
        }

        let mut page_texts = Vec::with_capacity(pages.len());
        for page in pages {
            let text = self.extract_page_text(&page);
            if !text.trim().is_empty() {
                page_texts.push(text);
            }
        }
        if page_texts.is_empty() {
            return Ok(String::from(
                "[Este PDF não contém texto selecionável — possivelmente digitalizado como imagem.]",
            ));
        }
        Ok(page_texts.join("\n\n"))
    }

    fn collect_pages(
        &mut self,
        node_ref: &PdfVal,
        inherited_resources: &BTreeMap<String, PdfVal>,
        out: &mut Vec<PageInfo>,
        visited: &mut HashSet<u32>,
        depth: usize,
    ) {
        if depth > 32 || out.len() >= MAX_PAGES {
            return;
        }
        if let Some((id, _)) = node_ref.as_ref() {
            if !visited.insert(id) {
                return;
            }
        }
        let node = self.resolve(node_ref);
        let Some(dict) = node.as_dict() else {
            return;
        };
        let mut merged_res = inherited_resources.clone();
        if let Some(res_val) = dict.get("Resources") {
            let res_obj = self.resolve(res_val);
            if let Some(res_dict) = res_obj.as_dict() {
                for (k, v) in res_dict {
                    merged_res.insert(k.clone(), v.clone());
                }
            }
        }
        let node_type = dict.get("Type").and_then(PdfVal::as_name);
        if node_type == Some("Pages") || dict.contains_key("Kids") {
            if let Some(kids_val) = dict.get("Kids").cloned() {
                let kids_obj = self.resolve(&kids_val);
                if let Some(kids) = kids_obj.as_array() {
                    for kid in kids {
                        self.collect_pages(kid, &merged_res, out, visited, depth + 1);
                    }
                }
            }
        } else {
            out.push(PageInfo {
                contents: dict.get("Contents").cloned(),
                resources: merged_res,
            });
        }
    }

    fn extract_page_text(&mut self, page: &PageInfo) -> String {
        let Some(contents_val) = &page.contents else {
            return String::new();
        };
        let mut stream_bytes = Vec::new();
        let resolved = self.resolve(contents_val);
        match resolved {
            PdfVal::Stream { dict, data } => {
                if let Ok(dec) = decode_stream_bytes(&dict, &data) {
                    stream_bytes = dec;
                }
            }
            PdfVal::Array(arr) => {
                for item in arr {
                    if let Some(part) = self.resolve_stream_bytes(&item) {
                        if !stream_bytes.is_empty() {
                            stream_bytes.push(b'\n');
                        }
                        stream_bytes.extend_from_slice(&part);
                    }
                }
            }
            _ => {}
        }
        if stream_bytes.is_empty() {
            return String::new();
        }
        let mut chunks = Vec::new();
        let mut seq = 0usize;
        self.interpret_content_stream(
            &stream_bytes,
            &page.resources,
            Matrix2D::IDENTITY,
            &mut chunks,
            &mut seq,
            0,
        );
        assemble_page_lines(chunks)
    }

    fn load_resource_fonts(
        &mut self,
        resources: &BTreeMap<String, PdfVal>,
    ) -> HashMap<String, Rc<PdfFont>> {
        let mut map = HashMap::new();
        let Some(font_val) = resources.get("Font") else {
            return map;
        };
        let font_obj = self.resolve(font_val);
        let Some(font_dict) = font_obj.as_dict() else {
            return map;
        };
        for (name, val) in font_dict.clone() {
            if let Some(font) = self.load_font(&val) {
                map.insert(name, font);
            }
        }
        map
    }

    fn load_font(&mut self, val: &PdfVal) -> Option<Rc<PdfFont>> {
        let cache_key = val.as_ref().map(|(id, _)| id);
        if let Some(id) = cache_key {
            if let Some(cached) = self.font_cache.get(&id) {
                return Some(Rc::clone(cached));
            }
        }
        let obj = self.resolve(val);
        let dict = obj.as_dict()?.clone();
        let subtype = dict.get("Subtype").and_then(PdfVal::as_name).unwrap_or("");
        let is_type0 = subtype == "Type0";

        let to_unicode = dict
            .get("ToUnicode")
            .and_then(|u| self.resolve_stream_bytes(u))
            .map(|b| parse_to_unicode_cmap(&b));

        // Encoding & /Differences
        let mut byte_map = win_ansi_table();
        if let Some(enc_val) = dict.get("Encoding") {
            let enc_obj = self.resolve(enc_val);
            match &enc_obj {
                PdfVal::Name(n) => {
                    byte_map = base_encoding_table(n);
                }
                PdfVal::Dict(ed) => {
                    if let Some(base) = ed.get("BaseEncoding").and_then(PdfVal::as_name) {
                        byte_map = base_encoding_table(base);
                    }
                    if let Some(diff_val) = ed.get("Differences") {
                        let diff_obj = self.resolve(diff_val);
                        if let Some(arr) = diff_obj.as_array() {
                            apply_differences(&mut byte_map, arr);
                        }
                    }
                }
                _ => {}
            }
        }

        // Widths
        let mut widths: HashMap<u32, f64> = HashMap::new();
        let mut default_width = 500.0_f64;
        if is_type0 {
            default_width = 1000.0;
            if let Some(desc_val) = dict.get("DescendantFonts") {
                let desc_obj = self.resolve(desc_val);
                if let Some(first_desc) = desc_obj.as_array().and_then(|a| a.first()).cloned() {
                    let cid_obj = self.resolve(&first_desc);
                    if let Some(cid_dict) = cid_obj.as_dict() {
                        if let Some(dw) = cid_dict.get("DW").and_then(PdfVal::as_f64) {
                            default_width = dw;
                        }
                        if let Some(w_val) = cid_dict.get("W").cloned() {
                            let w_obj = self.resolve(&w_val);
                            if let Some(w_arr) = w_obj.as_array() {
                                parse_cid_widths(w_arr, &mut widths);
                            }
                        }
                    }
                }
            }
        } else {
            if let Some(fd_val) = dict.get("FontDescriptor") {
                let fd_obj = self.resolve(fd_val);
                if let Some(mw) = fd_obj
                    .as_dict()
                    .and_then(|d| d.get("MissingWidth"))
                    .and_then(PdfVal::as_f64)
                {
                    if mw > 0.0 {
                        default_width = mw;
                    }
                }
            }
            let first_char = dict.get("FirstChar").and_then(PdfVal::as_i64).unwrap_or(0) as u32;
            if let Some(w_val) = dict.get("Widths").cloned() {
                let w_obj = self.resolve(&w_val);
                if let Some(w_arr) = w_obj.as_array() {
                    for (i, w) in w_arr.iter().enumerate() {
                        if let Some(v) = w.as_f64() {
                            widths.insert(first_char + i as u32, v);
                        }
                    }
                }
            }
        }

        // Determine nominal space width in 1/1000 em
        let mut space_w = widths.get(&32).copied().filter(|&w| w > 50.0);
        if space_w.is_none() {
            if let Some(cmap) = &to_unicode {
                for (&(_len, code), mapped) in &cmap.map {
                    if mapped == " " {
                        if let Some(&w) = widths.get(&code) {
                            if w > 50.0 {
                                space_w = Some(w);
                                break;
                            }
                        }
                    }
                }
            }
        }
        let space_width = space_w.unwrap_or(260.0).clamp(160.0, 600.0);

        let font = Rc::new(PdfFont {
            is_type0,
            to_unicode,
            byte_map,
            widths,
            default_width,
            space_width,
        });
        if let Some(id) = cache_key {
            self.font_cache.insert(id, Rc::clone(&font));
        }
        Some(font)
    }

    fn interpret_content_stream(
        &mut self,
        stream: &[u8],
        resources: &BTreeMap<String, PdfVal>,
        initial_ctm: Matrix2D,
        chunks: &mut Vec<TextChunk>,
        seq: &mut usize,
        depth: usize,
    ) {
        if depth > MAX_FORM_DEPTH {
            return;
        }
        let fonts = self.load_resource_fonts(resources);
        let default_font = Rc::new(PdfFont::default_win_ansi());

        let mut ctm = initial_ctm;
        let mut gs_stack: Vec<Matrix2D> = Vec::new();
        let mut cur_font: Rc<PdfFont> = Rc::clone(&default_font);
        let mut font_size = 12.0_f64;
        let mut tc = 0.0_f64;
        let mut tw = 0.0_f64;
        let mut tz = 100.0_f64;
        let mut tl = 0.0_f64;
        let mut tm = Matrix2D::IDENTITY;
        let mut tlm = Matrix2D::IDENTITY;
        let mut in_text = false;

        let mut operands: Vec<PdfVal> = Vec::new();
        let mut lex = Lexer::new(stream, 0);

        while lex.pos < stream.len() {
            let Some(b) = lex.peek() else {
                break;
            };
            if matches!(b, b'<' | b'(' | b'[' | b'/') {
                if let Some(v) = lex.parse_val() {
                    operands.push(v);
                } else {
                    lex.pos += 1;
                }
                continue;
            }
            let tok = lex.read_token();
            if tok.is_empty() {
                lex.pos += 1;
                continue;
            }
            if let Some(n) = parse_int(tok) {
                operands.push(PdfVal::Int(n));
                continue;
            }
            if let Some(f) = parse_real(tok) {
                operands.push(PdfVal::Real(f));
                continue;
            }
            match tok {
                b"true" | b"false" => {
                    operands.push(PdfVal::Bool);
                    continue;
                }
                b"null" => {
                    operands.push(PdfVal::Null);
                    continue;
                }
                // Inline image `BI ... ID <raw> EI`: skip safely to `EI`.
                b"BI" => {
                    operands.clear();
                    while lex.pos + 2 < stream.len() {
                        if stream[lex.pos..].starts_with(b"EI")
                            && is_pdf_ws(stream[lex.pos.saturating_sub(1)])
                            && stream
                                .get(lex.pos + 2)
                                .is_none_or(|&c| is_pdf_ws(c) || is_pdf_delim(c))
                        {
                            lex.pos += 2;
                            break;
                        }
                        lex.pos += 1;
                    }
                    continue;
                }
                _ => {}
            }

            // Operator execution
            match tok {
                b"q" => {
                    if gs_stack.len() < 64 {
                        gs_stack.push(ctm);
                    }
                }
                b"Q" => {
                    if let Some(prev) = gs_stack.pop() {
                        ctm = prev;
                    }
                }
                b"cm" if operands.len() >= 6 => {
                    let n = operands.len();
                    if let (Some(a), Some(b), Some(c), Some(d), Some(e), Some(f)) = (
                        operands[n - 6].as_f64(),
                        operands[n - 5].as_f64(),
                        operands[n - 4].as_f64(),
                        operands[n - 3].as_f64(),
                        operands[n - 2].as_f64(),
                        operands[n - 1].as_f64(),
                    ) {
                        ctm = Matrix2D::new(a, b, c, d, e, f).mul(ctm);
                    }
                }
                b"BT" => {
                    in_text = true;
                    tm = Matrix2D::IDENTITY;
                    tlm = Matrix2D::IDENTITY;
                }
                b"ET" => {
                    in_text = false;
                }
                b"Tf" if operands.len() >= 2 => {
                    let n = operands.len();
                    if let (Some(name), Some(sz)) =
                        (operands[n - 2].as_name(), operands[n - 1].as_f64())
                    {
                        cur_font = fonts
                            .get(name)
                            .map(Rc::clone)
                            .unwrap_or_else(|| Rc::clone(&default_font));
                        font_size = sz;
                    }
                }
                b"Tc" if !operands.is_empty() => {
                    if let Some(v) = operands.last().and_then(PdfVal::as_f64) {
                        tc = v;
                    }
                }
                b"Tw" if !operands.is_empty() => {
                    if let Some(v) = operands.last().and_then(PdfVal::as_f64) {
                        tw = v;
                    }
                }
                b"Tz" if !operands.is_empty() => {
                    if let Some(v) = operands.last().and_then(PdfVal::as_f64) {
                        tz = v;
                    }
                }
                b"TL" if !operands.is_empty() => {
                    if let Some(v) = operands.last().and_then(PdfVal::as_f64) {
                        tl = v;
                    }
                }
                b"Tm" if operands.len() >= 6 => {
                    let n = operands.len();
                    if let (Some(a), Some(b), Some(c), Some(d), Some(e), Some(f)) = (
                        operands[n - 6].as_f64(),
                        operands[n - 5].as_f64(),
                        operands[n - 4].as_f64(),
                        operands[n - 3].as_f64(),
                        operands[n - 2].as_f64(),
                        operands[n - 1].as_f64(),
                    ) {
                        tm = Matrix2D::new(a, b, c, d, e, f);
                        tlm = tm;
                    }
                }
                b"Td" if operands.len() >= 2 => {
                    let n = operands.len();
                    if let (Some(tx), Some(ty)) =
                        (operands[n - 2].as_f64(), operands[n - 1].as_f64())
                    {
                        tlm = Matrix2D::translate(tx, ty).mul(tlm);
                        tm = tlm;
                    }
                }
                b"TD" if operands.len() >= 2 => {
                    let n = operands.len();
                    if let (Some(tx), Some(ty)) =
                        (operands[n - 2].as_f64(), operands[n - 1].as_f64())
                    {
                        tl = -ty;
                        tlm = Matrix2D::translate(tx, ty).mul(tlm);
                        tm = tlm;
                    }
                }
                b"T*" => {
                    tlm = Matrix2D::translate(0.0, -tl).mul(tlm);
                    tm = tlm;
                }
                b"Tj" if in_text && !operands.is_empty() => {
                    if let Some(raw) = operands.last().and_then(PdfVal::as_str_bytes) {
                        emit_text_string(
                            raw,
                            &cur_font,
                            font_size,
                            tc,
                            tw,
                            tz,
                            &mut tm,
                            ctm,
                            chunks,
                            seq,
                        );
                    }
                }
                b"'" if in_text && !operands.is_empty() => {
                    tlm = Matrix2D::translate(0.0, -tl).mul(tlm);
                    tm = tlm;
                    if let Some(raw) = operands.last().and_then(PdfVal::as_str_bytes) {
                        emit_text_string(
                            raw,
                            &cur_font,
                            font_size,
                            tc,
                            tw,
                            tz,
                            &mut tm,
                            ctm,
                            chunks,
                            seq,
                        );
                    }
                }
                b"\"" if in_text && operands.len() >= 3 => {
                    let n = operands.len();
                    if let (Some(aw), Some(ac), Some(raw)) = (
                        operands[n - 3].as_f64(),
                        operands[n - 2].as_f64(),
                        operands[n - 1].as_str_bytes(),
                    ) {
                        tw = aw;
                        tc = ac;
                        tlm = Matrix2D::translate(0.0, -tl).mul(tlm);
                        tm = tlm;
                        emit_text_string(
                            raw,
                            &cur_font,
                            font_size,
                            tc,
                            tw,
                            tz,
                            &mut tm,
                            ctm,
                            chunks,
                            seq,
                        );
                    }
                }
                b"TJ" if in_text && !operands.is_empty() => {
                    if let Some(arr) = operands.last().and_then(PdfVal::as_array) {
                        let scale = tz / 100.0;
                        for item in arr {
                            match item {
                                PdfVal::Str(raw) => {
                                    emit_text_string(
                                        raw,
                                        &cur_font,
                                        font_size,
                                        tc,
                                        tw,
                                        tz,
                                        &mut tm,
                                        ctm,
                                        chunks,
                                        seq,
                                    );
                                }
                                PdfVal::Int(n) => {
                                    let shift = -(*n as f64) / 1000.0 * font_size * scale;
                                    tm = Matrix2D::translate(shift, 0.0).mul(tm);
                                }
                                PdfVal::Real(n) => {
                                    let shift = -(*n) / 1000.0 * font_size * scale;
                                    tm = Matrix2D::translate(shift, 0.0).mul(tm);
                                }
                                _ => {}
                            }
                        }
                    }
                }
                b"Do" if !operands.is_empty() => {
                    if let Some(xname) = operands.last().and_then(PdfVal::as_name) {
                        let xobj_ref = resources
                            .get("XObject")
                            .cloned()
                            .map(|v| self.resolve(&v))
                            .and_then(|o| o.as_dict().and_then(|d| d.get(xname).cloned()));
                        if let Some(xr) = xobj_ref {
                            let xobj = self.resolve(&xr);
                            if let PdfVal::Stream { dict, data } = xobj {
                                if dict.get("Subtype").and_then(PdfVal::as_name) == Some("Form") {
                                    if let Ok(form_bytes) = decode_stream_bytes(&dict, &data) {
                                        let form_mat = dict
                                            .get("Matrix")
                                            .and_then(PdfVal::as_array)
                                            .filter(|a| a.len() == 6)
                                            .and_then(|a| {
                                                Some(Matrix2D::new(
                                                    a[0].as_f64()?,
                                                    a[1].as_f64()?,
                                                    a[2].as_f64()?,
                                                    a[3].as_f64()?,
                                                    a[4].as_f64()?,
                                                    a[5].as_f64()?,
                                                ))
                                            })
                                            .unwrap_or(Matrix2D::IDENTITY);
                                        let mut form_res = resources.clone();
                                        if let Some(fr) = dict.get("Resources") {
                                            let fr_obj = self.resolve(fr);
                                            if let Some(fd) = fr_obj.as_dict() {
                                                for (k, v) in fd {
                                                    form_res.insert(k.clone(), v.clone());
                                                }
                                            }
                                        }
                                        self.interpret_content_stream(
                                            &form_bytes,
                                            &form_res,
                                            form_mat.mul(ctm),
                                            chunks,
                                            seq,
                                            depth + 1,
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
            operands.clear();
        }
    }
}

struct PageInfo {
    contents: Option<PdfVal>,
    resources: BTreeMap<String, PdfVal>,
}

// ============================================================================
// 2D Matrix & Text Layout Engine
// ============================================================================

#[derive(Clone, Copy, Debug)]
struct Matrix2D {
    a: f64,
    b: f64,
    c: f64,
    d: f64,
    e: f64,
    f: f64,
}

impl Matrix2D {
    const IDENTITY: Self = Self {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        e: 0.0,
        f: 0.0,
    };

    fn new(a: f64, b: f64, c: f64, d: f64, e: f64, f: f64) -> Self {
        Self { a, b, c, d, e, f }
    }

    fn translate(tx: f64, ty: f64) -> Self {
        Self {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            e: tx,
            f: ty,
        }
    }

    /// PDF row-vector composition: `self × other`.
    fn mul(self, other: Self) -> Self {
        Self {
            a: self.a * other.a + self.b * other.c,
            b: self.a * other.b + self.b * other.d,
            c: self.c * other.a + self.d * other.c,
            d: self.c * other.b + self.d * other.d,
            e: self.e * other.a + self.f * other.c + other.e,
            f: self.e * other.b + self.f * other.d + other.f,
        }
    }

    fn origin(self) -> (f64, f64) {
        (self.e, self.f)
    }
}

struct TextChunk {
    x: f64,
    y: f64,
    end_x: f64,
    font_size: f64,
    space_width: f64,
    text: String,
    seq: usize,
}

#[allow(clippy::too_many_arguments)]
fn emit_text_string(
    raw: &[u8],
    font: &PdfFont,
    font_size: f64,
    tc: f64,
    tw: f64,
    tz: f64,
    tm: &mut Matrix2D,
    ctm: Matrix2D,
    chunks: &mut Vec<TextChunk>,
    seq: &mut usize,
) {
    if raw.is_empty() {
        return;
    }
    let trm_start = tm.mul(ctm);
    let (start_x, start_y) = trm_start.origin();
    let eff_fs = (font_size.abs() * trm_start.c.hypot(trm_start.d)).max(1.0);
    let horiz_scale = (font_size.abs() * (tz / 100.0).abs() * trm_start.a.hypot(trm_start.b)).max(0.1);
    let space_width_pt = (font.space_width / 1000.0 * horiz_scale).max(eff_fs * 0.18);

    let mut decoded = String::new();
    let mut advance = 0.0_f64;
    let scale = tz / 100.0;

    let mut i = 0usize;
    while i < raw.len() {
        let (code, step, text_piece) = font.decode_next(&raw[i..]);
        i += step;
        let w = font.glyph_width(code);
        let is_space = text_piece == " ";
        let char_adv = (w / 1000.0 * font_size + tc + if is_space { tw } else { 0.0 }) * scale;
        advance += char_adv;
        decoded.push_str(&text_piece);
    }

    *tm = Matrix2D::translate(advance, 0.0).mul(*tm);
    let trm_end = tm.mul(ctm);
    let (end_x, _) = trm_end.origin();

    if !decoded.is_empty() {
        *seq += 1;
        chunks.push(TextChunk {
            x: start_x,
            y: start_y,
            end_x: end_x.max(start_x),
            font_size: eff_fs,
            space_width: space_width_pt,
            text: decoded,
            seq: *seq,
        });
    }
}

fn assemble_page_lines(mut chunks: Vec<TextChunk>) -> String {
    if chunks.is_empty() {
        return String::new();
    }
    // Sort top-to-bottom (descending Y), then left-to-right (ascending X).
    chunks.sort_by(|a, b| {
        b.y.partial_cmp(&a.y)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal))
            .then_with(|| a.seq.cmp(&b.seq))
    });

    struct LineBucket {
        y: f64,
        max_fs: f64,
        chunks: Vec<TextChunk>,
    }

    let mut lines: Vec<LineBucket> = Vec::new();
    for ch in chunks {
        if let Some(last) = lines.last_mut() {
            let tol = last.max_fs.max(ch.font_size) * 0.45;
            if (last.y - ch.y).abs() <= tol {
                last.max_fs = last.max_fs.max(ch.font_size);
                last.chunks.push(ch);
                continue;
            }
        }
        lines.push(LineBucket {
            y: ch.y,
            max_fs: ch.font_size,
            chunks: vec![ch],
        });
    }

    let mut out = String::new();
    let mut prev_y: Option<f64> = None;
    let mut prev_fs = 12.0_f64;

    for mut bucket in lines {
        bucket.chunks.sort_by(|a, b| {
            a.x.partial_cmp(&b.x)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.seq.cmp(&b.seq))
        });
        let mut line = String::new();
        let mut prev_end_x: Option<f64> = None;
        let mut prev_space_w = 3.0_f64;

        for ch in bucket.chunks {
            if let Some(pe) = prev_end_x {
                let gap = ch.x - pe;
                let threshold = prev_space_w.min(ch.space_width) * 0.38;
                if gap > threshold.max(1.2)
                    && !line.ends_with(' ')
                    && !ch.text.starts_with(' ')
                {
                    line.push(' ');
                }
            }
            line.push_str(&ch.text);
            prev_end_x = Some(ch.end_x);
            prev_space_w = ch.space_width;
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(py) = prev_y {
            let dy = (py - bucket.y).abs();
            let para_gap = prev_fs.max(bucket.max_fs) * 1.55;
            out.push('\n');
            if dy > para_gap {
                out.push('\n');
            }
        }
        out.push_str(trimmed);
        prev_y = Some(bucket.y);
        prev_fs = bucket.max_fs;
    }
    out
}

// ============================================================================
// Font, ToUnicode CMap & Adobe Glyph List
// ============================================================================

struct PdfFont {
    is_type0: bool,
    to_unicode: Option<ToUnicodeCMap>,
    byte_map: [char; 256],
    widths: HashMap<u32, f64>,
    default_width: f64,
    space_width: f64,
}

impl PdfFont {
    fn default_win_ansi() -> Self {
        Self {
            is_type0: false,
            to_unicode: None,
            byte_map: win_ansi_table(),
            widths: HashMap::new(),
            default_width: 500.0,
            space_width: 260.0,
        }
    }

    fn glyph_width(&self, code: u32) -> f64 {
        self.widths.get(&code).copied().unwrap_or(self.default_width)
    }

    fn decode_next(&self, slice: &[u8]) -> (u32, usize, String) {
        if self.is_type0 {
            if slice.len() >= 2 {
                let c2 = u16::from_be_bytes([slice[0], slice[1]]) as u32;
                if let Some(cmap) = &self.to_unicode {
                    if let Some(s) = cmap.map.get(&(2, c2)) {
                        return (c2, 2, s.clone());
                    }
                    if let Some(s) = cmap.map.get(&(1, slice[0] as u32)) {
                        return (slice[0] as u32, 1, s.clone());
                    }
                }
                if let Some(ch) = char::from_u32(c2) {
                    if !ch.is_control() || ch == '\t' {
                        return (c2, 2, ch.to_string());
                    }
                }
                return (c2, 2, String::new());
            }
            let c1 = slice[0] as u32;
            return (c1, 1, self.byte_map[slice[0] as usize].to_string());
        }

        // Single-byte font (Type1, TrueType, Type3)
        let b = slice[0];
        let c1 = b as u32;
        if let Some(cmap) = &self.to_unicode {
            if let Some(s) = cmap.map.get(&(1, c1)) {
                return (c1, 1, s.clone());
            }
        }
        let ch = self.byte_map[b as usize];
        let s = if ch == '\0' {
            String::new()
        } else {
            ch.to_string()
        };
        (c1, 1, s)
    }
}

struct ToUnicodeCMap {
    map: HashMap<(u8, u32), String>,
}

fn parse_to_unicode_cmap(bytes: &[u8]) -> ToUnicodeCMap {
    let mut map = HashMap::new();
    let mut lex = Lexer::new(bytes, 0);
    let mut tokens: Vec<CMapTok> = Vec::new();
    while lex.pos < bytes.len() {
        let Some(b) = lex.peek() else {
            break;
        };
        match b {
            b'<' if bytes.get(lex.pos + 1) != Some(&b'<') => {
                tokens.push(CMapTok::Hex(lex.parse_hex_string()));
            }
            b'[' => {
                lex.pos += 1;
                let mut arr = Vec::new();
                while lex.pos < bytes.len() {
                    let Some(nb) = lex.peek() else {
                        break;
                    };
                    if nb == b']' {
                        lex.pos += 1;
                        break;
                    }
                    if nb == b'<' {
                        arr.push(lex.parse_hex_string());
                    } else {
                        let _ = lex.read_token();
                    }
                }
                tokens.push(CMapTok::HexArray(arr));
            }
            b'<' | b'>' | b'/' | b'(' => {
                let _ = lex.parse_val();
            }
            _ => {
                let kw = lex.read_token();
                if kw.is_empty() {
                    lex.pos += 1;
                } else {
                    tokens.push(CMapTok::Word(String::from_utf8_lossy(kw).into_owned()));
                }
            }
        }
    }

    let mut i = 0usize;
    while i < tokens.len() {
        match &tokens[i] {
            CMapTok::Word(w) if w == "beginbfchar" => {
                i += 1;
                while i + 1 < tokens.len() {
                    if matches!(&tokens[i], CMapTok::Word(end) if end == "endbfchar") {
                        i += 1;
                        break;
                    }
                    if let (CMapTok::Hex(src), CMapTok::Hex(dst)) = (&tokens[i], &tokens[i + 1]) {
                        if !src.is_empty() && src.len() <= 4 {
                            let code = read_be_uint(src) as u32;
                            let text = decode_utf16be_bytes(dst);
                            map.insert((src.len() as u8, code), text);
                        }
                        i += 2;
                    } else {
                        i += 1;
                    }
                }
            }
            CMapTok::Word(w) if w == "beginbfrange" => {
                i += 1;
                while i + 2 < tokens.len() {
                    if matches!(&tokens[i], CMapTok::Word(end) if end == "endbfrange") {
                        i += 1;
                        break;
                    }
                    if let (CMapTok::Hex(lo), CMapTok::Hex(hi)) = (&tokens[i], &tokens[i + 1]) {
                        let key_len = lo.len().clamp(1, 4) as u8;
                        let start = read_be_uint(lo) as u32;
                        let end = read_be_uint(hi) as u32;
                        let span = end.saturating_sub(start).min(65535);
                        match &tokens[i + 2] {
                            CMapTok::Hex(dst) => {
                                let mut units = bytes_to_u16_be(dst);
                                for offset in 0..=span {
                                    if let Some(s) = decode_utf16_units(&units) {
                                        map.insert((key_len, start + offset), s);
                                    }
                                    if let Some(last) = units.last_mut() {
                                        *last = last.wrapping_add(1);
                                    }
                                }
                                i += 3;
                            }
                            CMapTok::HexArray(arr) => {
                                for (offset, dst) in arr.iter().enumerate() {
                                    let code = start.saturating_add(offset as u32);
                                    if code > end {
                                        break;
                                    }
                                    map.insert((key_len, code), decode_utf16be_bytes(dst));
                                }
                                i += 3;
                            }
                            _ => i += 1,
                        }
                    } else {
                        i += 1;
                    }
                }
            }
            _ => i += 1,
        }
    }
    ToUnicodeCMap { map }
}

enum CMapTok {
    Word(String),
    Hex(Vec<u8>),
    HexArray(Vec<Vec<u8>>),
}

fn bytes_to_u16_be(bytes: &[u8]) -> Vec<u16> {
    if bytes.len() == 1 {
        return vec![bytes[0] as u16];
    }
    bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| u16::from_be_bytes(*c))
        .collect()
}

fn decode_utf16_units(units: &[u16]) -> Option<String> {
    if units.is_empty() {
        return None;
    }
    Some(String::from_utf16_lossy(units))
}

fn decode_utf16be_bytes(bytes: &[u8]) -> String {
    decode_utf16_units(&bytes_to_u16_be(bytes)).unwrap_or_default()
}

fn parse_cid_widths(arr: &[PdfVal], out: &mut HashMap<u32, f64>) {
    let mut i = 0usize;
    while i + 1 < arr.len() {
        let Some(first) = arr[i].as_i64().filter(|&v| v >= 0) else {
            i += 1;
            continue;
        };
        if let Some(sub) = arr[i + 1].as_array() {
            for (off, v) in sub.iter().enumerate() {
                if let Some(w) = v.as_f64() {
                    out.insert(first as u32 + off as u32, w);
                }
            }
            i += 2;
        } else if i + 2 < arr.len() {
            if let (Some(last), Some(w)) = (arr[i + 1].as_i64(), arr[i + 2].as_f64()) {
                let end = last.min(first + 65535);
                for code in first..=end {
                    out.insert(code as u32, w);
                }
            }
            i += 3;
        } else {
            break;
        }
    }
}

fn base_encoding_table(name: &str) -> [char; 256] {
    match name {
        "MacRomanEncoding" => mac_roman_table(),
        "StandardEncoding" => standard_encoding_table(),
        _ => win_ansi_table(),
    }
}

fn win_ansi_table() -> [char; 256] {
    let mut t = ['\0'; 256];
    let all: Vec<u8> = (0..=255).collect();
    let (decoded, _, _) = encoding_rs::WINDOWS_1252.decode(&all);
    for (i, ch) in decoded.chars().enumerate().take(256) {
        t[i] = ch;
    }
    t
}

fn mac_roman_table() -> [char; 256] {
    let mut t = ['\0'; 256];
    let all: Vec<u8> = (0..=255).collect();
    let (decoded, _, _) = encoding_rs::MACINTOSH.decode(&all);
    for (i, ch) in decoded.chars().enumerate().take(256) {
        t[i] = ch;
    }
    t
}

fn standard_encoding_table() -> [char; 256] {
    let mut t = win_ansi_table();
    t[0x27] = '\u{2019}';
    t[0x60] = '\u{2018}';
    t[0xA4] = '/';
    t[0xA8] = '\u{00A4}';
    t[0xA9] = '\'';
    t[0xAA] = '\u{201C}';
    t[0xB1] = '\u{2013}';
    t[0xB7] = '\u{2022}';
    t[0xBA] = '\u{201D}';
    t[0xBC] = '\u{2026}';
    t[0xD0] = '\u{2014}';
    t
}

fn apply_differences(table: &mut [char; 256], arr: &[PdfVal]) {
    let mut code = 0usize;
    for item in arr {
        match item {
            PdfVal::Int(n) if (0..=255).contains(n) => code = *n as usize,
            PdfVal::Name(glyph) if code < 256 => {
                if let Some(ch) = glyph_name_to_char(glyph) {
                    table[code] = ch;
                }
                code += 1;
            }
            _ => {}
        }
    }
}

fn glyph_name_to_char(name: &str) -> Option<char> {
    let base = name.split('.').next().unwrap_or(name);
    if base.len() == 1 {
        return base.chars().next();
    }
    if let Some(hex) = base.strip_prefix("uni") {
        if hex.len() == 4 {
            if let Ok(v) = u32::from_str_radix(hex, 16) {
                return char::from_u32(v);
            }
        }
    }
    if let Some(hex) = base.strip_prefix('u') {
        if (4..=6).contains(&hex.len()) && hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            if let Ok(v) = u32::from_str_radix(hex, 16) {
                return char::from_u32(v);
            }
        }
    }
    let ch = match base {
        "space" | "nbspace" => ' ',
        "exclam" => '!',
        "quotedbl" => '"',
        "numbersign" => '#',
        "dollar" => '$',
        "percent" => '%',
        "ampersand" => '&',
        "quotesingle" => '\'',
        "parenleft" => '(',
        "parenright" => ')',
        "asterisk" => '*',
        "plus" => '+',
        "comma" => ',',
        "hyphen" | "minus" => '-',
        "period" => '.',
        "slash" => '/',
        "zero" => '0',
        "one" => '1',
        "two" => '2',
        "three" => '3',
        "four" => '4',
        "five" => '5',
        "six" => '6',
        "seven" => '7',
        "eight" => '8',
        "nine" => '9',
        "colon" => ':',
        "semicolon" => ';',
        "less" => '<',
        "equal" => '=',
        "greater" => '>',
        "question" => '?',
        "at" => '@',
        "bracketleft" => '[',
        "backslash" => '\\',
        "bracketright" => ']',
        "asciicircum" => '^',
        "underscore" => '_',
        "grave" => '`',
        "braceleft" => '{',
        "bar" => '|',
        "braceright" => '}',
        "asciitilde" => '~',
        "Agrave" => 'À', "Aacute" => 'Á', "Acircumflex" => 'Â', "Atilde" => 'Ã', "Adieresis" => 'Ä', "Aring" => 'Å',
        "Ccedilla" => 'Ç', "Egrave" => 'È', "Eacute" => 'É', "Ecircumflex" => 'Ê', "Edieresis" => 'Ë',
        "Igrave" => 'Ì', "Iacute" => 'Í', "Icircumflex" => 'Î', "Idieresis" => 'Ï', "Ntilde" => 'Ñ',
        "Ograve" => 'Ò', "Oacute" => 'Ó', "Ocircumflex" => 'Ô', "Otilde" => 'Õ', "Odieresis" => 'Ö',
        "Ugrave" => 'Ù', "Uacute" => 'Ú', "Ucircumflex" => 'Û', "Udieresis" => 'Ü', "Yacute" => 'Ý',
        "agrave" => 'à', "aacute" => 'á', "acircumflex" => 'â', "atilde" => 'ã', "adieresis" => 'ä', "aring" => 'å',
        "ccedilla" => 'ç', "egrave" => 'è', "eacute" => 'é', "ecircumflex" => 'ê', "edieresis" => 'ë',
        "igrave" => 'ì', "iacute" => 'í', "icircumflex" => 'î', "idieresis" => 'ï', "ntilde" => 'ñ',
        "ograve" => 'ò', "oacute" => 'ó', "ocircumflex" => 'ô', "otilde" => 'õ', "odieresis" => 'ö',
        "ugrave" => 'ù', "uacute" => 'ú', "ucircumflex" => 'û', "udieresis" => 'ü', "yacute" => 'ý', "ydieresis" => 'ÿ',
        "germandbls" => 'ß', "AE" => 'Æ', "ae" => 'æ', "OE" => 'Œ', "oe" => 'œ',
        "copyright" => '©', "registered" => '®', "trademark" => '™', "section" => '§',
        "degree" => '°', "plusminus" => '±', "ordfeminine" => 'ª', "ordmasculine" => 'º',
        "guillemotleft" => '«', "guillemotright" => '»', "exclamdown" => '¡', "questiondown" => '¿',
        "endash" => '–', "emdash" => '—', "bullet" => '•', "ellipsis" => '…', "Euro" => '€',
        "quoteleft" => '‘', "quoteright" => '’', "quotedblleft" => '“', "quotedblright" => '”',
        "fi" | "f_i" => '\u{FB01}',
        "fl" | "f_l" => '\u{FB02}',
        "ff" | "f_f" => '\u{FB00}',
        _ => return None,
    };
    Some(ch)
}

// ============================================================================
// Stream Filters & Predictors
// ============================================================================

fn decode_stream_bytes(dict: &BTreeMap<String, PdfVal>, raw: &[u8]) -> Result<Vec<u8>, String> {
    let mut filters: Vec<String> = Vec::new();
    match dict.get("Filter") {
        Some(PdfVal::Name(n)) => filters.push(n.clone()),
        Some(PdfVal::Array(arr)) => {
            for item in arr {
                if let Some(n) = item.as_name() {
                    filters.push(n.to_string());
                }
            }
        }
        _ => return Ok(raw.to_vec()),
    }
    let parms_list: Vec<Option<&BTreeMap<String, PdfVal>>> = match dict.get("DecodeParms") {
        Some(PdfVal::Dict(d)) => vec![Some(d)],
        Some(PdfVal::Array(arr)) => arr.iter().map(PdfVal::as_dict).collect(),
        _ => vec![],
    };

    let mut cur = raw.to_vec();
    for (idx, filter) in filters.iter().enumerate() {
        let parms = parms_list.get(idx).copied().flatten();
        cur = match filter.as_str() {
            "FlateDecode" | "Fl" => {
                let inflated = flate_decode(&cur)?;
                apply_predictor(&inflated, parms)?
            }
            "ASCIIHexDecode" | "AHx" => ascii_hex_decode(&cur),
            "ASCII85Decode" | "A85" => ascii85_decode(&cur),
            "RunLengthDecode" | "RL" => run_length_decode(&cur),
            "LZWDecode" | "LZW" => {
                let early = parms
                    .and_then(|d| d.get("EarlyChange"))
                    .and_then(PdfVal::as_i64)
                    .unwrap_or(1)
                    != 0;
                let decoded = lzw_decode(&cur, early);
                apply_predictor(&decoded, parms)?
            }
            _ => return Err(format!("Filtro PDF não suportado: {filter}")),
        };
    }
    Ok(cur)
}

fn flate_decode(data: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let res = ZlibDecoder::new(data)
        .take(MAX_DECOMPRESSED_STREAM as u64)
        .read_to_end(&mut out);
    if res.is_ok() || !out.is_empty() {
        return Ok(out);
    }
    // Fallback: some malformed PDFs have leading whitespace or raw deflate.
    let trimmed = data
        .iter()
        .position(|b| !b.is_ascii_whitespace())
        .map_or(data, |i| &data[i..]);
    out.clear();
    let res2 = ZlibDecoder::new(trimmed)
        .take(MAX_DECOMPRESSED_STREAM as u64)
        .read_to_end(&mut out);
    if res2.is_ok() || !out.is_empty() {
        return Ok(out);
    }
    out.clear();
    DeflateDecoder::new(trimmed)
        .take(MAX_DECOMPRESSED_STREAM as u64)
        .read_to_end(&mut out)
        .map_err(|e| format!("Falha ao descompactar FlateDecode: {e}"))?;
    Ok(out)
}

fn apply_predictor(
    data: &[u8],
    parms: Option<&BTreeMap<String, PdfVal>>,
) -> Result<Vec<u8>, String> {
    let Some(d) = parms else {
        return Ok(data.to_vec());
    };
    let predictor = d.get("Predictor").and_then(PdfVal::as_i64).unwrap_or(1);
    if predictor <= 1 {
        return Ok(data.to_vec());
    }
    let columns = d.get("Columns").and_then(PdfVal::as_i64).unwrap_or(1).max(1) as usize;
    let colors = d.get("Colors").and_then(PdfVal::as_i64).unwrap_or(1).max(1) as usize;
    let bpc = d
        .get("BitsPerComponent")
        .and_then(PdfVal::as_i64)
        .unwrap_or(8)
        .max(1) as usize;
    let bpp = (colors * bpc).div_ceil(8).max(1);
    let row_bytes = (columns * colors * bpc).div_ceil(8).max(1);

    if predictor == 2 {
        // TIFF Predictor 2 (horizontal differencing, 8-bit)
        let mut out = data.to_vec();
        for row in out.chunks_mut(row_bytes) {
            for i in bpp..row.len() {
                row[i] = row[i].wrapping_add(row[i - bpp]);
            }
        }
        return Ok(out);
    }

    if (10..=15).contains(&predictor) {
        let stride = row_bytes + 1;
        let mut out = Vec::with_capacity(data.len());
        let mut prev_row = vec![0u8; row_bytes];
        for chunk in data.chunks(stride) {
            if chunk.len() < 2 {
                break;
            }
            let filter = chunk[0];
            let src = &chunk[1..];
            let mut row = vec![0u8; src.len()];
            for i in 0..src.len() {
                let raw = src[i];
                let left = if i >= bpp { row[i - bpp] } else { 0 };
                let up = prev_row.get(i).copied().unwrap_or(0);
                let up_left = if i >= bpp {
                    prev_row.get(i - bpp).copied().unwrap_or(0)
                } else {
                    0
                };
                row[i] = match filter {
                    0 => raw,
                    1 => raw.wrapping_add(left),
                    2 => raw.wrapping_add(up),
                    3 => raw.wrapping_add((((left as u16) + (up as u16)) / 2) as u8),
                    4 => raw.wrapping_add(paeth(left, up, up_left)),
                    _ => raw,
                };
            }
            out.extend_from_slice(&row);
            prev_row = row;
        }
        return Ok(out);
    }

    Ok(data.to_vec())
}

fn paeth(a: u8, b: u8, c: u8) -> u8 {
    let p = a as i32 + b as i32 - c as i32;
    let pa = (p - a as i32).abs();
    let pb = (p - b as i32).abs();
    let pc = (p - c as i32).abs();
    if pa <= pb && pa <= pc {
        a
    } else if pb <= pc {
        b
    } else {
        c
    }
}

fn ascii_hex_decode(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut hi: Option<u8> = None;
    for &b in data {
        if b == b'>' {
            break;
        }
        if let Some(d) = hex_digit(b) {
            if let Some(h) = hi.take() {
                out.push((h << 4) | d);
            } else {
                hi = Some(d);
            }
        }
    }
    if let Some(h) = hi {
        out.push(h << 4);
    }
    out
}

fn ascii85_decode(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut group = [0u32; 5];
    let mut count = 0usize;
    let mut i = 0usize;
    while i < data.len() {
        let b = data[i];
        i += 1;
        if b.is_ascii_whitespace() {
            continue;
        }
        if b == b'~' {
            break;
        }
        if b == b'z' && count == 0 {
            out.extend_from_slice(&[0, 0, 0, 0]);
            continue;
        }
        if !(b'!'..=b'u').contains(&b) {
            continue;
        }
        group[count] = (b - b'!') as u32;
        count += 1;
        if count == 5 {
            let val = group
                .iter()
                .fold(0u64, |acc, &d| acc * 85 + d as u64) as u32;
            out.extend_from_slice(&val.to_be_bytes());
            count = 0;
        }
    }
    if count >= 2 {
        for slot in group.iter_mut().skip(count) {
            *slot = 84;
        }
        let val = group
            .iter()
            .fold(0u64, |acc, &d| acc * 85 + d as u64) as u32;
        out.extend_from_slice(&val.to_be_bytes()[..count - 1]);
    }
    out
}

fn run_length_decode(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < data.len() {
        let b = data[i];
        i += 1;
        if b == 128 {
            break;
        } else if b < 128 {
            let n = (b as usize) + 1;
            let end = (i + n).min(data.len());
            out.extend_from_slice(&data[i..end]);
            i = end;
        } else if i < data.len() {
            let n = 257 - (b as usize);
            out.resize(out.len() + n, data[i]);
            i += 1;
        }
    }
    out
}

fn lzw_decode(data: &[u8], early_change: bool) -> Vec<u8> {
    let mut out = Vec::new();
    let mut table: Vec<Vec<u8>> = (0..=255u8).map(|b| vec![b]).collect();
    table.push(Vec::new()); // 256 Clear
    table.push(Vec::new()); // 257 EOD
    let mut code_size = 9u32;
    let mut bit_buf = 0u32;
    let mut bits_in = 0u32;
    let mut byte_idx = 0usize;
    let mut prev: Option<Vec<u8>> = None;

    let read_code = |buf: &mut u32, bin: &mut u32, idx: &mut usize, width: u32| -> Option<u32> {
        while *bin < width {
            if *idx >= data.len() {
                return None;
            }
            *buf = (*buf << 8) | (data[*idx] as u32);
            *idx += 1;
            *bin += 8;
        }
        *bin -= width;
        Some((*buf >> *bin) & ((1 << width) - 1))
    };

    while let Some(code) = read_code(&mut bit_buf, &mut bits_in, &mut byte_idx, code_size) {
        if code == 257 || out.len() >= MAX_DECOMPRESSED_STREAM {
            break;
        }
        if code == 256 {
            table.truncate(258);
            code_size = 9;
            prev = None;
            continue;
        }
        let entry = if (code as usize) < table.len() {
            table[code as usize].clone()
        } else if code as usize == table.len() {
            if let Some(p) = &prev {
                let mut e = p.clone();
                if let Some(&first) = p.first() {
                    e.push(first);
                }
                e
            } else {
                break;
            }
        } else {
            break;
        };
        out.extend_from_slice(&entry);
        if let Some(p) = &prev {
            if table.len() < 4096 {
                let mut next = p.clone();
                if let Some(&first) = entry.first() {
                    next.push(first);
                }
                table.push(next);
                let threshold = (1usize << code_size) - usize::from(early_change);
                if table.len() >= threshold && code_size < 12 {
                    code_size += 1;
                }
            }
        }
        prev = Some(entry);
    }
    out
}

// ============================================================================
// Standard Empty-User-Password PDF RC4 Decryption (V=1/2, R=2/3)
// ============================================================================

const PDF_PADDING: [u8; 32] = [
    0x28, 0xBF, 0x4E, 0x5E, 0x4E, 0x75, 0x8A, 0x41, 0x64, 0x00, 0x4E, 0x56, 0xFF, 0xFA, 0x01, 0x08,
    0x2E, 0x2E, 0x00, 0xB6, 0xD0, 0x68, 0x3E, 0x80, 0x2F, 0x0C, 0xA9, 0xFE, 0x64, 0x53, 0x69, 0x7A,
];

fn build_empty_password_crypt(
    enc: &BTreeMap<String, PdfVal>,
    trailer: &BTreeMap<String, PdfVal>,
) -> Result<Rc4Crypt, String> {
    let v = enc.get("V").and_then(PdfVal::as_i64).unwrap_or(0);
    let r = enc.get("R").and_then(PdfVal::as_i64).unwrap_or(0);
    if !matches!(v, 1 | 2) || !matches!(r, 2 | 3) {
        return Err(
            "Este arquivo PDF está criptografado/protegido e não pode ser convertido para texto."
                .into(),
        );
    }
    let o = enc
        .get("O")
        .and_then(PdfVal::as_str_bytes)
        .ok_or("Dicionário /Encrypt incompleto.")?;
    let u = enc
        .get("U")
        .and_then(PdfVal::as_str_bytes)
        .ok_or("Dicionário /Encrypt incompleto.")?;
    let p = enc.get("P").and_then(PdfVal::as_i64).unwrap_or(0) as i32;
    let key_bits = enc
        .get("Length")
        .and_then(PdfVal::as_i64)
        .unwrap_or(if v == 1 { 40 } else { 128 }) as usize;
    let key_len = (key_bits / 8).clamp(5, 16);
    let id0 = trailer
        .get("ID")
        .and_then(PdfVal::as_array)
        .and_then(|a| a.first())
        .and_then(PdfVal::as_str_bytes)
        .unwrap_or(&[]);

    let mut hash_in = Vec::new();
    hash_in.extend_from_slice(&PDF_PADDING);
    hash_in.extend_from_slice(o);
    hash_in.extend_from_slice(&p.to_le_bytes());
    hash_in.extend_from_slice(id0);
    let mut digest = md5(&hash_in);
    if r >= 3 {
        for _ in 0..50 {
            digest = md5(&digest[..key_len]);
        }
    }
    let file_key = digest[..key_len].to_vec();

    // Verify empty user password matches /U.
    let valid = if r == 2 {
        let computed = rc4(&file_key, &PDF_PADDING);
        u.len() >= 16 && computed[..16] == u[..16]
    } else {
        let mut u_in = Vec::from(PDF_PADDING);
        u_in.extend_from_slice(id0);
        let mut step = md5(&u_in).to_vec();
        for i in 0..20u8 {
            let k: Vec<u8> = file_key.iter().map(|&b| b ^ i).collect();
            step = rc4(&k, &step);
        }
        u.len() >= 16 && step[..16] == u[..16]
    };
    if !valid {
        return Err(
            "Este arquivo PDF está protegido por senha e não pode ser convertido para texto."
                .into(),
        );
    }
    Ok(Rc4Crypt { file_key })
}

fn decrypt_val(val: &mut PdfVal, crypt: &Rc4Crypt, obj_num: u32, gen_num: u16) {
    match val {
        PdfVal::Str(b) => *b = crypt.decrypt(obj_num, gen_num, b),
        PdfVal::Array(arr) => {
            for v in arr {
                decrypt_val(v, crypt, obj_num, gen_num);
            }
        }
        PdfVal::Dict(d) => {
            for v in d.values_mut() {
                decrypt_val(v, crypt, obj_num, gen_num);
            }
        }
        _ => {}
    }
}

fn rc4(key: &[u8], data: &[u8]) -> Vec<u8> {
    if key.is_empty() {
        return data.to_vec();
    }
    let mut s = [0u8; 256];
    for (i, slot) in s.iter_mut().enumerate() {
        *slot = i as u8;
    }
    let mut j = 0u8;
    for i in 0..256 {
        j = j.wrapping_add(s[i]).wrapping_add(key[i % key.len()]);
        s.swap(i, j as usize);
    }
    let mut out = Vec::with_capacity(data.len());
    let (mut i, mut j) = (0u8, 0u8);
    for &b in data {
        i = i.wrapping_add(1);
        j = j.wrapping_add(s[i as usize]);
        s.swap(i as usize, j as usize);
        let k = s[(s[i as usize].wrapping_add(s[j as usize])) as usize];
        out.push(b ^ k);
    }
    out
}

/// Pure-Rust RFC 1321 MD5 (used solely for PDF Standard Security Handler key derivation).
fn md5(data: &[u8]) -> [u8; 16] {
    const S: [u32; 64] = [
        7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22,
        5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20,
        4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23,
        6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
    ];
    const K: [u32; 64] = [
        0xd76aa478, 0xe8c7b756, 0x242070db, 0xc1bdceee, 0xf57c0faf, 0x4787c62a, 0xa8304613, 0xfd469501,
        0x698098d8, 0x8b44f7af, 0xffff5bb1, 0x895cd7be, 0x6b901122, 0xfd987193, 0xa679438e, 0x49b40821,
        0xf61e2562, 0xc040b340, 0x265e5a51, 0xe9b6c7aa, 0xd62f105d, 0x02441453, 0xd8a1e681, 0xe7d3fbc8,
        0x21e1cde6, 0xc33707d6, 0xf4d50d87, 0x455a14ed, 0xa9e3e905, 0xfcefa3f8, 0x676f02d9, 0x8d2a4c8a,
        0xfffa3942, 0x8771f681, 0x6d9d6122, 0xfde5380c, 0xa4beea44, 0x4bdecfa9, 0xf6bb4b60, 0xbebfbc70,
        0x289b7ec6, 0xeaa127fa, 0xd4ef3085, 0x04881d05, 0xd9d4d039, 0xe6db99e5, 0x1fa27cf8, 0xc4ac5665,
        0xf4292244, 0x432aff97, 0xab9423a7, 0xfc93a039, 0x655b59c3, 0x8f0ccc92, 0xffeff47d, 0x85845dd1,
        0x6fa87e4f, 0xfe2ce6e0, 0xa3014314, 0x4e0811a1, 0xf7537e82, 0xbd3af235, 0x2ad7d2bb, 0xeb86d391,
    ];
    let mut msg = data.to_vec();
    let bit_len = (data.len() as u64).wrapping_mul(8);
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_le_bytes());

    let (mut a0, mut b0, mut c0, mut d0) =
        (0x67452301u32, 0xefcdab89u32, 0x98badcfeu32, 0x10325476u32);
    for chunk in msg.as_chunks::<64>().0 {
        let mut m = [0u32; 16];
        for (i, w) in chunk.as_chunks::<4>().0.iter().enumerate() {
            m[i] = u32::from_le_bytes(*w);
        }
        let (mut a, mut b, mut c, mut d) = (a0, b0, c0, d0);
        for i in 0..64 {
            let (f, g) = match i {
                0..=15 => ((b & c) | ((!b) & d), i),
                16..=31 => ((d & b) | ((!d) & c), (5 * i + 1) % 16),
                32..=47 => (b ^ c ^ d, (3 * i + 5) % 16),
                _ => (c ^ (b | (!d)), (7 * i) % 16),
            };
            let tmp = d;
            d = c;
            c = b;
            let sum = a.wrapping_add(f).wrapping_add(K[i]).wrapping_add(m[g]);
            b = b.wrapping_add(sum.rotate_left(S[i]));
            a = tmp;
        }
        a0 = a0.wrapping_add(a);
        b0 = b0.wrapping_add(b);
        c0 = c0.wrapping_add(c);
        d0 = d0.wrapping_add(d);
    }
    let mut out = [0u8; 16];
    out[0..4].copy_from_slice(&a0.to_le_bytes());
    out[4..8].copy_from_slice(&b0.to_le_bytes());
    out[8..12].copy_from_slice(&c0.to_le_bytes());
    out[12..16].copy_from_slice(&d0.to_le_bytes());
    out
}

// ============================================================================
// Binary Search Helpers
// ============================================================================

fn find_startxref_offsets(data: &[u8]) -> Vec<usize> {
    let mut offsets = Vec::new();
    let needle = b"startxref";
    let mut search_end = data.len();
    while search_end >= needle.len() {
        let window_start = search_end.saturating_sub(65536);
        let slice = &data[window_start..search_end];
        let Some(rel) = slice.windows(needle.len()).rposition(|w| w == needle) else {
            break;
        };
        let pos = window_start + rel;
        let mut lex = Lexer::new(data, pos + needle.len());
        if let Some(off) = parse_int(lex.read_token_ws()) {
            if off > 0 && (off as usize) < data.len() {
                offsets.push(off as usize);
            }
        }
        if pos == 0 || offsets.len() >= 4 {
            break;
        }
        search_end = pos;
    }
    offsets
}

fn find_endstream(data: &[u8], start: usize) -> usize {
    let needle = b"endstream";
    if let Some(rel) = data[start..].windows(needle.len()).position(|w| w == needle) {
        let mut end = start + rel;
        if end > start && data[end - 1] == b'\n' {
            end -= 1;
        }
        if end > start && data[end - 1] == b'\r' {
            end -= 1;
        }
        end
    } else {
        data.len()
    }
}

fn read_be_uint(bytes: &[u8]) -> u64 {
    let mut v = 0u64;
    for &b in bytes {
        v = (v << 8) | (b as u64);
    }
    v
}
