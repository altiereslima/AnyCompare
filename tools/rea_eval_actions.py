"""Evaluate BC4's action set per concrete actions class (REA, static).

Each BC4 view instantiates one T*Actions data module. Before the action tree is
pruned (TBaseActions.DataModuleCreate 0x19423a0), VMT+0x98 creates and frees
TUiAction fields depending on the session class returned by VMT+0xc8 and on the
edition. This tool interprets the Ghidra pseudocode of those VMT+0x98 routines,
taken verbatim from an exported REA evidence bundle, with the session-class
virtuals read from BCompare.exe (read-only).

Assumptions (documented in docs/rea/MENUS-REGRA-COMPOSICAO.md):
- Pro edition (BcIsPro = true), the user's target;
- global 0x215d69a = 0 (release build, not expired; licensing state not analysed);
- update check allowed (TCheckForUpdatesDialog field +0x58 = 0, wininet present);
- a field created at run time holds a component named like the field (verified
  for the "<Type>NewAction" creator 0x1944680).

Usage: rea_eval_actions.py <bundle.json> <out.json>
"""

import hashlib
import json
import re
import struct
import sys
from pathlib import Path

EXE = Path(r"C:\Projetos\BC4\bc4\BCompare.exe")
EXE_SHA256 = "aba1b98f848a01c3729cfee3e8818ab8e0d947276bc42c9a55c1475c9f8438db"
ROOT = Path(__file__).resolve().parents[1]

# ---------------------------------------------------------------- PE helpers
DATA = EXE.read_bytes()
assert hashlib.sha256(DATA).hexdigest() == EXE_SHA256
_pe = struct.unpack_from("<I", DATA, 0x3C)[0]
_opt = _pe + 24
BASE = struct.unpack_from("<Q", DATA, _opt + 24)[0]
SECS = []
for i in range(struct.unpack_from("<H", DATA, _pe + 6)[0]):
    s = _opt + struct.unpack_from("<H", DATA, _pe + 20)[0] + 40 * i
    vsize, va, rsize, raw = struct.unpack_from("<IIII", DATA, s + 8)
    SECS.append((DATA[s:s + 8].rstrip(b"\0").decode(), BASE + va, vsize, raw, rsize))
TEXT = next(s for s in SECS if s[0] == ".text")


def off(va):
    for _, sva, vsize, raw, rsize in SECS:
        if sva <= va < sva + max(vsize, rsize):
            return raw + (va - sva)
    raise ValueError(hex(va))


def q(va):
    return struct.unpack_from("<Q", DATA, off(va))[0]


def class_name(vmt):
    o = off(q(vmt - 0x88))
    return DATA[o + 1:o + 1 + DATA[o]].decode("latin-1")


def parent(vmt):
    ref = q(vmt - 0x78)
    return q(ref) if ref else 0


def field_table(vmt):
    ft = q(vmt - 0xA0)
    if not ft:
        return {}
    o = off(ft)
    count = struct.unpack_from("<H", DATA, o)[0]
    o += 10
    out = {}
    for _ in range(count):
        foff = struct.unpack_from("<I", DATA, o)[0]
        n = DATA[o + 6]
        out[foff] = DATA[o + 7:o + 7 + n].decode("latin-1")
        o += 7 + n
    return out


def chain(vmt):
    out = []
    while vmt:
        out.append(vmt)
        vmt = parent(vmt)
    return out


def tiny(fva):
    """Constant/getter functions recognised from their bytes, else None."""
    if not TEXT[1] <= fva < TEXT[1] + TEXT[2]:
        return None
    c = DATA[off(fva):off(fva) + 12]
    if c[:3] in (b"\x31\xc0\xc3", b"\x33\xc0\xc3") or c[:4] in (b"\x48\x31\xc0\xc3", b"\x48\x33\xc0\xc3"):
        return ("const", 0)
    if c[:3] == b"\xb0\x01\xc3":
        return ("const", 1)
    if c[0] == 0xB0 and c[2] == 0xC3:
        return ("const", c[1])
    if c[0] == 0xB8 and c[5] == 0xC3:
        return ("const", struct.unpack("<i", c[1:5])[0])
    if c[:2] == b"\xc7\xc0" and c[6] == 0xC3:
        return ("const", struct.unpack("<i", c[2:6])[0])
    if c[:3] == b"\x48\x8b\x05" and c[7] == 0xC3:
        return ("const", q(fva + 7 + struct.unpack("<i", c[3:7])[0]))
    if c[:3] == b"\x48\x8d\x05" and c[7] == 0xC3:
        return ("const", fva + 7 + struct.unpack("<i", c[3:7])[0])
    return None


