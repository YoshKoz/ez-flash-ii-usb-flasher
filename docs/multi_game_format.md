# Multi-Game Cartridge Format — Findings So Far

Context: question was whether `ez-flash-ii-usb-flasher` can write several `.gba`
files to one EZ-Flash II cart. The hardware supports it; the card in the test
unit already runs the EZ-Flash **loader/multi-game** firmware.

## What is on the test cartridge

Reading the cartridge header shows it is not a retail game but the loader image:

| Field | Value |
|-------|-------|
| Entry at 0x00 | `2E 00 00 EA` (ARM branch) |
| Title at 0xA0 | `EZLoader` |
| Code at 0xAC | `00 00 00 00` |
| Maker at 0xB0 | `30 31` (`"01"`) |
| Fixed at 0xB2 | `0x96` |
| ROM size byte 0x14 | `0x11` |

This matches `EZLoader2.bin` (37,836 bytes, ARM, "EZLoader" at 0xA0) from the
original driver package (`docs/original_driver_analysis.md`). The loader is
ARM/Thumb code; `0xC0` holds a second ARM branch (`06 00 00 EA`) and `0xE0`
onward is Thumb.

The loader region has no plain-text game-name list in the first 1 MB, so the
menu entries are either compressed or built into the loader's own data tables
rather than stored as ASCII.

## What a multi-game writer needs

The original EZ-Flash Writer writes three things:

1. The **loader/menu image** (EZLoader) at a fixed region near the start.
2. Each game at a ROM offset (the original uses size-aligned slots).
3. A **directory table** the loader reads to list games and launch them, plus
   the per-game save-slot mapping.

None of these is implemented here, and the directory format has not been fully
decoded yet.

## Blocker

Multi-game writing needs the ROM write path to address the whole cartridge, but
command `0x04` (see `docs/firmware_re_rom_write.md`) works within a single
64 KB window and has no bank byte. Until the bank-select sequence for writes is
confirmed, a writer cannot place games at offsets beyond the first window.

## Next steps

1. Confirm single-window ROM write on hardware (`docs/rom_write_test_plan.md`).
2. Find how the read path's bank byte (byte[3] of command `0x01`) maps to a
   write-side bank select — likely a command `0x19` register write or a
   dedicated bank command in the `0x04/0x05/0x06` family.
3. Disassemble the EZLoader ARM image (it is on the cartridge and in the
   original `EZLoader2.bin`) to locate the directory table and its entry format.
4. Only then design a multi-game writer that lays out loader + games + directory
   and writes each window.

This is a genuine project, not a quick change. The current tool remains a
backup/restore tool plus single-window ROM write.