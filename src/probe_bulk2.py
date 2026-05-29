import usb.core, usb.util, time

dev = usb.core.find(idVendor=0x0548, idProduct=0x1005)
if not dev: print("no device"); exit(1)

# Proper USB setup
dev.set_configuration()
cfg = dev.get_active_configuration()
print("Config:", cfg.bConfigurationValue)

# Claim interface
intf = cfg[(0, 0)]
usb.util.claim_interface(dev, intf)
print("Claimed interface 0")

# Try writing to EP2 OUT with various command bytes
# EP2 OUT = 0x02, EP2 IN = 0x82
test_ep_out = 0x02
test_ep_in = 0x82

# Safest possible: just send a cart detect/init byte
for cmd, desc in [
    ([0x01], "init"),
    ([0x00], "nop"),
    ([0xFF], "flash reset"),
    ([0x90], "read id entry"),
]:
    print("\n--- %s: %s ---" % (desc, " ".join("%02x"%c for c in cmd)))
    try:
        dev.write(test_ep_out, cmd, timeout=1000)
        print("  Wrote OK")
    except usb.core.USBError as e:
        print("  Write error:", e)
        continue
    
    time.sleep(0.1)
    for ep_in in [0x82, 0x81, 0x86]:
        try:
            buf = dev.read(ep_in, 64, timeout=500)
            print("  EP 0x%02X IN: %s" % (ep_in, " ".join("%02x"%b for b in buf)))
        except usb.core.USBError:
            pass
    
    time.sleep(0.3)
