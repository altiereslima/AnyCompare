use super::{reference_myers, InlineChunk, InlineChunkKind};

// Default plain-text character pipeline recovered through REA/Ghidra and
// controlled runtime traces. Grammar-specific segmentation remains separate.
pub(super) fn compare(left: &str, right: &str) -> (Vec<InlineChunk>, Vec<InlineChunk>) {
    let a: Vec<char> = left.chars().collect();
    let b: Vec<char> = right.chars().collect();
    let key = |c: &char| {
        let mut upper = c.to_uppercase();
        (upper.next().unwrap(), upper.next(), upper.next())
    };
    let ka: Vec<_> = a.iter().map(key).collect();
    let kb: Vec<_> = b.iter().map(key).collect();
    let initial = |chars: &[char]| {
        chars
            .iter()
            .map(|c| if c.is_whitespace() { 2u8 } else { 4u8 })
            .collect::<Vec<_>>()
    };
    let mut kinds = [initial(&a), initial(&b)];
    let mut mapping = vec![None; a.len()];
    // The native prefix scan skips minor whitespace before the first important
    // mismatch and anchors equal significant characters. Then Myers fills each
    // unanchored gap. This is observable in "Lich em" / "Liches em": the first
    // e is anchored across the skipped space, rather than realigning the prefix.
    let (mut x, mut y) = (0, 0);
    while x < a.len() && y < b.len() {
        if a[x].is_whitespace() {
            x += 1;
        } else if b[y].is_whitespace() {
            y += 1;
        } else if ka[x] == kb[y] {
            mapping[x] = Some(y);
            x += 1;
            y += 1;
        } else {
            break;
        }
    }
    let mut start = (0, 0);
    let anchors: Vec<_> = mapping
        .iter()
        .enumerate()
        .filter_map(|(x, y)| y.map(|y| (x, y)))
        .chain(std::iter::once((a.len(), b.len())))
        .collect();
    for (end_x, end_y) in anchors {
        for (tag, old, new) in reference_myers::align(&ka[start.0..end_x], &kb[start.1..end_y]) {
            if tag == similar::DiffTag::Equal {
                for (x, y) in old.zip(new) {
                    mapping[start.0 + x] = Some(start.1 + y);
                }
            }
        }
        start = (end_x + 1, end_y + 1);
    }
    for (x, y) in mapping
        .iter()
        .enumerate()
        .filter_map(|(x, y)| y.map(|y| (x, y)))
    {
        let kind = if a[x] == b[y] { 1 } else { 2 };
        kinds[0][x] = kind;
        kinds[1][y] = kind;
    }
    // Native +0x10 records identify contiguous grammar segments. With the
    // plain-text format these are alternating whitespace/non-whitespace runs.
    // An isolated matched character takes the shared kind of its neighbors
    // only inside that segment, and propagates to its aligned counterpart.
    fn absorb(chars: &[char], kinds: &mut [u8], i: usize) -> bool {
        if i == 0 || i + 1 >= chars.len() {
            return false;
        }
        let same_segment = chars[i - 1].is_whitespace() == chars[i].is_whitespace()
            && chars[i].is_whitespace() == chars[i + 1].is_whitespace();
        if matches!(kinds[i], 1 | 2)
            && same_segment
            && kinds[i - 1] == kinds[i + 1]
            && matches!(kinds[i - 1], 2 | 4)
            && kinds[i] != kinds[i - 1]
        {
            kinds[i] = kinds[i - 1];
            true
        } else {
            false
        }
    }
    for x in 1..a.len().saturating_sub(1) {
        if absorb(&a, &mut kinds[0], x) {
            if let Some(y) = mapping[x] {
                kinds[1][y] = kinds[0][x];
            }
        } else if let Some(y) = mapping[x] {
            if absorb(&b, &mut kinds[1], y) {
                kinds[0][x] = kinds[1][y];
            }
        }
    }
    fn chunks(chars: Vec<char>, kinds: Vec<u8>) -> Vec<InlineChunk> {
        let mut chunks: Vec<InlineChunk> = Vec::new();
        for (c, kind) in chars.into_iter().zip(kinds) {
            let kind = match kind {
                1 => InlineChunkKind::Same,
                2 => InlineChunkKind::Unimportant,
                _ => InlineChunkKind::Diff,
            };
            if let Some(last) = chunks.last_mut().filter(|last| last.kind == kind) {
                last.text.push(c);
            } else {
                chunks.push(InlineChunk {
                    text: c.to_string(),
                    kind,
                });
            }
        }
        chunks
    }
    let [left_kinds, right_kinds] = kinds;
    (chunks(a, left_kinds), chunks(b, right_kinds))
}
