//! Safe reimplementation of the bidirectional alignment observed in BC4's
//! 32-bit sequence comparator. Keep its diagonal traversal and tie ordering.
//! Evidence: docs/reference/ghidra-session/native-myers32-middle-snake.json.
use similar::DiffTag;
use std::ops::Range;

type Edit = (DiffTag, Range<usize>, Range<usize>);

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
    let mut out: Vec<Edit> = Vec::new();
    let (mut x, mut y) = (0, 0);
    for (_, old, new) in search.edits {
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
    if x < a.len() || y < b.len() {
        out.push((DiffTag::Equal, x..a.len(), y..b.len()));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

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
