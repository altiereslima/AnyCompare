"""QA independente: confere integração/fixtures/scripts, NÃO compila nem executa Rust."""
from pathlib import Path
import json,hashlib,struct,re
root=Path(__file__).resolve().parents[1]
checks=[]
def check(name,condition):
    checks.append({'test':name,'pass':bool(condition)})
corpus=root/'TESTES_REGISTRY'
manifest=json.loads((corpus/'manifest.json').read_text())
for item in manifest:
    b=(corpus/item['name']).read_bytes()
    check('sha256_'+item['name'],hashlib.sha256(b).hexdigest()==item['sha256'] and len(b)==item['bytes'])
    if item['encoding']=='utf16':
        check('utf16_'+item['name'],b.startswith(b'\xff\xfe') and b[2:].decode('utf-16le').startswith('Windows Registry Editor Version 5.00'))
    else:
        check('utf8_'+item['name'],b.startswith(b'\xef\xbb\xbf') and b[3:].decode('utf8').startswith(('REGEDIT4','Windows Registry Editor Version 5.00')))
src=root/'RUST_APP/src'
for p in ('main.rs','history.rs','ui/mod.rs','ui/home.rs','ui/session_menu.rs','ui/session_picker.rs','ui/registry_compare.rs'):
    check('integration_'+p, 'Registry' in (src/p).read_text() or 'registry_compare' in (src/p).read_text())
for f in ('lib.rs','main.rs'):
    check('rust_registry_'+f,(root/'RUST_APP/crates/bc5-registry/src'/f).is_file())
lib=(root/'RUST_APP/crates/bc5-registry/src/lib.rs').read_text()
for marker in ('parse_reg','read_live_local','MAX_REG_BYTES','REG_EXPAND_SZ','REG_MULTI_SZ','REG_DWORD','REG_QWORD','REG_BINARY','fn reject_patches','fn line_continuation','fn parents_propagate'):
    check('feature_'+marker,marker in lib)
check('no_os_registry_writes','RegSetValue' not in lib and 'reg.exe").args(["export"' in lib)
check('cargo_dependency','bc5-registry = { path = "crates/bc5-registry" }' in (root/'RUST_APP/Cargo.toml').read_text())
check('official_chm',hashlib.sha256((root/'AJUDA_ORIGINAL_BC5/BCompare.chm').read_bytes()).hexdigest()=="c0ca49151bfa06e910d7610cb50240ac6f176644a72f23b56e6c5a8e0ae1ca8a")
check('golden_script',(root/'VALIDACAO_WINDOWS/Coletar_Golden_Registry_BC5.ps1').is_file())
result={'stage':'16-registry','checks':checks,'total':len(checks),'passed':sum(x['pass'] for x in checks),'rust_compiled':False,'bc5_a_b_executed':False}
(root/'QA_ETAPA16.json').write_text(json.dumps(result,indent=2,ensure_ascii=False))
print('QA auxiliar:',result['passed'],'/',result['total'])
if result['passed']!=result['total']:
 for item in checks:
    if not item['pass']:print('FAIL:',item['test'])
 raise SystemExit(1)
