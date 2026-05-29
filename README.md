# EZ-Writer II / EZ-Flash II USB Flasher

Reverse-engineered, modern replacement for the ancient Windows XP-era EZ-Writer
flasher. Runs on Windows 11 x64 with **no kernel driver hacks**.

Uses WinUSB (Microsoft inbox driver) + libusb.

## Quick Start

### 1. Install Driver

**Easiest:** Use [Zadig](https://zadig.akeo.ie/) (replaces the unsigned INF
problem):

1. Download and run Zadig as Administrator
2. Options → List All Devices
3. Select `EZ-Writer II` or `USB\VID_0547&PID_2131`
4. Driver: `WinUSB (Microsoft)`
5. Install Driver
6. Repeat for `USB\VID_0548&PID_1005` if it appears

**Manual (test signing):**
```powershell
# Enable test signing
bcdedit /set testsigning on
# Reboot

# Right-click driver\winusb-inf\ezwriter-winusb.inf → Install

# Disable after done
bcdedit /set testsigning off
# Reboot
```

### 2. Build CLI

```bash
cd src\ezwriter-cli
cargo build --release
target\release\ezwriter-cli --help
```

### 3. First Run

```bash
# Check device is detected
ezwriter-cli list

# If in bootloader mode (VID 0547:PID 2131), download firmware
ezwriter-cli firmware-download original\EZClient\USB_Drivers\tusbez.bin

# Wait 3-5 seconds for re-enumeration, then:
ezwriter-cli list
```

## Commands

| Command | Description |
|---------|-------------|
| `list` | Show all EZ-Writer USB devices |
| `info` | Show device descriptors + configuration |
| `firmware-download <file>` | Download 8051 firmware to EZ-USB |
| `cart-info` | Detect cartridge and show info |
| `dump-rom <file>` | Read cartridge ROM to file (non-destructive) |
| `read-save <file>` | Read save data to file (non-destructive) |
| `write-rom <file>` | Write ROM to cartridge (requires confirmation) |
| `write-save <file>` | Write save data (requires confirmation) |
| `erase-cart` | Erase cartridge (requires confirmation) |

## Project Structure

```
ezwriter-reverse/
├── README.md              ← this file
├── SAFETY.md              ← risk levels, recovery, pre-flight checklist
├── docs/
│   ├── device_inventory.md
│   ├── original_driver_analysis.md
│   └── protocol_notes.md
├── src/
│   └── ezwriter-cli/      ← Rust CLI tool (libusb)
│       └── src/main.rs
├── driver/
│   └── winusb-inf/        ← WinUSB INF + install/uninstall scripts
│       ├── ezwriter-winusb.inf
│       ├── install.bat
│       └── uninstall.bat
├── captures/              ← USBPcap traces (future)
├── original_backup/       ← Backup of original driver files
├── original/
│   └── EZ Client/         ← Extracted from ezc326.7z
└── deps/                  ← libusb Windows binaries
```

## How It Works

### Architecture

The EZ-Writer is built around **Cypress EZ-USB AN2131Q** (FX2 family):

```
[Host] --USB--> [Cypress EZ-USB AN2131Q] --cart bus--> [EZ-Flash II Cartridge]
                         |
                   8051 firmware
                  (tusbez.bin)
```

**Boot sequence:**
1. Device plugs in as VID 0547:PID 2131 (Cypress bootloader mode)
2. Host downloads 8051 firmware (tusbez.bin) into EZ-USB RAM via vendor
   control request 0xA0
3. Host starts CPU → device re-enumerates as VID 0548:PID 1005
4. Host communicates via bulk endpoints for cartridge read/write

### Why Not Kernel Driver?

The original drivers (ezwinit.sys, ezwriter.sys) are:
- **Unsigned x32 drivers** — won't load on modern x64 Windows without hacks
- **Proprietary** — no source available
- **Thin wrappers** — just expose USB bulk IOCTLs to user-mode

This project replaces both with WinUSB (Microsoft signed) + libusb.

### Protocol

- EZ-USB firmware download: Standard Cypress vendor command 0xA0
- Cartridge communication: Bulk transfers (EP2 OUT / EP6 IN hypothesis)
- Cartridge commands embedded in 8051 firmware (tusbez.bin)

## Driver Signing Options

| Method | Pros | Cons |
|--------|------|------|
| **Zadig + WinUSB** | No watermark, easy | Requires Zadig download |
| **Test signing** | No extra tools | "Test Mode" desktop watermark |
| **Microsoft Attestation** | Clean, distributable | Requires HLK setup, costs money |

## Legal

This tool is designed for:
- Running your own homebrew on original hardware
- Backing up game saves from cartridges you own
- Developing and testing GBA software

Do not use for piracy.

## References

- [Cypress EZ-USB AN2131 Datasheet](https://www.cypress.com/documentation/datasheets/ez-usb-fx2-usb-20-microcontroller-high-speed-usb-peripheral-controller)
- [libusb 1.0](https://libusb.info/)
- [WinUSB](https://docs.microsoft.com/en-us/windows-hardware/drivers/usbcon/winusb)
- [Zadig](https://zadig.akeo.ie/)
- [USB Device Tree Viewer](https://www.uwe-sieber.de/usbtreeview_e.html)
