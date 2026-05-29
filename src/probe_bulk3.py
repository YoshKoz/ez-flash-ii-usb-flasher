import usb.core, usb.util, time

dev = usb.core.find(idVendor=0x0548, idProduct=0x1005)
if not dev: print("no device"); exit(1)
dev.set_configuration()
intf = dev.get_active_configuration()[(0, 0)]
usb.util.claim_interface(dev, intf)
print("Device ready")

# After sending a command, try reading from ALL IN endpoints with delay
out_ep = 0x02
in_eps = [0x81, 0x82, 0x83, 0x84, 0x85, 0x86]

def try_cmd(name, cmd_bytes):
    print("\n=== %s: [%s] ===" % (name, " ".join("%02x"%b for b in cmd_bytes)))
    try:
        dev.write(out_ep, cmd_bytes, timeout=1000)
        print("  Wrote EP2 OUT: OK")
    except usb.core.USBError as e:
        print("  Write error:", e)
        return
    
    for delay in [0.1, 0.5, 1.0]:
        time.sleep(delay)
        for ep in in_eps:
            try:
                buf = dev.read(ep, 64, timeout=200)
                print("  [%dms] EP 0x%02X: %s" % (delay*1000, ep, " ".join("%02x"%b for b in buf)))
            except usb.core.USBError:
                pass

# Try various command packet formats
# Format guess: [cmd] [addr_hi] [addr_lo] [len] or [dev_addr] [cmd] [params...]

# Simple cmd bytes with different patterns
for cmd in [
    ([0x01], "basic init"),
    ([0x00, 0x00], "two zeros"),
    ([0x02, 0x00, 0x00, 0x00], "cmd+addr"),
    ([0x90, 0x00, 0x00], "read id offset"),
    ([0x01, 0x00, 0x00], "init3"),
]:
    try_cmd(cmd[1], cmd[0])

# Try the JEDEC READ ID sequence directly
# Need to find GBA cart address mapping
# Standard NOR read ID: write AA to 5555, 55 to 2AAA, 90 to 5555
# But the firmware abstracts this, so we just send "READ_ID" command
print("\n=== Trying longer packets ===")
for cmd in [
    bytes([0xAA, 0x55, 0x90]),
    bytes([0x10, 0x00]),  # status read?
    bytes([0x70, 0x00]),  # status?
    bytes([0xFF, 0xFF]),  # reset all
]:
    try_cmd("bytes %s" % " ".join("%02x"%b for b in cmd), cmd)
