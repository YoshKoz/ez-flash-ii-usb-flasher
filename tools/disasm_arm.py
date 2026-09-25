"""Disassemble the EZLoader ARM/Thumb code from a dumped image.

The loader lives at offset 0 of the cartridge image and runs from the GBA ROM
base 0x08000000. This prints disassembly for a given ROM offset and can scan
literal pools for pointers into the cartridge.

Usage:
    python disasm_arm.py <image> --at 0x00 --thumb --len 60
    python disasm_arm.py <image> --at 0xC0 --arm --len 40
    python disasm_arm.py <image> --ptrs           # literal-pool pointer census
"""
import argparse
import struct

import capstone

ROM_BASE = 0x08000000


def load(path):
    return open(path, "rb").read()


def disasm(data, off, thumb, n):
    md = capstone.Cs(capstone.CS_ARCH_ARM, capstone.CS_MODE_LITTLE_ENDIAN)
    md.mode |= capstone.CS_MODE_THUMB if thumb else capstone.CS_MODE_ARM
    md.detail = False
    code = data[off:off + n * 4]
    addr = ROM_BASE + off
    out = []
    for ins in md.disasm(code, addr):
        out.append(f"{ins.address - ROM_BASE:06X}: {ins.mnemonic:<8} {ins.op_str}")
    return "\n".join(out)


def ptr_census(data, limit=0x200000):
    counts = {}
    for i in range(0, min(len(data), limit) - 4, 4):
        v = struct.unpack_from("<I", data, i)[0]
        if ROM_BASE <= v < ROM_BASE + len(data):
            counts.setdefault(v & 0xFFF00000, 0)
            counts[v & 0xFFF00000] += 1
    return sorted(counts.items())


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("image")
    ap.add_argument("--at", type=lambda s: int(s, 0))
    ap.add_argument("--len", type=int, default=60)
    ap.add_argument("--thumb", action="store_true")
    ap.add_argument("--arm", action="store_true")
    ap.add_argument("--ptrs", action="store_true")
    args = ap.parse_args()
    data = load(args.image)

    if args.ptrs:
        for base, n in ptr_census(data):
            print(f"0x{base:08X}: {n}")
        return

    thumb = args.thumb and not args.arm
    print(f"# {args.image} @ 0x{args.at:06X} ({'thumb' if thumb else 'arm'})")
    print(disasm(data, args.at, thumb, args.len))


if __name__ == "__main__":
    main()