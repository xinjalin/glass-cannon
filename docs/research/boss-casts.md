# Boss casts: which ability each cast ID is

Research notes, 2026-11-20. Nothing here is in the code yet: the last section lists
proposals, of which only the label changes (item 3) have been made, in
`gamedata/boss-mechanics.json`.

The question: when a boss casts, the meter sees a skill ID. For most of them the game's
text has no name (or only "Attack"), and one visible ability is often several IDs (a cast,
then damage under other IDs, often from invisible helper mobs). This report says, per cast
ID, what the player actually sees, as far as our evidence reaches.

## Evidence and how far it goes

| source | what it gave | limits |
| --- | --- | --- |
| The raid capture of 2026-11-20 (`glass-cannon-v0.34.0-windows-x64\captures\20261120-182454`, 169 min, map 600082) | Every message: casts, cast times, marked ground, hits, effects, spawns with positions, HP, stagger gauge. 1 Reactor kill, 12 Ketu attempts with the kill, 13 Eternal Ludra attempts (best 53.3%) | Ludra's phase 2 was reached once, for 30 s. No Rahu: the raid splits five and five, and this character was in Ketu's group. Positions: every other player and mob is tracked, each boss cast carries the boss's facing and target point, and each hit carries where its target stood; your own character's place is known only when it is hit |
| The app's saved fights (`%APPDATA%\io.github.xinjalin.glass-cannon\fights`): 11 Ludra fights of 13 Nov, 8 of 20 Nov | Boss casts, damage taken by skill and attacker type, deaths, boss HP. The only record of Ludra from 57% down to 42% (5 fights) | No helper mobs' casts, no spawns, no positions |
| The app's `boss-casts.json` | Which casts each boss type has ever made, with cast times | Counts only. No Rahu in it |
| The 23 other sessions under `Downloads\glass-cannon-test-*` and the release folders | Field bosses and a few short kills (last section but one) | No raid boss in any of them |
| `gamedata/skills.json`, `boss-skills.json`, `npcs.json` | Names, where there are any | Most of the raid's cast IDs have no entry at all |
| Guides, for names and what to do: MMOPIXEL's Ludra guide (10 Nov, the saved page), Mobalytics' Ludra guide (9 Nov), U4N's Ketu (7 Nov) and Rahu (6 Nov) guides, ExpCarry's raid guide (7 Nov; it says itself that it describes the older version) | The community's names, the order of phase 1, what to do | No skill IDs anywhere. Matching a name to an ID is ours, by order and by what the capture shows |
| Leads: GT Companion 0.5.111 has no warnings for this raid (earlier note; no newer installer on disk). A2Tools 2.0.99's local files hold only world event timers. Questlog 0.1.56 and LOXIN 0.1.114 were not unpacked for this: neither has boss-mechanic data in the versions looked at before | Nothing used | |

Confidence, as used in the tables:

- **confirmed**: the capture shows it directly (who is hit, how hard, which mobs appear, where).
  This confirms *what the cast does*, not what the community calls it.
- **likely**: the capture and a guide agree (same place in the fight's order, same shape of
  mechanic), so the guide's name is probably this ID.
- **guess**: one weak sign only. Say so before using it anywhere.

Numbers are from the 20 Nov capture unless a row says "13 Nov". Damage is per hit as the
game shows it; players had roughly 30,000 to 45,000 HP. "Avoided" means the hit arrived with
0 or 1 damage (dodged, blocked or jumped: the capture doesn't say which).

## How the IDs are built

- A boss's cast is usually `xxxxxx0` or `xxxxxx5`. Its damage comes under the same ID, or the
  ID plus a few, and for most big mechanics from a **helper mob**: an invisible mob spawned by
  the cast, with the boss's max HP (or 4,655 / 2 / 12 HP for things that can be hit), that
  does the casting and the damage. Ludra's helpers are mob types 2920745 to 2920820 (all
  named "Eternal Ludra" where they have a name), Ketu's 2920668 to 2920681, 2920775, 2920776,
  2920808, 2920818 and 2921311.
- **1000009 and 1001450 are not attacks.** They fire the moment a boss's stagger gauge
  empties (Ketu: gauge 4500 to 0 at about 128 s, full again 5 s later; confirmed on two
  attempts). 1000009 touches every player with no damage (effect 10000099, 1 s); 1001450 puts
  effect 10000002 on the boss for as long as it is down (Ketu 5 s; Ludra 10 s, twice, after
  the Trial). 9746 is Ludra getting back up.
- Effects that recur on players: 12000151 (20 s) is a damage-over-time wound from physical
  hits; 12000001 (1.5 to 3 s), 12000311 (2.5 to 3 s), 12000342 (3 s) and 16009201 (3 s) come
  with knock-downs or stuns (inferred from their length and the hits they ride on, not read
  from game text).

## Abyssal Forge: Ludra (map 600082)

The raid: the Central Abyss Fusion Reactor with all ten, then the group splits (Group 1
Rahu, Group 2 Ketu, five each, at the same time), then Eternal Ludra with all ten.

### Central Abyss Fusion Reactor (2301008)

One kill (162 s) in the capture; five more fights in the app's record with the same nine
IDs. None of the nine is in our game data. Guides say only that it alternates circular and
cross-shaped area attacks and that Auxiliary Reactors must be destroyed by two players sent
up the updraft, or the main reactor barely takes damage.

