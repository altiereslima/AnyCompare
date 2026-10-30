"""Generate the default toolbars of every BC4 view from the REA menu composition.

Inputs (all read-only):
- docs/rea/menus-composed.json: per concrete actions class, the pruned TUiActionTree with
  default_visible (MenuAllowed/ToolbarAllowed + display-filter style, REA round 3);
- docs/rea/forms.json + docs/rea/classes.json: English DFM action properties along the chain;
- C:\\Projetos\\BC4\\bc4\\BCompare.tr: the pt-BR translated DFMs (UTF-8 short strings).

Outputs:
- spec/toolbars.json: the toolbar items with captions, hints and image names;
- src/ui/toolbar_spec.rs: the same data as Rust constants (cargo fmt afterwards);
- assets/locales/en.json: pt-BR -> English pairs missing for those captions and hints.

Caption on a toolbar button: ShortCaption when present, else Caption without '&' and a
trailing "...". The hint is the part of Hint before '|'.
"""

import hashlib
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
REA = ROOT / "docs" / "rea"
TR = Path(r"C:\Projetos\BC4\bc4\BCompare.tr")
TR_SHA256 = None  # recorded in spec/toolbars.json

sys.path.insert(0, str(ROOT / "tools"))
from inspect_reference import DfmReader, walk  # noqa: E402

# Views the app renders today; names are the concrete actions classes.
VIEWS = {
    "TEXT_COMPARE": "TTextCompareActions",
    "FOLDER_COMPARE": "TDirCompareActions",
    "FOLDER_MERGE": "TDirMergeActions",
    "FOLDER_SYNC": "TDirSyncActions",
    "TEXT_MERGE": "TTextMergeActions",
    "TEXT_EDIT": "TTextEditActions",
    "TEXT_PATCH": "TTextPatchActions",
    "TABLE_COMPARE": "TDataCompareActions",
    "HEX_COMPARE": "THexCompareActions",
    "MP3_COMPARE": "TMP3CompareActions",
    "PICTURE_COMPARE": "TPixCompareActions",
    "REGISTRY_COMPARE": "TRegCompareActions",
    "VERSION_COMPARE": "TVerCompareActions",
}


def fix_utf8(text):
    # DfmReader decodes short strings as cp1252; BCompare.tr stores UTF-8 bytes.
    if not isinstance(text, str):
        return text
    try:
        return text.encode("cp1252").decode("utf-8")
    except (UnicodeEncodeError, UnicodeDecodeError):
        return text


def translated_forms():
    data = TR.read_bytes()
    forms = {}
    for m in re.finditer(b"TPF0", data):
        reader = DfmReader(data[m.start():])
        reader.read(4)
        tree = reader.component()
        forms[tree["class"]] = tree
    return forms, hashlib.sha256(data).hexdigest()


def chain(cls, classes):
    out = []
    while cls in classes:
        out.append(cls)
        cls = (classes[cls].get("parent") or {}).get("name")
    return list(reversed(out))


def merged_actions(order, forms_by_class, fix=False):
    props = {}
    for cls in order:
        tree = forms_by_class.get(cls)
        if tree is None:
            continue
        for node in walk(tree):
            if node.get("class") == "TUiAction":
                p = props.setdefault(node["name"].lower(), {})
                for k, v in node.get("props", {}).items():
                    p[k] = fix_utf8(v) if fix else v
    return props


def caption_of(p):
    short = p.get("ShortCaption")
    if short:
        return short
    cap = p.get("Caption") or ""
    cap = cap.replace("&&", "\0").replace("&", "").replace("\0", "&")
    return re.sub(r"\.\.\.$", "", cap).strip()


def menu_caption_of(p):
    cap = p.get("Caption") or p.get("ShortCaption") or ""
    return cap.replace("&&", "\0").replace("&", "").replace("\0", "&").strip()


def hint_of(p):
    return (p.get("Hint") or "").split("|")[0].strip()


def item(node, en, pt, in_group=False):
    kind = node["type"]
    act = node.get("action")
    if kind == "SeparatorButton":
        return {"kind": "Separator", "name": node["name"]}
    pe = en.get(act.lower(), {}) if act else {}
    pp = pt.get(act.lower(), {}) if act else {}
    side = False
    if act and act.endswith("SideAction") and not caption_of(pe):
        # "<X>Side" has no caption/image in the DFM; at run time it mirrors "<X>Right" or
        # "<X>Left" toward the inactive pane (INFERIDO; text-loaded.png shows "Copiar").
        right = act[: -len("SideAction")] + "RightAction"
        pe = {**en.get(right.lower(), {}), **{k: v for k, v in pe.items() if k == "Hint"}}
        pp = {**pt.get(right.lower(), {}), **{k: v for k, v in pp.items() if k == "Hint"}}
        side = True
    out = {
        "name": node["name"],
        "action": act,
        "kind": {
            "ToolButton": "Button",
            "TreeButton": "Tree",
            "ComboBox": "Combo",
            "MenuItem": "Item",
            "SubmenuButton": "Dropdown" if act else "Group",
        }[kind] if not side else "Side",
        "image": pe.get("ImageIndex") or "",
        "caption_en": (menu_caption_of if in_group else caption_of)(pe),
        "caption_pt": (menu_caption_of if in_group else caption_of)(pp) or (menu_caption_of if in_group else caption_of)(pe),
        "short_pt": caption_of(pp) or caption_of(pe),
        "short_en": caption_of(pe),
        "hint_en": hint_of(pe),
        "hint_pt": hint_of(pp) or hint_of(pe),
        "category": pe.get("Category") or "",
    }
    if kind == "SubmenuButton":
        out["children"] = [
            item(c, en, pt, True) for c in node["children"] if c.get("default_visible") and c["type"] != "SeparatorItem"
        ]
    return out


