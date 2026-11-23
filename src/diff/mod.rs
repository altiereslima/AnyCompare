pub(crate) mod align;
mod inline;
mod line_pairing;
mod reference_myers;

pub use align::AlignMethod;
use std::sync::{Arc, OnceLock};

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

type InlinePair = (Arc<[InlineChunk]>, Arc<[InlineChunk]>);

#[derive(Debug)]
pub struct AlignedLine {
    pub left_line_no: Option<usize>,
    pub right_line_no: Option<usize>,
    /// Shared: the incremental update clones the whole aligned list per edit.
    pub left_text: Arc<str>,
    pub right_text: Arc<str>,
    pub status: DiffStatus,
    /// Lazy intra-line segment cache (matches BC5 `TTextMap.UpdateSegs` / `NeedsSegs`):
    /// stored inline (zero heap allocation until the row is actually painted).
    inline: OnceLock<InlinePair>,
}

impl Clone for AlignedLine {
    #[inline]
    fn clone(&self) -> Self {
        let inline = OnceLock::new();
        if let Some(pair) = self.inline.get() {
            let _ = inline.set((Arc::clone(&pair.0), Arc::clone(&pair.1)));
        }
        Self {
            left_line_no: self.left_line_no,
            right_line_no: self.right_line_no,
            left_text: Arc::clone(&self.left_text),
            right_text: Arc::clone(&self.right_text),
            status: self.status,
            inline,
        }
    }
}

impl PartialEq for AlignedLine {
    fn eq(&self, other: &Self) -> bool {
        self.left_line_no == other.left_line_no
            && self.right_line_no == other.right_line_no
            && self.left_text == other.left_text
            && self.right_text == other.right_text
            && self.status == other.status
    }
}

impl Eq for AlignedLine {}

impl AlignedLine {
    #[inline]
    fn new(
        left_line_no: Option<usize>,
        right_line_no: Option<usize>,
        left_text: Arc<str>,
        right_text: Arc<str>,
        status: DiffStatus,
    ) -> Self {
        Self {
            left_line_no,
            right_line_no,
            left_text,
            right_text,
            status,
            inline: OnceLock::new(),
        }
    }

    fn inline_pair(&self) -> &InlinePair {
        self.inline.get_or_init(|| match self.status {
            DiffStatus::Same => (
                Arc::from([InlineChunk {
                    text: self.left_text.to_string(),
                    kind: InlineChunkKind::Same,
                }]),
                Arc::from([InlineChunk {
                    text: self.right_text.to_string(),
                    kind: InlineChunkKind::Same,
                }]),
            ),
            DiffStatus::LeftOrphan => (
                Arc::from([InlineChunk {
                    text: self.left_text.to_string(),
                    kind: InlineChunkKind::Diff,
                }]),
                Arc::from([]),
            ),
            DiffStatus::RightOrphan => (
                Arc::from([]),
                Arc::from([InlineChunk {
                    text: self.right_text.to_string(),
                    kind: InlineChunkKind::Diff,
                }]),
            ),
            DiffStatus::Different | DiffStatus::Unimportant => {
                let (l, r) = DiffEngine::compute_inline_diff(&self.left_text, &self.right_text);
                (l.into(), r.into())
            }
        })
    }

    #[inline]
    pub fn left_inline(&self) -> &[InlineChunk] {
        &self.inline_pair().0
    }

    #[inline]
    pub fn right_inline(&self) -> &[InlineChunk] {
        &self.inline_pair().1
    }

    #[inline]
    pub fn inline_chunks(&self, left: bool) -> &[InlineChunk] {
        if left {
            self.left_inline()
        } else {
            self.right_inline()
        }
    }

