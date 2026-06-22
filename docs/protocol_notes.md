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

### CONFIRMED protocol (firmware RE, 2026-06-22)

Reverse-engineered from the **patched** 8051 image (`an2131_fw_v2.bin` + `loader_table1.bin`
+ `loader_table2.bin` applied at their patch addresses — the loader tables contain the
actual cart-bus routines, so an unpatched image cannot talk to the cart at all).
Endpoints: **EP4 OUT** (commands), **EP2 IN / 0x82** (64-byte data frames).

Command dispatch is a Keil switch at `0x0733 (LCALL 0x15CE)`, keyed on the command byte
stored at XDATA `0x7CC0`. Case table at `0x0736`, record format `[addrHi, addrLo, caseval]`,
terminated by `00 00` then a default handler address.

| Cmd | Handler | Packet | Meaning |
|-----|---------|--------|---------|
| 0x01 | 0x075E | `[01, wlo, wmid, bank]` | ROM read, 24-bit **word** address (byte/2), streams 64 B, prefetched |
| 0x02 | 0x07B7 | `[02, alo, ahi, bank, type]` | Save setup + flash-ready toggle poll (`type` 0x66=FLASH @0x114A, 0x68, 0x65=EEPROM). **Hangs if flash not in read-array mode.** |
| 0x03 | 0x0A09 | `[03, alo, ahi, 0, 0]` | Stream **64 bytes** from save chip at 16-bit byte addr `ahi:alo` (one 64KB bank). Sets save chip-select itself. |
| 0x14 | 0x0AE1 | `[14, type, 0]` | Select save type/handler |
| 0x19 | 0x0AF3 | `[19, alo, amid, ahi, dlo, dhi]` | 16-bit **write** to ROM bus word address (JEDEC/CPLD) |
| 0x20 | 0x0BA3 | `[20, alo, ahi, data]` | **Write one byte** `data` to save chip addr `ahi:alo` |
| 0x21 | 0x0BDF | `[21, alo, ahi]` | Read one byte from save chip addr `ahi:alo` |

Cart-bus I/O is via AN2131 XDATA ports: `0x7F96/0x7F97` = address low/high latch,
`0x7F98` = strobe/control (0x9B latch, 0x89/0x8B read, 0x97/0xBF write, 0xB7/0xB5 read),
`0x7F99` = data-in, `0x7F9C/0x7F9D` = chip-select/mode.

### Dumping a 128KB GBA FLASH save (e.g. Pokémon Gen 3)

Save chip sits on GBA **/CS2** (8-bit), NOT the ROM bus — reachable only through the
native save commands above. Pokémon Sapphire = **Macronix MX29L010, JEDEC C2:09**,
two 64KB banks switched by a JEDEC **command** (no address pin).

1. `cmd 0x14 [14,0x66,0]` — select FLASH handler.
2. For each bank (0,1): switch bank with `cmd 0x20` writes
   `AA→0x5555, 55→0x2AAA, B0→0x5555, bank→0x0000`.
3. Read the bank in 64-byte frames: `cmd 0x03 [03, alo, ahi, 0, 0]` for `addr` 0..0xFFFF.
   Store bank 1 at output offset 0x10000.
4. On exit restore read-array: `AA→0x5555, 55→0x2AAA, F0→0x5555` (and bank 0).

JEDEC ID check: `AA→0x5555, 55→0x2AAA, 90→0x5555` then read byte 0x0000 (mfr) / 0x0001
(device); reset with F0. Use **`cmd 0x03`-only** for reads — it skips the `cmd 0x02`
toggle-poll that infinite-hangs the firmware when the flash is in a command state
(recovery from that hang requires a physical replug). Implemented in
`read_flash128_save` in `src/ezwriter-cli/src/main.rs`.

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
