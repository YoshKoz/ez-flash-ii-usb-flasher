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

## What the 32 MB backup shows

`map_rom.py` on the full backup reveals multiple game blocks separated by blank
space — the expected multi-game slot layout:

```
0x0000000-0x1070000  DATA    17,235,968 bytes   (loader + first content)
0x1070000-0x1080000  BLANK       65,536 bytes
0x1080000-0x1520000  DATA     4,849,664 bytes   (second block)
0x1520000-0x1B50000  BLANK    6,488,064 bytes
0x1B50000-0x1D10000  DATA     1,835,008 bytes
0x1D10000-0x1E40000  BLANK    1,245,184 bytes
0x1E40000-0x1E60000  DATA       131,072 bytes
0x1E60000-0x1E80000  BLANK      131,072 bytes
0x1E80000-0x1FD0000  DATA     1,376,256 bytes
0x1FD0000-0x2000000  BLANK      196,608 bytes
```

Notes:

- Blocks are **not** preceded by a standard GBA header. `0x1080000` starts
  `12 00 06 22`, not `2E 00 00 EA`, and has no valid title/logo. EZ-Flash stores
  the payload without the 512-byte style header a raw ROM would carry, so a
  writer must know the slot format rather than just concatenating `.gba` files.
- `0x1E80000` is a linear byte counter (`00 01 02 03 ...`) — a test pattern left
  on the cart from earlier experimentation, not a game.
- 2072 of 8192 4 KB pages are all `0xFF`: most of the 32 MB part is blank.

This confirms the cart is a **32 MB, multi-game-capable, writeable unit**, and
that the multi-game layout is real but its directory format is not a plain GBA
header table.

## Blocker for writing, independent of the directory

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