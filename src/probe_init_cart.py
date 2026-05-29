import usb.core, usb.util, time, sys
sys.stdout.reconfigure(line_buffering=True)

dev = usb.core.find(idVendor=0x0548, idProduct=0x1005)
if not dev: print("not found"); exit(1)
dev.set_configuration()
intf = dev.get_active_configuration()[(0, 0)]
usb.util.claim_interface(dev, intf)
print("ready\n")

for ep in range(0x01, 0x08):
    for e in [ep, ep | 0x80]:
        try: dev.clear_halt(e)
        except: pass

def ram(wVal):
    try: return list(dev.ctrl_transfer(0xC0, 0xA0, wVal, 0, 8, timeout=1000))
    except: return None

out_ep = 0x02

# Multi-step initialization sequence
# Step 1: Reset cartridge bus
print("Step 1: Reset (FF)", flush=True)
dev.write(out_ep, [0xFF], timeout=3000)
time.sleep(0.5)

# Step 2: Init
print("Step 2: Init (01)", flush=True)
dev.write(out_ep, [0x01], timeout=3000)
time.sleep(0.5)

# Step 3: Read status
print("Step 3: Cmd 04", flush=True)
dev.write(out_ep, [0x04], timeout=3000)
time.sleep(0.5)

# Check memory for cartridge response  
print("Step 4: Scan memory regions", flush=True)
for a in range(0x0000, 0x2000, 0x100):
    d = ram(a)
    if d and any(b not in [0x01, 0x04, 0xae] for b in d):
        # Check if this looks like data (not firmware code)
        if not (d[0] == 0x02 and d[1] < 0x20):  # not LJMP
            print("  0x%04X: %s" % (a, " ".join("%02x"%b for b in d)), flush=True)

# Check specific FIFO areas for response
print("\nStep 5: FIFO scan", flush=True)
for a in [0x7C00, 0x7C40, 0x7C80, 0x7CC0, 0x7D00, 0x7D40, 0x7D80, 0x7DC0]:
    d = ram(a)
    if d and any(b != 0x01 for b in d):
        print("  0x%04X: %s" % (a, " ".join("%02x"%b for b in d)), flush=True)

# Step 6: Read IN endpoints after init
print("\nStep 6: IN endpoint test after full init", flush=True)
for ep in [0x81, 0x82, 0x86]:
    try:
        time.sleep(0.2)
        buf = dev.read(ep, 64, timeout=500)
        print("  EP0x%02X: %s" % (ep, " ".join("%02x"%b for b in buf)), flush=True)
    except usb.core.USBError:
        print("  EP0x%02X: timeout" % ep, flush=True)
