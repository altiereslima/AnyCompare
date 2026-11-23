//! Regression guards for the strict BC5-facing profile. These tests do NOT
//! assert behavioral parity with the original application; they only prevent
//! prototype-specific forensic features leaking into normal tag comparisons.
use bc5_media::{inspect_media, InspectOptions};
use std::path::PathBuf;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

#[test]
fn file_size_is_file_info_not_a_comparable_tag() {
    let item = inspect_media(fixture("A.mp3"), InspectOptions::default()).unwrap();
    assert!(item.file_size > 0);
    assert!(!item.fields.contains_key("file.size_bytes"));
}

#[test]
fn cover_hash_is_not_automatically_a_bc5_tag() {
    let item = inspect_media(fixture("A_with_cover.mp3"), InspectOptions::default()).unwrap();
    assert!(!item.pictures.is_empty());
    assert!(!item.fields.contains_key("artwork.blake3"));
}

#[test]
fn only_the_documented_media_container_families_pass_default_inspection() {
    use bc5_media::MediaProfile;
    // An explicit default profile never opens arbitrary other containers.
    assert_eq!(InspectOptions::default().profile, MediaProfile::Bc5);
}
