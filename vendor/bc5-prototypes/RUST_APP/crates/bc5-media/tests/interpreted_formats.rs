//! Vetores extraídos do corpus sintético da Etapa 5.
//! Estes testes são de FORMATO, NÃO de equivalência com Beyond Compare 5.
use bc5_media::{observe_media_interpreted, observe_media_raw, interpret_raw_observation, InterpretedMediaObservation};
use std::fs;
use std::path::{Path, PathBuf};
fn corpus(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .join("TESTES_MEDIA/CASOS")
        .join(name)
}
fn read(name: &str) -> InterpretedMediaObservation {
    observe_media_interpreted(corpus(name)).unwrap()
}
fn values(obs: &InterpretedMediaObservation, key: &str) -> Vec<String> {
    obs.tags.iter().filter(|x| x.diagnostic_key==key)
        .flat_map(|x| x.values.clone()).collect()
}
#[test]
fn id3v22_title_is_not_mislabeled_v24() {
    let left=read("mp3_03_id3v22_A.mp3");
    let right=read("mp3_03_id3v22_B.mp3");
    assert_eq!(values(&left,"TT2"),vec!["Original"]);
    assert_eq!(values(&right,"TT2"),vec!["Alterado"]);
    assert!(left.tags.iter().any(|t| t.source=="id3v2.2" && t.raw_key=="TT2"));
    assert!(!left.matches_bc5);
}
#[test]
fn id3v23_and_id3v24_are_separate_sources() {
    let a=read("mp3_04_id3v23_A.mp3");
    let b=read("mp3_08_unicode_A.mp3");
    assert_eq!(values(&a,"TIT2"),vec!["Original"]);
    assert_eq!(values(&b,"TIT2"),vec!["AÇÃO"]);
    assert!(a.tags.iter().any(|x|x.source=="id3v2.3"));
    assert!(b.tags.iter().any(|x|x.source=="id3v2.4"));
}
#[test]
fn id3_conflicts_retain_both_sources() {
    let a=read("mp3_06_conflicting_versions_A.mp3");
    assert_eq!(values(&a,"TIT2"),vec!["Nova"]);
    assert!(a.tags.iter().any(|x|x.source=="id3v1" && x.raw_key=="TITLE"));
    assert!(!a.matches_bc5);
}
#[test]
fn vorbis_values_keep_multiplicity_and_order() {
    let a=read("flac_03_multivalue_A.flac");
    let b=read("flac_03_multivalue_B.flac");
    assert_eq!(values(&a,"ARTIST"),vec!["Ana","Bia"]);
    assert_eq!(values(&b,"ARTIST"),vec!["Ana"]);
    let dup=read("flac_05_duplicate_A.flac");
    assert_eq!(values(&dup,"ARTIST"),vec!["Ana","Ana"]);
}
#[test]
fn flac_streaminfo_is_observed_but_not_comparison_rule() {
    let obs=read("flac_01_unchanged_A.flac");
    assert!(obs.tags.iter().any(|t|t.raw_key=="STREAMINFO" &&
        t.values.iter().any(|v|v.starts_with("sample_rate="))));
    assert!(!obs.matches_bc5);
}
#[test]
fn mp4_title_track_and_disc_are_decoded() {
    let name=read("m4a_02_title_A.m4a");
    assert_eq!(values(&name,"0xa96e616d"),vec!["Alfa"]);
    let track=read("m4a_03_track_B.m4a");
    assert_eq!(values(&track,"trkn"),vec!["2/10"]);
    let disc=read("m4a_04_disc_B.m4a");
    assert_eq!(values(&disc,"disk"),vec!["1/3"]);
}
#[test]
fn mp4_freeform_retains_identity() {
    let a=read("m4a_06_freeform_A.m4a");
    let b=read("m4a_06_freeform_B.m4a");
    let key="----:com.apple.iTunes:TEST";
    assert_eq!(values(&a,key),vec!["One"]);
    assert_eq!(values(&b,key),vec!["Two"]);
}
#[test]
fn no_implicit_picture_equality() {
    let a=read("m4a_08_picture_A.m4a");
    let b=read("m4a_08_picture_B.m4a");
    assert!(a.tags.iter().any(|x|x.raw_key=="covr" && x.binary));
    assert!(b.tags.iter().any(|x|x.raw_key=="covr" && x.binary));
    assert!(!a.matches_bc5 && !b.matches_bc5);
}
#[test]
fn rejects_same_size_mutation_after_raw_snapshot() {
    let dir=tempfile::tempdir().unwrap();
    let path=dir.path().join("changed.flac");
    fs::copy(corpus("flac_02_title_A.flac"),&path).unwrap();
    let raw=observe_media_raw(&path).unwrap();
    let mut bytes=fs::read(&path).unwrap();
    let loc=bytes.windows(4).position(|w|w==b"Alfa").unwrap();
    bytes[loc..loc+4].copy_from_slice(b"Beta");
    fs::write(&path,bytes).unwrap();
    assert!(interpret_raw_observation(&raw,Path::new(&path)).is_err());
}

#[test]
fn full_snapshot_rejects_audio_data_mutation_not_just_metadata() {
    let temp=tempfile::tempdir().unwrap();
    let path=temp.path().join("audio_changed.flac");
    fs::copy(corpus("flac_01_unchanged_A.flac"),&path).unwrap();
    let raw=observe_media_raw(&path).unwrap();
    assert!(raw.file_blake3.is_some());
    let mut bytes=fs::read(&path).unwrap();
    // A última posição de um FLAC deste corpus fica nos quadros de áudio,
    // fora dos registros STREAMINFO/Vorbis. Mantém mesmo tamanho.
    let last=bytes.len()-1;
    assert!(raw.records.iter().all(|r| (last as u64)<r.offset ||
        (last as u64)>=r.offset+r.value_length as u64));
    bytes[last]^=1;
    fs::write(&path,bytes).unwrap();
    let err=interpret_raw_observation(&raw,&path).unwrap_err();
    assert!(err.to_string().contains("hash integral divergente"));
}

#[test]
fn legacy_snapshot_deserializes_but_warns_about_partial_integrity() {
    let src=corpus("flac_01_unchanged_A.flac");
    let raw=observe_media_raw(&src).unwrap();
    let mut old=serde_json::to_value(&raw).unwrap();
    old.as_object_mut().unwrap().remove("file_blake3");
    let old:bc5_media::RawMediaObservation=serde_json::from_value(old).unwrap();
    assert!(old.file_blake3.is_none());
    let obs=interpret_raw_observation(&old,&src).unwrap();
    assert!(obs.warnings.iter().any(|w|w.contains("snapshot RAW legado")));
    assert!(!obs.matches_bc5);
}
