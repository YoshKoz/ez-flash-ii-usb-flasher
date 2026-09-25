# Multi-Game Cartridge Format — Findings

Context: can `ez-flash-ii-usb-flasher` write several `.gba` files to one
EZ-Flash II cart? The hardware supports it; the test unit already runs the
EZ-Flash loader/multi-game firmware.

## What is on the test cartridge

The cartridge header is not a retail game — it is the loader image:

| Field | Value |
|-------|-------|
| Entry at 0x00 | `EA 00 00 2E` (`b 0x80000c0`) |
| Title at 0xA0 | `EZLoader` |
| Code at 0xAC | `00 00 00 00` |
| Maker at 0xB0 | `30 31` (`"01"`) |
| ROM size byte 0x14 | `0x11` |

This matches `EZLoader2.bin` (37,836 bytes, ARM, "EZLoader" at 0xA0) from the
original driver package.

## Layout from the 32 MB backup

`tools/map_rom.py` on the full backup shows game blocks separated by blank
space:

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

- `0x1080000` onward is **compressed**, not a raw GBA ROM: it starts
  `12 00 06 22` (not `2E 00 00 EA`) with high-entropy, nibble-repeating bytes.
  EZ-Flash applies its own LZ compression to stored games.
- `0x1E80000` is a linear byte counter (`00 01 02 03 ...`) — a leftover test
  pattern.
- 2072 of 8192 4 KB pages are all `0xFF`: most of the 32 MB part is blank.

## Loader internals (decoded)

The loader is ARM/Thumb code (`tools/disasm_arm.py`, `tools/arm_xref.py`).

### Boot path

- `0x000000` ARM `b 0x80000c0` → `0x0C0` → `0x0E0` boot stub → Thumb at `0x0FC`.
- `0x100` clears EWRAM/IWRAM.

### Menu state (IWRAM)

| Address | Meaning |
|---------|---------|
| `0x03000026` | **game count** — zero shows "No game found!" (`0x094C`) |
| `0x0300002C` | current page/first index |
| `0x03000034` | page count |
| `0x03000004` / `0x08` / `0x10` | directory entry fields |
| `0x03000044` | scan cursor |

Menu function at `0x0864` prints `EZLoader %s` + `V2.19C`, `Page(%d/%d)`, and
draws each entry. `0x094C` branches to "No game found!" when the count is 0.

### Directory scanner (`0x08064`)

The scan unlocks the EZ-Flash CPLD and walks cartridge banks:

- CPLD registers: `0x09FE0000`, `0x09880000`, `0x09FC0000`
- Unlock values: `0xD200` / `0x1500` (matches `docs/protocol_notes.md`)
- Bank windows: `0x08020000`, `0x08040000`, `0x08080000`
- Scans up to `0x09FFFFFF`

**Entry header (decoded at `0x08080`):**

```
entry + 0xB8: u16 length   -> next block start = current + (len << 15)
entry + 0xBE: u16 signature -> low byte must be 0xCE or 0xCF
```

The loader computes `next_start = entry + (u16@0xB8 << 15)` and checks
`byte@0xBE == 0xCE || 0xCF`; a mismatch jumps to the end of the scan
(`0x0832A`).

The loader image at `0x0` satisfies this: `u16@0xB8 = 0x0003` (<<15 =
`0x18000`), `byte@0xBE = 0xCE`.

### Stored entry header (GBA-header spare area)

| Offset | Value (loader) | Meaning |
|--------|---------------|---------|
| `0xB0` | `30 31` | maker/tag |
| `0xB2` | `0x96` | flags |
| `0xB4` | `80 C0` | `0xC080` |
| `0xB8` | `03 00` | length in 32 KB units (<<15) |
| `0xBE` | `CE 19` | signature low byte `0xCE` |

The test cart's other blocks (`0x1080000`, `0x1B50000`, `0x1E80000`) do **not**
carry this header, so they are not directory entries — they are raw compressed
payloads or leftovers, not games in the loader's list.

## What a multi-game writer needs

1. The loader/menu image at offset 0.
2. Each game LZ-compressed with EZ-Flash's own codec.
3. A per-entry `0xC0` header with the `0xCE`/`0xCF` signature and the length at
   `+0xB8`, so the scanner finds it.
4. Per-game save-slot mapping (the `Saver File Manager` side).

With `docs/firmware_re_rom_write.md` the raw transport now exists (including
bank flip). The remaining work is the **container format** (LZ codec + entry
header assembly), not the USB protocol.

## Status

- Directory `count` location, menu consumer, scanner location, entry signature
  (`0xCE`/`0xCF`), and length field (`+0xB8`) are **identified**.
- The exact LZ codec and the full entry header layout are **not** fully decoded.
- No multi-game write is implemented or tested.

## Next steps

1. Reverse the EZ-Flash LZ codec (the loader contains the decompressor).
2. Fully map the entry header and the saver file-system layout.
3. Then build a writer that assembles loader + compressed entries + headers.