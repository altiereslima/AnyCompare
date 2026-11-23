//! Etapa 14: coerência estrutural do observador RAW e interpretador.
//! Estes testes NÃO reproduzem nem afirmam as regras de igualdade do BC5.
use bc5_media::{observe_media_interpreted, observe_media_raw};
use std::io::Write;

fn from_bytes(
    bytes: &[u8],
) -> (
    bc5_media::RawMediaObservation,
    bc5_media::InterpretedMediaObservation,
) {
    let mut file = tempfile::NamedTempFile::new().expect("fixture");
    file.write_all(bytes).expect("gravar");
    file.flush().expect("flush");
    let raw = observe_media_raw(file.path()).expect("RAW");
    let interpreted = observe_media_interpreted(file.path()).expect("interpretação");
    assert!(!raw.is_bc5_comparison);
    assert!(!interpreted.matches_bc5);
    (raw, interpreted)
}
fn atom(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
    assert!(body.len() < u32::MAX as usize - 8);
    let mut a = ((body.len() + 8) as u32).to_be_bytes().to_vec();
    a.extend(kind);
    a.extend(body);
    a
}
fn mp4_ilst(entry: &[u8]) -> Vec<u8> {
    let mut meta = vec![0, 0, 0, 0];
    meta.extend(atom(b"ilst", entry));
    let mut file = atom(b"ftyp", b"M4A \0\0\0\0");
    file.extend(atom(b"moov", &atom(b"udta", &atom(b"meta", &meta))));
    file
}
fn flac_block(t: u8, last: bool, body: &[u8]) -> Vec<u8> {
    assert!(body.len() < (1 << 24));
    let n = body.len();
    let mut v = vec![
        (t & 0x7f) | (if last { 0x80 } else { 0 }),
        (n >> 16) as u8,
        (n >> 8) as u8,
        n as u8,
    ];
    v.extend(body);
    v
}
fn mp3_with_header(major: u8, flags: u8) -> Vec<u8> {
    vec![
        b'I', b'D', b'3', major, 0, flags, 0, 0, 0, 0, 0xff, 0xfb, 0x90, 0x64,
    ]
}

