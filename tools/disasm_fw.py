#!/usr/bin/env python3
"""Disassemble EZ-USB AN2131 8051 firmware binary."""
import sys
from capstone import *

fw_path = sys.argv[1] if len(sys.argv) > 1 else "../src/ezwriter-cli/tusbez.bin"
with open(fw_path, "rb") as f:
    code = f.read()

md = Cs(CS_ARCH_MCS51, CS_MODE_MCS51)
md.detail = True

print(f";; Firmware: {fw_path} ({len(code)} bytes)")
print(f";; Entry: {code[0]:02x} {code[1]:02x} {code[2]:02x}\n")

for addr, size, mnemonic, op_str in md.disasm_lite(code, 0):
    print(f"0x{addr:04X}:  {mnemonic:7s} {op_str}")
