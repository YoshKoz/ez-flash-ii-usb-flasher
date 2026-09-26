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

Write a short known pattern to a bank-0 offset that is already blank, so no
erase is needed and a failure cannot destroy data. On the test cart the loader
ends around `0xA5C4`, so `0xA600` is free. Use `--no-erase`:

```console
# pattern file: 61 bytes of a known sequence
ezwriter-cli rom-write pattern.bin --addr 0xA600 --no-erase --verify
```

Expected: verify passes, `cart-read 0xA600 1` shows the pattern.

Bank-0 offsets only: `rom-write` refuses ranges that cross the 64 KB window
boundary, because the write-side bank flip is not yet hardware-confirmed.

If verify fails, stop. The handler command `0x04`/`0x02` or the packet layout
is wrong and the flash command bytes are not the issue.

## Step 3 — erase one sector

Only after Step 2 round-trips. Sector 1 is at `0x10000`, which is outside
bank 0 and currently refused by `rom-write`. Until the bank flip is confirmed,
test the erase on **bank-0 sector 0** at `0xA600` — but only if you accept that
sector being erased (it holds the loader on the test cart). Do not run this on
a cartridge whose loader you care about.

```console
# DESTRUCTIVE: erases bank-0 sector 0 (0x0000..0xFFFF)
ezwriter-cli rom-write pattern.bin --addr 0xA600 --verify
```

This exercises the `0x29` erase on sector 0 before writing. Verify reads the
range back. Expected: the pattern at `0xA600`, rest of the sector `0xFF`.

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