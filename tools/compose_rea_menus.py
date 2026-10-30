"""Compose the effective TUiActionTree of each BC4 action module.

Reproduces the inherited-DFM merge observed statically in BCompare.exe 4.4.7.28397
(Ghidra/REA, 16/10/2026):

- TUiActionTree.DefineProperties (0xe31d50) registers "Nodes" with
  ReadData 0xe321d0 -> 0xe32000, WriteData 0xe323e0 and HasData 0xe31cb0.
- 0xe32000 reads the streamed list over the tree already built by the ancestor:
  each streamed node is looked up by (Name, Kind) with 0xe31120 (searching from
  the end); when absent it is inserted (0xe31ea0) right after the previous
  streamed node; container kinds (mask 0x273) recurse. Nothing is removed.
- Ancestor chain comes from the RTTI parent VMTs in docs/rea/classes.json.

After loading, TBaseActions.DataModuleCreate (0x19423a0) prunes the tree with
0x1942310: children are visited from last to first, a node is deleted when the
virtual predicate at VMT+0xa0 returns true, otherwise its children are visited.
Base predicate 0x1945280 (VMT+0xa0 of every module except TTextActions and
TDirSyncActions) deletes a node when:
  1. it needs an action (0x1945420: MenuItem, SubmenuItem, ToolButton, ComboBox,
     TreeButton, and SubmenuButton unless VMT+0xb0 accepts "<Name>DropDownBtn")
     and FindComponent("<Name>Action") fails;
  2. it is a Menu named MergeFile, Mac, MacTools or MacWindow;
  3. it is a Toolbar and VMT+0xc0 returns false (0x1945a40 -> 0 in TBaseActions
     and THomeActions; 0x10ba0d0 -> 1 from TSessionActions down).
TDirSyncActions (0x113af40) also deletes the ToolButton "Swap". TTextActions
(0x1193950) adds popup/button rules that depend on VMT+0xc8 -> 0x19662c0 and
VMT+0x178; they are reported as conditional, not applied.

Per DFM module ("modules"), rule 1 is only flagged ("action_not_in_dfm"), because
VMT+0x98 creates and frees actions at run time. Per concrete actions class
("views"), rule 1 uses the action set evaluated by tools/rea_eval_actions.py
(docs/rea/actions-effective.json), the TTextActions rules are applied, and every
node gets "default_visible" (MenuAllowed/ToolbarAllowed inherited along the DFM
chain plus the display-filter Category/mode rule; see DISPLAY_MODES below).
"default_layout" lists what the menu bar and toolbars show by default.

Inputs: docs/rea/menus.json, docs/rea/classes.json, docs/rea/forms.json,
docs/rea/actions-effective.json (optional).
Output: docs/rea/menus-composed.json.
"""

import hashlib
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
REA = ROOT / "docs" / "rea"

# TUiActionNodeKind (typeinfo at 0xe309e8), without the "actn" prefix.
KINDS = [
    "Root", "Menu", "MenuItem", "SeparatorItem", "SubmenuItem", "PopupMenu",
    "Toolbar", "ToolButton", "SeparatorButton", "SubmenuButton", "ComboBox",
    "TreeButton",
]
CONTAINER_MASK = 0x273  # Root, Menu, SubmenuItem, PopupMenu, Toolbar, SubmenuButton


def is_container(kind):
    k = KINDS.index(kind)
    return k < 16 and (1 << k) & CONTAINER_MASK != 0


def find_last(children, name, kind):
    # 0xe31120: iterates from Count-1 down to 0; name compare is exact (0x414dd0).
    for i in range(len(children) - 1, -1, -1):
        if children[i]["name"] == name and children[i]["type"] == kind:
            return i
    return -1


def read_into(existing, streamed, source):
    pos = 0
    for s in streamed:
        i = find_last(existing, s["name"], s["type"])
        if i < 0:
            existing.insert(pos, {"name": s["name"], "type": s["type"], "from": source, "children": []})
            i = pos
        if is_container(s["type"]):
            read_into(existing[i]["children"], s.get("children", []), source)
        elif s.get("children"):
            raise ValueError(f"non-container {s['name']} has streamed children")
        pos = i + 1


