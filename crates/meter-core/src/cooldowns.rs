//! Your skills' cooldowns.
//!
//! Casting a skill (`0x3802`) says how long until it can be cast again, and for skills
//! with charges how many are left and when the next one comes back. Later `0x3847`
//! messages change a skill's remaining time (a reset, or a reduction); 0 means ready.
//! Only your own casts count: the game sends `0x3847` for you alone.

use std::collections::HashMap;

use serde::Serialize;

use crate::cast::{parse_cast, parse_stop_cooldown, OPCODE_CAST, OPCODE_CAST_STOP, OPCODE_CHARGE_STOP};
use crate::protocol::read_varint;

/// Cooldowns changing: a varint count, then for each a u32 skill and a varint remaining
/// time in ms (0: ready).
pub const OPCODE_COOLDOWN: u16 = 0x3847;

/// Your skills, sent at login and on entering a dungeon: a u8 count, then entries of a
/// tag, a u32 id and more by kind (not all of them skills). The skills are the ids of a
/// player skill's base form (`xxyy0000`), all of one class.
pub const OPCODE_SKILL_LIST: u16 = 0x5100;

/// The base skill ids in a skill list, found by their shape: of the 8-digit ids with no
/// variant, the ones of the class most of them are.
pub fn parse_skill_list(p: &[u8]) -> Vec<u32> {
    let mut ids: Vec<u32> = p
        .windows(4)
        .map(|w| u32::from_le_bytes([w[0], w[1], w[2], w[3]]))
        .filter(|v| (11_000_000..20_000_000).contains(v) && v % 10_000 == 0)
        .collect();
    let mut by_class: HashMap<u32, usize> = HashMap::new();
    for v in &ids {
        *by_class.entry(v / 1_000_000).or_default() += 1;
    }
    let Some((&class, _)) = by_class.iter().max_by_key(|(c, n)| (**n, std::cmp::Reverse(**c))) else {
        return Vec::new();
    };
    ids.retain(|v| v / 1_000_000 == class);
    ids.sort_unstable();
    ids.dedup();
    ids
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Cooldown {
    /// The skill as cast (variants of a skill share its cooldown).
    skill: u32,
    ends_us: u64,
    duration_ms: u32,
    charges: Option<u8>,
}

/// A skill that isn't ready, as the cooldown window shows it.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CooldownView {
    pub skill: u32,
    pub remaining_ms: u32,
    pub duration_ms: u32,
    /// Charges left, for skills that have them (it can still be cast while above 0).
    pub charges: Option<u8>,
}

#[derive(Debug, Default)]
pub struct Cooldowns {
    by_base: HashMap<u32, Cooldown>,
    /// Your skills, from the last skill list.
    known: Vec<u32>,
}

/// Skill variants (a chosen specialty changes the last digits) share one cooldown.
fn base(skill: u32) -> u32 {
    skill - skill % 10_000
}

