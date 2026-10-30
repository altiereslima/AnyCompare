"""Owned read-only BC4 report fixtures for REA controlled runtime observation."""
from pathlib import Path
import shutil

root=Path(__file__).resolve().parents[1]
runtime=root/'.codex/reference-runtime'
runtime.mkdir(parents=True,exist_ok=True)
source=Path(r'C:\Projetos\BC4\bc4-en')
for name in ['BCompare.exe','7z.dll','BCUnRAR.dll','mscoree.dll']:
    target=runtime/name
    if not target.exists():shutil.copyfile(source/name,target)
# Presence beside the executable selects a separate portable settings folder.
state=runtime/'BCState.xml'
if not state.exists():state.write_bytes(b'')
fixtures=root/'docs/reference/controlled-replay'
fixtures.mkdir(parents=True,exist_ok=True)
archive=fixtures/'pass1'
if (fixtures/'report.html').exists() and not archive.exists():
    archive.mkdir()
    for name in ['left.txt','right.txt','report.html','report.xml','report-script.txt','expected.json','rea-runtime.json']:
        if (fixtures/name).exists():shutil.copyfile(fixtures/name,archive/name)
left='case:\nA pedra do sigilo\nwhite:\numa forja antiga\nmixed:\nA maga azul\nwords:\nFonte da Magia\nimportant:\nCadinho\npunctuation:\nTexto - antigo\nend:\n'
right='case:\nA Pedra do Sigilo\nwhite:\numa  forja antiga \nmixed:\na maga vermelha\nwords:\nFonte de Magia\nimportant:\nCrisol\npunctuation:\nTexto — antigo\nend:\n'
for index,(a,b) in enumerate([('aXbYz','aUbVz'),('aXbcYz','aUbcVz'),('aXbcdYz','aUbcdVz'),('aXabcdYz','aUabcdVz'),('aXbYcZd','aUbVcWd'),('lado A palavra comum fim Z','lado B palavra comum fim W')]):
    left+=f'anchor-{index}:\n{a}\n';right+=f'anchor-{index}:\n{b}\n'
(fixtures/'left.txt').write_text(left,encoding='utf-8',newline='')
(fixtures/'right.txt').write_text(right,encoding='utf-8',newline='')
commands=[]
for layout,extension,output in [('side-by-side','html',' output-options:html-color'),('xml','xml','')]:
    commands.append(f'text-report layout:{layout} options:display-all output-to:"{fixtures}/report.{extension}"{output} "{fixtures}/left.txt" "{fixtures}/right.txt"')
(fixtures/'report-script.txt').write_text('\r\n'.join(commands)+'\r\n',encoding='utf-8-sig',newline='')
print(runtime/'BCompare.exe')
print(fixtures/'report-script.txt')