# ------------------------------------------- session-class virtual semantics
# Non-trivial slot implementations, from their Ghidra pseudocode (REA evidence).
PANES_01 = {0x197AF60, 0x19715E0, 0x118D820, 0x17789F0, 0x178F580}
TEXT_MERGE_SESSION = 0x1187F78
PRO = True


def slot(cls, offset, arg=None):
    fva = q(cls + offset)
    t = tiny(fva)
    if t:
        return t[1]
    if fva in PANES_01:
        return int(arg in (0, 1))
    if fva == 0x118D710:
        return int(arg == 0)
    if fva == 0x1102100:
        return 3
    if fva == 0x184A5D0:
        return 2
    if fva == 0x118D7F0:
        return TEXT_MERGE_SESSION if PRO else 0
    if fva == 0x197AFD0:
        return int(arg == 3 and slot(cls, 0x198, 3))
    if fva == 0x197B000:
        return h(cls)
    raise KeyError(f"{class_name(cls)} slot {offset:#x} -> {fva:#x} not modelled")


def f(cls):  # 0x19662c0
    return int(bool(slot(cls, 0x190, 3) or slot(cls, 0x198, 3)))


def g(cls):  # 0x1966270
    return int(not f(cls) and not slot(cls, 0x168) and not slot(cls, 0x170) and not slot(cls, 0x178))


def h(cls):  # 0x197af90
    return int(any(slot(cls, 0x318, k) for k in range(4)))


def isa(cls, target):  # 0x40f9a0 TObject.InheritsFrom
    return int(target in chain(cls))


# Session classes registered in the globals used by 0x1944900, in field order
# DirCompareNew..XMLCompareNew. Table and XML sessions were not located.
REGISTERED = {
    0x2134930: 0x1131230, 0x2136B20: 0x11317C8, 0x2131208: 0x1105BD0,
    0x2133FD0: 0x1187860, 0x2136168: 0x1187F78, 0x21308E0: 0x1775128,
    0x2133E50: 0x178D080, 0x21383C0: 0x17A1800, 0x2136FA8: 0x1849320,
    0x2136D60: 0x185D168, 0x21358F0: -1, 0x2130A68: 0x1865B90, 0x21351C8: -2,
    0x21338F8: -3, 0x2130CC8: -4,
}

# ----------------------------------------------------------- pseudocode parser


class Parser:
    def __init__(self, text):
        self.t = text
        self.i = 0

    def ws(self):
        while self.i < len(self.t) and self.t[self.i].isspace():
            self.i += 1

    def starts(self, word):
        self.ws()
        if not self.t.startswith(word, self.i):
            return False
        end = self.i + len(word)
        return end >= len(self.t) or not (self.t[end].isalnum() or self.t[end] == "_")

    def balanced(self):
        self.ws()
        assert self.t[self.i] == "(", self.t[self.i:self.i + 40]
        depth = 0
        start = self.i
        while True:
            c = self.t[self.i]
            if c == "'":
                self.i = self.t.index("'", self.i + 1) + 1
                continue
            if c == "(":
                depth += 1
            elif c == ")":
                depth -= 1
                if depth == 0:
                    self.i += 1
                    return self.t[start + 1:self.i - 1]
            self.i += 1

    def until_semicolon(self):
        self.ws()
        start = self.i
        depth = 0
        while True:
            c = self.t[self.i]
            if c == "'":
                self.i = self.t.index("'", self.i + 1) + 1
                continue
            if c in "([":
                depth += 1
            elif c in ")]":
                depth -= 1
            elif c == ";" and depth == 0:
                self.i += 1
                return self.t[start:self.i - 1].strip()
            self.i += 1

    def block(self):
        self.ws()
        assert self.t[self.i] == "{"
        self.i += 1
        out = []
        while True:
            self.ws()
            if self.t[self.i] == "}":
                self.i += 1
                return out
            out.append(self.statement())

    def statement(self):
        self.ws()
        m = re.match(r"([A-Za-z_]\w*):(?!:)", self.t[self.i:])
        if m and (m.group(1).startswith("LAB_") or m.group(1).startswith("code_r")):
            self.i += m.end()
            return ("label", m.group(1))
        if self.starts("if"):
            self.i += 2
            cond = self.balanced()
            then = self.body()
            other = []
            if self.starts("else"):
                self.i += 4
                other = [self.statement()] if self.starts("if") else self.body()
            return ("if", cond, then, other)
        if self.starts("do"):
            self.i += 2
            body = self.block()
            assert self.starts("while")
            self.i += 5
            cond = self.balanced()
            self.until_semicolon()
            return ("do", body, cond)
        if self.starts("goto"):
            self.i += 4
            return ("goto", self.until_semicolon())
        if self.starts("return"):
            self.until_semicolon()
            return ("return",)
        return ("stmt", self.until_semicolon())

    def body(self):
        self.ws()
        return self.block() if self.t[self.i] == "{" else [self.statement()]


