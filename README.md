# Glass Cannon

A frosted-glass DPS meter overlay for **Aion 2** on Windows. It reads the game's own network traffic
(passively, through Npcap), so it never touches the game or its memory. No admin rights
while running, no hidden scheduled tasks, no silent auto-updates, no telemetry.

![The meter during a boss fight](docs/screenshots/overlay.png)

| Skill breakdown | Minimal mode, class-coloured bars |
| --- | --- |
| ![A player's skills](docs/screenshots/skills.png) | ![Minimal mode](docs/screenshots/minimal.png) |

![Settings beside the meter](docs/screenshots/settings.png)

<sub>Screenshots use example data; the player names are made up.</sub>

## Install

1. Install [Npcap](https://npcap.com) (once). Leave **"Restrict Npcap driver's access to
   Administrators only"** unticked, so the meter can run without admin rights.
2. Download `glass-cannon-vX.Y.Z-windows-x64.zip` from
   [Releases](../../releases) and extract it to a folder you own (for example
   `Documents\Glass Cannon`, not `Program Files`: the meter keeps its game data next to
   the exe). The `.sha256` file next to the zip lets you check the download.
3. Run `glass-cannon.exe`. The exe isn't code-signed, so Windows SmartScreen may warn the
   first time: **More info → Run anyway**.

On first start the meter downloads the game data it needs for skill, class and mob names
and icons (about 10 MB from GitHub, see [Game data](#game-data)). It needs Windows 10 or
11 with the WebView2 runtime (built into Windows 11).

## Using the meter

The meter opens and starts capturing straight away, waiting for the game if it isn't
running yet. It's an always-on-top, frosted-glass window (Windows blurs whatever is behind
it) in a warm rose, monospace style. It opens where you last left it (the first time: at
the right edge of the game's monitor).

- **mine / all**: your fight (targets you hit, plus everyone's damage on them) or every
  player fighting nearby.
- **Click a player** for their skill breakdown; hover a skill for its ID; double-click a
  skill or the target to name it yourself.
- **Clock icon**: fight history. Every finished fight is kept (the last 200), with the
  full breakdown. **↺** resets; **pin** toggles always-on-top; **×** quits.
- **≡** opens the **settings** beside the meter: status (Npcap, game, game data),
  appearance, show everyone, record fights (applies the next time you open the meter),
  update game data, fight history and a Desktop/Start-menu shortcut. Esc or × hides it.
- **Appearance** (saved): theme colour (rose, lavender, mint, sky, amber, mono), font
  size (10–18 px; the layout and the meter bars scale with it, the bars in whole pixels so
  every block and row gap is the same size), highlight just your row or fill every row,
  and the bar colour, chosen separately from the theme: follow the theme, each player's
  class, or any of the six colours.
- **Background**: *frosted* (Windows 11 acrylic, kept on while the meter is unfocused),
  *tinted* (no blur) or *clear* (nothing at all; text gets a heavier outline), with the
  tint adjustable from 0 to 100%.
- **Minimal mode**: only the target and the bars, with no title bar, frame or border.
  Drag it from anywhere; hover it for **≡** (settings).
- **Time & ping bar**: an optional strip along the top with the local time and ping.
- **Ping**: the header (or the time & ping bar) shows your round trip to the game
  server, measured from the timing of the game's own TCP traffic (the median of the last 10 seconds). No extra
  packets are sent.
- A fight ends after 15 s without action; the last fight stays on screen until the next.
- **Your character** is recognised the moment you hit a real mob, or at login or a zone
  change. Training dummies don't announce you, so until then the meter shows the name it
  saw last time (if your class matches), marked "(remembered)".
- Other players are named when they come into view, after a zone change, or when they
  start fighting a mob. Classes come from their skills. Mobs are named from their type,
  which the game only sends when a mob appears: mobs that were already there when the
  meter started show as `#ID` until you meet fresh ones.

Recordings (with **record fights** on) go to a `captures` folder next to the exe.
`--replay <session folder>` (with `--speed 10` to fast-forward) plays back a recording.

### Game data

The meter downloads it on first start; **update game data** in the settings refreshes it.
From a terminal:

```bash
packet-lab update-data
```

This downloads data from two open-source (GPL-3.0) Aion 2 meters into
a `gamedata` folder next to the exe: skill names and icons, class icons and boss lists from
[NOIA2](https://github.com/ZDYoung0519/NOIA2), and English mob names (about 9,900, with
boss and training-dummy flags) from [RATmeter](https://github.com/Kuroukihime/AIon2-Dps-Meter). Its tables use
the same IDs the game sends. `--lang ko|zh-TW|zh-CN` picks another language for skills.
The data stays on your machine and isn't part of this repository or the release zip
(see [THIRD_PARTY.md](THIRD_PARTY.md)); game names and art belong to NCSOFT. Apart from
this download the meter never goes online. Names you typed yourself survive updates.

If skills show as numbers, the status line says why, and `meter.log` in
`%APPDATA%\io.github.xinjalin.glass-cannon\` records where the app looked at startup.
That folder also holds `settings.json`, `profile.json` (your last-seen name) and
`history.jsonl`. Settings from before the rename (`%APPDATA%\local.aion-meter`) move
there on first start.

The terminal version is `packet-lab live`; `packet-lab meter <session>` runs the meter
over a recording.

## Building from source

- [Npcap](https://npcap.com), installed as above. The programs load `wpcap.dll` at
  runtime from Npcap's system folder, so no Npcap SDK is needed to build.
- Rust (stable, MSVC toolchain).

```bash
cargo build --release
```

```bash
target/release/glass-cannon.exe
```

`scripts/package.ps1` builds the release zip into `dist/`.

### Releases

GitHub Actions ([.github/workflows/build.yml](.github/workflows/build.yml)) lints, tests
and packages every push and pull request; the zip is kept as a build artifact. To publish
a release, bump `version` in `Cargo.toml`, commit, then push a matching tag:

```bash
git tag v0.2.0
```

```bash
git push origin v0.2.0
```

The workflow then attaches the zip and its checksum to a new GitHub release.

## packet-lab (capture and protocol tools)

`packet-lab` is the command-line tool the meter was built with: it captures and inspects
the game's traffic so the packet format can be worked out from your own captures.

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

- The packet format is worked out from our own captures. Facts learned from the GPL-3.0
  NOIA2 and RATmeter projects (which message carries what) are re-checked against our captures and
  implemented independently; their code is never copied, and their data is only
  downloaded at runtime into a local, untracked folder. Nothing from closed-source meters.
- Reading your own traffic passively doesn't modify the game, but the game's terms may
  still forbid third-party meters. Use at your own account's risk.

## License

[MIT](LICENSE) © Xinjalin. Third-party software and data are listed in
[THIRD_PARTY.md](THIRD_PARTY.md). Aion is a trademark of NCSOFT; this is an unofficial
fan project, not affiliated with or endorsed by NCSOFT.