    /// Swap left and right sides in place, matching BC5 `TTextMap.Swap` ():
    /// preserves any cached intra-line segments without re-running `CompareSegsTo`.
    pub fn swap_sides(&mut self) {
        std::mem::swap(&mut self.left_line_no, &mut self.right_line_no);
        std::mem::swap(&mut self.left_text, &mut self.right_text);
        self.status = match self.status {
            DiffStatus::LeftOrphan => DiffStatus::RightOrphan,
            DiffStatus::RightOrphan => DiffStatus::LeftOrphan,
            other => other,
        };
        if let Some((left, right)) = self.inline.take() {
            let _ = self.inline.set((right, left));
        }
    }
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

impl DiffResult {
    /// Swap left and right sides across all aligned rows in O(N), matching BC5 `TTextMap.Swap`.
    pub fn swap_sides(&mut self) {
        for line in &mut self.lines {
            line.swap_sides();
        }
        std::mem::swap(&mut self.left_orphan_count, &mut self.right_orphan_count);
    }
}

/// Number of logical lines as `str::lines` counts them.
pub(crate) fn line_count(text: &str) -> usize {
    count_newlines(text.as_bytes()) + usize::from(!text.is_empty() && !text.ends_with('\n'))
}

pub(crate) fn count_newlines(bytes: &[u8]) -> usize {
    let mut total = 0usize;
    let (chunks, rem) = bytes.as_chunks::<64>();
    for chunk in chunks {
        let mut c = 0u32;
        for &b in chunk {
            c += u32::from(b == b'\n');
        }
        total += c as usize;
    }
    for &b in rem {
        total += usize::from(b == b'\n');
    }
    total
}

/// Binary search over monotonic 1-based line numbers interspersed with `None`
/// orphan rows. Runs in O(log N) on aligned regions instead of O(N) linear scan.
pub(crate) fn find_line_no(lines: &[AlignedLine], left: bool, target: usize) -> Option<usize> {
    if target == 0 || lines.is_empty() {
        return None;
    }
    let own = |line: &AlignedLine| if left { line.left_line_no } else { line.right_line_no };
    let (mut lo, mut hi) = (0usize, lines.len());
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        let mut p = mid;
        while p < hi && own(&lines[p]).is_none() {
            p += 1;
        }
        if p == hi {
            hi = mid;
        } else {
            let v = own(&lines[p]).unwrap();
            match v.cmp(&target) {
                std::cmp::Ordering::Equal => return Some(p),
                std::cmp::Ordering::Less => lo = p + 1,
                std::cmp::Ordering::Greater => hi = mid,
            }
        }
    }
    None
}

/// Count of present lines on `left` side strictly before `idx` in O(1) amortized
/// (walks back across at most one orphan run to read the last 1-based line number).
#[inline]
fn present_before(lines: &[AlignedLine], left: bool, idx: usize) -> usize {
    let mut i = idx.min(lines.len());
    while i > 0 {
        i -= 1;
        let n = if left {
            lines[i].left_line_no
        } else {
            lines[i].right_line_no
        };
        if let Some(num) = n {
            return num;
        }
    }
    0
}

/// Byte offset where logical line `n` (0-based) starts; the end for `n >= count`.
pub(crate) fn line_start(text: &str, n: usize) -> usize {
    if n == 0 {
        return 0;
    }
    let bytes = text.as_bytes();
    let mut seen = 0;
    for (index, chunk) in bytes.chunks(4096).enumerate() {
        let count = count_newlines(chunk);
        if seen + count >= n {
            for (offset, &byte) in chunk.iter().enumerate() {
                if byte == b'\n' {
                    seen += 1;
                    if seen == n {
                        return index * 4096 + offset + 1;
                    }
                }
            }
        }
        seen += count;
    }
    bytes.len()
}

/// Length of the common prefix, compared a block at a time (memcmp).
pub(crate) fn common_prefix(a: &[u8], b: &[u8]) -> usize {
    const BLOCK: usize = 4096;
    let limit = a.len().min(b.len());
    let mut at = 0;
    while at + BLOCK <= limit && a[at..at + BLOCK] == b[at..at + BLOCK] {
        at += BLOCK;
    }
    while at < limit && a[at] == b[at] {
        at += 1;
    }
    at
}

/// Length of the common suffix, at most `limit` bytes.
pub(crate) fn common_suffix(a: &[u8], b: &[u8], limit: usize) -> usize {
    const BLOCK: usize = 4096;
    let (n, m) = (a.len(), b.len());
    let mut at = 0;
    while at + BLOCK <= limit && a[n - at - BLOCK..n - at] == b[m - at - BLOCK..m - at] {
        at += BLOCK;
    }
    while at < limit && a[n - at - 1] == b[m - at - 1] {
        at += 1;
    }
    at
}

pub struct DiffEngine;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiffRules {
    pub ignore_case: bool,
    pub ignore_whitespace: bool,
    pub align_method: AlignMethod,
    pub skew_tolerance: usize,
    pub use_closeness: bool,
    pub never_align_diffs: bool,
}

impl Default for DiffRules {
    fn default() -> Self {
        Self {
            ignore_case: false,
            ignore_whitespace: false,
            align_method: AlignMethod::Standard,
            skew_tolerance: 2000,
            use_closeness: true,
            never_align_diffs: false,
        }
    }
}

