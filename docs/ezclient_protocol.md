# EZClient 3.26 Host Protocol — Recovered by Disassembly

Source: `EZC326.7Z` (filetrip id 3645) -> `setup.exe` (InnoSetup) -> extracted
`EZ Client\EZClient.exe` (905,216 bytes, PE32/MFC, 2006-06-10).

This is the authoritative host-side write protocol the firmware `tusbez.bin`
expects. It was recovered statically; no USB capture was needed.

## Device open

`EZClient.exe` opens the kernel driver with `CreateFileA`:

- `\\\\.\\Ezw-0` (string at VA 0x4b1684) — the EZ-Writer (0548:1005).
- `\\\\.\\ez321000` (VA 0x4b1674) — EZ-Writer3 fallback.

If `Ezw-0` opens, an **endpoint profile** is selected (function 0x422830).

## Endpoint profiles (VA 0x422883 / 0x4228B0)

Fields are byte offsets in the device object:

| Field | Meaning |
|-------|---------|
| +0x3440 | command endpoint index |
| +0x3441 | read / data-in endpoint index |
| +0x3442 | write / data-out endpoint index |
| +0x3443 | auxiliary endpoint index |
| +0x3444 | max packet size (u16) |

| Profile | Selected when | +3440 cmd | +3441 read | +3442 write | +3443 aux | max pkt |
|---------|---------------|-----------|------------|-------------|-----------|---------|
| A | `ez321000` opens | 2 | 1 | 0 | 3 | 0x10 (16) |
| **B** | **`Ezw-0` opens** | **3** | **8** | **1** | **0xa** | **0x40 (64)** |

For the EZ-Writer (`\\\\.\\Ezw-0`, max packet 64) the client uses **Profile B**.
The indices are driver-side endpoint numbers; on the wire the command endpoint
is **EP4** and the data endpoints are **EP2/EP81** (confirmed empirically — a
`0x01` read sent to EP4 returns correct cartridge bytes on EP2 IN, while
EP2/EP6 return stale buffer contents).

## Transfers

All transfers go through one `__thiscall` wrapper at **VA 0x422110**, which
calls `DeviceIoControl(hDevice, ioctl, inBuf, inLen, outBuf, outLen, &ret, NULL)`.

Driver IOCTLs observed (see `original_driver_analysis.md` for the full set):

| IOCTL | Role |
|-------|------|
| `0x222051` | bulk OUT — send a command/payload block |
| `0x22204e` | bulk OUT — send a 2nd stream / data block |
| `0x222035` | bulk OUT (large block, used for >0x8000 transfers) |

Command blocks are built at `device + 4`:
- byte `+4` = command code
- bytes `+5..` = parameters (address etc.)

## Command set used by the client

Scanning for `lea reg,[base+4]` followed by `mov byte [reg], imm` yields the
complete command list:

| Cmd | Helper VA | Notes |
|-----|-----------|-------|
| `0x02` | 0x4213DE, 0x421A2C, 0x424916, ... | read (several variants) |
| `0x03` | 0x4223A0 | **write**: 3-byte header + data on `+3441` |
| `0x04` | 0x422400 | **ROM write**: 3-byte header + data on `+3442` |
| `0x05` | 0x4225B0 | |
| `0x06` | 0x452E20 | |
| `0x0F` | 0x421A70 | 2-byte param |
| `0x10` | 0x421B40 | |
| `0x11` | 0x421B20 | |
| `0x14` | 0x424C28, 0x424FD8, ... | select / register |
| `0x1D` | 0x4243B0 | 6-byte block |
| `0x1E` | 0x4243E0 | |

## ROM write — exact sequence (VA 0x422400)

```
mov cx, <word_addr>          ; 16-bit address argument
mov dl, [dev+0x3440]         ; command endpoint index
mov word [dev+5], cx         ; place address into header bytes 1..2
mov byte [dev+4], 0x04       ; command code
DeviceIoControl(h,(void*)dev+4, len=3, ioctl=0x222051, ep=cmd)   ; [04, lo, hi]
Sleep(5)
DeviceIoControl(h, data, len=<len>, ioctl=0x222051, ep=[dev+0x3442])  ; payload
```

