//! Regressão da Etapa 15: proteção do diagnóstico contra snapshots JSON
//! com metadados inventados. NÃO estabelece igualdade/semântica do BC5.
use bc5_media::{interpret_raw_observation, observe_media_raw};
use std::io::Write;

fn file_with(data: &[u8]) -> tempfile::NamedTempFile {
    let mut file = tempfile::NamedTempFile::new().expect("fixture");
    file.write_all(data).expect("write");
    file.flush().expect("flush");
    file
}
fn simple_id3() -> Vec<u8> {
    let payload = b"\x03Titulo";
    let mut frame = b"TIT2".to_vec();
    frame.extend([0, 0, 0, payload.len() as u8, 0, 0]);
    frame.extend(payload);
    let mut tag = b"ID3\x04\x00\x00\x00\x00\x00".to_vec();
    tag.push(frame.len() as u8); // synchsafe, fixture muito curta
    tag.extend(frame);
    tag.extend([0xff, 0xfb, 0x90, 0x64]);
    tag
}
fn atom(name: &[u8; 4], value: &[u8]) -> Vec<u8> {
    let mut b = ((value.len() + 8) as u32).to_be_bytes().to_vec();
    b.extend(name);
    b.extend(value);
    b
}
fn simple_mp4() -> Vec<u8> {
    let mut meta = vec![0, 0, 0, 0];
    let data = atom(b"data", &[0, 0, 0, 1, 0, 0, 0, 0, b'Z']);
    meta.extend(atom(b"ilst", &atom(b"\xa9nam", &data)));
    let mut file = atom(b"ftyp", b"M4A \0\0\0\0");
    file.extend(atom(b"moov", &atom(b"udta", &atom(b"meta", &meta))));
    file
}
#[test]
fn genuine_id3_snapshot_is_accepted_and_keeps_title() {
    let f = file_with(&simple_id3());
    let raw = observe_media_raw(f.path()).unwrap();
    let interpreted = interpret_raw_observation(&raw, f.path()).unwrap();
    assert!(interpreted
        .tags
        .iter()
        .any(|t| t.raw_key == "TIT2" && t.values == vec!["Titulo".to_string()]));
    assert!(!interpreted.matches_bc5);
}
#[test]
fn genuine_mp4_snapshot_is_accepted() {
    let f = file_with(&simple_mp4());
    let raw = observe_media_raw(f.path()).unwrap();
    let interpreted = interpret_raw_observation(&raw, f.path()).unwrap();
    assert!(interpreted
        .tags
        .iter()
        .any(|t| t.source == "mp4/ilst" && t.values == vec!["Z".to_string()]));
}
#[test]
fn snapshot_that_relabels_real_id3_bytes_is_rejected() {
    let f = file_with(&simple_id3());
    let mut raw = observe_media_raw(f.path()).unwrap();
    raw.records
        .iter_mut()
        .find(|r| r.key == "TIT2")
        .unwrap()
        .key = "TPE1".into();
    assert!(interpret_raw_observation(&raw, f.path()).is_err());
}
#[test]
fn snapshot_that_changes_occurrence_is_rejected() {
    let f = file_with(&simple_id3());
    let mut raw = observe_media_raw(f.path()).unwrap();
    raw.records
        .iter_mut()
        .find(|r| r.key == "TIT2")
        .unwrap()
        .occurrence = 99;
    assert!(interpret_raw_observation(&raw, f.path()).is_err());
}
#[test]
fn snapshot_with_duplicate_id3_record_is_rejected() {
    let f = file_with(&simple_id3());
    let mut raw = observe_media_raw(f.path()).unwrap();
    let copy = raw
        .records
        .iter()
        .find(|r| r.key == "TIT2")
        .unwrap()
        .clone();
    raw.records.push(copy);
    assert!(interpret_raw_observation(&raw, f.path()).is_err());
}
#[test]
fn snapshot_with_forged_mp4_parent_is_rejected() {
    let f = file_with(&simple_mp4());
    let mut raw = observe_media_raw(f.path()).unwrap();
    let entry = raw
        .records
        .iter_mut()
        .find(|r| r.source == "mp4/ilst")
        .unwrap();
    entry.container_path = Some("mp4/moov/inventado/ilst".into());
    assert!(interpret_raw_observation(&raw, f.path()).is_err());
}
#[test]
fn snapshot_with_forged_meta_identity_is_rejected() {
    let f = file_with(&simple_mp4());
    let mut raw = observe_media_raw(f.path()).unwrap();
    let entry = raw
        .records
        .iter_mut()
        .find(|r| r.source == "mp4/ilst")
        .unwrap();
    entry.meta_header_offset = Some(u64::MAX);
    assert!(interpret_raw_observation(&raw, f.path()).is_err());
}
#[test]
fn old_snapshot_missing_optional_provenance_still_loads() {
    let f = file_with(&simple_mp4());
    let raw = observe_media_raw(f.path()).unwrap();
    let mut json = serde_json::to_value(&raw).unwrap();
    for record in json["records"].as_array_mut().unwrap() {
        let map = record.as_object_mut().unwrap();
        map.remove("container_path");
        map.remove("meta_header_offset");
        map.remove("id3_header_offset");
    }
    json.as_object_mut().unwrap().remove("file_blake3");
    let historical = serde_json::from_value(json).unwrap();
    let interpreted = interpret_raw_observation(&historical, f.path()).unwrap();
    assert!(interpreted
        .warnings
        .iter()
        .any(|s| s.contains("snapshot RAW legado")));
}
#[test]
fn whole_file_hash_mismatch_is_still_rejected() {
    let f = file_with(&simple_id3());
    let mut raw = observe_media_raw(f.path()).unwrap();
    raw.file_blake3 = Some("0".repeat(64));
    assert!(interpret_raw_observation(&raw, f.path()).is_err());
}
