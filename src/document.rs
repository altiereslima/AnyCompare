use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Encoding {
    Utf8,
    Utf8Bom,
    Utf16Le,
    Utf16Be,
    Utf16LeNoBom,
    Utf16BeNoBom,
    Windows1252,
}

impl Encoding {
    pub const ALL: [Self; 7] = [
        Self::Utf8,
        Self::Utf8Bom,
        Self::Utf16Le,
        Self::Utf16Be,
        Self::Windows1252,
        Self::Utf16LeNoBom,
        Self::Utf16BeNoBom,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Utf8 => "UTF-8",
            Self::Utf8Bom => "UTF-8 BOM",
            Self::Utf16Le => "UTF-16 LE",
            Self::Utf16Be => "UTF-16 BE",
            Self::Utf16LeNoBom => "UTF-16 LE (sem BOM)",
            Self::Utf16BeNoBom => "UTF-16 BE (sem BOM)",
            Self::Windows1252 => "ANSI (Windows-1252)",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineEnding {
    CrLf,
    Lf,
    Cr,
    Mixed,
}

impl LineEnding {
    pub const ALL: [Self; 4] = [Self::CrLf, Self::Lf, Self::Cr, Self::Mixed];
    pub fn label(self) -> &'static str {
        match self {
            Self::CrLf => "PC",
            Self::Lf => "Unix",
            Self::Cr => "Mac",
            Self::Mixed => "Misto",
        }
    }
    fn text(self) -> &'static str {
        match self {
            Self::CrLf => "\r\n",
            Self::Cr => "\r",
            _ => "\n",
        }
    }
}

/// The edit buffer uses LF. Original terminators are retained separately so
/// editing one line does not silently convert the rest of a mixed-EOL file.
fn normalize_lines(text: &str) -> (String, Vec<LineEnding>) {
    let mut normalized = String::with_capacity(text.len());
    let mut endings = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\r' => {
                let ending = if chars.peek() == Some(&'\n') {
                    chars.next();
                    LineEnding::CrLf
                } else {
                    LineEnding::Cr
                };
                normalized.push('\n');
                endings.push(ending);
            }
            '\n' => {
                normalized.push('\n');
                endings.push(LineEnding::Lf);
            }
            _ => normalized.push(c),
        }
    }
    (normalized, endings)
}

#[derive(Clone, Debug)]
pub struct Document {
    pub path: Option<PathBuf>,
    pub text: String,
    pub encoding: Encoding,
    pub line_ending: LineEnding,
    saved_text: String,
    saved_encoding: Encoding,
    saved_line_ending: LineEnding,
    saved_endings: Vec<LineEnding>,
    disk_bytes: Option<Vec<u8>>,
    modified: Option<SystemTime>,
}

impl Default for Document {
    fn default() -> Self {
        Self {
            path: None,
            text: String::new(),
            encoding: Encoding::Utf8,
            line_ending: if cfg!(windows) {
                LineEnding::CrLf
            } else {
                LineEnding::Lf
            },
            saved_text: String::new(),
            saved_encoding: Encoding::Utf8,
            saved_line_ending: if cfg!(windows) {
                LineEnding::CrLf
            } else {
                LineEnding::Lf
            },
            saved_endings: Vec::new(),
            disk_bytes: None,
            modified: None,
        }
    }
}

impl Document {
    pub fn load(path: &Path) -> Result<Self, String> {
        Self::load_with_encoding(path, None)
    }

    pub fn load_with_encoding(path: &Path, forced: Option<Encoding>) -> Result<Self, String> {
        let bytes = fs::read(path)
            .map_err(|e| format!("Não foi possível abrir {}: {e}", path.display()))?;
        let (text, encoding) = if let Some(encoding) = forced {
            Self::decode_as(&bytes, encoding)?
        } else {
            Self::decode(&bytes)?
        };
        let (text, saved_endings) = normalize_lines(&text);
        let first = saved_endings.first().copied().unwrap_or(LineEnding::Lf);
        let line_ending = if saved_endings.iter().all(|&e| e == first) {
            first
        } else {
            LineEnding::Mixed
        };
        Ok(Self {
            path: Some(path.to_owned()),
            saved_text: text.clone(),
            text,
            encoding,
            line_ending,
            saved_encoding: encoding,
            saved_line_ending: line_ending,
            saved_endings,
            disk_bytes: Some(bytes),
            modified: fs::metadata(path)
                .ok()
                .and_then(|metadata| metadata.modified().ok()),
        })
    }

