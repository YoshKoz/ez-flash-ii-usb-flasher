"""Map the 0xA0 read addressing in active firmware"""
import usb.core, time, sys
sys.stdout.reconfigure(line_buffering=True)

dev = usb.core.find(idVendor=0x0548, idProduct=0x1005)
if not dev: print("not found"); exit(1)
dev.set_configuration()
intf = dev.get_active_configuration()[(0, 0)]
usb.util.claim_interface(dev, intf)

def ram_read(wVal, wIdx=0):
    try:
        return list(dev.ctrl_transfer(0xC0, 0xA0, wVal, wIdx, 8, timeout=1000))
    except:
        return None

# Test: does wIndex change the result?
print("=== wValue=0x0000, vary wIndex ===", flush=True)
for idx in [0, 1, 8, 0x100, 0x200, 0x300]:
    d = ram_read(0x0000, idx)
    if d: print("  wIdx=0x%04X: %s" % (idx, " ".join("%02x"%b for b in d)), flush=True)

# Test: systematic scan for non-0x01 responses
print("\n=== Scan for memory regions with non-0x01 data ===", flush=True)
interesting = []
for base in range(0, 0x8000, 0x100):
    d = ram_read(base)
    if d and any(b != 0x01 for b in d):
        interesting.append((base, d))
        print("  0x%04X: %s" % (base, " ".join("%02x"%b for b in d)), flush=True)
    if len(interesting) > 30:
        print("  (stopping at 30 regions)", flush=True)
        break

# Also try reading with non-zero wIndex to access more memory
print("\n=== wValue=0x0000, different wIndex to try full memory ===", flush=True)
for idx in [0x0001, 0x0010, 0x0100, 0x1000]:
    d = ram_read(0x0000, idx)
    if d and any(b != 0x01 for b in d):
        print("  wIdx=0x%04X: %s" % (idx, " ".join("%02x"%b for b in d)), flush=True)
