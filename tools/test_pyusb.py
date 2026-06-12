import os
import sys

os.add_dll_directory(os.path.expanduser('~'))
os.add_dll_directory(os.path.join(os.environ.get('SystemRoot', 'C:\\Windows'), 'System32'))

import usb
print('usb.__version__:', usb.__version__)

import usb.core
print('usb.core imported OK')

try:
    dev = usb.core.find(idVendor=0x0548, idProduct=0x1005)
    print('found:', dev)
except usb.core.NoBackendError as e:
    print('NoBackendError:', e)
except Exception as e:
    print('other error:', e)
