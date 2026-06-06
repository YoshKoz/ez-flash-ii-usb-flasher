mod app;
mod device;

pub const BUILD_STAMP: &str = "0.1.0";

fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    eprintln!("[build {}] args: {:?}", BUILD_STAMP, args);

    if args.len() > 1 {
        match args[1].as_str() {
            "--probe-header" => return cmd_probe_header(),
            "--dump-smoke" => {
                if args.len() < 3 {
                    eprintln!("usage: ezwriter-gui.exe --dump-smoke <output_path>");
                    std::process::exit(1);
                }
                return cmd_dump_smoke(&args[2]);
            }
            "--dump-full" => {
                if args.len() < 3 {
                    eprintln!("usage: ezwriter-gui.exe --dump-full <output_path>");
                    std::process::exit(1);
                }
                return cmd_dump_full(&args[2]);
            }
            "--dangerous-probe-addresses-i-accept-hardware-lockup-risk" => {
                return cmd_probe_addresses();
            }
            "--probe-ep0-rom" => {
                return cmd_probe_ep0_rom();
            }
            "--dump-ep0" => {
                if args.len() < 3 {
                    eprintln!("usage: ezwriter-gui.exe --dump-ep0 <output_path>");
                    std::process::exit(1);
                }
                return cmd_dump_ep0(&args[2]);
            }
            "--probe-ep0-repeat" => {
                let addr = if args.len() >= 3 {
                    u32::from_str_radix(args[2].trim_start_matches("0x"), 16)
                        .expect("invalid hex addr")
                } else {
                    0u32
                };
                return cmd_probe_ep0_repeat(addr);
            }
            "--scan-ep0-from-zero" => {
                return cmd_scan_ep0_from_zero();
            }
            "--probe-ep0-header" => {
                if args.len() < 3 {
                    eprintln!("usage: ezwriter-gui.exe --probe-ep0-header <hex_flash_offset>");
                    eprintln!("  e.g. --probe-ep0-header 400000");
                    std::process::exit(1);
                }
                let offset = u32::from_str_radix(args[2].trim_start_matches("0x"), 16)
                    .expect("invalid hex offset");
                return cmd_probe_ep0_header(offset);
            }
            "--dump-ep0-at" => {
                if args.len() < 5 {
                    eprintln!(
                        "usage: ezwriter-gui.exe --dump-ep0-at <hex_start> <hex_size> <output_path>"
                    );
                    eprintln!("  e.g. --dump-ep0-at 400000 400000 rom.gba");
                    std::process::exit(1);
                }
                let start = u32::from_str_radix(args[2].trim_start_matches("0x"), 16)
                    .expect("invalid hex start");
                let size = u64::from_str_radix(args[3].trim_start_matches("0x"), 16)
                    .expect("invalid hex size");
                return cmd_dump_ep0_at(start, size, &args[4]);
            }
            _ => {}
        }
    }

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([640.0, 480.0])
            .with_title(format!("EZ-Flash II USB Flasher {}", BUILD_STAMP)),
        ..Default::default()
    };
    eframe::run_native(
        &format!("EZ-Flash II USB Flasher {}", BUILD_STAMP),
        options,
        Box::new(|_cc| Ok(Box::new(app::EzWriterApp::default()))),
    )
}

