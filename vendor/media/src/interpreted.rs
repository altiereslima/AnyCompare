//! Camada de DIAGNÓSTICO sobre o observador RAW (não é o algoritmo BC5).
//!
//! Nunca elimina ocorrências, reordena campos ou escolhe importância.
//! As conversões abaixo são interpretações dos formatos ID3 / Vorbis / MP4;
//! o comportamento exato de exibição/igualdade do BC5 exige relatórios GOLDEN.
use crate::forensic::{
    observe_media_bytes, observe_media_raw, read_bounded, RawMediaObservation, RawMediaRecord,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterpretedTag {
    pub source: String,
    pub raw_key: String,
    /// Para MP4 freeform pode incluir `mean` e `name`; somente diagnóstico.
    pub diagnostic_key: String,
    pub occurrence: usize,
    pub raw_blake3: String,
    pub encoding: String,
    /// Valores na ordem de origem; NÃO utilizados como igualdade BC5.
    pub values: Vec<String>,
    pub binary: bool,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterpretedMediaObservation {
    pub file: String,
    pub raw_records: usize,
    pub tags: Vec<InterpretedTag>,
    pub warnings: Vec<String>,
    /// Sempre false: decodificar bytes não valida o comportamento do BC5.
    pub matches_bc5: bool,
}

fn invalid(msg: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg)
}
fn u16be(p: &[u8]) -> Option<u16> {
    Some(u16::from_be_bytes(p.get(..2)?.try_into().ok()?))
}
fn u32be(p: &[u8]) -> Option<u32> {
    Some(u32::from_be_bytes(p.get(..4)?.try_into().ok()?))
}

fn decode_utf16(bytes: &[u8], little: bool) -> Result<String, String> {
    if bytes.len() % 2 != 0 {
        return Err("UTF-16 com número ímpar de bytes".into());
    }
    let units: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|p| {
            if little {
                u16::from_le_bytes([p[0], p[1]])
            } else {
                u16::from_be_bytes([p[0], p[1]])
            }
        })
        .collect();
    String::from_utf16(&units).map_err(|_| "UTF-16 inválido".into())
}

fn id3_decode(enc: u8, payload: &[u8]) -> Result<(String, String), String> {
    match enc {
        0 => Ok((
            payload.iter().map(|b| char::from(*b)).collect(),
            "ISO-8859-1".into(),
        )),
        1 => {
            if payload.len() < 2 {
                return Err("UTF-16 sem BOM".into());
            }
            match &payload[..2] {
                [0xff, 0xfe] => {
                    decode_utf16(&payload[2..], true).map(|s| (s, "UTF-16LE BOM".into()))
                }
                [0xfe, 0xff] => {
                    decode_utf16(&payload[2..], false).map(|s| (s, "UTF-16BE BOM".into()))
                }
                _ => Err("ID3 UTF-16 sem BOM; não inferido endian".into()),
            }
        }
        2 => decode_utf16(payload, false).map(|s| (s, "UTF-16BE".into())),
        3 => std::str::from_utf8(payload)
            .map(|s| (s.to_string(), "UTF-8".into()))
            .map_err(|_| "sequência UTF-8 inválida".into()),
        _ => Err(format!(
            "ID3: indicação de codificação desconhecida ({enc})"
        )),
    }
}

fn split_id3_text(v: String) -> Vec<String> {
    // A final string terminator is not another field. Preserve interior empty
    // values and duplicates; the RAW observation still retains every byte.
    // ID3v2.4 native frames 4.2, e.g. TLAN/TCON terminated lists.
    v.strip_suffix('\0')
        .unwrap_or(&v)
        .split('\0')
        .map(ToOwned::to_owned)
        .collect()
}

