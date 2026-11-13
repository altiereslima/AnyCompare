#!/usr/bin/env python3
"""Cria um corpus MP4/mdta real usando FFmpeg, sem supor regras do BC5.

Os arquivos `.m4a` são contêineres AAC reais com `meta/keys` e `ilst`.
A gravação é local e sintética. Verificar antes de substituir um corpus.
"""
import argparse
import hashlib
import json
import shutil
import subprocess
import tempfile
from pathlib import Path

ROOT=Path(__file__).resolve().parent
TARGET=ROOT/'ETAPA10_MDTA'
CASES={
  'mdta_01_identical': ({'com.example.title':'Árvore','com.example.genre':'Teste'}, {'com.example.title':'Árvore','com.example.genre':'Teste'}),
  'mdta_02_value': ({'com.example.title':'Título A','com.example.genre':'Teste'}, {'com.example.title':'Título B','com.example.genre':'Teste'}),
  'mdta_03_missing_key': ({'com.example.title':'Um'}, {'com.example.title':'Um','com.example.artist':'Dois'}),
  'mdta_04_order_unicode': ({'com.example.artist':'Coração','com.example.title':'Água'}, {'com.example.title':'Água','com.example.artist':'Coração'}),
}

def run(cmd):
    subprocess.run(cmd,check=True,stdout=subprocess.PIPE,stderr=subprocess.PIPE,timeout=40)

def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()

def gen():
    TARGET.mkdir(parents=True,exist_ok=True)
    with tempfile.TemporaryDirectory() as temp:
        base=Path(temp)/'silent.m4a'
        run(['ffmpeg','-hide_banner','-loglevel','error','-f','lavfi','-i','anullsrc=r=44100:cl=mono',
            '-t','0.5','-c:a','aac','-b:a','48k','-y',str(base)])
        rows=[]
        for case,(left,right) in CASES.items():
            row={'id':case,'purpose':'MP4 keys/mdta structural observation (not BC5 parity)'}
            for side,tags in [('A',left),('B',right)]:
                filename=f'{case}_{side}.m4a'
                path=TARGET/filename
                cmd=['ffmpeg','-hide_banner','-loglevel','error','-i',str(base),'-map','0:a:0','-c:a','copy',
                     '-map_metadata','-1','-movflags','+use_metadata_tags']
                for key,value in tags.items():cmd+=['-metadata',key+'='+value]
                cmd+=['-y',str(path)]
                run(cmd)
                run(['ffprobe','-v','error','-show_entries','format=duration','-of','json',str(path)])
                b=path.read_bytes()
                assert all(x in b for x in [b'moov',b'meta',b'keys',b'ilst',b'mdta']),filename
                row[side]={'name':filename,'sha256':sha(path),'bytes':len(b),'expected_tags':tags}
            rows.append(row)
    manifest={'schema':'bc5-media-mdta-structural-v1','cases':rows,
              'reference':'ISO BMFF keys/ilst; decoded only for diagnostics',
              'bc5_semantic_golden':False}
    (TARGET/'manifest.json').write_text(json.dumps(manifest,indent=2,ensure_ascii=False)+'\n',encoding='utf-8')
    return manifest

if __name__=='__main__':
    if not shutil.which('ffmpeg') or not shutil.which('ffprobe'):
        raise SystemExit('FFmpeg e FFprobe requeridos para regeneração')
    print(json.dumps({'cases':len(gen()['cases']),'root':str(TARGET)},ensure_ascii=False))
