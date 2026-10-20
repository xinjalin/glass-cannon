//! Opcode 0x8d00: an entity's current HP (and some other stats we don't read).
//!
//! ```text
//! varint entity
//! u8     mask
//! [0x01] u8 n, n × (u8 stat, u32 value)
//! [0x02] u8 n, n × (u8 stat, u64 value)    stat 0 = current HP
//! ```
//!
//! Sent for every entity in view whose stats change: mobs, dummies, other players, you.
//! Every message in our captures parses to the last byte with this layout. Only HP (u64
//! stat 0) is confirmed: it falls with each hit on the entity (by the damage for players,
//! by 4.5× the damage for mobs and dummies) and reaches 0 when a mob dies.
//!
//! There's no max HP here. Mob spawns (`0x3641`) carry the mob's HP and max HP; for
//! anything else the highest HP seen stands in. See docs/protocol.md.

use crate::protocol::read_varint;

pub const OPCODE_HP: u16 = 0x8d00;
pub const OPCODE_MOB_SPAWN: u16 = crate::entities::OPCODE_MOB_SPAWN;

/// `0x3641` flag bit that adds 12 bytes before the HP (probably a second position).
const SPAWN_EXTRA_POS: u8 = 0x08;
/// Flag bits seen in spawns we can read; anything else is a layout we don't know.
const SPAWN_KNOWN_FLAGS: u8 = 0x08 | 0x40;

const STAT_HP: u8 = 0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HpUpdate {
    pub entity: u64,
    pub hp: u64,
}

/// Parses the payload of an opcode 0x8d00 message (the bytes after the opcode).
/// `None` if it's malformed or doesn't carry HP.
pub fn parse_hp(p: &[u8]) -> Option<HpUpdate> {
    let mut i = 0;
    let entity = read_varint(p, &mut i)?;
    let mask = *p.get(i)?;
    i += 1;
    if mask & 0x01 != 0 {
        let n = *p.get(i)? as usize;
        i += 1 + n * 5;
    }
    let mut hp = None;
    if mask & 0x02 != 0 {
        let n = *p.get(i)?;
        i += 1;
        for _ in 0..n {
            let stat = *p.get(i)?;
            let value = u64::from_le_bytes(p.get(i + 1..i + 9)?.try_into().ok()?);
            if stat == STAT_HP {
                hp = Some(value);
            }
            i += 9;
        }
    }
    if i != p.len() {
        return None;
    }
    Some(HpUpdate { entity, hp: hp? })
}

