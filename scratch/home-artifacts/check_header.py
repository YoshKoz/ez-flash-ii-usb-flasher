with open(r'C:\Development\ez-flash-ii-usb-flasher\scratch\home-artifacts\test_header.bin', 'rb') as f:
    d = f.read()
print('size=%d bytes' % len(d))
print('entry_point=%s' % d[0:4].hex())
print('title=%r' % d[0xa0:0xac])
print('game_code=%r' % d[0xac:0xb0])
print('maker_code=%r' % d[0xb0:0xb2])
print('first16=%s' % d[:16].hex())
