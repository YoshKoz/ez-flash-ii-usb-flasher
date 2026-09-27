# Session Findings — 2026-09-27 (capture + ROM-write attempt)

This records everything done in the session that got a real USB capture of
`EZClient.exe` and re-attempted the ROM write. It complements
`rom_write_status.md` (which has the new capture/endpoint section appended) and
`firmware_re_rom_write.md` (firmware dispatch).

## Environment

- Host: Windows 11, Secure Boot ON (untouched), Hyper-V enabled (untouched).
- Guest: `EZFlash-Win7x86` VirtualBox VM — Win7 Ultimate 32-bit, BIOS, no guest
  Secure Boot. Has the vendor driver `ezwriter.sys` and `EZ Client 3.26`
  installed at `C:\EZClient`.
- The writer cannot be owned by the host and the VM at once. Power the VM off
  before using `ezwriter-cli` / `ezwriter-gui`.

## Capture method (this is the working one)

- `VBoxManage controlvm <vm> usbattach <uuid> --capturefile=...` is **broken in
  VirtualBox 7.2.20** (`Wrong number of arguments`). Do not use it.
- Use **host-side USBPcap** on the root hub the writer is on — `\\.\USBPcap2`
  on this machine (`USBPcapCMD.exe -d \\.\USBPcap2 -o cap.pcap -A
  --inject-descriptors`). Then:
  `tshark -r cap.pcap -Y "usb.idVendor==0x0548" -T fields ...`.
- `dumpcap -D` does **not** list USBPcap interfaces here; call `USBPcapCMD.exe`
  directly.
- This captures the writer's URBs **even while the VM owns the device**.

## EZClient startup capture (1910 frames)

Source: `docs/captures/ezclient_startup_trace.txt` (+ `.pcap`).

- All commands OUT on EP `0x04`: `0x19` write-register (728×) and `0x1A`
  read-register (112×).
- The `0x19` sequence matches `rom_session_init` byte-for-byte
  (`FF0000=D2FF`, `000000=15FF`, …).
- **Every IN reply arrived on EP `0x84`; zero bulk INs on `0x82`.**
- EZClient does **not** read the ROM at startup — the cart list it shows is
  cached. A ROM read/write capture still needs a click in its UI.

## Fix: `reg_read` read the wrong IN endpoint (committed `fddd02b`)

`reg_read` read replies from `0x82` (a stale buffer). The client reads them from
`0x84`. Result: `session-init` flash-ID changed from the constant stale
`2e 00 20 00 20 00 20 00 …` to real, address-dependent JEDEC data
(`… 9d 38 88 42 c0 c7 b2 5f …`).

## Root cause of the "aliased"/`92 00` reads: stale 8051 firmware

- Native `cart-read` first returned aliased data (`2e 00 20 00 20 00 20 00 …`),
  then a fixed-position corruption (`92 00` at bytes 2–3 and 10–11 of every 16).
- That was a **half-initialised firmware**, not the read command.
- Correct re-init: `ezwriter-cli init-exact loader_table1.bin loader_table2.bin`
  → device returns to active mode. After it, reads are clean:
  `2e 00 00 ea 24 ff ae 51 69 9a a2 21 3d 84 82 0a`, `0xA0` = `EZLoader`,
  `cart-info` prints `Title: EZLoader`.
- `firmware-download tusbez.bin` left the device in **bootloader**, and its
  automatic "OS power cycle" **wedged the 8051** (`VID_0000&PID_0002`); only a
  physical replug cleared it. Use `init-exact` with the loader tables, not
  `tusbez.bin`.
- `reset-cart` also restores the flash to read-array mode after experimental
  reads.

## ROM write attempt (still fails — "no write")

`ezwriter-cli rom-write test_64k.bin 0 --init --verify`:
- session init + 1-sector erase + 1024×64-byte payloads all "ran";
- verify failed: 1024 chunks differed; **neither erase nor program changed the
  flash** (cart still reads `2e 00 00 ea …`, `EZLoader`).
- The cartridge was **not damaged**. `backup_bank0.bin` (64 KB) saved.

## Static RE of the firmware write dispatch (this session)

From `tools/disasm8051.py firmware/tusbez.bin --patches firmware/loader_table2.bin`:

- **EP2 bulk-out setup handler `0x0046`** branches on `[0x0A]`:
  - `[0x0A]==0x04` → `0x068B` (payload byte loop)
  - `[0x0A]==0x65` → `0x0436` (EEPROM)
  - `[0x0A]==0x66` → `0x037B` (save FLASH)
  - `[0x0A]==0x67` → `0x0517` (bank/page)
  - `[0x0A]==0x68` → `0x0095`: `MOV A,[08]; SUBB A,#0x80; JNC 0x009F`
    → `[08]>=0x80` runs the **program loop `0x009F`→`0x0130`** (AMD word
    program, reads 32 words from XRAM `0x7DC0` = the EP2 FIFO), else `0x0207`
    (erase).