fn cmd_probe_header() -> eframe::Result<()> {
    use std::io::Write;
    eprintln!("[probe-header] reading 256 bytes...");
    match device::read_cart_header() {
        Ok(hdr) => {
            eprintln!("[probe-header] Title: {}", hdr.title);
            eprintln!("[probe-header] Code: {}", hdr.code);
            eprintln!("[probe-header] Maker: {}", hdr.maker);
            eprintln!("[probe-header] ROM size: {} bytes", hdr.rom_size);
            eprintln!("[probe-header] Save type: {}", hdr.save_type);
            for (i, b) in hdr.raw_header.iter().enumerate() {
                if i % 16 == 0 {
                    eprint!("\n{:04x}: ", i);
                }
                eprint!("{:02x} ", b);
            }
            eprintln!();
            let path = "header_probe.bin";
            let mut f = std::fs::File::create(path).expect("create");
            f.write_all(&hdr.raw_header).expect("write");
            f.flush().expect("flush");
            eprintln!(
                "[probe-header] wrote {} ({} bytes)",
                path,
                hdr.raw_header.len()
            );
            let desktop = format!(
                "{}\\Desktop\\header_probe.bin",
                std::env::var("USERPROFILE").unwrap_or_else(|_| ".".into())
            );
            let mut f2 = std::fs::File::create(&desktop).expect("create desktop");
            f2.write_all(&hdr.raw_header).expect("write");
            f2.flush().expect("flush");
            eprintln!(
                "[probe-header] wrote {} ({} bytes)",
                desktop,
                hdr.raw_header.len()
            );
            Ok(())
        }
        Err(e) => {
            eprintln!("[probe-header] ERROR: {e}");
            eprintln!("[probe-header] Attempting raw read...");
            match device::read_chunks(1, 0, 0, 4) {
                Ok(raw) => {
                    for (i, b) in raw.iter().enumerate() {
                        if i % 16 == 0 {
                            eprint!("\n{:04x}: ", i);
                        }
                        eprint!("{:02x} ", b);
                    }
                    eprintln!();
                    let path = "header_raw_probe.bin";
                    let mut f = std::fs::File::create(path).expect("create");
                    f.write_all(&raw).expect("write");
                    f.flush().expect("flush");
                    eprintln!("[probe-header] wrote {} ({} bytes)", path, raw.len());
                }
                Err(e2) => eprintln!("[probe-header] Raw read failed: {e2}"),
            }
            std::process::exit(1);
        }
    }
}

