# Glass Cannon

A DPS meter and overlay for **Aion 2** on Windows.

Glass Cannon reads the game's network traffic passively through [Npcap](https://npcap.com).
It never reads or writes the game's memory, needs no admin rights, and never goes online.

![The overlay in a boss fight: world timers, the target frame, two action bars (buffs, and cooldowns with a gap), party cooldowns, the meter and party frames](docs/screenshots/overlay.png)

## Features

- **Meter**: damage, healing and damage taken for you, your party or everyone nearby, with
  each player's skills and combat power.
- **Overlay**: the meter, target frame, boss warnings and timers, player bars, party
  frames, action bars of your cooldowns, buffs and debuffs (each set up beside it), and
  world timers as widgets over the
  game. Arrange them freely; they
  step aside for the world map and full-screen menus.
- **Boss warnings**: dangerous casts, dungeon and raid mechanics with what to do, and when
  the boss turns to you.
- **Analysis**: every fight in detail: skills, rotation, buffs, healing, deaths and the
  boss's casts.
- **History**: fights grouped into dungeon and raid runs, with stage times, personal
  bests, and each character's attempts at every boss.
- **Map and pets**: where each pet's mobs spawn and where to gather, and a tracker for the
  pets you're working on.
- **Black market**: what the Black Cloud Merchant offers each character this hour, with the
  pets, skins and wings you don't have yet marked, and Theostones' effects.
- **Collection**: every skin, outfit, wing and Pantheon decoration (artworks, statues,
  colossi), which your characters have and which they're missing, slot by slot.
- **Boss casts**: every boss by content, with the casts the meter has seen it make: rename
  them, and choose which show in the boss warnings and timers.
- **Weekly checklist**: each character's Odyle energy and its weekly sources.

## Screenshots

| Launcher | Settings |
| --- | --- |
| ![The launcher: status, the overlay, the fight going on and the history](docs/screenshots/launcher.png) | ![Settings: the theme and its parts, text, glass and class colours](docs/screenshots/settings.png) |

| Fight history | Pet tracker |
| --- | --- |
| ![A raid night grouped into one run, with each attempt and where it wiped](docs/screenshots/history.png) | ![The pets you track, with their level and souls](docs/screenshots/pets.png) |

![The map: Altgard with its zones, and where each pet's mobs spawn](docs/screenshots/petmap.png)

![Fight analysis: each player's DPS over the fight and their numbers](docs/screenshots/analysis-overview.png)

| Collection | Black market |
| --- | --- |
| ![The collection: your greatswords, with the looks each skin shares](docs/screenshots/collection.png) | ![The black market: this hour's offers, with the skins and wings you're missing marked](docs/screenshots/blackmarket.png) |

![Boss casts: Eternal Ludra's casts, a few renamed and set to warn or not](docs/screenshots/bosscasts.png)

![Action bars: a buffs bar and a cooldowns bar, and bar 1's settings beside it on the overlay (what it tracks, its skills laid out with a gap, your class's active and stigma skills)](docs/screenshots/actionbars.png)

<sub>Fights from a real raid night over a painted backdrop. Other players' names are made
up.</sub>

## Install

Requires Windows 10 or 11 with WebView2 (built into Windows 11).

1. Install [Npcap](https://npcap.com). Leave **"Restrict Npcap driver's access to
   Administrators only"** unticked, so the meter can run without admin rights, and keep
   **"Support loopback traffic"** ticked (the default), which ExitLag and other ping
   reducers need.
2. Download the zip from [Releases](../../releases) and extract it anywhere.
3. Run `glass-cannon.exe`. The program isn't code-signed, so SmartScreen may ask:
   **More info → Run anyway**.

To update, extract a new release over the old one (or anywhere else). Your settings, layout
and history aren't in the program's folder: they're in `%APPDATA%\io.github.xinjalin.glass-cannon`
(paste that into Explorer's address bar), so every version finds them. To start afresh,
delete that folder while the meter's closed.

**Ping reducers and VPNs** (ExitLag and the like) work: the meter follows the game's
connection to wherever it goes, a relay on your PC included. If the launcher says Npcap
can't see your PC's own traffic, reinstall Npcap with "Support loopback traffic" ticked.

## Usage

Start Glass Cannon before or after the game. It begins capturing at once and waits for
the game if it isn't running.

| Action | How |
| --- | --- |
| Show the overlay | **▶** on the launcher |
| Open the settings | **≡** on the launcher |
| Arrange the widgets | The launcher's grid button, or **Ctrl+Shift+F10** in game |
| Hide or show the widgets | **Alt+E** in game |
| See a player's skills | Click their row on the meter |
| Open a fight's analysis | Right-click a player, or the chart icon |

Both hotkeys can be changed in the settings. The full guide is in
[docs/guide.md](docs/guide.md).

## Privacy

- **Network**: Npcap captures on the network adapter the game uses (and your PC's own
  loopback traffic, when the game goes through a ping reducer's relay). Packets that aren't
  the game's are dropped as they arrive; nothing is saved unless you turn on recording.
- **Screen**: to step aside for game menus, the overlay looks at a small square in the
  game's top-right corner, a strip at its bottom middle (for reward screens' Confirm
  button), the top of the screen (for a window's close X, as the Attendance window's) and
  a band across the middle (for a login pop-up's bar), only while the cursor is showing. None of it is ever saved, and the setting can be
  turned off. While you're at the Black Cloud Merchant, the same corner and a band at the
  top left (where the Shop's signs are) tell when you've left it, for the black market
  window to go (settings → general turns that off).
- **Storage**: settings, history and attempts stay on your PC, in
  `%APPDATA%\io.github.xinjalin.glass-cannon`. Nothing is uploaded.

## Building

Requires Rust (the toolchain in `rust-toolchain.toml`) and Npcap.

```bash
cargo build --release
```

See [docs/development.md](docs/development.md) for the tools, the game data and how
releases are made, and [docs/protocol.md](docs/protocol.md) for what is known about the
game's protocol.

## Disclaimer

Glass Cannon is an unofficial fan project. The game's terms may forbid third-party
meters: use it at your own risk. Aion is a trademark of NCSOFT.

## License

[MIT](LICENSE) © Xinjalin. Game data and third-party software have their own terms: see
[gamedata/README.md](gamedata/README.md) and [THIRD_PARTY.md](THIRD_PARTY.md).
