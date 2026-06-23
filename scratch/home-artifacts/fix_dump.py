import sys

path = r'C:\Development\ez-flash-ii-usb-flasher\src\ezwriter-cli\src\main.rs'
with open(path, 'r', encoding='utf-8') as f:
    text = f.read()

old = (
    '    // Prime the pipeline: the firmware holds one stale chunk in EP2 IN from the\n'
    '    // previous session. Send a dummy read command and discard its response so\n'
    '    // the real loop starts aligned to address 0.\n'
    '    {\n'
    '        let prime = [0x01u8, 0x00, 0x00, 0x00];\n'
    '        let _ = handle.write_bulk(0x04, &prime, TIMEOUT);\n'
    '        std::thread::sleep(Duration::from_millis(20));\n'
    '        let mut discard = [0u8; 64];\n'
    '        let _ = handle.read_bulk(0x82, &mut discard, Duration::from_millis(500));\n'
    '    }'
)

new = (
    '    // Drain stale EP2 IN data left from the previous command. clear_halt does not\n'
    '    // flush buffered IN data; the firmware queues one response that survives open/close.\n'
    '    // Loop until timeout/error (no data left), capped at 8 iterations for safety.\n'
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

if old not in text:
    print('ERROR: old block not found', file=sys.stderr)
    sys.exit(1)

text = text.replace(old, new, 1)
with open(path, 'w', encoding='utf-8') as f:
    f.write(text)
print('OK')
