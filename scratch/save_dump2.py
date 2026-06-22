import usb.core, usb.util, time, sys, struct
CMD_EP, DATA_EP = 0x04, 0x82
TYPE=0x66
GEN3=bytes([0x25,0x20,0x01,0x08])
dev = usb.core.find(idVendor=0x0548, idProduct=0x1005)
if dev is None: print("NO DEVICE"); sys.exit(1)
cfg=dev.get_active_configuration(); intf=cfg[(0,0)]
try: usb.util.claim_interface(dev,intf.bInterfaceNumber)
except Exception: pass
def w(d,to=1000): dev.write(CMD_EP,bytes(d),to)
def r(n=64,to=2000):
    try: return bytes(dev.read(DATA_EP,n,to))
    except usb.core.USBError: return None
def drain(to=60):
    c=0
    while r(64,to) is not None:
        c+=1
        if c>60: break
def sel(t=TYPE): w([0x14,t,0x00]); time.sleep(0.05)
def fwrite(addr,data): w([0x20,addr&0xFF,(addr>>8)&0xFF,data&0xFF]); time.sleep(0.004)
def read64(addr):
    drain(40)
    w([0x03,addr&0xFF,(addr>>8)&0xFF,0x00,0x00]); time.sleep(0.008)
    return r(64,2500)
def bank_switch(bk):
    fwrite(0x5555,0xAA); fwrite(0x2AAA,0x55); fwrite(0x5555,0xB0); fwrite(0x0000,bk); time.sleep(0.01)
def flash_reset():
    fwrite(0x5555,0xAA); fwrite(0x2AAA,0x55); fwrite(0x5555,0xF0); time.sleep(0.01)

sel(); drain()
out=bytearray(); t0=time.time()
for bk in (0,1):
    bank_switch(bk)
    for off in range(0,0x10000,64):
        d=read64(off)
        if d is None:
            print(f"TIMEOUT bank{bk} off 0x{off:04x}"); flash_reset(); sys.exit(1)
        out+=d
    print(f"bank{bk} done ({len(out)} bytes)")
bank_switch(0); flash_reset()
print(f"read {len(out)} bytes in {time.time()-t0:.1f}s")

sigs=sum(1 for j in range(len(out)-3) if out[j:j+4]==GEN3)
print("Gen3 sigs:", sigs)
# parse both 14-section slots
for slot,base in (('A',0x0000),('B',0xE000)):
    ids=[]; idxs=set()
    for s in range(14):
        sec=out[base+s*0x1000: base+s*0x1000+0x1000]
        sid,chk=struct.unpack_from('<HH',sec,0xFF4)
        sig,idx=struct.unpack_from('<II',sec,0xFF8)
        ids.append(sid);
        if sig==0x08012025: idxs.add(idx)
    ok = sorted(ids)==list(range(14))
    print(f"slot {slot} @0x{base:05x}: ids={ids} contiguous={ok} saveidx={idxs}")
path=r'C:\Development\ez-flash-ii-usb-flasher\src\ezwriter-cli\sapphire.sav'
open(path,'wb').write(out); print("wrote",path)
