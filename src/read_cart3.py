"""Read cart by capturing both alternating buffers"""
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

# After sending cmd 01, read alternating buffers
# Buffer A = data from NOR flash starting at some address
# Buffer B = data from another source (or status)

# Send cmd once, read both buffers, then determine what each contains
dev.write(CMD_EP, [0x01, 0x00, 0x00, 0x00], timeout=3000)
time.sleep(0.2)

# Read both alternating buffers
print("=== Alternating buffer contents ===", flush=True)
for i in range(6):
    try:
        buf = dev.read(DATA_EP, 64, timeout=500)
        h = " ".join("%02x"%b for b in buf[:24])
        a = "".join(chr(b) if 32<=b<127 else "." for b in buf[:16])
        print("  [%d] %s  %s" % (i, h, a), flush=True)
    except Exception as e:
        print("  [%d] %s" % (i, e), flush=True)
        break

# Now try: read BOTH buffers after each command to capture 128 bytes
print("\n=== Using both buffers to capture sequential data ===", flush=True)
# For each command, read buffer A (first read) = cartridge data
# Then read buffer B (second read) = also cartridge data, possibly next 64 bytes
# Discard subsequent reads which alternate back

for cmd_lo in [0x00, 0x40, 0x80, 0xC0, 0x00]:
    cmd = [0x01, cmd_lo, 0x00, 0x00]
    dev.write(CMD_EP, cmd, timeout=3000)
    time.sleep(0.15)
    
    # Read first buffer (A)
    buf_a = dev.read(DATA_EP, 64, timeout=500)
    print("  cmd [01 %02x 00 00] A: %s" % (cmd_lo, " ".join("%02x"%b for b in buf_a[:16])), flush=True)
    
    # Read second buffer (B)  
    try:
        buf_b = dev.read(DATA_EP, 64, timeout=500)
        print("                    B: %s" % " ".join("%02x"%b for b in buf_b[:16]), flush=True)
    except:
        pass
    
    time.sleep(0.1)
