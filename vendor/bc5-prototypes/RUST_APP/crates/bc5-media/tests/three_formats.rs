//! Regression corpus synthesized by FFmpeg: a 440 Hz sine tone (no third-party music).
use bc5_media::{compare_media, inspect_media, FieldStatus, InspectOptions, MediaRules};
use std::path::PathBuf;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)
}

#[test]
fn all_three_official_bc5_formats_read_real_audio_tags() {
    for extension in ["mp3", "flac", "m4a"] {
        let a = inspect_media(fixture(&format!("A.{extension}")), InspectOptions::default()).unwrap();
        let b = inspect_media(fixture(&format!("B.{extension}")), InspectOptions::default()).unwrap();
        assert_eq!(a.get("tag.title").expect("titulo A")[0], "Teste A", "{extension}");
        assert_eq!(b.get("tag.title").expect("titulo B")[0], "Teste B", "{extension}");
        assert!(!a.pictures.iter().any(|p| p.size_bytes == 0));
        let compared = compare_media(&a, &b, &MediaRules::default());
        let title = compared.fields.iter().find(|f| f.key == "tag.title").unwrap();
        assert_eq!(title.status, FieldStatus::ImportantDifference, "{extension}");
    }
}

#[test]
fn repeated_inspection_is_deterministic_and_original_is_unchanged() {
    let path = fixture("A.mp3");
    let before = std::fs::read(&path).unwrap();
    let a = inspect_media(&path, InspectOptions::default()).unwrap();
    let b = inspect_media(&path, InspectOptions::default()).unwrap();
    assert_eq!(a.fields, b.fields);
    assert_eq!(before, std::fs::read(&path).unwrap());
}

#[test]
fn invalid_extension_is_not_used_as_truth() {
    let dir = tempfile::tempdir().unwrap();
    let fake = dir.path().join("audio.mp3");
    std::fs::copy(fixture("A.flac"), &fake).unwrap();
    let media = inspect_media(&fake, InspectOptions::default()).unwrap();
    assert_eq!(media.file_type, "Flac", "o conteúdo prevalece sobre .mp3");
}

#[test]
fn embedded_cover_extracts_without_overwriting_any_file() {
    use bc5_media::extract_cover_art;
    let source = fixture("A_with_cover.mp3");
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("cover.jpg");
    let inspected = inspect_media(&source, InspectOptions::default()).unwrap();
    assert!(!inspected.pictures.is_empty(), "imagem embutida no MP3");
    let picture = extract_cover_art(&source, 0, &dest).unwrap();
    assert!(picture.size_bytes > 0);
    let bytes = std::fs::read(&dest).unwrap();
    assert_eq!(&bytes[..2], &[0xff, 0xd8]);
    // A second extraction must not overwrite the existing file.
    assert!(extract_cover_art(&source, 0, &dest).is_err());
    // Accidental extraction to the source path must not destroy it.
    let before = std::fs::read(&source).unwrap();
    assert!(extract_cover_art(&source, 0, &source).is_err());
    assert_eq!(before, std::fs::read(&source).unwrap());
}