REMOVED_MENUS = {"MergeFile", "Mac", "MacTools", "MacWindow"}
NO_TOOLBAR = {"TBaseActions", "THomeActions"}  # VMT+0xc0 -> 0x1945a40 (returns 0)
DROPDOWN_NAMES = {"DisplayFiltersDropDownBtn"}  # 0x1945560
DROPDOWN_EXTRA = {"TDirActions": {"DirHandlingDropDownBtn"}}  # 0x1139a20
ACTION_KINDS = {"MenuItem", "SubmenuItem", "ToolButton", "ComboBox", "TreeButton"}
TEXT_CONDITIONAL = {  # 0x1193950; condition source still unknown
    ("SubmenuButton", "DisplayFilters"): "removed when 0x19662c0(VMT+0xc8) is false",
    ("PopupMenu", "Edit"): "removed when 0x19662c0(VMT+0xc8) is true",
    ("PopupMenu", "LeftEdit"): "removed when 0x19662c0(VMT+0xc8) is false",
    ("PopupMenu", "CenterEdit"): "removed when 0x19662c0(VMT+0xc8) is false",
    ("PopupMenu", "RightEdit"): "removed when 0x19662c0(VMT+0xc8) is false",
    ("PopupMenu", "OutputEdit"): "removed when 0x19662c0(VMT+0xc8) is false",
    ("PopupMenu", "InputDetailsEdit"): "removed when 0x19662c0(VMT+0xc8) is false",
    ("PopupMenu", "DetailsEdit"): "removed when VMT+0xc8 object's VMT+0x178 is true",
}


def action_name(module, node):
    # 0x1945420
    if node["type"] in ACTION_KINDS:
        return node["name"] + "Action"
    if node["type"] == "SubmenuButton":
        special = DROPDOWN_NAMES | DROPDOWN_EXTRA.get(module, set())
        if node["name"] + "DropDownBtn" in special:
            return None
        return node["name"] + "Action"
    return None


def remove_reason(module, node):
    if node["type"] == "Menu":
        return "removed-menu" if node["name"] in REMOVED_MENUS else None
    if node["type"] == "Toolbar" and module in NO_TOOLBAR:
        return "toolbar-not-allowed"
    if module == "TDirSyncActions" and node["type"] == "ToolButton" and node["name"] == "Swap":
        return "dirsync-swap"
    return None


def prune(module, children, actions, removed, path=""):
    # 0x1942310: last to first; delete or descend.
    for i in range(len(children) - 1, -1, -1):
        node = children[i]
        here = f"{path}/{node['name']}:{node['type']}"
        reason = remove_reason(module, node)
        if reason:
            removed.append({"path": here, "reason": reason})
            del children[i]
            continue
        act = action_name(module, node)
        if act is not None:
            node["action"] = act
            if act.lower() not in actions:
                node["action_not_in_dfm"] = True
        cond = TEXT_CONDITIONAL.get((node["type"], node["name"])) if module == "TTextActions" else None
        if cond:
            node["conditional"] = cond
        prune(module, node["children"], actions, removed, here)


def module_actions(forms, order):
    names = set()

    def walk(n):
        if n.get("class") == "TUiAction":
            names.add(n["name"].lower())
        for c in n.get("children", []):
            walk(c)

    for f in forms:
        cls = f["hierarchy"]["class"]
        if cls in order:
            walk(f["hierarchy"])
    return names


def chain(cls, classes):
    out = []
    while cls in classes:
        out.append(cls)
        parent = classes[cls].get("parent") or {}
        cls = parent.get("name")
    return list(reversed(out))


# Concrete actions classes (no DFM of their own) -> DFM module they inherit, and
# the session flags used by the TTextActions predicate 0x1193950:
# merge = 0x19662c0(session), edit = session VMT+0x178 (REA round 2).
VIEW_MODULE = {
    "THomeActions": ("THomeActions", None),
    "TDirCompareActions": ("TDirActions", None),
    "TDirMergeActions": ("TDirActions", None),
    "TDirSyncActions": ("TDirSyncActions", None),
    "TTextCompareActions": ("TTextActions", (False, False)),
    "TTextMergeActions": ("TTextActions", (True, False)),
    "TTextEditActions": ("TTextActions", (False, True)),
    "TTextPatchActions": ("TTextActions", (False, False)),
    "TDataCompareActions": ("TDataActions", None),
    "THexCompareActions": ("THexActions", None),
    "TMP3CompareActions": ("TMP3Actions", None),
    "TPixCompareActions": ("TPixActions", None),
    "TRegCompareActions": ("TRegActions", None),
    "TVerCompareActions": ("TVerActions", None),
}
TEXT_EDIT_POPUPS = {"LeftEdit", "CenterEdit", "RightEdit", "OutputEdit", "InputDetailsEdit"}

