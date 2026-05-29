import usb.core, usb.util, time, sys
sys.stdout.reconfigure(line_buffering=True)

dev = usb.core.find(idVendor=0x0548, idProduct=0x1005)
if not dev: print("not found"); exit(1)
dev.set_configuration()
intf = dev.get_active_configuration()[(0, 0)]
usb.util.claim_interface(dev, intf)
print("ready\n")

for ep in range(0x01, 0x08):
    try: dev.clear_halt(ep)
    except: pass
    try: dev.clear_halt(ep | 0x80)
    except: pass

# Try: write to different OUT endpoints, then poll ALL IN
# The EZClient might use EP1 for cmd, EP2 for data
# Or EP6 for IN data based on IOCTL 0x00222054 (bulk read endpoint 6)

combos = [
    (0x01, 0x81, "EP1->EP1"),
    (0x02, 0x86, "EP2->EP6 (IOCTL match)"),
    (0x01, 0x82, "EP1->EP2"),
    (0x03, 0x83, "EP3->EP3"),
]

for out_ep, exp_in_ep, desc in combos:
    print("=== %s ===" % desc, flush=True)
    
    # Clear the OUT ep
    try: dev.clear_halt(out_ep)
    except: pass
    
    cmd = [0x01, 0x00, 0x00, 0x00]
    try:
        dev.write(out_ep, cmd, timeout=3000)
        print("  Wrote to EP%d OK" % out_ep, flush=True)
    except usb.core.USBError as e:
        print("  Write to EP%d: %s" % (out_ep, e), flush=True)
        continue
    
    time.sleep(0.3)
    
    # Try the expected IN ep first
    for ep in [exp_in_ep, 0x81, 0x82, 0x86]:
        try:
            buf = dev.read(ep, 64, timeout=300)
            print("  EP0x%02X: %s" % (ep, " ".join("%02x"%b for b in buf)), flush=True)
        except usb.core.USBError:
            pass
    
    time.sleep(0.3)
