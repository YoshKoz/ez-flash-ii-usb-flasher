"""Turn the captured EZClient Burn trace into a replay script for ezwriter-cli/gui.

Script lines:
  C <hex>        EP4 command
  P <hex>        0x1a status read, repeat until byte0 bit7 (flash ready)
  S <hex>        0x1a status read once (captured reply had no ready bit, e.g. ID reads)
  O              EP2 OUT 4096 bytes; first 8 = preamble (0x00), rest = ROM window
  R <ep> <len>   bulk IN read
"""

import sys

SRC = "docs/captures/ezclient_successful_burn_trace.txt"
DST = "docs/captures/ezclient_burn_script.txt"

rows = []
for line in open(SRC):
    f = line.rstrip("\n").split("\t")
    if len(f) < 7 or not f[6]:
        continue
    t = float(f[1])
    if t >= 51.5:
        rows.append((t, f[3], f[6]))

start = next(i for i, r in enumerate(rows) if r[1] == "0x04" and r[2] == "05")
end = next(i for i, r in enumerate(rows) if i > start and r[1] == "0x04" and r[2] == "06")
rows = rows[start : end + 1]

out = []
ep2 = 0
i = 0
while i < len(rows):
    t, ep, d = rows[i]
    if ep == "0x04" and d.startswith("1a") and i + 1 < len(rows) and rows[i + 1][1] == "0x84":
        # Collapse a run of (1a cmd, IN 0x84) pairs into one poll group.
        group = {}
        order = []
        while i + 1 < len(rows) and rows[i][1] == "0x04" and rows[i][2].startswith("1a") and rows[i + 1][1] == "0x84":
            cmd, st = rows[i][2], int(rows[i + 1][2][:2], 16)
            if cmd not in group:
                order.append(cmd)
                group[cmd] = []
            group[cmd].append(st)
            i += 2
        for cmd in order:
            sts = group[cmd]
            out.append(f"{'P' if sts[-1] & 0x80 else 'S'} {cmd}")
        continue
    if ep == "0x04":
        out.append(f"C {d}")
    elif ep == "0x02":
        if ep2 < 8 and d.strip("0"):
            sys.exit(f"preamble payload {ep2} not zero")
        out.append("O")
        ep2 += 1
    else:
        out.append(f"R {ep} {len(d) // 2}")
    i += 1

open(DST, "w", newline="\n").write("\n".join(out) + "\n")
print(f"{len(out)} lines, {ep2} EP2 payloads -> {DST}")
