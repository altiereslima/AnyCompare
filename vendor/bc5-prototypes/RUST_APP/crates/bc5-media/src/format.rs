//! Media support policy for Beyond Compare 5 parity and optional extra formats.
//! Based on BC5 v5 help: the documented core is MP3, FLAC and MP4/AAC.
//! Lofty can parse more; those are *extensions*, not claimed BC5 features.
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MediaProfile {
    #[default]
    Bc5,
    Extended,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaFormat {
    Mp3,
    Flac,
    Mp4Aac,
    Other,
}

impl MediaFormat {
    pub fn from_lofty_debug(name: &str) -> Self {
        match name {
            "Mpeg" => Self::Mp3,
            "Flac" => Self::Flac,
            "Mp4" => Self::Mp4Aac,
            _ => Self::Other,
        }
    }

    pub fn officially_supported(self) -> bool {
        self != Self::Other
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Mp3 => "MP3",
            Self::Flac => "FLAC",
            Self::Mp4Aac => "MP4/AAC",
            Self::Other => "Outro formato (extensão)",
        }
    }
}

/// Extension-based routing is only a hint; actual decoding sniffs the bytes.
/// Avoid auto-routing MP4/M4V videos, which can contain video streams.
pub fn is_media_extension(path: &Path, profile: MediaProfile) -> bool {
    let ext = path.extension().and_then(|x| x.to_str()).unwrap_or("").to_ascii_lowercase();
    // Conservative auto-routing: these three suffixes were tested locally.
    // This is not a claim that BC5 rejects other extensions (e.g. MP4/M4B).
    let bc5 = matches!(ext.as_str(), "mp3" | "flac" | "m4a");
    bc5 || (profile == MediaProfile::Extended && matches!(ext.as_str(),
        "wav" | "wave" | "aiff" | "aif" | "aac" | "ogg" | "opus" | "ape" | "wv" | "mpc" | "spx"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn official_core_and_optional_extensions_are_distinct() {
        assert!(MediaFormat::from_lofty_debug("Mp4").officially_supported());
        assert!(!MediaFormat::from_lofty_debug("Wav").officially_supported());
        assert!(is_media_extension(Path::new("demo.FLAC"), MediaProfile::Bc5));
        assert!(!is_media_extension(Path::new("demo.wav"), MediaProfile::Bc5));
        assert!(is_media_extension(Path::new("demo.wav"), MediaProfile::Extended));
        assert!(!is_media_extension(Path::new("movie.mp4"), MediaProfile::Bc5));
    }
}
