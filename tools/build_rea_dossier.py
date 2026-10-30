"""Join authenticated PE resources, Delphi event tables and retained REA evidence.

This is a bounded static decoder for this admitted Delphi x64 artifact, not a
claim that its menus execute correctly. No target code or external process runs.
"""
from collections import Counter
from datetime import date
import hashlib
import json
from pathlib import Path
import struct

ROOT = Path(__file__).resolve().parents[1]
REF = ROOT / 'docs/reference'
OUT = ROOT / 'docs/rea'


def digest(data):
    return hashlib.sha256(data).hexdigest()


def load(path):
    return json.loads(path.read_text(encoding='utf-8-sig'))


def save(name, value):
    (OUT / name).write_text(json.dumps(value, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')


class DelphiImage:
    def __init__(self, artifact):
        self.data = Path(artifact['path']).read_bytes()
        if digest(self.data) != artifact['sha256']:
            raise ValueError('Reference artifact changed; obtain new REA evidence first.')
        pe = struct.unpack_from('<I', self.data, 60)[0]
        if self.data[pe:pe + 4] != b'PE\0\0' or self.u16(pe + 24) != 0x20b:
            raise ValueError('Only the observed PE32+ image is supported.')
        self.base = self.u64(pe + 48)
        count, optional_size = self.u16(pe + 6), self.u16(pe + 20)
        self.sections = []
        for i in range(count):
            row = struct.unpack_from('<8sIIIIIIHHI', self.data, pe + 24 + optional_size + i * 40)
            self.sections.append(dict(name=row[0].rstrip(b'\0').decode('ascii'),
                rva=row[2], raw=row[4], size=row[3], executable=bool(row[9] & 0x20000000)))
        self.classes = {}

    def u16(self, offset):
        return struct.unpack_from('<H', self.data, offset)[0]

    def u64(self, offset):
        return struct.unpack_from('<Q', self.data, offset)[0]

    def offset(self, address):
        rows = [s for s in self.sections if s['rva'] <= address - self.base < s['rva'] + s['size']]
        if len(rows) != 1:
            raise ValueError(f'Unmapped/ambiguous address: {address:#x}')
        return rows[0]['raw'] + address - self.base - rows[0]['rva']

    def address(self, offset):
        rows = [s for s in self.sections if s['raw'] <= offset < s['raw'] + s['size']]
        if len(rows) != 1:
            raise ValueError(f'Unmapped/ambiguous offset: {offset:#x}')
        return self.base + rows[0]['rva'] + offset - rows[0]['raw']

    def executable(self, address):
        return any(s['executable'] and s['rva'] <= address - self.base < s['rva'] + s['size'] for s in self.sections)

    def class_at(self, vmt):
        if vmt in self.classes:
            return self.classes[vmt]
        off = self.offset(vmt)
        if self.u64(off - 200) != vmt:
            raise ValueError('VMT self-pointer failed validation')
        name_ptr = self.u64(off - 136)
        no = self.offset(name_ptr)
        size = self.data[no]
        name = self.data[no + 1:no + 1 + size].decode('ascii')
        if not name.startswith('T') or not name.replace('_', '').isalnum():
            raise ValueError('Invalid VMT class name')
        row = dict(name=name, vmt=hex(vmt), vmt_file_offset=off,
            class_name_address=hex(name_ptr), methods=[], parent=None)
        self.classes[vmt] = row
        methods = self.u64(off - 152)
        if methods:
            cursor = self.offset(methods)
            row['method_table_address'] = hex(methods)
            count = self.u16(cursor)
            if count > 4096:
                raise ValueError('Method count exceeds bounded decoder budget')
            cursor += 2
            for _ in range(count):
                length = self.u16(cursor)
                address = self.u64(cursor + 2)
                name_length = self.data[cursor + 10]
                if length < 11 + name_length or not self.executable(address):
                    raise ValueError('Invalid method-table entry')
                method_name = self.data[cursor + 11:cursor + 11 + name_length].decode('ascii')
                row['methods'].append(dict(name=method_name, address=hex(address),
                    record_file_offset=cursor, record_bytes=length,
                    record_sha256=digest(self.data[cursor:cursor + length])))
                cursor += length
        parent_ref = self.u64(off - 120)
        if parent_ref:
            parent = self.u64(self.offset(parent_ref))
            parent_row = self.class_at(parent)
            row['parent'] = dict(name=parent_row['name'], vmt=hex(parent))
        return row

    def find_class(self, name):
        raw = name.encode('ascii')
        needle = bytes([len(raw)]) + raw
        cursor, candidates = 0, {}
        while True:
            cursor = self.data.find(needle, cursor)
            if cursor < 0:
                break
            pointer = struct.pack('<Q', self.address(cursor))
            ref = 0
            while True:
                ref = self.data.find(pointer, ref)
                if ref < 0:
                    break
                try:
                    vmt = self.u64(ref - 64)
                    if self.address(ref) + 136 == vmt:
                        row = self.class_at(vmt)
                        if row['name'] == name:
                            candidates[vmt] = row
                except (ValueError, UnicodeError, struct.error):
                    pass
                ref += 1
            cursor += 1
        return list(candidates.values())

    def handler(self, cls, event):
        seen = set()
        while cls and cls['vmt'] not in seen:
            seen.add(cls['vmt'])
            matches = [m for m in cls['methods'] if m['name'].casefold() == event.casefold()]
            if len(matches) == 1:
                return dict(**matches[0], declaring_class=cls['name'], vmt=cls['vmt'])
            cls = self.classes.get(int(cls['parent']['vmt'], 16)) if cls['parent'] else None
        return None


def walk(node, prefix=''):
    path = prefix + '/' + node['name']
    yield node, path
    for child in node.get('children', []):
        yield from walk(child, path)


def menu_nodes(values, path=''):
    result = []
    for value in values:
        if isinstance(value, str):
            name, separator, kind = value.partition(': ')
            if not separator:
                raise ValueError(f'Unexpected action-tree declaration: {value}')
            result.append(dict(name=name, type=kind, path=path + '/' + name,
                               order=len(result), children=[]))
        elif isinstance(value, list) and result:
            result[-1]['children'] = menu_nodes(value, result[-1]['path'])
        else:
            raise ValueError('Unattached nested action-tree list')
    return result


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    inventory = load(REF / 'ui-inventory.json')
    pe_evidence = load(REF / 'operation-rea-pe.json')
    if pe_evidence['normalized_result']['artifact'] != inventory['artifact']:
        raise ValueError('PE evidence and decoded inventory refer to different artifacts')
    image = DelphiImage(inventory['artifact'])
    forms = [load(p) for p in sorted((REF / 'forms').glob('*.json'))]
    coverage = load(ROOT / 'docs/review/coverage.json')
    assessments = {(a['form'], a['name']): a for a in coverage['actions']}
    retained, functions = [], []
    for path in sorted(REF.rglob('*.json')):
        if 'controlled-' in str(path) or path.name == 'ui-inventory.json':
            continue
        try:
            evidence = load(path)
            evidence = evidence.get('structuredContent', evidence)
            if not isinstance(evidence, dict) or not evidence.get('evidence_id'):
                continue
            row = dict(path=path.relative_to(ROOT).as_posix(), sha256=digest(path.read_bytes()),
                evidence_id=evidence['evidence_id'], operation=evidence.get('operation'), provider=evidence.get('provider'))
            retained.append(row)
            procedure = evidence.get('parameters', {}).get('procedure')
            if procedure:
                functions.append(dict(**row, procedure=procedure,
                    limitations=evidence.get('limitations', [])))
        except (ValueError, KeyError, AttributeError):
            continue
    by_address = {}
    for item in functions:
        by_address.setdefault(item['procedure'].lower(), []).append(item)
    events, trees, action_rows, unknowns = [], [], [], []
    for form in forms:
        root = form['hierarchy']
        matches = image.find_class(root['class'])
        cls = matches[0] if len(matches) == 1 else None
        form['native_class_candidates'] = [dict(name=c['name'], vmt=c['vmt']) for c in matches]
        if not cls:
            unknowns.append(dict(form=form['name'], question='Unique validated native VMT not resolved'))
        for node, path in walk(root):
            for prop, value in node['props'].items():
                if prop.startswith('On') and isinstance(value, str):
                    handler = image.handler(cls, value) if cls else None
                    row = dict(form=form['name'], resource_index=form['resource_index'],
                        resource_sha256=form['sha256'], component=path, component_class=node['class'],
                        event=prop, declared_handler=value, native_handler=handler,
                        classification='confirmed_static' if handler else 'unknown_native_binding',
                        runtime_verified=False)
                    row['rea_function_evidence'] = by_address.get(handler['address'], []) if handler else []
                    events.append(row)
            if node['class'] == 'TUiActionTree':
                trees.append(dict(form=form['name'], component=path,
                    resource_index=form['resource_index'], resource_sha256=form['sha256'],
                    nodes=menu_nodes(node['props'].get('Nodes', [])),
                    classification='confirmed_resource_declaration',
                    limitation='Runtime composition, platform filters and dynamic items are not established by this tree.'))
            if node['class'] == 'TUiAction':
                assessment = assessments.get((form['name'], node['name']), {})
                action_rows.append(dict(form=form['name'], component=path, name=node['name'],
                    props=node['props'], inherited=bool(node.get('flags', 0) & 1),
                    bindings=[e for e in events if e['form'] == form['name'] and e['component'] == path],
                    implementation=assessment.get('status', 'unknown'),
                    implementation_note=assessment.get('assessment'), source=assessment.get('source'),
                    functional_equivalence='not_proven'))
    summary = dict(date=str(date.today()), artifact=inventory['artifact'],
        pe_evidence_id=pe_evidence['evidence_id'], method='REA-authenticated resources + validated x64 Delphi method-table records + retained REA evidence',
        counts=dict(forms=len(forms), actions=len(action_rows), action_trees=len(trees),
            classes=len(image.classes), published_methods=sum(len(c['methods']) for c in image.classes.values()),
            event_declarations=len(events), native_handlers_resolved=sum(bool(e['native_handler']) for e in events),
            events_with_retained_rea_function=sum(bool(e['rea_function_evidence']) for e in events),
            retained_evidence=len(retained), function_evidence=len(functions)),
        state_counts=dict(Counter(a['implementation'] for a in action_rows)),
        limitations=['Only this hash/PE32+ Delphi layout is decoded.',
            'Event-table address resolution proves a static named binding, not execution or dynamic OnUpdate state.',
            'Inherited components can omit event/property values; do not treat omission as no handler.',
            'Action-tree declaration order is preserved; runtime merging/reordering is not yet reconstructed.',
            'Published method tables do not enumerate every native function.',
            'Presence in AnyCompare source is not behavioral equivalence.'])
    # Resolve action *declarations* through the verified native class ancestry.
    # A same-named Execute method is deliberately NOT treated as a binding.
    actions_by_form = {}
    form_by_class = {f['hierarchy']['class']: f['name'] for f in forms}
    class_by_name = {c['name']: c for c in image.classes.values()}
    for action in action_rows:
        actions_by_form.setdefault(action['form'], []).append(action)
    effective = {}
    for form in forms:
        chain, current = [], class_by_name.get(form['hierarchy']['class'])
        while current and current['name'] not in chain:
            chain.append(current['name'])
            current = class_by_name.get(current['parent']['name']) if current['parent'] else None
        merged = {}
        for name in reversed(chain):
            for action in actions_by_form.get(form_by_class.get(name), []):
                previous = merged.get(action['name'], {})
                props = {**previous.get('props', {}), **action['props']}
                merged[action['name']] = dict(**action, props=props) if not previous else {
                    **action, 'props': props,
                    'bindings': previous.get('bindings', []) + action['bindings']}
        effective[form['name']] = dict(class_ancestry=chain, actions=merged)

    def link_nodes(nodes, context, parent_id):
        for node in nodes:
            node['node_id'] = f"{parent_id}/{node['order']}:{node['name']}:{node['type']}"
            action_name = node['name'] + 'Action'
            command = context['actions'].get(action_name)
            if command and node['type'] in {'MenuItem', 'SubmenuItem', 'ToolButton', 'SubmenuButton'}:
                node['declared_action'] = command
                node['binding_status'] = 'action_identifier_matches_declared_tree_node'
                node['runtime_binding'] = 'unknown'
            link_nodes(node['children'], context, node['node_id'])

    for tree in trees:
        context = effective[tree['form']]
        tree['class_ancestry'] = context['class_ancestry']
        link_nodes(tree['nodes'], context, tree['form'] + tree['component'])
    unknowns += [dict(question=q) for q in [
        'Runtime composition of inherited action trees and conditional Windows menus.',
        'Dynamic binding between action declarations and view Execute/Update methods without an explicit DFM event.',
        'Enabled/checked/visible state transitions for each command and session type.',
        'Menus created from history, profiles, installed converters and remote backends.',
        'Pixel equivalence with ClearType and native controls at every supported DPI.',
        'Functional equivalence outside the two captured text comparison corpora.',
    ]]
    save('effective-actions.json', effective)
    save('manifest.json', summary)
    save('forms.json', forms)
    save('menus.json', trees)
    save('handlers.json', events)
    save('classes.json', list(image.classes.values()))
    save('functions.json', functions)
    save('comparison.json', action_rows)
    save('unknowns.json', unknowns)
    save('evidence-index.json', retained)
    print(json.dumps(summary, ensure_ascii=False, indent=2))


if __name__ == '__main__':
    main()
