"""Configure only Codex's existing REA server; preserve all unrelated settings."""
from pathlib import Path
import copy,json,os,re,shutil,time,tomllib

path=Path(r'C:\Users\Altieres\.codex\config.toml')
install=Path(r'C:\Users\Altieres\.codex\tools\ghidra\ghidra_12.1.4_PUBLIC')
assert (install/'support/analyzeHeadless.bat').is_file()
assert (install/'Ghidra/Features/Decompiler/os/win_x86_64/decompile.exe').is_file()
original=path.read_bytes();text=original.decode('utf-8-sig');before=tomllib.loads(text)
assert 'rea' in before['mcp_servers']
header='[mcp_servers.rea.env]'
value=json.dumps(str(install))
if header in text:
    start=text.index(header);end=text.find('\n[',start+len(header))
    if end<0:end=len(text)
    section=text[start:end]
    if re.search(r'^GHIDRA_INSTALL_DIR\s*=',section,re.M):
        section=re.sub(r'^GHIDRA_INSTALL_DIR\s*=.*$',f'GHIDRA_INSTALL_DIR = {value}',section,flags=re.M)
    else:section+='\nGHIDRA_INSTALL_DIR = '+value+'\n'
    text=text[:start]+section+text[end:]
else:
    assert not before['mcp_servers']['rea'].get('env'), 'Existing inline env requires a structured migration.'
    text+='\n'+header+'\nGHIDRA_INSTALL_DIR = '+value+'\n'
after=tomllib.loads(text)
assert after['mcp_servers']['rea']['env']['GHIDRA_INSTALL_DIR']==str(install)
expected=copy.deepcopy(before)
expected['mcp_servers']['rea'].setdefault('env',{})['GHIDRA_INSTALL_DIR']=str(install)
assert after==expected,'Unrelated configuration changed.'
if path.read_bytes()!=original:raise RuntimeError('Configuration changed concurrently; retry from the current file.')
stamp=time.time_ns();backup=path.with_name(f'config.before-ghidra-{stamp}.toml.bak')
shutil.copy2(path,backup)
temporary=path.with_name(f'.config-ghidra-{stamp}.tmp')
with temporary.open('xb') as file:file.write(text.encode('utf-8'));file.flush();os.fsync(file.fileno())
os.replace(temporary,path)
print('REA Ghidra path:',install)
print('Configuration backup:',backup)
