//! Byte alignment with explicit original-file offsets. Gaps are view data;
//! they are never inserted into a BinaryDocument. Native replay fixtures live
//! in docs/reference/controlled-hex-replay (five default BC4 comparisons).
use similar::{capture_diff_slices_deadline, Algorithm};
use std::{
    ops::Range,
    time::{Duration, Instant},
};

#[derive(Default)]
struct Run {
    start: usize,
    len: usize,
    left: Range<usize>,
    right: Range<usize>,
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
impl BinaryDiff {
    pub fn compare(left: &[u8], right: &[u8]) -> Self {
        let deadline = Instant::now() + Duration::from_secs(4);
        // Keep exact boundary matches out of Compact's shifting pass. In the
        // native AB/BB replay the final B stays paired with the final B;
        // compacting the whole input instead created two false orphans.
        let prefix = left.iter().zip(right).take_while(|(a, b)| a == b).count();
        let (mut end_left, mut end_right) = (left.len(), right.len());
        while end_left > prefix && end_right > prefix && left[end_left - 1] == right[end_right - 1]
        {
            end_left -= 1;
            end_right -= 1;
        }
        let ops = capture_diff_slices_deadline(
            Algorithm::Myers,
            &left[prefix..end_left],
            &right[prefix..end_right],
            Some(deadline),
        );
        let mut diff = Self {
            lengths: [left.len(), right.len()],
            limited: Instant::now() >= deadline,
            ..Self::default()
        };
        diff.push(0..prefix, 0..prefix);
        for op in ops {
            let old = op.old_range();
            let new = op.new_range();
            let a = old.start + prefix..old.end + prefix;
            let b = new.start + prefix..new.end + prefix;
            let paired = a.len().min(b.len());
            diff.push(a.start..a.start + paired, b.start..b.start + paired);
            diff.push(a.start + paired..a.end, b.end..b.end);
            diff.push(a.end..a.end, b.start + paired..b.end);
        }
        diff.push(end_left..left.len(), end_right..right.len());
        for run in &diff.runs {
            if run.left.is_empty() {
                diff.right_orphans += run.len;
                diff.first_difference.get_or_insert(run.start);
                diff.last_difference = Some(run.start + run.len - 1);
            } else if run.right.is_empty() {
                diff.left_orphans += run.len;
                diff.first_difference.get_or_insert(run.start);
                diff.last_difference = Some(run.start + run.len - 1);
            } else {
                for (i, (a, b)) in left[run.left.clone()]
                    .iter()
                    .zip(&right[run.right.clone()])
                    .enumerate()
                {
                    if a == b {
                        diff.same += 1;
                    } else {
                        diff.different += 1;
                        diff.first_difference.get_or_insert(run.start + i);
                        diff.last_difference = Some(run.start + i);
                    }
                }
            }
        }
        diff
    }
    fn push(&mut self, left: Range<usize>, right: Range<usize>) {
        let len = left.len().max(right.len());
        if len > 0 {
            self.runs.push(Run {
                start: self.len,
                len,
                left,
                right,
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
    fn native_hex_audit_ambiguous_inputs() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("docs/reference/controlled-hex-audit-2026-10-30");
        let cases: serde_json::Value =
            serde_json::from_slice(&std::fs::read(root.join("expected.json")).unwrap()).unwrap();
        let mut failures = Vec::new();
        for case in cases.as_array().unwrap() {
            let name = case["name"].as_str().unwrap();
            let a = std::fs::read(root.join(format!("{name}-left.bin"))).unwrap();
            let b = std::fs::read(root.join(format!("{name}-right.bin"))).unwrap();
            let d = BinaryDiff::compare(&a, &b);
            let expected: Vec<(Option<usize>, Option<usize>)> =
                serde_json::from_value(case["pairs"].clone()).unwrap();
            let actual: Vec<_> = (0..d.len).map(|i| d.pair(i)).collect();
            if actual != expected {
                failures.push(
                    serde_json::json!({"name":name,"native_pairs":expected,"actual_pairs":actual}),
                );
            }
        }
        std::fs::write(
            root.join("audit-mismatches.json"),
            serde_json::to_vec_pretty(&failures).unwrap(),
        )
        .unwrap();
        assert!(
            failures.is_empty(),
            "divergent cases: {:?}",
            failures
                .iter()
                .map(|c| c["name"].as_str().unwrap())
                .collect::<Vec<_>>()
        );
    }
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
    #[test]
    fn native_hex_replays_preserve_every_pair_and_byte_count_in_both_directions() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("docs/reference/controlled-hex-replay");
        let cases: serde_json::Value =
            serde_json::from_slice(&std::fs::read(root.join("expected.json")).unwrap()).unwrap();
        for case in cases.as_array().unwrap() {
            let name = case["name"].as_str().unwrap();
            let a = std::fs::read(root.join(format!("{name}-left.bin"))).unwrap();
            let b = std::fs::read(root.join(format!("{name}-right.bin"))).unwrap();
            let pairs: Vec<(Option<usize>, Option<usize>)> =
                serde_json::from_value(case["pairs"].clone()).unwrap();
            for reversed in [false, true] {
                let (left, right) = if reversed { (&b, &a) } else { (&a, &b) };
                let diff = BinaryDiff::compare(left, right);
                assert!(!diff.limited);
                let expected: Vec<_> = pairs
                    .iter()
                    .map(|&(a, b)| if reversed { (b, a) } else { (a, b) })
                    .collect();
                assert_eq!(
                    (0..diff.len).map(|i| diff.pair(i)).collect::<Vec<_>>(),
                    expected,
                    "{name} reversed={reversed}"
                );
                assert_eq!(diff.same, case["counts"]["same"].as_u64().unwrap() as usize);
                assert_eq!(
                    diff.different,
                    case["counts"]["difference"].as_u64().unwrap() as usize
                );
                for i in 0..diff.len {
                    for left in [true, false] {
                        if let Some(offset) = diff.offset(left, i) {
                            assert_eq!(diff.index(left, offset), i);
                            assert_eq!(diff.range(left, i..i + 1), offset..offset + 1);
                        } else {
                            assert!(diff.range(left, i..i + 1).is_empty());
                        }
                    }
                }
            }
        }
    }
}
