//! Opcode 0x3804: one hit (or effect tick) from an attacker to a target.
//!
//! Layout as far as our captures show (see `docs/protocol.md`). Fields named `unknown_*`
//! are consistently present but not understood yet.
//!
//! ```text
//! varint target, u16 flags, varint attacker, u32 skill, u8 cast_id, [flags & 0x01: u32],
//! u8 result, [flags & 0x02: u8 special, varint, u8 position], u32 unknown_a, u32 hit,
//! varint unknown_b, [flags & 0x04: varint damage], [flags & 0x08: u32 MP damage],
//! [flags & 0x10 without 0x02: 1 byte], [flags & 0x20: varint n, n × varint parts],
//! [flags & 0x40: varint n, n × u32 barriers hit], [flags & 0x80: u8],
//! [flags & 0x100: varint n, n × u32 effects], u16 hit_echo,
//! [flags & 0x200: varint HP drained], [flags & 0x400: varint HP recovered],
//! [flags & 0x800: u32], [flags & 0x1000: u8], [flags & 0x2000: u32]
//! ```
//!
//! The fields from 0x08 on follow Questlog's SkillEffect; with them, all but 308 of 197,103
//! hits in our 2026-10-23/24 captures read to the last byte. HP drained and recovered heal
//! the attacker (Questlog counts both as healing): 6,635 hits recovered 3.7M HP.
//!
//! The damage value's position depends only on bits 0x02 and 0x04, so damage is read
//! correctly even when later fields include bits we haven't decoded.
//!
//! The hit outcome fields were worked out on a training dummy and checked against another
//! meter's per-skill numbers for the same fight, which they match exactly (see
//! `docs/protocol.md`, "Hit outcomes").

use crate::protocol::read_varint;

/// Flag bits whose fields we know how to skip.
const KNOWN_FLAGS: u16 = 0x3fff;

/// `result` of a critical hit (a normal hit is 2).
const RESULT_CRIT: u8 = 3;
/// Bits of `special`.
const SPECIAL_PERFECT: u8 = 0x04;
const SPECIAL_DOUBLE: u8 = 0x08;

/// Where the attacker stood relative to the target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Position {
    /// Neither front nor back (also every hit without the outcome block).
    Side,
    Back,
    Front,
    /// A value we haven't seen in tests.
    Other(u8),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DamageEvent {
    pub target: u64,
    pub attacker: u64,
    pub flags: u16,
    pub skill: u32,
    /// The attacker's cast this hit belongs to: every hit of one cast shares it (and the
    /// cast's `0x3802` message carries it too). Counts up and wraps at 255.
    pub cast_id: u8,
    /// How the hit landed (Questlog's hit types): 2 normal, 3 critical; 1 evaded, 6 resisted,
    /// 4, 5, 7 and 8 immune, 9 reflected (see [`DamageEvent::is_avoided`]); 0 no effect.
    pub result: u8,
    /// Outcome bits from the optional 3-byte block: 0x04 perfect, 0x08 double strike.
    /// Other bits (0x80 is common on hits taken) aren't identified yet. 0 without the block.
    pub special: u8,
    /// Raw position byte from the same block: 0 side, 1 back, 2 front. 0 without the block.
    pub position_byte: u8,
    /// 1, 2, 3… within a multi-hit skill. Not every skill starts at 1, so count casts with
    /// `cast_id` instead.
    pub hit: u32,
    /// `None` for events without a damage value (buffs, ticks, effects).
    pub damage: Option<u64>,
    /// The pieces of a multi-hit, listed after the damage when bit 0x20 is set. Two or
    /// more pieces make this a multi-hit (see [`DamageEvent::hit_count`]).
    pub parts: Vec<u64>,
    pub unknown_a: u32,
    pub unknown_b: u64,
    /// Every byte was accounted for. False when unknown flag bits add fields we skip.
    pub fully_parsed: bool,
    /// A damage-over-time tick (opcode 0x3805) rather than a direct hit.
    pub is_dot: bool,
    /// For ticks: the instance ID of the effect producing them (as in the buff messages).
    pub instance: Option<u64>,
    /// HP the hit gave back to its attacker (drained and recovered), 0 if none.
    pub healed: u64,
}

impl DamageEvent {
    pub fn is_self(&self) -> bool {
        self.target == self.attacker
    }

