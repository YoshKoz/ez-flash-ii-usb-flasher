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
        /// Path to 8051 firmware binary (e.g. tusbez.bin)
        firmware: PathBuf,
        /// Skip CPU reset/start (just download, no register writes)
        #[arg(long)]
        no_cpu: bool,
        /// Path to optional loader table (e.g. loader_table2.bin) to patch
        /// firmware with EEPROM bit-bang code before CPU start.
        #[arg(long)]
        loader_table: Option<PathBuf>,
        /// Allow flashing from active mode (0548:1005). CPU reset after flash
        /// restarts from XRAM without re-reading EEPROM, so patched firmware
        /// runs for this session only.
        #[arg(long)]
        force: bool,
        /// Poll for bootloader mode (0547:2131) until it appears, then flash
        /// immediately. Replug the device while this command is running.
        #[arg(long)]
        watch: bool,
    },
    /// Initialize AN2131 using exact chunk tables extracted from ezwinit.sys
    InitExact {
        /// First chunk table (loader_table1.bin)
        table1: PathBuf,
        /// Second chunk table (loader_table2.bin)
        table2: PathBuf,
    },
    /// Reset cartridge flash to array read mode (24-bit safe JEDEC)
    ResetCart,
    /// Check cartridge presence and info
    CartInfo,
    /// Read cartridge ROM via bulk EP4/EP2 protocol
    CartRead {
        /// Byte address
        #[arg(default_value = "0")]
        addr: u32,
        /// Number of 64-byte chunks
        #[arg(default_value = "4")]
        count: u32,
        /// Command byte: 0x01=ROM
        #[arg(long, default_value = "1")]
        cmd: u8,
        /// Bank number (writes to Port A 0x7F00 for >128KB access)
        #[arg(long)]
        bank: Option<u8>,
        /// Use byte[3] of the 4-byte EP4 command as the bank (replaces hardcoded 0x00)
        #[arg(long)]
        byte3_bank: bool,
    },
    /// Send EP0 vendor command 0x01 with 4-byte data payload to read GBA ROM
    Ep0VendorRead {
        /// Byte address
        #[arg(default_value = "0")]
        addr: u32,
        /// Number of 64-byte chunks
        #[arg(default_value = "4")]
        count: u32,
        /// Bank (upper 8 bits of 24-bit address)
        #[arg(long, default_value = "0")]
        bank: u8,
    },
    /// Write FPGA register via vendor 0xA0 to port data (test direct FPGA access)
    FpgaWrite {
        /// FPGA address to write to (0-0xFF)
        addr: u8,
        /// Byte value to write
        value: u8,
    },
    /// Dump cartridge ROM to file using byte[3] as bank (24-bit addressing)
    Dump {
        /// Output file path
        output: PathBuf,
        /// Start byte address
        #[arg(default_value = "0")]
        addr: u32,
        /// Number of bytes to dump (0 = full 32MB minus start)
        #[arg(default_value = "0")]
        size: u32,
        /// Delay between chunks in ms
        #[arg(long, default_value = "10")]
        delay_ms: u64,
        /// Pipelined mode: overlap read requests for speed (experimental)
        #[arg(long)]
        fast: bool,
    },
    /// Read save data (cmd 0x14 = select type, then cmd 0x02 = read)
    SaveRead {
        /// Save offset in bytes
        #[arg(default_value = "0")]
        addr: u32,
        /// Number of 64-byte chunks
        #[arg(default_value = "16")]
        count: u32,
        /// Save type: f=FLASH, e=EEPROM, g=?, h=?
        #[arg(long, default_value = "f")]
        save_type: char,
        /// Save read command byte. Experimental; default preserves the old command.
        #[arg(long, default_value_t = 0x02, value_parser = parse_u8_auto)]
        read_cmd: u8,
        /// Firmware save handler byte. Defaults to the known FLASH/SRAM handler for FLASH saves.
        #[arg(long, value_parser = parse_u8_auto)]
        inner_cmd: Option<u8>,
        /// Skip the 0x14 select-type command before reading.
        #[arg(long)]
        no_select: bool,
        /// Write output even when validation says it is not a real save.
        #[arg(long)]
        allow_unverified: bool,
        /// Output file path (writes binary; omit to print hex to stdout)
        #[arg(long)]
        output: Option<PathBuf>,
        /// Use byte addressing instead of the default word addressing (addr/2).
        #[arg(long)]
        byte_addr: bool,
    },
    /// USB bus reset (port reset)
    Reset,
    /// Send a vendor control request to probe device
    Probe {
        /// bRequest value (vendor command)
        request: u8,
        /// wValue
        value: u16,
    },
    /// Read internal RAM at address via vendor 0xA3
    RamRead {
        /// Address to read from
        address: u16,
    },
    /// Write to internal RAM via vendor 0xA0
    RamWrite {
        /// Address to write to
        address: u16,
        /// Byte value to write
        value: u8,
    },
    /// Write save data to cartridge
    SaveWrite {
        /// Input save file (.sav)
        input: PathBuf,
        /// Start offset in save memory
        #[arg(default_value = "0")]
        addr: u32,
        /// Save type: f=FLASH, e=EEPROM, s=SRAM
        #[arg(long, default_value = "f")]
        save_type: char,
        /// Write command byte (experimental)
        #[arg(long, default_value_t = 0x03)]
        write_cmd: u8,
        /// Erase command byte for FLASH sectors (experimental)
        #[arg(long, default_value_t = 0x15)]
        erase_cmd: u8,
    },
    /// Write ROM to cartridge (experimental - NOR flash protocol not yet confirmed)
    RomWrite {
        /// Input ROM file (.gba)
        input: PathBuf,
        /// Start offset in cartridge ROM
        #[arg(default_value = "0")]
        addr: u32,
        /// Delay between chunk writes in ms
        #[arg(long, default_value = "10")]
        delay_ms: u64,
        /// Skip sector erase
        #[arg(long)]
        no_erase: bool,
        /// Write command byte (experimental)
        #[arg(long, default_value_t = 0x11)]
        write_cmd: u8,
        /// Erase sector command byte (experimental)
        #[arg(long, default_value_t = 0x10)]
        erase_cmd: u8,
    },
    /// Passive read-only poll of active IN endpoints (sends no data)
    PassiveRead,
    /// Send bulk data to EP2 OUT, read from EP6 IN
    BulkTest,
    /// Probe EEPROM save read: try all cmd/inner combos, log responses
    ProbeEeprom {
        /// Comma-separated read_cmd bytes to try, e.g. "0x01,0x02,0x11,0x12"
        #[arg(long, default_value = "0x01,0x02,0x03,0x04,0x11,0x12,0x13,0x14,0x15")]
        cmds: String,
        /// Comma-separated inner/handler bytes to try (0xFF = send 4-byte packet, no inner byte)
        #[arg(long, default_value = "0xFF,0x65,0x67,0x69,0x00")]
        inners: String,
        /// Whether to send 0x14 select command before each read attempt
        #[arg(long)]
        with_select: bool,
        /// Timeout per read attempt in ms (short = fast scan)
        #[arg(long, default_value = "800")]
        timeout_ms: u64,
    },
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn print_hex(data: &[u8]) {
    for chunk in data.chunks(16) {
        let hex_str: String = chunk.iter().fold(String::new(), |mut s, b| {
            let _ = write!(s, "{b:02x} ");
            s
        });
        println!("    {hex_str}");
    }
}

