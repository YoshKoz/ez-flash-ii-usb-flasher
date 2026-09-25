"""Recover ARM/Thumb function entries and trace string-literal xrefs in the loader.

Approach: Thumb code loads literals with `ldr rX, [pc, #imm]` (opcode 0x4800-0x4FFF).
For each such instruction, decode the literal address and, if it points at a
string or table of interest, report the call site. Also detect `bl` targets to
build a call graph seed.

Usage:
    python arm_xref.py <image> --string 0x08007388
    python arm_xref.py <image> --scan-strings
"""

import argparse
import re
import struct

ROM_BASE = 0x08000000


def thumb_ldr_literals(data, start, end):
    """Yield (pc, reg, literal_addr) for Thumb `ldr rX,[pc,#imm]`."""
    pc = start
    while pc < end - 4:
        hw = struct.unpack_from('<H', data, pc)[0]
        if 0x4800 <= hw <= 0x4FFF:
            reg = (hw >> 8) & 0x7
            imm = (hw & 0xFF) * 4
            lit = ((pc + 4) & ~3) + imm
            if lit + 4 <= len(data):
                val = struct.unpack_from('<I', data, lit)[0]
                yield pc, reg, val
        pc += 2


def thumb_bl_targets(data, start, end):
    """Yield (pc, target) for Thumb BL pairs."""
    pc = start
    while pc < end - 4:
        hw1 = struct.unpack_from('<H', data, pc)[0]
        if (hw1 & 0xF800) == 0xF000:
            hw2 = struct.unpack_from('<H', data, pc + 2)[0]
            if (hw2 & 0xF800) == 0xF800:
                s = (hw1 >> 10) & 1
                j1 = (hw2 >> 13) & 1
                j2 = (hw2 >> 11) & 1
                i1 = (~(j1 ^ s)) & 1
                i2 = (~(j2 ^ s)) & 1
                off = (s << 24) | (i1 << 23) | (i2 << 22) | ((hw1 & 0x3FF) << 12) | ((hw2 & 0x7FF) << 1)
                if off & (1 << 24):
                    off -= 1 << 25
                yield pc, pc + 4 + off
        pc += 2


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("image")
    ap.add_argument("--string", type=lambda s: int(s, 0))
    ap.add_argument("--scan-strings", action="store_true")
    ap.add_argument("--bl", action="store_true")
    args = ap.parse_args()
    data = open(args.image, "rb").read()
    limit = min(len(data), 0x200000)

    if args.scan_strings:
        # literal -> ASCII string
        for pc, reg, val in thumb_ldr_literals(data, 0, limit):
            off = val - ROM_BASE
            if 0 <= off < len(data) - 4:
                chunk = data[off:off + 24]
                m = re.match(rb'[ -~]{5,}', chunk)
                if m and len(m.group()) >= 5:
                    print(f'0x{pc:06X}: ldr r{reg}, =0x{val:08X}  -> "{m.group().decode("latin1")}"')
        return

    if args.bl:
        for pc, tgt in thumb_bl_targets(data, 0, limit):
            print(f'0x{pc:06X}: bl 0x{tgt:06X}')
        return

    if args.string is not None:
        target = args.string
        print(f'# literals equal to 0x{target:08X}')
        for pc, reg, val in thumb_ldr_literals(data, 0, limit):
            if val == target:
                print(f'  0x{pc:06X}: ldr r{reg}, =0x{val:08X}')


if __name__ == "__main__":
    main()