/// HP and max HP from a mob spawn (`0x3641`):
///
/// ```text
/// varint entity, 3 bytes, u32 type, u8 flags, u8, f32 × 4, u16,
/// [flags 0x08] 12 bytes, u8 (1 alive, 0 a corpse), varint hp, varint max_hp, …
/// ```
///
/// Read for 418 of the 437 spawns in our captures (the rest are named NPCs, which have a
/// name before the type). Returns `(entity, hp, max_hp)`.
pub fn parse_spawn_hp(p: &[u8]) -> Option<(u64, u64, u64)> {
    let mut i = 0;
    let entity = read_varint(p, &mut i)?;
    i += 3 + 4; // unknown, type
    let flags = *p.get(i)?;
    if flags & !SPAWN_KNOWN_FLAGS != 0 {
        return None;
    }
    i += 2 + 16 + 2; // flags, a byte, f32 × 4, u16
    if flags & SPAWN_EXTRA_POS != 0 {
        i += 12;
    }
    if *p.get(i)? > 1 {
        return None;
    }
    i += 1;
    let hp = read_varint(p, &mut i)?;
    let max = read_varint(p, &mut i)?;
    (max > 0 && hp <= max).then_some((entity, hp, max))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(s: &str) -> Vec<u8> {
        s.split_whitespace()
            .map(|b| u8::from_str_radix(b, 16).unwrap())
            .collect()
    }

    #[test]
    fn training_dummy_at_full_hp() {
        // 20261020-184342: the dummy (38937) back at full HP, 119,700, after a reset.
        let u = parse_hp(&hex("99 b0 02 02 01 00 94 d3 01 00 00 00 00 00")).unwrap();
        assert_eq!(u, HpUpdate { entity: 38937, hp: 119_700 });
    }

    #[test]
    fn training_dummy_at_one_hp() {
        // Same capture: dummies stop at 1 HP and reset instead of dying.
        let u = parse_hp(&hex("99 b0 02 02 01 00 01 00 00 00 00 00 00 00")).unwrap();
        assert_eq!(u.hp, 1);
    }

    #[test]
    fn hp_after_a_u32_stat_list() {
        // 20261018-190019 (test D): you (3199), one u32 stat (6) then HP 18,458.
        let u = parse_hp(&hex(
            "ff 18 03 01 06 7b ad 03 00 01 00 1a 48 00 00 00 00 00 00",
        ))
        .unwrap();
        assert_eq!(u, HpUpdate { entity: 3199, hp: 18_458 });
        let u = parse_hp(&hex(
            "ff 18 03 02 03 10 b7 01 00 06 9b 7e 03 00 01 00 e9 46 00 00 00 00 00 00",
        ))
        .unwrap();
        assert_eq!(u, HpUpdate { entity: 3199, hp: 18_153 });
    }

    #[test]
    fn mob_spawn_at_full_hp() {
        // 20261018-190019 (test D): mob 31865, type 2400002, 119,700 of 119,700. Its first
        // 0x8d00, after a 556 hit, says 117,198 = 119,700 - 4.5 × 556.
        let p = hex(
            "f9 f8 01 04 20 00 02 9f 24 00 00 02 7e 21 a2 c7 4c 22 b8 c7 00 fa e8 46 00 04 a1 43 \
             00 e5 01 94 a7 07 94 a7 07 64 00 00 00 64 00 00 00",
        );
        assert_eq!(parse_spawn_hp(&p), Some((31865, 119_700, 119_700)));
    }

    #[test]
    fn mob_spawn_already_hurt() {
        // Same capture: a mob that came into view mid-fight (flags 0x40), at 76,643.
        let p = hex(
            "ff 85 01 05 20 00 02 9f 24 00 40 02 cc 98 a6 c7 d3 ef aa c7 00 d4 e1 46 15 dd 3f 43 \
             70 88 01 e3 d6 04 94 a7 07 64 00 00 00",
        );
        assert_eq!(parse_spawn_hp(&p), Some((17151, 76_643, 119_700)));
    }

    #[test]
    fn mob_spawn_with_the_extra_position() {
        // 20261018-190019: flags 0x08 add 12 bytes before the HP.
        let p = hex(
            "ae 99 01 05 20 00 04 9f 24 00 08 02 8a 77 b6 c7 97 79 c1 c7 f3 e9 08 47 13 9b 42 42 \
             99 22 e8 01 bc 42 42 a3 d5 42 53 e9 b0 c1 01 94 a7 07 94 a7 07",
        );
        assert_eq!(parse_spawn_hp(&p), Some((19630, 119_700, 119_700)));
    }

    #[test]
    fn corpse_spawn() {
        // 20261018-190019: a dead mob coming into view: 00, then HP 0.
        let p = hex(
            "98 c5 01 05 20 00 02 9f 24 00 00 02 09 16 aa c7 50 6f ab c7 00 fc e2 46 f3 ab 91 43 \
             2d cf 00 00 94 a7 07 64 00 00 00",
        );
        assert_eq!(parse_spawn_hp(&p).map(|s| (s.1, s.2)), Some((0, 119_700)));
    }

    #[test]
    fn named_npc_spawn_is_not_read() {
        // A named NPC: the name comes before the type, so the layout doesn't fit.
        let p = hex(
            "d5 98 02 1f 00 01 07 52 61 66 61 65 6c 61 ca 90 2c 00 40 02 80 ae ad c7 80 8f ae c7 \
             00 f0 e5 46 3c 9e 26 43 7c 76 01 d8 65 d8 65 cc 13 00 00",
        );
        assert_eq!(parse_spawn_hp(&p), None);
    }

    #[test]
    fn messages_without_hp() {
        // u32 stats only.
        assert_eq!(parse_hp(&hex("ff 18 01 01 06 7b 30 03 00")), None);
        // u64 stat 7, not HP.
        assert_eq!(parse_hp(&hex("b9 62 02 01 07 33 42 00 00 00 00 00 00")), None);
        // Truncated, and a trailing byte.
        assert_eq!(parse_hp(&hex("99 b0 02 02 01 00 94 d3 01 00")), None);
        assert_eq!(parse_hp(&hex("99 b0 02 02 01 00 94 d3 01 00 00 00 00 00 00")), None);
    }
}
