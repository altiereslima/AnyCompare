//! Safe reimplementation of the bidirectional Myers O(ND) alignment observed in
//! AnyCompare(`DiffUnitInts.TDiff`,  / ).
//! Keeps its diagonal traversal and tie ordering, while interning string lines
//! to dense `u32` IDs (matching `DiffUnitInts`'s 32-bit integer inner loop) and
//! parallelizing independent large divide-and-conquer halves deterministically.
use similar::DiffTag;
use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};
use std::ops::Range;

type Edit = (DiffTag, Range<usize>, Range<usize>);

/// Fast non-cryptographic FxHasher for line interning (zero extra crates).
#[derive(Default)]
pub(super) struct FxHasher(u64);

impl Hasher for FxHasher {
    #[inline]
    fn finish(&self) -> u64 {
        self.0
    }

    #[inline]
    fn write(&mut self, mut bytes: &[u8]) {
        const K: u64 = 0x517cc1b727220a95;
        while bytes.len() >= 8 {
            let word = u64::from_ne_bytes(bytes[..8].try_into().unwrap());
            self.0 = (self.0.rotate_left(5) ^ word).wrapping_mul(K);
            bytes = &bytes[8..];
        }
        if bytes.len() >= 4 {
            let word = u32::from_ne_bytes(bytes[..4].try_into().unwrap()) as u64;
            self.0 = (self.0.rotate_left(5) ^ word).wrapping_mul(K);
            bytes = &bytes[4..];
        }
        for &b in bytes {
            self.0 = (self.0.rotate_left(5) ^ (b as u64)).wrapping_mul(K);
        }
    }
}

pub(super) type FxHashMap<K, V> = HashMap<K, V, BuildHasherDefault<FxHasher>>;

/// Intern two `&str` slices into exact bijection dense `u32` IDs (0..unique_count).
/// Identical strings receive identical IDs with zero collision risk.
pub(super) fn intern_lines<'a>(a: &[&'a str], b: &[&'a str]) -> (Vec<u32>, Vec<u32>, u32) {
    let mut table: FxHashMap<&'a str, u32> =
        FxHashMap::with_capacity_and_hasher(a.len().max(b.len()), Default::default());
    let mut next_id = 0u32;
    let mut intern = |s: &'a str| -> u32 {
        *table.entry(s).or_insert_with(|| {
            let id = next_id;
            next_id += 1;
            id
        })
    };
    let ids_a: Vec<u32> = a.iter().map(|&s| intern(s)).collect();
    let ids_b: Vec<u32> = b.iter().map(|&s| intern(s)).collect();
    (ids_a, ids_b, next_id)
}

/// Fast `&str` alignment: trims identical prefix/suffix first, interns the
/// differing middle to dense `u32` IDs, and runs 32-bit bidirectional Myers.
/// Produces bit-for-bit identical edits and tie ordering to `align(a, b)`.
pub(super) fn align_strs(a: &[&str], b: &[&str]) -> Vec<Edit> {
    if a.is_empty() && b.is_empty() {
        return Vec::new();
    }
    if a.is_empty() {
        return vec![(DiffTag::Insert, 0..0, 0..b.len())];
    }
    if b.is_empty() {
        return vec![(DiffTag::Delete, 0..a.len(), 0..0)];
    }
    let mut prefix = 0;
    let min_len = a.len().min(b.len());
    while prefix < min_len && a[prefix] == b[prefix] {
        prefix += 1;
    }
    if prefix == a.len() && prefix == b.len() {
        return vec![(DiffTag::Equal, 0..a.len(), 0..b.len())];
    }
    let (mut end_a, mut end_b) = (a.len(), b.len());
    while prefix < end_a && prefix < end_b && a[end_a - 1] == b[end_b - 1] {
        end_a -= 1;
        end_b -= 1;
    }
    let mid_a = &a[prefix..end_a];
    let mid_b = &b[prefix..end_b];
    if mid_a.is_empty() || mid_b.is_empty() {
        let mut out = Vec::with_capacity(3);
        if prefix > 0 {
            out.push((DiffTag::Equal, 0..prefix, 0..prefix));
        }
        if mid_a.is_empty() {
            out.push((DiffTag::Insert, prefix..prefix, prefix..end_b));
        } else {
            out.push((DiffTag::Delete, prefix..end_a, prefix..prefix));
        }
        if end_a < a.len() {
            out.push((DiffTag::Equal, end_a..a.len(), end_b..b.len()));
        }
        return out;
    }
    let (ids_a, ids_b, _) = intern_lines(mid_a, mid_b);
    let mid_edits = align_u32_middle(&ids_a, &ids_b);
    let mut out = Vec::with_capacity(mid_edits.len() + 2);
    if prefix > 0 {
        out.push((DiffTag::Equal, 0..prefix, 0..prefix));
    }
    for (tag, old, new) in mid_edits {
        out.push((
            tag,
            old.start + prefix..old.end + prefix,
            new.start + prefix..new.end + prefix,
        ));
    }
    if end_a < a.len() || end_b < b.len() {
        out.push((DiffTag::Equal, end_a..a.len(), end_b..b.len()));
    }
    out
}