fn cmd_probe_addresses() -> eframe::Result<()> {
    use std::time::Duration;

    eprintln!("[probe-addresses] build: {}", BUILD_STAMP);

    let (_device, handle, _desc) =
        device::open_and_claim(device::EZWRITER_VID, device::EZWRITER_PID)
            .expect("open device failed");
    eprintln!("[probe-addresses] device opened OK");

    let offsets: [u32; 8] = [
        0x000000, 0x020000, 0x040000, 0x080000, 0x100000, 0x200000, 0x400000, 0x800000,
    ];

    let ep_out: u8 = 0x04;
    let ep_in: u8 = 0x82;
    let timeout = Duration::from_secs(5);
    let delay = Duration::from_millis(device::ROM_READ_DELAY_MS);

    // Helper to send cmd and read 64 bytes
    let send_recv = |cmd: &[u8], _offset: u32| -> Option<Vec<u8>> {
        handle.write_bulk(ep_out, cmd, timeout).ok()?;
        std::thread::sleep(delay);
        let mut buf = [0u8; 64];
        let len = handle.read_bulk(ep_in, &mut buf, timeout).ok()?;
        if len != 64 {
            return None;
        }
        Some(buf.to_vec())
    };

    // Test protocols
    #[allow(dead_code)]
    struct Proto {
        name: &'static str,
        run: &'static [u8], // not used, we inline
    }

    // Each protocol as a separate loop

    // A: 0x01 with 16-bit word addr
    eprintln!("\n===== Protocol A: 0x01 16bit word addr (cur) =====");
    {
        #[allow(unused_mut, unused_variables)]
        let mut ref_data: Option<Vec<u8>> = None;
        let mut all_same = true;
        for &off in &offsets {
            let wa = (off / 2) as u16;
            let cmd = [0x01u8, (wa & 0xFF) as u8, ((wa >> 8) & 0xFF) as u8, 0x00];
            if let Some(data) = send_recv(&cmd, off) {
                let f16 = &data[..16];
                eprintln!(
                    "  0x{:07X}: {:02x} {:02x} {:02x} {:02x} ...",
                    off, f16[0], f16[1], f16[2], f16[3]
                );
                if let Some(ref r) = ref_data {
                    if &data[..] != r.as_slice() {
                        eprintln!("    >> DIFFERENT from offset 0x000000");
                        all_same = false;
                    } else {
                        eprintln!("    >> MATCHES offset 0x000000");
                    }
                } else {
                    ref_data = Some(data);
                    eprintln!("    >> REFERENCE saved");
                }
            } else {
                eprintln!("  0x{:07X}: FAILED (no data)", off);
            }
        }
        if all_same {
            eprintln!("  >> WRAPPING (all identical)");
        } else {
            eprintln!("  >> NOT WRAPPING");
        }
    }

    // B: 0x01 with 24-bit word addr
    eprintln!("\n===== Protocol B: 0x01 24bit word addr =====");
    {
        #[allow(unused_mut, unused_variables)]
        let mut ref_data: Option<Vec<u8>> = None;
        let mut all_same = true;
        for &off in &offsets {
            let wa = off as u64 / 2;
            let cmd = [
                0x01u8,
                (wa & 0xFF) as u8,
                ((wa >> 8) & 0xFF) as u8,
                ((wa >> 16) & 0xFF) as u8,
            ];
            if let Some(data) = send_recv(&cmd, off) {
                let f16 = &data[..16];
                eprintln!(
                    "  0x{:07X}: {:02x} {:02x} {:02x} {:02x} ...",
                    off, f16[0], f16[1], f16[2], f16[3]
                );
                if let Some(ref r) = ref_data {
                    if &data[..] != r.as_slice() {
                        eprintln!("    >> DIFFERENT from offset 0x000000");
                        all_same = false;
                    } else {
                        eprintln!("    >> MATCHES offset 0x000000");
                    }
                } else {
                    ref_data = Some(data);
                    eprintln!("    >> REFERENCE saved");
                }
            } else {
                eprintln!("  0x{:07X}: FAILED", off);
            }
        }
        if all_same {
            eprintln!("  >> WRAPPING");
        } else {
            eprintln!("  >> NOT WRAPPING");
        }
    }

    // C: 0x02 24bit byte addr suffix=0x00
    eprintln!("\n===== Protocol C: 0x02 24bit byte addr suffix=0x00 =====");
    {
        #[allow(unused_mut, unused_variables)]
        let mut ref_data: Option<Vec<u8>> = None;
        let mut all_same = true;
        for &off in &offsets {
            let cmd = [
                0x02u8,
                (off & 0xFF) as u8,
                ((off >> 8) & 0xFF) as u8,
                ((off >> 16) & 0xFF) as u8,
                0x00,
            ];
            if let Some(data) = send_recv(&cmd, off) {
                let f16 = &data[..16];
                eprintln!(
                    "  0x{:07X}: {:02x} {:02x} {:02x} {:02x} ...",
                    off, f16[0], f16[1], f16[2], f16[3]
                );
                if let Some(ref r) = ref_data {
                    if &data[..] != r.as_slice() {
                        eprintln!("    >> DIFFERENT");
                        all_same = false;
                    } else {
                        eprintln!("    >> MATCHES 0x000000");
                    }
                } else {
                    ref_data = Some(data);
                    eprintln!("    >> REFERENCE");
                }
            } else {
                eprintln!("  0x{:07X}: FAILED", off);
            }
        }
        if all_same {
            eprintln!("  >> WRAPPING");
        } else {
            eprintln!("  >> NOT WRAPPING");
        }
    }

    // D1: 0x02 24bit byte addr suffix=0x66
    eprintln!("\n===== Protocol D1: 0x02 24bit byte addr suffix=0x66 =====");
    {
        #[allow(unused_mut, unused_variables)]
        let mut ref_data: Option<Vec<u8>> = None;
        let mut all_same = true;
        for &off in &offsets {
            let cmd = [
                0x02u8,
                (off & 0xFF) as u8,
                ((off >> 8) & 0xFF) as u8,
                ((off >> 16) & 0xFF) as u8,
                0x66,
            ];
            if let Some(data) = send_recv(&cmd, off) {
                let f16 = &data[..16];
                eprintln!(
                    "  0x{:07X}: {:02x} {:02x} {:02x} {:02x} ...",
                    off, f16[0], f16[1], f16[2], f16[3]
                );
                if let Some(ref r) = ref_data {
                    if &data[..] != r.as_slice() {
                        eprintln!("    >> DIFFERENT");
                        all_same = false;
                    } else {
                        eprintln!("    >> MATCHES 0x000000");
                    }
                } else {
                    ref_data = Some(data);
                    eprintln!("    >> REFERENCE");
                }
            } else {
                eprintln!("  0x{:07X}: FAILED", off);
            }
        }
        if all_same {
            eprintln!("  >> WRAPPING");
        } else {
            eprintln!("  >> NOT WRAPPING");
        }
    }

    // D2-4: 0x02 with other suffixes
    for (suffix, label) in &[
        (0x99u8, "D2: suffix=0x99"),
        (0x01u8, "D3: suffix=0x01"),
        (0x02u8, "D4: suffix=0x02"),
    ] {
        eprintln!("\n===== Protocol {} =====", label);
        #[allow(unused_mut, unused_variables)]
        let mut ref_data: Option<Vec<u8>> = None;
        let mut all_same = true;
        for &off in &offsets {
            let cmd = [
                0x02u8,
                (off & 0xFF) as u8,
                ((off >> 8) & 0xFF) as u8,
                ((off >> 16) & 0xFF) as u8,
                *suffix,
            ];
            if let Some(data) = send_recv(&cmd, off) {
                let f16 = &data[..16];
                eprintln!(
                    "  0x{:07X}: {:02x} {:02x} {:02x} {:02x} ...",
                    off, f16[0], f16[1], f16[2], f16[3]
                );
                if let Some(ref r) = ref_data {
                    if &data[..] != r.as_slice() {
                        eprintln!("    >> DIFFERENT");
                        all_same = false;
                    } else {
                        eprintln!("    >> MATCHES 0x000000");
                    }
                } else {
                    ref_data = Some(data);
                    eprintln!("    >> REFERENCE");
                }
            } else {
                eprintln!("  0x{:07X}: FAILED", off);
            }
        }
        if all_same {
            eprintln!("  >> WRAPPING");
        } else {
            eprintln!("  >> NOT WRAPPING");
        }
    }

    // Extra: 0x01 with 32-bit word addr (5 bytes)
    eprintln!("\n===== EXTRA: 0x01 32bit word addr (5 bytes) =====");
    for &off in &[0x000000u32, 0x020000, 0x040000, 0x100000, 0x200000] {
        let wa = off as u64 / 2;
        let cmd = [
            0x01u8,
            (wa & 0xFF) as u8,
            ((wa >> 8) & 0xFF) as u8,
            ((wa >> 16) & 0xFF) as u8,
            ((wa >> 24) & 0xFF) as u8,
        ];
        if let Some(data) = send_recv(&cmd, off) {
            eprintln!(
                "  0x{:07X}: {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} ...",
                off, data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7]
            );
        } else {
            eprintln!("  0x{:07X}: FAILED", off);
        }
    }

    // Extra: 0x13 bank select before 0x01 read
    for suffix in &[0x00u8, 0x01, 0x02, 0x10, 0x11, 0x12, 0x13, 0x80, 0x90, 0xA0] {
        eprintln!(
            "\n===== EXTRA: 0x01 16bit word addr with 4th byte=0x{:02x} =====",
            suffix
        );
        #[allow(unused_mut, unused_variables)]
        let mut ref_data: Option<Vec<u8>> = None;
        for &off in &[0x000000u32, 0x020000, 0x040000, 0x100000] {
            let wa = (off / 2) as u16;
            let cmd = [0x01u8, (wa & 0xFF) as u8, ((wa >> 8) & 0xFF) as u8, *suffix];
            if let Some(data) = send_recv(&cmd, off) {
                let f16 = &data[..16];
                eprintln!(
                    "  0x{:07X}: {:02x} {:02x} {:02x} {:02x} ...",
                    off, f16[0], f16[1], f16[2], f16[3]
                );
            } else {
                eprintln!("  0x{:07X}: FAILED", off);
            }
        }
    }

    // Extra: try vendor request 0xA0 read
    eprintln!("\n===== EXTRA: EP0 vendor 0xA0 read =====");
    for &off in &[0x000000u32, 0x020000] {
        let wa = off / 2;
        let mut buf = [0u8; 64];
        match handle.read_control(
            0xC0,
            0xA0,
            (wa & 0xFFFF) as u16,
            ((wa >> 16) & 0xFFFF) as u16,
            &mut buf,
            timeout,
        ) {
            Ok(len) => {
                eprintln!(
                    "  0x{:07X}: len={len} first={:02x} {:02x} {:02x} {:02x}",
                    off, buf[0], buf[1], buf[2], buf[3]
                );
            }
            Err(e) => eprintln!("  0x{:07X}: EP0 failed: {e}", off),
        }
    }

    Ok(())
}

