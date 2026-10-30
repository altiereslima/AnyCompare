mod inline;
mod line_pairing;
mod reference_myers;
use similar::{DiffOp, DiffTag};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffStatus {
    Same,
    Different,
    Unimportant,
    LeftOrphan,
    RightOrphan,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Deserialize)]
pub enum InlineChunkKind {
    Same,
    Diff,
    Unimportant,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct InlineChunk {
    pub text: String,
    pub kind: InlineChunkKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlignedLine {
    pub left_line_no: Option<usize>,
    pub right_line_no: Option<usize>,
    pub left_text: String,
    pub right_text: String,
    pub status: DiffStatus,
    pub left_inline: std::sync::Arc<[InlineChunk]>,
    pub right_inline: std::sync::Arc<[InlineChunk]>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterMode {
    All,
    Differences,
    Same,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffResult {
    pub lines: Vec<AlignedLine>,
    pub sections: Vec<std::ops::Range<usize>>,
    pub diff_count: usize,
    pub same_count: usize,
    pub left_orphan_count: usize,
    pub right_orphan_count: usize,
}

pub struct DiffEngine;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub struct DiffRules {
    pub ignore_case: bool,
    pub ignore_whitespace: bool,
}

impl DiffEngine {
    pub fn compare_rules(left: &str, right: &str, rules: DiffRules) -> DiffResult {
        if rules == DiffRules::default() {
            return Self::compare(left, right);
        }
        let normalize = |text: &str| {
            let mut normalized = text
                .lines()
                .map(|line| {
                    let line = if rules.ignore_whitespace {
                        line.chars()
                            .filter(|c| !c.is_whitespace())
                            .collect::<String>()
                    } else {
                        line.to_owned()
                    };
                    if rules.ignore_case {
                        line.to_lowercase()
                    } else {
                        line
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");
            if text.ends_with('\n') {
                normalized.push('\n');
            }
            normalized
        };
        let mut result = Self::compare(&normalize(left), &normalize(right));
        let left: Vec<_> = left.lines().collect();
        let right: Vec<_> = right.lines().collect();
        for line in &mut result.lines {
            line.left_text = line.left_line_no.map(|n| left[n - 1]).unwrap_or("").into();
            line.right_text = line
                .right_line_no
                .map(|n| right[n - 1])
                .unwrap_or("")
                .into();
            let (a, b) = if line.status == DiffStatus::Same {
                (
                    vec![InlineChunk {
                        text: line.left_text.clone(),
                        kind: InlineChunkKind::Same,
                    }],
                    vec![InlineChunk {
                        text: line.right_text.clone(),
                        kind: InlineChunkKind::Same,
                    }],
                )
            } else {
                Self::compute_inline_diff(&line.left_text, &line.right_text)
            };
            line.left_inline = a.into();
            line.right_inline = b.into();
        }
        result
    }

    pub fn update_rules(
        left: &str,
        right: &str,
        previous: &DiffResult,
        rules: DiffRules,
    ) -> DiffResult {
        if rules == DiffRules::default() {
            Self::update(left, right, previous)
        } else {
            Self::compare_rules(left, right, rules)
        }
    }
    /// Recompare only the changed alignment section, bounded by unchanged anchors.
    /// Files that were replaced wholesale fall back to the complete comparison.
    pub fn update(left_text: &str, right_text: &str, previous: &DiffResult) -> DiffResult {
        let left: Vec<_> = left_text.lines().collect();
        let right: Vec<_> = right_text.lines().collect();
        let old_left: Vec<_> = previous
            .lines
            .iter()
            .filter(|line| line.left_line_no.is_some())
            .map(|line| line.left_text.as_str())
            .collect();
        let old_right: Vec<_> = previous
            .lines
            .iter()
            .filter(|line| line.right_line_no.is_some())
            .map(|line| line.right_text.as_str())
            .collect();
        let changed = |old: &[&str], new: &[&str]| {
            let prefix = old.iter().zip(new).take_while(|(a, b)| a == b).count();
            let suffix = old[prefix..]
                .iter()
                .rev()
                .zip(new[prefix..].iter().rev())
                .take_while(|(a, b)| a == b)
                .count();
            (prefix != old.len() || prefix != new.len()).then_some(prefix..old.len() - suffix)
        };
        let changes = [changed(&old_left, &left), changed(&old_right, &right)];
        if changes.iter().all(Option::is_none) {
            return previous.clone();
        }
        if previous.lines.is_empty() {
            return Self::compare(left_text, right_text);
        }
        let mut start = previous.lines.len();
        let mut end = 0;
        for (side, change) in changes.iter().enumerate() {
            if let Some(change) = change {
                let own = |line: &AlignedLine| {
                    if side == 0 {
                        line.left_line_no
                    } else {
                        line.right_line_no
                    }
                };
                let begin = previous
                    .lines
                    .iter()
                    .position(|line| own(line) == Some(change.start + 1))
                    .unwrap_or(previous.lines.len());
                let finish = if change.is_empty() {
                    begin
                } else {
                    previous
                        .lines
                        .iter()
                        .position(|line| own(line) == Some(change.end))
                        .map(|index| index + 1)
                        .unwrap_or(previous.lines.len())
                };
                start = start.min(begin);
                end = end.max(finish);
            }
        }
        start = start.saturating_sub(1);
        end = (end + 1).min(previous.lines.len());
        while start > 0 && previous.lines[start].status != DiffStatus::Same {
            start -= 1;
        }
        while end < previous.lines.len() && previous.lines[end - 1].status != DiffStatus::Same {
            end += 1;
        }
        if end - start > previous.lines.len().max(64) / 2 {
            return Self::compare(left_text, right_text);
        }
        let prefix_left = previous.lines[..start]
            .iter()
            .filter(|line| line.left_line_no.is_some())
            .count();
        let prefix_right = previous.lines[..start]
            .iter()
            .filter(|line| line.right_line_no.is_some())
            .count();
        let suffix_left = previous.lines[end..]
            .iter()
            .filter(|line| line.left_line_no.is_some())
            .count();
        let suffix_right = previous.lines[end..]
            .iter()
            .filter(|line| line.right_line_no.is_some())
            .count();
        let region_left = left[prefix_left..left.len() - suffix_left].join("\n");
        let region_right = right[prefix_right..right.len() - suffix_right].join("\n");
        // Keep a trailing empty logical line in the bounded region when joining.
        let preserve_empty = |mut text: String, lines: &[&str]| {
            if lines.last() == Some(&"") {
                text.push('\n');
            }
            text
        };
        let region = Self::compare(
            &preserve_empty(region_left, &left[prefix_left..left.len() - suffix_left]),
            &preserve_empty(
                region_right,
                &right[prefix_right..right.len() - suffix_right],
            ),
        );
        let mut lines = previous.lines[..start].to_vec();
        for mut line in region.lines {
            line.left_line_no = line.left_line_no.map(|n| n + prefix_left);
            line.right_line_no = line.right_line_no.map(|n| n + prefix_right);
            lines.push(line);
        }
        let left_shift = left.len() as isize - old_left.len() as isize;
        let right_shift = right.len() as isize - old_right.len() as isize;
        for mut line in previous.lines[end..].iter().cloned() {
            line.left_line_no = line
                .left_line_no
                .map(|n| n.checked_add_signed(left_shift).unwrap());
            line.right_line_no = line
                .right_line_no
                .map(|n| n.checked_add_signed(right_shift).unwrap());
            lines.push(line);
        }
        Self::summarize(lines)
    }

    fn summarize(lines: Vec<AlignedLine>) -> DiffResult {
        let mut result = DiffResult {
            lines,
            sections: Vec::new(),
            diff_count: 0,
            same_count: 0,
            left_orphan_count: 0,
            right_orphan_count: 0,
        };
        let mut start = None;
        for (index, line) in result.lines.iter().enumerate() {
            match line.status {
                DiffStatus::Same => result.same_count += 1,
                DiffStatus::Different | DiffStatus::Unimportant => result.diff_count += 1,
                DiffStatus::LeftOrphan => result.left_orphan_count += 1,
                DiffStatus::RightOrphan => result.right_orphan_count += 1,
            }
            if line.status != DiffStatus::Same {
                start.get_or_insert(index);
            } else if let Some(start) = start.take() {
                result.sections.push(start..index);
            }
        }
        if let Some(start) = start {
            result.sections.push(start..result.lines.len());
        }
        result
    }
    pub fn compute_character_diff(left: &str, right: &str) -> (Vec<InlineChunk>, Vec<InlineChunk>) {
        Self::compute_inline_diff(left, right)
    }
    /// Copy the entire selected difference block, including inserted/deleted lines.
    pub fn merge_block(
        left: &str,
        right: &str,
        diff: &DiffResult,
        selected: usize,
        to_right: bool,
    ) -> Option<String> {
        if diff.lines.get(selected)?.status == DiffStatus::Same {
            return None;
        }
        let mut start = selected;
        let mut end = selected + 1;
        while start > 0 && diff.lines[start - 1].status != DiffStatus::Same {
            start -= 1;
        }
        while end < diff.lines.len() && diff.lines[end].status != DiffStatus::Same {
            end += 1;
        }
        Self::merge_range(left, right, diff, start..end, to_right)
    }

    pub fn merge_range(
        left: &str,
        right: &str,
        diff: &DiffResult,
        range: std::ops::Range<usize>,
        to_right: bool,
    ) -> Option<String> {
        let start = range.start;
        let end = range.end;
        if start >= end || end > diff.lines.len() {
            return None;
        }
        let (source, target) = if to_right {
            (left, right)
        } else {
            (right, left)
        };
        let source_lines: Vec<&str> = source.lines().collect();
        let mut target_lines: Vec<&str> = target.lines().collect();
        let source_no = |line: &AlignedLine| {
            if to_right {
                line.left_line_no
            } else {
                line.right_line_no
            }
        };
        let target_no = |line: &AlignedLine| {
            if to_right {
                line.right_line_no
            } else {
                line.left_line_no
            }
        };
        let insertion = diff.lines[..start]
            .iter()
            .filter(|line| target_no(line).is_some())
            .count();
        let removed = diff.lines[start..end]
            .iter()
            .filter(|line| target_no(line).is_some())
            .count();
        let copied: Vec<_> = diff.lines[start..end]
            .iter()
            .filter_map(source_no)
            .map(|n| source_lines[n - 1])
            .collect();
        let at_end = insertion + removed == target_lines.len();
        target_lines.splice(insertion..insertion + removed, copied);
        let mut result = target_lines.join("\n");
        let newline = if at_end {
            source.ends_with('\n')
        } else {
            target.ends_with('\n')
        };
        if newline && !target_lines.is_empty() {
            result.push('\n');
        }
        Some(result)
    }
    /// Compute intra-line character/word level differences between two strings
    fn minor_key(text: &str) -> String {
        text.chars()
            .filter(|c| !c.is_whitespace())
            .flat_map(char::to_lowercase)
            .collect()
    }

    /// Compare case-folded character keys first: case-only edits stay aligned
    /// and receive blue only on the actual changed characters. Whitespace-only
    /// inserts/deletes are minor; original characters always remain in the buffer.
    pub fn compute_inline_diff(left: &str, right: &str) -> (Vec<InlineChunk>, Vec<InlineChunk>) {
        inline::compare(left, right)
    }

    /// Compute aligned lines between left text and right text using Myers diff
    pub fn compare(left_text: &str, right_text: &str) -> DiffResult {
        let left_lines: Vec<&str> = left_text.lines().collect();
        let right_lines: Vec<&str> = right_text.lines().collect();

        // Use the same logical-line representation for alignment and rendering.
        // In particular CRLF/LF must not produce a red line with identical visible text.
        let ops: Vec<_> = reference_myers::align(&left_lines, &right_lines)
            .into_iter()
            .map(|(tag, old, new)| match tag {
                DiffTag::Equal => DiffOp::Equal {
                    old_index: old.start,
                    new_index: new.start,
                    len: old.len(),
                },
                DiffTag::Delete => DiffOp::Delete {
                    old_index: old.start,
                    old_len: old.len(),
                    new_index: new.start,
                },
                DiffTag::Insert => DiffOp::Insert {
                    old_index: old.start,
                    new_index: new.start,
                    new_len: new.len(),
                },
                DiffTag::Replace => DiffOp::Replace {
                    old_index: old.start,
                    old_len: old.len(),
                    new_index: new.start,
                    new_len: new.len(),
                },
            })
            .collect();

        let mut lines = Vec::new();
        let mut diff_count = 0;
        let mut same_count = 0;
        let mut left_orphan_count = 0;
        let mut right_orphan_count = 0;

        for op in &ops {
            match *op {
                DiffOp::Equal {
                    old_index,
                    new_index,
                    len,
                } => {
                    for i in 0..len {
                        let l_text = left_lines
                            .get(old_index + i)
                            .copied()
                            .unwrap_or("")
                            .to_string();
                        let r_text = right_lines
                            .get(new_index + i)
                            .copied()
                            .unwrap_or("")
                            .to_string();
                        same_count += 1;
                        lines.push(AlignedLine {
                            left_line_no: Some(old_index + i + 1),
                            right_line_no: Some(new_index + i + 1),
                            left_text: l_text.clone(),
                            right_text: r_text.clone(),
                            status: DiffStatus::Same,
                            left_inline: vec![InlineChunk {
                                text: l_text,
                                kind: InlineChunkKind::Same,
                            }]
                            .into(),
                            right_inline: vec![InlineChunk {
                                text: r_text,
                                kind: InlineChunkKind::Same,
                            }]
                            .into(),
                        });
                    }
                }
                DiffOp::Delete {
                    old_index, old_len, ..
                } => {
                    for i in 0..old_len {
                        let l_text = left_lines
                            .get(old_index + i)
                            .copied()
                            .unwrap_or("")
                            .to_string();
                        left_orphan_count += 1;
                        lines.push(AlignedLine {
                            left_line_no: Some(old_index + i + 1),
                            right_line_no: None,
                            left_text: l_text.clone(),
                            right_text: String::new(),
                            status: DiffStatus::LeftOrphan,
                            left_inline: vec![InlineChunk {
                                text: l_text,
                                kind: InlineChunkKind::Diff,
                            }]
                            .into(),
                            right_inline: Vec::new().into(),
                        });
                    }
                }
                DiffOp::Insert {
                    new_index, new_len, ..
                } => {
                    for i in 0..new_len {
                        let r_text = right_lines
                            .get(new_index + i)
                            .copied()
                            .unwrap_or("")
                            .to_string();
                        right_orphan_count += 1;
                        lines.push(AlignedLine {
                            left_line_no: None,
                            right_line_no: Some(new_index + i + 1),
                            left_text: String::new(),
                            right_text: r_text.clone(),
                            status: DiffStatus::RightOrphan,
                            left_inline: Vec::new().into(),
                            right_inline: vec![InlineChunk {
                                text: r_text,
                                kind: InlineChunkKind::Diff,
                            }]
                            .into(),
                        });
                    }
                }
                DiffOp::Replace {
                    old_index,
                    old_len,
                    new_index,
                    new_len,
                } => {
                    let pairs = line_pairing::pair(
                        &left_lines[old_index..old_index + old_len],
                        &right_lines[new_index..new_index + new_len],
                    );
                    for (left_index, right_index) in pairs {
                        let has_left = left_index.is_some();
                        let has_right = right_index.is_some();

                        let l_text = if has_left {
                            left_lines
                                .get(old_index + left_index.unwrap())
                                .copied()
                                .unwrap_or("")
                                .to_string()
                        } else {
                            String::new()
                        };

                        let r_text = if has_right {
                            right_lines
                                .get(new_index + right_index.unwrap())
                                .copied()
                                .unwrap_or("")
                                .to_string()
                        } else {
                            String::new()
                        };

                        let l_num = if has_left {
                            Some(old_index + left_index.unwrap() + 1)
                        } else {
                            None
                        };
                        let r_num = if has_right {
                            Some(new_index + right_index.unwrap() + 1)
                        } else {
                            None
                        };

                        let (status, left_inline, right_inline) = if has_left && has_right {
                            diff_count += 1;
                            let (l_in, r_in) = Self::compute_inline_diff(&l_text, &r_text);
                            let status = if Self::minor_key(&l_text) == Self::minor_key(&r_text) {
                                DiffStatus::Unimportant
                            } else {
                                DiffStatus::Different
                            };
                            (status, l_in, r_in)
                        } else if has_left {
                            left_orphan_count += 1;
                            (
                                DiffStatus::LeftOrphan,
                                vec![InlineChunk {
                                    text: l_text.clone(),
                                    kind: InlineChunkKind::Diff,
                                }],
                                Vec::new(),
                            )
                        } else {
                            right_orphan_count += 1;
                            (
                                DiffStatus::RightOrphan,
                                Vec::new(),
                                vec![InlineChunk {
                                    text: r_text.clone(),
                                    kind: InlineChunkKind::Diff,
                                }],
                            )
                        };

                        lines.push(AlignedLine {
                            left_line_no: l_num,
                            right_line_no: r_num,
                            left_text: l_text,
                            right_text: r_text,
                            status,
                            left_inline: left_inline.into(),
                            right_inline: right_inline.into(),
                        });
                    }
                }
            }
        }

        let mut sections = Vec::new();
        let mut start = None;
        for (index, line) in lines.iter().enumerate() {
            if line.status != DiffStatus::Same {
                start.get_or_insert(index);
            } else if let Some(start) = start.take() {
                sections.push(start..index);
            }
        }
        if let Some(start) = start {
            sections.push(start..lines.len());
        }
        DiffResult {
            lines,
            sections,
            diff_count,
            same_count,
            left_orphan_count,
            right_orphan_count,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_user_samples_match_original_report_colors() {
        assert_native_full_replay("controlled-full-replay");
    }

    #[test]
    fn reversed_user_samples_match_original_report_colors_and_section_count() {
        assert_native_full_replay("controlled-reverse-replay");
    }

    fn assert_native_full_replay(name: &str) {
        let folder = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("docs/reference")
            .join(name);
        let observed: serde_json::Value =
            serde_json::from_slice(&std::fs::read(folder.join("expected.json")).unwrap()).unwrap();
        let left = crate::document::Document::load(&folder.join("left.txt")).unwrap();
        let right = crate::document::Document::load(&folder.join("right.txt")).unwrap();
        let actual = DiffEngine::compare(&left.text, &right.text);
        let rows = observed["rows"].as_array().unwrap();
        let mut mismatches = Vec::new();
        let by_number: std::collections::HashMap<_, _> = actual
            .lines
            .iter()
            .map(|line| ((line.left_line_no, line.right_line_no), line))
            .collect();
        let mut missing_pairs = Vec::new();
        for (index, row) in rows.iter().enumerate() {
            let number = |key: &str| row[key].as_u64().map(|v| v as usize);
            let Some(&line) = by_number.get(&(number("left_line_no"), number("right_line_no")))
            else {
                missing_pairs.push(format!(
                    "row {} ({} / {}): {} | {}",
                    index + 1,
                    row["left_line_no"],
                    row["right_line_no"],
                    row["left"],
                    row["right"]
                ));
                continue;
            };
            let a: Vec<InlineChunk> = serde_json::from_value(row["left_inline"].clone()).unwrap();
            let b: Vec<InlineChunk> = serde_json::from_value(row["right_inline"].clone()).unwrap();
            let normalize = |chunks: &[InlineChunk]| {
                chunks
                    .iter()
                    .filter(|c| !c.text.is_empty())
                    .cloned()
                    .collect::<Vec<_>>()
            };
            let different = normalize(&line.left_inline) != a || normalize(&line.right_inline) != b;
            if different {
                mismatches.push(format!("row {} ({} / {}):\n{}\n{}\nleft got {:?}\nleft expected {:?}\nright got {:?}\nright expected {:?}",index+1,row["left_line_no"],row["right_line_no"],line.left_text,line.right_text,line.left_inline,a,line.right_inline,b));
            }
        }
        std::fs::write(
            folder.join("comparison-mismatches.txt"),
            mismatches.join("\n\n"),
        )
        .unwrap();
        println!("native rows={} current rows={} mismatched colors={} missing alignment pairs={} sections={}",rows.len(),actual.lines.len(),mismatches.len(),missing_pairs.len(),actual.sections.len());
        for missing in &missing_pairs {
            println!("{missing}");
        }
        assert_eq!(actual.lines.len(), rows.len());
        assert!(
            missing_pairs.is_empty(),
            "native alignment pairs missing:\n{}",
            missing_pairs.join("\n")
        );
        for (line, row) in actual.lines.iter().zip(rows) {
            assert_eq!(
                line.left_line_no,
                row["left_line_no"].as_u64().map(|n| n as usize)
            );
            assert_eq!(
                line.right_line_no,
                row["right_line_no"].as_u64().map(|n| n as usize)
            );
            assert_eq!(line.left_text, row["left"].as_str().unwrap());
            assert_eq!(line.right_text, row["right"].as_str().unwrap());
            let status = match row["status"].as_str().unwrap() {
                "same" => DiffStatus::Same,
                "similar" => DiffStatus::Unimportant,
                "leftorphan" => DiffStatus::LeftOrphan,
                "rightorphan" => DiffStatus::RightOrphan,
                "different" => DiffStatus::Different,
                unknown => panic!("unknown native status {unknown}"),
            };
            assert_eq!(line.status, status);
        }
        assert!(
            mismatches.is_empty(),
            "{} rows differ; first:\n{}",
            mismatches.len(),
            mismatches.first().map(String::as_str).unwrap_or("")
        );
        assert_eq!(actual.sections.len(), 1263, "{name}: native count");
    }

    #[test]
    fn inline_colors_match_original_reports_observed_through_rea_frida() {
        let observed: serde_json::Value = serde_json::from_str(include_str!(
            "../../docs/reference/controlled-replay/expected.json"
        ))
        .unwrap();
        let rows = observed["rows"].as_array().unwrap();
        let text = |side: &str| {
            rows.iter()
                .map(|row| format!("{}\n", row[side].as_str().unwrap()))
                .collect::<String>()
        };
        let diff = DiffEngine::compare(&text("left"), &text("right"));
        assert_eq!(diff.lines.len(), rows.len());
        for (i, (line, row)) in diff.lines.iter().zip(rows).enumerate() {
            let status = match row["status"].as_str().unwrap() {
                "same" => DiffStatus::Same,
                "similar" => DiffStatus::Unimportant,
                "different" => DiffStatus::Different,
                value => panic!("Uncovered reference status: {value}"),
            };
            assert_eq!(line.status, status, "reference line {}", i + 1);
            let expected =
                |side: &str| serde_json::from_value::<Vec<InlineChunk>>(row[side].clone()).unwrap();
            assert_eq!(
                line.left_inline.as_ref(),
                expected("left_inline"),
                "left reference line {}",
                i + 1
            );
            assert_eq!(
                line.right_inline.as_ref(),
                expected("right_inline"),
                "right reference line {}",
                i + 1
            );
        }
    }

    #[test]
    fn case_whitespace_and_mixed_differences_preserve_text_and_importance() {
        let left = "A Pedra do Sigilo\nMaga azul  antiga\nigual\n";
        let right = "a pedra do sigilo\nmaga vermelha antiga\nigual\n";
        let diff = DiffEngine::compare(left, right);
        assert_eq!(diff.lines[0].status, DiffStatus::Unimportant);
        assert_eq!(diff.lines[1].status, DiffStatus::Different);
        for line in &diff.lines {
            assert_eq!(
                line.left_inline
                    .iter()
                    .map(|c| c.text.as_str())
                    .collect::<String>(),
                line.left_text
            );
            assert_eq!(
                line.right_inline
                    .iter()
                    .map(|c| c.text.as_str())
                    .collect::<String>(),
                line.right_text
            );
        }
        assert!(diff.lines[1]
            .left_inline
            .iter()
            .any(|c| c.kind == InlineChunkKind::Unimportant && c.text == "M"));
        assert!(diff.lines[1]
            .right_inline
            .iter()
            .any(|c| c.kind == InlineChunkKind::Diff && c.text.contains("verm")));
        let chars = DiffEngine::compute_character_diff("Pedra", "pedra");
        assert_eq!(chars.0[0].kind, InlineChunkKind::Unimportant);
        let ignored = DiffEngine::compare_rules(
            left,
            right,
            DiffRules {
                ignore_case: true,
                ignore_whitespace: true,
            },
        );
        assert_eq!(ignored.lines[0].status, DiffStatus::Same);
        assert_eq!(ignored.lines[1].status, DiffStatus::Different);
        assert_eq!(ignored.lines[0].left_text, "A Pedra do Sigilo");
        assert_eq!(diff.sections, vec![0..2]);
    }

    #[test]
    fn bounded_updates_match_complete_comparison_for_structural_and_unicode_edits() {
        let left: Vec<_> = (0..160).map(|i| format!("linha {i:03} ação")).collect();
        let mut right = left.clone();
        for i in [11, 57, 123] {
            right[i] = format!("linha {i:03} alterada");
        }
        let join = |lines: &[String]| format!("{}\n", lines.join("\n"));
        let before = DiffEngine::compare(&join(&left), &join(&right));
        for index in [0, 1, 10, 11, 56, 57, 122, 123, 159, 160] {
            for replacement in ["", "🙂 novo", "uma\n\nduas"] {
                let mut edited = left.clone();
                edited.insert(index, replacement.into());
                let updated = DiffEngine::update(&join(&edited), &join(&right), &before);
                assert_eq!(
                    updated,
                    DiffEngine::compare(&join(&edited), &join(&right)),
                    "insert {index} {replacement:?}"
                );
                if index < left.len() {
                    let mut edited = left.clone();
                    edited[index] = replacement.into();
                    let updated = DiffEngine::update(&join(&edited), &join(&right), &before);
                    assert_eq!(
                        updated,
                        DiffEngine::compare(&join(&edited), &join(&right)),
                        "replace {index} {replacement:?}"
                    );
                    edited.remove(index);
                    assert_eq!(
                        DiffEngine::update(&join(&edited), &join(&right), &before),
                        DiffEngine::compare(&join(&edited), &join(&right)),
                        "delete {index}"
                    );
                }
            }
        }
        for (a, b) in [
            ("\n", ""),
            ("a\n\n", "a"),
            ("", "a\n"),
            ("a", "\n"),
            ("\n\n", "a\n\n"),
        ] {
            let previous = DiffEngine::compare(a, b);
            assert_eq!(
                DiffEngine::update(b, a, &previous),
                DiffEngine::compare(b, a)
            );
        }
    }

    #[test]
    fn logical_line_endings_do_not_create_false_differences() {
        let result = DiffEngine::compare("olá\r\nmundo\r\n", "olá\nmundo\n");
        assert_eq!(result.same_count, 2);
        assert_eq!(
            result.diff_count + result.left_orphan_count + result.right_orphan_count,
            0
        );
    }

    #[test]
    fn merge_handles_replacements_insertions_deletions_and_empty_files() {
        for (left, right) in [
            ("a\nnovo\nz\n", "a\nantigo\nz\n"),
            ("a\ninserido\nz", "a\nz"),
            ("a\nz", "a\nremover\nz"),
            ("a\núltima", "a"),
            ("", "apagar\n"),
            ("novo\n", ""),
        ] {
            let result = DiffEngine::compare(left, right);
            let selected = result
                .lines
                .iter()
                .position(|line| line.status != DiffStatus::Same)
                .unwrap();
            assert_eq!(
                DiffEngine::merge_block(left, right, &result, selected, true).unwrap(),
                left
            );
            assert_eq!(
                DiffEngine::merge_block(left, right, &result, selected, false).unwrap(),
                right
            );
        }
    }
}
