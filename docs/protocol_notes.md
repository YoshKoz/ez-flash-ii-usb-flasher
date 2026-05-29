# Protocol Notes: EZ-Writer II USB Protocol

## Architecture Overview

```
┌──────────────────┐     USB Control EP0      ┌──────────────────────────┐
│  EZClient.exe    │ ◄──────────────────────► │  Cypress EZ-USB AN2131  │
│  (User mode)     │     Vendor Req 0xA0       │  (8051 firmware)         │
│                  │     (firmware download)    │                          │
│  DeviceIoControl │                           │  tusbez.bin              │
│  → \\.\Ezw-0     │     USB Bulk EP2 OUT      │  ┌──────────────────┐   │
│                  │ ◄──────────────────────►  │  │ GBA Cart Protocol│   │
│  libusb/Python   │     USB Bulk EP6 IN       │  └──────────────────┘   │
│  (modern impl)   │                           │         │                │
└──────────────────┘                           └─────────┼────────────────┘
                                                          │
                                                  ┌───────▼────────┐
                                                  │ EZ-Flash II    │
                                                  │ GBA Cartridge  │
                                                  └────────────────┘
```

## Phase 1: Firmware Download (Bootloader Mode)

When the EZ-Writer is plugged in, it appears as a Cypress EZ-USB AN2131 in
**ReNumeration** mode (VID 0x0547, PID 0x2131). No 8051 firmware is running
yet — the chip only responds to vendor control requests on EP0.

### Cypress Vendor Commands

All use bmRequestType = `0x40` (Host-to-Device, Vendor, Device recipient).

| bRequest | wValue | wIndex | Data | Purpose |
|----------|--------|--------|------|---------|
| `0xA0` | Addr[15:0] | Addr[31:16] | Firmware bytes | Write to internal RAM |
| `0xA3` | 0x0000 | varies | depends | Cypress upload (rarely used) |
| `0xA6` | varies | varies | depends | CPU control (legacy) |

### Firmware Download Sequence

```
1. HOLD CPU IN RESET:
   bmRequestType = 0x40
   bRequest      = 0xA0
   wValue        = 0xE600  (CPUCS register address low)
   wIndex        = 0x0000  (CPUCS register address high)
   Data          = [0x00]  (CPU reset value)

2. DOWNLOAD FIRMWARE (repeat for each chunk):
   bmRequestType = 0x40
   bRequest      = 0xA0
   wValue        = offset[15:0]
   wIndex        = offset[31:16]
   Data          = 64-byte chunk of firmware

3. START CPU / RE-ENUMERATE:
   bmRequestType = 0x40
   bRequest      = 0xA0
   wValue        = 0xE600  (CPUCS register address low)
   wIndex        = 0x0000
   Data          = [0x01]  (CPU start value → CPU boots,
                             re-enumerates as VID 0x0548:PID 0x1005)
```

### Firmware Binary: tusbez.bin

| Property | Value |
|----------|-------|
| Size | 5,584 bytes |
| Architecture | Intel 8051 (Cypress enhanced) |
| Entry point | 0x0000 → LJMP 0x1202 |
| RAM target | EZ-USB internal RAM from address 0x0000 |
| Reset vector | 0x0000 |
| Interrupt vectors | Standard 8051 (0x0003, 0x000B, 0x0013, 0x001B, 0x0023) |

The firmware configures:
- USB endpoints (EP0 control, EP2 bulk OUT, EP6 bulk IN for FX2)
- GBA cartridge communication pins
- Command dispatch loop

## Phase 2: Active Mode Communication

After re-enumeration, the device appears as:

- VID 0x0548, PID 0x1005
- Interface: Vendor-specific class
- Endpoints (hypothesized based on EZ-USB FX2 reference):

| Endpoint | Direction | Type | Max Packet | Purpose |
|----------|-----------|------|------------|---------|
| EP0 | Control | Control | 64/64 | Standard + vendor control |
| EP2 | OUT | Bulk | 512 | Host → Device (commands/data to cart) |
| EP6 | IN | Bulk | 512 | Device → Host (response/data from cart) |