    fn decode(bytes: &[u8]) -> Result<(String, Encoding), String> {
        if bytes.starts_with(&[0xff, 0xfe, 0, 0]) || bytes.starts_with(&[0, 0, 0xfe, 0xff]) {
            return Err(
                "UTF-32 não é suportado. Converta o arquivo para UTF-8 ou UTF-16 antes de abrir."
                    .into(),
            );
        }
        if bytes.starts_with(&[0xff, 0xfe]) || bytes.starts_with(&[0xfe, 0xff]) {
            let little = bytes[0] == 0xff;
            if !(bytes.len() - 2).is_multiple_of(2) {
                return Err("Arquivo UTF-16 incompleto.".into());
            }
            let words: Vec<u16> = bytes[2..]
                .as_chunks::<2>()
                .0
                .iter()
                .map(|c| {
                    if little {
                        u16::from_le_bytes([c[0], c[1]])
                    } else {
                        u16::from_be_bytes([c[0], c[1]])
                    }
                })
                .collect();
            let text =
                String::from_utf16(&words).map_err(|_| "O arquivo contém UTF-16 inválido.")?;
            if text.contains('\0') {
                return Err(
                    "Este arquivo contém dados binários e não pode ser aberto como texto.".into(),
                );
            }
            return Ok((
                text,
                if little {
                    Encoding::Utf16Le
                } else {
                    Encoding::Utf16Be
                },
            ));
        }
        let bom = bytes.starts_with(&[0xef, 0xbb, 0xbf]);
        let content = if bom { &bytes[3..] } else { bytes };
        if content.contains(&0) {
            return Err(
                "Este arquivo contém dados binários e não pode ser aberto como texto.".into(),
            );
        }
        let text = match std::str::from_utf8(content) {
            Ok(text) => text,
            Err(_)
                if !bom
                    && content
                        .iter()
                        .filter(|&&c| c < 32 && ![9, 10, 13].contains(&c))
                        .count()
                        * 10
                        <= content.len() =>
            {
                let (text, had_errors) =
                    encoding_rs::WINDOWS_1252.decode_without_bom_handling(content);
                if had_errors {
                    return Err("O arquivo contém dados incompatíveis com Windows-1252.".into());
                }
                return Ok((text.into_owned(), Encoding::Windows1252));
            }
            Err(_) => {
                return Err(
                    "Este arquivo contém dados binários ou uma codificação não reconhecida.".into(),
                )
            }
        };
        if text.contains('\0') {
            return Err(
                "Este arquivo contém dados binários e não pode ser aberto como texto.".into(),
            );
        }
        Ok((
            text.to_owned(),
            if bom {
                Encoding::Utf8Bom
            } else {
                Encoding::Utf8
            },
        ))
    }

