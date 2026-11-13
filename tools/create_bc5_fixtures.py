"""Synthetic PE/version test inputs; never redistribute vendor DLLs."""
from pathlib import Path
import struct

ROOT = Path(__file__).resolve().parents[1] / "tests/fixtures/bc5"

def pad(data):
    return data + b"\0" * (-len(data) % 4)

def block(name, value=b"", children=(), text=False):
    result = struct.pack("<HHH", 0, len(value) // 2 if text else len(value), int(text))
    result = pad(result + name.encode("utf-16le") + b"\0\0") + value
    for child in children:
        result = pad(result) + child
    return struct.pack("<H", len(result)) + result[2:]

def pe(wide=False, changed=False):
    fixed = struct.pack("<13I", 0xFEEF04BD, 0x10000, 0x10002, 0x30004, 0x10002, 0x30004, 0x3F, 0, 0x40004, 2, 0, 0, 0)
    values = [("CompanyName", "AnyCompare"), ("FileDescription", "Test module" if not changed else "Changed mod"), ("FileVersion", "1.2.3.4"), ("OriginalFilename", "fixture.dll"), ("ProductName", "AnyCompare fixtures")]
    strings = [block(k, (v + "\0").encode("utf-16le"), text=True) for k, v in values]
    version = block("VS_VERSION_INFO", fixed, [block("StringFileInfo", children=[block("040904B0", children=strings, text=True)], text=True), block("VarFileInfo", children=[block("Translation", struct.pack("<HH", 0x409, 1200))], text=True)])
    size = ((len(version) + 88 + 511) // 512) * 512
    data = bytearray(512 + size)
    data[:2] = b"MZ"
    struct.pack_into("<I", data, 60, 128)
    data[128:132] = b"PE\0\0"
    optional_size = 240 if wide else 224
    struct.pack_into("<HHIIIHH", data, 132, 0x8664 if wide else 0x14C, 1, 1465535346, 0, 0, optional_size, 0x2022 if wide else 0x2122)
    optional = 152
    struct.pack_into("<HBB", data, optional, 0x20B if wide else 0x10B, 14, 0)
    struct.pack_into("<Q" if wide else "<I", data, optional + (24 if wide else 28), 0x180000000 if wide else 0x10000000)
    struct.pack_into("<II", data, optional + 32, 4096, 512)
    struct.pack_into("<HHHHHH", data, optional + 40, 6, 0, 1, 0, 6, 0)
    struct.pack_into("<II", data, optional + 56, 8192, 512)
    struct.pack_into("<HH", data, optional + 68, 3, 0x4140)
    directories = optional + (112 if wide else 96)
    struct.pack_into("<I", data, directories - 4, 16)
    struct.pack_into("<II", data, directories + 16, 4096, size)
    struct.pack_into("<8sIIIIIIHHI", data, optional + optional_size, b".rsrc\0\0\0", size, 4096, size, 512, 0, 0, 0, 0, 0x40000040)
    for pos, key, target in [(0, 16, 0x80000018), (24, 1, 0x80000030), (48, 0x409, 72)]:
        struct.pack_into("<HHII", data, 512 + pos + 12, 0, 1, key, target)
    struct.pack_into("<IIII", data, 512 + 72, 4096 + 88, len(version), 1200, 0)
    data[600:600 + len(version)] = version
    return bytes(data)

if __name__ == "__main__":
    ROOT.mkdir(parents=True, exist_ok=True)
    for name, data in [("pe32.bin", pe()), ("pe64.bin", pe(True)), ("pe32-changed.bin", pe(changed=True))]:
        (ROOT / name).write_bytes(data)
    print("Created three synthetic PE/version fixtures.")
