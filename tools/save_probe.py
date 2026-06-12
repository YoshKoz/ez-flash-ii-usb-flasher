#!/usr/bin/env python3
"""
EZ-Writer II Save Read Probe
Tests multiple strategies to read save data from EZ-Flash II cartridge.
Requires: libusb, pyusb (pip install pyusb)
"""
import os
import sys
if sys.platform == 'win32':
    os.add_dll_directory(os.path.dirname(__file__))
    os.add_dll_directory(os.path.expanduser('~'))
    os.add_dll_directory(os.path.join(os.environ.get('SystemRoot', 'C:\\Windows'), 'System32'))
import usb.core
import usb.util
import time
import struct

EZWRITER_VID = 0x0548
EZWRITER_PID = 0x1005
TIMEOUT = 5000

def find_device():
    dev = usb.core.find(idVendor=EZWRITER_VID, idProduct=EZWRITER_PID)
    if dev is None:
        print("EZ-Writer not found!")
        sys.exit(1)
    try:
        if dev.is_kernel_driver_active(0):
            dev.detach_kernel_driver(0)
    except NotImplementedError:
        pass
    try:
        dev.set_configuration()
    except usb.core.USBError:
        pass  # already configured
    return dev

def write_bulk(dev, data, pad=10):
    """Write to EP4 OUT (padded to 10 bytes)"""
    if isinstance(data, bytes):
        data = bytearray(data)
    if isinstance(data, list):
        data = bytearray(data)
    while len(data) < pad:
        data.append(0x00)
    dev.write(0x04, bytes(data[:pad]), TIMEOUT)

def read_bulk(dev, length=64):
    """Read from EP2 IN"""
    return bytes(dev.read(0x82, length, TIMEOUT))

def print_chunk(label, data):
    h = ' '.join(f'{b:02x}' for b in data[:16])
    print(f"  {label}: {h}... ({len(data)} bytes)")

def method_select_then_read(dev, select_type, byte_addr, count, word_addr=False):
    """Method: 0x14 select + 0x02 read (original)"""
    write_bulk(dev, bytearray([0x14, select_type, 0x00]))
    time.sleep(0.05)

    all_data = bytearray()
    for chunk in range(count):
        addr = byte_addr + chunk * 64
        if word_addr:
            addr = addr // 2
        cmd = [0x02, addr & 0xFF, (addr >> 8) & 0xFF, (addr >> 16) & 0xFF, select_type, 0]
        write_bulk(dev, bytes(cmd[:5]))
        time.sleep(0.1)
        try:
            buf = read_bulk(dev)
        except usb.core.USBError as e:
            print(f"    [{chunk}] error: {e}")
            break
        all_data.extend(buf)
    return bytes(all_data)

def method_rom_read_at_offset(dev, offset_bytes, byte_addr, count):
    """Method: use cmd 0x01 (ROM read) at a save-region offset"""
    all_data = bytearray()
    for chunk in range(count):
        save_addr = offset_bytes + byte_addr + chunk * 64
        word_addr = save_addr // 2
        bank = (word_addr >> 16) & 0xFF
        addr16 = word_addr & 0xFFFF
        cmd = [0x01, addr16 & 0xFF, (addr16 >> 8) & 0xFF, bank]
        write_bulk(dev, bytes(cmd))
        time.sleep(0.005)
        try:
            buf = read_bulk(dev)
        except usb.core.USBError as e:
            print(f"    [{chunk}] error: {e}")
            break
        all_data.extend(buf)
    return bytes(all_data)

def method_read_without_select(dev, byte_addr, count, select_type):
    """Method: 0x02 without 0x14 select first"""
    all_data = bytearray()
    for chunk in range(count):
        addr = byte_addr + chunk * 64
        cmd = [0x02, addr & 0xFF, (addr >> 8) & 0xFF, (addr >> 16) & 0xFF, select_type, 0]
        write_bulk(dev, bytes(cmd[:5]))
        time.sleep(0.1)
        try:
            buf = read_bulk(dev)
        except usb.core.USBError as e:
            print(f"    [{chunk}] error: {e}")
            break
        all_data.extend(buf)
    return bytes(all_data)

