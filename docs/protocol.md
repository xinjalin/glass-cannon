# Aion 2 protocol notes

Worked out from our own captures (test sessions of 2026-10-18), not from any other meter.
Every opcode seen so far, with what's known and what isn't: [opcodes.md](opcodes.md).

Confidence levels: **confirmed** = holds for every message in every capture so far;
**likely** = consistent but with few samples; **guess** = a hypothesis to test.

## Connection

- One TCP connection to the game server (port 13328 in our captures; the IP changes
  between logins). Other connections from `AION2.exe` are TLS on port 443 and can be
  ignored. `packet-lab` picks the game connection automatically by which stream frames
  cleanly.
- **Server → client is not encrypted** (confirmed). It's framed and partly
  LZ4-compressed, nothing more.
- **Client → server uses the same framing, but the bodies look encrypted** (likely).
  A meter doesn't need this direction.

## Framing (confirmed)

```text
message = varint L, then (L - 4) more bytes
        → the whole message, including the varint, is n + L - 4 bytes (n = varint size)
body    = u16 opcode (little-endian), payload
```

Varints are unsigned LEB128. Across 7 sessions (about 2.3 MB, 70k messages) every received
stream splits into messages with no leftover bytes.

## Bundles: opcode `0xffff` (confirmed)

```text
payload = u32 decompressed size (LE), LZ4 block
```

The decompressed block is more messages in the same framing. Damage events are often
inside bundles, so always unpack them.

## Damage / effect event: opcode `0x3804`

One event per hit or effect tick, for **every entity you can see** (other players, mobs),
not just you.

```text
varint  target            entity ID (confirmed)
u16     flags             bitmask, see below (confirmed)
varint  attacker          entity ID (confirmed)
u32     skill             e.g. 18010340 (confirmed; same skill → same ID)
u8      cast_id           the attacker's cast; shared by all its hits (confirmed)
[0x01]  u32               (Questlog: parts ID; never set in 531,199 hits)
u8      result            hit type: 2 normal, 3 critical (confirmed); 1 evaded, 6 resisted,
                          4/5/7/8 immune, 9 reflected, 0 no effect (Questlog's names)
[0x02]  u8 special        0x04 perfect, 0x08 double strike (confirmed); others unknown
        varint            HP restored by blocking (0 but when it happens)
        u8 position       0 side, 1 back, 2 front (confirmed)
u32     unknown_a         (Questlog: effect ID)
u32     hit               1, 2, 3… within a multi-hit cast; some skills start at 2
varint  unknown_b         constant per attacker for a while (Questlog: attack speed)
[0x04]  varint damage     (confirmed against 10 noted in-game numbers)
[0x08]  u32               MP damage
[0x10, without 0x02] 1 byte
[0x20]  varint n, n × varint parts   a multi-hit's pieces when n ≥ 2 (confirmed)
[0x40]  varint n, n × u32 shields (barriers) it hit
[0x80]  u8
[0x100] varint n, n × u32 effects
u16     hit echo          same as `hit` (likely)
[0x200] varint            HP the attacker drained
[0x400] varint            HP the attacker recovered (confirmed: heals its attacker)
[0x800] u32, [0x1000] u8, [0x2000] u32
```

The fields from `0x08` on follow Questlog's SkillEffect: with them all but 308 of 197,103
hits in the 2026-10-23/24 captures read to the last byte (before: 1,205 partly read). HP
drained and recovered are healing for the attacker, as Questlog counts them: 6,635 hits
recovered 3.7M HP (Gladiator skills mostly). A hit with no damage and an evade, resist or
immune result is a miss (accuracy in the analysis window). The damage value doesn't
depend on the later fields: its position depends only on bits `0x01`, `0x02` and `0x04`.

- **No damage field** (bit `0x04` clear): buffs, ticks and effects, often with
  `target == attacker`.
- **Damage you take** shows up with `target` = your entity ID.
- **The `0x02` block is optional**: hits from the side with no perfect or double strike
  leave it out (and so have no `0x02` flag).

### Hit outcomes (confirmed 2026-10-20)

Worked out on the training dummy, one variable at a time, from our own captures:

| test (capture) | what changed | what moved |
|---|---|---|
| Onslaught ×10 in front (`20261020-184342`) | — | position byte 2 on every hit |
| Onslaught ×10 from behind (`…184550`) | position | position byte 1 on all 29 hits |
| Onslaught ×10 from the side (`…184817`) | position | position 0; block usually absent |
| any | — | `result` 3 ⇒ that hit does ≈ 1.7× (crit); `special` 0x08 ⇒ exactly 2× |

`cast_id` groups hits: each Onslaught cast's two hits share one, each Resonance Crush
cast's five hits share one, and every cast seen in hits has a matching cast message
(`0x3802`, below) with the same caster, skill and ID (153 of 153 in the reference fight).
It counts up and wraps at 255. Count casts with it, not with `hit`: some skills' hits
never have `hit = 1`.

