//! Arquivos de teste sintéticos; nenhum áudio/asset de terceiros incluído.
use bc5_media::{compare_media, inspect_media, InspectOptions, MediaProfile, MediaRules};
use std::io::Write;
use std::path::Path;

fn make_wav(path: &Path, title: &str) {
    // PCM 16-bit mono / 8 kHz, com uma tag LIST/INFO INAM.
    let samples = vec![0_u8; 800];
    let mut info = Vec::new();
    info.extend_from_slice(b"INAM");
    let mut name = title.as_bytes().to_vec();
    name.push(0);
    info.extend_from_slice(&(name.len() as u32).to_le_bytes());
    info.extend_from_slice(&name);
    if name.len() % 2 != 0 { info.push(0); }
    let mut list = Vec::new();
    list.extend_from_slice(b"INFO");
    list.extend_from_slice(&info);

    let mut riff = Vec::new();
    riff.extend_from_slice(b"WAVEfmt ");
    riff.extend_from_slice(&16_u32.to_le_bytes());
    riff.extend_from_slice(&1_u16.to_le_bytes());
    riff.extend_from_slice(&1_u16.to_le_bytes());
    riff.extend_from_slice(&8000_u32.to_le_bytes());
    riff.extend_from_slice(&16000_u32.to_le_bytes());
    riff.extend_from_slice(&2_u16.to_le_bytes());
    riff.extend_from_slice(&16_u16.to_le_bytes());
    riff.extend_from_slice(b"data");
    riff.extend_from_slice(&(samples.len() as u32).to_le_bytes());
    riff.extend_from_slice(&samples);
    riff.extend_from_slice(b"LIST");
    riff.extend_from_slice(&(list.len() as u32).to_le_bytes());
    riff.extend_from_slice(&list);
    let mut file = std::fs::File::create(path).unwrap();
    file.write_all(b"RIFF").unwrap();
    file.write_all(&(riff.len() as u32).to_le_bytes()).unwrap();
    file.write_all(&riff).unwrap();
}

#[test]
fn reads_two_synthetic_pcm_files_and_compares_their_metadata() {
    let dir = tempfile::tempdir().unwrap();
    let left_path = dir.path().join("left.wav");
    let right_path = dir.path().join("right.wav");
    make_wav(&left_path, "Exemplo A");
    make_wav(&right_path, "Exemplo B");
    assert!(inspect_media(&left_path, InspectOptions::default()).is_err());
    let extended = InspectOptions { include_file_hash: false, profile: MediaProfile::Extended };
    let left = inspect_media(&left_path, extended).unwrap();
    let right = inspect_media(&right_path, extended).unwrap();
    assert_eq!(left.file_type, right.file_type);
    assert!(left.file_size > 800);
    let comparison = compare_media(&left, &right, &MediaRules::default());
    assert!(!comparison.fields.is_empty());
}

#[test]
fn invalid_media_does_not_become_a_successful_session() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("invalid.mp3");
    std::fs::write(&path, b"not a valid audio stream").unwrap();
    assert!(inspect_media(path, InspectOptions::default()).is_err());
}
