//! Shields (the game's barriers): how much damage each one on a player can still take.
//!
//! - `0x3646`: varint entity, varint count, then per shield: varint instance (the buff's),
//!   varint amount left. The shields an entity has, as it comes into view.
//! - `0x3805` (an effect's tick, see [`crate::damage`]): varint target, u8 flags, varint
//!   caster, varint instance, u32 effect, flag 1: varint amount left, flag 2: varint
//!   damage, flag 4: u8 count + count × u32, flag 8: u32 skill. A shield's tick has an
//!   amount left and no damage; a damage-over-time's has both (its amount left is the
//!   damage still to come), and its first tick only the amount, so an instance once seen
//!   with damage is never a shield.
//! - `0x382c` (a buff removed, see [`crate::buffs`]): its shield goes.
//!
//! Which skills give shields is the game data's to say (a "Protective Shield" in their
//! description): a tick's shield counts only when [`Shields::on`] is told its skill is one.
//! Checked on our captures (2026-11-13, the Ludra raid): Protection Circle's ticks carry the
//! party members' shields, 1,215 to 1,540 (about 5 % of their HP), every 6.5 s; Steel
//! Barrier's carry a damage-over-time on a mob, with damage. Message names and layouts came
//! from Questlog (CharBarrierInfo_NT, SkillEffectByAbnormal_NT); all 20 `0x3646` in the raid
//! parse to their last byte.

use std::collections::{HashMap, HashSet};

use crate::protocol::read_varint;

pub const OPCODE_SHIELDS: u16 = 0x3646;
pub const OPCODE_EFFECT_TICK: u16 = 0x3805;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Shield {
    /// The skill that gave it (None from `0x3646`, which only lists shields).
    skill: Option<u32>,
    left: u64,
}

/// The shields on everyone, by entity and buff instance.
#[derive(Debug, Default)]
pub struct Shields {
    by: HashMap<u64, HashMap<u64, Shield>>,
    /// Instances seen with damage: damage-over-time, never shields.
    dots: HashSet<(u64, u64)>,
}

/// An effect's tick (`0x3805`), as far as shields go.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tick {
    pub target: u64,
    pub instance: u64,
    pub left: Option<u64>,
    pub damage: Option<u64>,
    pub skill: Option<u32>,
}

pub fn parse_tick(p: &[u8]) -> Option<Tick> {
    let mut i = 0;
    let target = read_varint(p, &mut i)?;
    let flags = *p.get(i)?;
    i += 1;
    read_varint(p, &mut i)?; // caster
    let instance = read_varint(p, &mut i)?;
    i += 4; // effect
    let left = if flags & 1 != 0 { Some(read_varint(p, &mut i)?) } else { None };
    let damage = if flags & 2 != 0 { Some(read_varint(p, &mut i)?) } else { None };
    if flags & 4 != 0 {
        i += 1 + 4 * usize::from(*p.get(i)?);
    }
    let skill = if flags & 8 != 0 { Some(u32::from_le_bytes(p.get(i..i + 4)?.try_into().ok()?)) } else { None };
    Some(Tick { target, instance, left, damage, skill })
}

/// `0x3646`: the entity and its shields (instance, amount left), the whole message read.
pub fn parse_shields(p: &[u8]) -> Option<(u64, Vec<(u64, u64)>)> {
    let mut i = 0;
    let entity = read_varint(p, &mut i)?;
    let count = read_varint(p, &mut i)?;
    let mut out = Vec::new();
    for _ in 0..count {
        let instance = read_varint(p, &mut i)?;
        out.push((instance, read_varint(p, &mut i)?));
    }
    (i == p.len()).then_some((entity, out))
}

impl Shields {
    pub fn feed(&mut self, opcode: u16, payload: &[u8]) {
        match opcode {
            OPCODE_SHIELDS => {
                let Some((entity, list)) = parse_shields(payload) else { return };
                let old = self.by.remove(&entity).unwrap_or_default();
                let now: HashMap<u64, Shield> = list
                    .into_iter()
                    .filter(|(_, left)| *left > 0)
                    .map(|(inst, left)| (inst, Shield { skill: old.get(&inst).and_then(|s| s.skill), left }))
                    .collect();
                if !now.is_empty() {
                    self.by.insert(entity, now);
                }
            }
            OPCODE_EFFECT_TICK => {
                let Some(t) = parse_tick(payload) else { return };
                let key = (t.target, t.instance);
                if t.damage.is_some() {
                    self.dots.insert(key);
                    self.remove(t.target, t.instance);
                    return;
                }
                let Some(left) = t.left else { return };
                if self.dots.contains(&key) || left == 0 {
                    self.remove(t.target, t.instance);
                    return;
                }
                let on = self.by.entry(t.target).or_default();
                let skill = t.skill.or_else(|| on.get(&t.instance).and_then(|s| s.skill));
                on.insert(t.instance, Shield { skill, left });
            }
            crate::buffs::OPCODE_BUFF_REMOVE => {
                if let Some((target, instances)) = crate::buffs::parse_remove(payload) {
                    for inst in instances {
                        self.remove(target, inst);
                        self.dots.remove(&(target, inst));
                    }
                }
            }
            _ => {}
        }
    }