fn parse_u8_auto(s: &str) -> Result<u8, String> {
    let trimmed = s.trim();
    if let Some(hex) = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
    {
        u8::from_str_radix(hex, 16).map_err(|e| e.to_string())
    } else {
        trimmed.parse::<u8>().map_err(|e| e.to_string())
    }
}

fn gen3_save_signature_count(data: &[u8]) -> usize {
    // Gen 3 save sections start with this 4-byte signature; a full FLASH save has 14 sections.
    const GEN3_SIG: [u8; 4] = [0x25, 0x20, 0x01, 0x08];
    data.windows(GEN3_SIG.len())
        .filter(|window| **window == GEN3_SIG)
        .count()
}

fn starts_with_known_rom_stub(data: &[u8]) -> bool {
    data.starts_with(&[0xff, 0x07, 0x00, 0x28, 0x0c, 0xd1, 0x10, 0x48])
        || data.starts_with(&[0xff, 0xef, 0x00, 0x28, 0x0c, 0xd1, 0x10, 0x48])
}

fn validate_save_dump(data: &[u8], save_type: char) -> Result<()> {
    if data.is_empty() {
        bail!("save read returned no data");
    }
    if starts_with_known_rom_stub(data) {
        bail!("save data starts with the known ROM/stale endpoint pattern, not save RAM");
    }

    if matches!(save_type, 'f' | 'F') && data.len() >= 128 * 1024 {
        let signatures = gen3_save_signature_count(data);
        if signatures < 14 {
            bail!(
                "FLASH save validation failed: found {signatures} Gen 3 section signatures, expected at least 14"
            );
        }
    }

    Ok(())
}

fn default_save_read_inner_cmd(save_type: char) -> u8 {
    match save_type {
        // Patched firmware (loader_table2): cmd 0x02 checks inner byte:
        //   0x66 = FLASH/SRAM handler (CJNE at 0x07D5)
        //   0x65 = EEPROM handler (XRL #0x65 at 0x07FF)
        //   0x68 = other handler
        'f' | 'F' => 0x66,
        'e' | 'E' => 0x65,
        _ => save_type as u8,
    }
}

fn find_device(vid: u16, pid: u16) -> Result<(Device<GlobalContext>, DeviceDescriptor)> {
    for device in rusb::devices()?.iter() {
        let desc = device.device_descriptor()?;
        if desc.vendor_id() == vid && desc.product_id() == pid {
            return Ok((device, desc));
        }
    }
    bail!("No device found with VID={:04X} PID={:04X}", vid, pid);
}

fn print_device_info(desc: &DeviceDescriptor, handle: &DeviceHandle<GlobalContext>) -> Result<()> {
    println!("  Vendor ID:     0x{:04X}", desc.vendor_id());
    println!("  Product ID:    0x{:04X}", desc.product_id());
    println!(
        "  BCD USB:       {}.{}",
        desc.usb_version().0,
        desc.usb_version().1
    );
    println!("  Device Class:  0x{:02x}", desc.class_code());
    println!("  Device SubClass: 0x{:02x}", desc.sub_class_code());
    println!("  Device Protocol: 0x{:02x}", desc.protocol_code());
    println!("  Max Packet Size 0: {}", desc.max_packet_size());
    println!("  Num Configs:   {}", desc.num_configurations());
    println!(
        "  BCD Device:    {}.{}",
        desc.device_version().0,
        desc.device_version().1
    );

    // Read string descriptors if available
    let lang = handle.read_languages(TIMEOUT).unwrap_or_default();
    if let Some(&first_lang) = lang.first() {
        if let Ok(s) = handle.read_manufacturer_string(first_lang, desc, TIMEOUT) {
            println!("  Manufacturer:  {}", s);
        }
        if let Ok(s) = handle.read_product_string(first_lang, desc, TIMEOUT) {
            println!("  Product:       {}", s);
        }
        if let Ok(s) = handle.read_serial_number_string(first_lang, desc, TIMEOUT) {
            println!("  Serial:        {}", s);
        }
    }
    Ok(())
}

