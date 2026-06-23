"""
Disassemble tusbez.bin (Cypress AN2131 8051 firmware).
Finds the bulk EP command dispatch table to identify valid command bytes.
No external dependencies — pure Python 8051 disassembly.
"""

import sys
from pathlib import Path

FW = Path(__file__).parent / "src" / "ezwriter-cli" / "tusbez.bin"

with open(FW, "rb") as f:
    data = bytearray(f.read())

print(f"Firmware: {FW.name}  {len(data)} bytes")
print()

# ---------------------------------------------------------------------------
# Minimal 8051 opcode table (opcode -> (mnemonic, total_bytes))
# ---------------------------------------------------------------------------
OPS = {
    0x00: ("NOP",    1), 0x01: ("AJMP",  2), 0x02: ("LJMP",  3),
    0x03: ("RR A",   1), 0x04: ("INC A", 1), 0x05: ("INC d", 2),
    0x06: ("INC @R0",1), 0x07: ("INC @R1",1),
    0x08: ("INC R0", 1), 0x09: ("INC R1",1), 0x0A: ("INC R2",1),
    0x0B: ("INC R3", 1), 0x0C: ("INC R4",1), 0x0D: ("INC R5",1),
    0x0E: ("INC R6", 1), 0x0F: ("INC R7",1),
    0x10: ("JBC d,r",3), 0x11: ("ACALL",2), 0x12: ("LCALL",3),
    0x13: ("RRC A",  1), 0x14: ("DEC A", 1), 0x15: ("DEC d", 2),
    0x16: ("DEC @R0",1), 0x17: ("DEC @R1",1),
    0x18: ("DEC R0", 1), 0x19: ("DEC R1",1), 0x1A: ("DEC R2",1),
    0x1B: ("DEC R3", 1), 0x1C: ("DEC R4",1), 0x1D: ("DEC R5",1),
    0x1E: ("DEC R6", 1), 0x1F: ("DEC R7",1),
    0x20: ("JB d,r", 3), 0x21: ("AJMP",  2), 0x22: ("RET",   1),
    0x23: ("RL A",   1), 0x24: ("ADD A,#i",2), 0x25: ("ADD A,d",2),
    0x26: ("ADD A,@R0",1), 0x27: ("ADD A,@R1",1),
    0x28: ("ADD A,R0",1), 0x29: ("ADD A,R1",1), 0x2A: ("ADD A,R2",1),
    0x2B: ("ADD A,R3",1), 0x2C: ("ADD A,R4",1), 0x2D: ("ADD A,R5",1),
    0x2E: ("ADD A,R6",1), 0x2F: ("ADD A,R7",1),
    0x30: ("JNB d,r",3), 0x31: ("ACALL",2), 0x32: ("RETI",  1),
    0x33: ("RLC A",  1), 0x34: ("ADDC A,#i",2), 0x35: ("ADDC A,d",2),
    0x36: ("ADDC A,@R0",1), 0x37: ("ADDC A,@R1",1),
    0x38: ("ADDC A,R0",1), 0x39: ("ADDC A,R1",1), 0x3A: ("ADDC A,R2",1),
    0x3B: ("ADDC A,R3",1), 0x3C: ("ADDC A,R4",1), 0x3D: ("ADDC A,R5",1),
    0x3E: ("ADDC A,R6",1), 0x3F: ("ADDC A,R7",1),
    0x40: ("JC r",   2), 0x41: ("AJMP",  2), 0x42: ("ORL d,A",2),
    0x43: ("ORL d,#i",3), 0x44: ("ORL A,#i",2), 0x45: ("ORL A,d",2),
    0x46: ("ORL A,@R0",1), 0x47: ("ORL A,@R1",1),
    0x48: ("ORL A,R0",1), 0x49: ("ORL A,R1",1), 0x4A: ("ORL A,R2",1),
    0x4B: ("ORL A,R3",1), 0x4C: ("ORL A,R4",1), 0x4D: ("ORL A,R5",1),
    0x4E: ("ORL A,R6",1), 0x4F: ("ORL A,R7",1),
    0x50: ("JNC r",  2), 0x51: ("ACALL",2), 0x52: ("ANL d,A",2),
    0x53: ("ANL d,#i",3), 0x54: ("ANL A,#i",2), 0x55: ("ANL A,d",2),
    0x56: ("ANL A,@R0",1), 0x57: ("ANL A,@R1",1),
    0x58: ("ANL A,R0",1), 0x59: ("ANL A,R1",1), 0x5A: ("ANL A,R2",1),
    0x5B: ("ANL A,R3",1), 0x5C: ("ANL A,R4",1), 0x5D: ("ANL A,R5",1),
    0x5E: ("ANL A,R6",1), 0x5F: ("ANL A,R7",1),
    0x60: ("JZ r",   2), 0x61: ("AJMP",  2), 0x62: ("XRL d,A",2),
    0x63: ("XRL d,#i",3), 0x64: ("XRL A,#i",2), 0x65: ("XRL A,d",2),
    0x66: ("XRL A,@R0",1), 0x67: ("XRL A,@R1",1),
    0x68: ("XRL A,R0",1), 0x69: ("XRL A,R1",1), 0x6A: ("XRL A,R2",1),
    0x6B: ("XRL A,R3",1), 0x6C: ("XRL A,R4",1), 0x6D: ("XRL A,R5",1),
    0x6E: ("XRL A,R6",1), 0x6F: ("XRL A,R7",1),
    0x70: ("JNZ r",  2), 0x71: ("ACALL",2), 0x72: ("ORL C,b",2),
    0x73: ("JMP @A+DPTR",1), 0x74: ("MOV A,#i",2), 0x75: ("MOV d,#i",3),
    0x76: ("MOV @R0,#i",2), 0x77: ("MOV @R1,#i",2),
    0x78: ("MOV R0,#i",2), 0x79: ("MOV R1,#i",2), 0x7A: ("MOV R2,#i",2),
    0x7B: ("MOV R3,#i",2), 0x7C: ("MOV R4,#i",2), 0x7D: ("MOV R5,#i",2),
    0x7E: ("MOV R6,#i",2), 0x7F: ("MOV R7,#i",2),
    0x80: ("SJMP r", 2), 0x81: ("AJMP",  2), 0x82: ("ANL C,b",2),
    0x83: ("MOVC A,@A+PC",1), 0x84: ("DIV AB",1), 0x85: ("MOV d,d",3),
    0x86: ("MOV d,@R0",2), 0x87: ("MOV d,@R1",2),
    0x88: ("MOV d,R0",2), 0x89: ("MOV d,R1",2), 0x8A: ("MOV d,R2",2),
    0x8B: ("MOV d,R3",2), 0x8C: ("MOV d,R4",2), 0x8D: ("MOV d,R5",2),
    0x8E: ("MOV d,R6",2), 0x8F: ("MOV d,R7",2),
    0x90: ("MOV DPTR,#i16",3), 0x91: ("ACALL",2), 0x92: ("MOV b,C",2),
    0x93: ("MOVC A,@A+DPTR",1), 0x94: ("SUBB A,#i",2), 0x95: ("SUBB A,d",2),
    0x96: ("SUBB A,@R0",1), 0x97: ("SUBB A,@R1",1),
    0x98: ("SUBB A,R0",1), 0x99: ("SUBB A,R1",1), 0x9A: ("SUBB A,R2",1),
    0x9B: ("SUBB A,R3",1), 0x9C: ("SUBB A,R4",1), 0x9D: ("SUBB A,R5",1),
    0x9E: ("SUBB A,R6",1), 0x9F: ("SUBB A,R7",1),
    0xA0: ("ORL C,/b",2), 0xA1: ("AJMP",2), 0xA2: ("MOV C,b",2),
    0xA3: ("INC DPTR",1), 0xA4: ("MUL AB",1), 0xA5: ("???",1),
    0xA6: ("MOV @R0,d",2), 0xA7: ("MOV @R1,d",2),
    0xA8: ("MOV R0,d",2), 0xA9: ("MOV R1,d",2), 0xAA: ("MOV R2,d",2),
    0xAB: ("MOV R3,d",2), 0xAC: ("MOV R4,d",2), 0xAD: ("MOV R5,d",2),
    0xAE: ("MOV R6,d",2), 0xAF: ("MOV R7,d",2),
    0xB0: ("ANL C,/b",2), 0xB1: ("ACALL",2), 0xB2: ("CPL b",2),
    0xB3: ("CPL C",  1), 0xB4: ("CJNE A,#i,r",3), 0xB5: ("CJNE A,d,r",3),
    0xB6: ("CJNE @R0,#i,r",3), 0xB7: ("CJNE @R1,#i,r",3),
    0xB8: ("CJNE R0,#i,r",3), 0xB9: ("CJNE R1,#i,r",3),
    0xBA: ("CJNE R2,#i,r",3), 0xBB: ("CJNE R3,#i,r",3),
    0xBC: ("CJNE R4,#i,r",3), 0xBD: ("CJNE R5,#i,r",3),
    0xBE: ("CJNE R6,#i,r",3), 0xBF: ("CJNE R7,#i,r",3),
    0xC0: ("PUSH d", 2), 0xC1: ("AJMP",2), 0xC2: ("CLR b",2),
    0xC3: ("CLR C",  1), 0xC4: ("SWAP A",1), 0xC5: ("XCH A,d",2),
    0xC6: ("XCH A,@R0",1), 0xC7: ("XCH A,@R1",1),
    0xC8: ("XCH A,R0",1), 0xC9: ("XCH A,R1",1), 0xCA: ("XCH A,R2",1),
    0xCB: ("XCH A,R3",1), 0xCC: ("XCH A,R4",1), 0xCD: ("XCH A,R5",1),
    0xCE: ("XCH A,R6",1), 0xCF: ("XCH A,R7",1),
    0xD0: ("POP d",  2), 0xD1: ("ACALL",2), 0xD2: ("SETB b",2),
    0xD3: ("SETB C", 1), 0xD4: ("DA A",  1), 0xD5: ("DJNZ d,r",3),
    0xD6: ("XCHD A,@R0",1), 0xD7: ("XCHD A,@R1",1),
    0xD8: ("DJNZ R0,r",2), 0xD9: ("DJNZ R1,r",2), 0xDA: ("DJNZ R2,r",2),
    0xDB: ("DJNZ R3,r",2), 0xDC: ("DJNZ R4,r",2), 0xDD: ("DJNZ R5,r",2),
    0xDE: ("DJNZ R6,r",2), 0xDF: ("DJNZ R7,r",2),
    0xE0: ("MOVX A,@DPTR",1), 0xE1: ("AJMP",2), 0xE2: ("MOVX A,@R0",1),
    0xE3: ("MOVX A,@R1",1), 0xE4: ("CLR A",1), 0xE5: ("MOV A,d",2),
    0xE6: ("MOV A,@R0",1), 0xE7: ("MOV A,@R1",1),
    0xE8: ("MOV A,R0",1), 0xE9: ("MOV A,R1",1), 0xEA: ("MOV A,R2",1),
    0xEB: ("MOV A,R3",1), 0xEC: ("MOV A,R4",1), 0xED: ("MOV A,R5",1),
    0xEE: ("MOV A,R6",1), 0xEF: ("MOV A,R7",1),
    0xF0: ("MOVX @DPTR,A",1), 0xF1: ("ACALL",2), 0xF2: ("MOVX @R0,A",1),
    0xF3: ("MOVX @R1,A",1), 0xF4: ("CPL A",1), 0xF5: ("MOV d,A",2),
    0xF6: ("MOV @R0,A",1), 0xF7: ("MOV @R1,A",1),
    0xF8: ("MOV R0,A",1), 0xF9: ("MOV R1,A",1), 0xFA: ("MOV R2,A",1),
    0xFB: ("MOV R3,A",1), 0xFC: ("MOV R4,A",1), 0xFD: ("MOV R5,A",1),
    0xFE: ("MOV R6,A",1), 0xFF: ("MOV R7,A",1),
}