impl Cooldowns {
    /// Feeds one game message; `me` is your entity, when known.
    pub fn feed(&mut self, ts_us: u64, opcode: u16, payload: &[u8], me: Option<u64>) {
        match opcode {
            OPCODE_CAST => {
                let Some(c) = parse_cast(payload) else { return };
                if me != Some(c.caster) {
                    return;
                }
                let (ms, charges) = match c.charges {
                    // Charges left: castable now, the next one comes back in `recharge`.
                    Some((left, recharge)) if left > 0 => (recharge, Some(left)),
                    Some((_, recharge)) => (c.cooldown_ms.max(recharge), Some(0)),
                    None => (c.cooldown_ms, None),
                };
                if ms == 0 {
                    self.by_base.remove(&base(c.skill));
                    return;
                }
                self.by_base.insert(
                    base(c.skill),
                    Cooldown { skill: c.skill, ends_us: ts_us + u64::from(ms) * 1000, duration_ms: ms, charges },
                );
            }
            // A cast you stopped, or a charge you let go, can start the skill's cooldown.
            OPCODE_CAST_STOP | OPCODE_CHARGE_STOP => {
                let Some((caster, skill, ms)) = parse_stop_cooldown(payload) else { return };
                if me != Some(caster) || ms == 0 {
                    return;
                }
                self.by_base.insert(base(skill), Cooldown { skill, ends_us: ts_us + u64::from(ms) * 1000, duration_ms: ms, charges: None });
            }
            OPCODE_SKILL_LIST => {
                let skills = parse_skill_list(payload);
                if !skills.is_empty() {
                    self.known = skills;
                }
            }
            OPCODE_COOLDOWN => {
                let mut i = 0;
                let Some(n) = read_varint(payload, &mut i) else { return };
                for _ in 0..n.min(64) {
                    let Some(skill) = payload.get(i..i + 4).and_then(|b| b.try_into().ok()).map(u32::from_le_bytes) else {
                        return;
                    };
                    i += 4;
                    let Some(ms) = read_varint(payload, &mut i).and_then(|v| u32::try_from(v).ok()) else { return };
                    if ms == 0 {
                        self.by_base.remove(&base(skill));
                        continue;
                    }
                    let e = self.by_base.entry(base(skill)).or_insert(Cooldown {
                        skill,
                        ends_us: 0,
                        duration_ms: ms,
                        charges: None,
                    });
                    e.ends_us = ts_us + u64::from(ms) * 1000;
                    e.duration_ms = e.duration_ms.max(ms);
                }
            }
            _ => {}
        }
    }

    /// Skills not ready at `now_us`, soonest first.
    pub fn active(&self, now_us: u64) -> Vec<CooldownView> {
        let mut out: Vec<CooldownView> = self
            .by_base
            .values()
            .filter(|c| c.ends_us > now_us)
            .map(|c| CooldownView {
                skill: c.skill,
                remaining_ms: ((c.ends_us - now_us) / 1000) as u32,
                duration_ms: c.duration_ms,
                charges: c.charges,
            })
            .collect();
        out.sort_by_key(|c| (c.remaining_ms, c.skill));
        out
    }

    /// Your skills (base ids), if the game has sent their list since the meter started.
    pub fn known(&self) -> &[u32] {
        &self.known
    }

    /// Forgets the cooldowns, not the skill list: a login sends the list just before your
    /// character, which is what clears this.
    pub fn clear(&mut self) {
        self.by_base.clear();
    }
}

/// Long cooldowns of everyone's casts (the game sends each cast's cooldown for every
/// caster in view), for the party cooldowns window. Skills under [`PARTY_SHORTEST_MS`]
/// aren't kept.
pub const PARTY_SHORTEST_MS: u32 = 20_000;

#[derive(Debug, Default)]
pub struct CastCooldowns {
    /// (caster, base skill) -> its cooldown.
    by_cast: HashMap<(u64, u32), Cooldown>,
}

/// One of a player's skills: ready (`remaining_ms` 0) or how long left.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CasterCooldown {
    pub entity: u64,
    pub skill: u32,
    pub remaining_ms: u32,
    /// The longest cooldown it has been cast with.
    pub duration_ms: u32,
}

impl CastCooldowns {
    pub fn cast(&mut self, ts_us: u64, caster: u64, skill: u32, cooldown_ms: u32) {
        if cooldown_ms < PARTY_SHORTEST_MS {
            return;
        }
        let e = self.by_cast.entry((caster, base(skill))).or_insert(Cooldown {
            skill,
            ends_us: 0,
            duration_ms: 0,
            charges: None,
        });
        e.skill = skill;
        e.ends_us = ts_us + u64::from(cooldown_ms) * 1000;
        e.duration_ms = e.duration_ms.max(cooldown_ms);
    }

    /// The skills of those `keep` lets through, by caster then longest cooldown first.
    pub fn of(&self, now_us: u64, keep: impl Fn(u64) -> bool) -> Vec<CasterCooldown> {
        let mut out: Vec<CasterCooldown> = self
            .by_cast
            .iter()
            .filter(|((caster, _), _)| keep(*caster))
            .map(|((caster, _), c)| CasterCooldown {
                entity: *caster,
                skill: c.skill,
                remaining_ms: (c.ends_us.saturating_sub(now_us) / 1000) as u32,
                duration_ms: c.duration_ms,
            })
            .collect();
        out.sort_by_key(|c| (c.entity, std::cmp::Reverse(c.duration_ms), c.skill));
        out
    }

