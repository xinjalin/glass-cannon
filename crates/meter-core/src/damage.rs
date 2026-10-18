//! Opcode 0x3804: one hit (or effect tick) from an attacker to a target.
//!
//! Layout as far as our captures show (see `docs/protocol.md`). Fields named `unknown_*`
//! are consistently present but not understood yet.
//!
//! ```text
//! varint target, u16 flags, varint attacker, u32 skill, u16 seq,
//! [flags & 0x02: 3 bytes], u32 unknown_a, u32 hit, varint unknown_b,
//! [flags & 0x04: varint damage],
//! [flags & 0x10 without 0x02: 1 byte],
//! [flags & 0x20: varint n, n × varint parts],
//! [flags & 0x100: u8, varint],
//! u16 hit_echo,
//! [flags & 0x400: u16]
//! ```
//!
//! The damage value's position depends only on bits 0x02 and 0x04, so damage is read
//! correctly even when later fields include bits we haven't decoded.

use crate::protocol::read_varint;

/// Flag bits whose fields we know how to skip.
const KNOWN_FLAGS: u16 = 0x0002 | 0x0004 | 0x0010 | 0x0020 | 0x0100 | 0x0400;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DamageEvent {
    pub target: u64,
    pub attacker: u64,
    pub flags: u16,
    pub skill: u32,
    /// Increments per event from the same source.
    pub seq: u16,
    /// 1, 2, 3… within a multi-hit skill.
    pub hit: u32,
    /// `None` for events without a damage value (buffs, ticks, effects).
    pub damage: Option<u64>,
    /// Repeated values listed after the damage when bit 0x20 is set. Meaning unknown.
    pub parts: Vec<u64>,
    pub unknown_a: u32,
    pub unknown_b: u64,
    /// Every byte was accounted for. False when unknown flag bits add fields we skip.
    pub fully_parsed: bool,
}

impl DamageEvent {
    pub fn is_self(&self) -> bool {
        self.target == self.attacker
    }
}

/// Parses the payload of an opcode 0x3804 message (the bytes after the opcode).
pub fn parse(p: &[u8]) -> Option<DamageEvent> {
    let mut i = 0;
    let target = read_varint(p, &mut i)?;
    let flags = u16_at(p, &mut i)?;
    let attacker = read_varint(p, &mut i)?;
    let skill = u32_at(p, &mut i)?;
    let seq = u16_at(p, &mut i)?;
    if flags & 0x02 != 0 {
        take(p, &mut i, 3)?;
    }
    let unknown_a = u32_at(p, &mut i)?;
    let hit = u32_at(p, &mut i)?;
    let unknown_b = read_varint(p, &mut i)?;
    let damage = if flags & 0x04 != 0 {
        Some(read_varint(p, &mut i)?)
    } else {
        None
    };

    let mut ev = DamageEvent {
        target,
        attacker,
        flags,
        skill,
        seq,
        hit,
        damage,
        parts: Vec::new(),
        unknown_a,
        unknown_b,
        fully_parsed: false,
    };
    // Everything up to the damage value is solid. The rest is best effort: if it
    // doesn't line up, keep the event and mark it as not fully parsed.
    ev.fully_parsed = parse_trailer(p, i, flags, &mut ev.parts) && flags & !KNOWN_FLAGS == 0;
    Some(ev)
}

fn parse_trailer(p: &[u8], mut i: usize, flags: u16, parts: &mut Vec<u64>) -> bool {
    let mut inner = || -> Option<()> {
        if flags & 0x10 != 0 && flags & 0x02 == 0 {
            take(p, &mut i, 1)?;
        }
        if flags & 0x20 != 0 {
            let n = read_varint(p, &mut i)?;
            if n > 64 {
                return None;
            }
            for _ in 0..n {
                parts.push(read_varint(p, &mut i)?);
            }
        }
        if flags & 0x100 != 0 {
            take(p, &mut i, 1)?;
            read_varint(p, &mut i)?;
        }
        u16_at(p, &mut i)?;
        if flags & 0x400 != 0 {
            u16_at(p, &mut i)?;
        }
        (i == p.len()).then_some(())
    };
    let ok = inner().is_some();
    if !ok {
        parts.clear();
    }
    ok
}