def disasm_at(addr, n=20):
    """Disassemble n instructions starting at addr."""
    lines = []
    pc = addr
    for _ in range(n):
        if pc >= len(data):
            break
        op = data[pc]
        mnem, size = OPS.get(op, ("???", 1))
        raw = " ".join(f"{data[pc+i]:02x}" for i in range(min(size, len(data)-pc)))
        # Decode operands for jumps/calls
        extra = ""
        if size == 2 and pc+1 < len(data):
            imm = data[pc+1]
            if mnem in ("SJMP r", "JZ r", "JNZ r", "JC r", "JNC r"):
                rel = imm if imm < 128 else imm - 256
                extra = f"  -> 0x{(pc+2+rel)&0xFFFF:04X}"
            elif "CJNE" in mnem:
                rel = data[pc+2] if pc+2 < len(data) else 0
                rel = rel if rel < 128 else rel - 256
                extra = f"  -> 0x{(pc+3+rel)&0xFFFF:04X}"
            elif mnem.startswith("AJMP") or mnem.startswith("ACALL"):
                page = (pc+2) & 0xF800
                dest = page | ((op & 0xE0) << 3) | imm
                extra = f"  -> 0x{dest:04X}"
            else:
                extra = f"  #0x{imm:02X}"
        elif size == 3 and pc+2 < len(data):
            if "LJMP" in mnem or "LCALL" in mnem:
                dest = (data[pc+1] << 8) | data[pc+2]
                extra = f"  -> 0x{dest:04X}"
            elif "MOV DPTR" in mnem:
                dest = (data[pc+1] << 8) | data[pc+2]
                extra = f"  #0x{dest:04X}"
            elif "CJNE" in mnem:
                rel = data[pc+2]
                rel = rel if rel < 128 else rel - 256
                extra = f"  #0x{data[pc+1]:02X}, -> 0x{(pc+3+rel)&0xFFFF:04X}"
        lines.append(f"  {pc:04X}: {raw:<12} {mnem}{extra}")
        pc += size
    return lines

