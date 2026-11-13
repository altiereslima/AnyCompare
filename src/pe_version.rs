//! Bounded, read-only PE/version reader. Never calls LoadLibrary or executes a DLL.
use chrono::{Local, TimeZone};
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, ops::Range, path::Path, sync::Arc};

#[derive(Clone, Debug)]
pub struct Node {
    pub key: String,
    pub name: String,
    pub value: String,
    pub bytes: Option<Arc<[u8]>>,
    pub children: Vec<Node>,
}
impl Node {
    fn field(key: &str, name: &str, value: impl ToString) -> Self {
        Self {
            key: key.into(),
            name: name.into(),
            value: value.to_string(),
            bytes: None,
            children: Vec::new(),
        }
    }
    fn group(key: &str, name: &str, children: Vec<Node>) -> Self {
        Self {
            children,
            ..Self::field(key, name, "")
        }
    }
    pub fn same(&self, other: &Self) -> bool {
        self.value == other.value && self.bytes == other.bytes
    }
}
#[derive(Clone, Debug)]
pub struct Image {
    pub nodes: Vec<Node>,
    pub size: usize,
    pub sha256: [u8; 32],
    modified: Option<std::time::SystemTime>,
    pub path: Option<std::path::PathBuf>,
}
fn range(bytes: &[u8], at: usize, len: usize) -> Result<&[u8], String> {
    bytes
        .get(at..at.checked_add(len).ok_or("Tamanho PE inválido.")?)
        .ok_or_else(|| "Arquivo PE truncado ou deslocamento inválido.".into())
}
fn word(bytes: &[u8], at: usize) -> Result<u16, String> {
    Ok(u16::from_le_bytes(range(bytes, at, 2)?.try_into().unwrap()))
}
fn dword(bytes: &[u8], at: usize) -> Result<u32, String> {
    Ok(u32::from_le_bytes(range(bytes, at, 4)?.try_into().unwrap()))
}
fn qword(bytes: &[u8], at: usize) -> Result<u64, String> {
    Ok(u64::from_le_bytes(range(bytes, at, 8)?.try_into().unwrap()))
}
fn align4(at: usize) -> usize {
    (at + 3) & !3
}
fn number(n: usize) -> String {
    let mut result = String::new();
    for (i, c) in n.to_string().chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push('.');
        }
        result.push(c);
    }
    result.chars().rev().collect()
}

