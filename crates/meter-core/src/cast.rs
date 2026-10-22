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

#[cfg(test)]
mod tests {
    use super::*;

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
