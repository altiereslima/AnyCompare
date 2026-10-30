"""Recover UI definitions/assets from the exact PE inventory authenticated by REA.

No target code is executed. Re-run from any directory with Python 3.12+.
"""
import argparse
import hashlib
import json
from pathlib import Path
import re
import struct
from io import BytesIO

from PIL import Image

ROOT = Path(__file__).resolve().parents[1]


def sha(data):
    return hashlib.sha256(data).hexdigest()


def write_json(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


class DfmReader:
    """Delphi TPF0 reader, retaining byte locations for binary properties."""
    def __init__(self, data):
        self.data = data
        self.pos = 0

    def read(self, count):
        if count < 0 or self.pos + count > len(self.data):
            raise ValueError(f"truncated value at {self.pos}: requested {count}")
        result = self.data[self.pos:self.pos + count]
        self.pos += count
        return result

    def number(self, fmt):
        return struct.unpack('<' + fmt, self.read(struct.calcsize('<' + fmt)))[0]

    def short_string(self):
        return self.read(self.number('B')).decode('cp1252')

    def properties(self):
        props = {}
        while True:
            name = self.short_string()
            if not name:
                return props
            props[name] = self.value()

    def value(self):
        tag = self.number('B')
        if tag in (0, 13):
            return None
        if tag == 1:
            values = []
            while self.data[self.pos] != 0:
                values.append(self.value())
            self.read(1)
            return values
        if tag in (2, 3, 4, 19):
            return self.number({2: 'b', 3: 'h', 4: 'i', 19: 'q'}[tag])
        if tag == 5:
            return {'extended80_hex': self.read(10).hex()}
        if tag in (6, 7):
            return self.short_string()
        if tag in (8, 9):
            return tag == 9
        if tag == 10:
            length = self.number('I')
            offset = self.pos
            data = self.read(length)
            return {'binary': {'offset': offset, 'bytes': length, 'sha256': sha(data)}}
        if tag == 11:
            values = []
            while True:
                item = self.short_string()
                if not item:
                    return values
                values.append(item)
        if tag in (12, 18, 20):
            length = self.number('I')
            encoding = {12: 'cp1252', 18: 'utf-16-le', 20: 'utf-8'}[tag]
            return self.read(length * (2 if tag == 18 else 1)).decode(encoding)
        if tag == 14:
            items = []
            while self.data[self.pos] != 0:
                position = None
                if self.data[self.pos] != 1:
                    position = self.value()
                if self.number('B') != 1:
                    raise ValueError(f"expected collection item at {self.pos}")
                props = self.properties()
                if position is not None:
                    props['$position'] = position
                items.append(props)
            self.read(1)
            return items
        if tag == 15:
            return self.number('f')
        if tag == 16:
            return {'currency_scaled_10000': self.number('q')}
        if tag in (17, 21):
            return self.number('d')
        raise ValueError(f"unknown value type {tag} at {self.pos - 1}")

    def component(self):
        flags = 0
        child_position = None
        if self.data[self.pos] & 0xf0 == 0xf0:
            flags = self.number('B') & 0x0f
            if flags & 2:
                child_position = self.value()
        cls = self.short_string()
        name = self.short_string()
        if not cls:
            raise ValueError(f"empty component class at {self.pos}")
        props = self.properties()
        children = []
        while self.data[self.pos] != 0:
            children.append(self.component())
        self.read(1)
        result = {'class': cls, 'name': name, 'props': props, 'children': children}
        if flags:
            result['flags'] = flags
        if child_position is not None:
            result['child_position'] = child_position
        return result

    def parse(self):
        if self.read(4) != b'TPF0':
            raise ValueError('missing TPF0 signature')
        tree = self.component()
        if self.pos != len(self.data):
            raise ValueError(f"{len(self.data) - self.pos} unparsed trailing bytes")
        return tree


def walk(tree):
    yield tree
    for child in tree['children']:
        yield from walk(child)


def safe_name(value):
    return re.sub(r'[^A-Za-z0-9_.-]', '_', str(value))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--evidence', type=Path, default=ROOT / 'docs/reference/rea-pe-resources.json')
    args = parser.parse_args()
    evidence = json.loads(args.evidence.read_text(encoding='utf-8'))
    inventory = evidence['normalized_result']
    artifact = inventory['artifact']
    binary = Path(artifact['path']).read_bytes()
    if sha(binary) != artifact['sha256']:
        raise ValueError('reference executable differs from the REA inventory; inspect it again')
    forms, actions, icons, extracted, failures = [], [], [], [], []
    for resource in inventory['resources']:
        loc = resource['payload']['location']
        data = binary[loc['offset']:loc['offset'] + loc['bytes']]
        if len(data) != loc['bytes'] or sha(data) != resource['payload']['sha256']:
            raise ValueError(f"resource integrity mismatch: {resource['index']}")
        kind = resource['type'].get('name', str(resource['type'].get('id')))
        name = safe_name(resource['name'].get('name', resource['name'].get('id')))
        language = resource['language'].get('id', resource['language'].get('name'))
        # Preserve every resource, including translations, strings and version info.
        relative = Path('assets/reference/resources') / safe_name(kind) / f'{name}.{language}.bin'
        destination = ROOT / relative
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_bytes(data)
        extracted.append({'resource_index': resource['index'], 'path': relative.as_posix(), 'sha256': sha(data)})
        if kind == 'PNG':
            (ROOT / 'assets/png' / f'{name}.png').write_bytes(data)
        if not data.startswith(b'TPF0'):
            continue
        (ROOT / 'assets/forms' / f'{name}.tpf0').write_bytes(data)
        try:
            tree = DfmReader(data).parse()
            nodes = list(walk(tree))
            form = {'resource_index': resource['index'], 'name': name, 'sha256': sha(data),
                    'bytes': len(data), 'parse_status': 'complete', 'components': len(nodes), 'hierarchy': tree}
            write_json(ROOT / 'docs/reference/forms' / f'{name}.json', form)
            forms.append({k: v for k, v in form.items() if k != 'hierarchy'})
            for component in nodes:
                if component['class'].endswith('Action'):
                    actions.append({'form': name, **{k: component[k] for k in ('class', 'name', 'props')}})
                props = component['props']
                bitmap = props.get('ImagesBitmap.Data', {}).get('binary')
                names = props.get('ImageNames.Strings', [])
                if bitmap and names:
                    raw = data[bitmap['offset']:bitmap['offset'] + bitmap['bytes']]
                    bmp_start = raw.find(b'BM')
                    if bmp_start < 0:
                        raise ValueError('image list has no bitmap')
                    bmp = raw[bmp_start:]
                    pixel_offset = struct.unpack_from('<I', bmp, 10)[0]
                    width, signed_height = struct.unpack_from('<ii', bmp, 18)
                    bits = struct.unpack_from('<H', bmp, 28)[0]
                    if bits != 32:
                        raise ValueError(f'unsupported bitmap bit depth: {bits}')
                    sheet = Image.frombytes('RGBA', (width, abs(signed_height)), bmp[pixel_offset:],
                                            'raw', 'BGRA', width * 4, -1 if signed_height > 0 else 1)
                    cell_w, cell_h = props.get('Width', 16), props.get('Height', 16)
                    columns = sheet.width // cell_w
                    if columns < 1 or (sheet.height // cell_h) * columns < len(names):
                        raise ValueError(f'image list dimensions do not fit {len(names)} names')
                    scale = {'TBCIMAGES': '100', 'TBCIMAGES_125': '125', 'TBCIMAGES_150': '150',
                             'TBCIMAGES_200': '200'}.get(name, name)
                    for index, icon_name in enumerate(names):
                        x, y = (index % columns) * cell_w, (index // columns) * cell_h
                        icon = sheet.crop((x, y, x + cell_w, y + cell_h))
                        relative_icon = Path('assets/reference/icons') / scale / component['name'] / f'{safe_name(icon_name)}.png'
                        out = ROOT / relative_icon
                        out.parent.mkdir(parents=True, exist_ok=True)
                        icon.save(out)
                        icons.append({'name': icon_name, 'group': component['name'], 'scale': scale,
                                      'width': cell_w, 'height': cell_h, 'sha256': sha(out.read_bytes()),
                                      'form': name, 'property_offset': bitmap['offset'], 'image_index': index,
                                      'path': relative_icon.as_posix()})
                for item in component['props'].get('Images', []):
                    if not isinstance(item, dict):
                        continue
                    blob = item.get('PngImage.Data', {}).get('binary')
                    icon_name = item.get('Name')
                    if not blob or not icon_name:
                        continue
                    png = data[blob['offset']:blob['offset'] + blob['bytes']]
                    # TPNGObject prefixes the PNG with a little-endian payload size.
                    png_start = png.find(b'\x89PNG\r\n\x1a\n')
                    if png_start < 0:
                        raise ValueError(f'icon {icon_name} has no PNG signature')
                    png = png[png_start:]
                    scale = {'TBCIMAGES': '100', 'TBCIMAGES_125': '125', 'TBCIMAGES_150': '150', 'TBCIMAGES_200': '200'}.get(name, name)
                    folder = 'icons' if scale == '100' else f'icons_{scale}'
                    out = ROOT / 'assets' / folder
                    out.mkdir(parents=True, exist_ok=True)
                    (out / f'{safe_name(icon_name)}.png').write_bytes(png)
                    width, height = struct.unpack('>II', png[16:24])
                    icons.append({'name': icon_name, 'scale': scale, 'width': width, 'height': height,
                                  'sha256': sha(png), 'form': name, 'property_offset': blob['offset'],
                                  'path': (Path('assets') / folder / f'{safe_name(icon_name)}.png').as_posix()})
        except (ValueError, IndexError, UnicodeError, struct.error) as error:
            failures.append({'name': name, 'error': str(error), 'sha256': sha(data)})

    report = {'source_evidence_id': evidence['evidence_id'], 'artifact': artifact,
              'coverage': {'resource_payloads_verified': len(extracted), 'forms_parsed': len(forms),
                           'parse_failures': len(failures), 'action_definitions': len(actions),
                           'icon_variants': len(icons)},
              'resources': extracted, 'forms': forms, 'actions': actions, 'icons': icons, 'failures': failures,
              'limitations': ['Static resources describe design-time UI, not observed runtime behavior.',
                             'Action definitions and event handler names do not implement their behavior.',
                             'Inherited forms may omit base-class properties; runtime localization is separate.']}
    write_json(ROOT / 'docs/reference/ui-inventory.json', report)
    ico_dir = ROOT / 'assets/reference/ico'
    ico_dir.mkdir(parents=True, exist_ok=True)
    for group in inventory['icon_groups']:
        resource = inventory['resources'][group['resource_index']]
        icon_name = safe_name(resource['name'].get('name', resource['name'].get('id')))
        images = []
        for item in group['images']:
            image_index = item['same_language_resource_index']
            if image_index is None or not item['size_matches']:
                raise ValueError(f'ambiguous icon group {icon_name}')
            payload = inventory['resources'][image_index]['payload']
            loc = payload['location']
            images.append((item, binary[loc['offset']:loc['offset'] + loc['bytes']]))
        ico = bytearray(struct.pack('<HHH', 0, 1, len(images)))
        offset = 6 + 16 * len(images)
        for item, image in images:
            ico.extend(struct.pack('<BBBBHHII', item['width'], item['height'], item['color_count'],
                                   0, item['planes'], item['bit_count'], len(image), offset))
            offset += len(image)
        for _, image in images:
            ico.extend(image)
        (ico_dir / f'{icon_name}.ico').write_bytes(ico)
        if icon_name == 'MAINICON':
            Image.open(BytesIO(ico)).save(ico_dir / 'MAINICON.png')

    gallery_data = json.dumps(icons, ensure_ascii=False).replace('<', '\\u003c')
    gallery = '''<!doctype html><html lang="pt-BR"><meta charset="utf-8"><title>BC4 — ícones originais</title>
<style>body{font:14px Segoe UI,sans-serif;background:#f4f4f4;color:#222;margin:24px}header{position:sticky;top:0;background:#f4f4f4;padding:12px}input,select{font:inherit;padding:8px}main{display:grid;grid-template-columns:repeat(auto-fill,minmax(210px,1fr));gap:8px}article{background:white;padding:12px;min-height:100px;overflow-wrap:anywhere}img{object-fit:contain;max-width:128px;max-height:96px;display:block;margin:4px auto}small{color:#666}</style>
<header><h1>Beyond Compare 4.4.7 — ícones recuperados</h1><p>340 entradas em cada uma das quatro escalas. As listas HomeImages e HomeSmallImages são preservadas separadamente.</p><input id="q" placeholder="Nome ou lista de imagens"><select id="scale"><option>100</option><option>125</option><option>150</option><option>200</option></select><span id="count"></span></header><main id="grid"></main><script>
const icons=__ICONS__;const q=document.querySelector('#q'),scale=document.querySelector('#scale'),grid=document.querySelector('#grid');
function show(){grid.replaceChildren();const selected=icons.filter(i=>i.scale===scale.value&&(i.name+' '+i.group).toLowerCase().includes(q.value.toLowerCase()));document.querySelector('#count').textContent=' '+selected.length+' ícones';for(const i of selected){const card=document.createElement('article'),img=document.createElement('img'),name=document.createElement('div'),meta=document.createElement('small');img.src='../../'+i.path;img.alt=i.name;name.textContent=i.name;meta.textContent=i.group+' · '+i.width+' × '+i.height;card.append(img,name,meta);grid.append(card)}}q.oninput=scale.onchange=show;show();</script></html>'''.replace('__ICONS__', gallery_data)
    (ROOT / 'docs/reference/icon-gallery.html').write_text(gallery, encoding='utf-8')
    lines = ['# Mapa de formulários do Beyond Compare 4.4.7', '',
             f'Origem: REA `{evidence["evidence_id"]}`; SHA-256 `{artifact["sha256"]}`.', '',
             'Recursos estáticos extraídos e verificados. Os nomes de eventos não comprovam seu comportamento em execução.', '',
             '| Formulário | Componentes |', '| --- | ---: |']
    for form in forms:
        lines.append(f'| [{form["name"]}](forms/{form["name"]}.json) | {form["components"]} |')
    (ROOT / 'docs/reference/FORM_MAP.md').write_text('\n'.join(lines) + '\n', encoding='utf-8')
    catalog = ['// Generated by tools/inspect_reference.py from verified Delphi image lists.',
               'pub(super) const ICON_SOURCES: &[(&str, &str, u16, &[u8])] = &[']
    for icon in icons:
        catalog.append(f'    ("{icon["name"]}", "{icon["group"]}", {icon["scale"]}, '
                       f'include_bytes!("../../{icon["path"]}")),')
    catalog.append('];\n')
    (ROOT / 'src/ui/icon_catalog.rs').write_text('\n'.join(catalog), encoding='utf-8')
    print(json.dumps(report['coverage'], indent=2))
    for failure in failures:
        print(failure)
    if failures:
        raise SystemExit(1)


if __name__ == '__main__':
    main()