def main():
    sys.stdout.reconfigure(encoding="utf-8")
    composed = json.loads((REA / "menus-composed.json").read_text(encoding="utf-8"))
    classes = {c["name"]: c for c in json.loads((REA / "classes.json").read_text(encoding="utf-8"))}
    forms = {f["hierarchy"]["class"]: f["hierarchy"] for f in json.loads((REA / "forms.json").read_text(encoding="utf-8"))}
    tr_forms, tr_sha = translated_forms()
    views = {v["view"]: v for v in composed["views"]}
    spec = {
        "generated_by": "tools/build_toolbar_spec.py",
        "inputs": {
            "menus-composed.json": hashlib.sha256((REA / "menus-composed.json").read_bytes()).hexdigest(),
            "BCompare.tr": tr_sha,
        },
        "geometry": {
            "classification": "CONFIRMADO EM EXECUÇÃO (folder-empty.png, folder-sync.png, text-loaded.png a 100%)",
            "first_button_left": 2,
            "button_height": 36,
            "button_width": "max(45, GDI advance of the caption in Segoe UI 9pt + 12)",
            "dropdown_part": 14,
            "tree_dropdown_part": 15,
            "separator_width": 6,
            "separator_line_offset": 2,
        },
        "views": {},
    }
    for const, cls in VIEWS.items():
        view = views[cls]
        order = chain(view["module"], classes)
        en = merged_actions(order, forms)
        pt = merged_actions(order, tr_forms, fix=True)
        bar = next(n for n in view["nodes"] if n["type"] == "Toolbar" and n["name"] == "Main")
        items = []
        for node in bar["children"]:
            if node.get("default_visible"):
                items.append(item(node, en, pt))
        spec["views"][const] = {
            "actions_class": cls,
            "display_filter_mode": view["default_display_filter_mode"],
            "items": items,
        }
    out = ROOT / "spec" / "toolbars.json"
    out.parent.mkdir(exist_ok=True)
    out.write_text(json.dumps(spec, ensure_ascii=False, indent=1) + "\n", encoding="utf-8")
    write_rust(spec)
    add_translations(spec)
    for const, v in spec["views"].items():
        line = " ".join("|" if i["kind"] == "Separator" else i["caption_pt"] + ("▼" if i["kind"] in ("Dropdown", "Tree", "Group") else "") for i in v["items"])
        print(f"{const:17} {line}")
    return 0


def rust_str(s):
    return json.dumps(s, ensure_ascii=False)


def rust_item(i, indent):
    pad = " " * indent
    if i["kind"] == "Separator":
        return f"{pad}ToolItem::SEPARATOR,"
    children = "&[]"
    if i.get("children"):
        inner = "\n".join(rust_item(c, indent + 8) for c in i["children"])
        children = f"&[\n{inner}\n{pad}    ]"
    return (
        f"{pad}ToolItem {{\n"
        f"{pad}    name: {rust_str(i['name'])},\n"
        f"{pad}    kind: ToolKind::{i['kind']},\n"
        f"{pad}    image: {rust_str(i['image'])},\n"
        f"{pad}    caption: {rust_str(i['caption_pt'])},\n"
        f"{pad}    short: {rust_str(i['short_pt'])},\n"
        f"{pad}    hint: {rust_str(i['hint_pt'])},\n"
        f"{pad}    children: {children},\n"
        f"{pad}}},"
    )


def write_rust(spec):
    lines = [
        "//! Default toolbars of each view, generated by tools/build_toolbar_spec.py from",
        "//! docs/rea/menus-composed.json and BCompare.tr (pt-BR). Do not edit by hand.",
        "",
        "use super::toolbar::{ToolItem, ToolKind};",
        "",
    ]
    for const, v in spec["views"].items():
        lines.append(f"/// `{v['actions_class']}` (display filter style {v['display_filter_mode']}).")
        lines.append(f"#[allow(dead_code)]")
        lines.append(f"pub const {const}: &[ToolItem] = &[")
        for i in v["items"]:
            lines.append(rust_item(i, 4))
        lines.append("];")
        lines.append("")
    (ROOT / "src" / "ui" / "toolbar_spec.rs").write_text("\n".join(lines), encoding="utf-8")


def add_translations(spec):
    path = ROOT / "assets" / "locales" / "en.json"
    catalog = json.loads(path.read_text(encoding="utf-8"))
    tr = catalog["translations"]
    added = 0

    def put(pt, en):
        nonlocal added
        if pt and en and pt not in tr:
            tr[pt] = en
            added += 1

    def visit(i):
        if i["kind"] == "Separator":
            return
        put(i["caption_pt"], i["caption_en"])
        put(i["short_pt"], i["short_en"])
        put(i["hint_pt"], i["hint_en"])
        for c in i.get("children", []):
            visit(c)

    for v in spec["views"].values():
        for i in v["items"]:
            visit(i)
    path.write_text(json.dumps(catalog, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(f"en.json: {added} translations added")


if __name__ == "__main__":
    sys.exit(main())
