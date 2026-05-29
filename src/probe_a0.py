import usb.core, time

dev = usb.core.find(idVendor=0x0548, idProduct=0x1005)
if dev is None:
    print("Device not found")
    exit(1)
dev.set_configuration()
print("Probing 0xA0 vendor reads...")
for addr in [0x0000, 0x0008, 0x0100, 0x0200, 0x0400, 0x0800, 0x1000, 0x1400, 0x1900, 0x1B00]:
    try:
        buf = dev.ctrl_transfer(0xC0, 0xA0, addr, 0, 8, timeout=1000)
        h = " ".join("%02x" % b for b in buf)
        print("  0x%04X => %s" % (addr, h))
    except usb.core.USBError as e:
        s = str(e)
        print("  0x%04X => %s" % (addr, s))
print("Done")