fn id3_description_body(
    enc: u8,
    bytes: &[u8],
    prefix_len: usize,
) -> Result<(String, Vec<String>, String), String> {
    let all = bytes
        .get(prefix_len..)
        .ok_or("frame ID3 composto truncado")?;
    // Nos quadros UTF-16, delimitações precisam cair em limites de code unit.
    // O segundo texto pode reutilizar o endian do BOM da descrição.
    if enc == 1 || enc == 2 {
        let (little, data, codepage) = match enc {
            2 => (false, all, "UTF-16BE".to_owned()),
            1 if all.starts_with(&[0xff, 0xfe]) => (true, &all[2..], "UTF-16LE BOM".to_owned()),
            1 if all.starts_with(&[0xfe, 0xff]) => (false, &all[2..], "UTF-16BE BOM".to_owned()),
            _ => return Err("ID3 composto UTF-16 sem BOM".into()),
        };
        let sep = (0..data.len() / 2)
            .find(|i| data[2 * i] == 0 && data[2 * i + 1] == 0)
            .map(|i| 2 * i);
        let (description, value) = if let Some(p) = sep {
            let desc = decode_utf16(&data[..p], little)?;
            let mut value = &data[p + 2..];
            let mut val_little = little;
            if value.starts_with(&[0xff, 0xfe]) {
                val_little = true;
                value = &value[2..];
            } else if value.starts_with(&[0xfe, 0xff]) {
                val_little = false;
                value = &value[2..];
            }
            (desc, decode_utf16(value, val_little)?)
        } else {
            (String::new(), decode_utf16(data, little)?)
        };
        return Ok((description, split_id3_text(value), codepage));
    }
    let sep = all.iter().position(|b| *b == 0);
    let (description, value, codepage) = if let Some(p) = sep {
        let (desc, c) = id3_decode(enc, &all[..p])?;
        let (val, _) = id3_decode(enc, &all[p + 1..])?;
        (desc, val, c)
    } else {
        let (val, c) = id3_decode(enc, all)?;
        (String::new(), val, c)
    };
    Ok((description, split_id3_text(value), codepage))
}

/// ID3v2 WXXX/WXX: o byte de encoding vale apenas para a DESCRIÇÃO.
/// Segundo a estrutura do frame, a URL após o terminador é uma sequência
/// de bytes ISO-8859-1, mesmo quando a descrição usa UTF-16/UTF-8.
/// A interpretação continua sendo diagnóstica, não uma regra BC5.
fn decode_id3_user_url(enc: u8, frame: &[u8]) -> Result<(String, String, String), String> {
    let src = frame.get(1..).ok_or("WXXX sem byte de codificação")?;
    let (descr, url): (String, &[u8]) = match enc {
        0 | 3 => {
            let end = src
                .iter()
                .position(|b| *b == 0)
                .ok_or("WXXX sem terminador da descrição")?;
            (id3_decode(enc, &src[..end])?.0, &src[end + 1..])
        }
        1 | 2 => {
            let (little, text, start) = match enc {
                2 => (false, src, 0usize),
                1 if src.starts_with(&[0xff, 0xfe]) => (true, &src[2..], 2usize),
                1 if src.starts_with(&[0xfe, 0xff]) => (false, &src[2..], 2usize),
                _ => return Err("WXXX UTF-16 sem BOM".into()),
            };
            let sep = (0..text.len() / 2)
                .find(|i| text[2 * i] == 0 && text[2 * i + 1] == 0)
                .map(|i| 2 * i)
                .ok_or("WXXX sem terminador UTF-16 da descrição")?;
            let desc = decode_utf16(&text[..sep], little)?;
            (desc, &src[start + sep + 2..])
        }
        _ => return Err(format!("WXXX com codificação inválida {enc}")),
    };
    let url_value: String = url.iter().map(|b| char::from(*b)).collect();
    Ok((descr, url_value, "URL ID3 ISO-8859-1 (diagnóstico)".into()))
}