# Default visibility (BcCommands.pas, REA round 3). TUiAction RTTI: Category +0xb8,
# Visible +0x100, MenuAllowed +0x158 (default True), ToolbarAllowed +0x170 (default True).
# 0x1947c50/0x1947a10 copy MenuAllowed for MenuItem/SubmenuItem outside a SubmenuButton
# and ToolbarAllowed for every other node with an action (toolbar kinds and items inside a
# SubmenuButton). 0x19484a0 then shows a toolbar button only when that flag is set and its
# Category fits the display-filter mode (+0x5c: 0 DropDown, 1 Favorites, 2 Toggles):
# "DropDown" -> mode 1, "Favorites" -> mode 0 or 1, "Toggles" -> mode 2, others always;
# the DisplayFiltersDropDownBtn group is shown only in mode 0. Allowed modes (+0x5d):
# bit 0 when a SubmenuButton "DisplayFilters" exists, bit 1 for any DropDown/Favorites
# action, bit 2 for any Toggles action (0x1947f20); 0x1948f90 moves an unavailable mode
# to the next one (0 -> 1 -> 2 -> 0).
DISPLAY_MODES = ["DropDown", "Favorites", "Toggles"]
MENU_KINDS = {"MenuItem", "SubmenuItem"}


def action_props(forms, order):
    """Published TUiAction properties merged along the DFM chain (descendants override)."""
    props = {}

    def walk(n):
        if n.get("class") == "TUiAction":
            props.setdefault(n["name"].lower(), {}).update(n.get("props", {}))
        for c in n.get("children", []):
            walk(c)

    by_class = {f["hierarchy"]["class"]: f for f in forms}
    for cls in order:
        if cls in by_class:
            walk(by_class[cls]["hierarchy"])
    return props


def category_shown(category, mode):
    if category == "DropDown":
        return mode == 1
    if category == "Favorites":
        return mode in (0, 1)
    if category == "Toggles":
        return mode == 2
    return True


def display_modes(tree, props):
    allowed = 0

    def walk(nodes):
        nonlocal allowed
        for n in nodes:
            if n["type"] == "SubmenuButton" and n["name"] == "DisplayFilters":
                allowed |= 1
            act = n.get("action")
            cat = props.get(act.lower(), {}).get("Category") if act else None
            if cat in ("DropDown", "Favorites"):
                allowed |= 2
            elif cat == "Toggles":
                allowed |= 4
            walk(n["children"])

    walk(tree)
    mode = 0
    for _ in range(3):  # 0x1948f90 starting from the default DropDown style
        if allowed & (1 << mode):
            break
        mode = (mode + 1) % 3
    return allowed, mode


def mark_visibility(nodes, props, mode, in_button=False):
    """Annotate default_visible on every node; returns True when any child is visible."""
    any_visible = False
    for n in nodes:
        act = n.get("action")
        p = props.get(act.lower(), {}) if act else {}
        kind = n["type"]
        if kind in ("SeparatorItem", "SeparatorButton"):
            vis = None  # decided after siblings (separator reduction, INFERIDO)
        elif kind in MENU_KINDS and not in_button:
            vis = p.get("MenuAllowed", True) if act else True
            if kind == "SubmenuItem":
                vis = mark_visibility(n["children"], props, mode) and vis
        elif kind == "SubmenuButton" and not act:
            shown = mark_visibility(n["children"], props, mode, True)
            vis = shown and (n["name"] != "DisplayFilters" or mode == 0)
        elif act:
            vis = p.get("ToolbarAllowed", True)
            if not in_button:  # items of a DropDownBtn group skip the Category test
                vis = vis and category_shown(p.get("Category"), mode)
            if n["children"]:
                mark_visibility(n["children"], props, mode, kind == "SubmenuButton" or in_button)
        else:
            vis = mark_visibility(n["children"], props, mode, in_button) or kind in ("Menu", "PopupMenu", "Toolbar")
        n["default_visible"] = vis
        any_visible = any_visible or bool(vis)
    # Separator reduction: a separator shows only between visible items (no leading,
    # trailing or doubled separators). Not located in code; the resulting toolbars match
    # the original captures docs/reference/screenshots/{folder-empty,folder-sync,text-loaded}.png.
    last = None
    for n in nodes:
        if n["type"] in ("SeparatorItem", "SeparatorButton"):
            n["default_visible"] = False
            if last == "item":
                last = n
        elif n["default_visible"]:
            if last not in (None, "item"):
                last["default_visible"] = True
            last = "item"
    return any_visible