struct Section {
    rva: u32,
    virtual_size: u32,
    raw: usize,
    size: usize,
}
fn offset(
    rva: u32,
    size: usize,
    headers: usize,
    sections: &[Section],
    bytes: &[u8],
) -> Result<usize, String> {
    if (rva as usize) < headers {
        range(bytes, rva as usize, size)?;
        return Ok(rva as usize);
    }
    for s in sections {
        if let Some(delta) = rva
            .checked_sub(s.rva)
            .filter(|&d| d < (s.size as u32).max(s.virtual_size))
        {
            if (delta as usize)
                .checked_add(size)
                .is_some_and(|end| end <= s.size)
            {
                let at = s.raw.checked_add(delta as usize).ok_or("RVA inválido.")?;
                range(bytes, at, size)?;
                return Ok(at);
            }
        }
    }
    Err("Recurso PE não está mapeado em bytes do arquivo.".into())
}
fn resources(
    bytes: &[u8],
    root: usize,
    size: usize,
    relative: usize,
    depth: usize,
    stack: &mut BTreeSet<usize>,
    out: &mut Vec<(u32, usize)>,
) -> Result<(), String> {
    if depth > 8 || !stack.insert(relative) {
        return Err("Diretório de recursos PE cíclico ou muito profundo.".into());
    }
    if relative.checked_add(16).is_none_or(|end| end > size) {
        return Err("Diretório de recursos fora dos limites.".into());
    }
    let at = root + relative;
    let count = word(bytes, at + 12)? as usize + word(bytes, at + 14)? as usize;
    if relative
        .checked_add(16 + count * 8)
        .is_none_or(|end| end > size)
    {
        return Err("Tabela de recursos PE truncada.".into());
    }
    for i in 0..count {
        let key = dword(bytes, at + 16 + i * 8)?;
        let value = dword(bytes, at + 20 + i * 8)?;
        if depth == 0 && key != 16 {
            continue;
        }
        let child = (value & 0x7fff_ffff) as usize;
        if value & 0x8000_0000 != 0 {
            resources(bytes, root, size, child, depth + 1, stack, out)?;
        } else {
            if child.checked_add(16).is_none_or(|end| end > size) {
                return Err("Entrada de recurso PE inválida.".into());
            }
            out.push((
                dword(bytes, root + child)?,
                dword(bytes, root + child + 4)? as usize,
            ));
        }
    }
    stack.remove(&relative);
    Ok(())
}
struct Block<'a> {
    key: String,
    value: &'a [u8],
    children: Vec<Block<'a>>,
}
fn block(
    data: &[u8],
    start: usize,
    limit: usize,
    depth: usize,
) -> Result<(Block<'_>, usize), String> {
    if depth > 16 {
        return Err("Recurso de versão muito profundo.".into());
    }
    let len = word(data, start)? as usize;
    if len < 6 || start.checked_add(len).is_none_or(|end| end > limit) {
        return Err("Bloco de versão truncado.".into());
    }
    let end = start + len;
    let value_len = word(data, start + 2)? as usize;
    let kind = word(data, start + 4)?;
    let mut pos = start + 6;
    let mut key = Vec::new();
    loop {
        if pos + 2 > end {
            return Err("Nome do campo de versão não terminado.".into());
        }
        let c = word(data, pos)?;
        pos += 2;
        if c == 0 {
            break;
        }
        key.push(c);
    }
    let key = String::from_utf16(&key).map_err(|_| "Nome de campo UTF-16 inválido.")?;
    pos = align4(pos);
    let byte_len = if kind == 1 { value_len * 2 } else { value_len };
    if pos.checked_add(byte_len).is_none_or(|p| p > end) {
        return Err("Valor do recurso de versão fora dos limites.".into());
    }
    let value = range(data, pos, byte_len)?;
    pos = align4(pos + byte_len);
    let mut children = Vec::new();
    while pos + 6 <= end {
        if word(data, pos)? == 0 {
            break;
        }
        let (child, next) = block(data, pos, end, depth + 1)?;
        children.push(child);
        pos = align4(next);
    }
    Ok((
        Block {
            key,
            value,
            children,
        },
        end,
    ))
}
fn text(bytes: &[u8]) -> Result<String, String> {
    if !bytes.len().is_multiple_of(2) {
        return Err("Valor de versão UTF-16 truncado.".into());
    }
    let words: Vec<_> = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
        .take_while(|&c| c != 0)
        .collect();
    String::from_utf16(&words).map_err(|_| "Texto UTF-16 inválido no recurso de versão.".into())
}
fn version(bytes: &[u8], at: usize) -> Result<String, String> {
    let hi = dword(bytes, at)?;
    let lo = dword(bytes, at + 4)?;
    Ok(format!(
        "{}.{}.{}.{}",
        hi >> 16,
        hi & 65535,
        lo >> 16,
        lo & 65535
    ))
}
fn flags(bits: u32, values: &[(u32, &str)]) -> String {
    let mut names = Vec::new();
    let mut known = 0;
    for &(bit, name) in values {
        known |= bit;
        if bits & bit != 0 {
            names.push(crate::i18n::tr(name));
        }
    }
    if bits & !known != 0 {
        names.push(format!("0x{:X}", bits & !known));
    }
    names.join(", ")
}
fn language(id: u16, codepage: u16) -> String {
    use windows_sys::Win32::Globalization::{
        GetLocaleInfoW, LOCALE_SENGLISHDISPLAYNAME, LOCALE_SLOCALIZEDDISPLAYNAME,
    };
    let mut buf = [0u16; 256];
    let field = if crate::i18n::language().starts_with("en") {
        LOCALE_SENGLISHDISPLAYNAME
    } else {
        LOCALE_SLOCALIZEDDISPLAYNAME
    };
    let count = unsafe { GetLocaleInfoW(id as u32, field, buf.as_mut_ptr(), buf.len() as i32) };
    let name = if count > 1 {
        String::from_utf16_lossy(&buf[..count as usize - 1])
    } else {
        format!("0x{id:04X}")
    };
    format!(
        "{name}, {}",
        if codepage == 1200 {
            "UTF-16LE".into()
        } else {
            format!("CP{codepage}")
        }
    )
}
fn value_label(key: &str) -> &str {
    match key {
        "CompanyName" => "Nome da Companhia",
        "FileDescription" => "Descrição do Arquivo",
        "FileVersion" => "Versão do Arquivo",
        "InternalName" => "Nome Interno",
        "LegalCopyright" => "Copyright Legal",
        "OriginalFilename" => "Nome Original do Arquivo",
        "ProductName" => "Nome do Produto",
        "ProductVersion" => "Versão do Produto",
        "Comments" => "Comentários",
        "LegalTrademarks" => "Marcas Legais",
        "PrivateBuild" => "Compilação Privada",
        "SpecialBuild" => "Compilação Especial",
        _ => key,
    }
}

