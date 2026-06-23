text = r"""
use std::time::Duration;
use anyhow::{bail, Result};
use rusb::GlobalContext;

const EZWRITER_VID: u16 = 0x0548;
const EZWRITER_PID: u16 = 0x1005;
const CMD_EP: u8 = 0x04;
const DATA_EP: u8 = 0x82;

fn h(b: &[u8]) -> String { b.iter().map(|x| format!("{x:02x}")).collect::<Vec<_>>().join(" ") }

fn main() -> Result<()> {
    // Step 1: Find device and reset it
    println!("Step 1: Finding device...");
    for device in rusb::devices()?.iter() {
        let desc = device.device_descriptor()?;
        if desc.vendor_id() == EZWRITER_VID && desc.product_id() == EZWRITER_PID {
            println!("  Found. Resetting...");
            let handle = device.open()?;
            let _ = handle.reset();
            println!("  Reset sent. Waiting 3s for re-enumeration...");
            drop(handle);
            break;
        }
    }

    std::thread::sleep(Duration::from_secs(3));

    // Step 2: Find device again after reset
    println!("Step 2: Re-opening device after reset...");
    let mut found = false;
    for device in rusb::devices()?.iter() {
        let desc = device.device_descriptor()?;
        if desc.vendor_id() == EZWRITER_VID && desc.product_id() == EZWRITER_PID {
            let handle = device.open()?;
            let config = device.active_config_descriptor()?;
            for iface in config.interfaces() {
                for d in iface.descriptors() { let _ = handle.claim_interface(d.interface_number()); }
            }
            for ep in 0x01u8..=0x07 { let _ = handle.clear_halt(ep); let _ = handle.clear_halt(ep | 0x80); }
            println!("  Reopened OK. Waiting 1s for firmware init...");
            std::thread::sleep(Duration::from_secs(1));

            // Try a simple ROM read
            println!("Step 3: ROM read test [0x01 0x00 0x00 0x00]...");
            match handle.write_bulk(CMD_EP, &[0x01u8, 0x00, 0x00, 0x00], Duration::from_secs(10)) {
                Ok(n) => println!("  write_bulk OK ({n} bytes)"),
                Err(e) => { println!("  write_bulk FAIL: {e}"); return Ok(()); }
            }
            std::thread::sleep(Duration::from_millis(200));
            let mut buf = [0u8; 64];
            match handle.read_bulk(DATA_EP, &mut buf, Duration::from_secs(3)) {
                Ok(n) => println!("  read_bulk OK: {}", h(&buf[..n.min(16)])),
                Err(e) => println!("  read_bulk FAIL: {e}"),
            }

            // Step 4: cmd 0x14 then cmd 0x02 with different format - try ALL variants in sequence
            // drain first
            println!("Step 4: drain + cmd 0x14 + multiple cmd 0x02 variants");
            for _ in 0..12 {
                let mut b = [0u8; 64];
                if handle.read_bulk(DATA_EP, &mut b, Duration::from_millis(200)).is_err() { break; }
            }
            handle.write_bulk(CMD_EP, &[0x14, 0x66, 0x00], Duration::from_secs(5))?;
            std::thread::sleep(Duration::from_millis(300));
            for _ in 0..8 {
                let mut b = [0u8; 64];
                if handle.read_bulk(DATA_EP, &mut b, Duration::from_millis(200)).is_err() { break; }
            }

            // Try: [0x02, 0x00, 0x00, 0x00, 0x66] and listen for 5s
            println!("  trying [0x02, 0x00, 0x00, 0x00, 0x66] (5-byte, suffix last)");
            handle.write_bulk(CMD_EP, &[0x02u8, 0x00, 0x00, 0x00, 0x66], Duration::from_secs(5))?;
            let mut buf = [0u8; 64];
            match handle.read_bulk(DATA_EP, &mut buf, Duration::from_secs(5)) {
                Ok(n) => println!("  GOT: {}", h(&buf[..n.min(16)])),
                Err(e) => println!("  TIMEOUT: {e}"),
            }

            // Try: [0x02, 0x00, 0x00, 0x66, 0x00, 0x00] (6-byte, suffix at byte3)
            println!("  trying [0x02, 0x00, 0x00, 0x66, 0x00, 0x00] (6-byte, suffix=byte3)");
            handle.write_bulk(CMD_EP, &[0x02u8, 0x00, 0x00, 0x66, 0x00, 0x00], Duration::from_secs(5))?;
            match handle.read_bulk(DATA_EP, &mut buf, Duration::from_secs(5)) {
                Ok(n) => println!("  GOT: {}", h(&buf[..n.min(16)])),
                Err(e) => println!("  TIMEOUT: {e}"),
            }

            // Try: [0x0B, 0x00, 0x00, 0x66, 0x00, 0x00] (cmd 0x0B)
            println!("  trying cmd 0x0B (save read alt)");
            handle.write_bulk(CMD_EP, &[0x0Bu8, 0x00, 0x00, 0x66, 0x00, 0x00], Duration::from_secs(5))?;
            match handle.read_bulk(DATA_EP, &mut buf, Duration::from_secs(5)) {
                Ok(n) => println!("  GOT: {}", h(&buf[..n.min(16)])),
                Err(e) => println!("  TIMEOUT: {e}"),
            }

            found = true;
            break;
        }
    }
    if !found { bail!("device not found after reset"); }
    Ok(())
}
"""
with open(r'C:\Users\yoshi\diag_save\src\main.rs', 'w') as f:
    f.write(text)
print("OK")
