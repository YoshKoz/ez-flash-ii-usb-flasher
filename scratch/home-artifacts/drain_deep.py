text = r"""
use std::time::Duration;
use anyhow::{bail, Result};
use rusb::GlobalContext;

const EZWRITER_VID: u16 = 0x0548;
const EZWRITER_PID: u16 = 0x1005;
const DATA_EP: u8 = 0x82;

fn main() -> Result<()> {
    let mut found_handle = None;
    for device in rusb::devices()?.iter() {
        let desc = device.device_descriptor()?;
        if desc.vendor_id() == EZWRITER_VID && desc.product_id() == EZWRITER_PID {
            let handle = device.open()?;
            let config = device.active_config_descriptor()?;
            for iface in config.interfaces() {
                for d in iface.descriptors() { let _ = handle.claim_interface(d.interface_number()); }
            }
            for ep in 0x01u8..=0x07 { let _ = handle.clear_halt(ep); let _ = handle.clear_halt(ep | 0x80); }
            found_handle = Some(handle);
            break;
        }
    }
    let handle = found_handle.ok_or_else(|| anyhow::anyhow!("device not found"))?;
    println!("Device open. Deep-draining EP2 IN with 30s timeout per packet...");
    let mut total = 0usize;
    loop {
        let mut b = [0u8; 64];
        match handle.read_bulk(DATA_EP, &mut b, Duration::from_secs(30)) {
            Ok(n) if n > 0 => {
                total += 1;
                let hx: String = b[..16].iter().map(|x| format!("{x:02x}")).collect::<Vec<_>>().join(" ");
                println!("  pkt[{total}]: {hx}");
            }
            Ok(_) => { println!("  Zero-length packet. Done? total={total}"); break; }
            Err(e) => { println!("  30s timeout or error ({e}). Firmware idle. total={total}"); break; }
        }
        if total >= 64 {
            println!("  Stopped at 64 packets.");
            break;
        }
    }
    Ok(())
}
"""
with open(r'C:\Users\yoshi\diag_save\src\main.rs', 'w') as f:
    f.write(text)
print("OK")