### Command Packet Structure (Hypothesized)

Based on the driver IOCTL dispatch codes (0x2200xx range), the bulk transfer
protocol likely follows this pattern:

```
Host → Device (EP2 OUT):
┌───────┬───────┬───────┬───────┬──────────────────────────────────┐
│ Byte0 │ Byte1 │ Byte2 │ Byte3 │ Bytes 4..n                       │
├───────┼───────┼───────┼───────┼──────────────────────────────────┤
│ Cmd   │ Param0│ Param1│ Param2│ Optional payload / data          │
└───────┴───────┴───────┴───────┴──────────────────────────────────┘

Device → Host (EP6 IN):
┌───────┬───────┬──────────────────────────────────────────────────┐
│ Status│ LenLo │ LenHi │ Data...                                  │
├───────┼───────┼───────┼──────────────────────────────────────────┤
│ 0x00  │       │       │ Success payload                          │
│ 0xFF  │       │       │ Error code                               │
└───────┴───────┴───────┴──────────────────────────────────────────┘
```

### Hypothesized Commands (from EZClient strings + driver analysis)

| Cmd Byte | Name | Description |
|----------|------|-------------|
| `0x01` | INIT | Initialize cartridge interface |
| `0x02` | IDENTIFY | Detect cartridge presence and type |
| `0x03` | READ_ID | Read cartridge ID bytes |
| `0x04` | GET_STATUS | Get device status (4 DWORDS, see Status:[1]0x%x...) |
| `0x10` | READ_SECTOR | Read ROM/save sector |
| `0x11` | READ_SAVE | Read save data |
| `0x20` | WRITE_SECTOR | Write ROM sector |
| `0x21` | WRITE_SAVE | Write save data |
| `0x30` | ERASE_SECTOR | Erase sector |
| `0x40` | VERIFY | Verify written data |

This is preliminary — needs confirmation via USBPcap capture or firmware RE.

### Status Format

From EZClient.exe string: `"Status:[1]0x%x [2]0x%x [3]0x%x [4]0x%x"`

Likely returns 4 DWORDs:
```
[1] = Overall status / error code
[2] = Cartridge type (Flash/EEPROM/SRAM)
[3] = ROM size or sector number
[4] = Operation progress / bytes remaining
```

### Save Types (from EZClient strings)

- `FLASH_TYPE` — Flash-based save (128KB, etc.)
- `EEPROM_TYPE` — EEPROM-based save (512 bytes, 8KB, etc.)
- `SRAM_TYPE` — SRAM-based save (32KB, 64KB, etc.)

## EZ-Flash II Cartridge Interface

The EZ-Flash II cartridge uses:
- NOR flash for ROM storage (up to 256 Mbit / 32 MB)
- Separate SRAM/EEPROM for save data
- FPGA for bank switching

The 8051 firmware (tusbez.bin) handles:
- USB bulk transfer ↔ cartridge bus translation
- Flash command sequences (sector erase, program, read)
- Save memory read/write
- Bank switching for large ROMs

## Next Steps for Protocol RE

1. **USBPcap capture**: Install USBPcap + Wireshark on host, capture traffic while
   EZClient (in XP VM) performs each operation. Map captured URBs to IOCTL calls.

2. **8051 firmware disassembly**: Use IDA Pro or radare2/Ghidra with 8051 plugin
   to reverse engineer tusbez.bin. Look for:
   - USB interrupt handler setup
   - Command dispatch table
   - Bulk endpoint buffer management
   - GBA cartridge GPIO signaling

3. **I/O patterns**: EZ-USB FX2 has specific register addresses for endpoint
   buffers (EP2FIFOBUF, EP6FIFOBUF). Firmware RE should reveal command codes.

4. **Known similar projects**: The EZ-Flash series uses similar protocols across
   generations. Research gba-link-cable, OpenFlashcart, and GBARipper projects.

## References

- Cypress EZ-USB FX2 TRM (Technical Reference Manual)
- AN2131 Datasheet
- EZ-USB General Purpose Driver (ezusb.sys) documentation
- libusb bulk transfer examples
