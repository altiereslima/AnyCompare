//! Byte alignment with explicit original-file offsets. Gaps are view data;
//! they are never inserted into a BinaryDocument. Native replay fixtures live
//! in docs/reference/controlled-hex-replay (five default BC4 comparisons).
use crate::diff::{common_prefix, common_suffix};
use similar::{capture_diff_slices_deadline, Algorithm, DiffTag};
use std::{
    collections::HashSet,
    ops::Range,
    time::{Duration, Instant},
};

const EXACT_MYERS_LIMIT: usize = 64 * 1024;
const BLOCK_SIZE: usize = 64;

#[derive(Clone, Default)]
struct Run {
    start: usize,
    len: usize,
    left: Range<usize>,
    right: Range<usize>,
    /// True only when `left.len() == right.len() == len` and every byte pair in
    /// this run is identical. Allows `next_diff` and filtered `row_map` to skip
    /// multi-megabyte identical spans in O(1).
    all_same: bool,
}
impl Run {
    fn side(&self, left: bool) -> &Range<usize> {
        if left {
            &self.left
        } else {
            &self.right
        }
    }
}

#[derive(Default)]
pub struct BinaryDiff {
    runs: Vec<Run>,
    pub len: usize,
    lengths: [usize; 2],
    pub same: usize,
    pub different: usize,
    pub left_orphans: usize,
    pub right_orphans: usize,
    pub last_difference: Option<usize>,
    pub first_difference: Option<usize>,
    /// A bounded search can return a valid, less refined alignment. Do not
    /// present that as an exhaustive native-equivalent comparison.
    pub limited: bool,
}

