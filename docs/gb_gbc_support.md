# Game Boy / Game Boy Color ROM support

## What the feature is

A GBA cannot execute a Game Boy ROM. "GBC/GB support" means wrapping a GB/GBC
ROM with the **Goomba** emulator so the combined image runs on the GBA flash
cart — which is exactly what EZClient's `NES/PCE/GBC/GB ROM Support` tab does.
It is not a cartridge dumper.

The loader ships as `Sysbin\goomba.gba` in the original client: 43,608 bytes, a
valid GBA header, title `GOOMBAGOOMBA` and game code `GMBA`.

## The container format

Goomba and Goomba Color share one container, documented at
<https://lakora.us/gba/goomba/>:

> raw Game Boy ROMs are concatenated to the end of the emulator ROM, and this
> combined image can then be run on a GBA.

So the image is simply `goomba.gba || gb_rom`. Goomba locates the payload at
runtime — it carries the GB logo fragment `CE ED 66 66` at `0xc14` and validates
a GB header — so no offsets need patching.

On top of the concatenation, `device::wrap_gb_rom` replaces the GBA header title
(`0xA0..0xAC`) with the GB game's title and recomputes the header checksum at
`0xBD`, matching FluBBa/Dwedit's `goombafront.exe`, whose symbol table carries
`loader`, `oldheader`, `newheader` and `Title`. That field is cosmetic to Goomba
but identifies the cartridge to the console menu.

The **game code is left as `GMBA`** deliberately: `CRomManager::SpecialRomPatch`
keys its per-loader fixups on those codes (`GMBA`, `PNES`, `PCEA`, `FCA`), so
changing it would lose the loader identification.

Header checksum, per the GBATEK header notes: start at 0, subtract every byte of
`0xA0..=0xBC`, then subtract a further `0x19`. Verified against the unmodified
loader, whose stored checksum is `0xD0`.

## Using it

The loader ships in `firmware/goomba.gba` (43,608 bytes, taken from the EZ Client
install — Goomba is Dwedit's freely redistributable homebrew, and EZClient
bundled it too). The lookup checks beside the executable, `<exe>/firmware`, the
working directory and `firmware/`.

Select a `.gb` or `.gbc` file on the Burn section. The tool wraps it, writes
`<name>.goomba.gba` next to the source ROM, reports the sizes, and uses that
image as the write source — the rest of the burn path is unchanged.

The wrapped image is written beside the source ROM rather than to `%TEMP%`,
because the process can be denied access outside its own tree (`os error 5`).

## Verified

Wrapping Pokemon Yellow (GB, 1,048,576 bytes) and Pokemon Crystal (GBC,
2,097,152 bytes, CGB flag `0xC0` at `0x143`) with the shipped loader:

```
image 1092184 bytes = goomba 43608 + rom 1048576   True
GB ROM appended verbatim                           True
GBA logo intact                                    True
GBA title                POKEMON YELL
game code                GMBA
header checksum          stored=CB computed=CB     True
GB header intact at its offset                     True

Pokemon Crystal     2140760 bytes = 43608 + 2097152  True
GB ROM appended verbatim                             True
GBA title                PM_CRYSTAL
header checksum          stored=DC computed=DC       True
```

Not yet done: **booting the result on real hardware**. The writer currently
holds a Game Boy cartridge, so the burn and a read-back verify are still
outstanding.