fn print_config_descriptors(device: &Device<GlobalContext>) -> Result<()> {
    let config = device.active_config_descriptor()?;
    println!("  Active Config: {}", config.number());
    for iface in config.interfaces() {
        for desc in iface.descriptors() {
            println!(
                "  Interface {}: class={:02x} subclass={:02x} protocol={:02x}",
                desc.interface_number(),
                desc.class_code(),
                desc.sub_class_code(),
                desc.protocol_code()
            );
            for ep in desc.endpoint_descriptors() {
                let dir = match ep.direction() {
                    Direction::In => "IN",
                    Direction::Out => "OUT",
                };
                let addr = ep.address();
                println!(
                    "    EP {:#04x} {}  max_pkt={}",
                    addr,
                    dir,
                    ep.max_packet_size()
                );
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Cypress EZ-USB firmware download
// ---------------------------------------------------------------------------

/// Write a block of data to EZ-USB internal RAM using Cypress vendor command 0xA0.
fn ezusb_write_ram(handle: &DeviceHandle<GlobalContext>, address: u32, data: &[u8]) -> Result<()> {
    // bmRequestType: 0x40 = Host-to-Device, Vendor, Device
    // bRequest: 0xA0 (Cypress firmware download)
    // wValue: low 16 bits of address
    // wIndex: high 16 bits of address (for >64KB)
    // wLength: data length
    let wvalue = (address & 0xFFFF) as u16;
    let windex = ((address >> 16) & 0xFFFF) as u16;

    let actual = handle
        .write_control(0x40, VR_CYPRESS_WRITE, wvalue, windex, data, TIMEOUT)
        .context("EZ-USB firmware write failed")?;

    if actual != data.len() {
        bail!("Short write: wrote {} of {} bytes", actual, data.len());
    }
    Ok(())
}

/// Download firmware binary to EZ-USB, optionally write loader patch chunks,
/// then restart CPU to trigger renumeration with new firmware.
///
/// On AN2131: writing CPUCS to 0x00 (reset) disconnects USB before firmware
/// can be uploaded. So we write firmware+loader chunks with CPU running
/// bootloader, then do reset+start atomically at the end.
fn download_firmware(
    handle: &DeviceHandle<GlobalContext>,
    firmware: &[u8],
    loader_chunks: &[(u16, Vec<u8>)],
    no_cpu: bool,
) -> Result<()> {
    // 1. Download firmware to internal RAM (CPU runs bootloader, which
    //    intercepts 0xA0 vendor commands and writes to the correct RAM).
    println!("  Downloading {} bytes of firmware...", firmware.len());

    let chunk_size = 64;
    let mut offset = 0;
    while offset < firmware.len() {
        let end = (offset + chunk_size).min(firmware.len());
        ezusb_write_ram(handle, offset as u32, &firmware[offset..end])?;
        offset = end;

        if offset % 1024 == 0 || offset == firmware.len() {
            print!("\r    Progress: {}/{} bytes", offset, firmware.len());
            use std::io::Write;
            std::io::stdout().flush()?;
        }
    }
    println!();

    // 2. Optionally write loader patch chunks (EEPROM bit-bang code etc.)
    if !loader_chunks.is_empty() {
        println!("  Writing {} loader patch chunks...", loader_chunks.len());
        for (i, (addr, payload)) in loader_chunks.iter().enumerate() {
            ezusb_write_ram(handle, *addr as u32, payload)?;
            if i % 10 == 0 || i + 1 == loader_chunks.len() {
                println!("    [{}/{}] addr=0x{addr:04X} len={}", i + 1, loader_chunks.len(), payload.len());
            }
        }
    }

    // 3. Restart CPU: reset then start to switch from bootloader to firmware.
    //    With RENUM=0 the 8051 takes over USB and re-enumerates with new VID/PID.
    if !no_cpu {
        println!("  Restarting CPU (device will re-enumerate)...");
        ezusb_write_ram(handle, CPUCS_ADDR as u32, &[0x00])?;
        ezusb_write_ram(handle, CPUCS_ADDR as u32, &[0x01])?;
    } else {
        println!("  Skipping CPU restart (--no-cpu)");
    }

    println!("  Firmware download complete.");
    Ok(())
}

// ---------------------------------------------------------------------------
// Subcommands
// ---------------------------------------------------------------------------

fn cmd_list() -> Result<()> {
    println!("Scanning USB devices for EZ-Writer...");
    println!();

    // Check for bootloader mode
    match find_device(BOOTLOADER_VID, BOOTLOADER_PID) {
        Ok((device, desc)) => {
            println!("[1] EZ-Writer (BOOTLOADER mode)");
            let handle = device.open()?;
            print_device_info(&desc, &handle)?;
            print_config_descriptors(&device)?;
        }
        Err(_) => println!(
            "  No device in bootloader mode (VID {:04X}:{:04X})",
            BOOTLOADER_VID, BOOTLOADER_PID
        ),
    }

    // Check for EZ-Writer mode
    match find_device(EZWRITER_VID, EZWRITER_PID) {
        Ok((device, desc)) => {
            println!("\n[2] EZ-Writer (ACTIVE mode)");
            let handle = device.open()?;
            print_device_info(&desc, &handle)?;
            print_config_descriptors(&device)?;
        }
        Err(_) => println!(
            "\n  No device in active mode (VID {:04X}:{:04X})",
            EZWRITER_VID, EZWRITER_PID
        ),
    }

    // Also list all devices matching Cypress or EZ
    println!("\nAll USB devices:");
    for device in rusb::devices()?.iter() {
        let desc = device.device_descriptor()?;
        let vid = desc.vendor_id();
        let pid = desc.product_id();
        if vid == 0x0547 || vid == 0x0548 || vid == 0x0550 || vid == 0x0451 {
            println!("  VID={:04X} PID={:04X} (EZ Family)", vid, pid);
        }
    }

    Ok(())
}

fn cmd_info() -> Result<()> {
    // Try active mode first, then bootloader
    let (device, desc) = if let Ok(d) = find_device(EZWRITER_VID, EZWRITER_PID) {
        println!("Device in EZ-Writer ACTIVE mode:");
        d
    } else if let Ok(d) = find_device(BOOTLOADER_VID, BOOTLOADER_PID) {
        println!("Device in EZ-Writer BOOTLOADER mode:");
        d
    } else {
        bail!("No EZ-Writer device found. Check connections.");
    };

    let handle = device.open()?;
    print_device_info(&desc, &handle)?;
    print_config_descriptors(&device)?;
    Ok(())
}

fn cmd_firmware_download(firmware_path: &PathBuf, no_cpu: bool, loader_table: &Option<PathBuf>, force: bool, watch: bool) -> Result<()> {
    let (device, _desc) = if watch {
        println!("Watching for EZ-Writer bootloader (0547:2131)... replug device now.");
        loop {
            if let Ok(d) = find_device(BOOTLOADER_VID, BOOTLOADER_PID) {
                println!("Found EZ-Writer in bootloader mode.");
                break d;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    } else if let Ok(d) = find_device(BOOTLOADER_VID, BOOTLOADER_PID) {
        println!("Found EZ-Writer in bootloader mode.");
        d
    } else if force {
        let d = find_device(EZWRITER_VID, EZWRITER_PID)
            .context("No EZ-Writer device found in bootloader or active mode")?;
        println!("Found EZ-Writer in ACTIVE mode (--force). Writing firmware to XRAM.");
        println!("CPU will restart from XRAM after flash (EEPROM not re-read until power cycle).");
        d
    } else {
        bail!("No device in bootloader mode (0547:2131). Use --watch and replug, or --force to flash from active mode.");
    };

    // Load firmware binary
    let firmware = fs::read(firmware_path)
        .with_context(|| format!("Failed to read firmware file: {:?}", firmware_path))?;
    println!(
        "Loaded firmware: {} ({} bytes)",
        firmware_path.display(),
        firmware.len()
    );

    // Validate: should start with 8051 code
    if firmware.len() < 4 {
        bail!("Firmware file too small.");
    }
    // Most EZ-USB 8051 firmware starts with LJMP (0x02 XX XX)
    if firmware[0] != 0x02 {
        println!("  Warning: firmware doesn't start with 8051 LJMP opcode (0x02).");
        println!("  First byte: 0x{:02x}. Proceeding anyway...", firmware[0]);
    }

    let handle = device.open()?;

    // Try setting interface 0 to alt setting 1 (needed for AN2131 bootloader)
    match handle.set_alternate_setting(0, 1) {
        Ok(()) => println!("Set interface 0 to alt setting 1"),
        Err(rusb::Error::Pipe) => println!("Interface alt 1 not supported, using alt 0"),
        Err(e) => println!("Warning: set_alt_setting: {e}"),
    }

    // Detach kernel driver if any
    let _ = handle.detach_kernel_driver(0);

    // Claim interface 0 (usually the only interface in bootloader mode)
    let config = device.active_config_descriptor()?;
    if let Some(iface) = config.interfaces().next()
        && let Some(desc) = iface.descriptors().next()
    {
        handle.claim_interface(desc.interface_number())?;
    }

    // Load optional loader patch table
    let loader_chunks = if let Some(table_path) = loader_table {
        let chunks = load_chunk_table(table_path)?;
        println!("  Loaded loader table: {} chunks", chunks.len());
        chunks
    } else {
        Vec::new()
    };

    download_firmware(&handle, &firmware, &loader_chunks, no_cpu)?;
    println!("Firmware sent. Device should now re-enumerate.");
    println!("Run 'ezwriter-cli list' again after a few seconds.");

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
        let addr = chunk * 32; // word address (64 bytes / 2)
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
        if cart_data[4..8] == [0x24, 0xFF, 0xAE, 0x51] || cart_data[4..8] == [0xFE, 0x7F, 0x1C, 0xEA] {
             // Basic GBA logo or branch check
        }
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
    // Try both bootloader and active modes
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

    // Send vendor request: Host-to-Device, Vendor, Device
    println!(
        "Sending vendor request: bReq=0x{:02X} wVal=0x{:04X}",
        request, value
    );

    let mut buf = [0u8; 64];
    match handle.read_control(
        0xC0, // Device-to-Host, Vendor, Device
        request, value, 0, &mut buf, TIMEOUT,
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

    // Read from internal RAM via vendor request 0xA3 (Upload)
    let mut buf = [0u8; 64];
    println!("Reading RAM at 0x{address:04X} via vendor 0xA3...");
    match handle.read_control(
        0xC0, // Device-to-Host, Vendor, Device
        0xA3, // Cypress Upload from internal memory
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
        0x40, // Host-to-Device, Vendor, Device
        0xA0, // Cypress Firmware Download
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
    // JEDEC reset: F0 to any address. Also send unlock+F0 for chips needing full sequence.
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

/// Write a 16-bit value to a 24-bit cartridge-bus register via cmd 0x19.
/// Format: [0x19, addr_lo, addr_mid, addr_hi, data_lo, data_hi]
fn write_reg(
    handle: &DeviceHandle<GlobalContext>,
    cmd_ep: u8,
    addr: u32,
    data: u16,
) -> Result<()> {
    let buf = [
        0x19u8,
        (addr & 0xFF) as u8,
        ((addr >> 8) & 0xFF) as u8,
        ((addr >> 16) & 0xFF) as u8,
        (data & 0xFF) as u8,
        ((data >> 8) & 0xFF) as u8,
    ];
    handle.write_bulk(cmd_ep, &buf, TIMEOUT)?;
    Ok(())
}

fn cmd_save_read(
    byte_addr: u32,
    count: u32,
    save_type: char,
    read_cmd: u8,
    inner_cmd: Option<u8>,
    no_select: bool,
    allow_unverified: bool,
    output: Option<PathBuf>,
    word_addr: bool,
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

    // Unlock EZ-Flash II CPLD before save-chip access (asie unlock sequence).
    // Without this, the save chip is gated off and every read returns the same
    // stale 128-byte buffer regardless of address.
    write_reg(&handle, cmd_ep, 0x9FE000, 0xD200)?;
    write_reg(&handle, cmd_ep, 0x800000, 0x1500)?;
    write_reg(&handle, cmd_ep, 0x802000, 0xD200)?;
    write_reg(&handle, cmd_ep, 0x804000, 0x1500)?;
    let suffix = save_type as u8;
    let packet_tail = inner_cmd.unwrap_or_else(|| default_save_read_inner_cmd(save_type));
    println!(
        "Save read: type='{}' (0x{:02X}) read_cmd=0x{:02X} inner=0x{:02X} addr=0x{:X} {} chunks",
        save_type, suffix, read_cmd, packet_tail, byte_addr, count
    );

    // Step 1: Select save type via cmd 0x14 + firmware handler byte.
    if !no_select {
        let select_cmd = [0x14u8, packet_tail, 0x00];
        handle.write_bulk(cmd_ep, &select_cmd, TIMEOUT)?;
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    // Step 2: Read save data chunks
    // Protocol (from firmware disassembly):
    //   - cmd 0x02: page setup. Firmware dispatches to FLASH handler when byte3=0x66.
    //     Byte layout: [0x02, addr_lo, addr_mid, 0x66, addr_bank, 0]
    //     FLASH handler writes addr_mid→PORTB (latch high addr), addr_lo→PORTC.
    //     Must be sent once per 256-byte page to advance addr_mid.
    //   - cmd 0x03: reads 64 bytes into EP2 IN. Inner loop: PORTB=cmd(0x03)+i, PORTC=addr_lo.
    //     CPLD latches page from cmd 0x02 and auto-advances; addr_lo selects within-page offset.
    // Drain stale EP2 data. The firmware auto-streams 8×64-byte chunks into EP2 IN;
    // all 8 must be consumed before the first cmd 0x02 response is reliable.
    for _ in 0..8 {
        let mut drain = [0u8; 64];
        if handle.read_bulk(data_ep, &mut drain, Duration::from_millis(200)).is_err() { break; }
    }

    let mut cart_data = Vec::new();

    for chunk in 0..count {
        let byte_offset = chunk * 64;
        // cmd 0x02 (SAVE_READ): [cmd, addr_lo, addr_mid, addr_hi, suffix]
        let cmd = [
            read_cmd,
            (byte_offset & 0xFF) as u8,
            ((byte_offset >> 8) & 0xFF) as u8,
            ((byte_offset >> 16) & 0xFF) as u8,
            packet_tail,
        ];
        handle.write_bulk(cmd_ep, &cmd, TIMEOUT)?;

        let mut buf = [0u8; 64];
        let save_read_timeout = Duration::from_secs(30);
        match handle.read_bulk(data_ep, &mut buf, save_read_timeout) {
            Ok(len) => {
                cart_data.extend_from_slice(&buf[..len]);
                let h: String = buf[..16]
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<Vec<_>>()
                    .join(" ");
                if chunk % 2 == 0 {
                    println!("  [{chunk:02}] 0x{:06X}: {}", byte_addr + chunk * 64, h);
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
    println!(
        "  Gen 3 save signatures: {}",
        gen3_save_signature_count(&cart_data)
    );

    if let Some(path) = output {
        if let Err(error) = validate_save_dump(&cart_data, save_type) {
            if allow_unverified {
                println!("  WARNING: writing unverified save dump: {error}");
            } else {
                bail!(
                    "{error}. Refusing to write {}; pass --allow-unverified only for protocol experiments",
                    path.display()
                );
            }
        }
        fs::write(&path, &cart_data).context("writing save file")?;
        println!("  Wrote to {}", path.display());
    }

    // Re-lock the CPLD (best effort; ignore errors).
    let _ = write_reg(&handle, cmd_ep, 0x9FC000, 0x1500);

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

    // Auto-reset flash before reading to prevent lockups
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

    // Protocol: write 4-byte cmd to EP4 OUT (0x04), read 64b from EP2 IN (0x82)
    // Address param is in 2-byte units. Convert byte address by dividing by 2.
    let cmd_ep = 0x04;
    let data_ep = 0x82;
    let dev_addr = byte_addr / 2;

    // Determine bank from --bank flag, or auto-compute from address if --byte3-bank.
    let bank_val: u8 = if let Some(b) = bank {
        b
    } else if byte3_bank {
        (byte_addr >> 17) as u8 // word_addr >> 16
    } else {
        0
    };
    let use_bank = bank.is_some() || byte3_bank;

    // If bank is specified and NOT using byte3_bank, latch via 0xBF sequence
    // This matches the EP0 cmd=0x01 handler at 0x075E.
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
        let addr = dev_addr + chunk * 32; // advance 64 bytes per chunk
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

    // Parse GBA header if reading from start
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
        32 * 1024 * 1024 - start_addr // max GBA ROM minus start
    } else {
        size
    };
    let chunk_count = total_size.div_ceil(64);

    // Auto-add .gba extension if no extension present
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

    // The EZ-Flash II firmware maintains a 8-chunk (512-byte) auto-stream ring in
    // EP2 IN that advances with every USB IN transfer, independent of EP4 OUT cmds.
    // Step 1: drain all 8 auto-stream slots to clear the ring.
    for _ in 0..8 {
        let mut drain = [0u8; 64];
        let _ = handle.read_bulk(0x82, &mut drain, Duration::from_millis(200));
    }
    // Step 2: prime — send cmd addr=start_addr which loads ROM[start_addr] into the
    // prefetch buffer; the firmware responds with a phantom packet first.
    // Read the phantom and discard it; ROM[start_addr] is now prefetched.
    {
        let prime_word = start_addr / 2;
        let prime_a16 = (prime_word & 0xFFFF) as u16;
        let prime_bank = (prime_word >> 16) as u8;
        let prime_cmd = [0x01u8, (prime_a16 & 0xFF) as u8, ((prime_a16 >> 8) & 0xFF) as u8, prime_bank];
        let _ = handle.write_bulk(0x04, &prime_cmd, TIMEOUT);
        std::thread::sleep(Duration::from_millis(150));
        let mut phantom = [0u8; 64];
        let _ = handle.read_bulk(0x82, &mut phantom, Duration::from_secs(1));
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
        println!("  FAST pipelined mode — overlapping writes/reads (Experimental)");
        // Pipeline depth 2: write next command while reading previous response
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
                // Write next command immediately
                if let Err(e) = handle.write_bulk(cmd_ep, &cmd, TIMEOUT) {
                    println!("\n  ERROR at chunk {}: write_bulk: {e}", chunk);
                    break;
                }
            }
            // Read previous response while writing overlaps
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
                            use std::io::Write;
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
        // Non-pipelined: the firmware prefetches; send cmd addr+64 ahead so the
        // current chunk is already buffered and available to read immediately.
        // The prime already loaded ROM[start_addr]; each cmd loads the NEXT chunk.
        let mut last_pct = 0u32;
        for chunk in 0..chunk_count {
            let byte_addr = start_addr + chunk * 64;         // what we're reading now
            let next_addr = start_addr + (chunk + 1) * 64;  // what to prefetch next
            let next_word = next_addr / 2;
            let next_a16 = (next_word & 0xFFFF) as u16;
            let next_bank = (next_word >> 16) as u8;

            let cmd = [
                0x01u8,
                (next_a16 & 0xFF) as u8,
                ((next_a16 >> 8) & 0xFF) as u8,
                next_bank,
            ];

            handle.write_bulk(cmd_ep, &cmd, TIMEOUT)
                .with_context(|| format!("EP4 ROM write at byte_addr=0x{byte_addr:06X}"))?;

            // 5 ms: data is already prefetched; just time for state transition
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
                use std::io::Write;
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

    // Step 1: Select save type
    let select_cmd = [0x14u8, suffix, 0x00];
    handle.write_bulk(cmd_ep, &select_cmd, TIMEOUT)?;
    std::thread::sleep(Duration::from_millis(50));

    println!(
        "Writing {} bytes to save (type='{}') at offset 0x{:X}",
        data.len(),
        save_type,
        byte_addr
    );

    // For FLASH saves: erase sectors first (4KB sectors)
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

    // Step 2: Write save data in 64-byte chunks
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

        // Try reading a status byte back (best-effort)
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
    if !no_erase {
        println!("  NOTICE: ROM write needs confirmation from USB captures.");
        println!(
            "  Attempting sector erase + program with cmd=0x{:02X}, erase=0x{:02X}",
            write_cmd, erase_cmd
        );
    }

    // Reset flash to known state
    let seq: [(u8, u16); 4] = [(0xAA, 0xAAAA), (0x55, 0x5554), (0xF0, 0xAAAA), (0xFF, 0)];
    for (cb, a) in &seq {
        let da = a / 2;
        let c = [*cb, (da & 0xFF) as u8, ((da >> 8) & 0xFF) as u8, 0x00];
        let _ = handle.write_bulk(cmd_ep, &c, Duration::from_millis(500));
        std::thread::sleep(Duration::from_millis(5));
    }

    if !no_erase {
        // Erase sectors (64KB each for NOR flash)
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

    // Write 64-byte chunks
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

        // Optional: read back verify
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
    // Find device in any mode
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

    // Claim all interfaces
    for iface in config.interfaces() {
        for desc in iface.descriptors() {
            handle.claim_interface(desc.interface_number())?;
            println!("Claimed interface {}", desc.interface_number());
        }
    }

    // List available endpoints
    println!("\nAvailable endpoints:");
    for iface in config.interfaces() {
        for desc in iface.descriptors() {
            for ep in desc.endpoint_descriptors() {
                let dir = match ep.direction() {
                    Direction::In => "IN",
                    Direction::Out => "OUT",
                };
                let addr = ep.address();
                let ep_type = ep.transfer_type();
                println!(
                    "  EP 0x{:02X} {} max_pkt={} type={:?}",
                    addr,
                    dir,
                    ep.max_packet_size(),
                    ep_type
                );
            }
        }
    }

    // Try sending a small identify packet to EP2 OUT
    println!("\nSending probe packet to EP 0x02 (BULK OUT)...");
    // Simple probe: 4 bytes status request command
    let cmd = [0x04u8, 0x00, 0x00, 0x00]; // Hypothesized GET_STATUS
    match handle.write_bulk(0x02, &cmd, TIMEOUT) {
        Ok(len) => println!("  Wrote {} bytes to EP2", len),
        Err(e) => println!("  Write error: {}", e),
    }

    // Try reading from EP6 IN
    println!("Reading from EP 0x86 (BULK IN)...");
    let mut buf = [0u8; 512];
    match handle.read_bulk(0x86, &mut buf, TIMEOUT) {
        Ok(len) => {
            println!("  Read {} bytes from EP6:", len);
            print_hex(&buf[..len]);
        }
        Err(e) => println!("  Read error: {}", e),
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::List => cmd_list(),
        Commands::Info => cmd_info(),
        Commands::FirmwareDownload { firmware, no_cpu, loader_table, force, watch } => cmd_firmware_download(&firmware, no_cpu, &loader_table, force, watch),
        Commands::InitExact { table1, table2 } => cmd_init_exact(&table1, &table2),
        Commands::CartInfo => cmd_cart_info(),
        Commands::ResetCart => cmd_reset_cart(),
        Commands::Dump {
            output,
            addr,
            size,
            delay_ms,
            fast,
        } => cmd_dump(output, addr, size, delay_ms, fast),
        Commands::CartRead {
            addr,
            count,
            cmd,
            bank,
            byte3_bank,
        } => cmd_cart_read(addr, count, cmd, bank, byte3_bank),
        Commands::SaveRead {
            addr,
            count,
            save_type,
            read_cmd,
            inner_cmd,
            no_select,
            allow_unverified,
            output,
            byte_addr,
        } => cmd_save_read(
            addr,
            count,
            save_type,
            read_cmd,
            inner_cmd,
            no_select,
            allow_unverified,
            output,
            !byte_addr,
        ),
        Commands::Reset => cmd_reset(),
        Commands::Probe { request, value } => cmd_probe(request, value),
        Commands::RamRead { address } => cmd_ram_read(address),
        Commands::RamWrite { address, value } => cmd_ram_write(address, value),
        Commands::PassiveRead => cmd_passive_read(),
        Commands::BulkTest => cmd_bulk_test(),
        Commands::ProbeEeprom {
            cmds,
            inners,
            with_select,
            timeout_ms,
        } => cmd_probe_eeprom(&cmds, &inners, with_select, timeout_ms),
        Commands::SaveWrite {
            input,
            addr,
            save_type,
            write_cmd,
            erase_cmd,
        } => cmd_save_write(input, addr, save_type, write_cmd, erase_cmd),
        Commands::RomWrite {
            input,
            addr,
            delay_ms,
            no_erase,
            write_cmd,
            erase_cmd,
        } => cmd_rom_write(input, addr, delay_ms, no_erase, write_cmd, erase_cmd),
        Commands::Ep0VendorRead { addr, count, bank } => cmd_ep0_vendor_read(addr, count, bank),
        Commands::FpgaWrite { addr, value } => cmd_fpga_write(addr, value),
    }
}

// ---------------------------------------------------------------------------
// EP0 vendor command 0x01: 24-bit GBA ROM read via control endpoint
//
// The firmware's cmd=0x01 handler at 0x075E reads:
//   byte[0] from 0x7CC1 -> addr_lo  (written to 0x7F96)
//   byte[1] from 0x7CC2 -> addr_hi  (written to 0x7F97)
//   byte[2] from 0x7CC3 -> bank     (written to 0x7F96, then 0xBF latch)
//
// Protocol:
//   Control OUT: bmRequestType=0x40, bRequest=0x01, wValue=addr, wIndex=bank<<8
//   wLength=4, data=[0x01, addr_lo, addr_hi, bank]
//   Then read 64 bytes from EP2 IN (0x82)
fn cmd_ep0_vendor_read(byte_addr: u32, count: u32, bank: u8) -> Result<()> {
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
    let _ = handle.clear_halt(0x82); // EP2 IN

    let data_ep = 0x82; // EP2 IN
    let dev_addr = byte_addr / 2; // word address

    println!(
        "EP0 vendor read: addr=0x{:X} bank={} count={}",
        byte_addr, bank, count
    );

    let mut cart_data = Vec::new();
    for chunk in 0..count {
        let addr = dev_addr + chunk * 32; // advance 64 bytes per chunk

        // Send control OUT with 4-byte data payload
        // bmRequestType=0x40 (Host-to-Device, Vendor, Device)
        // bRequest=0x01 (vendor command)
        // wValue = low 16 bits of address
        // wIndex = (bank as u16) << 8  (bank in high byte)
        // wLength = 4
        let payload = [
            0x01u8,
            (addr & 0xFF) as u8,
            ((addr >> 8) & 0xFF) as u8,
            bank,
        ];
        let wvalue = (addr & 0xFFFF) as u16;
        let windex = (bank as u16) << 8;

        match handle.write_control(0x40, 0x01, wvalue, windex, &payload, TIMEOUT) {
            Ok(_len) => {
                // 5 ms delay for firmware to process and fill EP2 IN
                std::thread::sleep(std::time::Duration::from_millis(5));

                // Read response from EP2 IN
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
                        println!("  [{chunk:02}] EP2 IN read error: {e}");
                        // Try reading anyway to see what comes back
                        let mut buf2 = [0u8; 64];
                        if let Ok(len2) = handle.read_bulk(data_ep, &mut buf2, TIMEOUT) {
                            println!("  Retry: got {} bytes", len2);
                            print_hex(&buf2[..len2]);
                        }
                        break;
                    }
                }
            }
            Err(rusb::Error::Pipe) => {
                println!(
                    "  [{chunk:02}] STALL on control write (command not supported by firmware)"
                );
                break;
            }
            Err(e) => {
                println!("  [{chunk:02}] Control write error: {e}");
                break;
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }

    println!("  Total: {} bytes", cart_data.len());
    if !cart_data.is_empty() {
        // Parse GBA header if reading from start
        let h: String = cart_data[..16.min(cart_data.len())]
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<Vec<_>>()
            .join(" ");
        println!("  First 16 bytes: {}", h);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Direct FPGA register write via vendor 0xA0
//
// This sends a raw byte to the FPGA address on the external bus.
// 0x7F96 = FPGA data register, 0x7F97 = FPGA addr/status, 0x7F98 = FPGA command
fn cmd_fpga_write(fpga_addr: u8, value: u8) -> Result<()> {
    let (device, _desc) = find_device(EZWRITER_VID, EZWRITER_PID)?;
    println!("Found EZ-Writer active mode.");
    let handle = device.open()?;
    let config = device.active_config_descriptor()?;
    for iface in config.interfaces() {
        for iface_desc in iface.descriptors() {
            let _ = handle.claim_interface(iface_desc.interface_number());
        }
    }

    // Write to 0x7F00 + fpga_addr (FPGA register space)
    let abs_addr: u16 = 0x7F00 | (fpga_addr as u16);
    println!("Writing 0x{value:02X} to FPGA register 0x{abs_addr:04X} (port A) via 0xA0...");
    match ezusb_write_ram(&handle, abs_addr as u32, &[value]) {
        Ok(()) => println!("  Write OK"),
        Err(e) => println!("  Write error: {e}"),
    }

    // Verify by reading back (if possible)
    let mut buf = [0u8; 64];
    match handle.read_control(0xC0, 0xA3, abs_addr, 0, &mut buf, TIMEOUT) {
        Ok(len) => {
            if len > 0 {
                println!("  Read-back: 0x{:02X}", buf[0]);
            }
        }
        Err(e) => println!("  Read-back: {e}"),
    }

    Ok(())
}

fn parse_hex_list(s: &str) -> Vec<u8> {
    s.split(',')
        .filter_map(|tok| {
            let t = tok.trim();
            if let Some(h) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
                u8::from_str_radix(h, 16).ok()
            } else {
                t.parse::<u8>().ok()
            }
        })
        .collect()
}

fn open_device_handle() -> Result<rusb::DeviceHandle<GlobalContext>> {
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
    Ok(handle)
}

fn cmd_probe_eeprom(cmds: &str, inners: &str, with_select: bool, timeout_ms: u64) -> Result<()> {
    let cmd_bytes = parse_hex_list(cmds);
    let inner_bytes = parse_hex_list(inners);
    let timeout = Duration::from_millis(timeout_ms);
    let cmd_ep = 0x04u8;
    let data_ep = 0x82u8;

    println!("ProbeEeprom: {} cmd × {} inner = {} combos  with_select={with_select}  timeout={timeout_ms}ms",
        cmd_bytes.len(), inner_bytes.len(), cmd_bytes.len() * inner_bytes.len());
    println!("{:<10} {:<10} {:<8} {}", "read_cmd", "inner", "bytes", "hex (first 16)");
    println!("{}", "-".repeat(70));

    for &read_cmd in &cmd_bytes {
        for &inner in &inner_bytes {
            // Fresh handle each attempt — recovers from any stuck endpoint state
            let handle = match open_device_handle() {
                Ok(h) => h,
                Err(e) => {
                    println!("0x{read_cmd:02X}       0x{inner:02X}       OPEN_FAIL ({e})");
                    std::thread::sleep(Duration::from_millis(200));
                    continue;
                }
            };

            if with_select {
                let sel = [0x14u8, inner, 0x00];
                if handle.write_bulk(cmd_ep, &sel, timeout).is_err() {
                    println!("0x{read_cmd:02X}       0x{inner:02X}       WRITE_FAIL (select)");
                    continue;
                }
                std::thread::sleep(Duration::from_millis(50));
            }

            // Try 4-byte packet (inner=0xFF sentinel means skip 5th byte)
            let cmd: &[u8] = if inner == 0xFF {
                &[read_cmd, 0x00u8, 0x00u8, 0x00u8]
            } else {
                &[read_cmd, 0x00u8, 0x00u8, 0x00u8, inner]
            };
            if handle.write_bulk(cmd_ep, cmd, timeout).is_err() {
                println!("0x{read_cmd:02X}       0x{inner:02X}       WRITE_FAIL (read cmd)");
                continue;
            }

            let mut buf = [0u8; 64];
            match handle.read_bulk(data_ep, &mut buf, timeout) {
                Ok(len) => {
                    let hex: String = buf[..len.min(16)]
                        .iter()
                        .map(|b| format!("{b:02x}"))
                        .collect::<Vec<_>>()
                        .join(" ");
                    let all_ff = buf[..len].iter().all(|&b| b == 0xFF);
                    let all_zero = buf[..len].iter().all(|&b| b == 0x00);
                    let flag = if all_ff { " [all FF]" } else if all_zero { " [all 00]" } else { " [DATA]" };
                    println!("0x{read_cmd:02X}       0x{inner:02X}       {len:<8} {hex}{flag}");
                }
                Err(e) => {
                    println!("0x{read_cmd:02X}       0x{inner:02X}       TIMEOUT  ({e})");
                }
            }

            std::thread::sleep(Duration::from_millis(150));
        }
    }

    println!("\nDone. Look for [DATA] rows — those cmd/inner combos returned non-trivial bytes.");
    Ok(())
}