    pub fn is_crit(&self) -> bool {
        self.result == RESULT_CRIT
    }

    /// The target evaded, resisted or was immune: a miss, for accuracy (as Questlog counts
    /// them). Reflected damage still lands on someone, so it isn't one.
    pub fn is_avoided(&self) -> bool {
        self.damage.is_none() && matches!(self.result, 1 | 4..=8)
    }

    pub fn is_perfect(&self) -> bool {
        self.special & SPECIAL_PERFECT != 0
    }

    pub fn is_double(&self) -> bool {
        self.special & SPECIAL_DOUBLE != 0
    }

    pub fn position(&self) -> Position {
        match self.position_byte {
            0 => Position::Side,
            1 => Position::Back,
            2 => Position::Front,
            other => Position::Other(other),
        }
    }

    /// A multi-hit: two or more parts.
    pub fn is_multi(&self) -> bool {
        self.parts.len() >= 2
    }

    /// Hits this event stands for: its parts when it's a multi-hit, otherwise 1.
    pub fn hit_count(&self) -> u32 {
        if self.is_multi() {
            self.parts.len() as u32
        } else {
            1
        }
    }

    /// A heal-over-time tick (0x3805 with flag 0x01): `damage` is then the amount healed.
    pub fn is_heal(&self) -> bool {
        self.is_dot && self.flags & 0x01 != 0
    }
}

/// Parses the payload of an opcode 0x3804 message (the bytes after the opcode).
pub fn parse(p: &[u8]) -> Option<DamageEvent> {
    let mut i = 0;
    let target = read_varint(p, &mut i)?;
    let flags = u16_at(p, &mut i)?;
    let attacker = read_varint(p, &mut i)?;
    let skill = u32_at(p, &mut i)?;
    let cast_id = *take(p, &mut i, 1)?.first()?;
    if flags & 0x01 != 0 {
        take(p, &mut i, 4)?;
    }
    let result = *take(p, &mut i, 1)?.first()?;
    let (special, position_byte) = if flags & 0x02 != 0 {
        let special = *take(p, &mut i, 1)?.first()?;
        read_varint(p, &mut i)?; // HP restored by blocking (0 but when it happens)
        (special, *take(p, &mut i, 1)?.first()?)
    } else {
        (0, 0)
    };
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
        cast_id,
        result,
        special,
        position_byte,
        hit,
        damage,
        parts: Vec::new(),
        unknown_a,
        unknown_b,
        fully_parsed: false,
        is_dot: false,
        instance: None,
        healed: 0,
    };
    // Everything up to the damage value is solid. The rest is best effort: if it
    // doesn't line up, keep the event and mark it as not fully parsed.
    ev.fully_parsed = parse_trailer(p, i, flags, &mut ev.parts, &mut ev.healed) && flags & !KNOWN_FLAGS == 0;
    Some(ev)
}

/// Parses the payload of an opcode 0x3805 message: one damage-over-time or heal-over-time tick.
///
/// ```text
/// varint target, u8 flags, varint attacker, varint instance, u32 unknown_a,
/// [flags & 0x02: varint amount], [flags & 0x01: varint unknown],
/// u32 skill (absent on some self-effects)
/// ```
///
/// Flag 0x02 means an amount is present; flag 0x01 marks a heal (Recuperation's ticks are
/// 0x09 then 0x0b; damage ticks are 0x0a/0x08). `instance` is the effect's instance ID from
/// the buff messages. Worked out from our own captures (997 of 998 parse exactly).
pub fn parse_dot(p: &[u8]) -> Option<DamageEvent> {
    let mut i = 0;
    let target = read_varint(p, &mut i)?;
    let flags = *p.get(i)? as u16;
    i += 1;
    let attacker = read_varint(p, &mut i)?;
    let instance = read_varint(p, &mut i)?;
    let unknown_a = u32_at(p, &mut i)?;
    let damage = if flags & 0x02 != 0 {
        Some(read_varint(p, &mut i)?)
    } else {
        None
    };
    if flags & 0x01 != 0 {
        read_varint(p, &mut i)?;
    }
    let skill = if p.len() - i >= 4 { u32_at(p, &mut i)? } else { 0 };
    Some(DamageEvent {
        target,
        attacker,
        flags,
        skill,
        cast_id: 0,
        result: 0,
        special: 0,
        position_byte: 0,
        hit: 0,
        damage,
        parts: Vec::new(),
        unknown_a,
        unknown_b: 0,
        fully_parsed: i == p.len(),
        is_dot: true,
        instance: Some(instance),
        healed: 0,
    })
}