fn decode_id3(raw: &RawMediaRecord, bytes: &[u8], tag: &mut InterpretedTag) {
    if raw.key == "HEADER" || raw.key == "EXTENDED_HEADER" || raw.key == "FOOTER" {
        return;
    }
    let key = raw.key.as_str();
    if key == "COMM" || key == "COM" || key == "USLT" || key == "ULT" {
        if bytes.len() < 4 {
            tag.notes.push("comentário/letra ID3 truncado".into());
            return;
        }
        let language = String::from_utf8_lossy(&bytes[1..4]);
        match id3_description_body(bytes[0], bytes, 4) {
            Ok((descr, vals, enc)) => {
                tag.diagnostic_key = format!("{key}:{}:{descr}", language);
                tag.values = vals;
                tag.encoding = enc;
                tag.binary = false;
            }
            Err(err) => tag.notes.push(err),
        }
        return;
    }
    if key == "WXXX" || key == "WXX" {
        if bytes.is_empty() {
            tag.notes.push("WXXX vazio".into());
            return;
        }
        match decode_id3_user_url(bytes[0], bytes) {
            Ok((descr, url, enc)) => {
                tag.diagnostic_key = format!("{key}:{descr}");
                tag.values = vec![url];
                tag.encoding = enc;
                tag.binary = false;
            }
            Err(err) => tag.notes.push(err),
        }
        return;
    }
    if key == "TXXX" || key == "TXX" {
        if bytes.is_empty() {
            tag.notes.push("TXXX vazio".into());
            return;
        }
        match id3_description_body(bytes[0], bytes, 1) {
            Ok((descr, vals, enc)) => {
                tag.diagnostic_key = format!("{key}:{descr}");
                tag.values = vals;
                tag.encoding = enc;
                tag.binary = false;
            }
            Err(err) => tag.notes.push(err),
        }
        return;
    }
    if key.starts_with('T') && key != "TAG" {
        if let Some((&encoding, rest)) = bytes.split_first() {
            match id3_decode(encoding, rest) {
                Ok((v, enc)) => {
                    tag.values = split_id3_text(v);
                    tag.encoding = enc;
                    tag.binary = false;
                }
                Err(err) => tag.notes.push(err),
            }
        }
    } else if key.starts_with('W') && key != "WXXX" && key != "WXX" {
        match std::str::from_utf8(bytes) {
            Ok(s) => {
                tag.values = vec![s.to_owned()];
                tag.encoding = "URL bytes (UTF-8 válido)".into();
                tag.binary = false;
            }
            Err(_) => tag
                .notes
                .push("URL ID3 não-UTF8 (bytes brutos preservados)".into()),
        }
    } else if key == "APIC" || key == "PIC" {
        tag.notes
            .push("arte embutida: conteúdo binário; sem inferir regra BC5".into());
    }
}

fn flac_streaminfo(bytes: &[u8], tag: &mut InterpretedTag) {
    if bytes.len() != 34 {
        tag.notes
            .push("STREAMINFO FLAC deveria conter 34 bytes".into());
        return;
    }
    let rate = ((bytes[10] as u32) << 12) | ((bytes[11] as u32) << 4) | ((bytes[12] as u32) >> 4);
    let ch = ((bytes[12] & 0x0e) >> 1) + 1;
    let bits = ((((bytes[12] & 1) as u16) << 4) | ((bytes[13] as u16) >> 4)) + 1;
    let samples = ((bytes[13] as u64 & 15) << 32)
        | ((bytes[14] as u64) << 24)
        | ((bytes[15] as u64) << 16)
        | ((bytes[16] as u64) << 8)
        | bytes[17] as u64;
    tag.values = vec![
        format!("sample_rate={rate}"),
        format!("channels={ch}"),
        format!("bits_per_sample={bits}"),
        format!("total_samples={samples}"),
    ];
    tag.encoding = "FLAC STREAMINFO (ISO 20+3+5+36 bits)".into();
    tag.binary = false;
}

const MAX_MP4_ITEM_CHILDREN: usize = 10_000;
fn child_atoms(bytes: &[u8]) -> Result<Vec<([u8; 4], &[u8])>, String> {
    let mut children = Vec::new();
    let mut p = 0usize;
    while p < bytes.len() {
        if children.len() >= MAX_MP4_ITEM_CHILDREN {
            return Err("MP4 item: limite forense de subátomos atingido".into());
        }
        if p + 8 > bytes.len() {
            return Err("MP4 item: sobra de bytes sem cabeçalho de átomo".into());
        }
        let len = u32be(&bytes[p..]).ok_or("tamanho MP4 inválido")? as usize;
        let id: [u8; 4] = bytes[p + 4..p + 8]
            .try_into()
            .map_err(|_| "id MP4 inválido")?;
        let (size, header) = if len == 1 {
            let raw = bytes
                .get(p + 8..p + 16)
                .ok_or("extended size MP4 truncado")?;
            let n = u64::from_be_bytes(raw.try_into().map_err(|_| "extended size MP4 inválido")?);
            if n > usize::MAX as u64 {
                return Err("átomo MP4 grande demais".into());
            }
            (n as usize, 16)
        } else if len == 0 {
            // O observador RAW já rejeita zero-size aninhado. Não podemos
            // decodificar o mesmo item como válido usando o fim do pai.
            return Err("MP4 item: tamanho zero em átomo aninhado".into());
        } else {
            (len, 8)
        };
        if size < header || p.checked_add(size).map_or(true, |n| n > bytes.len()) {
            return Err("átomo MP4 excede limites".into());
        }
        children.push((id, &bytes[p + header..p + size]));
        p += size;
    }
    Ok(children)
}

