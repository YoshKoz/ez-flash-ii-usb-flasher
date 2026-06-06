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
- [ ] Test with the physical EZ-Writer II attached:
  - [ ] `ezwriter-cli list`
  - [ ] `ezwriter-cli firmware-download tusbez.bin`
  - [ ] `ezwriter-cli cart-info`
  - [ ] `ezwriter-cli dump test.gba`
  - [ ] `ezwriter-cli save-read 0 2048 --output test.sav`
- [ ] Attach screenshots or terminal output to the Reddit post if available.
- [ ] Rename the GitHub repo slug if desired.

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
- Upload the included 8051 firmware to the Cypress AN2131
- Read cartridge headers
- Dump GBA ROMs
- Back up save files
- Restore save files
- Use either a CLI or GUI

ROM writing and erase support are still experimental and risky, so I recommend treating this first public release as a preservation/read-only tool unless you already know what you are doing and have backups.

Repo:
https://github.com/YoshKoz/ezwriter-reverse

I made this because the original hardware is still useful, but the official software and drivers are stuck in the Windows XP era. I am proud that this old device can now be used from a modern setup again.

If you have an EZ-Writer II / EZ-Flash II USB writer, testing feedback would help a lot. Please back up your carts and saves before trying write operations.
```
