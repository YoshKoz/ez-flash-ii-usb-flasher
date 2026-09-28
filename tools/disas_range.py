"""Print an address range from a `dumpbin /disasm` dump.

usage: disas_range.py <disasm.txt> <lo-hex> <hi-hex> [--calls]

--calls filters to control-flow and immediate-push lines, which is the quickest
way to read what a function does with its string/byte-argument tables.
"""
import re
import sys

LINE = re.compile(r"\s+([0-9A-F]{8}):\s+((?:[0-9A-F]{2} )+)\s+(.*)$")


def load(path):
    ins = []
    for line in open(path, encoding="utf-8", errors="replace"):
        m = LINE.match(line.rstrip("\n"))
        if m:
            ins.append((int(m.group(1), 16), m.group(2).strip(), m.group(3).strip()))
    ins.sort()
    return ins


def main():
    path = sys.argv[1]
    lo = int(sys.argv[2], 16)
    hi = int(sys.argv[3], 16)
    only_calls = "--calls" in sys.argv
    for addr, raw, text in load(path):
        if not (lo <= addr < hi):
            continue
        if only_calls and not (text.startswith(("call", "j", "ret", "push"))):
            continue
        print("%08X  %-26s %s" % (addr, raw, text))


if __name__ == "__main__":
    main()