- **EP4 `0x02` handler `0x07B7`** copies packet bytes into XRAM:
  byte1→`[14]`, byte2→`[13]`, byte3→`[08]`, **byte4→`[0x0A]`**, byte5→`[0x16]`.
  So the EP4 `0x02` packet is what **sets `[0x0A]`** (the operation selector the
  EP2 handler reads). Suffix `0x68` then `LCALL 0x145F`.
- **EP4 `0x04` handler `0x0A82`** reads the address from `0x7CC1`/`0x7CC2` into
  `[0x11]`/`[0x10]` and strobes `0x7F98=0x9F`; it does **not** set `[0x0A]`.

### Implication

The payload must arrive on **EP2 OUT** and is triggered by the **EP2** handler
`0x0046` keyed on `[0x0A]`, which the preceding **EP4 `0x02` (suffix `0x68`,
op `>=0x80`)** set. The CLI's `rom_program_chunk` already sends the payload on
EP2 but *also* sends an EP4 `cmd 0x04` first; `0x0A82` does not touch `[0x0A]`,
so that is unlikely to be the blocker. The remaining suspects are the erase op
(`0x29` vs the real sector-erase) and whether the program payload address is the
raw byte address or word address, plus whether a CPLD unlock must precede it.

## Artifacts

Repo (`docs/captures/`):
- `ezclient_startup_trace.txt`, `ezclient_startup.pcap` — real EZClient session.
- `native_cartread_trace.txt`, `native_sessioninit_trace.txt`,
  `native_readreg_trace.txt` — native CLI transfers.

