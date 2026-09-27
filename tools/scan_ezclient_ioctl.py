"""Find DeviceIoControl call sites and IOCTL codes in EZClient.exe.

We locate the IAT entry for DeviceIoControl, find every `call [DeviceIoControl]`,
then walk backwards to find `push <ioctl>` and the buffer setup, so we can map
the driver IOCTLs the client actually uses for bulk read/write.
"""
import sys, struct
import capstone
import pefile

PATH = r"C:\Users\yoshi\AppData\Local\Temp\opencode\ezclient_dl\innosetup\EZ Client\EZClient.exe"

pe = pefile.PE(PATH, fast_load=False)
base = pe.OPTIONAL_HEADER.ImageBase
print(f"image base 0x{base:X}  entry 0x{base + pe.OPTIONAL_HEADER.AddressOfEntryPoint:X}")

# Build import map: IAT slot address -> (dll, name)
iat = {}   # vaddr -> name
for entry in pe.DIRECTORY_ENTRY_IMPORT:
    dll = entry.dll.decode(errors="replace")
    for imp in entry.imports:
        if imp.name:
            iat[imp.address] = f"{dll}!{imp.name.decode(errors='replace')}"
        else:
            iat[imp.address] = f"{dll}!#{imp.ordinal}"

# Collect executable sections
secs = []
for s in pe.sections:
    if s.Characteristics & 0x20000000:  # IMAGE_SCN_MEM_EXECUTE
        secs.append((base + s.VirtualAddress, s.get_data(),
                     base + s.VirtualAddress, s.Name.rstrip(b"\x00").decode()))

md = capstone.Cs(capstone.CS_ARCH_X86, capstone.CS_MODE_32)
md.detail = True

# Find call [abs] to DeviceIoControl, and pushes of immediate ioctl values nearby
dio_targets = {addr for addr, name in iat.items() if name.endswith("!DeviceIoControl")}
print("DeviceIoControl IAT slots:", [hex(a) for a in dio_targets])

call_sites = []
for va, data, secva, secname in secs:
    for insn in md.disasm(data, va):
        if insn.mnemonic == "call" and insn.op_str.startswith("dword ptr ["):
            try:
                tgt = int(insn.op_str.split("[")[1].rstrip("]"), 16)
            except ValueError:
                continue
            if tgt in dio_targets:
                call_sites.append((insn.address, secname))

print(f"\n{len(call_sites)} DeviceIoControl call sites")
for addr, sec in call_sites:
    print(f"  call @0x{addr:X} ({sec})")

# For each call site, scan backwards up to 260 bytes for `push imm32` collecting candidates
def find_immfile(path):
    return pe

print("\n=== backward scan for pushed immediates (ioctl candidates) ===")
allsec = []
for s in pe.sections:
    if s.Characteristics & 0x20000000:
        allsec.append((base + s.VirtualAddress, s.get_data()))
def disasm_at(vaddr, count=60):
    for va, data in allsec:
        if va <= vaddr < va + len(data):
            off = vaddr - va
            out = []
            for insn in md.disasm(data[off:off+count*15], vaddr):
                out.append(insn)
                if len(out) >= count:
                    break
            return out
    return []

for addr, sec in call_sites:
    insns = disasm_at(addr-260, 120)
    pushes = []
    for insn in insns:
        if insn.address >= addr:
            break
        if insn.mnemonic == "push" and insn.op_str.startswith("0x"):
            v = int(insn.op_str, 16)
            if 0x00220000 <= v <= 0x0022FFFF:
                pushes.append((insn.address, v))
    if pushes:
        print(f"call @0x{addr:X}: ioctl pushes: " +
              ", ".join(f"0x{v:X}@0x{a:X}" for a, v in pushes))