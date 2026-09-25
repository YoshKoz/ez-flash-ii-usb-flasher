"""Search for a table of (offset,size) or (ptr,size) entries referencing the
known multi-game block starts."""
import struct
import sys

ROM_BASE = 0x08000000
d = open(sys.argv[1], 'rb').read()
blocks = [0x0, 0x1080000, 0x1B50000, 0x1E40000, 0x1E80000]

# Search the loader region for any of these as 32-bit ROM pointers
for blk in blocks:
    if blk == 0:
        continue
    pat = struct.pack('<I', ROM_BASE + blk)
    start = 0
    print(f'--- pointer 0x{ROM_BASE + blk:08X} (block 0x{blk:06X}) ---')
    while True:
        i = d.find(pat, start)
        if i < 0:
            break
        ctx = d[i - 8:i + 16]
        words = ' '.join(f'{struct.unpack_from("<I", ctx, j)[0]:08X}' for j in range(0, 24, 4))
        print(f'  @0x{i:06X}: {words}')
        start = i + 4