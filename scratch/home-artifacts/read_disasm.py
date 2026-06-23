import os

# Check what disasm files contain
files = [
    r'C:\Development\ez-flash-ii-usb-flasher\disasm_v2_cmd02.py',
    r'C:\Development\ez-flash-ii-usb-flasher\disasm_handlers.py',
]
for f in files:
    if os.path.exists(f):
        with open(f) as fh:
            print('=== %s ===' % f)
            print(fh.read()[:3000])
        print()
