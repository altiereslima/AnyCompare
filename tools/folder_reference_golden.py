"""Decode original BC4 XML folder reports, preserving typed pairs and native order."""
import hashlib
import json
from pathlib import Path
import xml.etree.ElementTree as ET

ROOT=Path(__file__).resolve().parents[1]/'docs/reference/controlled-folder-replay-2026-10-30'
manifest=json.loads((ROOT/'inputs.json').read_text(encoding='utf-8'))
for name,digest in manifest['sha256'].items():
    assert hashlib.sha256((ROOT/name).read_bytes()).hexdigest()==digest,name


def rows(node,left_parent='',right_parent=''):
    result=[]
    for child in node:
        if child.tag not in ['filecomp','foldercomp']:
            continue
        a=child.findtext('lt/name');b=child.findtext('rt/name')
        left=f'{left_parent}/{a}'.strip('/') if a is not None else None
        right=f'{right_parent}/{b}'.strip('/') if b is not None else None
        descendants=rows(child,left or left_parent,right or right_parent) if child.tag=='foldercomp' else []
        if left is None: status='right-only'
        elif right is None: status='left-only'
        elif child.tag=='foldercomp': status='different' if any(r['status']!='same' for r in descendants) else 'same'
        else: status={'same':'same','diff':'different','ltonly':'left-only','rtonly':'right-only'}[child.attrib['status']]
        result.append(dict(kind='directory' if child.tag=='foldercomp' else 'file',left=left,right=right,status=status))
        result+=descendants
    return result

expected=[]
for case in manifest['cases']:
    report=Path(case['report'])
    expected.append(dict(name=case['name'],left=case['left'],right=case['right'],
                         report_sha256=hashlib.sha256(report.read_bytes()).hexdigest(),
                         entries=rows(ET.parse(report).getroot().find('foldercomp')),
                         filtered={mode: rows(ET.parse(path).getroot().find('foldercomp'))
                                   for mode,path in case.get('filtered',{}).items()}))
(ROOT/'expected.json').write_text(json.dumps(expected,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
print(json.dumps(dict(cases=len(expected),entries=sum(len(c['entries']) for c in expected))))
