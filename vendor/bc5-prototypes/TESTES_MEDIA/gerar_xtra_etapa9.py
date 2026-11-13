#!/usr/bin/env python3
"""Gera quatro pares M4A de TESTE ESTRUTURAL, não de semântica BC5.

Os payloads Xtra são deliberadamente artificiais; não afirmamos que o BC5
interpreta metadados proprietários desses bytes. Sempre preserva áudio original.
O 'moov' fica depois de 'mdat' no corpus, então nenhuma entrada stco muda.
"""
from __future__ import annotations
import hashlib,json,struct
from pathlib import Path

HERE=Path(__file__).resolve().parent
SRC=HERE/'CASOS'
OUT=HERE/'ETAPA9_XTRA'
OUT.mkdir(exist_ok=True)

def sha(b:bytes)->str:return hashlib.sha256(b).hexdigest()
def atom(k:bytes,v:bytes)->bytes:
    assert len(k)==4
    return struct.pack('>I',len(v)+8)+k+v

def inject(base:bytes,xtra:bytes)->bytes:
    pos=0
    while pos+8<=len(base):
        n=int.from_bytes(base[pos:pos+4],'big')
        assert n>=8 and pos+n<=len(base)
        if base[pos+4:pos+8]==b'moov':break
        pos+=n
    else:raise AssertionError('moov nao encontrado')
    assert pos+n==len(base),'moov deve estar por ultimo no corpus'
    moov_start=pos
    child=moov_start+8
    while child+8<=len(base):
        cn=int.from_bytes(base[child:child+4],'big')
        assert cn>=8 and child+cn<=len(base)
        if base[child+4:child+8]==b'udta':break
        child+=cn
    else:raise AssertionError('udta ausente')
    assert child+cn==len(base),'udta deve estar ao final de moov no corpus'
    extra=atom(b'Xtra',xtra)
    modified=(base[:moov_start]
              +struct.pack('>I',n+len(extra))+base[moov_start+4:child]
              +struct.pack('>I',cn+len(extra))+base[child+4:]
              +extra)
    assert len(modified)==len(base)+len(extra)
    return modified

scenarios=[
 ('xtra_01_unchanged','m4a_02_title_A.m4a','m4a_02_title_A.m4a',b'SYNTHETIC XTRA SAME',b'SYNTHETIC XTRA SAME'),
 ('xtra_02_only_xtra','m4a_02_title_A.m4a','m4a_02_title_A.m4a',b'SYNTHETIC XTRA LEFT',b'SYNTHETIC XTRA RIGHT'),
 ('xtra_03_title_only','m4a_02_title_A.m4a','m4a_02_title_B.m4a',b'SYNTHETIC XTRA SAME',b'SYNTHETIC XTRA SAME'),
 ('xtra_04_both','m4a_02_title_A.m4a','m4a_02_title_B.m4a',b'SYNTHETIC XTRA LEFT',b'SYNTHETIC XTRA RIGHT'),
]
entries=[]
for ident,l,r,lb,rb in scenarios:
    files={}
    for side,src,payload in (('A',l,lb),('B',r,rb)):
        result=inject((SRC/src).read_bytes(),payload)
        path=OUT/f'{ident}_{side}.m4a'
        path.write_bytes(result)
        files[side]={'name':path.name,'sha256':sha(result),'bytes':len(result),'xtra_hex':payload.hex()}
    entries.append({'id':ident,'left':files['A'],'right':files['B'],
                    'expected_bc5':'UNKNOWN; do not use as a golden result'})
manifest={'schema':'bc5-media-xtra-structural-probe-v1',
          'warning':'Xtra payloads are synthetic and not validated as Xtra metadata',
          'requires_windows_bc5_test':True,'cases':entries}
(OUT/'manifest.json').write_text(json.dumps(manifest,indent=2,ensure_ascii=False)+'\n')
print(json.dumps({'cases':len(entries),'files':len(entries)*2,'folder':str(OUT)}))
