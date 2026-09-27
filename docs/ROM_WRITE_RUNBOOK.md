# ROM Write — Finish Runbook

The protocol is fully recovered (see `docs/ezclient_protocol.md`). Only hardware
confirmation is left. Run these from a normal terminal (PowerShell) in:

    C:\Users\yoshi\AppData\Local\Temp\opencode\ezf

## 0. Prereqs

- EZ-Writer plugged in, firmware loaded:
  ```
  .\target\release\ezwriter-cli.exe list
  ```
  If it says "No EZ-Writer device found", plug in the writer and run `reload`.
  If it says "Bootloader mode", run `reload`.

## 1. Pick a safe blank target

Byte address `0xA600` (bank 0, blank on the test cart). Confirm:
```
.\target\release\ezwriter-cli.exe cart-read 42496 1
```
Expect 64 x FF.

## 2. The ROM write (client-exact sequence)

Command header is `[0x04, addr_lo, addr_hi]` with a **word address**, then a 5 ms
gap, then the 64-byte payload on a **separate endpoint**. Try payload endpoints in
this order: **2, then 1, then 8** (the client uses write-index 1; index->wire is
the one unknown). Use `--word-addr` so 0xA600 is sent as 0x5300.

```
$pat = "11181f262d343b424950575e656c737a81888f969da4abb2b9c0c7ced5dce3eaf1f8ff060d141b222930373e454c535a61686f767d848b9299a0a7aeb5c0c7ce"

.\target\release\ezwriter-cli.exe flash-probe 42496 --cmd 0x04 --cmd-ep 4 --ep 2 --word-addr --payload $pat
.\target\release\ezwriter-cli.exe cart-read 42496 1        # look for 11 18 1f 26 ...
```

If not written, repeat with `--ep 1`, then `--ep 8`.

## 3. If it writes

Fold the working sequence into `cmd_rom_write` in
`src\ezwriter-cli\src\main.rs` (replace `rom_program_chunk`), then:

```
cargo build --release -p ezwriter-cli
.\target\release\ezwriter-cli.exe rom-write pattern64.bin 42496 --no-erase --verify
```

## 4. If it still does not write

The remaining variable is the driver's endpoint-index -> wire-endpoint map.
Disassemble `ezwriter.sys` (in
`...\ezclient_dl\innosetup\EZ Client\USB_Drivers\`) — it maps the client's
index to the physical endpoint. Look for the IOCTL `0x222051` handler and how it
selects the endpoint number.

## Notes

- Every failure mode observed so far is non-destructive on the safe `0x04` path;
  only `0x69` and EP6 payloads wedge the firmware (needs a replug + `reload`).
- The read path already word-addresses, so `cart-read` is always correct.
- The extracted original client lives in
  `C:\Users\yoshi\AppData\Local\Temp\opencode\ezclient_dl\innosetup\EZ Client\`.
