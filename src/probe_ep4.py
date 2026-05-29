import usb.core, usb.util, time, sys
sys.stdout.reconfigure(line_buffering=True)

dev = usb.core.find(idVendor=0x0548, idProduct=0x1005)
if not dev: print("not found"); exit(1)
dev.set_configuration()
intf = dev.get_active_configuration()[(0, 0)]
usb.util.claim_interface(dev, intf)

for ep in range(0x01, 0x08):
    for e in [ep, ep | 0x80]:
        try: dev.clear_halt(e)
        except: pass

# The IOCTL from ezwriter.sys: 0x00222035 = BULK_WRITE EP2, 0x00222054 = BULK_READ EP6
# Let me try the EXACT endpoint mapping from the driver
# Write to EP2 OUT (0x02), Read from EP6 IN (0x86)

out_ep = 0x02
in_ep = 0x86

# Also try: firmware might read from EP4 OUT FIFO at 0x7DC0
# EP4 address = 0x04, FIFO starts at 0x7D00
# Let me try EP4 OUT

print("=== EP4 OUT test ===", flush=True)
for ep in [0x04]:
    try: dev.clear_halt(ep)
    except: pass
    try:
        dev.write(ep, [0x01, 0x00, 0x00, 0x00], timeout=3000)
        print("  EP4 OUT write OK", flush=True)
    except usb.core.USBError as e:
        print("  EP4 OUT: %s" % e, flush=True)
    
    time.sleep(0.3)
    for iep in [0x81, 0x82, 0x86, 0x84]:
        try:
            buf = dev.read(iep, 64, timeout=300)
            print("  EP0x%02X: %s" % (iep, " ".join("%02x"%b for b in buf)), flush=True)
        except usb.core.USBError:
            pass

# The big theory: maybe response is multi-read - need to read repeatedly
# From the EZClient status format: Status:[1]0x%x [2]0x%x [3]0x%x [4]0x%x
# This suggests they send a STATUS command and read 4 DWORDs totaling 16 bytes
print("\n=== Try STATUS command (repeatedly read) ===", flush=True)
dev.write(0x02, [0x04, 0x00, 0x00, 0x00], timeout=3000)
time.sleep(0.5)
# Keep reading IN endpoints in a loop
for i in range(5):
    for iep in [0x86, 0x81, 0x82, 0x84]:
        try:
            buf = dev.read(iep, 64, timeout=200)
            print("  [%d] EP0x%02X (%db): %s" % (i, iep, len(buf), " ".join("%02x"%b for b in buf)), flush=True)
        except:
            pass
    time.sleep(0.2)

# After all writes, try unsolicited read: maybe firmware sends when data ready
# Do many rapid reads from EP6 IN (the IOCTL ep)
print("\n=== Many rapid reads from EP6 IN ===", flush=True)
for i in range(20):
    try:
        buf = dev.read(0x86, 64, timeout=100)
        print("  Got %db: %s" % (len(buf), " ".join("%02x"%b for b in buf)), flush=True)
    except:
        pass
    time.sleep(0.05)
print("Done", flush=True)