| cast | what it does (capture) | cast time | from | seen | confidence |
| --- | --- | --- | --- | --- | --- |
| 1802580 | Seven marked spots, landing 1.1 to 1.5 s later; 8,000 (12,600 max). Filler, every 5 to 8 s | none | pull | 11 | confirmed |
| 1802520 | Hits 1.8 players a cast (4 at most); 14,600, 23,000 max. Those hit stood 4 to 11 m from the reactor, those not hit mostly farther: perhaps the inner circle (6 hit positions) | 1.5 s | pull | 6 | confirmed; shape unknown |
| 1802515 | Mostly avoided (0.5 players a cast); 21,300 | 2 s | 99% | 6 | confirmed; shape unknown |
| 1802510 | Mostly avoided (0.8 a cast); 25,700, 37,500 max; 1 death | 2 s | 90% | 5 | confirmed; shape unknown |
| 1802555 | 2.6 players, two hits, half avoided; 13,600; effect 18025501 (3 s) on those hit. Those hit stood 8 to 15 m out, those not hit mostly closer: perhaps an outer ring (11 hit positions). Both Auxiliary Reactors (2301012 at 56% HP, 2301013 at 27%) appeared within seconds of one | 3 s | 80% | 5 | confirmed; link to the reactors is a guess |
| 1802540 | 2.5 players (5 at most), two hits; 19,600; effect 18025401 (8 s) on one player | 3 s | 67% | 2 | confirmed; what the 8 s effect is: unknown |
| 1802570 | Aimed at a player; 2.7 players take 4 to 5 hits of 10,300 (17,500 max), all within 5 m of the aimed-at spot and of the reactor; 4 of the kill's 7 deaths. Eight Lasers (2301010, 60 s) appeared with one cast | 3 s | 60% | 3 | confirmed |
| 1802530 | 1.5 players; 13,200 (18,200 max) | 1.5 s | 57% | 4 | confirmed; shape unknown |
| 1802550 | 3 players (6 at most), two hits, half avoided; 9,800 (22,200 max); effect 18025501 (3 s). Four Lasers (2301011, 43 s) appeared with one cast | 3 s | 50% | 4 | confirmed |

Around it: waves of Ludra Follower Drakan, Beast, Warmage and Officer (2301001 to 2301004;
their skills are 12392xx to 12394xx, hits of 1,000 to 7,000), and the Lasers (2301010 casts
1225300 "Laser" twice a second; 2301011 casts 1231980).

### Abyssal Wings Ketu (2301007)

Twelve attempts, ten of them longer than 20 s, one kill. Five players. The fight in order,
as every attempt ran it:

1. Pull, normal attacks.
2. First Abyssal Gaze: its lead-in starts at 79 to 84% HP (34 to 45 s in).
3. Normal attacks, now with falling orbs, laser lines and (from about 74%) scythes.
4. Stagger gauge empties at about 128 s: Ketu is down 5 s (1000009, 1001450).
5. Second Abyssal Gaze: lead-in at 59 to 63% HP, with the colour orbs.
6. From 53%: the 6.5 s cast with colour orbs in a row.
7. Eruption of Power at 29 to 34% HP.
8. To the kill: everything again, with 1801956 in place of 1801950.

