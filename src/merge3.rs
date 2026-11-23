//! AnyCompare 3-Way Text Merge engine.
//!
//! Aligns `left` and `right` against a common ancestor (`center`/base) using the
//! existing BC5 diff engine, partitions the files into minimal synchronized merge
//! sections, auto-merges non-overlapping changes, flags true conflicts, and tracks
//! per-section resolutions plus manual output edits.

use crate::diff::{DiffEngine, DiffRules, DiffStatus};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MergeSectionKind {
    Unchanged,
    LeftOnly,
    RightOnly,
    BothSame,
    Conflict,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MergeChoice {
    TakeLeft,
    TakeCenter,
    TakeRight,
    TakeLeftThenRight,
    TakeRightThenLeft,
    ConflictUnresolved,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MergeSection {
    pub kind: MergeSectionKind,
    pub choice: MergeChoice,
    /// 1-based starting line number in each pane (None if zero lines in this section).
    pub left_start: usize,
    pub center_start: usize,
    pub right_start: usize,
    pub left_lines: Vec<String>,
    pub center_lines: Vec<String>,
    pub right_lines: Vec<String>,
}

impl MergeSection {
    pub fn is_changed(&self) -> bool {
        self.kind != MergeSectionKind::Unchanged
    }

    pub fn is_conflict(&self) -> bool {
        self.kind == MergeSectionKind::Conflict
    }

    pub fn is_unresolved_conflict(&self) -> bool {
        self.choice == MergeChoice::ConflictUnresolved
    }

    /// Number of synchronized visual rows occupied by this section in the top 3 panes.
    pub fn top_row_count(&self) -> usize {
        self.left_lines
            .len()
            .max(self.center_lines.len())
            .max(self.right_lines.len())
            .max(1)
    }

    /// Output lines produced by the active resolution choice.
    pub fn output_lines(&self) -> Vec<String> {
        match &self.choice {
            MergeChoice::TakeLeft => self.left_lines.clone(),
            MergeChoice::TakeCenter => self.center_lines.clone(),
            MergeChoice::TakeRight => self.right_lines.clone(),
            MergeChoice::TakeLeftThenRight => {
                let mut out = self.left_lines.clone();
                out.extend(self.right_lines.iter().cloned());
                out
            }
            MergeChoice::TakeRightThenLeft => {
                let mut out = self.right_lines.clone();
                out.extend(self.left_lines.iter().cloned());
                out
            }
            MergeChoice::ConflictUnresolved => {
                let mut out = Vec::new();
                out.push("<<<<<<< Left".into());
                out.extend(self.left_lines.iter().cloned());
                if !self.center_lines.is_empty() {
                    out.push("||||||| Center".into());
                    out.extend(self.center_lines.iter().cloned());
                }
                out.push("=======".into());
                out.extend(self.right_lines.iter().cloned());
                out.push(">>>>>>> Right".into());
                out
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct MergeRow {
    pub section_idx: usize,
    pub kind: MergeSectionKind,
    pub unresolved: bool,
    pub left_line_no: Option<usize>,
    pub left_text: String,
    pub center_line_no: Option<usize>,
    pub center_text: String,
    pub right_line_no: Option<usize>,
    pub right_text: String,
}

#[derive(Clone, Debug)]
pub struct OutputRow {
    pub section_idx: usize,
    pub output_line_no: usize,
    pub text: String,
    pub choice_tag: OutputSourceTag,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputSourceTag {
    Same,
    Left,
    Center,
    Right,
    Both,
    Conflict,
}

#[derive(Clone, Debug, Default)]
pub struct MergeResult {
    pub sections: Vec<MergeSection>,
    pub rows: Vec<MergeRow>,
    pub output_rows: Vec<OutputRow>,
    pub changed_sections: usize,
    pub conflict_sections: usize,
    pub unresolved_conflicts: usize,
    pub left_changes: usize,
    pub right_changes: usize,
    pub both_same: usize,
}

impl MergeResult {
    pub fn rebuild_views(&mut self) {
        self.rows.clear();
        self.output_rows.clear();
        self.changed_sections = 0;
        self.conflict_sections = 0;
        self.unresolved_conflicts = 0;
        self.left_changes = 0;
        self.right_changes = 0;
        self.both_same = 0;

        let mut out_line_no = 1usize;
        for (sec_idx, sec) in self.sections.iter().enumerate() {
            match sec.kind {
                MergeSectionKind::Unchanged => {}
                MergeSectionKind::LeftOnly => {
                    self.changed_sections += 1;
                    self.left_changes += 1;
                }
                MergeSectionKind::RightOnly => {
                    self.changed_sections += 1;
                    self.right_changes += 1;
                }
                MergeSectionKind::BothSame => {
                    self.changed_sections += 1;
                    self.both_same += 1;
                }
                MergeSectionKind::Conflict => {
                    self.changed_sections += 1;
                    self.conflict_sections += 1;
                }
            }
            if sec.is_unresolved_conflict() {
                self.unresolved_conflicts += 1;
            }

            let count = sec.top_row_count();
            for i in 0..count {
                let (lno, ltxt) = match sec.left_lines.get(i) {
                    Some(s) => (Some(sec.left_start + i), s.clone()),
                    None => (None, String::new()),
                };
                let (cno, ctxt) = match sec.center_lines.get(i) {
                    Some(s) => (Some(sec.center_start + i), s.clone()),
                    None => (None, String::new()),
                };
                let (rno, rtxt) = match sec.right_lines.get(i) {
                    Some(s) => (Some(sec.right_start + i), s.clone()),
                    None => (None, String::new()),
                };
                self.rows.push(MergeRow {
                    section_idx: sec_idx,
                    kind: sec.kind,
                    unresolved: sec.is_unresolved_conflict(),
                    left_line_no: lno,
                    left_text: ltxt,
                    center_line_no: cno,
                    center_text: ctxt,
                    right_line_no: rno,
                    right_text: rtxt,
                });
            }

            let tag = match &sec.choice {
                MergeChoice::TakeLeft => {
                    if sec.kind == MergeSectionKind::Unchanged {
                        OutputSourceTag::Same
                    } else if sec.kind == MergeSectionKind::BothSame {
                        OutputSourceTag::Both
                    } else {
                        OutputSourceTag::Left
                    }
                }
                MergeChoice::TakeCenter => {
                    if sec.kind == MergeSectionKind::Unchanged {
                        OutputSourceTag::Same
                    } else {
                        OutputSourceTag::Center
                    }
                }
                MergeChoice::TakeRight => {
                    if sec.kind == MergeSectionKind::Unchanged {
                        OutputSourceTag::Same
                    } else {
                        OutputSourceTag::Right
                    }
                }
                MergeChoice::TakeLeftThenRight | MergeChoice::TakeRightThenLeft => {
                    OutputSourceTag::Both
                }
                MergeChoice::ConflictUnresolved => OutputSourceTag::Conflict,
            };

            for line in sec.output_lines() {
                self.output_rows.push(OutputRow {
                    section_idx: sec_idx,
                    output_line_no: out_line_no,
                    text: line,
                    choice_tag: tag,
                });
                out_line_no += 1;
            }
        }
    }

    pub fn assemble_output_text(&self, line_ending: &str) -> String {
        let lines: Vec<String> = self
            .sections
            .iter()
            .flat_map(MergeSection::output_lines)
            .collect();
        lines.join(line_ending)
    }
}

/// Split raw text into logical lines matching `DiffEngine::compare_rules` (`str::lines`).
fn split_lines(text: &str) -> Vec<String> {
    text.lines().map(str::to_owned).collect()
}

/// Extract (base_index -> side_index) for lines strictly unchanged between `base` and `side`.
fn match_anchors(
    base: &str,
    side: &str,
    rules: DiffRules,
    ignore_unimportant: bool,
) -> Vec<(usize, usize)> {
    let diff = DiffEngine::compare_rules(base, side, rules);
    let mut anchors = Vec::new();
    for line in &diff.lines {
        let same = match line.status {
            DiffStatus::Same => true,
            DiffStatus::Unimportant => ignore_unimportant,
            _ => false,
        };
        if same {
            if let (Some(b), Some(s)) = (line.left_line_no, line.right_line_no) {
                anchors.push((b - 1, s - 1));
            }
        }
    }
    anchors
}

/// Compute a classic 3-way merge of `left` and `right` relative to common ancestor `center`.
///
/// When `center` is `None` (2-way merge without base), falls back to aligning `left` vs `right`:
/// matching spans are `Unchanged`, differing spans are `Conflict` (so the user picks Left/Right).
pub fn compute_three_way(
    left: &str,
    center: Option<&str>,
    right: &str,
    rules: DiffRules,
    ignore_unimportant: bool,
) -> MergeResult {
    let left_lines = split_lines(left);
    let right_lines = split_lines(right);

    let Some(center_str) = center else {
        return compute_two_way_merge(
            left,
            right,
            &left_lines,
            &right_lines,
            rules,
            ignore_unimportant,
        );
    };
    let center_lines = split_lines(center_str);

    let anchors_l = match_anchors(center_str, left, rules, ignore_unimportant);
    let anchors_r = match_anchors(center_str, right, rules, ignore_unimportant);

    let mut l_map = vec![None; center_lines.len()];
    for (b, s) in anchors_l {
        if b < l_map.len() {
            l_map[b] = Some(s);
        }
    }
    let mut r_map = vec![None; center_lines.len()];
    for (b, s) in anchors_r {
        if b < r_map.len() {
            r_map[b] = Some(s);
        }
    }

    // Find synchronized base anchors where BOTH left and right match center cleanly
    // in strictly increasing order.
    let mut sync_anchors: Vec<(usize, usize, usize)> = Vec::new();
    let (mut last_l, mut last_r) = (0usize, 0usize);
    for b in 0..center_lines.len() {
        if let (Some(li), Some(ri)) = (l_map[b], r_map[b]) {
            if (sync_anchors.is_empty() || (li >= last_l && ri >= last_r))
                && li < left_lines.len()
                && ri < right_lines.len()
            {
                sync_anchors.push((b, li, ri));
                last_l = li + 1;
                last_r = ri + 1;
            }
        }
    }
    // Sentinel end anchor.
    sync_anchors.push((center_lines.len(), left_lines.len(), right_lines.len()));

    let mut sections: Vec<MergeSection> = Vec::new();
    let (mut cb, mut cl, mut cr) = (0usize, 0usize, 0usize);

    let push_section = |sections: &mut Vec<MergeSection>,
                        kind: MergeSectionKind,
                        l_slice: &[String],
                        c_slice: &[String],
                        r_slice: &[String],
                        l_start: usize,
                        c_start: usize,
                        r_start: usize| {
        if l_slice.is_empty() && c_slice.is_empty() && r_slice.is_empty() {
            return;
        }
        // Coalesce adjacent Unchanged lines into a single clean Unchanged section.
        if kind == MergeSectionKind::Unchanged {
            if let Some(prev) = sections.last_mut() {
                if prev.kind == MergeSectionKind::Unchanged {
                    prev.left_lines.extend(l_slice.iter().cloned());
                    prev.center_lines.extend(c_slice.iter().cloned());
                    prev.right_lines.extend(r_slice.iter().cloned());
                    return;
                }
            }
        }
        let choice = match kind {
            MergeSectionKind::Unchanged => MergeChoice::TakeCenter,
            MergeSectionKind::LeftOnly => MergeChoice::TakeLeft,
            MergeSectionKind::RightOnly => MergeChoice::TakeRight,
            MergeSectionKind::BothSame => MergeChoice::TakeLeft,
            MergeSectionKind::Conflict => MergeChoice::ConflictUnresolved,
        };
        sections.push(MergeSection {
            kind,
            choice,
            left_start: l_start + 1,
            center_start: c_start + 1,
            right_start: r_start + 1,
            left_lines: l_slice.to_vec(),
            center_lines: c_slice.to_vec(),
            right_lines: r_slice.to_vec(),
        });
    };

    for (ab, al, ar) in sync_anchors {
        if cb < ab || cl < al || cr < ar {
            let l_slice = &left_lines[cl..al];
            let c_slice = &center_lines[cb..ab];
            let r_slice = &right_lines[cr..ar];

            let left_eq_center = lines_equal(l_slice, c_slice, rules);
            let right_eq_center = lines_equal(r_slice, c_slice, rules);
            let left_eq_right = lines_equal(l_slice, r_slice, rules);

            if left_eq_center && right_eq_center {
                push_section(
                    &mut sections,
                    MergeSectionKind::Unchanged,
                    l_slice,
                    c_slice,
                    r_slice,
                    cl,
                    cb,
                    cr,
                );
            } else if !left_eq_center && right_eq_center {
                push_section(
                    &mut sections,
                    MergeSectionKind::LeftOnly,
                    l_slice,
                    c_slice,
                    r_slice,
                    cl,
                    cb,
                    cr,
                );
            } else if left_eq_center && !right_eq_center {
                push_section(
                    &mut sections,
                    MergeSectionKind::RightOnly,
                    l_slice,
                    c_slice,
                    r_slice,
                    cl,
                    cb,
                    cr,
                );
            } else if left_eq_right {
                push_section(
                    &mut sections,
                    MergeSectionKind::BothSame,
                    l_slice,
                    c_slice,
                    r_slice,
                    cl,
                    cb,
                    cr,
                );
            } else {
                // Refine mixed hunk by splitting out any sub-runs where Left == Right
                // (`BothSame`) and checking one-sided edits around them.
                refine_mixed_hunk(
                    &mut sections,
                    l_slice,
                    c_slice,
                    r_slice,
                    cl,
                    cb,
                    cr,
                    &l_map[cb..ab],
                    &r_map[cb..ab],
                    rules,
                    ignore_unimportant,
                );
            }
        }

        // Emit the synchronized anchor line itself (except sentinel).
        if ab < center_lines.len() {
            push_section(
                &mut sections,
                MergeSectionKind::Unchanged,
                &left_lines[al..al + 1],
                &center_lines[ab..ab + 1],
                &right_lines[ar..ar + 1],
                al,
                ab,
                ar,
            );
            cb = ab + 1;
            cl = al + 1;
            cr = ar + 1;
        }
    }

    let mut result = MergeResult {
        sections,
        ..Default::default()
    };
    result.rebuild_views();
    result
}

#[allow(clippy::too_many_arguments)]
fn refine_mixed_hunk(
    sections: &mut Vec<MergeSection>,
    l_slice: &[String],
    c_slice: &[String],
    r_slice: &[String],
    l_base: usize,
    c_base: usize,
    r_base: usize,
    l_map: &[Option<usize>],
    r_map: &[Option<usize>],
    rules: DiffRules,
    ignore_unimportant: bool,
) {
    let emit = |sections: &mut Vec<MergeSection>,
                ls: &[String],
                cs: &[String],
                rs: &[String],
                l0: usize,
                c0: usize,
                r0: usize| {
        if ls.is_empty() && cs.is_empty() && rs.is_empty() {
            return;
        }
        let lc = lines_equal(ls, cs, rules);
        let rc = lines_equal(rs, cs, rules);
        let lr = lines_equal(ls, rs, rules);
        let (kind, choice) = if lc && rc {
            (MergeSectionKind::Unchanged, MergeChoice::TakeCenter)
        } else if !lc && rc {
            (MergeSectionKind::LeftOnly, MergeChoice::TakeLeft)
        } else if lc && !rc {
            (MergeSectionKind::RightOnly, MergeChoice::TakeRight)
        } else if lr {
            (MergeSectionKind::BothSame, MergeChoice::TakeLeft)
        } else {
            (MergeSectionKind::Conflict, MergeChoice::ConflictUnresolved)
        };
        if let Some(prev) = sections.last_mut() {
            if prev.kind == kind
                && matches!(
                    kind,
                    MergeSectionKind::Unchanged
                        | MergeSectionKind::LeftOnly
                        | MergeSectionKind::RightOnly
                        | MergeSectionKind::BothSame
                )
            {
                prev.left_lines.extend(ls.iter().cloned());
                prev.center_lines.extend(cs.iter().cloned());
                prev.right_lines.extend(rs.iter().cloned());
                return;
            }
        }
        sections.push(MergeSection {
            kind,
            choice,
            left_start: l_base + l0 + 1,
            center_start: c_base + c0 + 1,
            right_start: r_base + r0 + 1,
            left_lines: ls.to_vec(),
            center_lines: cs.to_vec(),
            right_lines: rs.to_vec(),
        });
    };

    // Fast exact 1-to-1 line-by-line refinement when Left, Center, and Right
    // have identical line counts within the hunk (very common for adjacent edits
    // to neighboring lines without insertions/deletions).
    if l_slice.len() == c_slice.len() && r_slice.len() == c_slice.len() {
        for i in 0..c_slice.len() {
            emit(
                sections,
                &l_slice[i..i + 1],
                &c_slice[i..i + 1],
                &r_slice[i..i + 1],
                i,
                i,
                i,
            );
        }
        return;
    }

    // General variable-length hunk: split on Left == Right sub-anchors and
    // propagate center indices via l_map / r_map.
    let l_text = l_slice.join("\n");
    let r_text = r_slice.join("\n");
    let lr_anchors = match_anchors(&l_text, &r_text, rules, ignore_unimportant);
    if lr_anchors.is_empty() {
        emit(sections, l_slice, c_slice, r_slice, 0, 0, 0);
        return;
    }

    let mut cur_l = 0usize;
    let mut cur_c = 0usize;
    let mut cur_r = 0usize;

    for (li, ri) in lr_anchors {
        if li < cur_l || ri < cur_r {
            continue;
        }
        let mut next_c = cur_c;
        while next_c < c_slice.len() {
            let lm = l_map[next_c].and_then(|abs| abs.checked_sub(l_base));
            let rm = r_map[next_c].and_then(|abs| abs.checked_sub(r_base));
            if lm.is_some_and(|x| x < li) || rm.is_some_and(|x| x < ri) {
                next_c += 1;
            } else {
                break;
            }
        }
        if cur_l < li || cur_c < next_c || cur_r < ri {
            emit(
                sections,
                &l_slice[cur_l..li],
                &c_slice[cur_c..next_c],
                &r_slice[cur_r..ri],
                cur_l,
                cur_c,
                cur_r,
            );
            cur_l = li;
            cur_c = next_c;
            cur_r = ri;
        }
        let both_c_end = if cur_c < c_slice.len()
            && l_map[cur_c].is_none()
            && r_map[cur_c].is_none()
        {
            cur_c + 1
        } else {
            cur_c
        };
        emit(
            sections,
            &l_slice[cur_l..cur_l + 1],
            &c_slice[cur_c..both_c_end],
            &r_slice[cur_r..cur_r + 1],
            cur_l,
            cur_c,
            cur_r,
        );
        cur_l += 1;
        cur_c = both_c_end;
        cur_r += 1;
    }

    if cur_l < l_slice.len() || cur_c < c_slice.len() || cur_r < r_slice.len() {
        emit(
            sections,
            &l_slice[cur_l..],
            &c_slice[cur_c..],
            &r_slice[cur_r..],
            cur_l,
            cur_c,
            cur_r,
        );
    }
}

fn compute_two_way_merge(
    left: &str,
    right: &str,
    left_lines: &[String],
    right_lines: &[String],
    rules: DiffRules,
    ignore_unimportant: bool,
) -> MergeResult {
    let diff = DiffEngine::compare_rules(left, right, rules);
    let mut sections: Vec<MergeSection> = Vec::new();
    let mut idx = 0;
    let mut next_l = 1usize;
    let mut next_r = 1usize;

    while idx < diff.lines.len() {
        let is_same = matches!(diff.lines[idx].status, DiffStatus::Same)
            || (ignore_unimportant && diff.lines[idx].status == DiffStatus::Unimportant);
        let start = idx;
        while idx < diff.lines.len() {
            let cur_same = matches!(diff.lines[idx].status, DiffStatus::Same)
                || (ignore_unimportant && diff.lines[idx].status == DiffStatus::Unimportant);
            if cur_same != is_same {
                break;
            }
            idx += 1;
        }

        let mut l_vec = Vec::new();
        let mut r_vec = Vec::new();
        let l_start = diff.lines[start..idx]
            .iter()
            .find_map(|l| l.left_line_no)
            .unwrap_or(next_l);
        let r_start = diff.lines[start..idx]
            .iter()
            .find_map(|l| l.right_line_no)
            .unwrap_or(next_r);

        for row in &diff.lines[start..idx] {
            if let Some(ln) = row.left_line_no {
                if let Some(s) = left_lines.get(ln - 1) {
                    l_vec.push(s.clone());
                }
                next_l = ln + 1;
            }
            if let Some(rn) = row.right_line_no {
                if let Some(s) = right_lines.get(rn - 1) {
                    r_vec.push(s.clone());
                }
                next_r = rn + 1;
            }
        }

        let (kind, choice) = if is_same {
            (MergeSectionKind::Unchanged, MergeChoice::TakeLeft)
        } else {
            (MergeSectionKind::Conflict, MergeChoice::ConflictUnresolved)
        };
        sections.push(MergeSection {
            kind,
            choice,
            left_start: l_start,
            center_start: 1,
            right_start: r_start,
            left_lines: l_vec,
            center_lines: Vec::new(),
            right_lines: r_vec,
        });
    }

    let mut result = MergeResult {
        sections,
        ..Default::default()
    };
    result.rebuild_views();
    result
}

fn lines_equal(a: &[String], b: &[String], rules: DiffRules) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b.iter()).all(|(x, y)| {
        if !rules.ignore_case && !rules.ignore_whitespace {
            x == y
        } else {
            normalize(x, rules) == normalize(y, rules)
        }
    })
}

fn normalize(s: &str, rules: DiffRules) -> String {
    let mut out = if rules.ignore_whitespace {
        s.split_whitespace().collect::<Vec<_>>().join(" ")
    } else {
        s.to_owned()
    };
    if rules.ignore_case {
        out = out.to_lowercase();
    }
    out
}

/// Compute indices of visible top rows given a filter mode and optional context lines.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MergeFilter {
    All,
    Changed,
    Conflicts,
    LeftChanges,
    RightChanges,
    Mergeable,
    Unchanged,
}

pub fn visible_row_indices(
    result: &MergeResult,
    filter: MergeFilter,
    show_context: bool,
    context_lines: usize,
) -> Vec<usize> {
    if filter == MergeFilter::All {
        return (0..result.rows.len()).collect();
    }
    let matches_row = |row: &MergeRow| match filter {
        MergeFilter::All => true,
        MergeFilter::Changed => row.kind != MergeSectionKind::Unchanged,
        MergeFilter::Conflicts => row.kind == MergeSectionKind::Conflict,
        MergeFilter::LeftChanges => matches!(
            row.kind,
            MergeSectionKind::LeftOnly | MergeSectionKind::BothSame | MergeSectionKind::Conflict
        ),
        MergeFilter::RightChanges => matches!(
            row.kind,
            MergeSectionKind::RightOnly | MergeSectionKind::BothSame | MergeSectionKind::Conflict
        ),
        MergeFilter::Mergeable => matches!(
            row.kind,
            MergeSectionKind::LeftOnly | MergeSectionKind::RightOnly | MergeSectionKind::BothSame
        ),
        MergeFilter::Unchanged => row.kind == MergeSectionKind::Unchanged,
    };

    let mut keep = BTreeSet::new();
    for (idx, row) in result.rows.iter().enumerate() {
        if matches_row(row) {
            keep.insert(idx);
            if show_context {
                let lo = idx.saturating_sub(context_lines);
                let hi = (idx + context_lines).min(result.rows.len().saturating_sub(1));
                for j in lo..=hi {
                    keep.insert(j);
                }
            }
        }
    }
    keep.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn three_way_auto_merges_non_overlapping_and_flags_true_conflicts() {
        let base = "line 1\nline 2\nline 3\nline 4\nline 5\nline 6";
        let left = "line 1 LEFT\nline 2\nline 3\nline 4 BOTH\nline 5 LEFT_CONFLICT\nline 6";
        let right = "line 1\nline 2\nline 3 RIGHT\nline 4 BOTH\nline 5 RIGHT_CONFLICT\nline 6";

        let mut merge = compute_three_way(left, Some(base), right, DiffRules::default(), false);
        assert_eq!(merge.left_changes, 1);
        assert_eq!(merge.right_changes, 1);
        assert_eq!(merge.both_same, 1);
        assert_eq!(merge.conflict_sections, 1);
        assert_eq!(merge.unresolved_conflicts, 1);

        // Resolve the single conflict by taking Left then Right.
        let conflict_idx = merge
            .sections
            .iter()
            .position(|s| s.is_conflict())
            .unwrap();
        merge.sections[conflict_idx].choice = MergeChoice::TakeLeftThenRight;
        merge.rebuild_views();
        assert_eq!(merge.unresolved_conflicts, 0);

        let output = merge.assemble_output_text("\n");
        assert_eq!(
            output,
            "line 1 LEFT\nline 2\nline 3 RIGHT\nline 4 BOTH\nline 5 LEFT_CONFLICT\nline 5 RIGHT_CONFLICT\nline 6"
        );
    }

    #[test]
    fn three_way_handles_insertions_deletions_and_two_way_fallback() {
        let base = "a\nb\nc";
        let left = "a\ninserted_left\nb\nc";
        let right = "a\nb"; // deleted 'c'

        let merge = compute_three_way(left, Some(base), right, DiffRules::default(), false);
        assert_eq!(merge.conflict_sections, 0);
        assert_eq!(merge.assemble_output_text("\n"), "a\ninserted_left\nb");

        // Without center (2-way merge): differing lines are conflicts.
        let two = compute_three_way(
            "hello\nworld",
            None,
            "hello\nrust",
            DiffRules::default(),
            false,
        );
        assert_eq!(two.conflict_sections, 1);
        assert_eq!(two.unresolved_conflicts, 1);
    }

    #[test]
    fn three_way_splits_adjacent_left_and_right_edits_without_false_conflict() {
        let base = "header\nhost = 0.0.0.0\nport = 8080\nworkers = 4\ntimeout = 30\ncache = false\nlog = info\nregion = sa\nretries = 3";
        let left = "header\nhost = 127.0.0.1\nport = 8080\nworkers = 8\ntimeout = 30\ncache = true\nlog = debug\nregion = sa\nretries = 5";
        let right = "header\nhost = 0.0.0.0\nport = 9090\nworkers = 8\ntimeout = 30\ncache = false\nlog = warn\nregion = us\nretries = 3";

        let merge = compute_three_way(left, Some(base), right, DiffRules::default(), false);
        assert_eq!(merge.conflict_sections, 1);
        assert_eq!(merge.left_changes, 3);
        assert_eq!(merge.right_changes, 2);
        assert_eq!(merge.both_same, 1);
    }
}
