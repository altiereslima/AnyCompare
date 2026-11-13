use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, thiserror::Error)]
pub enum MediaError {
    #[error("I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("falha ao interpretar arquivo de mídia: {0}")]
    Parse(#[from] lofty::error::FileParseError),
    #[error("formato de mídia sem suporte: {0}")]
    Unsupported(String),
    #[error("índice de capa inválido: {0}")]
    BadPictureIndex(usize),
    #[error("JSON: {0}")]
    Json(#[from] serde_json::Error),
}

/// Semantic fields used by the provisional comparator. Values retain original order and multiplicity.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MediaField {
    pub key: String,
    pub values: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MediaTag {
    /// Original tag representation (e.g., Id3v2 or VorbisComments).
    pub format: String,
    /// Raw generic Lofty ItemKey label for diagnostics.
    pub key: String,
    /// Safe string representation; binary data is summarized by length and digest.
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MediaSource {
    pub format: String,
    pub items: usize,
    pub pictures: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PictureInfo {
    pub index: usize,
    pub tag_format: String,
    pub picture_type: String,
    pub mime: Option<String>,
    pub description: Option<String>,
    pub size_bytes: usize,
    pub blake3: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MediaInfo {
    pub path: String,
    pub file_size: u64,
    pub file_type: String,
    pub fields: BTreeMap<String, Vec<String>>,
    pub raw_tags: Vec<MediaTag>,
    pub tag_sources: Vec<MediaSource>,
    pub pictures: Vec<PictureInfo>,
    pub warnings: Vec<String>,
}

impl MediaInfo {
    pub fn get(&self, key: &str) -> Option<&[String]> {
        self.fields.get(key).map(Vec::as_slice)
    }
    pub fn add_field(&mut self, key: impl Into<String>, value: impl Into<String>) {
        let value = value.into();
        if value.trim().is_empty() { return; }
        self.fields.entry(key.into()).or_default().push(value);
    }
    /// Retained for source compatibility with earlier stages. Prior versions
    /// sorted and deduplicated tags here, permanently destroying evidence.
    /// Canonicalization is now applied ONLY by a comparison policy, never
    /// by the data model. This does not establish the original BC5 policy.
    pub fn finish(&mut self) {}
}

#[cfg(test)]
mod stage22_evidence_tests {
    use super::*;
    #[test]
    fn finish_preserves_order_and_duplicate_occurrences() {
        let mut m=MediaInfo{path:"sample".into(),file_size:0,file_type:"Flac".into(),
            fields:BTreeMap::new(),raw_tags:vec![],tag_sources:vec![],pictures:vec![],warnings:vec![]};
        for value in ["B", "A", "A"] { m.add_field("tag.artist",value); }
        m.finish();
        assert_eq!(m.get("tag.artist").unwrap().iter().map(String::as_str).collect::<Vec<_>>(),vec!["B","A","A"]);
        let json=serde_json::to_string(&m).unwrap();
        let restored: MediaInfo=serde_json::from_str(&json).unwrap();
        assert_eq!(restored.get("tag.artist").unwrap().iter().map(String::as_str).collect::<Vec<_>>(),vec!["B","A","A"]);
    }
}
