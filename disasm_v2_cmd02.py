"""Disassemble an2131_fw_v2.bin cmd 0x02 handler"""
from pathlib import Path

FW = Path("src/ezwriter-cli/an2131_fw_v2.bin")
data = bytearray(FW.read_bytes())

OPS = {}
table = [
    (0x00,"NOP",1),(0x01,"AJMP",2),(0x02,"LJMP",3),(0x04,"INC A",1),
    (0x10,"JBC d,r",3),(0x11,"ACALL",2),(0x12,"LCALL",3),(0x20,"JB d,r",3),
    (0x22,"RET",1),(0x24,"ADD A,#i",2),(0x30,"JNB d,r",3),
    (0x40,"JC r",2),(0x50,"JNC r",2),(0x60,"JZ r",2),
    (0x70,"JNZ r",2),(0x74,"MOV A,#i",2),
    (0x78,"MOV R0,#i",2),(0x79,"MOV R1,#i",2),(0x7A,"MOV R2,#i",2),(0x7B,"MOV R3,#i",2),
    (0x7C,"MOV R4,#i",2),(0x7D,"MOV R5,#i",2),(0x7E,"MOV R6,#i",2),(0x7F,"MOV R7,#i",2),
    (0x80,"SJMP r",2),(0x81,"AJMP",2),
    (0x85,"MOV d,d",3),(0x88,"MOV d,R0",2),(0x89,"MOV d,R1",2),
    (0x8A,"MOV d,R2",2),(0x8B,"MOV d,R3",2),(0x8C,"MOV d,R4",2),(0x8D,"MOV d,R5",2),
    (0x8E,"MOV d,R6",2),(0x8F,"MOV d,R7",2),
    (0x90,"MOV DPTR,#i16",3),(0x93,"MOVC A,@A+DPTR",1),(0x94,"SUBB A,#i",2),
    (0xA3,"INC DPTR",1),(0xA8,"MOV R0,d",2),(0xA9,"MOV R1,d",2),
    (0xAA,"MOV R2,d",2),(0xAB,"MOV R3,d",2),(0xAC,"MOV R4,d",2),(0xAD,"MOV R5,d",2),
    (0xAE,"MOV R6,d",2),(0xAF,"MOV R7,d",2),
    (0xB4,"CJNE A,#i,r",3),(0xB5,"CJNE A,d,r",3),
    (0xBF,"CJNE R7,#i,r",3),
    (0xC0,"PUSH d",2),(0xC3,"CLR C",1),(0xD0,"POP d",2),(0xD3,"SETB C",1),
    (0xE0,"MOVX A,@DPTR",1),(0xE4,"CLR A",1),(0xE5,"MOV A,d",2),
    (0xE8,"MOV A,R0",1),(0xE9,"MOV A,R1",1),
    (0xEA,"MOV A,R2",1),(0xEB,"MOV A,R3",1),(0xEC,"MOV A,R4",1),(0xED,"MOV A,R5",1),
    (0xEE,"MOV A,R6",1),(0xEF,"MOV A,R7",1),
    (0xF0,"MOVX @DPTR,A",1),(0xF5,"MOV d,A",2),
    (0xF8,"MOV R0,A",1),(0xF9,"MOV R1,A",1),(0xFA,"MOV R2,A",1),(0xFB,"MOV R3,A",1),
    (0xFC,"MOV R4,A",1),(0xFD,"MOV R5,A",1),(0xFE,"MOV R6,A",1),(0xFF,"MOV R7,A",1),
]
for opcode, mnem, size in table:
    OPS[opcode] = (mnem, size)

def xram_bank(addr):
    if 0x7C00 <= addr < 0x7D00: return " EP2FIFO"
    if 0x7D00 <= addr < 0x7E00: return " EP4FIFO"
    if 0x7E00 <= addr < 0x7F00: return " EP6FIFO"
    return ""

def disasm_at(addr, n=80):
    pc = addr
    for _ in range(n):
        if pc >= len(data): break
        op = data[pc]
        mnem, size = OPS.get(op, (f"???({op:02x})",1))
        raw = " ".join(f"{data[pc+i]:02x}" for i in range(min(size, len(data)-pc)))
        extra = ""
        if size == 2 and pc+1 < len(data):
            imm = data[pc+1]
            if mnem in ("SJMP r","JZ r","JNZ r","JC r","JNC r"):
                rel = imm if imm < 128 else imm-256
                extra = f" -> 0x{(pc+2+rel)&0xFFFF:04X}"
            elif "AJMP" in mnem:
                dest = ((pc+2)&0xF800)|((op&0xE0)<<3)|imm
                extra = f" -> 0x{dest:04X}"
            else:
                extra = f" 0x{imm:02X}"
        elif size == 3 and pc+2 < len(data):
            if "LJMP" in mnem or "LCALL" in mnem:
                dest = (data[pc+1]<<8)|data[pc+2]
                extra = f" -> 0x{dest:04X}"
            elif "MOV DPTR" in mnem:
                dest = (data[pc+1]<<8)|data[pc+2]
                extra = f" #0x{dest:04X}{xram_bank(dest)}"
            elif "CJNE" in mnem:
                rel = data[pc+2]; rel = rel if rel<128 else rel-256
                extra = f" #0x{data[pc+1]:02X}, -> 0x{(pc+3+rel)&0xFFFF:04X}"
        print(f"{pc:04X}: {raw:<18} {mnem:<16} {extra}")
        pc += size

print("=== cmd 0x02 handler @ 0x0DC5 ===")
disasm_at(0x0DC5, 80)
