import usb.core, usb.util, time, sys

CMD_EP = 0x04
DATA_EP = 0x82

dev = usb.core.find(idVendor=0x0548, idProduct=0x1005)
if dev is None:
    print("NO DEVICE (0548:1005)"); sys.exit(1)
try:
    dev.set_configuration()
except Exception as e:
    pass
cfg = dev.get_active_configuration()
intf = cfg[(0,0)]
try:
    usb.util.claim_interface(dev, intf.bInterfaceNumber)
except Exception:
    pass

def w(data, to=1000):
    return dev.write(CMD_EP, bytes(data), to)

def r(n=64, to=2000):
    try:
        return bytes(dev.read(DATA_EP, n, to))
    except usb.core.USBError as e:
        return None

def drain(n=8, to=200):
    for _ in range(n):
        if r(64, to) is None: break

def hx(b, n=16):
    if b is None: return "<none>"
    return ' '.join('%02x'%x for x in b[:n])

def wreg(addr, data):
    w([0x19, addr&0xFF, (addr>>8)&0xFF, (addr>>16)&0xFF, data&0xFF, (data>>8)&0xFF])

# ---- baseline ROM read ----
def rom_read_chunk(byte_addr):
    word = byte_addr//2
    drain(8)
    w([0x01, word&0xFF, (word>>8)&0xFF, (word>>16)&0xFF]); time.sleep(0.15)
    r(64, 1000)  # phantom
    w([0x01, (word+32)&0xFF, ((word+32)>>8)&0xFF, ((word+32)>>16)&0xFF]); time.sleep(0.005)
    return r(64,1000)

print("=== ROM[0] baseline (expect Nintendo logo / header) ===")
print(hx(rom_read_chunk(0), 32))
print("=== ROM[0xA0] (title area) ===")
print(hx(rom_read_chunk(0xA0), 32))

# ---- native save read: select 0x14 then cmd 0x02 ----
def save_select(handler):
    w([0x14, handler, 0x00]); time.sleep(0.1)

def save_read_native(addr, handler=0x66, drain_n=8):
    drain(drain_n)
    # packet [0x02, addr_lo, addr_mid, addr_hi, handler]
    w([0x02, addr&0xFF, (addr>>8)&0xFF, (addr>>16)&0xFF, handler]); time.sleep(0.05)
    return r(64, 3000)

print("\n=== native save read (select 0x66, cmd 0x02) at several addrs ===")
save_select(0x66)
for a in [0x00, 0x40, 0x80, 0x100, 0x1000, 0x10000]:
    d = save_read_native(a, 0x66)
    print(f"addr 0x{a:06x}: {hx(d)}")

print("\n=== vary handler byte at addr 0 ===")
for h in [0x66, 0x68, 0x65, 0x67, 0x64, 0x60, 0x00, 0x01, 0x02]:
    save_select(h)
    d = save_read_native(0x00, h)
    print(f"handler 0x{h:02x}: {hx(d)}")