# ---------------------------------------------------------------------------
# Strategy 1: find CJNE Rn,#imm patterns — classic 8051 command dispatch
# Look for chains of CJNE Rx,#xx,rel followed by LJMP/LCALL
# ---------------------------------------------------------------------------
print("=" * 60)
print("STRATEGY 1: CJNE chains (command dispatch)")
print("=" * 60)

dispatch_hits = []
i = 0
while i < len(data) - 3:
    op = data[i]
    # CJNE Rn,#imm,rel: opcodes B8-BF
    if 0xB8 <= op <= 0xBF and i+2 < len(data):
        imm = data[i+1]
        # Collect consecutive CJNE instructions
        chain = []
        j = i
        while j < len(data) - 2:
            op2 = data[j]
            if 0xB8 <= op2 <= 0xBF:
                chain.append((j, op2, data[j+1]))
                j += 3
            elif op2 == 0xB4 and j+2 < len(data):  # CJNE A,#imm,rel
                chain.append((j, op2, data[j+1]))
                j += 3
            else:
                break
        if len(chain) >= 3:
            dispatch_hits.append((i, chain))
    i += 1

print(f"Found {len(dispatch_hits)} CJNE chain(s) with 3+ comparisons\n")
for addr, chain in dispatch_hits[:10]:
    cmds = [f"0x{c[2]:02X}" for c in chain]
    print(f"  @ 0x{addr:04X}: compares against [{', '.join(cmds)}]")
    for line in disasm_at(addr, 30):
        print(line)
    print()

