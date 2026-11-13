use crate::model::{MediaError, MediaInfo, MediaSource, MediaTag, PictureInfo};
use crate::format::{MediaFormat, MediaProfile};
use lofty::file::{AudioFile, TaggedFileExt};
use lofty::tag::ItemValue;
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;

#[derive(Debug, Clone, Copy)]
pub struct InspectOptions {
    /// Reads the file through a streaming hash. Disabled by default for large libraries.
    pub include_file_hash: bool,
    /// Bc5 = the three documented formats; Extended = all formats handled by Lofty.
    pub profile: MediaProfile,
}
impl Default for InspectOptions {
    fn default() -> Self {
        Self { include_file_hash: false, profile: MediaProfile::Bc5 }
    }
}

fn key_alias(debug_name: &str) -> String {
    match debug_name {
        "TrackTitle" => "tag.title".into(),
        "TrackArtist" => "tag.artist".into(),
        "AlbumTitle" => "tag.album".into(),
        "AlbumArtist" => "tag.album_artist".into(),
        "TrackNumber" => "tag.track_number".into(),
        "TrackTotal" => "tag.track_total".into(),
        "DiscNumber" => "tag.disc_number".into(),
        "DiscTotal" => "tag.disc_total".into(),
        "Year" => "tag.year".into(),
        "RecordingDate" => "tag.recording_date".into(),
        "Genre" => "tag.genre".into(),
        "Comment" => "tag.comment".into(),
        "Composer" => "tag.composer".into(),
        "Lyrics" | "UnsyncLyrics" => "tag.lyrics".into(),
        "EncoderSoftware" | "EncodedBy" => "tag.encoder".into(),
        "Bpm" | "IntegerBpm" => "tag.bpm".into(),
        "CopyrightMessage" => "tag.copyright".into(),
        "Description" => "tag.description".into(),
        _ => format!("tag.{}", snake_case(debug_name)),
    }
}

fn snake_case(s: &str) -> String {
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if c.is_ascii_uppercase() && i > 0 && !out.ends_with('_') { out.push('_'); }
        if c.is_ascii_alphanumeric() { out.push(c.to_ascii_lowercase()); }
        else if !out.ends_with('_') { out.push('_'); }
    }
    out.trim_matches('_').to_owned()
}

fn digest_file(path: &Path) -> Result<String, MediaError> {
    let mut f = File::open(path)?;
    let mut h = blake3::Hasher::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let n = f.read(&mut buffer)?;
        if n == 0 { break; }
        h.update(&buffer[..n]);
    }
    Ok(h.finalize().to_hex().to_string())
}

