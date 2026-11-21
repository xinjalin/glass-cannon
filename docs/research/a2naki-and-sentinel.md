# A2Naki and Sentinel: what their boss warnings know

Research notes, 2026-11-21. A lead report only: nothing here is in the code, the game data
or any other doc. The last section lists proposals; none is applied.

The question: two more tools say they warn of boss mechanics. A2Naki Companion 1.0.6
(a2naki.com) and AION Sentinel 0.6.13 (a GitHub repository with a project page). Does either
have boss casts mapped that would fill the gaps in `docs/research/boss-casts.md`?

**The short answer: A2Naki yes, and more than we have; Sentinel no.**

- **A2Naki** reads the game's traffic as we do and has calls written per boss for eight
  dungeons and the Ludra raid, Rahu included. Its rules are inside the program, which its
  licence forbids taking apart, so they were not read. What it publishes is still useful:
  its changelog names the mechanics it calls, and the fight logs players upload to its site
  carry each fight's boss casts with their skill IDs.
- **Sentinel** reads the screen (it recognises the cast's name in the picture). It has no
  cast mapped for the Ludra raid and says so, its program is not in the download, and the
  public mechanics file it builds on has no skill IDs.

## What was read, and what was not

All on 2026-11-21. Neither program was run; no installer was unpacked.

| source | what was read | limits |
| --- | --- | --- |
| `A2Naki-1.0.6.zip` (the user's download) | `README.txt`, `LICENSE.txt`, `THIRD-PARTY-NOTICES.txt`, `BUILD.txt`, and `CHANGELOG.txt` (1,100 lines): versions 1.0.2 to 1.0.6 in full, the rest searched for the raid's bosses and for skill IDs | `A2Naki-Setup.exe` (101 MB) was **not opened**: the licence forbids reverse engineering it and copying or extracting its data files (boss rules, timings, schedules) |
| a2naki.com | `robots.txt`, the terms, the boss mod page, the boss mechanics page, the Abyssal Forge guide, the raids page, and one public fight log (a Rahu kill) with its Casts tab | Read as a visitor would. The terms forbid scraping or bulk-downloading the guides or boss data, so nothing was fetched in bulk: one log, to see what a log holds |
| `Aion-2-Sentinel-DBM-and-Damage-Meter--0.6.13.zip` (the user's download) | The distribution notes, boss warnings, cast timing, encounter guides and research, coverage and the project page in full; all 58 documents searched for skill IDs and the raid's bosses | It is the repository's documents only. The program is a separate 95 MB archive that was not downloaded |
| `github.com/SkeeveAN/Aion-DPS-Meter` (MIT), which Sentinel names as its mechanics source | `Backend/src/data/aion2/mechanics.json` and `bosses.json`, with `gh` | Only those two files |

Confidence, as in the other reports: **confirmed** (agrees with our capture notes or the
app's own record), **plausible** (fits, not proven), **unchecked** (nothing of ours to hold
it against).

## A2Naki Companion

A Windows tray app (.NET 8, Npcap through SharpPcap) with a browser page, an overlay and
widgets for a Corsair screen. Freeware, closed source, by one author.

### What its boss mod does

From its readme, changelog and boss mod page:

- **Calls written per boss**, each tied to the cast that starts the mechanic, for
  Exploration and Conquest, or per stage for Shattered Arkanis and Deus Research Base.
  Spoken by the Windows voice and shown as one warning line with a countdown.
- **Early warnings** where a mechanic can be seen coming from a cast, an effect on a player
  or an HP mark ("Stagger soon", "Stop DPS soon").
- **Learned in the fight**, on a boss with no written calls: group hits, adds, debuffs to
  cleanse and casts worth interrupting, remembered for the next pull.
- **Count boss casts**: a learning mode. Each spell gets a number the first time it is cast,
  and the voice says that number and the last two digits of the spell's ID every time it
  comes back. A player then learns which number is which move.
- **Kinds of call**, each with its own switch: interrupt, stagger, wipe check, group damage,
  targeted, adds, cleanse, take cover, slam, fire walls, stop DPS, enrage, orbs.
- **Enrage**: a call when the projected kill comes after the enrage timer.
- **Interrupt** warnings only for casts of 2 s or more.

### What it calls in the Ludra raid

From the changelog (1.0.4 to 1.0.6). No IDs are published; these are the mechanics it has
rules for.

| boss | calls it names | ours |
| --- | --- | --- |
| Isolation Specialist Rahu | the bomb (and "You have the bomb!"), Feeding Time ("Kill the Predators"), the eye line, the stack marker, the wheel, the second black hole, the enrage | Two cards (`ludra.rahu.wheel`, `ludra.rahu.feeding`) and one warned cast (1802730). No capture |
| Abyssal Wings Ketu | the Gazes, Eruption of Power, the orbs. "Break your crystal!" comes just before the Eruption hits, not at the cast | Cards for all three. Ours also found that breaking a stone protects for 3.5 s, which is why the call has to come late (**confirmed**) |
| Eternal Ludra | the orbs, the Bingo, the colours, and the eye: the call names the player it marks. 1.0.6 stopped a repeated "targeted" call for a mark that does almost no damage | Orbs and the Trial: yes. The colours: never seen (below 42%). The eye: **not found** in ours |

Two things in that table are new to us:

- **Ludra's eye marks a player in a way the traffic shows**, since their call names the
  player. Our notes could not find it and guessed at effect 12000064. (unchecked)
- **A mark on Ludra that does almost no damage**: this fits our 1803260 (five marked spots
  at a player, 27 casts, never damaged anyone). (plausible)

### What its public fight logs hold

Players who connect an account upload their boss fights, and each log has a page on the
site. One Rahu kill was opened (Conquest, EU, 290 s, uploaded by app 1.0.6):

- **The page's Casts tab** shows the boss's casts on a time axis, named by the game's own
  names: 66 "Attack", one "Shackles of Pain" at 1:47 and one "Feeding Time" at 3:38.
- **The log's data, which the page loads, has more than the page shows**: every boss cast
  with its skill ID, the second it began and, for some, a kind ("targeted", "aoe"); the
  fight's events (source, target, skill, amount: 25,629 of them); the mobs that appeared, by
  type; deaths; and the party's positions.

So a public log is close to what one of our own captures gives for a fight, without the raw
messages. For Rahu, where we have nothing, that one log lists 23 cast IDs:

| cast | seen (s from the pull) | their kind | our data |
| --- | --- | --- | --- |
| 1802760 | 3 | | |
| 1802690 | 5, 12, 20, 63, 74, 159 (10 in all) | | |
| 1802730 | 9, 70, 168 | targeted | Our warned cast: 17 of 76 deaths on 6 Nov, a 1.5 s cast |
| 1802740 | 16 | | |
| 1802805 | 23, 90, 117, 141, 200, 211 (7) | aoe | |
| 1802880 | 31, 126, 208 | | |
| 1802710 | 33 | targeted | |
| 1802800 | 38, 55, 67, 129, 153, 197 (8) | | |
| 1802820 | 42, 78, 173, 239 | | |
| 1802770, 1802780, 1802790 | 46 / 48 / 50, 82 / 84 / 85, 177 / 179 / 181 | | Always the three in a row, two seconds apart |
| 1802715 | 55 | | |
| 1802750 | 59, 256 | | |
| 1802900 | 107 | | "Wheel of Torment" in our data; they show it as "Shackles of Pain" (the same skill, another translation) |
| 1802840 | 111, 164, 265 | | |
| 1802700 | 114, 139, 268 | | |
| 1802810 | 135, 188, 249, 277 | | |
| 1802860 | 147 | | |
| 1802980 | 219 | aoe | Feeding Time |
| 1802981 | 224, 229, 235 | | Feeding Time (the second ID) |
| 1802995, 1802997 | 287 | | At the kill |

This is one group's kill, read once, as a lead (**unchecked**: we have no Rahu capture to
hold it against). It agrees with the little we had: 1802730 exists and is aimed at a
player, 1802900 comes once after the first stretch of the fight, Feeding Time comes late
and once. 1802680 (Refracting Spear) and 1802720 (Soul Strike), which our game data names,
were not cast in it.

The mobs in that log, by type: Rahu 2301006, and 2920718, 2920719, 2920722, 2920723,
2920724 and 2920744. Which is the Abyss Predator and which the black hole is not worked out.

Eternal Ludra has ranked kills on the same site, so logs of her whole fight, below 42%
included, exist there too. None was opened.

### Its other dungeons

Its boss mechanics page gives one line per wipe mechanic with the game's skill name where
there is one. Held against our data by name:

| their line | our data |
| --- | --- |
| Ultimate Berk: stones, then Wave of Fury | 1800050 to 1800053 |
| Ultimate Berk: Extinction for Eternity on a failed stagger check | 1800140, 1800141 |
| Transcendent Bakarma: Wave of Death | 1800640, 1800655, 1800656 |
| Kromede's Desire: Blazing Judgment, the fire walls | 1801180 by name. The Kinah Ledger report guessed the walls were our unnamed 16.5 s cast 1801185: the name puts them five IDs apart, in the same block (**plausible**) |
| Divine Auldor: Summon Whirlwind, then Storm Strike on the marked player | Summon Whirlwind 1601010 to 1601020; no "Storm Strike" in ours (our card calls it Tempest Strike) |
| Atiel: Changing World, the stagger check | Not in our data under that name |
| Phantasmal Lakshmi: Reflect; Predator Saraswati: Orb Summon | Not in our data under those names |

The per-dungeon guide pages were not read: the Kinah Ledger report covers the same bosses
from a fuller guide.

## Sentinel

A Python and Qt app by two authors. The repository publishes documents and release
manifests; the program's source is deliberately left out, and the documents say plainly
that no gameplay validation has been done.

- **It reads the screen, not the traffic.** Warnings fire when its text recognition sees a
  known skill name twice in the cast area under the target's name. A cast-end countdown
  comes from reading the cast bar or its seconds. Its damage meter is a separate part and,
  in its own words, gives no boss cast events.
- **Its skill list is from the game's files**: which mob owns which skills, by English
  name. 550 skill IDs across 209 mobs, 859 rules. Skills only named "Attack" are dropped, so
  nearly all of the Ludra raid's casts could never fire a rule.
- **The Ludra raid is switched off.** Its guide answers for the raid came from early
  access, and it keeps them out of the current rules. It has three HP hints for the Central
  Abyss Fusion Reactor (90%, 60%, 30%), from the dungeon's files: those fit our record of
  the Auxiliary Reactors appearing at 56% and 27% only loosely (**unchecked**).
- **Its mechanics come from other public sources**: SkeeveAN's Aion-DPS-Meter
  (`mechanics.json`, MIT), Aion 2 Maps' guides, and A2Naki's dungeon guides.
- **One fact worth having**: the game's files give each skill a cast time and a
  range-warning lead. Their example is Bakarma's Wave of Death, 1800655, cast time 18000,
  which matches the 18 s other guides give for that check. If the same files have Rahu's
  skills, their cast times could be read without a capture. Our `boss-skills.json` has
  names only. (unchecked)

### The mechanics file it builds on

`mechanics.json` in SkeeveAN's Aion-DPS-Meter (MIT, marked "facts only"): 263 entries, each
a boss, a short key for the mechanic and a severity (169 ordinary, 74 avoidable wipes, 20
wipes). 54 of them carry an HP percentage; the other 209 have no trigger at all. No skill
IDs, no text on what to do, and nothing for the Ludra raid (its `bosses.json` lists the
raid's bosses by name and type only). It covers 19 instances by its own keys (among them
`CradleNihility`, `BlueBreathIsle`, `DaevaCitadel`, `SunkenTemple` and `MirrorHeart`); whether
we have cards for those was not checked.

## Licence and reuse

- **A2Naki, the program**: freeware, all rights reserved. Its licence forbids modifying it,
  reverse engineering it, and copying, extracting or republishing its data files, naming
  boss rules and timings. That is why the installer stayed closed.
- **A2Naki, the site**: free to read, link and quote in short parts with a link back. It
  forbids scraping, crawling or bulk-downloading the guides or the boss data, and
  republishing the data without written permission. Its `robots.txt` lets crawlers read the
  pages and the public fight reads. What players upload stays theirs.
- **Sentinel**: the repository grants no licence for the program. Its documents can be read;
  there is nothing in them to copy.
- **Aion-DPS-Meter**: MIT, so its files could be used with the notice kept. There is little
  in `mechanics.json` to use.
- **Against our rule** (leads only, confirm on our own captures, copy nothing): the rule
  already fits all three. The table of Rahu's casts above is from one public log and is in
  this report as a lead with its source named; it should not go into the game data on that
  basis. Building a table from many logs would be bulk use of their site, which their terms
  rule out without permission.

## Proposals

Proposals only. No code, game data or other doc was changed.

1. **Take nothing from either tool into the project.** A2Naki's rules are closed and
   protected; Sentinel has none we lack.
2. **Rahu: record his side once.** The public log shows what to expect (23 casts, the
   three-cast chain every 35 to 95 s, the wheel near 1:47, Feeding Time near 3:38), so one
   capture from Group 1 would be quick to label. (unchecked)
3. **Or ask.** A2Naki's terms invite a request for more than a short quote
   (contact@a2naki.com, or their Discord). Permission to study uploaded logs of Rahu and of
   Ludra kills would cover both of our gaps at once. This is the user's call.
4. **Look for Ludra's eye mark in our captures again**, as an effect on one player followed
   by a hit on them, now that we know another tool finds it in the traffic. (unchecked)
5. **Ideas of theirs worth building our own way**: a "count boss casts" learning mode; a
   call timed to when a mechanic lands rather than when it is cast (Ketu's crystal); naming
   the marked player in a warning; a warning when the kill would come after the enrage;
   learning group hits, adds and interruptible casts on a boss with no cards.
6. **Check whether our game data source has cast times and range-warning leads per skill.**
   If it does, Rahu's and Ludra's later casts get cast times with no capture. (unchecked)
7. **Naming**: 1802900 is "Wheel of Torment" in our `boss-skills.json`, "Pain Bridle" in our
   `skills.json` and "Shackles of Pain" on their site. Worth one look at which the game shows
   now.

## Summary

A2Naki has the widest boss coverage of any tool looked at so far, including Rahu, but its
rules cannot be read under its licence. Its public fight logs do carry boss casts with skill
IDs, and one Rahu log gave us his cast list as a lead. Sentinel reads the screen and has
nothing mapped that we lack. Worth following up: a capture from Rahu's side (or permission
to study their logs), another look for Ludra's eye mark, and cast times from the game files.
