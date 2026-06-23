import usb.core, usb.util, time, sys

CMD_EP, DATA_EP = 0x04, 0x82
dev = usb.core.find(idVendor=0x0548, idProduct=0x1005)
if dev is None: print("NO DEVICE"); sys.exit(1)
cfg = dev.get_active_configuration()
intf = cfg[(0,0)]
try: usb.util.claim_interface(dev, intf.bInterfaceNumber)
except Exception: pass

def w(data, to=1000): dev.write(CMD_EP, bytes(data), to)
def r(n=64, to=1500):
    try: return bytes(dev.read(DATA_EP, n, to))
    except usb.core.USBError: return None
def drain(to=150):
    c=0
    while True:
        if r(64, to) is None: break
        c+=1
        if c>40: break
    return c
def hx(b, n=24):
    return "<none>" if b is None else ' '.join('%02x'%x for x in b[:n])

print("initial drain frames:", drain())

# How many frames does ONE cmd 0x02 emit? send, then read up to 4 frames
def probe(p3, handler=0x66, addr=0, select=True):
    drain()
    if select:
        w([0x14, handler, 0x00]); time.sleep(0.05)
        drain()
    w([0x02, addr&0xFF, (addr>>8)&0xFF, p3, handler]); time.sleep(0.03)
    frames=[]
    for _ in range(3):
        f=r(64, 800)
        if f is None: break
        frames.append(f)
    return frames

print("\n=== vary packet[3] (flash command byte), handler 0x66, addr 0 ===")
for p3 in [0x00, 0xF0, 0xFF, 0x01, 0x66, 0xA0, 0x90, 0xB0]:
    fr = probe(p3, 0x66, 0)
    print(f"p3=0x{p3:02x}: {len(fr)} frames")
    for i,f in enumerate(fr):
        print(f"   [{i}] {hx(f)}")

print("\n=== handler 0x66, p3=0xF0, walk addr to see if content shifts ===")
for a in [0x0000, 0x0040, 0x0080, 0x00C0, 0x0100]:
    fr = probe(0xF0, 0x66, a)
    print(f"addr 0x{a:04x}: {hx(fr[-1]) if fr else '<none>'}")
