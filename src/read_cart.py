"""Read full GBA cartridge ROM via decoded protocol"""
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

CMD_EP = 0x04  # EP4 OUT
DATA_EP = 0x82 # EP2 IN

# Send the "read/status" command (4 bytes)
cmd = [0x01, 0x00, 0x00, 0x00]
print("Sending cmd: [%s]" % " ".join("%02x"%c for c in cmd), flush=True)
dev.write(CMD_EP, cmd, timeout=3000)
time.sleep(0.3)

# Read first 256 bytes in 64-byte chunks
print("\n=== GBA Cartridge ROM Header ===", flush=True)
all_data = bytearray()
for chunk in range(4):
    try:
        buf = dev.read(DATA_EP, 64, timeout=1000)
        all_data.extend(buf)
        hex_s = " ".join("%02x"%b for b in buf[:16])
        ascii_s = "".join(chr(b) if 32<=b<127 else "." for b in buf)
        print("  [%d] %s  %s" % (chunk, hex_s, ascii_s), flush=True)
    except usb.core.USBError as e:
        print("  [%d] %s" % (chunk, e), flush=True)
        break

# Parse GBA header
header = bytes(all_data[:256])
print("\n=== Header Parsing ===", flush=True)
if len(header) >= 4:
    branch = " ".join("%02x"%b for b in header[:4])
    print("  ARM branch: %s" % branch, flush=True)
    
if len(header) >= 0xA0:
    title = header[0xA0:0xAC].rstrip(b'\x00').decode('ascii', errors='replace')
    print("  Game title: %s" % title, flush=True)
    
if len(header) >= 0xB0:
    code = header[0xAC:0xB0].rstrip(b'\x00').decode('ascii', errors='replace')
    print("  Game code: %s" % code, flush=True)

if len(header) >= 0xB2:
    maker = header[0xB0:0xB2].rstrip(b'\x00').decode('ascii', errors='replace')
    print("  Maker code: %s" % maker, flush=True)

# Also try reading larger chunk to check if read continues
print("\n=== Reading more ROM data ===", flush=True)
for chunk in range(4, 20):
    try:
        buf = dev.read(DATA_EP, 64, timeout=500)
        all_data.extend(buf)
        hex_s = " ".join("%02x"%b for b in buf[:8])
        ascii_s = "".join(chr(b) if 32<=b<127 else "." for b in buf[:16])
        print("  [%d] %s  %s" % (chunk, hex_s, ascii_s), flush=True)
    except usb.core.USBError:
        print("  [%d] timeout (end of auto-read?)" % chunk, flush=True)
        break

print("\nTotal read: %d bytes" % len(all_data), flush=True)
