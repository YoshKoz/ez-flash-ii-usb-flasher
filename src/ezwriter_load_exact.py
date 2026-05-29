#!/usr/bin/env python3
"""Exact ezwinit.sys AN2131 loader sequence, using PyUSB/libusb under WSL."""
import struct
import time
from pathlib import Path
import usb.core
import usb.util

EZWINIT_SYS = Path('/mnt/c/Users/yoshi/ezwriter-reverse/original_backup/ezwinit.sys')
IMAGE_BASE = 0x10000
BOOT_VID = 0x0547
BOOT_PID = 0x2131
CPUCS = 0x7F92
MAX_ADDR = 0x1B3F

# PE sections from ezwinit.sys
SECTIONS = [
    ('.text',  0x02C0, 0x02C0, 0x0540),
    ('.rdata', 0x0800, 0x0800, 0x00A0),
    ('.data',  0x08A0, 0x08A0, 0x27E0),
    ('INIT',   0x3080, 0x3080, 0x01A0),
]

def rva_to_raw(rva):
    for _, vaddr, raw, size in SECTIONS:
        if vaddr <= rva < vaddr + size:
            return raw + (rva - vaddr)
    raise ValueError(hex(rva))

def va_to_raw(va):
    return rva_to_raw(va - IMAGE_BASE)

def parse_table(data, va):
    raw = va_to_raw(va)
    table = data[raw:]
    blocks = []
    off = 0
    while True:
        block = table[off:off+0x16]
        if len(block) < 0x16:
            break
        length = block[0]
        addr = struct.unpack('<H', block[2:4])[0]
        term = block[4]
        payload = bytes(block[5:5+length])
        blocks.append((addr, payload, length, off))
        off += 0x16
        # driver loop: after adding 0x16, continue while byte [ebx+2] == 0
        # with ebx pointing block_start+2; next byte [ebx+2] == next block[4]
        if off + 4 >= len(table) or table[off + 4] != 0:
            break
    return blocks

def ctrl_write(dev, addr, payload):
    n = dev.ctrl_transfer(0x40, 0xA0, addr, 0, payload, timeout=5000)
    if n != len(payload):
        raise RuntimeError(f'short write addr={addr:04x} wrote={n} expected={len(payload)}')

def cpucs(dev, value):
    print(f'CPUCS <- {value}')
    ctrl_write(dev, CPUCS, bytes([value]))
    time.sleep(0.05)

def write_table(dev, name, blocks):
    # Driver also has a read pass for addr > MAX_ADDR via A3; skip because bootloader read unsupported.
    writable = [(addr,payload,length,off) for addr,payload,length,off in blocks if addr <= MAX_ADDR and length]
    print(f'{name}: writing {len(writable)} chunks')
    for i, (addr, payload, length, off) in enumerate(writable):
        ctrl_write(dev, addr, payload)
        if i % 20 == 0 or i == len(writable)-1:
            print(f'  {i+1:03}/{len(writable)} addr=0x{addr:04x} len={len(payload)}')
    time.sleep(0.1)

def main():
    data = EZWINIT_SYS.read_bytes()
    table1 = parse_table(data, 0x12B58)
    table2 = parse_table(data, 0x108A0)
    print(f'table1={len(table1)} blocks table2={len(table2)} blocks')

    dev = usb.core.find(idVendor=BOOT_VID, idProduct=BOOT_PID)
    if dev is None:
        raise SystemExit('device 0547:2131 not found')
    print(f'found {dev.idVendor:04x}:{dev.idProduct:04x}')
    try:
        dev.set_configuration()
    except Exception as e:
        print(f'set_configuration: {e}')

    # Exact main sequence from ezwinit.sys around 0x4d8.
    cpucs(dev, 1)
    # Function 0x6e4 internally asserts CPUCS=1 before writing; keep exact-enough.
    cpucs(dev, 1)
    write_table(dev, 'table1', table1)
    cpucs(dev, 0)

    cpucs(dev, 1)
    write_table(dev, 'table2', table2)

    cpucs(dev, 1)
    cpucs(dev, 0)

    print('done; wait for re-enumeration')
    time.sleep(5)

if __name__ == '__main__':
    main()
