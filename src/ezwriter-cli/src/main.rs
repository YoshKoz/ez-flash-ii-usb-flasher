use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use rusb::{Device, DeviceDescriptor, DeviceHandle, Direction, GlobalContext};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

// ---------------------------------------------------------------------------
// Device identities
// ---------------------------------------------------------------------------

/// VID/PID at bootloader (Cypress EZ-USB AN2131 default)
const BOOTLOADER_VID: u16 = 0x0547;
const BOOTLOADER_PID: u16 = 0x2131;

/// VID/PID after firmware upload (EZ-Writer mode)
const EZWRITER_VID: u16 = 0x0548;
const EZWRITER_PID: u16 = 0x1005;

/// Cypress vendor request: write to internal RAM
const VR_CYPRESS_WRITE: u8 = 0xA0;

/// CPUCS register address for AN2131 (EZ-USB FX, not FX2)
/// AN2131 register map: CPUCS at 0x7F92
/// Bit 0: 8051RES (0=reset, 1=run)
const CPUCS_ADDR: u16 = 0x7F92;

/// Timeout for USB control transfers
const TIMEOUT: Duration = Duration::from_secs(5);

/// Embedded firmware tables (compiled in so `reload` needs no file args)
/// Looks in CWD first, then beside the running executable.
fn resolve_asset(name: &str) -> std::path::PathBuf {
    let cwd = std::path::PathBuf::from(name);
    if cwd.exists() {
        return cwd;
    }
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        let beside = dir.join(name);
        if beside.exists() {
            return beside;
        }
    }
    cwd
}

/// `loader_table1.bin`/`loader_table2.bin` ship in `firmware/`, extracted
/// from the vendor Windows driver — copy next to the executable (or CWD).
fn load_loader_table(name: &str) -> Result<Vec<u8>> {
    let path = resolve_asset(name);
    std::fs::read(&path).with_context(|| {
        format!(
            "couldn't read {name} (looked at {}); see README for how to obtain it",
            path.display()
        )
    })
}

// ---------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------

#[derive(Parser)]
#[command(name = "ezwriter-cli")]
#[command(about = "EZ-Flash II USB Flasher for EZ-Writer II hardware")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// List all EZ-Writer devices detected
    List,
    /// Show detailed USB info about connected device
    Info,
    /// Download firmware to EZ-USB (from bootloader mode)
    FirmwareDownload {
        /// Path to firmware binary
        fw: PathBuf,
        /// Skip CPU start after upload
        #[arg(long)]
        no_cpu: bool,
    },
    /// Exact init sequence (loader tables)
    InitExact { table1: PathBuf, table2: PathBuf },
    /// Read cartridge header and display game info
    CartInfo,
    /// Read save data from cartridge
    SaveRead {
        /// Starting byte address (0 = start of save)
        #[arg(default_value = "0")]
        addr: u32,
        /// Number of 64-byte chunks to read
        #[arg(default_value = "4")]
        count: u32,
        /// Save type suffix: 'f'=FLASH, 'e'=EEPROM, 's'=SRAM
        #[arg(default_value = "s", short = 't')]
        save_type: char,
        /// Output file (optional)
        #[arg(short)]
        output: Option<PathBuf>,
        /// Use word addressing (byte_addr/2) like ROM reads
        #[arg(long)]
        word_addr: bool,
        /// New protocol: send cmd 0x1A (read register) after select
        #[arg(long)]
        use_reg: bool,
        /// Use cmd 0x01 (ROM read) at offset instead
        #[arg(long)]
        use_rom_read: bool,
        /// ROM read offset for save area (in bytes)
        #[arg(long, default_value = "1572864")]
        rom_offset: u32,
        /// Skip the per-chunk read confirmation (faster, but a stale USB buffer
        /// or an unstable cartridge can then corrupt the save silently)
        #[arg(long)]
        no_confirm: bool,
    },
    /// Advanced save probe: try multiple strategies
    SaveProbe {
        /// Number of chunks to read per attempt
        #[arg(default_value = "4")]
        count: u32,
    },
    /// Read ROM data from cartridge
    CartRead {
        /// Starting byte address
        #[arg(default_value = "0")]
        addr: u32,
        /// Number of 64-byte chunks to read
        #[arg(default_value = "4")]
        count: u32,
        /// Command byte (default 0x01)
        #[arg(default_value = "1", short = 'c')]
        cmd: u8,
        /// Bank number (for 32MB+ ROMs)
        #[arg(long)]
        bank: Option<u8>,
        /// Use byte[3] as bank (derived from address >> 17)
        #[arg(long)]
        byte3_bank: bool,
    },
    /// Dump entire ROM to file
    Dump {
        output: PathBuf,
        /// Start address
        #[arg(default_value = "0")]
        start: u32,
        /// Size to dump (0 = max)
        #[arg(default_value = "0")]
        size: u32,
        /// Delay between chunks in ms
        #[arg(default_value = "5", long)]
        delay: u64,
        /// Fast pipelined mode (experimental)
        #[arg(long)]
        fast: bool,
        /// Number of ROM read commands to keep in flight (default 1 = one
        /// command per chunk). >1 trades a little safety for a large speedup;
        /// run `bench` on your hardware first to find the depth it sustains.
        #[arg(long, value_name = "N")]
        pipeline: Option<usize>,
        /// Re-read the cartridge after dumping and compare byte-for-byte.
        /// Use this to tell "systematically wrong dump" (bootleg/hacked cart)
        /// apart from "unstable reads" (failing cart / bad connection).
        #[arg(long)]
        verify: bool,
        /// Skip the per-chunk read confirmation. Faster, but a stale USB
        /// buffer or an unstable cartridge can then corrupt the dump silently.
        #[arg(long)]
        no_confirm: bool,
    },
    /// Read the save chip's JEDEC manufacturer/device ID (diagnostic)
    ///
    /// Uses only the verified cmd 0x14 / 0x20 / 0x03 primitives, never cmd 0x02.
    /// A retail Gen 3 cart reports Macronix (0xC2); anything else is a strong
    /// signal of a bootleg/reproduction PCB or a non-standard flash chip.
    SaveId,
    /// Benchmark ROM read paths on the connected hardware
    ///
    /// Measures per-chunk latency, whether the firmware streams more than one
    /// packet per command, and sustained throughput at several pipeline depths.
    /// Read-only: it never writes to the cartridge.
    Bench {
        /// 64-byte chunks to read per measurement
        #[arg(default_value = "256")]
        chunks: u32,
        /// Pipeline depths to test
        #[arg(long, value_delimiter = ',', default_value = "1,2,4,8,16")]
        depths: Vec<usize>,
    },
    /// Reset USB device
    Reset,
    /// Probe a vendor request
    Probe {
        request: u8,
        #[arg(default_value = "0")]
        value: u16,
    },
    /// Read internal RAM via vendor request
    RamRead { address: u16 },
    /// Write to internal RAM via vendor request
    RamWrite { address: u16, value: u8 },
    /// Passive read: try reading all IN endpoints without sending anything
    PassiveRead,
    /// Reset cartridge NOR flash to read array mode
    ResetCart,
    /// Write save data to cartridge
    SaveWrite {
        input: PathBuf,
        /// Starting byte address
        #[arg(default_value = "0")]
        addr: u32,
        /// Save type suffix
        #[arg(default_value = "s", short = 't')]
        save_type: char,
        /// Write command byte (default 0x03)
        #[arg(default_value = "3", long)]
        write_cmd: u8,
        /// Erase command byte (default 0x15)
        #[arg(default_value = "0x15", long)]
        erase_cmd: u8,
    },
    /// Write ROM data to cartridge
    RomWrite {
        input: PathBuf,
        /// Starting byte address
        #[arg(default_value = "0")]
        addr: u32,
        /// Delay between chunks in ms
        #[arg(default_value = "50", long)]
        delay: u64,
        /// Skip erase
        #[arg(long)]
        no_erase: bool,
        /// Write command byte
        #[arg(default_value = "0x41", long)]
        write_cmd: u8,
        /// Erase command byte
        #[arg(default_value = "0x40", long)]
        erase_cmd: u8,
    },
    /// Reload firmware: CPUCS reset → OS power cycle if needed → auto init-exact
    Reload,
    /// Bulk endpoint test
    BulkTest,
    /// Write register via cmd 0x19 (Write_Operation = 25)
    WriteReg {
        /// 24-bit address
        #[arg(default_value = "0")]
        addr: u32,
        /// 16-bit value
        #[arg(default_value = "0")]
        value: u16,
    },
    /// Read register via cmd 0x1A (Read_Operation = 26)
    ReadReg {
        /// 24-bit address
        #[arg(default_value = "0")]
        addr: u32,
    },
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn find_device(vid: u16, pid: u16) -> Result<(Device<GlobalContext>, DeviceDescriptor)> {
    for device in rusb::devices()?.iter() {
        let desc = device.device_descriptor()?;
        if desc.vendor_id() == vid && desc.product_id() == pid {
            return Ok((device, desc));
        }
    }
    bail!(
        "Device {vid:#06x}:{pid:#06x} not found. \
         Check cable/power, and on Windows confirm WinUSB is bound to this VID:PID in Zadig \
         (List All Devices) — the device re-enumerates under a new VID:PID after firmware \
         load, so it needs binding twice. See README Troubleshooting."
    );
}

fn print_hex(data: &[u8]) {
    let mut s = String::new();
    for (i, b) in data.iter().enumerate() {
        let _ = write!(s, "{b:02x} ");
        if (i + 1) % 32 == 0 && i + 1 < data.len() {
            println!("  {}", s);
            s.clear();
        }
    }
    if !s.is_empty() {
        println!("  {}", s);
    }
}

fn print_device_info(desc: &DeviceDescriptor, handle: &DeviceHandle<GlobalContext>) -> Result<()> {
    let _timeout = Duration::from_secs(2);
    println!(
        "  Vendor/Product: {:#06x}:{:#06x}",
        desc.vendor_id(),
        desc.product_id()
    );
    // Read string descriptors
    if let Ok(s) = handle.read_manufacturer_string_ascii(desc) {
        println!("  Manufacturer: {s}");
    }
    if let Ok(s) = handle.read_product_string_ascii(desc) {
        println!("  Product: {s}");
    }
    if let Ok(s) = handle.read_serial_number_string_ascii(desc) {
        println!("  Serial: {s}");
    }
    println!("  Device class: {:#04x}", desc.class_code());
    println!("  Device subclass: {:#04x}", desc.sub_class_code());
    println!("  Protocol: {:#04x}", desc.protocol_code());
    println!("  Max EP0 size: {}", desc.max_packet_size());
    println!("  Num configs: {}", desc.num_configurations());
    // Active config
    if let Ok(config) = device_config_descriptor(handle, 0) {
        for iface in config.interfaces() {
            for iface_desc in iface.descriptors() {
                println!(
                    "  Interface {}: {} EP(s), class={:#04x} subclass={:#04x} protocol={:#04x}",
                    iface_desc.interface_number(),
                    iface_desc.num_endpoints(),
                    iface_desc.class_code(),
                    iface_desc.sub_class_code(),
                    iface_desc.protocol_code()
                );
                for ep in iface_desc.endpoint_descriptors() {
                    let dir = match ep.direction() {
                        Direction::In => "IN",
                        Direction::Out => "OUT",
                    };
                    println!(
                        "    EP {:#04x} {}: {} ({})",
                        ep.address(),
                        dir,
                        match ep.transfer_type() {
                            rusb::TransferType::Bulk => "BULK",
                            rusb::TransferType::Interrupt => "INTERRUPT",
                            rusb::TransferType::Isochronous => "ISOCHRONOUS",
                            _ => "CONTROL",
                        },
                        ep.max_packet_size()
                    );
                }
            }
        }
    }
    Ok(())
}

fn device_config_descriptor(
    handle: &DeviceHandle<GlobalContext>,
    index: u8,
) -> Result<rusb::ConfigDescriptor> {
    let device = handle.device();
    Ok(device.config_descriptor(index)?)
}

// ---------------------------------------------------------------------------
// Firmware download helpers
// ---------------------------------------------------------------------------

fn ezusb_write_ram(handle: &DeviceHandle<GlobalContext>, addr: u32, data: &[u8]) -> Result<()> {
    let wval = (addr & 0xFFFF) as u16;
    let windex = ((addr >> 16) & 0xFFFF) as u16;
    handle
        .write_control(0x40, VR_CYPRESS_WRITE, wval, windex, data, TIMEOUT)
        .with_context(|| format!("vendor 0xA0 write to addr 0x{addr:04X}"))?;
    Ok(())
}

#[allow(dead_code)]
fn ezusb_read_ram(handle: &DeviceHandle<GlobalContext>, addr: u32) -> Result<u8> {
    let wval = (addr & 0xFFFF) as u16;
    let windex = ((addr >> 16) & 0xFFFF) as u16;
    let mut buf = [0u8; 1];
    handle
        .read_control(
            0xC0, // Device-to-Host, Vendor, Device
            0xA3, // Cypress Upload from internal memory
            wval, windex, &mut buf, TIMEOUT,
        )
        .with_context(|| format!("vendor 0xA3 read from addr 0x{addr:04X}"))?;
    Ok(buf[0])
}