def visible_layout(nodes):
    out = []
    for n in nodes:
        if not n.get("default_visible"):
            continue
        if n["type"] in ("SeparatorItem", "SeparatorButton"):
            out.append("|")
        elif n["type"] == "SubmenuButton" and not n.get("action"):
            out.append(n["name"] + "[" + " ".join(visible_layout(n["children"])) + "]")
        else:
            out.append(n["name"])
    return out


def text_rule(flags, node):
    """0x1193950 after the base predicate; flags = (merge, edit)."""
    if flags is None:
        return None
    merge, edit = flags
    kind, name = node["type"], node["name"]
    if not merge and kind == "SubmenuButton" and name == "DisplayFilters":
        return "text-compare-no-display-filters"
    if merge and kind == "PopupMenu" and name == "Edit":
        return "text-merge-no-edit-popup"
    if not merge and kind == "PopupMenu" and name in TEXT_EDIT_POPUPS:
        return "text-compare-no-pane-popups"
    if edit and kind == "PopupMenu" and name in ("DetailsEdit", "Nav"):
        return "text-edit-no-details"
    return None


def prune_view(view, module, flags, children, effective, removed, path=""):
    """Full DataModuleCreate prune (0x1942310) for one concrete actions class."""
    for i in range(len(children) - 1, -1, -1):
        node = children[i]
        here = f"{path}/{node['name']}:{node['type']}"
        act = action_name(module, node)
        reason = None
        if act is not None and act.lower() not in effective:
            reason = "missing-action"
        reason = reason or remove_reason(module, node) or text_rule(flags, node)
        if reason:
            removed.append({"path": here, "reason": reason})
            del children[i]
            continue
        if act is not None:
            node["action"] = act
        prune_view(view, module, flags, node["children"], effective, removed, here)


def compose_views(modules_tree, effective_doc, forms, order_of):
    views = []
    for view, (module, flags) in VIEW_MODULE.items():
        info = effective_doc["classes"][view]
        effective = {a.lower() for a in info["effective_actions"]}
        tree = json.loads(json.dumps(modules_tree[module]))
        removed = []
        prune_view(view, module, flags, tree, effective, removed)
        props = action_props(forms, order_of[module])
        allowed, mode = display_modes(tree, props)
        mark_visibility(tree, props, mode)
        layout = {}
        for n in tree:
            if n["type"] in ("Menu", "Toolbar"):
                layout[f"{n['name']}:{n['type']}"] = visible_layout(n["children"])
        views.append({
            "view": view,
            "session_class": info["session_class"],
            "module": module,
            "load_chain": info["load_chain"],
            "windows_top_level": [{"name": n["name"], "type": n["type"]} for n in tree],
            "display_filter_modes": [m for i, m in enumerate(DISPLAY_MODES) if allowed & (1 << i)],
            "default_display_filter_mode": DISPLAY_MODES[mode],
            "default_layout": layout,
            "removed": removed,
            "nodes": tree,
        })
    return views


