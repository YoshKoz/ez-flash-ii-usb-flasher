# Device Inventory: EZ-Writer II / CNFlash / EZ-Flash II USB Writer

## Device Identity

| Field | Value |
|-------|-------|
| USB Vendor ID | `0x0547` (Anchor Chips / Cypress Semiconductor) |
| USB Product ID | `0x2131` |
| Revision | `0x0004` |
| Hardware IDs | `USB\VID_0547&PID_2131&REV_0004` |
| | `USB\VID_0547&PID_2131` |
| Manufacturer String | (not yet captured via descriptor) |
| Product String | (not yet captured via descriptor) |
| Chipset | Cypress EZ-USB AN2131Q (FX2 family, 8051 core) |
| USB Class | Vendor-specific |
| Compatible IDs | Cypress EZ-USB Bootloader / "ReNumeration" mode |

## USB Descriptors (from INF)

To collect full descriptors:
1. Install [USB Device Tree Viewer](https://www.uwe-sieber.de/usbtreeview_e.html)
2. Run as Administrator
3. Plug in device
4. Right-click → Save device tree as text
5. Paste output into `captures/device_tree.txt`

Device currently appears in **initialization/bootloader mode**. After firmware download via kernel driver, it re-enumerates with:

## Post-Firmware Device Identity (EZ-Writer mode)

| Field | Value |
|-------|-------|
| USB Vendor ID | `0x0548` |
| USB Product ID | `0x1005` |
| Windows INF section | `EZWRITER` |
| Driver .sys | `ezwriter.sys` |

## Transition Flow

```
[Plug in] → VID 0547 PID 2131 → ezwinit.sys loads → 
  sends tusbez.bin (8051 firmware) via EZ-USB vendor commands → 
    device soft-resets → re-enumerates as VID 0548 PID 1005 → 
      ezwriter.sys loads → DeviceIoControl (\\.\Ezw-0) → EZClient.exe
```

## All Supported Hardware ID Pairs (from ezwriter.inf)

| Bootloader VID:PID | Boot Driver | App VID:PID | App Driver | Product |
|--------------------|-------------|-------------|------------|---------|
| `0547:2131` | ezwinit.sys | `0548:1005` | ezwriter.sys | EZ-Writer (this device) |
| `0547:2130` | ezwinit2.sys | `0548:2105` | ezwrite2.sys | EZ-Writer2 |
| `0451:2136` | ApLoader.sys + firmware | `0550:1007` | ezwrit3.sys | EZ-Writer3 |

## Driver Architecture

### Initialization Driver (ezwinit.sys)
- Source project: `D:\EZ_Writer3.0\ezloader\LIB\i386\`
- PDB: `ezwinit.pdb`
- Function: Download 8051 firmware blob to EZ-USB RAM via vendor control requests
- Firmware file: `tusbez.bin` (5584 bytes, raw 8051 code)
- After firmware loads, device re-enumerates

### Main Driver (ezwriter.sys)
- Source project: `D:\EzFlash\pfw_usbfx2\ezusbdrv\LIB\i386\`
- PDB: `ezusb.pdb`
- NT Device: `\Device\Ezw-0`
- Win32 Device: `\\.\Ezw-0`
- IOCTL interface (7 codes):
  - `0x00220007` — unknown (METHOD_NEITHER, no buffer)
  - `0x00222000` — unknown (METHOD_BUFFERED)
  - `0x00222035` — unknown (METHOD_OUT_DIRECT)
  - `0x00222051` — unknown (METHOD_OUT_DIRECT)
  - `0x00222054` — unknown (METHOD_BUFFERED)
  - `0x0022206D` — unknown (METHOD_OUT_DIRECT)
  - `0x00222074` — unknown (METHOD_BUFFERED)
- Implementation: thin wrapper around USB bulk/control transfers
- Driver size: 12,544 bytes (very small — confirms minimal abstraction)

### EZ-USB Firmware (tusbez.bin)
- Size: 5,584 bytes
- Format: raw Intel-8051 binary
- First instruction: `LJMP 0x1202` (8051 reset vector)
- Contains: USB interrupt handlers, bulk endpoint setup, GBA cartridge communication protocol

### Firmware Blobs (Sysbin/)
| File | Size | Purpose |
|------|------|---------|
| `EZLoader2.bin` | 37,836 | EZ-Flash II cartridge firmware (ARM) |
| `EZLoader_GBA.bin` | 42,436 | EZ-Flash GBA loader |
| `ez_flash.bin` | 95,608 | Main EZ-Flash cartridge firmware (encrypted/lz?) |
| `ezback_LZ.bin` | 11,600 | Background image (LZ compressed) |
| `ezlogo_LZ.bin` | 11,924 | Logo (LZ compressed) |
| `bb.bin` | 37,868 | Unknown (same size as EZLoader2, maybe backup) |

## File Inventory

### USB_Drivers/
| File | Size | Type |
|------|------|------|
| `ezwriter.inf` | 5,733 | INF setup file |
| `ezwinit.sys` | 14,494 | Initialization kernel driver |
| `ezwinit2.sys` | 14,720 | Init driver (v2) |
| `ezwriter.sys` | 12,544 | Main EZ-Writer kernel driver |
| `ezwrite2.sys` | 12,544 | Main EZ-Writer2 kernel driver |
| `ezwrit3.sys` | 12,672 | Main EZ-Writer3 kernel driver |
| `ApLoader.sys` | 21,376 | Chip-specific loader (EZ-Writer3) |
| `tusbez.bin` | 5,584 | EZ-USB 8051 firmware (this device) |
| `TUSBEZ3.BIN` | 6,979 | EZ-USB 8051 firmware (v3 device) |

### Application
| File | Size | Purpose |
|------|------|---------|
| `EZClient.exe` | 905,216 | Main GUI flasher v3.26 |
| `patchDLL.dll` | 81,920 | ROM patching (IPS, intro removal, saver) |
| `SDL.dll` | 221,184 | SDL library |
| `XTP8510Lib.dll` | 1,380,352 | CodeJock XTP UI toolkit |
| `XSP8000Lib.dll` | 577,536 | CodeJock XTP UI toolkit |
| `zlib.dll` | 53,248 | Compression |
| `unrar.dll` | 158,208 | RAR extraction |
| `ar2cht.exe` | 24,576 | AR -> CHT conversion |
| `EZ_Mode.exe` | 24,576 | Mode switcher |

## Next Actions

See `original_driver_analysis.md` for detailed driver RE.
See `protocol_notes.md` for USB protocol reverse engineering.