    pub fn clear(&mut self) {
        self.by_cast.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(s: &str) -> Vec<u8> {
        s.split_whitespace().map(|b| u8::from_str_radix(b, 16).unwrap()).collect()
    }

    const S: u64 = 1_000_000;

    #[test]
    fn casts_start_cooldowns_and_updates_change_them() {
        let mut c = Cooldowns::default();
        // Your Recuperation (11792): one charge left, the next back in 14.4 s.
        c.feed(S, OPCODE_CAST, &hex("90 5c 0c b8 7d 14 01 92 00 90 5c 8b 70 e9 c2 96 56 01 00 01 c0 70"), Some(11792));
        let r = &c.active(S)[0];
        assert_eq!((r.skill, r.remaining_ms, r.charges), (18_120_120, 14_400, Some(1)));
        // Someone else's cast doesn't count.
        c.feed(S, OPCODE_CAST, &hex("90 5c 0c b8 7d 14 01 92 00 90 5c 8b 70 e9 c2 96 56 01 00 01 c0 70"), Some(3199));
        assert_eq!(c.active(S).len(), 1);
        // From the Vakron capture: skill 18290000 has 18.05 s left.
        c.feed(2 * S, OPCODE_COOLDOWN, &hex("01 50 15 17 01 82 8d 01"), Some(11792));
        let v = c.active(2 * S);
        assert_eq!(v.iter().map(|c| (c.skill, c.remaining_ms)).collect::<Vec<_>>(), [(18_120_120, 13_400), (18_290_000, 18_050)]);
        // Ready again.
        c.feed(3 * S, OPCODE_COOLDOWN, &hex("01 50 15 17 01 00"), Some(11792));
        assert_eq!(c.active(3 * S).len(), 1);
        assert!(c.active(20 * S).is_empty());
    }

    #[test]
    fn a_charge_let_go_starts_its_cooldown() {
        let mut c = Cooldowns::default();
        // You (11792) let go of a charged skill: 12 s.
        let mut p = vec![0x90, 0x5c, 0, 0];
        p.extend(14_300_000u32.to_le_bytes());
        p.extend([0xe0, 0x5d]);
        c.feed(S, OPCODE_CHARGE_STOP, &p, Some(11792));
        assert_eq!(c.active(S)[0].remaining_ms, 12_000);
        // Someone else's doesn't count.
        let mut c = Cooldowns::default();
        c.feed(S, OPCODE_CHARGE_STOP, &p, Some(3199));
        assert!(c.active(S).is_empty());
    }

    #[test]
    fn your_skill_list() {
        // Entering Vakron Sky Island: a Chanter's 30 skills.
        let p = hex(include_str!("../testdata/skill-list.hex"));
        let skills = parse_skill_list(&p);
        assert_eq!(skills.len(), 30);
        assert!(skills.iter().all(|s| s / 1_000_000 == 18 && s % 10_000 == 0));
        assert!(skills.contains(&18_120_000) && skills.contains(&18_290_000));
        let mut c = Cooldowns::default();
        c.feed(S, OPCODE_SKILL_LIST, &p, None);
        assert_eq!(c.known(), &skills[..]);
        assert!(parse_skill_list(&[0, 1, 2]).is_empty());
    }

    #[test]
    fn party_members_long_cooldowns() {
        let mut c = CastCooldowns::default();
        // From Vakron: a Cleric's Summon Resurrection (112 s), a short skill (not kept).
        c.cast(S, 9268, 17_390_000, 111_950);
        c.cast(S, 9268, 17_060_000, 4_000);
        c.cast(2 * S, 4875, 12_450_000, 83_950);
        let v = c.of(60 * S, |e| e != 4875);
        assert_eq!(v, [CasterCooldown { entity: 9268, skill: 17_390_000, remaining_ms: 52_950, duration_ms: 111_950 }]);
        // Ready again: still listed, with nothing left.
        assert_eq!(c.of(200 * S, |_| true)[1].remaining_ms, 0);
    }
}
