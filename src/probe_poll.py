import usb.core, usb.util, time, sys
sys.stdout.reconfigure(line_buffering=True)

dev = usb.core.find(idVendor=0x0548, idProduct=0x1005)
if not dev: print("not found"); exit(1)
dev.set_configuration()
intf = dev.get_active_configuration()[(0, 0)]
usb.util.claim_interface(dev, intf)
print("ready\n")

for ep in range(0x01, 0x08):
    try: dev.clear_halt(ep)
    except: pass
    try: dev.clear_halt(ep | 0x80)
    except: pass

out_ep = 0x02

def ram_read(wVal):
    try: return list(dev.ctrl_transfer(0xC0, 0xA0, wVal, 0, 8, timeout=1000))
    except: return None

# Send a cartridge info request (0x01 as command byte + 0s for params)
# Then aggressively poll both EP0 registers and IN endpoints for response
cmd = [0x01, 0x00, 0x00, 0x00]
print("Sending: [%s]" % " ".join("%02x"%b for b in cmd), flush=True)
dev.write(out_ep, cmd, timeout=3000)
print("Sent. Polling...\n", flush=True)

# Poll for up to 3 seconds
for poll in range(30):
    # Check EP0 register for status change
    r = ram_read(0x7FC0)
    if r:
        cnt = r[1]
    
    # Poll all IN endpoints
    for ep in [0x81, 0x82, 0x83, 0x84, 0x85, 0x86]:
        try:
            buf = dev.read(ep, 64, timeout=100)
            print("[%3dms] EP 0x%02X: %s" % (poll*100, ep, " ".join("%02x"%b for b in buf)), flush=True)
        except usb.core.USBError:
            pass
    
    # Also read FIFO buffer area for any data
    for fb in [0x7DC0, 0x7D00]:
        d = ram_read(fb)
        if d and any(b != 0x01 for b in d):
            print("[%3dms] 0x%04X: %s" % (poll*100, fb, " ".join("%02x"%b for b in d)), flush=True)
    
    time.sleep(0.1)

print("\nDone polling.", flush=True)