fn cmd_dump_smoke(path: &str) -> eframe::Result<()> {
    use std::io::Write;

    eprintln!("[dump-smoke] build: {}", BUILD_STAMP);
    eprintln!("[dump-smoke] output: {}", path);

    let session = device::CartSession::open().expect("CartSession::open failed");
    eprintln!("[dump-smoke] CartSession opened OK");

    let mut file = std::fs::File::create(path).expect("create output file");
    let total_smoke: u64 = 4096;

    for chunk_idx in 0..(total_smoke / 64) as u32 {
        let byte_addr = chunk_idx * 64;
        let chunk = session
            .read_rom_chunk(byte_addr)
            .unwrap_or_else(|e| panic!("read_rom_chunk(0x{byte_addr:06X}) failed: {e}"));

        if chunk_idx == 0 {
            eprintln!(
                "[dump-smoke] chunk 0 first 16 bytes: {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} ...",
                chunk[0], chunk[1], chunk[2], chunk[3], chunk[4], chunk[5], chunk[6], chunk[7]
            );
            if chunk[4..8] != [0x24, 0xFF, 0xAE, 0x51] {
                panic!("GBA magic FAILED");
            }
        }

        file.write_all(&chunk).expect("write");
        file.flush().expect("flush");

        let meta_len = std::fs::metadata(path).expect("metadata").len();
        if chunk_idx % 8 == 0 {
            eprintln!(
                "[dump-smoke] chunk={} byte_addr=0x{byte_addr:06X} written={} fs_len={}",
                chunk_idx,
                (chunk_idx as u64 + 1) * 64,
                meta_len
            );
        }
    }

    file.sync_all().expect("sync_all");
    eprintln!("[dump-smoke] DONE path={}", path);
    Ok(())
}

