#!/usr/bin/env python3
"""
Upload tusbez.bin firmware to AN2131 EZ-USB via EP0 control transfers.
Works in ACTIVE mode (0548:1005) by holding CPU in reset first.
"""
import os
import sys
import time

if sys.platform == 'win32':
    os.add_dll_directory(os.path.expanduser('~'))
    os.add_dll_directory(os.path.join(os.environ.get('SystemRoot', 'C:\\Windows'), 'System32'))

import usb.core
import usb.util

EZWRITER_VID_BOOT = 0x0547
EZWRITER_PID_BOOT = 0x2131
EZWRITER_VID = 0x0548
EZWRITER_PID = 0x1005
CPUCS_ADDR = 0x7F92

def main():
    if len(sys.argv) < 2:
        print(f"Usage: {sys.argv[0]} <firmware.bin>")
        sys.exit(1)
    
    fw_path = sys.argv[1]
    with open(fw_path, 'rb') as f:
        fw = f.read()
    print(f"Firmware: {len(fw)} bytes from {fw_path}")
    
    dev = usb.core.find(idVendor=EZWRITER_VID, idProduct=EZWRITER_PID)
    if dev is None:
        dev = usb.core.find(idVendor=EZWRITER_VID_BOOT, idProduct=EZWRITER_PID_BOOT)
    if dev is None:
        print("Device not found in active or bootloader mode!")
        sys.exit(1)
    
    try:
        dev.set_configuration()
    except:
        pass
    
    print("Device found. Holding CPU in reset...")
    
    # Step 1: Hold CPU in reset (write 0x00 to CPUCS)
    ret = dev.ctrl_transfer(0x40, 0xA0, CPUCS_ADDR & 0xFFFF, (CPUCS_ADDR >> 16) & 0xFFFF, b'\x00', 5000)
    time.sleep(0.1)
    print(f"CPU held (wrote 0x00 to CPUCS).")
    
    # Step 2: Upload firmware in 64-byte chunks
    chunk_size = 64
    total = len(fw)
    for i in range(0, total, chunk_size):
        chunk = fw[i:i+chunk_size]
        addr = i
        wval = addr & 0xFFFF
        windex = (addr >> 16) & 0xFFFF
        dev.ctrl_transfer(0x40, 0xA0, wval, windex, chunk, 5000)
        if (i // chunk_size) % 16 == 0:
            pct = min(i + chunk_size, total)
            print(f"\r  Uploading... {pct}/{total} bytes", end='', flush=True)
    
    print(f"\n  Uploaded {total} bytes.")
    
    # Step 3: Verify first few bytes
    ret = dev.ctrl_transfer(0xC0, 0xA0, 0, 0, 4, 5000)
    print(f"  First 4 bytes of RAM: {' '.join(f'{b:02x}' for b in ret)}")
    
    # Step 4: Start CPU (write 0x01 to CPUCS)
    print("Starting CPU (device will re-enumerate)...")
    try:
        dev.ctrl_transfer(0x40, 0xA0, CPUCS_ADDR & 0xFFFF, (CPUCS_ADDR >> 16) & 0xFFFF, b'\x01', 5000)
    except:
        pass  # device disconnects as it re-enumerates
    print("CPU started. Waiting for device to re-enumerate...")
    time.sleep(3)
    
    # Check if device came back in active mode
    dev2 = usb.core.find(idVendor=EZWRITER_VID, idProduct=EZWRITER_PID)
    if dev2:
        print(f"Device re-enumerated: {dev2}")
    else:
        print("Warning: device did not re-enumerate in active mode")
    
    print("\nFirmware upload complete!")

if __name__ == '__main__':
    main()
