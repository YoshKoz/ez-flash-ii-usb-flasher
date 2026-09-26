# ROM Write: Where It Stands

Short version: the EZ2 writer's **write** path is not cracked yet. This file
records exactly what is proven, what was tried, and what the next step must be,
so the work does not have to be re-derived.

## What works (do not re-investigate)

- Device loads `tusbez.bin` and re-enumerates as `0548:1005`.
- **EP4 is the only cart command endpoint.** The 8051 IRQ handler `0x0707`
  reads EP4's FIFO at XRAM `0x7CC0` and dispatches on packet byte 0.
- **ROM reads work**: cmd `0x01` packet `[01, addr_lo, addr_hi, bus_byte]`,
  response on EP2 IN (64 bytes). Address is the **byte address**. For addresses
  above 64 KB pass the bank in packet byte 3 (`addr >> 17`); `cart-read` needs
  `--byte3-bank` or it aliases back to bank 0.
- **EP2 and EP6 accept writes but never drive the cart.** Sending any command
  other than a read to them returns stale EP2 IN data (once it looked like the
  ASCII string "version" — that was a stale buffer, not a real reply).
- **Save write works** and uses cmd `0x20` `[20, addr_lo, addr_hi, data]`,
  a one-byte bus write.

## The firmware's own program/erase routines (fully disassembled)

Both live on the **EP2** dispatch handler `0x0046`, keyed on `[0x0A]`, which is
**set by the EP4 `0x02` handler** from packet byte 4:

```
EP4: cmd 0x02  [02, addr_lo, addr_hi, bus_byte, selector, bank_token]
                 |        |         |         |          |
                 |        |         |         |          +-- [0x16] bank/length
                 |        |         |         +------------- [0x0A] op selector
                 |        |         +----------------------- [0x08] fixed bus byte
                 |        +--------------------------------- [0x13] addr high
                 +------------------------------------------ [0x14] addr low
```

The two routines, both reading a 64-byte block from **EP2's FIFO `0x7DC0`**
(confirmed: DPTR is computed as `0x7D00 + 0xC0 + index`, i.e. `0x7DC0 + i`):

- **Program** (`selector == 0x68`, `bus_byte >= 0x80`, handler `0x009F` →
  `0x0130`): full AMD/Fujitsu word-program — `55@05`, `AA@00`, `AA@02`,
  `55@00`, `55@05`, `A0@00`, then 32 iterations programming 2 bytes each
  (64 bytes) with status polling, advancing the address per word.
- **Erase** (`selector == 0x68`, `bus_byte < 0x80`, handler `0x0207`): writes
  `55@05`, `AA@00`, `[bus_byte]`, `25@00`, `0F@00`, `[bus_byte]` then
  `0x29` at the address — the AMD sector-erase command sequence.
- **`selector == 0x69` is a dead end.** Its payload-copy branch (`0x0561` →
  `0x1761`) needs `[0x0C]:[0x0D]` non-zero, and nothing in the firmware ever
  sets those (only zeroes them). So `0x69` always falls into a CPLD status wait
  and **hangs the 8051**. Never send `0x69`.

## What was tried and what happened

| Test | Result |
|------|--------|
| cmd `0x02` selector `0x68`, `bus_byte 0x80`, 64-byte EP2 payload | no wedge, **no write** |
| same, word address (`addr/2`) | no wedge, no write |
| `bus_byte` `0x40` / `0x81` / `0xFF` | no wedge, no write |
| EP4 cmd `0x04` (address stage) + 64-byte EP2 payload | no wedge, no write |
| payload sent **before** the command | no wedge, no write |
| payload on **EP4** instead of EP2 | no wedge, no write |
| `0x1F` reset before programming | no wedge, no write |
| `selector 0x69` (any form) | **wedges the 8051**, needs a replug |
| payload to EP6 | wedges |

**Reads need the word address.** `0x01` with the raw byte address returns
garbage (XRAM-looking bytes); with `byte_addr / 2` it matches the backup exactly.
`flash-probe --word-addr` selects that. The `0x68` program path also takes a
word address.

So the command reaches the firmware and the safe path runs the flash sequence,
but the bytes the routine reads out of `0x7DC0` are **not** the bytes sent to
EP2 (nor EP4). Either `0x7FC9`/`0x7DC0` belong to an endpoint the host is not
addressing, or the CPLD needs a preceding unlock the firmware does not issue.

## Recovery when the 8051 wedges

A wedged 8051 ignores a USB bus reset. `ezwriter-cli reload` sends the CPUCS
reset then a Windows PnP power cycle; that removes the device from the bus.
**Replug once** → it returns as bootloader `0547:2131` → run `reload` again to
re-upload `tusbez.bin`. Budget one physical replug per wedge.

## Next step (recommended)

The firmware says the payload lives at `0x7DC0`, but no host-side test has put
bytes there. The fastest way to settle it is a **USB capture of the original EZ
Client writing a ROM**, which pins down the exact packet that fills `0x7DC0`:

1. Run the original EZ Client against the EZ-Writer in a Windows XP VM (or a
   USB-capturing setup such as `usbmon`/Wireshark or a hardware USB analyzer).
2. Capture "Update All ROMs" for one small ROM.
3. Diff the OUT packets against what `flash-probe` sends.

A second option is to keep reverse-engineering the EZ-Writer's CPLD/Xilinx
XCR3128 interface, which is what maps `0x7DC0` onto an OUT endpoint.

The original EZ Client binaries were searched for but the reliable mirrors are
now gone or login-gated: `filetrip.net` (no Wayback snapshot), `ezflash.cn`
(403), `rbenda.de` (now a parking page), `dekazeta.net` (IPS login). The EZ
Client 3.26 also needs its XP-era kernel driver, which will not bind to WinUSB,
so a live capture here needs an XP VM anyway.

## Tooling

- `ezwriter-cli flash-probe` drives arbitrary command packets:
  `flash-probe <addr> --cmd <hex> --b3 <hex> --b4 <hex> --cmd-ep <n> --ep <n>
  --payload <hex>`.
- `tools/disasm8051.py` (with `--patches firmware/loader_table2.bin`),
  `tools/disasm_arm.py`, `tools/arm_xref.py`, `tools/map_rom.py`.

## External references checked

- `tbex78/ezfadvanceIII` — a **capture-derived EZ3** toolset with a published
  protocol (`5A A5 92` frame, command echo, `0x91` read / `0x92` program /
  `0x96` erase, addresses as `byte/2`). The EZ2 image contains **no `5A A5 92`**
  and uses an older command set, so this does not transfer.
- `ez-flash/omega-kernel`, asie's wiki (cart-side unlock), EZ Client manual
  (`BL.bin` loader first, then game blocks; `.ezf` = ROM + saver).
- No public EZ2 `rom-write` USB capture was found.