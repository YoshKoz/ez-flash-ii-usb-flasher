"""Disassemble ezwinit.sys with Capstone to find USB firmware download code"""
import struct
from capstone import *
from capstone.x86 import *

with open(r"C:\Users\yoshi\ezwriter-reverse\original_backup\ezwinit.sys", 'rb') as f:
    data = f.read()

# PE parsing
pe_offset = struct.unpack('<I', data[0x3C:0x40])[0]
coff = pe_offset + 4
entry_rva = struct.unpack('<I', data[coff+16:coff+20])[0]
num_sections = struct.unpack('<H', data[coff+2:coff+4])[0]
opt_header_size = struct.unpack('<H', data[coff+16:coff+18])[0]
image_base = struct.unpack('<I', data[coff+52:coff+56])[0]
section_start = coff + 20 + opt_header_size

print(f"Image Base: 0x{image_base:08X}")
print(f"Entry RVA:  0x{entry_rva:08X}")

# Build section map for RVA → raw conversion
sections = []
for i in range(num_sections):
    so = section_start + i * 40
    name = data[so:so+8].rstrip(b'\x00').decode('ascii', errors='replace')
    vsize = struct.unpack('<I', data[so+8:so+12])[0]
    vaddr = struct.unpack('<I', data[so+12:so+16])[0]
    raw_size = struct.unpack('<I', data[so+16:so+20])[0]
    raw_offset = struct.unpack('<I', data[so+20:so+24])[0]
    sections.append({'name': name, 'vaddr': vaddr, 'vsize': vsize, 'raw': raw_offset, 'rsize': raw_size, 'rva_start': vaddr, 'rva_end': vaddr + max(vsize, raw_size)})
    print(f"  {name:10s} RVA=0x{vaddr:08X}-0x{vaddr+max(vsize,raw_size):08X} raw=0x{raw_offset:X}")

# Disassemble all code in .text section
for sec in sections:
    if sec['name'] == '.text':
        text_data = data[sec['raw']:sec['raw']+sec['rsize']]
        text_rva = sec['vaddr']

print(f"\n.text size: {len(text_data)} bytes at RVA 0x{text_rva:X}")

md = Cs(CS_ARCH_X86, CS_MODE_32)
md.detail = True

# Look for patterns:
# 1. PUSH immediate values that look like setup packet bytes
# 2. CALL instructions to USBD or USB functions (import thunks)
# 3. MOV with constants 0x40 (bmReqType) or 0xA0 (bRequest)

print("\n=== Scanning for 0x40 and 0xA0 constants in instructions ===")
count = 0
for insn in md.disasm(text_data, text_rva):
    insn_str = f"0x{insn.address:08X}: {insn.mnemonic} {insn.op_str}"
    
    # Look for PUSH 0xA0 or PUSH 0x40
    if insn.mnemonic == 'push':
        for op in insn.operands:
            if op.type == X86_OP_IMM and op.imm in [0x40, 0xA0, 0xC0, 0x80, 0x7F92, 0xE600]:
                # Show context (±3 instructions)
                ctx_insns = list(md.disasm(text_data[max(0, insn.address - text_rva - 30):min(len(text_data), insn.address - text_rva + 30)], max(0, insn.address - 15)))
                for ci in ctx_insns:
                    marker = " <--" if ci.address == insn.address else ""
                    print(f"  0x{ci.address:08X}: {ci.mnemonic} {ci.op_str}{marker}")
                print()
                count += 1
                if count > 20:
                    break
    if count > 20:
        break

# Also look for URB_FUNCTION_VENDOR_DEVICE = 0x0010 
print("\n=== Scanning for URB_FUNCTION_VENDOR_DEVICE (0x0010) ===")
count = 0
for insn in md.disasm(text_data, text_rva):
    if insn.mnemonic == 'push':
        for op in insn.operands:
            if op.type == X86_OP_IMM and op.imm == 0x0010:
                ctx_insns = list(md.disasm(text_data[max(0, insn.address - text_rva - 30):min(len(text_data), insn.address - text_rva + 30)], max(0, insn.address - 15)))
                for ci in ctx_insns:
                    marker = " <--" if ci.address == insn.address else ""
                    print(f"  0x{ci.address:08X}: {ci.mnemonic} {ci.op_str}{marker}")
                print()
                count += 1
    if count > 5:
        break
