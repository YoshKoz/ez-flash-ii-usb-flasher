# Reverse-Engineering the EZ-Flash II Save Dump

How 128KB GBA FLASH saves (e.g. Pokémon Ruby/Sapphire) were dumped from the
EZ-Flash II / EZ-Writer II over USB, despite there being no documented
"read save" command. This is the narrative behind the `save-read` subcommand
and the `scratch/` + `tools/` reverse-engineering scripts.

---

## Why it took so long

The EZ-Flash II is a USB device built around a Cypress AN2131Q 8051
microcontroller. The original Windows "ezwriter" tool can pull saves, but that
knowledge only exists as 8051 machine code inside `tusbez.bin`. There was no
spec to read — every byte of the save protocol had to be inferred.

Three compounding problems:

```
┌─────────────────────────────────────────────────────────────────┐
│ PROBLEM 1: The save chip is invisible from USB                    │
│   The save RAM/FLASH lives in GBA cartridge address space,        │
│   NOT in the AN2131's address space. The 8051 has to bridge it.   │
│                                                                   │
│   PC ──USB──► AN2131 (8051) ──cart bus──► GBA save chip            │
│              (this hop is the whole mystery)                      │
├─────────────────────────────────────────────────────────────────┤
│ PROBLEM 2: GBA saves are 3 incompatible chip types                │
│   SRAM ('s'/0x73), EEPROM ('e'/0x65), FLASH ('f'/0x66)            │
│   Each needs a totally different read ritual. Pokémon Ruby/        │
│   Sapphire is 128KB FLASH — the hardest of the three (banked).    │
├─────────────────────────────────────────────────────────────────┤
│ PROBLEM 3: A stateful pipeline that lies to you                   │
│   The firmware auto-streams ROM data into EP2 and holds one       │
│   stale 64-byte chunk between commands. So your "save read"       │
│   returns leftover ROM bytes and looks like it half-worked.       │
└─────────────────────────────────────────────────────────────────┘
```

Problem 3 is the nasty one. A wrong attempt didn't fail cleanly; it returned
*plausible-looking ROM data*, so for days the reads looked "almost right" when
they were actually returning garbage from the previous command's buffer.

### Timeline (from git history)

```
2026-06-06  dump/cart-info work — ROM reading solved first
2026-06-17  fix(save-read): send byte address directly, drop dead dispatcher
2026-06-18  fix(save-read): restore cmd_save_read, rename save_read_byte_addr
2026-06-18  fix(save-read): re-send 0x14 select before EACH 0x02 chunk   ← key insight
2026-06-18  feat(reload): auto firmware reload (no manual replug)
2026-06-20  fix: ROM dump alignment + improve save-read protocol
2026-06-22  feat(cli): dump 128KB FLASH saves via native firmware path   ← BREAKTHROUGH
2026-06-22  feat(gui): same, in the GUI
2026-06-22  fix(gui): correct Ruby/Sapphire save type + headless --dump-save
2026-06-23  merge: native FLASH save reader (CLI + GUI)
```

Roughly **June 17 → June 22: five days of protocol guessing** before the FLASH
path worked, then GUI + polish.

---

## What was actually done — the investigation, in order

### Phase A — Disassemble the firmware to find the command handlers

`disasm_v2_cmd02.py` and `disasm_v2_cmd02_full.py` build a full **8051 opcode
table** and disassemble `an2131_fw_v2.bin` starting at the cmd-`0x02` handler
(`0x0DC5`) and the dispatch table (`0x0E4A`). This is how the command bytes were
identified at all:

```
cmd 0x01 = ROM read (word-addressed, 24-bit bank in byte[3])
cmd 0x02 = select save chip / set bank + flash-ready check
cmd 0x03 = stream 64 bytes from a 16-bit address   ← the actual save reader
cmd 0x14 = select save TYPE ('s'/'e'/'f')
cmd 0x19 = write cartridge register (unlock sequences)
cmd 0x20 = single-byte FLASH write (used for bank switching)
```

The disassembler also tags XRAM addresses as `EP2FIFO/EP4FIFO/EP6FIFO`, which is
how the EP2-streaming behaviour (Problem 3) was spotted in the code.

### Phase B — Brute-force every plausible read method

`tools/save_probe.py` (267 lines) is a **shotgun of 9 different theories**, each
a self-contained method:

