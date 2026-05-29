"""Test address commands on Windows CLI via pyusb — once per session"""
import sys
sys.path.insert(0, r"C:\Users\yoshi\ezwriter-reverse\src\ezwriter-cli\target\release")
# Can't import exe. Will just use subprocess
import subprocess, time

base = r"C:\Users\yoshi\ezwriter-reverse"
cli = f"{base}\\src\\ezwriter-cli\\target\\release\\ezwriter-cli.exe"
# Build a quick test binary or just test via the cart-read we have

# For now, let's try: read 64 bytes, then send new cmd, read 64 bytes, etc
# Use the CLI to do sequential reads - need to modify it

# Actually let me just modify the rust code to try different address bytes
print("Need to modify Rust CLI to accept address param")
