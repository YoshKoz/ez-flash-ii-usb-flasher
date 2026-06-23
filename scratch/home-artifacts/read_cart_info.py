with open(r'C:\Development\ez-flash-ii-usb-flasher\src\ezwriter-cli\src\main.rs', 'r') as f:
    lines = f.readlines()
for i, l in enumerate(lines):
    if 'fn cmd_cart_info' in l or 'fn cmd_info' in l:
        for j in range(i, min(i + 60, len(lines))):
            print('%d: %s' % (j+1, lines[j].rstrip()))
        print('---')
        break
