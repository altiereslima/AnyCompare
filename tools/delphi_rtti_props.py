"""Print the published properties of a Delphi class in BCompare.exe with their field offsets.

Read-only; checks the SHA-256 of the reference executable. Used in REA round 3 (18/10/2026) to
map TUiAction fields: Category +0xb8, Visible +0x100, MenuAllowed +0x158, ToolbarAllowed +0x170.
Delphi x64 VMT layout here: SelfPtr -0xC8, TypeInfo -0xA8, ClassName -0x88, Parent -0x78.

Usage: python -I tools/delphi_rtti_props.py TUiAction [TOtherClass ...]
"""

import hashlib
import re
import struct
import sys

EXE = r"C:\Projetos\BC4\bc4\BCompare.exe"
SHA256 = "aba1b98f848a01c3729cfee3e8818ab8e0d947276bc42c9a55c1475c9f8438db"

DATA = open(EXE, "rb").read()
if hashlib.sha256(DATA).hexdigest() != SHA256:
    sys.exit("unexpected BCompare.exe hash")
_pe = struct.unpack_from("<I", DATA, 0x3C)[0]
_opt = _pe + 24
BASE = struct.unpack_from("<Q", DATA, _opt + 24)[0]
SECS = []
for _i in range(struct.unpack_from("<H", DATA, _pe + 6)[0]):
    _s = _opt + struct.unpack_from("<H", DATA, _pe + 20)[0] + 40 * _i
    _vsize, _va, _rsize, _raw = struct.unpack_from("<IIII", DATA, _s + 8)
    SECS.append((BASE + _va, _vsize, _raw, _rsize))


def off(va):
    for sva, vsize, raw, rsize in SECS:
        if sva <= va < sva + max(vsize, rsize):
            return raw + (va - sva)
    raise ValueError(hex(va))


def va_of(o):
    for sva, vsize, raw, rsize in SECS:
        if raw <= o < raw + rsize:
            return sva + (o - raw)
    return None


def q(va):
    return struct.unpack_from("<Q", DATA, off(va))[0]


def find_vmt(cls):
    pat = bytes([len(cls)]) + cls.encode()
    for m in re.finditer(re.escape(pat), DATA):
        nva = va_of(m.start())
        if nva is None:
            continue
        for r in re.finditer(re.escape(struct.pack("<Q", nva)), DATA):
            ref = va_of(r.start())
            if ref is None:
                continue
            vmt = ref + 0x88
            try:
                if q(vmt - 0xC8) == vmt:
                    return vmt
            except ValueError:
                pass
    return None


def class_props(ti):
    o = off(ti)
    n = DATA[o + 1]
    name = DATA[o + 2:o + 2 + n].decode("latin-1")
    o += 2 + n
    _cls, parent, _count = struct.unpack_from("<QQh", DATA, o)
    o += 18
    o += 1 + DATA[o]  # unit name
    count = struct.unpack_from("<H", DATA, o)[0]
    o += 2
    out = []
    for _ in range(count):
        _ptype, get, _set, _stored, _index, default, _nidx = struct.unpack_from("<QQQQiih", DATA, o)
        o += 42
        ln = DATA[o]
        pname = DATA[o + 1:o + 1 + ln].decode("latin-1")
        o += 1 + ln
        field = get & 0xFFFFFFFF if get >> 56 == 0xFF else None
        out.append({"name": pname, "field": field, "getter": None if field is not None else get, "default": default})
    return name, parent, out


def main():
    for cls in sys.argv[1:]:
        vmt = find_vmt(cls)
        if vmt is None:
            print(f"{cls}: VMT not found")
            continue
        print(f"{cls} VMT {vmt:#x}")
        ti = q(vmt - 0xA8)
        while ti:
            name, parent, props = class_props(ti)
            for p in props:
                where = f"field +{p['field']:#x}" if p["field"] is not None else f"getter {p['getter']:#x}"
                print(f"  {name}.{p['name']}: {where} default {p['default']}")
            ti = q(parent) if parent else 0
    return 0


if __name__ == "__main__":
    sys.exit(main())
