# Aion 2 protocol notes

Worked out from our own captures (test sessions of 2026-10-18), not from any other meter.
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
u16     seq               increments per event (likely)
[0x02]  3 bytes           unknown
u32     unknown_a
u32     hit               1, 2, 3… within a multi-hit skill (likely)
varint  unknown_b         constant per attacker for a while (guess: level/stat related)
[0x04]  varint damage     (confirmed against 10 noted in-game numbers)
[0x10, without 0x02] 1 byte
[0x20]  varint n, n × varint parts   values ≈ damage / 45 (meaning unknown)
[0x100] u8, varint        unknown; appears on events without damage
u16     hit echo          same as `hit` (likely)
[0x400] u16               unknown
```

Flag bits `0x40` and `0x1000` add fields we can't skip yet (rare: 8 of about 12,000
events). The damage value doesn't depend on them, because its position depends only on
bits `0x02` and `0x04`.

- **No damage field** (bit `0x04` clear): buffs, ticks and effects, often with
  `target == attacker`.
- **Damage you take** shows up with `target` = your entity ID.

## Damage over time: opcode `0x3805` (confirmed)

```text
varint  target
u8      flags
varint  attacker
varint  unknown
u32     unknown
[0x02]  varint damage
[0x01]  varint unknown
u32     skill            (absent on some self-effects with flags 0)
```

997 of 998 messages in our captures parse to the last byte. NOIA2's code reads the skill
before the damage; in the current client the damage comes first. These ticks used to be
missing from our totals.

### Verified against in-game numbers

| Test | Noted in game | Found |
|---|---|---|
| B (dummy, skill 18010340) | 3034, 2674, 1932, 2886, 3084 | all exact |
| C (dummy, skill 18040350) | 11096, 10380, 10336 | 11096, 10380 exact; 10338 (likely misread) |
| D (mobs hitting you) | 435, 490, 386, 286 | 435, 490, 386 exact; **286 not found** |

## Buffs: opcodes `0x382a` / `0x382b` / `0x382c` (decoded)

Worked out from our own captures (the dummy session), not from any other meter.

| opcode   | meaning                                                        |
|----------|----------------------------------------------------------------|
| `0x382a` | a buff was applied (or a debuff landed)                         |
| `0x382b` | a buff's state was refreshed (re-sent periodically while active)|
| `0x382c` | a buff was removed                                              |

Layout (server → client, after the opcode):

```text
varint  target        the entity the effect is on (confirmed)
varint  count/flags   usually 1
varint  …             apply/remove only; refresh skips this field
varint  buff id       the "abnormal id" (e.g. 1605, 1173, 106478)
…                     apply/refresh only: more fields, then a fixed `4e a1 01 00 00`,
                      then the source entity as a varint
```

- `meter-core/src/buff.rs::parse_buff` extracts **target**, **buff id** and **source**;
  the remaining fields (duration, stacks, …) are ignored for now — the meter only needs
  which effect is on which entity, from whom, and for how long.
- The buff id is a small "abnormal id" (seen: 21…106478), not an 8-digit damage skill.
  NOIA2 (GPL-3.0, at the pinned commit) maps these to names in `abnormal_ids_full.json` /
  `src-tauri/data/buff_templates.json`; that name table still needs wiring in like the skill
  names.
- The source follows a fixed `4e a1 01 00 00` marker in apply/refresh; remove messages
  don't carry one, so spans are keyed by `(target, buff id)`.
- Unit tests use the real captured bytes: apply/refresh/remove on the player, and a debuff
  applied by another player.

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
u8      unknown
f32 ×3  probably position (x, y, z)
...     about 100 bytes total
```

Arrives before the mob's first damage event. **There's no name in it**: the client looks
names up in its own data files, so a meter needs its own type-ID → name table built from
our observations. A few messages have a different layout (type field doesn't fit).

### A player engages a mob: opcode `0x8d04` (confirmed)

```text
varint  mob entity ID
u32     skill used
varint  player entity ID
varint  server
u8      name length, then the player's name (UTF-8)
```

Sent when a player first hits a mob, for every player in view, including you. That makes
it the quickest way to learn your own name without a zone change. It isn't sent for
training dummies.

### Classes (confirmed)

The first two digits of a player's skill IDs give the class: 11 Gladiator, 12 Templar,
13 Assassin, 14 Ranger, 15 Sorcerer, 16 Elementalist, 17 Cleric, 18 Chanter, 19 Fighter.
(30xxxxxx are Theostone item effects, not a class.)

### Other messages (from NOIA2, not yet used)

- `0x3656`: your combat power (u32 after the opcode).
- `0x8d00`: remaining HP of an entity (useful for boss HP %).
- `0x382a` / `0x382b` / `0x382c`: buff apply/refresh/remove (decoded — see the "Buffs" section).
- `0x9101`: field boss timers.

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
| `0x9200` | your party's full roster, resent on every join/leave  |
| `0x9213` | you left the party (`00 00`)                          |
| `0x9702` | a cross-server group's full roster (expeditions)      |
| `0x970b` | one member joining that group                         |
| `0x971d` | you left the group (`00 00`)                          |

Each member entry: `u32 character ID`, `00 00`, `u16 server ID` (1501..2507 seen), `u8
length`, UTF-8 name, then level, class, legion name and other fields. Entries aren't at
fixed offsets, so the meter scans for that shape (`entities::parse_roster`). Seen in the
"joining a party and leaving a party" capture (5 → 4 members, leave, rejoin with a new
member, leave) and in the expedition capture (several groups joined and left). A `0x9702`
without your own name is someone else's group and is ignored.

## Skill IDs (guess)

The 8-digit IDs look structured: `18 01 034 0`, `18 02 034 0`, `18 04 035 0`. Possibly
class (18), skill number, then level or rank. `18800001` fires regularly between skills
and may be the basic attack. Needs a name table from our own observations.

## Open questions

1. Where are class and level in `0x3633`/`0x3645`? (Your character: level 45, 88.36K
   combat power, title "Vanguard of Atreia" — search for these to find the fields.)
2. Where did the 286 damage in test D go? Not a DoT tick (checked); maybe a shield
   absorb or a different opcode.
3. What are `unknown_a`, `unknown_b` and `parts`? Is `parts` a per-tick breakdown or
   something like soul/aggro?
4. Crit/back/front/perfect/double/parry flags: where are they in `0x3804`? From the dummy
   capture: `unknown_b` is constant per attacker (a stat), `unknown_a` is constant per
   (attacker, skill, hit index), and `parts` is `damage / 45` per sub-hit — none is an
   obvious per-hit flag field. The flags need in-game correlation (note a crit or a back
   attack while capturing, then find which byte changes), not just the bytes. Once decoded
   they feed `meter::RotationEvent`.
5. Opcodes `0x371d`, `0x3600`, `0x3802` are the most frequent; probably movement/state.