def compile_function(code):
    signature = code[:code.index("{")]
    pointer = "longlong *param_1" in signature
    nodes = Parser(code[code.index("{"):]).block()
    ops, counter = [], [0]

    def label():
        counter[0] += 1
        return f"__L{counter[0]}"

    def emit(nodes):
        for n in nodes:
            if n[0] == "if":
                other, end = label(), label()
                ops.append(("jz", n[1], other))
                emit(n[2])
                ops.append(("jmp", end))
                ops.append(("label", other))
                emit(n[3])
                ops.append(("label", end))
            elif n[0] == "do":
                top = label()
                ops.append(("label", top))
                emit(n[1])
                ops.append(("jnz", n[2], top))
            elif n[0] == "goto":
                ops.append(("jmp", n[1]))
            else:
                ops.append(n)

    emit(nodes)
    return pointer, ops


# --------------------------------------------------------- expression engine
SESSION_CALL = re.compile(r"\(\*\*\(code \*\*\)\(\*param_1 \+ (0x[0-9a-f]+|\d+)\)\)\(\*param_1\)")
SLOT_CALL = re.compile(r"\(\*\*\(code \*\*\)\((\w+) \+ (0x[0-9a-f]+|\d+)\)\)\(\1(?:,([^()]+))?\)")


def translate(expr):
    e = " ".join(expr.split())
    e = re.sub(r"\(int\)CONCAT71\(\(int7\)\(\(ulonglong\)\w+ >> 8\),1\)", "1", e)
    e = SESSION_CALL.sub(lambda m: f"ACT({int(m.group(1), 0)})", e)
    e = SLOT_CALL.sub(
        lambda m: f"S({m.group(1)},{int(m.group(2), 0)}" + (f",{m.group(3)})" if m.group(3) else ")"), e
    )
    e = re.sub(r"FUN_0040f9a0\((\w+),&PTR_FUN_([0-9a-f]+)\)", r"ISA(\1,0x\2)", e)
    e = e.replace("FUN_00fd5a10()", "PRO")
    e = e.replace("FUN_0085bbe0()", "1")
    e = re.sub(r"FUN_019662c0\((\w+)\)", r"F(\1)", e)
    e = re.sub(r"FUN_01966270\((\w+)\)", r"G(\1)", e)
    e = re.sub(r"FUN_0197af90\((\w+)\)", r"H(\1)", e)
    e = re.sub(r"\*\(longlong \*\)PTR_DAT_([0-9a-f]+)", r"REG(0x\1)", e)
    e = e.replace("*PTR_DAT_02135ae8", "0")
    e = re.sub(r"\*\(char \*\)\(lVar3 \+ 0x58\)", "0", e)
    e = re.sub(r"FUN_0191d170\([^)]*\)", "1", e)
    e = re.sub(r"'\\x([0-9a-f]{2})'", lambda m: str(int(m.group(1), 16)), e)
    e = e.replace("'\\0'", "0").replace("'\\t'", "9").replace("'\\a'", "7")
    e = re.sub(r"\((?:byte|int|char|uint|ulonglong|longlong \*|longlong|undefined8 \*)\)", "", e)
    e = re.sub(r"(\d)U\b", r"\1", e)
    e = e.replace("&&", " and ").replace("||", " or ")
    e = re.sub(r"!(?!=)", " not ", e)
    if "FUN_" in e or "param_1[" in e or "&" in e.replace(" and ", ""):
        raise ValueError(e)
    return e


