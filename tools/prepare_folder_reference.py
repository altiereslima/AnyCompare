"""Prepare owned, immutable-input folder comparison cases for native BC4 replay."""
from pathlib import Path
import hashlib
import json
import os
import shutil

ROOT = Path(__file__).resolve().parents[1]
RUNTIME = ROOT / '.codex/reference-folder-runtime'
OUTPUT = ROOT / 'docs/reference/controlled-folder-replay-2026-10-30'
RUNTIME.mkdir(exist_ok=True)
OUTPUT.mkdir(parents=True, exist_ok=True)
for name in ['BCompare.exe', '7z.dll', 'BCUnRAR.dll', 'mscoree.dll']:
    source = Path('C:/Projetos/BC4/bc4') / name
    target = RUNTIME / name
    if not target.exists():
        shutil.copyfile(source, target)
    assert hashlib.sha256(target.read_bytes()).digest() == hashlib.sha256(source.read_bytes()).digest()
assert hashlib.sha256((RUNTIME/'BCompare.exe').read_bytes()).hexdigest() == 'aba1b98f848a01c3729cfee3e8818ab8e0d947276bc42c9a55c1475c9f8438db'
if not (RUNTIME/'BCState.xml').exists():
    (RUNTIME/'BCState.xml').write_bytes(b'')
cases = {
    'basic': ({'same.txt': b'equal', 'changed.txt': b'left', 'left.txt': b'left'},
              {'same.txt': b'equal', 'changed.txt': b'diff', 'right.txt': b'right'}),
    'nested': ({'parent/equal.txt': b'a', 'parent/deep/diff.txt': b'a', 'parent/left.txt': b'l', 'empty': None},
               {'parent/equal.txt': b'a', 'parent/deep/diff.txt': b'b', 'parent/right.txt': b'r', 'empty': None}),
    'case-parent': ({'Data/equal.txt': b'a'}, {'data/EQUAL.TXT': b'a', 'data/new.txt': b'new'}),
    'unicode': ({'Ação/índice.txt': 'olá'.encode(), 'ÁRVORE.txt': b'1'},
                {'ação/ÍNDICE.TXT': 'olá'.encode(), 'árvore.txt': b'2', 'ação/novo.txt': b'new'}),
    'file-directory': ({'conflict': b'file', 'other/child.txt': b'a'},
                      {'conflict/child.txt': b'child', 'other': b'file'}),
    'empty': ({'both-empty': None, 'left-empty': None}, {'both-empty': None, 'right-empty': None}),
    'extensionless': ({'README': b'a', '.hidden': b'b', 'z-dir/first.txt': b'1', 'A-file.txt': b'2'},
                      {'README': b'a', '.hidden': b'c', 'z-dir/first.txt': b'1', 'A-file.txt': b'2'}),
    'ordering': ({name: b'equal' for name in ['item1.txt','item2.txt','item10.txt','índice.txt','novo.txt','Árvore.txt','zeta.txt']},
                 {name: b'equal' for name in ['item1.txt','item2.txt','item10.txt','índice.txt','novo.txt','Árvore.txt','zeta.txt']}),
    'large-binary': ({'chunk.bin': bytes(65536)+b'A'+bytes(65536), 'empty.bin': b''},
                     {'chunk.bin': bytes(65536)+b'B'+bytes(65536), 'empty.bin': b''}),
}
manifest=[]
commands=[f'log verbose "{OUTPUT / "runtime.log"}"']
for name,(left,right) in cases.items():
    paths=[]
    for side,files in [('left',left),('right',right)]:
        base=OUTPUT/name/side
        base.mkdir(parents=True,exist_ok=True)
        for relative,data in files.items():
            path=base/relative
            if data is None:
                path.mkdir(parents=True,exist_ok=True)
            else:
                path.parent.mkdir(parents=True,exist_ok=True)
                if path.exists():
                    assert path.read_bytes()==data,path
                else:
                    path.write_bytes(data)
                os.utime(path,(1700000000,1700000000))
        paths.append(base)
    for suffix,a,b in [('',paths[0],paths[1]),('-reverse',paths[1],paths[0])]:
        report=OUTPUT/(name+suffix+'.xml')
        commands += ['criteria binary', f'load "{a}" "{b}"', 'expand all', 'select all',
                     'compare binary', f'folder-report layout:xml options:display-all output-to:"{report}"']
        filtered = {}
        for mode, option in [('matches', 'display-matches'), ('differences', 'display-mismatches')]:
            filtered[mode] = str(OUTPUT/(name+suffix+'-'+mode+'.xml'))
            commands.append(f'folder-report layout:xml options:{option} output-to:"{filtered[mode]}"')
        manifest.append(dict(name=name+suffix,left=str(a),right=str(b),report=str(report),filtered=filtered))
(OUTPUT/'report-script.txt').write_text('\r\n'.join(commands)+'\r\n',encoding='utf-8-sig')
inputs={str(p.relative_to(OUTPUT)):hashlib.sha256(p.read_bytes()).hexdigest()
        for name in cases for p in (OUTPUT/name).rglob('*') if p.is_file()}
(OUTPUT/'inputs.json').write_text(json.dumps(dict(cases=manifest,sha256=inputs),indent=2)+'\n',encoding='utf-8')
print(json.dumps(dict(program=str(RUNTIME/'BCompare.exe'),script=str(OUTPUT/'report-script.txt'),cases=len(manifest))))