    fn remove(&mut self, entity: u64, instance: u64) {
        if let Some(on) = self.by.get_mut(&entity) {
            on.remove(&instance);
            if on.is_empty() {
                self.by.remove(&entity);
            }
        }
    }

    /// The damage an entity's shields can still take: those `0x3646` listed, and those
    /// whose skill `is_shield` says gives one (by its skill ID).
    pub fn on(&self, entity: u64, is_shield: &dyn Fn(u32) -> bool) -> u64 {
        self.by
            .get(&entity)
            .map_or(0, |on| on.values().filter(|s| s.skill.is_none_or(is_shield)).map(|s| s.left).sum())
    }

    /// Entity IDs from another login mean nothing.
    pub fn clear(&mut self) {
        self.by.clear();
        self.dots.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::tests::encode_varint;

    fn tick(target: u64, inst: u64, left: Option<u64>, damage: Option<u64>, skill: u32) -> Vec<u8> {
        let mut p = encode_varint(target);
        p.push(u8::from(left.is_some()) | (u8::from(damage.is_some()) << 1) | 8);
        p.extend(encode_varint(77)); // caster
        p.extend(encode_varint(inst));
        p.extend(1_873_000_211u32.to_le_bytes());
        if let Some(l) = left {
            p.extend(encode_varint(l));
        }
        if let Some(d) = damage {
            p.extend(encode_varint(d));
        }
        p.extend(skill.to_le_bytes());
        p
    }

    const CIRCLE: u32 = 18_730_002;
    const BARRIER: u32 = 15_160_030;
    fn shield_skill(s: u32) -> bool {
        [1873, 1516].contains(&(s / 10_000))
    }

    #[test]
    fn a_shields_ticks_count_and_go_with_its_buff() {
        let mut s = Shields::default();
        // Protection Circle on a party member: 1,215 left (the raid).
        s.feed(OPCODE_EFFECT_TICK, &tick(9463, 14879, Some(1215), None, CIRCLE));
        assert_eq!(s.on(9463, &shield_skill), 1215);
        // Not a shield skill: not counted.
        assert_eq!(s.on(9463, &|_| false), 0);
        // Hit: less left.
        s.feed(OPCODE_EFFECT_TICK, &tick(9463, 14879, Some(400), None, CIRCLE));
        assert_eq!(s.on(9463, &shield_skill), 400);
        // Its buff removed: gone.
        let mut rm = encode_varint(9463);
        rm.extend([1, 0]);
        rm.extend(encode_varint(14879));
        rm.push(0);
        s.feed(crate::buffs::OPCODE_BUFF_REMOVE, &rm);
        assert_eq!(s.on(9463, &shield_skill), 0);
    }

    #[test]
    fn a_damage_over_time_is_no_shield() {
        let mut s = Shields::default();
        // Steel Barrier on a mob: its first tick has only what's to come, then damage.
        s.feed(OPCODE_EFFECT_TICK, &tick(10658, 13900, Some(4611), None, BARRIER));
        s.feed(OPCODE_EFFECT_TICK, &tick(10658, 13900, Some(4458), Some(153), BARRIER));
        assert_eq!(s.on(10658, &shield_skill), 0);
        // And it stays not one.
        s.feed(OPCODE_EFFECT_TICK, &tick(10658, 13900, Some(4305), None, BARRIER));
        assert_eq!(s.on(10658, &shield_skill), 0);
    }

    #[test]
    fn the_shields_an_entity_comes_with() {
        // 0x3646 from the raid: entity 13700 with one shield of 1,275.
        let p = [0x84, 0x6b, 0x01, 0xc8, 0x44, 0xfb, 0x09];
        assert_eq!(parse_shields(&p), Some((13700, vec![(8776, 1275)])));
        let mut s = Shields::default();
        s.feed(OPCODE_SHIELDS, &p);
        assert_eq!(s.on(13700, &|_| false), 1275, "listed as a shield: counted whatever its skill");
        // None any more.
        s.feed(OPCODE_SHIELDS, &[0x84, 0x6b, 0x00]);
        assert_eq!(s.on(13700, &|_| true), 0);
    }
}
