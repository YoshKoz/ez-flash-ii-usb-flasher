import sys
import ctypes
import ctypes.util

# Replicate what pyusb libloader does
candidates = ('usb-1.0', 'libusb-1.0', 'usb')
use_dll_workaround = sys.platform == 'win32'
print('DLL workaround:', use_dll_workaround)

for candidate in candidates:
    search = candidate + '.dll' if use_dll_workaround else candidate
    print(f'  Searching: {search!r}', end='')
    libname = ctypes.util.find_library(search)
    print(f' -> {libname!r}')
    if libname:
        print(f'  Loading: {libname!r}')
        try:
            lib = ctypes.CDLL(libname)
            if hasattr(lib, 'libusb_init'):
                print(f'  Found libusb_init! lib = {lib}')
            else:
                print(f'  No libusb_init symbol in {libname}')
        except Exception as e:
            print(f'  Load failed: {e}')

# Also try direct load
print()
print('Direct CDLL("libusb-1.0.dll"):')
try:
    lib = ctypes.CDLL('libusb-1.0.dll')
    print(f'  OK, has libusb_init: {hasattr(lib, "libusb_init")}')
except Exception as e:
    print(f'  Failed: {e}')
