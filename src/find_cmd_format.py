"""Find command format for address selection"""
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

def send_and_read(cmd_bytes):
    dev.write(CMD_EP, cmd_bytes, timeout=3000)
    time.sleep(0.15)
    try:
        buf = dev.read(DATA_EP, 64, timeout=500)
        return buf
    except:
        return None

# Baseline: what does cmd 01 00 00 00 return?
print("Baseline: cmd [01 00 00 00]", flush=True)
buf = send_and_read([0x01, 0x00, 0x00, 0x00])
print("  %s" % " ".join("%02x"%b for b in buf[:32]) if buf else "  timeout", flush=True)

# Try different command bytes (first byte = command code)
print("\n=== Different cmd bytes ===", flush=True)
for cmd_byte in [0x01, 0x02, 0x03, 0x04, 0x10, 0x20, 0x40, 0x80]:
    buf = send_and_read([cmd_byte, 0x00, 0x00, 0x00])
    if buf:
        h = " ".join("%02x"%b for b in buf[:8])
        print("  cmd %02x: %s" % (cmd_byte, h), flush=True)
    else:
        print("  cmd %02x: timeout" % cmd_byte, flush=True)
    time.sleep(0.1)

# Try different address parameters with cmd=01
print("\n=== Different address params with cmd 01 ===", flush=True)
for addr_lo in [0x00, 0x40, 0x80, 0xC0]:
    cmd = [0x01, addr_lo, 0x00, 0x00]
    buf = send_and_read(cmd)
    if buf:
        h = " ".join("%02x"%b for b in buf[:8])
        print("  [01 %02x 00 00]: %s" % (addr_lo, h), flush=True)
    time.sleep(0.1)

# Try WORD address
print("\n=== Word address with cmd 01 ===", flush=True)
for addr in [0x0000, 0x0040, 0x0100, 0x4000]:
    cmd = [0x01, addr & 0xFF, (addr >> 8) & 0xFF, 0x00]
    buf = send_and_read(cmd)
    if buf:
        h = " ".join("%02x"%b for b in buf[:8])
        print("  addr 0x%04X: %s" % (addr, h), flush=True)
    time.sleep(0.1)
