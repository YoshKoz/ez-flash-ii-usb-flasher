# ROM Write Test Plan

Scope: prove `write-rom` / `erase` work against a real, writeable EZ-Flash II
cartridge, using the protocol in `docs/firmware_re_rom_write.md`.

**Status: not yet run.** Nothing in this repo has written to a cartridge.

## Preconditions

- A cartridge you accept can be erased. A retail game is not a good first
  target — start with a cart already in loader/multi-game mode (reads as
  `EZLoader` at 0xA0).
- A full backup exists: `ezwriter-cli dump backup_before.gba`.
- Writer detected in active mode: `ezwriter-cli list`.

## Step 1 — read back the current state

```console
ezwriter-cli cart-info
ezwriter-cli cart-read 0 1        # confirm entry point bytes
```

Record the first 64 bytes. Every later step compares against this.

## Step 2 — smallest possible write (no erase)

Write a 64-byte known pattern to a sector that is already blank, so no erase is
needed and a failure cannot destroy data. Use `--no-erase`:

```console
# pattern file: 64 bytes, e.g. DE AD BE EF ... 
ezwriter-cli rom-write pattern64.bin --addr 0x0 --no-erase --verify
```

Expected: verify passes, `cart-read 0 1` shows the pattern.

If verify fails, stop. The handler command `0x04` or the packet layout is wrong
and the flash command bytes are not the issue.

## Step 3 — erase one sector

Only after Step 2 round-trips:

```console
ezwriter-cli rom-write pattern64.bin --addr 0x10000 --verify
```

This exercises the `AA/55/0F/29` erase on sector 1 before writing. Verify reads
the sector back. Expected: 64 bytes written, rest of the sector reads `0xFF`.

## Step 4 — confirm the rest of the sector is blank

```console
ezwriter-cli cart-read 0x10000 8
```

Expected: the pattern, then `FF` for the remaining chunks. If the erase did not
run, the old data is still present and `--verify` would have failed at Step 3.

## Step 5 — restore

Write `backup_before.gba` back over the sector(s) that were touched, or leave
the cart in a known state you accept.

## Failure modes

| Symptom | Meaning |
|---------|---------|
| Step 2 verify fails | `0x04` payload layout wrong, or cart not writeable |
| Step 3 verify fails at first chunk | erase sequence wrong (sector still programmed) |
| Step 3 verify fails at later chunks | per-chunk addressing/program timing |
| Whole cart reads `FF` after Step 3 | erase worked but program did not |
| Cart stops responding | CPLD locked — replug to recover (RAM firmware resets) |

## Safety

- Never interrupt an erase or write.
- Keep the cart on a direct USB port, not a hub.
- If the writer stops responding, unplug/replug: firmware lives in RAM, so the
  writer always recovers by power-cycling.
- A cartridge erase is not recoverable except from a backup.