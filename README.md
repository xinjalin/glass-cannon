# aion-meter

A DPS meter for Aion 2, built without the risky parts of existing meters. It needs no
admin rights while running, doesn't auto-update, and sends nothing to any server.

Milestone 1 is **packet-lab**, a tool for capturing and inspecting the game's own traffic
so the packet format can be reverse-engineered from your captures.

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
```

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