Local scratch (`C:\Users\yoshi\AppData\Local\Temp\opencode\`):
- `cart_backup\backup_bank0.bin` — 64 KB backup of the (EZLoader) bank-0 sector.
- `cart_backup\test_64k.bin` — the test image used for the failed Burn.
- `captures\*` — all pcaps (rh2.pcap is the EZClient one; others are host probes).
- `ezdata.vhd` — the VM data disk (app + driver + setup scripts).

## Open questions / next step

1. Capture EZClient actually performing an **erase** and then a **program**
   (needs a UI click) and diff the packets against ours.
2. Confirm the correct erase op / sequence (the current `0x29` sector erase did
   nothing).
3. Confirm whether the program payload address is byte or word addressed.
4. Confirm whether a CPLD unlock must precede the program.

## EZClient "Burn" capture attempt (no write issued)

Captured (host USBPcap2) a full EZClient session while clicking Burn.
Trace: `docs/captures/ezclient_burn_click_trace.txt` (4330 frames, ~30 s).

- EP4 command histogram: `0x19` ×1858, `0x1A` ×152, `0x05` ×1.
  **No `0x02`, `0x03` or `0x04`** — no flash/ROM write command was sent.
- So Burn was a no-op: EZClient never entered its write path. The ROM list
  showed only `C EZ2 256M` (no child ROM entries), so there was nothing to
  burn. An `Open ROM` of a hand-made 64 KB `test.gba` printed "Add rom" in the
  Output list but did not add a burnable entry — likely the header is not a
  valid GBA ROM (only the 8-byte logo prefix was set, not the full 156-byte
  Nintendo logo / checksum), and/or the cart's multi-game directory no longer
  enumerates after the earlier experimental reads.

### To actually capture the write

1. Stage a **valid** GBA ROM (full 156-byte Nintendo logo + correct header
   checksum) in the guest, load it with Open ROM, select it, then Burn.
2. Or get EZClient to enumerate the cart's existing ROMs again (the list was
   non-empty on the first-ever run: `EZLoader`, `Pokemon Shiny Gold`) and Burn
   one of those.

Until a burn actually starts, there is no write packet to capture.

## Follow-up: valid ROM loaded, still no write

A valid GBA program (the cart's own bank-0 dump, `ezloader.gba`) was loaded with
Open ROM — EZClient listed it as `R EZLoader` and showed its info. Clicking Burn
then opened a **"Backing up Rom"** dialog (progress 100%, CANCEL) and stalled.

Capture `docs/captures/ezclient_burn_validrom_trace.txt` (host USBPcap2):

- EP4 command histogram: `0x19` ×4387, `0x1A` ×500, `0x05` ×1.
- **Still no `0x01`, `0x02`, `0x03` or `0x04`.**
- The `0x1A` reads are only tiny addresses (4–7, 0–3, and `0xC00000..3`) — no
  sequential ROM reads, so the "backup" step never actually read the ROM.

Conclusion: EZClient enumerates the cart (`C EZ2 256M`) and accepts a ROM, but its
burn/backup path does not drive the writer in this environment (repro/multicart
plus the CPLD/firmware state left by the earlier diagnostics). No write packet is
produced, so the ROM-write protocol could not be captured here.

### Recommended next step

Run the capture on a **standard, clean EZ-Flash II cartridge** (not this
multicart repro) in the VM, or reset the writer to the vendor firmware by
replugging to bootloader and letting the guest's `ezwinit.sys` load it (rather
than the CLI `init-exact` loader tables). Either may let EZClient reach its write
path so the `0x02`/`0x04` packets can be captured and diffed.

## SOLVED: captured a successful Burn — the real write protocol

The blocker was the firmware: EZClient's write path only engages with the
**vendor firmware**. After replugging to bootloader and letting the guest's
`ezwinit.sys` load it (captured: `docs/captures/ezwinit_firmware_load_trace.txt`,
device `0547:2131` → `0548:1005`), EZClient burned the ROM successfully (the cart
then showed two `EZLoader` entries).

Capture: `docs/captures/ezclient_successful_burn_trace.txt` (11058 frames) and the
compact command/​payload list `docs/captures/ezclient_write_sequence.txt`.

Endpoints: **EP2 OUT** = payload (4096-byte transfers), **EP4 OUT** = commands,
**EP4 IN `0x84`** = replies.

Command sequence (EP4 unless noted):

```
05                       ; begin (1 byte)
01 00 00 00              ; read (cmd 0x01) at 0x0000
01 00 40 00, 01 00 80 00, 01 00 c0 00, 01 00 00 01, ...   ; 16 KB steps
02 00 00 02 67           ; flash op: byte3=0x02, byte4=0x67
04 00 00                 ; address stage (cmd 0x04)
EP2 OUT 4096 x 8         ; 32 KB of 0x00
02 00 00 00 67           ; program: byte3=0x00, byte4=0x67
EP2 OUT 4096 x 8         ; real ROM data (2e 00 00 ea 24 ff ae 51 ...)
02 00 00 40 67           ; byte3=0x40
EP2 OUT 4096 x 8         ; data then 0xFF padding
02 00 00 80 67           ; byte3=0x80
EP2 OUT 4096 x 8         ; 0x00
...                      ; byte3 = 00,40,80,c0,01,41,81,c1,...
06                       ; end (1 byte)
```

### Key deltas vs the current `rom-write`

1. The program command is `cmd 0x02` with **byte4 = `0x67`** (the firmware's
   bank/page engine at `0x0517`), **not** `0x68`/op `0xA0`.
2. The payload is **4096 bytes** on EP2 OUT, **not 64**.
3. Byte 3 (`0x00,0x40,0x80,0xC0,0x01,...`) is the **page/op selector**, not a
   program-setup op.
4. `cmd 0x05` begins and `cmd 0x06` ends the session.
5. No `0x29` sector-erase or `0xA0` program-setup packet was observed.

This is why every previous `rom-write` attempt wrote nothing: it used the wrong
op path (`0x68`) and a 64-byte payload. Porting the above (`0x67` + 4 KB EP2
payloads + `0x05`/`0x06`) is the correct fix.




## Update (end of day): write works natively

The port above still failed on hardware (EP2 payload NAK, then a wedged
writer). The decoded sequence was incomplete:

1. `ezclient_write_sequence.txt` only kept `0x01/0x02/0x04/0x05/0x06` and EP2.
   The full burn also has **3054 `0x19` bus writes** and **256 `0x1a` status
   reads**: EZ-Flash CPLD unlock (`D200`/`1500`), Intel flash `60`+`D0` block
   unlock, `70` status, `50` clear, `FF` read array, erase, and `60`+`01`
   re-lock of all 540 blocks at the end. Full stream:
   `captures/ezclient_burn_full_stream.txt`.
2. Correction to item 3: in `02 00 <b2> <b3> 67`, **byte 2** is the 32 KB page
   (`00/40/80/C0`) and **byte 3** the upper step (`00/01`); `02` in byte 3 is
   the preamble.
3. `0x1a` replies on EP 0x84; byte0 bit7 = flash ready. They must be polled.

Fix (commit `659b442`): `tools/gen_burn_script.py` -> `captures/ezclient_burn_script.txt`,
replayed by CLI `rom_write_ez` and GUI `write_rom` with the ROM data swapped in.
Read-back (`rom_read_ez`, `rom-verify` in `a6775be`) streams EP 0x82 after one
`01 00 00 00`.

Hardware result (VERIFIED): 64 KB write + verify passed three times (original,
`0xF000` `FF`->`5A`, original). The cart is back on the EZLoader image.