fn decode_mp4_data(key: &str, payload: &[u8], tag: &mut InterpretedTag) {
    if payload.len() < 8 {
        tag.notes.push("MP4 data: type/locale truncados".into());
        return;
    }
    let kind = (u32be(payload).unwrap_or(0)) & 0x00ff_ffff;
    let body = &payload[8..];
    match kind {
        1 => match std::str::from_utf8(body) {
            Ok(s) => {
                tag.values.push(s.to_owned());
                tag.encoding = "MP4 UTF-8".into();
                tag.binary = false;
            }
            Err(_) => tag.notes.push("MP4 data UTF-8 inválido".into()),
        },
        2 => match decode_utf16(body, false) {
            Ok(s) => {
                tag.values.push(s);
                tag.encoding = "MP4 UTF-16BE".into();
                tag.binary = false;
            }
            Err(err) => tag.notes.push(err),
        },
        0 if (key == "trkn" || key == "disk") && body.len() >= 6 => {
            if let (Some(number), Some(total)) = (u16be(&body[2..]), u16be(&body[4..])) {
                tag.values.push(format!("{number}/{total}"));
                tag.encoding = "MP4 track/disc tuple (diagnóstico)".into();
                tag.binary = false;
            }
        }
        13 | 14 => tag.notes.push(format!(
            "arte MP4 tipo {kind}: {} bytes (não decidir igualdade BC5)",
            body.len()
        )),
        _ => tag.notes.push(format!(
            "MP4 data type={kind}; preservado sem interpretação"
        )),
    }
}

fn decode_mp4_item(raw: &RawMediaRecord, bytes: &[u8], tag: &mut InterpretedTag) {
    let children = match child_atoms(bytes) {
        Ok(a) => a,
        Err(err) => {
            tag.notes.push(err);
            return;
        }
    };
    let mut means = Vec::new();
    let mut names = Vec::new();
    for (kind, body) in &children {
        match kind {
            b"mean" | b"name" => {
                if let Some(text) = body.get(4..).and_then(|v| std::str::from_utf8(v).ok()) {
                    if kind == b"mean" {
                        means.push(text.to_owned());
                    } else {
                        names.push(text.to_owned());
                    }
                } else {
                    tag.notes.push("MP4 mean/name inválido".into());
                }
            }
            _ => (),
        }
    }
    if raw.key == "----" {
        // Não escolher silenciosamente o último mean/name: os bytes RAW
        // continuam disponíveis e uma tag freeform ambígua não é uma chave.
        if means.len() == 1 && names.len() == 1 {
            tag.diagnostic_key = format!("----:{}:{}", means[0], names[0]);
        } else {
            tag.notes.push(format!(
                "MP4 freeform ambíguo: {} mean e {} name; chave diagnóstica não atribuída",
                means.len(),
                names.len()
            ));
        }
    }
    for (kind, body) in children {
        if &kind == b"data" {
            decode_mp4_data(&raw.key, body, tag);
        }
    }
}

/// Tabela ISO BMFF `meta/keys`: versão/flags, contagem e entradas
/// [size_be32][namespace_4][key_bytes]. Os índices em ilst começam em 1.
/// Esta interpretação NÃO corresponde a uma regra comprovada do BC5.
fn decode_mp4_keys(bytes: &[u8]) -> Result<Vec<String>, String> {
    if bytes.len() < 8 {
        return Err("MP4 keys: cabeçalho truncado".into());
    }
    if &bytes[0..4] != &[0, 0, 0, 0] {
        return Err("MP4 keys: versão/flags não suportados no diagnóstico".into());
    }
    let count = u32be(&bytes[4..8]).ok_or("MP4 keys: contagem ausente")? as usize;
    if count > 10_000 {
        return Err("MP4 keys: excesso de entradas".into());
    }
    let mut keys = Vec::new();
    let mut pos = 8usize;
    for _ in 0..count {
        let end_header = pos.checked_add(8).ok_or("MP4 keys: overflow")?;
        if end_header > bytes.len() {
            return Err("MP4 keys: entrada truncada".into());
        }
        let size = u32be(&bytes[pos..]).ok_or("MP4 keys: tamanho ausente")? as usize;
        let end = pos.checked_add(size).ok_or("MP4 keys: tamanho excessivo")?;
        if size < 8 || end > bytes.len() {
            return Err("MP4 keys: tamanho de entrada inválido".into());
        }
        let namespace = &bytes[pos + 4..pos + 8];
        let key =
            std::str::from_utf8(&bytes[pos + 8..end]).map_err(|_| "MP4 keys: nome não UTF-8")?;
        let ns = std::str::from_utf8(namespace).map_err(|_| "MP4 keys: namespace não UTF-8")?;
        keys.push(format!("{ns}:{key}"));
        pos = end;
    }
    if pos != bytes.len() {
        return Err("MP4 keys: bytes extras não interpretados".into());
    }
    Ok(keys)
}

