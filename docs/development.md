# Building and the tools

For developers: building Glass Cannon, the game data, and packet-lab. For installing the app, see the [README](../README.md).

## Game data

The `gamedata` folder holds skill, mob, NPC and class names, icons, skill details,
Theostone effects, dungeon mobs and pets. It isn't under the MIT licence: see
[gamedata/README.md](../gamedata/README.md) for sources and how to update it.

If skills show as numbers, check `meter.log` in `%APPDATA%\io.github.xinjalin.glass-cannon`.
That folder also holds your settings, history and the names you typed.

## Building

Needs Rust (stable, MSVC) and [Npcap](https://npcap.com); no Npcap SDK.

```bash
cargo build --release
```

`scripts/package.ps1` builds the release zip. CI builds every push; pushing a `vX.Y.Z`
tag (after bumping `Cargo.toml` and writing `docs/releases/vX.Y.Z.md`) publishes a release.

## packet-lab

The command-line tool behind the meter: it captures and inspects the game's traffic. Run
`target/release/packet-lab --help` for every command.

```bash
# Capture until Ctrl+C
target/release/packet-lab capture --process AION2

# The meter over a recording, or the meter live in the terminal
target/release/packet-lab meter captures/<session> --party
target/release/packet-lab live

# Every message of one type, or which messages contain a number you saw in game
target/release/packet-lab messages captures/<session> 8d00
target/release/packet-lab find captures/<session> 2674

# Count the message types, or write every message of recordings as JSON lines
# (session, flow, time, opcode, payload in hex) for analysis in other tools
target/release/packet-lab opcodes captures/<session>
target/release/packet-lab export captures/<session> captures/<other> --out all.jsonl
```

### Packet Lab (GUI)

`target/release/packet-lab-gui` (`cargo build --release -p packet-lab-gui`) shows the
message stream as it comes in, live from the game or from a recording (a session or flow
folder, all at once or played at 1×, 4× or 16×). Only what the server sends can be read:
what your game sends is encrypted. The window has:

- **opcodes** by family, each marked by how well it's understood (decoded, understood,
  named elsewhere, unknown, or not in the notes at all), with counts and rates; click one
  (or a family) to show only it, ctrl-click to add more;
- **the stream** through that filter, following the newest (scroll up to stop), or frozen;
- **one message**: its bytes, and every reading of the value at the byte you click (u8 to
  u64, f32, varint, zigzag, a length-first string, a timestamp);
- **find** a number (as varint, u16, u32 or u64) or bytes in hex, across the stream and
  marked in the message.

What each opcode is comes from [docs/opcodes.md](opcodes.md), built in: of the 428
opcodes in our recordings, 52 are decoded by the meter and 49 more understood. These include
other players' and mobs' movement, targets, skill acknowledgements, chain skills, loot and
the login lists. 109 are named by other projects but not checked yet, and 218 rare ones
(0.26 % of the traffic) are unknown. Learn something new? Fix its row in the notes and
rebuild; the window shows it from then on. It uses Glass Cannon's launcher colours, and
`--hidden` starts it without a window, for scripted checks.

The app replays a recording with `glass-cannon.exe --replay <session> --speed 10` (the
overlay then covers the launcher's screen); `--hidden` starts it without the launcher, for
scripted checks.
What's known about the protocol is in [docs/protocol.md](protocol.md). Captures go to
`captures/` and can hold login tokens: don't share them.

## Ground rules

- The protocol is worked out from our own captures. Leads from open-source projects
  (NOIA2, RATmeter, Aion DPS Meter and the others in
  [docs/opcodes.md](opcodes.md#sources)) are re-checked and implemented
  independently; nothing from closed-source meters.
- Some game data is scraped without a licence and will be removed on request (see
  [gamedata/README.md](../gamedata/README.md)).
- The game's terms may forbid third-party meters. Use at your own risk.
