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

## Client CPLD/bank init sequence (new, 2026-09-27)

Decoded from `EZClient.exe` 3.26's disassembly (`ROM session init` at `0x414f90`,
cmd19 helper at `0x422460`, cmd1a helper at `0x4224b0`). The client runs this
**every session, before any cmd `0x04`/`0x02` write**, via cmd `0x19` (write
register, `[19, addr_lo, addr_mid, addr_hi, val_lo, val_hi]`) and cmd `0x1A`
(read register, flash ID query at addr `0x000064`):

| addr | val |
|------|-----|
| 0xFF0000 | 0xD2FF |
| 0x000000 | 0x15FF |
| 0x010000 | 0xD2FF |
| 0x020000 | 0x15FF |
| 0xA00000 | 0x667A |
| 0xFE0000 | 0x15FF |
| 0xFF0000 | 0xD2FF |
| 0x000000 | 0x15FF |
| 0x010000 | 0xD2FF |
| 0x020000 | 0x15FF |
| 0xE20000 | 0x51FF |
| 0xFE0000 | 0x15FF |

...then a flash-ID read (cmd `0x1A` addr `0x000064`) before the client decides
which cart profile it's talking to.

Implemented as `rom_session_init()` in `main.rs`, exposed two ways:
- `ezwriter-cli session-init` — runs the sequence + flash-ID read only, no
  erase/program. Safe to test in isolation.
- `ezwriter-cli rom-write ... --init` — runs it once before the erase/program
  loop.

**Tested 2026-09-27, does not fix the write path.** `session-init` alone: ran
clean, no wedge, flash-ID read returned `82 87 2e 00 00 ea 24 ff ae 51 69 9a
a2 21 3d 84` (never independently verified against a known-good flash
datasheet ID — could be a real ID or could be reading the same
address-decode issue as everything else here). `rom-write --init` on a 16-byte
test pattern at addr 0: same result as without `--init` — "no wedge, no
write," verify failed, original loader bytes unchanged. So the init sequence
by itself isn't the missing unlock; whatever gap is described above (payload
never reaching `0x7DC0`) is still unexplained.

Also confirmed this session: **`reload`'s OS power-cycle (`Disable-PnpDevice`
/ `Enable-PnpDevice`) is not a real port power cycle on this hardware's USB3
root hub controller.** Tried both at the device level and at the parent root
hub level — neither recovers a device wedged as `VID_0000&PID_0002`
("descriptor request failed"). Only a physical unplug/replug clears it. Do
not re-attempt a PnP-layer fix for this; there isn't one. (Code for the
hub-level attempt was written, tested, confirmed not to work, and reverted —
not left in the tree.)

## Word-addressing fix tested, still no write (2026-09-27)

`docs/ezclient_protocol.md` (recovered separately, see above) states the cmd
`0x04` ROM-write header must be **word-addressed** (`byte_addr / 2`), same as
reads — `rom_program_chunk` and `rom_flash_op` were both still using the raw
byte address. Fixed both in `main.rs`. Also confirmed via `ezwriter.sys`
disassembly (`sub_10F76`, `USBD_ParseConfigurationDescriptorEx`) that the
driver's pipe-index table is built in USB descriptor enumeration order, so
client write-endpoint index `1` really does map to physical `EP2 OUT` (index
0=EP1, 1=EP2, 2=EP3, 3=EP4 — matches the already-confirmed cmd-endpoint
mapping). So the endpoint we've been using was already right.

Tested on hardware at the documented-safe address `0xA600`:
- Word-addressed `rom_program_chunk`/`rom_flash_op` alone: **no wedge, no
  write** (identical symptom to every prior attempt in this file).
- Same, plus `--init` (the CPLD/bank register replay from the earlier
  session-init work): **no wedge, no write**, same result.

Device stayed responsive both times — no new wedge, unlike the earlier
`--init` test at addr `0x0` (loader sector) which did wedge. Not yet
understood why addr 0 wedges and 0xA600 doesn't; could be addr-0-specific
(loader sector CPLD mapping) rather than something `--init` does generally.

This does not change the conclusion below: the packet reaches the firmware
and the safe path runs, but the bytes never land in flash. Word-addressing
was a real bug (now fixed) but not *the* bug. Do not re-try this exact
combination (word-addr + EP2 + 0xA0 unlock, with or without `--init`) as if
it were untested — it now is.

## JEDEC ID-detection replication, still unresponsive (2026-09-27)

Traced `EZClient.exe`'s cart-profile detector (`sub_414860`) back to its input
source, `sub_423830` — a JEDEC-style manufacturer/device ID probe: writes
`0xF0`/`0x90` (reset/autoselect) at various addresses via cmd `0x19`, reads
back via cmd `0x1A`, and branches on the result to pick a cart-profile class
and its vtable (which is what ultimately decides the CPLD unlock sequence
used for writes — see the `sub_421910` finding above).

Replicated the probe reads/writes directly against hardware:
- `read-reg 0`, `read-reg 0x1000`, `read-reg 0x802000`, `read-reg 0x803000`:
  `0x8782`, `0xFFFF`, `0x8782`, `0xFFFF` — addr 0 and 0x802000 mirror exactly,
  suggesting a 0x800000-aliased address space (useful, unexplained detail).