fn download_firmware(handle: &DeviceHandle<GlobalContext>, fw: &[u8], no_cpu: bool) -> Result<()> {
    // Hold CPU in reset
    ezusb_write_ram(handle, CPUCS_ADDR as u32, &[0x00])?;
    println!("CPU held in reset.");

    // Upload firmware in chunks
    let chunk_size = 64;
    let total = fw.len();
    for (i, chunk) in fw.chunks(chunk_size).enumerate() {
        let addr = i * chunk_size;
        let wval = (addr & 0xFFFF) as u16;
        let windex = ((addr >> 16) & 0xFFFF) as u16;
        handle.write_control(0x40, VR_CYPRESS_WRITE, wval, windex, chunk, TIMEOUT)?;
        if i % 16 == 0 {
            print!("\r  Uploading... {}/{} bytes", (i + 1) * chunk_size, total);
            use std::io::Write;
            std::io::stdout().flush()?;
        }
    }
    println!("\r  Uploaded {} bytes.", total);

    if !no_cpu {
        // Start CPU
        println!("Starting CPU (device will re-enumerate)...");
        ezusb_write_ram(handle, CPUCS_ADDR as u32, &[0x01])?;
        println!("CPU started.");
    }

    Ok(())
}

fn parse_chunk_table(data: &[u8]) -> Result<Vec<(u16, Vec<u8>)>> {
    if data.len() < 10 || &data[..8] != b"EZWLDR1\0" {
        bail!("Invalid chunk table (bad magic)");
    }
    let count = u16::from_le_bytes([data[8], data[9]]) as usize;
    let mut chunks = Vec::with_capacity(count);
    let mut offset = 10;
    for _ in 0..count {
        if offset + 3 > data.len() {
            bail!("Truncated chunk table header");
        }
        let addr = u16::from_le_bytes([data[offset], data[offset + 1]]);
        let len = data[offset + 2] as usize;
        offset += 3;
        if offset + len > data.len() {
            bail!("Truncated chunk table payload");
        }
        chunks.push((addr, data[offset..offset + len].to_vec()));
        offset += len;
    }
    Ok(chunks)
}

fn load_chunk_table(path: &PathBuf) -> Result<Vec<(u16, Vec<u8>)>> {
    let data =
        fs::read(path).with_context(|| format!("reading chunk table: {}", path.display()))?;
    parse_chunk_table(&data)
}

fn write_chunks(
    handle: &DeviceHandle<GlobalContext>,
    name: &str,
    chunks: &[(u16, Vec<u8>)],
) -> Result<()> {
    println!("Writing {name}: {} chunks", chunks.len());
    for (index, (addr, payload)) in chunks.iter().enumerate() {
        ezusb_write_ram(handle, *addr as u32, payload)?;
        if index % 20 == 0 || index + 1 == chunks.len() {
            println!(
                "  {}/{} addr=0x{:04X} len={}",
                index + 1,
                chunks.len(),
                addr,
                payload.len()
            );
        }
    }
    Ok(())
}

fn cpucs(handle: &DeviceHandle<GlobalContext>, value: u8) -> Result<()> {
    println!("CPUCS <- {value}");
    ezusb_write_ram(handle, CPUCS_ADDR as u32, &[value])
}

fn cmd_init_exact(table1: &PathBuf, table2: &PathBuf) -> Result<()> {
    let chunks1 = load_chunk_table(table1)?;
    let chunks2 = load_chunk_table(table2)?;
    let (device, _desc) = find_device(BOOTLOADER_VID, BOOTLOADER_PID)?;
    println!("Found EZ-Writer bootloader. Exact init sequence.");

    let handle = device.open()?;
    let _ = handle.detach_kernel_driver(0);
    let config = device.active_config_descriptor()?;
    if let Some(iface) = config.interfaces().next()
        && let Some(desc) = iface.descriptors().next()
    {
        let _ = handle.claim_interface(desc.interface_number());
    }

    cpucs(&handle, 1)?;
    cpucs(&handle, 1)?;
    write_chunks(&handle, "table1", &chunks1)?;
    cpucs(&handle, 0)?;
    cpucs(&handle, 1)?;
    write_chunks(&handle, "table2", &chunks2)?;
    cpucs(&handle, 1)?;
    cpucs(&handle, 0)?;

    println!("Init sent. Wait 5 seconds, then run list.");
    Ok(())
}

fn cmd_cart_info() -> Result<()> {
    let (device, desc) = find_device(EZWRITER_VID, EZWRITER_PID)?;
    println!("Found EZ-Writer in active mode.");

    let handle = device.open()?;
    print_device_info(&desc, &handle)?;

    // Claim interface
    let config = device.active_config_descriptor()?;
    for iface in config.interfaces() {
        for iface_desc in iface.descriptors() {
            handle.claim_interface(iface_desc.interface_number())?;
        }
    }

    println!("\nReading cartridge header...");
    let cmd_ep = 0x04;
    let data_ep = 0x82;
    let mut cart_data = Vec::new();

    for chunk in 0..4 {
        let addr = chunk * 32;
        let cmd = [
            0x01u8,
            (addr & 0xFF) as u8,
            ((addr >> 8) & 0xFF) as u8,
            0x00,
        ];
        handle.write_bulk(cmd_ep, &cmd, TIMEOUT)?;
        std::thread::sleep(std::time::Duration::from_millis(5));

        let mut buf = [0u8; 64];
        match handle.read_bulk(data_ep, &mut buf, TIMEOUT) {
            Ok(len) => {
                cart_data.extend_from_slice(&buf[..len]);
            }
            Err(e) => {
                println!("  [chunk {chunk}] read error: {e}");
                break;
            }
        }
    }

    if cart_data.len() >= 0xB2 {
        let title: String = cart_data[0xA0..0xAC]
            .iter()
            .take_while(|&&b| b != 0 && b.is_ascii())
            .map(|&b| b as char)
            .collect();
        let code: String = cart_data[0xAC..0xB0]
            .iter()
            .take_while(|&&b| b != 0 && b.is_ascii())
            .map(|&b| b as char)
            .collect();
        let maker: String = cart_data[0xB0..0xB2]
            .iter()
            .take_while(|&&b| b != 0 && b.is_ascii())
            .map(|&b| b as char)
            .collect();

        if !title.is_empty() {
            println!("  Title:    {title}");
            println!("  Code:     {code}");
            println!("  Maker:    {maker}");
        } else {
            println!("  No valid GBA title found in header.");
        }
    } else {
        println!("  Could not read enough data for header.");
    }

    Ok(())
}

fn cmd_reset() -> Result<()> {
    let (device, _desc) = if let Ok(d) = find_device(EZWRITER_VID, EZWRITER_PID) {
        println!("Device in ACTIVE mode.");
        d
    } else if let Ok(d) = find_device(BOOTLOADER_VID, BOOTLOADER_PID) {
        println!("Device in BOOTLOADER mode.");
        d
    } else {
        bail!("No EZ-Writer device found.");
    };

    let handle = device.open()?;
    println!("Sending USB bus reset...");
    handle.reset()?;
    println!("Reset sent. Device may re-enumerate.");
    Ok(())
}

fn cmd_probe(request: u8, value: u16) -> Result<()> {
    let vid_pid = if find_device(EZWRITER_VID, EZWRITER_PID).is_ok() {
        (EZWRITER_VID, EZWRITER_PID, "ACTIVE")
    } else if find_device(BOOTLOADER_VID, BOOTLOADER_PID).is_ok() {
        (BOOTLOADER_VID, BOOTLOADER_PID, "BOOTLOADER")
    } else {
        bail!("No EZ-Writer device found.");
    };

    let (device, _desc) = find_device(vid_pid.0, vid_pid.1)?;
    println!("Device in {} mode.", vid_pid.2);

    let handle = device.open()?;
    let config = device.active_config_descriptor()?;
    if let Some(iface) = config.interfaces().next()
        && let Some(desc) = iface.descriptors().next()
    {
        handle.claim_interface(desc.interface_number())?;
    }

    println!(
        "Sending vendor request: bReq=0x{:02X} wVal=0x{:04X}",
        request, value
    );

    let mut buf = [0u8; 64];
    match handle.read_control(0xC0, request, value, 0, &mut buf, TIMEOUT) {
        Ok(len) => {
            println!("  Response: {} bytes", len);
            print_hex(&buf[..len]);
        }
        Err(rusb::Error::Pipe) => {
            println!("  STALL (command not supported)");
        }
        Err(rusb::Error::Timeout) => {
            println!("  Timeout (no response)");
        }
        Err(e) => {
            println!("  Error: {}", e);
        }
    }

    Ok(())
}

fn cmd_ram_read(address: u16) -> Result<()> {
    let (device, _desc) = if let Ok(d) = find_device(EZWRITER_VID, EZWRITER_PID) {
        println!("Device in ACTIVE mode.");
        d
    } else if let Ok(d) = find_device(BOOTLOADER_VID, BOOTLOADER_PID) {
        println!("Device in BOOTLOADER mode.");
        d
    } else {
        bail!("No EZ-Writer device found.");
    };

    let handle = device.open()?;
    let config = device.active_config_descriptor()?;
    if let Some(iface) = config.interfaces().next()
        && let Some(desc) = iface.descriptors().next()
    {
        handle.claim_interface(desc.interface_number())?;
    }

    let mut buf = [0u8; 64];
    println!("Reading RAM at 0x{address:04X} via vendor 0xA3...");
    match handle.read_control(0xC0, 0xA3, address, 0, &mut buf, TIMEOUT) {
        Ok(len) => {
            println!("  Read {len} bytes:");
            print_hex(&buf[..len]);
        }
        Err(rusb::Error::Pipe) => {
            println!("  STALL (command not supported)");
        }
        Err(rusb::Error::Timeout) => {
            println!("  Timeout");
        }
        Err(e) => {
            println!("  Error: {e}");
        }
    }
    Ok(())
}

fn cmd_ram_write(address: u16, value: u8) -> Result<()> {
    let (device, _desc) = if let Ok(d) = find_device(EZWRITER_VID, EZWRITER_PID) {
        println!("Device in ACTIVE mode.");
        d
    } else if let Ok(d) = find_device(BOOTLOADER_VID, BOOTLOADER_PID) {
        println!("Device in BOOTLOADER mode.");
        d
    } else {
        bail!("No EZ-Writer device found.");
    };

    let handle = device.open()?;
    let config = device.active_config_descriptor()?;
    if let Some(iface) = config.interfaces().next()
        && let Some(desc) = iface.descriptors().next()
    {
        handle.claim_interface(desc.interface_number())?;
    }

    let data = [value];
    println!("Writing 0x{value:02X} to RAM at 0x{address:04X} via vendor 0xA0...");
    match handle.write_control(0x40, 0xA0, address, 0, &data, TIMEOUT) {
        Ok(_) => {
            println!("  Write OK. Verifying with read...");
            let mut buf = [0u8; 64];
            match handle.read_control(0xC0, 0xA3, address, 0, &mut buf, TIMEOUT) {
                Ok(len) => {
                    if len > 0 {
                        println!("  Read back: buf[0] = 0x{:02X}", buf[0]);
                        if buf[0] == value {
                            println!("  ✓ Read-back matches!");
                        } else {
                            println!("  ✗ MISMATCH: wrote 0x{value:02X}, read 0x{:02X}", buf[0]);
                        }
                    } else {
                        println!("  Read returned 0 bytes");
                    }
                }
                Err(e) => println!("  Verify read error: {e}"),
            }
        }
        Err(e) => println!("  Write error: {e}"),
    }
    Ok(())
}

fn cmd_passive_read() -> Result<()> {
    let (device, _desc) = find_device(EZWRITER_VID, EZWRITER_PID)?;
    println!("Found EZ-Writer active mode. Passive read only; no OUT transfers.");
    let handle = device.open()?;
    let config = device.active_config_descriptor()?;
    for iface in config.interfaces() {
        for iface_desc in iface.descriptors() {
            let _ = handle.claim_interface(iface_desc.interface_number());
        }
    }

    let timeout = Duration::from_millis(250);
    for ep in 0x81u8..=0x87u8 {
        let mut buf = [0u8; 64];
        match handle.read_bulk(ep, &mut buf, timeout) {
            Ok(len) => {
                println!("EP 0x{ep:02X}: {len} bytes");
                print_hex(&buf[..len]);
            }
            Err(rusb::Error::Timeout) => println!("EP 0x{ep:02X}: timeout (no queued data)"),
            Err(e) => println!("EP 0x{ep:02X}: {e}"),
        }
    }
    Ok(())
}

