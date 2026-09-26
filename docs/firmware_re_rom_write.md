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

## Corrections after hardware probing (supersedes parts above)

A run of on-hardware probes against a writeable EZ-Flash II cart corrected
several assumptions in the section above. Everything below is **observed on
hardware + matched to disassembly**; the earlier "confirmed by disassembly"
claims that conflict with this section are wrong.

### Endpoint roles (measured)

Sending command `0x01` to each OUT endpoint and reading EP2 IN:

| OUT EP | cmd `0x01` result | Role |
|--------|-------------------|------|
| EP1 | `Pipe error` | not an OUT channel |
| **EP2** | accepted, no fresh data | **write-side dispatcher** (`0x0046`) |
| EP3 | accepted, no reply | unused |
| **EP4** | **returns cart ROM bytes** | **cart command channel** (`0x0707`) |
| EP5 | accepted, no reply | unused |
| EP6 | accepted, no fresh data | unused |
| EP7 | `Pipe error` | unused |

- The EP4 OUT IRQ handler is `0x0707` and reads FIFO `0x7CC0`. **EP4 is the only
  cart command endpoint.** Reads (`0x01`) work on it; both EP2 and EP6 accept
  writes but never drive the cart.
- EP2's IRQ handler is `0x0046` and reads FIFO payload `0x7DC0` / count
  `0x7FC9`. EP2 is the **write-side** dispatcher, but it is keyed on `[0x0A]`,
  which is **set by the EP4 `0x02` handler**, not by EP2's own packet.

### Command `0x02` field map (measured + disassembled)

Handler `0x07B7` reads the packet as:

| Packet byte | XRAM | Meaning |
|-------------|------|---------|
| 1 | `0x14` | address low |
| 2 | `0x13` | address high |
| 3 | `0x08` | fixed bus byte for the op |
| **4** | **`0x0A`** | **flash-op / type selector** |
| 5 | `0x16` | bank/length token |

`[0x0A]` then routes: `0x65` → reset (`0x70`), `0x66` → save FLASH, `0x68` →
ROM, `0x69` → SRAM, anything else → the `0x0904` bank/page path.

### The single-byte write primitive: command `0x20`

`[0x20, addr_lo, addr_hi, data]` (handler `0x0BA3`) is the **byte write** the
working save path uses. It strobes:

```
0x7F9C <- 0xFF
0x7F98 <- 0x9F
0x7F96 <- data            ; packet byte 3
0x7F98 <- 0xBF, 0x9F
0x7F96 <- addr_lo ; 0x7F97 <- addr_hi
0x7F98 <- 0x97, 0x96, 0x97, 0x9F
```

The `0x97`/`0x96` strobe pair is the **data-write** strobe (versus `0x9A`/`0x9B`
for command/address writes). One byte per packet — no payload buffer involved.

### The `0x68` program path (`0x009F` → `0x0130`) — real 64-byte program

When `[0x0A]==0x68` **and** packet byte 3 (`[0x08]`) `>= 0x80`, the EP2 handler
runs the full AMD/Fujitsu word-program loop, exactly 32 iterations (`[0x19]`
0→`0x20`), i.e. **64 bytes**:

```
0x55 @ 0x05   strobe 0x9B
0xAA @ 0x00   strobe 0x9A,0x9F
0xAA @ 0x02   strobe 0x9B
0x55 @ 0x00   strobe 0x9A,0x9F
0x55 @ 0x05   strobe 0x9B
0xA0 @ 0x00   strobe 0x9A,0x9F          ; program-setup
[14]:[13]     strobe 0x9B                ; target word address
per 2 payload bytes from 0x7DC0:
  data_lo -> 0x7F96 ; data_hi -> 0x7F97 ; strobe 0x9A,0x9B
  poll status (0x89/0x8B -> [15],[17]) until equal
  advance [14]/[13] ; [0x19]++
```

With packet byte 3 `< 0x80` the same path instead just issues a chip-erase
sequence at `0x0207`.

**Observed:** this path does **not** wedge the firmware (device stays alive),
but no bytes were written in testing — the payload must be exactly **64 bytes**,
and it is still unconfirmed whether the CPLD needs a prior unlock or whether the
address is a word address.

### The `0x69` path is a dead end

`[0x0A]==0x69` reaches `0x054E`, which copies the payload (`LCALL 0x1761`, 32
words) **only when `[0x0C]:[0x0D]` is non-zero**. Nothing in the firmware ever
writes those registers to non-zero (only `0x0572/0x0575` and `0x08FB/0x08FE`
zero them). So `0x69` **always** falls to `0x0620` (command-issue) and then
hangs on a CPLD status poll. **Never use `0x69`** — it wedges the 8051 and needs
a physical replug to recover.

### Recovering a wedged writer

A wedged 8051 does not respond to a USB bus reset. `ezwriter-cli reload` sends a
CPUCS reset then a Windows PnP power cycle; that drops the device off the bus and
it re-appears in **bootloader mode** (`0547:2131`). Replug once, then `reload`
re-uploads `tusbez.bin` and the device returns to active mode. Budget one
physical replug per wedge.

### Still unconfirmed

- Whether command `0x20` alone programs ROM, or needs a prior program-setup.
- Whether the `0x68`/`[08]>=0x80` payload address is a byte or word address.
- Whether the CPLD requires an unlock cycle before `0x20`/`0x68` writes.

`flash-probe` was added to the CLI to drive these experiments:
`ezwriter-cli flash-probe <addr> --cmd <hex> --b3 <hex> --b4 <hex> --cmd-ep <n>
--ep <n> --payload <hex>`.

## External references checked (2026)

Searched for an existing write-protocol capture to avoid re-deriving it.

- **`tbex78/ezfadvanceIII`** (GitHub) — a fully capture-derived, hardware-proven
  toolset for the **EZ-Flash Advance III**, with a documented USB protocol:
  a 13-byte command frame `5A A5 92 <count> <selector> 00 00 00 <count> 00 00 00 00`,
  a `<count>`-byte data frame 750 µs later, and the **command echoed back** on
  EP IN. Commands: `0x91` read, `0x92` flash program, `0x95` manager prime,
  `0x96` sector erase, `0x97`/`0x98`/`0x99` startup, with read addresses encoded
  as `byte_offset / 2`.
- **Conclusion: this is a different generation.** The EZ2 firmware image
  `tusbez.bin` contains **no `5A A5 92` sequence** (checked by search) and its
  EP4 dispatch table (`0x0736`) is the older
  `01,02,03,04,05,06,14,19,1A,1F,20,21` set. The EZ3 protocol cannot be copied
  onto the EZ2 writer.
- **`ez-flash/omega-kernel`** — EZ-Flash Omega kernel source; documents the NOR
  filesystem and loader behaviour but not the EZ-Writer USB protocol.
- **asie's wiki** — EZ-Flash GBA **cart-side** unlock sequence
  (`0x9FE0000=0xD200`, `0x8000000=0x1500`, `0x8020000=0xD200`,
  `0x8040000=0x1500`); the writer's CPLD presumably issues these internally.
- **EZ Client user manual** — confirms the original workflow writes `BL.bin` (the
  loader) first, then per-game blocks, and that an `.ezf` file is a ROM + saver.
  No byte-level protocol is published.

No public USB capture of the EZ2/EZ-Writer `rom-write` sequence was found, so the
remaining gap (how `0x7DC0` is filled) still needs either a capture of the
original EZ Client or further CPLD/interface RE.