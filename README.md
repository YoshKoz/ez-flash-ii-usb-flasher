<div align="center">

# EZ-Writer II / EZ-Flash II USB Flasher

Modern open-source replacement for the Windows XP-era EZ-Writer flasher.  
**No kernel drivers. Just WinUSB + libusb.**

[![License: GPL v3](https://img.shields.io/badge/License-GPLv3-blue.svg)](LICENSE)
[![Platform](https://img.shields.io/badge/Platform-Windows%2010%2F11-lightgrey)]()
[![Rust](https://img.shields.io/badge/Rust-2024%20edition-orange)](https://www.rust-lang.org/)
[![GitHub last commit](https://img.shields.io/github/last-commit/YoshKoz/ezwriter-reverse)]()

</div>

---

## Quick Start

**1. Install driver** — [Download Zadig](https://zadig.akeo.ie/) → Run as Admin → Options → List All Devices → select `EZ-Writer II` → `WinUSB` → Install Driver.

**2. Build**

```bash
cd src\ezwriter-cli
cargo build --release
```

**3. Dump a cartridge**

```bash
ezwriter-cli firmware-download src\ezwriter-cli\tusbez.bin
ezwriter-cli dump myrom                # → myrom.gba
ezwriter-cli save-read 0 2048 --output myrom.sav
```

---

## Status

| Feature | Status |
|---------|--------|
| ROM dump | Done |
| Save read | Done |
| Cartridge header parse | Done |
| GUI (egui, 5 tabs) | Done |
| Save write | In progress |
| ROM write | In progress |

---

## GUI

```bash
cd src\ezwriter-gui
cargo build --release
.\target\release\ezwriter-gui.exe
```

Keep `loader_table1.bin` and `loader_table2.bin` next to the `.exe`.

Tabs: Status · Cart Info · Read ROM · Read Save · Write Save (soon)

---

<details>
<summary><b>Architecture</b> — how it works</summary>

```
PC (libusb) ←→ USB EP0/EP4/EP2 ←→ Cypress AN2131Q (8051) ←→ GBA cartridge
```

**Boot sequence:** Device boots as 0547:2131 (Cypress bootloader). Host sends `tusbez.bin` via vendor request 0xA0, starts CPU, device re-enumerates as 0548:1005 (active mode).

**Why not kernel driver?** Original drivers are unsigned x32-only (won't load on modern Windows) and just wrap USB bulk IOCTLs anyway. WinUSB + libusb does the same thing cleanly.

Full protocol reference: [docs/protocol_notes.md](docs/protocol_notes.md)
</details>

<details>
<summary><b>Project structure</b></summary>

```
ezwriter-reverse/
├── src/ezwriter-cli/      ─ Rust CLI (libusb, clap)
├── src/ezwriter-gui/      ─ Rust GUI (egui/eframe)
├── src/*.py               ─ RE/prototyping scripts
├── docs/                  ─ Protocol notes, driver analysis
├── driver/winusb-inf/     ─ WinUSB INF + install scripts
├── original/              ─ EZClient v3.26 (reference only)
├── original_backup/       ─ Extracted firmware + drivers
└── captures/              ─ USB packet dumps
```
</details>

---

## Safety

> Write ops can brick your cart if interrupted. Always dump ROM + save first.

| Operation | Risk |
|-----------|------|
| `list`, `info`, `cart-info`, `dump`, `save-read` | Safe (read-only) |
| `firmware-download` | Low (RAM only) |
| `write-save` | Medium |
| `write-rom`, `erase` | **High** — can brick |

See [SAFETY.md](SAFETY.md).

---

## Roadmap

- [x] Device detection, firmware download, ROM dump, save read
- [x] Cartridge header parse, GUI, WinUSB driver
- [ ] Save write, ROM write
- [ ] Speed optimization (pipelined reads)
- [ ] Write Save tab in GUI

---

## Legal

For homebrew, backing up saves from carts you own, and GBA development. **Not for piracy.**

---

## References

[Cypress AN2131](https://www.cypress.com/documentation/datasheets/ez-usb-fx2-usb-20-microcontroller-high-speed-usb-peripheral-controller) ·
[libusb](https://libusb.info/) ·
[WinUSB](https://docs.microsoft.com/en-us/windows-hardware/drivers/usbcon/winusb) ·
[Zadig](https://zadig.akeo.ie/) ·
[egui](https://github.com/emilk/egui)
