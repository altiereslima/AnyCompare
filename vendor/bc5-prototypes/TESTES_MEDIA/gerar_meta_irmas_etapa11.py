#!/usr/bin/env python3
"""Gera AAC/M4A com dois átomos `meta` irmãos para demonstrar colisão de caminhos.

Sem comparação semântica: apenas estrutura ISO BMFF, offsets e hashes.
Requer apenas Python 3 e a base de mídia sintética da etapa 10.
"""
from __future__ import annotations
import hashlib, json, struct
from pathlib import Path
BASE=Path(__file__).resolve().parent
SRC=BASE/'ETAPA10_MDTA/mdta_01_identical_A.m4a'
DST=BASE/'ETAPA11_META_IRMAS'

def box(typ: bytes, body: bytes) -> bytes:
    assert len(typ)==4
    return struct.pack('>I',len(body)+8)+typ+body

def boxes(b:bytes, start=0):
    p=start
    while p<len(b):
        if len(b)-p<8:raise ValueError('truncated')
        n=int.from_bytes(b[p:p+4],'big')
        if n<8 or p+n>len(b):raise ValueError('invalid box')
        yield p,b[p+4:p+8],b[p+8:p+n]
        p+=n

def make_meta(key: str, value: str) -> bytes:
    key_b=key.encode('utf8')
    entry=box(b'mdta',key_b)
    keys=box(b'keys',b'\0\0\0\0'+struct.pack('>I',1)+entry)
    data=box(b'data',struct.pack('>II',1,0)+value.encode('utf8'))
    ilst=box(b'ilst',box(struct.pack('>I',1),data))
    # hdlr is optional for structural observer; use hdlr from existing base
    hdlr=box(b'hdlr',b'\0'*8+b'mdta'+b'\0'*12)
    return box(b'meta',b'\0\0\0\0'+hdlr+keys+ilst)

def replace_udta(b:bytes, metas:list[tuple[str,str]]) -> bytes:
    moov_found=False
    result=[]
    for _,type_,body in boxes(b):
        if type_!=b'moov':
            result.append(box(type_,body));continue
        moov_found=True
        internal=[];udta_found=False
        for _,kind,data in boxes(body):
            if kind==b'udta':
                assert not udta_found
                udta_found=True
                internal.append(box(b'udta',b''.join(make_meta(k,v) for k,v in metas)))
            else:internal.append(box(kind,data))
        assert udta_found
        result.append(box(b'moov',b''.join(internal)))
    assert moov_found
    return b''.join(result)

def main():
    DST.mkdir(parents=True,exist_ok=True)
    src=SRC.read_bytes()
    # Preserve AAC payload; the original mdat precedes moov.
    assert [k for _,k,_ in boxes(src)]==[b'ftyp',b'free',b'mdat',b'moov']
    cases=[
      ('01_orders', [('com.bc5.alpha','Primeiro'),('com.bc5.beta','Segundo')],
                     [('com.bc5.beta','Segundo'),('com.bc5.alpha','Primeiro')]),
      ('02_value', [('com.bc5.alpha','Igual'),('com.bc5.beta','Antes')],
                    [('com.bc5.alpha','Igual'),('com.bc5.beta','Depois')]),
      ('03_unicode', [('com.bc5.título','São Paulo'),('com.bc5.beta','ação')],
                      [('com.bc5.título','São Paulo'),('com.bc5.beta','ação')]),
      ('04_repeated', [('com.bc5.same','A'),('com.bc5.same','B')],
                       [('com.bc5.same','B'),('com.bc5.same','A')]),
    ]
    result={'schema':'bc5-mp4-meta-siblings-v1','bc5_semantic_golden':False,'source':SRC.name,'cases':[]}
    for id_,left,right in cases:
        row={'id':id_}
        for side,seq in [('A',left),('B',right)]:
            data=replace_udta(src,seq)
            name=f'{id_}_{side}.m4a';(DST/name).write_bytes(data)
            row[side]={'name':name,'bytes':len(data),'sha256':hashlib.sha256(data).hexdigest(),
                       'meta': [{'key':'mdta:'+k,'value':v} for k,v in seq]}
        result['cases'].append(row)
    (DST/'manifest.json').write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n')
    print(f'Gerados {len(cases)*2} M4A com dois meta irmãos cada (sem expectativa BC5)')

if __name__=='__main__': main()
