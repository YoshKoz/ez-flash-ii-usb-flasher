use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use rusb::{Device, DeviceDescriptor, DeviceHandle, Direction, GlobalContext};
use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;
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
    InitExact {
        table1: PathBuf,
        table2: PathBuf,
    },
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
    RamRead {
        address: u16,
    },
    /// Write to internal RAM via vendor request
    RamWrite {
        address: u16,
        value: u8,
    },
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
    bail!("Device {vid:#06x}:{pid:#06x} not found");
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
    if let Ok(s) = handle.read_manufacturer_string_ascii(&desc) {
        println!("  Manufacturer: {s}");
    }
    if let Ok(s) = handle.read_product_string_ascii(&desc) {
        println!("  Product: {s}");
    }
    if let Ok(s) = handle.read_serial_number_string_ascii(&desc) {
        println!("  Serial: {s}");
    }
    println!("  Device class: {:#04x}", desc.class_code());
    println!("  Device subclass: {:#04x}", desc.sub_class_code());
    println!("  Protocol: {:#04x}", desc.protocol_code());
    println!("  Max EP0 size: {}", desc.max_packet_size());
    println!("  Num configs: {}", desc.num_configurations());
    // Active config
    if let Ok(config) = device_config_descriptor(&handle, 0) {
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
            wval,
            windex,
            &mut buf,
            TIMEOUT,
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
        handle.write_control(
            0x40,
            VR_CYPRESS_WRITE,
            wval,
            windex,
            chunk,
            TIMEOUT,
        )?;
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

fn load_chunk_table(path: &PathBuf) -> Result<Vec<(u16, Vec<u8>)>> {
    let data = fs::read(path).with_context(|| format!("Failed to read chunk table: {:?}", path))?;
    if data.len() < 10 || &data[..8] != b"EZWLDR1\0" {
        bail!("Invalid chunk table: {}", path.display());
    }
    let count = u16::from_le_bytes([data[8], data[9]]) as usize;
    let mut chunks = Vec::with_capacity(count);
    let mut offset = 10;
    for _ in 0..count {
        if offset + 3 > data.len() {
            bail!("Truncated chunk table header: {}", path.display());
        }
        let addr = u16::from_le_bytes([data[offset], data[offset + 1]]);
        let len = data[offset + 2] as usize;
        offset += 3;
        if offset + len > data.len() {
            bail!("Truncated chunk table payload: {}", path.display());
        }
        chunks.push((addr, data[offset..offset + len].to_vec()));
        offset += len;
    }
    Ok(chunks)
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
    match handle.read_control(
        0xC0, request, value, 0, &mut buf, TIMEOUT,
    ) {
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
    match handle.read_control(
        0xC0,
        0xA3,
        address, 0, &mut buf, TIMEOUT,
    ) {
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
    match handle.write_control(
        0x40,
        0xA0,
        address, 0, &data, TIMEOUT,
    ) {
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
                let h: String = buf[..8].iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" ");
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
        (0xE00000u32, 0x0000u16),  // RAM page = 0
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
                let h: String = buf[..8].iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" ");
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

/// Try save read with word addressing (byte_addr/2) - matching ROM protocol
fn save_read_word_addr(
    handle: &DeviceHandle<GlobalContext>,
    data_ep: u8,
    cmd_ep: u8,
    byte_addr: u32,
    count: u32,
    suffix: u8,
) -> Result<Vec<u8>> {
    // Select save type
    let select_cmd = [0x14u8, suffix, 0x00];
    handle.write_bulk(cmd_ep, &select_cmd, TIMEOUT)?;
    std::thread::sleep(Duration::from_millis(50));

    let mut all = Vec::new();
    for chunk in 0..count {
        let addr = byte_addr + chunk * 64;
        let word_addr = addr / 2;  // Convert to word address like ROM
        let cmd = [
            0x02u8,
            (word_addr & 0xFF) as u8,
            ((word_addr >> 8) & 0xFF) as u8,
            ((word_addr >> 16) & 0xFF) as u8,
            suffix,
            0,
        ];
        handle.write_bulk(cmd_ep, &cmd[..5], TIMEOUT)?;
        std::thread::sleep(Duration::from_millis(200));

        let mut buf = [0u8; 64];
        match handle.read_bulk(data_ep, &mut buf, TIMEOUT) {
            Ok(len) => {
                all.extend_from_slice(&buf[..len]);
                let h: String = buf[..8].iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" ");
                println!("  [{chunk:02}] word=0x{:06X} byte=0x{:06X}: {}...", word_addr, addr, h);
            }
            Err(e) => {
                println!("  [{chunk:02}] {e}");
                break;
            }
        }
    }
    Ok(all)
}

// ---------------------------------------------------------------------------
// Save read - original method
// ---------------------------------------------------------------------------

fn cmd_save_read(
    byte_addr: u32,
    count: u32,
    save_type: char,
    output: Option<PathBuf>,
    use_word_addr: bool,
    use_reg: bool,
    use_rom_read: bool,
    rom_offset: u32,
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

    if use_rom_read {
        println!("Method: rom_read (0x01) at offset 0x{rom_offset:X}");
        let data = save_read_via_rom_read(&handle, data_ep, cmd_ep, byte_addr, count, rom_offset, suffix)?;
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

    if use_word_addr {
        println!("Method: word addressing (byte_addr/2)");
        let data = save_read_word_addr(&handle, data_ep, cmd_ep, byte_addr, count, suffix)?;
        if let Some(path) = output {
            fs::write(&path, &data)?;
            println!("Wrote {} bytes to {}", data.len(), path.display());
        }
        println!("Total: {} bytes", data.len());
        return Ok(());
    }

    // Original method
    println!(
        "Method: original (byte address, 0x14+0x02) type='{}' (0x{:02X})",
        save_type, suffix
    );
    let select_cmd = [0x14u8, suffix, 0x00];
    handle.write_bulk(cmd_ep, &select_cmd, TIMEOUT)?;
    std::thread::sleep(std::time::Duration::from_millis(100));

    let mut cart_data = Vec::new();
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
                cart_data.extend_from_slice(&buf[..len]);
                let h: String = buf[..16]
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<Vec<_>>()
                    .join(" ");
                if chunk % 2 == 0 {
                    println!("  [{chunk:02}] 0x{:06X}: {}", addr, h);
                }
            }
            Err(e) => {
                println!("  [{chunk:02}] {e}");
                break;
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    println!("  Total: {} bytes", cart_data.len());

    if let Some(path) = output {
        fs::write(&path, &cart_data).context("writing save file")?;
        println!("  Wrote to {}", path.display());
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

    // Test 4: Word addressing with 'f' type (most common in GUI)
    println!("\n=== Test 4: Word address + type='f' ===");
    let data4 = save_read_word_addr(&handle, data_ep, cmd_ep, 0, count, b'f')?;
    println!("  Got {} bytes", data4.len());

    // Test 5: Read from different byte addresses
    println!("\n=== Test 5: Type='f' at different addresses ===");
    let select_cmd = [0x14u8, b'f', 0x00];
    handle.write_bulk(cmd_ep, &select_cmd, TIMEOUT)?;
    std::thread::sleep(Duration::from_millis(50));
    for sub_addr in [0u32, 64, 128, 256, 512, 1024, 0x10000, 0x100000] {
        let addr = sub_addr;
        let cmd = [0x02u8, (addr & 0xFF) as u8, ((addr >> 8) & 0xFF) as u8, ((addr >> 16) & 0xFF) as u8, b'f', 0];
        handle.write_bulk(cmd_ep, &cmd[..5], TIMEOUT)?;
        std::thread::sleep(Duration::from_millis(100));
        let mut buf = [0u8; 64];
        match handle.read_bulk(data_ep, &mut buf, Duration::from_millis(500)) {
            Ok(len) => {
                let h: String = buf[..8].iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" ");
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
        let cmd = [0x02u8, (addr & 0xFF) as u8, ((addr >> 8) & 0xFF) as u8, ((addr >> 16) & 0xFF) as u8, b'f', 0];
        handle.write_bulk(cmd_ep, &cmd[..5], TIMEOUT)?;
        std::thread::sleep(Duration::from_millis(100));
        let mut buf = [0u8; 64];
        match handle.read_bulk(data_ep, &mut buf, TIMEOUT) {
            Ok(len) => {
                all.extend_from_slice(&buf[..len]);
                let h: String = buf[..8].iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" ");
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

    let h0: String = buf0[..8].iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" ");
    let h1: String = buf1[..8].iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" ");
    println!("  addr=0x000000: {}...", h0);
    println!("  addr=0xFFFFFF: {}...", h1);
    if buf0[..8] == buf1[..8] {
        println!("  IDENTICAL - address parameter IS ignored!");
    } else {
        println!("  DIFFERENT - address parameter works!");
    }

    // Test 9: Try 0x1A register reads from the cartridge address space
    println!("\n=== Test 9: Register reads via 0x1A ===");
    for reg_addr in [0x000000u32, 0x800000, 0x9C0000, 0xE00000, 0x9FE000, 0xFF0000, 0x0E0000] {
        let cmd = [0x1Au8, (reg_addr & 0xFF) as u8, ((reg_addr >> 8) & 0xFF) as u8, ((reg_addr >> 16) & 0xFF) as u8];
        handle.write_bulk(cmd_ep, &cmd, TIMEOUT)?;
        std::thread::sleep(Duration::from_millis(20));
        let mut buf = [0u8; 64];
        match handle.read_bulk(data_ep, &mut buf, TIMEOUT) {
            Ok(len) => {
                println!("  reg=0x{reg_addr:06X}: {} bytes: {}", len, buf[..len.min(8)].iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" "));
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
    let _lock_writes = vec![
        (0x9FC000u32, 0x1500u16),
    ];

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

fn cmd_dump(
    mut output: PathBuf,
    start_addr: u32,
    size: u32,
    _delay_ms: u64,
    fast: bool,
) -> Result<()> {
    let total_size = if size == 0 {
        32 * 1024 * 1024 - start_addr
    } else {
        size
    };
    let chunk_count = total_size.div_ceil(64);

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

    let cmd_ep = 0x04;
    let data_ep = 0x82;

    println!(
        "Dumping {} bytes ({} chunks) starting at 0x{:X} to {}",
        total_size,
        chunk_count,
        start_addr,
        output.display()
    );
    if fast {
        println!("  Mode: EP4 bulk pipelined (Experimental - fast, but potentially unreliable)");
    } else {
        println!("  Mode: EP4 bulk non-pipelined (Reliable - same as GUI)");
    }
    println!();

    let mut file = fs::File::create(&output)
        .with_context(|| format!("Failed to create output file: {}", output.display()))?;
    use std::io::Write;

    if fast {
        let mut prev_buf: Option<[u8; 64]> = None;
        for chunk in 0..=chunk_count {
            if chunk < chunk_count {
                let byte_addr = start_addr + chunk * 64;
                let word_addr = byte_addr / 2;
                let bank = (word_addr >> 16) as u8;
                let addr_16 = (word_addr & 0xFFFF) as u16;
                let cmd = [
                    0x01u8,
                    (addr_16 & 0xFF) as u8,
                    ((addr_16 >> 8) & 0xFF) as u8,
                    bank,
                ];
                if let Err(e) = handle.write_bulk(cmd_ep, &cmd, TIMEOUT) {
                    println!("\n  ERROR at chunk {}: write_bulk: {e}", chunk);
                    break;
                }
            }
            if let Some(buf) = prev_buf.take() {
                file.write_all(&buf)?;
            }
            if chunk < chunk_count {
                let mut buf = [0u8; 64];
                match handle.read_bulk(data_ep, &mut buf, Duration::from_millis(500)) {
                    Ok(_len) => {
                        prev_buf = Some(buf);
                        if chunk % 256 == 0 && chunk > 0 {
                            let pct = (chunk * 100) / chunk_count;
                            let addr_mb = (start_addr + chunk * 64) as f64 / (1024.0 * 1024.0);
                            print!("\r  Progress: {}% ({:.1} MB)", pct, addr_mb);
                            std::io::stdout().flush()?;
                        }
                    }
                    Err(e) => {
                        println!("\n  ERROR at chunk {}: read_bulk: {e}", chunk);
                        break;
                    }
                }
            }
        }
    } else {
        let mut last_pct = 0u32;
        for chunk in 0..chunk_count {
            let byte_addr = start_addr + chunk * 64;
            let word_addr = byte_addr / 2;
            let addr_16 = (word_addr & 0xFFFF) as u16;
            let bank = (word_addr >> 16) as u8;

            let cmd = [
                0x01u8,
                (addr_16 & 0xFF) as u8,
                ((addr_16 >> 8) & 0xFF) as u8,
                bank,
            ];

            handle.write_bulk(cmd_ep, &cmd, TIMEOUT)
                .with_context(|| format!("EP4 ROM write at byte_addr=0x{byte_addr:06X}"))?;

            std::thread::sleep(Duration::from_millis(5));

            let mut buf = [0u8; 64];
            match handle.read_bulk(data_ep, &mut buf, Duration::from_secs(3)) {
                Ok(len) => {
                    file.write_all(&buf[..len])?;
                }
                Err(e) => {
                    println!("\n  ERROR at chunk {chunk}: read_bulk: {e}");
                    break;
                }
            }

            let pct = (chunk * 100) / chunk_count;
            if pct != last_pct {
                last_pct = pct;
                let addr_mb = byte_addr as f64 / (1024.0 * 1024.0);
                print!("\r  Progress: {pct}% ({addr_mb:.1} MB)");
                std::io::stdout().flush()?;
            }
        }
    }
    println!();

    let file_size = std::fs::metadata(&output).map(|m| m.len()).unwrap_or(0);
    println!("  Dumped {} bytes to {}", file_size, output.display());
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
                let h: String = buf[..8].iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" ");
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
            let hex = buf[..len.min(8)].iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" ");
            println!("  Response ({} bytes): {}", len, hex);
            if len >= 2 {
                println!("  Word value: 0x{:04X}", u16::from_le_bytes([buf[0], buf[1]]));
            }
        }
        Err(e) => println!("  Error: {e}"),
    }
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
                (Ok(_), _) => println!("Bootloader mode:   EZ-Writer detected (VID 0x{BOOTLOADER_VID:04x}:PID 0x{BOOTLOADER_PID:04x})"),
                (_, Ok(_)) => println!("Active mode:       EZ-Writer detected (VID 0x{EZWRITER_VID:04x}:PID 0x{EZWRITER_PID:04x})"),
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
            let fw_data = fs::read(&fw).with_context(|| format!("reading firmware: {}", fw.display()))?;
            download_firmware(&handle, &fw_data, no_cpu)?;
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
        } => cmd_save_read(addr, count, save_type, output, word_addr, use_reg, use_rom_read, rom_offset)?,
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
        } => cmd_dump(output, start, size, delay, fast)?,
        Commands::Reset => cmd_reset()?,
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
