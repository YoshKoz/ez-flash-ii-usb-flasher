# EZ-Writer II Reverse Engineering — Session Save

## Date
2026-05-28

## Hardware

| Item | Value |
|------|-------|
| Device | EZ-Writer II / CNFlash / EZ-Flash II USB Writer |
| Bootloader | VID 0547 PID 2131 (Cypress EZ-USB AN2131Q) |
| Active mode | VID 0548 PID 1005 "EZ-Writer fujitsu" |
| Manufacturer | USTC |
| Chipset | Cypress EZ-USB AN2131 (FX family, 8051 core) |
| USB speed | Full Speed 12 Mbps |
| Endpoints | EP1-EP7 OUT + EP1-EP7 IN (bulk, 64 bytes) |

## Project Location
`C:\Users\yoshi\ezwriter-reverse\`

## Project Structure

```
ezwriter-reverse/
├── README.md                    — Project overview + setup guide
├── SAFETY.md                    — Risk levels, recovery
├── analyze_driver.py            — IOCTL analysis script
├── disasm_fwloader.py           — Capstone x86 disassembler for ezwinit.sys
├── disasm_full.py               — Full section disassembly
├── disasm_ezwinit.py            — EZWINIT analysis
├── docs/
│   ├── device_inventory.md      — Full USB descriptor analysis
│   ├── original_driver_analysis.md — INF analysis, IOCTL mapping, PDB paths
│   └── protocol_notes.md        — Protocol architecture, EZ-USB commands
├── captures/
│   └── active_0548_1005_lsusb.txt — Linux lsusb dump of active device
├── original_backup/
│   ├── (all original driver .sys and .bin files)
│   ├── ezwinit.sys              — Initialization kernel driver
│   ├── ezwriter.sys             — Main kernel driver
│   ├── tusbez.bin               — EZ-Writer3 firmware (TI TUSB3210, NOT FOR THIS DEVICE)
│   ├── an2131_firmware.bin      — Extracted AN2131 firmware from .data section
│   ├── an2131_fw_driver.bin     — 2928 bytes, first extraction attempt
│   ├── an2131_fw_v2.bin         — 6975 bytes, second extraction
│   ├── active_fw_full.bin       — 6441 bytes, reconstructed from both chunk tables
│   ├── active_fw_full.d52       — Full 8051 disassembly (d52 format)
│   └── ezwinit_reconstructed.bin — Full firmware from chunk table reconstruction
├── original/
│   └── EZ Client/               — Extracted from ezc326.7z (EZClient v3.26)
├── src/
│   ├── poc_identify.py          — Python proof-of-concept device detection
│   ├── probe_*                  — Various WSL USB probing scripts
│   ├── read_cart*.py            — Cartridge reading test scripts
│   ├── find_cmd_format.py       — EZClient protocol RE
│   └── ezwriter-cli/            — RUST CLI (mature)
│       ├── Cargo.toml
│       ├── tusbez.bin
│       ├── loader_table1.bin    — Firmware init chunk table 1 (59 chunks)
│       ├── loader_table2.bin    — Firmware init chunk table 2 (403 chunks)
│       ├── an2131_fw_v2.bin
│       └── src/main.rs          — ~900 lines, all protocol + CLI commands
│   └── ezwriter-gui/            — RUST GUI (latest)
│       ├── Cargo.toml
│       ├── loader_table1.bin
│       ├── loader_table2.bin
│       └── src/
│           ├── main.rs          — eframe entry point
│           ├── app.rs           — egui tabs (Status, Cart Info, Read ROM, Read Save, Write Save)
│           └── device.rs        — Protocol code ported from CLI
└── driver/winusb-inf/
    ├── ezwriter-winusb.inf      — WinUSB INF for both VID/PID
    ├── install.bat
    └── uninstall.bat
