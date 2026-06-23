import struct

base = 'C:/Development/ez-flash-ii-usb-flasher/src/ezwriter-cli'
with open(f'{base}/an2131_fw_v2.bin', 'rb') as f:
    v2 = bytearray(f.read())
with open(f'{base}/loader_table2.bin', 'rb') as f:
    data = f.read()
if data[:8] != b'EZWLDR1\0':
    raise SystemExit('Bad sig')
count = data[8] | (data[9] << 8)
patched = bytearray(v2)
offset = 10
for i in range(count):
    addr = data[offset] | (data[offset+1] << 8)
    length = data[offset+2]
    payload = data[offset+3:offset+3+length]
    for j, b in enumerate(payload):
        if addr + j < len(patched):
            patched[addr + j] = b
    offset += 3 + length

def hex_dump(data, start, length):
    for i in range(start, min(start+length, len(data))):
        if (i-start) % 16 == 0:
            print(f'{i:04X}: ', end='')
        print(f'{data[i]:02x} ', end='')
        if (i-start) % 16 == 15:
            print()
    print()

print('=== USB vectors area (0x0040-0x0050) ===')
hex_dump(patched, 0x40, 0x20)

print('=== vector table at 0x1600-0x1620 ===')
hex_dump(patched, 0x1600, 0x20)

print('=== 0x1850 (EP4 OUT?) ===')
hex_dump(patched, 0x1850, 0x40)

print('=== 0x182B ===')
hex_dump(patched, 0x1825, 0x30)

print('=== 0x1804 ===')
hex_dump(patched, 0x1800, 0x30)

print('=== 0x17DD ===')
hex_dump(patched, 0x17DD, 0x30)

print('=== 0x1658 ===')
hex_dump(patched, 0x1658, 0x20)

# Find LCALL/LJMP 0x15CE
print('=== Calls to 0x15CE ===')
for i in range(len(patched)-2):
    if patched[i] == 0x12 and patched[i+1] == 0x15 and patched[i+2] == 0xCE:
        pre = patched[max(0,i-8):i]
        pre_hex = ' '.join(f'{b:02x}' for b in pre)
        print(f'  LCALL 0x15CE at 0x{i:04X}: pre={pre_hex}')
