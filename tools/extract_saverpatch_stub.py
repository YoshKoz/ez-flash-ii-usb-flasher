"""Extract the code stubs CRomManager::SaverPatch injects into a ROM.

From the disassembly of SaverPatch (patchDLL.dll RVA 0x3070):

  0x1000333D  mov ecx,2Fh          ; 47 dwords = 188 bytes
  0x10003342  mov esi,10010064h    ; template source
  0x10003347  lea edi,[esp+44h]
  0x1000334E  rep movs             ; stub -> stack

  0x10003359  mov esi,1001004Ch    ; 24-byte companion
  0x10003365  mov ecx,6            ; 6 dwords = 24 bytes
  0x10003376  rep movs

  0x1000335E  mov [esp+0F8h],cl    ; relocation: stub+0xB8 = offset low, +0x21
  0x1000337C  mov [esp+0FEh],al    ; stub+0xBE = offset >> 16
  0x100033FE  mov [esp+0F9h],dl    ; stub+0xB9 = (offset+0x1F) >> 8
  0x1000341B  mov [esp+2Ch],dl     ; 10-byte header written at the blank area
  0x1000342D  mov [esp+2Dh],cl
  0x1000342F  mov [esp+2Eh],dl

  With the stack frame as allocated (sub esp,0ECh, plus the pushes) the stub
  lands at [esp+40h], so offsets 0xF8/0xF9/0xFE above are +0xB8/+0xB9/+0xBE
  into the stub itself.

usage: extract_saverpatch_stub.py <patchDLL.dll> [out.txt]
"""
import sys

DLL = sys.argv[1] if len(sys.argv) > 1 else (
    r"C:\Users\yoshi\AppData\Local\Temp\opencode\ezclient_dl\innosetup"
    r"\EZ Client\patchDLL.dll"
)
OUT = sys.argv[2] if len(sys.argv) > 2 else None

BASE = 0x10000000
# file offset == RVA for .data (VA 0x10000 / RAW 0x10000)
TEMPLATE_A_RVA, TEMPLATE_A_LEN = 0x1004C, 24
TEMPLATE_B_RVA, TEMPLATE_B_LEN = 0x10064, 188

img = open(DLL, "rb").read()

lines = []


def w(s=""):
    lines.append(s)


w("# Code stubs CRomManager::SaverPatch injects into a ROM")
w("# Source: patchDLL.dll .data, read at the addresses pushed at 0x10003342 and")
w("# 0x10003359. Lengths come from the rep movs counts at 0x1000333D and")
w("# 0x10003365. See tools/extract_saverpatch_stub.py for the derivation.")
w()


def dump(name, rva, length):
    data = img[rva:rva + length]
    w("=== %s: .data 0x%05X, %d bytes ===" % (name, rva, length))
    for i in range(0, len(data), 16):
        chunk = data[i:i + 16]
        w("  +0x%03X  %-47s %s" % (
            i,
            " ".join("%02X" % b for b in chunk),
            "".join(chr(b) if 32 <= b < 127 else "." for b in chunk),
        ))
    w()


dump("template A (companion)", TEMPLATE_A_RVA, TEMPLATE_A_LEN)
dump("template B (188-byte stub)", TEMPLATE_B_RVA, TEMPLATE_B_LEN)

w("=== relocation: stack slots written with bytes derived from the target offset ===")
w("  0x1000335E  mov [esp+0F8h],cl   cl = (offset & 0xFF) + 0x21   (0x10003354/0x10003356)")
w("  0x100033FE  mov [esp+0F9h],dl   dl = (offset + 0x1F) >> 8     (0x100033F4)")
w("  0x1000340E  mov [esp+0F9h],cl   cl = offset >> 8              (0x1000340B, else branch:")
w("                                 chosen when the byte before the aligned")
w("                                 offset is 0xFF or 0xCD, at 0x100033EA/EF)")
w("  0x1000337C  mov [esp+0FEh],al   al = offset >> 16              (0x10003373)")
w()
w("  NOTE: the exact byte offset of these slots *within the stub* is not pinned")
w("  here. The template is copied to [esp+44h] at 0x10003347 and the final rep")
w("  movs reads [esp+40h] at 0x10003455, but further pushes and pops in between")
w("  (the two FindMotif calls at 0x100033A0 / 0x100033CC) shift esp, so mapping")
w("  the slots onto stub offsets needs a full stack-frame simulation of the")
w("  function. Treat the stub offsets as unresolved rather than guessed.")
w()
w("=== 10-byte header written at the blank area before the stub ===")
w("  [0] = dword from [esp+28h]   (0x1000343D)")
w("  [4] = dword from [esp+2Ch]   (0x10003444)")
w("  [8] = word  from [esp+30h]   (0x1000344C)")
w("  where [esp+2Ch]/[esp+2Dh]/[esp+2Eh] hold the aligned offset bytes")

text = "\n".join(lines) + "\n"
if OUT:
    open(OUT, "w", encoding="utf-8", newline="\n").write(text)
    print("wrote %s" % OUT)
else:
    print(text)
