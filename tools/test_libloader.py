import sys
sys.path.insert(0, 'C:\\Users\\yoshi\\AppData\\Roaming\\Python\\Python314\\site-packages')
import importlib
import usb.libloader
spec = importlib.util.spec_from_file_location('usb.libloader', usb.libloader.__file__)
libloader = importlib.util.module_from_spec(spec)
spec.loader.exec_module(libloader)

try:
    lib = libloader.load_locate_library(
        ('usb-1.0', 'libusb-1.0', 'usb'),
        'cygusb-1.0.dll', 'Libusb 1',
        find_library=None, check_symbols=('libusb_init',))
    print('load_locate_library OK:', lib)
except Exception as e:
    print('load_locate_library failed:', type(e).__name__, e)
