//! Opcode 0x3802: a skill cast, by players and mobs alike.
//!
//! ```text
//! varint caster, u8 flags, u32 skill, u8 cast_id, [flags 1: u8],
//! target: u8 flags2, varint target, [flags2 1: u32 part], f32 facing, [flags2 2: 3 × f32],
//! varint attack speed, u8, [flags 2: varint], varint cooldown (ms),
//! [flags 4: u8 charges left], [flags 8: varint recharge (ms)], …
//! ```
//!
//! `cast_id` is the same number the cast's hits carry in `0x3804`: in our reference fight
//! every one of 153 casts seen in hits had a matching `0x3802`. Mob skills are 6–7 digits,
//! player skills 8. The cooldown and charges follow Questlog's message list; Recuperation's
//! cast reads as 1 charge left, 14.4 s to the next.

use crate::protocol::read_varint;

pub const OPCODE_CAST: u16 = 0x3802;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cast {
    pub caster: u64,
    pub skill: u32,
    pub cast_id: u8,
    pub target: u64,
    /// How long until it can be cast again (0: no cooldown, or it has charges left).
    pub cooldown_ms: u32,
    /// For skills with charges: how many are left, and how long until the next one.
    pub charges: Option<(u8, u32)>,
}

/// Parses the payload of an opcode 0x3802 message (the bytes after the opcode).
pub fn parse_cast(p: &[u8]) -> Option<Cast> {
    let mut i = 0;
    let caster = read_varint(p, &mut i)?;
    let flags = *p.get(i)?;
    i += 1;
    let skill = u32::from_le_bytes(p.get(i..i + 4)?.try_into().ok()?);
    i += 4;
    let cast_id = *p.get(i)?;
    i += 1 + usize::from(flags & 1 != 0);
    let target_flags = *p.get(i)?;
    i += 1;
    let target = read_varint(p, &mut i)?;
    let base = Cast { caster, skill, cast_id, target, cooldown_ms: 0, charges: None };
    // The rest may be cut short (older captures' tests); the cast itself is still good.
    let rest = || -> Option<(u32, Option<(u8, u32)>)> {
        let mut i = i;
        i += if target_flags & 1 != 0 { 4 } else { 0 } + 4 + if target_flags & 2 != 0 { 12 } else { 0 };
        read_varint(p, &mut i)?; // attack speed
        i += 1;
        if flags & 2 != 0 {
            read_varint(p, &mut i)?;
        }
        let cooldown = u32::try_from(read_varint(p, &mut i)?).ok()?;
        let left = if flags & 4 != 0 { *p.get(i)? } else { 0 };
        i += usize::from(flags & 4 != 0);
        let recharge = if flags & 8 != 0 { u32::try_from(read_varint(p, &mut i)?).ok()? } else { 0 };
        Some((cooldown, (flags & 12 != 0).then_some((left, recharge))))
    };
    Some(match rest() {
        Some((cooldown_ms, charges)) => Cast { cooldown_ms, charges, ..base },
        None => base,
    })
}

/// A cast with a cast time starting: like 0x3802 up to the target, then the cast time.
///
/// ```text
/// varint caster, u8 flags, u32 skill, u8 level, target block (as 0x3802),
/// varint cast time (ms), u8, [flags 1: varint], [flags 2: varint attack speed], …
/// ```
///
/// Questlog's SkillCastingStart has the flag-1 field after the cast time (our captures have
/// flags 0, 2 and 6 only).
///
/// Mobs and bosses send it as well as players. In our captures all 184 that were followed
/// by the caster's 0x3802 of the same skill got it within 20 % (or 300 ms) of the cast time.
pub const OPCODE_CAST_START: u16 = 0x3809;

/// A cast stopped before it finished: varint caster, u16 result, u32 skill, varint cooldown
/// (ms; Questlog's SkillCastingStop). All 23 in our first captures stop the caster's open
/// 0x3809 of that skill.
pub const OPCODE_CAST_STOP: u16 = 0x380c;
/// A charged skill let go (Questlog's SkillChargingStop): laid out as [`OPCODE_CAST_STOP`].
pub const OPCODE_CHARGE_STOP: u16 = 0x3821;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CastStart {
    pub caster: u64,
    pub skill: u32,
    pub target: u64,
    pub cast_ms: u32,
}

pub fn parse_cast_start(p: &[u8]) -> Option<CastStart> {
    let mut i = 0;
    let caster = read_varint(p, &mut i)?;
    i += 1; // flags: for fields after the cast time
    let skill = u32::from_le_bytes(p.get(i..i + 4)?.try_into().ok()?);
    i += 5;
    let target_flags = *p.get(i)?;
    i += 1;
    let target = read_varint(p, &mut i)?;
    i += if target_flags & 1 != 0 { 4 } else { 0 } + 4 + if target_flags & 2 != 0 { 12 } else { 0 };
    let cast_ms = u32::try_from(read_varint(p, &mut i)?).ok()?;
    Some(CastStart { caster, skill, target, cast_ms })
}

