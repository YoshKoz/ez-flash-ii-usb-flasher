# ROM Write: Capture-and-Replay Plan

Supersedes the "Next step" section of `rom_write_status.md`. The blocker named
there — no working original client to capture — is gone: EZ Client 3.26 now
runs in the `EZFlash-Win7x86` VirtualBox VM, detects the writer
(`Driver Version 1.40.0`) and reads the cart (`C EZ2 256M`, `EZLoader`,
`POKEMON FIRE`). This plan turns that into a working native `rom-write`.

## What changes compared to the previous approach

The earlier sessions guessed a packet, sent it, and read back. Every guess
ended "no wedge, no write", and static RE is exhausted. This plan changes
three things:

1. **Capture everything, not only the write.** The original stack does more
   than the write packet: the Windows driver uploads the firmware and loader
   patches, and EZ Client runs its own init and a JEDEC probe at startup. Any
   of those could hold the missing enable step. The capture starts at cold
   enumeration.
2. **Check the capture against paths that already work.** Before reading
   anything into a ROM-write trace, decode a ROM read and a save write
   (both work natively) and confirm they match the Rust code. That checks
   the capture pipeline and settles the endpoint mapping for good.
3. **Replay first, then understand.** Replay the captured bytes verbatim
   from native Rust. If the replay writes, the protocol is solved and the
   rest is minimising. If it doesn't, the difference is below the packet
   level (timing, zero-length packets, control requests, firmware), and
   the capture shows which.

## Clue already on the table

In the last session, native `cart-info` printed an **empty** title, code and
maker. EZ Client in the VM read the same cart as `POKEMON FIRE`. So the
native session is already in a different cart state from the client's,
before any write. It's the same symptom as the write failure: commands are
accepted, but the cart-side state is wrong. The Phase 2 startup capture
should explain it, and it is cheaper to chase than the write.

## Phase 0 — Safety net (native, before touching the VM)

- `ezwriter-cli dump backup_full.gba` (32 MB) and `save-read` → keep both
  outside the repo.
- Record `cart-read 0 1`, `cart-read 42496 1` (0xA600) and `cart-info`
  output as the baseline.
- Recovery path now exists: if a burn goes wrong, EZ Client in the VM can
  reflash the cart from `backup_full.gba`. Confirm the VM still launches
  EZ Client before Phase 3.

## Phase 1 — Pick and prove the capture method

Try in order and keep the first that shows a known transfer:

| # | Method | Why |
|---|--------|-----|
| A | `VBoxManage controlvm EZFlash-Win7x86 usbattach <uuid> --capturefile C:\...\ez.pcap` | VirtualBox records the passed-through device's URBs itself. No extra driver, and it starts at attach time, so it includes enumeration. |
| B | Host USBPcap (`USBPcapCMD.exe`, already installed) on the root hub the writer is on | VBoxUSB still submits URBs through the host stack, so the root-hub filter should see them. Needs checking. |
| C | USBPcap inside the Win7 guest | Last resort: one more driver in the VM. |

Proof of method: capture VM boot → EZ Client launch → close. Open it in
tshark and confirm you can see the `0xA0` control writes and at least one
`04`-led OUT packet on EP4. If you can't, move to the next method.

Run the VM in **GUI mode** and have the user click in EZ Client. Driving it
through `keyboardputscancode` and screenshots worked, but it is slow and
fragile for a multi-step burn.

## Phase 2 — Control captures (paths that already work natively)

One pcap per action, each starting from a fresh EZ Client launch:

1. `startup.pcap` — cold replug in the VM → driver firmware upload → EZ Client
   open → cart detected. Diff the `0xA0` writes against native
   `firmware-download` / `loader_table1.bin` / `loader_table2.bin`
   byte for byte. **A mismatch here is the top suspect for the whole
   write problem.**
2. `readinfo.pcap` — EZ Client "ROM info" (cart header read). Compare with
   native `cart-info` and explain the empty title.
