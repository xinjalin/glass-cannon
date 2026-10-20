# Glass Cannon

A frosted-glass DPS meter overlay for **Aion 2** on Windows. It reads the game's own
network traffic (passively, through Npcap), so it never touches the game or its memory.
No admin rights while running, no hidden scheduled tasks, no auto-updates, and it never
goes online: everything it needs is in the download.

![The meter in a fight with eight players](docs/screenshots/overlay.png)

| Skill breakdown | Minimal mode, class-coloured bars |
| --- | --- |
| ![A player's skills](docs/screenshots/skills.png) | ![Minimal mode](docs/screenshots/minimal.png) |

![Settings beside the meter](docs/screenshots/settings.png)

![Fight analysis: overview](docs/screenshots/analysis-overview.png)

![Skills with each player's specialties, and a skill's tooltip](docs/screenshots/analysis-skills.png)

| Rotation | Buffs |
| --- | --- |
| ![Skill rotation timeline](docs/screenshots/analysis-rotation.png) | ![Buff timeline and buffs per skill](docs/screenshots/analysis-buffs.png) |

![Healing done, by healer, heal and who was healed](docs/screenshots/analysis-healing.png)

<sub>All screenshots are real fights at training dummies, replayed from recordings; other players' names (and the dummies') are made up.</sub>

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
- **dmg / heal / taken** (the button by the fight clock; in minimal mode, hover): what
  the rows show. *heal* is healing done: heals over time (credited to whoever applied
  the effect) and heals players cast on themselves, counted by the HP they restored;
  direct heals on other players haven't been seen yet. *taken* is the damage players
  took from mobs. Saved for next time.
- **Click a player** for their skill breakdown; hover a skill for its ID; double-click a
  skill or the target to name it yourself.
- **Right-click a player** (or the chart icon) to open the **analysis** window on that
  fight, live or from history. It's a full dashboard:
  - *overview*: DPS, casts, crit, back and front attacks, perfect and double strikes,
    DPS over time for the top players with the target's HP in a strip beneath, and every
    player side by side;
  - *skills*: every skill with the specialties the player chose (read from the skill
    IDs the game sends), casts, hits, multi-hit, crit, double, perfect, back, front, min,
    max, average and time between casts (click a column to sort). Hover a skill for its
    cooldown, cost, range, description and all its specialties, the chosen ones ticked.
    The game doesn't send skill levels, so descriptions show level 1 (and the top level);
  - *rotation*: your opening casts and a zoomable timeline of every cast by skill
    (crits ringed, back attacks dotted, the target's HP shaded behind). Zoomed in to the
    end of a live fight, the timeline follows it as it happens (**[now]** jumps there);
  - *buffs*: buffs on you and debuffs on the target over time with their uptime, and
    for each skill the share of its casts that landed with each effect up;
  - *healing* and *taken*: healing done and damage taken over time, per player, with
    each player's heals (or the mob skills that hit them) and who they healed (or which
    mobs hit them).
  **[copy text]** puts a summary on the clipboard for Discord (a table of the players);
  **[copy image]** copies a summary card with the players and their DPS over time.
  Filter by target when a fight had several. The newest 50 fights keep their full
  analysis; older ones keep the summary.
- **Clock icon**: fight history. Every finished fight is kept (the last 200), with the
  full breakdown. **▷|** ends the fight now (your next hit starts a new one, without
  waiting for the fight to go quiet); **↺** resets; **pin** toggles always-on-top; **×**
  quits.
- **Lock** (bottom-right corner, or *click-through* in the settings): clicks go through
  the meter to the game. While it's locked, the lock button is the only part of the meter
  that takes the mouse; click it to unlock. It stays locked next time you open the meter.
