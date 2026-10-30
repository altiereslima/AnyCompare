"""Prepare a separate portable replay; never edit the reference/profile/inputs."""
import hashlib
import json
from pathlib import Path
import shutil

root=Path(__file__).resolve().parents[1]
runtime=root/'.codex/reference-reverse-runtime'
folder=root/'docs/reference/controlled-reverse-replay'
runtime.mkdir(parents=True,exist_ok=True)
folder.mkdir(parents=True,exist_ok=True)
for name in ['BCompare.exe','7z.dll','BCUnRAR.dll','mscoree.dll']:
    shutil.copyfile(root/'.codex/reference-runtime'/name,runtime/name)
assert hashlib.sha256((runtime/'BCompare.exe').read_bytes()).hexdigest()=='aba1b98f848a01c3729cfee3e8818ab8e0d947276bc42c9a55c1475c9f8438db'
state=runtime/'BCState.xml'
if not state.exists(): state.write_bytes(b'')
inputs=[]
for side,name in [('left','resumo 1.txt'),('right','resumo.txt')]:
    source=Path('C:/Users/Altieres/Desktop')/name
    data=source.read_bytes()
    (folder/f'{side}.txt').write_bytes(data)
    inputs.append(dict(source=str(source),copy=f'{side}.txt',sha256=hashlib.sha256(data).hexdigest()))
commands=[]
for layout,extension,options in [('side-by-side','html',' output-options:html-color'),('xml','xml','')]:
    commands.append(f'text-report layout:{layout} options:display-all output-to:"{folder}/report.{extension}"{options} "{folder}/left.txt" "{folder}/right.txt"')
(folder/'report-script.txt').write_text('\r\n'.join(commands)+'\r\n',encoding='utf-8-sig',newline='')
(folder/'inputs.json').write_text(json.dumps(inputs,indent=2)+'\n',encoding='utf-8')
print(folder)
