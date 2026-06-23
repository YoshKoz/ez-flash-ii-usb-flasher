with open(r'C:\Development\ez-flash-ii-usb-flasher\src\ezwriter-gui\src\device.rs', 'r') as f:
    lines = f.readlines()
for i, l in enumerate(lines):
    if 'fn read_save_with_type' in l:
        for j in range(i, min(i + 80, len(lines))):
            print('%d: %s' % (j+1, lines[j].rstrip()))
        break
