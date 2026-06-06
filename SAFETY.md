# Safety Guide: EZ-Flash II USB Flasher

## Risk Levels

| Operation | Risk | Notes |
|-----------|------|-------|
| `list` | **None** | Read-only descriptor enumeration |
| `info` | **None** | Read-only USB descriptor |
| `firmware-download` | **Low** | RAM firmware only; unplugging resets it |
| `identify` / `cart-info` | **Low** | Read-only command to cartridge |
| `dump` | **Low** | Read-only ROM dump |
| `save-read` | **Low** | Read-only save data |
| `write-save` | **Medium** | Save data write; back up first |
| `write-rom` | **HIGH** | Experimental; can brick cart if interrupted |
| `erase` | **HIGH** | Destructive - wipes cartridge |

## Recovery Options

### Bricked Cartridge
- If write is interrupted, the cartridge may enter an indeterminate state.
- Recovery: Re-run `write-rom` with a known-good ROM (do NOT erase first).
- The EZ-Flash II has a bootloader in ROM that can usually recover from partial writes.

### Bricked Writer
- The EZ-USB firmware is stored in RAM only. Unplug and replug = factory reset.
- The ONLY exception is if you flash the EEPROM (not supported by this tool).
- **Never write to EEPROM unless you have a backup programmer.**

### Corrupted Driver
- Uninstall via Device Manager, replug device.
- Use Zadig to reinstall WinUSB.

## Before Any Write Operation

1. **Always dump ROM first**: `ezwriter-cli dump backup.gba`
2. **Always dump save first**: `ezwriter-cli save-read 0 2048 --output backup.sav`
3. **Verify the dump**: check file size and confirm it is not all `00` or all `FF`
4. **Keep the backup somewhere safe** before erasing or writing
5. **Do not write from a USB hub** if you can plug the writer directly into the PC

## Pre-flight Checklist

- [ ] Device shows in `ezwriter-cli list`
- [ ] Firmware loaded if the device starts in bootloader mode
- [ ] Cartridge detected by `ezwriter-cli cart-info`
- [ ] Current ROM backup exists
- [ ] Current save backup exists
- [ ] Backup sizes look plausible
- [ ] No other USB device is using the same VID/PID
- [ ] Laptop or PC will not sleep during the operation

## Windows Driver Signing Policy

This project uses WinUSB (Microsoft-signed inbox driver). No kernel-mode code
is loaded from this project. The INF file is NOT signed, which means:

### Option 1: Test Signing (Default for Dev)
```powershell
bcdedit /set testsigning on
# Reboot, install driver, test, then:
bcdedit /set testsigning off
# Reboot
```
The watermark "Test Mode" will appear on desktop while enabled.

### Option 2: Zadig (No watermark)
Zadig creates signed driver catalogs automatically.

### Option 3: Microsoft Attestation Signing (For Distribution)
- Register on Windows Hardware Developer Center
- Use Windows Hardware Lab Kit (HLK) for signature
- Submit for Microsoft signature
- This produces a properly signed catalog file

## Known Risks and Limits

1. **Voltage/current**: The original writer operates at USB spec. No risk.
2. **Short circuit**: Standard USB cable. Don't use damaged cables.
3. **Electrostatic discharge**: Touch grounded metal before touching cart.
4. **USB cable length**: Keep under 2m for reliable operation.
5. **Power**: The EZ-Flash cart draws power from the writer. Don't chain hubs.
6. **ROM write support**: Still experimental. Prefer read-only use unless you are prepared to recover.

## If Something Goes Wrong

1. **Don't panic.** Most issues are recoverable.
2. **Unplug the device.** This resets the EZ-USB firmware.
3. **Replug after 5 seconds.**
4. **Run `ezwriter-cli list`** to confirm detection.
5. If the cartridge is stuck, try the original XP setup as recovery.
6. As a last resort, use a GBA with flashcart writing capability to reflash.