class Machine:
    def __init__(self, actions_vmt, sources):
        self.actions = actions_vmt
        self.sources = sources
        self.session = None
        self.created = set()
        self.freed = []
        self.trace = []

    def act(self, offset):
        fva = q(self.actions + offset)
        t = tiny(fva)
        if t is None:
            raise KeyError(f"actions slot {offset:#x} -> {fva:#x}")
        return t[1]

    def run(self, address):
        pointer, ops = self.sources[address]
        scale = 8 if pointer else 1
        labels = {op[1]: i for i, op in enumerate(ops) if op[0] == "label"}
        env, pending = {}, {}
        api = {
            "ACT": self.act, "S": slot, "F": f, "G": g, "H": h, "ISA": isa, "PRO": int(PRO),
            "REG": lambda p: REGISTERED[p],
        }
        pc = steps = 0
        while pc < len(ops):
            steps += 1
            assert steps < 100000
            op = ops[pc]
            pc += 1
            kind = op[0]
            if kind == "label":
                continue
            if kind == "jmp":
                pc = labels[op[1]]
                continue
            if kind in ("jz", "jnz"):
                value = eval(translate(op[1]), api, env)
                if (not value) == (kind == "jz"):
                    pc = labels[op[2]]
                continue
            if kind == "return":
                return
            text = op[1]
            m = re.fullmatch(r"FUN_([0-9a-f]+)\(param_1\)", text)
            if m and int(m.group(1), 16) in self.sources:
                self.run(int(m.group(1), 16))
                continue
            m = re.fullmatch(r"(local_\w+) = (?:\(undefined8 \*\))?\(?param_1 \+ (0x[0-9a-f]+|\d+)\)?", text)
            if m:
                pending[m.group(1)] = int(m.group(2), 0) * scale
                continue
            m = re.fullmatch(r"(\w+) = \*(local_\w+)", text)
            if m:
                env[m.group(1)] = ("field", pending[m.group(2)])
                continue
            m = re.fullmatch(r"(\w+) = \*\(undefined8 \*\)\(param_1 \+ (0x[0-9a-f]+)\)", text)
            if m:
                env[m.group(1)] = ("field", int(m.group(2), 16))
                continue
            m = re.fullmatch(r"FUN_0040f350\((\w+)\)", text)
            if m:
                value = env.get(m.group(1))
                if isinstance(value, tuple) and value[0] == "field":
                    self.freed.append(value[1])
                continue
            m = re.fullmatch(r"param_1\[(0x[0-9a-f]+|\d+)\] = (\w+)", text)
            if m:
                if env.get(m.group(2)) == "created":
                    self.created.add(int(m.group(1), 0) * 8)
                continue
            m = re.fullmatch(r"\*\(undefined8 \*\)\(param_1 \+ (0x[0-9a-f]+|\d+)\) = (\w+)", text)
            if m:
                if env.get(m.group(2)) == "created":
                    self.created.add(int(m.group(1), 0))
                continue
            m = re.fullmatch(r"(\w+) = FUN_(01944680|0113c3f0|0113c6a0|010f48f0|01191820|01191550)\(([^,]+),(.*)\)", text)
            if m:
                arg = m.group(4).strip()
                ok = True
                reg = re.fullmatch(r"\*\(undefined8 \*\)PTR_DAT_([0-9a-f]+)", arg)
                if reg:
                    ok = REGISTERED[int(reg.group(1), 16)] != 0
                env[m.group(1)] = "created" if ok else None
                continue
            # String temporaries: LStrAsg-like 0x419630 sets, 0x4128c0 clears.
            m = re.fullmatch(r"FUN_00419630\(&(local_\w+),.*\)", text)
            if m:
                env[m.group(1)] = 1
                continue
            m = re.fullmatch(r"FUN_004128c0\(&(local_\w+)\)", text)
            if m:
                env[m.group(1)] = 0
                continue
            m = re.fullmatch(r"(local_\w+) = 0", text)
            if m:
                env[m.group(1)] = 0
                continue
            m = re.fullmatch(r"([cilub]Var\d+) = (.*)", text, re.S)
            if m:
                try:
                    env[m.group(1)] = eval(translate(m.group(2)), api, env)
                except (ValueError, KeyError, NameError, SyntaxError, TypeError):
                    env[m.group(1)] = None
                continue
            # Everything else (property setters, captions, string temporaries) does
            # not create or free actions.


