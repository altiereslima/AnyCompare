//! Etapa 13: limites brutos de recipientes. Não afirma paridade com o BC5.
use bc5_media::observe_media_raw;
use std::io::Write;

fn observe(data: &[u8]) -> bc5_media::RawMediaObservation {
    let mut tmp = tempfile::NamedTempFile::new().expect("arquivo temporário");
    tmp.write_all(data).expect("gravar fixture");
    tmp.flush().expect("flush fixture");
    observe_media_raw(tmp.path()).expect("observação forense")
}
fn atom(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut result = ((body.len() + 8) as u32).to_be_bytes().to_vec();
    result.extend_from_slice(kind);
    result.extend_from_slice(body);
    result
}
fn ftyp() -> Vec<u8> {
    atom(b"ftyp", b"M4A \0\0\0\0")
}
fn with_mp4_body(atom_bytes: &[u8]) -> Vec<u8> {
    let mut bytes = ftyp();
    bytes.extend_from_slice(atom_bytes);
    bytes
}
fn flac_block(kind: u8, final_block: bool, content: &[u8]) -> Vec<u8> {
    let n = content.len();
    assert!(n < (1 << 24));
    let mut out = vec![
        kind | (if final_block { 0x80 } else { 0 }),
        (n >> 16) as u8,
        (n >> 8) as u8,
        n as u8,
    ];
    out.extend_from_slice(content);
    out
}

#[test]
fn mp4_nested_zero_size_must_not_use_parent_end() {
    let nested = [0, 0, 0, 0, b'f', b'r', b'e', b'e'];
    let x = observe(&with_mp4_body(&atom(b"moov", &nested)));
    assert!(x.warnings.iter().any(|w| w.contains("tamanho zero dentro")));
    assert_eq!(x.container, "mp4");
    assert!(!x.is_bc5_comparison);
}
#[test]
fn mp4_top_level_zero_size_is_not_misdiagnosed_as_nested() {
    let mut mdat = vec![0, 0, 0, 0, b'm', b'd', b'a', b't'];
    mdat.extend_from_slice(b"audio");
    let x = observe(&with_mp4_body(&mdat));
    assert!(!x.warnings.iter().any(|w| w.contains("tamanho zero")));
}
#[test]
fn mp4_trailing_bytes_in_container_are_reported() {
    let x = observe(&with_mp4_body(&atom(b"moov", &[1, 2, 3])));
    assert!(x
        .warnings
        .iter()
        .any(|w| w.contains("3 byte(s) sem cabeçalho completo")));
}
#[test]
fn mp4_trailing_bytes_at_top_level_are_reported() {
    let mut b = ftyp();
    b.extend_from_slice(&[9, 8, 7]);
    let x = observe(&b);
    assert!(x
        .warnings
        .iter()
        .any(|w| w.contains("3 byte(s) sem cabeçalho completo")));
}
#[test]
fn mp4_oversized_nested_atom_is_reported() {
    let x = observe(&with_mp4_body(&atom(
        b"moov",
        &[0, 0, 0, 50, b'f', b'r', b'e', b'e', 1],
    )));
    assert!(x.warnings.iter().any(|w| w.contains("átomo MP4 inválido")));
}
#[test]
fn mp4_nested_depth_limit_creates_warning() {
    let mut inner = atom(b"free", b"payload");
    for _ in 0..14 {
        inner = atom(b"moov", &inner);
    }
    let x = observe(&with_mp4_body(&inner));
    assert!(x
        .warnings
        .iter()
        .any(|w| w.contains("limite forense de profundidade")));
}
#[test]
fn mp4_valid_ilst_child_does_not_get_padding_warning() {
    let data = atom(b"data", &[0, 0, 0, 1, 0, 0, 0, 0, b'A']);
    let title = atom(b"trkn", &data);
    let ilst = atom(b"ilst", &title);
    let mut meta_body = vec![0, 0, 0, 0];
    meta_body.extend_from_slice(&ilst);
    let mp4 = with_mp4_body(&atom(b"moov", &atom(b"udta", &atom(b"meta", &meta_body))));
    let x = observe(&mp4);
    assert!(x
        .records
        .iter()
        .any(|r| r.source == "mp4/ilst" && r.key == "trkn"));
    assert!(x.warnings.is_empty(), "{:?}", x.warnings);
}
#[test]
fn flac_block_without_last_flag_is_incomplete() {
    let mut b = b"fLaC".to_vec();
    b.extend(flac_block(0, false, &[0; 34]));
    let x = observe(&b);
    assert!(x
        .warnings
        .iter()
        .any(|w| w.contains("não foi encontrado bloco final")));
}
#[test]
fn flac_last_flag_is_accepted_without_false_warning() {
    let mut b = b"fLaC".to_vec();
    b.extend(flac_block(0, true, &[0; 34]));
    let x = observe(&b);
    assert_eq!(x.container, "flac");
    assert!(x.warnings.is_empty(), "{:?}", x.warnings);
}
#[test]
fn flac_missing_metadata_block_warns() {
    let x = observe(b"fLaC");
    assert!(x
        .warnings
        .iter()
        .any(|w| w.contains("sem bloco de metadados")));
}
#[test]
fn flac_oversized_vorbis_field_count_is_bounded() {
    let mut b = b"fLaC".to_vec();
    let comments = [0, 0, 0, 0, 0xff, 0xff, 0xff, 0xff];
    b.extend(flac_block(4, true, &comments));
    let x = observe(&b);
    assert!(x
        .warnings
        .iter()
        .any(|w| w.contains("excedem o limite forense")));
    assert!(x.records.len() < 100);
}
#[test]
fn id3_nonzero_partial_frame_is_not_silent_padding() {
    let mut b = b"ID3\x04\0\0\0\0\0\x04".to_vec();
    b.extend_from_slice(b"TIT?");
    let x = observe(&b);
    assert!(x.warnings.iter().any(|w| w.contains("cabeçalho parcial")));
}
#[test]
fn id3_zero_padding_does_not_produce_partial_frame_warning() {
    let mut b = b"ID3\x04\0\0\0\0\0\x04".to_vec();
    b.extend_from_slice(&[0; 4]);
    let x = observe(&b);
    assert!(!x.warnings.iter().any(|w| w.contains("cabeçalho parcial")));
}
