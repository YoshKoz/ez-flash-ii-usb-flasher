import struct

fw = open('tusbez.bin', 'rb').read()
loader = open('loader_table2.bin', 'rb').read()

count = struct.unpack_from('<H', loader, 8)[0]
off = 10
buf = bytearray(fw)
for _ in range(count):
    addr = struct.unpack_from('<H', loader, off)[0]
    ln = loader[off + 2]; off += 3
    buf[addr:addr + ln] = loader[off:off + ln]; off += ln

def dis(start, length=160):
    pc = start; end = start + length; lines = []
    while pc < end and pc < len(buf):
        op = buf[pc]
        b1 = buf[pc+1] if pc+1 < len(buf) else 0
        b2 = buf[pc+2] if pc+2 < len(buf) else 0
        def rel2(r): return pc + 2 + (r if r < 128 else r - 256)
        def rel3(r): return pc + 3 + (r if r < 128 else r - 256)
        def a16(): return (b1<<8)|b2
        if   op==0x02: lines.append(f'{pc:04X}: LJMP  0x{a16():04X}'); pc+=3
        elif op==0x12: lines.append(f'{pc:04X}: LCALL 0x{a16():04X}'); pc+=3
        elif op==0x22: lines.append(f'{pc:04X}: RET'); pc+=1
        elif op==0x80: lines.append(f'{pc:04X}: SJMP  0x{rel2(b1):04X}'); pc+=2
        elif op==0x70: lines.append(f'{pc:04X}: JNZ   0x{rel2(b1):04X}'); pc+=2
        elif op==0x60: lines.append(f'{pc:04X}: JZ    0x{rel2(b1):04X}'); pc+=2
        elif op==0x73: lines.append(f'{pc:04X}: JMP   @A+DPTR'); pc+=1
        elif op==0xb4: lines.append(f'{pc:04X}: CJNE  A,#{b1:02X},0x{rel3(b2):04X}'); pc+=3
        elif op==0xd5: lines.append(f'{pc:04X}: DJNZ  [{b1:02X}],0x{rel3(b2):04X}'); pc+=3
        elif op==0x90: lines.append(f'{pc:04X}: MOV   DPTR,#0x{a16():04X}'); pc+=3
        elif op==0xe0: lines.append(f'{pc:04X}: MOVX  A,@DPTR'); pc+=1
        elif op==0xf0: lines.append(f'{pc:04X}: MOVX  @DPTR,A'); pc+=1
        elif op==0x74: lines.append(f'{pc:04X}: MOV   A,#0x{b1:02X}'); pc+=2
        elif op==0xe4: lines.append(f'{pc:04X}: CLR   A'); pc+=1
        elif op==0xf5: lines.append(f'{pc:04X}: MOV   [0x{b1:02X}],A'); pc+=2
        elif op==0xe5: lines.append(f'{pc:04X}: MOV   A,[0x{b1:02X}]'); pc+=2
        elif op==0xa3: lines.append(f'{pc:04X}: INC   DPTR'); pc+=1
        elif op==0x04: lines.append(f'{pc:04X}: INC   A'); pc+=1
        elif op==0x14: lines.append(f'{pc:04X}: DEC   A'); pc+=1
        elif op==0x44: lines.append(f'{pc:04X}: ORL   A,#0x{b1:02X}'); pc+=2
        elif op==0x54: lines.append(f'{pc:04X}: ANL   A,#0x{b1:02X}'); pc+=2
        elif op==0x64: lines.append(f'{pc:04X}: XRL   A,#0x{b1:02X}'); pc+=2
        elif op==0x65: lines.append(f'{pc:04X}: XRL   A,[0x{b1:02X}]'); pc+=2
        elif op==0x75: lines.append(f'{pc:04X}: MOV   [0x{b1:02X}],#0x{b2:02X}'); pc+=3
        elif op==0xc3: lines.append(f'{pc:04X}: CLR   C'); pc+=1
        elif op==0x33: lines.append(f'{pc:04X}: RLC   A'); pc+=1
        elif op==0x03: lines.append(f'{pc:04X}: RR    A'); pc+=1
        elif op==0x13: lines.append(f'{pc:04X}: RRC   A'); pc+=1
        elif op==0x24: lines.append(f'{pc:04X}: ADD   A,#0x{b1:02X}'); pc+=2
        elif op==0x34: lines.append(f'{pc:04X}: ADDC  A,#0x{b1:02X}'); pc+=2
        elif op==0x25: lines.append(f'{pc:04X}: ADD   A,[0x{b1:02X}]'); pc+=2
        elif op==0xc0: lines.append(f'{pc:04X}: PUSH  [0x{b1:02X}]'); pc+=2
        elif op==0xd0: lines.append(f'{pc:04X}: POP   [0x{b1:02X}]'); pc+=2
        elif op==0xef: lines.append(f'{pc:04X}: MOV   A,R7'); pc+=1
        elif op==0xee: lines.append(f'{pc:04X}: MOV   A,R6'); pc+=1
        elif op==0xed: lines.append(f'{pc:04X}: MOV   A,R5'); pc+=1
        elif op==0xec: lines.append(f'{pc:04X}: MOV   A,R4'); pc+=1
        elif op==0xf8: lines.append(f'{pc:04X}: MOV   R0,A'); pc+=1
        elif op==0xf9: lines.append(f'{pc:04X}: MOV   R1,A'); pc+=1
        elif op==0xfa: lines.append(f'{pc:04X}: MOV   R2,A'); pc+=1
        elif op==0xfb: lines.append(f'{pc:04X}: MOV   R3,A'); pc+=1
        elif op==0xfc: lines.append(f'{pc:04X}: MOV   R4,A'); pc+=1
        elif op==0xfd: lines.append(f'{pc:04X}: MOV   R5,A'); pc+=1
        elif op==0xfe: lines.append(f'{pc:04X}: MOV   R6,A'); pc+=1
        elif op==0xff: lines.append(f'{pc:04X}: MOV   R7,A'); pc+=1
        elif op==0xe8: lines.append(f'{pc:04X}: MOV   A,R0'); pc+=1
        elif op==0xe9: lines.append(f'{pc:04X}: MOV   A,R1'); pc+=1
        elif op==0xea: lines.append(f'{pc:04X}: MOV   A,R2'); pc+=1
        elif op==0xeb: lines.append(f'{pc:04X}: MOV   A,R3'); pc+=1
        elif op==0xd3: lines.append(f'{pc:04X}: SETB  C'); pc+=1
        elif op==0x94: lines.append(f'{pc:04X}: SUBB  A,#0x{b1:02X}'); pc+=2
        elif op==0x20: t=pc+3+(b2 if b2<128 else b2-256); lines.append(f'{pc:04X}: JB    {b1:02X},0x{t:04X}'); pc+=3
        elif op==0x30: t=pc+3+(b2 if b2<128 else b2-256); lines.append(f'{pc:04X}: JNB   {b1:02X},0x{t:04X}'); pc+=3
        else: lines.append(f'{pc:04X}: [{op:02X} {b1:02X} {b2:02X}] ???'); pc+=1
    return '\n'.join(lines)

print('=== cmd 0x03 handler at 0x0A09 ===')
print(dis(0x0A09, 120))
print()
print('=== cmd 0x04 handler at 0x0A82 ===')
print(dis(0x0A82, 80))
print()
print('=== cmd 0x05 handler at 0x0A9F ===')
print(dis(0x0A9F, 80))
print()
print('=== cmd 0x06 handler at 0x0AB8 ===')
print(dis(0x0AB8, 80))
print()
print('=== cmd 0x14 handler at 0x0AE1 (full) ===')
print(dis(0x0AE1, 30))
print()
print('=== cmd 0x19 handler at 0x0AF3 (full) ===')
print(dis(0x0AF3, 140))
print()
print('=== cmd 0x1A handler at 0x0B42 ===')
print(dis(0x0B42, 100))
print()
print('=== 0x0C12 (common exit path) ===')
print(dis(0x0C12, 60))
print()
print('=== 0x172D (EP2 fill for ROM read) ===')
print(dis(0x172D, 80))
