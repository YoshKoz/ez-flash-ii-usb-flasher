import sys

path = r'C:\Development\ez-flash-ii-usb-flasher\src\ezwriter-cli\src\main.rs'
with open(path, 'r', encoding='utf-8') as f:
    text = f.read()

old = (
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

new = (
    '    // Drain + warmup: consume stale EP2 IN data then consume firmware phantom packet.\n'
    '    // Print byte counts to diagnose alignment.\n'
    '    {\n'
    '        let mut drain = [0u8; 64];\n'
    '        let mut d_iter = 0u32;\n'
    '        loop {\n'
    '            match handle.read_bulk(0x82, &mut drain, Duration::from_millis(100)) {\n'
    '                Ok(n) if n > 0 => {\n'
    '                    eprintln!("  [DIAG] drain iter {d_iter}: read {n} bytes (first4={:02X?})", &drain[..4]);\n'
    '                    d_iter += 1;\n'
    '                    if d_iter >= 8 { break; }\n'
    '                }\n'
    '                Ok(n) => { eprintln!("  [DIAG] drain iter {d_iter}: read {n} bytes (timeout/zero) -> stop"); break; }\n'
    '                Err(e) => { eprintln!("  [DIAG] drain iter {d_iter}: err={e} -> stop"); break; }\n'
    '            }\n'
    '        }\n'
    '    }\n'
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

if old not in text:
    print('ERROR: old block not found', file=sys.stderr)
    sys.exit(1)

text = text.replace(old, new, 1)

# Also patch the non-pipelined loop to print chunk read lengths
old2 = (
    '            let mut buf = [0u8; 64];\n'
    '            match handle.read_bulk(data_ep, &mut buf, Duration::from_secs(3)) {\n'
    '                Ok(len) => {\n'
    '                    file.write_all(&buf[..len])?;\n'
    '                }\n'
    '                Err(e) => {\n'
    '                    println!("\\n  ERROR at chunk {chunk}: read_bulk: {e}");\n'
    '                    break;\n'
    '                }\n'
    '            }'
)

new2 = (
    '            let mut buf = [0u8; 64];\n'
    '            match handle.read_bulk(data_ep, &mut buf, Duration::from_secs(3)) {\n'
    '                Ok(len) => {\n'
    '                    eprintln!("  [DIAG] chunk {chunk} addr=0x{byte_addr:06X}: read {len} bytes (first4={:02X?})", &buf[..4]);\n'
    '                    file.write_all(&buf[..len])?;\n'
    '                }\n'
    '                Err(e) => {\n'
    '                    println!("\\n  ERROR at chunk {chunk}: read_bulk: {e}");\n'
    '                    break;\n'
    '                }\n'
    '            }'
)

if old2 not in text:
    print('ERROR: old2 block not found', file=sys.stderr)
    sys.exit(1)

text = text.replace(old2, new2, 1)
with open(path, 'w', encoding='utf-8') as f:
    f.write(text)
print('OK')
