"""Extract CRomManager::SaverPatch's FindMotif sites from a dumpbin disassembly.

Each site is one FindMotif(buf, len, motif, motiflen) call: the save-library
code pattern SaverPatch looks for in the ROM, with its length and the bytes
themselves read out of the DLL image.
"""
import re
import sys

DLL = r"C:\Users\yoshi\AppData\Local\Temp\opencode\ezclient_dl\innosetup\EZ Client\patchDLL.dll"
DISASM = r"C:\Development\ez-flash-ii-usb-flasher\target\diag\patchdll.disasm.txt"
DST = r"C:\Development\ez-flash-ii-usb-flasher\docs\captures\patchdll_saverpatch_motifs.txt"

SP_LO, SP_HI = 0x10003070, 0x10003C70
FINDMOTIF = 0x10003010

img = open(DLL, "rb").read()

ins = []
for line in open(DISASM, encoding="utf-8", errors="replace"):
    m = re.match(r"\s+([0-9A-F]{8}):\s+((?:[0-9A-F]{2} )+)\s+(.*)$", line)
    if m:
        ins.append((int(m.group(1), 16), m.group(2).strip(), m.group(3).strip()))
ins.sort()

sp = [x for x in ins if SP_LO <= x[0] < SP_HI]
push_re = re.compile(r"push\s+(?:0x)?([0-9A-F]+)h?")

sites = []
for idx, (addr, _bytes, text) in enumerate(sp):
    if text.startswith("call") and ("%X" % FINDMOTIF) in text.upper():
        pushes = []
        j = idx - 1
        while j >= 0 and len(pushes) < 3:
            t = sp[j][2]
            m = push_re.match(t)
            if m:
                pushes.append(m.group(1))
            elif t.startswith(("call", "ret")):
                break
            j -= 1
        pushes.reverse()
        sites.append((addr, pushes))

out = open(DST, "w", encoding="utf-8", newline="\n")
out.write("# CRomManager::SaverPatch (patchDLL.dll, PE32, ImageBase 0x10000000)\n")
out.write("# FindMotif sites, extracted from RVA 0x3070..0x3C70 via MSVC dumpbin /disasm.\n")
out.write("#\n")
out.write("# Each row is one FindMotif(rom, romlen, motif, motiflen) call. The motif is a\n")
out.write("# byte pattern SaverPatch searches the ROM for: either an SDK save-library\n")
out.write("# marker string (FLASH1M_V etc) or a GBA THUMB code sequence from the game's\n")
out.write("# save routine. Bytes are read from the DLL image at the pushed address.\n")
out.write("#\n")
out.write("# call RVA   motiflen  motif RVA  motif bytes\n")

n = 0
for addr, pushes in sites:
    if len(pushes) >= 2:
        len_ok = int(pushes[-2], 16)
        ptr = int(pushes[-1], 16)
        # Pushed values are virtual addresses (ImageBase 0x10000000). For the
        # .rdata and .data sections RVA == file offset, so subtract the base.
        rva = ptr - 0x10000000
        raw = img[rva:rva + min(len_ok, 48)]
        hexs = " ".join("%02X" % x for x in raw)
        asc = "".join(chr(x) if 32 <= x < 127 else "." for x in raw)
        tag = "  <- .rdata SDK marker string" if 0x11270 <= rva < 0x11310 else ""
        out.write("%08X   0x%-5X    %08X  %-48s %s%s\n" % (addr, len_ok, ptr, hexs, asc, tag))
        n += 1
    else:
        out.write("%08X   ?          ?           pushes=%r (not a simple 2-push site)\n" % (addr, pushes))
out.close()
print("sites: %d, decoded: %d -> %s" % (len(sites), n, DST))
