"""Pull size/offset constants out of an address range of a dumpbin disassembly.

usage: disas_consts.py <disasm.txt> <lo-hex> <hi-hex> [min] [max]

Useful for reading a function whose job is laying out a binary format: the
immediates in the 0x100..0x2000000 range are the sizes and offsets.
"""
import re
import sys

LINE = re.compile(r"\s+([0-9A-F]{8}):\s+((?:[0-9A-F]{2} )+)\s+(.*)$")
IMM = re.compile(r"\b([0-9A-F]{2,8})h\b")


def main():
    path = sys.argv[1]
    lo, hi = int(sys.argv[2], 16), int(sys.argv[3], 16)
    lo_v = int(sys.argv[4], 16) if len(sys.argv) > 4 else 0x100
    hi_v = int(sys.argv[5], 16) if len(sys.argv) > 5 else 0x2000000

    ins = []
    for line in open(path, encoding="utf-8", errors="replace"):
        m = LINE.match(line.rstrip("\n"))
        if m:
            ins.append((int(m.group(1), 16), m.group(3).strip()))
    ins.sort()
    body = [(a, t) for a, t in ins if lo <= a < hi]
    print("instructions in %08X..%08X: %d" % (lo, hi, len(body)))

    print("=== immediates in 0x%X..0x%X ===" % (lo_v, hi_v))
    n = 0
    for a, t in body:
        for m in IMM.finditer(t):
            v = int(m.group(1), 16)
            if lo_v <= v <= hi_v:
                print("  %08X  %-56s = %d (0x%X)" % (a, t, v, v))
                n += 1
    print("total: %d" % n)


if __name__ == "__main__":
    main()
