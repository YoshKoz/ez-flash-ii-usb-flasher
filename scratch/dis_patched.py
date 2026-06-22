import struct, sys, os

BASE = r'C:\Development\ez-flash-ii-usb-flasher\src\ezwriter-cli'
fw = bytearray(open(os.path.join(BASE,'an2131_fw_v2.bin'),'rb').read())
# pad to 16K so patches at high addr fit
if len(fw) < 0x4000:
    fw += bytes(0x4000 - len(fw))

def apply(path):
    d = open(path,'rb').read()
    assert d[:8] == b'EZWLDR1\0', (path, d[:8])
    count = struct.unpack_from('<H', d, 8)[0]
    off = 10
    for _ in range(count):
        addr = struct.unpack_from('<H', d, off)[0]
        ln = d[off+2]; off += 3
        fw[addr:addr+ln] = d[off:off+ln]; off += ln

apply(os.path.join(BASE,'loader_table1.bin'))
apply(os.path.join(BASE,'loader_table2.bin'))
buf = fw

def dis(start, length=256):
    pc = start; end = start+length; out=[]
    while pc < end and pc < len(buf):
        op = buf[pc]
        def b1(): return buf[pc+1]
        def b2(): return buf[pc+2]
        def rel(r): return pc+2+(r if r<128 else r-256)
        def rel3(r): return pc+3+(r if r<128 else r-256)
        def a16(): return (buf[pc+1]<<8)|buf[pc+2]
        # AJMP/ACALL (2-byte, page in top 3 bits of opcode)
        if (op & 0x1f) == 0x01:  # AJMP
            t = (pc+2 & 0xF800) | ((op>>5)<<8) | b1(); out.append(f'{pc:04X}: AJMP  0x{t:04X}'); pc+=2; continue
        if (op & 0x1f) == 0x11:  # ACALL
            t = (pc+2 & 0xF800) | ((op>>5)<<8) | b1(); out.append(f'{pc:04X}: ACALL 0x{t:04X}'); pc+=2; continue
        if op==0x02: out.append(f'{pc:04X}: LJMP  0x{a16():04X}'); pc+=3
        elif op==0x12: out.append(f'{pc:04X}: LCALL 0x{a16():04X}'); pc+=3
        elif op==0x22: out.append(f'{pc:04X}: RET'); pc+=1
        elif op==0x32: out.append(f'{pc:04X}: RETI'); pc+=1
        elif op==0x00: out.append(f'{pc:04X}: NOP'); pc+=1
        elif op==0x80: out.append(f'{pc:04X}: SJMP  0x{rel(b1()):04X}'); pc+=2
        elif op==0x70: out.append(f'{pc:04X}: JNZ   0x{rel(b1()):04X}'); pc+=2
        elif op==0x60: out.append(f'{pc:04X}: JZ    0x{rel(b1()):04X}'); pc+=2
        elif op==0x40: out.append(f'{pc:04X}: JC    0x{rel(b1()):04X}'); pc+=2
        elif op==0x50: out.append(f'{pc:04X}: JNC   0x{rel(b1()):04X}'); pc+=2
        elif op==0x20: out.append(f'{pc:04X}: JB    {b1():02X},0x{rel3(b2()):04X}'); pc+=3
        elif op==0x30: out.append(f'{pc:04X}: JNB   {b1():02X},0x{rel3(b2()):04X}'); pc+=3
        elif op==0x10: out.append(f'{pc:04X}: JBC   {b1():02X},0x{rel3(b2()):04X}'); pc+=3
        elif op==0xb4: out.append(f'{pc:04X}: CJNE  A,#{b1():02X},0x{rel3(b2()):04X}'); pc+=3
        elif op==0xb5: out.append(f'{pc:04X}: CJNE  A,[{b1():02X}],0x{rel3(b2()):04X}'); pc+=3
        elif 0xb8<=op<=0xbf: out.append(f'{pc:04X}: CJNE  R{op-0xb8},#{b1():02X},0x{rel3(b2()):04X}'); pc+=3
        elif op==0xd5: out.append(f'{pc:04X}: DJNZ  [{b1():02X}],0x{rel3(b2()):04X}'); pc+=3
        elif 0xd8<=op<=0xdf: out.append(f'{pc:04X}: DJNZ  R{op-0xd8},0x{rel(b1()):04X}'); pc+=2
        elif op==0x73: out.append(f'{pc:04X}: JMP   @A+DPTR  ;*** JUMPTABLE ***'); pc+=1
        elif op==0x83: out.append(f'{pc:04X}: MOVC  A,@A+PC'); pc+=1
        elif op==0x93: out.append(f'{pc:04X}: MOVC  A,@A+DPTR'); pc+=1
        elif op==0x90: out.append(f'{pc:04X}: MOV   DPTR,#0x{a16():04X}'); pc+=3
        elif op==0xe0: out.append(f'{pc:04X}: MOVX  A,@DPTR'); pc+=1
        elif op==0xf0: out.append(f'{pc:04X}: MOVX  @DPTR,A'); pc+=1
        elif op==0xe2: out.append(f'{pc:04X}: MOVX  A,@R0'); pc+=1
        elif op==0xe3: out.append(f'{pc:04X}: MOVX  A,@R1'); pc+=1
        elif op==0xf2: out.append(f'{pc:04X}: MOVX  @R0,A'); pc+=1
        elif op==0xf3: out.append(f'{pc:04X}: MOVX  @R1,A'); pc+=1
        elif op==0x74: out.append(f'{pc:04X}: MOV   A,#0x{b1():02X}'); pc+=2
        elif op==0xe4: out.append(f'{pc:04X}: CLR   A'); pc+=1
        elif op==0xf5: out.append(f'{pc:04X}: MOV   [{b1():02X}],A'); pc+=2
        elif op==0xe5: out.append(f'{pc:04X}: MOV   A,[{b1():02X}]'); pc+=2
        elif op==0x85: out.append(f'{pc:04X}: MOV   [{b2():02X}],[{b1():02X}]'); pc+=3
        elif op==0x75: out.append(f'{pc:04X}: MOV   [{b1():02X}],#0x{b2():02X}'); pc+=3
        elif 0x78<=op<=0x7f: out.append(f'{pc:04X}: MOV   R{op-0x78},#0x{b1():02X}'); pc+=2
        elif 0xa8<=op<=0xaf: out.append(f'{pc:04X}: MOV   R{op-0xa8},[{b1():02X}]'); pc+=2
        elif 0x88<=op<=0x8f: out.append(f'{pc:04X}: MOV   [{b1():02X}],R{op-0x88}'); pc+=2
        elif op==0xa3: out.append(f'{pc:04X}: INC   DPTR'); pc+=1
        elif op==0x04: out.append(f'{pc:04X}: INC   A'); pc+=1
        elif op==0x05: out.append(f'{pc:04X}: INC   [{b1():02X}]'); pc+=2
        elif 0x08<=op<=0x0f: out.append(f'{pc:04X}: INC   R{op-0x08}'); pc+=1
        elif op==0x14: out.append(f'{pc:04X}: DEC   A'); pc+=1
        elif 0x18<=op<=0x1f: out.append(f'{pc:04X}: DEC   R{op-0x18}'); pc+=1
        elif op==0x44: out.append(f'{pc:04X}: ORL   A,#0x{b1():02X}'); pc+=2
        elif op==0x45: out.append(f'{pc:04X}: ORL   A,[{b1():02X}]'); pc+=2
        elif op==0x42: out.append(f'{pc:04X}: ORL   [{b1():02X}],A'); pc+=2
        elif op==0x43: out.append(f'{pc:04X}: ORL   [{b1():02X}],#0x{b2():02X}'); pc+=3
        elif op==0x54: out.append(f'{pc:04X}: ANL   A,#0x{b1():02X}'); pc+=2
        elif op==0x55: out.append(f'{pc:04X}: ANL   A,[{b1():02X}]'); pc+=2
        elif op==0x52: out.append(f'{pc:04X}: ANL   [{b1():02X}],A'); pc+=2
        elif op==0x64: out.append(f'{pc:04X}: XRL   A,#0x{b1():02X}'); pc+=2
        elif op==0x65: out.append(f'{pc:04X}: XRL   A,[{b1():02X}]'); pc+=2
        elif op==0x62: out.append(f'{pc:04X}: XRL   [{b1():02X}],A'); pc+=2
        elif op==0x75: out.append(f'{pc:04X}: MOV   [{b1():02X}],#0x{b2():02X}'); pc+=3
        elif op==0xc3: out.append(f'{pc:04X}: CLR   C'); pc+=1
        elif op==0xd3: out.append(f'{pc:04X}: SETB  C'); pc+=1
        elif op==0xc2: out.append(f'{pc:04X}: CLR   {b1():02X}'); pc+=2
        elif op==0xd2: out.append(f'{pc:04X}: SETB  {b1():02X}'); pc+=2
        elif op==0xb2: out.append(f'{pc:04X}: CPL   {b1():02X}'); pc+=2
        elif op==0x33: out.append(f'{pc:04X}: RLC   A'); pc+=1
        elif op==0x23: out.append(f'{pc:04X}: RL    A'); pc+=1
        elif op==0x03: out.append(f'{pc:04X}: RR    A'); pc+=1
        elif op==0x13: out.append(f'{pc:04X}: RRC   A'); pc+=1
        elif 0xe8<=op<=0xef: out.append(f'{pc:04X}: MOV   A,R{op-0xe8}'); pc+=1
        elif 0xf8<=op<=0xff: out.append(f'{pc:04X}: MOV   R{op-0xf8},A'); pc+=1
        elif op==0x25: out.append(f'{pc:04X}: ADD   A,[{b1():02X}]'); pc+=2
        elif op==0x24: out.append(f'{pc:04X}: ADD   A,#0x{b1():02X}'); pc+=2
        elif op==0x34: out.append(f'{pc:04X}: ADDC  A,#0x{b1():02X}'); pc+=2
        elif op==0x94: out.append(f'{pc:04X}: SUBB  A,#0x{b1():02X}'); pc+=2
        elif op==0xc0: out.append(f'{pc:04X}: PUSH  [{b1():02X}]'); pc+=2
        elif op==0xd0: out.append(f'{pc:04X}: POP   [{b1():02X}]'); pc+=2
        elif op==0xc5: out.append(f'{pc:04X}: XCH   A,[{b1():02X}]'); pc+=2
        elif op==0x7f: out.append(f'{pc:04X}: MOV   R7,#0x{b1():02X}'); pc+=2
        else: out.append(f'{pc:04X}: [{op:02X} {buf[pc+1]:02X} {buf[pc+2]:02X}] ???'); pc+=1
    return '\n'.join(out)

what = sys.argv[1] if len(sys.argv)>1 else 'save'
if what=='save':
    print('=== save handler region 0x07C0..0x0860 ===')
    print(dis(0x07C0, 0xA0))
elif what=='disp':
    print('=== dispatch 0x0700..0x07C0 ===')
    print(dis(0x0700, 0xC0))
elif what=='jt':
    for pc in range(0,len(buf)):
        if buf[pc]==0x73:
            print(f'{pc:04X}: JMP @A+DPTR'); print(dis(max(0,pc-40),45))
            print('---')
else:
    a=int(what,16); print(dis(a, int(sys.argv[2],16) if len(sys.argv)>2 else 0x80))