| cast | game name | what it is | what it does, what to do | cast time | damage and effect IDs | how identified | confidence |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 1801800, 1801810 | Attack | Opening hits on the tank | At the pull: 11,400 and 17,400 on the tank. 1801800 once more late in a long fight | none | same IDs | capture | confirmed |
| 1801870 | Attack | Flurry on the tank | Aimed at the tank 39 of 40 times: 8 hits over 2.8 s, 6,500 each (26,600 max), then two bleeds. Every ~98 s | 1.5 s | 1801870; effects 12000151, 18018601 (20 s) | capture | confirmed |
| 1801860 | Attack | Strike at a random player | Never aimed at the tank (0 of 27). A cone towards the aimed-at player (60° each side, reaching 24 m): two hits of 7,400 on 1.5 players, a bleed | 1.5 s | 1801860; effect 12000151 | capture | confirmed |
| 1801820 | Attack | Blast at the tank | 1.9 players; 10,700, 27,900 max; a 3 s knock-down on some. Rare (9 casts) | 2 s | 1801820; effect 12000001 | capture | confirmed; shape unknown |
| 1801840 then 1801841 or 1801842, then 1801843 then the other one | Attack; 1801843 has no entry | Scythe spin: two hits on opposite halves of the arena | 1801840 (hit on the tank, 11,800), then one of 1801841 / 1801842 lands 1.2 to 1.4 s later: two hits of 18,000 (29,700 max) on whoever is in it, half avoided, 3 s knock-down. By where the hit players stood, 1801841 takes one side of his facing (27 of 36) and 1801842 the other (19 of 28), reaching 20 m; players close behind him were rarely hit by either. Then 1801843 (tank, 11,100) and the other one. The order of 1801841 and 1801842 changes from chain to chain (25 to 22). 14 deaths. Move out of the first half, then cross into it. Every ~78 s | 1.3 s, none, 1.3 s, none | 1801840 to 1801843; effect 12000342 (3 s) | capture (positions); U4N (the scythe spin, then two halves) | likely: left and right of his facing. The split is not clean in the positions, and which is his left on screen was not checked |
| 1801850, then 1801851 twice, 1801934 with 1801930, then 1801932 (or 1801933) | Attack; 1801851, 1801932 to 1801934 have no entry | The eye mark and the jump | 1801850, then two quick hits (1801851: 6,400, on anyone within 5 m of him, front or back) as he leaves for the far end. 1801934 marks a random player (the tank 12 of 49 times): no damage, effect 18021302 for 3 s. 1801930 is cast at the same moment from 31 to 39 m away and lands 2.2 to 2.3 s later on 2.7 players, mostly for nothing (23,100 at most): the landing, which the tank should block. All 105 hit players stood within 9 m of the marked player's spot. 3.2 s later 1801932 (46 times, no damage on anyone) or 1801933 (3 times, and a mob 2920776 then casts 1801931: 19,300 to 22,500 on up to 5, 1 death). Every ~62 s | 2 s, then none | 1801851 (effect 18005602, 1 s); 1801930 (effects 18019301 2 s, 12000001 3 s); 1801931; mark 18021302, 18020008 | capture (positions); U4N (the jump the tank must block) | confirmed: a jump from the far end onto the marked player's spot. That 1801932 is "blocked, no ring" and 1801933 "not blocked, jump the ring" is a guess from 46 to 3 |
| 1801940 | Attack | Two falling orbs ("Sink silently and perish") | 4 s after the cast starts, two orbs (2920668, cast 1801942, 1 s) fall on two spots (2920669). Catching one puts effect 18019421 (30 s) on the catcher: 62 of 70 were caught. One that lands: 1801943, 28,200 (35,900 max) on 2.3 players; 5 deaths from 4 landings. From ~79% HP, every ~54 s | 2 s | 1801942, 1801943, 1801941 (a marker, 2920808); effects 18019421, 18019404 | capture; U4N | confirmed |
| 1801950 | (no entry) | Laser line | A row of 14 casters 6 m apart along an edge of the arena (2920676 × 14, each casting 1801951 for 1 s); the players hit stood 17 to 35 m from the caster that hit them, so these are beams across the arena, not round blasts: 22,500 (45,100 max), half avoided, 1.5 s knock-down, 12 deaths. A marker mob (2920702) stays 20 s. Outside the Gaze it is followed at once by 1801952: his own hit at close range (3 to 7 m, mostly in front), 11,300 (29,700 max) on 1.5 players and a 20 s bleed. Every ~45 s; 64 casts | 1.16 s | 1801951 "Attack", 1801952 "Attack"; effects 12000001, 18018601 | capture (positions); U4N (his hand shows where the lasers come) | confirmed for the row; the name likely |
| 1801956 | (no entry) | Laser line, late | As 1801950, below 34% HP (after Eruption of Power) | 1.16 s | as above | capture | confirmed |
| 1801830 then 1801831 | Attack; (no entry) | Chasing scythes | 1801831 is cast on a player and a Ketu's Scythe (2920670, 12 HP) appears and follows: 1801832, 9,100 a hit (12,900 max), no deaths. Kill the scythe. From ~74% HP; 21 casts | 2.1 s, none | 1801832; boss effect 18018812 (4 s) | capture; U4N | confirmed |
| 1801980, 1801880, then 1801963 once a second | Attack | Abyssal Gaze, lead-in | 1801980 at a player (the boss gets effect 568 for 42 s), 1801880, then 1801963 ticks for about 30 s while spirits come in waves: Fearsome Evil Spirits (2920674, 8 HP; 1801910 hits for nothing and slows, effect 12000461) and Destruction Evil Spirits (2920675, 2 HP; 1801920, 4,700). Kill them before they reach him | none | 1801963 (8,000, 61 hits in all); boss effects 568, 18019631 (30 s), 18018001 (39 s) | capture; guides | confirmed |
| 1801950 then 1801887, inside the lead-in | (no entry) | Ketu's double and its lasers | 1801887 brings a second "Abyssal Wings Ketu" (2920671) that marks 20 spots every 1.25 s for the rest of the Gaze: 1801881, 14,400 (24,900 max); with 1801885 and 1801888, 15 deaths | 1.16 s, none | 1801881, 1801882, 1801885, 1801886, 1801888, 1801891 | capture | confirmed |
| 1801902, then 1801969 (first Gaze) or 1801962 (second) | (no entry); Abyssal Gaze | Abyssal Gaze, the cast | A 20 s cast. As it ends: 1801892 every 0.5 s about 15 times (3,500 on 4 of 5) and 1801966 once a second 7 times (12,600 on nearly everyone; 4 deaths). End in the middle | 20 s | 1801892 "Attack", 1801966 "Abyssal Gaze"; boss effects 18018921 (7 s), 18019622 (6 s) | capture; game name | confirmed |
| 1801967, 1801968 | (no entry) | The colour orbs: 1801967 blue (turn away), 1801968 red (face it) | Each puts one orb out: 1801967 → 2920677, which casts 1801964 (1.5 s): 19,000 (26,900 max), 10 deaths. 1801968 → 2920678, which casts 1801965 (1.5 s): 23,200, 1 death. In the second Gaze and after the 6.5 s cast. As 1801965 went off, 57 of the 93 players who took nothing were looking within 60° of the orb and the 3 it hit were looking away; as 1801964 went off, 89 of 147 unhurt players had their backs more than half turned to it | none | 1801964 "Attack", 1801965 "Attack"; boss effects 18019671, 18019681 | capture (where players were looking); U4N | confirmed that they are the two orbs; the colours likely (1801965 is clear, 1801964 less so: its 15 victims looked every way) |
| 1801961 | Attack | Blast, then colour orbs in a row | From 53% HP, every 62 to 72 s: his own hit 26,200 (39,000 max), half avoided, 5 deaths; then two or three orbs (the two boss effects come with 1.0, 1.8 and 2.6 s) | 6.5 s | 1801961; then 1801967 / 1801968 | capture; U4N (orbs also come outside the Gaze, several in a row) | likely |
| 1801981, then 1801970, then 1801972 ten times | Attack; Eruption of Power; Attack | Eruption of Power | At 29 to 34% HP, once. Sixteen Mortal Soul Stones (2920672, 2 HP: two hits each) appear on a grid. Five casters at Ketu's place (2920679, 2920680, 2920681, 2920818, 2921311), one per player, each cast 1801971 (4.5 s) at one player, 17 to 34 m away: 15,000 (29,100 max) on about 3 players each (a fan towards that player, not a circle on them: those hit stood up to 46 m from the aimed-at spot). 20 deaths, the most of any ID in the fight. Breaking a stone gives the breaker effect 18018924 for 3.5 s (17 times in 8 attempts): the immunity | none, 9 s, none | 1801971 "Attack", 1801893, 1801894; effects 18018924 (3.5 s), 12000342; boss effects 568 (20 s), 18018816, 18019701 | capture; game name; U4N | confirmed |
| 1000009, 1001450 | (no entry) | Ketu staggered | See "How the IDs are built". About 128 s in, and again some 200 s later | none | effects 10000099, 10000002 (5 s) | capture (the gauge) | confirmed |

### Isolation Specialist Rahu (2301006)

**No capture at all.** The recordings of 6 Nov are no longer on disk, the app's record has no
Rahu, and on 20 Nov this character was in Ketu's group. What there is:

- From the game data: 1802680 Refracting Spear, 1802720 Soul Strike, 1802900 Wheel of
  Torment, 1802980 and 1802981 Feeding Time. From `boss-mechanics.json`: 1802730 (a 1.5 s
  cast that took 17 of 76 deaths on 6 Nov).