def main():
    menus = json.loads((REA / "menus.json").read_text(encoding="utf-8"))
    classes = {c["name"]: c for c in json.loads((REA / "classes.json").read_text(encoding="utf-8"))}
    forms = json.loads((REA / "forms.json").read_text(encoding="utf-8"))
    by_class = {}
    for m in menus:
        cls = "T" + m["component"].split("/")[1]
        by_class[cls] = m

    result = []
    merged_trees = {}
    order_of = {}
    for cls, m in sorted(by_class.items()):
        order = [c for c in chain(cls, classes) if c in by_class]
        order_of[cls] = order
        tree = []
        for c in order:
            read_into(tree, by_class[c]["nodes"], c)
        merged_trees[cls] = json.loads(json.dumps(tree))
        merged_top = [{"name": n["name"], "type": n["type"], "from": n["from"]} for n in tree]
        actions = module_actions(forms, order)
        removed = []
        prune(cls, tree, actions, removed)
        result.append({
            "module": cls,
            "load_order": order,
            "merged_top_level": merged_top,
            "windows_top_level": [{"name": n["name"], "type": n["type"], "from": n["from"]} for n in tree],
            "removed": removed,
            "nodes": tree,
        })

    doc = {
        "target": {
            "path": "C:\\Projetos\\BC4\\bc4\\BCompare.exe",
            "sha256": "aba1b98f848a01c3729cfee3e8818ab8e0d947276bc42c9a55c1475c9f8438db",
        },
        "rule": (
            "inherited-DFM merge of TUiActionTree.Nodes by (Name, Kind), insert after previous streamed node; "
            "then DataModuleCreate prune via VMT+0xa0 (see tools/compose_rea_menus.py docstring)"
        ),
        "classification": (
            "CONFIRMADO ESTATICAMENTE (merge algorithm; prune rules 2-3 and DirSync Swap) applied to DFM data; "
            "prune rule 1 (missing action) is flagged, not applied, because VMT+0x98 creates/frees actions at "
            "runtime; TTextActions conditional rules, action Visible/Enabled state and user toolbar "
            "customisation are not modelled"
        ),
        "inputs": {
            p: hashlib.sha256((REA / p).read_bytes()).hexdigest()
            for p in ("menus.json", "classes.json", "forms.json")
        },
        "modules": result,
    }
    effective_path = REA / "actions-effective.json"
    if effective_path.exists():
        effective_doc = json.loads(effective_path.read_text(encoding="utf-8"))
        doc["views"] = compose_views(merged_trees, effective_doc, forms, order_of)
        doc["inputs"]["actions-effective.json"] = hashlib.sha256(effective_path.read_bytes()).hexdigest()
        doc["views_classification"] = (
            "CONFIRMADO ESTATICAMENTE: full prune per concrete actions class (rule 1 with the action set "
            "evaluated by tools/rea_eval_actions.py, rules 2-3, DirSync Swap, TTextActions popup rules) and "
            "default_visible from MenuAllowed/ToolbarAllowed inherited along the DFM chain plus the "
            "display-filter Category/mode rule (0x1947c50, 0x1947a10, 0x19484a0, 0x1947f20, 0x1948f90). "
            "Separator reduction and the starting DropDown style (help: display_filters.html) are not located "
            "in code; the Main toolbars of TDirCompareActions, TDirSyncActions and TTextCompareActions match "
            "the original captures folder-empty.png, folder-sync.png and text-loaded.png item by item "
            "(CONFIRMADO EM EXECUÇÃO for those three views, INFERIDO for the others). "
            "Assumes Pro edition, normal (non-expired) state and no user toolbar customisation; "
            "Visible/Enabled updates at run time are not modelled"
        )
        for v in doc["views"]:
            bar = [t["name"] for t in v["windows_top_level"] if t["type"] == "Menu"]
            popups = [t["name"] for t in v["windows_top_level"] if t["type"] == "PopupMenu"]
            print(f"view {v['view']:21} menus={bar} popups={popups} removed={len(v['removed'])} "
                  f"modes={v['display_filter_modes']} default={v['default_display_filter_mode']}")
            for k, items in v["default_layout"].items():
                if k.endswith(":Toolbar"):
                    print(f"{'':26}{k}: {' '.join(items)}")
    for r in result:
        bar = [t["name"] for t in r["windows_top_level"] if t["type"] == "Menu"]
        bars = [t["name"] for t in r["windows_top_level"] if t["type"] == "Toolbar"]
        reasons = {}
        for x in r["removed"]:
            reasons[x["reason"]] = reasons.get(x["reason"], 0) + 1
        flagged = []

        def count(nodes):
            for n in nodes:
                if n.get("action_not_in_dfm"):
                    flagged.append(n["name"])
                count(n["children"])

        count(r["nodes"])
        r["action_not_in_dfm_count"] = len(flagged)
        print(f"{r['module']:16} {' > '.join(r['load_order'])}")
        print(f"{'':16} menus: {bar} toolbars: {bars} removed: {reasons} action_not_in_dfm: {len(flagged)}")
    out = REA / "menus-composed.json"
    out.write_text(json.dumps(doc, ensure_ascii=False, indent=1) + "\n", encoding="utf-8")
    return 0


if __name__ == "__main__":
    sys.exit(main())
