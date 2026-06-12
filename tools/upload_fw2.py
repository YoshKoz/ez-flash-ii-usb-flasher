#!/usr/bin/env python3
"""
Upload tusbez.bin firmware to AN2131 EZ-USB via EP0 control transfers.
Works in BOOTLOADER mode (0547:2131) using Cypress firmware load protocol.
"""
import os
import sys
import time

if sys.platform == 'win32':
    os.add_dll_directory(os.path.expanduser('~'))
    os.add_dll_directory(os.path.join(os.environ.get('SystemRoot', 'C:\\Windows'), 'System32'))

import usb.core
import usb.util

VID_BOOT = 0x0547
PID_BOOT = 0x2131
VID_ACTIVE = 0x0548
PID_ACTIVE = 0x1005
CPUCS_ADDR = 0x7F92

def main():
    if len(sys.argv) < 2:
        print(f"Usage: {sys.argv[0]} <firmware.bin>")
        sys.exit(1)
    
    fw_path = sys.argv[1]
    with open(fw_path, 'rb') as f:
        fw = f.read()
    print(f"Firmware: {len(fw)} bytes from {fw_path}")
    
    dev = usb.core.find(idVendor=VID_BOOT, idProduct=PID_BOOT)
    if dev is None:
        print("Device not found in bootloader mode!")
        sys.exit(1)
    
    print("Device found in bootloader mode.")
    
    # First try: upload WITHOUT holding CPU in reset
    # The bootloader 8051 should be running and handle 0xA0 commands
    print("Uploading firmware while bootloader is running...")
    
    chunk_size = 64
    total = len(fw)
    
    try:
        for i in range(0, total, chunk_size):
            chunk = fw[i:i+chunk_size]
            addr = i
            wval = addr & 0xFFFF
            windex = (addr >> 16) & 0xFFFF
            dev.ctrl_transfer(0x40, 0xA0, wval, windex, chunk, 5000)
            if (i // chunk_size) % 16 == 0:
                pct = min(i + chunk_size, total)
                print(f"\r  Uploading... {pct}/{total} bytes", end='', flush=True)
    except usb.core.USBError as e:
        print(f"\n  Upload failed at {i}/{total}: {e}")
        return 1
    
    print(f"\n  Uploaded {total} bytes successfully while bootloader running.")
    
    # Now release bootloader and start firmware
    print("Starting firmware (writing CPUCS=0x01 to release CPU)...")
    try:
        dev.ctrl_transfer(0x40, 0xA0, CPUCS_ADDR & 0xFFFF, (CPUCS_ADDR >> 16) & 0xFFFF, b'\x01', 1000)
    except:
        pass  # device may disconnect during re-enumeration
    
    print("Waiting for device to re-enumerate...")
    time.sleep(3)
    
    # Check for device in active mode
    dev2 = usb.core.find(idVendor=VID_ACTIVE, idProduct=PID_ACTIVE)
    if dev2:
        print(f"Device re-enumerated in active mode: {dev2}")
        return 0
    else:
        print("Device did not re-enumerate in active mode.")
        # Maybe it's still in bootloader
        dev_boot = usb.core.find(idVendor=VID_BOOT, idProduct=PID_BOOT)
        if dev_boot:
            print("Device still in bootloader mode.")
        return 1

if __name__ == '__main__':
    sys.exit(main())
