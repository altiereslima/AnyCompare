#!/usr/bin/env python3
"""Validação auxiliar independente dos fixtures ID3, sem executar Rust ou BC5."""
import hashlib,json,subprocess,sys
from pathlib import Path
ROOT=Path(__file__).resolve().parent
folder=ROOT/'ETAPA12_ID3_LIMITES'
m=json.loads((folder/'manifest.json').read_text())
assert m['bc5_semantic_golden'] is False
source=ROOT.parent/'RUST_APP/crates/bc5-media/tests/fixtures/A.mp3'
def ss(b):return (b[0]<<21)|(b[1]<<14)|(b[2]<<7)|b[3]
src=source.read_bytes()
audio=src[10+ss(src[6:10]):]
assert hashlib.sha256(audio).hexdigest()==m['source_audio_sha256']
assert len(m['cases'])==10
ok=0
for case in m['cases']:
    for side in ('A','B'):
        row=case[side]; p=folder/row['name'];data=p.read_bytes()
        assert len(data)==row['bytes']
        assert hashlib.sha256(data).hexdigest()==row['sha256']
        assert data.endswith(audio)
        assert data[row['first_audio_offset']:]==audio
        assert data[:3]==b'ID3'
        assert data[3] in (3,4)
        probe=subprocess.run(['ffprobe','-v','error','-show_entries','format=duration',
                              '-of','default=noprint_wrappers=1:nokey=1',str(p)],
                              capture_output=True,text=True,timeout=10)
        assert probe.returncode==0,(p.name,probe.stderr)
        assert float(probe.stdout.strip())>0,(p.name,probe.stdout)
        ok+=1
print(f'{ok}/{ok} arquivos MP3 aceitos e corpus byte a byte validado; BC5 GOLDEN: AUSENTE')