Then a normal 90-second rotation (`…190034`) recorded alongside another meter's
per-skill table for the same fight, used only as an answer key. Our numbers match it
exactly, in total and for every skill:

| | that meter | ours |
|---|---|---|
| damage / casts | 2.34M / 154 | 2,342,182 / 154 |
| crit / back / front | 46.0% / 37.7% / 25.7% | 46.0% / 37.7% / 25.7% |
| perfect / double | 19.2% / 4.9% | 19.2% (`0x04`) / 4.9% (`0x08`) |
| hits / multi-hit | 606 hits; Multi% per skill | 265 events, 606 counting parts; hits with ≥ 2 parts match its Multi% for all 10 skills |
| min / max per skill | e.g. 13.4K / 54.2K | 13,374 / 54,196 |

Parry hasn't been seen yet (dummies don't parry); it's probably another `special` bit.
`special` 0x80 is common on hits players take and isn't identified yet.

## Casts: opcode `0x3802` (confirmed)

```text
varint  caster
u8      flags             bits add fields after the target
u32     skill             mob skills are 6–7 digits, player skills 8
u8      cast_id           same as the cast's hits in 0x3804
u8      ?
varint  target
…
```

Sent for every cast in view, players' and mobs'.

## Damage and heals over time: opcode `0x3805` (confirmed)

```text
varint  target
u8      flags            0x02 amount present; 0x01 heal (see below)
varint  attacker
varint  instance         the effect's instance ID (as in 0x382a/0x382c)
u32     unknown
[0x02]  varint amount    damage, or healing when 0x01 is set
[0x01]  varint unknown
u32     skill            (absent on some self-effects with flags 0)
```

**Heals**: Recuperation (an 8 s heal over time) ticked every ~2 s with flags `0x09` (first
tick, no amount) then `0x0b`, healing 976, 651, 326, 1 as your HP filled; the ticks
carry the buff's instance ID. Damage ticks use `0x0a`/`0x08`. Across all captures, flag
`0x01` only appears on heals, so the meter doesn't count those ticks as damage.

