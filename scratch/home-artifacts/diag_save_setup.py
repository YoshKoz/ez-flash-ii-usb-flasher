# Diagnostic save-read: try different drain amounts and no-select variants
# to figure out what the EZ-Flash II save protocol actually needs

text = r"""
use std::{time::Duration, path::PathBuf};
use anyhow::{bail, Result};
use rusb::{GlobalContext, DeviceHandle};

const EZWRITER_VID: u16 = 0x0548;
const EZWRITER_PID: u16 = 0x1005;
const TIMEOUT: Duration = Duration::from_secs(5);
const CMD_EP: u8 = 0x04;
const DATA_EP: u8 = 0x82;

fn find_device() -> Result<DeviceHandle<GlobalContext>> {
    for device in rusb::devices()?.iter() {
        let desc = device.device_descriptor()?;
        if desc.vendor_id() == EZWRITER_VID && desc.product_id() == EZWRITER_PID {
            let handle = device.open()?;
            let config = device.active_config_descriptor()?;
            for iface in config.interfaces() {
                for d in iface.descriptors() {
                    let _ = handle.claim_interface(d.interface_number());
                }
            }
            for ep in 0x01u8..=0x07 {
                let _ = handle.clear_halt(ep);
                let _ = handle.clear_halt(ep | 0x80);
            }
            return Ok(handle);
        }
    }
    bail!("device not found");
}

fn drain_all(handle: &DeviceHandle<GlobalContext>, n: usize, label: &str) {
    for i in 0..n {
        let mut buf = [0u8; 64];
        match handle.read_bulk(DATA_EP, &mut buf, Duration::from_millis(200)) {
            Ok(len) if len > 0 => eprintln!("  drain[{label}][{i}]: {len}b {}", hex(&buf[..4])),
            Ok(_)              => { eprintln!("  drain[{label}][{i}]: zero/timeout -> stop"); break; }
            Err(e)             => { eprintln!("  drain[{label}][{i}]: err={e} -> stop"); break; }
        }
    }
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect::<Vec<_>>().join(" ")
}

fn write_ep4(handle: &DeviceHandle<GlobalContext>, buf: &[u8]) {
    let r = handle.write_bulk(CMD_EP, buf, TIMEOUT);
    eprintln!("  write_ep4 {:?}: {:?}", hex(buf), r);
}

fn read_ep2(handle: &DeviceHandle<GlobalContext>, label: &str) -> Vec<u8> {
    let mut buf = [0u8; 64];
    match handle.read_bulk(DATA_EP, &mut buf, Duration::from_secs(2)) {
        Ok(n) => { eprintln!("  read_ep2[{label}]: {n}b {}", hex(&buf[..n.min(16)])); buf[..n].to_vec() }
        Err(e) => { eprintln!("  read_ep2[{label}]: err={e}"); vec![] }
    }
}

fn main() -> Result<()> {
    let handle = find_device()?;
    eprintln!("Device open.");

    // ---- Phase 1: Big drain to clear any ROM auto-stream ----
    eprintln!("=== Phase 1: drain 16 packets ===");
    drain_all(&handle, 16, "pre");

    // ---- Phase 2: CPLD unlock (cmd 0x19) ----
    eprintln!("=== Phase 2: CPLD unlock ===");
    // write_reg(0x9FE000, 0xD200) -> [0x19, 0x00, 0xE0, 0x9F, 0x00, 0xD2]
    write_ep4(&handle, &[0x19, 0x00, 0xE0, 0x9F, 0x00, 0xD2]);
    write_ep4(&handle, &[0x19, 0x00, 0x00, 0x80, 0x00, 0x15]);
    write_ep4(&handle, &[0x19, 0x00, 0x20, 0x80, 0x00, 0xD2]);
    write_ep4(&handle, &[0x19, 0x00, 0x40, 0x80, 0x00, 0x15]);
    std::thread::sleep(Duration::from_millis(200));
    // Check if any EP2 responses came from write_reg
    eprintln!("=== After unlock: drain to see if 0x19 generates EP2 data ===");
    drain_all(&handle, 8, "post-unlock");

    // ---- Phase 3: cmd 0x14 select FLASH ----
    eprintln!("=== Phase 3: cmd 0x14 select type=0x66 ===");
    write_ep4(&handle, &[0x14, 0x66, 0x00]);
    std::thread::sleep(Duration::from_millis(200));
    eprintln!("=== After 0x14: drain ===");
    drain_all(&handle, 4, "post-select");

    // ---- Phase 4: Read 8 save chunks at addr 0x000 ====
    eprintln!("=== Phase 4: read 8 chunks via cmd 0x02 ===");
    for chunk in 0..8usize {
        let addr = (chunk * 64) as u32;
        let cmd = [0x02u8, (addr & 0xFF) as u8, ((addr >> 8) & 0xFF) as u8, ((addr >> 16) & 0xFF) as u8, 0x66];
        write_ep4(&handle, &cmd);
        std::thread::sleep(Duration::from_millis(50));
        let _ = read_ep2(&handle, &format!("chunk{chunk}"));
    }

    // ---- Phase 5: ROM read (cmd 0x01 addr=0) to see if auto-stream kicked back in ----
    eprintln!("=== Phase 5: ROM read cmd 0x01 addr=0 ===");
    write_ep4(&handle, &[0x01, 0x00, 0x00, 0x00]);
    std::thread::sleep(Duration::from_millis(200));
    drain_all(&handle, 3, "rom-read");

    Ok(())
}
"""

with open(r'C:\Users\yoshi\diag_save\src\main.rs', 'w') as f:
    f.write(text)

toml = r"""
[package]
name = "diag_save"
version = "0.1.0"
edition = "2021"

[dependencies]
anyhow = "1"
rusb = "0.9"
"""
with open(r'C:\Users\yoshi\diag_save\Cargo.toml', 'w') as f:
    f.write(toml)

print("Written")
