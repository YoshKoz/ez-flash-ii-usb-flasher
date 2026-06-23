import struct
p = r'C:\Development\ez-flash-ii-usb-flasher\src\ezwriter-cli\sapphire.sav'
d = open(p,'rb').read()
print("len", len(d))
b0, b1 = d[:65536], d[65536:]
print("bank0==bank1 ?", b0==b1)
# Gen3: 14 sections of 0x1000; footer at +0xFF4: section_id(2), checksum(2), signature(4)=0x08012025, save_index(4)
def parse_block(blk, base):
    print(f"-- block @0x{base:05x} --")
    for s in range(16):
        off = s*0x1000
        if off+0x1000 > len(blk): break
        sec = blk[off:off+0x1000]
        sid, chk = struct.unpack_from('<HH', sec, 0xFF4)
        sig, idx = struct.unpack_from('<II', sec, 0xFF8)
        mark = 'OK' if sig==0x08012025 else '--'
        if sig==0x08012025 or sid<14:
            print(f"  sec{s:2d} id={sid:5d} chk=0x{chk:04x} sig={sig:08x} {mark} saveidx={idx}")
parse_block(d[0:0x10000], 0)
# also check two 57344 save slots layout (slot A 0x0000, slot B 0xE000)
print("\nfirst 16 bytes each 4KB section, bank0:")
for s in range(16):
    print(f"  sec{s:2d}: {' '.join('%02x'%x for x in d[s*0x1000:s*0x1000+8])}  footer:{' '.join('%02x'%x for x in d[s*0x1000+0xFF4:s*0x1000+0x1000])}")