def method_unlock_then_read(dev, select_type, byte_addr, count):
    """Method: send unlock sequence first, then 0x14+0x02"""
    # Unlock (asie protocol)
    unlock = [(0x9FE000, 0xD200), (0x800000, 0x1500),
              (0x802000, 0xD200), (0x804000, 0x1500)]
    for addr, val in unlock:
        cmd = [0x19, addr & 0xFF, (addr >> 8) & 0xFF, (addr >> 16) & 0xFF,
               val & 0xFF, (val >> 8) & 0xFF]
        write_bulk(dev, bytes(cmd))
        time.sleep(0.005)

    return method_select_then_read(dev, select_type, byte_addr, count, word_addr=False)

def method_register_setup(dev, select_type, byte_addr, count):
    """Method: EZ3 register setup (unlock + set page) + save read"""
    # EZ3-style open
    reg_writes = [(0xFF0000, 0xD2FF), (0x000000, 0x15FF),
                  (0x010000, 0xD2FF), (0x020000, 0x15FF),
                  (0xE20000, 0x15FF), (0xFE0000, 0x15FF)]
    for addr, val in reg_writes:
        cmd = [0x19, addr & 0xFF, (addr >> 8) & 0xFF, (addr >> 16) & 0xFF,
               val & 0xFF, (val >> 8) & 0xFF]
        write_bulk(dev, bytes(cmd))
        time.sleep(0.005)

    # Set RAM page 0
    page_writes = [(0xFF0000, 0xD2FF), (0x000000, 0x15FF),
                   (0x010000, 0xD2FF), (0x020000, 0x15FF),
                   (0xE00000, 0x0000), (0xFE0000, 0x15FF)]
    for addr, val in page_writes:
        cmd = [0x19, addr & 0xFF, (addr >> 8) & 0xFF, (addr >> 16) & 0xFF,
               val & 0xFF, (val >> 8) & 0xFF]
        write_bulk(dev, bytes(cmd))
        time.sleep(0.005)

    return method_select_then_read(dev, select_type, byte_addr, count, word_addr=False)


def method_rom_read_offset_range(dev):
    """Try reading cmd 0x01 at various offsets to find save data"""
    offsets = [0, 0x100000, 0x200000, 0x400000, 0x600000,
               0x800000, 0xA00000, 0xC00000, 0xE00000,
               0x1000000, 0x1200000, 0x1400000, 0x1600000,
               0x1800000, 0x1A00000, 0x1C00000, 0x1E00000]
    results = {}
    for off in offsets:
        data = method_rom_read_at_offset(dev, off, 0, 4)
        # Check if looks like GBA ROM (header magic at offset 4)
        is_gba = len(data) >= 8 and data[4:8] in [
            bytes([0x24, 0xFF, 0xAE, 0x51]),  # GBA boot
            bytes([0xFE, 0x7F, 0x1C, 0xEA]),  # GBA boot alt
        ]
        if is_gba:
            title = ''
            if len(data) >= 0xAC:
                title = bytes(data[0xA0:0xAC]).decode('ascii', errors='replace').rstrip('\x00').strip()
            results[off] = ('GBA_ROM', title)
        else:
            results[off] = ('data', ' '.join(f'{b:02x}' for b in data[:8]))
        print(f"  0x{off:07X}: {results[off][0]:8s} {results[off][1]}")
    return results


