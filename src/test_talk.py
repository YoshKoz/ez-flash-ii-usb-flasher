"""Test: try bulk transfers and vendor commands on 0547:2131"""
import sys
import struct
import os

os.chdir(r"C:\Users\yoshi\ezwriter-reverse")

from ctypes import *
from ctypes import windll

# Use winusb directly through the Rust CLI binary
import subprocess

# First check: did the firmware download actually change anything?
print("=== Before anything: list devices ===")
result = subprocess.run(
    [r"src/ezwriter-cli/target/release/ezwriter-cli", "info"],
    capture_output=True, text=True
)
print(result.stdout)
if result.stderr:
    print("STDERR:", result.stderr)

print("\n=== Try firmware download with original tusbez.bin (likely wrong for this device) ===")
print("SKIPPING - tusbez.bin is for EZ-Writer3 (TI TUSB3210)")

print("\n=== Try with extracted firmware from ezwinit.sys (.data section) ===")
# Copy the extracted firmware
import shutil
shutil.copy("original_backup/an2131_firmware.bin", "src/ezwriter-cli/an2131_firmware.bin")

result = subprocess.run(
    [r"src/ezwriter-cli/target/release/ezwriter-cli", "firmware-download", 
     "src/ezwriter-cli/an2131_firmware.bin"],
    capture_output=True, text=True
)
print(result.stdout)
if result.stderr:
    print("STDERR:", result.stderr)
