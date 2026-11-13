#!/usr/bin/env python3
"""Auditoria externa do corpus etapa 11: recipientes meta irmãos ISO BMFF.

Não executa Rust, nem infere decisões de igualdade do BC5. Um parser independente
recupera o offset de cada `meta`, suas respectivas keys e seus ilst/data.
"""
from __future__ import annotations
import hashlib, json, subprocess, shutil
from pathlib import Path
HERE=Path(__file__).resolve().parent/'ETAPA11_META_IRMAS'

def boxes(data: bytes,start:int,end:int):
    pos=start
    while pos<end:
        if pos+8>end:raise ValueError(f'truncated header {pos}')
        n=int.from_bytes(data[pos:pos+4],'big');typ=data[pos+4:pos+8];hdr=8
        if n==1:
            if pos+16>end:raise ValueError('extended size truncated')
            n=int.from_bytes(data[pos+8:pos+16],'big');hdr=16
        elif n==0:n=end-pos
        if n<hdr or pos+n>end:raise ValueError(f'bad box: {typ!r} {pos}')
        yield pos,typ,pos+hdr,pos+n
        pos+=n

def collect(b:bytes):
    matches=[];mdat_hash=None;meta_offsets=[]
    root=list(boxes(b,0,len(b)))
    assert [x[1] for x in root]==[b'ftyp',b'free',b'mdat',b'moov']
    for start,typ,body,end in root:
        if typ==b'mdat':mdat_hash=hashlib.sha256(b[body:end]).hexdigest()
        if typ!=b'moov':continue
        for _,moov_typ,moov_body,moov_end in boxes(b,body,end):
            if moov_typ!=b'udta':continue
            for meta_start,meta_type,m_body,m_end in boxes(b,moov_body,moov_end):
                assert meta_type==b'meta'
                assert b[m_body:m_body+4]==b'\0\0\0\0'
                keys=[];values=[]
                for _,item_type,item_body,item_end in boxes(b,m_body+4,m_end):
                    if item_type==b'keys':
                        assert b[item_body:item_body+4]==b'\0'*4
                        count=int.from_bytes(b[item_body+4:item_body+8],'big')
                        for _,namespace,key_body,key_end in boxes(b,item_body+8,item_end):
                            assert namespace==b'mdta'
                            keys.append(b[key_body:key_end].decode('utf8'))
                        assert count==len(keys)
                    if item_type==b'ilst':
                        for _,key_type,field_body,field_end in boxes(b,item_body,item_end):
                            index=int.from_bytes(key_type,'big')
                            for _,subtype,dat_body,dat_end in boxes(b,field_body,field_end):
                                if subtype==b'data':
                                    assert b[dat_body:dat_body+8]==b'\0\0\0\x01\0\0\0\0'
                                    values.append((index,b[dat_body+8:dat_end].decode('utf8')))
                assert len(keys)==1 and len(values)==1
                assert values[0][0]==1
                matches.append({'meta_header_offset':meta_start,'key':'mdta:'+keys[0],
                                'value':values[0][1], 'path':'mp4/moov/udta/meta'})
                meta_offsets.append(meta_start)
    assert len(matches)==2 and len(set(meta_offsets))==2
    assert len(set(row['path'] for row in matches))==1
    return matches,mdat_hash

def main():
    manifest=json.loads((HERE/'manifest.json').read_text('utf8'))
    assert manifest['schema']=='bc5-mp4-meta-siblings-v1'
    assert manifest['bc5_semantic_golden'] is False
    reference=Path(__file__).resolve().parent/'ETAPA10_MDTA/mdta_01_identical_A.m4a'
    _,ref_mdat_hash = collect_mdat_only(reference.read_bytes())
    ffprobe=shutil.which('ffprobe')
    total=0
    for case in manifest['cases']:
        for side in ('A','B'):
            record=case[side]; path=HERE/record['name']; data=path.read_bytes()
            assert hashlib.sha256(data).hexdigest()==record['sha256']
            assert len(data)==record['bytes']
            parsed,mdat_hash=collect(data)
            assert mdat_hash==ref_mdat_hash
            assert [{k:v for k,v in r.items() if k in ('key','value')} for r in parsed]==record['meta']
            if ffprobe:
                p=subprocess.run([ffprobe,'-v','error','-show_entries','format=duration',
                                  '-of','default=noprint_wrappers=1:nokey=1',str(path)],
                                  capture_output=True,text=True,timeout=15)
                assert p.returncode==0 and float(p.stdout.strip())>0
            total+=1
    print(f'PASS: {total}/{total} arquivos MP4 com dois meta irmãos, offsets distintos, AAC preservado')

def collect_mdat_only(data:bytes):
    mdat=[hashlib.sha256(data[b:e]).hexdigest() for p,t,b,e in boxes(data,0,len(data)) if t==b'mdat']
    assert len(mdat)==1
    return None,mdat[0]

if __name__=='__main__': main()
