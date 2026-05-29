"""Read cartridge ROM sequentially - send cmd after each 64-byte chunk"""
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

CMD_EP = 0x04
DATA_EP = 0x82

# Read first 256 bytes by sending cmd before each 64-byte chunk
print("=== Sequential cartridge read ===", flush=True)
all_data = bytearray()

for chunk in range(8):
    # Send read command
    cmd = [0x00, 0x00, 0x00, 0x00]  # try cmd 0x00 with params
    dev.write(CMD_EP, cmd, timeout=3000)
    time.sleep(0.15)
    
    # Read response
    try:
        buf = dev.read(DATA_EP, 64, timeout=500)
        all_data.extend(buf)
        hex_s = " ".join("%02x"%b for b in buf[:16])
        print("  [%d] %s" % (chunk, hex_s), flush=True)
    except usb.core.USBError as e:
        print("  [%d] %s" % (chunk, e), flush=True)
        break

# Try different commands to see which one advances the address
print("\n=== Try cmd = [01 XX XX XX] with different address ===", flush=True)
for addr in [0x00, 0x40, 0x80, 0xC0, 0x100]:
    cmd = [0x01, addr, 0x00, 0x00]
    dev.write(CMD_EP, cmd, timeout=3000)
    time.sleep(0.15)
    try:
        buf = dev.read(DATA_EP, 64, timeout=500)
        hex_s = " ".join("%02x"%b for b in buf[:16])
        print("  cmd=[%02x %02x %02x %02x] => %s" % tuple(cmd + list(buf[:16])), flush=True)
    except usb.core.USBError as e:
        print("  cmd=[%02x %02x %02x %02x] => %s" % (cmd[0], cmd[1], cmd[2], cmd[3], e), flush=True)
    
print("\nTotal first pass: %d bytes" % len(all_data), flush=True)
