# EZ-Writer II USB Protocol Notes

## Overview

The EZ-Writer II works in two stages:

### Bootloader mode
- Device appears as a Cypress EZ-USB AN2131.
- PC uploads firmware over USB control endpoint EP0.

### Active mode
- Firmware starts, device re-enumerates with new VID/PID.
- PC communicates over USB bulk endpoints.
- Firmware translates USB commands into EZ-Flash II cartridge operations.

```
EZClient.exe / libusb
        │
        │ USB control EP0 / bulk transfers
        ▼
Cypress EZ-USB AN2131
8051 firmware: tusbez.bin
        │
        ▼
EZ-Flash II GBA cartridge
```

## Phase 1 — Firmware Upload

### Initial USB identity

| Field | Value |
|-------|-------|
| VID | 0x0547 |
| PID | 0x2131 |
| Mode | Cypress EZ-USB bootloader / ReNumeration mode |

The main vendor request:

| Field | Value |
|-------|-------|
| bmRequestType | 0x40 |
| Direction | Host → device |
| Type | Vendor |
| Request | 0xA0 |
| Purpose | Write bytes to internal RAM |

### Upload sequence

1. **Hold CPU in reset**
   - bRequest = 0xA0, wValue = 0xE600, wIndex = 0x0000, Data = [0x00]

2. **Upload firmware chunks** (repeat for each chunk)
   - bRequest = 0xA0, wValue = firmware offset low, wIndex = firmware offset high, Data = firmware bytes

3. **Start CPU** (triggers re-enumeration)
   - bRequest = 0xA0, wValue = 0xE600, wIndex = 0x0000, Data = [0x01]

### After re-enumeration

| Field | Value |
|-------|-------|
| VID | 0x0548 |
| PID | 0x1005 |

### Firmware: tusbez.bin

| Property | Value |
|----------|-------|
| Size | 5,584 bytes |
| CPU | 8051 / Cypress enhanced 8051 |
| Entry | 0x0000 → LJMP 0x1202 |
| Target | EZ-USB internal RAM |

Firmware handles:
- USB endpoint setup (EP0 control, EP2 bulk OUT, EP6 bulk IN)
- GBA cartridge communication pins
- Command dispatch loop

## Phase 2 — Active Mode

### USB endpoint layout

| Endpoint | Direction | Type | Purpose |
|----------|-----------|------|---------|
| EP0 | Control | Control | Standard + vendor control |
| EP2 | OUT | Bulk | Host → device (commands/data to cart) |
| EP6 | IN | Bulk | Device → host (responses/data from cart) |

### Hypothesized packet format (needs confirmation)

**Host → Device (EP2 OUT):**

| Byte 0 | Byte 1 | Byte 2 | Byte 3 | Bytes 4..n |
|--------|--------|--------|--------|-------------|
| Command | Param 0 | Param 1 | Param 2 | Optional payload |

**Device → Host (EP6 IN):**

| Byte 0 | Byte 1 | Byte 2 | Bytes 3..n |
|--------|--------|--------|-------------|
| Status | Length low | Length high | Response data |

**Status values:**

| Value | Meaning |
|-------|---------|
| 0x00 | Success |
| 0xFF | Error |

### Hypothesized commands (from EZClient strings + driver analysis)

| Cmd | Name | Purpose |
|-----|------|---------|
| 0x01 | INIT | Initialize cartridge interface |
| 0x02 | IDENTIFY | Detect cartridge |
| 0x03 | READ_ID | Read cartridge ID |
| 0x04 | GET_STATUS | Read device/cart status |
| 0x10 | READ_SECTOR | Read ROM/save sector |
| 0x11 | READ_SAVE | Read save data |
| 0x20 | WRITE_SECTOR | Write ROM sector |
| 0x21 | WRITE_SAVE | Write save data |
| 0x30 | ERASE_SECTOR | Erase flash sector |
| 0x40 | VERIFY | Verify written data |

### Status response

EZClient string: `Status:[1]0x%x [2]0x%x [3]0x%x [4]0x%x`

Likely four DWORDs:

| Field | Possible meaning |
|-------|------------------|
| [1] | Status / error code |
| [2] | Cartridge type (Flash/EEPROM/SRAM) |
| [3] | ROM size or sector number |
| [4] | Progress / bytes remaining |

### Save types (from EZClient strings)

| Type | Meaning |
|------|---------|
| FLASH_TYPE | Flash-based save (128KB, etc.) |
| EEPROM_TYPE | EEPROM-based save (512B, 8KB, etc.) |
| SRAM_TYPE | SRAM-based save (32KB, 64KB, etc.) |

## Cartridge Interface

The EZ-Flash II cartridge uses:
- NOR flash for ROM storage (up to 256 Mbit / 32 MB)
- Separate SRAM/EEPROM for save data
- FPGA for bank switching

The 8051 firmware handles:
- USB bulk transfer ↔ cartridge bus translation
- Flash command sequences (sector erase, program, read)
- Save memory read/write
- Bank switching for large ROMs

## Reverse Engineering TODO

- Capture USB traffic with USBPcap + Wireshark in XP VM
- Capture per action: firmware upload, cartridge detect, ROM read/write, save read/write, erase, verify
- Disassemble tusbez.bin as 8051 code; look for:
  - Command dispatch table
  - EP2/EP6 buffer handling
  - Cartridge bus routines
  - Flash command sequences
- Research related projects: gba-link-cable, OpenFlashcart, GBARipper

### Main unknowns

| Unknown | How to confirm |
|---------|----------------|
| Exact command bytes | USB capture or firmware RE |
| Exact packet structure | USB capture |
| Status field meanings | Compare captures |
| Save/flash algorithms | Firmware RE |
| Endpoint usage | USB descriptors + firmware RE |

## Summary

```
1. Device starts as Cypress bootloader (0547:2131)
2. PC uploads tusbez.bin via request 0xA0, starts 8051 CPU
3. Device re-enumerates as 0x0548:0x1005
4. PC sends cartridge commands over bulk EP2 OUT
5. Device replies over bulk EP6 IN
6. Firmware bridges USB ↔ EZ-Flash II cartridge bus
```

Once tusbez.bin and the USB packet format are understood, a modern replacement can be written with libusb, Python, Rust, or C/C++.

## References

- Cypress EZ-USB FX2 TRM (Technical Reference Manual)
- [Infineon EZ-USB FX2LP](https://www.infineon.com/cms/en/product/usb-solutions/ez-usb-fx2lp/)
- EZ-USB General Purpose Driver (ezusb.sys) documentation
- libusb bulk transfer examples