fn cmd_dump_full(path: &str) -> eframe::Result<()> {
    eprintln!("[dump-full] build: {}", BUILD_STAMP);
    eprintln!("[dump-full] output: {}", path);

    let rom_size: u64 = match device::read_cart_header() {
        Ok(hdr) => {
            eprintln!(
                "[dump-full] Detected: {} [{}] size={}",
                hdr.title, hdr.code, hdr.rom_size
            );
            hdr.rom_size as u64
        }
        Err(e) => {
            eprintln!("[dump-full] Header probe failed: {e}, using 16MB");
            16 * 1024 * 1024
        }
    };

    let output = std::path::Path::new(path);
    let partial = device::partial_path(output);
    eprintln!("[dump-full] partial: {}", partial.display());

    let session = device::CartSession::open().expect("CartSession::open failed");
    eprintln!("[dump-full] CartSession opened OK");

    let result = session.dump_rom_stream(output, rom_size, 0, |fs_bytes, total_bytes| {
        let pct = fs_bytes as f64 / total_bytes as f64 * 100.0;
        eprintln!("[dump-full] {} / {} ({:.2}%)", fs_bytes, total_bytes, pct);
        Ok(())
    });

    match result {
        Ok(()) => {
            eprintln!("[dump-full] SUCCESS");
            let flen = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
            eprintln!(
                "[dump-full] Final file: {} bytes (expected {rom_size})",
                flen
            );
            Ok(())
        }
        Err(e) => {
            eprintln!("[dump-full] FAILED: {e}");
            if let Ok(meta) = std::fs::metadata(&partial) {
                eprintln!("[dump-full] .partial: {} bytes", meta.len());
            }
            std::process::exit(1);
        }
    }
}

