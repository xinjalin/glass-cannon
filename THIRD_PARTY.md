# Third-party software and data

Glass Cannon itself is MIT licensed (see `LICENSE`). It builds on, or works alongside, the
following, none of which is covered by that licence.

## Game data (the `gamedata` folder, in this repository and the release)

None of the `gamedata` folder is covered by Glass Cannon's MIT licence. The names, text,
numbers and artwork in it are from Aion 2 and belong to NCSOFT. It comes from:

- [NOIA2](https://github.com/ZDYoung0519/NOIA2) by ZDYoung0519, GPL-3.0: skill names,
  skill and class icons, boss lists, and which field boss each world-map ID is.
- [RATmeter (AIon2-Dps-Meter)](https://github.com/Kuroukihime/AIon2-Dps-Meter) by
  Kuroukihime, GPL-3.0: English mob names and extra skill names.
- [aion2.app](https://aion2.app/db/skills), **no licence**: `skill-details.json`
  (cooldowns, costs, descriptions, specialties), scraped from its skill database. Its
  terms don't allow that without permission; it's included at the maintainer's own risk
  and will be removed on request.
- A scraped Aion 2 item database, **no licence**: `theostones.json` (Theostone grades,
  effects and cooldowns, 51 Theostones). The bundle it came from was collected from public
  fan databases and doesn't name them. Only the Theostone entries are used; included at
  the maintainer's own risk and removed on request.
- The same bundle's monsters and dungeons, **no licence**: `dungeons.json` (which dungeon
  and mode each of 1,536 dungeon mobs is in, for the history's dungeon runs). Only those
  entries are used; same terms.
- The same bundle's items, **no licence**: `gear.json` (3,637 weapons, armour and
  accessories: name, grade and item level, for the gear players wear in the party view's
  tooltips). Only those fields are kept; same terms.
- The same bundle's pets and NPCs, **no licence**: `pets.json` (208 pets' names and genus,
  for the pet soul ticker) and `npc-names.json` (3,719 NPCs' names, for the quest givers,
  merchants and the like that the mob lists don't name). Only names are kept; same terms.
- [Aion DPS Meter](https://github.com/SkeeveAN/Aion-DPS-Meter) by SkeeveAN, MIT:
  `pet-spawns.json` (which of 2,256 mob types give which pet's souls, and the areas 163
  pets are found in), from its `Client/assets/aion2/pets/` files. Its licence:
  `gamedata/LICENSE-MIT-AionDPSMeter.txt` (Copyright (c) 2026 SkeeveTV).
- NCSOFT's game asset server, **no licence**: 56 consumable icons (`Icon_Item_Usable_*.png`,
  scrolls, potions and food, shrunk to 64 px) for their buffs, and 208 pet portraits
  (`pet_<id>.png`, 64 px) for the pet tracker. NCSOFT's artwork, like the
  rest of the game data; removed on request.

The NOIA2 and RATmeter files are a separate work alongside the meter (aggregation) and
stay under their projects' GPL-3.0 licence; the release adds the licence text
(`LICENSE-GPL-3.0.txt`) and a `SOURCES.txt` naming the exact commits (pinned in
`scripts/gamedata-pins.json`). Their data is published in its preferred form (JSON and
the original images) in those projects' repositories.

## Npcap (installed separately)

Packet capture uses [Npcap](https://npcap.com), loaded at runtime from its install folder.
Npcap's free licence doesn't allow bundling it, so it isn't included; install it once
from npcap.com.

## Microsoft Edge WebView2

The windows are drawn by the WebView2 runtime, which ships with Windows 11.

## Rust crates

The programs are built with [Tauri](https://tauri.app) (MIT/Apache-2.0) and other Rust
crates, all under permissive licences (MIT, Apache-2.0, BSD, ISC, Zlib, Unicode-3.0,
MPL-2.0, CDLA-Permissive-2.0 and similar). The release zip lists every crate with its
licence in `THIRD-PARTY-CRATES.txt`; `Cargo.lock` pins the exact versions.

## Trademarks

Aion and Aion 2 are trademarks of NCSOFT Corporation. This is an unofficial fan project,
not affiliated with or endorsed by NCSOFT. It only reads the game's network traffic on
your own PC; it doesn't modify the game or its memory.
