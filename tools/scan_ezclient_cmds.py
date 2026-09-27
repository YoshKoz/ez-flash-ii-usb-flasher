"""Enumerate EZClient.exe's USB command helpers.

Every helper sets a command byte at [base+4] (base held in a register that was
just `lea`d) and then calls the driver wrapper at 0x422110 with IOCTL 0x222051
(send) / 0x22204e (data). We scan for `lea reg,[base+4]` followed shortly by a
`mov byte [reg], imm`, and also for `mov byte ptr [reg+4], imm`.
"""
import pefile

P = r"C:\Users\yoshi\AppData\Local\Temp\opencode\ezclient_dl\innosetup\EZ Client\EZClient.exe"
pe = pefile.PE(P)
base = pe.OPTIONAL_HEADER.ImageBase

code = []
for s in pe.sections:
    if s.Characteristics & 0x20000000:
        code.append((base + s.VirtualAddress, s.get_data()))

# LEA forms: 8D 41 04 (lea eax,[ecx+4]); 8D 71 04 (lea esi,[ecx+4]); 8D 79 04 (lea edi,[ecx+4]);
#           8D 51 04 (lea edx,[ecx+4]); 8D 59 04 (lea ebx,[ecx+4]); 8D 69 04 (lea ebp,[ecx+4])
# plus reg=eax with other bases: 8D 40 04 [eax+4], 8D 43 04 [ebx+4], 8D 46 04 [esi+4], 8D 47 04 [edi+4]
LEA_DISP4 = set()
for b in range(8):
    LEA_DISP4.add(bytes([0x8D, 0x40 + b, 0x04]))          # [eax+4]
    LEA_DISP4.add(bytes([0x8D, 0x41 + b, 0x04]))          # [ecx+4]
# mov byte [reg], imm = C6 00/01/02/03/06/07 imm
MOVB = {bytes([0xC6, r]): r for r in (0x00, 0x01, 0x02, 0x03, 0x06, 0x07, 0x04, 0x05)}

rows = []
for va, d in code:
    i = 0
    n = len(d)
    while i < n - 8:
        m = None
        for pat in LEA_DISP4:
            if d[i:i+3] == pat:
                m = pat
                break
        if m:
            # scan next 24 bytes for mov byte [reg], imm
            for j in range(i+3, min(i+26, n-2)):
                if d[j] in (0xC6,) and d[j+1] in (0x00,0x01,0x02,0x03,0x06,0x07,0x04,0x05):
                    # find the push <count> near the wrapper call (0x222051)
                    end = min(j+40, n-6)
                    blk = d[i:end]
                    rows.append((va+i, d[j+2], bytes(blk)))
                    break
        i += 1

print(f"{len(rows)} candidate command helpers")
seen = {}
for addr, cmd, blk in rows:
    # find push imm32 0x222051 and the push before it (count) in 6 bytes before
    key = (cmd,)
    seen.setdefault(cmd, []).append(addr)
for cmd in sorted(seen):
    print(f"  cmd 0x{cmd:02X}  at " + ", ".join(f"0x{a:X}" for a in seen[cmd]))