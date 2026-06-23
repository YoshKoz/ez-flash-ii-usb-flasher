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
            for ep in 0x01u8..=0x07 { let _ = handle.clear_halt(ep); let _ = handle.clear_halt(ep | 0x80); }
            return Ok(handle);
        }
    }
    bail!("device not found");
}

fn drain(handle: &DeviceHandle<GlobalContext>, max: usize) {
    for i in 0..max {
        let mut b = [0u8; 64];
        match handle.read_bulk(DATA_EP, &mut b, Duration::from_millis(300)) {
            Ok(n) if n > 0 => eprintln!("  drain[{i}] {}", h(&b[..16])),
            _ => { eprintln!("  drain[{i}] stop"); break; }
        }
    }
}

fn h(b: &[u8]) -> String { b.iter().map(|x| format!("{x:02x}")).collect::<Vec<_>>().join(" ") }

fn w(handle: &DeviceHandle<GlobalContext>, buf: &[u8]) {
    let r = handle.write_bulk(CMD_EP, buf, TIMEOUT);
    eprintln!("  w[{}] -> {:?}", h(buf), r);
}

fn r(handle: &DeviceHandle<GlobalContext>, label: &str) -> bool {
    let mut b = [0u8; 64];
    match handle.read_bulk(DATA_EP, &mut b, Duration::from_secs(2)) {
        Ok(n) => { println!("  [{label}] {}", h(&b[..16])); true }
        Err(e) => { println!("  [{label}] TIMEOUT: {e}"); false }
    }
}

fn main() -> Result<()> {
    let handle = find_device()?;

    // ---- Test A: no CPLD unlock, cmd 0x14 0x66, then cmd 0x02+0x03 protocol ----
    println!("=== TEST A: cmd 0x14(0x66) then 0x02(page)+0x03(read) ===");
    eprintln!("--- drain ---"); drain(&handle, 12);
    w(&handle, &[0x14, 0x66, 0x00]);
    std::thread::sleep(Duration::from_millis(150));
    eprintln!("--- drain after 0x14 ---"); drain(&handle, 4);
    // Try 0x02 page-setup + 0x03 read
    for chunk in 0..4usize {
        let addr = (chunk as u32) * 64;
        let lo = (addr & 0xFF) as u8;
        let mid = ((addr >> 8) & 0xFF) as u8;
        let bank: u8 = 0;
        // page setup
        w(&handle, &[0x02, lo, mid, 0x66, bank, 0x00]);
        // read trigger
        w(&handle, &[0x03, lo, 0x00, 0x00]);
        std::thread::sleep(Duration::from_millis(100));
        let got = r(&handle, &format!("A chunk{chunk}"));
        if !got { break; }
    }

    // ---- Test B: 0x14 with 0x65 (EEPROM handler), try 0x02+0x03 ----
    println!("=== TEST B: cmd 0x14(0x65) then 0x02+0x03 ===");
    eprintln!("--- drain ---"); drain(&handle, 12);
    w(&handle, &[0x14, 0x65, 0x00]);
    std::thread::sleep(Duration::from_millis(150));
    eprintln!("--- drain ---"); drain(&handle, 4);
    for chunk in 0..2usize {
        let addr = (chunk as u32) * 64;
        let lo = (addr & 0xFF) as u8;
        let mid = ((addr >> 8) & 0xFF) as u8;
        w(&handle, &[0x02, lo, mid, 0x65, 0x00, 0x00]);
        w(&handle, &[0x03, lo, 0x00, 0x00]);
        std::thread::sleep(Duration::from_millis(100));
        let got = r(&handle, &format!("B chunk{chunk}"));
        if !got { break; }
    }

    // ---- Test C: 0x14 with 0x66, then just 0x02 (4-byte, no inner cmd) ----
    println!("=== TEST C: cmd 0x14(0x66) then [0x02, lo, mid, bank] 4-byte ===");
    eprintln!("--- drain ---"); drain(&handle, 12);
    w(&handle, &[0x14, 0x66, 0x00]);
    std::thread::sleep(Duration::from_millis(150));
    eprintln!("--- drain ---"); drain(&handle, 4);
    for chunk in 0..4usize {
        let addr = (chunk as u32) * 64;
        let lo = (addr & 0xFF) as u8;
        let mid = ((addr >> 8) & 0xFF) as u8;
        w(&handle, &[0x02, lo, mid, 0x00]);  // 4 bytes
        std::thread::sleep(Duration::from_millis(100));
        let got = r(&handle, &format!("C chunk{chunk}"));
        if !got { break; }
    }

    // ---- Test D: cmd 0x03 alone (no prior 0x02), different inner bytes ----
    println!("=== TEST D: cmd 0x03 alone with different bytes ===");
    eprintln!("--- drain ---"); drain(&handle, 12);
    w(&handle, &[0x14, 0x66, 0x00]);
    std::thread::sleep(Duration::from_millis(150));
    eprintln!("--- drain ---"); drain(&handle, 4);
    for inner in [0x66u8, 0x65u8, 0x68u8, 0x00u8] {
        w(&handle, &[0x03, 0x00, 0x00, inner]);
        std::thread::sleep(Duration::from_millis(100));
        let got = r(&handle, &format!("D inner=0x{inner:02x}"));
        if !got { break; }
    }

    Ok(())
}
"""
with open(r'C:\Users\yoshi\diag_save\src\main.rs', 'w') as f:
    f.write(text)
print("OK")
