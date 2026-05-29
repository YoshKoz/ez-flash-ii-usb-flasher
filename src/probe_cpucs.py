"""AN2131 firmware download probe - try multiple CPUCS addresses"""
import sys
import struct
import usb.core
import usb.util
import os

BOOT_VID = 0x0547
BOOT_PID = 0x2131

def find():
    return usb.core.find(idVendor=BOOT_VID, idProduct=BOOT_PID)

def ezusb_write(dev, addr, data):
    """Vendor write to AN2131 RAM/register via 0xA0."""
    return dev.ctrl_transfer(
        bmRequestType=0x40,  # Host-to-Device, Vendor, Device
        bRequest=0xA0,
        wValue=addr & 0xFFFF,
        wIndex=(addr >> 16) & 0xFFFF,
        data_or_wLength=data,
        timeout=5000
    )

def download_fw(dev, fw_path):
    with open(fw_path, 'rb') as f:
        fw = f.read()
    print(f"Firmware: {len(fw)} bytes")

    # Download in 64-byte chunks starting at address 0
    chunk = 64
    for off in range(0, len(fw), chunk):
        end = min(off + chunk, len(fw))
        ezusb_write(dev, off, fw[off:end])
    print(f"  Downloaded {len(fw)} bytes")
    return True

def try_cpucs_write(dev, addr, val, label):
    """Try writing a value to a potential CPUCS address."""
    try:
        ezusb_write(dev, addr, bytes([val]))
        print(f"  [{label}] Wrote 0x{val:02X} to 0x{addr:04X} - OK")
        return True
    except Exception as e:
        print(f"  [{label}] Wrote 0x{val:02X} to 0x{addr:04X} - FAIL: {e}")
        return False

def main():
    fw_path = sys.argv[1] if len(sys.argv) > 1 else "src/ezwriter-cli/tusbez.bin"
    
    # Find device
    dev = find()
    if dev is None:
        print("Device not found in bootloader mode.")
        return 1

    print(f"Found: {dev.idVendor:04X}:{dev.idProduct:04X}")
    
    # Set config and claim interface
    try:
        dev.set_configuration()
    except:
        pass
    
    print(f"Loading firmware...")
    download_fw(dev, fw_path)

    # Now try various CPU release mechanisms
    print("\nProbing CPU release mechanisms...")
    
    # Option A: CPUCS at 0x7F92 (AN2131 documented)
    # Bit 0 = 8051RES: 0=run, 1=reset
    print("\nA) CPUCS 0x7F92 (standard AN2131):")
    try_cpucs_write(dev, 0x7F92, 0x00, "release CPU (0x00)")
    
    # Option B: USBCS register to trigger renumeration
    # USBCS: bit 1 = RENUM
    print("\nB) USBCS 0x7F96 (trigger RENUM):")
    try_cpucs_write(dev, 0x7F96, 0x02, "set RENUM bit")
    
    # Option C: RCPUDAT 0x7F94 - Reset/Upload Data
    print("\nC) RCPUDAT 0x7F94:")
    try_cpucs_write(dev, 0x7F94, 0x00, "write 0")
    
    # Option D: Just do a USB reset by closing
    print("\nD) USB bus reset by closing device...")
    try:
        usb.util.dispose_resources(dev)
        print("  Device closed - should reset on next enumeration")
    except:
        pass
    
    print("\nDone. Check with 'ezwriter-cli list' for VID 0548:1005.")

if __name__ == "__main__":
    sys.exit(main())
