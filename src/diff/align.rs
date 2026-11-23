//! Faithful Rust implementation of AnyCompare's line alignment engine
//! (`BcAlignLines.pas`):
//! - `AlignMethod::Standard`  → `TBcSmartAlign` ()
//! - `AlignMethod::Patience`  → `TBcPatienceDiffAlign` ()
//! - `AlignMethod::Alternate` → `TBcQuickAlign` ( / Myers `u32`)
//! - `AlignMethod::None`      → `alignNone` (direct 1-to-1)

use super::line_pairing::{
    build_compact_hist, closeness_score_bytes, closeness_score_from_common,
};
use super::reference_myers::{self, FxHashMap};
use serde::{Deserialize, Serialize};
use similar::DiffTag;

/// Alignment engine selection matching BC5's `TBcAlignKind` enum.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AlignMethod {
    /// BC5 `alignSmart` (3): Standard alignment — weighted diagonal runs,
    /// sliding `SkewTolerance` window, `EGoodEnough` early exit, and
    /// Needleman-Wunsch DP with closeness matching. Factory default in BC5.
    #[default]
    Standard,
    /// BC5 `alignPatience` (1): Patience Diff hybrid — unique-line anchors +
    /// LIS, with closeness DP on small gaps and `Standard` fallback on large ones.
    Patience,
    /// BC5 `alignQuick` (2): Bidirectional Myers O(ND) on 32-bit interned line
    /// IDs + closeness pairing on replace blocks.
    Alternate,
    /// BC5 `alignNone` (0): Direct line-by-line alignment without skew search.
    None,
}

impl AlignMethod {
    pub const ALL: [Self; 4] = [Self::Standard, Self::Patience, Self::Alternate, Self::None];

    pub fn label(self) -> &'static str {
        match self {
            Self::Standard => "text.standard_bc5_smart",
            Self::Patience => "text.patience_diff_bc5",
            Self::Alternate => "text.alternate_myers",
            Self::None => "text.none_line_by_line",
        }
    }

    pub fn index(self) -> usize {
        match self {
            Self::Standard => 0,
            Self::Patience => 1,
            Self::Alternate => 2,
            Self::None => 3,
        }
    }

    pub fn from_index(index: usize) -> Self {
        Self::ALL.get(index).copied().unwrap_or_default()
    }
}

#[derive(Clone, Copy)]
pub(crate) struct AlignConfig {
    pub method: AlignMethod,
    pub skew_tolerance: usize,
    pub use_closeness: bool,
    pub never_align_diffs: bool,
    pub ignore_case: bool,
    pub ignore_whitespace: bool,
}

#[derive(Clone, Copy)]
struct AnchorRun {
    count: usize,
    weight: i32,
    src: usize,
    tgt: usize,
}

#[derive(Clone, Copy)]
struct WindowSpan {
    l0: usize,
    r0: usize,
    l1: usize,
    r1: usize,
}

/// BC5 `BcAlignLines.CalcLineWeight` ():
/// empty line = 2, whitespace-only/unimportant = 31, normal code line = 70.
#[inline]
fn calc_line_weight(raw: &str, is_sig: bool) -> i32 {
    if raw.is_empty() {
        return 2;
    }
    if is_sig {
        70 // 2 * 10 + 50
    } else {
        31 // 1 + 30
    }
}

/// Build the closeness key (UTF-8 bytes, case-preserving) matching BC5's
/// `TBcAlignLines.GetClosenessStr` ( -> `GetSigStr(side, idx, caseSig=true)`)
/// and `TBcSmartAlign.GetClosenessAnsiCached` (, `CP_UTF8 = 0xFDE9`).
/// By default (`sub_1410134C0`), leading/trailing whitespace is unimportant while
/// embedded whitespace and original letter case are preserved; if the significant
/// string is empty, BC5 falls back to the raw line.
fn build_closeness_key(line: &str, ignore_whitespace: bool) -> Vec<u8> {
    if ignore_whitespace {
        let mut out = Vec::with_capacity(line.len());
        for c in line.chars() {
            if !c.is_whitespace() {
                let mut buf = [0u8; 4];
                out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
            }
        }
        if out.is_empty() {
            line.as_bytes().to_vec()
        } else {
            out
        }
    } else {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            line.as_bytes().to_vec()
        } else {
            trimmed.as_bytes().to_vec()
        }
    }
}