/// Probe the EP0 24-bit ROM read path at key addresses.
/// Tests addresses 0x000000, 0x020000 (128KB), 0x040000 (256KB), 0x100000 (1MB).
/// Compares against EP4 reads at the same addresses to confirm or deny banking.
fn cmd_probe_ep0_rom() -> eframe::Result<()> {
    eprintln!("[probe-ep0-rom] build: {}", BUILD_STAMP);

    let session = device::CartSession::open().expect("CartSession::open failed");
    eprintln!("[probe-ep0-rom] CartSession opened");

    let offsets: &[(u32, &str)] = &[
        (0x000000, "header"),
        (0x020000, "128KB"),
        (0x040000, "256KB"),
        (0x080000, "512KB"),
        (0x100000, "1MB"),
        (0x200000, "2MB"),
        (0x400000, "4MB"),
        (0x800000, "8MB"),
    ];

    // Reference via EP4 path (wraps at 128KB)
    eprintln!("\n=== EP4 16-bit path (wraps at 128KB) ===");
    let mut ep4_ref: Option<[u8; 64]> = None;
    for &(off, label) in offsets {
        match session.read_rom_chunk(off) {
            Ok(chunk) => {
                let same = ep4_ref.as_ref() == Some(&chunk);
                eprintln!(
                    "  0x{:07X} ({label:6}): {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} {:02x}  {}",
                    off,
                    chunk[0],
                    chunk[1],
                    chunk[2],
                    chunk[3],
                    chunk[4],
                    chunk[5],
                    chunk[6],
                    chunk[7],
                    if same { "SAME as 0x000000" } else { "" }
                );
                if ep4_ref.is_none() {
                    ep4_ref = Some(chunk);
                }
            }
            Err(e) => eprintln!("  0x{:07X} ({label}): ERROR: {e}", off),
        }
    }

    // EP0 24-bit path
    eprintln!("\n=== EP0 24-bit path ===");
    let mut ep0_ref: Option<[u8; 64]> = None;
    for &(off, label) in offsets {
        match session.read_rom_chunk_ep0(off) {
            Ok(chunk) => {
                let same_as_0 = ep0_ref.as_ref() == Some(&chunk);
                eprintln!(
                    "  0x{:07X} ({label:6}): {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} {:02x}  {}",
                    off,
                    chunk[0],
                    chunk[1],
                    chunk[2],
                    chunk[3],
                    chunk[4],
                    chunk[5],
                    chunk[6],
                    chunk[7],
                    if same_as_0 {
                        "SAME as 0x000000"
                    } else {
                        "DIFFERENT"
                    }
                );
                if ep0_ref.is_none() {
                    ep0_ref = Some(chunk);
                }
            }
            Err(e) => eprintln!("  0x{:07X} ({label}): ERROR: {e}", off),
        }
    }

    if let (Some(ep4), Some(ep0)) = (ep4_ref, ep0_ref) {
        if ep4 == ep0 {
            eprintln!("\n[probe-ep0-rom] EP4 and EP0 agree at 0x000000 — header valid.");
        } else {
            eprintln!("\n[probe-ep0-rom] WARNING: EP4 vs EP0 differ at 0x000000!");
        }
    }

    Ok(())
}

/// Dump full ROM using the EP0 24-bit path.
fn cmd_dump_ep0(path: &str) -> eframe::Result<()> {
    eprintln!("[dump-ep0] build: {}", BUILD_STAMP);
    eprintln!("[dump-ep0] output: {}", path);

    let rom_size: u64 = match device::read_cart_header() {
        Ok(hdr) => {
            eprintln!(
                "[dump-ep0] Detected: {} [{}] size={}",
                hdr.title, hdr.code, hdr.rom_size
            );
            hdr.rom_size as u64
        }
        Err(e) => {
            eprintln!("[dump-ep0] Header probe failed: {e}, using 16MB");
            16 * 1024 * 1024
        }
    };

    let output = std::path::Path::new(path);
    let partial = device::partial_path(output);
    eprintln!("[dump-ep0] partial: {}", partial.display());

    let session = device::CartSession::open().expect("CartSession::open failed");
    eprintln!("[dump-ep0] CartSession opened OK");

    let result = session.dump_rom_stream(output, rom_size, 0, |fs_bytes, total_bytes| {
        let pct = fs_bytes as f64 / total_bytes as f64 * 100.0;
        eprintln!("[dump-ep0] {} / {} ({:.2}%)", fs_bytes, total_bytes, pct);
        Ok(())
    });

    match result {
        Ok(()) => {
            eprintln!("[dump-ep0] SUCCESS");
            let flen = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
            eprintln!(
                "[dump-ep0] Final file: {} bytes (expected {rom_size})",
                flen
            );
            Ok(())
        }
        Err(e) => {
            eprintln!("[dump-ep0] FAILED: {e}");
            if let Ok(meta) = std::fs::metadata(&partial) {
                eprintln!("[dump-ep0] .partial: {} bytes", meta.len());
            }
            std::process::exit(1);
        }
    }
}

