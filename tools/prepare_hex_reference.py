"""Prepare deterministic Hex reports in an isolated portable BC4 copy."""
import hashlib
import json
from pathlib import Path
import shutil
import sys

ROOT = Path(__file__).resolve().parents[1]
audit = '--audit' in sys.argv
RUNTIME = ROOT / ('.codex/reference-hex-audit-runtime' if audit else '.codex/reference-hex-runtime')
OUTPUT = ROOT / ('docs/reference/controlled-hex-audit-2026-10-30' if audit else 'docs/reference/controlled-hex-replay')
RUNTIME.mkdir(parents=True, exist_ok=True)
OUTPUT.mkdir(parents=True, exist_ok=True)
for name in ['BCompare.exe', '7z.dll', 'BCUnRAR.dll', 'mscoree.dll']:
    source = ROOT / '.codex/reference-reverse-runtime' / name
    target = RUNTIME / name
    if not target.exists():
        shutil.copyfile(source, target)
assert hashlib.sha256((RUNTIME / 'BCompare.exe').read_bytes()).hexdigest() == 'aba1b98f848a01c3729cfee3e8818ab8e0d947276bc42c9a55c1475c9f8438db'
if not (RUNTIME / 'BCState.xml').exists():
    (RUNTIME / 'BCState.xml').write_bytes(b'')
base = bytes(range(64))
changed = bytearray(base)
changed[3] = 200
changed[35] = 250
cases = [('equal', base, base), ('replacement', base, bytes(changed)),
         ('inserted', base, base[:17] + b'\xFF' + base[17:]),
         ('shorter', base, base[:35]), ('empty', b'', base)]
if audit:
    cases += [
        ('duplicate-anchor', b'AB', b'BB'),
        ('repeat-insert', b'A'*24, b'A'*25),
        ('alternating', b'AB'*16, b'BA'*16),
        ('zero-padding', b'HEAD'+bytes(64)+b'TAIL', b'HEAD'+bytes(65)+b'TAIL'),
        ('moved-block', b'12345678ABCDEFGH', b'ABCDEFGH12345678'),
        ('replacement-run', b'HEADabcdefghTAIL', b'HEADABCDEFGHTAIL'),
        ('repeated-delete', b'AAABAAABAAAB', b'AAABAAAB'),
        ('prefix-insert', base, b'XYZ'+base),
        ('two-inserts', base, base[:8]+b'XY'+base[8:40]+b'Z'+base[40:]),
        ('equal-blocks', b'abcd1234abcd5678', b'abcd5678abcd1234'),
    ]
    cases += [(name+'-reverse', b, a) for name, a, b in list(cases)]
commands = [f'log verbose "{OUTPUT / "runtime.log"}"']
manifest = []
for name, left, right in cases:
    a, b = OUTPUT / f'{name}-left.bin', OUTPUT / f'{name}-right.bin'
    a.write_bytes(left)
    b.write_bytes(right)
    for layout, suffix, options in [('summary', '.txt', ''), ('side-by-side', '-side-by-side.txt', ' options:display-all,line-numbers')]:
        commands.append(f'hex-report layout:{layout}{options} output-to:"{OUTPUT / (name + suffix)}" "{a}" "{b}"')
    manifest.append(dict(name=name, left_sha256=hashlib.sha256(left).hexdigest(),
                         right_sha256=hashlib.sha256(right).hexdigest(),
                         left_bytes=len(left), right_bytes=len(right)))
(OUTPUT / 'report-script.txt').write_text('\n'.join(commands) + '\n', encoding='utf-8')
(OUTPUT / 'inputs.json').write_text(json.dumps(manifest, indent=2) + '\n', encoding='utf-8')
print(json.dumps(dict(program=str(RUNTIME / 'BCompare.exe'), script=str(OUTPUT / 'report-script.txt'), cases=len(cases))))