```
Test 1  0x14 select + 0x02 read, 3 save types          (the obvious guess)
Test 2  same but WORD addressing (addr/2)               (GBA buses are 16-bit)
Test 3  sweep addresses 0…1MB                           (where does save live?)
Test 4  0x02 WITHOUT a preceding 0x14 select            (is select needed?)
Test 5  asie-style unlock (0x19 ×4) then read           (borrowed from EZ3)
Test 6  EZ3 register setup (unlock + RAM-page) then read
Test 7  cmd 0x01 ROM-read at 17 offsets across 32MB     (is save mapped into ROM space?)
Test 8  register reads (0x1A) at 8 key addresses
Test 9  cmd 0x01 across first 256 addresses
```

Three of these theories survived into Rust as fallback strategies — that's why
`main.rs` has **three** save functions:

- `save_read_via_rom_read` (Test 7 theory — read save as if it were ROM)
- `save_read_via_reg` (Tests 5/6 — unlock + map RAM page)
- `save_read_byte_addr` (Test 1 — the direct select+read that eventually won)

### Phase C — Isolate the protocol on real hardware (the `diag_save*` series)

Four files, an **iterative narrowing experiment**. Each is a Python script that
*writes a complete Rust `diag_save` probe program* to `C:\Users\yoshi\diag_save\`
to be `cargo run`. They go from broad to surgical:

```
diag_save_setup.py  Baseline: drain → CPLD unlock (0x19) → 0x14 select →
                    read 8 chunks → check if 0x19 itself emits EP2 data.
                    GOAL: map the state machine.

diag_save2.py       A/B test of cmd 0x02 byte layout:
                      NEW  [0x02, lo, mid, 0x66, bank, 0x00]   (type at byte3)
                      OLD  [0x02, lo, mid, hi,   0x66]         (type at byte4)
                    GOAL: where does the type byte actually go?

diag_save3.py       4 sub-tests A/B/C/D:
                      A  0x14(0x66) → 0x02 page-setup → 0x03 read   ← the winner
                      B  same with EEPROM type 0x65
                      C  0x02 4-byte, no inner cmd
                      D  0x03 alone, sweeping inner byte
                    GOAL: is read = 0x02 THEN 0x03? (yes)

diag_save4.py       Endgame: restore ROM auto-stream, 5-second timeouts,
                    probe cmd bytes 0x03–0x15 for "data that differs from
                    the known ROM pattern", and try save-chip word addresses.
                    GOAL: confirm 0x03 returns REAL save bytes, not ROM echo.
```

`diag_save4` is where Problem 3 got pinned: it explicitly compares returned data
against the known ROM signatures `32 00 00 ea` / `fc 7f 00 03` and only flags
`*** DIFFERENT DATA` when a command returns something that *isn't* a stale ROM
echo.

### Phase D — Fix the stale-buffer bug

`fix_dump.py` is a one-shot patcher that rewrites a block in `main.rs`. It
replaces the old **"prime the pipeline with a dummy 0x01 read"** hack with a
proper **drain loop**:

```diff
- // Send a dummy 0x01 read and discard one response  ← fragile, races
+ // Drain stale EP2 IN until timeout/error, capped at 8 iterations
+ for _ in 0..8 {
+     match handle.read_bulk(0x82, &mut drain, 50ms) {
+         Ok(n) if n > 0 => {}   // keep draining
+         _ => break,            // empty → aligned
+     }
+ }
```

This is the fix that finally made reads **start aligned to address 0** instead
of one chunk off. `read_save_fn.py` is a tiny helper that prints the
`cmd_save_read` function out of `main.rs` so it could be inspected while
iterating.

### Phase E — The working dumpers

Once the recipe was known, two clean Python dumpers proved it end-to-end before
it went into Rust.

**`scratch/save_dump.py`** — the canonical FLASH read recipe:

```python
sel(0x66)                       # cmd 0x14: select FLASH handler
def read_chunk(off):
    drain(60)                   # clear stale EP2  (Problem 3)
    w([0x02, lo, mid, bank, 0x66])   # select chip + bank + flash-ready
    w([0x03, lo, mid, 0, 0])         # stream 64 bytes at 16-bit addr
    return r(64)