fn numeric_mp4_ilst_index(key: &str) -> Option<usize> {
    let hex = key.strip_prefix("0x")?;
    if hex.len() != 8 {
        return None;
    }
    let index = u32::from_str_radix(hex, 16).ok()? as usize;
    (index > 0).then_some(index)
}

fn new_tag(raw: &RawMediaRecord) -> InterpretedTag {
    InterpretedTag {
        source: raw.source.clone(),
        raw_key: raw.key.clone(),
        diagnostic_key: raw.key.clone(),
        occurrence: raw.occurrence,
        raw_blake3: raw.blake3.clone(),
        encoding: "raw (sem decodificação)".into(),
        values: Vec::new(),
        binary: true,
        notes: Vec::new(),
    }
}

/// Uma assinatura criptográfica só autentica os BYTES do arquivo. Não autentica
/// as chaves e os offsets declarados em um snapshot JSON, que poderiam ser
/// alterados por outra ferramenta. Exigimos que cada registro interpretado seja
/// observado novamente nos mesmos bytes por um leitor forense independente do
/// conteúdo do JSON. Campos de proveniência opcionais podem faltar em JSONs
/// antigos, mas campos fornecidos não podem contradizer a nova observação.
/// Não comprova 1:1 do BC5 nem exige que um snapshot antigo seja exaustivo.
fn verify_snapshot_provenance(
    raw: &RawMediaObservation,
    canonical: &RawMediaObservation,
) -> io::Result<()> {
    if raw.records.len() > 100_000 {
        return Err(invalid("snapshot RAW excede o limite forense de registros"));
    }
    let mut refs: HashMap<(&str, &str, u64, usize, &str), Vec<&RawMediaRecord>> = HashMap::new();
    for r in &canonical.records {
        refs.entry((&r.source, &r.key, r.offset, r.value_length, &r.blake3))
            .or_default()
            .push(r);
    }
    for claimed in &raw.records {
        let signature = (
            &*claimed.source,
            &*claimed.key,
            claimed.offset,
            claimed.value_length,
            &*claimed.blake3,
        );
        let Some(candidates) = refs.get_mut(&signature) else {
            return Err(invalid(
                "snapshot RAW contém registro não observado no arquivo original",
            ));
        };
        let matched = candidates.iter().position(|actual| {
            claimed.occurrence == actual.occurrence
                && claimed
                    .container_path
                    .as_ref()
                    .map_or(true, |p| actual.container_path.as_ref() == Some(p))
                && claimed
                    .meta_header_offset
                    .map_or(true, |o| actual.meta_header_offset == Some(o))
                && claimed
                    .id3_header_offset
                    .map_or(true, |o| actual.id3_header_offset == Some(o))
        });
        match matched {
            Some(index) => {
                candidates.swap_remove(index);
            }
            None => {
                return Err(invalid(
                    "snapshot RAW contém ocorrência ou proveniência incompatível",
                ))
            }
        }
    }
    Ok(())
}