- **≡** opens the **settings** beside the meter (always fully on screen): appearance on
  the left; status (Npcap, game, game data), whose damage counts, how often the meter
  updates (0.2–2 s; 0.5 s by default), click-through, record fights (applies
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
  start fighting a mob: the game sends no other name for them. So players who were
  already around you when you opened the meter show as `#ID` until one of those happens
  (a teleport or zone change names everyone nearby). The meter remembers names for the
  rest of your login, so restarting it doesn't lose them. Classes come from their skills. Mobs are named from their type,
  which the game only sends when a mob appears: mobs that were already there when the
  meter started show as `#ID` until you meet fresh ones.

Recordings (with **record fights** on) go to a `captures` folder next to the exe.
`--replay <session folder>` (with `--speed 10` to fast-forward) plays back a recording.

### Game data

The release zip has a `gamedata` folder next to the exe, the same as the repository's
[gamedata](gamedata) folder:

- skill names and icons, class icons and boss lists from
  [NOIA2](https://github.com/ZDYoung0519/NOIA2), and English mob names (about 9,900, with
  boss and training-dummy flags) plus about 2,200 more skill names from
  [RATmeter](https://github.com/Kuroukihime/AIon2-Dps-Meter), two open-source Aion 2
  meters, under their GPL-3.0 licence;
- skill details (cooldowns, costs, descriptions, specialties) scraped from
  [aion2.app](https://aion2.app/db/skills), without a licence.

None of it is covered by Glass Cannon's MIT licence, and the game's names, text and art
belong to NCSOFT; [gamedata/README.md](gamedata/README.md) has the details. Their tables
use the same IDs the game sends.

Skill and mob names you type yourself are saved in `names.json` (below), so a new
release's data never overwrites them.

If skills show as numbers, the status line says why, and `meter.log` in
`%APPDATA%\io.github.xinjalin.glass-cannon\` records where the app looked at startup.
That folder also holds `settings.json`, `names.json`, `profile.json` (your last-seen
name), `history.jsonl`, `fights` (the full analysis of the newest 50 fights),
`players.json` (names of players seen this login) and `window.json` (where you left the
meter). Settings from before the rename (`%APPDATA%\local.aion-meter`)
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

A build in `target/release` uses the repository's `gamedata` folder.
`scripts/package.ps1` builds the release zip into `dist/` with that folder.

### Updating the game data

The game data is committed, so builds don't download anything. To take the latest NOIA2
and RATmeter data (`--lang ko|zh-TW|zh-CN` for other skill languages):

```bash
target/release/packet-lab update-data --pins scripts/gamedata-pins.json --bump --out gamedata
```

That updates `scripts/gamedata-pins.json` and the `gamedata` folder; check the meter still
names skills and mobs, then commit both. The skill details come from a scrape of
aion2.app's skill database (a JSON object of skills by ID):

```bash
target/release/packet-lab skill-details <scrape.json> --scraped <date>
```

### Releases

GitHub Actions ([.github/workflows/build.yml](.github/workflows/build.yml)) lints, tests
and packages every push and pull request; the zip is kept as a build artifact. To publish
a release, bump `version` in `Cargo.toml`, write what's new in `docs/releases/vX.Y.Z.md`
(optional; it goes above GitHub's list of merged pull requests), commit, then push a
matching tag:

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

# The meter over a recording (--party or --everyone for the other scopes): per-skill
# casts, hits, crit, back/front, perfect, double strike and multi-hit
target/release/packet-lab meter captures/<session> --party

# The last fight as the analysis window gets it (JSON)
target/release/packet-lab meter captures/<session> --analysis

# Every message of one type, with its time and payload in hex
target/release/packet-lab messages captures/<session> 8d00

# HP updates checked against the damage on each entity (--on <id> lists one entity's)
target/release/packet-lab hp captures/<session>

# Buff and debuff spans (who, what, from whom, how long), optionally on one entity
target/release/packet-lab buffs captures/<session> --on 11792

# Damage events with names, per-attacker totals and DPS (--mine = only your hits)
target/release/packet-lab damage captures/<session> --mine

# Build gamedata/skill-details.json from a scrape of aion2.app's skill database
target/release/packet-lab skill-details <scrape.json> --scraped <date>

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
  implemented independently; their code is never copied. Their data is in the
  `gamedata` folder (reformatted) under their GPL-3.0 licence. Nothing from closed-source
  meters.
- `gamedata/skill-details.json` was scraped from aion2.app, whose terms don't allow that
  without permission. It's included at the maintainer's own risk and will be removed on
  request from aion2.app or NCSOFT.
- Reading your own traffic passively doesn't modify the game, but the game's terms may
  still forbid third-party meters. Use at your own account's risk.

## License

[MIT](LICENSE) © Xinjalin. Third-party software and data are listed in
[THIRD_PARTY.md](THIRD_PARTY.md). Aion is a trademark of NCSOFT; this is an unofficial
fan project, not affiliated with or endorsed by NCSOFT.