- From the guides (U4N, Mobalytics), with no IDs: cross-shaped attacks; a close hit followed
  by a ring with a safe centre; the eye mark with a line attack to aim away; the SOS Bomb
  (20 s, passed by standing with someone; three failures wipe); ring attacks to stack for;
  Wheel of Torment after the first HP threshold (everyone thrown to the edges, regroup on
  the marked player); puddles to drop at the edge; the arena cut in four at about 50%;
  Feeding Time (aim the eye's line through a monster).

Which ID is which of these, beyond the four named ones, is unknown. One recorded night in
Group 1 would settle it the way this report does for Ketu.

### Eternal Ludra (2301014)

13 attempts on 20 Nov (best 53.3%), 11 saved fights from 13 Nov (best 42.3%). Only three
of her IDs are in our game data (1803000, 1803010, 1803011).

**The 37 s block that 1803240 opens (2026-11-28).** Inside phase 1's script there is a
block with no variation at all: in all 26 of its runs in the replayed fights of the 21 Nov
night, each cast began within 0.1 s of the same moment after 1803240's cast began, and its
damage landed within a few tenths. The meter now counts down to each step from that cast
(`ludra.ludra.closing` in `gamedata/boss-mechanics.json`).

| step | cast begins | damage lands | what it is |
| --- | ---: | ---: | --- |
| 1803240 | 0 | (1803245 ticks from about +4, rarely) | The outer floor turns deadly and stays so for the block: the arena closes in |
| 1803210 | +6.9 | +10.0 (1803215) | Orbs to catch, and tornadoes leaving the centre in lanes |
| 1803120 | +12.5 | +15.3 | The breath, a cone in front of her |
| 1803250 | +17.9 | +20.4 | Lightning on every player |
| 1803210 | +21.7 | +24.9 (1803215) | Orbs and tornado lanes again |
| 1803120 | +27.3 | +30.2 | The breath again |
| 1803230 | +32.7 | +37.2 to +39.6 (1803235) | The red circle |

The eye mark (effect 12000064) goes on one player 6 to 7 s in (26 of 26) and comes off at
about +32 in 16 of them, just after the second breath, so it belongs to the breaths. Its
holder was hit by the breath in 2 of 43 breaths. Seen in the fight (the user, 2026-11-28):
she aims the breath, a cone, at where the eye's player stands; if that player dies before
she takes aim, it goes at a random player. So the card tells the eye's holder to keep it
off the raid, and everyone else to get out of the cone.

The red circle, seen in the fight too (2026-11-28): the centre takes a massive hit, and
outside the red circle, on the outer floor, there are only small ticks that can be healed
through. That settles 1803230's safe place, and it fits our positions after all: the
pulses we counted (1803235, about 5,000 each, on players 18 to 33 m out) are those ticks,
and the players they missed stood further in. Whether the centre's big hit is in our
records under an ID of its own is not checked.

What each step is on screen is a lead from a practice trainer made from clear videos
(angyeety.itch.io/aion2-zikel-practice, `ludra.html`; no licence, so nothing of it is
copied, and its author says it isn't exact). Its clock minus 2.6 s is seconds after
1803240's cast begins: with that, its tornado burst (+9.7), breath (+15.1), lightning
strike (+20.4) and red circle's blast (+36.7) fall within 0.5 s of our damage times. It
says: the outer floor floods purple and the arena shrinks to about 18 m; twelve tornado
lanes leave the centre with gaps between them; the breath is a cone of about 100° that
follows the player with the eye and is set 1.5 s before it fires; a circle about 5 m in
radius appears under every player 1.2 s before its strike; and for the red circle, which
fills the inner arena, the outer floor is the safe place (confirmed in the fight, above).
It corrects three of our rows below: 1803240 is the arena closing, not the tornadoes; the
"blast around her" that comes with 1803210's orbs is the tornadoes; and 1803230's pulses
are the outer floor's ticks, the price of standing where it's safe.

Its other page (`zikel.html`) is phase 2's Zikel sequence (our 1803390): four lines 3.9 s
apart after a 1.5 s lead, 17.1 s in all (we see the next cast 16.4 to 16.5 s after
1803390, 6 of 6); each line is marked 1.1 s ahead, struck, then safe to stand on while
everything off it blasts 1.6 s later; rows of pillars then cross the arena from the south,
west, east and north in turn. Hard mode adds a puddle under each player every 3.7 s. Its
"colours" mode is the later phase we have no record of: blue players west and red east
facing her, turning to red north and blue south after the second line. None of that is
checked against a capture.

**Phase 1 is a script.** Every attempt runs the same order; the only variation is a filler
around 29 to 40 s (one or two of 1803060, 1803070, 1803080, 1803090, 1803100, 1803110,
1803130) that moves everything after it by up to 9 s. The usual times, in seconds from the
pull: 0 tail swipe · 4 cleave · 7 double hit · 13 and 16 cleave and follow-up · 21 cleave ·
24 and 29 marked ground · 35 orbs (4 blue and red) · 44 fields · 50 tornadoes from the
centre · 57 orbs, then a blast around her · 63 double hit · 68 lightning on every player (1803250) ·
72 orbs, then a blast around her · 77 double hit ·
83 arena-wide · 92 fields · 98 sweeping breath · 102 and 108 marked ground · 113 orbs
(4 blue and red) · then the same blocks again until the Trial.

| cast | what it is | what it does, what to do | cast time | damage and effect IDs | how identified | confidence |
| --- | --- | --- | --- | --- | --- | --- |
| 1803080 | Tail swipe | Opens every pull, then about every 82 s. Lands 0.3 s after in the arc behind her (all 24 hit players stood 120° or more round from her facing, out to about 25 m): 2.2 players, three hits; 22,900 (40,300 max) when it connects, two thirds avoided; 3 s knock-down and a bleed. Tank: block it | 0.9 s | 1803080; effects 12000311, 12000151 | capture (positions); Mobalytics (the pull opens with a tail attack on the tank) | confirmed that it hits behind her; the name likely |
| 1803050 | Cleave on the tank | Aimed at the tank 22 of 28 times; two hits 1.2 s later in a cone in front of her (about 150° wide, 12 m; of 48 hit players none stood beside or behind her), 17,200 (23,900 max), a bleed. Second cast of every pull | none | 1803050; effect 12000151 | capture (positions); Mobalytics (a cleave follows the tail) | confirmed frontal cone; the name likely |
| 1803060, 1803070 | Side swipes, one side each | Fillers: 1.4 to 2.5 players within about 10 m, two hits, 21,300 and 16,500, a bleed. 1803060 hit only players on one side of her (10 of 10), 1803070 only players on the other (6 of 6) | 0.6 s | same IDs | capture (positions) | likely (16 hit positions) |
| 1803090, 1803100 | Heavy single hits | Fillers, 3 casts in all: one or two players, 32,400 | 1.5 s | same IDs | capture | guess (too few) |
| 1803110 | Heavy strike on one player | One player a cast, at any angle and any distance (9 to 58 m from her): 37,500, 68,600 max. 13 deaths in 13 casts on 20 Nov. A filler in phase 1, and twice in every Trial, 9 s apart | 1.5 s | 1803110 | capture | confirmed |
| 1803120 | The double hit | Third cast of every pull and straight after every 1803210. Two impacts, 0.5 and about 1 s after it goes off, over the half in front of her out to at least 24 m (all 19 hit players stood within 90° of her facing; of 117 players not hit, 107 were beside or behind her); usually avoided (0.5 players a cast); 23,100, 55,500 max; 11 deaths. Be behind her, or dodge or block both | 2.33 s | 1803120 | capture (positions); Mobalytics (a double hit to dodge or block follows the eye) | confirmed frontal half; the name likely |
| 1803130 | Two expanding rings | Aimed at the tank; two waves 1.6 and 2.2 s after it goes off reach 3.6 players (8 at most) on every side of her, 7 to 26 m out, two thirds avoided; 27,400 max; knock-down. Jump both. 5 casts | 1.5 s | 1803130; effects 12000311, 12000001 | capture; Mobalytics (two rings to jump, early in phase 1) | likely |
| 1803140 then 1803150 | Cleave, follow-up, and two blasts under players | 1803140: 16,700 on the spot of the player it is aimed at (those hit stood within 9 m of it; the tank 11 of 19 times). 1803150 at once: a cone in front of her (60° each side, 20 m), hits at 1.3 and 2.2 s, 21,300 (30,800 max), 6 deaths. And a field appears under each of two players (helper 2920745) that goes off 3 s later: 1803155, 23,200 (27,000 max), 13 deaths. Move off the field. At 13 s, then every ~106 s | 1.5 s, none | 1803140, 1803150, 1803155; effect 18031552 (5.5 s, on the field) | capture (positions) | confirmed; no guide name fits cleanly |
| 1803220, 1803225 | Twelve marked spots | Twelve marks on the ground each, landing 4.05 to 4.75 s after. 1803220 is aimed at a player who is not the tank (21 of 22). 26,800 and 15,500 (41,400 max), 2 s knock-down; 12 deaths together. Always as a pair 5 s apart, and 1803225 once more just before the Trial | 1.5 s | same IDs; effect 12000001 | capture (the marks); Mobalytics (plus-shaped areas) | confirmed for the marks; the plus shape is a guess |
| 1803200 | Orb set: four blue and the red | Four blue orbs fall from 10 m (2920753 over landing spots 2920752) at four fixed corners 28 m out, and the red one (2920755 over 2920754) 16 m from the centre, in front of her. 10 s to catch them. Red caught: 1803202 on the catcher, no damage, effect 18032014 for 15 s (17 of 20). A blue one lands: 1803203, 21,000 (31,500 max) on 7 of 10. The red one goes off: 1803204, 162,000 (224,700 max) on everyone: a wipe, 3 times. Blue: anyone jumps into them; red: the tank, blocking. Every ~78 s from 35 s | 2.33 s | 1803201 to 1803204; effects 18032011, 18032012, 18032021, 18032022 (10 s, on the orbs), 18032014 | capture (positions and heights); all three Ludra guides | confirmed |
| 1803210 | Orb set (two or three blue) and a blast around her | Two or three blue orbs 20 m out, and as the cast ends a blast around her (helper 2920793 at her feet casts 1803215 on itself, 0.7 s): 20,800 (40,900 max) on 2.9 players (6 at most), half avoided, 3 s knock-down, 12 deaths. The 91 hit players stood 7 to 18 m from her, on every side; those not hit were mostly 19 m or more away. Comes in pairs 15 s apart; effect 12000064 (25 s) goes on one player just before the first of each pair. Be 20 m from her as the cast ends | 2 s | 1803215, 1803201, 1803203; effects 16009201, 12000064 | capture (positions) | confirmed for the orbs and the blast around her. It is *not* the guides' eye attack (a large area on a marked player): 1803215 is centred on her and never aimed at the player with 12000064. 12000064 may still be the eye mark, with its attack unfound (guess) |
| 1803170 then 1803450 | Seven fields: "arena reduction", later "chasing tornadoes" | 1803450 touches every player (no damage, effect 18034502 for 3 s) and seven fields appear (helper 2920748) that tick 1803185 twice a second for 12 s on anyone within about 7 m of one: 7,200 (11,200 max), 9 deaths. The first two times in a fight they stand on fixed points (the centre and six of eight points 20 and 28 m out, two left free); the third time (137 to 158 s) they start where players are. Every ~48 s from 44 s | 2 s, none | 1803185; effects 18034502, 18031851 | capture (positions); Mobalytics' phase 1 order | confirmed for the fields; the two names likely |
| 1803240 | Tornadoes from the centre | A helper at the centre casts 1803246 (3.8 s), then another (2920751) stays there 30 s ticking 1803245 every 0.8 s; it hit only 4 times all night (15,300). At ~50 s, then every ~89 s | 1.5 s | 1803245, 1803246 | capture; Mobalytics (tornadoes come out of the centre together with an orb set; 1803210 follows it every time) | likely |
| 1803230 | The arena-wide attack | A helper at the centre casts 1803236 (4.2 s), then 2920750 pulses 1803235 four times in 2.5 s on 8 of 10 players: 5,000 a pulse, 31,500 max; 10 deaths. Twice before the Trial. Guides: run all the way out of the marked area. Positions: the players hit stood 18 to 33 m from the centre, the few not hit 11 to 25 m (11 samples), so where it is safe is not settled | 1.5 s | 1803235, 1803236 | capture; MMOPIXEL and Mobalytics (it comes twice before the first Bingo) | likely |
| 1803160 | Sweeping breath | Three bands of three blasts 30 m apart (helper 2920746 × 9, each casting 1803165 for 0.8 s), one band a second, crossing the arena from a side that changes (east to west, west to east, south to north all seen): 28,500 (42,500 max), 4 deaths; each blast reached players up to 23 m from its centre. Go to the far side | 1.67 s | 1803165; effect 18031651 | capture (positions); guides (a breath that sweeps from one side to the other) | likely |
| 1803250 | Lightning strike on every player | 1.0 s after it goes off, a strike lands on each player: three hits, 16,500 (30,500 max), two thirds avoided; 2 s knock-down and a bleed. 16 deaths, the most in phase 1. A player always took 3, 6 or 9 hits, by how many players stood within about 4 m, themselves included (alone 3, two together 6, three together 9), so players standing together take each other's strikes; 40 of the 42 players who took nothing stood alone. Always between a double hit and an orb set. Spread out and dodge your own | 1.5 s | 1803250; effects 12000001, 12000151 | seen on screen by the maintainer (2026-11-20); capture (hits per player against who stood together) | confirmed. (An earlier reading here, "a blast all round her", was wrong: the hit players were on every side of her because the melee group stands there together) |

**Ludra's Trial** (the guides' "Bingo"). Starts between 59 and 55% HP in all 15 recorded
Trials, always straight after a 1803225 (so probably a 60% trigger that waits for the
running block).

| cast | by | what it is | notes | confidence |
| --- | --- | --- | --- | --- |
| 1803010 "Ludra's Trial" (1.67 s) | Ludra | The grid appears | 6 × 6 tiles of 6 m: 35 unsafe tiles (2920772) and one safe tile (2920771), somewhere different each time | confirmed |
| 1803011 "Ludra's Trial" (16 s) | helper 2920770, centre | The wipe timer | Starts 1.8 s after 1803010 starts; its damage lands 2.9 s after it ends: 144,000 (224,700 max) on everyone outside the safe tile. So the blow lands about 20.7 s after the Trial's cast begins | confirmed |
| 1803012 (2.75 s, again every 3.75 s) | helper 2920757, centre | The blasts aimed at players that reveal tiles | 24,600 (43,300 max) on anyone standing in one; 30 deaths over both nights. Tiles it lands on get effect 18030121, then 18030123 for good | confirmed |
| 1803014 | the unsafe tiles | Standing on a revealed tile | 3,300 a tick (4,500 max) | confirmed |
| 1803013, 1803015 | the safe tile | The safe tile's protection | No damage; effect 18030131 (3 s) on players in it | confirmed |
| 1803221, 1803222, 1803223, 1803224 (instant) | Ludra | Unknown: one attack in four variants | Two per Trial (about 4.5 and 13 s in), a different pair each time. No damage ID of their own seen. Guides: after the first two reveals she attacks the middle of the arena | guess |
| 1803110 (1.5 s) | Ludra | Heavy strike, twice | See phase 1 | confirmed |
| 1803440 (instant) | Ludra | The Trial is survived | About 22 s after the Trial's cast began | confirmed |
| 1000009, 1001450 twice (10 s apart), 9746 | Ludra | Staggered, then up again | The free damage window: about 20 s | confirmed |
| 1803000 "Supply Eldritch Energy", 1803451, 1803275 (all instant) | Ludra | Into phase 2 | 33 to 38 s after the Trial's cast began. A helper (2920769, 4,655 HP) stands at the centre for 27 s | confirmed that it is the transition; what the player sees: unknown |

The Trial was survived in 7 of 15 records.

**Phase 2** (after the Trial, 57% down to 45%). From the five long fights of 13 Nov, plus
30 s of one attempt on 20 Nov. The damage numbers are thin and there are no positions, so
most names are weaker here.

| cast | what it is | what the records show | cast time | damage IDs | confidence |
| --- | --- | --- | --- | --- | --- |
| 1803260 | Five marked spots at a player | Five marks landing 2 s later. 27 casts, about every 23 s; it never damaged anyone | none | none seen | confirmed for the marks; what it is: unknown |
| 1803310 | Blasts under players that hit twice | Her own hit, 30,400 to 37,300 (2 hits), and fields under two players (helper 2920760, as 2920745 in phase 1): 1803315, 21,100 (27,100 max), 3 deaths. 17 casts. Spread, dodge both | 1.5 s | 1803310, 1803315 | likely (Mobalytics: areas under players that strike twice) |
| 1803280 | Expanding rings, two jumps | 12 casts; caught one player once: 1803285 (helper 2920819), 139,400, dead | 2.67 s | 1803285 | likely |
| 1803290 | The other ring cast | 6 casts; a helper (2920820) casts 1803295 for 3.23 s; hit no one | 2.67 s | 1803295 | guess (perhaps the rings that come with orbs) |
| 1803380 | Unknown | 11 casts, usually straight after a ring cast: 1803385 (helper 2920763), 31,700 (42,600 max) on about 2, half avoided, 8 deaths | 2 s | 1803385 | guess: the sweeping breath (Mobalytics gives the early phase 2 order as rings, breath, then the areas under players) |
| 1803350 then 1803370 | Unknown: a heavy hit that often comes twice | 1803370: her own hit, 32,900 (50,400 max) on about 2, a third avoided, 14 deaths. 17 casts, often two 19 s apart | 2 s, 1.67 s | 1803370 | guess: the "pizza" (two hits, opposite safe sides, can be blocked) |
| 1803270, 1803271, 1803272, 1803273 | Unknown: one attack in four variants | 12 casts over the four IDs; hit no one | 2 s | none seen | guess: four directions of one attack |
| 1803390 | The Zikel sequence | At 45% HP in all four records, then no cast for 16 s. Helper 2920764: 1803395, 36,000 (56,300 max), 8 deaths; and 1803396, 164,000 to 226,000, 6 hits, 6 deaths. Guides: lasers from south, west, east, north; face each as it comes | 3 s | 1803395, 1803396 | likely |
| 1803360 | Unknown | Straight after the Zikel sequence (3 of 4), once more later | 2 s | none seen | guess |

**Below 42%: never seen.** The guides have the colours (red and blue players kept apart),
a second Bingo at about 25%, and the coloured safe spots to enter in the last second.

## Seen in captures, still unidentified

| IDs | what would settle it |
| --- | --- |
| Ludra 1803090 / 1803100 | More casts: 4 hit positions in all (three in front of her, one off to a side) |
| Ludra 1803221 to 1803224 (in the Trial) | A Trial recorded with a screen recording beside the capture: they have no damage ID of their own to place |
| The guides' eye attack on Ludra (a large area on a marked player) | Settled 2026-11-28: it is the breath, 1803120 inside the block 1803240 opens, aimed at the eye's player (seen in the fight; above) |
| Ludra 1803230's safe place (inside or outside) | Settled 2026-11-28: outside the red circle, on the outer floor (seen in the fight; above) |
| Ludra 1803260, 1803270 to 1803273, 1803290, 1803350 / 1803370, 1803380, 1803360, 1803275 | A full capture of phase 2 (the saved fights have no helper casts, spawns or positions). One night that gets past the Trial a few times |
| Ketu 1801964 against 1801965: which orb is red | Likely settled (1801965 red). One look at the screen in a second Gaze would confirm it |
| Ketu 1801841 against 1801842: which half | Sides of his facing, but which is his left on screen needs one look at a recording |
| Ketu 1801932 against 1801933 | A night where the jump is deliberately not blocked once |
| Ketu 1801820, 1801961 | More casts: the 9 and 16 hit positions show no shape |
| Reactor 1802510, 1802515, 1802530, 1802550 (circle or cross), 1802540's 8 s effect, what brings the Auxiliary Reactors | More kills: one kill gives 1 to 11 hit positions a cast. The effect's icon in a screen recording |
| Rahu, everything | A recorded night in Group 1 |
| Everything of Ludra below 42% | Progress |

## In the game data for these bosses, never seen in a capture

| IDs | name in our data | note |
| --- | --- | --- |
| 1801890 | Attack | Ketu's range; never cast |
| 1801973 | Eruption of Power | A second ID of Ketu's Eruption; only 1801970 is ever cast |
| 1802680, 1802720, 1802900, 1802980, 1802981 | Refracting Spear, Soul Strike, Wheel of Torment, Feeding Time (twice) | Rahu: no capture |
| 1803020 | Condensed Forcefield | Ludra, later phases |
| 1803030, 1803035 | Ure's Area | Ludra, later phases |
| 1803040, 1803041 | Ludra's Trial | Probably the second Trial (guess) |
| 1803570, 1803571 | Screaming Storm | Ludra, later phases |
| 1803615 | Judgement of the Distorted Soul | Ludra, later phases |
| 1802000 to 1802160 (21 IDs) | Attack; 1802153 Blue Light of Destruction | Not this raid's: Ferocious Horn Nuakum's (2320910). The app's record has him casting 27 IDs in this range in one fight (2026-11-21; the capture `glass-cannon-test-20261120d\captures\20261121-000908` has it), three of them long: 1802131 (5.5 s, four times), 1802141 (5 s) and 1802154 (3 s). Ketu's mark borrows two of his effects: 18021302 and 18020008 on players, whose IDs point at his 1802130 and 1802000. What each of his casts does is not worked out |
| 1802220, 1802270 | Nebular Collapse, Supernova Explosion | Owner unknown |
| 1802430 to 1802504 (18 IDs) | Rapid Slice, Sword Aura Blast, Smite Ground, Chain Blade Rampage, Greatsword Plunge, Whirling Rush, Backstep Slice, Shatter Ground | Another boss, not this raid's (the Reactor's casts are 1802510 to 1802580 and none of them is in our data) |

