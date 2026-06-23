import sys

path = r'C:\Development\ez-flash-ii-usb-flasher\src\ezwriter-cli\src\main.rs'
with open(path, 'r', encoding='utf-8') as f:
    text = f.read()

old = (
    '    {\n'
    '        let warmup = [0x01u8, 0x00, 0x00, 0x00];\n'
    '        let _ = handle.write_bulk(0x04, &warmup, TIMEOUT);\n'
    '        std::thread::sleep(Duration::from_millis(150));\n'
    '        let mut phantom = [0u8; 64];\n'
    '        match handle.read_bulk(0x82, &mut phantom, Duration::from_millis(500)) {\n'
    '            Ok(n) => eprintln!("  [DIAG] warmup: read {n} bytes (first4={:02X?})", &phantom[..4]),\n'
    '            Err(e) => eprintln!("  [DIAG] warmup: err={e}"),\n'
    '        }\n'
    '    }'
)

new = (
    '    {\n'
    '        let warmup = [0x01u8, 0x00, 0x00, 0x00];\n'
    '        let _ = handle.write_bulk(0x04, &warmup, TIMEOUT);\n'
    '        std::thread::sleep(Duration::from_millis(150));\n'
    '        // Each cmd generates 2 EP2 IN responses (phantom + data). Consume both.\n'
    '        for i in 0..2 {\n'
    '            let mut phantom = [0u8; 64];\n'
    '            match handle.read_bulk(0x82, &mut phantom, Duration::from_millis(500)) {\n'
    '                Ok(n) => eprintln!("  [DIAG] warmup[{i}]: read {n} bytes (first4={:02X?})", &phantom[..4]),\n'
    '                Err(e) => eprintln!("  [DIAG] warmup[{i}]: err={e}"),\n'
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
