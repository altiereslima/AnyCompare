//! UI-agnostic session state: load, swap, recompare, filter, navigate.
use crate::{compare_media, inspect_media, FieldComparison, FieldStatus, InspectOptions, MediaComparison, MediaError, MediaInfo, MediaRules};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaViewFilter { All, Differences, Same, None }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaSession {
    pub left: Option<MediaInfo>,
    pub right: Option<MediaInfo>,
    pub rules: MediaRules,
    pub filter: MediaViewFilter,
    pub cursor: Option<usize>,
    pub locked: bool,
}
impl Default for MediaSession {
    fn default() -> Self {
        Self { left: None, right: None, rules: MediaRules::default(),
               filter: MediaViewFilter::All, cursor: None, locked: false }
    }
}
impl MediaSession {
    pub fn new(left: MediaInfo, right: MediaInfo) -> Self {
        Self { left:Some(left), right:Some(right), ..Self::default() }
    }
    pub fn from_paths(left: impl AsRef<Path>, right: impl AsRef<Path>, options: InspectOptions) -> Result<Self, MediaError> {
        Ok(Self::new(inspect_media(left,options)?,inspect_media(right,options)?))
    }
    pub fn compare(&self) -> Option<MediaComparison> {
        Some(compare_media(self.left.as_ref()?,self.right.as_ref()?,&self.rules))
    }
    pub fn swap_sides(&mut self) {
        if !self.locked { std::mem::swap(&mut self.left,&mut self.right);self.cursor=None; }
    }
    pub fn clear(&mut self) {
        if !self.locked { self.left=None;self.right=None;self.cursor=None; }
    }
    pub fn reload(&mut self, options: InspectOptions) -> Result<(),MediaError> {
        if self.locked { return Ok(()); }
        // Transactional: keep old session intact if either load fails.
        let left = if let Some(l)=&self.left { Some(inspect_media(&l.path,options)?) } else { None };
        let right = if let Some(r)=&self.right { Some(inspect_media(&r.path,options)?) } else { None };
        self.left=left;self.right=right;self.cursor=None;
        Ok(())
    }
    pub fn set_importance(&mut self, key: &str, important: bool) {
        if !self.locked { self.rules.set_importance(key,important); self.cursor=None; }
    }
    pub fn is_visible(&self, f: &FieldComparison) -> bool {
        let same = f.status==FieldStatus::Same ||
            (self.rules.ignore_unimportant && f.status==FieldStatus::UnimportantDifference);
        match self.filter {
            MediaViewFilter::All=>true,
            MediaViewFilter::Differences=>!same,
            MediaViewFilter::Same=>same,
            MediaViewFilter::None=>false,
        }
    }
    pub fn visible<'a>(&self, comparison: &'a MediaComparison) -> Vec<&'a FieldComparison> {
        comparison.fields.iter().filter(|f|self.is_visible(f)).collect()
    }
    /// Returns an index into MediaComparison.fields for next/previous visible difference.
    /// Locate a visible row containing a case-insensitive substring in its key or value.
    /// Returns an index into `MediaComparison.fields`, wrapping at both ends.
    pub fn find_field(&mut self, query: &str, forward: bool) -> Option<usize> {
        let query = query.trim().to_lowercase();
        if query.is_empty() { return None; }
        let comparison = self.compare()?;
        let matches: Vec<usize> = comparison.fields.iter().enumerate().filter_map(|(idx, field)| {
            if !self.is_visible(field) { return None; }
            let found = field.key.to_lowercase().contains(&query)
                || field.left.iter().chain(field.right.iter()).any(|text| text.to_lowercase().contains(&query));
            found.then_some(idx)
        }).collect();
        if matches.is_empty() { return None; }
        let target = match (self.cursor, forward) {
            (None, true) => matches[0],
            (None, false) => *matches.last()?,
            (Some(cur), true) => *matches.iter().find(|&&x| x > cur).unwrap_or(&matches[0]),
            (Some(cur), false) => *matches.iter().rev().find(|&&x| x < cur).unwrap_or(matches.last()?),
        };
        self.cursor = Some(target);
        Some(target)
    }

    pub fn next_difference(&mut self, forward: bool) -> Option<usize> {
        let comparison=self.compare()?;
        let diff: Vec<usize>=comparison.fields.iter().enumerate().filter_map(|(idx,f)| {
            let visible=self.is_visible(f);
            let relevant=f.status==FieldStatus::ImportantDifference ||
                (f.status==FieldStatus::UnimportantDifference && !self.rules.ignore_unimportant);
            if visible && relevant { Some(idx) } else {None}
        }).collect();
        if diff.is_empty(){self.cursor=None;return None;}
        let target=match (self.cursor, forward) {
            (None,true)=>diff[0],(None,false)=>*diff.last()?,
            (Some(cur),true)=>*diff.iter().find(|&&x| x>cur).unwrap_or(&diff[0]),
            (Some(cur),false)=>*diff.iter().rev().find(|&&x| x<cur).unwrap_or(diff.last()?),
        };
        self.cursor=Some(target);Some(target)
    }
    pub fn to_json(&self)->Result<String,MediaError> { Ok(serde_json::to_string_pretty(self)?) }
    pub fn from_json(text:&str)->Result<Self,MediaError> { Ok(serde_json::from_str(text)?) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    fn base(s:&str)->MediaInfo {
        MediaInfo{path:s.into(),file_size:0,file_type:"Mpeg".into(),fields:BTreeMap::new(),raw_tags:vec![],tag_sources:vec![],pictures:vec![],warnings:vec![]}
    }
    #[test]
    fn navigation_and_swapping() {
        let mut l=base("left");let mut r=base("right");
        l.add_field("tag.title","A");r.add_field("tag.title","B");
        l.add_field("tag.artist","A");r.add_field("tag.artist","B");
        let mut s=MediaSession::new(l,r);s.filter=MediaViewFilter::Differences;
        assert_eq!(s.visible(&s.compare().unwrap()).len(),2);
        assert_eq!(s.next_difference(true),Some(0));
        assert_eq!(s.next_difference(true),Some(1));
        assert_eq!(s.next_difference(true),Some(0));
        s.swap_sides();assert_eq!(s.left.as_ref().unwrap().path,"right");
        assert_eq!(s.cursor,None);
    }
    #[test]
    fn case_insensitive_find_wraps_and_ignores_hidden_rows() {
        let mut left = base("a"); let mut right = base("b");
        left.add_field("tag.artist", "Alpha"); right.add_field("tag.artist", "Beta");
        left.add_field("tag.album", "ALPHA"); right.add_field("tag.album", "Gamma");
        let mut session = MediaSession::new(left, right);
        assert_eq!(session.find_field("alpha", true), Some(0));
        assert_eq!(session.find_field("alpha", true), Some(1));
        assert_eq!(session.find_field("alpha", true), Some(0));
        session.filter = MediaViewFilter::None;
        assert_eq!(session.find_field("alpha", true), None);
    }
    #[test]
    fn serialization_round_trip() {
        let s=MediaSession::new(base("a"),base("b"));
        let copy=MediaSession::from_json(&s.to_json().unwrap()).unwrap();
        assert_eq!(copy.left.unwrap().path,"a");
    }
}