    fn decode_as(bytes: &[u8], encoding: Encoding) -> Result<(String, Encoding), String> {
        if bytes.starts_with(&[0xff, 0xfe, 0, 0]) || bytes.starts_with(&[0, 0, 0xfe, 0xff]) {
            return Err(
                "UTF-32 não é suportado. Converta o arquivo para UTF-8 ou UTF-16 antes de abrir."
                    .into(),
            );
        }
        let text = match encoding {
            Encoding::Utf8 | Encoding::Utf8Bom => {
                let content = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes);
                let text = std::str::from_utf8(content)
                    .map_err(|_| "O arquivo contém UTF-8 inválido.")?
                    .to_owned();
                let actual = if content.len() < bytes.len() {
                    Encoding::Utf8Bom
                } else {
                    Encoding::Utf8
                };
                if text.contains('\0') {
                    return Err(
                        "Este arquivo contém dados binários e não pode ser aberto como texto."
                            .into(),
                    );
                }
                return Ok((text, actual));
            }
            Encoding::Windows1252 => {
                let (text, errors) = encoding_rs::WINDOWS_1252.decode_without_bom_handling(bytes);
                if errors {
                    return Err("O arquivo contém dados incompatíveis com Windows-1252.".into());
                }
                text.into_owned()
            }
            _ => {
                let little = matches!(encoding, Encoding::Utf16Le | Encoding::Utf16LeNoBom);
                let bom = if little { &[0xff, 0xfe] } else { &[0xfe, 0xff] };
                let content = bytes.strip_prefix(bom).unwrap_or(bytes);
                if !content.len().is_multiple_of(2) {
                    return Err("Arquivo UTF-16 incompleto.".into());
                }
                let words: Vec<_> = content
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|b| {
                        if little {
                            u16::from_le_bytes([b[0], b[1]])
                        } else {
                            u16::from_be_bytes([b[0], b[1]])
                        }
                    })
                    .collect();
                let text =
                    String::from_utf16(&words).map_err(|_| "O arquivo contém UTF-16 inválido.")?;
                let actual = match (little, content.len() < bytes.len()) {
                    (true, true) => Encoding::Utf16Le,
                    (false, true) => Encoding::Utf16Be,
                    (true, false) => Encoding::Utf16LeNoBom,
                    (false, false) => Encoding::Utf16BeNoBom,
                };
                if text.contains('\0') {
                    return Err(
                        "Este arquivo contém dados binários e não pode ser aberto como texto."
                            .into(),
                    );
                }
                return Ok((text, actual));
            }
        };
        if text.contains('\0') {
            return Err(
                "Este arquivo contém dados binários e não pode ser aberto como texto.".into(),
            );
        }
        Ok((text, encoding))
    }

    pub fn dirty(&self) -> bool {
        self.text != self.saved_text
            || self.encoding != self.saved_encoding
            || self.line_ending != self.saved_line_ending
    }

    pub fn metadata_label(&self) -> Option<String> {
        self.disk_bytes.as_ref().map(|bytes| {
            let date = self
                .modified
                .map(|time| {
                    chrono::DateTime::<chrono::Local>::from(time)
                        .format("%d/%m/%Y %H:%M:%S")
                        .to_string()
                })
                .unwrap_or_default();
            let digits = bytes.len().to_string();
            let mut size = String::new();
            for (index, digit) in digits.chars().enumerate() {
                if index > 0 && (digits.len() - index).is_multiple_of(3) {
                    size.push('.');
                }
                size.push(digit);
            }
            format!("{date}   {size} bytes")
        })
    }

    pub fn encoding_label(&self) -> &'static str {
        self.encoding.label()
    }

    /// Only unchanged, loaded documents can establish byte equality from the
    /// retained disk snapshot. Equal visible text alone cannot prove it.
    pub fn same_saved_bytes(&self, other: &Self) -> bool {
        !self.dirty()
            && !other.dirty()
            && matches!((&self.disk_bytes,&other.disk_bytes),(Some(a),Some(b)) if a==b)
    }

    pub fn save(&mut self, path: &Path) -> Result<(), String> {
        if self
            .path
            .as_deref()
            .is_some_and(|original| same_file(original, path))
        {
            let current = fs::read(path)
                .map_err(|e| format!("Não foi possível verificar {}: {e}", path.display()))?;
            if self.disk_bytes.as_ref() != Some(&current) {
                return Err("O arquivo foi alterado no disco. Use Salvar como para preservar sua edição ou recarregue o arquivo.".into());
            }
            if !self.dirty() {
                return Ok(());
            }
        }
        let normalized = normalize_lines(&self.text).0;
        let text = self.output_text(&normalized);
        let mut bytes = Vec::new();
        match self.encoding {
            Encoding::Windows1252 => {
                let (encoded, _, errors) = encoding_rs::WINDOWS_1252.encode(&text);
                if errors {
                    return Err("O texto contém caracteres que não cabem em ANSI. Selecione UTF-8 antes de salvar.".into());
                }
                bytes.extend(encoded.as_ref());
            }
            Encoding::Utf8 | Encoding::Utf8Bom => {
                if self.encoding == Encoding::Utf8Bom {
                    bytes.extend([0xef, 0xbb, 0xbf]);
                }
                bytes.extend(text.as_bytes());
            }
            encoding => {
                let little = matches!(encoding, Encoding::Utf16Le | Encoding::Utf16LeNoBom);
                if matches!(encoding, Encoding::Utf16Le | Encoding::Utf16Be) {
                    bytes.extend(if little { [0xff, 0xfe] } else { [0xfe, 0xff] });
                }
                for word in text.encode_utf16() {
                    bytes.extend(if little {
                        word.to_le_bytes()
                    } else {
                        word.to_be_bytes()
                    });
                }
            }
        }
        // Save As of an untouched document must preserve its exact input bytes.
        if !self.dirty() {
            if let Some(original) = &self.disk_bytes {
                bytes.clone_from(original);
            }
        }
        atomic_write(path, &bytes)?;
        self.path = Some(path.to_owned());
        self.disk_bytes = Some(bytes);
        self.modified = fs::metadata(path)
            .ok()
            .and_then(|metadata| metadata.modified().ok());
        self.text = normalized;
        self.saved_text = self.text.clone();
        self.saved_encoding = self.encoding;
        self.saved_line_ending = self.line_ending;
        self.saved_endings = normalize_lines(&text).1;
        Ok(())
    }

    fn output_text(&self, normalized: &str) -> String {
        if self.line_ending != LineEnding::Mixed {
            return normalized.replace('\n', self.line_ending.text());
        }
        let default = self
            .saved_endings
            .first()
            .copied()
            .unwrap_or(LineEnding::Lf);
        let old: Vec<_> = self.saved_text.split_inclusive('\n').collect();
        let new: Vec<_> = normalized.split_inclusive('\n').collect();
        let mut endings = vec![default; new.len()];
        for op in similar::TextDiff::from_slices(&old, &new).ops() {
            let (_, old_range, new_range) = op.as_tag_tuple();
            for offset in 0..old_range.len().min(new_range.len()) {
                if let Some(&ending) = self.saved_endings.get(old_range.start + offset) {
                    endings[new_range.start + offset] = ending;
                }
            }
        }
        let mut output = String::with_capacity(normalized.len());
        for (line, ending) in new.iter().zip(endings) {
            if let Some(body) = line.strip_suffix('\n') {
                output.push_str(body);
                output.push_str(ending.text());
            } else {
                output.push_str(line);
            }
        }
        output
    }
}