```

## Protocol Discoveries

### Device Init (Firmware Download)
- ezwinit.sys contains firmware in two chunk tables (RVA 0x12B58 and 0x108A0)
- Exact sequence: CPUCS=1, CPUCS=1, write table1, CPUCS=0, CPUCS=1, write table2, CPUCS=1, CPUCS=0
- CPUCS register at 0x7F92 (AN2131)
- Vendor request 0xA0 for RAM writes
- Image base: 0x10000

### Cartridge Communication
- Write 4-byte packets to EP4 OUT (0x04)
- Read 64-byte chunks from EP2 IN (0x82)
- EP2 IN double-buffer alternates (A/B), so need to discard every other read
- Fixed by sending 2 commands per full header read

### Command Format (from EZClient.exe RE)
```
[cmd_byte, addr0, addr1, addr2, ?]  → 4-6 bytes
```
- cmd=0x01: ROM read (address in 2-byte units)
- cmd=0x02: save type operation (with suffix byte)
- cmd=0x14: select save type (suffix byte: 'f'=FLASH, 'e'=EEPROM, 'g'=?, 'h'=?)
- Suffix: 0x66('f')=FLASH, 0x65('e')=EEPROM, 0x67=?, 0x68=?

### IOCTL Codes (from ezwriter.sys)
- 0x00222051 = CTRL_WRITE (used 39 times by EZClient)
- 0x00222035 = BULK_WRITE (hypothesized)
- 0x00222054 = BULK_READ (hypothesized)
- 0x00220007 = GET_STATUS
- 0x0022206D, 0x00222074 = vendor commands

### Key EZClient String Offsets (in .exe)
- Status: 0x0B16B0
- NO Cart: 0x0B10C0
- BackFile: 0x0B0554
- BackSign: 0x0B108C
- FLASH_TYPE: 0x0B2640
- EEPROM_TYPE: 0x0B264C
- SRAM_TYPE: 0x0B2658
- WriteSaver: 0x0B08AD

## Current Capabilities

| Feature | Status | Notes |
|---------|--------|-------|
| Device detection | ✅ | Windows native, libusb/WinUSB |
| Firmware init | ✅ | From ezwinit.sys chunk tables |
| Cartridge ROM read | ✅ | Sequential, any address, auto-reset |
| Cartridge header parse | ✅ | Include Nintendo logo decode |
| Save read (cmd 0x02) | ✅ | FLASH type tested, need EEPROM/SRAM |
| Save write | ❌ | Protocol not reversed yet |
| ROM write | ❌ | Protocol not reversed yet (erase + program) |
| WSL dependency | ❌ removed | All Windows native now |
| GUI | ✅ | Rust egui with 5 tabs |
| Game DB | ✅ | 40+ GBA titles with save types |

## Driver Analysis Summary

### ezwinit.sys (Init driver)
- PDB: `D:\EZ_Writer3.0\ezloader\LIB\i386\ezwinit.pdb`
- Source project: EZ_Writer3.0\ezloader
- PE32 kernel driver, image base 0x10000
- .text: 1318 bytes, .data: 10208 bytes (contains firmware)
- Firmware at .data+0x2C3 (file offset 0xB63)

### ezwriter.sys (Main driver)
- PDB: `D:\EzFlash\pfw_usbfx2\ezusbdrv\LIB\i386\ezusb.pdb`
- Source project: EzFlash\pfw_usbfx2 ("pico flash writer USB FX2")
- Same driver used for ezwrite2.sys and ezwrit3.sys
- 7 IOCTL codes, thin USB bulk wrapper
- Device paths: \Device\Ezw-0 (\\.\Ezw-0)

### EZ-Writer II (this device) boot flow:
1. Plug in → VID 0547 PID 2131 (EZ-USB bootloader)
2. ezwinit.sys sends firmware via vendor 0xA0 writes
3. Device re-enumerates as VID 0548 PID 1005
4. ezwriter.sys handles bulk IOCTLs

## Built Binaries

```
src/ezwriter-cli/target/release/ezwriter-cli.exe  — 825KB CLI
  Commands: list, info, init-exact, cart-read, save-read, 
            reset-cart, probe, ram-read, ram-write
  
src/ezwriter-gui/target/release/ezwriter-gui.exe  — 4.9MB GUI
  Tabs: Status, Cart Info, Read ROM, Read Save, Write Save
  Features: Nintendo logo render, game DB lookup, progress bars
  Copy loader_table*.bin alongside the .exe
```

## Next Work

1. **Save write protocol** — Reverse EZClient.exe save write path
2. **ROM write** — Erase + program NOR flash (requires JEDEC commands)
3. **Game DB** — Expand to 500+ titles with save types
4. **Write Save UI** — Complete the Write Save tab
5. **Full ROM dump** — 32MB takes ~30 minutes, add progress/time estimate

## Save File Format
- ROM: `.gba` (raw binary, 8MB–32MB)
- Save: `.sav` (raw binary, 512B–256KB depending on chip type)
- Save type per game: SRAM 32K, SRAM 64K, EEPROM 512, EEPROM 8K, FLASH 64K, FLASH 128K
