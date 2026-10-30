"""Finite geometry check against the user's original captures at 100% DPI.

Compares the two detail seams, the editor-field dividers and the global status
dividers. Variable text, scroll positions and ClearType pixels are not compared.
"""
import argparse
import hashlib
import json
from pathlib import Path
from PIL import Image

root=Path(__file__).resolve().parents[1]
args=argparse.ArgumentParser()
args.add_argument('capture',type=Path)
args.add_argument('--output',type=Path)
opt=args.parse_args()
original_path=root/'docs/reference/screenshots/text-loaded.png'
status_path=root/'docs/reference/screenshots/side-by-side-2026-10-14.png'
ours=Image.open(opt.capture).convert('RGB')
# The original client is 1616 px wide. The 0.1.10 comparison used a crop
# missing its final white border pixel; retain that historical width too.
assert ours.size in [(1615,838),(1616,838)],ours.size
original=Image.open(original_path).convert('RGB').crop((9,42,9+ours.width,880))
assert ours.size==original.size,(ours.size,original.size)

def seams(image):
    return [y for y in range(image.height-95,image.height-19)
        if sum(image.getpixel((x,y))==(160,160,160) for x in range(45,image.width-2))>image.width*0.90]

def pane_fields(image):
    # Count a vertical run: a single font fringe can alter one crossing pixel
    # from 215 to 214 even when the divider itself occupies the correct column.
    return [x for x in range(40,image.width-2)
        if sum(image.getpixel((x,y))==(215,215,215)
            for y in range(image.height-87,image.height-72))>=12]

def status_fields(image):
    return [x for x in range(image.width)
        if sum(image.getpixel((x,y))==(215,215,215) for y in range(image.height-16,image.height-2))>=10]

status=Image.open(status_path).convert('RGB').crop((960,0,1919,1030))
checks=[]
for name,expected,actual,tolerance in [
    ('detail_seams',seams(original),seams(ours),0),
    ('pane_dividers',pane_fields(original),pane_fields(ours),0),
    ('window_status_dividers',status_fields(status),status_fields(ours),0),
]:
    passed=len(expected)==len(actual) and all(abs(a-b)<=tolerance for a,b in zip(expected,actual))
    checks.append(dict(name=name,expected=expected,actual=actual,tolerance_px=tolerance,passed=passed))
report=dict(scope='finite geometry, not complete visual equivalence',
    source_sha256={str(p.relative_to(root)):hashlib.sha256(p.read_bytes()).hexdigest() for p in [original_path,status_path]},
    capture=str(opt.capture),capture_sha256=hashlib.sha256(opt.capture.read_bytes()).hexdigest(),checks=checks)
if opt.output: opt.output.write_text(json.dumps(report,indent=2)+'\n',encoding='utf-8')
print(json.dumps(report,indent=2))
raise SystemExit(0 if all(c['passed'] for c in checks) else 1)
