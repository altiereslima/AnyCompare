"""Add translation boundaries to UI captions, never to document buffers/icons/IDs."""
from pathlib import Path
import re
ROOT=Path(__file__).resolve().parents[1]
CALLS=re.compile(r'\.(button|small_button|menu_button|label|weak|colored_label|checkbox|radio_value|selectable_value|selectable_label|on_hover_text|hint_text)\s*\(|\b(Button|CollapsingHeader|RichText)::new\s*\(')
STRINGS=re.compile(r'"(?:\\.|[^"\\])*"')

def args(text,start):
    stack=['(']; spans=[];at=start;quote=False;escape=False
    for i in range(start,len(text)):
        c=text[i]
        if quote:
            if escape:escape=False
            elif c=='\\':escape=True
            elif c=='"':quote=False
            continue
        if c=='"':quote=True;continue
        if c in '([{':stack.append(c)
        elif c in ')]}':
            stack.pop()
            if not stack:spans.append((at,i));return spans
        elif c==',' and len(stack)==1:spans.append((at,i));at=i+1
    raise ValueError('Unbalanced UI call')

for path in list((ROOT/'src/ui').glob('*.rs'))+[ROOT/'src/main.rs']:
    if path.name in ('icon_catalog.rs','icons.rs','aligned_editor.rs','native_prompt.rs','native_files.rs','theme.rs','dialogs.rs','text_view.rs','chrome.rs'):continue
    original=path.read_text(encoding='utf-8');cut=original.find('#[cfg(test)]\nmod tests');source=original[:cut] if cut>=0 else original
    masked=list(source)
    for string in STRINGS.finditer(source):masked[string.start():string.end()]=' '*(string.end()-string.start())
    changes=[]
    for call in CALLS.finditer(''.join(masked)):
        name=call.group(1) or call.group(2);arguments=args(source,call.end())
        index=1 if name in ('checkbox','colored_label','selectable_label') else 2 if name in ('radio_value','selectable_value') else 0
        if index>=len(arguments):continue
        start,end=arguments[index];value=source[start:end].strip()
        if value.startswith('crate::i18n::tr') or value.startswith('RichText::'):continue
        literal=bool(STRINGS.fullmatch(value))
        if name in ('label','weak','RichText','selectable_value','selectable_label') and not (literal or value.startswith('format!')):continue
        changes.append((start,end,'crate::i18n::tr(&('+value+'))'))
    for start,end,value in sorted(changes,reverse=True):source=source[:start]+value+source[end:]
    if cut>=0:source+=original[cut:]
    if source!=original:path.write_text(source,encoding='utf-8');print(path.name,len(changes))
