//! Closeness pairing for ranges between exact line anchors.
//! Matches BC5 `TBcSmartAlign.AlignGapDP` (), `LinePairScore`
//! (), and `CountCommonCharsHist` ():
//! - 1D `scores` vector (L1-cache friendly) + flat byte backtrack table
//! - gap penalty `-20` (unaligning a pair costs `-40`)
//! - O(lenA + lenB) 256-byte stack histogram similarity (no O(len^2) Levenshtein)

#[inline]
pub(super) fn build_compact_hist(bytes: &[u8]) -> Vec<(u8, u16)> {
    let mut counts = [0u16; 256];
    let mut used = [0u8; 256];
    let mut used_len = 0usize;
    for &c in bytes {
        let slot = &mut counts[c as usize];
        if *slot == 0 {
            used[used_len] = c;
            used_len += 1;
        }
        if *slot < i16::MAX as u16 {
            *slot += 1;
        }
    }
    let mut out = Vec::with_capacity(used_len);
    for &c in &used[..used_len] {
        out.push((c, counts[c as usize]));
    }
    out
}

#[inline]
pub(super) fn closeness_score_from_common(n: i32, m: i32, common: i32) -> i32 {
    if n == 0 || m == 0 {
        return (-(n + m) * 4 - 1).clamp(-40, -11);
    }
    let value = common * 3 - n - m;
    if value <= 0 {
        (value * 4 - 1).max(-40)
    } else {
        (value * 25) / n.min(m)
    }
}

#[inline]
pub(super) fn count_common_bytes(a: &[u8], b: &[u8]) -> i32 {
    let mut counts = [0i16; 256];
    for &c in a {
        counts[c as usize] = counts[c as usize].saturating_add(1);
    }
    let mut common = 0i32;
    for &c in b {
        let slot = &mut counts[c as usize];
        *slot -= 1;
        if *slot >= 0 {
            common += 1;
        }
    }
    common
}

/// BC5 `LinePairScore` () closeness branch for two differing lines.
#[inline]
pub(super) fn closeness_score_bytes(a: &[u8], b: &[u8]) -> i32 {
    let (n, m) = (a.len() as i32, b.len() as i32);
    if n == 0 || m == 0 {
        return (-(n + m) * 4 - 1).clamp(-40, -11);
    }
    let common = count_common_bytes(a, b);
    closeness_score_from_common(n, m, common)
}

/// Pair an unmatched replace block using BC5's 1D-score Needleman-Wunsch DP
/// with closeness matching. Preserves exact BC4/BC5 tie-breaking:
/// diagonal (`3`) > delete (`1`) > insert (`2`), traversing from the end.
pub(super) fn pair_with_options(
    left: &[&str],
    right: &[&str],
    use_closeness: bool,
    never_align_diffs: bool,
) -> Vec<(Option<usize>, Option<usize>)> {
    let (n, m) = (left.len(), right.len());
    if never_align_diffs {
        let mut out = Vec::with_capacity(n + m);
        for i in 0..n {
            out.push((Some(i), None));
        }
        for j in 0..m {
            out.push((None, Some(j)));
        }
        return out;
    }
    if !use_closeness || (n == 1 && m == 1) || n.saturating_mul(m) > 4_000_000 {
        return (0..n.max(m))
            .map(|i| ((i < n).then_some(i), (i < m).then_some(i)))
            .collect();
    }
    fn key(s: &str) -> &[u8] {
        let trimmed = s.trim();
        if trimmed.is_empty() {
            s.as_bytes()
        } else {
            trimmed.as_bytes()
        }
    }
    let a: Vec<&[u8]> = left.iter().map(|s| key(s)).collect();
    let b: Vec<&[u8]> = right.iter().map(|s| key(s)).collect();

    // Precompute compact distinct-byte histograms in reverse left order (`n - i`)
    // so the inner DP loop streams sequentially through L1 cache with zero memset.
    let mut a_entries: Vec<(u8, u16)> = Vec::with_capacity(n * 24);
    let mut a_ranges: Vec<(u32, u32)> = Vec::with_capacity(n);
    for i in 1..=n {
        let start = a_entries.len() as u32;
        a_entries.extend(build_compact_hist(a[n - i]));
        a_ranges.push((start, a_entries.len() as u32));
    }

    // 1D score row indexed by `i` (0..=n), matching BC5 `TBcSmartAlign.AlignGapDP`
    // (). Direction matrix is stored row-major by `j` so the inner
    // `i = 1..=n` loop does sequential stride-1 writes in L1 cache.
    let stride = n + 1;
    let mut scores = vec![0i32; stride];
    let mut direction = vec![0u8; (m + 1) * stride];
    for i in 1..=n {
        scores[i] = -(i as i32) * 20;
        direction[i] = 1;
    }
    let mut hist_b = [0u16; 256];
    let mut used_b = [0u8; 256];
    for j in 1..=m {
        let bj = b[m - j];
        let bj_len = bj.len() as i32;
        let mut used_len = 0usize;
        for &byte in bj {
            let slot = &mut hist_b[byte as usize];
            if *slot == 0 {
                used_b[used_len] = byte;
                used_len += 1;
            }
            if *slot < i16::MAX as u16 {
                *slot += 1;
            }
        }
        let dir_row = &mut direction[j * stride..(j + 1) * stride];
        let mut prev_diag = scores[0];
        scores[0] -= 20;
        dir_row[0] = 2;
        for i in 1..=n {
            let ai = a[n - i];
            let pair_score = if ai == bj {
                if ai.is_empty() {
                    -10
                } else {
                    (ai.len() as i32) * 4
                }
            } else if ai.is_empty() || bj.is_empty() {
                (-(ai.len() as i32 + bj_len) * 4 - 1).clamp(-40, -11)
            } else {
                let (s, e) = a_ranges[i - 1];
                let mut common = 0i32;
                for &(byte, cnt) in &a_entries[s as usize..e as usize] {
                    common += cnt.min(hist_b[byte as usize]) as i32;
                }
                closeness_score_from_common(ai.len() as i32, bj_len, common)
            };
            let mut best = prev_diag + pair_score;
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
        for &byte in &used_b[..used_len] {
            hist_b[byte as usize] = 0;
        }
    }

    let (mut i, mut j) = (n, m);
    let mut out = Vec::with_capacity(n.max(m));
    while i > 0 || j > 0 {
        match direction[j * stride + i] {
            3 => {
                out.push((Some(n - i), Some(m - j)));
                i -= 1;
                j -= 1;
            }
            1 => {
                out.push((Some(n - i), None));
                i -= 1;
            }
            _ => {
                out.push((None, Some(m - j)));
                j -= 1;
            }
        }
    }
    out
}
