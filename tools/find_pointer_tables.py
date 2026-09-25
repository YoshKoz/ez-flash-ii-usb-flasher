"""Find pointer tables in the EZLoader region.

Looks for runs of 32-bit values that are valid GBA-ROM pointers
(0x08000000 + [0, image size)), optionally followed by sizes.
"""
import struct
import sys

ROM_BASE = 0x08000000
d = open(sys.argv[1], 'rb').read()
limit = int(sys.argv[2], 0) if len(sys.argv) > 2 else 0x100000
size = len(d)

runs = []
cur = []
for i in range(0, limit - 4, 4):
    v = struct.unpack_from('<I', d, i)[0]
    off = v - ROM_BASE
    if ROM_BASE <= v and 0 <= off < size:
        cur.append((i, v))
    else:
        if len(cur) >= 3:
            runs.append(cur)
        cur = []
if len(cur) >= 3:
    runs.append(cur)

for r in runs:
    print(f'table @0x{r[0][0]:06X}: {len(r)} pointers')
    for i, v in r[:12]:
        print(f'   +{i - r[0][0]:3d}: 0x{v:08X}  (offset 0x{v - ROM_BASE:06X})')