/// Read and display the GBA ROM header at an arbitrary EP0 flash byte offset.
/// Reads 192 bytes (3 chunks of 64), covers the full 192-byte GBA header.
fn cmd_probe_ep0_header(flash_offset: u32) -> eframe::Result<()> {
    eprintln!("[probe-ep0-header] build: {}", BUILD_STAMP);
    eprintln!("[probe-ep0-header] flash_offset=0x{:06X}", flash_offset);

    let session = device::CartSession::open().expect("CartSession::open failed");

    let mut header = [0u8; 192];
    for i in 0..3u32 {
        let chunk = session
            .read_rom_chunk_ep0(flash_offset + i * 64)
            .unwrap_or_else(|e| panic!("read chunk {i} failed: {e}"));
        header[(i as usize * 64)..((i as usize + 1) * 64)].copy_from_slice(&chunk);
    }

    // GBA header layout (all offsets relative to ROM byte 0 / flash_offset)
    let magic_ok = header[4..8] == [0x24, 0xFF, 0xAE, 0x51];
    let title: &str = std::str::from_utf8(&header[0xA0..0xAC]).unwrap_or("???");
    let code: &str = std::str::from_utf8(&header[0xAC..0xB0]).unwrap_or("????");
    let maker: &str = std::str::from_utf8(&header[0xB0..0xB2]).unwrap_or("??");
    let rom_ver = header[0xBC];
    let checksum = header[0xBD];

    eprintln!(
        "[probe-ep0-header] GBA magic: {} ({:02x} {:02x} {:02x} {:02x})",
        if magic_ok { "VALID" } else { "INVALID" },
        header[4],
        header[5],
        header[6],
        header[7]
    );
    eprintln!(
        "[probe-ep0-header] Title    : {:?}",
        title.trim_end_matches('\0')
    );
    eprintln!("[probe-ep0-header] Game code: {:?}", code);
    eprintln!("[probe-ep0-header] Maker    : {:?}", maker);
    eprintln!("[probe-ep0-header] ROM ver  : 0x{:02X}", rom_ver);
    eprintln!("[probe-ep0-header] Checksum : 0x{:02X}", checksum);

    eprintln!("[probe-ep0-header] First 16 bytes:");
    eprintln!(
        "  {:02x} {:02x} {:02x} {:02x}  {:02x} {:02x} {:02x} {:02x}  {:02x} {:02x} {:02x} {:02x}  {:02x} {:02x} {:02x} {:02x}",
        header[0],
        header[1],
        header[2],
        header[3],
        header[4],
        header[5],
        header[6],
        header[7],
        header[8],
        header[9],
        header[10],
        header[11],
        header[12],
        header[13],
        header[14],
        header[15]
    );

    // Look up in game DB
    if let Some(entry) = device::GAME_DB
        .iter()
        .find(|e| e.code == code.trim_end_matches('\0'))
    {
        eprintln!(
            "[probe-ep0-header] DB match  : {} / save={} / rom_size=0x{:X}",
            entry.title, entry.save_type, entry.rom_size
        );
        eprintln!(
            "[probe-ep0-header] Dump cmd  : --dump-ep0-at {:X} {:X} <output.gba>",
            flash_offset, entry.rom_size
        );
    } else {
        eprintln!(
            "[probe-ep0-header] DB match  : none (unknown game code {:?})",
            code
        );
        eprintln!(
            "[probe-ep0-header] Dump hint : --dump-ep0-at {:X} <size_hex> <output.gba>",
            flash_offset
        );
    }

    Ok(())
}

/// Dump a specific byte range from EP0 flash space to a file.
fn cmd_dump_ep0_at(start: u32, size: u64, path: &str) -> eframe::Result<()> {
    eprintln!("[dump-ep0-at] build: {}", BUILD_STAMP);
    eprintln!(
        "[dump-ep0-at] start=0x{:06X} size=0x{:X} output={}",
        start, size, path
    );

    let output = std::path::Path::new(path);
    let partial = device::partial_path(output);

    let session = device::CartSession::open().expect("CartSession::open failed");
    eprintln!("[dump-ep0-at] CartSession opened OK");

    let result = session.dump_rom_stream(output, size, start, |fs_bytes, total_bytes| {
        let pct = fs_bytes as f64 / total_bytes as f64 * 100.0;
        eprintln!("[dump-ep0-at] {} / {} ({:.1}%)", fs_bytes, total_bytes, pct);
        Ok(())
    });

    match result {
        Ok(()) => {
            eprintln!("[dump-ep0-at] SUCCESS");
            let flen = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
            eprintln!(
                "[dump-ep0-at] Final file: {} bytes (expected {})",
                flen, size
            );
            Ok(())
        }
        Err(e) => {
            eprintln!("[dump-ep0-at] FAILED: {e}");
            if let Ok(meta) = std::fs::metadata(&partial) {
                eprintln!("[dump-ep0-at] .partial: {} bytes", meta.len());
            }
            std::process::exit(1);
        }
    }
}

