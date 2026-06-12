#!/usr/bin/env python3
"""
Power-cycle a specific USB port on a hub using USB hub class requests.
This resets the EZ-Writer II device by turning off port power.
"""
import os
import sys
import time

if sys.platform == 'win32':
    os.add_dll_directory(os.path.expanduser('~'))
    os.add_dll_directory(os.path.join(os.environ.get('SystemRoot', 'C:\\Windows'), 'System32'))

import usb.core
import usb.util

# Hub class requests
HUB_REQUEST_SET_FEATURE = 0x03
HUB_REQUEST_CLEAR_FEATURE = 0x01
PORT_FEAT_POWER = 0x08
PORT_FEAT_RESET = 0x04

# The EZ-Writer is on a Genesys Logic hub at VID 05E3 PID 0610
# It's on port 2 (device instance ends with &0&2)
HUB_VID = 0x05E3
HUB_PID = 0x0610
EZ_PORT = 2  # port number

def find_hub():
    """Find the USB hub that EZ-Writer is connected to"""
    hubs = list(usb.core.find(find_all=True, idVendor=HUB_VID, idProduct=HUB_PID))
    if not hubs:
        print(f"No hub found with VID={HUB_VID:04X} PID={HUB_PID:04X}")
        return None
    print(f"Found hub: {hubs[0]}")
    hub = hubs[0]
    try:
        hub.set_configuration()
    except:
        pass
    # Claim interface 0 (hub class uses interface 0 for control)
    try:
        if hub.is_kernel_driver_active(0):
            hub.detach_kernel_driver(0)
    except:
        pass
    try:
        usb.util.claim_interface(hub, 0)
    except:
        pass
    return hub

def power_cycle_port(hub, port):
    """Power-cycle a port on a USB hub"""
    # First, turn off port power
    print(f"Turning OFF power on port {port}...")
    try:
        ret = hub.ctrl_transfer(
            0x23,  # HOST_TO_DEVICE | CLASS | INTERFACE
            HUB_REQUEST_CLEAR_FEATURE,
            PORT_FEAT_POWER,
            port,
            None,
            5000
        )
        print(f"  Power OFF: {ret}")
    except Exception as e:
        print(f"  Power OFF failed: {e}")
        return False

    time.sleep(3)

    # Turn on port power
    print(f"Turning ON power on port {port}...")
    try:
        ret = hub.ctrl_transfer(
            0x23,  # HOST_TO_DEVICE | CLASS | INTERFACE
            HUB_REQUEST_SET_FEATURE,
            PORT_FEAT_POWER,
            port,
            None,
            5000
        )
        print(f"  Power ON: {ret}")
    except Exception as e:
        print(f"  Power ON failed: {e}")
        return False

    time.sleep(3)

    # Also send reset
    print(f"Sending reset to port {port}...")
    try:
        ret = hub.ctrl_transfer(
            0x23,
            HUB_REQUEST_SET_FEATURE,
            PORT_FEAT_RESET,
            port,
            None,
            5000
        )
        print(f"  Reset: {ret}")
    except Exception as e:
        print(f"  Reset failed: {e}")

    time.sleep(2)

    return True

def main():
    hub = find_hub()
    if not hub:
        sys.exit(1)

    # Check current port status
    print(f"Reading port {EZ_PORT} status...")
    try:
        data = hub.ctrl_transfer(
            0xA3,  # DEVICE_TO_HOST | CLASS | INTERFACE
            0x00,  # GetHubDescriptor (just for testing)
            0,
            0,
            8,
            5000
        )
        print(f"  Hub descriptor: {' '.join(f'{b:02x}' for b in data)}")
    except Exception as e:
        print(f"  Hub descriptor read: {e}")

    if power_cycle_port(hub, EZ_PORT):
        print("\nPort power-cycled. Waiting for re-enumeration...")
        time.sleep(5)
        
        # Check for EZ-Writer
        for (vid, pid, name) in [(0x0547, 0x2131, "bootloader"), (0x0548, 0x1005, "active")]:
            dev = usb.core.find(idVendor=vid, idProduct=pid)
            print(f"  {name}: {'FOUND' if dev else 'not found'}")

    usb.util.release_interface(hub, 0)

if __name__ == '__main__':
    main()