/// Public helper for already-interned `u32` sequences (e.g. `TBcQuickAlign`).
pub(super) fn align_u32(a: &[u32], b: &[u32]) -> Vec<Edit> {
    if a == b {
        return if a.is_empty() {
            Vec::new()
        } else {
            vec![(DiffTag::Equal, 0..a.len(), 0..b.len())]
        };
    }
    if a.is_empty() {
        return vec![(DiffTag::Insert, 0..0, 0..b.len())];
    }
    if b.is_empty() {
        return vec![(DiffTag::Delete, 0..a.len(), 0..0)];
    }
    let mut prefix = 0;
    let min_len = a.len().min(b.len());
    while prefix < min_len && a[prefix] == b[prefix] {
        prefix += 1;
    }
    let (mut end_a, mut end_b) = (a.len(), b.len());
    while prefix < end_a && prefix < end_b && a[end_a - 1] == b[end_b - 1] {
        end_a -= 1;
        end_b -= 1;
    }
    let mid_edits = align_u32_middle(&a[prefix..end_a], &b[prefix..end_b]);
    let mut out = Vec::with_capacity(mid_edits.len() + 2);
    if prefix > 0 {
        out.push((DiffTag::Equal, 0..prefix, 0..prefix));
    }
    for (tag, old, new) in mid_edits {
        out.push((
            tag,
            old.start + prefix..old.end + prefix,
            new.start + prefix..new.end + prefix,
        ));
    }
    if end_a < a.len() || end_b < b.len() {
        out.push((DiffTag::Equal, end_a..a.len(), end_b..b.len()));
    }
    out
}

fn align_u32_middle(a: &[u32], b: &[u32]) -> Vec<Edit> {
    let raw_edits = if a.len() + b.len() >= 8192 {
        let max_depth =
            std::thread::available_parallelism().map_or(0, |n| (n.ilog2()).min(3) as u8);
        visit_u32_parallel(a, b, 0, 0, a.len() as i32, b.len() as i32, max_depth)
    } else {
        let mut search = SearchU32::new(a, b);
        search.visit(0, 0, a.len() as i32, b.len() as i32);
        search.edits
    };
    coalesce_edits(raw_edits, a.len(), b.len())
}

struct SearchU32<'a> {
    a: &'a [u32],
    b: &'a [u32],
    forward: Vec<i32>,
    reverse: Vec<i32>,
    offset: isize,
    edits: Vec<Edit>,
}

enum SplitResult {
    Done,
    Recurse {
        lx0: i32,
        ly0: i32,
        lx1: i32,
        ly1: i32,
        rx0: i32,
        ry0: i32,
        rx1: i32,
        ry1: i32,
    },
}

impl<'a> SearchU32<'a> {
    fn new(a: &'a [u32], b: &'a [u32]) -> Self {
        Self::with_span(a, b, a.len(), b.len())
    }

    fn with_span(a: &'a [u32], b: &'a [u32], max_n: usize, max_m: usize) -> Self {
        let offset = (max_n + max_m + 2) as isize;
        let len = offset as usize * 2 + 1;
        Self {
            a,
            b,
            forward: vec![0; len],
            reverse: vec![0; len],
            offset,
            edits: Vec::new(),
        }
    }

