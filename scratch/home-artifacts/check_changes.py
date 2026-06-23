import subprocess

# Check app.rs diff
result = subprocess.run(
    ['git', 'diff', 'src/ezwriter-gui/src/app.rs'],
    cwd=r'C:\Development\ez-flash-ii-usb-flasher',
    capture_output=True, text=True
)
print("=== app.rs diff ===")
print(result.stdout[:2000])

# Check save_read function in main.rs to see drain
with open(r'C:\Development\ez-flash-ii-usb-flasher\src\ezwriter-cli\src\main.rs', 'r') as f:
    lines = f.readlines()
for i, l in enumerate(lines):
    if 'Drain stale EP2' in l:
        for j in range(i-2, min(i+10, len(lines))):
            print('%d: %s' % (j+1, lines[j].rstrip()))
        print('---')