#[test]
fn nested_zero_size_mp4_never_becomes_text() {
    let mut invalid = vec![0, 0, 0, 0];
    invalid.extend(b"data");
    invalid.extend([0, 0, 0, 1, 0, 0, 0, 0, b'A']);
    let item = atom(b"\xa9nam", &invalid);
    let (raw, interp) = from_bytes(&mp4_ilst(&item));
    assert!(
        raw.warnings
            .iter()
            .any(|w| w.contains("tamanho zero dentro")),
        "{:?}",
        raw.warnings
    );
    let tag = interp
        .tags
        .iter()
        .find(|t| t.source == "mp4/ilst" && t.raw_key == "0xa96e616d")
        .expect("entrada original");
    assert!(
        tag.values.is_empty(),
        "texto inválido jamais pode ser mostrado como válido"
    );
    assert!(tag.binary);
    assert!(
        tag.notes
            .iter()
            .any(|n| n.contains("tamanho zero em átomo aninhado")),
        "{:?}",
        tag.notes
    );
}
#[test]
fn valid_nested_mp4_data_still_decodes() {
    let data = atom(b"data", &[0, 0, 0, 1, 0, 0, 0, 0, b'O', b'K']);
    let item = atom(b"\xa9nam", &data);
    let (raw, interp) = from_bytes(&mp4_ilst(&item));
    assert!(raw.warnings.is_empty(), "{:?}", raw.warnings);
    let tag = interp
        .tags
        .iter()
        .find(|t| t.source == "mp4/ilst" && t.raw_key == "0xa96e616d")
        .unwrap();
    assert_eq!(tag.values, vec!["OK".to_owned()]);
    assert!(!tag.binary);
}
#[test]
fn mp4_subatom_count_has_explicit_limit() {
    let mut children = Vec::with_capacity(10_001 * 8);
    for _ in 0..10_001 {
        children.extend([0, 0, 0, 8, b'd', b'a', b't', b'a']);
    }
    let entry = atom(b"----", &children);
    let (_, interp) = from_bytes(&mp4_ilst(&entry));
    let tag = interp
        .tags
        .iter()
        .find(|t| t.source == "mp4/ilst" && t.raw_key == "----")
        .unwrap();
    assert!(
        tag.notes
            .iter()
            .any(|n| n.contains("limite forense de subátomos")),
        "{:?}",
        tag.notes
    );
    assert!(tag.values.is_empty());
}
#[test]
fn flac_requires_streaminfo_first_for_diagnostic() {
    let mut f = b"fLaC".to_vec();
    f.extend(flac_block(4, true, &[0, 0, 0, 0, 0, 0, 0, 0]));
    let (raw, _) = from_bytes(&f);
    assert_eq!(raw.container, "flac");
    assert!(raw
        .warnings
        .iter()
        .any(|w| w.contains("primeiro bloco") && w.contains("STREAMINFO")));
}
#[test]
fn flac_duplicate_streaminfo_has_distinct_occurrences() {
    let mut f = b"fLaC".to_vec();
    f.extend(flac_block(0, false, &[0; 34]));
    f.extend(flac_block(0, true, &[0; 34]));
    let (raw, _) = from_bytes(&f);
    let seen: Vec<_> = raw
        .records
        .iter()
        .filter(|r| r.key == "STREAMINFO")
        .map(|r| r.occurrence)
        .collect();
    assert_eq!(seen, vec![0, 1]);
    assert!(raw
        .warnings
        .iter()
        .any(|w| w.contains("STREAMINFO duplicado")));
}
#[test]
fn flac_wrong_streaminfo_length_warns_without_dropping_raw() {
    let mut f = b"fLaC".to_vec();
    f.extend(flac_block(0, true, &[1, 2, 3]));
    let (raw, _) = from_bytes(&f);
    assert!(raw
        .records
        .iter()
        .any(|r| r.key == "STREAMINFO" && r.value_length == 3));
    assert!(raw
        .warnings
        .iter()
        .any(|w| w.contains("STREAMINFO tem 3 bytes")));
}
#[test]
fn flac_reserved_type_127_retains_payload_and_warns() {
    let mut f = b"fLaC".to_vec();
    f.extend(flac_block(0, false, &[0; 34]));
    f.extend(flac_block(127, true, b"RAW"));
    let (raw, _) = from_bytes(&f);
    assert!(raw
        .records
        .iter()
        .any(|r| r.key == "BLOCK_127" && r.value_length == 3));
    assert!(raw.warnings.iter().any(|w| w.contains("tipo de bloco 127")));
}
#[test]
fn id3v2_version_specific_reserved_flags() {
    for (major, flag, should_warn) in [
        (2, 0x20, true),
        (2, 0x40, false),
        (3, 0x10, true),
        (3, 0x20, false),
        (4, 0x08, true),
        (4, 0x10, false),
    ] {
        let (raw, _) = from_bytes(&mp3_with_header(major, flag));
        let has = raw.warnings.iter().any(|w| w.contains("bits reservados"));
        assert_eq!(
            has, should_warn,
            "ID3v2.{major} flag 0x{flag:02x}: {:?}",
            raw.warnings
        );
    }
}

#[test]
fn mp4_freeform_duplicate_mean_is_not_silently_overwritten() {
    let mut children = Vec::new();
    for text in ["com.apple.iTunes", "com.example"] {
        let mut body = vec![0, 0, 0, 0];
        body.extend(text.as_bytes());
        children.extend(atom(b"mean", &body));
    }
    let mut name = vec![0, 0, 0, 0];
    name.extend(b"CUSTOM");
    children.extend(atom(b"name", &name));
    children.extend(atom(b"data", &[0, 0, 0, 1, 0, 0, 0, 0, b'V']));
    let entry = atom(b"----", &children);
    let (raw, interp) = from_bytes(&mp4_ilst(&entry));
    assert!(raw
        .records
        .iter()
        .any(|r| r.source == "mp4/ilst" && r.key == "----"));
    let tag = interp
        .tags
        .iter()
        .find(|t| t.source == "mp4/ilst" && t.raw_key == "----")
        .unwrap();
    assert_eq!(tag.diagnostic_key, "----");
    assert_eq!(tag.values, vec!["V".to_owned()]);
    assert!(tag.notes.iter().any(|n| n.contains("2 mean e 1 name")));
}
