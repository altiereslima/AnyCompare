//! ID3v2.3/2.4 boundaries, extended header and footers.
//! PARIDADE BC5: NÃO COMPROVADA. Testes exclusivamente de parsing RAW/diagnóstico.
use bc5_media::{observe_media_raw, observe_media_interpreted, interpret_raw_observation};
use std::path::PathBuf;

fn media(case: &str, side: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .join("TESTES_MEDIA/ETAPA12_ID3_LIMITES")
        .join(format!("{case}_{side}.mp3"))
}

#[test]
fn valid_footer_keeps_exact_offset_and_raw_bytes() {
    for side in ["A", "B"] {
        let raw=observe_media_raw(media("01_footer_valid",side)).unwrap();
        let head=raw.records.iter().find(|r|r.key=="HEADER" && r.source=="id3v2.4").unwrap();
        let frame=raw.records.iter().find(|r|r.key=="TIT2" && r.source=="id3v2.4").unwrap();
        let foot=raw.records.iter().find(|r|r.key=="FOOTER" && r.source=="id3v2.4").unwrap();
        assert_eq!(head.offset,0);
        assert_eq!(head.id3_header_offset,Some(0));
        assert_eq!(frame.id3_header_offset,Some(0));
        assert_eq!(foot.id3_header_offset,Some(0));
        assert_eq!(foot.value_length,10);
        assert!(foot.offset>frame.offset);
        assert!(!raw.warnings.iter().any(|w|w.contains("footer inválido")));
        assert!(!raw.is_bc5_comparison);
    }
}

#[test]
fn missing_footer_never_consumes_audio_as_footer() {
    let a=observe_media_raw(media("02_footer_missing","A")).unwrap();
    let b=observe_media_raw(media("02_footer_missing","B")).unwrap();
    assert_eq!(a.records.iter().filter(|r|r.key=="FOOTER").count(),1);
    assert_eq!(b.records.iter().filter(|r|r.key=="FOOTER").count(),0);
    assert!(b.warnings.iter().any(|w|w.contains("footer inválido") || w.contains("footer declarado")));
    assert_eq!(b.container,"mpeg-audio-possible");
}

#[test]
fn footer_signature_version_and_size_must_match_header() {
    for case in ["03_footer_signature","09_footer_wrong_version","10_footer_wrong_size"] {
        let broken=observe_media_raw(media(case,"B")).unwrap();
        assert!(!broken.records.iter().any(|r|r.key=="FOOTER"));
        assert!(broken.warnings.iter().any(|w|w.contains("footer inválido")),"{case}");
    }
}

#[test]
fn consecutive_v23_v24_headers_retain_absolute_offsets_and_occurrences() {
    let raw=observe_media_raw(media("04_two_tags","A")).unwrap();
    let headers:Vec<_>=raw.records.iter().filter(|r|r.key=="HEADER").collect();
    assert_eq!(headers.len(),2);
    assert_eq!(headers[0].offset,0);
    assert!(headers[1].offset>headers[0].offset);
    assert_eq!(headers[0].source,"id3v2.3");
    assert_eq!(headers[1].source,"id3v2.4");
    for frame in raw.records.iter().filter(|r|r.key=="TIT2") {
        let owner=frame.id3_header_offset.unwrap();
        assert!(headers.iter().any(|h|h.offset==owner && h.source==frame.source));
        assert!(frame.offset>owner);
    }
}

#[test]
fn extended_headers_obey_version_specific_length() {
    for (case,version,minimum) in [
        ("05_extended_v23","id3v2.3",10),
        ("06_extended_v24","id3v2.4",6),
    ] {
        let raw=observe_media_raw(media(case,"A")).unwrap();
        let ext=raw.records.iter().find(|r|r.key=="EXTENDED_HEADER" && r.source==version).unwrap();
        assert_eq!(ext.value_length,minimum, "{case}");
        assert_eq!(ext.offset,10);
        assert!(raw.records.iter().any(|r|r.key=="TIT2" && r.source==version));
    }
}

#[test]
fn short_extended_header_is_rejected_without_fake_frame() {
    let raw=observe_media_raw(media("07_extended_too_short","A")).unwrap();
    assert!(raw.warnings.iter().any(|w|w.contains("extended header menor que")));
    assert!(!raw.records.iter().any(|r|r.key=="TIT2"));
}

#[test]
fn unsynchronisation_flags_belong_to_the_correct_id3_tag() {
    let raw=observe_media_raw(media("08_unsync_per_tag","A")).unwrap();
    let tags=observe_media_interpreted(media("08_unsync_per_tag","A")).unwrap();
    let mut frames=tags.tags.iter().filter(|t|t.raw_key=="TIT2");
    let first=frames.next().unwrap();
    let second=frames.next().unwrap();
    assert!(frames.next().is_none());
    assert!(first.binary && first.values.is_empty(),"primeira tag global_unsync não deve ser decodificada sem remover unsync");
    assert!(second.values.iter().any(|s|s=="Segunda"),"segunda tag não possui unsync");
    assert_eq!(raw.records.iter().filter(|r|r.key=="HEADER").count(),2);
    assert!(!tags.matches_bc5);
}

#[test]
fn old_json_snapshots_still_load_without_new_id3_anchor() {
    let path=media("04_two_tags","A");
    let raw=observe_media_raw(&path).unwrap();
    let mut json=serde_json::to_value(&raw).unwrap();
    for record in json["records"].as_array_mut().unwrap() {
        record.as_object_mut().unwrap().remove("id3_header_offset");
    }
    let old:bc5_media::RawMediaObservation=serde_json::from_value(json).unwrap();
    let observed=interpret_raw_observation(&old,&path).unwrap();
    assert!(old.records.iter().all(|r|r.id3_header_offset.is_none()));
    assert!(observed.tags.iter().any(|t|t.raw_key=="TIT2"));
    assert!(!observed.matches_bc5);
}
