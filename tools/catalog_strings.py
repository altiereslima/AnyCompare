import json,re
from pathlib import Path
root=Path(__file__).resolve().parents[1]
keys=set()
for path in list((root/'src/ui').glob('*.rs'))+[root/'src/main.rs',root/'src/document.rs',root/'src/report.rs',root/'src/file_ops.rs']:
    if path.name in ('icon_catalog.rs','aligned_editor.rs','icons.rs','native_prompt.rs','native_files.rs'):continue
    text=path.read_text(encoding='utf-8').split('#[cfg(test)]\nmod tests')[0]
    for match in re.finditer(r'"(?:\\.|[^"\\])*"',text):
        try:value=json.loads(match.group())
        except (ValueError,json.JSONDecodeError):continue
        if value.startswith(('img','tb_')):continue
        if any(c.isupper() or c in 'áãçéíóúêôõ' for c in value):keys.add(value)
print(json.dumps(sorted(keys),ensure_ascii=False,indent=2))
