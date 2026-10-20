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
u8      cast_id           the attacker's cast; shared by all its hits (confirmed)
u8      result            2 normal, 3 critical (confirmed); 0, 1, 6 rare, unknown
[0x02]  u8 special        0x04 perfect, 0x08 double strike (confirmed); others unknown
        u8                always 0 so far
        u8 position       0 side, 1 back, 2 front (confirmed)
u32     unknown_a
u32     hit               1, 2, 3… within a multi-hit cast; some skills start at 2
varint  unknown_b         constant per attacker for a while (guess: level/stat related)
[0x04]  varint damage     (confirmed against 10 noted in-game numbers)
[0x10, without 0x02] 1 byte
[0x20]  varint n, n × varint parts   a multi-hit's pieces when n ≥ 2 (confirmed)
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