pub fn same_file(a: &Path, b: &Path) -> bool {
    if a == b {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::Storage::FileSystem::{
            GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION,
        };
        if let (Ok(a), Ok(b)) = (fs::File::open(a), fs::File::open(b)) {
            let mut ia: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
            let mut ib: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
            let valid = unsafe {
                GetFileInformationByHandle(a.as_raw_handle(), &mut ia) != 0
                    && GetFileInformationByHandle(b.as_raw_handle(), &mut ib) != 0
            };
            if valid {
                return (ia.dwVolumeSerialNumber, ia.nFileIndexHigh, ia.nFileIndexLow)
                    == (ib.dwVolumeSerialNumber, ib.nFileIndexHigh, ib.nFileIndexLow);
            }
        }
    }
    matches!((fs::canonicalize(a), fs::canonicalize(b)), (Ok(a), Ok(b)) if a == b)
}

pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temporary = path.with_file_name(format!(".anycompare-{}-{stamp}.tmp", std::process::id()));
    let result = (|| -> std::io::Result<()> {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        if let Ok(metadata) = fs::metadata(path) {
            fs::set_permissions(&temporary, metadata.permissions())?;
        }
        fs::rename(&temporary, path)
    })();
    if let Err(error) = result {
        #[cfg(windows)]
        #[allow(clippy::permissions_set_readonly_false)]
        // Clears only the Windows read-only attribute on our temporary file.
        if let Ok(metadata) = fs::metadata(&temporary) {
            let mut permissions = metadata.permissions();
            permissions.set_readonly(false);
            let _ = fs::set_permissions(&temporary, permissions);
        }
        let _ = fs::remove_file(&temporary);
        return Err(format!(
            "Não foi possível salvar {}: {error}",
            path.display()
        ));
    }
    Ok(())
}