/// Zero-allocation streaming check for minor equality (case + whitespace ignored).
#[inline]
fn is_minor_equal(left: &str, right: &str) -> bool {
    let mut a = left
        .chars()
        .filter(|c| !c.is_whitespace())
        .flat_map(char::to_lowercase);
    let mut b = right
        .chars()
        .filter(|c| !c.is_whitespace())
        .flat_map(char::to_lowercase);
    loop {
        match (a.next(), b.next()) {
            (Some(ca), Some(cb)) if ca == cb => continue,
            (None, None) => return true,
            _ => return false,
        }
    }
}

#[inline]
fn lines_equal_under_rules(left: &str, right: &str, rules: DiffRules) -> bool {
    if left == right {
        return true;
    }
    if !rules.ignore_case && !rules.ignore_whitespace {
        return false;
    }
    if rules.ignore_case && rules.ignore_whitespace {
        return is_minor_equal(left, right);
    }
    if rules.ignore_whitespace {
        return left
            .chars()
            .filter(|c| !c.is_whitespace())
            .eq(right.chars().filter(|c| !c.is_whitespace()));
    }
    left.chars()
        .flat_map(char::to_lowercase)
        .eq(right.chars().flat_map(char::to_lowercase))
}

impl DiffEngine {
    pub fn compare_rules(left_text: &str, right_text: &str, rules: DiffRules) -> DiffResult {
        let left_lines: Vec<&str> = left_text.lines().collect();
        let right_lines: Vec<&str> = right_text.lines().collect();

        let config = align::AlignConfig {
            method: rules.align_method,
            skew_tolerance: rules.skew_tolerance.max(1),
            use_closeness: rules.use_closeness,
            never_align_diffs: rules.never_align_diffs,
            ignore_case: rules.ignore_case,
            ignore_whitespace: rules.ignore_whitespace,
        };

        let pairs = align::align_lines(&left_lines, &right_lines, config);
        let empty: Arc<str> = Arc::from("");

        let mut lines = Vec::with_capacity(pairs.len());
        let mut diff_count = 0;
        let mut same_count = 0;
        let mut left_orphan_count = 0;
        let mut right_orphan_count = 0;

        for (left_idx, right_idx) in pairs {
            match (left_idx, right_idx) {
                (Some(li), Some(ri)) => {
                    let l_str = left_lines[li];
                    let r_str = right_lines[ri];
                    let status = if lines_equal_under_rules(l_str, r_str, rules) {
                        same_count += 1;
                        DiffStatus::Same
                    } else {
                        diff_count += 1;
                        if is_minor_equal(l_str, r_str) {
                            DiffStatus::Unimportant
                        } else {
                            DiffStatus::Different
                        }
                    };
                    let l_arc: Arc<str> = Arc::from(l_str);
                    let r_arc: Arc<str> = if l_str == r_str {
                        Arc::clone(&l_arc)
                    } else {
                        Arc::from(r_str)
                    };
                    lines.push(AlignedLine::new(
                        Some(li + 1),
                        Some(ri + 1),
                        l_arc,
                        r_arc,
                        status,
                    ));
                }
                (Some(li), None) => {
                    left_orphan_count += 1;
                    lines.push(AlignedLine::new(
                        Some(li + 1),
                        None,
                        Arc::from(left_lines[li]),
                        Arc::clone(&empty),
                        DiffStatus::LeftOrphan,
                    ));
                }
                (None, Some(ri)) => {
                    right_orphan_count += 1;
                    lines.push(AlignedLine::new(
                        None,
                        Some(ri + 1),
                        Arc::clone(&empty),
                        Arc::from(right_lines[ri]),
                        DiffStatus::RightOrphan,
                    ));
                }
                (None, None) => {}
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

    pub fn update_rules(
        left: &str,
        right: &str,
        previous: &DiffResult,
        rules: DiffRules,
    ) -> DiffResult {
        Self::update_with_rules(left, right, previous, rules)
    }

    /// Recompare only the changed alignment section, bounded by unchanged anchors.
    /// Files that were replaced wholesale fall back to the complete comparison.
    #[cfg(test)]
    pub fn update(left_text: &str, right_text: &str, previous: &DiffResult) -> DiffResult {
        Self::update_with_rules(left_text, right_text, previous, DiffRules::default())
    }

    fn update_with_rules(
        left_text: &str,
        right_text: &str,
        previous: &DiffResult,
        rules: DiffRules,
    ) -> DiffResult {
        let left: Vec<_> = left_text.lines().collect();
        let right: Vec<_> = right_text.lines().collect();
        let old_left: Vec<_> = previous
            .lines
            .iter()
            .filter(|line| line.left_line_no.is_some())
            .map(|line| &*line.left_text)
            .collect();
        let old_right: Vec<_> = previous
            .lines
            .iter()
            .filter(|line| line.right_line_no.is_some())
            .map(|line| &*line.right_text)
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
        Self::update_changes(
            [left_text, right_text],
            previous,
            changes,
            [old_left.len(), old_right.len()],
            [left.len(), right.len()],
            rules,
        )
    }

    /// Same result as [`Self::update`] after an edit of one side whose previous
    /// text is known (the editor's buffer before the keystroke): the changed lines
    /// come from a byte comparison instead of rebuilding every line of both sides.
    #[cfg(test)]
    pub fn update_edit(
        old_own: &str,
        new_own: &str,
        other: &str,
        own_left: bool,
        previous: &DiffResult,
    ) -> DiffResult {
        Self::update_edit_rules(
            old_own,
            new_own,
            other,
            own_left,
            previous,
            DiffRules::default(),
        )
    }

    pub fn update_edit_rules(
        old_own: &str,
        new_own: &str,
        other: &str,
        own_left: bool,
        previous: &DiffResult,
        rules: DiffRules,
    ) -> DiffResult {
        let (old, new) = (old_own.as_bytes(), new_own.as_bytes());
        // Line totals come from the previous alignment (it describes old_own and
        // other) plus the newlines added or removed: no pass over whole buffers.
        let last_number = |left: bool| {
            previous
                .lines
                .iter()
                .rev()
                .find_map(|line| {
                    if left {
                        line.left_line_no
                    } else {
                        line.right_line_no
                    }
                })
                .unwrap_or(0)
        };
        let old_count = last_number(own_left);
        let other_count = last_number(!own_left);
        let mut new_count = old_count;
        let change = (old != new).then(|| {
            let prefix = common_prefix(old, new);
            let suffix = common_suffix(old, new, old.len().min(new.len()) - prefix);
            // Lines containing the first and last changed byte (conservative: a
            // wider section is only recompared, never misaligned).
            let first = count_newlines(&old[..prefix]);
            let (old_end, new_end) = (old.len() - suffix, new.len() - suffix);
            let mut last = first + count_newlines(&old[prefix..old_end]);
            // A suffix that starts a line in both texts leaves that line unchanged.
            let line_start_at = |text: &[u8], at: usize| at == 0 || text[at - 1] == b'\n';
            if line_start_at(old, old_end) && line_start_at(new, new_end) && last > 0 {
                last -= 1;
            }
            new_count = old_count + count_newlines(&new[prefix..new_end])
                - count_newlines(&old[prefix..old_end]);
            // `lines()` ignores a final newline: adding or removing the last line
            // terminator changes the count only through a non-empty last line.
            let ends = |text: &[u8]| text.last().is_some_and(|&b| b != b'\n');
            new_count = new_count + usize::from(ends(new)) - usize::from(ends(old));
            first.min(old_count)..(last + 1).max(first).min(old_count)
        });
        debug_assert_eq!(old_count, line_count(old_own));
        debug_assert_eq!(new_count, line_count(new_own));
        let (texts, changes, counts, new_counts) = if own_left {
            (
                [new_own, other],
                [change, None],
                [old_count, other_count],
                [new_count, other_count],
            )
        } else {
            (
                [other, new_own],
                [None, change],
                [other_count, old_count],
                [other_count, new_count],
            )
        };
        Self::update_changes(texts, previous, changes, counts, new_counts, rules)
    }

    /// In-place incremental update after editing one side whose previous text is
    /// known. For typical single-line typing (`left_shift == 0 && right_shift == 0`),
    /// overwrites only the few touched rows in `result.lines` with zero 80k-line
    /// `Vec` allocations and zero `Arc` refcount churn.
    pub fn update_edit_rules_in_place(
        old_own: &str,
        new_own: &str,
        other: &str,
        own_left: bool,
        result: &mut DiffResult,
        rules: DiffRules,
    ) {
        let (old, new) = (old_own.as_bytes(), new_own.as_bytes());
        if old == new {
            return;
        }
        if result.lines.is_empty() {
            *result = if own_left {
                Self::compare_rules(new_own, other, rules)
            } else {
                Self::compare_rules(other, new_own, rules)
            };
            return;
        }
        let old_count = present_before(&result.lines, own_left, result.lines.len());
        let other_count = present_before(&result.lines, !own_left, result.lines.len());
        let prefix = common_prefix(old, new);
        let suffix = common_suffix(old, new, old.len().min(new.len()) - prefix);
        let first = count_newlines(&old[..prefix]);
        let (old_end, new_end) = (old.len() - suffix, new.len() - suffix);
        let mut last = first + count_newlines(&old[prefix..old_end]);
        let line_start_at = |text: &[u8], at: usize| at == 0 || text[at - 1] == b'\n';
        if line_start_at(old, old_end) && line_start_at(new, new_end) && last > 0 {
            last -= 1;
        }
        let mut new_count = old_count + count_newlines(&new[prefix..new_end])
            - count_newlines(&old[prefix..old_end]);
        let ends = |text: &[u8]| text.last().is_some_and(|&b| b != b'\n');
        new_count = new_count + usize::from(ends(new)) - usize::from(ends(old));
        let change = first.min(old_count)..(last + 1).max(first).min(old_count);

        let ([left_text, right_text], old_counts, new_counts) = if own_left {
            ([new_own, other], [old_count, other_count], [new_count, other_count])
        } else {
            ([other, new_own], [other_count, old_count], [other_count, new_count])
        };

        let begin = find_line_no(&result.lines, own_left, change.start + 1)
            .unwrap_or(result.lines.len());
        let finish = if change.is_empty() {
            begin
        } else {
            find_line_no(&result.lines, own_left, change.end)
                .map(|index| index + 1)
                .unwrap_or(result.lines.len())
        };
        let mut start = begin.saturating_sub(1);
        let mut end = (finish + 1).min(result.lines.len());
        while start > 0 && result.lines[start].status != DiffStatus::Same {
            start -= 1;
        }
        while end < result.lines.len() && result.lines[end - 1].status != DiffStatus::Same {
            end += 1;
        }
        if end - start > result.lines.len().max(64) / 2 {
            *result = Self::compare_rules(left_text, right_text, rules);
            return;
        }
        let prefix_left = present_before(&result.lines, true, start);
        let prefix_right = present_before(&result.lines, false, start);
        let suffix_left = old_counts[0] - present_before(&result.lines, true, end);
        let suffix_right = old_counts[1] - present_before(&result.lines, false, end);

        fn slice_lines(text: &str, prefix: usize, count: usize, suffix: usize) -> &str {
            let from = line_start(text, prefix);
            let to = line_start(text, count - suffix);
            &text[from..to]
        }
        let mut region = Self::compare_rules(
            slice_lines(left_text, prefix_left, new_counts[0], suffix_left),
            slice_lines(right_text, prefix_right, new_counts[1], suffix_right),
            rules,
        );
        for line in &mut region.lines {
            line.left_line_no = line.left_line_no.map(|n| n + prefix_left);
            line.right_line_no = line.right_line_no.map(|n| n + prefix_right);
        }
        let left_shift = new_counts[0] as isize - old_counts[0] as isize;
        let right_shift = new_counts[1] as isize - old_counts[1] as isize;

        // Subtract counts of replaced slice
        for line in &result.lines[start..end] {
            match line.status {
                DiffStatus::Same => result.same_count -= 1,
                DiffStatus::Different | DiffStatus::Unimportant => result.diff_count -= 1,
                DiffStatus::LeftOrphan => result.left_orphan_count -= 1,
                DiffStatus::RightOrphan => result.right_orphan_count -= 1,
            }
        }
        result.same_count += region.same_count;
        result.diff_count += region.diff_count;
        result.left_orphan_count += region.left_orphan_count;
        result.right_orphan_count += region.right_orphan_count;

        let old_span = end - start;
        let new_span = region.lines.len();
        if left_shift != 0 || right_shift != 0 {
            for line in &mut result.lines[end..] {
                if left_shift != 0 {
                    line.left_line_no = line
                        .left_line_no
                        .map(|n| n.checked_add_signed(left_shift).unwrap());
                }
                if right_shift != 0 {
                    line.right_line_no = line
                        .right_line_no
                        .map(|n| n.checked_add_signed(right_shift).unwrap());
                }
            }
        }
        if old_span == new_span {
            for (dst, src) in result.lines[start..end].iter_mut().zip(region.lines) {
                *dst = src;
            }
        } else {
            result.lines.splice(start..end, region.lines);
        }
        // Incrementally update `result.sections` in O(S) (S = number of diff sections)
        let row_delta = new_span as isize - old_span as isize;
        let sec_from = result.sections.partition_point(|s| s.end < start);
        let sec_to = result.sections.partition_point(|s| s.start <= end);
        let mut replacement_sections = Vec::new();
        let scan_start = if sec_from < sec_to {
            result.sections[sec_from].start.min(start)
        } else {
            start
        };
        let scan_end = if sec_from < sec_to {
            result.sections[sec_to - 1]
                .end
                .max(end)
                .checked_add_signed(row_delta)
                .unwrap()
                .min(result.lines.len())
        } else {
            (start + new_span).min(result.lines.len())
        };
        let mut cur_start = None;
        for (idx, line) in result.lines[scan_start..scan_end].iter().enumerate() {
            let abs = scan_start + idx;
            if line.status != DiffStatus::Same {
                cur_start.get_or_insert(abs);
            } else if let Some(s) = cur_start.take() {
                replacement_sections.push(s..abs);
            }
        }
        if let Some(s) = cur_start {
            replacement_sections.push(s..scan_end);
        }
        if row_delta != 0 {
            for sec in &mut result.sections[sec_to..] {
                sec.start = sec.start.checked_add_signed(row_delta).unwrap();
                sec.end = sec.end.checked_add_signed(row_delta).unwrap();
            }
        }
        result
            .sections
            .splice(sec_from..sec_to, replacement_sections);
    }

    fn update_changes(
        texts: [&str; 2],
        previous: &DiffResult,
        changes: [Option<std::ops::Range<usize>>; 2],
        old_counts: [usize; 2],
        new_counts: [usize; 2],
        rules: DiffRules,
    ) -> DiffResult {
        let [left_text, right_text] = texts;
        if changes.iter().all(Option::is_none) {
            return previous.clone();
        }
        if previous.lines.is_empty() {
            return Self::compare_rules(left_text, right_text, rules);
        }
        let mut start = previous.lines.len();
        let mut end = 0;
        for (side, change) in changes.iter().enumerate() {
            if let Some(change) = change {
                let left = side == 0;
                let begin = find_line_no(&previous.lines, left, change.start + 1)
                    .unwrap_or(previous.lines.len());
                let finish = if change.is_empty() {
                    begin
                } else {
                    find_line_no(&previous.lines, left, change.end)
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
            return Self::compare_rules(left_text, right_text, rules);
        }
        let prefix_left = present_before(&previous.lines, true, start);
        let prefix_right = present_before(&previous.lines, false, start);
        let suffix_left = old_counts[0] - present_before(&previous.lines, true, end);
        let suffix_right = old_counts[1] - present_before(&previous.lines, false, end);
        // The section's lines, each with its terminator: `lines()` of this slice is
        // exactly the joined lines (a trailing empty line keeps its newline).
        fn region(text: &str, prefix: usize, count: usize, suffix: usize) -> &str {
            let from = line_start(text, prefix);
            let to = line_start(text, count - suffix);
            &text[from..to]
        }
        let region = Self::compare_rules(
            region(left_text, prefix_left, new_counts[0], suffix_left),
            region(right_text, prefix_right, new_counts[1], suffix_right),
            rules,
        );
        let left_shift = new_counts[0] as isize - old_counts[0] as isize;
        let right_shift = new_counts[1] as isize - old_counts[1] as isize;
        let mut lines = Vec::with_capacity(start + region.lines.len() + previous.lines.len() - end);
        lines.extend_from_slice(&previous.lines[..start]);
        lines.extend(region.lines.into_iter().map(|mut line| {
            line.left_line_no = line.left_line_no.map(|n| n + prefix_left);
            line.right_line_no = line.right_line_no.map(|n| n + prefix_right);
            line
        }));
        if left_shift == 0 && right_shift == 0 {
            lines.extend_from_slice(&previous.lines[end..]);
        } else {
            lines.extend(previous.lines[end..].iter().map(|line| {
                let mut line = line.clone();
                line.left_line_no = line
                    .left_line_no
                    .map(|n| n.checked_add_signed(left_shift).unwrap());
                line.right_line_no = line
                    .right_line_no
                    .map(|n| n.checked_add_signed(right_shift).unwrap());
                line
            }));
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
    #[cfg(test)]
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
    /// Compare case-folded character keys first: case-only edits stay aligned
    /// and receive blue only on the actual changed characters. Whitespace-only
    /// inserts/deletes are minor; original characters always remain in the buffer.
    pub fn compute_inline_diff(left: &str, right: &str) -> (Vec<InlineChunk>, Vec<InlineChunk>) {
        inline::compare(left, right)
    }

    /// Compute aligned lines between left text and right text using the default rules.
    pub fn compare(left_text: &str, right_text: &str) -> DiffResult {
        Self::compare_rules(left_text, right_text, DiffRules::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edit_update_matches_line_update_for_every_kind_of_edit() {
        let base: String = (0..400)
            .map(|i| match i % 7 {
                0 => "\n".to_string(),
                1 => format!("EN=linha {i} ação\n"),
                2 => format!("PT=linha {}\n", i / 3),
                _ => format!("[ID {:04}]\n", i % 50),
            })
            .collect();
        let other = base
            .replace("PT=linha 1", "PT=uma linha")
            .replace("[ID 0003]\n", "");
        let previous = DiffEngine::compare(&base, &other);
        let mut seed = 7u64;
        let mut next = |n: usize| {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (seed >> 33) as usize % n.max(1)
        };
        let inserts = ["x", "\n", "nova\n", "\n\n", "", "PT=linha 1\n", "ç"];
        for _ in 0..300 {
            let chars: Vec<(usize, char)> = base.char_indices().collect();
            let at = next(chars.len() + 1);
            let start = chars.get(at).map_or(base.len(), |c| c.0);
            let remove = next(4);
            let end = chars.get(at + remove).map_or(base.len(), |c| c.0);
            let mut edited = base.clone();
            edited.replace_range(start..end, inserts[next(inserts.len())]);
            for own_left in [true, false] {
                let expected = if own_left {
                    DiffEngine::update(&edited, &other, &previous)
                } else {
                    DiffEngine::update(&other, &edited, &DiffEngine::compare(&other, &base))
                };
                let prior = if own_left {
                    previous.clone()
                } else {
                    DiffEngine::compare(&other, &base)
                };
                let actual = DiffEngine::update_edit(&base, &edited, &other, own_left, &prior);
                assert_eq!(actual, expected, "edit {start}..{end} left={own_left}");
            }
        }
        for text in ["", "a", "a\n", "\n", "a\nb", "a\n\nb\n"] {
            assert_eq!(line_count(text), text.lines().count(), "{text:?}");
            for n in 0..=line_count(text) + 1 {
                let lines: Vec<&str> = text.lines().collect();
                let expected = lines[..n.min(lines.len())]
                    .iter()
                    .map(|l| l.len())
                    .sum::<usize>();
                assert!(line_start(text, n) >= expected);
            }
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
                line.left_inline()
                    .iter()
                    .map(|c| c.text.as_str())
                    .collect::<String>(),
                line.left_text.to_string()
            );
            assert_eq!(
                line.right_inline()
                    .iter()
                    .map(|c| c.text.as_str())
                    .collect::<String>(),
                line.right_text.to_string()
            );
        }
        assert!(diff.lines[1]
            .left_inline()
            .iter()
            .any(|c| c.kind == InlineChunkKind::Unimportant && c.text == "M"));
        assert!(diff.lines[1]
            .right_inline()
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
                ..Default::default()
            },
        );
        assert_eq!(ignored.lines[0].status, DiffStatus::Same);
        assert_eq!(ignored.lines[1].status, DiffStatus::Different);
        assert_eq!(&*ignored.lines[0].left_text, "A Pedra do Sigilo");
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

    #[test]
    fn alignment_modes_preserve_lines_and_options() {
        let left = "\
fn alpha() {
    let x = 1;
    let y = 2;
}

fn beta() {
    println!(\"hello world\");
    let z = 3;
}
";
        let right = "\
fn alpha() {
    let x = 10;
    let y = 2;
}

fn inserted() {
    do_work();
}

fn beta() {
    println!(\"hello brave world\");
    let z = 3;
}
";
        for method in AlignMethod::ALL {
            let rules = DiffRules {
                align_method: method,
                ..Default::default()
            };
            let result = DiffEngine::compare_rules(left, right, rules);
            let reconstructed_left: Vec<&str> = result
                .lines
                .iter()
                .filter(|l| l.left_line_no.is_some())
                .map(|l| &*l.left_text)
                .collect();
            let reconstructed_right: Vec<&str> = result
                .lines
                .iter()
                .filter(|l| l.right_line_no.is_some())
                .map(|l| &*l.right_text)
                .collect();
            assert_eq!(
                reconstructed_left,
                left.lines().collect::<Vec<_>>(),
                "left lines lost in {method:?}"
            );
            assert_eq!(
                reconstructed_right,
                right.lines().collect::<Vec<_>>(),
                "right lines lost in {method:?}"
            );
            if method != AlignMethod::None {
                assert!(
                    result.same_count >= 6,
                    "{method:?} expected >= 6 same lines, got {}",
                    result.same_count
                );
                // The modified println! line must stay paired via closeness
                assert!(
                    result.lines.iter().any(|l| {
                        l.left_text.contains("hello world")
                            && l.right_text.contains("hello brave world")
                            && l.status == DiffStatus::Different
                    }),
                    "{method:?} should align similar modified line"
                );
            }
        }

        // NeverAlignDiffs must split mismatches into orphans instead of pairing them
        for method in [
            AlignMethod::Standard,
            AlignMethod::Patience,
            AlignMethod::Alternate,
        ] {
            let rules = DiffRules {
                align_method: method,
                never_align_diffs: true,
                use_closeness: false,
                ..Default::default()
            };
            let result = DiffEngine::compare_rules(left, right, rules);
            assert_eq!(
                result.diff_count, 0,
                "{method:?} with never_align_diffs must emit 0 aligned diffs"
            );
        }
    }


    #[test]
    fn bc5_closeness_preserves_case_and_embedded_spaces_on_dissimilar_text() {
        // Synthetic reproduction of dissimilar README-style text vs short notes
        // containing blank lines and Portuguese accents: BC5 preserves letter case
        // and embedded spaces in GetClosenessStr (), so dissimilar lines
        // are NOT prematurely scattered into early short headings/paragraphs.
        let mut readme_lines = vec![
            "MYTH 1.3  README".to_string(),
            "".to_string(),
            "".to_string(),
            "---------------------------------------------------------".to_string(),
            "CONTENTS:".to_string(),
            "".to_string(),
            "       1).  Introduction".to_string(),
            "".to_string(),
            "       2).  3RD Party plugins".to_string(),
            "".to_string(),
            "       3).  Body Count".to_string(),
            "".to_string(),
        ];
        for i in 0..28 {
            readme_lines.push(format!(
                "Paragraph line {i} explaining plugin feature updates and map behavior."
            ));
            if i % 6 == 2 {
                readme_lines.push(String::new());
            }
        }
        // Later section with a closer prose line length/character distribution
        readme_lines.push("Check your email address or continue from where we stopped.".to_string());
        readme_lines.push(String::new());
        readme_lines.push("user.example@mail.com".to_string());
        let readme = readme_lines.join("\n");
        let notes = [
            "Salve um handoff para a próxima sessão",
            "",
            "Onde paramos? Continue de onde paramos, use o ai-memory",
            "",
            "user.example@mail.com",
        ]
        .join("\n");

        let res = DiffEngine::compare_rules(&readme, &notes, DiffRules::default());
        assert_eq!(res.lines[0].left_text.as_ref(), "MYTH 1.3  README");
        assert_eq!(
            res.lines[0].right_text.as_ref(),
            "Salve um handoff para a próxima sessão"
        );
        assert_eq!(res.lines[1].status, DiffStatus::Same);
        // Lines 3..12 (CONTENTS block) must stay clean left orphans instead of being
        // interrupted by premature matches.
        for row in &res.lines[2..12] {
            assert_eq!(
                row.status,
                DiffStatus::LeftOrphan,
                "unexpected early match on row {:?}",
                row
            );
        }
        // Inline diff on row 0 must mark M, H, and trailing E in README as Unimportant (blue)
        // and the three spaces as Same, matching BC5 CompareSegsTo.
        let left_inline = res.lines[0].left_inline();
        assert_eq!(
            left_inline
                .iter()
                .filter(|c| c.kind == InlineChunkKind::Unimportant)
                .map(|c| c.text.as_str())
                .collect::<Vec<_>>(),
            vec!["M", "H", "E"]
        );
        // Swapping sides via DiffResult::swap_sides (matching BC5 TTextMap.Swap) preserves
        // cached inline segments so E remains Unimportant on the swapped right pane too.
        let mut swapped = res.clone();
        swapped.swap_sides();
        assert_eq!(
            swapped.lines[0]
                .right_inline()
                .iter()
                .filter(|c| c.kind == InlineChunkKind::Unimportant)
                .map(|c| c.text.as_str())
                .collect::<Vec<_>>(),
            vec!["M", "H", "E"]
        );
    }

}
