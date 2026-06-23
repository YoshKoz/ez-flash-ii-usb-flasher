text = r"""
use std::time::Duration;
use anyhow::{bail, Result};
use rusb::{GlobalContext, DeviceHandle};

const EZWRITER_VID: u16 = 0x0548;
const EZWRITER_PID: u16 = 0x1005;
const CMD_EP: u8 = 0x04;
const DATA_EP: u8 = 0x82;

fn find_device() -> Result<DeviceHandle<GlobalContext>> {
    for device in rusb::devices()?.iter() {
        let desc = device.device_descriptor()?;
        if desc.vendor_id() == EZWRITER_VID && desc.product_id() == EZWRITER_PID {
            let handle = device.open()?;
            let config = device.active_config_descriptor()?;
            for iface in config.interfaces() {
                for d in iface.descriptors() { let _ = handle.claim_interface(d.interface_number()); }
            }
            for ep in 0x01u8..=0x07 { let _ = handle.clear_halt(ep); let _ = handle.clear_halt(ep | 0x80); }
            return Ok(handle);
        }
    }
    bail!("device not found");
}

fn drain(handle: &DeviceHandle<GlobalContext>, max: usize, ms: u64) -> usize {
    let mut n = 0;
    for i in 0..max {
        let mut b = [0u8; 64];
        match handle.read_bulk(DATA_EP, &mut b, Duration::from_millis(ms)) {
            Ok(sz) if sz > 0 => { eprintln!("  drain[{i}] {}", h(&b[..16])); n += 1; }
            _ => { eprintln!("  drain[{i}] stop after {i} packets"); break; }
        }
    }
    n
}

fn h(b: &[u8]) -> String { b.iter().map(|x| format!("{x:02x}")).collect::<Vec<_>>().join(" ") }

fn w(handle: &DeviceHandle<GlobalContext>, buf: &[u8]) -> bool {
    match handle.write_bulk(CMD_EP, buf, Duration::from_secs(5)) {
        Ok(_) => true,
        Err(e) => { eprintln!("  WRITE_ERR {}: {e}", h(buf)); false }
    }
}

fn r_with_timeout(handle: &DeviceHandle<GlobalContext>, ms: u64, label: &str) -> Option<Vec<u8>> {
    let mut b = [0u8; 64];
    match handle.read_bulk(DATA_EP, &mut b, Duration::from_millis(ms)) {
        Ok(n) => { println!("  [{label}] GOT {n}B: {}", h(&b[..16])); Some(b[..n].to_vec()) }
        Err(_) => { println!("  [{label}] TIMEOUT ({ms}ms)"); None }
    }
}

fn main() -> Result<()> {
    let handle = find_device()?;
    println!("Device open. Starting diagnostics.");
    std::thread::sleep(Duration::from_millis(500));  // firmware settle

    // ---- Restore ROM auto-stream with a couple of cmd 0x01 reads ----
    println!("=== Restore ROM auto-stream ===");
    drain(&handle, 8, 200);
    if !w(&handle, &[0x01, 0x00, 0x00, 0x00]) { return Ok(()); }
    std::thread::sleep(Duration::from_millis(200));
    drain(&handle, 4, 200);
    let _ = r_with_timeout(&handle, 500, "ROM[0]");

    // ---- Test: cmd 0x14 then cmd 0x02 with LONG timeout (5s) ----
    println!("=== cmd 0x14 + cmd 0x02 with 5s timeout ===");
    drain(&handle, 12, 200);
    w(&handle, &[0x14, 0x66, 0x00]);
    std::thread::sleep(Duration::from_millis(300));
    drain(&handle, 8, 200);
    for chunk in 0..3usize {
        let addr = (chunk as u32) * 64;
        let lo = (addr & 0xFF) as u8;
        let mid = ((addr >> 8) & 0xFF) as u8;
        if !w(&handle, &[0x02u8, lo, mid, 0x00u8, 0x66u8]) { break; }
        let _ = r_with_timeout(&handle, 5000, &format!("0x02+5s chunk{chunk}"));
    }

    // ---- Probe other cmd bytes (0x03..0x15) with cmd 0x14 active ----
    println!("=== Probe cmd bytes 0x03..0x15 for save data ===");
    drain(&handle, 12, 200);
    w(&handle, &[0x14, 0x66, 0x00]);
    std::thread::sleep(Duration::from_millis(200));
    drain(&handle, 8, 200);
    for cmd_byte in [0x03u8, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x15] {
        // Try simple [cmd, 0x00, 0x00, 0x66]
        if !w(&handle, &[cmd_byte, 0x00, 0x00, 0x66]) { break; }
        match r_with_timeout(&handle, 1000, &format!("cmd=0x{cmd_byte:02x}")) {
            Some(data) => {
                // Check if data differs from ROM pattern
                if data[..4] != [0x32, 0x00, 0x00, 0xea] && data[..4] != [0xfc, 0x7f, 0x00, 0x03] {
                    println!("  *** DIFFERENT DATA from cmd 0x{cmd_byte:02x}: {}", h(&data));
                }
            }
            None => {}
        }
    }

    // ---- cmd 0x01 at save chip addresses (byte_addr/2 word addressing) ----
    println!("=== cmd 0x01 at save chip word addresses ===");
    drain(&handle, 12, 200);
    // GBA SRAM/FLASH at 0x0E000000 byte → 0x07000000 word → bank=0x70, addr=0x0000
    for (bank, label) in [(0x70u8, "0x0E000000/2"), (0x07u8, "bank7"), (0x08u8, "bank8"), (0x0Eu8, "bankE")] {
        if !w(&handle, &[0x01u8, 0x00, 0x00, bank]) { break; }
        std::thread::sleep(Duration::from_millis(150));
        let _ = r_with_timeout(&handle, 2000, &format!("ROM_at_bank={bank:#04x}_{label}"));
    }

    Ok(())
}
"""
with open(r'C:\Users\yoshi\diag_save\src\main.rs', 'w') as f:
    f.write(text)
print("OK")
