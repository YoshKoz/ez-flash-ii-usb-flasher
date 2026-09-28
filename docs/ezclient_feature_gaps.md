# EZClient feature gaps (what the original has that we don't)

Evidence for each item is from the extracted 2006 client at
`C:\Users\yoshi\AppData\Local\Temp\opencode\ezclient_dl\innosetup\EZ Client\`
(`EZClient.exe` 3.26, `patchDLL.dll`, `data\`, `Sysbin\`).

## Implemented in this port

| Feature | Where |
|---|---|
| Trim ROM | `device::trim_rom_padding` + Burn checkbox |
| IPS patch | `device::apply_ips` + Burn file picker |
| Read-array reset | `device::reset_jedec` (`0x19` bus write, `0xFF`) |
| Skip erase | `build_burn_script(.., skip_erase)` + Burn checkbox |

## Not implemented, and why

### Saver Patch / Special ROM patch (incl. "Soft Reset")

`patchDLL.dll` exports these by mangled name, so their signatures are exact:

```
?SaverPatch@CRomManager@@QAEKPAPAEPAKW4EZCARTTYPE@@IPAW4SAVE...
    SaverPatch(unsigned char**, unsigned long*, EZCARTTYPE, unsigned int, SAVERTYPE*)
?SpecialRomPatch@CRomManager@@QAEKPAPAEPAKW4EZCARTTYPE@@K@Z
    SpecialRomPatch(unsigned char**, unsigned long*, EZCARTTYPE, unsigned int)
?Modify1MSaverRom@CRomManager@@QAEXPAPAEPAKI@Z
    Modify1MSaverRom(unsigned char**, unsigned long*, unsigned int)
?GetSaverTypeAndSize@CRomManager@@QAEKPAPAEPAKPAW4SAVERTYPE@
?GetSaverSpacial@CRomManager@@QAEEPAE@Z
?ApplyIPSPatch@CRomManager@@QAEKPBDPAEK@Z
?TrimRom@CRomManager@@QAEXPAPAEPAH@Z
```

These are **compiled routines that rewrite the ROM image in memory**, keyed on
`EZCARTTYPE` (cart type), the ROM length, and the **detected** `SAVERTYPE` — not
a per-game data table. Checks that came back negative:

- No GBA game code (`BPRE`, `BPEE`, `AXVE`, `AXPE`, `BPGE`, `AGSE`) appears as a
  string literal in `patchDLL.dll` or `data\index.rec`.
- None appears as a little-endian 32-bit immediate either (which is how a
  `memcmp(rom+0xAC, "BPRE", 4)` would encode it), in `patchDLL.dll` or
  `EZClient.exe`.
- `data\index.rec` (356 KB) and `langdata\romname.lst` are ROM-name/cheat
  indexes; the `data\*.cht` files are GoldenFinger cheat lists, not patches.
- The only saver-type literals are `FLASH`, `FLASH1M`, `EEPROM`, `SRAM` around
  `patchDLL.dll+0x11270`.

So the patch bytes are **logic inside `patchDLL.dll`**, and there is no IDA
database for that DLL — only `EZClient.exe.i64` and
`USB_Drivers\ezwriter.sys.i64` ship with the client.

Implementing these faithfully therefore means disassembling `CRomManager`'s
patch routines from scratch and validating the result against real cartridges.
Guessing at per-game byte patches would produce something that looks plausible
and silently corrupts a game, so this is deliberately left undone rather than
approximated. `Modify1MSaverRom` is the most tractable entry point if it is
ever picked up: it is narrow (1 Mbit saver ROMs) and named for exactly what it
does.

### Other systems (NES / PCE / GBC / GB)

`Sysbin\` ships the loaders — `pocketnes.gba`, `pceadvance.gba`,
`EZNDSLoader.nds.gba`, `goombafront.exe`, `ez_flash.bin` — so support is a
matter of writing those plus a different loader table per system, not a write
protocol change. Not started.

### GoldenFinger / Password

`data\*.cht` is a cheat database keyed by a numeric id. Out of scope for the
flasher's write path.

### Multi-game / ROM Lists

The original's `ROM Lists` pane holds multiple ROMs with a loader menu. Our
sidebar shows the one inserted cartridge. The layout side is in
`docs/multi_game_format.md`; the write side would need an offset write, which
`write_rom` still refuses (`byte_addr != 0`).
