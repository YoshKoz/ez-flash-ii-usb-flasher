<div align="center">

# EZ-Writer II / EZ-Flash II USB Flasher

**Modern, open-source replacement for the Windows XP-era EZ-Writer flasher.**
**No kernel driver hacks — just WinUSB + libusb.**

[![License: GPL v3](https://img.shields.io/badge/License-GPLv3-blue.svg)](LICENSE)
[![Platform](https://img.shields.io/badge/Platform-Windows%2010%2F11-lightgrey)](https://github.com/YoshKoz/ezwriter-reverse)
[![Rust](https://img.shields.io/badge/Rust-2024-edition-orange)](https://www.rust-lang.org/)
[![GitHub last commit](https://img.shields.io/github/last-commit/YoshKoz/ezwriter-reverse)](https://github.com/YoshKoz/ezwriter-reverse/commits/master)

</div>

---

## Quick Start

### 1. Install Driver (Zadig — 30 seconds)

1. Download [Zadig](https://zadig.akeo.ie/) and run as **Administrator**
2. **Options → List All Devices**
3. Select `EZ-Writer II` (or `USB\VID_0547&PID_2131`)
4. Choose `WinUSB (Microsoft)` driver
5. **Install Driver** (also repeat for `USB\VID_0548&PID_1005` if it appears)

### 2. Build

```bash
cd src\ezwriter-cli
cargo build --release
target\release\ezwriter-cli --help
```

### 3. Dump a Cartridge

```bash
# List connected devices
ezwriter-cli list

# If it shows bootloader mode (0547:2131), download firmware first:
ezwriter-cli firmware-download src\ezwriter-cli\tusbez.bin
# Wait 3-5 seconds for re-enumeration, then:

# Dump ROM → mycart.gba (auto-appends .gba)
ezwriter-cli dump mycart

# Dump save data → mycart.sav
ezwriter-cli save-read 0 2048 --output mycart.sav

# View cartridge info
ezwriter-cli cart-info
```

---

## Features

| Feature | Status | Description |
|---------|--------|-------------|
| Device detection | ✅ | USB enumeration, works with WinUSB/libusb |
| Firmware download | ✅ | Loads 8051 firmware to EZ-USB RAM via Cypress 0xA0 |
| ROM dump | ✅ | Full 32MB dump with auto-banking via byte[3] |
| Save read | ✅ | FLASH 128K, EEPROM, SRAM (selectable) |
| Cartridge header parse | ✅ | Title, game code, maker, Nintendo logo verify |
| Save write | 🚧 | Protocol understood, not yet implemented |
| ROM write | 🚧 | Erase + program NOR flash, not yet implemented |
| GUI | ✅ | egui-based desktop app with 5 tabs |
| Driver | ✅ | WinUSB INF + install script (no kernel driver) |
| Game DB | ✅ | 40+ GBA titles with save type lookup |

---

## All Commands

| Command | Description |
|---------|-------------|
| `list` | Show all EZ-Writer USB devices |
| `info` | Show USB descriptors and configuration |
| `firmware-download <file>` | Download 8051 firmware to EZ-USB RAM |
| `init-exact <table1> <table2>` | Init AN2131 using chunk tables from ezwinit.sys |
| `reset-cart` | Reset cartridge flash to array read mode |
| `cart-info` | Detect cartridge, read and parse GBA header |
| `dump <output>` | Full 32MB ROM dump to file (auto-banking via byte[3]) |
| `save-read [addr] [count]` | Read save data from cartridge |
| `cart-read <addr> <count>` | Low-level bulk read from cartridge |
| `fpga-write` | Write FPGA register (test direct access) |
| `ep0-vendor-read` | EP0 vendor command 0x01 test |
| `reset` | USB bus reset |
| `probe` | Send vendor control request |
| `ram-read <addr>` | Read EZ-USB internal RAM |
| `ram-write <addr> <data>` | Write to EZ-USB internal RAM |
| `bulk-test` | Send/recv bulk data for endpoint probing |

---

## GUI

The project includes a native Windows GUI built with [egui](https://github.com/emilk/egui):

```bash
cd src\ezwriter-gui
cargo build --release
target\release\ezwriter-gui.exe
```

Tabs: **Status** (device detection + firmware init) · **Cart Info** (header + logo render) · **Read ROM** (streaming dump with progress) · **Read Save** · **Write Save** (coming soon)

Make sure `loader_table1.bin` and `loader_table2.bin` are next to the `.exe`.

---

## Architecture

```
┌──────────────────┐     USB Control EP0      ┌──────────────────────────────┐
│  ezwriter-cli    │ ◄──────────────────────►  │  Cypress EZ-USB AN2131Q     │
│  (libusb)        │     Vendor Req 0xA0       │  (8051 firmware, tusbez.bin)│
│                  │     (firmware download)    │                              │
│                  │                           │  ┌──────────────────────────┐│
│  cargo build     │     USB Bulk EP4 OUT      │  │ GBA Cart Protocol        ││
│  Windows 10/11   │ ◄──────────────────────►  │  │ (cmd 0x01=ROM, 0x02=save)││
│                  │     USB Bulk EP2 IN       │  └──────────────────────────┘│
└──────────────────┘                           └──────────────┬───────────────┘
                                                              │
                                                   ┌──────────▼──────────┐
                                                   │  GBA Cartridge       │
                                                   │  (EZ-Flash II / any) │
                                                   └─────────────────────┘
```

### Boot Sequence

1. Device plugs in → **VID 0547:PID 2131** (Cypress bootloader mode, no firmware)
2. Host sends 8051 firmware (`tusbez.bin`) to EZ-USB internal RAM via vendor request `0xA0`
3. Host writes `CPUCS` register (address `0x7F92`) to start the CPU
4. Device re-enumerates → **VID 0548:PID 1005** (EZ-Writer active mode)
5. Host communicates via bulk endpoints for all cartridge operations

### ROM Dump Protocol

4-byte command written to **EP4 OUT**:
```
[cmd, addr[7:0], addr[15:8], bank]
```
- `cmd = 0x01` (ROM read)
- `addr = target address / 2` (16-bit word address)
- `bank = word_addr >> 16` (upper bits select 128KB bank)

64 bytes read back from **EP2 IN** per command.

### Save Protocol

```
1. Select save type: [0x14, suffix, 0x00] → EP4 OUT
   suffix: 'f' = FLASH, 'e' = EEPROM
2. Read each chunk:  [0x02, addr0, addr1, addr2, suffix] → EP4 OUT → 64 bytes ← EP2 IN
```

### Why Not Kernel Driver?

The original drivers (`ezwinit.sys`, `ezwriter.sys`) are:
- **Unsigned x32 drivers** — won't load on modern x64 Windows without test signing
- **Proprietary** — no source code available
- **Thin wrappers** — just expose USB bulk IOCTLs to user-mode

This project replaces both with **WinUSB** (Microsoft-signed inbox driver) + **libusb**.

---

## Project Structure

```
ezwriter-reverse/
├── README.md
├── LICENSE                     ─ GPLv3
├── SAFETY.md                   ─ Risk levels, recovery, pre-flight checklist
├── src/
│   ├── ezwriter-cli/           ─ Rust CLI tool (libusb, clap)
│   │   ├── src/main.rs         ─ ~1200 lines, all protocol + commands
│   │   └── tusbez.bin          ─ 8051 firmware (needed for init)
│   ├── ezwriter-gui/           ─ Rust GUI (egui/eframe)
│   │   └── src/
│   │       ├── main.rs         ─ Entry point
│   │       ├── app.rs          ─ 5-tab egui interface
│   │       └── device.rs       ─ Protocol + USB + game DB
│   ├── *.py                    ─ 22 Python RE/prototyping scripts
│   └── find_cmd_format.py      ─ EZClient.exe protocol analysis
├── docs/
│   ├── protocol_notes.md       ─ Protocol architecture and commands
│   ├── original_driver_analysis.md  ─ INF/IOCTL/firmware analysis
│   └── device_inventory.md     ─ Full USB descriptor inventory
├── driver/
│   └── winusb-inf/             ─ WinUSB INF + install/uninstall scripts
├── original/                   ─ Original EZClient v3.26 (reference only)
├── original_backup/            ─ Extracted firmware and driver files
├── captures/                   ─ USB packet capture dumps
└── deps/                       ─ libusb Windows binaries
```

---

## Driver Signing Options

| Method | Pros | Cons |
|--------|------|------|
| **Zadig + WinUSB** | No watermark, easy, no reboot | Requires Zadig download |
| **Test signing** | No extra tools | "Test Mode" desktop watermark |
| **Microsoft Attestation** | Clean, distributable | Requires HLK, costs money |

> [!TIP]
> Zadig is the recommended approach. It creates signed driver catalogs automatically with no desktop watermark.

---

## Safety

> [!WARNING]
> **Write operations can brick your cartridge if interrupted.** Always dump ROM + save before writing.

| Operation | Risk | Notes |
|-----------|------|-------|
| `list`, `info` | ✅ None | Read-only descriptor enumeration |
| `firmware-download` | ✅ Low | RAM firmware, resets on power cycle |
| `cart-info`, `dump` | ✅ Low | Read-only cartridge access |
| `save-read` | ✅ Low | Read-only save data |
| `write-save` | ⚠️ Medium | Can corrupt save data |
| `write-rom` | 🔴 HIGH | Can brick cart if interrupted |
| `erase` | 🔴 HIGH | Destructive — wipes cartridge |

See [SAFETY.md](SAFETY.md) for full details and recovery procedures.

---

## Roadmap

- [x] Device detection + firmware download
- [x] ROM dump (full 32MB with auto-banking)
- [x] Save read (FLASH, EEPROM)
- [x] Cartridge header parsing + Nintendo logo verification
- [x] GUI with 5 tabs
- [ ] Save write protocol (reverse EZClient.exe path)
- [ ] ROM write (erase + program NOR flash via JEDEC commands)
- [ ] Game DB expansion (500+ titles)
- [ ] Write Save tab in GUI
- [ ] Speed optimization (pipelined reads)

---

## Legal

This tool is designed for:
- Running your own homebrew on original hardware
- Backing up game saves from cartridges you own
- Developing and testing GBA software

**Do not use for piracy.**

---

## References

- [Cypress EZ-USB AN2131 Datasheet](https://www.cypress.com/documentation/datasheets/ez-usb-fx2-usb-20-microcontroller-high-speed-usb-peripheral-controller)
- [libusb 1.0](https://libusb.info/)
- [WinUSB](https://docs.microsoft.com/en-us/windows-hardware/drivers/usbcon/winusb)
- [Zadig](https://zadig.akeo.ie/)
- [USB Device Tree Viewer](https://www.uwe-sieber.de/usbtreeview_e.html)
- [egui](https://github.com/emilk/egui) — Immediate-mode GUI library

---

## Contributing

Contributions welcome! Open an issue or PR.

1. Fork the repo
2. Create your feature branch (`git checkout -b feature/amazing`)
3. Commit your changes
4. Push to the branch
5. Open a Pull Request

<sup>Made with ❤️ by reverse-engineering the original EZ-Writer driver stack.</sup>
