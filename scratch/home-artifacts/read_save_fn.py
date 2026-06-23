with open(r'C:\Development\ez-flash-ii-usb-flasher\src\ezwriter-cli\src\main.rs', 'r') as f:
    lines = f.readlines()
for i, l in enumerate(lines):
    if 'fn cmd_save_read' in l:
        start = i
        for j in range(i, min(i + 150, len(lines))):
            print('%d: %s' % (j+1, lines[j].rstrip()))
        break