#[inline]
fn hash_block64(block: &[u8; BLOCK_SIZE]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for chunk in block.as_chunks::<8>().0 {
        let word = u64::from_le_bytes(*chunk);
        h ^= word;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

impl BinaryDiff {
    pub fn compare(left: &[u8], right: &[u8]) -> Self {
        // Keep exact boundary matches out of Compact's shifting pass. In the
        // native AB/BB replay the final B stays paired with the final B;
        // compacting the whole input instead created two false orphans.
        let prefix = common_prefix(left, right);
        let suffix = common_suffix(left, right, left.len().min(right.len()) - prefix);
        let end_left = left.len() - suffix;
        let end_right = right.len() - suffix;

        let mut diff = Self {
            lengths: [left.len(), right.len()],
            ..Self::default()
        };
        diff.push(0..prefix, 0..prefix, true);

        let mid_left = &left[prefix..end_left];
        let mid_right = &right[prefix..end_right];
        if !mid_left.is_empty() || !mid_right.is_empty() {
            if mid_left.len().saturating_add(mid_right.len()) <= EXACT_MYERS_LIMIT {
                let deadline = Instant::now() + Duration::from_millis(500);
                let ops = capture_diff_slices_deadline(
                    Algorithm::Myers,
                    mid_left,
                    mid_right,
                    Some(deadline),
                );
                if Instant::now() >= deadline {
                    diff.limited = true;
                }
                for op in ops {
                    let old = op.old_range();
                    let new = op.new_range();
                    let a = old.start + prefix..old.end + prefix;
                    let b = new.start + prefix..new.end + prefix;
                    let is_eq = op.tag() == DiffTag::Equal;
                    let paired = a.len().min(b.len());
                    diff.push(a.start..a.start + paired, b.start..b.start + paired, is_eq);
                    diff.push(a.start + paired..a.end, b.end..b.end, false);
                    diff.push(a.end..a.end, b.start + paired..b.end, false);
                }
            } else {
                diff.compare_large_middle(left, right, prefix..end_left, prefix..end_right);
            }
        }

        diff.push(end_left..left.len(), end_right..right.len(), true);
        diff.recompute_stats(left, right);
        diff
    }

    /// High-speed block-anchored alignment for multi-megabyte binary spans where
    /// byte-level O(ND) Myers would stall for seconds.
    fn compare_large_middle(
        &mut self,
        left: &[u8],
        right: &[u8],
        l_range: Range<usize>,
        r_range: Range<usize>,
    ) {
        let mid_l = &left[l_range.clone()];
        let mid_r = &right[r_range.clone()];
        let l_blocks = mid_l.as_chunks::<BLOCK_SIZE>().0;
        let r_blocks = mid_r.as_chunks::<BLOCK_SIZE>().0;

        // Fast O(N) anchor probe: if two multi-MB binaries share virtually no
        // 64-byte blocks (e.g. compressed/encrypted/unrelated files), emit direct
        // 1:1 alignment immediately in a few milliseconds instead of stalling.
        let mut l_hashes: HashSet<u64> = HashSet::with_capacity(l_blocks.len().min(131_072));
        let stride_l = (l_blocks.len() / 65_536).max(1);
        for blk in l_blocks.iter().step_by(stride_l) {
            l_hashes.insert(hash_block64(blk));
        }
        let stride_r = (r_blocks.len() / 16_384).max(1);
        let mut probed = 0usize;
        let mut hits = 0usize;
        for blk in r_blocks.iter().step_by(stride_r) {
            probed += 1;
            if l_hashes.contains(&hash_block64(blk)) {
                hits += 1;
            }
        }
        if probed == 0 || hits * 200 < probed {
            self.limited = true;
            let paired = l_range.len().min(r_range.len());
            self.push(
                l_range.start..l_range.start + paired,
                r_range.start..r_range.start + paired,
                false,
            );
            self.push(l_range.start + paired..l_range.end, r_range.end..r_range.end, false);
            self.push(l_range.end..l_range.end, r_range.start + paired..r_range.end, false);
            return;
        }

        // Run Myers over 64-byte chunks (64x smaller N) with a tight 150ms cap,
        // then refine small replace spans at byte granularity.
        let deadline = Instant::now() + Duration::from_millis(150);
        let ops = capture_diff_slices_deadline(Algorithm::Myers, l_blocks, r_blocks, Some(deadline));
        if Instant::now() >= deadline {
            self.limited = true;
        }
        let mut cur_l = l_range.start;
        let mut cur_r = r_range.start;
        for op in ops {
            let old = op.old_range();
            let new = op.new_range();
            let a_start = l_range.start + old.start * BLOCK_SIZE;
            let a_end = l_range.start + old.end * BLOCK_SIZE;
            let b_start = r_range.start + new.start * BLOCK_SIZE;
            let b_end = r_range.start + new.end * BLOCK_SIZE;
            if op.tag() == DiffTag::Equal {
                self.push_refined_gap(left, right, cur_l..a_start, cur_r..b_start);
                self.push(a_start..a_end, b_start..b_end, true);
                cur_l = a_end;
                cur_r = b_end;
            }
        }
        self.push_refined_gap(left, right, cur_l..l_range.end, cur_r..r_range.end);
    }

    fn push_refined_gap(
        &mut self,
        left: &[u8],
        right: &[u8],
        l_range: Range<usize>,
        r_range: Range<usize>,
    ) {
        if l_range.is_empty() && r_range.is_empty() {
            return;
        }
        let sl = &left[l_range.clone()];
        let sr = &right[r_range.clone()];
        let pre = common_prefix(sl, sr);
        let suf = common_suffix(sl, sr, sl.len().min(sr.len()) - pre);
        self.push(
            l_range.start..l_range.start + pre,
            r_range.start..r_range.start + pre,
            true,
        );
        let ml = l_range.start + pre..l_range.end - suf;
        let mr = r_range.start + pre..r_range.end - suf;
        if !ml.is_empty() || !mr.is_empty() {
            if ml.len().saturating_add(mr.len()) <= 4096 {
                let ops = capture_diff_slices_deadline(
                    Algorithm::Myers,
                    &left[ml.clone()],
                    &right[mr.clone()],
                    Some(Instant::now() + Duration::from_millis(25)),
                );
                for op in ops {
                    let old = op.old_range();
                    let new = op.new_range();
                    let a = old.start + ml.start..old.end + ml.start;
                    let b = new.start + mr.start..new.end + mr.start;
                    let is_eq = op.tag() == DiffTag::Equal;
                    let paired = a.len().min(b.len());
                    self.push(a.start..a.start + paired, b.start..b.start + paired, is_eq);
                    self.push(a.start + paired..a.end, b.end..b.end, false);
                    self.push(a.end..a.end, b.start + paired..b.end, false);
                }
            } else {
                let paired = ml.len().min(mr.len());
                self.push(ml.start..ml.start + paired, mr.start..mr.start + paired, false);
                self.push(ml.start + paired..ml.end, mr.end..mr.end, false);
                self.push(ml.end..ml.end, mr.start + paired..mr.end, false);
            }
        }
        self.push(
            l_range.end - suf..l_range.end,
            r_range.end - suf..r_range.end,
            true,
        );
    }

    fn recompute_stats(&mut self, left: &[u8], right: &[u8]) {
        self.same = 0;
        self.different = 0;
        self.left_orphans = 0;
        self.right_orphans = 0;
        self.first_difference = None;
        self.last_difference = None;
        for run in &mut self.runs {
            if run.left.is_empty() {
                run.all_same = false;
                self.right_orphans += run.len;
                self.first_difference.get_or_insert(run.start);
                self.last_difference = Some(run.start + run.len - 1);
            } else if run.right.is_empty() {
                run.all_same = false;
                self.left_orphans += run.len;
                self.first_difference.get_or_insert(run.start);
                self.last_difference = Some(run.start + run.len - 1);
            } else if run.all_same {
                self.same += run.len;
            } else {
                let mut any_diff = false;
                let a = &left[run.left.clone()];
                let b = &right[run.right.clone()];
                let mut i = 0usize;
                let n = a.len();
                // Fast scan over 64-byte equal chunks inside mixed runs
                while i + 64 <= n {
                    if a[i..i + 64] == b[i..i + 64] {
                        self.same += 64;
                        i += 64;
                    } else {
                        for k in 0..64 {
                            if a[i + k] == b[i + k] {
                                self.same += 1;
                            } else {
                                any_diff = true;
                                self.different += 1;
                                self.first_difference.get_or_insert(run.start + i + k);
                                self.last_difference = Some(run.start + i + k);
                            }
                        }
                        i += 64;
                    }
                }
                while i < n {
                    if a[i] == b[i] {
                        self.same += 1;
                    } else {
                        any_diff = true;
                        self.different += 1;
                        self.first_difference.get_or_insert(run.start + i);
                        self.last_difference = Some(run.start + i);
                    }
                    i += 1;
                }
                if !any_diff {
                    run.all_same = true;
                }
            }
        }
    }

    /// O(1) incremental alignment update for same-length in-place byte overwrites
    /// inside paired 1:1 runs (covers ~99% of interactive hex editing without
    /// re-running full diff over multi-MB buffers). Returns `false` if a full
    /// `recompare` is needed (e.g. range touches an orphan gap).
    pub fn update_overwrite_in_place(
        &mut self,
        left_side: bool,
        physical_start: usize,
        before: &[u8],
        left: &[u8],
        right: &[u8],
    ) -> bool {
        let len = before.len();
        if len == 0 {
            return true;
        }
        let physical_end = physical_start + len;
        let run_idx = self
            .runs
            .partition_point(|r| r.side(left_side).end <= physical_start);
        let Some(run) = self.runs.get(run_idx) else {
            return false;
        };
        let side = run.side(left_side);
        if run.left.is_empty()
            || run.right.is_empty()
            || physical_start < side.start
            || physical_end > side.end
        {
            return false;
        }

        let offset_in_run = physical_start - side.start;
        let aligned_start = run.start + offset_in_run;
        let other_start = run.side(!left_side).start + offset_in_run;
        let other_slice = if left_side {
            &right[other_start..other_start + len]
        } else {
            &left[other_start..other_start + len]
        };
        let new_slice = if left_side {
            &left[physical_start..physical_end]
        } else {
            &right[physical_start..physical_end]
        };

        let mut need_first_recalc = false;
        let mut need_last_recalc = false;
        let mut any_new_diff = false;
        for k in 0..len {
            let was_same = before[k] == other_slice[k];
            let now_same = new_slice[k] == other_slice[k];
            let aligned_pos = aligned_start + k;
            if !now_same {
                any_new_diff = true;
            }
            if was_same && !now_same {
                self.same = self.same.saturating_sub(1);
                self.different += 1;
                self.first_difference = Some(
                    self.first_difference
                        .map_or(aligned_pos, |cur| cur.min(aligned_pos)),
                );
                self.last_difference = Some(
                    self.last_difference
                        .map_or(aligned_pos, |cur| cur.max(aligned_pos)),
                );
            } else if !was_same && now_same {
                self.different = self.different.saturating_sub(1);
                self.same += 1;
                if self.first_difference == Some(aligned_pos) {
                    need_first_recalc = true;
                }
                if self.last_difference == Some(aligned_pos) {
                    need_last_recalc = true;
                }
            }
        }

        // If we modified a byte inside a large `all_same` run, split it around the
        // edited range so the untouched prefix/suffix stay `all_same = true` for
        // instant O(1) skipping during navigation & filtering.
        if self.runs[run_idx].all_same && any_new_diff {
            let orig = self.runs[run_idx].clone();
            let mut replacement = Vec::with_capacity(3);
            if offset_in_run > 0 {
                replacement.push(Run {
                    start: orig.start,
                    len: offset_in_run,
                    left: orig.left.start..orig.left.start + offset_in_run,
                    right: orig.right.start..orig.right.start + offset_in_run,
                    all_same: true,
                });
            }
            replacement.push(Run {
                start: aligned_start,
                len,
                left: orig.left.start + offset_in_run..orig.left.start + offset_in_run + len,
                right: orig.right.start + offset_in_run..orig.right.start + offset_in_run + len,
                all_same: false,
            });
            let tail_off = offset_in_run + len;
            if tail_off < orig.len {
                replacement.push(Run {
                    start: orig.start + tail_off,
                    len: orig.len - tail_off,
                    left: orig.left.start + tail_off..orig.left.end,
                    right: orig.right.start + tail_off..orig.right.end,
                    all_same: true,
                });
            }
            self.runs.splice(run_idx..=run_idx, replacement);
        }

        if need_first_recalc {
            self.first_difference = self.find_diff_from(0, true, left, right);
        }
        if need_last_recalc {
            self.last_difference = if self.len == 0 {
                None
            } else {
                self.find_diff_from(self.len - 1, false, left, right)
            };
        }
        true
    }

    /// Run-skipping difference search: skips entire `all_same` runs in O(1).
    pub fn find_diff_from(
        &self,
        start_pos: usize,
        forward: bool,
        left: &[u8],
        right: &[u8],
    ) -> Option<usize> {
        if self.runs.is_empty() || start_pos >= self.len {
            return None;
        }
        let mut r_idx = self
            .runs
            .partition_point(|run| run.start + run.len <= start_pos);
        if r_idx >= self.runs.len() {
            return None;
        }
        if forward {
            while r_idx < self.runs.len() {
                let run = &self.runs[r_idx];
                if run.all_same {
                    r_idx += 1;
                    continue;
                }
                let col_start = start_pos.saturating_sub(run.start);
                if run.left.is_empty() || run.right.is_empty() {
                    return Some(run.start + col_start);
                }
                for col in col_start..run.len {
                    if left[run.left.start + col] != right[run.right.start + col] {
                        return Some(run.start + col);
                    }
                }
                r_idx += 1;
            }
            None
        } else {
            loop {
                let run = &self.runs[r_idx];
                if !run.all_same {
                    let col_end = (start_pos - run.start).min(run.len - 1);
                    if run.left.is_empty() || run.right.is_empty() {
                        return Some(run.start + col_end);
                    }
                    for col in (0..=col_end).rev() {
                        if left[run.left.start + col] != right[run.right.start + col] {
                            return Some(run.start + col);
                        }
                    }
                }
                if r_idx == 0 {
                    break;
                }
                r_idx -= 1;
            }
            None
        }
    }

    /// Fast filtered row list walking `self.runs` directly (skips identical runs
    /// in O(1) when showing differences only, and skips orphan runs in O(1) when
    /// showing identical rows only).
    pub fn filtered_rows(
        &self,
        bytes_per_row: usize,
        want_same: bool,
        left: &[u8],
        right: &[u8],
    ) -> Vec<usize> {
        if bytes_per_row == 0 || self.len == 0 {
            return Vec::new();
        }
        let mut rows = Vec::new();
        let mut last_pushed: Option<usize> = None;
        let mut push_row = |row: usize| {
            if last_pushed != Some(row) {
                rows.push(row);
                last_pushed = Some(row);
            }
        };
        for run in &self.runs {
            if run.all_same {
                if want_same {
                    let first_row = run.start / bytes_per_row;
                    let last_row = (run.start + run.len - 1) / bytes_per_row;
                    for r in first_row..=last_row {
                        push_row(r);
                    }
                }
            } else if run.left.is_empty() || run.right.is_empty() {
                if !want_same {
                    let first_row = run.start / bytes_per_row;
                    let last_row = (run.start + run.len - 1) / bytes_per_row;
                    for r in first_row..=last_row {
                        push_row(r);
                    }
                }
            } else {
                let mut col = 0usize;
                while col < run.len {
                    let is_same = left[run.left.start + col] == right[run.right.start + col];
                    if is_same == want_same {
                        let row = (run.start + col) / bytes_per_row;
                        push_row(row);
                        // Jump to the start of the next row!
                        let next_aligned = (row + 1) * bytes_per_row;
                        col = next_aligned.saturating_sub(run.start);
                    } else {
                        col += 1;
                    }
                }
            }
        }
        rows
    }

    fn push(&mut self, left: Range<usize>, right: Range<usize>, all_same: bool) {
        let len = left.len().max(right.len());
        if len > 0 {
            if let Some(prev) = self.runs.last_mut() {
                if prev.all_same
                    && all_same
                    && prev.left.end == left.start
                    && prev.right.end == right.start
                {
                    prev.len += len;
                    prev.left.end = left.end;
                    prev.right.end = right.end;
                    self.len += len;
                    return;
                }
            }
            self.runs.push(Run {
                start: self.len,
                len,
                left,
                right,
                all_same,
            });
            self.len += len;
        }
    }
    fn run(&self, index: usize) -> Option<&Run> {
        self.runs.get(
            self.runs
                .partition_point(|run| run.start + run.len <= index),
        )
    }
    pub fn pair(&self, index: usize) -> (Option<usize>, Option<usize>) {
        let Some(run) = self.run(index) else {
            return (None, None);
        };
        let col = index - run.start;
        (
            (col < run.left.len()).then_some(run.left.start + col),
            (col < run.right.len()).then_some(run.right.start + col),
        )
    }
    pub fn offset(&self, left: bool, index: usize) -> Option<usize> {
        let pair = self.pair(index);
        if left {
            pair.0
        } else {
            pair.1
        }
    }
    fn boundary(&self, left: bool, index: usize) -> usize {
        self.run(index)
            .map_or(self.lengths[usize::from(!left)], |run| {
                let side = run.side(left);
                side.start + (index - run.start).min(side.len())
            })
    }
    pub fn range(&self, left: bool, aligned: Range<usize>) -> Range<usize> {
        self.boundary(left, aligned.start)..self.boundary(left, aligned.end)
    }
    pub fn index(&self, left: bool, physical: usize) -> usize {
        let index = self
            .runs
            .partition_point(|run| run.side(left).end <= physical);
        self.runs.get(index).map_or(self.len, |run| {
            run.start + physical.saturating_sub(run.side(left).start)
        })
    }
    pub fn changed(&self, index: usize, left: &[u8], right: &[u8]) -> bool {
        let (a, b) = self.pair(index);
        a.map(|i| left[i]) != b.map(|i| right[i])
    }
    pub fn difference_count(&self) -> usize {
        self.different + self.left_orphans + self.right_orphans
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn large_insertion_uses_compressed_runs_and_exact_original_offsets() {
        let left: Vec<_> = (0..1_000_000).map(|i| (i % 251) as u8).collect();
        let mut right = left.clone();
        right.insert(400_000, 255);
        let diff = BinaryDiff::compare(&left, &right);
        assert!(!diff.limited);
        assert_eq!(diff.same, left.len());
        assert_eq!(diff.right_orphans, 1);
        assert_eq!(diff.runs.len(), 3);
        assert_eq!(diff.pair(400_000), (None, Some(400_000)));
        assert_eq!(diff.pair(700_000), (Some(699_999), Some(700_000)));
        assert_eq!(diff.range(true, 399_999..400_002), 399_999..400_001);
    }
}