def main():
    bundle_path, out_path = Path(sys.argv[1]), Path(sys.argv[2])
    bundle = json.loads(bundle_path.read_text(encoding="utf-8"))
    wanted = {0x1944900, 0x10B9830, 0x113C930, 0x10F4B90, 0x1138410, 0x1191B00, 0x17900B0, 0x184B150, 0x185FD40}
    sources, evidence = {}, {}
    for record in bundle["records"]:
        if record["operation"] != "procedure_pseudo_code":
            continue
        address = int(record["parameters"]["procedure"], 16)
        if address in wanted and address not in sources:
            sources[address] = compile_function(record["normalized_result"])
            evidence[f"{address:#x}"] = record["evidence_id"]
    missing = wanted - set(sources)
    assert not missing, [hex(a) for a in missing]

    forms = json.loads((ROOT / "docs" / "rea" / "forms.json").read_text(encoding="utf-8"))
    dfm = {}
    for form in forms:
        names = []

        def walk(node):
            if node.get("class") == "TUiAction":
                names.append(node["name"])
            for child in node.get("children", []):
                walk(child)

        walk(form["hierarchy"])
        dfm[form["hierarchy"]["class"]] = names

    concrete = {
        "THomeActions": 0x10E9D58, "TDirSyncActions": 0x113A960,
        "TDirCompareActions": 0x1137B40, "TDirMergeActions": 0x1138008,
        "TTextCompareActions": 0x1190360, "TTextMergeActions": 0x1190CB8,
        "TTextEditActions": 0x1190830, "TTextPatchActions": 0x1191188,
        "TDataCompareActions": 0x1779270, "THexCompareActions": 0x178FCE8,
        "TMP3CompareActions": 0x17A24A0, "TPixCompareActions": 0x184AD90,
        "TRegCompareActions": 0x185F930, "TVerCompareActions": 0x1866768,
    }
    result = {}
    for name, vmt in concrete.items():
        machine = Machine(vmt, sources)
        entry = q(vmt + 0x98)
        machine.run(entry)
        fields = {}
        for cls in reversed(chain(vmt)):
            fields.update(field_table(cls))
        modules = [class_name(c) for c in reversed(chain(vmt))]
        declared = [n for m in modules for n in dfm.get(m, [])]
        freed = {fields[o] for o in machine.freed if o in fields}
        created = {fields[o] for o in machine.created if o in fields}
        effective = sorted((set(declared) | created) - freed)
        session = None
        try:
            session = class_name(q(vmt + 0xC8)) if name != "THomeActions" else None
        except ValueError:
            pass
        t = tiny(q(vmt + 0xC8)) if name != "THomeActions" else None
        result[name] = {
            "vmt": f"{vmt:#x}",
            "load_chain": modules,
            "vmt_0x98": f"{entry:#x}",
            "session_class": class_name(t[1]) if t else None,
            "created": sorted(created),
            "freed": sorted(freed),
            "unnamed_offsets": sorted(f"{o:#x}" for o in set(machine.freed) | machine.created if o not in fields),
            "effective_actions": effective,
        }
        print(f"{name:22} session={result[name]['session_class']} created={len(created)} freed={len(freed)} effective={len(effective)}")
    doc = {
        "target_sha256": EXE_SHA256,
        "assumptions": ["Pro edition", "global 0x215d69a = 0", "update check allowed", "created component name = field name"],
        "pseudocode_evidence": evidence,
        "bundle": {"path": str(bundle_path), "sha256": hashlib.sha256(bundle_path.read_bytes()).hexdigest()},
        "classes": result,
    }
    out_path.write_text(json.dumps(doc, ensure_ascii=False, indent=1) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