pub fn inspect_media(path: impl AsRef<Path>, options: InspectOptions) -> Result<MediaInfo, MediaError> {
    let path = path.as_ref();
    let size = fs::metadata(path)?.len();
    // Sniff content; don't trust a mislabeled extension.
    let tagged = lofty::probe::Probe::open(path)?.guess_file_type()?.read()?;
    let detected = MediaFormat::from_lofty_debug(&format!("{:?}", tagged.file_type()));
    if options.profile == MediaProfile::Bc5 && !detected.officially_supported() {
        return Err(MediaError::Unsupported(format!("{:?} (somente MP3, FLAC e MP4/AAC no perfil BC5)", tagged.file_type())));
    }
    let mut info = MediaInfo {
        path: path.to_string_lossy().into_owned(), file_size: size,
        file_type: format!("{:?}", tagged.file_type()),
        fields: BTreeMap::new(), raw_tags: Vec::new(), tag_sources: Vec::new(),
        pictures: Vec::new(), warnings: Vec::new(),
    };
    if !detected.officially_supported() {
        info.warnings.push(format!("{:?}: disponível somente no perfil estendido, não confirmado no BC5 original", tagged.file_type()));
    }
    // File size belongs to the file-info pane, not to a media tag.
    // Do not invent a BC5 comparison row for it without a live reference.
    info.add_field("technical.format", info.file_type.clone());
    let props = tagged.properties();
    if !props.duration().is_zero() {
        info.add_field("technical.duration_ms", props.duration().as_millis().to_string());
    }
    if let Some(v) = props.overall_bitrate() { info.add_field("technical.overall_bitrate_kbps", v.to_string()); }
    if let Some(v) = props.audio_bitrate() { info.add_field("technical.audio_bitrate_kbps", v.to_string()); }
    if let Some(v) = props.sample_rate() { info.add_field("technical.sample_rate_hz", v.to_string()); }
    if let Some(v) = props.channels() { info.add_field("technical.channels", v.to_string()); }
    if let Some(v) = props.bit_depth() { info.add_field("technical.bit_depth", v.to_string()); }
    if options.include_file_hash {
        info.add_field("file.blake3", digest_file(path)?);
    }

    for tag in tagged.tags() {
        let format = format!("{:?}", tag.tag_type());
        info.tag_sources.push(MediaSource { format: format.clone(), items: tag.items().len(), pictures: tag.pictures().len() });
        for item in tag.items() {
            let key_debug = format!("{:?}", item.key());
            let value = match item.value() {
                ItemValue::Text(s) | ItemValue::Locator(s) => s.to_owned(),
                ItemValue::Binary(bytes) => format!("[binary: {} bytes, blake3={}]", bytes.len(), blake3::hash(bytes).to_hex()),
            };
            info.add_field(key_alias(&key_debug), value.clone());
            info.raw_tags.push(MediaTag { format: format.clone(), key: key_debug, value });
        }
        for picture in tag.pictures() {
            let bytes = picture.data();
            let p = PictureInfo {
                index: info.pictures.len(), tag_format: format.clone(),
                picture_type: format!("{:?}", picture.pic_type()),
                mime: picture.mime_type().map(|m| format!("{:?}", m)),
                description: picture.description().map(ToOwned::to_owned),
                size_bytes: bytes.len(), blake3: blake3::hash(bytes).to_hex().to_string(),
            };
            // The cover digest is forensic metadata only. The original BC5
            // artwork comparison behavior has not been established yet.
            // Do not present a BLAKE3 digest as a native BC5 comparison row.
            info.pictures.push(p);
        }
    }
    if info.tag_sources.is_empty() { info.warnings.push("Arquivo sem tags legíveis".into()); }
    info.finish();
    Ok(info)
}

/// Writes the selected cover without mutating the original media.
/// Caller must provide the destination file path. Returns its media metadata.
pub fn extract_cover_art(path: impl AsRef<Path>, index: usize, dest: impl AsRef<Path>) -> Result<PictureInfo, MediaError> {
    let tagged = lofty::probe::Probe::open(path.as_ref())?.guess_file_type()?.read()?;
    let mut found = 0_usize;
    for tag in tagged.tags() {
        for picture in tag.pictures() {
            if found == index {
                let p = PictureInfo {
                    index, tag_format: format!("{:?}", tag.tag_type()),
                    picture_type: format!("{:?}", picture.pic_type()),
                    mime: picture.mime_type().map(|m| format!("{:?}", m)),
                    description: picture.description().map(ToOwned::to_owned),
                    size_bytes: picture.data().len(),
                    blake3: blake3::hash(picture.data()).to_hex().to_string(),
                };
                // create_new prevents overwriting the audio itself, an existing
                // cover, or an existing symlink/hardlink to another file.
                let mut file = OpenOptions::new().write(true).create_new(true).open(dest.as_ref())?;
                file.write_all(picture.data())?;
                file.flush()?;
                return Ok(p);
            }
            found += 1;
        }
    }
    Err(MediaError::BadPictureIndex(index))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn aliases_align_across_formats() {
        assert_ne!(key_alias("Year"), key_alias("RecordingDate"));
        assert_eq!(key_alias("Year"), "tag.year");
        assert_eq!(key_alias("RecordingDate"), "tag.recording_date");
        assert_eq!(key_alias("TrackTitle"), "tag.title");
        assert_eq!(snake_case("ReplayGainTrackGain"), "replay_gain_track_gain");
    }
}
