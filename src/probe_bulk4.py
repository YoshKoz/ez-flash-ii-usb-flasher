import usb.core, usb.util, time, sys
sys.stdout.reconfigure(line_buffering=True)

dev = usb.core.find(idVendor=0x0548, idProduct=0x1005)
if not dev: print("no device"); exit(1)
dev.set_configuration()
intf = dev.get_active_configuration()[(0, 0)]
usb.util.claim_interface(dev, intf)
print("ready\n")

out_ep = 0x02
in_eps = [0x81, 0x82, 0x83, 0x84, 0x85, 0x86]

# Clear any stalled state on endpoints
for ep in [out_ep] + in_eps:
    try:
        dev.clear_halt(ep)
    except:
        pass

print("=== Write 1 byte cmd to EP2 OUT, read IN after ===", flush=True)

for name, cmd in [
    ("init 01", [0x01]),
    ("nop 00", [0x00]),
    ("reset FF", [0xFF]),
    ("read ID 90", [0x90]),
    ("status 70", [0x70]),
]:
    try:
        dev.write(out_ep, cmd, timeout=3000)
        print("  %s -> OK" % name, flush=True)
    except usb.core.USBError as e:
        print("  %s -> %s" % (name, e), flush=True)
        continue
    
    time.sleep(0.5)
    for ep in in_eps:
        try:
            buf = dev.read(ep, 64, timeout=500)
            print("    EP0x%02X: %s" % (ep, " ".join("%02x"%b for b in buf)), flush=True)
        except usb.core.USBError:
            pass
    
    time.sleep(0.3)
