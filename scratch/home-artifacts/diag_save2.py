text = r"""
use std::time::Duration;
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
                for d in iface.descriptors() { let _ = handle.claim_interface(d.interface_number()); }
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

fn drain_all(handle: &DeviceHandle<GlobalContext>, n: usize) {
    for i in 0..n {
        let mut b = [0u8; 64];
        match handle.read_bulk(DATA_EP, &mut b, Duration::from_millis(200)) {
            Ok(n) if n > 0 => eprintln!("  drain[{i}]: {}", hex16(&b)),
            _ => { eprintln!("  drain[{i}]: stop"); break; }
        }
    }
}

fn hex16(b: &[u8]) -> String {
    b[..16.min(b.len())].iter().map(|x| format!("{x:02x}")).collect::<Vec<_>>().join(" ")
}

fn write4(handle: &DeviceHandle<GlobalContext>, buf: &[u8]) {
    let _ = handle.write_bulk(CMD_EP, buf, TIMEOUT);
}

fn read2(handle: &DeviceHandle<GlobalContext>, label: &str) -> Vec<u8> {
    let mut b = [0u8; 64];
    match handle.read_bulk(DATA_EP, &mut b, Duration::from_secs(3)) {
        Ok(n) => { println!("  [{label}]: {}", hex16(&b)); b[..n].to_vec() }
        Err(e) => { println!("  [{label}]: err={e}"); vec![] }
    }
}

fn main() -> Result<()> {
    let handle = find_device()?;

    // Drain all stale auto-stream
    eprintln!("=== drain ===");
    drain_all(&handle, 16);

    // CPLD unlock
    write4(&handle, &[0x19, 0x00, 0xE0, 0x9F, 0x00, 0xD2]);
    write4(&handle, &[0x19, 0x00, 0x00, 0x80, 0x00, 0x15]);
    write4(&handle, &[0x19, 0x00, 0x20, 0x80, 0x00, 0xD2]);
    write4(&handle, &[0x19, 0x00, 0x40, 0x80, 0x00, 0x15]);
    std::thread::sleep(Duration::from_millis(200));

    // cmd 0x14 select FLASH
    write4(&handle, &[0x14, 0x66, 0x00]);
    std::thread::sleep(Duration::from_millis(200));

    // Drain after select (it might put data in EP2)
    eprintln!("=== drain after select ===");
    drain_all(&handle, 8);

    // Test the CORRECT cmd format: [0x02, addr_lo, addr_mid, 0x66, bank, 0x00]
    // (0x66 at byte[3], bank at byte[4])
    println!("=== cmd 0x02 CORRECT format (0x66 at byte3, bank at byte4) ===");
    for chunk in 0..8usize {
        let addr = (chunk as u32) * 64;
        let addr_lo = (addr & 0xFF) as u8;
        let addr_mid = ((addr >> 8) & 0xFF) as u8;
        let bank: u8 = 0;
        let cmd = [0x02u8, addr_lo, addr_mid, 0x66, bank, 0x00];
        write4(&handle, &cmd);
        std::thread::sleep(Duration::from_millis(50));
        let data = read2(&handle, &format!("chunk{chunk} addr={addr:#06x}"));
        let _ = data;
    }

    // Also test without the 0x00 trailing byte (5-byte variant)
    eprintln!("=== drain to reset ===");
    drain_all(&handle, 8);
    // Re-select
    write4(&handle, &[0x14, 0x66, 0x00]);
    std::thread::sleep(Duration::from_millis(200));
    drain_all(&handle, 4);

    println!("=== cmd 0x02 OLD format (0x66 at byte4, 5 bytes) for comparison ===");
    for chunk in 0..4usize {
        let addr = (chunk as u32) * 64;
        let addr_lo = (addr & 0xFF) as u8;
        let addr_mid = ((addr >> 8) & 0xFF) as u8;
        let addr_hi = ((addr >> 16) & 0xFF) as u8;
        let cmd = [0x02u8, addr_lo, addr_mid, addr_hi, 0x66];
        write4(&handle, &cmd);
        std::thread::sleep(Duration::from_millis(50));
        let data = read2(&handle, &format!("chunk{chunk} OLD"));
        let _ = data;
    }

    Ok(())
}
"""
with open(r'C:\Users\yoshi\diag_save\src\main.rs', 'w') as f:
    f.write(text)
print("OK")
