"""Extract the original's observed per-character colors; no inferred colors."""
from html.parser import HTMLParser
from pathlib import Path
import argparse,hashlib,json
import xml.etree.ElementTree as ET

root=Path(__file__).resolve().parents[1]
args=argparse.ArgumentParser()
args.add_argument('--folder', type=Path, default=root/'docs/reference/controlled-replay')
args.add_argument('--evidence', default='ev_7d4dba38e7571b48eb7f763e320b8e003f9448abfa13f20234cbf32f62e12216')
args=args.parse_args()
folder=args.folder
class Report(HTMLParser):
    def __init__(self):super().__init__(convert_charrefs=True);self.cells=[];self.cell=None;self.kind='Same'
    def handle_starttag(self,tag,attrs):
        values=dict(attrs)
        if tag=='td' and values.get('class','').startswith('TextItem'):
            self.cell={'style':values['class'].split()[0],'chunks':[]};self.kind='Same'
        elif tag=='span' and self.cell is not None:
            self.kind={'TextSegInsigDiff':'Unimportant','TextSegSigDiff':'Diff','TextSegReplacedDiff':'Unimportant'}[values['class']]
    def handle_endtag(self,tag):
        if tag=='td' and self.cell is not None:self.cells.append(self.cell);self.cell=None
        elif tag=='span':self.kind='Same'
    def handle_data(self,text):
        if self.cell is None:return
        if not text:return
        chunks=self.cell['chunks']
        if chunks and chunks[-1]['kind']==self.kind:chunks[-1]['text']+=text
        else:chunks.append({'kind':self.kind,'text':text})

parser=Report();parser.feed((folder/'report.html').read_text(encoding='utf-8-sig'))
comparisons=ET.parse(folder/'report.xml').findall('.//linecomp')
assert len(parser.cells)==2*len(comparisons)
rows=[]
for i,line in enumerate(comparisons):
    cells=parser.cells[2*i:2*i+2]
    texts=[]; numbers=[]
    for side,cell in zip(('ltid','rtid'),cells):
        element=next((t for t in line.findall('text') if side in t.attrib),None)
        text='' if element is None else element.text or ''
        number=None if element is None else int(element.attrib[side])
        raw=''.join(c['text'] for c in cell['chunks'])
        if not text and raw in ('','\xa0'):
            cell['chunks']=[]
        else:
            assert len(raw)==len(text),(i,side,len(raw),len(text))
            at=0
            for chunk in cell['chunks']:
                value=text[at:at+len(chunk['text'])]
                assert all(a==b or a=='\xa0' and b==' ' for a,b in zip(chunk['text'],value)),(i,side,repr(value))
                chunk['text']=value; at+=len(value)
        texts.append(text); numbers.append(number)
    rows.append({'status':line.attrib['status'],'left':texts[0],'right':texts[1],'left_line_no':numbers[0],'right_line_no':numbers[1],'left_inline':cells[0]['chunks'],'right_inline':cells[1]['chunks']})
manifest={'artifact_sha256':hashlib.sha256((root/'.codex/reference-runtime/BCompare.exe').read_bytes()).hexdigest(),'rea_evidence_id':args.evidence,'files':{p.name:hashlib.sha256(p.read_bytes()).hexdigest() for p in folder.iterdir() if p.name in ['left.txt','right.txt','report.html','report.xml']},'rows':rows,'limitations':['Output from factory/default portable settings; not every user session or grammar.','Frida metadata interception is bounded by the hook and does not establish complete API coverage; per-character expectations come from the original HTML/XML reports.']}
(folder/'expected.json').write_text(json.dumps(manifest,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
print('Observed reference rows:',len(rows))
