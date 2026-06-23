import sys

path = r'C:\Development\ez-flash-ii-usb-flasher\src\ezwriter-cli\src\main.rs'
with open(path, 'r', encoding='utf-8') as f:
    text = f.read()

old = (
    '    // Drain stale EP2 IN data left from the previous command. clear_halt does not\n'
    '    // flush buffered IN data; the firmware queues one response that survives open/close.\n'
    '    // Loop until timeout/error (no data left), capped at 8 iterations for safety.\n'
    '    // NOTE: no JEDEC sequence here — it generates its own EP2 IN responses that\n'
    '    // complicate alignment. The flash is already in array-read mode from prior use.\n'
    '    {\n'
    '        let mut drain = [0u8; 64];\n'
    '        for _ in 0..8 {\n'
    '            match handle.read_bulk(0x82, &mut drain, Duration::from_millis(50)) {\n'
    '                Ok(n) if n > 0 => {}\n'
    '                _ => break,\n'
    '            }\n'
    '        }\n'
    '    }'
)

new = (
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

if old not in text:
    print('ERROR: old block not found', file=sys.stderr)
    sys.exit(1)

text = text.replace(old, new, 1)
with open(path, 'w', encoding='utf-8') as f:
    f.write(text)
print('OK')
