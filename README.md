# aion-meter

A DPS meter for Aion 2, built without the risky parts of existing meters. It needs no
admin rights while running, doesn't auto-update, and sends nothing to any server.

Milestone 1 is **packet-lab**, a tool for capturing and inspecting the game's own traffic
so the packet format can be reverse-engineered from your captures.

## The overlay (Aion Meter)

```bash
cargo build --release
```

```bash
target/release/meter-app.exe
```

It's an always-on-top, see-through window that starts capturing on its own and waits
for the game if it isn't running yet. Drag the title bar to move it; drag an edge to
resize it.

- **Mine / All**: your fight (targets you hit, plus everyone's damage on them) or every
  player fighting nearby.
- **Click a player** to see their skill breakdown.
- **↺** resets; **pin** toggles always-on-top.
- A fight ends after 15 s without action; the last fight stays on screen until the next.
- Names appear when players come into view or after a zone change. Players already
  nearby when the meter starts show as `#ID` until then.

- It opens where you last left it; the first time (or if that monitor is gone) it
  opens at the right edge of the monitor the game is on.

Options: `--replay <session folder>` plays back a recorded capture (`--speed 10` to
fast-forward), `--record` also saves the live session to `captures/`.

### Skill names and icons

```bash
target/release/packet-lab import-skills
```

This downloads skill names and icons from the open-source
[NOIA2](https://github.com/ZDYoung0519/NOIA2) project (GPL-3.0) into a `skills` folder
next to the programs (`target/release/skills`). Its tables use the same skill IDs the game
sends. The overlay looks there first, then in `%APPDATA%\local.aion-meter\skills\`.
`cargo clean` deletes it, so run the import again after that.
`--lang ko|zh-TW|zh-CN` picks another language. The data stays on your machine and isn't
part of this repository; game names and art belong to NCSoft. The overlay itself never
goes online.

In a skill breakdown, hover a skill for its ID, or double-click it to name it yourself.
Your own names survive a re-import.

If skills show as numbers, the status line says why, and `meter.log` in
`%APPDATA%\local.aion-meter\` records where the overlay looked at startup.

The terminal version is `packet-lab live`; `packet-lab meter <session>` runs the meter
over a recording.

## Requirements

- [Npcap](https://npcap.com), installed **without** "Restrict Npcap driver's access to
  Administrators only". `packet-lab` loads `wpcap.dll` at runtime from Npcap's system
  folder, so no Npcap SDK is needed to build.
- Rust (stable, MSVC toolchain).

## Usage

```bash
cargo build --release
```

```bash
# Which process is the game, and which sockets does it have?
target/release/packet-lab procs aion2

# Capture the game's traffic until Ctrl+C (or --seconds N)
target/release/packet-lab capture --process AION2

# Inspect a flow: chunks in time order, then a hex dump of any position
target/release/packet-lab chunks captures/<session>/flows/<flow> --side in
target/release/packet-lab dump captures/<session>/flows/<flow> --side in --pos 0 --len 512

# Re-run reassembly on a capture (e.g. after improving the parser)
target/release/packet-lab replay captures/<session>/raw.pcap

# Message types in the game stream (bundles unpacked; game connection picked automatically)
target/release/packet-lab opcodes captures/<session>

# Damage events with names, per-attacker totals and DPS (--mine = only your hits)
target/release/packet-lab damage captures/<session> --mine

# Who was identified: you, nearby players, mob types
target/release/packet-lab entities captures/<session>

# Which messages contain a number you saw in game?
target/release/packet-lab find captures/<session> 2674
```

What's known about the protocol so far is in [docs/protocol.md](docs/protocol.md).

## Session layout

```text
captures/<YYYYmmdd-HHMMSS>/
  session.json   process, adapter and time of the capture
  raw.pcap       every game packet (opens in Wireshark)
  events.jsonl   flows opened/closed, with per-direction stats
  flows/<id>-<proto>-<localport>-to-<ip>_<port>/
    in.bin / out.bin   reassembled bytes received / sent
    index.jsonl        one line per chunk: time, side, kind, stream offset, file pos, length
```

Captures contain your own game session, possibly including login tokens. `captures/` is
git-ignored; don't share captures publicly.

## How it works

- **Finding the game:** the Windows connection tables (`GetExtendedTcpTable` /
  `GetExtendedUdpTable`) list which sockets the game process owns. They are polled every
  500 ms.
- **Capture:** one Npcap handle per adapter the game uses, filtered to TCP/UDP.
  Packets from connections not yet identified are held for up to 3 s, so a new
  connection's handshake and first messages aren't lost.
- **Reassembly:** per-direction TCP stream rebuilding with out-of-order, retransmission,
  overlap and wraparound handling. Holes that never fill are skipped and recorded as gaps.

## Ground rules

- The packet format is worked out from our own captures. Don't copy code, data files or
  assets from other meters.
- Reading your own traffic passively doesn't modify the game, but the game's terms may
  still forbid third-party meters. Use at your own account's risk.
