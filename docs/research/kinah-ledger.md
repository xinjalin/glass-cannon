# Kinah Ledger: what it knows about boss casts

Research notes, 2026-11-21. A lead report only: nothing here is in the code, the game data
or any other doc. The last section lists proposals; none is applied.

The question: Kinah Ledger is a fan site (kinahledger.com) with dungeon and raid guides, and
a Windows tracker with a DPS meter whose source is on GitHub. Does either hold boss cast
data (skill IDs, names, cast times, timelines, what to do) that would fill the gaps in
`docs/research/boss-casts.md`?

**The short answer: no cast IDs anywhere.** The site's boss pages are hand-written guides
with no skill ID on any of them and no data file behind them. The tracker has no boss cast
detection, no mechanic data, no cast timers and no warnings, and it deliberately never names
a boss's skill. Its network decoder is not in the repository. What is useful is second-hand:
what to do per mechanic, HP thresholds and a few durations, for bosses we have thin or no
cards for, and one remark on their Horn Den page that led to settling an "owner unknown" row
of ours from our own record.

## What was read, and when

All on 2026-11-21.

| source | what was read | limits |
| --- | --- | --- |
| `kinahledger.com/dungeons/abyssal-forge-ludra` (the user's link; the four boss sections, which the page hides behind tabs, read from the page's own HTML) | Every boss section, the wipe list, the Hard mode table, the embedded JSON (`ilp-data`: loot tooltips only), the page's network requests | Page says "Checked 18 November 2026" |
| `kinahledger.com/guides/aion-2-ludra-raid` | The longer raid guide | "Checked 15 November 2026". Same mechanics as the dungeon page, almost line for line |
| The other 95 pages in `sitemaps/dungeons.xml` | Fetched all; 11 have boss sections (listed below), the other 85 are loot and monster lists | None has a seven-digit skill ID anywhere in its text |
| `/monsters/isolation-specialist-rahu`, `/monsters/abyssal-wings-ketu`, `/monsters/central-abyss-fusion-reactor` | Level, rank, instance, loot | No skills. `/monsters/eternal-ludra` is a 404 |
| `/about/`, `/privacy/`, `/tracker/dps-meter`, `robots.txt`, the sitemaps, the page scripts (`guide.js`, `kit.js`, `kl-app.js`, `ilp.js`) for what they fetch | Where their data comes from, what the meter does, what the scripts load | No terms page (`/terms/` is a 404) |
| `github.com/StevenMacchia/kinah-ledger` at commit `2b1df60` (2026-11-21), read with `gh` and a shallow partial clone; nothing built or run | `LICENSE`, `README.md`, `data/bosses.txt`, `data/skills.json`, `data/mobs.txt`, `tools/skill-fill.md`, `Packets/BossEvents.cs`, `Packets/Decoder.cs`, `Parser/BossBook.cs`, `Parser/BossParser.cs`, `Parser/Abilities.cs`, `Parser/PullTimeline.cs`, `Ui/ParserTimeline.cs`, `Session/BossTimers.cs`, the two workflows, all 271 release notes (to v1.0.41, searched for casts, mechanics, warnings, timers) | The decoder (`klnet.dll`) is built from a private repository and is not in this one |

Reading the site: nothing blocked the in-app browser and no page asked for a sign-in. Their
`robots.txt` bars `/api/`, `/auth/`, `/admin/`, `/v1/`, `/zones/`, `/prices/`, `/pets/` and
`/pulse.json` for every crawler, and the whole site for AI training crawlers; it says in a
comment that agents reading on a person's behalf may read the rest. None of the barred paths
was fetched for this report. The dungeon page itself loads `/zones/all.json` (players' run
times and loot for the "Players' runs" panel); its body was not read. The scripts also call
`/api/item/<id>` (item tooltips) and `/assets/search.json` (site search). None of these is
about bosses.

Confidence, as used below:

- **confirmed**: checked against our own capture notes (`boss-casts.md`) or the app's own
  record (`%APPDATA%\io.github.xinjalin.glass-cannon\boss-casts.json`), and they agree.
- **plausible**: fits what our notes show, but our notes don't prove it.
- **unchecked**: we have nothing to hold it against.

## 1. What they have about boss casts

**On the site** (`/dungeons/<slug>`, built into the HTML; no JSON or API behind the boss
sections):

