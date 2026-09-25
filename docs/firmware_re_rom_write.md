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
| `0x7F96` | cart data / command |
| `0x7F97` | cart address low |
| `0x7F98` | cart control / strobe |
| `0x7F99` | cart data in (CPLD status read) |
| `0x7F9C` | cart reset / control A |
| `0x7F9D` | cart reset / control B |

Strobe values written to `0x7F98` (`0x9A/0x9B/0x9F/0xBF/0x89/0x8B/0x97/0x6F`)
clock address, data and reads onto the bus. `0x89` starts a CPLD status read
(`0x7F99`), `0x8B` clears it.

## EP4 OUT buffer (host command packet)

The EP4 OUT FIFO is mapped at `0x7CC0`:

| XRAM | Packet byte | Meaning |
|------|-------------|---------|
| `0x7CC0` | 0 | command byte |
| `0x7CC1` | 1 | address low |
| `0x7CC2` | 2 | address high |
| `0x7CC3` | 3 | sub-op / flash command (`[0x08]`) |
| `0x7CC4` | 4 | type/suffix (`[0x0A]`) |
| `0x7CC5` | 5 | bank / upper address (`[0x16]`) |
| `0x7DC0`+ | | payload for command `0x04` |

`0x7FC9` holds the EP4 OUT byte count.

## Primary command dispatch

EP4 OUT interrupt handler at `0x0707` loads the command byte from `0x7CC0` and
calls the dispatcher at `0x15CE`, which `JMP @A+DPTR` into a table of
`[handler_hi, handler_lo, cmd]` triples at `0x0736`:

| Cmd | Handler | Purpose |
|-----|---------|---------|
| `0x01` | `0x075E` | ROM read |
| `0x02` | `0x07B7` | dispatch by **suffix** (see below) |
| `0x03` | `0x0A09` | save write (64-byte stream) |
| `0x04` | `0x0A82` | ROM/CPLD write: address setup + payload byte loop `0x068B` |
| `0x05` | `0x0A9F` | ROM/CPLD write increment |
| `0x06` | `0x0AB8` | ROM/CPLD write finish (`0x6F`) |
| `0x14` | `0x0AE1` | save type select (FLASH) |
| `0x19` | `0x0AF3` | write cart register (CPLD unlock) |
| `0x1A` | `0x0B42` | read cart register |
| `0x1F` | `0x0AEA` | reset (`0x7FBD <- 0x40`) |
| `0x20` | `0x0BA3` | write one byte (save bank switch) |
| `0x21` | `0x0BDF` | read one byte (JEDEC ID) |

## Command `0x02` — typed access, selected by suffix

Handler `0x07B7` loads packet bytes into XRAM, then switches on the **suffix**
byte (`[0x0A]`, packet byte 4):

| Suffix | Handler | Target |
|--------|---------|--------|
| `0x66` `'f'` | `0x114A` | save FLASH |
| `0x68` `'h'` | `0x145F` | **ROM flash op** (erase / program / status) |
| `0x65` `'e'` | `0x0806` | EEPROM |
| `0x69` `'i'` | `0x089F` | SRAM |

For the ROM path (`0x145F`):

```
cart_data(0x7F96) = [0x08]      ; packet byte 3 = flash sub-op / command
cart_ctrl(0x7F98) = 0xBF, 0x9F
cart_data(0x7F96) = [0x14]      ; addr low
cart_addr(0x7F97) = [0x13]      ; addr high
cart_ctrl(0x7F98) = 0x9B
clr(0x7F9C)
cart_ctrl(0x7F98) = 0x89        ; start CPLD status read
[0x15] = cart_data_in(0x7F99)
cart_ctrl(0x7F98) = 0x8B        ; clear
```

So **`[0x02, addr_lo, addr_hi, op, 0x68, bank]`** sends one flash command byte
(`op`) at `addr`, then polls the CPLD until ready.

## The real write dispatch (EP0 bulk handler `0x0046`)

The bulk-out setup handler at `0x0046` (reached from `0x162C`) branches on the
**type byte in packet byte 4** (`[0x0A]`). This is the authoritative operation
selector for ROM writes:

| Byte 4 | Handler | Operation |
|--------|---------|-----------|
| `0x04` | `0x068B` | **payload byte loop** — write data bytes to ROM |
| `0x65` `'e'` | `0x0436` | EEPROM program |
| `0x66` `'f'` | `0x037B` | save FLASH program |
| `0x67` `'g'` | `0x0517` | **ROM bank/page program** (bank token in byte 5) |
| `0x68` `'h'` | `0x0095` | **ROM chip erase** (`[0x08] < 0x80`) / program (`>= 0x80`) |
| `0x69` `'i'` | `0x054E` | SRAM |