- Sent the client's `0xFF`/`0x90` autoselect-unlock write sequence
  (`write-reg 0 0xFF`, `write-reg 1 0xFF`, `write-reg 0 0x90`), then
  re-read addr 0/2: **identical to before the unlock** (`0x8782`/`0xFFFF`).
  No wedge, but also no observable effect from the unlock at all.

This is the same pattern as every other test this session: the firmware
accepts every command packet we send and never errors, but nothing we send
ever changes cart-side state in an observable way (register reads are
static, ROM writes don't land, JEDEC unlock doesn't budge the ID readout).
That consistency points at something upstream of any specific command
sequence — most likely a CPLD chip-select/enable step that's simply never
being asserted by anything we've sent, rather than a wrong unlock recipe.

Also confirmed: the vtable-based per-chunk write bracket used
by the real client's save/ROM-write functions (`sub_41BB80`, `sub_41ADD0`) —
open (vtable+0x3C), page-select (vtable+0x44, passing a computed page
number), close (vtable+0x3C again), *then* the cmd `0x04` write — was never
replicated in our Rust code. Static tracing could not pin down which
concrete cart-profile vtable (and thus which CPLD sequence) matches our
physical cart without live confirmation, and further progress needs to see
real register state changing in response to a command, which we have not
observed for anything yet.

**Static RE is exhausted for now.** Every remaining unknown needs to see
actual register/flash state respond to something, which requires either the
XP-VM USB capture (see top of this file) or discovering why register writes
have no observable effect on this specific cartridge/writer pairing.

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

## 2026-09-27 (session 2): USB capture working, read path fixed, EP 0x84

This session got a USB capture of the real EZClient running against the writer
(inside the Win7 VirtualBox VM) and fixed a read-path bug it exposed.

### Capture method (Phase 1) — solved

- `VBoxManage controlvm <vm> usbattach <uuid> --capturefile=...` is **broken in
  VirtualBox 7.2.20** ("Wrong number of arguments"); do not use it.
- Working method: **host-side USBPcap** on the root hub the writer sits on
  (`\\.\USBPcap2` on this machine), with descriptor injection:
  `USBPcapCMD.exe -d \\.\USBPcap2 -o cap.pcap -A --inject-descriptors`
  then `tshark -r cap.pcap -Y "usb.idVendor==0x0548" -T fields ...`.
- `dumpcap -D` does **not** list USBPcap interfaces here, but `USBPcapCMD.exe`
  works directly against `\\.\USBPcapN`.
- This captures the writer's URBs **even while the VM owns the device**
  (VBoxUSBMon forwards through the host USB stack).

### EZClient startup capture (1910 frames)

- All commands go OUT on EP `0x04`: `0x19` write-register (728×) and `0x1A`
  read-register (112×).
- The `0x19` sequence matches `rom_session_init` in `main.rs` byte-for-byte.
- **Every IN reply arrived on EP `0x84`; there were zero bulk INs on `0x82`.**
- EZClient does **not** read the ROM at startup (the cart list it displays is
  cached); a real ROM-read/write capture still needs a click in its UI.

### Bug fixed: `reg_read` read the wrong IN endpoint

- `reg_read` read replies from `0x82` (a stale buffer). The client reads them
  from **`0x84`**.
- Fix (one line in `src/ezwriter-cli/src/main.rs`): `read_bulk(0x82, …)` →
  `read_bulk(0x84, …)` in `reg_read`.
- Verified: `session-init` flash-ID changed from the constant stale
  `2e 00 20 00 20 00 20 00 …` to real, address-dependent JEDEC data matching the
  client's response body (`… 9d 38 88 42 c0 c7 b2 5f …`).

### Root cause of the "aliased" / `92 00` reads: stale 8051 firmware state

- Native `cart-read` first returned aliased data (`2e 00 20 00 20 00 20 00 …`),
  then a fixed-position corruption (`92 00` at bytes 2–3 and 10–11 of every 16).
- After a physical replug the writer is bootloader `0547:2131`.
  `firmware-download tusbez.bin` leaves it in bootloader, and its automatic
  "OS power cycle" **wedges the 8051** (`VID_0000&PID_0002`, "Device Descriptor
  Request Failed"). Only a physical replug clears that.
- **Correct re-init: `ezwriter-cli init-exact loader_table1.bin loader_table2.bin`**
  → device returns to active mode `0548:1005`.
- After `init-exact` + `reset-cart`, reads are clean:
  `2e 00 00 ea 24 ff ae 51 69 9a a2 21 3d 84 82 0a` (valid entry point +
  Nintendo logo), `0xA0` = `EZLoader`, `cart-info` prints `Title: EZLoader`.
- So the aliasing was a half-initialised firmware, **not** the read command or
  the address scheme.

### Still open

- `rom-write` is still unverified on hardware; the remaining step is a capture of
  EZClient performing an actual ROM read and write (needs a UI click, and the
  write erases/writes the cart).
- EZClient showed "No Cart" while the firmware was stale; behaviour after a clean
  `init-exact` has not been re-checked in the VM yet.