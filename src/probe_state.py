import usb.core, usb.util, time, sys
sys.stdout.reconfigure(line_buffering=True)

dev = usb.core.find(idVendor=0x0548, idProduct=0x1005)
if not dev: print("no device"); exit(1)
dev.set_configuration()
intf = dev.get_active_configuration()[(0, 0)]
usb.util.claim_interface(dev, intf)
print("ready\n")

out_ep = 0x02

# Clear any stalled state
for ep in [0x01, 0x02, 0x81, 0x82]:
    try: dev.clear_halt(ep)
    except: pass

# Read baseline via EP0 vendor
def ram_read(addr):
    return list(dev.ctrl_transfer(0xC0, 0xA0, addr, 0, 8, timeout=2000))

print("Baseline EP0 reads:", flush=True)
for a in [0x0000, 0x7F92, 0x7F96]:
    d = ram_read(a)
    print("  0x%04X: %s" % (a, " ".join("%02x"%b for b in d)), flush=True)

# Send a known command, then check if state changed
for name, cmd_bytes in [
    ("reset FF", [0xFF]),
    ("init 01", [0x01]),
]:
    print("\n--- %s ---" % name, flush=True)
    dev.write(out_ep, cmd_bytes, timeout=3000)
    print("  Wrote OK, waiting...", flush=True)
    time.sleep(0.5)
    
    # Poll EP0 for state changes
    for a in [0x7F92, 0x7F96, 0x0000]:
        d = ram_read(a)
        print("  0x%04X: %s" % (a, " ".join("%02x"%b for b in d)), flush=True)
    
    time.sleep(0.5)
