import struct, sys

fw = open('tusbez.bin', 'rb').read()
loader = open('loader_table2.bin', 'rb').read()

count = struct.unpack_from('<H', loader, 8)[0]
off = 10
buf = bytearray(fw)
for _ in range(count):
    addr = struct.unpack_from('<H', loader, off)[0]
    ln = loader[off + 2]
    off += 3
    buf[addr:addr + ln] = loader[off:off + ln]
    off += ln

print(f"Patched firmware: {len(buf)} bytes")

# Find CJNE A,#imm,rel instructions (opcode 0xB4) - these form cmd dispatch tables
print("\nAll CJNE A,#imm,offset in first 0x2000 bytes:")
for pc in range(0, min(len(buf) - 3, 0x2000)):
    if buf[pc] == 0xb4:
        imm = buf[pc + 1]
        rel = buf[pc + 2]
        npc = pc + 3
        t = npc + (rel if rel < 128 else rel - 256)
        print(f"  {pc:04X}: CJNE A,#{imm:02X} -> {t:04X}")

# Also look for the main command loop - usually reads EP4 OUT data
# EP4 OUT = 0x04, the FIFO address would be at some XRAM location
# Look for reads from specific XRAM addresses (could be EP4 FIFO)
print("\nReads from XRAM 0x7E** range (likely USB FIFOs):")
for pc in range(0, min(len(buf) - 3, 0x2000)):
    if buf[pc] == 0x90 and buf[pc+1] == 0x7e:
        hi = buf[pc+1]; lo = buf[pc+2]
        addr = (hi << 8) | lo
        print(f"  {pc:04X}: MOV DPTR,#0x{addr:04X}")

print("\nReads from XRAM 0x7C** range (likely cmd packet buffer):")
for pc in range(0, min(len(buf) - 3, 0x2000)):
    if buf[pc] == 0x90 and buf[pc+1] == 0x7c:
        hi = buf[pc+1]; lo = buf[pc+2]
        addr = (hi << 8) | lo
        nxt = buf[pc+3] if pc+3 < len(buf) else 0
        op_str = "MOVX A,@DPTR" if nxt == 0xe0 else f"then {nxt:02X}"
        print(f"  {pc:04X}: MOV DPTR,#0x{addr:04X}  -> {op_str}")
