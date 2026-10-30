"""Prepare read-only reference report inputs without changing the user's files."""
import hashlib, json
from pathlib import Path

root = Path(__file__).resolve().parents[1]
folder = root / 'docs/reference/controlled-full-replay'
folder.mkdir(parents=True, exist_ok=True)
state = root / '.codex/reference-runtime/BCState.xml'
if not state.exists(): state.write_bytes(b'')
sources = [Path(r'C:\Users\Altieres\Desktop\resumo.txt'), Path(r'C:\Users\Altieres\Desktop\resumo 1.txt')]
manifest = {'inputs': [], 'portable_state_present_before_spawn': state.exists(), 'portable_state_sha256': hashlib.sha256(state.read_bytes()).hexdigest(), 'limitations': ['Copies of the current user samples; a separate portable settings marker is created before each execution.']}
for side, source in zip(('left', 'right'), sources):
    original = source.read_bytes()
    target = folder / f'{side}.txt'
    target.write_bytes(original)
    digest = hashlib.sha256(original).hexdigest()
    assert hashlib.sha256(target.read_bytes()).hexdigest() == digest
    manifest['inputs'].append({'source': str(source), 'copy': str(target), 'sha256': digest, 'bytes': len(original)})
commands = []
for layout, extension, options in [('side-by-side', 'html', ' output-options:html-color'), ('xml', 'xml', '')]:
    commands.append(f'text-report layout:{layout} options:display-all output-to:"{folder}/report.{extension}"{options} "{folder}/left.txt" "{folder}/right.txt"')
(folder / 'report-script.txt').write_text('\r\n'.join(commands) + '\r\n', encoding='utf-8-sig', newline='')
(folder / 'inputs.json').write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')
print(json.dumps(manifest, ensure_ascii=False))
