//! Adapters for read-only property comparisons. External readers were reviewed
//! from ETAPA24; this module does not reuse its obsolete GUI/application root.
use crate::{
    history::SessionKind,
    pe_version::{Image, Node},
};
use sha2::{Digest, Sha256};

pub fn media_path(path: &Path) -> bool {
    // Native BC5 also opens MP4 containers with video in Media Compare. This
    // is observed in the user's 03/11/2026 capture; do not exclude them by guess.
    path.extension().is_some_and(|e| {
        ["mp4", "wav", "wave", "opus"]
            .iter()
            .any(|ext| e.eq_ignore_ascii_case(ext))
    }) || anycompare_media::is_media_extension(path, anycompare_media::MediaProfile::Bc5)
}
use std::{collections::BTreeMap, path::Path, sync::Arc};

pub fn load(kind: SessionKind, path: &Path) -> Result<Image, String> {
    if kind == SessionKind::Version {
        return Image::load(path);
    }
    if std::fs::metadata(path).map_err(|e| e.to_string())?.len() > 256 * 1024 * 1024 {
        return Err("Arquivo acima do limite de 256 MiB desta visualização.".into());
    }
    let before = anycompare_media::SourceFingerprint::capture(path).map_err(|e| e.to_string())?;
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let (nodes, digest): (Result<Vec<Node>, String>, [u8; 32]) = std::thread::scope(|scope| {
        let hash_handle = scope.spawn(|| {
            let d: [u8; 32] = Sha256::digest(&bytes).into();
            d
        });
        let nodes = match kind {
            SessionKind::Registry => registry(path),
            SessionKind::Media => media(path, &bytes),
            _ => Err("Tipo de propriedades inválido.".into()),
        };
        (nodes, hash_handle.join().unwrap())
    });
    let nodes = nodes?;
    let after = anycompare_media::SourceFingerprint::capture(path).map_err(|e| e.to_string())?;
    if before != after {
        return Err("O arquivo mudou durante a leitura. Recarregue a comparação.".into());
    }
    Ok(Image::properties(path, nodes, bytes.len(), digest))
}
fn field(key: String, name: String, value: String, bytes: Vec<u8>) -> Node {
    Node {
        key,
        name,
        value,
        bytes: Some(Arc::from(bytes)),
        children: Vec::new(),
    }
}
#[derive(Default)]
struct Branch {
    name: String,
    children: BTreeMap<String, Branch>,
    values: Vec<Node>,
}
fn branch_nodes(branch: Branch) -> Vec<Node> {
    let mut nodes: Vec<_> = branch
        .children
        .into_iter()
        .map(|(key, child)| {
            let name = child.name.clone();
            Node {
                key,
                name,
                value: String::new(),
                bytes: None,
                children: branch_nodes(child),
            }
        })
        .collect();
    nodes.extend(branch.values);
    nodes
}
fn registry(path: &Path) -> Result<Vec<Node>, String> {
    let parsed = anycompare_registry::read_file(path)?;
    let mut root = Branch::default();
    for key in parsed.keys.values() {
        let mut branch = &mut root;
        for part in key.path.split('\\') {
            branch = branch
                .children
                .entry(part.to_lowercase())
                .or_insert_with(|| Branch {
                    name: part.into(),
                    ..Branch::default()
                });
        }
        for value in key.values.values() {
            let mut bytes = value.type_id.to_le_bytes().to_vec();
            bytes.extend_from_slice(&value.bytes);
            let name = if value.name.is_empty() {
                "version.default".into()
            } else {
                value.name.clone()
            };
            branch.values.push(field(
                format!("value:{}", value.name.to_lowercase()),
                name,
                format!("{}: {}", value.type_name(), value.displayed_preview(256)),
                bytes,
            ));
        }
    }
    Ok(branch_nodes(root))
}
fn media(path: &Path, bytes: &[u8]) -> Result<Vec<Node>, String> {
    let fields = match anycompare_media::inspect_media(path, anycompare_media::InspectOptions::default()) {
        Ok(parsed) => {
            let mut fields = parsed.fields;
            // The prototype aliases both encoder keys. Native MP3 and M4A
            // reports distinguish software settings from the MP4 encoder.
            fields.remove("tag.encoder");
            for tag in parsed
                .raw_tags
                .iter()
                .filter(|tag| matches!(tag.key.as_str(), "EncoderSoftware" | "EncodedBy"))
            {
                let key = if tag.key == "EncodedBy" || parsed.file_type == "Mp4" {
                    "tag.encoder"
                } else {
                    "tag.encoder_settings"
                };
                fields
                    .entry(key.into())
                    .or_insert_with(Vec::new)
                    .push(tag.value.clone());
            }
            fields
        }
        Err(anycompare_media::MediaError::Unsupported(_))
            if (bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WAVE"))
                || bytes.starts_with(b"OggS")
                    && bytes
                        .get(..128.min(bytes.len()))
                        .is_some_and(|head| head.windows(8).any(|s| s == b"OpusHead")) =>
        {
            // Native controlled reports for the authorized WAV/Opus samples
            // show only Audio/Data, with the complete file as data. Do not
            // expose Extended-profile tags that the original did not show.
            BTreeMap::new()
        }
        Err(error) => return Err(error.to_string()),
    };
    let mut tags = Vec::new();
    for (key, values) in fields {
        if !key.starts_with("tag.") {
            continue;
        }
        // Preserve order/multiplicity and separate values unambiguously. The
        // BC5 precedence/importance policies remain an open parity requirement.
        let identity = serde_json::to_vec(&values).unwrap();
        let short = key.strip_prefix("tag.").unwrap_or(&key);
        let name = match short {
            "title" => "version.title",
            "artist" => "version.artist",
            "album" => "version.album",
            "encoder" => "version.encoder",
            "encoder_settings" => "version.encoding_settings",
            "recording_date" => "version.recording_date",
            "comment" => "version.comment",
            "year" => "version.year",
            "genre" => "version.genre",
            "track_number" => "version.track",
            _ => short,
        }
        .to_owned();
        let node = field(short.into(), name, values.join("; "), identity);
        tags.push(node);
    }
    let order = [
        "title",
        "artist",
        "album",
        "year",
        "recording_date",
        "genre",
        "track_number",
        "comment",
        "encoder",
        "encoder_settings",
    ];
    tags.sort_by_key(|n| {
        order
            .iter()
            .position(|key| *key == n.key)
            .unwrap_or(usize::MAX)
    });
    let data = audio_bytes(bytes)?;
    let audio = field(
        "data".into(),
        "version.data".into(),
        format!("{} bytes", data.len()),
        data,
    );
    let mut nodes = Vec::new();
    if !tags.is_empty() {
        nodes.push(Node {
            key: "metadata".into(),
            name: "version.metadata".into(),
            value: String::new(),
            bytes: None,
            children: tags,
        });
    }
    nodes.push(Node {
        key: "audio".into(),
        name: "version.audio".into(),
        value: String::new(),
        bytes: None,
        children: vec![audio],
    });
    Ok(nodes)
}

pub fn display_number(n: usize) -> String {
    let digits = n.to_string();
    let separator = if crate::i18n::language().starts_with("en") {
        ','
    } else {
        '.'
    };
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(separator);
        }
        out.push(digit);
    }
    out
}