    /// Finds the exact middle snake using BC5 `DiffUnitInts.TDiff.RecursiveDiff`
    /// traversal order and tie-breaking.
    #[inline]
    fn find_split(&mut self, x0: i32, y0: i32, x1: i32, y1: i32) -> SplitResult {
        if x0 == x1 || y0 == y1 {
            if x0 != x1 || y0 != y1 {
                self.edits.push((
                    if x0 == x1 {
                        DiffTag::Insert
                    } else {
                        DiffTag::Delete
                    },
                    x0 as usize..x1 as usize,
                    y0 as usize..y1 as usize,
                ));
            }
            return SplitResult::Done;
        }
        let n = x1 - x0;
        let m = y1 - y0;
        let delta = n - m;
        let offset = self.offset;
        let fwd = self.forward.as_mut_slice();
        let rev = self.reverse.as_mut_slice();
        let a = self.a;
        let b = self.b;

        let idx = |k: i32| (offset + k as isize) as usize;
        fwd[idx(0)] = x0;
        rev[idx(delta)] = x1;
        fwd[idx(n)] = x1;
        rev[idx(n)] = x1;
        if delta > 0 {
            fwd[idx(-m - 1)] = x0;
            rev[idx(-m - 1)] = x0;
        }

        let max_d = n.max(m);
        for d in 1..=max_d {
            let mut k = -d;
            if k < -m {
                k += ((-m - k + 1) / 2) * 2;
            }
            let k_max = d.min(n - 1);
            while k <= k_max {
                let ki = idx(k);
                let mut x = if k == -d || (k < d && fwd[ki - 1] < fwd[ki + 1]) {
                    fwd[ki + 1]
                } else {
                    fwd[ki - 1] + 1
                };
                let mut y = x - x0 + y0 - k;
                while x < x1 && y < y1 && a[x as usize] == b[y as usize] {
                    x += 1;
                    y += 1;
                }
                fwd[ki] = x;
                if (delta & 1) != 0 && delta - d < k && k < delta + d && rev[ki] <= x {
                    let (end_x, end_y) = (x, y);
                    while x0 < x && y0 < y && a[(x - 1) as usize] == b[(y - 1) as usize] {
                        x -= 1;
                        y -= 1;
                    }
                    return SplitResult::Recurse {
                        lx0: x0,
                        ly0: y0,
                        lx1: x,
                        ly1: y,
                        rx0: end_x,
                        ry0: end_y,
                        rx1: x1,
                        ry1: y1,
                    };
                }
                k += 2;
            }

            let mut k = delta - d;
            if k < -m {
                k += ((-m - k + 1) / 2) * 2;
            }
            let k_max = (delta + d).min(n - 1);
            while k <= k_max {
                let ki = idx(k);
                let mut x = if k == delta + d || (delta - d < k && rev[ki - 1] < rev[ki + 1]) {
                    rev[ki - 1]
                } else {
                    rev[ki + 1] - 1
                };
                let mut y = x - x0 + y0 - k;
                while x0 < x && y0 < y && a[(x - 1) as usize] == b[(y - 1) as usize] {
                    x -= 1;
                    y -= 1;
                }
                rev[ki] = x;
                if (delta & 1) == 0 && -d <= k && k <= d && x <= fwd[ki] {
                    if n == 1 && m == 1 {
                        self.edits.push((
                            DiffTag::Replace,
                            x0 as usize..x1 as usize,
                            y0 as usize..y1 as usize,
                        ));
                        return SplitResult::Done;
                    }
                    let (lx1, ly1) = (x, y);
                    while x < x1 && y < y1 && a[x as usize] == b[y as usize] {
                        x += 1;
                        y += 1;
                    }
                    return SplitResult::Recurse {
                        lx0: x0,
                        ly0: y0,
                        lx1,
                        ly1,
                        rx0: x,
                        ry0: y,
                        rx1: x1,
                        ry1: y1,
                    };
                }
                k += 2;
            }
        }
        unreachable!("bidirectional edit search must overlap");
    }

    fn visit(&mut self, x0: i32, y0: i32, x1: i32, y1: i32) {
        match self.find_split(x0, y0, x1, y1) {
            SplitResult::Done => {}
            SplitResult::Recurse {
                lx0,
                ly0,
                lx1,
                ly1,
                rx0,
                ry0,
                rx1,
                ry1,
            } => {
                self.visit(lx0, ly0, lx1, ly1);
                self.visit(rx0, ry0, rx1, ry1);
            }
        }
    }
}

/// Deterministic parallel divide-and-conquer wrapper over `SearchU32`: splits
/// only when both halves are large and `depth > 0`, concatenating in exact order.
fn visit_u32_parallel(
    a: &[u32],
    b: &[u32],
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
    depth: u8,
) -> Vec<Edit> {
    let n = (x1 - x0) as usize;
    let m = (y1 - y0) as usize;
    let mut search = SearchU32::with_span(a, b, n, m);
    if depth == 0 || n + m < 4096 {
        search.visit(x0, y0, x1, y1);
        return search.edits;
    }
    match search.find_split(x0, y0, x1, y1) {
        SplitResult::Done => search.edits,
        SplitResult::Recurse {
            lx0,
            ly0,
            lx1,
            ly1,
            rx0,
            ry0,
            rx1,
            ry1,
        } => {
            let left_work = (lx1 - lx0 + ly1 - ly0) as usize;
            let right_work = (rx1 - rx0 + ry1 - ry0) as usize;
            if left_work >= 2048 && right_work >= 2048 {
                let (mut left_edits, right_edits) = std::thread::scope(|s| {
                    let right_handle =
                        s.spawn(|| visit_u32_parallel(a, b, rx0, ry0, rx1, ry1, depth - 1));
                    let left_edits = visit_u32_parallel(a, b, lx0, ly0, lx1, ly1, depth - 1);
                    (left_edits, right_handle.join().unwrap())
                });
                left_edits.extend(right_edits);
                left_edits
            } else {
                search.visit(lx0, ly0, lx1, ly1);
                search.visit(rx0, ry0, rx1, ry1);
                search.edits
            }
        }
    }
}

