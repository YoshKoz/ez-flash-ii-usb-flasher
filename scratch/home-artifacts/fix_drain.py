# Fix drain in CLI save-read: 1 packet → 8 packets
import sys

path = r'C:\Development\ez-flash-ii-usb-flasher\src\ezwriter-cli\src\main.rs'
with open(path, 'r') as f:
    text = f.read()

old = (
    '    // Drain stale EP2 data from previous operations (cart-info leaves ROM stub in FIFO).\n'
    '    {\n'
    '        let mut drain = [0u8; 64];\n'
    '        let _ = handle.read_bulk(data_ep, &mut drain, Duration::from_millis(200));\n'
    '    }\n'
)
new = (
    '    // Drain stale EP2 data. The firmware auto-streams 8×64-byte chunks into EP2 IN;\n'
    '    // all 8 must be consumed before the first cmd 0x02 response is reliable.\n'
    '    for _ in 0..8 {\n'
    '        let mut drain = [0u8; 64];\n'
    '        if handle.read_bulk(data_ep, &mut drain, Duration::from_millis(200)).is_err() { break; }\n'
    '    }\n'
)
if old not in text:
    print('ERROR: drain block not found in main.rs', file=sys.stderr)
    sys.exit(1)
text = text.replace(old, new, 1)
with open(path, 'w') as f:
    f.write(text)
print('Fixed main.rs drain')

# Fix drain in GUI device.rs: same 1→8 packet fix
path2 = r'C:\Development\ez-flash-ii-usb-flasher\src\ezwriter-gui\src\device.rs'
with open(path2, 'r') as f:
    text2 = f.read()

old2 = (
    '    // Drain stale endpoint data\n'
    '    let mut drain = [0u8; 64];\n'
    '    let _ = handle.read_bulk(DATA_EP, &mut drain, Duration::from_millis(200));\n'
)
new2 = (
    '    // Drain stale EP2 data (firmware auto-streams 8 packets; all must be consumed\n'
    '    // before save-chip data is reliable).\n'
    '    for _ in 0..8 {\n'
    '        let mut drain = [0u8; 64];\n'
    '        if handle.read_bulk(DATA_EP, &mut drain, Duration::from_millis(200)).is_err() { break; }\n'
    '    }\n'
)
if old2 not in text2:
    print('ERROR: drain block not found in device.rs', file=sys.stderr)
    sys.exit(1)
text2 = text2.replace(old2, new2, 1)
with open(path2, 'w') as f:
    f.write(text2)
print('Fixed device.rs drain')
