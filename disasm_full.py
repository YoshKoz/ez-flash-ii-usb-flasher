"""Full disassembly search in all code sections for USB patterns"""
import struct
from capstone import Cs, CS_ARCH_X86, CS_MODE_32

with open(r"C:\Users\yoshi\ezwriter-reverse\original_backup\ezwinit.sys", 'rb') as f:
    data = f.read()

pe_offset = struct.unpack('<I', data[0x3C:0x40])[0]
coff = pe_offset + 4
num_sections = struct.unpack('<H', data[coff+2:coff+4])[0]
opt_header_size = struct.unpack('<H', data[coff+16:coff+18])[0]
section_start = coff + 20 + opt_header_size

sections = []
for i in range(num_sections):
    so = section_start + i * 40
    name = data[so:so+8].rstrip(b'\x00').decode('ascii', errors='replace')
    vsize = struct.unpack('<I', data[so+8:so+12])[0]
    vaddr = struct.unpack('<I', data[so+12:so+16])[0]
    raw_size = struct.unpack('<I', data[so+16:so+20])[0]
    raw_offset = struct.unpack('<I', data[so+20:so+24])[0]
    flags = struct.unpack('<I', data[so+36:so+40])[0]
    sections.append({'name': name, 'vaddr': vaddr, 'vsize': vsize, 
                     'raw': raw_offset, 'rsize': raw_size, 'flags': flags})

md = Cs(CS_ARCH_X86, CS_MODE_32)
md.detail = True

# Search ALL sections marked as executable or containing code
for sec in sections:
    if sec['rsize'] == 0:
        continue
    sec_data = data[sec['raw']:sec['raw']+sec['rsize']]
    rva = sec['vaddr']
    name = sec['name']
    flags = sec['flags']
    
    # Also search sections that might have code (INIT)
    if name not in ['.text', 'INIT'] and not (flags & 0x20000000):
        continue
    
    print(f"\n{'='*60}")
    print(f"Section {name} (flags=0x{flags:X}, size={len(sec_data)})")
    print(f"{'='*60}")
    
    # Just dump all instructions - the section is small
    found = False
    for insn in md.disasm(sec_data, rva):
        s = f"0x{insn.address:08X}: {insn.mnemonic} {insn.op_str}"
        
        # Highlight interesting constants
        for op in insn.operands:
            if op.type == 1:  # X86_OP_IMM
                if op.imm in [0x40, 0xA0, 0xA3, 0xC0, 0x80, 
                             0x0010, 0x0011, 0x7F92, 0xE600]:
                    s += "  ***"
        print(s)
    
    # Also show raw hex for INIT section
    if name == 'INIT':
        print("\n  Raw bytes (first 256):")
        for i in range(0, min(256, len(sec_data)), 16):
            line = sec_data[i:i+16]
            hex_s = ' '.join(f'{b:02x}' for b in line)
            ascii_s = ''.join(chr(b) if 32 <= b < 127 else '.' for b in line)
            print(f"    {i:04x}: {hex_s}  {ascii_s}")
