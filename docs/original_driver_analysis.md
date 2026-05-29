# Original Driver Analysis: EZ-Writer II USB Flasher

## Source of Files

- Archive: `ezc326.7z`
- File: `setup.exe` (InnoSetup, extracted with 7-zip)
- Application: EZClient v3.26 (2006-06-10)
- Chipset: Cypress EZ-USB AN2131Q (FX2)

## INF File Analysis

File: `ezwriter.inf` (5,733 bytes)

### Hardware ID Mapping

```
[BDN]
%USB\VID_0547&PID_2131.DeviceDesc%=EZWINIT, USB\VID_0547&PID_2131
%USB\VID_0548&PID_1005.DeviceDesc%=EZWRITER, USB\VID_0548&PID_1005
%USB\VID_0547&PID_2130.DeviceDesc%=EZWINIT2, USB\VID_0547&PID_2130
%USB\VID_0548&PID_2105.DeviceDesc%=EZWRITE2, USB\VID_0548&PID_2105
%DESCRIPTION%=DriverInstall,USB\VID_0451&PID_2136
%USB\VID_0550&PID_1007.DeviceDesc%=EZWRIT3, USB\VID_0550&PID_1007
```

### String Definitions

```ini
[Strings]
BDN="EZ GROUP"
USB\VID_0547&PID_2131.DeviceDesc="EZ-Writer Initialization"
USB\VID_0548&PID_1005.DeviceDesc="EZ-Writer"
USB\VID_0547&PID_2130.DeviceDesc="EZ-Writer2 Initialization"
USB\VID_0548&PID_2105.DeviceDesc="EZ-Writer2"
DESCRIPTION="EZ-Writer3 Initialization"
FIRMWARE_FILENAME="TUSBEZ.BIN"
USB\VID_0550&PID_1007.DeviceDesc="EZ-Writer3"
```

### Driver Sections for This Device

**Initialization phase (at bootloader VID 0547:PID 2131):**
```ini
[EZWINIT]
CopyFiles=EZWINIT.Files.Ext, EZWINIT.Files.Inf

[EZWINIT.NT.Services]
Addservice = EZWINIT, 0x00000002, EZWINIT.AddService

[EZWINIT.AddService]
ServiceType    = 1                  ; SERVICE_KERNEL_DRIVER
StartType      = 2                  ; SERVICE_AUTO_START
ErrorControl   = 1                  ; SERVICE_ERROR_NORMAL
ServiceBinary  = %10%\System32\Drivers\ezwinit.sys
LoadOrderGroup = Base

[EZWINIT.Files.Ext]
ezwinit.sys
ezwriter.sys
```

**Application phase (at EZ-Writer VID 0548:PID 1005):**
```ini
[EZWRITER.NT.Services]
Addservice = EZWRITER, 0x00000002, EZWRITER.AddService

[EZWRITER.AddService]
ServiceType    = 1                  ; SERVICE_KERNEL_DRIVER
StartType      = 2                  ; SERVICE_AUTO_START
ErrorControl   = 1                  ; SERVICE_ERROR_NORMAL
ServiceBinary  = %10%\System32\Drivers\ezwriter.sys
LoadOrderGroup = Base
```

### EZ-Writer3 Special Case (ApLoader)

The EZ-Writer3 uses a different approach with explicit firmware download:
```ini
[DriverInstall.NT.Services]
AddService=APLOADER,2,DriverService

[DriverService]
ServiceType=1
StartType=3
ErrorControl=1
ServiceBinary=%10%\system32\drivers\ApLoader.sys

[DriverHwAddReg]
HKR,,FWFileName,,"TUSBEZ.BIN"
```

This stores the firmware filename in the registry so the driver can load it on startup. The original EZ-Writer (this device) hardcodes the firmware file handling in ezwinit.sys.

## PDB Paths

### ezwinit.sys
```
D:\EZ_Writer3.0\ezloader\LIB\i386\ezwinit.pdb
```
Project: `EZ_Writer3.0\ezloader` — firmware downloader for EZ-USB

### ezwinit2.sys
```
D:\EzFlash\USBdriver\newwriter_ezloader\LIB\i386\ezwinit2.pdb
```
Project: `EzFlash\USBdriver\newwriter_ezloader` — newer loader version

### ezwrit3.sys / ezwrite2.sys / ezwriter.sys
```
D:\EzFlash\pfw_usbfx2\ezusbdrv\LIB\i386\ezusb.pdb
```
Project: `EzFlash\pfw_usbfx2\ezusbdrv` — "Pico Flash Writer USB FX2 Driver"
All three are the same PDB — identical driver with different device names.

## Device Names

| Driver | NT Device Name | Win32 Path |
|--------|---------------|------------|
| ezwriter.sys | `\Device\Ezw-0` | `\\.\Ezw-0` |
| ezwrite2.sys | `\Device\Ezw-0` | `\\.\Ezw-0` |
| ezwrit3.sys | `\Device\Ez3210-0` | `\\.\Ez3210-0` |

Note: ezwriter.sys and ezwrite2.sys use the **same device name** but different VID/PID pairs. EZClient.exe opens `\\.\Ezw-0` regardless of which device is present.

