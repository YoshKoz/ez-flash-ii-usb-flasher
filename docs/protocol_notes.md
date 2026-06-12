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

## Phase 2 — Active Mode (Reverse Engineered)

### USB endpoint layout (confirmed by probing)

| Endpoint | Direction | Type | Size | Purpose |
|----------|-----------|------|------|---------|
| EP0 | Bidirectional | Control | 64 | Standard + vendor requests |
| EP4 | OUT | Bulk | 64 | Command OUT (CMD_EP) |
| EP2 | IN | Bulk | 64 | Data IN (DATA_EP) |
| EP3-6 | Bidirectional | Bulk | 64 | Other (FPGA, etc.) |

**Note:** Earlier protocol notes speculated EP2 OUT / EP6 IN. Probing confirms:
- CMD_EP = 0x04 (OUT)
- DATA_EP = 0x82 (2 IN, address 0x82)

### Bulk command format

**Host → Device (CMD_EP 0x04):**

| Byte 0 | Byte 1 | Byte 2 | Byte 3 | Bytes 4..n |
|--------|--------|--------|--------|-------------|
| Command | Param 0 | Param 1 | Param 2 | Optional payload |

**Device → Host (DATA_EP 0x82):**

Raw data (64 bytes). No header/status bytes observed.

### Confirmed commands

| Cmd | Size | Name | Purpose |
|-----|------|------|---------|
| 0x01 | 4 | ROM_READ | Read from NOR flash (word addressing) |
| 0x02 | 5 | SAVE_READ | Read save data (byte addressing?) |
| 0x03 | 5+n | SAVE_WRITE | Write save data |
| 0x14 | 3 | SAVE_SELECT | Select save type (FLASH/EEPROM/SRAM) |
| 0x15 | 5 | SAVE_ERASE | Erase FLASH save sector |
| 0x19 | 6 | WRITE_REG | Write cartridge register (24-bit addr + 16-bit data) |
| 0x1A | 4 | READ_REG | Read cartridge register (24-bit addr → 16-bit data) |

### Command details

#### 0x01 — ROM Read (NOR flash)

Format: `[0x01, addr_lo, addr_mid, bank]`
- Address is **word address** (byte address / 2), 16-bit
- Bank is upper address bits (8-bit)
- Returns 64 bytes of NOR flash data
- Used for reading GBA ROM

Firmware handler (at 0x06C3):
- Takes address from reg[0x12], multiplies by 2 → converts word to byte offset
- Writes to EP control register 0xFFF3

#### 0x02 — Save Read

Format: `[0x02, addr_lo, addr_mid, addr_hi, suffix, 0x00]`
- Address is a 3-byte address (possibly byte or word)
- Suffix matches the save type from 0x14
- Returns 64 bytes

Firmware handler (at 0x06D8):
- 16-bit address in reg[0x11]:reg[0x12]
- If address >= 0x0100, uses high byte for bank switching (reg[0x11] & 7)
- Programs cartridge bus to read save chip

Note: There are TWO dispatch points in firmware (0x06AC and 0x10C8), both handling cmds 0x01/0x02/0x03 with different register sets:
- Dispatch 1: uses reg[0x11]:reg[0x12] for address, reg[0x30] for cmd
- Dispatch 2: uses reg[0x17]:reg[0x18] for address, reg[0x07] for save type

#### 0x03 — Save Write

Format: `[0x03, addr_lo, addr_mid, addr_hi, suffix, 64-byte payload]`
- Suffix matches save type
- 64 bytes of data follow the 5-byte header

#### 0x14 — Select Save Type

Format: `[0x14, suffix, 0x00]`
- Suffix: 'f' (0x66) = FLASH, 'e' (0x65) = EEPROM, 's' (0x73) = SRAM
- Must be sent before 0x02 or 0x03

**Key finding from ez3manage GUI code:** The GUI always uses suffix 0x66 ('f') for both read and write, regardless of the actual save type. The save type selection might be handled differently.

#### 0x15 — Erase FLASH Save Sector

Format: `[0x15, addr_lo, addr_mid, addr_hi, suffix]`
- Erases 4KB FLASH sector at the given address

#### 0x19 — Write Cartridge Register (WriteDevice)

Format: `[0x19, addr_lo, addr_mid, addr_hi, data_lo, data_hi]`
- Writes 16-bit value to 24-bit cartridge bus address
- Used for FPGA/CPLD register programming
- See EZ3 source: `WriteDevice(hDev, addr, data)` → `[0x19, addr[0], addr[1], addr[2], data[0], data[1]]`

#### 0x1A — Read Cartridge Register (ReadDevice)

Format: `[0x1A, addr_lo, addr_mid, addr_hi]`
- Reads 16-bit value from 24-bit cartridge bus address
- Returns data on DATA_EP

### EZ-Flash II Register Map (from asie wiki + firmware analysis)

The cartridge has a CPLD/FPGA that maps control registers into the GBA address space.
The 24-bit bus addresses used by 0x19/0x1A correspond to GBA addresses.

**Unlock sequence** (must be sent before accessing cartridge):
```
0x9FE000 → 0xD200
0x800000 → 0x1500
0x802000 → 0xD200
0x804000 → 0x1500
```

**Lock sequence:**
```
0x9FC000 → 0x1500
```

**Key registers:**
- `EZ_ROM_OFFSET` at 0x9880000: ROM page select (in megabits)
- `EZ_RAM_OFFSET` at 0x9C00000: SRAM page select (in megabits; EZ3+)
- Note: EZ3 code uses different addresses (0xC40000 for ROM, 0xE00000 for RAM) — the address mapping is not fully understood

## Save Access Protocol (Hypothesized)

