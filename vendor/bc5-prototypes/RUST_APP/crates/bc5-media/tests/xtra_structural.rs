//! Sintéticos com átomo Xtra artificial. Não validam semântica do BC5.
use bc5_media::{compare_raw_observations, observe_media_raw};
use std::path::PathBuf;

fn corpus(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .join("TESTES_MEDIA/ETAPA9_XTRA")
        .join(name)
}
fn check_pair(name: &str, same_xtra: bool) {
    let left=observe_media_raw(corpus(&format!("{name}_A.m4a"))).unwrap();
    let right=observe_media_raw(corpus(&format!("{name}_B.m4a"))).unwrap();
    assert_eq!(left.container,"mp4");
    assert_eq!(right.container,"mp4");
    let out=compare_raw_observations(&left,&right);
    let xtra=out.entries.iter().find(|e|e.key.ends_with("/Xtra")).unwrap();
    assert_eq!(xtra.source,"mp4/auxiliary");
    assert_eq!(xtra.identical,same_xtra);
    assert!(out.matches_bc5.is_none());
}
#[test] fn xtra_unchanged_preserves_identity() {check_pair("xtra_01_unchanged",true);}
#[test] fn only_xtra_changed_is_detected_as_raw() {check_pair("xtra_02_only_xtra",false);}
#[test] fn ilst_difference_does_not_modify_xtra() {check_pair("xtra_03_title_only",true);}
#[test] fn both_xtra_and_ilst_changed_remain_separate() {check_pair("xtra_04_both",false);}
