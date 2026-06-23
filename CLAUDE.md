# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

Two independent Rust crates for the EZ-Writer II / EZ-Flash II GBA cartridge flasher. Both live under `src/` with no shared workspace — build each separately.

| Crate | Path | Purpose |
|-------|------|---------|
| `ezwriter-cli` | `src/ezwriter-cli/` | CLI tool, all protocol logic |
| `ezwriter-gui` | `src/ezwriter-gui/` | egui GUI wrapping same protocol |

## Build & run

```console
# CLI
cd src/ezwriter-cli
cargo build --release
.\target\release\ezwriter-cli.exe list

# GUI
cd src/ezwriter-gui
cargo build --release
.\target\release\ezwriter-gui.exe
```

No workspace — run `cargo` from inside each crate dir. No tests exist yet.

## Architecture

### CLI (`src/ezwriter-cli/src/main.rs`)
Single-file. All USB logic, subcommands, and protocol state live here. Uses `rusb` (vendored libusb) directly with `clap` derive for CLI.

Key subcommands: `list`, `info`, `firmware-download`, `init-exact`, `cart-info`, `cart-read`, `dump`, `save-read`, `save-write`, `write-rom`, `erase`.

### GUI (`src/ezwriter-gui/src/`)
- `device.rs` — all USB/protocol logic (mirrors CLI logic), `GAME_DB` static lookup table
- `app.rs` — egui app, 5 tabs (Status / Cart Info / Read ROM / Read Save / Write Save), background thread via `mpsc` for non-blocking USB ops
- `main.rs` — entry point

GUI runs device ops on a worker thread and receives results via channel to keep UI responsive.

### USB protocol
Two-phase operation:
1. **Bootloader** (`0547:2131`): upload `tusbez.bin` firmware over EP0 vendor request `0xA0`. Toggle CPUCS at `0x7F92` for reset/run. Device then re-enumerates.
2. **Active** (`0548:1005`): bulk EP4 OUT for commands, EP2 IN for data. 24-bit bank addressing via byte[3] of the 4-byte command packet for >128KB ROM access.

Firmware (`tusbez.bin`, `loader_table1.bin`, `loader_table2.bin`) is NOT in the repo — user must supply originals. The GUI expects loader files in CWD.

## Key constants (both crates)

| Constant | Value | Meaning |
|----------|-------|---------|
| `BOOTLOADER_VID/PID` | `0547:2131` | Cypress default before firmware |
| `EZWRITER_VID/PID` | `0548:1005` | After firmware upload |
| `CPUCS_ADDR` | `0x7F92` | AN2131 CPU control/status register |
| `CMD_EP` | `0x04` | Bulk OUT for commands |
| `DATA_EP` | `0x82` | Bulk IN for data |

## Windows driver requirement

WinUSB must be installed via Zadig before any USB communication works. The `driver/winusb-inf/` directory has an optional INF approach. Without this, all `rusb` calls fail silently or error with access denied.

## Safety

`write-rom` and `erase` are experimental and can brick cartridges. See `SAFETY.md`. Read-only ops (`dump`, `save-read`, `cart-info`) are safe.

## Docs

- `docs/protocol_notes.md` — full USB protocol reverse-engineering notes
- `docs/original_driver_analysis.md` — Windows XP driver analysis
- `src/*.py` — historical reverse-engineering probes, not production code