/// Read the same EP0 address 4 times in a row.
/// If addressing is working, all 4 reads should return identical data.
/// If sequential/auto-increment, each read returns the NEXT 64 bytes.
fn cmd_probe_ep0_repeat(addr: u32) -> eframe::Result<()> {
    eprintln!("[probe-ep0-repeat] build: {}", BUILD_STAMP);
    eprintln!("[probe-ep0-repeat] addr=0x{:06X}", addr);

    let session = device::CartSession::open().expect("CartSession::open failed");

    let mut chunks: Vec<[u8; 64]> = Vec::new();
    for i in 0..4 {
        match session.read_rom_chunk_ep0(addr) {
            Ok(chunk) => {
                let same = chunks.last() == Some(&chunk);
                eprintln!(
                    "  read {i}: {:02x} {:02x} {:02x} {:02x}  {:02x} {:02x} {:02x} {:02x}  {}",
                    chunk[0],
                    chunk[1],
                    chunk[2],
                    chunk[3],
                    chunk[4],
                    chunk[5],
                    chunk[6],
                    chunk[7],
                    if i == 0 {
                        "baseline"
                    } else if same {
                        "SAME as prev"
                    } else {
                        "DIFFERENT from prev"
                    }
                );
                chunks.push(chunk);
            }
            Err(e) => eprintln!("  read {i}: ERROR: {e}"),
        }
    }

    let all_same = chunks.windows(2).all(|w| w[0] == w[1]);
    if all_same {
        eprintln!("[probe-ep0-repeat] RESULT: ALL SAME => addressing is repeatable");
    } else {
        eprintln!("[probe-ep0-repeat] RESULT: DIFFERENT => reads are SEQUENTIAL (auto-increment)");
        eprintln!("[probe-ep0-repeat] Use --scan-ep0-from-zero to locate the GBA ROM header");
    }
    Ok(())
}

/// Read 32 sequential 64-byte chunks starting from EP0 byte address 0.
/// Prints the first 8 bytes of each and flags any chunk with GBA magic at [4:8].
fn cmd_scan_ep0_from_zero() -> eframe::Result<()> {
    eprintln!("[scan-ep0-from-zero] build: {}", BUILD_STAMP);
    eprintln!("[scan-ep0-from-zero] scanning 32 chunks x 64 bytes = 2048 bytes from addr 0");

    let session = device::CartSession::open().expect("CartSession::open failed");

    let gba_magic = [0x24u8, 0xFF, 0xAE, 0x51];
    let mut found_offset: Option<u32> = None;

    for i in 0u32..32 {
        let byte_addr = i * 64;
        match session.read_rom_chunk_ep0(byte_addr) {
            Ok(chunk) => {
                let has_magic = chunk[4..8] == gba_magic;
                eprintln!(
                    "  chunk {:3} (addr 0x{:06X}): {:02x} {:02x} {:02x} {:02x}  {:02x} {:02x} {:02x} {:02x}  {}",
                    i,
                    byte_addr,
                    chunk[0],
                    chunk[1],
                    chunk[2],
                    chunk[3],
                    chunk[4],
                    chunk[5],
                    chunk[6],
                    chunk[7],
                    if has_magic {
                        "<-- GBA magic found!"
                    } else {
                        ""
                    }
                );
                if has_magic && found_offset.is_none() {
                    found_offset = Some(byte_addr);
                }
            }
            Err(e) => eprintln!("  chunk {:3} (addr 0x{:06X}): ERROR: {e}", i, byte_addr),
        }
    }

    match found_offset {
        Some(off) => {
            eprintln!(
                "[scan-ep0-from-zero] GBA header found at scan offset 0x{:06X} (chunk {})",
                off,
                off / 64
            );
            eprintln!("[scan-ep0-from-zero] Run: --probe-ep0-header {:X}", off);
        }
        None => {
            eprintln!("[scan-ep0-from-zero] GBA magic NOT found in first 2048 bytes");
            eprintln!(
                "[scan-ep0-from-zero] Try --probe-ep0-repeat to check if reads are sequential"
            );
        }
    }
    Ok(())
}
