"""
EZ-Writer II Device Detection & Identification
Python proof-of-concept using pyusb / libusb.

Prerequisites:
  pip install pyusb
  
  On Windows: install libusb driver via Zadig (WinUSB mode) for:
    - VID 0547 PID 2131 (bootloader mode)
    - VID 0548 PID 1005 (active mode)

Usage:
  python poc_identify.py [list|info|download]
"""

import sys
import os
import struct

try:
    import usb.core
    import usb.util
except ImportError:
    print("ERROR: pyusb not installed. Run: pip install pyusb")
    sys.exit(1)

# Device identities
BOOTLOADER_VID = 0x0547
BOOTLOADER_PID = 0x2131
ACTIVE_VID = 0x0548
ACTIVE_PID = 0x1005

def find_device(vid, pid):
    """Find a USB device by VID/PID."""
    dev = usb.core.find(idVendor=vid, idProduct=pid)
    return dev

def print_descriptors(dev, label):
    """Print USB descriptors for a device."""
    if dev is None:
        print(f"  [{label}] Not found")
        return
    
    print(f"\n=== {label} ===")
    print(f"  VID:PID = {dev.idVendor:04X}:{dev.idProduct:04X}")
    
    # Device descriptor
    desc = dev.device_descriptor
    print(f"  bcdUSB = {desc.bcdUSB>>8}.{desc.bcdUSB&0xFF:02d}")
    print(f"  bDeviceClass = 0x{desc.bDeviceClass:02X}")
    print(f"  bDeviceSubClass = 0x{desc.bDeviceSubClass:02X}")
    print(f"  bDeviceProtocol = 0x{desc.bDeviceProtocol:02X}")
    print(f"  bMaxPacketSize0 = {desc.bMaxPacketSize0}")
    print(f"  idVendor = 0x{desc.idVendor:04X}")
    print(f"  idProduct = 0x{desc.idProduct:04X}")
    print(f"  bcdDevice = {desc.bcdDevice>>8}.{desc.bcdDevice&0xFF:02d}")
    print(f"  iManufacturer = {desc.iManufacturer}")
    print(f"  iProduct = {desc.iProduct}")
    print(f"  iSerialNumber = {desc.iSerialNumber}")
    print(f"  bNumConfigurations = {desc.bNumConfigurations}")
    
    # String descriptors
    try:
        # Need to set config first for string descriptors on some systems
        try:
            dev.set_configuration()
        except usb.core.USBError:
            pass  # Already configured
        
        lang = usb.util.get_lang_ids(dev)
        if lang:
            if desc.iManufacturer:
                try:
                    man = usb.util.get_string(dev, lang[0], desc.iManufacturer)
                    print(f"  Manufacturer = '{man}'")
                except:
                    pass
            if desc.iProduct:
                try:
                    prod = usb.util.get_string(dev, lang[0], desc.iProduct)
                    print(f"  Product = '{prod}'")
                except:
                    pass
            if desc.iSerialNumber:
                try:
                    sn = usb.util.get_string(dev, lang[0], desc.iSerialNumber)
                    print(f"  Serial = '{sn}'")
                except:
                    pass
    except Exception as e:
        print(f"  (String descriptors: {e})")
    
    # Configurations and interfaces
    try:
        cfg = dev.get_active_configuration()
        print(f"  Active Config: {cfg.bConfigurationValue}")
        for iface in cfg:
            alt = iface.bAlternateSetting
            print(f"  Interface {iface.bInterfaceNumber} alt={alt}: "
                  f"class=0x{iface.bInterfaceClass:02X} "
                  f"subclass=0x{iface.bInterfaceSubClass:02X} "
                  f"protocol=0x{iface.bInterfaceProtocol:02X}")
            for ep in iface:
                direction = "IN" if usb.util.endpoint_direction(ep.bEndpointAddress) == usb.util.ENDPOINT_IN else "OUT"
                print(f"    EP 0x{ep.bEndpointAddress:02X} {direction}  "
                      f"max_pkt={ep.wMaxPacketSize}  type={ep.bmAttributes & 0x03}")
    except Exception as e:
        print(f"  (Config: {e})")

def cmd_list():
    """List all EZ-Writer related devices."""
    print("EZ-Writer II Device Detection")
    print("=" * 50)
    
    boot = find_device(BOOTLOADER_VID, BOOTLOADER_PID)
    active = find_device(ACTIVE_VID, ACTIVE_PID)
    
    print_descriptors(boot, "BOOTLOADER MODE (Cypress EZ-USB)")
    print_descriptors(active, "ACTIVE MODE (EZ-Writer)")
    
    if boot is None and active is None:
        print("\n⚠ No EZ-Writer device found.")
        print("  Make sure the device is plugged in.")
        print("  Check Device Manager for VID 0547 PID 2131 or VID 0548 PID 1005.")
        print("  If found but not working, install WinUSB driver via Zadig.")
        print("\n  All USB devices matching EZ family:")
        for dev in usb.core.find(find_all=True):
            vid = dev.idVendor
            pid = dev.idProduct
            if vid in (0x0547, 0x0548, 0x0550, 0x0451):
                print(f"    VID={vid:04X} PID={pid:04X}")