| piece | do they have it | where | how complete |
| --- | --- | --- | --- |
| Skill IDs | No | — | Not one on any of the 96 dungeon pages or the monster pages |
| Move names | Their own short names ("Red strip", "Laser walls", "Bingo"), and the game's skill name where a boss shouts it or the files name it (Tempest Strike, Surging Thorn, Lodestone Prison, Thunder Cry, Soul Caller, Blue Light of Destruction, the six of Divided Aponos) | Boss sections | 11 dungeons: Abyssal Forge: Ludra, Urugugu Canyon, Vakron Sky Island, Krao Cave, Draupnir, Fire Temple, Ferocious Horn Den, Deus Research Base, Shattered Arkanis, Nightmare Altar, Sanctum of Loathing. The other expeditions and the two other raids (Chalice of Muspel, Corroded Decontamination Facility) have boss names only |
| Cast times | A handful, in prose: six for Divided Aponos, Auldor's 10 s Tempest Strike, Kromede's 16.5 s flame walls, a few "N seconds to break it" | Boss sections | Sparse. None for the Ludra raid |
| Cooldowns, timelines | No. Only HP thresholds per phase ("at 75%", "50 → 25%") and enrage timers | Boss sections | Thresholds for every boss they cover; several are given as ranges because their sources disagree |
| Damage, shape | Shape in words (circle, cross, cone, ring, strip, fan). Damage rarely, as a share of HP (Bakarma's wave, Kromede's wall, Vakron's prison blast) | Boss sections | Loose |
| What to do | Yes: this is what the pages are. One line per move, and three numbered steps per wipe check, with notes per role | Boss sections | Good for the 11 dungeons; marked "One source" where only one of their sources says it |

**In the repository:**

| piece | do they have it | where |
| --- | --- | --- |
| Boss list | Boss names per instance in route order, with the final boss marked, a "wave" stage and bosses fought as one pull | `data/bosses.txt` (20 instances, 60 bosses; names only, no mob type IDs). It leaves the Central Abyss Fusion Reactor out on purpose ("unsure") |
| Skill names | 538 player skills (class skills, spirits, Theostones). No boss skill | `data/skills.json`, built by `tools/skill-fill.js` |
| Boss skill names | None, by design: a comment in `Ui/ParserTimeline.cs` says a boss's skill is never named from the list, because monster skill IDs can collide with player skill IDs. A hit a player took is shown by its attacker's name | `Ui/ParserTimeline.cs`, `Parser/Abilities.cs` |
| Opcodes, message layouts | Not in the source. The decoder is a native library (`klnet.dll`) built from a private repository and shipped only inside the release; the open code sees its output as events (instance entered and left, boss engaged and ended, group, damage, boss HP, deaths, hits, heals, members' casts, effects on members) | `Packets/Decoder.cs`, `Packets/BossEvents.cs`, `.github/workflows/release.yml` |
| Boss casts, cast times, marked ground | Not read at all. Their cast event is a member's cast (for the player's own timeline); there is no event for a boss's cast, a cast bar or ground marks | `Packets/BossEvents.cs` |
| Mechanic timers, warnings | None. "Boss timers" there are field boss respawn timers (read from the game's own list while the world map is open) and scheduled events | `Session/BossTimers.cs`, release notes |

## 2. Where it comes from

- **The boss guides are hand-written from other guides and videos.** Each page lists its
  sources at the bottom and says it was written in their own words from them. For the Ludra
  raid: MetaBot.GG (game data), Aion2 Tier List, AION 2 Atlas, Scaryel Codex, Keymap,
  ExpCarry, AllThings.how, MetaRoad, Aion 2 Meta, Aion2T, NCSOFT's patch notes of 4, 11 and
  18 November, and YouTube runs (FRESHY, trueeevil, Maddoshi, NoBS Game Guides for Rahu and
  Ketu on global, PhonkTV, gigawhale for Hard). The raid page says itself that the fight
  details come from Advance Access and Korean runs and may differ on live.
- **Game-file facts come through MetaBot.GG**: monsters, levels, loot, dungeon lists, and
  the skill names and cast times on the Nightmare Altar page (their sources there link
  MetaBot's page for Divided Aponos). Their `/about/` page says "the game files" via MetaBot.
- **Nothing on the boss pages comes from their own captures.** The tracker records damage,
  deaths and HP, not casts, and the pages cite no tracker data for mechanics (only for clear
  times and loot).
- **Player skill names**: MetaBot's skill pages plus three MIT-licensed tables from other
  meters (AcompanION, pingmaker, waffle_meter), each name checked against a fourth.

## 3. Against ours: Abyssal Forge: Ludra

They have no IDs, so the rows go by name and order. "Ours" is `boss-casts.md` and
`gamedata/boss-mechanics.json`.

### Central Abyss Fusion Reactor (2301008)

| their entry | ours | verdict | confidence |
| --- | --- | --- | --- |
| Side reactors shield the core; send two players up the updraft | Auxiliary Reactors 2301012 (at 56%) and 2301013 (at 27%) appear; the same from our guides | Agree. Nothing new | confirmed |
| Floor marks: a circle, then a cross | Nine casts 1802510 to 1802580, shapes unknown; 1802520 perhaps an inner circle, 1802555 perhaps an outer ring | Same as our other guides. The order (circle first) is the only extra, and it does not say which ID | unchecked |
| Packs keep arriving | Waves of Ludra Followers 2301001 to 2301004 | Agree | confirmed |

Their meter keeps no Reactor fights (left off their boss list as "unsure").

### Isolation Specialist Rahu (2301006)

We have no capture, so everything here is a guide against our guides.

| their entry | ours | verdict | confidence |
| --- | --- | --- | --- |
| Red strip on one player: aim it away from the group | The eye mark with a line attack (U4N, Mobalytics) | Agree | unchecked |
| Rings grow around the group: stack so they overlap | Ring attacks to stack for | Agree | unchecked |
| Everyone flung to the edge: regroup, drop puddles at the rim | 1802900 Wheel of Torment; card `ludra.rahu.wheel` | Agree | unchecked |
| Bomb pass: a timer on one player all fight; pass it by standing in the carrier's ring; a missed pass leaves a black hole; three black holes wipe | The SOS Bomb (20 s, three failures wipe) | Agree. New: the failure is a black hole that stays, which is why three wipe | unchecked |
| Arena split into four quarters at about 50%; crossing a line hurts badly | The arena cut in four at about 50% | Agree | unchecked |
| Feeders at about 25%: marked players aim the line through the adds; an add left alive leaves a black hole (counting towards the three) | 1802980, 1802981 Feeding Time; card `ludra.rahu.feeding` | Agree. New: the 25% threshold, and that a missed add counts as a black hole | unchecked |
| Phases 100 → 50 → 25 → 0% | No thresholds in ours | New to us | unchecked |

Nothing here says which ID is which. 1802680, 1802720 and 1802730 stay unplaced.

### Abyssal Wings Ketu (2301007)

| their entry | ours | verdict | confidence |
| --- | --- | --- | --- |
| Charge and slam: one player is charged, then a ring sweeps; jump the ring | The 1801850 chain: the mark (effect 18021302), the jump onto the marked player (1801930), then 1801932 (46 times, nothing) or 1801933 with 1801931 (3 times, damage on up to 5) | Agree on the charge. Their ring to jump supports our guess that 1801933 and 1801931 are the ring when the landing is not blocked, but does not prove it | plausible |
| Laser walls: a laser, then spirits and turning walls; dodge, kill the spirits, gather | Abyssal Gaze's lead-in: 1801950, spirits 2920674 and 2920675, Ketu's double (1801887) marking 20 spots every 1.25 s; end in the middle | Agree. "Turning walls" is a description of the double's lasers we did not have | plausible |
| Sigil heal: a white ring closes on the middle; the healer stands on it and heals; the group needs a healer for this | The end of the Gaze: 1801892 and 1801966 tick on nearly everyone (12,600 seven times) | New to us: it explains why those ticks cannot be avoided and why "end in the middle" | plausible |
| Falling orbs: two orbs, one catcher each | 1801940: two orbs; catching one gives effect 18019421 for 30 s | Agree. "One each" fits a 30 s effect on the catcher | confirmed for the orbs; plausible for "one each" |
| Red and blue eyes from about 50%: red, face it; blue, turn away | 1801968 red (face it), 1801967 blue (turn away); from the second Gaze at 59 to 63% and after 1801961 from 53% | Agree | confirmed (colours "likely" in ours) |
| Soul Stones: a late burst that cannot be dodged; group so the fans overlap, break a stone, a stone covers only the player who breaks it, leave one for the healer | Eruption of Power at 29 to 34%: 16 stones, five fans (1801971) one per player, effect 18018924 for 3.5 s on the breaker | Agree, down to the fans. Nothing about a stagger: supports dropping "stagger them" from `ludra.ketu.eruption` (already proposal 5 in `boss-casts.md`) | confirmed |
| Phases 100 → 50 → 0% | Our order of the fight, by HP | Theirs is coarser | — |

They have no entry for the scythe spin (1801840 to 1801843), the chasing scythes (1801830),
the laser line outside the Gaze (1801950) or the 6.5 s blast (1801961).

### Eternal Ludra (2301014)

| their entry | ours | verdict | confidence |
| --- | --- | --- | --- |
| Red orb: take it with a defence up | 1803200: the red orb, the tank blocking; 1803204 wipes if it goes off | Agree | confirmed |
| Blue orbs: anyone jumps up and pops them | 1803200, 1803210 | Agree | confirmed |
| Tail swipe at the rear: knocks down and bleeds | 1803080: the arc behind her, 3 s knock-down and a bleed | Agree | confirmed |
| Breath: a line from the front; the tank aims it away | 1803160 is three bands of blasts crossing the arena from a side that changes; the frontal cones are 1803050 and 1803150 | Differ. Either their "breath" is our frontal cone, or their sources describe the older version. Ours is from positions | — |
| Puddles: a stomp, then one under each player; spread, then move off | 1803140 then 1803150, with fields under two players (1803155) | Agree in kind. Ours counted two players, not each | plausible |
| Bingo at about 50%; only one tile is safe; opening tiles finds it; he keeps attacking; surviving it leaves him groggy | Ludra's Trial 1803010 at 59 to 55% in 15 of 15 records; 1803012 reveals tiles; 1803110 twice; staggered about 20 s after | Agree, but the threshold is ours: 55 to 59%, not 50% | confirmed |
| Phase 2, 50 → 25%: red and blue players keep to their own sides; mixed colours close together hurt each other | Never seen. Our other guides put the colours below 42% | Their placement is looser than our guides' | unchecked |
| Phase 2: line attacks sweep across one after another; step out of each lane | Candidates: 1803270 to 1803273 (four variants, hit no one), 1803380, 1803390 (the Zikel sequence: lasers from four sides) | A name for one of our phase 2 guesses, but not which | unchecked |
| Phase 2: the outer floor burns away; stay close to the boss (marked "One source") | Nothing in ours | New to us. A candidate for 1803360 or the helper at the centre after 1803000 | unchecked |
| Bingo 2 at about 25%, with a sweeping wave on top | 1803040, 1803041 "Ludra's Trial" in the game data, never seen | Agree with our other guides; the wave is an extra detail | unchecked |
| Colour circles: a late breath kills anyone not in a circle of their colour; one player to a circle; step in late or the shield is wasted | "The coloured safe spots to enter in the last second" | Agree. New: one player per circle | unchecked |
| Berserk at 10:00 on Normal (no figure for Hard); 2 revives a run; 2 hours a run | No enrage in ours | New to us | unchecked |

They have no entry for the cleave, the double hit (1803120), the lightning on every player
(1803250, our top killer in phase 1), the heavy strike (1803110), the tornadoes (1803240),
the arena-wide attack (1803230), the seven fields (1803170) or the marked spots (1803220,
1803225). Our phase 1 is far more detailed than theirs.

### What this settles under "Seen in captures, still unidentified"

Nothing in the raid. Row by row:

| our open item | does Kinah Ledger settle it |
| --- | --- |
| Ludra 1803090 / 1803100 | No |
| Ludra 1803221 to 1803224 | No |
| The guides' eye attack on Ludra | No. Their Ludra section has no eye attack at all; the marked-player line is Rahu's on their pages |
| Ludra 1803230's safe place | No |
| Ludra's phase 2 IDs | No IDs. Two names to hold against them: "line attacks" and "burning floor" |
| Ketu 1801964 against 1801965 | No more than our guides (red: face it; blue: turn away) |
| Ketu 1801841 against 1801842 | No entry |
| Ketu 1801932 against 1801933 | Weak support for the ring reading (above) |
| Ketu 1801820, 1801961 | No entry |
| The Reactor's shapes | No |
| Rahu, everything | No IDs. Thresholds (50%, 25%) and the black hole rule, as leads |
| Ludra below 42% | The same three mechanics our other guides have, plus the 10:00 berserk |

**One row outside the raid is settled, by our own record.** Their Ferocious Horn Den page
says the game files call Nuakum's 30% black hole "Blue Light of Destruction". That is our
1802153, listed in `boss-casts.md` under "In the game data, never seen in a capture" with
"1802000 to 1802160 (21 IDs): owner unknown". The app's own `boss-casts.json` has Ferocious
Horn Nuakum (2320910) casting 27 IDs from 1802000 to 1802160, among them 1802131 (5.5 s),
1802141 (5 s) and 1802154 (3 s); and `boss-mechanics.json` already ties effect 18020003 to
Nuakum's bleed. So that block is Nuakum's (**confirmed**), and the effects Ketu's mark puts
on players (18021302, 18020008) are borrowed from Nuakum's skills, as that row guessed.
That 1802153 / 1802154 is the black hole is **plausible** (name and neighbouring ID; no
capture of it on disk).

## 4. The other bosses: what they have that we lack

Our cards are in `gamedata/boss-mechanics.json`. All of the following is guide text
(**unchecked** unless a row says otherwise), so each needs a run of ours before use.

| boss | what they have that we don't | our own record beside it |
| --- | --- | --- |
| Raur (Urugugu) | Counter stance comes at about 70% and 30%; a stagger drops the shield (one source); an unmarked shield rush that stuns | Card `urugugu.raur.counter` has no thresholds |
| Urahum | A three-step stomp, the last the widest; faster from about 65% | Poison cards only |
| Auldor | Tempest Strike at 75% and 30%: a 10 s cast with the circle for the last 5 s; the rim burns after the second; enrage 5:00 (Hard 4:00); a tornado bind of 3 s | Card `urugugu.auldor.tempest` lands at 10.75 s: agrees, but that card is itself an unconfirmed lead (**plausible**) |
| Tiere (Vakron Sky Island) | Bloom at 75% and about 25%; strips behind her with 1 s of warning; unmarked half-circle cleaves on each side | Cards for the flower, the step together and the pools; no thresholds |
| Thamon | A wide slam that hurts petrified players; a grab | Shield and petrify cards agree with theirs (red: physical stops; purple: magic stops) |
| Vakron | Rings at about 60% and 40%, 1.7 s apart, a ring hit burns 20 s, the blast after them stuns 2.5 s; the prison at about 55% and 35%, 5.5 s to break out; recovery orbs in Exploration; enrage 5:00 | Our record: 1604425 Lodestone Prison is a 4.5 s cast. Their 5.5 s may count from the pull-in (1604420): differs by 1 s until checked |
| Gadioton, Harcon (Krao Cave) | Fire pools and a floor laser, a pull and stun below 35%; crossing lines of fire, twice | No cards |
| Ultimate Berk | Rock check at 75% and again between 35 and 25%; stagger check at 50% with about 9 s to break it; falling stones that stun 1 s; enrage 5:00 | `krao.berk.stagger` runs 25 s |
| Saraswati (Draupnir) | Poison puddles, a cone that turns you into a worm, eggs on the rim | No cards |
| Lakshmi | Reflect for 8 s; a healer add at 50% to kill first; a stagger also ends the reflect | `draupnir.lakshmi.reflect` runs 9 s: agrees (**plausible**) |
| Transcendent Bakarma | Wave checks at 70% and 30%: 18 s to break the bar, a wave hit takes 45% HP, the third kills; whirlpools on the second (and on the first in Hard) | `draupnir.bakarma.stagger` runs 25 s |
| Ignus (Fire Temple) | Fire pools at 75%, the blast 3 s after the mark; Blazing Speed at 65%; stand close for Burning Earth | Our record has Blazing Speed 1600810 (1 s cast) and Burning Earth 1600850 (3 s cast) |
| Murute | Great Earthquake from 50%: three rings reaching 8, 15 and 25 m, to block or dodge each | No cards. Our record: 1600720 Great Earthquake, 0.83 s cast |
| Kromede's Desire | Flame walls at about 70% and 40%, each row with one gap, the cast 16.5 s; a leap at the farthest player; a clone per player; enrage 5:00 | No cards. Our record: 1801185, an unnamed 16.5 s cast, and 1801191 at 15.1 s. The length agrees (**confirmed** for 16.5 s; that 1801185 is the walls is **plausible**) |
| Kwapo (Ferocious Horn Den) | Thunder Cry at 75% and 25%: pop every orb on one side, then jump the ring (on the second howl in Conquest) | Card `hornden.kwapo.thunder` agrees; no thresholds |
| Duanka | Souls at 75%, 50% and 15% with a 15 s countdown, killed only with the altar orb; Soul Caller at 65% and 30% with an 8 s alert | Cards agree; `hornden.duanka.soulcaller` runs 12 s |
| Nuakum | The Hunt at about 75% (twice) and 55% (three times): block the clone's dash, then a small circle with arrows in means stack and a big one with arrows out means leave, never the same blocker twice; the black hole at 30% (three of his four throws must hit the orb, a fear before the third); red balls at about 15%; enrage 5:00 in Conquest | Only the bleed card. Our record's unexplained long casts 1802131 (5.5 s), 1802141 (5 s) and 1802154 (3 s) are candidates for these |
| Gelcos (Deus Research Base) | A marked charge whose pools go to the rim; a pull-in whip; a slam with lines | No cards |
| Nazmun | Rotating lasers to jump; the white ring blasts 3 s after it appears; three time bombs with a gap | `deus.nazmun.bomb` runs 5 s |
| Atiel | Platform collapse at about 75% and 45%; clone charges 2 s after their strips; the stagger check at 20 to 30% with about 10 s; the bound player's hit lands in 3 s | `deus.atiel.morphic` lands at 10 s: agrees (**plausible**) |
| Siliator (Shattered Arkanis) | Two chakrams chase two players and vanish only when they collide; an eye mark to run out; returning blades at 60% | No cards. 11 casts in our record, all "Attack" or unnamed |
| Ruthilis | The floor locks quarter by quarter, clockwise from the door; a red floor that is safe only at his feet; scythe lines from 50% | Lightning card only |
| Talisra | The odd spear (break the pale one of four; hits count, not damage); orbs and shields at about 60 to 67% and again near 33% (two players per colour); enrage 5:00 | `arkanis.talisra.castfall` says "break the pillar she names": differs in wording. Our record has two unnamed 18 s casts (1801690, 1801696) that could be the orb checks |
| Divided Aponos (Nightmare Altar) | Six named casts with cast times, and what follows: Smite Ground stuns 2.5 s, Energy Cannon knocks down 3 s | Our record has all six by the game's names (1504240 to 1504305). Cast times agree on five of six (**confirmed**); Shadow Teleport is 0.5 s in ours against their 0.3 s |
| Duduri and Dodori (Sanctum of Loathing) | One pull for both; red floor marks; a stagger bar | Thin on both sides |

For the instances with no boss guide on their site (Abyssal Horn Den, Azure Breath Island,
Citadel of the Fallen Daeva, Cradle of Nihility, Dying Dramata's Nest, Hall of Illusion,
Mirror of Scarlet Desire, Submerged Life Temple, Chalice of Muspel, Corroded Decontamination
Facility, Snowfield of Sorrow) they have only the boss names in route order with the final
boss marked (`data/bosses.txt`). Whether our `gamedata/dungeons.json` already has the same
was not checked.

## 5. Licence and reuse

- **The repository is MIT** ("Copyright (c) 2026 Kinah Ledger authors"). MIT allows copying,
  changing, distributing and selling the code and the files in it, provided the copyright
  notice and the licence text stay with any substantial copy. That covers the tracker's
  source and `data/` (the boss list, the skill table). The skill table is itself compiled
  from MetaBot.GG and three MIT tables, each entry marked with its source.
- **The decoder is not open.** `klnet.dll` is built from a private repository
  (`kinah-ledger-packets`) and ships only inside the release. There is no licence to read it
  under and nothing to read; their installer and executable were not downloaded or run.
- **The site's pages are not in the repository and carry no licence.** There is no terms
  page. Each guide says it was written in their own words, and `robots.txt` bars AI training
  crawlers and the API paths. So the guide text is theirs by default: facts and mechanics
  can be learned from it, the wording and tables cannot be taken.
- **Against our rule** (leads only, confirm on our own captures, copy nothing): MIT would
  allow more than we do for the repository's code and data, with attribution. The site
  allows less than MIT, and our rule already fits it. Either way nothing in the repository
  is worth copying for boss casts, since it has none.

No text or table from their pages is reproduced here; the moves are named and described in
our words.

## 6. How their meter does what ours does

- **Which fights count.** The closed decoder announces a pull only for bosses on its own
  list. On top of that the open code keeps a whitelist of boss names per instance
  (`data/bosses.txt`), so trash, elites and named adds never make a fight; an instance not
  on the list falls back to whatever the game marks as a boss. Two bosses fought together
  are one pull; a wave room is a split of the run, never a pull.
- **Start.** At the pull event, moved to the first damage when that is more than 1 s away
  from it; damage in the 10 s before the pull is counted into it.
- **End.** A kill at the boss's death. A wipe when the game says the group fell. Where no
  end arrives, it infers one: damage, then none and no HP fall for 60 s, is a wipe dated at
  the last damage; the boss back at full HP with no damage for 5 s is a reset; nothing heard
  for 10 minutes is "left". A field boss going quiet is "left", never a wipe (the crowd
  fights on out of sight). A pull under 5 s that did not kill is dropped.
- **Lost traffic.** A gap, a reconnect or a restart never splits a fight: the record carries
  on and is marked partial with the reason.
- **Naming the place.** When the game enters an instance without a name, they name it from
  the map event's code (`FierceHornCave_Easy_...`); a `G` in place of the mode word marks a
  Transcendence, and they map its number to a stage only where play proved it (06 is stage
  3, 09 is stage 4).
- **Deaths.** A death row carries the killer and the killing skill; the recap shows the last
  hits taken with the killing blow last, by attacker name, never by boss skill name.
- **Effects.** They state as a rule what we inferred: an effect's ID is its skill's ID times
  100 plus a number (our 18021302 from 1802130).
- **Boss cast detection: none.** No boss casts, cast bars, ground marks, mechanic cards or
  warnings. This is where Glass Cannon is well ahead; there is nothing of theirs to compare.

## Proposals

Proposals only. No code, game data or other doc was changed.

1. **Take nothing from Kinah Ledger into the project.** It has no cast IDs, cast timelines
   or opcodes. Its use is as one more guide to hold beside the others.
2. **Correct the "owner unknown" row in `boss-casts.md`**: 1802000 to 1802160 are Ferocious
   Horn Nuakum's (our own `boss-casts.json`), 1802153 is in his range, and Ketu's mark
   borrows two of his effects. (confirmed)
3. **Nuakum has three mechanics we have no card for** (The Hunt, the black hole, the red
   balls) and three long casts in our record with no meaning yet (1802131, 1802141,
   1802154). One captured Conquest run of Ferocious Horn Den would pair them. (plausible)
4. **Kromede's flame walls**: our record's unnamed 16.5 s cast 1801185 matches their stated
   length. A capture of Fire Temple's last boss would confirm it and give a countdown card.
   (plausible)
5. **Card lengths to check on our next runs**, where their figure is much shorter than ours:
   Berk's stagger check (about 9 s against our 25 s), Bakarma's (18 s against 25 s),
   Duanka's Soul Caller (8 s against 12 s), Ignus's mark (3 s against 6 s), Vakron's prison
   (5.5 s against a 4.5 s cast in our record). (unchecked)
6. **Ludra raid, as leads for the next captures**: Rahu's thresholds (split at 50%, Feeders
   at 25%) and the black hole count; Ketu's sigil heal as the reading of 1801966 at the end
   of the Gaze; Ludra's phase 2 "line attacks" and "burning floor" as names to test against
   1803270 to 1803273, 1803380, 1803390 and 1803360; the 10:00 berserk on Normal, which
   would be a simple fight-clock card once seen. (unchecked)
7. **Their source list is a reading list we have not used**: Scaryel Codex, MetaRoad, Aion 2
   Meta, Aion2T, AION 2 Atlas and AllThings.how for the raid, and NoBS Game Guides' global
   videos of Rahu (4 Nov) and Ketu (5 Nov). A video of Rahu is the best thing short of a
   Group 1 capture for naming his moves; the IDs still need the capture. MetaBot.GG's
   monster pages appear to be where their cast times come from, and those agreed with our
   record for Aponos, so MetaBot is worth one look for Rahu's skills. (unchecked)

## Summary

Kinah Ledger has no boss cast IDs, timelines or cast detection: the site is a hand-written
guide built from other guides, and the tracker's decoder is closed and reads no boss casts.
Worth following up: fix our "owner unknown" row (1802000 to 1802160 are Nuakum's, by our own
record), capture Ferocious Horn Den and Fire Temple to pair Nuakum's and Kromede's unnamed
long casts with the mechanics their pages describe, and read the sources they cite for Rahu.
