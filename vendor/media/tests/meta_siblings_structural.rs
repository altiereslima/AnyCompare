//! MP4 com átomos `meta` irmãos: mesma hierarquia textual, identidades distintas.
//! Estes testes certificam somente o parser/observador Rust, NÃO a igualdade BC5.
use bc5_media::{interpret_raw_observation, observe_media_interpreted, observe_media_raw};
use std::collections::BTreeSet;
use std::path::PathBuf;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .join("TESTES_MEDIA/ETAPA11_META_IRMAS")
        .join(name)
}

#[test]
fn sibling_meta_boxes_keep_distinct_offsets_despite_same_textual_path() {
    let raw = observe_media_raw(fixture("01_orders_A.m4a")).unwrap();
    assert_eq!(raw.container, "mp4");
    let keys: Vec<_> = raw
        .records
        .iter()
        .filter(|r| r.source == "mp4/auxiliary" && r.key.ends_with("/keys"))
        .collect();
    let ilsts: Vec<_> = raw
        .records
        .iter()
        .filter(|r| r.source == "mp4/ilst" && r.key == "0x00000001")
        .collect();
    assert_eq!(keys.len(), 2);
    assert_eq!(ilsts.len(), 2);
    assert_eq!(keys[0].container_path, keys[1].container_path);
    let owners: BTreeSet<_> = keys.iter().map(|r| r.meta_header_offset.unwrap()).collect();
    assert_eq!(owners.len(), 2);
    for item in ilsts {
        assert!(owners.contains(&item.meta_header_offset.unwrap()));
    }
}

#[test]
fn sibling_meta_ilst_indices_resolve_to_the_correct_sibling() {
    for side in ["A", "B"] {
        let decoded = observe_media_interpreted(fixture(&format!("01_orders_{side}.m4a"))).unwrap();
        let tags: Vec<_> = decoded
            .tags
            .iter()
            .filter(|t| t.source == "mp4/ilst")
            .collect();
        assert_eq!(tags.len(), 2);
        assert!(tags.iter().any(|t| t.diagnostic_key == "mdta:com.bc5.alpha"
            && t.values.len() == 1
            && t.values[0] == "Primeiro"));
        assert!(tags.iter().any(|t| t.diagnostic_key == "mdta:com.bc5.beta"
            && t.values.len() == 1
            && t.values[0] == "Segundo"));
        assert!(!decoded.matches_bc5);
    }
}

#[test]
fn sibling_meta_change_is_visible_without_bc5_equivalence_claim() {
    let left = observe_media_interpreted(fixture("02_value_A.m4a")).unwrap();
    let right = observe_media_interpreted(fixture("02_value_B.m4a")).unwrap();
    assert!(left
        .tags
        .iter()
        .any(|t| t.diagnostic_key == "mdta:com.bc5.beta"
            && t.values.len() == 1
            && t.values[0] == "Antes"));
    assert!(right
        .tags
        .iter()
        .any(|t| t.diagnostic_key == "mdta:com.bc5.beta"
            && t.values.len() == 1
            && t.values[0] == "Depois"));
    assert!(!left.matches_bc5 && !right.matches_bc5);
}

#[test]
fn identical_sibling_paths_without_offsets_stay_ambiguous_in_legacy_snapshot() {
    let path = fixture("01_orders_A.m4a");
    let mut legacy = observe_media_raw(&path).unwrap();
    for r in &mut legacy.records {
        r.meta_header_offset = None;
    }
    let decoded = interpret_raw_observation(&legacy, &path).unwrap();
    let ilsts: Vec<_> = decoded
        .tags
        .iter()
        .filter(|t| t.source == "mp4/ilst")
        .collect();
    assert_eq!(ilsts.len(), 2);
    assert!(ilsts
        .iter()
        .all(|t| t.notes.iter().any(|n| n.contains("múltiplas keys"))));
    assert!(ilsts.iter().all(|t| t.diagnostic_key == "0x00000001"));
}

#[test]
fn legacy_json_without_meta_offset_is_still_deserializable() {
    let raw = observe_media_raw(fixture("01_orders_A.m4a")).unwrap();
    let mut value = serde_json::to_value(&raw).unwrap();
    let records = value.get_mut("records").unwrap().as_array_mut().unwrap();
    for record in records {
        record.as_object_mut().unwrap().remove("meta_header_offset");
    }
    let old: bc5_media::RawMediaObservation = serde_json::from_value(value).unwrap();
    assert!(old.records.iter().all(|r| r.meta_header_offset.is_none()));
}

#[test]
fn forged_oversized_raw_snapshot_is_rejected_before_reading_entire_file() {
    let path = fixture("01_orders_A.m4a");
    let mut forged = observe_media_raw(&path).unwrap();
    forged.file_size = u64::MAX;
    let err = interpret_raw_observation(&forged, &path).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
}

#[test]
fn unicode_and_duplicate_keys_preserve_both_siblings() {
    let unicode = observe_media_interpreted(fixture("03_unicode_A.m4a")).unwrap();
    assert!(unicode
        .tags
        .iter()
        .any(|t| t.diagnostic_key == "mdta:com.bc5.título"
            && t.values.len() == 1
            && t.values[0] == "São Paulo"));
    let repeated = observe_media_interpreted(fixture("04_repeated_A.m4a")).unwrap();
    let values: Vec<_> = repeated
        .tags
        .iter()
        .filter(|t| t.diagnostic_key == "mdta:com.bc5.same")
        .flat_map(|t| t.values.iter().map(String::as_str))
        .collect();
    assert_eq!(values, vec!["A", "B"]);
}