fn cmd_reset_cart() -> Result<()> {
    let (device, _desc) = find_device(EZWRITER_VID, EZWRITER_PID)?;
    let handle = device.open()?;
    for i in 0..2 {
        let _ = handle.claim_interface(i);
    }
    for ep in 0x01u8..=0x07u8 {
        let _ = handle.clear_halt(ep);
        let _ = handle.clear_halt(ep | 0x80);
    }

    let cmd_ep = 0x04;
    let sequence: [(u8, u16); 4] = [(0xAA, 0xAAAA), (0x55, 0x5554), (0xF0, 0xAAAA), (0xFF, 0)];
    println!("Resetting cartridge flash...");
    for (cmd_byte, addr) in &sequence {
        let dev_addr = addr / 2;
        let cmd = [
            *cmd_byte,
            (dev_addr & 0xFF) as u8,
            ((dev_addr >> 8) & 0xFF) as u8,
            0x00,
        ];
        let _ = handle.write_bulk(cmd_ep, &cmd, Duration::from_millis(1000));
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    println!("Done.");
    Ok(())
}

/// Try to read save data using cmd 0x01 (ROM read) at specified offset.
/// This tests if save data is mapped to a different area in the cartridge's address space.
fn save_read_via_rom_read(
    handle: &DeviceHandle<GlobalContext>,
    data_ep: u8,
    cmd_ep: u8,
    byte_addr: u32,
    count: u32,
    rom_offset: u32,
    suffix: u8,
) -> Result<Vec<u8>> {
    // First select save type (needed for some implementations)
    let select_cmd = [0x14u8, suffix, 0x00];
    let _ = handle.write_bulk(cmd_ep, &select_cmd, Duration::from_millis(100));
    std::thread::sleep(Duration::from_millis(50));

    let mut all = Vec::new();
    for chunk in 0..count {
        // Use ROM read command (0x01) but address into the "save" area
        let save_addr = rom_offset + byte_addr + chunk * 64;
        let word_addr = save_addr / 2;
        let bank = (word_addr >> 16) as u8;
        let addr_16 = (word_addr & 0xFFFF) as u16;
        let cmd = [
            0x01u8,
            (addr_16 & 0xFF) as u8,
            ((addr_16 >> 8) & 0xFF) as u8,
            bank,
        ];
        handle.write_bulk(cmd_ep, &cmd, TIMEOUT)?;
        std::thread::sleep(Duration::from_millis(5));

        let mut buf = [0u8; 64];
        match handle.read_bulk(data_ep, &mut buf, TIMEOUT) {
            Ok(len) => {
                all.extend_from_slice(&buf[..len]);
                let h: String = buf[..8]
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<Vec<_>>()
                    .join(" ");
                println!("  [{chunk:02}] 0x{:06X}: {}...", save_addr, h);
            }
            Err(e) => {
                println!("  [{chunk:02}] read error: {e}");
                break;
            }
        }
    }
    Ok(all)
}

/// Try save read using register protocol (0x19/0x1A) - unlock + set RAM page
fn save_read_via_reg(
    handle: &DeviceHandle<GlobalContext>,
    cmd_ep: u8,
    data_ep: u8,
    byte_addr: u32,
    count: u32,
    suffix: u8,
) -> Result<Vec<u8>> {
    // Step 1: Unlock cartridge (asie protocol)
    println!("  Unlocking cartridge...");
    let unlock_writes = vec![
        (0x9FE000u32, 0xD200u16),
        (0x800000u32, 0x1500u16),
        (0x802000u32, 0xD200u16),
        (0x804000u32, 0x1500u16),
    ];
    for (addr, val) in &unlock_writes {
        let cmd = [
            0x19u8,
            (addr & 0xFF) as u8,
            ((addr >> 8) & 0xFF) as u8,
            ((addr >> 16) & 0xFF) as u8,
            (val & 0xFF) as u8,
            ((val >> 8) & 0xFF) as u8,
        ];
        handle.write_bulk(cmd_ep, &cmd, TIMEOUT)?;
        std::thread::sleep(Duration::from_millis(5));
    }

    // Step 2: Set ROM page to "map RAM" mode
    // EZ-RAM-OFFSET = 0x9C00000 in GBA space
    // In 24-bit mode: lower 24 bits = 0x1C00000
    println!("  Setting RAM page...");
    let _ram_offset = 0x1C0000u32; // This might map to EZ-RAM-OFFSET
    for (addr, val) in &[
        (0xFF0000u32, 0xD2FFu16),
        (0x000000u32, 0x15FFu16),
        (0x010000u32, 0xD2FFu16),
        (0x020000u32, 0x15FFu16),
        (0xE00000u32, 0x0000u16), // RAM page = 0
        (0xFE0000u32, 0x15FFu16),
    ] {
        let cmd = [
            0x19u8,
            (addr & 0xFF) as u8,
            ((addr >> 8) & 0xFF) as u8,
            ((addr >> 16) & 0xFF) as u8,
            (val & 0xFF) as u8,
            ((val >> 8) & 0xFF) as u8,
        ];
        handle.write_bulk(cmd_ep, &cmd, TIMEOUT)?;
        std::thread::sleep(Duration::from_millis(5));
    }

    // Step 3: Select save type
    println!("  Selecting save type...");
    let select_cmd = [0x14u8, suffix, 0x00];
    handle.write_bulk(cmd_ep, &select_cmd, TIMEOUT)?;
    std::thread::sleep(Duration::from_millis(50));

    // Step 4: Try reading with 0x02
    println!("  Reading save data...");
    let mut all = Vec::new();
    for chunk in 0..count {
        let addr = byte_addr + chunk * 64;
        let cmd = [
            0x02u8,
            (addr & 0xFF) as u8,
            ((addr >> 8) & 0xFF) as u8,
            ((addr >> 16) & 0xFF) as u8,
            suffix,
            0,
        ];
        handle.write_bulk(cmd_ep, &cmd[..5], TIMEOUT)?;
        std::thread::sleep(Duration::from_millis(200));

        let mut buf = [0u8; 64];
        match handle.read_bulk(data_ep, &mut buf, TIMEOUT) {
            Ok(len) => {
                all.extend_from_slice(&buf[..len]);
                let h: String = buf[..8]
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<Vec<_>>()
                    .join(" ");
                println!("  [{chunk:02}] 0x{:06X}: {}...", addr, h);
            }
            Err(e) => {
                println!("  [{chunk:02}] {e}");
                break;
            }
        }
    }
    Ok(all)
}

// Firmware applies (addr & 7) × 2 to Xfff3 bus config register.
// Host sends byte address directly — no word conversion needed.
// Re-sends 0x14 select before each 0x02 read — firmware appears to
// arm for a single read per select command.
fn save_read_byte_addr(
    handle: &DeviceHandle<GlobalContext>,
    cmd_ep: u8,
    data_ep: u8,
    byte_addr: u32,
    count: u32,
    suffix: u8,
) -> Result<Vec<u8>> {
    let mut all = Vec::with_capacity(count as usize * 64);
    for chunk in 0..count {
        let addr = byte_addr + chunk * 64;

        let select = [0x14u8, suffix, 0x00];
        handle.write_bulk(cmd_ep, &select, TIMEOUT)?;
        std::thread::sleep(Duration::from_millis(50));

        let cmd = [
            0x02u8,
            (addr & 0xFF) as u8,
            ((addr >> 8) & 0xFF) as u8,
            ((addr >> 16) & 0xFF) as u8,
            suffix,
        ];
        handle.write_bulk(cmd_ep, &cmd, TIMEOUT)?;
        std::thread::sleep(Duration::from_millis(100));

        let mut buf = [0u8; 64];
        match handle.read_bulk(data_ep, &mut buf, TIMEOUT) {
            Ok(len) => {
                let h: String = buf[..len.min(8)]
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<Vec<_>>()
                    .join(" ");
                eprintln!("  [{chunk:03}] 0x{addr:06X}: {h}... ({len}B)");
                all.extend_from_slice(&buf[..len]);
            }
            Err(e) => {
                eprintln!("  [{chunk:03}] 0x{addr:06X}: {e}");
                break;
            }
        }
    }
    Ok(all)
}

#[allow(clippy::too_many_arguments)]
fn cmd_save_read(
    byte_addr: u32,
    count: u32,
    save_type: char,
    output: Option<PathBuf>,
    use_word_addr: bool,
    use_reg: bool,
    use_rom_read: bool,
    rom_offset: u32,
    confirm: bool,
) -> Result<()> {
    let (device, _desc) = find_device(EZWRITER_VID, EZWRITER_PID)?;
    let handle = device.open()?;
    let config = device.active_config_descriptor()?;
    for iface in config.interfaces() {
        for iface_desc in iface.descriptors() {
            let _ = handle.claim_interface(iface_desc.interface_number());
        }
    }
    for ep in 0x01u8..=0x07u8 {
        let _ = handle.clear_halt(ep);
        let _ = handle.clear_halt(ep | 0x80);
    }

    let cmd_ep = 0x04;
    let data_ep = 0x82;
    let suffix = save_type as u8;

    // Default path for FLASH saves: firmware-native two-bank reader. The save chip
    // is on GBA /CS2 and is only reachable via cmd 0x14/0x20/0x03 (not the ROM bus).
    // The experimental --reg / --rom-read methods are kept for protocol research.
    if matches!(save_type, 'f' | 'F') && !use_rom_read && !use_reg {
        println!("Method: native FLASH (0x14 select + 0x20 bank switch + 0x03 read)");
        let data = read_flash128_native(&handle, cmd_ep, data_ep, confirm)?;
        println!("  Read {} bytes", data.len());
        let signatures = gen3_sig_count(&data);
        println!("  Gen3 save signatures: {signatures}");
        if let Some(path) = output {
            fs::write(&path, &data)?;
            println!("Wrote {} bytes to {}", data.len(), path.display());
        }
        println!("Total: {} bytes", data.len());

        if data.len() != 128 * 1024 {
            bail!(
                "expected a 131072-byte 128KB FLASH save, got {} bytes — the read did not complete",
                data.len()
            );
        }
        if signatures < 14 {
            bail!(
                "save validation failed: found {signatures} Gen 3 section signatures, expected at \
                 least 14. The bytes were written for inspection, but this is not a usable save — \
                 the cartridge is unstable, or it is not a retail Gen 3 cart."
            );
        }
        return Ok(());
    }

    if use_rom_read {
        println!("Method: rom_read (0x01) at offset 0x{rom_offset:X}");
        let data = save_read_via_rom_read(
            &handle, data_ep, cmd_ep, byte_addr, count, rom_offset, suffix,
        )?;
        if let Some(path) = output {
            fs::write(&path, &data)?;
            println!("Wrote {} bytes to {}", data.len(), path.display());
        }
        println!("Total: {} bytes", data.len());
        return Ok(());
    }

    if use_reg {
        println!("Method: register unlock + RAM page + save read");
        let data = save_read_via_reg(&handle, cmd_ep, data_ep, byte_addr, count, suffix)?;
        if let Some(path) = output {
            fs::write(&path, &data)?;
            println!("Wrote {} bytes to {}", data.len(), path.display());
        }
        println!("Total: {} bytes", data.len());
        return Ok(());
    }

    // --word-addr kept as flag for backward compat; both paths now use byte addressing
    let method = if use_word_addr {
        "byte_addr (corrected)"
    } else {
        "byte_addr"
    };
    println!(
        "Method: {method} (0x14+0x02) type='{}' (0x{:02X})",
        save_type, suffix
    );
    let data = save_read_byte_addr(&handle, cmd_ep, data_ep, byte_addr, count, suffix)?;
    if let Some(path) = output {
        fs::write(&path, &data)?;
        println!("Wrote {} bytes to {}", data.len(), path.display());
    }
    println!("Total: {} bytes", data.len());
    Ok(())
}

fn gen3_sig_count(data: &[u8]) -> usize {
    const SIG: [u8; 4] = [0x25, 0x20, 0x01, 0x08];
    data.windows(4).filter(|w| **w == SIG).count()
}

/// Read a genuine 128KB GBA FLASH save (e.g. Pokémon Gen 3, Macronix MX29L010
/// C2:09) via the firmware-native save path. The save chip is on GBA /CS2 and is
/// only reachable through cmd 0x14 (select) + cmd 0x20 (byte write) + cmd 0x03
/// (read 64). Two 64KB banks switched by a JEDEC command (AA/55/B0/bank), not an
/// address pin. Returns 131072 bytes; restores read-array mode (F0) on exit.
fn read_flash128_native(
    handle: &DeviceHandle<GlobalContext>,
    cmd_ep: u8,
    data_ep: u8,
    confirm: bool,
) -> Result<Vec<u8>> {
    const BYTES_PER_BANK: u32 = 65536;

    let fwrite = |addr: u16, data: u8| -> Result<()> {
        handle.write_bulk(
            cmd_ep,
            &[0x20u8, (addr & 0xFF) as u8, (addr >> 8) as u8, data],
            TIMEOUT,
        )?;
        std::thread::sleep(Duration::from_millis(4));
        Ok(())
    };
    let drain = || {
        let mut buf = [0u8; 64];
        for _ in 0..64 {
            if handle
                .read_bulk(data_ep, &mut buf, Duration::from_millis(40))
                .is_err()
            {
                break;
            }
        }
    };
    let bank_switch = |bank: u8| -> Result<()> {
        fwrite(0x5555, 0xAA)?;
        fwrite(0x2AAA, 0x55)?;
        fwrite(0x5555, 0xB0)?;
        fwrite(0x0000, bank)?;
        std::thread::sleep(Duration::from_millis(10));
        Ok(())
    };

    handle.write_bulk(cmd_ep, &[0x14u8, 0x66, 0x00], TIMEOUT)?;
    std::thread::sleep(Duration::from_millis(50));
    drain();

    let mut all = Vec::with_capacity((BYTES_PER_BANK * 2) as usize);
    for bank in 0u8..=1 {
        bank_switch(bank)?;
        let mut off = 0u32;
        while off < BYTES_PER_BANK {
            let chunk = chunk_read_confirmed(off, confirm, || {
                drain();
                handle
                    .write_bulk(
                        cmd_ep,
                        &[
                            0x03u8,
                            (off & 0xFF) as u8,
                            ((off >> 8) & 0xFF) as u8,
                            0x00,
                            0x00,
                        ],
                        TIMEOUT,
                    )
                    .with_context(|| format!("FLASH128 bank{bank} off 0x{off:04X}"))?;
                std::thread::sleep(Duration::from_millis(8));
                let mut buf = [0u8; 64];
                let len = handle
                    .read_bulk(data_ep, &mut buf, Duration::from_secs(3))
                    .with_context(|| format!("FLASH128 bank{bank} off 0x{off:04X}"))?;
                if len != 64 {
                    bail!(
                        "short save read at bank{bank} off 0x{off:04X}: got {len} bytes, expected 64"
                    );
                }
                Ok(buf)
            });

            match chunk {
                Ok(c) => all.extend_from_slice(&c),
                Err(e) => {
                    let _ = bank_switch(0);
                    let _ = fwrite(0x5555, 0xAA);
                    let _ = fwrite(0x2AAA, 0x55);
                    let _ = fwrite(0x5555, 0xF0);
                    return Err(e.context(format!(
                        "FLASH128 read failed at bank{bank} off 0x{off:04X}"
                    )));
                }
            }
            off += 64;
        }
    }

    bank_switch(0)?;
    fwrite(0x5555, 0xAA)?;
    fwrite(0x2AAA, 0x55)?;
    fwrite(0x5555, 0xF0)?;
    Ok(all)
}

/// Decode a JEDEC manufacturer ID byte. Retail Gen 3 Pokémon carts use
/// Macronix (0xC2) or Sanyo (0x62); anything else on a "Pokémon" cart is a
/// strong bootleg/reproduction signal.
fn jedec_manufacturer_name(id: u8) -> &'static str {
    match id {
        0x01 => "AMD/Spansion",
        0x0B | 0x98 => "Toshiba/Kioxia",
        0x1F => "Atmel",
        0x20 => "STMicroelectronics",
        0x37 => "AMIC",
        0x62 => "Sanyo",
        0x7F => "EON",
        0x89 => "Intel",
        0xBF => "SST",
        0xC2 => "Macronix",
        0xC8 => "GigaDevice",
        0xEF => "Winbond",
        _ => "unknown",
    }
}