def cmd_info():
    """Show detailed info."""
    boot = find_device(BOOTLOADER_VID, BOOTLOADER_PID)
    active = find_device(ACTIVE_VID, ACTIVE_PID)
    
    if active:
        print_descriptors(active, "EZ-Writer ACTIVE")
    elif boot:
        print_descriptors(boot, "EZ-Writer BOOTLOADER")
        print("\n⚠ Device in bootloader mode.")
        print("  Run 'firmware-download' to load tusbez.bin firmware.")
    else:
        print("No EZ-Writer device found.")

def firmware_download():
    """Download 8051 firmware to EZ-USB."""
    boot = find_device(BOOTLOADER_VID, BOOTLOADER_PID)
    if boot is None:
        print("ERROR: Device not in bootloader mode (VID 0547 PID 2131).")
        print("  Unplug and replug the device, then try again.")
        return 1
    
    print(f"Found device in bootloader mode.")
    
    # Find firmware file
    search_paths = [
        "original/EZ Client/USB_Drivers/tusbez.bin",
        "../original/EZ Client/USB_Drivers/tusbez.bin",
        "tusbez.bin",
    ]
    fw_path = None
    for p in search_paths:
        if os.path.exists(p):
            fw_path = p
            break
    
    if fw_path is None:
        print("ERROR: Cannot find tusbez.bin firmware.")
        print(f"  Searched: {search_paths}")
        print("  Copy tusbez.bin to this directory or provide path.")
        return 1
    
    with open(fw_path, "rb") as f:
        firmware = f.read()
    
    print(f"Firmware: {fw_path} ({len(firmware)} bytes)")
    
    # Validate
    if len(firmware) < 4:
        print("ERROR: Firmware too small.")
        return 1
    
    try:
        # Set configuration
        boot.set_configuration()
        
        # Claim interface
        cfg = boot.get_active_configuration()
        iface = cfg[(0, 0)]
        
        print("\nStarting EZ-USB firmware download...")
        
        # EZ-USB CPUCS register at 0xE600
        CPUCS_ADDR = 0xE600
        VR_WRITE = 0xA0  # Cypress vendor write to RAM
        
        # 1. Hold CPU in reset
        print("  [1/3] Holding CPU in reset...")
        boot.ctrl_transfer(
            bmRequestType=0x40,  # Host-to-Device, Vendor, Device
            bRequest=VR_WRITE,
            wValue=CPUCS_ADDR & 0xFFFF,
            wIndex=0,
            data_or_wLength=b'\x00',
            timeout=5000
        )
        
        # 2. Download firmware in 64-byte chunks
        print(f"  [2/3] Downloading {len(firmware)} bytes...")
        chunk_size = 64
        offset = 0
        while offset < len(firmware):
            end = min(offset + chunk_size, len(firmware))
            chunk = firmware[offset:end]
            boot.ctrl_transfer(
                bmRequestType=0x40,
                bRequest=VR_WRITE,
                wValue=offset & 0xFFFF,
                wIndex=(offset >> 16) & 0xFFFF,
                data_or_wLength=chunk,
                timeout=5000
            )
            offset = end
            if offset % 1024 == 0 or offset == len(firmware):
                print(f"    Progress: {offset}/{len(firmware)} bytes", end='\r')
        print()
        
        # 3. Start CPU (re-enumerate)
        print("  [3/3] Starting CPU (device will re-enumerate)...")
        boot.ctrl_transfer(
            bmRequestType=0x40,
            bRequest=VR_WRITE,
            wValue=CPUCS_ADDR & 0xFFFF,
            wIndex=0,
            data_or_wLength=b'\x01',
            timeout=5000
        )
        
        print("\n✓ Firmware download complete!")
        print("  Device should now re-enumerate as VID 0548 PID 1005.")
        print("  Wait 3-5 seconds, then run: python poc_identify.py list")
        
    except usb.core.USBError as e:
        if "No such device" in str(e) or "not found" in str(e):
            print("\n✓ Firmware sent (device disconnected for re-enumeration - this is normal).")
            print("  Wait 3-5 seconds, then run: python poc_identify.py list")
        else:
            print(f"\nERROR during firmware download: {e}")
            return 1
    
    return 0

def main():
    if len(sys.argv) < 2:
        print("Usage:")
        print("  python poc_identify.py list      - List EZ-Writer devices")
        print("  python poc_identify.py info      - Show device info")
        print("  python poc_identify.py download  - Download firmware to EZ-USB")
        return
    
    cmd = sys.argv[1].lower()
    
    if cmd == "list":
        cmd_list()
    elif cmd == "info":
        cmd_info()
    elif cmd == "download":
        sys.exit(firmware_download())
    else:
        print(f"Unknown command: {cmd}")
        sys.exit(1)

if __name__ == "__main__":
    main()