## IOCTL Interface

The driver exposes 7 IOCTL codes via `DeviceIoControl`:

| IOCTL Code | Function | Method | Access | Notes |
|-----------|----------|--------|--------|-------|
| `0x00220007` | 1 | METHOD_NEITHER (3) | FILE_ANY_ACCESS (0) | Likely GET_DESCRIPTOR or GET_STATUS |
| `0x00222000` | 0x800 (2048) | METHOD_BUFFERED (0) | FILE_ANY_ACCESS (0) | Likely RESET_PIPE or ABORT |
| `0x00222035` | 0x80D (2061) | METHOD_OUT_DIRECT (1) | FILE_ANY_ACCESS (0) | Likely BULK_WRITE |
| `0x00222051` | 0x814 (2068) | METHOD_OUT_DIRECT (1) | FILE_ANY_ACCESS (0) | Likely control transfer or vendor command |
| `0x00222054` | 0x815 (2069) | METHOD_BUFFERED (0) | FILE_ANY_ACCESS (0) | Likely BULK_READ |
| `0x0022206D` | 0x81B (2075) | METHOD_OUT_DIRECT (1) | FILE_ANY_ACCESS (0) | Likely vendor write |
| `0x00222074` | 0x81D (2077) | METHOD_BUFFERED (0) | FILE_ANY_ACCESS (0) | Likely vendor read |

### IOCTL Analysis Pattern

The function numbers are sequential (0x800, 0x80D, 0x814, 0x815, 0x81B, 0x81D), suggesting the driver uses a switch-case dispatch with these as IOCTL function codes.

**Hypothesized mapping** (based on function number ordering and method types):
- `0x00222000` → RESET/INIT
- `0x00222035` → WRITE TO BULK OUT ENDPOINT
- `0x00222051` → CONTROL TRANSFER (VENDOR OUT)
- `0x00222054` → READ FROM BULK IN ENDPOINT
- `0x0022206D` → CONTROL TRANSFER WRITE
- `0x00222074` → CONTROL TRANSFER READ
- `0x00220007` → GET DESCRIPTOR / GET STATUS

## EZ-USB Firmware Download Protocol

Based on Cypress EZ-USB FX2 documentation, the initialization driver (ezwinit.sys) likely uses these standard vendor requests:

| Request | bmRequestType | bRequest | wValue | wIndex | wLength | Purpose |
|---------|--------------|----------|--------|--------|---------|---------|
| FW Load | 0x40 (Vendor OUT) | 0xA0 | Address low | Address high | Count | Write firmware to internal RAM |
| CPU Reset | 0x40 (Vendor OUT) | 0xA0 | 0xE600 | 0x0000 | 0 | Write CPUCS register (reset CPU) |
| CPU Start | 0x40 (Vendor OUT) | 0xA0 | 0xE600 | 0x0001 | 0 | Set CPUCS (start CPU, triggers re-enumeration) |

After firmware download and CPU start, the device re-enumerates with the new VID/PID pair (0548:1005).

## EZClient Application Protocol

EZClient uses `DeviceIoControl` to send commands to `\\.\Ezw-0`. Key strings from binary:

```
"Status:[1]0x%x [2]0x%x [3]0x%x [4]0x%x"  → status debug output
"execute read thread failed"                 → threaded read operations
"execute write thread failed"                → threaded write operations
"execute Verify thread failed"               → threaded verify operations
"NO Cart" / "No Cart" / "NON EZCart"         → cart presence detection
"FLASH_TYPE"                                 → save type detection
"EEPROM_TYPE"                                → save type detection
"Burn saver"                                 → save data burning
"BackFile" / "BackSign"                      → backup operations
"Delete Last ROM"                            → ROM management
"File header broken"                         → ROM header validation
```

The application uses MFC + CodeJock XTP for UI and communicates with the driver via a worker thread model (read/write/verify threads).

## Firmware File Analysis

### tusbez.bin (8051 firmware for EZ-USB)
- Size: 5,584 bytes
- Architecture: Intel 8051 (enhanced for EZ-USB)
- Reset vector: `LJMP 0x1202` (`02 12 02`)
- Contains: USB endpoint handlers, GBA cartridge SPI/parallel communication

### EZLoader2.bin (GBA cartridge firmware)
- Size: 37,836 bytes
- Architecture: ARM (GBA ROM)
- Magic: `2E 00 00 EA` (ARM branch to header)
- String at 0xA0: "EZLoader"
- Contains: GBA cartridge initialization and menu system

### ez_flash.bin (Main EZ-Flash firmware)
- Size: 95,608 bytes
- Structure: Looks encrypted or compressed (high entropy)

## Version Information

| Component | Version | Date |
|-----------|---------|------|
| EZClient.exe | 3.26 | 2006-06-10 |
| ezwinit.sys | (from PDB) EZ_Writer3.0\ezloader | ~2005 |
| ezwriter.sys | (from PDB) pfw_usbfx2 | ~2005 |
| ApLoader.sys | (from PDB) ti3210 | ~2005 |
| tusbez.bin | (raw 8051) | ~2005 |