/// Read the save chip's JEDEC manufacturer/device ID.
///
/// Deliberately built only from the primitives the 128KB save reader already
/// proved on hardware: cmd 0x14 (select FLASH), cmd 0x20 (single-byte write,
/// used for flash command sequences) and cmd 0x03 (stream 64 bytes). It never
/// sends cmd 0x02, which is documented to hang the 8051 in a write-completion
/// poll and lock the CPLD until the device is physically replugged.
///
/// Sequence: reset to read-array -> baseline read -> READ ID (AA/55/90) ->
/// read -> reset back to read-array.
fn cmd_save_id() -> Result<()> {
    let (device, _desc) = find_device(EZWRITER_VID, EZWRITER_PID)?;
    let handle = device.open()?;
    let config = device.active_config_descriptor()?;
    for iface in config.interfaces() {
        for iface_desc in iface.descriptors() {
            let _ = handle.claim_interface(iface_desc.interface_number());
        }
    }
    for ep in 0x01u8..=0x07u8 {
        let _ = handle.clear_halt(ep);
        let _ = handle.clear_halt(ep | 0x80);
    }

    let cmd_ep = 0x04;
    let data_ep = 0x82;

    let fwrite = |addr: u16, data: u8| -> Result<()> {
        handle.write_bulk(
            cmd_ep,
            &[0x20u8, (addr & 0xFF) as u8, (addr >> 8) as u8, data],
            TIMEOUT,
        )?;
        std::thread::sleep(Duration::from_millis(4));
        Ok(())
    };
    let drain = || {
        let mut buf = [0u8; 64];
        for _ in 0..64 {
            if handle
                .read_bulk(data_ep, &mut buf, Duration::from_millis(40))
                .is_err()
            {
                break;
            }
        }
    };
    let read64 = |addr: u16| -> Result<[u8; 64]> {
        drain();
        handle.write_bulk(
            cmd_ep,
            &[
                0x03u8,
                (addr & 0xFF) as u8,
                ((addr >> 8) & 0xFF) as u8,
                0x00,
                0x00,
            ],
            TIMEOUT,
        )?;
        std::thread::sleep(Duration::from_millis(8));
        let mut buf = [0u8; 64];
        let len = handle
            .read_bulk(data_ep, &mut buf, TIMEOUT)
            .with_context(|| format!("save read at 0x{addr:04X}"))?;
        if len != 64 {
            bail!("short save read at 0x{addr:04X}: got {len} bytes, expected 64");
        }
        Ok(buf)
    };
    // Leave the flash in read-array mode (AA / 55 / F0).
    let flash_reset = || -> Result<()> {
        fwrite(0x5555, 0xAA)?;
        fwrite(0x2AAA, 0x55)?;
        fwrite(0x5555, 0xF0)?;
        Ok(())
    };

    println!("Selecting FLASH save handler...");
    handle.write_bulk(cmd_ep, &[0x14u8, 0x66, 0x00], TIMEOUT)?;
    std::thread::sleep(Duration::from_millis(50));
    drain();
    flash_reset()?;

    let baseline = read64(0x0000)?;

    println!("Issuing JEDEC READ ID (AA/55/90)...");
    fwrite(0x5555, 0xAA)?;
    fwrite(0x2AAA, 0x55)?;
    fwrite(0x5555, 0x90)?;
    let id = read64(0x0000)?;
    flash_reset()?;

    let hex = |b: &[u8; 64]| {
        b[..8]
            .iter()
            .map(|v| format!("{v:02x}"))
            .collect::<Vec<_>>()
            .join(" ")
    };
    println!();
    println!("Save chip identification");
    println!("  read-array baseline [0..8]: {}", hex(&baseline));
    println!("  after READ ID       [0..8]: {}", hex(&id));
    println!(
        "  manufacturer: 0x{:02X} ({})",
        id[0],
        jedec_manufacturer_name(id[0])
    );
    println!("  device:       0x{:02X}", id[1]);

    if id[..2] == baseline[..2] {
        println!();
        println!(
            "  NOTE: the first two bytes are unchanged from the baseline read, so the chip did not \
             enter READ ID mode. Either it is not a JEDEC-compatible flash, or it does not \
             implement the AA/55/90 sequence (common on bootleg/reproduction PCBs)."
        );
    } else if id[0] == 0xC2 || id[0] == 0x62 {
        println!();
        println!(
            "  This looks like a genuine Gen 3 save flash (Macronix/Sanyo). If the ROM dump still \
             fails, the fault is more likely the cartridge's own hardware than the chip type."
        );
    } else {
        println!();
        println!(
            "  Unrecognised vendor for a retail Pokémon cart. A bootleg/reproduction PCB is likely; \
             the tool cannot reliably dump saves from non-standard flash."
        );
    }

    Ok(())
}

/// Probe all save read strategies
fn cmd_save_probe(count: u32) -> Result<()> {
    let (device, _desc) = find_device(EZWRITER_VID, EZWRITER_PID)?;
    let handle = device.open()?;
    let config = device.active_config_descriptor()?;
    for iface in config.interfaces() {
        for iface_desc in iface.descriptors() {
            let _ = handle.claim_interface(iface_desc.interface_number());
        }
    }
    for ep in 0x01u8..=0x07u8 {
        let _ = handle.clear_halt(ep);
        let _ = handle.clear_halt(ep | 0x80);
    }

    let cmd_ep = 0x04;
    let data_ep = 0x82;

    // Test 1: Original method with type 's' (SRAM)
    println!("\n=== Test 1: Original 0x14+0x02 type='s' (SRAM) ===");
    let data1 = save_read_original(&handle, cmd_ep, data_ep, 0, count, b's')?;
    println!("  Got {} bytes", data1.len());

    // Test 2: Original method with type 'e' (EEPROM)
    println!("\n=== Test 2: Original 0x14+0x02 type='e' (EEPROM) ===");
    let data2 = save_read_original(&handle, cmd_ep, data_ep, 0, count, b'e')?;
    println!("  Got {} bytes", data2.len());

    // Test 3: Original method with type 'f' (FLASH)
    println!("\n=== Test 3: Original 0x14+0x02 type='f' (FLASH) ===");
    let data3 = save_read_original(&handle, cmd_ep, data_ep, 0, count, b'f')?;
    println!("  Got {} bytes", data3.len());

    // Test 4: Byte-address method with 'f' type (replaces removed word-address variant)
    println!("\n=== Test 4: Byte address + type='f' ===");
    let data4 = save_read_byte_addr(&handle, cmd_ep, data_ep, 0, count, b'f')?;
    println!("  Got {} bytes", data4.len());

    // Test 5: Read from different byte addresses
    println!("\n=== Test 5: Type='f' at different addresses ===");
    let select_cmd = [0x14u8, b'f', 0x00];
    handle.write_bulk(cmd_ep, &select_cmd, TIMEOUT)?;
    std::thread::sleep(Duration::from_millis(50));
    for sub_addr in [0u32, 64, 128, 256, 512, 1024, 0x10000, 0x100000] {
        let addr = sub_addr;
        let cmd = [
            0x02u8,
            (addr & 0xFF) as u8,
            ((addr >> 8) & 0xFF) as u8,
            ((addr >> 16) & 0xFF) as u8,
            b'f',
            0,
        ];
        handle.write_bulk(cmd_ep, &cmd[..5], TIMEOUT)?;
        std::thread::sleep(Duration::from_millis(100));
        let mut buf = [0u8; 64];
        match handle.read_bulk(data_ep, &mut buf, Duration::from_millis(500)) {
            Ok(len) => {
                let h: String = buf[..8]
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<Vec<_>>()
                    .join(" ");
                println!("  addr=0x{addr:06X}: {}... ({} bytes)", h, len);
            }
            Err(e) => println!("  addr=0x{addr:06X}: error: {e}"),
        }
        std::thread::sleep(Duration::from_millis(50));
    }

    // Test 6: Try register unlock sequence first
    println!("\n=== Test 6: Unlock cartridge first, then save read ===");
    send_unlock(&handle, cmd_ep)?;
    let data6 = save_read_original(&handle, cmd_ep, data_ep, 0, count, b'f')?;
    println!("  Got {} bytes", data6.len());

    // Test 7: Try 0x02 without 0x14 select first
    println!("\n=== Test 7: 0x02 WITHOUT 0x14 select ===");
    let mut all = Vec::new();
    for chunk in 0..count {
        let addr = chunk * 64;
        let cmd = [
            0x02u8,
            (addr & 0xFF) as u8,
            ((addr >> 8) & 0xFF) as u8,
            ((addr >> 16) & 0xFF) as u8,
            b'f',
            0,
        ];
        handle.write_bulk(cmd_ep, &cmd[..5], TIMEOUT)?;
        std::thread::sleep(Duration::from_millis(100));
        let mut buf = [0u8; 64];
        match handle.read_bulk(data_ep, &mut buf, TIMEOUT) {
            Ok(len) => {
                all.extend_from_slice(&buf[..len]);
                let h: String = buf[..8]
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<Vec<_>>()
                    .join(" ");
                println!("  [{chunk:02}] 0x{:06X}: {}...", addr, h);
            }
            Err(e) => println!("  [{chunk:02}] {e}"),
        }
    }
    println!("  Total: {} bytes", all.len());

    // Test 8: Try read after 0x02 backward-fill (send addr = 0, then addr = big)
    println!("\n=== Test 8: Type='f' select + 0x02, addr=0 vs addr=max ===");
    let select_cmd = [0x14u8, b'f', 0x00];
    handle.write_bulk(cmd_ep, &select_cmd, TIMEOUT)?;
    std::thread::sleep(Duration::from_millis(50));
    // Read at 0
    let cmd0 = [0x02u8, 0, 0, 0, b'f', 0];
    handle.write_bulk(cmd_ep, &cmd0[..5], TIMEOUT)?;
    std::thread::sleep(Duration::from_millis(100));
    let mut buf0 = [0u8; 64];
    handle.read_bulk(data_ep, &mut buf0, TIMEOUT)?;
    // Read at max
    let cmd1 = [0x02u8, 0xFF, 0xFF, 0xFF, b'f', 0];
    handle.write_bulk(cmd_ep, &cmd1[..5], TIMEOUT)?;
    std::thread::sleep(Duration::from_millis(100));
    let mut buf1 = [0u8; 64];
    handle.read_bulk(data_ep, &mut buf1, TIMEOUT)?;

    let h0: String = buf0[..8]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(" ");
    let h1: String = buf1[..8]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(" ");
    println!("  addr=0x000000: {}...", h0);
    println!("  addr=0xFFFFFF: {}...", h1);
    if buf0[..8] == buf1[..8] {
        println!("  IDENTICAL - address parameter IS ignored!");
    } else {
        println!("  DIFFERENT - address parameter works!");
    }

    // Test 9: Try 0x1A register reads from the cartridge address space
    println!("\n=== Test 9: Register reads via 0x1A ===");
    for reg_addr in [
        0x000000u32,
        0x800000,
        0x9C0000,
        0xE00000,
        0x9FE000,
        0xFF0000,
        0x0E0000,
    ] {
        let cmd = [
            0x1Au8,
            (reg_addr & 0xFF) as u8,
            ((reg_addr >> 8) & 0xFF) as u8,
            ((reg_addr >> 16) & 0xFF) as u8,
        ];
        handle.write_bulk(cmd_ep, &cmd, TIMEOUT)?;
        std::thread::sleep(Duration::from_millis(20));
        let mut buf = [0u8; 64];
        match handle.read_bulk(data_ep, &mut buf, TIMEOUT) {
            Ok(len) => {
                println!(
                    "  reg=0x{reg_addr:06X}: {} bytes: {}",
                    len,
                    buf[..len.min(8)]
                        .iter()
                        .map(|b| format!("{b:02x}"))
                        .collect::<Vec<_>>()
                        .join(" ")
                );
            }
            Err(e) => println!("  reg=0x{reg_addr:06X}: {e}"),
        }
    }

    Ok(())
}

fn save_read_original(
    handle: &DeviceHandle<GlobalContext>,
    cmd_ep: u8,
    data_ep: u8,
    byte_addr: u32,
    count: u32,
    suffix: u8,
) -> Result<Vec<u8>> {
    let select_cmd = [0x14u8, suffix, 0x00];
    handle.write_bulk(cmd_ep, &select_cmd, TIMEOUT)?;
    std::thread::sleep(std::time::Duration::from_millis(100));

    let mut all = Vec::new();
    for chunk in 0..count {
        let addr = byte_addr + chunk * 64;
        let mut cmd = [0x02u8, 0, 0, 0, suffix, 0];
        cmd[1] = (addr & 0xFF) as u8;
        cmd[2] = ((addr >> 8) & 0xFF) as u8;
        cmd[3] = ((addr >> 16) & 0xFF) as u8;
        handle.write_bulk(cmd_ep, &cmd[..5], TIMEOUT)?;
        std::thread::sleep(std::time::Duration::from_millis(200));
        let mut buf = [0u8; 64];
        match handle.read_bulk(data_ep, &mut buf, TIMEOUT) {
            Ok(len) => {
                all.extend_from_slice(&buf[..len]);
            }
            Err(e) => {
                println!("    [{chunk:02}] {e}");
                break;
            }
        }
    }
    Ok(all)
}

