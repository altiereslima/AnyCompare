//! MP4/mdta real (FFmpeg `use_metadata_tags`). Testes do observador,
//! NÃO são resultados de igualdade ou importância do Beyond Compare 5.
use bc5_media::{compare_raw_observations, observe_media_interpreted, observe_media_raw};
use std::path::PathBuf;

fn file(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .join("TESTES_MEDIA/ETAPA10_MDTA")
        .join(name)
}

#[test]
fn mdta_table_resolves_real_utf8_field_names() {
    let path=file("mdta_02_value_A.m4a");
    let raw=observe_media_raw(&path).unwrap();
    assert_eq!(raw.container, "mp4");
    assert!(raw.records.iter().any(|r| r.source=="mp4/auxiliary" && r.key.ends_with("/keys")));
    assert!(raw.records.iter().any(|r| r.source=="mp4/ilst" && r.key=="0x00000001"));
    let interpreted=observe_media_interpreted(path).unwrap();
    let title=interpreted.tags.iter().find(|t| t.diagnostic_key=="mdta:com.example.title").unwrap();
    assert_eq!(title.values, vec!["Título A"]);
    assert!(!interpreted.matches_bc5);
}

#[test]
fn mdta_value_change_detected_only_as_raw_difference() {
    let left=observe_media_raw(file("mdta_02_value_A.m4a")).unwrap();
    let right=observe_media_raw(file("mdta_02_value_B.m4a")).unwrap();
    let diff=compare_raw_observations(&left,&right);
    assert!(diff.different>0);
    assert!(diff.matches_bc5.is_none());
}

#[test]
fn mdta_unchanged_is_raw_identical() {
    let left=observe_media_raw(file("mdta_01_identical_A.m4a")).unwrap();
    let right=observe_media_raw(file("mdta_01_identical_B.m4a")).unwrap();
    let diff=compare_raw_observations(&left,&right);
    assert_eq!(diff.different,0);
    assert!(diff.matches_bc5.is_none());
}

#[test]
fn mdta_missing_key_preserves_unmapped_difference() {
    let left=observe_media_interpreted(file("mdta_03_missing_key_A.m4a")).unwrap();
    let right=observe_media_interpreted(file("mdta_03_missing_key_B.m4a")).unwrap();
    assert!(!left.tags.iter().any(|t|t.diagnostic_key=="mdta:com.example.artist"));
    assert!(right.tags.iter().any(|t|t.diagnostic_key=="mdta:com.example.artist"));
}

#[test]
fn mdta_order_does_not_change_identity_of_decoded_key() {
    for side in ["A", "B"] {
        let x=observe_media_interpreted(file(&format!("mdta_04_order_unicode_{side}.m4a"))).unwrap();
        assert!(x.tags.iter().any(|t|t.diagnostic_key=="mdta:com.example.artist" && t.values==vec!["Coração".to_string()]));
        assert!(x.tags.iter().any(|t|t.diagnostic_key=="mdta:com.example.title" && t.values==vec!["Água".to_string()]));
        assert!(!x.matches_bc5);
    }
}
