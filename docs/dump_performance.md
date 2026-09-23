# Dump Performance: Where the Time Actually Goes

A 16 MB GBA cartridge is 262,144 chunks of 64 bytes. The current dumper reads one
chunk per USB command, with a fixed delay before each read. That round trip — not
the USB bus — is the bottleneck.

## The numbers

| | Value |
|---|---|
| Cartridge | 16 MB = 262,144 × 64-byte chunks |
| Per-chunk cost today | `ROM_READ_DELAY_MS` (5 ms) + one USB round trip |
| Projected 16 MB dump | ~23 min (single pass), ~46 min with per-chunk confirmation |
| USB 1.1 full-speed ceiling | 12 Mbit/s ⇒ ~1.2 MB/s practical |
| Best possible 16 MB dump | ~14 s |

That is a **~100× gap**, and it is almost entirely latency, not bandwidth: the
device is idle for the 5 ms delay plus the host round trip, and only then moves
64 bytes.

The AN2131 is a USB 1.1 full-speed part, so ~1.2 MB/s is a hard ceiling. No host
change can beat ~14 s for 16 MB.

## Lever 1 — command pipelining (host-only, no firmware change)

Issue the next read command *before* waiting for the current packet, so the
device is never idle while the host does USB bookkeeping. The old `--fast` flag
was a depth-2 attempt at this; it is now a proper depth-`N` queue:

```console
ezwriter-cli dump game.gba --pipeline 8
```

Reads stay strict (a short packet aborts rather than shifting the rest of the
file), but the pipelined path is **unconfirmed** — it does not read each chunk
twice. Use it with `--verify` for a full second pass, or accept that per-chunk
confirmation and pipelining are mutually exclusive.

## Lever 2 — find the depth the hardware actually sustains

Endpoint buffering decides how many commands can be outstanding. Rather than
guess, measure:

```console
ezwriter-cli bench
ezwriter-cli bench 512 --depths 1,2,4,8,16,32
```

`bench` is read-only and reports three things:

1. **Auto-stream probe** — how many packets arrive after a *single* command. If
   the firmware streams on its own, a plain read loop beats any command queue.
2. **Round-trip latency** — min/median/p95 per chunk and sustained KB/s.
3. **Throughput vs pipeline depth** — KB/s, projected 16 MB time, and speedup
   against depth 1, so the best depth is a measurement rather than a guess.

## Lever 3 — the firmware already has a batch loop (patch required)

Disassembling `firmware/tusbez.bin` shows the read routine is *counter-driven*:

```
06B0: MOV R5, 0x03      ; count low  <- R3
06B2: MOV R4, 0x02      ; count high <- R2
...
071B: SETB C            ; ---- read loop ----
071C: MOV A,R5
071D: SUBB A,#0x01
071F: MOV A,R4
0720: SUBB A,#0x00
0722: JC  0761          ; count < 1 -> single-shot
0724: ...
072D: LCALL 155B        ; wait for EP4 ready
...
0757: LCALL 12B7        ; perform one block transfer
075A: MOV A,R5
075B: DEC R5
075C: JNZ 0724          ; loop
075E: DEC R4
075F: SJMP 0724
```

A second, structurally identical loop sits at `0x1132` in the save/ROM dispatch
at `0x10C8`. Both walk a 24-bit address pointer (`0x15:0x16:0x17` and
`0x1B:0x1C:0x1D`) and auto-increment it per iteration.

**Every caller hardcodes the counter to 1:**

| Call site | Bytes | Meaning |
|---|---|---|
| `0x0513` | `7B 01 7A 00` | `MOV R3,#1` / `MOV R2,#0` → count = 1 |
| `0x0555` | `7B 01 7A 00` | same, save/ROM dispatch |
| `0x109C` | `7B 01 7A 00` | same, second dispatch |

So the firmware can move `count` blocks per command; nothing ever asks it to.
Changing one immediate (or taking the count from a spare command byte) would cut
262,144 round trips to a few thousand and push a dump toward the bus ceiling.

`an2131_fw_v2.bin` does **not** contain the count loop (`1d 70` / `1c 80` /
`ad 03 ac 02` are absent), so this applies to `tusbez.bin` only.

Two caveats before going down this road:

- One loop iteration is *inferred* to produce one 64-byte EP2 packet. The
  `bench` auto-stream probe settles this on real hardware.
- Patching the shipped firmware means re-verifying every other command that
  shares the dispatcher. The repository already carries firmware-patch tooling
  (`merge_fw_patches.py`, `loader_table2.bin`), so it is tractable — but it is a
  firmware change, not a host change.

## Recommended order

1. Run `bench` and record the real numbers. Everything below is guesswork until
   then.
2. If a single command yields several packets, replace the command loop with a
   read-only loop.
3. Otherwise pick the best pipeline depth from the `bench` table and use
   `dump --pipeline N --verify`.
4. Only if that is still too slow, patch the firmware counter.

## Why the delay exists at all

The firmware streams through a single EP2 buffer, so a read issued too early can
return the *previous* chunk. That is why `ROM_READ_DELAY_MS` is a fixed sleep
rather than zero, and why a single read cannot prove correctness. Pipelining
hides the latency without removing the hazard, which is exactly why the
pipelined path is unconfirmed and `--verify` exists.
