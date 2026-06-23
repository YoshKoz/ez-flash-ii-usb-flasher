import sys

path = r'C:\Development\ez-flash-ii-usb-flasher\src\ezwriter-cli\src\main.rs'
with open(path, 'r', encoding='utf-8') as f:
    text = f.read()

old = (
    '    // The EZ-Flash II firmware sends a fixed 64-byte "init" packet on interface claim\n'
    '    // before any commands are processed. Wait 300 ms for it to arrive, then drain\n'
    '    // it (and any prior stale response) so the real loop starts aligned at addr 0.\n'
    '    std::thread::sleep(Duration::from_millis(300));\n'
    '    {\n'
    '        let mut drain = [0u8; 64];\n'
    '        for _ in 0..8 {\n'
    '            match handle.read_bulk(0x82, &mut drain, Duration::from_millis(100)) {\n'
    '                Ok(n) if n > 0 => {}\n'
    '                _ => break,\n'
    '            }\n'
    '        }\n'
    '    }'
)

new = (
    '    // Step 1: drain any stale EP2 IN response left by the previous command session.\n'
    '    // clear_halt resets the toggle but does not flush buffered IN data.\n'
    '    {\n'
    '        let mut drain = [0u8; 64];\n'
    '        for _ in 0..8 {\n'
    '            match handle.read_bulk(0x82, &mut drain, Duration::from_millis(100)) {\n'
    '                Ok(n) if n > 0 => {}\n'
    '                _ => break,\n'
    '            }\n'
    '        }\n'
    '    }\n'
    '    // Step 2: the firmware ALWAYS sends a fixed 64-byte phantom/sync packet as its\n'
    '    // very first response after inactivity — regardless of address. Send one warmup\n'
    '    // read command and discard the phantom so the real loop receives aligned ROM data.\n'
    '    {\n'
    '        let warmup = [0x01u8, 0x00, 0x00, 0x00];\n'
    '        let _ = handle.write_bulk(0x04, &warmup, TIMEOUT);\n'
    '        let mut phantom = [0u8; 64];\n'
    '        let _ = handle.read_bulk(0x82, &mut phantom, Duration::from_millis(500));\n'
    '    }'
)

if old not in text:
    print('ERROR: old block not found', file=sys.stderr)
    sys.exit(1)

text = text.replace(old, new, 1)
with open(path, 'w', encoding='utf-8') as f:
    f.write(text)
print('OK')
