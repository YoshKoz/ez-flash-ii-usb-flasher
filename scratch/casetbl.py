import struct, os
BASE = r'C:\Development\ez-flash-ii-usb-flasher\src\ezwriter-cli'
fw = bytearray(open(os.path.join(BASE,'an2131_fw_v2.bin'),'rb').read())
if len(fw)<0x4000: fw += bytes(0x4000-len(fw))
def apply(p):
    d=open(p,'rb').read(); assert d[:8]==b'EZWLDR1\0'
    cnt=struct.unpack_from('<H',d,8)[0]; off=10
    for _ in range(cnt):
        a=struct.unpack_from('<H',d,off)[0]; ln=d[off+2]; off+=3
        fw[a:a+ln]=d[off:off+ln]; off+=ln
apply(os.path.join(BASE,'loader_table1.bin')); apply(os.path.join(BASE,'loader_table2.bin'))

# raw dump around dispatch table
print("raw 0x0733..0x0760:")
for a in range(0x0733,0x0762):
    print("  %04X: %02X"%(a,fw[a]))

# Keil ?C?CASE table at 0x0736: format is series of records.
# Common Keil layout: WORD default_addr? Actually 0x15CE variant.
# Try interpret as triples [val, addrHi, addrLo] until terminator.
print("\nInterpret as [val:1][addrHi:1][addrLo:1] triples from 0x0736:")
off=0x0736
for i in range(12):
    v=fw[off]; hi=fw[off+1]; lo=fw[off+2]
    print("  val=0x%02X -> 0x%04X"%(v,(hi<<8)|lo))
    off+=3