#[derive(Default)]
pub struct BinaryDocument {
    pub path: Option<PathBuf>,
    pub bytes: Vec<u8>,
    saved: Vec<u8>,
}
impl BinaryDocument {
    pub fn load(path: &Path) -> Result<Self, String> {
        let bytes = fs::read(path)
            .map_err(|e| format!("Não foi possível abrir {}: {e}", path.display()))?;
        Ok(Self {
            path: Some(path.into()),
            saved: bytes.clone(),
            bytes,
        })
    }
    pub fn dirty(&self) -> bool {
        self.bytes != self.saved
    }
    pub fn save(&mut self, path: &Path) -> Result<(), String> {
        if self
            .path
            .as_deref()
            .is_some_and(|original| same_file(original, path))
        {
            let current = fs::read(path).map_err(|e| e.to_string())?;
            if current != self.saved {
                return Err(
                    "O arquivo foi alterado no disco. Use Salvar como ou recarregue.".into(),
                );
            }
        }
        atomic_write(path, &self.bytes)?;
        self.path = Some(path.into());
        self.saved = self.bytes.clone();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_directory(name: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("anycompare-{name}-{}-{stamp}", std::process::id()));
        fs::create_dir(&dir).unwrap();
        dir
    }

    fn remove_test_directory(dir: &Path) {
        for entry in fs::read_dir(dir).unwrap() {
            fs::remove_file(entry.unwrap().path()).unwrap();
        }
        fs::remove_dir(dir).unwrap();
    }

    #[test]
    fn save_as_and_edits_preserve_encoding_bom_and_final_newline() {
        let dir = test_directory("encoding-roundtrip");
        for encoding in Encoding::ALL {
            for text in [
                "",
                "ação €",
                "ação €\r\nfim\r\n",
                "ação €\nfim",
                "ação €\rfim\r",
            ] {
                let input = match encoding {
                    Encoding::Windows1252 => encoding_rs::WINDOWS_1252.encode(text).0.into_owned(),
                    Encoding::Utf8 => text.as_bytes().to_vec(),
                    Encoding::Utf8Bom => [b"\xef\xbb\xbf".as_slice(), text.as_bytes()].concat(),
                    _ => {
                        let little = matches!(encoding, Encoding::Utf16Le | Encoding::Utf16LeNoBom);
                        let mut bytes = match encoding {
                            Encoding::Utf16Le => vec![0xff, 0xfe],
                            Encoding::Utf16Be => vec![0xfe, 0xff],
                            _ => vec![],
                        };
                        for word in text.encode_utf16() {
                            bytes.extend(if little {
                                word.to_le_bytes()
                            } else {
                                word.to_be_bytes()
                            });
                        }
                        bytes
                    }
                };
                let source = dir.join("source.txt");
                let copy = dir.join("copy.txt");
                fs::write(&source, &input).unwrap();
                let mut document = Document::load_with_encoding(&source, Some(encoding)).unwrap();
                assert!(!document.dirty());
                document.save(&copy).unwrap();
                assert_eq!(
                    fs::read(&copy).unwrap(),
                    input,
                    "untouched {encoding:?} {text:?}"
                );
                document.text.push('!');
                document.save(&copy).unwrap();
                let saved = Document::load_with_encoding(&copy, Some(encoding)).unwrap();
                assert_eq!(saved.text, format!("{}!", normalize_lines(text).0));
                assert_eq!(saved.encoding, encoding);
                assert!(!document.dirty());
                assert_eq!(fs::read(&source).unwrap(), input);
            }
        }
        remove_test_directory(&dir);
    }

    #[test]
    fn mixed_endings_survive_edits_insertions_and_save_as() {
        let dir = test_directory("mixed-endings");
        let source = dir.join("source.txt");
        let copy = dir.join("copy.txt");
        fs::write(&source, b"one\r\ntwo\nthree\rfour").unwrap();
        let mut doc = Document::load(&source).unwrap();
        assert_eq!(doc.line_ending, LineEnding::Mixed);
        assert_eq!(doc.text, "one\ntwo\nthree\nfour");
        doc.save(&copy).unwrap();
        assert_eq!(fs::read(&copy).unwrap(), fs::read(&source).unwrap());
        doc.text = "one\nTWO\ninserted\nthree\nfour".into();
        doc.save(&copy).unwrap();
        assert_eq!(
            fs::read(&copy).unwrap(),
            b"one\r\nTWO\ninserted\r\nthree\rfour"
        );
        doc.line_ending = LineEnding::Lf;
        assert!(doc.dirty());
        doc.save(&copy).unwrap();
        assert_eq!(fs::read(&copy).unwrap(), b"one\nTWO\ninserted\nthree\nfour");
        remove_test_directory(&dir);
    }