3. `savewrite.pcap` — write a save (use the save you dumped in Phase 0, so it
   is a no-op). Confirm the `0x03`/`0x20` shape and the payload endpoint
   match the Rust code.

Exit criterion: every packet in (2) and (3) maps onto a known native command.
Until it does, don't interpret the write capture.

## Phase 3 — The write capture

- Target: the smallest write EZ Client will do. Add one tiny homebrew
  GBA ROM (a few KB) to the existing multi-game list and burn it. Check
  whether EZ Client rewrites the loader or only appends. File size and
  packet count will show it.
- Capture `write.pcap` from EZ Client launch to "burn finished". Then use
  the VM's EZ Client to read the ROM back and confirm the write really
  landed.
- If EZ Client insists on rewriting the whole cart, that's acceptable:
  Phase 0 has the backup, and the capture will be large but complete.

## Phase 4 — Tooling (Rust + Python, in this repo)

1. `tools/pcap_to_trace.py` — pcap (USBPcap linktype 249 or usbmon 220) → a
   plain-text trace, one transfer per line:
   `t=+0.005 OUT ep=0x04 len=3 04 00 53` / `IN ep=0x82 len=64 …` /
   `CTRL 40 A0 wValue=… wIndex=… len=…`. It includes zero-length packets and
   inter-transfer gaps, because those are candidates too.
2. `tools/trace_diff.py` — normalises addresses and payload bytes and diffs two
   traces (captured vs a native run of the same operation). Run
   `ezwriter-cli` under host USBPcap to get the native trace.
3. `ezwriter-cli replay <trace> [--from N --to M] [--keep-timing]` — marked
   **experimental**. It sends OUT/control transfers exactly as recorded, reads
   IN transfers, and flags any IN that differs from the capture. It refuses
   traces that contain erase commands unless `--allow-erase` is passed.

## Phase 5 — Replay, then minimise

1. Replay `startup` + `write` natively, then run `cart-read` over the written
   range.
   - **Writes** → protocol confirmed. Delta-debug: drop blocks of the trace
     (startup extras, gaps, register writes) until you have the minimal set
     that still writes. Fold that into `cmd_rom_write` / `rom_program_chunk`.
   - **Doesn't write, and every IN matches** → the difference is timing or
     host-side. Try `--keep-timing` and zero-length packets.
   - **Doesn't write, and an IN differs** → the first differing IN marks
     where native and client state diverge. Work back from there.
2. Re-run `rom_write_test_plan.md` Steps 2–5 with the fixed `rom-write`.
3. Port the fix to the GUI's `device.rs` only after the CLI passes the test
   plan.

## Phase 6 — Record it

- Commit the traces in `docs/captures/` (as text; the pcaps are small, keep
  them too if they are under a few MB) and update `rom_write_status.md` with
  the answer.
- Update `ROM_WRITE_RUNBOOK.md` so it points to `rom-write`, not
  `flash-probe` trial and error.

## Stop rules

- Never send selector `0x69` or EP6 payloads (they wedge the 8051). `replay`
  must refuse them too.
- If a replay wedges the device: replug once, run `reload`, and record which
  trace line did it before continuing.
- Two replays that don't write with identical IN data → stop and compare
  timing before trying anything else. Don't go back to guessing packets.
- The VM and the host can't own the writer at the same time. Power the VM
  off (not just close EZ Client) before any native step.

## Order of work (checklist)

- [ ] Phase 0 backup + baseline
- [ ] Phase 1 capture method proven (A → B → C)
- [ ] `startup.pcap` + firmware/patch byte diff
- [ ] `readinfo.pcap` explains empty native `cart-info`
- [ ] `savewrite.pcap` matches native save-write
- [ ] `write.pcap` + read-back in the VM confirms the write landed
- [ ] `pcap_to_trace.py`, `trace_diff.py`, `replay` subcommand
- [ ] Verbatim replay writes natively
- [ ] Minimal sequence folded into `rom-write`, test plan passes
- [ ] Docs updated, GUI port
