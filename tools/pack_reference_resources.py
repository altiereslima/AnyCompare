"""Build and independently verify the application's authenticated resource bundle."""
import argparse
import hashlib
import json
from pathlib import Path
import struct

ROOT = Path(__file__).resolve().parents[1]
MAGIC = b"ACRSv1\0\0"
DESTINATION = ROOT / "assets/reference/ui-resources.pack"


def digest(data):
    return hashlib.sha256(data).hexdigest()


def verify(data):
    if data[:8] != MAGIC:
        raise ValueError("Invalid resource bundle signature")
    size = struct.unpack_from("<Q", data, 8)[0]
    manifest = json.loads(data[16:16 + size])
    payload = data[16 + size:]
    for entry in manifest["resources"] + manifest["icons"]:
        value = payload[entry["offset"]:entry["offset"] + entry["bytes"]]
        if len(value) != entry["bytes"] or digest(value) != entry["sha256"]:
            raise ValueError(f"Corrupt resource: {entry['name']}")
    indices = [entry["index"] for entry in manifest["resources"]]
    if sorted(indices) != list(range(729)):
        raise ValueError("Expected exactly the 729 original resource identities")
    reference = json.loads((ROOT / "docs/reference/rea-pe-resources.json").read_text(encoding="utf-8"))["normalized_result"]
    if manifest["reference_sha256"] != reference["artifact"]["sha256"]:
        raise ValueError("Wrong reference executable identity")
    for entry, expected in zip(manifest["resources"], reference["resources"], strict=True):
        if entry["index"] != expected["index"] or entry["sha256"] != expected["payload"]["sha256"]:
            raise ValueError("Bundle differs from authenticated reference inventory")
        for field, key in [("kind", "type"), ("name", "name")]:
            expected_key = expected[key].get("name", "#" + str(expected[key].get("id")))
            if entry[field] != expected_key:
                raise ValueError("Resource identity differs from reference")
        if entry["language"] != expected["language"]["id"]:
            raise ValueError("Resource language differs from reference")
    return manifest


def build():
    inventory = json.loads((ROOT / "docs/reference/ui-inventory.json").read_text(encoding="utf-8"))
    evidence = json.loads((ROOT / "docs/reference/rea-pe-resources.json").read_text(encoding="utf-8"))["normalized_result"]
    payload = bytearray()

    def append(data, expected):
        if digest(data) != expected:
            raise ValueError("Source resource digest mismatch")
        result = {"offset": len(payload), "bytes": len(data), "sha256": expected}
        payload.extend(data)
        return result

    def key(value):
        return value["name"] if value["kind"] == "name" else "#" + str(value["id"])

    resources = []
    strings = []
    for resource, extracted in zip(evidence["resources"], inventory["resources"], strict=True):
        if resource["index"] != extracted["resource_index"]:
            raise ValueError("Resource order mismatch")
        data = (ROOT / extracted["path"]).read_bytes()
        resources.append({"index": resource["index"], "kind": key(resource["type"]),
                          "name": key(resource["name"]), "language": resource["language"]["id"],
                          **append(data, resource["payload"]["sha256"])})
        if resource["type"].get("id") == 6:
            cursor = 0
            for slot in range(16):
                length = struct.unpack_from("<H", data, cursor)[0]
                cursor += 2
                text = data[cursor:cursor + length * 2].decode("utf-16-le")
                cursor += length * 2
                if text:
                    strings.append({"id": (resource["name"]["id"] - 1) * 16 + slot,
                                    "language": resource["language"]["id"], "text": text})
    icons = [{"name": icon["name"], "group": icon["group"], "scale": int(icon["scale"]),
              **append((ROOT / icon["path"]).read_bytes(), icon["sha256"])} for icon in inventory["icons"]]
    forms = [json.loads((ROOT / "docs/reference/forms" / (form["name"] + ".json")).read_text(encoding="utf-8"))
             for form in inventory["forms"]]
    from inspect_reference import DfmReader
    for form in forms:
        source = resources[form["resource_index"]]
        data = payload[source["offset"]:source["offset"] + source["bytes"]]
        if DfmReader(data).parse() != form["hierarchy"]:
            raise ValueError(f"Form differs from original resource: {form['name']}")
    manifest = {"version": 1, "reference_sha256": evidence["artifact"]["sha256"],
                "resources": resources, "icons": icons, "forms": forms, "strings": strings}
    encoded = json.dumps(manifest, ensure_ascii=False, separators=(",", ":")).encode("utf-8")
    bundle = MAGIC + struct.pack("<Q", len(encoded)) + encoded + payload
    verify(bundle)
    DESTINATION.write_bytes(bundle)
    return manifest


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--verify-executable", type=Path)
    args = parser.parse_args()
    if args.verify_executable:
        binary = args.verify_executable.read_bytes()
        offset = 0
        manifest = None
        while (offset := binary.find(MAGIC, offset)) >= 0:
            try:
                manifest = verify(binary[offset:])
                break
            except (ValueError, KeyError, struct.error, UnicodeError):
                offset += len(MAGIC)
        if manifest is None:
            raise ValueError("Resource bundle absent from executable")
    else:
        manifest = build()
    print(json.dumps({"resources_verified": len(manifest["resources"]), "icon_variants": len(manifest["icons"]),
                      "forms": len(manifest["forms"]), "strings": len(manifest["strings"]),
                      "reference_sha256": manifest["reference_sha256"]}))
