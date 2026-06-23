import sys

path = r'C:\Development\ez-flash-ii-usb-flasher\src\ezwriter-cli\src\main.rs'
with open(path, 'r', encoding='utf-8') as f:
    text = f.read()

# Remove the JEDEC reset sequence from cmd_dump
old = (
    '    // Reset flash before reading\n'
    '    let cmd_ep_rst = 0x04;\n'
    '    let seq: [(u8, u16); 4] = [(0xAA, 0xAAAA), (0x55, 0x5554), (0xF0, 0xAAAA), (0xFF, 0)];\n'
    '    for (cb, a) in &seq {\n'
    '        let da = a / 2;\n'
    '        let c = [*cb, (da & 0xFF) as u8, ((da >> 8) & 0xFF) as u8, 0x00];\n'
    '        let _ = handle.write_bulk(cmd_ep_rst, &c, Duration::from_millis(500));\n'
    '        std::thread::sleep(std::time::Duration::from_millis(5));\n'
    '    }\n'
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

new = (
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

if old not in text:
    print('ERROR: old block not found', file=sys.stderr)
    # Show what we have around line 1295
    lines = text.splitlines()
    for i, l in enumerate(lines[1290:1330], 1291):
        print(f'{i}: {l}', file=sys.stderr)
    sys.exit(1)

text = text.replace(old, new, 1)
with open(path, 'w', encoding='utf-8') as f:
    f.write(text)
print('OK')