/// Where a skill will land, shown on the ground (Questlog's SkillRangeNoticeFilterLocation):
///
/// ```text
/// varint caster, u32 skill, u8 cast ID (as the 0x3802 it follows), u32, u16,
/// f32 × 3 position, f32 facing, u16 time until it lands (ms)
/// ```
///
/// One per area: Nazmun's 1605616 came with six at once (1.9-2.7 s), all with its cast.
pub const OPCODE_TELEGRAPH: u16 = 0x3848;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Telegraph {
    pub caster: u64,
    pub skill: u32,
    pub cast_id: u8,
    pub lands_ms: u32,
}

pub fn parse_telegraph(p: &[u8]) -> Option<Telegraph> {
    let mut i = 0;
    let caster = read_varint(p, &mut i)?;
    if p.len() != i + 29 {
        return None;
    }
    Some(Telegraph {
        caster,
        skill: u32::from_le_bytes(p[i..i + 4].try_into().ok()?),
        cast_id: p[i + 4],
        lands_ms: u32::from(u16::from_le_bytes([p[i + 27], p[i + 28]])),
    })
}

/// The caster and skill of a 0x380c.
pub fn parse_cast_stop(p: &[u8]) -> Option<(u64, u32)> {
    let mut i = 0;
    let caster = read_varint(p, &mut i)?;
    let skill = u32::from_le_bytes(p.get(i + 2..i + 6)?.try_into().ok()?);
    Some((caster, skill))
}

/// A stopped cast's or charge's caster, skill and the cooldown it starts (ms, 0: none).
pub fn parse_stop_cooldown(p: &[u8]) -> Option<(u64, u32, u32)> {
    let (caster, skill) = parse_cast_stop(p)?;
    let mut i = 0;
    read_varint(p, &mut i)?;
    i += 6;
    let ms = u32::try_from(read_varint(p, &mut i)?).ok()?;
    Some((caster, skill, ms))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_boss_starts_and_stops_a_cast() {
        // From Vakron Sky Island: Vakron casts 1604380 at a party member (9268), 2,167 ms.
        let s = parse_cast_start(&hex(
            "d1 f2 01 00 1c 7b 18 00 01 02 b4 48 8c fd f9 40 f9 6b 0c c7 1d 89 b5 c6 7a f3 58 47 f7 10 01",
        ))
        .unwrap();
        assert_eq!(s, CastStart { caster: 31057, skill: 1_604_380, target: 9268, cast_ms: 2167 });
        // With flags 2 (more after the cast time): 1,333 ms.
        let s = parse_cast_start(&hex(
            "d1 f2 01 02 30 7b 18 00 01 02 8b 26 a0 d7 1b 41 ca 39 16 c7 62 57 b5 c6 00 d9 58 47 b5 0a 01 90 4e",
        ))
        .unwrap();
        assert_eq!((s.skill, s.target, s.cast_ms), (1_604_400, 4875, 1333));
        assert_eq!(parse_cast_stop(&hex("d1 f2 01 08 18 3a 7b 18 00 98 75")), Some((31057, 1_604_410)));
        assert_eq!(parse_stop_cooldown(&hex("d1 f2 01 08 18 3a 7b 18 00 98 75")), Some((31057, 1_604_410, 15_000)));
    }

    #[test]
    fn where_a_skill_lands() {
        // A Fire Temple trap's (mob 2300301, 2026-10-23): 2,850 ms until it lands.
        let t = parse_telegraph(&hex("9e a0 04 6e b6 12 00 02 01 00 00 00 01 00 d3 aa f5 45 1b ae 76 c5 00 60 79 45 00 00 00 00 22 0b")).unwrap();
        assert_eq!(t, Telegraph { caster: 69_662, skill: 1_226_350, cast_id: 2, lands_ms: 2850 });
        assert!(parse_telegraph(&hex("9e a0 04 6e b6 12 00 02")).is_none());
    }

    fn hex(s: &str) -> Vec<u8> {
        s.split_whitespace()
            .map(|b| u8::from_str_radix(b, 16).unwrap())
            .collect()
    }

    #[test]
    fn your_recuperation_cast() {
        // From the live dummy tests: you (11792) cast Recuperation on yourself, cast 0x92.
        let c = parse_cast(&hex("90 5c 0c b8 7d 14 01 92 00 90 5c 8b 70 e9 c2 96 56 01 00 01 c0 70")).unwrap();
        assert_eq!(
            c,
            Cast {
                caster: 11792,
                skill: 18_120_120,
                cast_id: 0x92,
                target: 11792,
                cooldown_ms: 0,
                charges: Some((1, 14_400)),
            }
        );
    }

    #[test]
    fn cast_on_a_target() {
        // Earth's Promise on the training dummy (25002).
        let c = parse_cast(&hex("90 5c 00 61 8f 1e 01 e0 00 aa c3 01 8c fa 71 43 96 56 01 00")).unwrap();
        assert_eq!((c.skill, c.cast_id, c.target), (18_780_001, 0xe0, 25002));
    }

    #[test]
    fn mob_cast_with_a_short_skill_id() {
        let c = parse_cast(&hex(
            "c0 8b 02 00 b2 86 01 00 a0 02 f5 a1 02 14 6b 1c 43 09 5c 9a c7 4b af 04 c8 00 ed 0d 47 8c 60 02 00",
        ))
        .unwrap();
        assert_eq!((c.caster, c.skill, c.cast_id, c.target), (34240, 100_018, 0xa0, 37109));
    }
}
