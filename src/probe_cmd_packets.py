import usb.core, usb.util, time, sys
sys.stdout.reconfigure(line_buffering=True)

dev = usb.core.find(idVendor=0x0548, idProduct=0x1005)
if not dev: print("not found"); exit(1)
dev.set_configuration()
intf = dev.get_active_configuration()[(0, 0)]
usb.util.claim_interface(dev, intf)
print("ready\n")

# Clear stalls
for ep in range(0x01, 0x08):
    try: dev.clear_halt(ep)
    except: pass
    try: dev.clear_halt(ep | 0x80)
    except: pass

def ram_read(wVal):
    try:
        return list(dev.ctrl_transfer(0xC0, 0xA0, wVal, 0, 8, timeout=1000))
    except: return None

out_ep = 0x02

# Multi-byte command packets for cartridge read
# Format guess: [cmd_byte] [addr_lo] [addr_hi] [len]
commands = [
    ([0x01],                      "init only"),
    ([0x02, 0x00],                "2byte"),
    ([0x02, 0x00, 0x00],          "cmd+addr16"),
    ([0x01, 0x00, 0x00, 0x00],   "init4"),
    ([0x02, 0x00, 0x00, 0x00],   "cmd2+addr"),
    ([0x00, 0x00],                "2zeros"),
    ([0x04, 0x00, 0x00, 0x00],   "status"),
    ([0x01, 0x00, 0x00],         "init3"),
    ([0xFF, 0x00, 0x00],         "reset3"),
    ([0xAA, 0x55, 0x90],         "flashID"),
]

for cmd_bytes, desc in commands:
    # Check baseline first
    b0 = ram_read(0x7FC0)
    b1 = ram_read(0x7FC8)
    
    print("--- %s: %s ---" % (desc, " ".join("%02x"%b for b in cmd_bytes)), flush=True)
    
    # Write command
    try:
        dev.write(out_ep, cmd_bytes, timeout=3000)
        wrote = True
    except usb.core.USBError as e:
        print("  Write: %s" % e, flush=True)
        wrote = False
    
    time.sleep(0.3)
    
    if wrote:
        # Check state change at key registers
        for a, name in [(0x7FC0, "EP_RDY"), (0x7FC8, "EP_CNT")]:
            d = ram_read(a)
            if d != b0 if a == 0x7FC0 else d != b1:
                print("  %s changed: %s" % (name, " ".join("%02x"%b for b in d)), flush=True)
        
        # Read IN endpoints
        for ep in [0x81, 0x82, 0x86]:
            try:
                buf = dev.read(ep, 64, timeout=200)
                print("  EP0x%02X: %s" % (ep, " ".join("%02x"%b for b in buf)), flush=True)
            except usb.core.USBError:
                pass
    
    time.sleep(0.2)
