"""Analyse a function in a dumpbin /disasm dump.

usage: analyze_saverpatch.py <disasm.txt> <lo-hex> <hi-hex>

Reports the distinct call targets (with counts) and every instruction that
stores an immediate through a register with a displacement -- which is how a
ROM patcher writes its replacement bytes.
"""
import re
import sys
from collections import Counter

LINE = re.compile(r"\s+([0-9A-F]{8}):\s+((?:[0-9A-F]{2} )+)\s+(.*)$")
STORE = re.compile(
    r"mov\s+(?:byte|word|dword)\s+ptr\s+\[(e[a-z]{2})(?:\+([0-9A-F]+)h?)?\],\s*([0-9A-F]+)h?\s*$"
)


def main():
    path, lo, hi = sys.argv[1], int(sys.argv[2], 16), int(sys.argv[3], 16)
    ins = []
    for line in open(path, encoding="utf-8", errors="replace"):
        m = LINE.match(line.rstrip("\n"))
        if m:
            ins.append((int(m.group(1), 16), m.group(3).strip()))
    ins.sort()
    body = [(a, t) for a, t in ins if lo <= a < hi]

    calls = Counter()
    for _a, t in body:
        m = re.match(r"call\s+(\S+)", t)
        if m:
            calls[m.group(1)] += 1
    print("=== distinct call targets in %08X..%08X ===" % (lo, hi))
    for target, n in calls.most_common():
        print("   %-14s x%d" % (target, n))

    print()
    print("=== immediate stores through a register (candidate ROM writes) ===")
    prev = None
    for a, t in body:
        m = STORE.match(t)
        if m:
            print("   %08X  %-40s  (prev: %s)" % (a, t, prev or "-"))
        if not t.startswith(("mov", "push", "add", "cmp", "test", "lea")):
            prev = t


if __name__ == "__main__":
    main()
