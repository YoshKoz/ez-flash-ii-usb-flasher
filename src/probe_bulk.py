import usb.core, time, sys

dev = usb.core.find(idVendor=0x0548, idProduct=0x1005)
if not dev: print("Device not found"); sys.exit(1)
dev.set_configuration()
print("Device found. Cart inserted confirmed.")

# Try 1-byte command + read response
# Start with safest: just send a read request and listen

# EP1 OUT -> EP1 IN protocol probe
cmds = [
    (1, [0x01],  "init/reset"),
    (1, [0x00],  "soft reset"),
    (2, [0x01, 0x00], "init w/ param"),
]

out_ep = 0x01  # EP1 OUT
in_ep = 0x81   # EP1 IN

for ep_num, cmd_bytes, desc in cmds:
    print("\n--- Send to EP%d OUT: [%s] (%s) ---" % (out_ep, " ".join("%02x"%b for b in cmd_bytes), desc))
    try:
        wrote = dev.write(out_ep, cmd_bytes, timeout=2000)
        print("  Wrote %d bytes" % wrote)
    except usb.core.USBError as e:
        print("  Write error: %s" % e)
        continue
    
    time.sleep(0.1)
    
    # Read response from corresponding IN endpoint
    for ep in [0x81, 0x82, 0x86]:
        try:
            buf = dev.read(ep, 64, timeout=500)
            print("  EP 0x%02X IN: %d bytes [%s]" % (ep, len(buf), " ".join("%02x"%b for b in buf)))
        except usb.core.USBError as e:
            print("  EP 0x%02X IN: timeout" % ep)

    time.sleep(0.5)
