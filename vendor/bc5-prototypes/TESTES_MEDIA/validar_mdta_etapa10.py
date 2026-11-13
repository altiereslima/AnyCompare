#!/usr/bin/env python3
"""Validação independente dos contêineres ISO BMFF gerados nesta etapa.

Não representa o algoritmo BC5, apenas confere keys/ilst e valores escritos.
"""
from __future__ import annotations
import hashlib,json,struct,sys
from pathlib import Path
ROOT=Path(__file__).resolve().parent/'ETAPA10_MDTA'

def boxes(b, start=0, end=None):
    if end is None:end=len(b)
    p=start
    while p<end:
        if end-p<8:raise ValueError('box header truncated')
        n=int.from_bytes(b[p:p+4],'big');typ=b[p+4:p+8];head=8
        if n==1:
            if p+16>end:raise ValueError('largesize truncated')
            n=int.from_bytes(b[p+8:p+16],'big');head=16
        if n==0:n=end-p
        if n<head or p+n>end:raise ValueError('box size invalid')
        yield typ,b[p+head:p+n]
        p+=n

def inspect(b):
    keys=[];values=[];containers=[]
    def visit(payload, container):
        for typ,data in boxes(payload):
            if typ==b'keys':
                assert len(data)>=8 and data[:4]==b'\0'*4
                count=int.from_bytes(data[4:8],'big')
                entries=list(boxes(data,8))
                assert len(entries)==count
                for namespace,key in entries:
                    assert namespace==b'mdta'
                    keys.append(key.decode('utf-8'))
            elif typ==b'ilst':
                for key, item in boxes(data):
                    index=int.from_bytes(key,'big')
                    for subtype,body in boxes(item):
                        if subtype==b'data':
                            assert len(body)>=8 and int.from_bytes(body[:4],'big')==1
                            values.append((index,body[8:].decode('utf-8')))
            elif typ in (b'moov',b'udta'):
                visit(data,typ)
            elif typ==b'meta':
                assert len(data)>=4
                visit(data[4:],typ)
    visit(b,None)
    return keys,values

def main():
    manifest=json.loads((ROOT/'manifest.json').read_text(encoding='utf-8'))
    assert manifest['schema']=='bc5-media-mdta-structural-v1'
    n=0
    for case in manifest['cases']:
        for side in ('A','B'):
            row=case[side];path=ROOT/row['name'];data=path.read_bytes()
            assert hashlib.sha256(data).hexdigest()==row['sha256']
            assert len(data)==row['bytes']
            keys,values=inspect(data)
            mapped={keys[i-1]:value for i,value in values}
            for key,value in row['expected_tags'].items():
                assert mapped.get(key)==value,(case['id'],side,key,mapped)
            assert len(keys)==len(values)
            n+=1
    print(f'PASS: {n}/{n} MP4 mdta válidos; sem inferência sobre BC5')

if __name__=='__main__':main()
