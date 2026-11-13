#!/usr/bin/env python3
"""Cria MP3 sintéticos determinísticos para instrumentar o observador ID3.

Não define expectativas sobre comparação do Beyond Compare 5; apenas sobre bytes
ID3 e condições de contorno do parser. Usa áudio MP3 original do corpus anterior.
"""
from __future__ import annotations
import hashlib, json
from pathlib import Path
HERE=Path(__file__).resolve().parent
OUT=HERE/'ETAPA12_ID3_LIMITES'
OUT.mkdir(exist_ok=True)
SOURCE=HERE.parent/'RUST_APP/crates/bc5-media/tests/fixtures/A.mp3'
raw=SOURCE.read_bytes()
def ss(n: int)->bytes:
    assert 0<=n<1<<28
    return bytes((n>>21&127,n>>14&127,n>>7&127,n&127))
assert raw[:3]==b'ID3'
header_size=(raw[6]<<21)|(raw[7]<<14)|(raw[8]<<7)|raw[9]
audio=raw[10+header_size:]
assert audio[:2] in (b'\xff\xfb',b'\xff\xf3',b'\xff\xf2')

def frame(v: int, val: str)->bytes:
    payload=b'\x03'+val.encode('utf8')
    return b'TIT2'+(ss(len(payload)) if v==4 else len(payload).to_bytes(4,'big'))+b'\0\0'+payload

def tag(v: int, val: str, *, ext:bytes=b'', footer:bool=False, footer_mode:str='valid', global_unsync:bool=False)->bytes:
    flags=(0x40 if ext else 0)|(0x10 if footer else 0)|(0x80 if global_unsync else 0)
    data=ext+frame(v,val)
    header=b'ID3'+bytes((v,0,flags))+ss(len(data))
    result=header+data
    if footer and footer_mode!='missing':
        f=b'3DI'+header[3:]
        if footer_mode=='signature':f=b'3DX'+f[3:]
        if footer_mode=='version':f=f[:3]+b'\x03'+f[4:]
        if footer_mode=='size':f=f[:9]+bytes((f[9]^1,))
        result+=f
    return result

ext23=b'\x00\x00\x00\x06\x00\x00\x00\x00\x00\x00'
ext24=b'\x00\x00\x00\x06\x01\x00'
# Dois tags idênticos em casos contrastivos reproduzem situações de contorno.
cases=[
 ('01_footer_valid', tag(4,'Faixa A',footer=True),tag(4,'Faixa B',footer=True),'ID3v2.4 footer válido'),
 ('02_footer_missing',tag(4,'Faixa',footer=True),tag(4,'Faixa',footer=True,footer_mode='missing'),'footer declarado mas ausente'),
 ('03_footer_signature',tag(4,'Faixa',footer=True),tag(4,'Faixa',footer=True,footer_mode='signature'),'assinatura footer inválida'),
 ('04_two_tags',tag(3,'V23 A')+tag(4,'V24 A'),tag(3,'V23 A')+tag(4,'V24 B'),'duas tags consecutivas'),
 ('05_extended_v23',tag(3,'A',ext=ext23),tag(3,'B',ext=ext23),'extended header v2.3'),
 ('06_extended_v24',tag(4,'A',ext=ext24),tag(4,'B',ext=ext24),'extended header v2.4'),
 ('07_extended_too_short',tag(4,'A',ext=b'\0\0\0\x04'),tag(4,'B',ext=b'\0\0\0\x04'),'extended header v2.4 abaixo do mínimo'),
 ('08_unsync_per_tag',tag(4,'Primeira',global_unsync=True)+tag(4,'Segunda'),tag(4,'Primeira',global_unsync=True)+tag(4,'Terceira'),'unsync diferente entre tags'),
 ('09_footer_wrong_version',tag(4,'Faixa',footer=True),tag(4,'Faixa',footer=True,footer_mode='version'),'versão footer diferente do header'),
 ('10_footer_wrong_size',tag(4,'Faixa',footer=True),tag(4,'Faixa',footer=True,footer_mode='size'),'tamanho footer diferente do header'),
]
manifest={'schema':'bc5-id3-boundaries-stage12-v1','stage':12,'source_audio':str(SOURCE.relative_to(HERE.parent)),'source_audio_sha256':hashlib.sha256(audio).hexdigest(),
          'bc5_semantic_golden':False,'cases':[]}
for identifier,left,right,description in cases:
    item={'id':identifier,'description':description}
    for side,data in (('A',left),('B',right)):
        name=f'{identifier}_{side}.mp3'
        payload=data+audio
        (OUT/name).write_bytes(payload)
        item[side]={'name':name,'bytes':len(payload),'sha256':hashlib.sha256(payload).hexdigest(),
                    'first_audio_offset':len(data),'id3_prefix_len':len(data)}
    manifest['cases'].append(item)
(OUT/'manifest.json').write_text(json.dumps(manifest,ensure_ascii=False,indent=2)+'\n')
print('Created',len(cases),'ID3 pairs, 20 MP3 files; BC5 semantics NOT established')
