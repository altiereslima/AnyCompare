"""Decode the original's Hex text reports, retaining physical byte pairings."""
import hashlib
import json
from pathlib import Path
import re
import sys

folder = Path(sys.argv[1]) if len(sys.argv)>1 else Path(__file__).resolve().parents[1] / 'docs/reference/controlled-hex-replay'
manifest = json.loads((folder / 'inputs.json').read_text())
cases = []
for case in manifest:
    name = case['name']
    left = (folder / f'{name}-left.bin').read_bytes()
    right = (folder / f'{name}-right.bin').read_bytes()
    assert hashlib.sha256(left).hexdigest() == case['left_sha256']
    assert hashlib.sha256(right).hexdigest() == case['right_sha256']
    summary = (folder / f'{name}.txt').read_text(encoding='utf-8-sig')
    counts = {}
    for label in ['same', 'difference', 'left orphan', 'right orphan']:
        match = re.search(r'(\d+) ' + label + r' byte', summary)
        counts[label.replace(' ', '_')] = int(match[1]) if match else 0
    pairs = []
    report = (folder / f'{name}-side-by-side.txt').read_text(encoding='cp1252')
    for line in report.splitlines():
        addresses = list(re.finditer(r'(?<![0-9A-F])([0-9A-F]{8}) (?=[0-9A-F ]{23} )', line))
        if not addresses:
            continue
        chunks = [None, None]
        for address in addresses:
            side = 0 if address.start() == 0 else 1
            start = address.end()
            values = [line[start + 3*i:start + 3*i + 2].strip() for i in range(8)]
            chunks[side] = (int(address[1], 16), values)
        cursors = [chunk[0] if chunk else 0 for chunk in chunks]
        for i in range(8):
            pair = []
            for side, chunk in enumerate(chunks):
                value = chunk[1][i] if chunk else ''
                if value:
                    data = left if side == 0 else right
                    assert data[cursors[side]] == int(value, 16)
                    pair.append(cursors[side])
                    cursors[side] += 1
                else:
                    pair.append(None)
            if pair != [None, None]:
                pairs.append(pair)
    assert [a for a, _ in pairs if a is not None] == list(range(len(left)))
    assert [b for _, b in pairs if b is not None] == list(range(len(right)))
    actual = dict(same=0, difference=0, left_orphan=0, right_orphan=0)
    for a, b in pairs:
        kind = 'right_orphan' if a is None else 'left_orphan' if b is None else 'same' if left[a] == right[b] else 'difference'
        actual[kind] += 1
    assert actual == counts, (name, actual, counts)
    cases.append(dict(**case, counts=counts, pairs=pairs))
(folder / 'expected.json').write_text(json.dumps(cases, indent=2) + '\n', encoding='utf-8')
print(json.dumps([dict(name=case['name'], pairs=len(case['pairs']), **case['counts']) for case in cases], indent=2))