fn coalesce_edits(edits: Vec<Edit>, len_a: usize, len_b: usize) -> Vec<Edit> {
    let mut out: Vec<Edit> = Vec::with_capacity(edits.len() + 2);
    let (mut x, mut y) = (0, 0);
    for (_, old, new) in edits {
        if x < old.start || y < new.start {
            out.push((DiffTag::Equal, x..old.start, y..new.start));
        }
        let tag = if old.is_empty() {
            DiffTag::Insert
        } else if new.is_empty() {
            DiffTag::Delete
        } else {
            DiffTag::Replace
        };
        if let Some(last) = out.last_mut().filter(|op| op.0 != DiffTag::Equal) {
            last.1.end = old.end;
            last.2.end = new.end;
            if !last.1.is_empty() && !last.2.is_empty() {
                last.0 = DiffTag::Replace;
            }
        } else {
            out.push((tag, old.clone(), new.clone()));
        }
        x = old.end;
        y = new.end;
    }
    if x < len_a || y < len_b {
        out.push((DiffTag::Equal, x..len_a, y..len_b));
    }
    out
}

pub(super) fn align<T: Eq>(a: &[T], b: &[T]) -> Vec<Edit> {
    if a == b {
        return if a.is_empty() {
            Vec::new()
        } else {
            vec![(DiffTag::Equal, 0..a.len(), 0..b.len())]
        };
    }
    if a.is_empty() {
        return vec![(DiffTag::Insert, 0..0, 0..b.len())];
    }
    if b.is_empty() {
        return vec![(DiffTag::Delete, 0..a.len(), 0..0)];
    }
    struct Search<'a, T> {
        a: &'a [T],
        b: &'a [T],
        forward: Vec<isize>,
        reverse: Vec<isize>,
        offset: isize,
        edits: Vec<Edit>,
    }
    impl<T: Eq> Search<'_, T> {
        fn visit(&mut self, x0: isize, y0: isize, x1: isize, y1: isize) {
            if x0 == x1 || y0 == y1 {
                if x0 != x1 || y0 != y1 {
                    self.edits.push((
                        if x0 == x1 {
                            DiffTag::Insert
                        } else {
                            DiffTag::Delete
                        },
                        x0 as usize..x1 as usize,
                        y0 as usize..y1 as usize,
                    ));
                }
                return;
            }
            let n = x1 - x0;
            let m = y1 - y0;
            let delta = n - m;
            let index = |k: isize| (self.offset + k) as usize;
            self.forward[index(0)] = x0;
            self.reverse[index(delta)] = x1;
            self.forward[index(n)] = x1;
            self.reverse[index(n)] = x1;
            if delta > 0 {
                self.forward[index(-m - 1)] = x0;
                self.reverse[index(-m - 1)] = x0;
            }
            for d in 1..=n.max(m) {
                let mut k = -d;
                while k < -m {
                    k += 2;
                }
                while k <= d.min(n - 1) {
                    let mut x = if k == -d
                        || k < d && self.forward[index(k - 1)] < self.forward[index(k + 1)]
                    {
                        self.forward[index(k + 1)]
                    } else {
                        self.forward[index(k - 1)] + 1
                    };
                    let mut y = x - x0 + y0 - k;
                    while x < x1 && y < y1 && self.a[x as usize] == self.b[y as usize] {
                        x += 1;
                        y += 1;
                    }
                    self.forward[index(k)] = x;
                    if delta & 1 != 0
                        && delta - d < k
                        && k < delta + d
                        && self.reverse[index(k)] <= x
                    {
                        let (end_x, end_y) = (x, y);
                        while x0 < x
                            && y0 < y
                            && self.a[(x - 1) as usize] == self.b[(y - 1) as usize]
                        {
                            x -= 1;
                            y -= 1;
                        }
                        self.visit(x0, y0, x, y);
                        self.visit(end_x, end_y, x1, y1);
                        return;
                    }
                    k += 2;
                }
                let mut k = delta - d;
                while k < -m {
                    k += 2;
                }
                while k <= (delta + d).min(n - 1) {
                    let mut x = if k == delta + d
                        || delta - d < k && self.reverse[index(k - 1)] < self.reverse[index(k + 1)]
                    {
                        self.reverse[index(k - 1)]
                    } else {
                        self.reverse[index(k + 1)] - 1
                    };
                    let mut y = x - x0 + y0 - k;
                    while x0 < x && y0 < y && self.a[(x - 1) as usize] == self.b[(y - 1) as usize] {
                        x -= 1;
                        y -= 1;
                    }
                    self.reverse[index(k)] = x;
                    if delta & 1 == 0 && -d <= k && k <= d && x <= self.forward[index(k)] {
                        if n == 1 && m == 1 {
                            self.edits.push((
                                DiffTag::Replace,
                                x0 as usize..x1 as usize,
                                y0 as usize..y1 as usize,
                            ));
                            return;
                        }
                        self.visit(x0, y0, x, y);
                        while x < x1 && y < y1 && self.a[x as usize] == self.b[y as usize] {
                            x += 1;
                            y += 1;
                        }
                        self.visit(x, y, x1, y1);
                        return;
                    }
                    k += 2;
                }
            }
            unreachable!("bidirectional edit search must overlap");
        }
    }
    let offset = (a.len() + b.len() + 2) as isize;
    let mut search = Search {
        a,
        b,
        offset,
        forward: vec![0; offset as usize * 2 + 1],
        reverse: vec![0; offset as usize * 2 + 1],
        edits: Vec::new(),
    };
    let mut prefix = 0;
    while prefix < a.len() && prefix < b.len() && a[prefix] == b[prefix] {
        prefix += 1;
    }
    let (mut end_a, mut end_b) = (a.len(), b.len());
    while prefix < end_a && prefix < end_b && a[end_a - 1] == b[end_b - 1] {
        end_a -= 1;
        end_b -= 1;
    }
    search.visit(
        prefix as isize,
        prefix as isize,
        end_a as isize,
        end_b as isize,
    );
    coalesce_edits(search.edits, a.len(), b.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn align_strs_matches_generic_align_exactly() {
        let left: Vec<String> = (0..120)
            .map(|i| match i % 5 {
                0 => String::new(),
                1 => format!("EN=line {i}"),
                2 => "PT=".into(),
                _ => format!("[ID {:04}]", i % 17),
            })
            .collect();
        let right: Vec<String> = (0..125)
            .map(|i| match i % 5 {
                0 => String::new(),
                1 => format!("EN=line {i}"),
                2 => format!("PT=traducao {i}"),
                _ => format!("[ID {:04}]", (i + 1) % 17),
            })
            .collect();
        let lref: Vec<&str> = left.iter().map(String::as_str).collect();
        let rref: Vec<&str> = right.iter().map(String::as_str).collect();
        assert_eq!(align_strs(&lref, &rref), align(&lref, &rref));
    }

    #[test]
    fn alignment_preserves_sequences_and_minimal_edit_cost_for_repeated_symbols() {
        let mut samples = vec![Vec::<u8>::new()];
        for length in 1..=4 {
            for value in 0..3usize.pow(length) {
                let mut value = value;
                samples.push(
                    (0..length)
                        .map(|_| {
                            let c = (value % 3) as u8;
                            value /= 3;
                            c
                        })
                        .collect(),
                );
            }
        }
        for a in &samples {
            for b in &samples {
                let mut longest = vec![vec![0usize; b.len() + 1]; a.len() + 1];
                for i in 0..a.len() {
                    for j in 0..b.len() {
                        longest[i + 1][j + 1] = if a[i] == b[j] {
                            longest[i][j] + 1
                        } else {
                            longest[i][j + 1].max(longest[i + 1][j])
                        };
                    }
                }
                let (mut x, mut y, mut cost) = (0, 0, 0);
                for (tag, old, new) in align(a, b) {
                    assert_eq!((old.start, new.start), (x, y), "{a:?}/{b:?}");
                    if tag == DiffTag::Equal {
                        assert_eq!(a[old.clone()], b[new.clone()]);
                    } else {
                        cost += old.len() + new.len();
                    }
                    x = old.end;
                    y = new.end;
                }
                assert_eq!((x, y), (a.len(), b.len()));
                assert_eq!(
                    cost,
                    a.len() + b.len() - 2 * longest[a.len()][b.len()],
                    "{a:?}/{b:?}"
                );
            }
        }
    }
}
