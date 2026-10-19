# Glass Cannon

A frosted-glass DPS meter overlay for **Aion 2** on Windows. It reads the game's own
network traffic (passively, through Npcap), so it never touches the game or its memory.
No admin rights while running, no hidden scheduled tasks, no auto-updates, and it never
goes online: everything it needs is in the download.

![The meter during a boss fight](docs/screenshots/overlay.png)

| Skill breakdown | Minimal mode, class-coloured bars |
| --- | --- |
| ![A player's skills](docs/screenshots/skills.png) | ![Minimal mode](docs/screenshots/minimal.png) |

![Settings beside the meter](docs/screenshots/settings.png)

<sub>Screenshots use example data; the player names are made up.</sub>

## Install

1. Install [Npcap](https://npcap.com) (once). Leave **"Restrict Npcap driver's access to
   Administrators only"** unticked, so the meter can run without admin rights.
2. Download `glass-cannon-vX.Y.Z-windows-x64.zip` from [Releases](../../releases) and
   extract it anywhere, for example `Documents\Glass Cannon`. It holds the meter and its
   game data (skill, class and mob names and icons). The `.sha256` file next to the zip
   lets you check the download.
3. Run `glass-cannon.exe`. The exe isn't code-signed, so Windows SmartScreen may warn the
   first time: **More info → Run anyway**.

To update, extract a new release over the old folder; your settings, history and the
names you typed yourself live elsewhere (see below) and stay. It needs Windows 10 or 11
with the WebView2 runtime (built into Windows 11).

## Using the meter

The meter opens and starts capturing straight away, waiting for the game if it isn't
running yet. It's an always-on-top, frosted-glass window (Windows blurs whatever is behind
it) in a warm rose, monospace style. It opens where you last left it (the first time: at
the right edge of the game's monitor), tall enough for five players; it resizes itself
to keep five in view when you change the font size, the time bar or minimal mode.

- **mine / party / all** (click to cycle; also in settings):
  - *mine*: your fight, the targets you hit, with everyone's damage on them.
  - *party*: your party's or group's fight, the targets any of you hit, with only your
    party's damage. The meter reads the roster the game sends when you join a party or an
    expedition group; on your own it's just you.
  - *all*: every player fighting nearby.
- **Click a player** for their skill breakdown; hover a skill for its ID; double-click a
  skill or the target to name it yourself.
- **Clock icon**: fight history. Every finished fight is kept (the last 200), with the
  full breakdown. **↺** resets; **pin** toggles always-on-top; **×** quits.
- **≡** opens the **settings** beside the meter (always fully on screen): appearance on
  the left; status (Npcap, game, game data), whose damage counts, record fights (applies
  the next time you open the meter), fight history, a Desktop/Start-menu shortcut and
  **reset appearance** (click twice: everything below, and the window size, back to the
  defaults) on the right. The window sizes itself to fit. Esc or × hides it.
- **Appearance** (saved): theme colour (rose, lavender, mint, sky, amber, mono), font
  size (10–18 px; the layout and the meter bars scale with it, the bars in whole pixels so
  every block and row gap is the same size), font (any font installed on your PC, picked
  from a searchable list with monospace fonts first; Cascadia Mono by default), highlight
  just your row or fill every row,
  and the bar colour, chosen separately from the theme: follow the theme, each player's
  class, or any of the six colours.
- **Background**: *frosted* (Windows 11 acrylic, kept on while the meter is unfocused),
  *tinted* (no blur) or *clear* (nothing at all; text gets a heavier outline), with the
  tint adjustable from 0 to 100%.
- **Minimal mode**: only the target and the bars, with no title bar, frame or border.
  Drag it from anywhere; hover it for **≡** (settings).
- **Time & ping bar**: an optional strip along the top with the local time and ping.
- **Ping**: the header (or the time & ping bar) shows your round trip to the game
  server, measured from the timing of the game's own TCP traffic (the median of the last
  10 seconds): green under 200 ms, amber under 300 ms, red above. No extra packets are
  sent.
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

The release zip has a `gamedata` folder next to the exe: skill names and icons, class
icons and boss lists from [NOIA2](https://github.com/ZDYoung0519/NOIA2), and English mob
names (about 9,900, with boss and training-dummy flags) from
[RATmeter](https://github.com/Kuroukihime/AIon2-Dps-Meter), two open-source Aion 2
meters. Their tables use the same IDs the game sends. That folder stays under those
projects' GPL-3.0 licence (its `SOURCES.txt` says which commits it came from); game names
and art belong to NCSOFT. It isn't in this repository: `scripts/package.ps1` downloads it
for each release with `packet-lab update-data`, from the upstream commits pinned in
[scripts/gamedata-pins.json](scripts/gamedata-pins.json), so every build of a version
ships the same data. The same command (`--lang ko|zh-TW|zh-CN` for other skill
languages) fills a local build's `target/release/gamedata`.

Skill and mob names you type yourself are saved in `names.json` (below), so a new
release's data never overwrites them.

If skills show as numbers, the status line says why, and `meter.log` in
`%APPDATA%\io.github.xinjalin.glass-cannon\` records where the app looked at startup.
That folder also holds `settings.json`, `names.json`, `profile.json` (your last-seen
name) and `history.jsonl`. Settings from before the rename (`%APPDATA%\local.aion-meter`)
move there on first start.

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

`target/release/packet-lab update-data --pins scripts/gamedata-pins.json` fetches the
game data for a local build (a few seconds; icons download in parallel).
`scripts/package.ps1` builds the release zip into `dist/` with the pinned game data
(`-GameDataCache <folder>` keeps the download and reuses it while the pins don't change;
`-GameData <folder>` uses a folder as it is).

### Updating the game data

The release's game data only changes when you move the pins. To take the latest NOIA2 and
RATmeter data:

```bash
target/release/packet-lab update-data --pins scripts/gamedata-pins.json --bump
```

That updates `scripts/gamedata-pins.json` (and your local `gamedata`); check the meter
still names skills and mobs, then commit the pins file. CI keeps the downloaded data in
its cache, keyed on the pins, so builds only download it again after a bump.

### Releases

GitHub Actions ([.github/workflows/build.yml](.github/workflows/build.yml)) lints, tests
and packages every push and pull request; the zip is kept as a build artifact. To publish
a release, bump `version` in `Cargo.toml`, commit, then push a matching tag:

```bash
git tag vX.Y.Z
```

```bash
git push origin vX.Y.Z
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

# The meter over a recording (--party or --everyone for the other scopes)
target/release/packet-lab meter captures/<session> --party

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
  implemented independently; their code is never copied. Their data ships unchanged
  (reformatted) in the release's `gamedata` folder under their GPL-3.0 licence, and is
  never committed here. Nothing from closed-source meters.
- Reading your own traffic passively doesn't modify the game, but the game's terms may
  still forbid third-party meters. Use at your own account's risk.

## License

[MIT](LICENSE) © Xinjalin. Third-party software and data are listed in
[THIRD_PARTY.md](THIRD_PARTY.md). Aion is a trademark of NCSOFT; this is an unofficial
fan project, not affiliated with or endorsed by NCSOFT.