/// Build normalized comparison key when `ignore_case` or `ignore_whitespace` is active.
fn build_norm_key(line: &str, ignore_case: bool, ignore_whitespace: bool) -> String {
    let mut out = String::with_capacity(line.len());
    for c in line.chars() {
        if ignore_whitespace && c.is_whitespace() {
            continue;
        }
        if ignore_case {
            out.extend(c.to_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

#[derive(Clone)]
struct CachedHist {
    key_len: i32,
    entries: Vec<(u8, u16)>,
}

struct PreparedLines<'a> {
    raw: [&'a [&'a str]; 2],
    ids: [Vec<u32>; 2],
    weights: [Vec<i32>; 2],
    closeness_cache: [Vec<Option<CachedHist>>; 2],
    config: AlignConfig,
}

impl<'a> PreparedLines<'a> {
    fn new(left: &'a [&'a str], right: &'a [&'a str], config: AlignConfig) -> Self {
        let (ids_l, ids_r) = if !config.ignore_case && !config.ignore_whitespace {
            let (a, b, _) = reference_myers::intern_lines(left, right);
            (a, b)
        } else {
            let mut table: FxHashMap<String, u32> = FxHashMap::with_capacity_and_hasher(
                left.len().max(right.len()),
                Default::default(),
            );
            let mut next_id = 0u32;
            let mut intern = |s: &str| -> u32 {
                let key = build_norm_key(s, config.ignore_case, config.ignore_whitespace);
                if let Some(&id) = table.get(&key) {
                    id
                } else {
                    let id = next_id;
                    next_id += 1;
                    table.insert(key, id);
                    id
                }
            };
            let a = left.iter().map(|s| intern(s)).collect();
            let b = right.iter().map(|s| intern(s)).collect();
            (a, b)
        };

        let make_weights = |lines: &[&str]| -> Vec<i32> {
            lines
                .iter()
                .map(|s| {
                    let is_sig = s.bytes().any(|b| !b.is_ascii_whitespace())
                        || (!s.is_ascii() && s.chars().any(|c| !c.is_whitespace()));
                    calc_line_weight(s, is_sig)
                })
                .collect()
        };

        let wl = make_weights(left);
        let wr = make_weights(right);
        let cache_l = if config.use_closeness {
            vec![None; left.len()]
        } else {
            Vec::new()
        };
        let cache_r = if config.use_closeness {
            vec![None; right.len()]
        } else {
            Vec::new()
        };

        Self {
            raw: [left, right],
            ids: [ids_l, ids_r],
            weights: [wl, wr],
            closeness_cache: [cache_l, cache_r],
            config,
        }
    }

    #[inline]
    fn ensure_hist(&mut self, side: usize, idx: usize) {
        if self.closeness_cache[side][idx].is_none() {
            let raw = self.raw[side][idx];
            let (key_len, entries) = if !self.config.ignore_whitespace {
                let trimmed = raw.trim();
                let bytes = if trimmed.is_empty() {
                    raw.as_bytes()
                } else {
                    trimmed.as_bytes()
                };
                (bytes.len() as i32, build_compact_hist(bytes))
            } else {
                let key = build_closeness_key(raw, true);
                (key.len() as i32, build_compact_hist(&key))
            };
            self.closeness_cache[side][idx] = Some(CachedHist { key_len, entries });
        }
    }

    /// Exact BC5 `TBcSmartAlign.LinePairScore` ().
    #[inline]
    fn line_pair_score(&mut self, i: usize, j: usize) -> i32 {
        if self.ids[0][i] == self.ids[1][j] {
            let la = self.raw[0][i].len();
            let lb = self.raw[1][j].len();
            if la == 0 || lb == 0 {
                return -10;
            }
            let mut s = 2 * (self.weights[0][i] + self.weights[1][j]);
            if la != lb {
                s -= 1;
            }
            return s;
        }
        if self.config.use_closeness && !self.config.never_align_diffs {
            if !self.config.ignore_whitespace {
                let a = self.raw[0][i].trim();
                let a = if a.is_empty() {
                    self.raw[0][i].as_bytes()
                } else {
                    a.as_bytes()
                };
                let b = self.raw[1][j].trim();
                let b = if b.is_empty() {
                    self.raw[1][j].as_bytes()
                } else {
                    b.as_bytes()
                };
                closeness_score_bytes(a, b)
            } else {
                let a = build_closeness_key(self.raw[0][i], true);
                let b = build_closeness_key(self.raw[1][j], true);
                closeness_score_bytes(&a, &b)
            }
        } else if self.config.never_align_diffs {
            -5000
        } else {
            -18
        }
    }

    /// BC5 `TBcSmartAlign.AlignGapDP` ():
    /// 1D-score Needleman-Wunsch (gap = -20) with flat byte backtrack table.
    /// When `stop_at_edge` is true (open sliding window without anchor runs),
    /// backtracking stops as soon as either side reaches the window end, returning
    /// the advanced `(next_l, next_r)` so the next window picks up cleanly.
    fn align_gap_dp(
        &mut self,
        l0: usize,
        l1: usize,
        r0: usize,
        r1: usize,
        stop_at_edge: bool,
        out: &mut Vec<(Option<usize>, Option<usize>)>,
    ) -> (usize, usize) {
        let n = l1 - l0;
        let m = r1 - r0;
        if n == 0 {
            for j in r0..r1 {
                out.push((None, Some(j)));
            }
            return (l1, r1);
        }
        if m == 0 {
            for i in l0..l1 {
                out.push((Some(i), None));
            }
            return (l1, r1);
        }
        if self.config.never_align_diffs && !self.config.use_closeness {
            for i in l0..l1 {
                out.push((Some(i), None));
            }
            for j in r0..r1 {
                out.push((None, Some(j)));
            }
            return (l1, r1);
        }
        if n == 1 && m == 1 && !self.config.never_align_diffs {
            if self.line_pair_score(l0, r0) >= -40 {
                out.push((Some(l0), Some(r0)));
            } else {
                out.push((Some(l0), None));
                out.push((None, Some(r0)));
            }
            return (l1, r1);
        }

        let stride = n + 1;
        let mut scores = vec![0i32; stride];
        let mut dir = vec![0u8; (m + 1) * stride];
        for i in 1..=n {
            scores[i] = -(i as i32) * 20;
            dir[i] = 1;
        }
        if self.config.use_closeness && !self.config.never_align_diffs {
            #[derive(Clone, Copy)]
            struct CellMeta {
                id: u32,
                raw_len: usize,
                weight: i32,
                key_len: i32,
                start: u32,
                end: u32,
            }
            for i in l0..l1 {
                self.ensure_hist(0, i);
            }
            for j in r0..r1 {
                self.ensure_hist(1, j);
            }
            let mut left_entries: Vec<(u8, u16)> = Vec::with_capacity(n * 24);
            let mut left_meta: Vec<CellMeta> = Vec::with_capacity(n);
            for i in 1..=n {
                let l_idx = l1 - i;
                let h = self.closeness_cache[0][l_idx].as_ref().unwrap();
                let start = left_entries.len() as u32;
                left_entries.extend_from_slice(&h.entries);
                let end = left_entries.len() as u32;
                left_meta.push(CellMeta {
                    id: self.ids[0][l_idx],
                    raw_len: self.raw[0][l_idx].len(),
                    weight: self.weights[0][l_idx],
                    key_len: h.key_len,
                    start,
                    end,
                });
            }
            let mut right_entries: Vec<(u8, u16)> = Vec::with_capacity(m * 24);
            let mut right_meta: Vec<CellMeta> = Vec::with_capacity(m);
            for j in 1..=m {
                let r_idx = r1 - j;
                let h = self.closeness_cache[1][r_idx].as_ref().unwrap();
                let start = right_entries.len() as u32;
                right_entries.extend_from_slice(&h.entries);
                let end = right_entries.len() as u32;
                right_meta.push(CellMeta {
                    id: self.ids[1][r_idx],
                    raw_len: self.raw[1][r_idx].len(),
                    weight: self.weights[1][r_idx],
                    key_len: h.key_len,
                    start,
                    end,
                });
            }

            #[inline]
            fn score_row_into(
                rm: CellMeta,
                r_slice: &[(u8, u16)],
                left_meta: &[CellMeta],
                left_entries: &[(u8, u16)],
                hist_r: &mut [u16; 256],
                out_row: &mut [i16],
            ) {
                for &(byte, cnt) in r_slice {
                    hist_r[byte as usize] = cnt;
                }
                for (col, lm) in left_meta.iter().enumerate() {
                    let s = if lm.id == rm.id {
                        if lm.raw_len == 0 || rm.raw_len == 0 {
                            -10
                        } else {
                            let mut sc = 2 * (lm.weight + rm.weight);
                            if lm.raw_len != rm.raw_len {
                                sc -= 1;
                            }
                            sc
                        }
                    } else if lm.key_len == 0 || rm.key_len == 0 {
                        (-4 * (lm.key_len + rm.key_len) - 1).clamp(-40, -11)
                    } else {
                        let mut common = 0i32;
                        for &(byte, cnt) in &left_entries[lm.start as usize..lm.end as usize] {
                            common += cnt.min(hist_r[byte as usize]) as i32;
                        }
                        closeness_score_from_common(lm.key_len, rm.key_len, common)
                    };
                    out_row[col] = s as i16;
                }
                for &(byte, _) in r_slice {
                    hist_r[byte as usize] = 0;
                }
            }

            let threads = if n.saturating_mul(m) >= 32_768 {
                std::thread::available_parallelism()
                    .map(|p| p.get())
                    .unwrap_or(4)
                    .clamp(1, 16)
            } else {
                1
            };

            if threads <= 1 {
                let mut hist_r = [0u16; 256];
                let mut row_scores = vec![0i16; n];
                for j in 1..=m {
                    let rm = right_meta[j - 1];
                    let r_slice = &right_entries[rm.start as usize..rm.end as usize];
                    score_row_into(
                        rm,
                        r_slice,
                        &left_meta,
                        &left_entries,
                        &mut hist_r,
                        &mut row_scores,
                    );
                    let dir_row = &mut dir[j * stride..(j + 1) * stride];
                    let mut prev_diag = scores[0];
                    scores[0] -= 20;
                    dir_row[0] = 2;
                    for i in 1..=n {
                        let mut best = prev_diag + row_scores[i - 1] as i32;
                        let mut step = 3u8;
                        let del = scores[i - 1] - 20;
                        if del > best {
                            best = del;
                            step = 1;
                        }
                        let ins = scores[i] - 20;
                        if ins > best {
                            best = ins;
                            step = 2;
                        }
                        prev_diag = scores[i];
                        scores[i] = best;
                        dir_row[i] = step;
                    }
                }
            } else {
                // Parallelize score evaluation across bands of rows while keeping
                // the L1-cached band small (~256 KiB) and exact sequential DP recurrence.
                let band_rows = (threads * 8).clamp(32, 128).min(m);
                let mut band_scores = vec![0i16; band_rows * n];
                let lm_ref = &left_meta;
                let le_ref = &left_entries;
                let rm_ref = &right_meta;
                let re_ref = &right_entries;

                let mut j_base = 0usize;
                while j_base < m {
                    let cur_rows = (m - j_base).min(band_rows);
                    let rows_per_worker = cur_rows.div_ceil(threads).max(1);
                    let active_slice = &mut band_scores[..cur_rows * n];
                    std::thread::scope(|scope| {
                        for (w_idx, chunk) in active_slice.chunks_mut(rows_per_worker * n).enumerate()
                        {
                            let r_start = j_base + w_idx * rows_per_worker;
                            scope.spawn(move || {
                                let mut hist_r = [0u16; 256];
                                for (local_r, out_row) in chunk.chunks_exact_mut(n).enumerate() {
                                    let rm = rm_ref[r_start + local_r];
                                    let r_slice = &re_ref[rm.start as usize..rm.end as usize];
                                    score_row_into(
                                        rm,
                                        r_slice,
                                        lm_ref,
                                        le_ref,
                                        &mut hist_r,
                                        out_row,
                                    );
                                }
                            });
                        }
                    });
                    for bj in 0..cur_rows {
                        let j = j_base + bj + 1;
                        let row_scores = &band_scores[bj * n..(bj + 1) * n];
                        let dir_row = &mut dir[j * stride..(j + 1) * stride];
                        let mut prev_diag = scores[0];
                        scores[0] -= 20;
                        dir_row[0] = 2;
                        for i in 1..=n {
                            let mut best = prev_diag + row_scores[i - 1] as i32;
                            let mut step = 3u8;
                            let del = scores[i - 1] - 20;
                            if del > best {
                                best = del;
                                step = 1;
                            }
                            let ins = scores[i] - 20;
                            if ins > best {
                                best = ins;
                                step = 2;
                            }
                            prev_diag = scores[i];
                            scores[i] = best;
                            dir_row[i] = step;
                        }
                    }
                    j_base += cur_rows;
                }
            }
        } else {
            for j in 1..=m {
                let r_idx = r1 - j;
                let row_off = j * stride;
                let mut prev_diag = scores[0];
                scores[0] -= 20;
                dir[row_off] = 2;
                for i in 1..=n {
                    let l_idx = l1 - i;
                    let mut best = prev_diag + self.line_pair_score(l_idx, r_idx);
                    let mut step = 3u8;
                    let del = scores[i - 1] - 20;
                    if del > best {
                        best = del;
                        step = 1;
                    }
                    let ins = scores[i] - 20;
                    if ins > best {
                        best = ins;
                        step = 2;
                    }
                    prev_diag = scores[i];
                    scores[i] = best;
                    dir[row_off + i] = step;
                }
            }
        }

        let (mut i, mut j) = (n, m);
        while if stop_at_edge {
            i > 0 && j > 0
        } else {
            i > 0 || j > 0
        } {
            match dir[j * stride + i] {
                3 => {
                    out.push((Some(l1 - i), Some(r1 - j)));
                    i -= 1;
                    j -= 1;
                }
                1 => {
                    out.push((Some(l1 - i), None));
                    i -= 1;
                }
                _ => {
                    out.push((None, Some(r1 - j)));
                    j -= 1;
                }
            }
        }
        (l1 - i, r1 - j)
    }

    fn recalc_run_weight(&self, run: &mut AnchorRun) {
        let mut w = 0i32;
        for k in 0..run.count {
            w += self.weights[0][run.src + k];
        }
        run.weight = if w < 500 { 0 } else { w };
    }

    /// BC5 `TBcSmartAlign.SelectRunsAndFillGaps` ():
    /// sorts runs by descending weight, resolves overlaps (discarding crossing
    /// runs and trimming partial overlaps), sorts survivors by position, and
    /// fills gaps with `align_gap_dp`.
    fn select_runs_and_fill_gaps(
        &mut self,
        runs: &mut [AnchorRun],
        span: WindowSpan,
        open_ended: bool,
        out: &mut Vec<(Option<usize>, Option<usize>)>,
    ) -> (usize, usize) {
        let WindowSpan {
            l0,
            r0,
            l1: w_l1,
            r1: w_r1,
        } = span;
        if !runs.is_empty() {
            if !open_ended {
                for run in runs.iter_mut() {
                    if run.src + run.count > w_l1 || run.tgt + run.count > w_r1 {
                        let max_c = w_l1
                            .saturating_sub(run.src)
                            .min(w_r1.saturating_sub(run.tgt));
                        run.count = run.count.min(max_c);
                        self.recalc_run_weight(run);
                    }
                }
            }
            let cmp_weight = |a: &AnchorRun, b: &AnchorRun| {
                b.weight
                    .cmp(&a.weight)
                    .then_with(|| a.tgt.cmp(&b.tgt))
                    .then_with(|| a.src.cmp(&b.src))
            };
            runs.sort_unstable_by(cmp_weight);

            let len = runs.len();
            for i in 0..len {
                if runs[i].weight <= 0 {
                    continue;
                }
                let a = runs[i];
                let a_src_end = a.src + a.count;
                let a_tgt_end = a.tgt + a.count;
                let mut trimmed = false;
                for b in &mut runs[(i + 1)..len] {
                    if b.weight <= 0 {
                        continue;
                    }
                    let b_src_end = b.src + b.count;
                    let b_tgt_end = b.tgt + b.count;
                    // Strictly before or strictly after -> no conflict
                    if (b_src_end <= a.src && b_tgt_end <= a.tgt)
                        || (b.src >= a_src_end && b.tgt >= a_tgt_end)
                    {
                        continue;
                    }
                    // Partial overlap at the start of `a` (`b` starts before `a` on both sides)
                    if b.src < a.src
                        && b.tgt < a.tgt
                        && b_src_end < a_src_end
                        && b_tgt_end < a_tgt_end
                    {
                        b.count = (a.src - b.src).min(a.tgt - b.tgt);
                        self.recalc_run_weight(b);
                        trimmed = true;
                    } else if b.src > a.src
                        && b.tgt > a.tgt
                        && b_src_end > a_src_end
                        && b_tgt_end > a_tgt_end
                    {
                        // Partial overlap at the end of `a` (`b` ends after `a` on both sides)
                        let cut = a_src_end
                            .saturating_sub(b.src)
                            .max(a_tgt_end.saturating_sub(b.tgt));
                        if cut >= b.count {
                            b.weight = -1;
                        } else {
                            b.count -= cut;
                            b.src += cut;
                            b.tgt += cut;
                            self.recalc_run_weight(b);
                            trimmed = true;
                        }
                    } else {
                        b.weight = -1;
                    }
                }
                if trimmed {
                    runs[i + 1..].sort_unstable_by(cmp_weight);
                }
            }

            runs.sort_unstable_by(|a, b| a.tgt.cmp(&b.tgt).then_with(|| a.src.cmp(&b.src)));
        }

        let mut cur_l = l0;
        let mut cur_r = r0;
        let mut accepted = 0usize;
        for run in runs.iter() {
            if run.weight > 0 && run.count > 0 {
                accepted += 1;
                if run.src > cur_l || run.tgt > cur_r {
                    self.align_gap_dp(cur_l, run.src, cur_r, run.tgt, false, out);
                }
                for k in 0..run.count {
                    out.push((Some(run.src + k), Some(run.tgt + k)));
                }
                cur_l = run.src + run.count;
                cur_r = run.tgt + run.count;
            }
        }

        if open_ended && accepted > 0 {
            return (cur_l, cur_r);
        }
        if cur_l < w_l1 || cur_r < w_r1 {
            let stop_edge = open_ended && cur_l < w_l1 && cur_r < w_r1;
            return self.align_gap_dp(cur_l, w_l1, cur_r, w_r1, stop_edge, out);
        }
        (cur_l, cur_r)
    }

    /// BC5 `TBcSmartAlign.AlignWindow` () + `ScanDiagonalRun` ():
    /// expanding-L search inside a `skew_tolerance` window with lazy row allocation
    /// and `EGoodEnough` (weight >= 5000) early exit + extension beyond the window.
    fn smart_align_window(
        &mut self,
        l0: usize,
        r0: usize,
        l1: usize,
        r1: usize,
        out: &mut Vec<(Option<usize>, Option<usize>)>,
    ) -> (usize, usize) {
        let skew = self.config.skew_tolerance.max(1);
        let w_l1 = (l0 + skew).min(l1);
        let w_r1 = (r0 + skew).min(r1);
        let rows = w_l1 - l0;
        let cols = w_r1 - r0;
        let open_ended = w_l1 < l1 || w_r1 < r1;

        let mut visited: Vec<Option<Vec<u8>>> = vec![None; rows];
        let mut runs: Vec<AnchorRun> = Vec::new();

        let ensure_row = |visited: &mut [Option<Vec<u8>>], row_idx: usize| {
            if visited[row_idx].is_none() {
                visited[row_idx] = Some(vec![0u8; cols]);
            }
        };

        let scan_diag = |i0: usize,
                         j0: usize,
                         visited: &mut [Option<Vec<u8>>],
                         runs: &mut Vec<AnchorRun>|
         -> Option<(usize, usize, usize)> {
            // Fast path: non-matching cells never start a run and are never visited
            // twice by the strictly outward L-sweep, so skip `visited` allocation/writes.
            if self.ids[0][i0] != self.ids[1][j0] {
                return None;
            }
            let ri = i0 - l0;
            let cj = j0 - r0;
            if let Some(row) = visited[ri].as_ref() {
                if row[cj] != 0 {
                    return None;
                }
            }
            let mut i = i0;
            let mut j = j0;
            let mut count = 0usize;
            let mut weight = 0i32;
            let lim_l = if open_ended { l1 } else { w_l1 };
            let lim_r = if open_ended { r1 } else { w_r1 };
            while i < lim_l && j < lim_r && self.ids[0][i] == self.ids[1][j] {
                weight += self.weights[0][i];
                count += 1;
                i += 1;
                j += 1;
                if weight >= 5000 {
                    return Some((i0, j0, count));
                }
            }
            for k in 0..count {
                let mi = i0 + k;
                let mj = j0 + k;
                if mi >= w_l1 || mj >= w_r1 {
                    break;
                }
                ensure_row(visited, mi - l0);
                visited[mi - l0].as_mut().unwrap()[mj - r0] = 1;
            }
            if weight > 500 {
                runs.push(AnchorRun {
                    count,
                    weight,
                    src: i0,
                    tgt: j0,
                });
            }
            None
        };

        let mut cur_x = l0;
        let mut cur_y = r0;
        let mut good_enough = scan_diag(cur_x, cur_y, &mut visited, &mut runs);

        while good_enough.is_none() && (cur_x + 1 < w_l1 || cur_y + 1 < w_r1) {
            if cur_y + 1 < w_r1 {
                cur_y += 1;
            }
            if cur_x + 1 < w_l1 {
                cur_x += 1;
            }
            for x in (l0..=cur_x).rev() {
                if let Some(hit) = scan_diag(x, cur_y, &mut visited, &mut runs) {
                    good_enough = Some(hit);
                    break;
                }
            }
            if good_enough.is_some() {
                break;
            }
            if cur_y > r0 {
                for y in (r0..cur_y).rev() {
                    if let Some(hit) = scan_diag(cur_x, y, &mut visited, &mut runs) {
                        good_enough = Some(hit);
                        break;
                    }
                }
            }
        }

        if let Some((g_src, g_tgt, initial_count)) = good_enough {
            // Exact BC5 SEH handler at :
            // 1) Resolve prefix before the GoodEnough block
            if g_src > l0 || g_tgt > r0 {
                self.select_runs_and_fill_gaps(
                    &mut runs,
                    WindowSpan {
                        l0,
                        r0,
                        l1: g_src,
                        r1: g_tgt,
                    },
                    false,
                    out,
                );
            }
            // 2) Extend the GoodEnough block as far as lines keep matching
            let mut end_l = g_src + initial_count;
            let mut end_r = g_tgt + initial_count;
            let lim_l = if open_ended { l1 } else { w_l1 };
            let lim_r = if open_ended { r1 } else { w_r1 };
            while end_l < lim_l && end_r < lim_r && self.ids[0][end_l] == self.ids[1][end_r] {
                end_l += 1;
                end_r += 1;
            }
            for k in 0..(end_l - g_src) {
                out.push((Some(g_src + k), Some(g_tgt + k)));
            }
            return (end_l, end_r);
        }

        self.select_runs_and_fill_gaps(
            &mut runs,
            WindowSpan {
                l0,
                r0,
                l1: w_l1,
                r1: w_r1,
            },
            open_ended,
            out,
        )
    }

    /// BC5 `TBcSmartAlign.AlignRegion` () + `FastScanAndDispatch` ().
    fn smart_align_region(
        &mut self,
        mut l0: usize,
        mut r0: usize,
        l1: usize,
        r1: usize,
        out: &mut Vec<(Option<usize>, Option<usize>)>,
    ) {
        while l0 < l1 && r0 < r1 {
            // Fast-path linear scan over matching lines before allocating any window table
            while l0 < l1 && r0 < r1 && self.ids[0][l0] == self.ids[1][r0] {
                out.push((Some(l0), Some(r0)));
                l0 += 1;
                r0 += 1;
            }
            if l0 >= l1 || r0 >= r1 {
                break;
            }
            let (next_l, next_r) = self.smart_align_window(l0, r0, l1, r1, out);
            debug_assert!(next_l > l0 || next_r > r0);
            l0 = next_l;
            r0 = next_r;
        }
        for i in l0..l1 {
            out.push((Some(i), None));
        }
        for j in r0..r1 {
            out.push((None, Some(j)));
        }
    }

    /// BC5 `TBcPatienceDiffAlign.PatienceRecurse` ():
    /// prefix/suffix trim → unique-line hash matching → Patience Sorting LIS →
    /// recurse on gaps; base case uses `align_gap_dp` (small) or falls back to
    /// `smart_align_region` (large).
    fn patience_recurse(
        &mut self,
        mut l0: usize,
        mut r0: usize,
        mut l1: usize,
        mut r1: usize,
        out: &mut Vec<(Option<usize>, Option<usize>)>,
    ) {
        // 1) Trim matching prefix
        while l0 < l1 && r0 < r1 && self.ids[0][l0] == self.ids[1][r0] {
            out.push((Some(l0), Some(r0)));
            l0 += 1;
            r0 += 1;
        }
        // 2) Trim matching suffix
        let mut suffix = 0usize;
        while l1 > l0 && r1 > r0 && self.ids[0][l1 - 1] == self.ids[1][r1 - 1] {
            l1 -= 1;
            r1 -= 1;
            suffix += 1;
        }

        let len_l = l1 - l0;
        let len_r = r1 - r0;

        if len_l == 0 {
            for j in r0..r1 {
                out.push((None, Some(j)));
            }
        } else if len_r == 0 {
            for i in l0..l1 {
                out.push((Some(i), None));
            }
        } else if len_l == 1 && len_r == 1 {
            // Instant O(1) fast-path for 1-line gaps (zero hash table allocation)
            if self.config.never_align_diffs {
                out.push((Some(l0), None));
                out.push((None, Some(r0)));
            } else if !self.config.use_closeness || self.line_pair_score(l0, r0) >= -40 {
                out.push((Some(l0), Some(r0)));
            } else {
                out.push((Some(l0), None));
                out.push((None, Some(r0)));
            }
        } else {
            // 3) Find lines unique in BOTH left[l0..l1] and right[r0..r1]
            let mut left_unique: FxHashMap<u32, usize> =
                FxHashMap::with_capacity_and_hasher(len_l, Default::default());
            let mut dup_count = 0usize;
            for (i, &id) in self.ids[0].iter().enumerate().take(l1).skip(l0) {
                left_unique
                    .entry(id)
                    .and_modify(|slot| {
                        *slot = usize::MAX;
                        dup_count += 1;
                    })
                    .or_insert(i);
            }

            let mut right_match: Vec<usize> = vec![usize::MAX; len_r];
            let mut right_seen: FxHashMap<u32, usize> =
                FxHashMap::with_capacity_and_hasher(len_r.min(len_l), Default::default());
            let mut unique_matches = 0usize;
            for j in r0..r1 {
                let id = self.ids[1][j];
                if let Some(&l_idx) = left_unique.get(&id) {
                    if l_idx != usize::MAX {
                        if let Some(&prev_j) = right_seen.get(&id) {
                            if prev_j != usize::MAX {
                                right_match[prev_j - r0] = usize::MAX;
                                unique_matches -= 1;
                                right_seen.insert(id, usize::MAX);
                                left_unique.insert(id, usize::MAX);
                            }
                        } else {
                            right_seen.insert(id, j);
                            right_match[j - r0] = l_idx;
                            unique_matches += 1;
                        }
                    }
                }
            }

            // 4) Longest Increasing Subsequence (Patience Sorting, O(K log K) + O(1) tail fast-path)
            if unique_matches > 0 {
                let mut tails_l: Vec<usize> = Vec::with_capacity(unique_matches.min(64));
                let mut tails_r: Vec<usize> = Vec::with_capacity(unique_matches.min(64));
                let mut prev_r: Vec<usize> = vec![usize::MAX; len_r];
                for j_rel in 0..len_r {
                    let l_idx = right_match[j_rel];
                    if l_idx == usize::MAX {
                        continue;
                    }
                    let pos = if tails_l.last().is_some_and(|&last| last < l_idx) {
                        tails_l.len()
                    } else {
                        tails_l.partition_point(|&v| v < l_idx)
                    };
                    if pos > 0 {
                        prev_r[j_rel] = tails_r[pos - 1];
                    }
                    if pos == tails_l.len() {
                        tails_l.push(l_idx);
                        tails_r.push(j_rel);
                    } else {
                        tails_l[pos] = l_idx;
                        tails_r[pos] = j_rel;
                    }
                }

                // Reconstruct LIS anchors in ascending order
                let mut anchors: Vec<(usize, usize)> = Vec::with_capacity(tails_r.len());
                let mut cur = *tails_r.last().unwrap();
                while cur != usize::MAX {
                    anchors.push((right_match[cur], r0 + cur));
                    cur = prev_r[cur];
                }
                anchors.reverse();

                // Recurse on gaps between anchors
                let mut cl = l0;
                let mut cr = r0;
                for (al, ar) in anchors {
                    if cl < al || cr < ar {
                        self.patience_recurse(cl, cr, al, ar, out);
                    }
                    out.push((Some(al), Some(ar)));
                    cl = al + 1;
                    cr = ar + 1;
                }
                if cl < l1 || cr < r1 {
                    self.patience_recurse(cl, cr, l1, r1, out);
                }
            } else if self.config.use_closeness && !self.config.never_align_diffs {
                // Base case (no unique anchors): BC5  lines 296-339
                if dup_count <= 1000
                    && len_l.max(len_r) <= 5000
                    && len_l.saturating_mul(len_r) <= 4_000_000
                {
                    self.align_gap_dp(l0, l1, r0, r1, false, out);
                } else {
                    self.smart_align_region(l0, r0, l1, r1, out);
                }
            } else {
                let paired = if self.config.never_align_diffs {
                    0
                } else {
                    len_l.min(len_r)
                };
                for k in 0..paired {
                    out.push((Some(l0 + k), Some(r0 + k)));
                }
                for i in (l0 + paired)..l1 {
                    out.push((Some(i), None));
                }
                for j in (r0 + paired)..r1 {
                    out.push((None, Some(j)));
                }
            }
        }

        // 5) Emit trimmed matching suffix
        for k in 0..suffix {
            out.push((Some(l1 + k), Some(r1 + k)));
        }
    }
}

