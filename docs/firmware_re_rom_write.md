# ROM Write / Erase Protocol — Firmware RE Findings

Reverse-engineered from `firmware/tusbez.bin` with `firmware/loader_table2.bin`
patches applied (the real runtime image), using `tools/disasm8051.py`.

This supersedes the guessed `write-rom` / `erase` command bytes.

## Method

`loader_table2.bin` is an `EZWLDR1` patch table: `[count:u16][addr:u16,len:u8,data]*`.
Apply it over `tusbez.bin` before disassembling — the shipped `tusbez.bin` alone
has different bytes in the dispatch region.

```console
python tools/disasm8051.py firmware/tusbez.bin --patches firmware/loader_table2.bin --at 0x0707 --len 120
```

## Cartridge bus registers

The 8051 reaches the EZ-Flash II cartridge through XRAM-mapped registers:

| XRAM | Meaning |
|------|---------|
| `0x7F96` | cart data |
| `0x7F97` | cart address low |
| `0x7F98` | cart control / strobe |
| `0x7F99` | cart data in |
| `0x7F9C` | cart reset / control A |
| `0x7F9D` | cart reset / control B |

Strobe values written to `0x7F98` (`0x9A/0x9B/0x9F/0xBF/0x97/0x89`) clock
address and data onto the cartridge bus. They are CPLD plumbing, not flash
commands. The flash command itself goes to `0x7F96`.

## EP4 OUT buffer (host command packet)

The EP4 OUT FIFO is mapped at `0x7CC0`:

| XRAM | Packet byte | Meaning |
|------|-------------|---------|
| `0x7CC0` | 0 | command byte |
| `0x7CC1` | 1 | address low |
| `0x7CC2` | 2 | address high |
| `0x7CC3` | 3 | param / suffix |
| `0x7CC4` | 4 | count low |
| `0x7CC5` | 5 | count high |
| `0x7DC0`+ | 6.. | payload (for streaming writes) |

`0x7FC9` holds the EP4 OUT byte count.

## Primary command dispatch

EP4 OUT interrupt handler at `0x0707` loads the command byte from `0x7CC0` and
calls the dispatcher at `0x15CE`, which `JMP @A+DPTR` into a table of
`[handler_hi, handler_lo, cmd]` triples at `0x0736`:

| Cmd | Handler | Purpose |
|-----|---------|---------|
| `0x01` | `0x075E` | ROM read (used by `dump`) |
| `0x02` | `0x07B7` | read with suffix: `0x66`=FLASH, `0x68`=ROM/save-flash, `0x69`=SRAM, `0x65`=EEPROM |
| `0x03` | `0x0A09` | save write (64-byte stream) |
| `0x04` | `0x0A82` | **ROM/CPLD write setup** — set addr, emit `0x9F` |
| `0x05` | `0x0A9F` | ROM/CPLD write increment |
| `0x06` | `0x0AB8` | ROM/CPLD write finish — emit `0x6F` |
| `0x14` | `0x0AE1` | save type select (FLASH) |
| `0x19` | `0x0AF3` | write cart register (CPLD unlock) |
| `0x1A` | `0x0B42` | read cart register |
| `0x1F` | `0x0AEA` | reset (`0x7FBD <- 0x40`) |
| `0x20` | `0x0BA3` | write one byte (save bank switch) |
| `0x21` | `0x0BDF` | read one byte (JEDEC ID) |

## ROM write: command `0x04` (body at `0x068B`)

`0x04` writes a run of bytes from the EP4 payload straight to cartridge ROM:

```
count = [0x7FC9]                     ; EP4 OUT byte count
for i in 0..count:
    A = XRAM[0x7DC0 + i]             ; payload byte
    cart_data(0x7F96) = A
    cart_ctrl(0x7F98) = 0xBF, 0x9F   ; strobes
    cart_data(0x7F96) = addr_lo
    cart_addr(0x7F97) = addr_hi
    cart_ctrl(0x7F98) = 0x97, 0x9F   ; strobes
    advance addr
```

So the host packet is **`[0x04, addr_lo, addr_hi, 0, count_lo, count_hi, payload...]`**,
where the payload length matches `count`. This is the real ROM write primitive.

**Limitation:** the firmware byte loop increments only the 16-bit address
(`[0x11]`/`[0x10]`) and the packet has no bank byte, so command `0x04` addresses
**one 64 KB window at a time**. Writing past the window boundary would wrap.
`rom-write` therefore refuses ranges that cross a 64 KB boundary until the
bank-select sequence for writes is confirmed.

## ROM flash command bytes (AMD/Fujitsu JEDEC, word mode)

The cartridge NOR flash uses the standard AMD/Fujitsu command set, emitted as
16-bit words on the cart bus by the save/ROM dispatch:

| Value | Routine | Meaning |
|-------|---------|---------|
| `0xAA` | `0x00D3` etc. | unlock cycle 1 |
| `0x55` | `0x00C1` etc. | unlock cycle 2 |
| `0xA0` | `0x0123` | program setup |
| `0x0F` | `0x02BB` | erase setup |
| `0x29` | `0x0347` | erase confirm (per sector) |
| `0x41` | `0x064A` | program (write) |
| `0x70` | `0x081C` | read status register |
| `0x25` | `0x0294` | autoselect / ID entry |
| `0x40` | `0x12A7` | save-FLASH sector erase (separate chip) |

### Erase sequence (`0x020D` → `0x0347`)

```
cart_data = 0x00, cart_ctrl = 0xBF,0x9F
cart_data = 0x55, cart_addr = 0x05, cart_ctrl = 0x9B
cart_data = 0xAA, cart_addr = 0x00, cart_ctrl = 0x9A,0x9B
... 0x0F erase-setup, then 0x29 erase-confirm per sector
advance addr by 0x10 words (32 bytes/row) per step
```

Note: the address step in the erase loop is `+0x10` **words** = 32 bytes, which
matches programming the flash one row at a time.

## Status

- Dispatch table, EP4 packet layout, `0x04` write primitive, and the full
  flash command set: **confirmed by disassembly**.
- `write-rom` should use command `0x04` with a real payload, not the guessed
  raw `0x41`.
- `erase` must walk the AMD unlock + `0x0F`/`0x29` sequence, not send a bare
  `0x40`.
- Hardware confirmation still pending — see `docs/rom_write_test_plan.md`.

## Tooling

- `tools/disasm8051.py` — 8051 disassembler with patch application, jump-table
  and xref finders. Recovered from the pre-rewrite `disasm_v2_cmd02*.py`,
  `dis_dispatch.py`, `merge_fw_patches.py` (still retrievable from old GitHub
  commit SHAs).