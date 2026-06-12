import sys
import ctypes.util
print('Python:', sys.version)

# Test find_library
for name in ['libusb-1.0.dll', 'libusb-1.0', 'usb-1.0']:
    result = ctypes.util.find_library(name)
    print(f'find_library({name!r}) = {result!r}')

# Test os.add_dll_directory
import os
user_path = os.path.expanduser('~')
print('User home:', user_path)
os.add_dll_directory(user_path)

# Try loading directly
try:
    lib = ctypes.CDLL('libusb-1.0.dll')
    print('Direct CDLL load: OK')
except Exception as e:
    print('Direct CDLL load failed:', e)
