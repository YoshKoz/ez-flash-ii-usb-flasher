import usb.core, usb.util, time, sys
CMD_EP, DATA_EP = 0x04, 0x82
TYPE=0x66
dev = usb.core.find(idVendor=0x0548, idProduct=0x1005)
if dev is None: print("NO DEVICE"); sys.exit(1)
cfg=dev.get_active_configuration(); intf=cfg[(0,0)]
try: usb.util.claim_interface(dev,intf.bInterfaceNumber)
except Exception: pass
def w(d,to=1000): dev.write(CMD_EP,bytes(d),to)
def r(n=64,to=1500):
    try: return bytes(dev.read(DATA_EP,n,to))
    except usb.core.USBError: return None
def drain(to=80):
    c=0
    while r(64,to) is not None:
        c+=1
        if c>60: break
    return c
def hx(b,n=16): return "<none>" if b is None else ' '.join('%02x'%x for x in b[:n])
def sel(t=TYPE): w([0x14,t,0x00]); time.sleep(0.05)
def fwrite(addr,data):  # cmd 0x20 single-byte flash write
    w([0x20, addr&0xFF, (addr>>8)&0xFF, data&0xFF]); time.sleep(0.005)
def read64(addr):       # cmd 0x03 only, 16-bit addr
    drain(50)
    w([0x03, addr&0xFF, (addr>>8)&0xFF, 0x00, 0x00]); time.sleep(0.01)
    return r(64,2000)
def read64_with02(addr,bank):
    drain(50)
    w([0x02, addr&0xFF,(addr>>8)&0xFF,bank,TYPE]); time.sleep(0.02)
    w([0x03, addr&0xFF,(addr>>8)&0xFF,0x00,0x00]); time.sleep(0.01)
    return r(64,2000)

sel(); drain()

print("=== cmd03-only read addr0 (baseline bank0) ===")
a = read64(0x0000); print(hx(a))

print("\n=== JEDEC ID: AA->5555,55->2AAA,90->5555, read 0x00/0x01 ===")
fwrite(0x5555,0xAA); fwrite(0x2AAA,0x55); fwrite(0x5555,0x90); time.sleep(0.02)
idbytes = read64(0x0000)
print("after ID cmd, addr0:", hx(idbytes))
# reset to read array
fwrite(0x5555,0xAA); fwrite(0x2AAA,0x55); fwrite(0x5555,0xF0); time.sleep(0.02)
b = read64(0x0000); print("after F0 reset, addr0:", hx(b), " (should match baseline)")

print("\n=== bank switch to 1, read addr0 ===")
fwrite(0x5555,0xAA); fwrite(0x2AAA,0x55); fwrite(0x5555,0xB0); fwrite(0x0000,0x01); time.sleep(0.02)
b1 = read64(0x0000); print("bank1 addr0 (cmd03-only):", hx(b1))
b1b = read64_with02(0x0000,0x00); print("bank1 addr0 (cmd02+03):", hx(b1b))

print("\n=== switch back to bank 0 ===")
fwrite(0x5555,0xAA); fwrite(0x2AAA,0x55); fwrite(0x5555,0xB0); fwrite(0x0000,0x00); time.sleep(0.02)
b0 = read64(0x0000); print("bank0 addr0 again:", hx(b0))
print("\nbank0 != bank1 ?", a != b1)
