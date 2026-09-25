import sys
import re

d = open(sys.argv[1], 'rb').read()
n = len(d)

# GBA header title is 12 bytes at 0xA0; require a plausible title.
def title_at(off):
    t = d[off + 0xA0:off + 0xAC]
    return t

# Find candidate ROM starts: Nintendo logo at 0x04 of a GBA ROM.
LOGO = bytes.fromhex(
    '24ffae51699aa2213d84820a84e409ad11248b98c0817f21a352be199309'
    'd3c1f8e9ce4959d2')
def logo_ok(off):
    return d[off + 4:off + 4 + 0x9C].startswith(LOGO) or d[off + 4:off + 4 + len(LOGO)] == LOGO

print('candidate GBA ROM starts (logo check + title):')
for i in range(0, n - 0x100, 4):
    if d[i] == 0x2E and d[i + 1] == 0x00 and d[i + 2] == 0x00 and d[i + 3] == 0xEA:
        t = title_at(i)
        if re.fullmatch(rb'[A-Za-z0-9 ]{1,12}', t) and t.strip():
            print(f'  0x{i:06X}  title={t!r}')