def main():
    print("EZ-Writer II Save Read Probe")
    print("=" * 50)

    dev = find_device()
    print(f"Device found: {dev}")
    time.sleep(0.1)

    # === Test 1: Original method with different save types ===
    print("\n=== Test 1: 0x14+0x02 with different types ===")
    for stype_int, label in [(ord('s'), 'SRAM'), (ord('e'), 'EEPROM'), (ord('f'), 'FLASH')]:
        data = method_select_then_read(dev, stype_int, 0, 4, word_addr=False)
        print_chunk(f"type={label} (0x{stype_int:02X})", data)
        time.sleep(0.05)

    # === Test 2: Word addressing ===
    print("\n=== Test 2: Word addressing (byte_addr/2) ===")
    for stype_int, label in [(ord('f'), 'FLASH'), (ord('e'), 'EEPROM'), (ord('s'), 'SRAM')]:
        data = method_select_then_read(dev, stype_int, 0, 4, word_addr=True)
        print_chunk(f"type={label} word_addr", data)
        time.sleep(0.05)

    # === Test 3: Different byte addresses ===
    print("\n=== Test 3: Different addresses (type='f') ===")
    write_bulk(dev, [0x14, ord('f'), 0x00])
    time.sleep(0.05)
    for sub_addr in [0, 64, 128, 256, 512, 1024, 4096, 65536, 0x100000]:
        addr = sub_addr
        cmd = [0x02, addr & 0xFF, (addr >> 8) & 0xFF, (addr >> 16) & 0xFF, ord('f'), 0]
        write_bulk(dev, bytes(cmd[:5]))
        time.sleep(0.1)
        try:
            buf = read_bulk(dev)
            h = ' '.join(f'{b:02x}' for b in buf[:8])
            print(f"  addr=0x{addr:06X}: {h}...")
        except usb.core.USBError as e:
            print(f"  addr=0x{addr:06X}: error: {e}")
        time.sleep(0.05)

    # === Test 4: Without 0x14 select ===
    print("\n=== Test 4: 0x02 WITHOUT 0x14 select ===")
    data = method_read_without_select(dev, 0, 4, ord('f'))
    print_chunk("no select, type=f", data)

    # === Test 5: Unlock sequence first ===
    print("\n=== Test 5: Unlock + 0x14+0x02 ===")
    data = method_unlock_then_read(dev, ord('f'), 0, 4)
    print_chunk("unlocked, type=f", data)

    # === Test 6: EZ3 register setup ===
    print("\n=== Test 6: EZ3 register setup + save read ===")
    data = method_register_setup(dev, ord('f'), 0, 4)
    print_chunk("register setup, type=f", data)

    # === Test 7: ROM read at various offsets ===
    print("\n=== Test 7: ROM read (cmd 0x01) at various offsets ===")
    method_rom_read_offset_range(dev)

    # === Test 8: Register probe ===
    print("\n=== Test 8: Register reads (0x1A) at key addresses ===")
    for reg_addr in [0x000000, 0x800000, 0x9C0000, 0xE00000, 0x9FE000,
                     0xFF0000, 0x0E0000, 0x600000]:
        cmd = [0x1A, reg_addr & 0xFF, (reg_addr >> 8) & 0xFF, (reg_addr >> 16) & 0xFF]
        write_bulk(dev, bytes(cmd))
        time.sleep(0.01)
        try:
            buf = read_bulk(dev)
            h = ' '.join(f'{b:02x}' for b in buf[:8])
            print(f"  reg=0x{reg_addr:06X}: {h}...")
        except usb.core.USBError as e:
            print(f"  reg=0x{reg_addr:06X}: {e}")

    # === Test 9: NOR flash address region read via 0x01 === 
    print("\n=== Test 9: cmd 0x01 at first 256 addresses ===")
    # Read first 256 bytes at word addresses 0..127
    data = bytearray()
    for addr in range(128):
        cmd = [0x01, (addr*2) & 0xFF, ((addr*2) >> 8) & 0xFF, 0]
        write_bulk(dev, bytes(cmd))
        time.sleep(0.002)
        try:
            buf = read_bulk(dev)
            data.extend(buf[:2])  # each word addr returns 2 bytes at addr*2
        except:
            pass
    print(f"  Read {len(data)} bytes")
    h = ' '.join(f'{b:02x}' for b in data[:32])
    print(f"  First 32 bytes: {h}")

    print("\n=== Probe complete ===")

if __name__ == '__main__':
    main()
