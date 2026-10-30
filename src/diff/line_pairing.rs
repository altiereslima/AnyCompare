//! Closeness pairing for ranges between exact line anchors.
//! Native evidence: BcStandardAlign-unmatched-range, -closeness-score and
//! -common-length in docs/reference/ghidra-session. Gap cost is -20; the score
//! counts shared UTF-8 bytes of significant text, rather than an LCS ratio.
pub(super) fn pair(left: &[&str], right: &[&str]) -> Vec<(Option<usize>, Option<usize>)> {
    let (n, m) = (left.len(), right.len());
    if n == 1 && m == 1 || n.saturating_mul(m) > 1_000_000 {
        return (0..n.max(m))
            .map(|i| ((i < n).then_some(i), (i < m).then_some(i)))
            .collect();
    }
    let key = |s: &&str| s.chars().filter(|c| !c.is_whitespace()).collect::<String>();
    let a: Vec<_> = left.iter().map(key).collect();
    let b: Vec<_> = right.iter().map(key).collect();
    fn score(a: &str, b: &str) -> i32 {
        let (n, m) = (a.len() as i32, b.len() as i32);
        if a == b {
            return if n == 0 { -10 } else { n * 4 };
        }
        if n == 0 || m == 0 {
            return (-(n + m) * 4 - 1).clamp(-40, -11);
        }
        let mut counts = [0i32; 256];
        for c in a.bytes() {
            counts[c as usize] += 1;
        }
        let mut common = 0;
        for c in b.bytes() {
            counts[c as usize] -= 1;
            if counts[c as usize] >= 0 {
                common += 1;
            }
        }
        let value = common * 3 - n - m;
        if value < 1 {
            (value * 4 - 1).max(-40)
        } else {
            value * 25 / n.min(m)
        }
    }
    // Traverse from the end, as the native routine does. Strict comparisons
    // retain diagonal pairing on equal scores, then deletion before insertion.
    let width = m + 1;
    let mut scores = vec![0i32; (n + 1) * width];
    let mut direction = vec![0u8; scores.len()];
    for i in 1..=n {
        scores[i * width] = -(i as i32) * 20;
        direction[i * width] = 1;
    }
    for j in 1..=m {
        scores[j] = -(j as i32) * 20;
        direction[j] = 2;
    }
    for j in 1..=m {
        for i in 1..=n {
            let mut best = scores[(i - 1) * width + j - 1] + score(&a[n - i], &b[m - j]);
            let mut step = 3;
            if best < scores[(i - 1) * width + j] - 20 {
                best = scores[(i - 1) * width + j] - 20;
                step = 1;
            }
            if best < scores[i * width + j - 1] - 20 {
                best = scores[i * width + j - 1] - 20;
                step = 2;
            }
            scores[i * width + j] = best;
            direction[i * width + j] = step;
        }
    }
    let (mut i, mut j) = (n, m);
    let mut out = Vec::new();
    while i > 0 || j > 0 {
        match direction[i * width + j] {
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