The other way round: 93 IDs between 1801800 and 1803999 were seen in captures and have
**no entry at all** in `skills.json` or `boss-skills.json` (every Reactor cast, every Ludra
cast but three, and 21 of Ketu's). Whether the game's own text has "Attack" for them was not
re-checked here.

## Other bosses in the captures

Short, because the test captures only brush them: two field bosses that never went below
85%, and three kills of a few seconds. Names are from the game data.

| boss | cast | name | cast time | what the capture shows |
| --- | --- | --- | --- | --- |
| Deceiver Trid (2400274), 6 fights | 1502770 | (none) | none | Basic attack, every 2 to 4 s |
| | 1502780 | (none) | 1.83 s | Every ~18 s |
| | 1502790 | Magic Bullet | 1.67 s | Every ~18 s |
| | 1502800 | Rooting Spell | 1.33 s | 7 casts |
| | 1502810 | Rapid Magic Bullet | 2 s | Four marked spots landing 0.75 to 1.05 s later; every ~15 s |
| | 1910170 | (none) | none | On itself every 15 s; unknown |
| Blood Warrior Lannar (2400223), 8 fights | 1218010 | Call Ally | none | Once, at the pull |
| | 1502300 | (none) | none | Basic attack, every 2 s |
| | 1502320 | Axe Throw | none | Every ~19 s |
| | 1502330 | (none) | 0.5 s | Hits for nothing; every ~10 s |
| | 1502340 | Overhead Slam | 0.67 s | Every ~19 s |
| | 1502350 | (none) | 1.5 s | Every ~33 s |
| | 1910255 | Spectral Blade | none | Every 10 s: three phantoms (2920317), each casting 1910256 Spectral Blade for 1.5 s (620 a hit, half avoided) |
| Predator Garsan (2400212) | 1235340; 1235380, 1235390, 1235400; 1910255 | (none); Spectral Blade | none; 0.7 to 0.8 s | Too short to say more |
| Grandios (2345107), a 15 s kill | 1600410; 1600440 with 1600441; 1600490 | Attack; Expansion; Absorb | none; 3.5 s; 2 s | One cast each |
| Oathbreaker Gartua (2920323) | 1600445, then 1600446 and 1600447 25 times each | Expansion | 0.5 s | One fight of seconds |

The app's own record (`boss-casts.json`) has the casts of 28 more boss types from earlier
weeks (Vakron Sky Island's, the Fire Temple's and Arkanis's among them), but no capture of
those is left on disk, so nothing beyond their cast times can be said here.

## What the meter gets wrong or leaves unnamed today (proposals only)

1. **Helper mobs' casts never reach the boss cast log.** A cast is logged only when its
   caster's mob type is a boss, and the helpers are not. So the meter never shows the
   Trial's 16 s wipe timer (1803011), the arena-wide attack's 4.2 s (1803236), the five
   4.5 s casts of Eruption of Power (1801971, Ketu's top killer), the colour orbs' 1.5 s
   (1801964, 1801965) or the laser line's 1 s (1801951). Proposal: a table of helper mob
   types per boss (the lists under "How the IDs are built"), so their casts count as the
   boss's.
2. **1000009, 1001450 and 9746 are logged as boss casts.** They mean "staggered" and "up
   again". Proposal: name them so, never warn or time them, and consider a "boss is down"
   card (5 s on Ketu; about 20 s on Ludra after the Trial).
3. **Labels in `boss-mechanics.json` that the capture now contradicts or sharpens:**

   | ID | label today | the capture says |
   | --- | --- | --- |
   | 1801840, 1801843 | weapon smash | The two wind-up hits of the scythe spin; the half-arena hits are 1801841 and 1801842 |
   | 1801950 | strike | A row of 14 blasts (laser line); the strike is 1801952 |
   | 1803120 | strike | The double hit (2.33 s, two impacts) over the half in front of her |
   | 1803130 | area blast | Two expanding rings (likely) |
   | 1803160 | heavy area blast | Sweeping breath in three bands |
   | 1803200 | raid-wide blast | Orb set: four blue and the red |
   | 1803210 | area blast | Orb set (two or three blue) and a blast around her (18 m) |
   | 1803250 | area pulses | A lightning strike on every player, three hits each, shared by those standing together |
   | 1803260 | marked ground | Right, but it is a phase 2 cast: five spots, 2 s |
   | 1803080, 1803110, 1803220, 1803225, 1803230 | tail swipe, heavy strike, marked ground (twice), raid-wide pulses | Agree |

   Done on 2026-11-20 ("label today" above is the label before): 1801840 and 1801843
   "strike, side hit next"; 1801841 and 1801842 "side hit" (new); 1801950 "line of beams";
   1801952 "strike" (new); 1803120 "frontal double hit"; 1803130 "two rings"; 1803160
   "sweeping blasts"; 1803200 "orbs: 4 blue + red"; 1803210 "orbs + blast around her";
   1803250 "lightning on every player". 1803260 and the five that agree are unchanged. The
   mechanic card `ludra.ketu.smash` still says "weapon smash" (item 5).

4. **Unnamed casts worth a label:** 1803050 (cleave on the tank), 1803140 and 1803150
   (cleave and follow-up, blasts under two players), 1803170 (seven fields), 1803240
   (tornadoes from the centre), 1803390 (Zikel sequence), 1803280 (rings), 1803310 (blasts
   under players), 1801850 (the jump), 1801940 (falling orbs), 1801830 (scythes), 1801961
   (blast and colour orbs), 1801860, 1801870, and the Reactor's nine.
5. **Mechanic cards:**
   - `ludra.ketu.gaze` lists 1801966, which is the damage tick at the *end* of the Gaze
     (7 a Gaze), so the card comes up again as the Gaze finishes. The 20 s casts are 1801969
     and 1801962; the lead-in starts about 12 s earlier with 1801980.
   - `ludra.ketu.mark` (effect 18021302) says the blast lands 4.3 s on. From the mark it is
     2.2 to 2.3 s; 4.3 s is from 1801850 going off. Triggering on 1801850's cast would give
     6.3 s of notice.
   - `ludra.ketu.eruption` says "stagger them". Nothing in the capture shows a stagger
     there: the stones have 2 HP, the immunity is 3.5 s, and the damage is five 4.5 s casts.
   - `ludra.ketu.smash` says "dodge the dash, jump as it lands". The capture fits U4N's
     description better: two hits on opposite halves.
   - `ludra.ludra.trial` runs until 1803000, about 16 s after the Trial is already survived
     (1803440). The lethal blow lands about 20.7 s after the cast starts: a countdown is
     possible.
6. **Deaths paired to casts by rounding the ID down to a ten** works for most (1803155,
   1803165, 1803215, 1803235, 1801971, 1801951), but not for 1803185 (its cast is 1803170
   with 1803450), 1801964 and 1801965 (1801967 and 1801968), or the orbs that land after
   1803210 (they are 1803203, which rounds to 1803200).
7. **Warnings by deaths.** On 20 Nov the killers were, for Ludra: 1803250 (16), the Trial's
   blow 1803011 (15), 1803110 (13), 1803155 from 1803150 (13), 1803215 from 1803210 (12), the
   red orb 1803204 (12), 1803120 (11); for Ketu: 1801971 (20), 1801951 (12), 1801964 (10),
   1801881 (8), 1801841 (8). `warn_casts` has 1803200, 1803210, 1803160, 1803230 and 1803250
   for Ludra; 1803140 and 1803110 are not in it, and 1801870 (in it) killed one player.
8. **Phase 1 of Ludra is a fixed script**, so the boss timers could show what comes next
   from the pull, not only what has been cast before (the schedule is above the phase 1
   table).

## How this was done

`packet-lab export` on every session, then Python scripts over the JSON lines that read the
same message layouts as `meter-core` (`0x3802`, `0x3809`, `0x380c`, `0x3848`, `0x3804`,
`0x3805`, `0x3641`, `0x3642`, `0x8d00`, `0x8d04`, `0x8d21`, `0x382a`, `0xe005`): attempts
split by the boss's combat state, each cast joined to the hits, effects and spawns that
follow it, and helper mobs placed by their spawn positions against the boss's. Shapes: positions tracked as
`meter-core/src/positions.rs` does (keyframes and steps, and each hit notice `0x3803`'s
place of its target), then each hit player's place taken against the boss's place and the
facing in its cast message, beside the players who were not hit. This can mislead: a strike
on each player looks like a circle round the boss when the melee group stands there (it
did for 1803250). The check is hits per player: one area gives everyone the same count,
strikes on players give multiples by how many stand together. Of the attacks placed here,
only 1803250 shows that pattern; 1803130's 3 or 6 are its two waves. The scripts
and exports are scratch files and are not in the repository. No player names or character
IDs were kept.

Guides read: MMOPIXEL, "Aion 2 Global Ludra Mechanics Guide" (10 Nov 2026, the saved page);
[Mobalytics, Ludra boss guide](https://mobalytics.gg/gamebase/guides/aion-2-ludra-boss);
[U4N, Ketu raid guide](https://www.u4n.com/news/aion-2-ketu-raid-guide-master-every-boss-mechanic.html);
[U4N, Rahu boss guide](https://www.u4n.com/news/aion-2-rahu-boss-guide-master-the-ludra-raid.html);
[ExpCarry, Abyssal Forge: Ludra boss guide](https://expcarry.com/aion-2-ludra-boss-guide).
Their wording is not copied; only the mechanics' names and what to do.
