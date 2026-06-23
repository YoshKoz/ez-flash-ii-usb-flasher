# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

See parent `CLAUDE.md` at `C:\Development\ez-flash-ii-usb-flasher\CLAUDE.md` for full architecture, USB protocol, and key constants. This file covers crate-specific detail.

## Build & run

```console
cargo build --release
.\target\release\ezwriter-cli.exe list
```

No workspace — must run `cargo` from this dir. No tests.

Firmware files (`tusbez.bin`, `loader_table1.bin`, `loader_table2.bin`) are present in this crate root (CWD when running). `firmware-download` and `init-exact` require them.

## Subcommand stability

| Stable (safe, read-only) | Experimental / destructive |
|---|---|
| `list`, `info`, `cart-info` | `write-rom`, `erase` (can brick cart) |
| `dump`, `save-read` | `save-write`, `fpga-write`, `ram-write` |
| `firmware-download`, `init-exact` | `probe-eeprom`, `bulk-test`, `passive-read` |

## Single-file structure (`src/main.rs`)

All code in one file. Sections (delimited by `// ---` comments):
1. **Constants** — VIDs/PIDs, EP addresses, timeouts
2. **CLI** — `clap` derive enums (`Commands`, subcommand args)
3. **Helpers** — `print_hex`, `parse_u8_auto`, save validation (`validate_save_dump`, `gen3_save_signature_count`)
4. **USB open/init** — `open_bootloader`, `open_active`, `upload_firmware_chunk`
5. **Protocol ops** — one function per subcommand: `do_dump`, `do_save_read`, `do_cart_info`, etc.
6. **`main`** — dispatches `Commands` enum to protocol ops

## Key implementation patterns

**24-bit bank addressing** (`dump`, `cart-read --byte3-bank`): byte[3] of the 4-byte EP4 OUT command packet carries the upper bank byte. Bank 0 = addresses 0–0x3FFFF, bank 1 = 0x40000–0x7FFFF, etc.

**EP0 control path for dump title/header**: `dump` uses vendor request `0x01` over EP0 for the first 0xA0 bytes (GBA header) to get correct bytes, then switches to bulk EP4/EP2 for the body. This was necessary because stale EP2 IN data corrupted the header bytes when starting bulk reads at offset 0.

**Save validation**: `validate_save_dump` rejects data that starts with the known ROM stub pattern (firmware bug: endpoint returns stale ROM data instead of save). For FLASH saves ≥128KB it also checks for ≥14 Gen 3 section signatures (`0x25 0x20 0x01 0x08`).

**`save-read` inner cmd**: `loader_table2.bin` patches the firmware so cmd `0x02` reads inner byte: `0x66` = FLASH/SRAM handler, `0x65` = EEPROM handler. Pass `--inner-cmd` to override.

**`init-exact`** replays the exact chunk table writes extracted from `ezwinit.sys` (Windows XP driver). Prefer this over `firmware-download` when driver-accurate init is needed.