/// Compare encoded payload as well as tags: equal duration/sample rate cannot
/// establish equal audio. These bounded container slices are not decoded PCM.
fn audio_bytes(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let mut start = 0usize;
    while bytes.get(start..start + 3) == Some(b"ID3") {
        let header = bytes
            .get(start..start + 10)
            .ok_or("Cabeçalho ID3 incompleto.")?;
        if header[6..10].iter().any(|b| b & 128 != 0) {
            return Err("Tamanho ID3 inválido.".into());
        }
        let size = header[6..10]
            .iter()
            .fold(0usize, |n, b| (n << 7) | usize::from(*b));
        let footer = if header[3] == 4 && header[5] & 0x10 != 0 {
            10
        } else {
            0
        };
        start = start
            .checked_add(10 + size + footer)
            .filter(|n| *n <= bytes.len())
            .ok_or("Tag ID3 truncada.")?;
    }
    if bytes.get(start..start + 4) == Some(b"fLaC") {
        start += 4;
        loop {
            let header = bytes.get(start..start + 4).ok_or("Bloco FLAC truncado.")?;
            let size = (usize::from(header[1]) << 16)
                | (usize::from(header[2]) << 8)
                | usize::from(header[3]);
            let last = header[0] & 0x80 != 0;
            start = start
                .checked_add(4 + size)
                .filter(|n| *n <= bytes.len())
                .ok_or("Metadados FLAC truncados.")?;
            if last {
                break;
            }
        }
        return Ok(bytes[start..].to_vec());
    }
    if bytes.get(4..8) == Some(b"ftyp") {
        // Pre-scan MP4 top-level atoms to validate sizes and pre-allocate exact
        // `mdat` capacity (avoids repeated Vec doubling on large MP4 containers).
        let mut at = 0usize;
        let mut mdat_total = 0usize;
        let mut mdat_spans: Vec<(usize, usize)> = Vec::new();
        while at < bytes.len() {
            let header = bytes.get(at..at + 8).ok_or("Átomo MP4 truncado.")?;
            let short = u32::from_be_bytes(header[..4].try_into().unwrap());
            let (size, head) = if short == 1 {
                let size = bytes.get(at + 8..at + 16).ok_or("Átomo MP4 incompleto.")?;
                (
                    usize::try_from(u64::from_be_bytes(size.try_into().unwrap()))
                        .map_err(|_| "Átomo MP4 grande demais.")?,
                    16,
                )
            } else if short == 0 {
                (bytes.len() - at, 8)
            } else {
                (short as usize, 8)
            };
            let end = at
                .checked_add(size)
                .filter(|end| size >= head && *end <= bytes.len())
                .ok_or("Tamanho de átomo MP4 inválido.")?;
            if &header[4..8] == b"mdat" {
                // Native reports for both user-provided MP4 files include
                // the mdat atom header (8 bytes here), not only its payload.
                mdat_spans.push((at, end));
                mdat_total += end - at;
            }
            at = end;
        }
        let mut data = Vec::with_capacity(mdat_total);
        for (s, e) in mdat_spans {
            data.extend_from_slice(&bytes[s..e]);
        }
        return Ok(data);
    }
    let mut end = bytes.len();
    if end >= 128 && bytes.get(end - 128..end - 125) == Some(b"TAG") {
        end -= 128;
    }
    if start > end {
        return Err("Limites de áudio inválidos.".into());
    }
    Ok(bytes[start..end].to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wav_opus_and_mp4_route_to_media_and_truncated_supported_containers_are_rejected() {
        for ext in ["wav", "WAVE", "opus", "mp4"] {
            assert_eq!(
                SessionKind::for_paths(
                    Path::new(&format!("a.{ext}")),
                    Path::new(&format!("b.{ext}"))
                ),
                SessionKind::Media
            );
        }
        assert!(audio_bytes(b"ID3\x04\0\0\0\0\0\x7f").is_err());
        assert!(audio_bytes(b"\0\0\0\x20ftypisom").is_err());
    }

    #[test]
    fn mp4_video_extensions_route_to_media_instead_of_text_or_hex() {
        let a = Path::new("left-video.MP4");
        let b = Path::new("right-video.mp4");
        assert_eq!(SessionKind::for_paths(a, b), SessionKind::Media);
    }
    #[test]
    fn native_mp3_probe_matches_audio_size_and_compares_audio_bytes_independently_of_tags() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("vendor/media/tests/fixtures");
        for extension in ["mp3", "flac", "m4a"] {
            let a = load(SessionKind::Media, &root.join(format!("A.{extension}"))).unwrap();
            let b = load(SessionKind::Media, &root.join(format!("B.{extension}"))).unwrap();
            let data = &a.nodes[1].children[0];
            assert!(!data.bytes.as_ref().unwrap().is_empty());
            assert!(
                data.same(&b.nodes[1].children[0]),
                "audio of supplied fixtures is identical: {extension}"
            );
            assert_ne!(a.nodes[0].children[0].value, b.nodes[0].children[0].value);
            if extension == "mp3" {
                assert_eq!(data.bytes.as_ref().unwrap().len(), 2689);
            }
        }
        let mut a = std::fs::read(root.join("A.mp3")).unwrap();
        let before = audio_bytes(&a).unwrap();
        let n = a.len();
        a[n - 1] ^= 1;
        assert_ne!(audio_bytes(&a).unwrap(), before);
    }
    #[test]
    fn imported_sources_are_read_by_their_actual_readers_and_type_identity_is_retained() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("vendor");
        let media = root.join("media/tests/fixtures/A.mp3");
        let image = load(SessionKind::Media, &media).unwrap();
        assert!(!image.nodes.is_empty());
        assert_eq!(image.path.as_deref(), Some(media.as_path()));
        assert!(load(SessionKind::Registry, &media).is_err());
        let sample = std::env::temp_dir().join(format!(
            "anycompare-reg-sample-{}.reg",
            std::process::id()
        ));
        std::fs::write(
            &sample,
            "Windows Registry Editor Version 5.00\r\n\r\n[HKEY_CURRENT_USER\\Software\\AnyCompareTest]\r\n\"Theme\"=\"Light\"\r\n",
        )
        .unwrap();
        assert!(!load(SessionKind::Registry, &sample)
            .unwrap()
            .nodes
            .is_empty());
        let _ = std::fs::remove_file(&sample);
    }
}
