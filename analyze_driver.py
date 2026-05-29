import struct, os

def analyze_sys(path, name):
    with open(path, 'rb') as f:
        data = f.read()
    
    print(f"\n=== {name} ({len(data)} bytes) ===")
    
    # Look for IOCTL-like values
    seen = set()
    for i in range(len(data) - 4):
        val = struct.unpack('<I', data[i:i+4])[0]
        device_type = (val >> 16) & 0xFFFF
        func = (val >> 2) & 0xFFF
        method = val & 3
        access = (val >> 14) & 3
        
        if device_type in [0x22] and 0 < func < 0x1000 and val not in seen:
            seen.add(val)
    
    for val in sorted(seen)[:20]:
        device_type = (val >> 16) & 0xFFFF
        func = (val >> 2) & 0xFFF
        method = val & 3
        access = (val >> 14) & 3
        print(f"  IOCTL 0x{val:08X}  func={func} method={method} access={access}")
    
    # Look for strings with backslash (device paths)
    for i in range(len(data) - 8):
        try:
            chunk = data[i:i+64]
            s = chunk.decode('ascii', errors='replace')
            s = ''.join(c for c in s if c.isprintable())
            if len(s) >= 6 and '\\' in s and s[0].isalpha():
                print(f"  String: {s[:60]}")
        except:
            pass

base = r"C:\Users\yoshi\ezwriter-reverse\original\EZ Client\USB_Drivers"
for f in sorted(os.listdir(base)):
    if f.endswith('.sys'):
        analyze_sys(os.path.join(base, f), f)
