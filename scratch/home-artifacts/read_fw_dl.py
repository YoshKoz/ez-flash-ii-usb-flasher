with open(r'C:\Development\ez-flash-ii-usb-flasher\src\ezwriter-cli\src\main.rs', 'r') as f:
    lines = f.readlines()
for i, l in enumerate(lines):
    if 'fn cmd_firmware_download' in l:
        for j in range(i + 20, min(i + 120, len(lines))):
            print('%d: %s' % (j+1, lines[j].rstrip()))
        break