# 2048 chunks × 64 = 128KB, checks for Gen3 signature 25 20 01 08
```

**`scratch/save_dump2.py`** — the *bank-switched* version, the real 128KB
solution. GBA FLASH only exposes 64KB at a time, so it issues the **flash
bank-switch command sequence** (`AA→2AAA:55→5555:B0→0000:bank`) to page between
the two 64KB banks, then reads each:

```python
for bk in (0,1):
    bank_switch(bk)             # AA / 55 / B0 / bank   ← classic Atmel/SST unlock
    for off in range(0, 0x10000, 64):
        out += read64(off)
flash_reset()                   # AA / 55 / F0  → leave read-array mode
```

It also **validates** the dump: parses the 14 Gen-3 save sections, checks their
footers (`signature 0x08012025`, section IDs 0–13 contiguous, save-index
counters) across both 0x0000 and 0xE000 slots. That validation is how the bytes
were confirmed to be a real Pokémon save and not noise.

**`scratch/analyze_sav.py`** — standalone forensic tool for an existing `.sav`:
confirms `bank0 == bank1`, walks the 16×4KB sections, and dumps each section
footer. The "did we actually get a valid save?" checker.

### Phase F — Promote into the product

The winning recipe became `save_read_byte_addr()` in `main.rs` (the `0x14`-
before-every-chunk + `0x02` pattern), wired into the `save-read` CLI subcommand
and the GUI's "Read Save" tab with a headless `--dump-save` flag. The two losing
theories stayed as `save_read_via_reg` / `save_read_via_rom_read` fallbacks
behind flags (`--use-reg`, `--use-rom-read`).

---

## The protocol that emerged (the actual answer)

```
   PER 64-BYTE CHUNK                        128KB FLASH DUMP
   ════════════════                         ════════════════
   ┌────────────────────────┐               ┌──────────────────────┐
   │ drain EP2 (kill stale)  │              │ select FLASH (0x14,66)│
   ├────────────────────────┤               ├──────────────────────┤
   │ 0x14  select FLASH 'f'  │              │  for bank in 0,1:     │
   ├────────────────────────┤               │    AA/55/B0/bank ─────┼─► bank switch
   │ 0x02  set bank +        │               │    for off 0..64KB:   │
   │       flash-ready check │               │      0x02 set addr    │
   ├────────────────────────┤               │      0x03 read 64B ───┼─► append
   │ 0x03  stream 64 bytes ──┼──► EP2 IN     │  AA/55/F0  flash reset│
   └────────────────────────┘               └──────────────────────┘
   re-select EVERY chunk —                   validate: Gen3 sig 25 20 01 08,
   the firmware forgets state                14 sections, footer 08012025
```

---

## File-by-file reference

| File | Role |
|---|---|
| `disasm_v2_cmd02.py` / `_full.py` | 8051 disassembler; decoded the cmd handlers (`0x02@0x0DC5`, dispatch `0x0E4A`) that named every command byte |
| `tools/save_probe.py` | 9-strategy brute-force harness — the theory-generation stage |
| `scratch/home-artifacts/diag_save_setup.py` | Baseline state-machine probe (drain→unlock→select→read) |
| `scratch/home-artifacts/diag_save2.py` | A/B test of the `0x02` byte layout (type at byte3 vs byte4) |
| `scratch/home-artifacts/diag_save3.py` | Found the `0x02`-then-`0x03` two-step read (test A) |
| `scratch/home-artifacts/diag_save4.py` | Confirmed `0x03` returns real save bytes, not stale ROM echo |
| `scratch/home-artifacts/fix_dump.py` | Patched `main.rs`: dummy-read prime → proper EP2 drain loop |
| `scratch/home-artifacts/read_save_fn.py` | Helper to print `cmd_save_read` from `main.rs` while iterating |
| `scratch/save_dump.py` | Clean single-bank FLASH read recipe (proof of concept) |
| `scratch/save_dump2.py` | **The 128KB solution** — bank-switched read + Gen3 validation |
| `scratch/analyze_sav.py` | Standalone `.sav` validator (sections, footers, bank compare) |
| `src/ezwriter-cli/src/main.rs` → `save_read_byte_addr` | Production implementation of the winning recipe |
| `src/ezwriter-cli/src/main.rs` → `save_read_via_reg` / `_via_rom_read` | The two losing theories, kept as flag-gated fallbacks |

The reason this is spread across ~10 throwaway scripts rather than one clean
file is exactly *because* it was reverse engineering: each script is one
falsified (or confirmed) hypothesis about a protocol nobody documented. The
work is that body of experiments.