fn take<'a>(p: &'a [u8], i: &mut usize, n: usize) -> Option<&'a [u8]> {
    let s = p.get(*i..*i + n)?;
    *i += n;
    Some(s)
}

fn u16_at(p: &[u8], i: &mut usize) -> Option<u16> {
    Some(u16::from_le_bytes(take(p, i, 2)?.try_into().ok()?))
}

fn u32_at(p: &[u8], i: &mut usize) -> Option<u32> {
    Some(u32::from_le_bytes(take(p, i, 4)?.try_into().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(s: &str) -> Vec<u8> {
        s.split_whitespace()
            .map(|b| u8::from_str_radix(b, 16).unwrap())
            .collect()
    }

    // Payloads below are real 0x3804 messages from our test captures (opcode removed).

    #[test]
    fn plain_hit() {
        // Test B: noted 2674 damage on the training dummy.
        let ev = parse(&hex(
            "f5 a1 02 04 00 ff 18 e4 d0 12 01 62 03 0d 20 59 6b 02 00 00 00 96 56 f2 14 02 00",
        ))
        .unwrap();
        assert_eq!(ev.target, 37109);
        assert_eq!(ev.attacker, 3199);
        assert_eq!(ev.skill, 18_010_340);
        assert_eq!(ev.hit, 2);
        assert_eq!(ev.damage, Some(2674));
        assert!(ev.fully_parsed);
    }

    #[test]
    fn hit_with_parts() {
        // Test C: noted 11096 damage, a four-part skill.
        let ev = parse(&hex(
            "f5 a1 02 24 00 ff 18 1e 46 13 01 30 03 d0 63 87 6b 02 00 00 00 96 56 d8 56 04 f6 01 f6 01 f6 01 f6 01 02 00",
        ))
        .unwrap();
        assert_eq!(ev.skill, 18_040_350);
        assert_eq!(ev.damage, Some(11096));
        assert_eq!(ev.parts, vec![246, 246, 246, 246]);
        assert!(ev.fully_parsed);
    }

    #[test]
    fn hit_with_extra_block() {
        // Test D: noted 435 damage taken. Bit 0x02 adds 3 bytes before the damage.
        let ev = parse(&hex(
            "ff 18 06 00 ad ae 02 f8 90 12 00 0e 02 00 00 02 eb a0 40 07 01 00 00 00 90 4e b3 03 01 00",
        ))
        .unwrap();
        assert_eq!(ev.target, 3199);
        assert_eq!(ev.damage, Some(435));
        assert!(ev.fully_parsed);
    }

    #[test]
    fn self_effect_has_no_damage() {
        let ev = parse(&hex(
            "ff 18 00 00 ff 18 95 19 15 01 14 02 3f fe 3d 6c 01 00 00 00 96 56 01 00",
        ))
        .unwrap();
        assert!(ev.is_self());
        assert_eq!(ev.damage, None);
        assert!(ev.fully_parsed);
    }

    #[test]
    fn trailing_u16_flag() {
        let ev = parse(&hex(
            "f5 a1 02 06 04 cf 34 18 48 c7 00 89 03 00 00 01 6c 29 d8 4d 01 00 00 00 86 81 01 fa ac 01 01 00 d2 04",
        ))
        .unwrap();
        assert_eq!(ev.damage, Some(22138));
        assert!(ev.fully_parsed);
    }

    #[test]
    fn unknown_flag_keeps_damage() {
        // Bit 0x1000/0x40 adds fields we can't skip yet; damage is still readable.
        let ev = parse(&hex(
            "ae 50 46 10 f5 a1 02 b2 08 13 00 9e 02 82 00 02 f5 56 be 00 01 00 00 00 90 4e 01 01 13 b7 a3 6f 01 00",
        ))
        .unwrap();
        assert_eq!(ev.damage, Some(1));
        assert!(!ev.fully_parsed);
    }

    #[test]
    fn truncated_payload_is_rejected() {
        assert!(parse(&hex("f5 a1 02 04 00 ff 18")).is_none());
    }
}