The `attacker` of a heal tick is the entity the effect sits on, not always the healer.
Chanter skill `18730002` is a debuff on the mob that heals whoever hits it: its ticks
name the training dummy (37109) as attacker and the player as target. The meter credits a
heal to the `source` of the effect instance the tick carries (from `0x382a`), which in
`…185504` gives `18730002` to the Chanter (8355) who applied it. Heals on other players
are in our captures too (Elementalist `16190040`, Cleric `17090xxx` "Light of
Regeneration").

**Heals a player casts on themselves** come as a `0x3804` "hit" with `target ==
attacker` and an amount (flags `0x0004`), e.g. Cleric Healing Light `17100010`. In test D
the Cleric Rafaela (15585) sat at 16,381 HP; a mob hit her for 745 (→ 15,636), then
Healing Light "for 1047" put her back to exactly 16,381 in the same millisecond: the
amount is the heal before the max-HP cap. Shields look the same (Sorcerer Steel Barrier
`15160007`, one event of exactly 30,000) but never raise HP. Across our captures, 13
self-targeted skills raised HP at least once (Healing Light, Radiant Recovery, Radiant
Benediction, Blood Absorption, Protection Armor…); the meter counts a self-heal only
when the player's next `0x8d00` (within 50 ms) shows a rise, and counts the rise.
`1010000` "Restore HP" is natural regeneration and isn't counted; mobs and summoned
spirits use `1699xxxx` creature skills on themselves. `packet-lab damage --self-target`
lists these events.

**Heals cast on other players** come the same way, with the ally as `target`: in a
600055 run (2026-10-24), 1,016 of 1,850 player skills on other players were followed by
that player's HP rising by the amount (less when nearly full; 893 within 50 ms), e.g. a
Cleric's `17800001`. The meter counts them like self-heals, and never counts a player's
skill on another player as damage (player fights are the PvP tracker's).

**Damage taken**: mob hits on players are ordinary `0x3804` damage events with a player
as `target` (flags `0x0006`); test D has 71 of them across 6 players.

997 of 998 messages in our captures parse to the last byte. NOIA2's code reads the skill
before the damage; in the current client the damage comes first. These ticks used to be
missing from our totals.

### Verified against in-game numbers

| Test | Noted in game | Found |
|---|---|---|
| B (dummy, skill 18010340) | 3034, 2674, 1932, 2886, 3084 | all exact |
| C (dummy, skill 18040350) | 11096, 10380, 10336 | 11096, 10380 exact; 10338 (likely misread) |
| D (mobs hitting you) | 435, 490, 386, 286 | 435, 490, 386 exact; **286 not found** |

## Entities

Entity IDs are per login (yours was 10397 in one login and 3199 in the next). The same ID
space is used by damage events, spawns and the self message.

### Self: opcode `0x364a` (likely)

```text
varint  your entity ID, then 9 bytes (mostly zero)
```

Arrives about once per second, and only ever names one entity: you. Seen in all 7 sessions
across 2 logins, including while in a group. A meter can learn your ID from it within a
second of starting, even mid-session.

### You appear: opcode `0x3633` (confirmed)

```text
varint  your entity ID
5 bytes unknown
u8      name length, then your character name (UTF-8)
...     about 2.3 KB more (your full character data)
```

Sent **at login and after every zone change**, always with the current entity ID. A
zone change keeps the same entity ID; a re-login gets a new one.

### Another player appears: opcode `0x3645` (confirmed)

Same layout as `0x3633`: entity ID, 5 bytes, length-prefixed UTF-8 name, then 1–2 KB
more. Latin, Korean and Chinese names seen.

It's sent when a player **comes into view**, and for **everyone nearby right after a zone
change**. Players already nearby when the capture starts stay unnamed until one of those
happens.

### Mob / NPC appears: opcode `0x3641` (likely)

```text
varint  entity ID
3 bytes unknown
u32     type ID, e.g. 2400009 (same kind of mob → same ID; 24xxxxx and 29xxxxx seen)
u8      flags             0x40, 0x08 seen; 0x08 adds 12 bytes (below)
u8      unknown (always 0x02)
f32 ×4  probably position (x, y, z) and heading
u16     unknown
[0x08]  12 bytes          probably a second position (where it's walking to?)
u8      1 alive, 0 a corpse
varint  hp                (confirmed, see 0x8d00)
varint  max hp            (confirmed, see 0x8d00)
...     about 100 bytes total
```

Arrives before the mob's first damage event. **There's no name in it**: the client looks
names up in its own data files, so a meter needs its own type-ID → name table built from
our observations. A few messages have a different layout (type field doesn't fit): named
NPCs have `1f 00 01`, a length-prefixed name, then the type.

HP and max HP read for 418 of the 437 spawns in our captures; the other 19 are named NPCs.
Every one of the 99 spawns later followed by `0x8d00` updates has all of them at or below
its max. Mobs in the open world were all 119,700 max; a mob that comes into view mid-fight
says so (test D, mob 17151: `e3 d6 04 94 a7 07` = 76,643 of 119,700), and corpses come
in as `00`, HP 0.

### Mobs made in a player's likeness (confirmed 2026-10-22)

Some `0x3641` spawns carry a party member's name:

```text
varint  entity ID
1c 00 01
u8      length, then the player's name (UTF-8)
u32     mob type (2920364 Gluttonous Tiere, 2920371 Thamon, 2920360 / 2920663 Vakron's
        Thorn Root in Vakron Sky Island)
...     a position, the summon mark below with its own ID, no owner
```

Bosses copy the party: in Vakron Sky Island Tiere made six of each member and Thamon
three, and the copies attack the party with mob skills (`1604521`, `1604531`). They're
mobs: your hits on them are damage dealt, theirs on you damage taken. Read as players they
made a party of five show as eleven.

### Summoned creatures and their owners (confirmed 2026-10-20)

Elementalist spirits, Cleric servants and Sorcerer summons arrive as mob spawns
(`0x3641`; spirits have types `2920xxx`) and attack in their own name, with their own
entity IDs. Before this, a spirit showed up as its own unnamed "#ID" Elementalist row, and
most of its damage was lost: its basic attacks use 6-digit skills (`100014` … `100055`),
which the meter took for mob skills.

Every spawn has, after eight `ff` bytes and the u64 `80 75 d5 2a bb 03 00 00` (ms since
the epoch: 2100-01-01 00:00 KST, i.e. "never"), a varint entity ID:

```text
ff ×8, u64 "never"
varint  who        the mob itself; for an Elementalist spirit, its owner
u8                 01 for mobs; 02..14 for spirits
u8      flags      02 on summons
12 bytes           probably a position (3 × f32)
summon tail:       07 02 06 u32 owner      (Cleric/Sorcerer summons, Elementalist spirits)
                or 07 02 01 / 09 01 / 08 01 u32 owner   (other spirits)
```

- **Spirits** (header `5f 10 00`, `1f 10 00`, `1d 10 00`): `who` is the owner, and the
  same ID comes again as a u32 within the next 24 bytes. 138 spawns in 16 captures; every
  one whose owner fought belongs to a player with Elementalist (16xxxxxx) skills, and in
  the `5f` form a server ID and the owner's name follow (`c7 09 06 "PUNION"`, owner 3826
  in test B). E.g. `20261020-184039`, spirits 49775, 25691, 38651 → 55; `…185747`, spirit
  21466 → 1802 (`8a 0e` … `07 02 06 0a 07 00 00`).
- **Other summons**: `who` is the summon itself, and `07 02 06 u32` after the position
  names the owner. 12 spawns: Cleric servants (skills 17150xxx) → Clerics (40634 →
  Morzetic 14637 in `…184817`; 36037 → 6910 and 54599 → 3600 in test D), Sorcerer
  summons (15280xxx) 39405, 45749 → Sorcerer 14632 in `…190034`.
- Ordinary mobs have their own ID as `who`, or no mark at all (287 spawns, none with an owner).

A spirit that was summoned before the capture started has no spawn. Its owner shows in
its hits instead: these skills from a spirit land **only on its owner** — 16990002/3
after each basic attack (damage 20), 16107000, 16257000 and 16780002–16780006: 610 hits
by spirits with a known owner, every one on that owner. Spirits without a spawn use them
on exactly one entity each, always an Elementalist (12 spirits in 7 captures, e.g. 41727
→ 55 in `…184817`, 65 times). Owners also cast Spirit Communion (16770001) on their spirits.

The spirits' 6-digit hits are real damage: 101 of 108 that landed alone on a training
dummy between two `0x8d00` updates took 4.5 × the damage off its HP (see HP below).

The meter credits a summon's hits to its owner (the owner's skill list shows the
summon's skills), and leaves out hits between a player and their own summons (the echoes
and Spirit Communion). A spirit's hits before its first echo can't be placed: in
`…184817` that's 1 hit of 38.

### A death: opcode `0x8d04` (confirmed 2026-10-22)

```text
varint  the entity that died
u32     the skill that did it (0: none)
varint  who did it (0: nobody named)
u16     a player killer's server
u8      length, then a player killer's name (UTF-8; 0: none)
u8      length, then their legion
u16
varint  a mob killer's type
u32, u8, u8
```

Our notes had this as "a player engages a mob" (it names the player in open-world kills,
which is how it named you). Questlog's message list called it a death, and the Vakron
Sky Island capture agrees: all 101 arrive with the entity's HP at 0. In that dungeon
kills carry no player; a party member's death names the mob that did it.

### Combat state, NPCs leaving and the game's ping (confirmed 2026-10-22)

| opcode   | layout | what |
| -------- | ------ | ---- |
| `0x8d21` | varint entity, u8 flags, u8 state, flag 1: u64 berserk time (ms since 1970) | in combat (1) or out (0); a boss's carries when it goes berserk (Vakron's: 5 minutes after the pull) |
| `0x3642` | varint entity, u8 flags, u8 reason, flag 1: u32 | an NPC left view: reason 3 = it died (all 97 deaths in Vakron), 1 and 7 others |
| `0x3603` | u16 result, u64 client time, u64 server time | the game's ping coming back, every 10 s |

