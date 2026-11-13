//! Semantic, two-pane media-tag comparison.
use crate::model::MediaInfo;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldStatus {
    Same,
    ImportantDifference,
    UnimportantDifference,
}
impl FieldStatus {
    pub fn different(self) -> bool { self != Self::Same }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldComparison {
    pub key: String,
    pub left: Vec<String>,
    pub right: Vec<String>,
    pub important: bool,
    pub status: FieldStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaRules {
    /// A key present here is compared as an important difference.
    pub important: BTreeSet<String>,
    /// A key present here is compared as unimportant. Takes precedence over `important`.
    pub unimportant: BTreeSet<String>,
    /// A key present here is excluded from the comparison altogether.
    pub excluded: BTreeSet<String>,
    pub ignore_case: bool,
    pub trim_whitespace: bool,
    pub ignore_unimportant: bool,
}
impl Default for MediaRules {
    fn default() -> Self {
        Self {
            important: BTreeSet::new(),
            unimportant: BTreeSet::new(),
            excluded: BTreeSet::new(),
            ignore_case: false,
            trim_whitespace: false,
            ignore_unimportant: false,
        }
    }
}
impl MediaRules {
    /// A final * matches a prefix, e.g. tag.* or technical.*; plain keys match exactly.
    fn matches(patterns: &BTreeSet<String>, key: &str) -> bool {
        patterns.iter().any(|pattern| {
            if let Some(prefix) = pattern.strip_suffix('*') { key.starts_with(prefix) }
            else { pattern == key }
        })
    }
    pub fn is_important(&self, key: &str) -> bool {
        if Self::matches(&self.unimportant, key) { return false; }
        if Self::matches(&self.important, key) { return true; }
        // Provisional importance policy only. The exact BC5 default tree is
        // not known yet and MUST be calibrated against the original program.
        key.starts_with("tag.") || key.starts_with("artwork.")
    }
    pub fn set_importance(&mut self, key: impl Into<String>, value: bool) {
        let key = key.into();
        self.important.remove(&key);
        self.unimportant.remove(&key);
        if value { self.important.insert(key); } else { self.unimportant.insert(key); }
    }
    fn normalized(&self, values: &[String]) -> Vec<String> {
        let mut items: Vec<String> = values.iter().map(|v| {
            let v = if self.trim_whitespace { v.trim() } else { v.as_str() };
            if self.ignore_case { v.to_lowercase() } else { v.to_owned() }
        }).collect();
        items.sort();
        items.dedup();
        items
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaComparison {
    pub left: MediaInfo,
    pub right: MediaInfo,
    pub fields: Vec<FieldComparison>,
    pub same: usize,
    pub important_differences: usize,
    pub unimportant_differences: usize,
    pub ignore_unimportant: bool,
}
impl MediaComparison {
    pub fn effective_different(&self) -> bool {
        self.important_differences > 0 || (!self.ignore_unimportant && self.unimportant_differences > 0)
    }
    pub fn summary(&self) -> &'static str {
        if self.important_differences > 0 { "important_differences" }
        else if self.unimportant_differences > 0 && !self.ignore_unimportant { "unimportant_differences" }
        else { "same" }
    }
    pub fn field_map(&self) -> BTreeMap<&str, &FieldComparison> {
        self.fields.iter().map(|f| (f.key.as_str(), f)).collect()
    }
}

pub fn compare_media(left: &MediaInfo, right: &MediaInfo, rules: &MediaRules) -> MediaComparison {
    let mut fields = Vec::new();
    let keys: BTreeSet<_> = left.fields.keys().chain(right.fields.keys()).cloned().collect();
    let (mut same, mut important_differences, mut unimportant_differences) = (0, 0, 0);
    for key in keys {
        if MediaRules::matches(&rules.excluded, &key) { continue; }
        let l = left.fields.get(&key).cloned().unwrap_or_default();
        let r = right.fields.get(&key).cloned().unwrap_or_default();
        let important = rules.is_important(&key);
        let status = if rules.normalized(&l) == rules.normalized(&r) {
            same += 1;
            FieldStatus::Same
        } else if important {
            important_differences += 1;
            FieldStatus::ImportantDifference
        } else {
            unimportant_differences += 1;
            FieldStatus::UnimportantDifference
        };
        fields.push(FieldComparison { key, left: l, right: r, important, status });
    }
    MediaComparison {
        left: left.clone(), right: right.clone(), fields,
        same, important_differences, unimportant_differences,
        ignore_unimportant: rules.ignore_unimportant,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn media() -> MediaInfo {
        MediaInfo { path:"left".into(),file_size:5,file_type:"Mpeg".into(),
            fields: BTreeMap::new(),raw_tags:vec![],tag_sources:vec![],pictures:vec![],warnings:vec![] }
    }
    #[test]
    fn difference_importance_and_recompare() {
        let mut l=media(); let mut r=media();
        l.add_field("tag.title","Love"); r.add_field("tag.title","Song");
        l.add_field("file.size_bytes","10");r.add_field("file.size_bytes","12");
        let mut rules=MediaRules::default();
        let c=compare_media(&l,&r,&rules);
        assert_eq!((c.important_differences,c.unimportant_differences),(1,1));
        assert!(c.effective_different());
        rules.set_importance("tag.title",false);rules.ignore_unimportant=true;
        let c=compare_media(&l,&r,&rules);
        assert_eq!(c.summary(),"same");
        assert!(!c.effective_different());
    }
    #[test]
    fn group_masks_and_precedence() {
        let mut l = media(); let mut r = media();
        l.add_field("tag.title","A");r.add_field("tag.title","B");
        l.add_field("technical.channels","1");r.add_field("technical.channels","2");
        l.add_field("technical.sample_rate_hz","44100");r.add_field("technical.sample_rate_hz","48000");
        let mut rules=MediaRules::default();
        rules.important.insert("technical.*".into());
        rules.unimportant.insert("technical.channels".into());
        rules.excluded.insert("tag.*".into());
        let c=compare_media(&l,&r,&rules);
        assert_eq!(c.important_differences,1);
        assert_eq!(c.unimportant_differences,1);
        assert!(!c.fields.iter().any(|f| f.key=="tag.title"));
    }
    #[test]
    fn normalization_is_configurable() {
        let mut l=media();let mut r=media();
        l.add_field("tag.title"," AbC ");r.add_field("tag.title","abc");
        let mut rules=MediaRules::default();
        assert!(compare_media(&l,&r,&rules).effective_different());
        rules.ignore_case=true;rules.trim_whitespace=true;
        assert!(!compare_media(&l,&r,&rules).effective_different());
    }
}
