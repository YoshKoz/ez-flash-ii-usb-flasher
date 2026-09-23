use anyhow::{Context, Result, bail};
use rusb::{Device, DeviceDescriptor, DeviceHandle, GlobalContext};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const BOOTLOADER_VID: u16 = 0x0547;
pub const BOOTLOADER_PID: u16 = 0x2131;
pub const EZWRITER_VID: u16 = 0x0548;
pub const EZWRITER_PID: u16 = 0x1005;
pub const CPUCS_ADDR: u16 = 0x7F92;
pub const CMD_EP: u8 = 0x04;
pub const DATA_EP: u8 = 0x82;
pub const ROM_READ_DELAY_MS: u64 = 5;
/// How many read/confirm rounds a chunk gets before the cartridge is declared
/// unstable. A marginal cart usually settles within two or three reads.
pub const ROM_READ_ATTEMPTS: u32 = 4;
const TIMEOUT: Duration = Duration::from_secs(5);

/// Format the first 16 bytes of a chunk for error messages.
fn hex_prefix(bytes: &[u8; 64]) -> String {
    bytes[..16]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Run `read` against one address until two consecutive results agree, retrying
/// transient failures, then give up rather than returning data that cannot be
/// trusted.
///
/// Both the ROM path and the save path suffer the same failure mode: the
/// firmware streams through one EP2 buffer, so a mistimed or marginal read
/// returns a *full-length* but stale block. Nothing in a single read
/// distinguishes that from real data, so the only defence is to read twice and
/// compare. With `confirm` false only hard errors are retried: faster, but a
/// bad read can reach the output file unnoticed.
fn read_confirmed<F>(addr: u32, confirm: bool, mut read: F) -> Result<[u8; 64]>
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

/// Resolve a bundled data file (firmware loader tables, etc.) without depending
/// on the process working directory. Checks the current dir first, then the
/// directory containing the executable. Falls back to the bare name so callers
/// still surface a meaningful "not found" path in their error.
pub fn resolve_asset(name: &str) -> PathBuf {
    let cwd = PathBuf::from(name);
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

/// Canonical save-size lookup — the single source of truth shared by the GUI.
/// Matches the exact catalogue strings the game DB emits first, then falls back
/// to a tolerant substring parse so unrecognised header text still yields a sane
/// size rather than a wrong one.
pub fn save_size_bytes(save_type: &str) -> usize {
    match save_type.trim() {
        "FLASH 128K" => return 128 * 1024,
        "FLASH 256K" => return 256 * 1024,
        "FLASH 64K" => return 64 * 1024,
        "SRAM 256K" => return 256 * 1024,
        "SRAM 64K" => return 64 * 1024,
        "SRAM 32K" => return 32 * 1024,
        "EEPROM 8K" => return 8 * 1024,
        "EEPROM 512" => return 512,
        _ => {}
    }
    // Fallback for header text that doesn't match the catalogue exactly.
    let t = save_type.to_ascii_uppercase();
    if t.contains("256K") {
        256 * 1024
    } else if t.contains("128K") {
        128 * 1024
    } else if t.contains("64K") {
        64 * 1024
    } else if t.contains("8K") {
        8 * 1024
    } else if t.contains("512") {
        512
    } else {
        32 * 1024
    }
}

pub struct GameDBEntry {
    pub code: &'static str,
    pub title: &'static str,
    pub save_type: &'static str,
    pub rom_size: u32,
}

pub const GAME_DB: &[GameDBEntry] = &[
    GameDBEntry {
        code: "BPGE",
        title: "Pokemon Leaf Green",
        save_type: "FLASH 128K",
        rom_size: 0x1000000,
    },
    GameDBEntry {
        code: "BPGI",
        title: "Pokemon Leaf Green (Italian)",
        save_type: "FLASH 128K",
        rom_size: 0x1000000,
    },
    GameDBEntry {
        code: "BPRE",
        title: "Pokemon Fire Red",
        save_type: "FLASH 128K",
        rom_size: 0x1000000,
    },
    GameDBEntry {
        code: "BPRI",
        title: "Pokemon Fire Red (Italian)",
        save_type: "FLASH 128K",
        rom_size: 0x1000000,
    },
    GameDBEntry {
        code: "AXVE",
        title: "Pokemon Ruby",
        save_type: "FLASH 128K",
        rom_size: 0x1000000,
    },
    GameDBEntry {
        code: "AXPE",
        title: "Pokemon Sapphire",
        save_type: "FLASH 128K",
        rom_size: 0x1000000,
    },
    GameDBEntry {
        code: "AXTE",
        title: "Pokemon Emerald",
        save_type: "FLASH 128K",
        rom_size: 0x1000000,
    },
    GameDBEntry {
        code: "AXFE",
        title: "Pokemon Fire Red (old)",
        save_type: "EEPROM 512",
        rom_size: 0x1000000,
    },
    GameDBEntry {
        code: "AZLE",
        title: "Pokemon Diamond/Pearl Hack",
        save_type: "EEPROM 8K",
        rom_size: 0x1000000,
    },
    GameDBEntry {
        code: "AZLP",
        title: "Pokemon Mystery Dungeon",
        save_type: "EEPROM 8K",
        rom_size: 0x1000000,
    },
    GameDBEntry {
        code: "BPEB",
        title: "Pokemon Pinball R&S",
        save_type: "SRAM 256K",
        rom_size: 0x1000000,
    },
    GameDBEntry {
        code: "AGBJ",
        title: "Mario & Luigi: Superstar Saga",
        save_type: "EEPROM 512",
        rom_size: 0x1000000,
    },
    GameDBEntry {
        code: "ABQP",
        title: "Zelda: Minish Cap",
        save_type: "SRAM 64K",
        rom_size: 0x1000000,
    },
    GameDBEntry {
        code: "AZEJ",
        title: "Zelda: A Link to the Past",
        save_type: "SRAM 64K",
        rom_size: 0x1000000,
    },
    GameDBEntry {
        code: "AROP",
        title: "WarioWare Inc.",
        save_type: "EEPROM 512",
        rom_size: 0x1000000,
    },
    GameDBEntry {
        code: "A3AP",
        title: "Super Mario Advance",
        save_type: "SRAM 256K",
        rom_size: 0x1000000,
    },
    GameDBEntry {
        code: "A4AP",
        title: "Super Mario Advance 2",
        save_type: "SRAM 256K",
        rom_size: 0x1000000,
    },
    GameDBEntry {
        code: "A5AP",
        title: "Super Mario Advance 3",
        save_type: "SRAM 256K",
        rom_size: 0x1000000,
    },
    GameDBEntry {
        code: "AMCP",
        title: "Super Mario Advance 4",
        save_type: "SRAM 256K",
        rom_size: 0x1000000,
    },
    GameDBEntry {
        code: "AV3P",
        title: "Mario Kart Super Circuit",
        save_type: "EEPROM 512",
        rom_size: 0x1000000,
    },
    GameDBEntry {
        code: "AMBP",
        title: "Metroid Fusion",
        save_type: "SRAM 256K",
        rom_size: 0x1000000,
    },
    GameDBEntry {
        code: "BZQE",
        title: "Metroid Zero Mission",
        save_type: "SRAM 256K",
        rom_size: 0x1000000,
    },
    GameDBEntry {
        code: "AAAP",
        title: "F-Zero",
        save_type: "EEPROM 512",
        rom_size: 0x1000000,
    },
    GameDBEntry {
        code: "AFQE",
        title: "Fire Emblem",
        save_type: "FLASH 128K",
        rom_size: 0x1000000,
    },
    GameDBEntry {
        code: "ABRE",
        title: "Fire Emblem: The Sacred Stones",
        save_type: "FLASH 128K",
        rom_size: 0x1000000,
    },
    GameDBEntry {
        code: "AGBE",
        title: "Golden Sun",
        save_type: "SRAM 64K",
        rom_size: 0x1000000,
    },
    GameDBEntry {
        code: "AGZP",
        title: "Golden Sun: The Lost Age",
        save_type: "SRAM 64K",
        rom_size: 0x1000000,
    },
    GameDBEntry {
        code: "AC7P",
        title: "Castlevania: Aria of Sorrow",
        save_type: "SRAM 64K",
        rom_size: 0x1000000,
    },
    GameDBEntry {
        code: "ABOP",
        title: "Castlevania: Harmony of Dissonance",
        save_type: "EEPROM 512",
        rom_size: 0x1000000,
    },
    GameDBEntry {
        code: "AVAP",
        title: "Final Fantasy VI Advance",
        save_type: "SRAM 64K",
        rom_size: 0x1000000,
    },
    GameDBEntry {
        code: "AF3P",
        title: "Final Fantasy Tactics Advance",
        save_type: "FLASH 128K",
        rom_size: 0x1000000,
    },
    GameDBEntry {
        code: "B5EW",
        title: "Dragon Ball Z: Buu's Fury",
        save_type: "EEPROM 512",
        rom_size: 0x1000000,
    },
    GameDBEntry {
        code: "AZCJ",
        title: "Crash Bandicoot Advance",
        save_type: "EEPROM 512",
        rom_size: 0x1000000,
    },
    GameDBEntry {
        code: "BGBP",
        title: "Boktai",
        save_type: "SRAM 64K",
        rom_size: 0x1000000,
    },
    GameDBEntry {
        code: "AGBP",
        title: "Advance Wars",
        save_type: "SRAM 64K",
        rom_size: 0x1000000,
    },
    GameDBEntry {
        code: "AW2P",
        title: "Advance Wars 2",
        save_type: "FLASH 128K",
        rom_size: 0x1000000,
    },
];

pub fn lookup_game(code: &str) -> Option<&'static GameDBEntry> {
    GAME_DB.iter().find(|e| e.code == code)
}

pub enum DeviceMode {
    Bootloader,
    Active,
    None,
}

pub struct CartHeader {
    pub title: String,
    pub code: String,
    pub maker: String,
    pub save_type: String,
    pub rom_size: u32,
    pub raw_header: [u8; 256],
}

pub fn find_device(vid: u16, pid: u16) -> Result<(Device<GlobalContext>, DeviceDescriptor)> {
    for device in rusb::devices()?.iter() {
        let desc = device.device_descriptor()?;
        if desc.vendor_id() == vid && desc.product_id() == pid {
            return Ok((device, desc));
        }
    }
    bail!("No device found VID={:04X} PID={:04X}", vid, pid);
}

pub fn detect_mode() -> DeviceMode {
    if find_device(BOOTLOADER_VID, BOOTLOADER_PID).is_ok() {
        return DeviceMode::Bootloader;
    }
    if find_device(EZWRITER_VID, EZWRITER_PID).is_ok() {
        return DeviceMode::Active;
    }
    DeviceMode::None
}

pub fn open_and_claim(
    vid: u16,
    pid: u16,
) -> Result<(
    Device<GlobalContext>,
    DeviceHandle<GlobalContext>,
    DeviceDescriptor,
)> {
    let (device, desc) = find_device(vid, pid)?;
    let handle = device.open()?;
    let config = device.active_config_descriptor()?;
    for iface in config.interfaces() {
        for d in iface.descriptors() {
            let _ = handle.claim_interface(d.interface_number());
        }
    }
    for ep in 0x01u8..=0x07u8 {
        let _ = handle.clear_halt(ep);
        let _ = handle.clear_halt(ep | 0x80);
    }
    Ok((device, handle, desc))
}

fn ezusb_write_ram(handle: &DeviceHandle<GlobalContext>, address: u32, data: &[u8]) -> Result<()> {
    let wvalue = (address & 0xFFFF) as u16;
    let windex = ((address >> 16) & 0xFFFF) as u16;
    let actual = handle
        .write_control(0x40, 0xA0, wvalue, windex, data, TIMEOUT)
        .context("EZ-USB write failed")?;
    if actual != data.len() {
        bail!("Short write");
    }
    Ok(())
}

pub fn init_exact(table1_path: &PathBuf, table2_path: &PathBuf) -> Result<String> {
    let (_device, handle, _desc) = open_and_claim(BOOTLOADER_VID, BOOTLOADER_PID)?;
    let chunks1 = load_chunk_table(table1_path)?;
    let chunks2 = load_chunk_table(table2_path)?;

    ezusb_write_ram(&handle, CPUCS_ADDR as u32, &[1])?;
    ezusb_write_ram(&handle, CPUCS_ADDR as u32, &[1])?;
    write_chunks(&handle, &chunks1)?;
    ezusb_write_ram(&handle, CPUCS_ADDR as u32, &[0])?;
    ezusb_write_ram(&handle, CPUCS_ADDR as u32, &[1])?;
    write_chunks(&handle, &chunks2)?;
    ezusb_write_ram(&handle, CPUCS_ADDR as u32, &[1])?;
    ezusb_write_ram(&handle, CPUCS_ADDR as u32, &[0])?;
    std::thread::sleep(Duration::from_secs(4));
    Ok("Firmware loaded. Device should re-enumerate.".into())
}

fn load_chunk_table(path: &PathBuf) -> Result<Vec<(u16, Vec<u8>)>> {
    let data = std::fs::read(path)?;
    if data.len() < 10 || &data[..8] != b"EZWLDR1\0" {
        bail!("Invalid chunk table: {}", path.display());
    }
    let count = u16::from_le_bytes([data[8], data[9]]) as usize;
    let mut chunks = Vec::with_capacity(count);
    let mut off = 10;
    for _ in 0..count {
        if off + 3 > data.len() {
            bail!("Truncated");
        }
        let addr = u16::from_le_bytes([data[off], data[off + 1]]);
        let len = data[off + 2] as usize;
        off += 3;
        if off + len > data.len() {
            bail!("Truncated payload");
        }
        chunks.push((addr, data[off..off + len].to_vec()));
        off += len;
    }
    Ok(chunks)
}

fn write_chunks(handle: &DeviceHandle<GlobalContext>, chunks: &[(u16, Vec<u8>)]) -> Result<()> {
    for (addr, payload) in chunks {
        ezusb_write_ram(handle, *addr as u32, payload)?;
    }
    Ok(())
}

pub fn reset_jedec(handle: &DeviceHandle<GlobalContext>) {
    let seq: [(u8, u16); 4] = [(0xAA, 0xAAAA), (0x55, 0x5554), (0xF0, 0xAAAA), (0xFF, 0)];
    for (cb, a) in &seq {
        let da = a / 2;
        let c = [*cb, (da & 0xFF) as u8, ((da >> 8) & 0xFF) as u8, 0x00];
        let _ = handle.write_bulk(CMD_EP, &c, Duration::from_millis(500));
        std::thread::sleep(Duration::from_millis(5));
    }
}

// ---------------------------------------------------------------------------
// CartSession — streaming dump
// ---------------------------------------------------------------------------

/// Holds a single open USB handle for the duration of a cart operation.
/// Opens the device once, claims interfaces once, clears halts once.
pub struct CartSession {
    handle: DeviceHandle<GlobalContext>,
}

impl CartSession {
    /// Open device, claim interface, clear halts, and issue JEDEC reset.
    pub fn open() -> Result<Self> {
        let (_, handle, _) = open_and_claim(EZWRITER_VID, EZWRITER_PID)?;
        Ok(Self { handle })
    }

    /// Read a single 64-byte chunk from the cartridge at the given byte address.
    ///
    /// Protocol: write 4-byte command to EP4 OUT, sleep `ROM_READ_DELAY_MS`,
    /// read 64 bytes from EP2 IN. Word address goes in bytes[1..3], the 128 KB
    /// page number in byte[3], which covers the full 32 MB cartridge window.
    ///
    /// A short transfer is an error: accepting one would shift every following
    /// chunk in the output file.
    pub fn read_rom_chunk(&self, byte_addr: u32) -> Result<[u8; 64]> {
        let word_addr = byte_addr / 2;
        let addr_16 = (word_addr & 0xFFFF) as u16;
        let bank = (word_addr >> 16) as u8;
        let cmd = [
            0x01,
            (addr_16 & 0xFF) as u8,
            ((addr_16 >> 8) & 0xFF) as u8,
            bank,
        ];

        self.handle
            .write_bulk(CMD_EP, &cmd, TIMEOUT)
            .with_context(|| {
                format!("write 0x01 wa=0x{word_addr:x} at byte_addr=0x{byte_addr:06X}")
            })?;

        std::thread::sleep(Duration::from_millis(ROM_READ_DELAY_MS));

        let mut buf = [0u8; 64];
        let len = self
            .handle
            .read_bulk(DATA_EP, &mut buf, TIMEOUT)
            .with_context(|| format!("read EP2 at byte_addr=0x{byte_addr:06X}"))?;

        if len != 64 {
            bail!(
                "short read at byte_addr=0x{byte_addr:06X}: got {} bytes, expected 64",
                len
            );
        }

        Ok(buf)
    }

    /// Read a chunk, retrying transient failures, and optionally requiring two
    /// consecutive reads to agree. See [`read_confirmed`].
    pub fn read_rom_chunk_checked(&self, byte_addr: u32, confirm: bool) -> Result<[u8; 64]> {
        read_confirmed(byte_addr, confirm, || self.read_rom_chunk(byte_addr))
    }

    /// Read a single 64-byte chunk using the EP0 vendor request path.
    ///
    /// Protocol:
    ///   1. EP0 OUT vendor request: bRequest=0x01, wValue=word_addr[15:0],
    ///      wIndex=word_addr[23:16] in low byte.
    ///      Firmware (handler @ 0x075E) sends 3-byte address to FPGA then DMA-fills
    ///      EP2 IN with 64 bytes.
    ///   2. Sleep ROM_READ_DELAY_MS.
    ///   3. Read 64 bytes from EP2 IN bulk.
    ///
    /// This is the ONLY path that supports full 24-bit word addressing (16 MB ROM).
    /// The firmware AN2131 SETUPDAT layout (bRequest at 0x7CC0, not bmRequestType):
    ///   SETUPDAT[0]=bRequest, [1]=wValue_lo, [2]=wValue_hi, [3]=wIndex_lo.
    ///
    /// Not called by the shipped GUI (`read_rom_chunk`'s bank byte covers today's
    /// cartridge sizes) but kept for >8MB carts that need the full 24-bit range.
    #[allow(dead_code)]
    pub fn read_rom_chunk_ep0(&self, byte_addr: u32) -> Result<[u8; 64]> {
        let word_addr = byte_addr / 2;
        let wvalue: u16 = (word_addr & 0xFFFF) as u16;
        let windex: u16 = ((word_addr >> 16) & 0xFF) as u16;

        self.handle
            .write_control(
                0x40,   // bmRequestType: vendor, device, host→device
                0x01,   // bRequest: ROM read (→ SETUPDAT[0] = 0x7CC0)
                wvalue, // wValue: word_addr[15:0] → SETUPDAT[1:2]
                windex, // wIndex: word_addr[23:16] in low byte → SETUPDAT[3]
                &[],
                TIMEOUT,
            )
            .with_context(|| {
                format!("EP0 ROM read: wa=0x{word_addr:06X} byte_addr=0x{byte_addr:06X}")
            })?;

        std::thread::sleep(Duration::from_millis(ROM_READ_DELAY_MS));

        let mut buf = [0u8; 64];
        let len = self
            .handle
            .read_bulk(DATA_EP, &mut buf, TIMEOUT)
            .with_context(|| format!("EP2 read after EP0 cmd at byte_addr=0x{byte_addr:06X}"))?;

        if len != 64 {
            bail!("short EP0-path read at 0x{byte_addr:06X}: got {len} bytes, expected 64");
        }

        Ok(buf)
    }
    /// Stream the entire ROM to a file.
    ///
    /// - Deletes any old `.partial` file before starting.
    /// - Flushes every 256KB instead of every chunk.
    /// - sync_all only at end (not in the loop).
    /// - Progress callback every 64KB with `(bytes_read, total_bytes)`.
    /// - Validates first chunk magic (`24 FF AE 51`) at offset 4.
    /// - First writes to `{path}.partial`, renames to `path` only on success.
    /// - Verifies final file length equals `rom_size`.
    /// - Verifies GBA magic at offset 4.
    /// - With `confirm`, every chunk is read twice and must agree; an unstable
    ///   chunk is retried and the dump fails rather than writing bad bytes.
    pub fn dump_rom_stream<F>(
        &self,
        path: &Path,
        rom_size: u64,
        start_offset: u32,
        confirm: bool,
        progress: F,
    ) -> Result<()>
    where
        F: Fn(u64, u64) -> Result<()>,
    {
        let partial = partial_path(path);
        eprintln!("[dump] partial path: {}", partial.display());

        if partial.exists() {
            std::fs::remove_file(&partial)
                .with_context(|| format!("remove old {}", partial.display()))?;
            eprintln!("[dump] removed old .partial");
        }

        if let Some(parent) = partial.parent() {
            let probe = parent.join(".ezwriter_writable_test");
            std::fs::write(&probe, b"x")
                .with_context(|| format!("parent dir not writable: {}", parent.display()))?;
            std::fs::remove_file(&probe)?;
            eprintln!("[dump] parent dir writable: {}", parent.display());
        }

        let mut file = std::fs::File::create(&partial)
            .with_context(|| format!("create {}", partial.display()))?;
        eprintln!("[dump] created .partial");

        const FLUSH_INTERVAL: u64 = 256 * 1024;
        const PROGRESS_INTERVAL: u64 = 64 * 1024;
        const CHUNK_SIZE: u64 = 64;

        let mut written: u64 = 0;
        let mut last_flush: u64 = 0;
        let mut last_progress: u64 = 0;
        let mut header_validated = false;

        while written < rom_size {
            let wish = std::cmp::min(CHUNK_SIZE, rom_size - written) as usize;
            let chunk = self.read_rom_chunk_checked(start_offset + written as u32, confirm)?;

            if !header_validated {
                eprintln!(
                    "[dump] first 16 bytes: {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} {:02x}",
                    chunk[0],
                    chunk[1],
                    chunk[2],
                    chunk[3],
                    chunk[4],
                    chunk[5],
                    chunk[6],
                    chunk[7],
                    chunk[8],
                    chunk[9],
                    chunk[10],
                    chunk[11],
                    chunk[12],
                    chunk[13],
                    chunk[14],
                    chunk[15],
                );
                let magic = &chunk[4..8];
                eprintln!(
                    "[dump] magic: {:02x} {:02x} {:02x} {:02x}",
                    magic[0], magic[1], magic[2], magic[3]
                );
                if *magic != [0x24, 0xFF, 0xAE, 0x51] {
                    bail!(
                        "GBA magic FAILED on chunk 0: got {:02x} {:02x} {:02x} {:02x}",
                        magic[0],
                        magic[1],
                        magic[2],
                        magic[3]
                    );
                }
                header_validated = true;
                eprintln!("[dump] magic VALID");
            }

            file.write_all(&chunk[..wish]).with_context(|| {
                format!("write at 0x{:06X} chunk {}", written, written / CHUNK_SIZE)
            })?;
            written += wish as u64;

            if written - last_flush >= FLUSH_INTERVAL {
                file.flush()
                    .with_context(|| format!("flush at 0x{:06X}", written))?;
                last_flush = written;
            }

            if written - last_progress >= PROGRESS_INTERVAL || written == rom_size {
                let fs_len = std::fs::metadata(&partial)
                    .with_context(|| format!("metadata {}", partial.display()))?
                    .len();
                eprintln!(
                    "[dump] chunk={} 0x{:06X} w={} fs={}",
                    written / CHUNK_SIZE,
                    written,
                    written,
                    fs_len
                );
                if fs_len < written {
                    bail!(
                        "fs_len={fs_len} < written={written} fs stall (partial: {})",
                        partial.display()
                    );
                }
                progress(fs_len, rom_size)?;
                last_progress = written;
            }
        }

        file.flush().context("final flush")?;
        file.sync_all().context("final sync_all")?;
        drop(file);

        let final_len = std::fs::metadata(&partial)
            .with_context(|| format!("final metadata {}", partial.display()))?
            .len();
        eprintln!("[dump] final fs_len={} expected={}", final_len, rom_size);

        if final_len != rom_size {
            bail!(
                "size mismatch: expected {rom_size} got {final_len} ({})",
                partial.display()
            );
        }

        {
            let f = std::fs::File::open(&partial).context("re-open .partial")?;
            use std::io::Read;
            let mut v = [0u8; 8];
            (&f).read_exact(&mut v).context("read first 8 bytes")?;
            eprintln!(
                "[dump] final verify bytes 0-7: {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} {:02x}",
                v[0], v[1], v[2], v[3], v[4], v[5], v[6], v[7]
            );
            if v[4..8] != [0x24, 0xFF, 0xAE, 0x51] {
                bail!(
                    "final magic failed: {:02x} {:02x} {:02x} {:02x}",
                    v[4],
                    v[5],
                    v[6],
                    v[7]
                );
            }
        }

        eprintln!(
            "[dump] renaming {} -> {}",
            partial.display(),
            path.display()
        );
        std::fs::rename(&partial, path)
            .with_context(|| format!("rename {} -> {}", partial.display(), path.display()))?;
        eprintln!("[dump] DONE");
        Ok(())
    }
}

/// Return `path` with `.partial` appended.
pub fn partial_path(path: &Path) -> PathBuf {
    let mut p = path.to_path_buf();
    let s = p.to_string_lossy().to_string();
    p.set_file_name(format!("{}.partial", s));
    p
}

// ---------------------------------------------------------------------------
// Read a small number of chunks (used by Cart Info detection and save reads).
// Opens the device fresh each call.
// ---------------------------------------------------------------------------

/// Read `count` 64-byte chunks starting at `byte_addr`.
/// Used by Cart Info detection (count=4) and save read operations.
pub fn read_chunks(cmd_byte: u8, suffix: u8, byte_addr: u32, count: u32) -> Result<Vec<u8>> {
    let (_device, handle, _desc) = open_and_claim(EZWRITER_VID, EZWRITER_PID)?;

    if cmd_byte == 2 {
        let select = [0x14u8, suffix, 0x00];
        let _ = handle.write_bulk(CMD_EP, &select, Duration::from_millis(1000));
    }

    let mut all = Vec::with_capacity((count * 64) as usize);
    for chunk in 0..count {
        let addr = byte_addr + chunk * 64;
        let cmd = if cmd_byte == 2 {
            [
                0x02u8,
                (addr & 0xFF) as u8,
                ((addr >> 8) & 0xFF) as u8,
                ((addr >> 16) & 0xFF) as u8,
                suffix,
                0,
            ]
        } else {
            let word_addr = addr / 2;
            [
                0x01u8,
                (word_addr & 0xFF) as u8,
                ((word_addr >> 8) & 0xFF) as u8,
                0x00,
                0,
                0,
            ]
        };
        let cmd_len = if cmd_byte == 2 { 5 } else { 4 };
        handle
            .write_bulk(CMD_EP, &cmd[..cmd_len], TIMEOUT)
            .with_context(|| format!("read_chunks write at addr=0x{addr:06X}"))?;
        std::thread::sleep(Duration::from_millis(ROM_READ_DELAY_MS));

        let mut buf = [0u8; 64];
        let len = handle
            .read_bulk(DATA_EP, &mut buf, TIMEOUT)
            .with_context(|| format!("read_chunks read at addr=0x{addr:06X}"))?;
        all.extend_from_slice(&buf[..len]);
    }
    Ok(all)
}

fn write_reg(handle: &DeviceHandle<GlobalContext>, addr: u32, data: u16) -> Result<()> {
    let buf = [
        0x19u8,
        (addr & 0xFF) as u8,
        ((addr >> 8) & 0xFF) as u8,
        ((addr >> 16) & 0xFF) as u8,
        (data & 0xFF) as u8,
        ((data >> 8) & 0xFF) as u8,
    ];
    handle.write_bulk(CMD_EP, &buf, TIMEOUT)?;
    Ok(())
}

pub fn read_save_with_type(
    byte_addr: u32,
    count: u32,
    save_type: &str,
    confirm: bool,
) -> Result<Vec<u8>> {
    if !is_known_save_type(save_type) {
        bail!(
            "unrecognised save type '{save_type}' (game code not in the built-in database). \
             Refusing to guess: the fallback path sends cmd 0x02 with suffix 0x66, which locks the \
             cartridge CPLD until the device is replugged. Identify the chip with \
             `ezwriter-cli save-id`, then read explicitly with \
             `ezwriter-cli save-read -t f|s|e --output <file>`."
        );
    }
    if save_read_handler_byte(save_type) == 0x66 {
        bail!(
            "refusing to read '{save_type}' over cmd 0x02: the FLASH handler byte (0x66) makes the \
             firmware poll for a FLASH write completion that never arrives, locking the CPLD until \
             the device is replugged. 128KB FLASH saves must use read_flash128_save(); no verified \
             reader exists for other FLASH sizes."
        );
    }

    let (_device, handle, _desc) = open_and_claim(EZWRITER_VID, EZWRITER_PID)?;

    // Unlock EZ-Flash II CPLD before save-chip access. Without this, the save
    // chip is gated off and every read returns the same stale 128-byte buffer.
    write_reg(&handle, 0x9FE000, 0xD200)?;
    write_reg(&handle, 0x800000, 0x1500)?;
    write_reg(&handle, 0x802000, 0xD200)?;
    write_reg(&handle, 0x804000, 0x1500)?;

    let suffix = save_read_handler_byte(save_type);
    let select = [0x14u8, suffix, 0x00];
    let _ = handle.write_bulk(CMD_EP, &select, Duration::from_millis(1000));
    std::thread::sleep(Duration::from_millis(100));

    // Drain stale EP2 data (firmware auto-streams 8 packets; all must be consumed
    // before save-chip data is reliable).
    for _ in 0..8 {
        let mut drain = [0u8; 64];
        if handle
            .read_bulk(DATA_EP, &mut drain, Duration::from_millis(200))
            .is_err()
        {
            break;
        }
    }

    let mut all = Vec::with_capacity((count * 64) as usize);
    for chunk in 0..count {
        let addr = byte_addr + chunk * 64;
        let data = read_confirmed(addr, confirm, || {
            let cmd = [
                0x02u8,
                (addr & 0xFF) as u8,
                ((addr >> 8) & 0xFF) as u8,
                ((addr >> 16) & 0xFF) as u8,
                suffix,
            ];
            handle
                .write_bulk(CMD_EP, &cmd, TIMEOUT)
                .with_context(|| format!("save read write at addr=0x{addr:06X}"))?;
            std::thread::sleep(Duration::from_millis(50));

            let mut buf = [0u8; 64];
            let len = handle
                .read_bulk(DATA_EP, &mut buf, Duration::from_secs(30))
                .with_context(|| format!("save read at addr=0x{addr:06X}"))?;
            if len != 64 {
                bail!("short save read at addr=0x{addr:06X}: got {len} bytes, expected 64");
            }
            Ok(buf)
        })?;
        all.extend_from_slice(&data);
    }

    // Re-lock CPLD (best effort)
    let _ = write_reg(&handle, 0x9FC000, 0x1500);

    Ok(all)
}

/// Read a genuine 128KB GBA FLASH save (e.g. Pokémon Gen 3, Macronix MX29L010
/// C2:09) via the firmware-native save path. The save chip is on GBA /CS2 and is
/// only reachable through cmd 0x14 (select) + cmd 0x20 (byte write) + cmd 0x03
/// (read 64). The flash is two 64KB banks switched by a JEDEC command, not an
/// address pin. Returns 131072 bytes; leaves the flash in read-array mode.
pub fn read_flash128_save(confirm: bool, cb: impl Fn(u64, u64)) -> Result<Vec<u8>> {
    let (_device, handle, _desc) = open_and_claim(EZWRITER_VID, EZWRITER_PID)?;
    const BYTES_PER_BANK: u32 = 65536;
    let total = (BYTES_PER_BANK * 2) as u64;

    let fwrite = |addr: u16, data: u8| -> Result<()> {
        handle.write_bulk(
            CMD_EP,
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
                .read_bulk(DATA_EP, &mut buf, Duration::from_millis(40))
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

    // Select the FLASH save handler.
    handle.write_bulk(CMD_EP, &[0x14u8, 0x66, 0x00], TIMEOUT)?;
    std::thread::sleep(Duration::from_millis(50));
    drain();

    let mut all = Vec::with_capacity((BYTES_PER_BANK * 2) as usize);
    for bank in 0u8..=1 {
        bank_switch(bank)?;
        let mut off = 0u32;
        while off < BYTES_PER_BANK {
            let chunk = read_confirmed(off, confirm, || {
                drain();
                handle
                    .write_bulk(
                        CMD_EP,
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
                    .read_bulk(DATA_EP, &mut buf, Duration::from_secs(3))
                    .with_context(|| format!("FLASH128 bank{bank} off 0x{off:04X}"))?;
                if len != 64 {
                    bail!(
                        "short save read at bank{bank} off 0x{off:04X}: got {len} bytes, expected 64"
                    );
                }
                Ok(buf)
            });

            let chunk = match chunk {
                Ok(c) => c,
                Err(e) => {
                    // Leave the flash in read-array mode before bailing.
                    let _ = bank_switch(0);
                    let _ = fwrite(0x5555, 0xAA);
                    let _ = fwrite(0x2AAA, 0x55);
                    let _ = fwrite(0x5555, 0xF0);
                    return Err(e.context(format!(
                        "FLASH128 read failed at bank{bank} off 0x{off:04X}"
                    )));
                }
            };

            all.extend_from_slice(&chunk);
            cb(all.len() as u64, total);
            off += 64;
        }
    }

    // Restore bank 0 + read-array (F0).
    bank_switch(0)?;
    fwrite(0x5555, 0xAA)?;
    fwrite(0x2AAA, 0x55)?;
    fwrite(0x5555, 0xF0)?;

    Ok(all)
}

fn save_read_handler_byte(save_type: &str) -> u8 {
    if save_type.contains("EEPROM") {
        0x65 // XRL #0x65 branch at 0x07FF in tusbez.bin
    } else if save_type.contains("SRAM") {
        0x73 // cmd 0x14 SRAM select: (0x73 & 7) * 2 = 0x06 -> SRAM bus mapping
    } else {
        0x66 // FLASH handler, CJNE branch at 0x07D5 in tusbez.bin
    }
}

/// True when the built-in catalogue recognises this save type.
///
/// Unknown types must never be guessed at. `save_read_handler_byte` falls back
/// to the FLASH handler byte (0x66), and cmd 0x02 with suffix 0x66 is the packet
/// that hangs the 8051 in a FLASH write-completion poll and locks the CPLD until
/// the device is physically replugged.
pub fn is_known_save_type(save_type: &str) -> bool {
    save_type.contains("FLASH") || save_type.contains("SRAM") || save_type.contains("EEPROM")
}

pub fn gen3_save_signature_count(data: &[u8]) -> usize {
    const GEN3_SIG: [u8; 4] = [0x25, 0x20, 0x01, 0x08];
    data.windows(GEN3_SIG.len())
        .filter(|window| **window == GEN3_SIG)
        .count()
}

fn starts_with_known_rom_stub(data: &[u8]) -> bool {
    data.starts_with(&[0xff, 0x07, 0x00, 0x28, 0x0c, 0xd1, 0x10, 0x48])
        || data.starts_with(&[0xff, 0xef, 0x00, 0x28, 0x0c, 0xd1, 0x10, 0x48])
}

pub fn validate_save_dump(data: &[u8], save_type: &str) -> Result<()> {
    if data.is_empty() {
        bail!("save read returned no data");
    }
    if starts_with_known_rom_stub(data) {
        bail!("save data starts with the known ROM/stale endpoint pattern, not save RAM");
    }
    // A read that stopped early used to be handed to the user as a save file.
    let expected = save_size_bytes(save_type);
    if data.len() != expected {
        bail!(
            "save dump is {} bytes but a {save_type} save is {expected} bytes — the read stopped \
             early, so this is a partial dump, not a save",
            data.len()
        );
    }
    if save_type.contains("FLASH") && data.len() >= 128 * 1024 {
        let signatures = gen3_save_signature_count(data);
        if signatures < 14 {
            bail!(
                "FLASH save validation failed: found {signatures} Gen 3 section signatures, expected at least 14"
            );
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Header parsing and cart detection
// ---------------------------------------------------------------------------

/// Parse a GBA cartridge header from a raw byte buffer.
/// Validates the GBA magic at offset 4 and extracts title, code, maker, save type.
pub fn parse_gba_header(buf: &[u8]) -> Result<CartHeader> {
    if buf.len() < 0xB2 {
        bail!("Buffer too short for GBA header");
    }
    if buf[4..8] != [0x24, 0xFF, 0xAE, 0x51] {
        bail!("No valid GBA cartridge header. Insert cartridge.");
    }
    let title = String::from_utf8_lossy(&buf[0xA0..0xAC])
        .trim_end_matches(char::from(0))
        .to_string();
    let code = String::from_utf8_lossy(&buf[0xAC..0xB0])
        .trim_end_matches(char::from(0))
        .to_string();
    let maker = String::from_utf8_lossy(&buf[0xB0..0xB2])
        .trim_end_matches(char::from(0))
        .to_string();
    let save_type = lookup_game(&code)
        .map_or("UNKNOWN", |e| e.save_type)
        .to_string();
    let rom_size = lookup_game(&code).map_or(0x1000000, |e| e.rom_size);
    let mut raw_header = [0u8; 256];
    let n = buf.len().min(256);
    raw_header[..n].copy_from_slice(&buf[..n]);
    Ok(CartHeader {
        title,
        code,
        maker,
        save_type,
        rom_size,
        raw_header,
    })
}

pub fn read_cart_header() -> Result<CartHeader> {
    let buf = read_chunks(1, 0, 0, 4)?;
    parse_gba_header(&buf)
}

pub fn dump_to_file(path: &PathBuf, data: &[u8]) -> Result<()> {
    std::fs::write(path, data).with_context(|| format!("Failed to write {}", path.display()))
}

// ---------------------------------------------------------------------------
// Save write
// ---------------------------------------------------------------------------

/// Write save data to cartridge.
/// Protocol:
///   1. Select save type via cmd 0x14 + suffix byte ('f'=FLASH, 'e'=EEPROM, 's'=SRAM)
///   2. Erase sectors for FLASH using cmd 0x15
///   3. Write 64-byte chunks using cmd 0x03 + address + suffix + data
pub fn write_save(data: &[u8], save_type: &str, cb: impl Fn(u64, u64)) -> Result<String> {
    if !is_known_save_type(save_type) {
        bail!(
            "unrecognised save type '{save_type}' (game code not in the built-in database). \
             Refusing to write: the fallback would erase and program the chip as FLASH. \
             Identify the chip with `ezwriter-cli save-id` first."
        );
    }
    let (_device, handle, _desc) = open_and_claim(EZWRITER_VID, EZWRITER_PID)?;
    let suffix = if save_type.contains("EEPROM") {
        b'e'
    } else if save_type.contains("SRAM") {
        b's'
    } else {
        b'f'
    };

    // Select save type
    let select = [0x14u8, suffix, 0x00];
    handle.write_bulk(CMD_EP, &select, TIMEOUT)?;
    std::thread::sleep(Duration::from_millis(50));

    // Erase FLASH sectors
    if suffix == b'f' {
        let sector_size = 4096u32;
        let sectors = (data.len() as u32).div_ceil(sector_size);
        for s in 0..sectors {
            let sec_addr = s * sector_size;
            let erase = [
                0x15,
                (sec_addr & 0xFF) as u8,
                ((sec_addr >> 8) & 0xFF) as u8,
                ((sec_addr >> 16) & 0xFF) as u8,
                suffix,
            ];
            let _ = handle.write_bulk(CMD_EP, &erase[..5], TIMEOUT);
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    // Write 64-byte chunks
    let total = data.len() as u64;
    for (i, chunk) in data.chunks(64).enumerate() {
        let addr = (i * 64) as u32;
        let mut cmd = vec![
            0x03u8,
            (addr & 0xFF) as u8,
            ((addr >> 8) & 0xFF) as u8,
            ((addr >> 16) & 0xFF) as u8,
            suffix,
        ];
        cmd.extend_from_slice(chunk);
        handle.write_bulk(CMD_EP, &cmd, TIMEOUT)?;
        std::thread::sleep(Duration::from_millis(10));

        let mut status = [0u8; 64];
        let _ = handle.read_bulk(DATA_EP, &mut status, Duration::from_millis(20));

        if i % 64 == 0 || i + 1 == data.len().div_ceil(64) {
            let written = ((i + 1) * 64) as u64;
            cb(written.min(total), total);
        }
    }

    Ok(format!("Wrote {} bytes to cartridge save", data.len()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_game_found() {
        let e = lookup_game("BPRE").unwrap();
        assert_eq!(e.title, "Pokemon Fire Red");
        assert_eq!(e.save_type, "FLASH 128K");
    }

    #[test]
    fn lookup_game_not_found() {
        assert!(lookup_game("XXXX").is_none());
    }

    #[test]
    fn parse_gba_header_valid() {
        let mut buf = vec![0u8; 256];
        buf[4..8].copy_from_slice(&[0x24, 0xFF, 0xAE, 0x51]);
        buf[0xA0..0xA4].copy_from_slice(b"TEST");
        buf[0xAC..0xB0].copy_from_slice(b"BPRE");
        buf[0xB0..0xB2].copy_from_slice(b"01");

        let hdr = parse_gba_header(&buf).unwrap();
        assert_eq!(hdr.title, "TEST");
        assert_eq!(hdr.code, "BPRE");
        assert_eq!(hdr.maker, "01");
        assert_eq!(hdr.save_type, "FLASH 128K");
    }

    #[test]
    fn parse_gba_header_unknown_code_is_flagged_unknown() {
        let mut buf = vec![0u8; 256];
        buf[4..8].copy_from_slice(&[0x24, 0xFF, 0xAE, 0x51]);
        buf[0xAC..0xB0].copy_from_slice(b"XXXX");
        let hdr = parse_gba_header(&buf).unwrap();
        assert_eq!(hdr.save_type, "UNKNOWN");
        assert!(!is_known_save_type(&hdr.save_type));
    }

    #[test]
    fn known_save_types_are_recognised() {
        assert!(is_known_save_type("FLASH 128K"));
        assert!(is_known_save_type("SRAM 32K"));
        assert!(is_known_save_type("EEPROM 512"));
        assert!(!is_known_save_type("UNKNOWN"));
    }

    #[test]
    fn save_handler_bytes_match_documented_select_suffixes() {
        assert_eq!(save_read_handler_byte("FLASH 128K"), 0x66);
        assert_eq!(save_read_handler_byte("SRAM 32K"), 0x73);
        assert_eq!(save_read_handler_byte("EEPROM 512"), 0x65);
        // Any type that misses the catalogue falls back to the FLASH handler,
        // which is exactly what the cmd 0x02 guards exist to catch.
        assert_eq!(save_read_handler_byte("UNKNOWN"), 0x66);
    }

    #[test]
    fn parse_gba_header_bad_magic_errors() {
        let buf = vec![0u8; 256];
        assert!(parse_gba_header(&buf).is_err());
    }

    #[test]
    fn parse_gba_header_too_short_errors() {
        let buf = vec![0u8; 0x80];
        assert!(parse_gba_header(&buf).is_err());
    }
}
