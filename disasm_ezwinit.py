"""Disassemble ezwinit.sys to find firmware download patterns"""
import struct, os

os.chdir(r"C:\Users\yoshi\ezwriter-reverse\original_backup")

with open('ezwinit.sys', 'rb') as f:
    data = f.read()

pe_offset = struct.unpack('<I', data[0x3C:0x40])[0]
coff = pe_offset + 4
entry_rva = struct.unpack('<I', data[coff+16:coff+20])[0]
print(f'Entry point RVA: 0x{entry_rva:X}')

# Find all occurrences of 0x40 (Host-to-Device Vendor) + 0xA0 (Firmware Load)
print('\n=== 0x40 ... 0xA0 patterns (vendor firmware write) ===')
for i in range(len(data) - 20):
    if data[i] == 0x40:
        for j in range(i+1, min(i+16, len(data))):
            if data[j] == 0xA0:
                ctx = data[max(0,i-8):min(len(data),i+20)]
                hexstr = ctx.hex()
                print(f'  raw 0x{i:04X}: {hexstr[:48]}')
                break

# Find specific USB-related string constants
print('\n=== String references ===')
for i in range(len(data) - 8):
    try:
        s = data[i:i+40].decode('ascii', errors='replace')
        s_clean = ''.join(c if 32 <= ord(c) < 127 else ' ' for c in s)
        if 'ez' in s.lower():
            print(f'  0x{i:04X}: {s_clean.strip()[:50]}')
    except:
        pass
