import struct, sys

fw = open('tusbez.bin','rb').read()
loader = open('loader_table2.bin','rb').read()

# apply patches
count = struct.unpack_from('<H', loader, 8)[0]
off = 10
buf = bytearray(fw)
for _ in range(count):
    addr = struct.unpack_from('<H', loader, off)[0]
    ln = loader[off+2]
    off += 3
    buf[addr:addr+ln] = loader[off:off+ln]
    off += ln

def dis(start, length=512):
    pc = start
    end = start + length
    out = []
    while pc < end and pc < len(buf):
        op = buf[pc]
        if op == 0x02:
            a = (buf[pc+1]<<8)|buf[pc+2]; out.append(f'  {pc:04X}: LJMP      0x{a:04X}'); pc+=3
        elif op == 0x12:
            a = (buf[pc+1]<<8)|buf[pc+2]; out.append(f'  {pc:04X}: LCALL     0x{a:04X}'); pc+=3
        elif op == 0x22:
            out.append(f'  {pc:04X}: RET'); pc+=1
        elif op == 0x80:
            r = buf[pc+1]; t = pc+2+(r if r<128 else r-256); out.append(f'  {pc:04X}: SJMP      0x{t:04X}'); pc+=2
        elif op == 0x70:
            r = buf[pc+1]; t = pc+2+(r if r<128 else r-256); out.append(f'  {pc:04X}: JNZ       0x{t:04X}'); pc+=2
        elif op == 0x60:
            r = buf[pc+1]; t = pc+2+(r if r<128 else r-256); out.append(f'  {pc:04X}: JZ        0x{t:04X}'); pc+=2
        elif op == 0x20:
            b=buf[pc+1]; r=buf[pc+2]; t=pc+3+(r if r<128 else r-256)
            out.append(f'  {pc:04X}: JB        bit_{b:02X}, 0x{t:04X}'); pc+=3
        elif op == 0x30:
            b=buf[pc+1]; r=buf[pc+2]; t=pc+3+(r if r<128 else r-256)
            out.append(f'  {pc:04X}: JNB       bit_{b:02X}, 0x{t:04X}'); pc+=3
        elif op == 0x10:
            b=buf[pc+1]; r=buf[pc+2]; t=pc+3+(r if r<128 else r-256)
            out.append(f'  {pc:04X}: JBC       bit_{b:02X}, 0x{t:04X}'); pc+=3
        elif op == 0xb4:
            imm=buf[pc+1]; r=buf[pc+2]; t=pc+3+(r if r<128 else r-256)
            out.append(f'  {pc:04X}: CJNE      A,#0x{imm:02X}, 0x{t:04X}'); pc+=3
        elif op == 0xb5:
            dr=buf[pc+1]; r=buf[pc+2]; t=pc+3+(r if r<128 else r-256)
            out.append(f'  {pc:04X}: CJNE      A,[0x{dr:02X}], 0x{t:04X}'); pc+=3
        elif op == 0xd5:
            dr=buf[pc+1]; r=buf[pc+2]; t=pc+3+(r if r<128 else r-256)
            out.append(f'  {pc:04X}: DJNZ      [0x{dr:02X}], 0x{t:04X}'); pc+=3
        elif op == 0x90:
            a=(buf[pc+1]<<8)|buf[pc+2]; out.append(f'  {pc:04X}: MOV       DPTR,#0x{a:04X}'); pc+=3
        elif op == 0xe0: out.append(f'  {pc:04X}: MOVX      A,@DPTR'); pc+=1
        elif op == 0xf0: out.append(f'  {pc:04X}: MOVX      @DPTR,A'); pc+=1
        elif op == 0x74: out.append(f'  {pc:04X}: MOV       A,#0x{buf[pc+1]:02X}'); pc+=2
        elif op == 0xe4: out.append(f'  {pc:04X}: CLR       A'); pc+=1
        elif op == 0xf5: out.append(f'  {pc:04X}: MOV       [{0}],A  ; direct 0x{buf[pc+1]:02X}'.format(f'r{buf[pc+1]:02X}')); pc+=2
        elif op == 0xe5: out.append(f'  {pc:04X}: MOV       A,[r{buf[pc+1]:02X}]  ; direct 0x{buf[pc+1]:02X}'); pc+=2
        elif op == 0xa3: out.append(f'  {pc:04X}: INC       DPTR'); pc+=1
        elif op == 0x04: out.append(f'  {pc:04X}: INC       A'); pc+=1
        elif op == 0x14: out.append(f'  {pc:04X}: DEC       A'); pc+=1
        elif op == 0x44: out.append(f'  {pc:04X}: ORL       A,#0x{buf[pc+1]:02X}'); pc+=2
        elif op == 0x54: out.append(f'  {pc:04X}: ANL       A,#0x{buf[pc+1]:02X}'); pc+=2
        elif op == 0x64: out.append(f'  {pc:04X}: XRL       A,#0x{buf[pc+1]:02X}'); pc+=2
        elif op == 0x65: out.append(f'  {pc:04X}: XRL       A,[r{buf[pc+1]:02X}]  ; direct 0x{buf[pc+1]:02X}'); pc+=2
        elif op == 0x75: out.append(f'  {pc:04X}: MOV       [r{buf[pc+1]:02X}],#0x{buf[pc+2]:02X}'); pc+=3
        elif op == 0xc3: out.append(f'  {pc:04X}: CLR       C'); pc+=1
        elif op == 0x33: out.append(f'  {pc:04X}: RLC       A'); pc+=1
        elif op == 0x03: out.append(f'  {pc:04X}: RR        A'); pc+=1
        elif op == 0x13: out.append(f'  {pc:04X}: RRC       A'); pc+=1
        elif op == 0xef: out.append(f'  {pc:04X}: MOV       A,R7'); pc+=1
        elif op == 0xee: out.append(f'  {pc:04X}: MOV       A,R6'); pc+=1
        elif op == 0xed: out.append(f'  {pc:04X}: MOV       A,R5'); pc+=1
        elif op == 0xec: out.append(f'  {pc:04X}: MOV       A,R4'); pc+=1
        elif op == 0x25: out.append(f'  {pc:04X}: ADD       A,[r{buf[pc+1]:02X}]'); pc+=2
        elif op == 0x24: out.append(f'  {pc:04X}: ADD       A,#0x{buf[pc+1]:02X}'); pc+=2
        elif op == 0xc0: out.append(f'  {pc:04X}: PUSH      [0x{buf[pc+1]:02X}]'); pc+=2
        elif op == 0xd0: out.append(f'  {pc:04X}: POP       [0x{buf[pc+1]:02X}]'); pc+=2
        else:
            out.append(f'  {pc:04X}: [{op:02X} {buf[pc+1]:02X} {buf[pc+2]:02X}] ???'); pc+=1
    return '\n'.join(out)

print('=== FLASH128 handler (LCALL 0x114A) ===')
print(dis(0x114A, 400))
print()
print('=== 0x11F6 ===')
print(dis(0x11F6, 200))
