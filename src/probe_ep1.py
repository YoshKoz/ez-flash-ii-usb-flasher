import usb.core, usb.util, time, sys
sys.stdout.reconfigure(line_buffering=True)

dev = usb.core.find(idVendor=0x0548, idProduct=0x1005)
if not dev: print("no device"); exit(1)
dev.set_configuration()
intf = dev.get_active_configuration()[(0, 0)]
usb.util.claim_interface(dev, intf)
print("ready\n")

# Clear stalls
for ep in range(0x01, 0x08): 
    try: dev.clear_halt(ep)
    except: pass
    try: dev.clear_halt(ep | 0x80)
    except: pass

def ram_read(addr):
    return list(dev.ctrl_transfer(0xC0, 0xA0, addr, 0, 8, timeout=2000))

# Try EP1 OUT (0x01) instead of EP2 
print("=== Try EP1 OUT (0x01) ===", flush=True)
for name, cmd, out_ep in [
    ("reset FF on EP1", [0xFF], 0x01),
    ("init 01 on EP1", [0x01], 0x01),
]:
    out_ep = 0x01
    try:
        dev.write(out_ep, cmd, timeout=3000)
        print("  %s -> OK" % name, flush=True)
    except usb.core.USBError as e:
        print("  %s -> %s" % (name, e), flush=True)
    
    time.sleep(0.5)
    for ep in [0x81, 0x82, 0x86]:
        try:
            buf = dev.read(ep, 64, timeout=300)
            print("    EP0x%02X: %s" % (ep, " ".join("%02x"%b for b in buf)), flush=True)
        except usb.core.USBError:
            pass
    
    # Check EP0 state
    for a in [0x7F96, 0x7F92]:
        d = ram_read(a)
        print("  0x%04X: %s" % (a, " ".join("%02x"%b for b in d)), flush=True)
    time.sleep(0.3)

# Also try probing the endpoint buffer memory areas
print("\n=== Probe EP buffer memory via EP0 ===", flush=True)
for a in [0x7DC0, 0x7D00, 0x7C00, 0x7E00, 0x7F40, 0x7F50, 0x7F60, 0x2000, 0x7FC0, 0x7F00]:
    try:
        d = ram_read(a)
        h = " ".join("%02x"%b for b in d)
        print("  0x%04X: %s" % (a, h), flush=True)
    except usb.core.USBError as e:
        print("  0x%04X: %s" % (a, e), flush=True)