Key points that explain earlier failed tests:

1. **The address is a word address** (`byte_addr / 2`) — reads and writes both.
2. The header is exactly **3 bytes** `[0x04, addr_lo, addr_hi]`.
3. A **5 ms sleep** separates header and payload.
4. The payload uses **the same IOCTL `0x222051`** as the header (not `0x22204e`),
   but is addressed to the **write endpoint index `+0x3442`** — a different
   endpoint from the command.
5. The firmware then runs the program loop at `0x068B` (count from `0x7FC9`,
   payload at `0x7DC0`), which matches the RE'd firmware exactly.

## Save write — VA 0x4223A0

Identical shape but command `0x03` and the payload goes to `+0x3441`.

## Multi-game / loader files (from the same archive)

`Sysbin\` contains the loader assets the client writes:

| File | Size | Note |
|------|------|------|
| `EZLoader2.bin` | 37,836 | multi-game menu loader (ARM) |
| `EZLoader_GBA.bin` | 42,436 | GBA loader variant |
| `ez_flash.bin` | 95,608 | main flash image |
| `ezback_LZ.bin` | 11,600 | LZ-compressed background |
| `ezlogo_LZ.bin` | 11,924 | LZ-compressed logo |
| `bb.bin` | 37,868 | |

`patchDLL.dll` exports `CRomManager` with the ROM tooling:
- `?InflateROM@CRomManager@@QAEXPAPAEPAK@Z` — **the LZ decompressor**
- `?SaverPatch@CRomManager`, `?GetSaverTypeAndSize@CRomManager`,
  `?TrimRom@CRomManager`, `?RemoveIntro@CRomManager`, `?HeaderValid@CRomManager`

These give the multi-game LZ codec and saver metadata needed for the writer.

## Status

- Command set, endpoint roles, ROM-write and save-write sequences: **recovered
  by disassembly**.
- The one remaining check is confirming the driver endpoint index -> wire
  endpoint mapping on real hardware (cmd index 3 -> EP4 confirmed; write index 1
  -> EP2/EP81 to confirm).


## Files and tools (local)

Extracted client (keep — needed for any re-analysis):

- `C:\Users\yoshi\AppData\Local\Temp\opencode\ezclient_dl\EZC326.7z` (2,930,205 bytes)
- `...\ezclient_dl\extracted\setup.exe` (3,079,041 bytes)
- `...\ezclient_dl\innosetup\EZ Client\` — full install tree
  - `EZClient.exe` (905,216) — host protocol
  - `patchDLL.dll` (81,920) — `CRomManager`: LZ, saver, trim
  - `USB_Drivers\ezwriter.sys`, `ezwinit.sys`, `ezwrit3.sys`, `tusbez.bin`, `TUSBEZ3.BIN`
  - `Sysbin\EZLoader2.bin`, `EZLoader_GBA.bin`, `ez_flash.bin`, `ezback_LZ.bin`,
    `ezlogo_LZ.bin`, `bb.bin`

Analysis scripts written this session:

- `tools/scan_ezclient_ioctl.py` — DeviceIoControl call sites + IOCTL immediates
- `tools/scan_ezclient_cmds.py` — enumerates command helpers (`lea reg,[base+4]` + `mov byte [reg],imm`)

To finish on hardware, run from a normal terminal:

1. `ezwriter-cli flash-probe <byteaddr> --cmd 0x04 --cmd-ep 4 --ep <N> --word-addr --payload <64-byte-hex>`
   for the ROM write, trying `N` in {2, 1, 8} for the payload endpoint.
2. Read back with `cart-read <byteaddr> 1` (it already word-addresses).

Session note: the last run lost shell access (the tool catalog no longer exposes a
shell), so the Rust CLI and Python tooling could not be executed from that session.
Run them from a normal terminal.