# ---------------------------------------------------------------------------
# Strategy 2: JMP @A+DPTR (0x73) — jump table dispatch
# ---------------------------------------------------------------------------
print("=" * 60)
print("STRATEGY 2: JMP @A+DPTR (jump table dispatch)")
print("=" * 60)

jmp_hits = []
for i, b in enumerate(data):
    if b == 0x73:
        jmp_hits.append(i)

print(f"Found {len(jmp_hits)} JMP @A+DPTR instruction(s)\n")
for addr in jmp_hits:
    print(f"  @ 0x{addr:04X}:")
    # Show context before (likely DPTR load + table)
    for line in disasm_at(max(0, addr-30), 25):
        print(line)
    print()

# ---------------------------------------------------------------------------
# Strategy 3: scan for command byte constants 0x01-0x20 compared with CJNE A
# ---------------------------------------------------------------------------
print("=" * 60)
print("STRATEGY 3: CJNE A,#imm — A-register command comparisons")
print("=" * 60)

a_cjne = []
for i in range(len(data)-2):
    if data[i] == 0xB4:  # CJNE A,#imm,rel
        imm = data[i+1]
        if 0x01 <= imm <= 0x20:
            a_cjne.append((i, imm))

print(f"Found {len(a_cjne)} CJNE A,#imm with imm in 0x01-0x20\n")
for addr, imm in a_cjne[:30]:
    print(f"  0x{addr:04X}: CJNE A,#0x{imm:02X},rel")
    for line in disasm_at(addr, 8):
        print(line)
    print()