fn send_unlock(handle: &DeviceHandle<GlobalContext>, cmd_ep: u8) -> Result<()> {
    let unlock_writes = vec![
        (0x9FE000u32, 0xD200u16),
        (0x800000u32, 0x1500u16),
        (0x802000u32, 0xD200u16),
        (0x804000u32, 0x1500u16),
    ];
    let _lock_writes = [(0x9FC000u32, 0x1500u16)];

    println!("    Sending unlock sequence...");
    for (addr, val) in &unlock_writes {
        let cmd = [
            0x19u8,
            (addr & 0xFF) as u8,
            ((addr >> 8) & 0xFF) as u8,
            ((addr >> 16) & 0xFF) as u8,
            (val & 0xFF) as u8,
            ((val >> 8) & 0xFF) as u8,
        ];
        handle.write_bulk(cmd_ep, &cmd, TIMEOUT)?;
        std::thread::sleep(Duration::from_millis(5));
    }
    Ok(())
}

fn cmd_cart_read(
    byte_addr: u32,
    count: u32,
    cmd_byte: u8,
    bank: Option<u8>,
    byte3_bank: bool,
) -> Result<()> {
    let (device, _desc) = find_device(EZWRITER_VID, EZWRITER_PID)?;
    println!("Found EZ-Writer active mode.");
    let handle = device.open()?;
    let config = device.active_config_descriptor()?;
    for iface in config.interfaces() {
        for iface_desc in iface.descriptors() {
            let _ = handle.claim_interface(iface_desc.interface_number());
        }
    }

    for ep in 0x01u8..=0x07u8 {
        let _ = handle.clear_halt(ep);
        let _ = handle.clear_halt(ep | 0x80);
    }

    if cmd_byte == 0x01 || cmd_byte == 0x02 {
        let cmd_ep_rst = 0x04;
        let seq: [(u8, u16); 4] = [(0xAA, 0xAAAA), (0x55, 0x5554), (0xF0, 0xAAAA), (0xFF, 0)];
        for (cb, a) in &seq {
            let da = a / 2;
            let c = [*cb, (da & 0xFF) as u8, ((da >> 8) & 0xFF) as u8, 0x00];
            let _ = handle.write_bulk(cmd_ep_rst, &c, Duration::from_millis(500));
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }

    let cmd_ep = 0x04;
    let data_ep = 0x82;
    let dev_addr = byte_addr / 2;

    let bank_val: u8 = if let Some(b) = bank {
        b
    } else if byte3_bank {
        (byte_addr >> 17) as u8
    } else {
        0
    };
    let use_bank = bank.is_some() || byte3_bank;

    if use_bank && !byte3_bank {
        println!("  Latching bank={} via 0xBF/0x9F sequence", bank_val);
        ezusb_write_ram(&handle, 0x7F96, &[bank_val])?;
        ezusb_write_ram(&handle, 0x7F98, &[0xBF])?;
        ezusb_write_ram(&handle, 0x7F98, &[0x9F])?;
    }

    println!(
        "Reading {} chunks from addr=0x{:X} cmd=0x{:02X}{}",
        count,
        byte_addr,
        cmd_byte,
        if byte3_bank {
            format!(" byte[3]=0x{:02X}", bank_val)
        } else {
            String::new()
        }
    );

    let mut cart_data = Vec::new();
    for chunk in 0..count {
        let addr = dev_addr + chunk * 32;
        let b3 = if byte3_bank { bank_val } else { 0x00 };
        let cmd = [
            cmd_byte,
            (addr & 0xFF) as u8,
            ((addr >> 8) & 0xFF) as u8,
            b3,
        ];
        handle.write_bulk(cmd_ep, &cmd, TIMEOUT)?;
        std::thread::sleep(std::time::Duration::from_millis(150));

        let mut buf = [0u8; 64];
        match handle.read_bulk(data_ep, &mut buf, TIMEOUT) {
            Ok(len) => {
                cart_data.extend_from_slice(&buf[..len]);
                let h: String = buf[..16]
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<Vec<_>>()
                    .join(" ");
                println!("  [{chunk:02}] 0x{:06X}: {}", byte_addr + chunk * 64, h);
            }
            Err(e) => {
                println!("  [{chunk:02}] read error: {e}");
                break;
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }

    if byte_addr == 0 && cart_data.len() >= 0xB2 {
        let title: String = cart_data[0xA0..0xAC]
            .iter()
            .take_while(|&&b| b != 0)
            .map(|&b| b as char)
            .collect();
        let code: String = cart_data[0xAC..0xB0]
            .iter()
            .take_while(|&&b| b != 0)
            .map(|&b| b as char)
            .collect();
        let maker: String = cart_data[0xB0..0xB2]
            .iter()
            .take_while(|&&b| b != 0)
            .map(|&b| b as char)
            .collect();
        if !title.is_empty() {
            println!("\n  Cartridge: {title} [{code}] maker={maker}");
        }
    }

    println!("  Total: {} bytes", cart_data.len());
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn cmd_dump(
    mut output: PathBuf,
    start_addr: u32,
    size: u32,
    delay_ms: u64,
    fast: bool,
    pipeline: Option<usize>,
    verify: bool,
    confirm: bool,
) -> Result<()> {
    if output.extension().is_none_or(|e| e.is_empty()) {
        output.set_extension("gba");
    }

    let (device, _desc) = find_device(EZWRITER_VID, EZWRITER_PID)?;
    println!("Found EZ-Writer active mode.");
    let handle = device.open()?;
    let config = device.active_config_descriptor()?;
    for iface in config.interfaces() {
        for iface_desc in iface.descriptors() {
            let _ = handle.claim_interface(iface_desc.interface_number());
        }
    }
    for ep in 0x01u8..=0x07u8 {
        let _ = handle.clear_halt(ep);
        let _ = handle.clear_halt(ep | 0x80);
    }

    let cmd_ep_rst = 0x04;
    let seq: [(u8, u16); 4] = [(0xAA, 0xAAAA), (0x55, 0x5554), (0xF0, 0xAAAA), (0xFF, 0)];
    for (cb, a) in &seq {
        let da = a / 2;
        let c = [*cb, (da & 0xFF) as u8, ((da >> 8) & 0xFF) as u8, 0x00];
        let _ = handle.write_bulk(cmd_ep_rst, &c, Duration::from_millis(500));
        std::thread::sleep(Duration::from_millis(5));
    }

    let data_ep = 0x82;

    let total_size = if size == 0 {
        println!("No size given — detecting cartridge ROM size...");
        let detected = detect_rom_size(&handle, delay_ms)?;
        println!("  Detected a {} MB cartridge", detected / (1024 * 1024));
        if start_addr >= detected {
            bail!(
                "start address 0x{start_addr:X} is past the end of the detected {detected}-byte \
                 cartridge — pass an explicit size if you really want to read beyond it"
            );
        }
        detected - start_addr
    } else {
        size
    };

    if total_size == 0 {
        bail!("nothing to dump: size resolves to 0 bytes");
    }
    if start_addr as u64 + total_size as u64 > ROM_WINDOW_BYTES {
        bail!(
            "0x{start_addr:X} + {total_size} bytes runs past the {}-byte cartridge address window",
            ROM_WINDOW_BYTES
        );
    }
    let chunk_count = total_size.div_ceil(64);

    // Never touch the destination until the dump is complete and consistent: a
    // half-written .gba that looks like a valid dump is itself a corruption bug.
    let partial = partial_path(&output)?;

    println!(
        "Dumping {} bytes ({} chunks) starting at 0x{:X} to {}",
        total_size,
        chunk_count,
        start_addr,
        output.display()
    );
    // --fast is the historical depth-2 pipelined mode; --pipeline is explicit.
    let depth = pipeline.unwrap_or(if fast { 2 } else { 1 }).max(1);

    if depth > 1 {
        println!(
            "  Mode: EP4 bulk pipelined, {depth} commands in flight (unconfirmed — run \
             `ezwriter-cli bench` to check the depth your hardware sustains)"
        );
    } else if confirm {
        println!("  Mode: EP4 bulk non-pipelined, every chunk read twice and compared");
    } else {
        println!("  Mode: EP4 bulk non-pipelined (--no-confirm: a bad read can go unnoticed)");
    }
    println!();

    if partial.exists() {
        fs::remove_file(&partial)
            .with_context(|| format!("removing stale {}", partial.display()))?;
    }
    let mut file = fs::File::create(&partial)
        .with_context(|| format!("Failed to create output file: {}", partial.display()))?;
    use std::io::Write;
    let mut written: u64 = 0;

    if depth > 1 {
        // Keep `depth` read commands in flight so the device is never idle while
        // the host is doing USB bookkeeping. Reads stay strict: a short packet
        // aborts rather than shifting everything after it.
        let mut issued: u64 = 0;
        let mut received: u64 = 0;
        let mut last_pct = u64::MAX;

        while issued < chunk_count as u64 && issued < depth as u64 {
            send_rom_read_cmd(&handle, start_addr + (issued as u32) * 64)?;
            issued += 1;
        }

        while received < chunk_count as u64 {
            let byte_addr = start_addr + (received as u32) * 64;
            let mut buf = [0u8; 64];
            let len = handle
                .read_bulk(data_ep, &mut buf, Duration::from_secs(3))
                .with_context(|| format!("EP2 ROM read at byte_addr=0x{byte_addr:06X}"))?;
            if len != 64 {
                bail!(
                    "short ROM read at byte_addr=0x{byte_addr:06X}: got {len} bytes, expected 64"
                );
            }
            file.write_all(&buf)?;
            written += 64;
            received += 1;

            if issued < chunk_count as u64 {
                send_rom_read_cmd(&handle, start_addr + (issued as u32) * 64)?;
                issued += 1;
            }

            let pct = (received * 100) / chunk_count as u64;
            if pct != last_pct {
                last_pct = pct;
                let addr_mb = byte_addr as f64 / (1024.0 * 1024.0);
                print!("\r  Progress: {pct}% ({addr_mb:.1} MB)");
                std::io::stdout().flush()?;
            }
        }
    } else {
        let mut last_pct = u64::MAX;
        for chunk in 0..chunk_count {
            let byte_addr = start_addr + chunk * 64;
            let want = std::cmp::min(64, total_size - chunk * 64) as usize;

            let buf = if confirm {
                rom_read_chunk_confirmed(&handle, byte_addr, delay_ms)?
            } else {
                rom_read_chunk(&handle, byte_addr, delay_ms)?
            };

            file.write_all(&buf[..want])?;
            written += want as u64;

            let pct = ((chunk as u64) * 100) / chunk_count as u64;
            if pct != last_pct {
                last_pct = pct;
                let addr_mb = byte_addr as f64 / (1024.0 * 1024.0);
                print!("\r  Progress: {pct}% ({addr_mb:.1} MB)");
                std::io::stdout().flush()?;
            }
        }
    }
    println!();

    file.flush()?;
    file.sync_all()?;
    drop(file);

    if written != total_size as u64 {
        bail!(
            "dump stopped after {written} of {total_size} bytes — {} left in place and {} was not \
             touched",
            partial.display(),
            output.display()
        );
    }
    let file_size = fs::metadata(&partial)
        .with_context(|| format!("stat {}", partial.display()))?
        .len();
    if file_size != total_size as u64 {
        bail!("size mismatch: wrote {file_size} bytes, expected {total_size}");
    }

    if verify {
        verify_dump(&handle, &partial, start_addr, written, delay_ms)?;
    }

    fs::rename(&partial, &output)
        .with_context(|| format!("renaming {} to {}", partial.display(), output.display()))?;

    println!("  Dumped {file_size} bytes to {}", output.display());
    Ok(())
}

/// Append `.partial` to a path's file name.
fn partial_path(output: &Path) -> Result<PathBuf> {
    let name = output
        .file_name()
        .ok_or_else(|| anyhow::anyhow!("output path {} has no file name", output.display()))?;
    let mut partial = output.to_path_buf();
    partial.set_file_name(format!("{}.partial", name.to_string_lossy()));
    Ok(partial)
}

/// Format the first 16 bytes of a chunk for diagnostics.
fn hex_prefix(bytes: &[u8; 64]) -> String {
    bytes[..16]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// The GBA cartridge bus exposes a 32 MB window.
const ROM_WINDOW_BYTES: u64 = 32 * 1024 * 1024;

/// How many read/confirm rounds a chunk gets before the cartridge is declared
/// unstable. A marginal cart usually settles within two or three reads.
const ROM_READ_ATTEMPTS: u32 = 4;

/// Read exactly one 64-byte ROM chunk via cmd 0x01 (16-bit word address + 8-bit
/// bank in byte[3]). Strict: a short transfer is an error rather than silently
/// padding the output.
/// Issue a cmd 0x01 ROM read without waiting for its data.
///
/// The firmware's read routine is driven by a 16-bit block counter, but every
/// call site in `tusbez.bin` hardcodes that counter to 1 (`MOV R3,#1; MOV R2,#0`
/// at 0x0513, 0x0555 and 0x109C), so one command yields exactly one 64-byte
/// packet. Overlapping commands on the host side is therefore the only way to
/// hide the per-command round trip without patching the firmware.
fn send_rom_read_cmd(handle: &DeviceHandle<GlobalContext>, byte_addr: u32) -> Result<()> {
    let word_addr = byte_addr / 2;
    let addr_16 = (word_addr & 0xFFFF) as u16;
    let bank = (word_addr >> 16) as u8;
    let cmd = [
        0x01u8,
        (addr_16 & 0xFF) as u8,
        ((addr_16 >> 8) & 0xFF) as u8,
        bank,
    ];
    handle
        .write_bulk(0x04, &cmd, TIMEOUT)
        .with_context(|| format!("EP4 ROM read cmd at byte_addr=0x{byte_addr:06X}"))?;
    Ok(())
}

/// Read one 64-byte EP2 packet. Strict: a short packet is an error, never padding.
fn read_rom_packet(
    handle: &DeviceHandle<GlobalContext>,
    byte_addr: u32,
    timeout: Duration,
) -> Result<[u8; 64]> {
    let mut buf = [0u8; 64];
    let len = handle
        .read_bulk(0x82, &mut buf, timeout)
        .with_context(|| format!("EP2 ROM read at byte_addr=0x{byte_addr:06X}"))?;
    if len != 64 {
        bail!("short ROM read at byte_addr=0x{byte_addr:06X}: got {len} bytes, expected 64");
    }
    Ok(buf)
}

fn rom_read_chunk(
    handle: &DeviceHandle<GlobalContext>,
    byte_addr: u32,
    delay_ms: u64,
) -> Result<[u8; 64]> {
    send_rom_read_cmd(handle, byte_addr)?;
    std::thread::sleep(Duration::from_millis(delay_ms));
    read_rom_packet(handle, byte_addr, TIMEOUT)
}

/// Drain any stale EP2 packets left over from earlier commands.
fn drain_ep2(handle: &DeviceHandle<GlobalContext>) {
    let mut buf = [0u8; 64];
    for _ in 0..64 {
        if handle
            .read_bulk(0x82, &mut buf, Duration::from_millis(40))
            .is_err()
        {
            break;
        }
    }
}

/// Read a chunk and require two consecutive reads to agree, retrying a few
/// times before giving up.
///
/// A stale EP2 buffer or a marginal cartridge returns a *full-length* but wrong
/// 64 bytes, which a single read cannot detect — that is how a dump ends up
/// silently corrupt. Two agreeing reads cannot prove correctness, but they do
/// catch the stale-buffer and unstable-cart cases.
fn rom_read_chunk_confirmed(
    handle: &DeviceHandle<GlobalContext>,
    byte_addr: u32,
    delay_ms: u64,
) -> Result<[u8; 64]> {
    chunk_read_confirmed(byte_addr, true, || {
        rom_read_chunk(handle, byte_addr, delay_ms)
    })
}

/// Confirm/retry loop shared by the ROM and save readers.
///
/// The firmware streams through a single EP2 buffer, so a mistimed or marginal
/// read returns a *full-length* but stale block. Nothing in a single read
/// distinguishes that from real data, so the only defence is to read twice and
/// compare. With `confirm` false only hard errors are retried: faster, but a bad
/// read can reach the output file unnoticed.
fn chunk_read_confirmed<F>(addr: u32, confirm: bool, mut read: F) -> Result<[u8; 64]>
where
    F: FnMut() -> Result<[u8; 64]>,
{
    let mut last_error: Option<anyhow::Error> = None;

    for attempt in 1..=ROM_READ_ATTEMPTS {
        let first = match read() {
            Ok(v) => v,
            Err(e) => {
                last_error = Some(e);
                std::thread::sleep(Duration::from_millis(20));
                continue;
            }
        };

        if !confirm {
            return Ok(first);
        }

        match read() {
            Ok(second) if second == first => return Ok(first),
            Ok(second) => {
                last_error = Some(anyhow::anyhow!(
                    "two reads disagreed on attempt {attempt}/{ROM_READ_ATTEMPTS} \
                     (first: {} / second: {})",
                    hex_prefix(&first),
                    hex_prefix(&second)
                ));
            }
            Err(e) => last_error = Some(e),
        }
        std::thread::sleep(Duration::from_millis(20));
    }

    match last_error {
        Some(e) => Err(e.context(format!(
            "unstable cartridge read at 0x{addr:06X} after {ROM_READ_ATTEMPTS} attempts"
        ))),
        None => bail!("unstable cartridge read at 0x{addr:06X}"),
    }
}

/// Detect the cartridge ROM size by finding where it mirrors onto itself.
///
/// The GBA exposes a 32 MB window; a smaller ROM ignores the address lines it
/// does not have, so offset 0 reappears at the ROM's own size (a 16 MB cart
/// mirrors at 0x1000000). Candidates are probed in ascending order and
/// confirmed at a second offset, so a coincidental 64-byte match cannot truncate
/// the dump. Defaulting to the full 32 MB window instead — as this command used
/// to — makes every 16 MB cart dump twice its real size, with the upper half
/// being a mirror image rather than ROM.
fn detect_rom_size(handle: &DeviceHandle<GlobalContext>, delay_ms: u64) -> Result<u32> {
    const SECOND_PROBE: u32 = 0x1000;

    let head = rom_read_chunk(handle, 0, delay_ms)
        .context("reading the cartridge header — is a cartridge inserted?")?;
    if head[4..8] != [0x24, 0xFF, 0xAE, 0x51] {
        bail!(
            "no GBA cartridge header at address 0 (got {:02x} {:02x} {:02x} {:02x}) — is a \
             cartridge inserted and seated properly?",
            head[4],
            head[5],
            head[6],
            head[7]
        );
    }
    let tail = rom_read_chunk(handle, SECOND_PROBE, delay_ms)?;

    mirrored_size(&head, &tail, |addr| rom_read_chunk(handle, addr, delay_ms))
}

/// ROM sizes a cartridge can plausibly have, smallest first.
const ROM_SIZE_CANDIDATES: [u32; 5] = [0x100000, 0x200000, 0x400000, 0x800000, 0x1000000];

/// Pick the smallest candidate size whose data mirrors `head`/`tail`.
///
/// Both probe points must match, so a chance 64-byte coincidence at one offset
/// cannot silently halve the dump.
fn mirrored_size<F>(head: &[u8; 64], tail: &[u8; 64], mut probe: F) -> Result<u32>
where
    F: FnMut(u32) -> Result<[u8; 64]>,
{
    const SECOND_PROBE: u32 = 0x1000;

    for size in ROM_SIZE_CANDIDATES {
        if probe(size)? != *head {
            continue;
        }
        if probe(size + SECOND_PROBE)? == *tail {
            return Ok(size);
        }
    }
    Ok(ROM_WINDOW_BYTES as u32)
}

/// Re-read the cartridge and compare it byte-for-byte against the file just
/// written. This is the test that separates the two failure modes users report
/// as "corrupted dump":
///
///   * every run differs at the same offsets, or differs from a known-good
///     image  -> the cartridge is not what the tool thinks it is
///     (bootleg/repro PCB, ROM hack, wrong ROM size).
///   * a second read disagrees with the first -> the reads themselves are
///     unstable (failing cart, marginal battery, bad cable/hub).
///
/// Returns an error when any chunk differs, so scripts and CI see a non-zero
/// exit code instead of a silently corrupt dump.
fn verify_dump(
    handle: &DeviceHandle<GlobalContext>,
    output: &Path,
    start_addr: u32,
    written: u64,
    delay_ms: u64,
) -> Result<()> {
    use std::io::Read;
    use std::io::Write as _;

    if written == 0 {
        bail!("nothing was dumped, so there is nothing to verify");
    }

    let total_chunks = written.div_ceil(64);
    println!("\nVerifying: re-reading {written} bytes from the cartridge...");

    let mut file = fs::File::open(output)
        .with_context(|| format!("re-open {} for verification", output.display()))?;

    let mut offset: u64 = 0;
    let mut mismatched: u64 = 0;
    let mut first_mismatch: Option<(u64, [u8; 64], [u8; 64])> = None;
    let mut file_buf = [0u8; 64];
    let mut last_pct = 0u64;

    while offset < written {
        let want = std::cmp::min(64, (written - offset) as usize) as usize;
        file.read_exact(&mut file_buf[..want])
            .with_context(|| format!("read {} at 0x{offset:06X}", output.display()))?;

        let cart = rom_read_chunk(handle, start_addr + offset as u32, delay_ms)?;
        if cart[..want] != file_buf[..want] {
            mismatched += 1;
            if first_mismatch.is_none() {
                let mut file_copy = [0u8; 64];
                file_copy[..want].copy_from_slice(&file_buf[..want]);
                first_mismatch = Some((offset, cart, file_copy));
            }
        }

        offset += want as u64;
        let pct = (offset * 100) / written;
        if pct != last_pct {
            last_pct = pct;
            print!("\r  Verifying: {pct}%");
            std::io::stdout().flush()?;
        }
    }
    println!();

    if let Some((off, cart, file_copy)) = first_mismatch {
        println!("  First mismatch at 0x{off:06X}:");
        println!("    cartridge: {}", hex_prefix(&cart));
        println!("    file:      {}", hex_prefix(&file_copy));
        bail!(
            "VERIFY FAILED: {mismatched} of {total_chunks} chunks differ between the cartridge and \
             {}. The cartridge is unstable, or it is not the ROM the tool assumed. Re-run the dump \
             to see whether the mismatches move around (unstable reads) or stay at the same \
             offsets (wrong/patched image).",
            output.display()
        );
    }

    println!(
        "  Verify OK: cartridge matches {} ({total_chunks} chunks).",
        output.display()
    );
    Ok(())
}

fn cmd_save_write(
    input: PathBuf,
    byte_addr: u32,
    save_type: char,
    write_cmd: u8,
    erase_cmd: u8,
) -> Result<()> {
    let data =
        fs::read(&input).with_context(|| format!("reading save file: {}", input.display()))?;
    let (device, _desc) = find_device(EZWRITER_VID, EZWRITER_PID)?;
    let handle = device.open()?;
    let config = device.active_config_descriptor()?;
    for iface in config.interfaces() {
        for iface_desc in iface.descriptors() {
            let _ = handle.claim_interface(iface_desc.interface_number());
        }
    }
    for ep in 0x01u8..=0x07u8 {
        let _ = handle.clear_halt(ep);
        let _ = handle.clear_halt(ep | 0x80);
    }

    let cmd_ep = 0x04;
    let suffix = save_type as u8;

    let select_cmd = [0x14u8, suffix, 0x00];
    handle.write_bulk(cmd_ep, &select_cmd, TIMEOUT)?;
    std::thread::sleep(Duration::from_millis(50));

    println!(
        "Writing {} bytes to save (type='{}') at offset 0x{:X}",
        data.len(),
        save_type,
        byte_addr
    );

    if save_type == 'f' || save_type == 'F' {
        let sector_size = 4096u32;
        let start_sector = byte_addr / sector_size;
        let end_sector = (byte_addr + data.len() as u32).div_ceil(sector_size);
        println!("  Erasing {} sectors...", end_sector - start_sector);
        for sector in start_sector..end_sector {
            let sec_addr = sector * sector_size;
            let erase = [
                erase_cmd,
                (sec_addr & 0xFF) as u8,
                ((sec_addr >> 8) & 0xFF) as u8,
                ((sec_addr >> 16) & 0xFF) as u8,
                suffix,
            ];
            let _ = handle.write_bulk(cmd_ep, &erase[..5], TIMEOUT);
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    for (i, chunk) in data.chunks(64).enumerate() {
        let addr = byte_addr + (i * 64) as u32;
        let mut cmd = vec![
            write_cmd,
            (addr & 0xFF) as u8,
            ((addr >> 8) & 0xFF) as u8,
            ((addr >> 16) & 0xFF) as u8,
            suffix,
        ];
        cmd.extend_from_slice(chunk);
        handle.write_bulk(cmd_ep, &cmd, TIMEOUT)?;
        std::thread::sleep(Duration::from_millis(10));

        let mut status = [0u8; 64];
        let _ = handle.read_bulk(0x82, &mut status, Duration::from_millis(20));

        if i % 64 == 0 || i + 1 == data.len().div_ceil(64) {
            println!("  Written {}/{} bytes", (i + 1) * 64, data.len());
        }
    }

    println!(
        "  Save write complete: {} bytes to {}",
        data.len(),
        input.display()
    );
    Ok(())
}

fn cmd_rom_write(
    input: PathBuf,
    byte_addr: u32,
    delay_ms: u64,
    no_erase: bool,
    write_cmd: u8,
    erase_cmd: u8,
) -> Result<()> {
    let data =
        fs::read(&input).with_context(|| format!("reading ROM file: {}", input.display()))?;
    let (device, _desc) = find_device(EZWRITER_VID, EZWRITER_PID)?;
    let handle = device.open()?;
    let config = device.active_config_descriptor()?;
    for iface in config.interfaces() {
        for iface_desc in iface.descriptors() {
            let _ = handle.claim_interface(iface_desc.interface_number());
        }
    }
    for ep in 0x01u8..=0x07u8 {
        let _ = handle.clear_halt(ep);
        let _ = handle.clear_halt(ep | 0x80);
    }

    let cmd_ep = 0x04;
    let data_ep = 0x82;
    let delay = Duration::from_millis(delay_ms);

    println!(
        "Writing {} bytes to ROM at offset 0x{:X}",
        data.len(),
        byte_addr
    );

    let seq: [(u8, u16); 4] = [(0xAA, 0xAAAA), (0x55, 0x5554), (0xF0, 0xAAAA), (0xFF, 0)];
    for (cb, a) in &seq {
        let da = a / 2;
        let c = [*cb, (da & 0xFF) as u8, ((da >> 8) & 0xFF) as u8, 0x00];
        let _ = handle.write_bulk(cmd_ep, &c, Duration::from_millis(500));
        std::thread::sleep(Duration::from_millis(5));
    }

    if !no_erase {
        let sector_size = 65536u32;
        let start_sector = byte_addr / sector_size;
        let end_sector = (byte_addr + data.len() as u32).div_ceil(sector_size);
        println!("  Erasing sectors {start_sector}..{end_sector}...");
        for sector in start_sector..end_sector {
            let sec_addr = sector * sector_size;
            let word_addr = sec_addr / 2;
            let erase = [
                erase_cmd,
                (word_addr & 0xFF) as u8,
                ((word_addr >> 8) & 0xFF) as u8,
                ((word_addr >> 16) & 0xFF) as u8,
            ];
            handle.write_bulk(cmd_ep, &erase, TIMEOUT)?;
            std::thread::sleep(Duration::from_millis(100));
            let _ = handle.read_bulk(data_ep, &mut [0u8; 64], Duration::from_secs(1));
        }
    }

    for (i, chunk) in data.chunks(64).enumerate() {
        let addr = byte_addr + (i * 64) as u32;
        let word_addr = addr / 2;
        let bank = (word_addr >> 16) as u8;

        let mut cmd = vec![
            write_cmd,
            (word_addr & 0xFF) as u8,
            ((word_addr >> 8) & 0xFF) as u8,
            bank,
        ];
        cmd.extend_from_slice(chunk);
        handle.write_bulk(cmd_ep, &cmd, TIMEOUT)?;
        std::thread::sleep(delay);

        let _ = handle.read_bulk(data_ep, &mut [0u8; 64], Duration::from_millis(50));

        if i % 256 == 0 || i + 1 == data.len().div_ceil(64) {
            println!("  Written {}/{} bytes", (i + 1) * 64, data.len());
        }
    }

    println!(
        "  ROM write complete: {} bytes to {}",
        data.len(),
        input.display()
    );
    Ok(())
}

fn cmd_bulk_test() -> Result<()> {
    let (device, _desc) = if let Ok(d) = find_device(EZWRITER_VID, EZWRITER_PID) {
        println!("Device in ACTIVE mode.");
        d
    } else if let Ok(d) = find_device(BOOTLOADER_VID, BOOTLOADER_PID) {
        println!("Device in BOOTLOADER mode.");
        d
    } else {
        bail!("No EZ-Writer device found.");
    };

    let handle = device.open()?;
    let config = device.active_config_descriptor()?;
    for iface in config.interfaces() {
        for iface_desc in iface.descriptors() {
            let _ = handle.claim_interface(iface_desc.interface_number());
        }
    }

    let out_eps: [u8; 7] = [0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07];
    let in_eps: [u8; 7] = [0x81, 0x82, 0x83, 0x84, 0x85, 0x86, 0x87];
    let test_cmd = [0x01u8, 0x00, 0x00, 0x00];

    println!("Testing OUT endpoints...");
    for ep in out_eps {
        match handle.write_bulk(ep, &test_cmd, Duration::from_millis(200)) {
            Ok(_) => println!("  EP 0x{ep:02X} OUT: OK"),
            Err(e) => println!("  EP 0x{ep:02X} OUT: {e}"),
        }
    }

    // Clear any pending data
    for ep in in_eps {
        let mut buf = [0u8; 64];
        let _ = handle.read_bulk(ep, &mut buf, Duration::from_millis(50));
    }

    // Test which IN endpoints respond to a command
    println!("\nSending cmd 0x01 to EP 0x04, checking all IN endpoints...");
    handle.write_bulk(0x04, &test_cmd, TIMEOUT)?;
    std::thread::sleep(Duration::from_millis(10));
    for ep in in_eps {
        let mut buf = [0u8; 64];
        match handle.read_bulk(ep, &mut buf, Duration::from_millis(200)) {
            Ok(len) => {
                let h: String = buf[..8]
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<Vec<_>>()
                    .join(" ");
                println!("  EP 0x{ep:02X} IN: {len} bytes, first 8: {h}...");
            }
            Err(rusb::Error::Timeout) => println!("  EP 0x{ep:02X} IN: timeout"),
            Err(rusb::Error::Pipe) => println!("  EP 0x{ep:02X} IN: stall"),
            Err(e) => println!("  EP 0x{ep:02X} IN: {e}"),
        }
    }

    Ok(())
}

fn cmd_write_reg(addr: u32, value: u16) -> Result<()> {
    let (device, _desc) = find_device(EZWRITER_VID, EZWRITER_PID)?;
    let handle = device.open()?;
    let config = device.active_config_descriptor()?;
    for iface in config.interfaces() {
        for iface_desc in iface.descriptors() {
            let _ = handle.claim_interface(iface_desc.interface_number());
        }
    }

    let cmd_ep = 0x04;
    let cmd = [
        0x19u8,
        (addr & 0xFF) as u8,
        ((addr >> 8) & 0xFF) as u8,
        ((addr >> 16) & 0xFF) as u8,
        (value & 0xFF) as u8,
        ((value >> 8) & 0xFF) as u8,
    ];
    println!("WriteReg: addr=0x{addr:06X} val=0x{value:04X}");
    handle.write_bulk(cmd_ep, &cmd, TIMEOUT)?;
    println!("OK");
    Ok(())
}

fn cmd_read_reg(addr: u32) -> Result<()> {
    let (device, _desc) = find_device(EZWRITER_VID, EZWRITER_PID)?;
    let handle = device.open()?;
    let config = device.active_config_descriptor()?;
    for iface in config.interfaces() {
        for iface_desc in iface.descriptors() {
            let _ = handle.claim_interface(iface_desc.interface_number());
        }
    }

    let cmd_ep = 0x04;
    let data_ep = 0x82;
    let cmd = [
        0x1Au8,
        (addr & 0xFF) as u8,
        ((addr >> 8) & 0xFF) as u8,
        ((addr >> 16) & 0xFF) as u8,
    ];
    println!("ReadReg: addr=0x{addr:06X}");
    handle.write_bulk(cmd_ep, &cmd, TIMEOUT)?;
    std::thread::sleep(Duration::from_millis(10));
    let mut buf = [0u8; 64];
    match handle.read_bulk(data_ep, &mut buf, TIMEOUT) {
        Ok(len) => {
            let hex = buf[..len.min(8)]
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<Vec<_>>()
                .join(" ");
            println!("  Response ({} bytes): {}", len, hex);
            if len >= 2 {
                println!(
                    "  Word value: 0x{:04X}",
                    u16::from_le_bytes([buf[0], buf[1]])
                );
            }
        }
        Err(e) => println!("  Error: {e}"),
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Firmware reload helpers
// ---------------------------------------------------------------------------

fn run_init_exact_embedded(handle: &DeviceHandle<GlobalContext>) -> Result<()> {
    let table1 = load_loader_table("loader_table1.bin")?;
    let table2 = load_loader_table("loader_table2.bin")?;
    let chunks1 = parse_chunk_table(&table1)?;
    let chunks2 = parse_chunk_table(&table2)?;
    cpucs(handle, 1)?;
    cpucs(handle, 1)?;
    write_chunks(handle, "table1", &chunks1)?;
    cpucs(handle, 0)?;
    cpucs(handle, 1)?;
    write_chunks(handle, "table2", &chunks2)?;
    cpucs(handle, 1)?;
    cpucs(handle, 0)?;
    Ok(())
}

fn wait_for_mode(vid: u16, pid: u16, timeout_secs: u64) -> bool {
    let deadline = std::time::Instant::now() + Duration::from_secs(timeout_secs);
    while std::time::Instant::now() < deadline {
        if find_device(vid, pid).is_ok() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    false
}

#[cfg(target_os = "windows")]
fn power_cycle_windows(vid: &str, pid: &str) -> Result<()> {
    println!("Power cycling via Windows PnP manager...");
    let script = format!(
        r#"
$dev = Get-PnpDevice -PresentOnly | Where-Object {{ $_.HardwareID -match 'VID_{vid}.*PID_{pid}' }};
if (-not $dev) {{ Write-Error 'Device not found'; exit 1 }}
Disable-PnpDevice -InstanceId $dev.InstanceId -Confirm:$false;
Start-Sleep -Milliseconds 1000;
Enable-PnpDevice -InstanceId $dev.InstanceId -Confirm:$false;
Write-Output "Power cycled OK"
"#
    );
    let out = std::process::Command::new("powershell")
        .args(["-Command", &script])
        .output()?;
    if !out.status.success() {
        bail!(
            "PowerShell power cycle failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    println!("{}", String::from_utf8_lossy(&out.stdout).trim());
    Ok(())
}

#[cfg(target_os = "linux")]
fn power_cycle_linux(device: &rusb::Device<GlobalContext>) -> Result<()> {
    println!("Power cycling via sysfs...");
    let bus = device.bus_number();
    let ports = device.port_numbers()?;
    let sysfs_path = if ports.is_empty() {
        format!("/sys/bus/usb/devices/usb{bus}")
    } else {
        let port_str: Vec<String> = ports.iter().map(|p| p.to_string()).collect();
        format!("/sys/bus/usb/devices/{bus}-{}", port_str.join("."))
    };
    let auth = format!("{sysfs_path}/authorized");
    println!("  sysfs path: {auth}");
    let out = std::process::Command::new("pkexec")
        .args([
            "sh",
            "-c",
            &format!("echo 0 > {auth} && sleep 0.5 && echo 1 > {auth}"),
        ])
        .output()?;
    if !out.status.success() {
        bail!(
            "sysfs power cycle failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(())
}

/// macOS has no sysfs-style per-port power switch and no stock CLI for the
/// Windows PnP disable/enable trick, so this falls back to a raw USB bus
/// reset (`libusb_reset_device`) on the still-open handle. That's a weaker
/// signal than an actual power cycle — it resets the USB PHY/link, not the
/// device's power rail — so it may not force re-enumeration on every AN2131
/// revision. Unverified on real hardware; needs confirmation from a Mac user.
#[cfg(target_os = "macos")]
fn power_cycle_macos(handle: &DeviceHandle<GlobalContext>) -> Result<()> {
    println!("No macOS power-cycle API available; attempting a USB bus reset instead...");
    handle
        .reset()
        .context("USB bus reset failed (unplug/replug the cable manually)")?;
    Ok(())
}

fn cmd_reload() -> Result<()> {
    // Step 1: try CPUCS vendor request reset while in active mode
    // This works if the USB auto-vector ISR is still running despite the 8051 being stuck
    let active = find_device(EZWRITER_VID, EZWRITER_PID);
    let in_boot = find_device(BOOTLOADER_VID, BOOTLOADER_PID).is_ok();

    if let Ok((device, _)) = active {
        println!("Device in active mode. Sending CPUCS reset...");
        let handle = device.open()?;
        let _ = handle.detach_kernel_driver(0);
        // AN2131 CPUCS=0x7F92, bit0 8051RES: 1=hold in reset
        let _ = handle.write_control(
            0x40,
            VR_CYPRESS_WRITE,
            CPUCS_ADDR,
            0,
            &[0x01],
            Duration::from_millis(500),
        );
        drop(handle);
        std::thread::sleep(Duration::from_millis(1500));

        // If still in active mode, need OS-level power cycle
        if find_device(EZWRITER_VID, EZWRITER_PID).is_ok() {
            println!("CPUCS reset didn't trigger re-enumeration — using OS power cycle...");
            #[cfg(target_os = "windows")]
            power_cycle_windows("0548", "1005")?;
            #[cfg(target_os = "linux")]
            {
                let (dev, _) = find_device(EZWRITER_VID, EZWRITER_PID)?;
                power_cycle_linux(&dev)?;
            }
            #[cfg(target_os = "macos")]
            {
                let (dev, _) = find_device(EZWRITER_VID, EZWRITER_PID)?;
                let h = dev.open()?;
                power_cycle_macos(&h)?;
            }
            #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
            bail!("OS power cycle not supported on this platform. Unplug and replug manually.");
        }
    } else if in_boot {
        println!("Device already in bootloader mode, skipping reset.");
    } else {
        bail!("No EZ-Writer device found.");
    }

    // Step 2: wait for bootloader
    println!("Waiting for bootloader (up to 10s)...");
    if !wait_for_mode(BOOTLOADER_VID, BOOTLOADER_PID, 10) {
        bail!("Device did not enter bootloader mode. Try unplugging and replugging manually.");
    }
    println!("Bootloader detected.");

    // Step 3: run init-exact with embedded tables
    let (device, _desc) = find_device(BOOTLOADER_VID, BOOTLOADER_PID)?;
    let handle = device.open()?;
    let _ = handle.detach_kernel_driver(0);
    let config = device.active_config_descriptor()?;
    if let Some(iface) = config.interfaces().next()
        && let Some(desc) = iface.descriptors().next()
    {
        let _ = handle.claim_interface(desc.interface_number());
    }
    run_init_exact_embedded(&handle)?;
    drop(handle);

    // Step 4: wait for active mode
    println!("Waiting for active mode (up to 10s)...");
    if !wait_for_mode(EZWRITER_VID, EZWRITER_PID, 10) {
        bail!("Device did not enter active mode after firmware load.");
    }
    println!("Firmware loaded. Device ready.");
    Ok(())
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::List => {
            let found_boot = find_device(BOOTLOADER_VID, BOOTLOADER_PID);
            let found_active = find_device(EZWRITER_VID, EZWRITER_PID);

            match (found_boot, found_active) {
                (Ok(_), _) => println!(
                    "Bootloader mode:   EZ-Writer detected (VID 0x{BOOTLOADER_VID:04x}:PID 0x{BOOTLOADER_PID:04x})"
                ),
                (_, Ok(_)) => println!(
                    "Active mode:       EZ-Writer detected (VID 0x{EZWRITER_VID:04x}:PID 0x{EZWRITER_PID:04x})"
                ),
                _ => println!("No EZ-Writer device found."),
            }
        }
        Commands::Info => {
            let (device, desc) = find_device(EZWRITER_VID, EZWRITER_PID)
                .or_else(|_| find_device(BOOTLOADER_VID, BOOTLOADER_PID))?;
            let handle = device.open()?;
            let _ = handle.detach_kernel_driver(0);
            print_device_info(&desc, &handle)?;
        }
        Commands::FirmwareDownload { fw, no_cpu } => {
            let (device, _desc) = find_device(BOOTLOADER_VID, BOOTLOADER_PID)?;
            let handle = device.open()?;
            let _ = handle.detach_kernel_driver(0);
            println!("Found EZ-Writer in bootloader mode.");
            let fw_data =
                fs::read(&fw).with_context(|| format!("reading firmware: {}", fw.display()))?;
            download_firmware(&handle, &fw_data, no_cpu)?;

            if !no_cpu && !wait_for_mode(EZWRITER_VID, EZWRITER_PID, 5) {
                println!(
                    "Device didn't re-enumerate as ACTIVE (0x{EZWRITER_VID:04x}:0x{EZWRITER_PID:04x}) \
                     after 5s — CPU may be running but the host missed the USB reconnect."
                );
                println!("Attempting OS-level power cycle...");
                #[cfg(target_os = "windows")]
                let cycled = power_cycle_windows("0547", "2131").is_ok();
                #[cfg(target_os = "linux")]
                let cycled = power_cycle_linux(&device).is_ok();
                #[cfg(target_os = "macos")]
                let cycled = power_cycle_macos(&handle).is_ok();
                #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
                let cycled = false;

                if cycled && wait_for_mode(EZWRITER_VID, EZWRITER_PID, 5) {
                    println!("Device is now ACTIVE.");
                } else {
                    println!(
                        "Still not ACTIVE. Unplug and replug the USB cable, then run `list` again."
                    );
                }
            }
        }
        Commands::InitExact { table1, table2 } => cmd_init_exact(&table1, &table2)?,
        Commands::CartInfo => cmd_cart_info()?,
        Commands::SaveRead {
            addr,
            count,
            save_type,
            output,
            word_addr,
            use_reg,
            use_rom_read,
            rom_offset,
            no_confirm,
        } => cmd_save_read(
            addr,
            count,
            save_type,
            output,
            word_addr,
            use_reg,
            use_rom_read,
            rom_offset,
            !no_confirm,
        )?,
        Commands::SaveProbe { count } => cmd_save_probe(count)?,
        Commands::CartRead {
            addr,
            count,
            cmd,
            bank,
            byte3_bank,
        } => cmd_cart_read(addr, count, cmd, bank, byte3_bank)?,
        Commands::Dump {
            output,
            start,
            size,
            delay,
            fast,
            pipeline,
            verify,
            no_confirm,
        } => cmd_dump(
            output,
            start,
            size,
            delay,
            fast,
            pipeline,
            verify,
            !no_confirm,
        )?,
        Commands::SaveId => cmd_save_id()?,
        Commands::Bench { chunks, depths } => cmd_bench(chunks, depths)?,
        Commands::Reset => cmd_reset()?,
        Commands::Reload => cmd_reload()?,
        Commands::Probe { request, value } => cmd_probe(request, value)?,
        Commands::RamRead { address } => cmd_ram_read(address)?,
        Commands::RamWrite { address, value } => cmd_ram_write(address, value)?,
        Commands::PassiveRead => cmd_passive_read()?,
        Commands::ResetCart => cmd_reset_cart()?,
        Commands::SaveWrite {
            input,
            addr,
            save_type,
            write_cmd,
            erase_cmd,
        } => cmd_save_write(input, addr, save_type, write_cmd, erase_cmd)?,
        Commands::RomWrite {
            input,
            addr,
            delay,
            no_erase,
            write_cmd,
            erase_cmd,
        } => cmd_rom_write(input, addr, delay, no_erase, write_cmd, erase_cmd)?,
        Commands::BulkTest => cmd_bulk_test()?,
        Commands::WriteReg { addr, value } => cmd_write_reg(addr, value)?,
        Commands::ReadReg { addr } => cmd_read_reg(addr)?,
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk(seed: u8) -> [u8; 64] {
        let mut c = [0u8; 64];
        for (i, b) in c.iter_mut().enumerate() {
            *b = seed.wrapping_add(i as u8);
        }
        c
    }

    #[test]
    fn confirmed_read_retries_transient_failures() {
        let mut calls = 0;
        let got = chunk_read_confirmed(0x40, true, || {
            calls += 1;
            if calls == 1 {
                bail!("transient USB error");
            }
            Ok(chunk(7))
        })
        .unwrap();
        assert_eq!(got, chunk(7));
        assert_eq!(calls, 3, "one failed read, then the confirming pair");
    }

    #[test]
    fn confirmed_read_rejects_a_cartridge_that_never_settles() {
        let mut calls = 0;
        let err = chunk_read_confirmed(0x80, true, || {
            calls += 1;
            Ok(chunk(calls as u8))
        })
        .unwrap_err();
        assert!(
            err.to_string().contains("unstable cartridge read"),
            "unexpected error: {err}"
        );
        // Two reads per attempt, every attempt used.
        assert_eq!(calls, (ROM_READ_ATTEMPTS * 2) as usize);
    }

    #[test]
    fn unconfirmed_read_takes_the_first_result() {
        let mut calls = 0;
        let got = chunk_read_confirmed(0, false, || {
            calls += 1;
            Ok(chunk(calls as u8))
        })
        .unwrap();
        assert_eq!(calls, 1);
        assert_eq!(got, chunk(1));
    }

    #[test]
    fn mirrored_size_finds_the_smallest_confirmed_mirror() {
        let head = chunk(1);
        let tail = chunk(2);
        // A 16 MB cart: only offset 0x1000000 mirrors the probes.
        let size = mirrored_size(&head, &tail, |addr| {
            Ok(match addr {
                a if a == 0x1000000 => head,
                a if a == 0x1000000 + 0x1000 => tail,
                _ => chunk(9),
            })
        })
        .unwrap();
        assert_eq!(size, 0x1000000);
    }

    #[test]
    fn mirrored_size_ignores_a_single_offset_coincidence() {
        let head = chunk(1);
        let tail = chunk(2);
        // Offset 0x800000 happens to match the header probe but not the second
        // probe, so the real 16 MB mirror must still win.
        let size = mirrored_size(&head, &tail, |addr| {
            Ok(match addr {
                0x800000 => head,
                a if a == 0x1000000 => head,
                a if a == 0x1000000 + 0x1000 => tail,
                _ => chunk(9),
            })
        })
        .unwrap();
        assert_eq!(size, 0x1000000);
    }

    #[test]
    fn mirrored_size_falls_back_to_the_full_window() {
        let head = chunk(1);
        let tail = chunk(2);
        let size = mirrored_size(&head, &tail, |_| Ok(chunk(9))).unwrap();
        assert_eq!(size, ROM_WINDOW_BYTES as u32);
    }

    #[test]
    fn partial_path_appends_suffix() {
        let p = partial_path(Path::new("/tmp/emerald.gba")).unwrap();
        assert_eq!(p, PathBuf::from("/tmp/emerald.gba.partial"));
    }
}

/// Benchmark the ROM read path on the connected hardware.
///
/// Read-only. Answers the three questions that decide how fast a dump can go:
/// how long one command/response round trip costs, whether the firmware ever
/// streams more than one packet per command, and how much of the round trip a
/// host-side command queue can hide.
fn cmd_bench(chunks: u32, depths: Vec<usize>) -> Result<()> {
    use std::time::Instant;

    if chunks == 0 {
        bail!("--chunks must be at least 1");
    }
    if chunks > 0x4000 {
        bail!("--chunks is capped at 16384 (1 MB) to stay inside the first ROM bank");
    }
    let bytes = chunks as u64 * 64;

    let (device, _desc) = find_device(EZWRITER_VID, EZWRITER_PID)?;
    println!("Found EZ-Writer active mode.");
    let handle = device.open()?;
    let config = device.active_config_descriptor()?;
    for iface in config.interfaces() {
        for iface_desc in iface.descriptors() {
            let _ = handle.claim_interface(iface_desc.interface_number());
        }
    }
    for ep in 0x01u8..=0x07u8 {
        let _ = handle.clear_halt(ep);
        let _ = handle.clear_halt(ep | 0x80);
    }

    // Same preamble as `dump`, minus anything that writes to the cartridge.
    let seq: [(u8, u16); 4] = [(0xAA, 0xAAAA), (0x55, 0x5554), (0xF0, 0xAAAA), (0xFF, 0)];
    for (cb, a) in &seq {
        let da = a / 2;
        let c = [*cb, (da & 0xFF) as u8, ((da >> 8) & 0xFF) as u8, 0x00];
        let _ = handle.write_bulk(0x04, &c, Duration::from_millis(500));
        std::thread::sleep(Duration::from_millis(5));
    }

    println!(
        "\nEZ-Writer read benchmark — {chunks} chunks ({} KB) per measurement\n",
        bytes / 1024
    );

    // --- 1. Does one command produce more than one packet? ----------------
    println!("1) Auto-stream probe: one command, then read until the endpoint is idle");
    drain_ep2(&handle);
    send_rom_read_cmd(&handle, 0)?;
    let probe_start = Instant::now();
    let mut packets = 0u32;
    loop {
        let mut buf = [0u8; 64];
        match handle.read_bulk(0x82, &mut buf, Duration::from_millis(100)) {
            Ok(64) => {
                packets += 1;
                if packets >= 512 {
                    break;
                }
            }
            _ => break,
        }
    }
    println!(
        "   {packets} packet(s) arrived from a single command in {:?}",
        probe_start.elapsed()
    );
    if packets <= 1 {
        println!("   -> one 64-byte packet per command: speed is round-trip bound, not bus bound");
    } else {
        println!("   -> the firmware streams without further commands; a read-only loop wins");
    }

    // --- 2. Per-chunk round-trip latency ----------------------------------
    println!("\n2) Round trip: send command, wait for its packet, repeat");
    drain_ep2(&handle);
    let mut times = Vec::with_capacity(chunks as usize);
    let seq_start = Instant::now();
    for i in 0..chunks {
        let addr = i * 64;
        let t = Instant::now();
        send_rom_read_cmd(&handle, addr)?;
        read_rom_packet(&handle, addr, TIMEOUT)?;
        times.push(t.elapsed());
    }
    let seq_elapsed = seq_start.elapsed();
    times.sort_unstable();
    let median = times[times.len() / 2];
    let p95 = times[(times.len() * 95) / 100];
    let fastest = times[0];
    let per_chunk = seq_elapsed.as_secs_f64() / chunks as f64;
    println!("   per chunk: min {fastest:?}, median {median:?}, p95 {p95:?}");
    println!(
        "   sustained {:.1} KB/s -> 16 MB in {:.1} min",
        (bytes as f64 / 1024.0) / seq_elapsed.as_secs_f64(),
        (16.0 * 1024.0 * 1024.0 / 64.0 * per_chunk) / 60.0
    );

    // --- 3. Pipelined throughput at several depths -------------------------
    println!("\n3) Pipelined: keep N commands in flight");
    println!(
        "   {:>6}  {:>12}  {:>12}  {:>14}",
        "depth", "KB/s", "16 MB in", "vs depth 1"
    );

    let mut baseline: Option<f64> = None;
    for depth in depths {
        let depth = depth.max(1);
        drain_ep2(&handle);

        let t = Instant::now();
        let mut issued: u32 = 0;
        let mut received: u32 = 0;
        let mut failed: Option<String> = None;

        while issued < chunks && issued < depth as u32 {
            if let Err(e) = send_rom_read_cmd(&handle, issued * 64) {
                failed = Some(format!("could not queue depth {depth}: {e}"));
                break;
            }
            issued += 1;
        }

        if failed.is_none() {
            while received < chunks {
                if let Err(e) = read_rom_packet(&handle, received * 64, TIMEOUT) {
                    failed = Some(format!("read failed at depth {depth}: {e}"));
                    break;
                }
                received += 1;
                if issued < chunks {
                    if let Err(e) = send_rom_read_cmd(&handle, issued * 64) {
                        failed = Some(format!("could not keep depth {depth}: {e}"));
                        break;
                    }
                    issued += 1;
                }
            }
        }

        if let Some(reason) = failed {
            println!("   {depth:>6}  {reason}");
            continue;
        }

        let elapsed = t.elapsed().as_secs_f64();
        let kb_s = (bytes as f64 / 1024.0) / elapsed;
        let mins = (16.0 * 1024.0 * 1024.0 / 1024.0) / kb_s / 60.0;
        let speedup = match baseline {
            Some(b) if b > 0.0 => format!("{:.1}x", kb_s / b),
            _ => {
                baseline = Some(kb_s);
                "baseline".to_string()
            }
        };
        println!(
            "   {depth:>6}  {kb_s:>12.1}  {:>9.1} min  {speedup:>14}",
            mins
        );
    }

    println!(
        "\nNote: the AN2131 is a USB 1.1 full-speed device (12 Mbit/s), so ~1.2 MB/s is the\n\
         hard ceiling — 16 MB cannot beat roughly 14 s. `dump --pipeline N` uses the depth\n\
         that measured best here."
    );
    Ok(())
}
