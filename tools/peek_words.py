import struct
import sys

d = open(sys.argv[1], 'rb').read()
for off in [int(x, 0) for x in sys.argv[2:]]:
    print(f'--- 0x{off:06X} ---')
    for r in range(0, 0x40, 16):
        c = d[off + r:off + r + 16]
        words = ' '.join(f'{struct.unpack_from("<I", c, i)[0]:08X}' for i in range(0, 16, 4))
        print(f'  +{r:02X}: {words}')