/// Faz interpretação opcional dos registros RAW; rejeita arquivo alterado
/// entre observação e interpretação (mesmo tamanho com bytes diferentes).
/// NUNCA muda a saída original de `observe_media_raw` ou o motor de comparação.
pub fn interpret_raw_observation(
    raw: &RawMediaObservation,
    path: impl AsRef<Path>,
) -> io::Result<InterpretedMediaObservation> {
    let path = path.as_ref();
    // Mesmo snapshots JSON fornecidos externamente NÃO podem contornar
    // o teto de memória usado na coleta RAW.
    const MAX_INTERPRET_BYTES: u64 = 256 * 1024 * 1024;
    if raw.file_size > MAX_INTERPRET_BYTES {
        return Err(invalid(
            "arquivo acima do limite de 256 MiB para interpretação",
        ));
    }
    // A leitura é limitada mesmo que o arquivo cresça durante o read().
    let bytes = read_bounded(path)?;
    if bytes.len() as u64 != raw.file_size {
        return Err(invalid(
            "arquivo alterado desde observação RAW: tamanho mudou",
        ));
    }
    if let Some(expected) = raw.file_blake3.as_ref() {
        if blake3::hash(&bytes).to_hex().as_str() != expected {
            return Err(invalid(
                "arquivo alterado desde observação RAW: hash integral divergente",
            ));
        }
    }
    // Um JSON adulterado poderia manter o hash integral válido, mas trocar
    // `TIT2` por `TPE1` ou falsificar container_path. Revalidar registros.
    let canonical = observe_media_bytes(path, &bytes);
    verify_snapshot_provenance(raw, &canonical)?;
    let mut out = InterpretedMediaObservation {
        file: raw.file.clone(),
        raw_records: raw.records.len(),
        tags: Vec::new(),
        warnings: raw.warnings.clone(),
        matches_bc5: false,
    };
    if raw.file_blake3.is_none() {
        out.warnings.push(
            "snapshot RAW legado sem hash integral; registros foram revalidados pela estrutura"
                .into(),
        );
    }
    if raw.records.len() != canonical.records.len() {
        out.warnings.push(format!(
            "snapshot RAW parcial: {} registros declarados, {} observados; não atesta completude",
            raw.records.len(),
            canonical.records.len()
        ));
    }
    // Não associar um índice ao dicionário errado: se há mais de uma
    // tabela keys, sem identificador de hierarquia na estrutura RAW antiga,
    // a associação fica explicitamente indeterminada.
    let key_tables: Vec<&RawMediaRecord> = raw
        .records
        .iter()
        .filter(|r| r.source == "mp4/auxiliary" && r.key.ends_with("/keys"))
        .collect();
    // path do `keys` é `mp4/moov/.../meta`; os itens do ilst guardam
    // `mp4/moov/.../meta/ilst`. A associação só ocorre com prefixo idêntico.
    // A estrutura antiga sem path não é promovida a uma associação adivinhada.
    let mut key_maps: Vec<(String, Option<u64>, Vec<String>)> = Vec::new();
    for r in key_tables {
        let Some(parent) = r.container_path.as_ref() else {
            out.warnings
                .push("MP4 keys legado sem caminho: índices ilst não associados".into());
            continue;
        };
        let start = usize::try_from(r.offset).map_err(|_| invalid("offset keys inválido"))?;
        let end = start
            .checked_add(r.value_length)
            .ok_or_else(|| invalid("overflow keys"))?;
        let content = bytes
            .get(start..end)
            .ok_or_else(|| invalid("keys fora do arquivo"))?;
        match decode_mp4_keys(content) {
            Ok(keys) => key_maps.push((parent.to_string(), r.meta_header_offset, keys)),
            Err(error) => out.warnings.push(format!("{parent}: {error}")),
        }
    }
    for record in &raw.records {
        let start = usize::try_from(record.offset).map_err(|_| invalid("offset fora do limite"))?;
        let end = start
            .checked_add(record.value_length)
            .ok_or_else(|| invalid("overflow em offset+length"))?;
        let value = bytes
            .get(start..end)
            .ok_or_else(|| invalid("registro bruto fora dos limites"))?;
        if blake3::hash(value).to_hex().to_string() != record.blake3 {
            return Err(invalid(
                "arquivo alterado desde observação RAW: hash de registro diferente",
            ));
        }
        let mut tag = new_tag(record);
        if record.source == "mp4/ilst" {
            if let Some(index) = numeric_mp4_ilst_index(&record.key) {
                if let Some(parent) = record
                    .container_path
                    .as_deref()
                    .and_then(|s| s.strip_suffix("/ilst"))
                {
                    // Um caminho de texto não é identificador exclusivo: dois
                    // átomos meta irmãos têm o MESMO caminho. O endereço
                    // do cabeçalho meta é a identidade forense estável.
                    // Snapshots antigos sem offset só associam se não houver
                    // nenhuma ambiguidade (o teste abaixo detecta múltiplos).
                    let mut matches = key_maps.iter().filter(|(path, offset, _)| {
                        path.as_str() == parent && *offset == record.meta_header_offset
                    });
                    let first = matches.next();
                    if matches.next().is_some() {
                        tag.notes.push(format!(
                            "Índice MP4 ilst #{index}: múltiplas keys no mesmo meta"
                        ));
                    } else if let Some((_, _, keys)) = first {
                        if let Some(name) = keys.get(index - 1) {
                            tag.diagnostic_key = name.clone();
                            tag.notes.push(format!("Índice MP4 ilst #{index} resolvido pelo meta/keys do mesmo recipiente (somente diagnóstico)"));
                        } else {
                            tag.notes
                                .push(format!("Índice MP4 ilst #{index} fora da tabela keys"));
                        }
                    } else {
                        tag.notes.push(format!(
                            "Índice MP4 ilst #{index} sem keys no mesmo recipiente"
                        ));
                    }
                } else {
                    tag.notes.push(format!(
                        "Índice MP4 ilst #{index} com caminho de recipiente desconhecido"
                    ));
                }
            }
        }
        match record.source.as_str() {
            s if s.starts_with("id3v2.") => {
                // Um parser de frames não pode apresentar bytes com
                // unsynchronisation, compressão, criptografia ou grouping
                // como se fossem texto plano.
                // Cada frame herda as flags de SUA tag; múltiplas tags
                // ID3v2 podem ter políticas de unsync distintas.
                let header_at = record
                    .id3_header_offset
                    .and_then(|x| usize::try_from(x).ok())
                    .or_else(|| {
                        // JSON antigo sem campo id3_header_offset: recupere a
                        // última âncora HEADER precedente da MESMA versão.
                        // Não atribua indiscriminadamente as flags do byte 5.
                        raw.records
                            .iter()
                            .filter(|r| {
                                r.key == "HEADER"
                                    && r.source == record.source
                                    && r.offset <= record.offset
                            })
                            .filter_map(|r| usize::try_from(r.offset).ok())
                            .max()
                    });
                let global_unsync = header_at
                    .and_then(|at| bytes.get(at.saturating_add(5)))
                    .map(|b| b & 0x80 != 0)
                    .unwrap_or(false);
                let flags = if s == "id3v2.2" || start < 2 {
                    [0, 0]
                } else {
                    [bytes[start - 2], bytes[start - 1]]
                };
                let flagged = match s {
                    "id3v2.4" => flags[1] & (0x40 | 0x08 | 0x04 | 0x02 | 0x01) != 0,
                    "id3v2.3" => flags[1] & (0x80 | 0x40 | 0x20) != 0,
                    _ => false,
                };
                if (global_unsync || flagged)
                    && record.key != "HEADER"
                    && record.key != "EXTENDED_HEADER"
                    && record.key != "FOOTER"
                {
                    tag.notes.push(format!("ID3 com flags não tratadas para interpretação: global_unsync={global_unsync}, frame={flags:02x?}; bytes RAW preservados"));
                } else {
                    decode_id3(record, value, &mut tag);
                }
            }
            "id3v1" => {
                if record.key != "GENRE" && record.key != "TRACK" {
                    tag.values
                        .push(value.iter().map(|b| char::from(*b)).collect());
                    tag.encoding = "ID3v1 Latin-1 (inclusive padding original)".into();
                    tag.binary = false;
                }
            }
            "vorbis" => {
                if let Ok(s) = std::str::from_utf8(value) {
                    tag.values.push(s.to_owned());
                    tag.encoding = "Vorbis UTF-8".into();
                    tag.binary = false;
                } else {
                    tag.notes.push("Vorbis campo não UTF-8".into());
                }
            }
            "flac" if record.key == "STREAMINFO" => flac_streaminfo(value, &mut tag),
            "mp4/ilst" => decode_mp4_item(record, value, &mut tag),
            // `mp4/ilst-data` já é representado pelo registro pai `mp4/ilst`;
            // mantemos os bytes, mas não duplicamos o valor interpretado.
            _ => (),
        }
        out.tags.push(tag);
    }
    Ok(out)
}