The client time in a ping is the client's clock: milliseconds since the PC started (its
high-resolution counter) plus 2^24 seconds (16,777,216,000 ms). The game's ping is when
the reply arrives, on that clock, minus that time; the meter works it out the same way.
In the Vakron dungeon it came to 60-176 ms (median 123). On the world server the replies
came at a steady 310-320 ms in two sessions and 39 ms in another, which looks like the
server's own timing more than the network.

Fights end the way the game says: on the boss's death (`0x8d04` / `0x3642` reason 3),
when its HP is back to full after taking damage (a reset), when everyone in the party has
left combat (`0x8d21`), after 15 s without damage (90 s while a boss is alive and in
combat), or on a map change (`0x4002`).

### Casts with a cast time, revivals, middle bosses and your skills (confirmed 2026-10-23)

Named in Questlog's message list, checked on our captures:

| opcode   | layout | what |
| -------- | ------ | ---- |
| `0x3809` | varint caster, u8 flags, u32 skill, u8 level, target block (as `0x3802`), varint cast time (ms), u8, [flags 1: varint], [flags 2: varint attack speed], … | a cast with a cast time starting, by players and mobs (bosses too) |
| `0x380c` | varint caster, u16 result, u32 skill, varint cooldown (ms) | that cast stopped before it went off (its cooldown starts) |
| `0x381c`, `0x3821` | as `0x3809` (no cast time) / as `0x380c` | a charged skill started / let go (its cooldown starts) |
| `0x3848` | varint caster, u32 skill, u8 cast ID, u32, u16, f32 × 3 position, f32 facing, u16 time until it lands (ms) | where a cast will land, one per area, with the cast's `0x3802` |
| `0x8d52` | varint entity | a mob took aggro for the first time (a boss's pull) |
| `0x8d26` | varint entity | a player came back to life |
| `0x572e` | u32 mob type | a dungeon's middle boss was killed |
| `0x5100` | u8 count, entries of a tag, u32 id and more by kind | your skills, at login and on entering a dungeon (just before `0x3633`) |

Of the 184 cast starts followed by the caster's `0x3802` of the same skill, all went off
within 20 % (or 300 ms) of their cast time; the times are round (1,000, 1,500, 2,000,
10,000 ms). All 23 `0x380c` stop the caster's open cast of that skill. In Vakron Sky Island
the 4 revivals each follow that player's death by 4–8 s, and `0x572e` came the moment Tiere
(2300810) and Thamon (2300811) died; the last boss ends the dungeon with `0x6101` instead.
Nazmun's (2340615) casts show the pattern the meter's boss cast line follows: a cast time
(`0x3809`, 1,030 ms), the cast going off on time (`0x3802`), and with it its areas
(`0x3848`, six at once landing 1.9-2.7 s later). Boss timelines run from the boss's pull
(its `0x8d21` in combat, or `0x8d52`), so they line up from one fight to the next.
The skill list's entries vary in length, and not all are skills; its 30 base skill ids
(`xxyy0000`) are all of the character's class. The meter uses them for the target frame's
cast bar, revivals in the deaths tab (and to stop counting a revived player as dead), a
second sign that a middle boss died, and your long cooldowns in the party cooldowns window
before you've used them.

### Dungeon stages, content tickets, the Abyss artifacts and the Force

| opcode   | layout | what |
| -------- | ------ | ---- |
| `0x922e` | u32 event ID, u8 length, its name, u8 state, varint stage, varint step, … | joining a map event (a dungeon's objectives) |
| `0x922d` | u32 event ID, u8 state (2 running, 3 done), varint stage, varint step, u8 count, that many objective flags, … | its progress (confirmed 2026-10-23; Vakron's stage goes up after each middle boss, Fire Temple's stays 1 and its step goes up, so stages are split at `0x572e`) |
| `0x922f` | u32 event ID | leaving it |
| `0x610b` | varint count, then tickets | every content ticket (confirmed: 73, every byte read) |
| `0x610c` | u8 flags, a ticket, u8 reason, flag 1: i32 | one ticket changed (confirmed) |
| `0xe307` | u16 0, u8 wars, wars, u8 artifacts, artifacts | the Abyss map: who holds each artifact (confirmed 2026-10-23: 144 bytes, 2 wars, 6 artifacts) |
| `0xe305` | a war, u8 artifacts, artifacts | entering the Abyss: that layer's (same) |
| `0x9602`, `0x960a`, `0x961b`, `0x961d`, `0x961e` | members as in a party roster: u32 character ID, `00 00`, u16 server, u8 length, name | the Force's roster and members (confirmed 2026-10-23: a 20-member roster) |
| `0x9622` | u64 member (u32 character ID, `00 00`, u16 server), u64 party, u8 reason | a member left the Force (confirmed) |

A ticket is u8 flags, u32 ID, then by flag 1 an i64 time, 2 an i64 time, 4 a varint count,
8 a varint charged count. Odyle energy is ticket 60000001 (count basic, charged charged): it
fell by 80 as each dungeon was completed. The other tickets' contents aren't named in any
data we have.

In both Vakron Sky Island runs the main map event (`BakronIsland_Normal_MapEvent`) went to
stage 2 just after Tiere died and stage 3 about 20 s after Thamon. Fire Temple's
(`Fire_Temple_Normal_MapEvent`, six runs on 2026-10-23) stays at stage 1, its step going
1 -> 2 as Red Spark Ignus dies and 2 -> 3 about 30 s after Black Smoke Murute. So the meter
starts each stage at the dungeon's start (`0x6100` step 2) and each middle boss's death
(`0x572e`), and the history shows each stage's time.

A war is u8 (1: an "until" follows), u32 AbyssArtifactEvent (1001 the first layer, 2001 the
second), u8, u8, u64 since (ms), then the u64 until; an artifact is u32 ID (1001-1003, 2001-2003), u32, u8 owner
(1 Elyos, 2 Asmodians, 0 nobody), 8 bytes. A message is only read when every byte fits.

The meter reads the Force's members where they read as roster entries (server 1000-2999, a
name of 2-24 letters and digits), drops one at `0x9622` and all of them at `0x961a` or
`0x962e`: a player whose name is a member's counts as your party. In the 2026-10-23 capture
the Force's 20 members churned (many `0x9622`), and in its open-world fights most of the
dozen players nearby weren't in it.

Bosses' casts (`0x3802`, and `0x3809` / `0x380c` for those with a cast time) are kept per
fight for the boss casts window.

### Hidden cubes and where you are (confirmed 2026-10-24)

| opcode   | layout | what |
| -------- | ------ | ---- |
| `0x3634` | varint entity, u8 flags, u8 desc flags, [desc 1: varint length, string], u32 object type, [2: u8], [4: u8], [8: u8, u32], [16: u8], f32 × 3 position, … | a world object came into view (Questlog's EnvObjSpawn) |
| `0x3635` | varint entity, u8 state, u8 reason, varint | it left view; reason 3 (the game's "Dead") when it was used up |

Hidden cubes are objects of 19 types (Questlog's list, gold and red); a red one (1301199)
is in the Force session, 28 m from you and 66 m up. Coordinates are centimetres.

The game never sends your own movement: every `0x371a`-`0x371c` in our captures is
another player's, and party updates (`0x921c`) leave you out. Your position is in your
`0x3633`, 164 bytes after your name, after a u16 of flags starting `0x92` (37 of 40 in our
captures; the other three have none there), so it's known only after you log in, teleport
or change zone. As Questlog does, the meter trusts it for 60 s for a cube's distance and
direction, taking north as -y.

### Pet souls (confirmed 2026-10-24)

| opcode   | layout | what |
| -------- | ------ | ---- |
| `0x9000` | u32, varint n, n × (u32 key, u32 pet, u32 level), varint n, n × (u32 pet, u32 souls), … | your pets and their souls (each connection) |
| `0x9009` | varint entity, u32 pet, u32 level, … | a pet summoned |
| `0x900d` | varint n, n × (u32 pet, u32 souls) | souls gained: 1 a kill (23 in our captures) |
| `0x900e` | the same | souls spent: 5 to summon a pet (with its `0x9009`) |

Names are Questlog's (RideInitData, CharRideAcquire, RideSoulPointIncrease/Decrease). A
pet's ID is the mob it comes from's kind (1091, Red Spark Ignus, from Fire Temple's first
boss). It takes 5 souls to summon, then 25 and 75 for its next two levels (Questlog's table;
the scraped pet data says "Souls to summon: 5").

### Classes (confirmed)

The first two digits of a player's skill IDs give the class: 11 Gladiator, 12 Templar,
13 Assassin, 14 Ranger, 15 Sorcerer, 16 Elementalist, 17 Cleric, 18 Chanter, 19 Fighter.
(30xxxxxx are Theostone item effects, not a class.)

### Other messages (from NOIA2, not yet used)

- `0x3656`: your combat power (u32 after the opcode).
- `0x9101`: field boss timers.
- `0x3822`: about 14.5 s after each Recuperation cast; probably "skill ready again".

### Permanent character ID (guess)

`58 de 03 00` (u32 253528) followed by `00 00 00 c7 09` sits right before
"NeneneTowawa" in several messages (`0x8a05`, `0x8d57`, party lists). It looks like a
character ID that survives logins, and `c7 09` may be a server ID.

The training dummy was 37109 in these sessions; it was already loaded before every
capture, so its spawn message hasn't been seen.

### Not entity data

- `0x9701`: party-finder listings (post titles, leader names). No entity IDs.

### Party and group rosters

Used for the meter's *party* scope. Members are listed by name and a longer-lived
character ID (u32), not by entity ID, so the meter matches them to rows by name.

| opcode   | what                                                  |
| -------- | ----------------------------------------------------- |
| `0x9200` | your party's full roster, resent on most joins/leaves |
| `0x920d` | one member joining your party: a party roster entry (Questlog's PartyAddMember; confirmed 2026-10-24, two joined this way with no new roster) |
| `0x9222` | a party member back online: the same (Questlog's PartyReconnectMember) |
| `0x9213` | you left the party: u16 result (only 0 means you left; Questlog's PartyLeave) |
| `0x9702` | a cross-server group's full roster (expeditions)      |
| `0x970b` | one member joining that group                         |
| `0x971d` | you left the group: u16 result (the same)             |
| `0x971e` | a member left the group: u32 character ID, `00 00`, u16 server |
| `0x9221` | a member left the party (same layout)                 |
| `0x9716` | a group member readied up or stopped: character ID, `00 00`, server, u8 ready |
| `0x9715` | you readied up or stopped: `00 00`, u8 ready          |
| `0x9211` | the party's leader changed: character ID, `00 00`, server (Vakron: to us near the end) |
| `0x921f` | the party broke up: u8 reason (Vakron's end: 8)       |
| `0x972a` | the party finder group removed you: u8 reason (the expedition capture) |
| `0x9219` | you were kicked from the party: u8 by vote (Questlog's list; not captured yet) |
| `0x931f` | you left the party: u64 party, u8 reason (3 leaving after an expedition's run, with `0x971d`; confirmed 2026-10-23) |
| `0x961a`, `0x962e` | the Force (a 20-player raid group) broke up; you left it (Questlog's list; not captured yet). No `0x96xx` Force message is in any capture yet: we haven't been in a Force, and expeditions are the party finder's `0x97xx` groups |

Each member entry: `u32 character ID`, `00 00`, `u16 server ID` (1501..2507 seen), `u8
length`, UTF-8 name, then level, class, legion name and other fields. In group rosters
(`0x9702`, `0x970b`) two bytes come first: flags (`0x0c` always, `0x02` ready, `0x10`
inside the dungeon) and the slot (1 = the leader, who starts rather than readies).
Seen in the Vakron capture's party finder: robi03, TAQE and Chandra readied (TAQE
unreadied and readied again) by `0x9716`, ours by `0x9715`, and robi03 leaving at the end
by `0x971e` and `0x9221` together. A kick hasn't been captured; the meter takes
`0x9219` and `0x972a`, and `0x9221`/`0x971e` with your own character ID, as you being removed. Entries aren't at
fixed offsets, so the meter scans for that shape (`entities::parse_roster`).

After the name, **group** rosters (`0x9702`) have: u32 (not the class), u32 level, u32
**gear score**, the server again (one entry had an extra byte first), 3 bytes, u32 **combat
power**. **Party** rosters (`0x9200`) have HP, MP, the map, a position and the legion name
first, then u32 level, u32 0, u32 gear score, u32 combat power (one entry of five had 8
more bytes before combat power). Checked against the game for our character (gear score
2,023, combat power 88.27K) in the Vakron Sky Island capture.

`0x921c` (**a member's update**, resent every few seconds, never for you):

```text
u32     character ID
00 00
u16     server
u32     (not the class)
12 B    zeros
12 B    position (3 floats)
u32     level
u32
u32     gear score
u8      length, then the legion name (0: none)
u32     (0 or 1)
u8
u32     combat power
```

All 1,333 in the Vakron capture parse this way and match the roster's numbers. Seen in the
"joining a party and leaving a party" capture (5 → 4 members, leave, rejoin with a new
member, leave) and in the expedition capture (several groups joined and left). A `0x9702`
without your own name is someone else's group and is ignored.

## HP: opcode `0x8d00` (confirmed)

NOIA2's notes call this "remaining HP of an entity". Worked out again from our captures:

```text
varint  entity
u8      mask
[0x01]  u8 n, n × (u8 stat, u32 value)     stats 1, 3, 4, 6, 8, 10–13 seen; unknown
[0x02]  u8 n, n × (u8 stat, u64 value)     stat 0 = current HP; stat 7 unknown
```

All 11,377 messages in the 19 captures parse to the last byte with this layout (mask
`0x02` 10,329, `0x01` 1,019, `0x03` 29). Sent for every entity in view whose stats change:
mobs, training dummies, other players and you. `packet-lab hp <session>` walks a capture
and compares each entity's HP drops with the damage (`0x3804` and `0x3805`, heals left
out) that landed on it in between; `--on <entity>` lists them.

- **Players** lose exactly the damage dealt: 97 drops in the captures match the hits
  1:1 (e.g. you in test D, hit for 435, 490, 386 by mobs).
- **Mobs and dummies** lose **4.5 HP per point of damage shown**. Test D, mob 47144:

  | hit | HP before → after | fell |
  |---|---|---|
  | 402 | 115,448 → 113,639 | 1,809 = 4.5 × 402 |
  | 2,626 | 113,639 → 101,818 | 11,821 (4.5 × 2,626 = 11,817) |
  | 3,130 | 101,818 → 87,733 | 14,085 = 4.5 × 3,130 |
  | 1,400 | 68,505 → 62,205 | 6,300 = 4.5 × 1,400 |
  | 2,752 (the kill) | 2,899 → 0 | 2,899 (overkill) |

  Across the 84 mobs that died in our captures, 749 of their 895 HP drops are 4.5× the
  damage within 0.5%, and 84 more are the killing blow (overkill). The other 62 haven't
  been looked at one by one. Damage values themselves match the game's numbers (see
  above), so it's HP that's in other units; the meter only shows the fraction of max HP.
- **HP reaches 0 when a mob dies**: 88 entities in our captures: 85 mobs in test D, test E
  and the zone change capture, and 3 small objects with 6 HP in the party capture (hit for
  1 each, and their HP fell 1 at a time to 0).
- **Training dummies** have 119,700 max, never go below 1, and jump back to 119,700 soon
  after reaching it, so their HP line is a sawtooth.
- **Max HP** isn't in this message. It's in the mob's spawn (`0x3641`, above): mob 31865
  spawned at 119,700 of 119,700, and its first `0x8d00`, after a 556 hit, said 117,198
  = 119,700 − 4.5 × 556. For anything without a spawn, the meter uses the highest HP seen.

## Buffs and debuffs: opcodes `0x382a` / `0x382b` / `0x382c` (confirmed)

```text
entry:  u8 kind          0x13 (has a skill) or 0x11 (no skill: system effects)
        varint instance  one application on one target
        u32 effect       9 digits starting with the skill family (Recuperation 181200101)
        u64 duration_ms
        u64 time_ms      server time, ms since the Unix epoch
        varint source    who applied it
        u8
        [0x13] u32 skill
        …                tail of varying length

0x382a  apply:        varint target, varint count (always 1 so far), entry
0x382b  still active: varint target, entry      (about once a second per long effect)
0x382c  removed:      varint target, varint count, then per effect
                      00 varint instance u8
                   or 07 varint instance u8 varint source u32 skill u32
```

Evidence from the live tests (`packet-lab buffs <session> --on <entity>`):

- Recuperation (8 s, self) ×3 (`…185504`): each apply has `duration 8000`, and a remove
  for the same instance arrives 8.0–8.1 s later. Each cast also applies a 200 ms effect.
- Earth's Promise (5 s debuff on the dummy, `…185747`): spans of 4.9–5.1 s, `duration
  5000`. Another Chanter was hitting the same dummy: their applications show their
  entity as `source`, and yours didn't land while theirs was up (same debuff, one at a
  time).

### Effect IDs and names

An effect ID is the ID of the skill behind it with one more digit: Recuperation's
`18120010` gives `181200101`; Theostone skill `3001015` gives `30010151`. That's how
effects without a skill (kind `0x11`, e.g. `183300071`, family 1833 Marchutan's Wrath)
get a name. Theostone skills arrive as 7 digits (`3001013`, `3001015`, `3001016` seen in
our captures), but the game data lists them by their effect ID (`30010151` "Theostone:
Santras's Secret"): in `…190034`, skill `3001015` applied effect `30010151` on 20644.
Short IDs like `231` are system effects with no name.

## Skill IDs (guess)

The 8-digit IDs look structured: `18 01 034 0`, `18 02 034 0`, `18 04 035 0`. Possibly
class (18), skill number, then level or rank. `18800001` fires regularly between skills
and may be the basic attack. Needs a name table from our own observations.

## Open questions

1. Where are class and level in `0x3633`/`0x3645`? (Your character: level 45, 88.36K
   combat power, title "Vanguard of Atreia" — search for these to find the fields.)
2. Where did the 286 damage in test D go? Not a DoT tick (checked); maybe a shield
   absorb or a different opcode.
3. What are `unknown_a` and `unknown_b`?
4. Parry: probably another `special` bit; needs a fight where an enemy parries.
5. `result` values 0, 1, 6 and `special` bits 0x01, 0x02, 0x10, 0x20, 0x40, 0x80: what
   are they? (0x80 is common on hits players take: block? shield?)
6. Opcodes `0x371d`, `0x3600` are among the most frequent; probably movement/state.
7. Why do mobs lose 4.5 HP per point of damage? Is it the same for bosses and in dungeons?
8. `0x8d00`'s u32 stats and u64 stat 7: one of them is probably MP (stat 3 on you counts
   down by 500 at a time).
9. Summon spawns: what are the byte after the owner (02..14) and the `07 02 06` /
   `07 02 01` / `09 01` tags? A spirit that's already out when the meter starts and never
   uses an echo skill (e.g. `#39331` in `…184039`, one hit) still gets its own row.

## Maps and dungeons (confirmed 2026-10-22)

Worked out from a Vakron Sky Island run with screenshots every 3 seconds. Entering the
dungeon moved the game to **another server connection** (the world's stopped the same
second), and leaving moved it back; each connection starts with `0x4002`. Fire Temple
(2026-10-23) also sends `0x4002` and `0x6100` step 2 again twice mid-run, as you're moved on
to the next part: the same stay, with the same entity IDs. A new run of the same dungeon
starts with step 1 (or after it was completed).

| opcode   | layout                                              | what                                     |
| -------- | --------------------------------------------------- | ---------------------------------------- |
| `0x4002` | u32 map ID, u8                                      | you're on this map (600072 = Vakron Sky Island, Conquest; 1110 = the world map we left from) |
| `0x6100` | u32 map ID, u8 state, u64 ms, u64 ms               | a dungeon's state (Questlog's DungeonState): 1 on entering, **2 when the countdown ends**, 3 at "Expedition Complete", 4 over; its time limit's start and end |
| `0x6101` | u8 flags, u32 map ID, u8 1 when cleared, rewards…   | its result: only a clear (1) counts as completed, as in Questlog |
| `0x6109` | u32                                                 | deaths so far against the dungeon's limit (Questlog; not used) |

The map ID is also in `0x3633` (your character), `0x9702` (the group roster's header)
and `0x921c`. The meter keeps each stay on a map, and a dungeon's start and completion
(`instances.jsonl`): a history run is the fights of one stay, and its clear time runs
from step 2 to step 3.

## Capturing (how packet-lab and the meter get the traffic)

- **Finding the game:** the Windows connection tables (`GetExtendedTcpTable` /
  `GetExtendedUdpTable`) list the game process's sockets, polled every 500 ms.
- **Capture:** one Npcap handle per adapter the game uses, filtered to TCP/UDP. Packets
  from connections not yet identified are held for up to 3 s, so a new connection's
  handshake and first messages aren't lost.
- **Reassembly:** per-direction TCP streams with out-of-order, retransmission, overlap and
  wraparound handling. Holes that never fill are skipped and recorded as gaps.

A recording (`packet-lab capture`, or **record fights** in the meter) looks like this:

```text
captures/<YYYYmmdd-HHMMSS>/
  session.json   process, adapter and time of the capture
  raw.pcap       every game packet (opens in Wireshark)
  events.jsonl   flows opened/closed, with per-direction stats
  flows/<id>-<proto>-<localport>-to-<ip>_<port>/
    in.bin / out.bin   reassembled bytes received / sent
    index.jsonl        one line per chunk: time, side, kind, stream offset, file pos, length
```

Captures contain your own game session, possibly including login tokens: don't share them.
