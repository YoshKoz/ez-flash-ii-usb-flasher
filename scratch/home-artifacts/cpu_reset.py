# Cypress EZ-USB CPU reset: write 1 to CPUCS (0xE600) then 0 to release
# This restarts the 8051 from EEPROM (re-enumerates with factory firmware)
text = r"""
use std::time::Duration;
use anyhow::{bail, Result};
use rusb::GlobalContext;

const EZWRITER_VID: u16 = 0x0548;
const EZWRITER_PID: u16 = 0x1005;
const BOOTLOADER_VID: u16 = 0x0547;
const BOOTLOADER_PID: u16 = 0x2131;

fn ezusb_write_ram(handle: &rusb::DeviceHandle<GlobalContext>, addr: u32, data: &[u8]) -> Result<()> {
    let timeout = Duration::from_secs(5);
    let n = handle.write_control(
        0x40,        // bmRequestType: vendor, device, host->device
        0xA0,        // bRequest: EZ-USB firmware load
        addr as u16, // wValue: address low
        (addr >> 16) as u16, // wIndex: address high
        data,
        timeout,
    )?;
    if n != data.len() { bail!("Short write: {n} != {}", data.len()); }
    Ok(())
}

fn main() -> Result<()> {
    // Try to find device in active mode
    let found = rusb::devices()?.iter().find(|d| {
        let desc = d.device_descriptor().unwrap();
        (desc.vendor_id() == EZWRITER_VID && desc.product_id() == EZWRITER_PID) ||
        (desc.vendor_id() == BOOTLOADER_VID && desc.product_id() == BOOTLOADER_PID)
    });

    let device = found.ok_or_else(|| anyhow::anyhow!("no device found"))?;
    let desc = device.device_descriptor()?;
    println!("Found device: {:04x}:{:04x}", desc.vendor_id(), desc.product_id());

    let handle = device.open()?;

    // EZ-USB reset: put CPU in reset
    println!("Putting 8051 CPU in reset (write 1 to CPUCS 0xE600)...");
    match ezusb_write_ram(&handle, 0xE600, &[0x01]) {
        Ok(()) => println!("  CPU reset OK"),
        Err(e) => println!("  CPU reset FAIL: {e}"),
    }

    std::thread::sleep(Duration::from_millis(100));

    // EZ-USB restart: release CPU from reset (reads firmware from EEPROM)
    println!("Releasing 8051 CPU from reset (write 0 to CPUCS 0xE600)...");
    match ezusb_write_ram(&handle, 0xE600, &[0x00]) {
        Ok(()) => println!("  CPU release OK"),
        Err(e) => println!("  CPU release FAIL: {e}"),
    }

    println!("CPU restart sent. Device will re-enumerate. Wait 5s...");
    drop(handle);
    std::thread::sleep(Duration::from_secs(5));

    // Check what device shows up
    println!("Checking devices after restart:");
    for dev in rusb::devices()?.iter() {
        let d = dev.device_descriptor()?;
        if d.vendor_id() == EZWRITER_VID || d.vendor_id() == BOOTLOADER_VID {
            println!("  Found: {:04x}:{:04x}", d.vendor_id(), d.product_id());
        }
    }

    Ok(())
}
"""
with open(r'C:\Users\yoshi\diag_save\src\main.rs', 'w') as f:
    f.write(text)
print("OK")