    #[test]
    fn invalid_unicode_and_unrepresentable_ansi_never_overwrite_input() {
        for invalid in [
            &[0xff, 0xfe, 0, 0, 0x61, 0, 0, 0][..],
            &[0, 0, 0xfe, 0xff, 0, 0, 0, 0x61],
            &[0xff, 0xfe, 0, 0xd8],
            &[0xef, 0xbb, 0xbf, 0xff],
            &[0xff, 0xfe, 0, 0],
        ] {
            assert!(Document::decode(invalid).is_err(), "{invalid:?}");
        }
        let dir = test_directory("ansi-loss");
        let path = dir.join("ansi.txt");
        fs::write(&path, b"a\xe7\xe3o").unwrap();
        let mut doc = Document::load(&path).unwrap();
        doc.text.push('🙂');
        assert!(doc.save(&path).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"a\xe7\xe3o");
        assert!(doc.dirty());
        doc.encoding = Encoding::Utf8;
        doc.save(&path).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "ação🙂");
        remove_test_directory(&dir);
    }

    #[test]
    fn forced_decoding_and_pasted_crlf_remain_lossless() {
        let dir = test_directory("forced-decoding");
        let path = dir.join("ambiguous.txt");
        fs::write(&path, "ação").unwrap();
        let utf8 = Document::load(&path).unwrap();
        assert_eq!(utf8.text, "ação");
        let ansi = Document::load_with_encoding(&path, Some(Encoding::Windows1252)).unwrap();
        assert_eq!(ansi.text, "aÃ§Ã£o");
        let mut doc = utf8;
        doc.text.push_str("\r\ncolado\r\n🙂");
        doc.line_ending = LineEnding::CrLf;
        doc.save(&path).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "ação\r\ncolado\r\n🙂");
        assert_eq!(doc.text, "ação\ncolado\n🙂");
        remove_test_directory(&dir);
    }

    #[test]
    fn file_aliases_cannot_bypass_external_edit_protection() {
        let dir = test_directory("alias-conflict");
        let path = dir.join("source.txt");
        let alias = dir.join("alias.txt");
        fs::write(&path, b"original").unwrap();
        fs::hard_link(&path, &alias).unwrap();
        let mut text = Document::load(&path).unwrap();
        let mut binary = BinaryDocument::load(&path).unwrap();
        fs::write(&path, b"external").unwrap();
        text.text.push('!');
        binary.bytes.push(1);
        assert!(text.save(&alias).is_err());
        assert!(binary.save(&alias).is_err());
        #[cfg(windows)]
        {
            let upper = PathBuf::from(path.to_string_lossy().to_uppercase());
            assert!(text.save(&upper).is_err());
            assert!(binary.save(&upper).is_err());
        }
        assert_eq!(fs::read(&path).unwrap(), b"external");
        assert_eq!(fs::read(&alias).unwrap(), b"external");
        remove_test_directory(&dir);
    }

    #[test]
    fn unicode_decoding_rejects_binary_and_broken_utf16() {
        assert!(Document::decode(b"MZ\0binary").is_err());
        assert!(Document::decode(&[0xff, 0xfe, 0x61]).is_err());
        assert_eq!(
            Document::decode(&[0xfe, 0xff, 0, 0x61]).unwrap(),
            ("a".into(), Encoding::Utf16Be)
        );
        assert_eq!(
            Document::decode(b"a\xe7\xe3o").unwrap(),
            ("ação".into(), Encoding::Windows1252)
        );
    }

    #[test]
    fn saving_preserves_utf16_crlf_and_detects_external_edits() {
        let path = std::env::temp_dir().join(format!(
            "anycompare-document-test-{}.txt",
            std::process::id()
        ));
        let original: Vec<u8> = [0xff, 0xfe]
            .into_iter()
            .chain("olá\r\n".encode_utf16().flat_map(u16::to_le_bytes))
            .collect();
        fs::write(&path, &original).unwrap();
        let mut doc = Document::load(&path).unwrap();
        assert_eq!(doc.text, "olá\n");
        doc.text.push_str("fim\n");
        assert!(doc.dirty());
        doc.save(&path).unwrap();
        assert!(!doc.dirty());
        let saved = fs::read(&path).unwrap();
        assert_eq!(Document::decode(&saved).unwrap().0, "olá\r\nfim\r\n");
        fs::write(&path, b"external change").unwrap();
        doc.text.push('!');
        assert!(doc.save(&path).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"external change");
        fs::remove_file(path).unwrap();
    }
}
