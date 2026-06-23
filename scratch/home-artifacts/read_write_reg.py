with open(r'C:\Development\ez-flash-ii-usb-flasher\src\ezwriter-cli\src\main.rs', 'r') as f:
    lines = f.readlines()
for i, l in enumerate(lines):
    if 'fn write_reg' in l:
        for j in range(i, min(i + 30, len(lines))):
            print('%d: %s' % (j+1, lines[j].rstrip()))
        print('---')
for i, l in enumerate(lines):
    if 'fn default_save_read_inner_cmd' in l:
        for j in range(i, min(i + 30, len(lines))):
            print('%d: %s' % (j+1, lines[j].rstrip()))
        print('---')