impl Image {
    /// Shared property-view container; Registry/Media use their own readers,
    /// never the PE parser. Nodes retain binary type/value identity.
    pub fn properties(path: &Path, nodes: Vec<Node>, size: usize, sha256: [u8; 32]) -> Self {
        Self {
            path: Some(path.to_path_buf()),
            nodes,
            size,
            sha256,
            modified: std::fs::metadata(path).ok().and_then(|m| m.modified().ok()),
        }
    }
    pub fn load(path: &Path) -> Result<Self, String> {
        let mut image =
            Self::parse(&std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?)?;
        image.modified = std::fs::metadata(path).ok().and_then(|m| m.modified().ok());
        image.path = Some(path.to_path_buf());
        Ok(image)
    }
    pub fn metadata_label(&self) -> String {
        let date = self
            .modified
            .map(|d| {
                chrono::DateTime::<Local>::from(d)
                    .format("%d/%m/%Y %H:%M:%S")
                    .to_string()
            })
            .unwrap_or_default();
        format!("{date}  {} bytes", number(self.size))
    }
    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        if range(bytes, 0, 2)? != b"MZ" {
            return Err("Este arquivo não é um executável PE do Windows.".into());
        }
        let pe = dword(bytes, 60)? as usize;
        if range(bytes, pe, 4)? != b"PE\0\0" {
            return Err("Assinatura PE inválida.".into());
        }
        let coff = pe + 4;
        let optional = pe + 24;
        let optional_size = word(bytes, coff + 16)? as usize;
        let magic = word(bytes, optional)?;
        let wide = match magic {
            0x10b => false,
            0x20b => true,
            _ => return Err("Cabeçalho opcional PE não suportado.".into()),
        };
        let directories = if wide { 112 } else { 96 };
        if optional_size < directories {
            return Err("Cabeçalho PE truncado.".into());
        }
        range(bytes, optional, optional_size)?;
        let count = word(bytes, coff + 2)? as usize;
        range(bytes, optional + optional_size, count * 40)?;
        let mut sections = Vec::new();
        for i in 0..count {
            let at = optional + optional_size + i * 40;
            sections.push(Section {
                virtual_size: dword(bytes, at + 8)?,
                rva: dword(bytes, at + 12)?,
                size: dword(bytes, at + 16)? as usize,
                raw: dword(bytes, at + 20)? as usize,
            });
        }
        let headers = dword(bytes, optional + 60)? as usize;
        let mut version_resources = Vec::new();
        if dword(bytes, optional + directories - 4)? > 2 && optional_size >= directories + 24 {
            let rva = dword(bytes, optional + directories + 16)?;
            let size = dword(bytes, optional + directories + 20)? as usize;
            if rva != 0 && size != 0 {
                let root = offset(rva, size, headers, &sections, bytes)?;
                resources(
                    bytes,
                    root,
                    size,
                    0,
                    0,
                    &mut BTreeSet::new(),
                    &mut version_resources,
                )?;
            }
        }
        let mut excluded: Vec<Range<usize>> =
            std::iter::once(0..optional + optional_size).collect();
        let mut fixed = Vec::new();
        let mut translations = Vec::new();
        let mut variable = Vec::new();
        for (index, (rva, size)) in version_resources.into_iter().enumerate() {
            let start = offset(rva, size, headers, &sections, bytes)?;
            let data = range(bytes, start, size)?;
            excluded.push(start..start + size);
            let (root, _) = block(data, 0, data.len(), 0)?;
            if root.key != "VS_VERSION_INFO" {
                return Err("Raiz de versão inválida.".into());
            }
            if !root.value.is_empty() {
                if root.value.len() < 52 || dword(root.value, 0)? != 0xFEEF04BD {
                    return Err("Assinatura VS_FIXEDFILEINFO inválida.".into());
                }
                let v = root.value;
                let os = dword(v, 32)?;
                let kind = dword(v, 36)?;
                fixed.push(Node::group(
                    &format!("fixed/{index}"),
                    "Fixo",
                    vec![
                        Node::field("file-version", "Versão do Arquivo", version(v, 8)?),
                        Node::field("product-version", "Versão do Produto", version(v, 16)?),
                        Node::field(
                            "os",
                            "Sistema Operacional",
                            match os & 65535 {
                                1 => "Windows 16-bit".into(),
                                4 => "Windows 32-bit".into(),
                                _ => format!("0x{os:08X}"),
                            },
                        ),
                        Node::field(
                            "type",
                            "Tipo de Arquivo",
                            match kind {
                                1 => crate::i18n::tr("Executável"),
                                2 => "DLL".into(),
                                3 => "Driver".into(),
                                4 => crate::i18n::tr("Fonte"),
                                7 => crate::i18n::tr("Biblioteca estática"),
                                _ => format!("0x{kind:08X}"),
                            },
                        ),
                    ],
                ));
                if dword(v, 28)? != 0 {
                    fixed.last_mut().unwrap().children.push(Node::field(
                        "flags",
                        "Marcas",
                        format!("0x{:08X}", dword(v, 28)?),
                    ));
                }
            }
            for child in root.children {
                if child.key == "VarFileInfo" {
                    for entry in child.children {
                        if entry.key == "Translation" {
                            for (j, part) in entry.value.as_chunks::<4>().0.iter().enumerate() {
                                translations.push(Node::field(
                                    &format!("{index}/{j}"),
                                    "Idiomas",
                                    language(word(part, 0)?, word(part, 2)?),
                                ));
                            }
                        }
                    }
                } else if child.key == "StringFileInfo" {
                    for (j, table) in child.children.into_iter().enumerate() {
                        let code = u32::from_str_radix(&table.key, 16).unwrap_or(0);
                        let mut values = vec![Node::field(
                            "language",
                            "Idioma",
                            language((code >> 16) as u16, code as u16),
                        )];
                        for (k, entry) in table.children.into_iter().enumerate() {
                            values.push(Node::field(
                                &format!("{}/{}", entry.key, k),
                                value_label(&entry.key),
                                text(entry.value)?,
                            ));
                        }
                        variable.push(Node::group(
                            &format!("variable/{index}/{j}"),
                            "Variável",
                            values,
                        ));
                    }
                }
            }
        }
        excluded.sort_by_key(|r| r.start);
        let mut code = Vec::new();
        let mut cursor = 0;
        for Range { start, end } in excluded {
            if start > cursor {
                code.extend_from_slice(&bytes[cursor..start]);
            }
            cursor = cursor.max(end);
        }
        code.extend_from_slice(&bytes[cursor..]);
        let mut code_node = Node::field("data", "Dados", format!("{} bytes", number(code.len())));
        code_node.bytes = Some(code.into());
        let machine = word(bytes, coff)?;
        let stamp = dword(bytes, coff + 4)?;
        let subsystem = word(bytes, optional + 68)?;
        let header = vec![
            Node::field(
                "os",
                "Sistema Operacional",
                format!(
                    "Windows NT {}.{}",
                    word(bytes, optional + 40)?,
                    word(bytes, optional + 42)?
                ),
            ),
            Node::field(
                "machine",
                "Processador",
                match machine {
                    0x14c => "I386".into(),
                    0x8664 => "AMD64".into(),
                    0xaa64 => "ARM64".into(),
                    0x1c4 => "ARMNT".into(),
                    _ => format!("0x{machine:04X}"),
                },
            ),
            Node::field(
                "timestamp",
                "Data de Criação",
                Local.timestamp_opt(stamp as i64, 0).single().map_or_else(
                    || stamp.to_string(),
                    |d| d.format("%d/%m/%Y %H:%M:%S").to_string(),
                ),
            ),
            Node::field(
                "flags",
                "Marcas",
                flags(
                    word(bytes, coff + 18)? as u32,
                    &[
                        (2, "Executável"),
                        (0x20, "Endereços maiores que 2 GB"),
                        (0x100, "Palavras de 32 bits"),
                        (0x2000, "DLL"),
                    ],
                ),
            ),
            Node::field(
                "linker",
                "Versão do Vinculador",
                format!("{}.{}", bytes[optional + 2], bytes[optional + 3]),
            ),
            Node::field(
                "base",
                "Base de Imagem",
                format!(
                    "0x{:08X}",
                    if wide {
                        qword(bytes, optional + 24)?
                    } else {
                        dword(bytes, optional + 28)? as u64
                    }
                ),
            ),
            Node::field(
                "image",
                "Versão da Imagem",
                format!(
                    "{}.{}",
                    word(bytes, optional + 44)?,
                    word(bytes, optional + 46)?
                ),
            ),
            Node::field(
                "checksum",
                "Soma de Verificação",
                format!("0x{:08X}", dword(bytes, optional + 64)?),
            ),
            Node::field(
                "ui",
                "Interface do Usuário",
                format!(
                    "{} {}.{}",
                    match subsystem {
                        2 => "Windows GUI",
                        3 => "Windows console",
                        1 => "Native",
                        _ => "Other",
                    },
                    word(bytes, optional + 48)?,
                    word(bytes, optional + 50)?
                ),
            ),
            Node::field(
                "dll-flags",
                "Sinalizadores DLL",
                flags(
                    word(bytes, optional + 70)? as u32,
                    &[
                        (0x20, "Alta entropia VA"),
                        (0x40, "Base dinâmica"),
                        (0x80, "Integridade forçada"),
                        (0x100, "Compatível com NX"),
                        (0x200, "Sem isolamento"),
                        (0x400, "Sem SEH"),
                        (0x800, "Sem bind"),
                        (0x1000, "AppContainer"),
                        (0x2000, "Driver WDM"),
                        (0x4000, "Proteção de fluxo de controle"),
                        (0x8000, "Terminal Server"),
                    ],
                ),
            ),
        ];
        let mut nodes = vec![
            Node::group("code", "Código", vec![code_node]),
            Node::group("pe", "Cabeçalho PE", header),
        ];
        nodes.extend(fixed);
        if !translations.is_empty() {
            nodes.push(Node::group("translations", "Tradução", translations));
        }
        nodes.extend(variable);
        // Covers fields not yet individually rendered, without hiding header-only changes.
        let header_sha = format!("{:x}", Sha256::digest(&bytes[..optional + optional_size]));
        nodes.push(Node::group(
            "additional",
            "Cabeçalho completo",
            vec![Node::field("sha256", "SHA-256", header_sha)],
        ));
        Ok(Self {
            nodes,
            size: bytes.len(),
            sha256: Sha256::digest(bytes).into(),
            modified: None,
            path: None,
        })
    }
}