pub fn observe_media_interpreted(
    path: impl AsRef<Path>,
) -> io::Result<InterpretedMediaObservation> {
    let path = path.as_ref();
    let raw = observe_media_raw(path)?;
    interpret_raw_observation(&raw, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn id3v2_decodes_latin_utf8_utf16_without_normalizing() {
        assert_eq!(id3_decode(0, &[0xc7, 0x20, 0x20]).unwrap().0, "Ç  ");
        assert_eq!(id3_decode(3, "AÇÃO".as_bytes()).unwrap().0, "AÇÃO");
        assert_eq!(
            id3_decode(1, &[0xff, 0xfe, 0xc7, 0, 0xe3, 0]).unwrap().0,
            "Çã"
        );
        assert!(id3_decode(1, &[0x44, 0x00]).is_err());
        // UTF-16LE: descrição "A", separador em byte par, valor "B".
        let joined = [1, 0xff, 0xfe, 0x41, 0, 0, 0, 0x42, 0];
        assert_eq!(id3_description_body(1, &joined, 1).unwrap().1, vec!["B"]);
    }
    #[test]
    fn user_url_frame_keeps_url_latin1_when_description_utf16() {
        // WXXX: encoding=1; descrição "BR" UTF-16LE BOM, URL Latin-1.
        let bytes = [
            1, 0xff, 0xfe, b'B', 0, b'R', 0, 0, 0, b'h', b't', b't', b'p', b':', b'/', b'/', 0xe7,
        ];
        let (descr, url, _) = decode_id3_user_url(bytes[0], &bytes).unwrap();
        assert_eq!(descr, "BR");
        assert_eq!(url, "http://ç");
        // A descrição nunca deve consumir nem converter a URL em UTF-16.
        assert!(decode_id3_user_url(1, &[1, 0xff, 0xfe, b'A', 0]).is_err());
    }
    #[test]
    fn user_url_frame_rejects_missing_separator() {
        assert!(decode_id3_user_url(0, &[0, b'A', b'B']).is_err());
        assert!(decode_id3_user_url(2, &[2, 0, b'A']).is_err());
    }
    #[test]
    fn mp4_disc_track_are_not_tag_normalization() {
        let raw = RawMediaRecord {
            source: "mp4/ilst".into(),
            key: "trkn".into(),
            occurrence: 0,
            offset: 0,
            value_length: 0,
            blake3: "".into(),
            container_path: None,
            meta_header_offset: None,
            id3_header_offset: None,
            utf8_value: None,
            hex_prefix: "".into(),
        };
        let mut tag = new_tag(&raw);
        let body = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 3, 0, 8];
        decode_mp4_data("trkn", &body, &mut tag);
        assert_eq!(tag.values, vec!["3/8"]);
    }
    #[test]
    fn mp4_mdta_key_table_preserves_index_and_unicode() {
        // keys: versão 0, contagem 2. Índices ilst 1 e 2.
        let mut bytes = vec![0, 0, 0, 0, 0, 0, 0, 2];
        for name in ["com.example.title", "Álbum"] {
            let key = name.as_bytes();
            bytes.extend(((key.len() + 8) as u32).to_be_bytes());
            bytes.extend(b"mdta");
            bytes.extend(key);
        }
        assert_eq!(
            decode_mp4_keys(&bytes).unwrap(),
            vec!["mdta:com.example.title", "mdta:Álbum"]
        );
        assert_eq!(numeric_mp4_ilst_index("0x00000001"), Some(1));
        assert_eq!(numeric_mp4_ilst_index("0x00000000"), None);
        assert_eq!(numeric_mp4_ilst_index("trkn"), None);
    }
    #[test]
    fn mp4_mdta_keys_reject_truncation_without_guessing() {
        assert!(
            decode_mp4_keys(&[0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 25, b'm', b'd', b't', b'a'])
                .is_err()
        );
        assert!(decode_mp4_keys(&[0, 0, 0, 1, 0, 0, 0, 0]).is_err());
        assert!(decode_mp4_keys(&[0, 0, 0, 0, 0, 0, 0, 0, 0]).is_err());
    }
    #[test]
    fn freeform_keeps_mean_and_name() {
        fn atom(id: &[u8; 4], content: &[u8]) -> Vec<u8> {
            let mut b = ((content.len() + 8) as u32).to_be_bytes().to_vec();
            b.extend(id);
            b.extend(content);
            b
        }
        let mut data = Vec::new();
        data.extend(atom(b"mean", b"\0\0\0\0com.apple.iTunes"));
        data.extend(atom(b"name", b"\0\0\0\0TEST"));
        data.extend(atom(b"data", b"\0\0\0\x01\0\0\0\0VALUE"));
        let raw = RawMediaRecord {
            source: "mp4/ilst".into(),
            key: "----".into(),
            occurrence: 0,
            offset: 0,
            value_length: data.len(),
            blake3: "".into(),
            container_path: None,
            meta_header_offset: None,
            id3_header_offset: None,
            utf8_value: None,
            hex_prefix: "".into(),
        };
        let mut tag = new_tag(&raw);
        decode_mp4_item(&raw, &data, &mut tag);
        assert_eq!(tag.diagnostic_key, "----:com.apple.iTunes:TEST");
        assert_eq!(tag.values, vec!["VALUE"]);
    }
    #[test]
    fn duplicate_values_are_not_discarded() {
        assert_eq!(split_id3_text("A\0A".into()), vec!["A", "A"]);
    }
    #[test]
    fn one_optional_final_terminator_is_not_an_extra_value() {
        assert_eq!(split_id3_text("A\0B\0".into()), vec!["A", "B"]);
        assert_eq!(split_id3_text("A\0\0B\0".into()), vec!["A", "", "B"]);
        assert_eq!(split_id3_text("A\0A\0".into()), vec!["A", "A"]);
    }
}
