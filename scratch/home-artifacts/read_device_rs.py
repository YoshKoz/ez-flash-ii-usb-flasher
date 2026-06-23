with open(r'C:\Development\ez-flash-ii-usb-flasher\src\ezwriter-gui\src\device.rs', 'r') as f:
    lines = f.readlines()
for i, l in enumerate(lines):
    if 'read_save_with_type' in l or 'save_read_handler' in l or 'write_reg' in l:
        print('%d: %s' % (i+1, l.rstrip()))
