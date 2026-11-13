"""Corpus sintético inofensivo para .reg; nunca executa nem importa arquivos no Windows."""
from pathlib import Path
import json,hashlib
root=Path(__file__).resolve().parent
head='Windows Registry Editor Version 5.00\r\n\r\n'
def write(name,text,enc='utf16'):
    raw=('\ufeff'+text).encode('utf-16le') if enc=='utf16' else text.encode('utf-8-sig')
    (root/name).write_bytes(raw)
    return {'name':name,'encoding':enc,'sha256':hashlib.sha256(raw).hexdigest(),'bytes':len(raw)}
files=[]
files.append(write('01_iguais_esquerda.reg',head+'[HKEY_CURRENT_USER\\Software\\BC5-Teste]\r\n"Nome"="Teste"\r\n"Ativo"=dword:00000001\r\n'))
files.append(write('01_iguais_direita.reg',head+'[hkey_current_user\\SOFTWARE\\bc5-teste]\r\n"nome"="Teste"\r\n"ativo"=hex(4):01,00,00,00\r\n'))
files.append(write('02_diferencas_esquerda.reg',head+'[HKEY_CURRENT_USER\\Software\\BC5-Teste]\r\n@="Padrão A"\r\n"Ativo"=dword:00000001\r\n"SóEsquerda"=hex:aa,bb,cc\r\n'))
files.append(write('02_diferencas_direita.reg',head+'[HKEY_CURRENT_USER\\Software\\BC5-Teste]\r\n@="Padrão B"\r\n"Ativo"=dword:00000000\r\n"SóDireita"=hex:aa,bb,cc\r\n'))
files.append(write('03_tipos_esquerda.reg',head+'[HKEY_LOCAL_MACHINE\\Software\\BC5-Teste]\r\n"Expandir"=hex(2):25,00,54,00,45,00,53,00,54,00,25,00,00,00\r\n"Lista"=hex(7):41,00,00,00,42,00,00,00,00,00\r\n"Qword"=hex(b):08,07,06,05,04,03,02,01\r\n"Binario"=hex:00,ff,01,02,\\\r\n  10,11,12\r\n'))
files.append(write('03_tipos_direita.reg',head+'[HKEY_LOCAL_MACHINE\\Software\\BC5-Teste]\r\n"Expandir"=hex(2):25,00,54,00,45,00,53,00,54,00,25,00,00,00\r\n"Lista"=hex(7):41,00,00,00,42,00,00,00,00,00\r\n"Qword"=hex(b):08,07,06,05,04,03,02,01\r\n"Binario"=hex:00,ff,01,02,10,11,12\r\n'))
files.append(write('04_chaves_esquerda.reg',head+'[HKEY_CURRENT_USER\\Software\\BC5-Teste\\A]\r\n"a"="1"\r\n'))
files.append(write('04_chaves_direita.reg',head+'[HKEY_CURRENT_USER\\Software\\BC5-Teste\\B]\r\n"b"="2"\r\n'))
files.append(write('05_delecao_invalida.reg',head+'[-HKEY_CURRENT_USER\\Software\\BC5-Teste]\r\n'))
files.append(write('06_duplicado_conflitante.reg',head+'[HKEY_CURRENT_USER\\Software\\BC5-Teste]\r\n"X"=dword:00000000\r\n"X"=dword:00000001\r\n'))
files.append(write('07_ascii_regedit4.reg','REGEDIT4\r\n\r\n[HKEY_CURRENT_USER\\Software\\BC5-Teste]\r\n"x"="ascii"\r\n','utf8'))
files.append(write('08_utf8_bom.reg',head+'[HKEY_CURRENT_USER\\Software\\BC5-Teste]\r\n"Titulo"="ação"\r\n','utf8'))
files.append(write('09_utf16_curto.reg',head+'[HKEY_CURRENT_USER\\Software\\BC5-Teste]\r\n"x"=hex(7):01,00,00\r\n'))
(root/'manifest.json').write_text(json.dumps(files,ensure_ascii=False,indent=2))
print('corpus:',len(files),'arquivos, tamanho total',sum(x['bytes'] for x in files))