### Problem
The current `save-read` sends: `[0x14, suffix, 0x00]` then `[0x02, addr, suffix]`. The address parameter is **ignored** — all reads return the same 128-byte pattern.

### Possible causes to test

1. **Word addressing**: The 0x02 command might need word addresses (byte_addr/2), like 0x01 does for ROM
2. **Missing unlock**: The cartridge CPLD might need the unlock sequence before save chip access
3. **Register setup**: The save chip page/offset may need to be set via 0x19 writes (like EZ3's CartSetRAMPage)
4. **Save type significance**: The 'f'/'e'/'s' suffix might be ignored; firmware always reads from a fixed internal buffer
5. **Separate dispatch**: Dispatch 2 at 0x10C8 handles cmds differently than dispatch 1; the firmware may route save commands to the wrong handler

### Proposed test strategies

The tool `tools/save_probe.py` tests all these strategies:

| # | Strategy | Expected if correct |
|---|----------|-------------------|
| 1 | Original 0x14+0x02 with byte addr | All addresses return same data |
| 2 | 0x14+0x02 with word addr | Different addresses return different data |
| 3 | 0x14+0x02 with different save types | Different types return different data |
| 4 | Send unlock first, then 0x14+0x02 | Save data accessible |
| 5 | EZ3 register setup + 0x14+0x02 | Save data accessible |
| 6 | 0x01 (ROM read) at SRAM offset | Save data found at specific offset |
| 7 | 0x1A register reads at key addresses | Reveals register values for debugging |

## Cartridge Interface

The EZ-Flash II cartridge uses:
- NOR flash for ROM storage (up to 256 Mbit / 32 MB)
- Separate 2Mbit SRAM (256 KB) for save data + battery
- CPLD/FPGA for bank switching and interface logic

The 8051 firmware handles:
- USB bulk transfer ↔ cartridge bus translation
- Flash command sequences (sector erase, program, read)
- Save memory read/write
- Bank switching for large ROMs

## Firmware Analysis (tusbez.bin)

### Entry and Interrupts
- Entry: 0x0000 → LJMP 0x1202
- USB interrupt vector at 0x0043: LJMP to 0xCC61 (EZ-USB auto-vector)
- External interrupt 0 at 0x0003: LJMP 0x7211

### Command Dispatch

Two nearly-identical command dispatch routines:

**Dispatch 1** at 0x06AC (ROM read context):
- Entry: R4 = high addr, R5 = low addr, reg[0x02]:reg[0x03] = count
- Checks reg[0x02]:reg[0x03] for non-zero to continue
- Command byte from reg[0x30]
- Handles: 0x01 (read ROM), 0x02 (read with bank), 0x03 (write)
- Address stored in reg[0x11]:reg[0x12]

**Dispatch 2** at 0x10C8 (Save read context):
- Entry: R4 = high addr, R5 = low addr, reg[0x02]:reg[0x03] = count
- Also copies reg[0x07] into R1 (save type parameter)
- Command byte from reg[0x30]
- Handles: 0x01 (read), 0x02 (read with bank), 0x03 (write)
- Address stored in reg[0x17]:reg[0x18], save type in reg[0x07]

### Key Registers
- `0xFFF0` — EP4 OUT byte count / control
- `0xFFF1` — Status / ready flag
- `0xFFF3` — Cartridge bus control / mode select
- `reg[0x30]` — Command byte
- `reg[0x02]:reg[0x03]` — Count / size
- `reg[0x11]:reg[0x12]` — Address for dispatch 1
- `reg[0x17]:reg[0x18]` — Address for dispatch 2
- `reg[0x07]` — Save type parameter for dispatch 2

### Value encoding for 0xFFF3
From dispatch 2, the value written to 0xFFF3 is derived from:
```
R3 = (save_type & 7) * 2
```
Where save_type = 'f'(0x66) → R3=0x0C, 'e'(0x65) → R3=0x0A, 's'(0x73) → R3=0x06.

But for dispatch 1, the 0xFFF3 value is:
```
A = (reg[0x12] * 2) | 0x01
```
For ROM reads (cmd=0x01).

## Reverse Engineering TODO

- [x] Confirm endpoint layout (EP4 OUT / EP2 IN)
- [x] Confirm basic command format (cmd byte + params)
- [x] Implement ROM dumping (cmd 0x01)
- [x] Identify save commands (0x14 + 0x02)
- [ ] Fix save read (address ignored — determine correct address format)
- [ ] Capture USB traffic with USBPcap in XP VM
- [ ] Full disassembly of tusbez.bin
- [ ] Implement save write verify
- [ ] Implement ROM writing

## Summary

```
1. Device starts as Cypress bootloader (0547:2131)
2. PC uploads tusbez.bin via request 0xA0, starts 8051 CPU
3. Device re-enumerates as 0x0548:0x1005
4. PC sends cartridge commands over bulk EP4 OUT
5. Device replies over bulk EP2 IN
6. Firmware bridges USB ↔ EZ-Flash II cartridge bus
7. Cartridge needs specific CPLD unlock sequence for full access
```

## References

- Cypress EZ-USB AN2131 TRM (Technical Reference Manual)
- [Infineon EZ-USB FX2LP](https://www.infineon.com/cms/en/product/usb-solutions/ez-usb-fx2lp/)
- EZ-USB General Purpose Driver (ezusb.sys) documentation
- [asie's wiki — EZ Flash registers](https://wiki.asie.pl/doku.php?id=notes%3Aflashcart%3Aezflash)
- [EZ3 manage source](https://github.com/ez-flash/ez3manage)
- [bibanon — EZ Flash specs](https://wiki.bibanon.org/EZ_Flash/Specifications)
- libusb bulk transfer examples