/// Align two line slices using the selected BC5 algorithm.
pub(crate) fn align_lines(
    left: &[&str],
    right: &[&str],
    config: AlignConfig,
) -> Vec<(Option<usize>, Option<usize>)> {
    if left.is_empty() && right.is_empty() {
        return Vec::new();
    }
    if left.is_empty() {
        return (0..right.len()).map(|j| (None, Some(j))).collect();
    }
    if right.is_empty() {
        return (0..left.len()).map(|i| (Some(i), None)).collect();
    }

    match config.method {
        AlignMethod::None => {
            let n = left.len().max(right.len());
            let mut out = Vec::with_capacity(n);
            if config.never_align_diffs {
                for i in 0..left.len() {
                    out.push((Some(i), None));
                }
                for j in 0..right.len() {
                    out.push((None, Some(j)));
                }
            } else {
                for i in 0..n {
                    out.push((
                        (i < left.len()).then_some(i),
                        (i < right.len()).then_some(i),
                    ));
                }
            }
            out
        }
        AlignMethod::Alternate => {
            let ops = if !config.ignore_case && !config.ignore_whitespace {
                reference_myers::align_strs(left, right)
            } else {
                let prepared = PreparedLines::new(left, right, config);
                reference_myers::align_u32(&prepared.ids[0], &prepared.ids[1])
            };
            let mut out = Vec::with_capacity(left.len().max(right.len()));
            for (tag, old, new) in ops {
                match tag {
                    DiffTag::Equal => {
                        for k in 0..old.len() {
                            out.push((Some(old.start + k), Some(new.start + k)));
                        }
                    }
                    DiffTag::Delete => {
                        for i in old {
                            out.push((Some(i), None));
                        }
                    }
                    DiffTag::Insert => {
                        for j in new {
                            out.push((None, Some(j)));
                        }
                    }
                    DiffTag::Replace => {
                        let pairs = super::line_pairing::pair_with_options(
                            &left[old.clone()],
                            &right[new.clone()],
                            config.use_closeness,
                            config.never_align_diffs,
                        );
                        for (li, ri) in pairs {
                            out.push((li.map(|i| old.start + i), ri.map(|j| new.start + j)));
                        }
                    }
                }
            }
            out
        }
        AlignMethod::Standard => {
            let mut prepared = PreparedLines::new(left, right, config);
            let mut out = Vec::with_capacity(left.len().max(right.len()));
            prepared.smart_align_region(0, 0, left.len(), right.len(), &mut out);
            out
        }
        AlignMethod::Patience => {
            let mut prepared = PreparedLines::new(left, right, config);
            let mut out = Vec::with_capacity(left.len().max(right.len()));
            prepared.patience_recurse(0, 0, left.len(), right.len(), &mut out);
            out
        }
    }
}
