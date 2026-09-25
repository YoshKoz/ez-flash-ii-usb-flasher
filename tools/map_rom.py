import sys

d = open(sys.argv[1], 'rb').read()
n = len(d)
pages = []
state = None
start = 0
for i in range(0, n, 0x10000):
    blk = d[i:i + 0x10000]
    blank = blk == (b'\xff' * len(blk))
    if state is None:
        state = blank
        start = i
    elif blank != state:
        pages.append((start, i, state))
        start = i
        state = blank
pages.append((start, n, state))
for s, e, blank in pages:
    kind = 'BLANK' if blank else 'DATA '
    print(f'0x{s:06X}-0x{e:06X}  {kind}  {e - s} bytes')