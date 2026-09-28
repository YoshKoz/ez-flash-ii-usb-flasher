"""Extract CRomManager::SpecialRomPatch's per-game patch byte lists.

SpecialRomPatch (patchDLL.dll RVA 0x3D70) makes no calls: it byte-compares the
GBA title field at rom+0xA0 against a marker string, then writes fixed bytes at
fixed ROM offsets. This walks the disassembly, resolves every marker string it
references, and lists the stores in address order so each block can be tied to
its marker.

usage: extract_specialpatch.py <patchDLL.dll> <disasm.txt> [out.txt]
"""
import re
import sys

LINE = re.compile(r"\s+([0-9A-F]{8}):\s+((?:[0-9A-F]{2} )+)\s+(.*)$")
STORE = re.compile(
    r"mov\s+(byte|word|dword)\s+ptr\s+\[(e[a-z]{2})\+([0-9A-F]+)h?\],\s*([0-9A-F]+)h?\s*$"
)
IMM = re.compile(r",\s*?(?:0x)?([0-9A-F]+)h?\s*$")
MARKER = re.compile(r"(?:mov|lea|push)\s+\S*?,?\s*(?:offset\s+)?(10011[0-9A-F]{3})h?", re.I)

SP_LO, SP_HI = 0x10003D70, 0x100040F0
BASE = 0x10000000


def cstr(img, va):
    off = va - BASE
    end = img.find(b"\x00", off)
    if off < 0 or end < 0 or end - off > 64:
        return None
    raw = img[off:end]
    if not raw or not all(32 <= c < 127 for c in raw):
        return None
    return raw.decode("ascii")


def main():
    dll, disasm = sys.argv[1], sys.argv[2]
    out_path = sys.argv[3] if len(sys.argv) > 3 else None
    img = open(dll, "rb").read()

    ins = []
    for line in open(disasm, encoding="utf-8", errors="replace"):
        m = LINE.match(line.rstrip("\n"))
        if m:
            ins.append((int(m.group(1), 16), m.group(2).strip(), m.group(3).strip()))
    ins.sort()
    body = [(a, raw, t) for a, raw, t in ins if SP_LO <= a < SP_HI]

    lines = []

    def w(s=""):
        lines.append(s)

    w("# CRomManager::SpecialRomPatch (patchDLL.dll, ImageBase 0x10000000)")
    w("# RVA 0x3D70..0x40F0. Makes no calls: it compares the GBA title field at")
    w("# rom+0xA0 against a marker string, then writes fixed bytes at fixed ROM")
    w("# offsets. Stores are listed in address order; markers are marked where")
    w("# the code loads them so each block can be tied to its game.")
    w("#")
    w("# Immediates are decoded from the raw instruction bytes, NOT taken from the")
    w("# disassembler text: dumpbin renders the dword at 0x10003E51 as")
    w("# '0F8428009h' when the bytes are 09 80 42 F8, i.e. 0xF8428009.")
    w()

    markers = []
    for a, _raw, t in body:
        for m in MARKER.finditer(t):
            va = int(m.group(1), 16)
            s = cstr(img, va)
            if s:
                markers.append((a, va, s))
    w("=== marker strings referenced ===")
    for a, va, s in markers:
        w("  %08X  loads %08X  %r" % (a, va, s))

    w()
    w("=== fixed stores (decoded from raw bytes) ===")
    w("  %-10s %-6s %-12s %-12s %s" % ("insn", "width", "offset", "value", "nearest marker before it"))
    cur = "-"
    marker_at = {a: s for a, _va, s in markers}
    n = 0
    flagged = 0
    for a, raw, t in body:
        if a in marker_at:
            cur = marker_at[a]
        # Read the instruction from the image, not from dumpbin's byte column:
        # dumpbin truncates that column for instructions with a disp32+imm32.
        b = img[a - BASE:a - BASE + 11]
        # C7 /0 = mov r/m32, imm32 ; C6 /0 = mov r/m8, imm8.
        # modrm: mod in bits 7-6, reg in bits 5-3 (must be 0), rm in bits 2-0.
        if not b:
            continue
        modrm = b[1]
        if (modrm >> 3) & 7:
            continue
        mod = modrm >> 6
        if b[0] == 0xC7 and mod == 2 and len(b) >= 10:
            disp = int.from_bytes(b[2:6], "little")
            val, width = int.from_bytes(b[6:10], "little"), "dword"
        elif b[0] == 0xC7 and mod == 1 and len(b) >= 7:
            disp = b[2]
            val, width = int.from_bytes(b[3:7], "little"), "dword"
        elif b[0] == 0xC6 and mod == 2 and len(b) >= 7:
            disp, val, width = int.from_bytes(b[2:6], "little"), b[6], "byte"
        elif b[0] == 0xC6 and mod == 1 and len(b) >= 4:
            disp, val, width = b[2], b[3], "byte"
        else:
            continue
        text_val = IMM.search(t)
        rendered = text_val.group(1).lstrip("0") or "0" if text_val else "?"
        if int(rendered, 16) != val:
            flagged += 1
            note = "  <-- disassembler said 0x%s" % rendered
        else:
            note = ""
        w("  %08X  %-6s 0x%-10X 0x%-10X %s%s" % (a, width, disp, val, cur, note))
        n += 1

    w()
    w("total fixed stores: %d, marker blocks: %d, immediates the disassembler misrendered: %d"
      % (n, len({s for _a, _v, s in markers}), flagged))

    text = "\n".join(lines) + "\n"
    if out_path:
        open(out_path, "w", encoding="utf-8", newline="\n").write(text)
        print("wrote %s (%d stores, %d markers)" % (out_path, n, len({s for _a, _v, s in markers})))
    else:
        print(text)


if __name__ == "__main__":
    main()
