import usb.core, usb.util, time, sys

CMD_EP, DATA_EP = 0x04, 0x82
TYPE = 0x66  # FLASH handler
GEN3_SIG = bytes([0x25,0x20,0x01,0x08])

dev = usb.core.find(idVendor=0x0548, idProduct=0x1005)
if dev is None: print("NO DEVICE (0548:1005) - replug + re-init first"); sys.exit(1)
cfg = dev.get_active_configuration()
intf = cfg[(0,0)]
try: usb.util.claim_interface(dev, intf.bInterfaceNumber)
except Exception: pass

def w(data, to=1000): dev.write(CMD_EP, bytes(data), to)
def r(n=64, to=2000):
    try: return bytes(dev.read(DATA_EP, n, to))
    except usb.core.USBError: return None
def drain(to=120):
    c=0
    while r(64, to) is not None:
        c+=1
        if c>40: break
    return c
def hx(b,n=24): return "<none>" if b is None else ' '.join('%02x'%x for x in b[:n])

def sel(t=TYPE):
    w([0x14, t, 0x00]); time.sleep(0.05)

def read_chunk(off):
    # off = byte offset into save. b1=lo, b2=hi (16-bit within bank), b3=bank
    b1, b2, b3 = off&0xFF, (off>>8)&0xFF, (off>>16)&0xFF
    drain(60)
    # cmd 0x02: select save chip + bank + flash-ready check
    w([0x02, b1, b2, b3, TYPE]); time.sleep(0.02)
    # cmd 0x03: stream 64 bytes at 16-bit addr (b1:b2)
    w([0x03, b1, b2, 0x00, 0x00]); time.sleep(0.01)
    return r(64, 3000)

mode = sys.argv[1] if len(sys.argv)>1 else 'peek'

print("predrain:", drain())
sel()
drain()

if mode == 'peek':
    print("=== first 8 chunks (clean native flash read) ===")
    for i in range(8):
        d = read_chunk(i*64)
        flag = ' <SIG!>' if d and GEN3_SIG in d else ''
        print(f"chunk {i} off 0x{i*64:04x}: {hx(d)}{flag}")
    sys.exit(0)

# full dump
out = bytearray()
N = 2048  # 128KB / 64
t0=time.time()
for i in range(N):
    d = read_chunk(i*64)
    if d is None:
        print(f"\nTIMEOUT at chunk {i} (off 0x{i*64:05x}) - flash may have left read-array")
        break
    out += d
    if i % 128 == 0:
        print(f"  {i*64:6d}/{N*64} bytes  [{hx(d,12)}]")
sigs = sum(1 for j in range(len(out)-3) if out[j:j+4]==GEN3_SIG)
print(f"\nread {len(out)} bytes in {time.time()-t0:.1f}s, Gen3 sigs={sigs}")
path = r'C:\Development\ez-flash-ii-usb-flasher\src\ezwriter-cli\sapphire.sav'
open(path,'wb').write(out)
print("wrote", path)
