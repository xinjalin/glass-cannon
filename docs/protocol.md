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
u8      unknown
f32 ×3  probably position (x, y, z)
...     about 100 bytes total
```

Arrives before the mob's first damage event. **There's no name in it**: the client looks
names up in its own data files, so a meter needs its own type-ID → name table built from
our observations. A few messages have a different layout (type field doesn't fit).

### Permanent character ID (guess)

`58 de 03 00` (u32 253528) followed by `00 00 00 c7 09` sits right before
"NeneneTowawa" in several messages (`0x8a05`, `0x8d57`, party lists). It looks like a
character ID that survives logins, and `c7 09` may be a server ID.

The training dummy was 37109 in these sessions; it was already loaded before every
capture, so its spawn message hasn't been seen.

### Not entity data

- `0x9701`: party-finder listings (post titles, leader names). No entity IDs.
- `0x9702`: one party's member list, keyed by a longer-lived character ID (u32) rather
  than the entity ID.

## Skill IDs (guess)

The 8-digit IDs look structured: `18 01 034 0`, `18 02 034 0`, `18 04 035 0`. Possibly
class (18), skill number, then level or rank. `18800001` fires regularly between skills
and may be the basic attack. Needs a name table from our own observations.

## Open questions

1. Where are class and level in `0x3633`/`0x3645`? (Your character: level 45, 88.36K
   combat power, title "Vanguard of Atreia" — search for these to find the fields.)
2. Where did the 286 damage in test D go? Possibly a DoT tick, a shield absorb, or a
   different opcode.
3. What are `unknown_a`, `unknown_b` and `parts`? Is `parts` a per-tick breakdown or
   something like soul/aggro?
4. Crit flag: is it in `flags`, `unknown_b`, or the `0x02` block?
5. Opcodes `0x371d`, `0x3600`, `0x3802` are the most frequent; probably movement/state.