### Write-side bank select — solved

In the `0x0517` bank/page engine, packet byte 5 (`[0x16]`) is compared to
**`0x5A`** at `0x05F8`:

```
if [0x16] == 0x5A:
    [0x13] += 2          ; advance the high address by 2 -> next 64 KB bank
else:
    [0x14] += 0x80       ; advance within the bank
```

So **byte 5 = `0x5A` selects the bank-flip path**, and ordinary values advance
within the current bank. This is the write-side equivalent of the read path's
bank byte, and it removes the 64 KB limit.

### Full-cartridge write recipe

```
per 64 KB window:
  bank/page program : cmd 0x02 [02, 00, 00, <op>, 0x67, 0x5A]   ; flip to next bank
  program setup     : cmd 0x02 [02, addr_lo, addr_hi, 0xA0, 0x68, 0]
  payload           : cmd 0x04 [04, addr_lo, addr_hi, payload...]
  status / reset    : cmd 0x02 [02, addr_lo, addr_hi, 0x70, 0x68, 0]
erase a sector      : cmd 0x02 [02, addr_lo, addr_hi, 0x29, 0x68, 0]
read back           : cmd 0x01 [01, addr_lo, addr_hi, bank]
```

## Command `0x04` — payload write

Handler `0x0A82` stages the address from packet bytes 1-2, and the byte loop at
`0x068B` copies the whole EP4 payload to the cart bus, auto-incrementing the
16-bit address per byte. The packet is `[0x04, addr_lo, addr_hi, payload..]`
with **no count field**; the write length is the EP4 packet length. Payload max
is 61 bytes (64-byte packet minus the 3-byte header).

## Flash command bytes (AMD/Fujitsu JEDEC, word mode)

Emitted on the cart bus by the firmware's own flash routines:

| Value | Routine | Meaning |
|-------|---------|---------|
| `0xAA` / `0x55` | `0x00D3` / `0x00C1` | unlock cycle 1 / 2 |
| `0x25` | `0x0294` | command prefix / autoselect |
| `0x0F` | `0x02BB` | erase setup |
| `0x29` | `0x0347` | erase confirm |
| `0x41` | `0x064A` | program |
| `0xA0` | `0x0123` | program setup |
| `0x70` | `0x081C` | read status / reset to read array |
| `0x40` | `0x12A7` | save-FLASH sector erase |

### Chip/full erase sequence (`0x0207`, via `0x02`+suffix `0x68`)

```
0x7F96 <- 0xAA, 0x7F97 <- 0x02, strobe 0x9B     ; unlock AA @ 0x0200
0x7F96 <- 0x55, 0x7F97 <- 0x00, strobe 0x9A/0x9B; unlock 55 @ 0x0000
0x7F96 <- [0x08], strobe 0xBF/0x9F              ; host op byte = erase command
0x7F96 <- 0x00, 0x7F97 <- [0x13], strobe 0x9B   ; sector address
0x7F96 <- 0x25, 0x7F97 <- 0x00, strobe 0x9A     ; prefix
0x7F96 <- 0x00, 0x7F97 <- [0x13], strobe 0x9B   ; address again
```

## What the host must do

```
erase a sector:   cmd 0x02 [02, addr_lo, addr_hi, 0x29, 0x68, bank]
program a run:    cmd 0x02 [02, addr_lo, addr_hi, 0xA0, 0x68, bank]   ; program setup
                  cmd 0x04 [04, addr_lo, addr_hi, payload..]          ; bytes
                  cmd 0x02 [02, addr_lo, addr_hi, 0x70, 0x68, bank]   ; status/reset
flip write bank:  cmd 0x02 [02, 0, 0, <op>, 0x67, 0x5A]
read back:        cmd 0x01 [01, addr_lo, addr_hi, bank]
```

## Status

- Dispatch tables, EP4 packet layout, the `0x0046` type-byte dispatch, the
  `0x02`+suffix ROM path, the `0x04` payload write, the `0x5A` write-bank
  token, and the flash command set: **confirmed by disassembly**.
- The exact op byte the host must pass (`0x29` erase-confirm, `0xA0`
  program-setup, `0x70` reset) and the `0x67`/`0x5A` bank flip still need
  **hardware confirmation** — see `docs/rom_write_test_plan.md`.

## Tooling

- `tools/disasm8051.py` — 8051 disassembler with patch application, jump-table
  and xref finders. Recovered from the pre-rewrite `disasm_v2_cmd02*.py`,
  `dis_dispatch.py`, `merge_fw_patches.py` (still retrievable from old GitHub
  commit SHAs).