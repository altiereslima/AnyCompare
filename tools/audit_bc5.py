"""Cross-check the isolated BC5 reference with REA + Craft, then inventory forms.

Inputs are observations, never instructions. This tool does not run the target,
change user profiles or overwrite the BC4 assets used by the current renderer.
"""
from collections import Counter
import hashlib
import json
from pathlib import Path
from inspect_reference import DfmReader, walk

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "docs/reference/bc5"


def load(path):
    data = path.read_bytes()
    return json.loads(data.decode("utf-16" if data.startswith(b"\xff\xfe") else "utf-8-sig"))


def main():
    rea = load(OUT / "runtime-rea.json")
    craft = load(ROOT / ".codex/bc5-runtime-craft.json")
    inv = rea["normalized_result"]
    binary = Path(inv["artifact"]["path"]).read_bytes()
    assert hashlib.sha256(binary).hexdigest() == inv["artifact"]["sha256"]
    expected = Counter((r["payload"]["location"]["offset"], r["payload"]["location"]["bytes"], r["payload"]["sha256"]) for r in inv["resources"])
    observed = Counter((r["file_offset"], r["size"], r["sha256"]) for r in craft["result"]["entries"])
    assert expected == observed, "Independent PE inventories disagree."
    forms, actions, failed = [], [], []
    for resource in inv["resources"]:
        payload = resource["payload"]
        loc = payload["location"]
        data = binary[loc["offset"]:loc["offset"] + loc["bytes"]]
        assert hashlib.sha256(data).hexdigest() == payload["sha256"]
        if not data.startswith(b"TPF0"):
            continue
        name = str(resource["name"].get("name", resource["name"].get("id")))
        try:
            tree = DfmReader(data).parse()
            forms.append(dict(name=name, sha256=payload["sha256"], components=sum(1 for _ in walk(tree))))
            folder = OUT / "forms"
            folder.mkdir(exist_ok=True, parents=True)
            (folder / (name + ".json")).write_text(json.dumps(tree, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
            for component in walk(tree):
                if component["class"].endswith("Action"):
                    actions.append(dict(form=name, **{k: component[k] for k in ["class", "name", "props"]}))
        except (ValueError, KeyError, IndexError) as error:
            failed.append(dict(name=name, error=str(error)))
    old = load(ROOT / "docs/reference/ui-inventory.json")
    old_actions = {(a["form"], a["name"]): a for a in old["actions"]}
    new_actions = {(a["form"], a["name"]): a for a in actions}
    delta = dict(
        added=[new_actions[k] for k in sorted(new_actions.keys() - old_actions.keys())],
        removed=[old_actions[k] for k in sorted(old_actions.keys() - new_actions.keys())],
        changed=[dict(before=old_actions[k], after=new_actions[k]) for k in sorted(old_actions.keys() & new_actions.keys()) if old_actions[k] != new_actions[k]],
    )
    summary = dict(
        version="5.2.7.32913", artifact_sha256=inv["artifact"]["sha256"],
        resources=len(inv["resources"]), independent_hash_location_checks=sum(expected.values()),
        forms=forms, form_failures=failed, actions=actions, delta=delta,
        limitations=["Resource/action presence is not functional parity.", "No licensing code was analyzed.", "Executable resources are English; Portuguese has a separate translation module."],
    )
    (OUT / "inventory.json").write_text(json.dumps(summary, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    (OUT / "craft-runtime-resources.json").write_text(json.dumps(craft, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(dict(resources=len(inv["resources"]), forms=len(forms), form_failures=failed, actions=len(actions), action_delta={k: len(v) for k, v in delta.items()})))


if __name__ == "__main__":
    main()
