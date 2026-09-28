# Public Release Notes

## Recommended Public Name

Project name: **EZ-Flash II USB Flasher**

Recommended GitHub repository slug: `ez-flash-ii-usb-flasher`

Keep the binaries named `ezwriter-cli` and `ezwriter-gui` for now because the
hardware is commonly recognized as EZ-Writer II and existing instructions already
use those command names.

## Release Checklist

- [x] CLI builds and tests pass on Linux.
- [x] GUI builds and tests pass on Linux.
- [x] Clippy passes for CLI and GUI with warnings denied.
- [x] README explains install, firmware upload, ROM dump, save dump, GUI, and safety.
- [x] Write operations are documented as risky.
- [ ] Test on Windows 10/11 with Zadig WinUSB installed.
- [x] Test with the physical EZ-Writer II attached (tested via SSH to Windows desktop):
  - [x] `ezwriter-cli list` — detected ACTIVE mode 0548:1005
  - [ ] `ezwriter-cli firmware-download tusbez.bin` — was already active, couldn't test
  - [x] `ezwriter-cli cart-info` — read POKEMON SAPP (AXPE) header correctly
  - [x] `ezwriter-cli dump test.gba` — 64KB dump with valid GBA magic
  - [ ] `ezwriter-cli save-read 0 2048 --output test.sav` — not yet tested
- [ ] Attach screenshots or terminal output to the Reddit post if available.
- [x] Rename the GitHub repo slug to `ez-flash-ii-usb-flasher`.

## v0.2.0 Checklist

Everything below was done against the physical writer and cartridge during this cycle.

- [x] `cargo fmt --check`, `clippy -D warnings`, `cargo test` clean (37 tests).
- [x] Full-cartridge writes: 256 KB, 512 KB, 1.45 MB and 16 MB burned and verified.
- [x] Read-back wrap at 128 KB found and fixed (per-32 KB re-arm, 20 ms settle).
- [x] `rom-verify` saves `<input>.readback.bin` on mismatch instead of only reporting it.
- [x] Trim ROM verified on hardware (1048576 -> 786432 bytes).
- [x] IPS patch verified on hardware (2 records, 20 bytes, RLE + literal).
- [x] *Skip erase* verified by writing `0xFF` over `0x5A` and confirming the bits stayed set.
- [x] *Reset Cartridge Flash* fixed (was a pre-capture command form, and opened without claiming).
- [x] Eject Cartridge Safely verified; the 8051 is deliberately left running.
- [x] SDK save-library detection, agreeing with GAME_DB on FireRed (`FLASH 128K`).
- [x] Game Boy ROM wrapping: Pokemon Yellow burned, read back verified, **boots on an original GBA**.
- [x] Game Boy Color wrapping byte-verified (Pokemon Crystal) — not yet booted on hardware.
- [ ] Boot a GBC image on real hardware.
- [ ] Read a Goomba save back off the cartridge (saves live in the GBA save area, not the ROM).

## v0.1.2 Checklist

Automated gates, run locally and in CI:

- [x] `cargo fmt --check`
- [x] `cargo clippy -- -D warnings`
- [x] `cargo test` — 21 tests (7 CLI, 14 GUI)
- [x] `cargo build --release -p ezwriter-cli -p ezwriter-gui`
- [x] CLI smoke test with no device attached: `list`, `dump`, `bench`, `save-id` all
      fail cleanly, and a failed `dump` leaves **no** file behind.

## Open items

Still to confirm on the physical EZ-Writer II:

- [ ] `ezwriter-cli bench` — per-chunk latency and the pipeline depth this
      hardware actually sustains (`docs/dump_performance.md` is unmeasured without it)
- [ ] `ezwriter-cli dump <cart>.gba` — confirm the auto-detected ROM size matches
      the cartridge and that the dump completes and verifies
- [ ] `ezwriter-cli save-id` — confirm a retail cart reports Macronix (0xC2) or Sanyo (0x62)
- [ ] `ezwriter-cli save-read -t f --output <cart>.sav` — confirmation and the
      14-signature validation on a real save

## Reddit Post Draft

Title:

```text
I released EZ-Flash II USB Flasher, an open-source modern tool for the EZ-Writer II GBA flasher
```

Body:

```text
Hi everyone,

I built and released EZ-Flash II USB Flasher, an open-source replacement for the old Windows XP-era EZ-Writer II / EZ-Flash II USB software.

It lets you use the original USB flasher on modern Windows, Linux, and macOS without the old unsigned kernel drivers. The project uses Rust, libusb, and WinUSB.

What works now:

- Detect the EZ-Writer II
- Upload the original 8051 firmware (pulled from your existing driver install) to the Cypress AN2131
- Read cartridge headers
- Dump GBA ROMs
- Back up save files
- Restore save files
- Use either a CLI or GUI

ROM writing and erase support are still experimental and risky, so I recommend treating this first public release as a preservation/read-only tool unless you already know what you are doing and have backups.

Repo:
https://github.com/YoshKoz/ez-flash-ii-usb-flasher

I made this because the original hardware is still useful, but the official software and drivers are stuck in the Windows XP era. I am proud that this old device can now be used from a modern setup again.

If you have an EZ-Writer II / EZ-Flash II USB writer, testing feedback would help a lot. Please back up your carts and saves before trying write operations.
```