pub fn supported(path: &Path) -> bool {
    matches!(
        path.extension()
            .unwrap_or_default()
            .to_string_lossy()
            .to_ascii_lowercase()
            .as_str(),
        "dll" | "exe" | "ocx" | "sys" | "cpl" | "scr" | "mui"
    ) && std::fs::File::open(path)
        .and_then(|mut f| {
            use std::io::Read;
            let mut mz = [0; 2];
            f.read_exact(&mut mz)?;
            Ok(mz == *b"MZ")
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_short_and_malformed_images_without_panicking() {
        for len in 0..1000 {
            let mut bytes = vec![0; len];
            if len > 1 {
                bytes[..2].copy_from_slice(b"MZ");
            }
            assert!(Image::parse(&bytes).is_err());
        }
    }
    #[test]
    fn embedded_executable_has_version_metadata_and_raw_code() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/bc5/pe32.bin");
        let image = Image::load(&path).unwrap();
        assert!(!image.nodes[0].children[0]
            .bytes
            .as_ref()
            .unwrap()
            .is_empty());
        assert!(image.nodes.iter().any(|n| n.name == "Fixo"));
    }
    #[test]
    fn code_headers_and_version_values_are_compared_independently() {
        let a = Image::parse(include_bytes!("../tests/fixtures/bc5/pe32.bin")).unwrap();
        let b = Image::parse(include_bytes!("../tests/fixtures/bc5/pe32-changed.bin")).unwrap();
        assert!(a.nodes[0].children[0].same(&b.nodes[0].children[0]));
        assert_ne!(a.sha256, b.sha256);
        let strings = a.nodes.iter().find(|n| n.name == "Variável").unwrap();
        assert!(strings.children.iter().any(|n| n.value == "AnyCompare"));
        let fixed = a.nodes.iter().find(|n| n.name == "Fixo").unwrap();
        assert_eq!(fixed.children[0].value, "1.2.3.4");
        let wide = Image::parse(include_bytes!("../tests/fixtures/bc5/pe64.bin")).unwrap();
        assert_eq!(wide.nodes[1].children[1].value, "AMD64");
        let mut changed = include_bytes!("../tests/fixtures/bc5/pe32.bin").to_vec();
        *changed.last_mut().unwrap() = 1;
        let changed = Image::parse(&changed).unwrap();
        assert!(!a.nodes[0].children[0].same(&changed.nodes[0].children[0]));
        let mut bad = include_bytes!("../tests/fixtures/bc5/pe32.bin").to_vec();
        bad[532..536].copy_from_slice(&0x80000000u32.to_le_bytes());
        assert!(Image::parse(&bad).is_err());
    }
    #[test]
    #[ignore = "read-only verification of the DLL supplied by the user"]
    fn reference_vcruntime140_fields() {
        let image = Image::load(Path::new(
            "C:/Projetos/Into the Breach/Jogo/vcruntime140.dll",
        ))
        .unwrap();
        assert_eq!(image.size, 83784);
        assert_eq!(
            image.nodes[0].children[0].bytes.as_ref().unwrap().len(),
            82356
        );
        let fixed = image.nodes.iter().find(|n| n.name == "Fixo").unwrap();
        assert_eq!(fixed.children[0].value, "14.0.24210.0");
        let strings = image.nodes.iter().find(|n| n.name == "Variável").unwrap();
        assert!(strings
            .children
            .iter()
            .any(|n| n.value == "Microsoft Corporation"));
    }
}