fn parse_trailer(p: &[u8], mut i: usize, flags: u16, parts: &mut Vec<u64>, healed: &mut u64) -> bool {
    let mut inner = || -> Option<u64> {
        let u32_list = |i: &mut usize| -> Option<()> {
            let n = read_varint(p, i)?;
            if n > 64 {
                return None;
            }
            take(p, i, 4 * n as usize).map(|_| ())
        };
        if flags & 0x08 != 0 {
            take(p, &mut i, 4)?;
        }
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
        if flags & 0x40 != 0 {
            u32_list(&mut i)?;
        }
        if flags & 0x80 != 0 {
            take(p, &mut i, 1)?;
        }
        if flags & 0x100 != 0 {
            u32_list(&mut i)?;
        }
        u16_at(p, &mut i)?;
        let mut back = 0;
        for bit in [0x200, 0x400] {
            if flags & bit != 0 {
                back += read_varint(p, &mut i)?;
            }
        }
        for (bit, n) in [(0x800, 4), (0x1000, 1), (0x2000, 4)] {
            if flags & bit != 0 {
                take(p, &mut i, n)?;
            }
        }
        (i == p.len()).then_some(back)
    };
    match inner() {
        Some(back) => {
            *healed = back;
            true
        }
        None => {
            parts.clear();
            false
        }
    }
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

    #[test]
    fn a_hit_that_heals_its_attacker() {
        // A Gladiator's (8967) hit from behind: 1,352 damage, 497 HP back (2026-10-24).
        let p = hex_bytes("a7 ea 03 16 04 87 46 9c 9c a8 00 0e 02 80 00 01 fb 2c dd 41 01 00 00 00 d6 5d c8 0a 01 00 f1 03");
        let e = parse(&p).unwrap();
        assert_eq!((e.attacker, e.skill, e.damage, e.healed, e.position()), (8967, 11_050_140, Some(1352), 497, Position::Back));
        assert!(e.fully_parsed);
    }

    fn hex_bytes(s: &str) -> Vec<u8> {
        s.split_whitespace().map(|b| u8::from_str_radix(b, 16).unwrap()).collect()
    }

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
    fn dot_tick() {
        // Rafaela (15585), a Cleric, ticking 416 damage with skill 17400000.
        let ev = parse_dot(&hex("9c c0 02 0a e1 79 02 0b 4b b6 67 a0 03 c0 80 09 01")).unwrap();
        assert_eq!(ev.attacker, 15585);
        assert_eq!(ev.damage, Some(416));
        assert_eq!(ev.skill, 17_400_000);
        assert!(ev.is_dot && ev.fully_parsed);
    }

    #[test]
    fn dot_with_extra_field() {
        // Flag 0x01 adds a varint after the damage.
        let ev = parse_dot(&hex("ff 18 0b f5 a1 02 e3 0c 13 b7 a3 6f 8f 06 01 12 cc 1d 01")).unwrap();
        assert_eq!((ev.target, ev.attacker), (3199, 37109));
        assert_eq!(ev.damage, Some(783));
        assert_eq!(ev.skill, 18_730_002);
        assert!(ev.fully_parsed);
    }

    #[test]
    fn dot_without_damage_or_skill() {
        let ev = parse_dot(&hex("f5 a1 02 00 f5 a1 02 01 0a e1 f5 05")).unwrap();
        assert_eq!(ev.damage, None);
        assert_eq!(ev.skill, 0);
        assert!(ev.fully_parsed);
    }

    // Hit outcomes, from the live dummy tests on 2026-10-20 (you = entity 11792 = `90 5c`,
    // the dummy = 25002 = `aa c3 01`).

    #[test]
    fn crit_from_the_front() {
        // Resonance Crush, first hit of cast 103: result 3 (crit), position 2 (front).
        let ev = parse(&hex(
            "aa c3 01 06 00 90 5c f4 f7 12 01 67 03 00 00 02 5b db 68 6b 01 00 00 00 96 56 d2 0a 01 00",
        ))
        .unwrap();
        assert_eq!((ev.target, ev.attacker, ev.skill), (25002, 11792, 18_020_340));
        assert_eq!((ev.cast_id, ev.damage), (103, Some(1362)));
        assert!(ev.is_crit() && !ev.is_perfect() && !ev.is_double());
        assert_eq!(ev.position(), Position::Front);
        assert!(ev.fully_parsed);
    }

    #[test]
    fn normal_hit_from_the_front() {
        let ev = parse(&hex(
            "aa c3 01 06 00 90 5c 81 dd 1e 01 63 02 00 00 02 6f 86 0e 70 01 00 00 00 96 56 f0 1c 01 00",
        ))
        .unwrap();
        assert_eq!((ev.skill, ev.cast_id, ev.damage), (18_800_001, 99, Some(3696)));
        assert!(!ev.is_crit());
        assert_eq!(ev.position(), Position::Front);
    }

    #[test]
    fn back_attack() {
        let ev = parse(&hex(
            "aa c3 01 26 00 90 5c e4 d0 12 01 9a 03 00 00 01 1b 99 59 6b 01 00 00 00 96 56 e0 17 01 43 01 00",
        ))
        .unwrap();
        assert_eq!((ev.skill, ev.cast_id, ev.damage), (18_010_340, 154, Some(3040)));
        assert_eq!(ev.position(), Position::Back);
        assert!(ev.is_crit());
        // A single part isn't a multi-hit.
        assert_eq!((ev.parts.len(), ev.hit_count()), (1, 1));
    }

    #[test]
    fn side_hits_have_no_outcome_block() {
        // From the side, with nothing special, the 3-byte block is left out (no flag 0x02).
        let ev = parse(&hex(
            "aa c3 01 04 00 90 5c 81 dd 1e 01 fd 03 6f 86 0e 70 01 00 00 00 96 56 b2 35 01 00",
        ))
        .unwrap();
        assert_eq!((ev.cast_id, ev.damage), (253, Some(6834)));
        assert_eq!(ev.position(), Position::Side);
        assert!(ev.is_crit() && ev.fully_parsed);
    }

    #[test]
    fn perfect_and_double() {
        let perfect = parse(&hex(
            "aa c3 01 06 00 90 5c f4 f7 12 01 d4 03 04 00 01 61 62 68 6b 04 00 00 00 96 56 be 0b 04 00",
        ))
        .unwrap();
        assert!(perfect.is_perfect() && !perfect.is_double());
        assert_eq!((perfect.damage, perfect.position()), (Some(1470), Position::Back));

        let double = parse(&hex(
            "aa c3 01 06 00 90 5c 54 c4 18 01 a5 03 08 00 01 db b0 ac 6d 01 00 00 00 96 56 8c 96 03 01 00",
        ))
        .unwrap();
        assert!(double.is_double() && !double.is_perfect());
        assert_eq!((double.skill, double.damage), (18_400_340, Some(51980)));
    }

    #[test]
    fn multi_hit_counts_its_parts() {
        let ev = parse(&hex(
            "aa c3 01 26 00 90 5c 1e 46 13 01 ab 02 00 00 01 c6 63 87 6b 01 00 00 00 96 56 92 33 04 91 01 91 01 91 01 91 01 01 00",
        ))
        .unwrap();
        assert_eq!(ev.damage, Some(6546));
        assert_eq!(ev.parts, vec![145; 4]);
        assert!(ev.is_multi());
        assert_eq!(ev.hit_count(), 4);
    }

    #[test]
    fn heal_tick() {
        // Recuperation (18120120) healing you for 976; 2093 is the buff's instance ID.
        let ev = parse_dot(&hex("90 5c 0b 90 5c ad 10 f3 f0 00 6c d0 07 c5 02 b8 7d 14 01")).unwrap();
        assert!(ev.is_heal());
        assert_eq!((ev.target, ev.attacker, ev.skill), (11792, 11792, 18_120_120));
        assert_eq!((ev.damage, ev.instance), (Some(976), Some(2093)));
        assert!(ev.fully_parsed);
        // A damage tick isn't a heal.
        let dot = parse_dot(&hex("9c c0 02 0a e1 79 02 0b 4b b6 67 a0 03 c0 80 09 01")).unwrap();
        assert!(!dot.is_heal());
    }

    #[test]
    fn truncated_payload_is_rejected() {
        assert!(parse(&hex("f5 a1 02 04 00 ff 18")).is_none());
    }
}
