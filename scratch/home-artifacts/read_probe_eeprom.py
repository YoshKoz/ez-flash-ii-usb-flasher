with open(r'C:\Development\ez-flash-ii-usb-flasher\src\ezwriter-cli\src\main.rs', 'r') as f:
    lines = f.readlines()
for i, l in enumerate(lines):
    if 'fn cmd_probe_eeprom' in l or 'fn cmd_reset' in l or 'fn cmd_passive_read' in l:
        print('Found at line %d: %s' % (i+1, l.rstrip()))
        for j in range(i, min(i + 80, len(lines))):
            print('%d: %s' % (j+1, lines[j].rstrip()))
        print('---')
