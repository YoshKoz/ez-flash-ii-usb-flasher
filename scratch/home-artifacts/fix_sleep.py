import sys

path = r'C:\Development\ez-flash-ii-usb-flasher\src\ezwriter-cli\src\main.rs'
with open(path, 'r', encoding='utf-8') as f:
    text = f.read()

old = (
    '            // 5 ms delay for firmware to process and fill EP2 IN\n'
    '            std::thread::sleep(Duration::from_millis(5));\n'
    '\n'
    '            let mut buf = [0u8; 64];\n'
    '            match handle.read_bulk(data_ep, &mut buf, Duration::from_secs(3)) {\n'
    '                Ok(len) => {\n'
    '                    eprintln!("  [DIAG] chunk {chunk} addr=0x{byte_addr:06X}: read {len} bytes (first4={:02X?})", &buf[..4]);\n'
    '                    file.write_all(&buf[..len])?;\n'
    '                }'
)

new = (
    '            // 100 ms: enough for firmware to process cmd and fill EP2 IN\n'
    '            std::thread::sleep(Duration::from_millis(100));\n'
    '\n'
    '            let mut buf = [0u8; 64];\n'
    '            match handle.read_bulk(data_ep, &mut buf, Duration::from_secs(3)) {\n'
    '                Ok(len) => {\n'
    '                    eprintln!("  [DIAG] chunk {chunk} addr=0x{byte_addr:06X}: read {len} bytes (first4={:02X?})", &buf[..4]);\n'
    '                    file.write_all(&buf[..len])?;\n'
    '                }'
)

if old not in text:
    print('ERROR: old block not found', file=sys.stderr)
    sys.exit(1)

text = text.replace(old, new, 1)
with open(path, 'w', encoding='utf-8') as f:
    f.write(text)
print('OK')
