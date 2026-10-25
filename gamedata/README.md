# Game data

Skill, class and mob names, icons and skill details for Glass Cannon's overlay and
analysis window. The release zip ships this folder next to the program.

**This folder is not covered by Glass Cannon's MIT licence.** The names, text, numbers
and artwork are Aion 2 game data and belong to NCSOFT. It comes from these places:

| file | from | licence |
| --- | --- | --- |
| `skills.json` (names), skill and class icons, `npcs.json` (bosses) | [NOIA2](https://github.com/ZDYoung0519/NOIA2) | GPL-3.0 |
| `npcs.json` (English mob names), extra names in `skills.json` | [RATmeter](https://github.com/Kuroukihime/AIon2-Dps-Meter) | GPL-3.0 |
| `skill-details.json` (cooldowns, costs, descriptions, specialties) | [aion2.app](https://aion2.app/db/skills), compiled from NCSOFT's data | none: used without a licence |
| `theostones.json` (Theostone grades, effects, cooldowns) | a scraped item database collected from public fan databases (the bundle names no sites) | none: used without a licence |
| `dungeons.json` (each dungeon mob's dungeon and mode) | the same bundle's monsters and dungeons | none: used without a licence |
| `pets.json` (pets' names and genus, for the pet soul ticker) | the same bundle's pets | none: used without a licence |
| `npc-names.json` (NPCs' names: quest givers, merchants and the like, which the mob lists don't have) | the same bundle's NPCs | none: used without a licence |
| `pet-spawns.json` (which mob types give which pet's souls, and where each pet is found, for the pet tracker) | [Aion DPS Meter](https://github.com/SkeeveAN/Aion-DPS-Meter) by SkeeveAN (`Client/assets/aion2/pets/`) | MIT (`LICENSE-MIT-AionDPSMeter.txt`) |
| `field-bosses.json` (world-map field boss IDs to map and mob type) | [NOIA2](https://github.com/ZDYoung0519/NOIA2) (`field_boss_timer.rs`) | GPL-3.0 |
| `field-bosses.json` (field bosses' level and location) | the same bundle's monsters | none: used without a licence |
| `Icon_Item_Usable_*.png` and `item-icons.json` (icons for scroll, potion and food buffs) | NCSOFT's game asset server (`assets.playnccdn.com`), matched to skills by the bundle's item names | NCSOFT's artwork, used without a licence |

The NOIA2 and RATmeter data stays under their GPL-3.0 licence (the release includes
`LICENSE-GPL-3.0.txt`); the exact upstream commits are in `scripts/gamedata-pins.json` and
in each file's `source` field. `skill-details.json` was scraped from aion2.app, whose
terms don't allow that without permission; it's here at the maintainer's own risk and
will be removed on request from aion2.app or NCSOFT. `theostones.json` and `dungeons.json`
hold only the Theostone entries and the dungeon mobs (and `pets.json` the pets' names, `npc-names.json` the NPCs' names) of a larger scraped bundle whose
sources aren't named; they're here on the same terms and will be removed on request.

## Updating

- Names and icons: `packet-lab update-data --pins scripts/gamedata-pins.json --out gamedata`
  (add `--bump` to move to the latest upstream commits, `--lang ko|zh-TW|zh-CN` for other
  skill languages), check the meter still names skills and mobs, then commit the folder.
- Skill details: `packet-lab skill-details <scrape.json> --scraped <date>` from a fresh
  scrape of aion2.app's skill database.
- Theostones: `packet-lab theostones <items.json> --scraped <date>` from a scraped item
  database with in-game descriptions. Theostones missing from `skills.json` are skipped.
- Consumable icons: `packet-lab item-icons <items.json> --fetched <date>` downloads the
  icons of consumables whose buffs are named in `skills.json` but have no icon (64 px).
- Dungeons: `packet-lab dungeons <monsters.json> <dungeons.json> --scraped <date>` from the
  same kind of scrape (monsters with their dungeon and mode, and the dungeon list).
- Pets: `packet-lab pets <pets.json> --scraped <date>` from the same kind of scrape.
- NPC names: `packet-lab npc-names <npcs.json> --scraped <date>` from the same kind of
  scrape. They only name types the mob lists in `npcs.json` don't.
- Pet spawns: `packet-lab pet-spawns <pets.json> <regions.json> --commit <sha>` from Aion DPS
  Meter's `Client/assets/aion2/pets/` at that commit.
