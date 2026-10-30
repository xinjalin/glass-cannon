//! Where the mobs and players in view are, and where you are.
//!
//! Mobs come into view with their position (`0x3641`), and the game sends where other
//! players and mobs move: keyframes with a position (mobs' `0x371c` once a second; see
//! protocol.md, "Other players and mobs moving") and small steps between them, about ten a
//! second (signed bytes in cm), which this adds up. It never sends your own movement, so
//! your position comes from:
//!
//! - your `0x3633` (login, teleport, zone change): exact;
//! - a hit notice (`0x3803`: attacker, 00, target, skill u32, 5 bytes, the target's
//!   position) on you: exact, about every half second while mobs hit you (on our captures
//!   the position is the target's: 12,585 of 18,241 within 3 m of its last keyframe);
//! - one of yours on a mob: the mob's position, which you're next to (within a ranged
//!   skill's reach).
//!
//! Each counts as exact for [`crate::cubes::POSITION_FRESH_US`]; after that the last one
//! stays, as rough (you've likely moved, but it's still the best there is).

use std::collections::HashMap;

use crate::cubes::{parse_self_position, POSITION_FRESH_US};
use crate::dungeon::OPCODE_MAP_ENTERED;
use crate::entities::OPCODE_SELF_SPAWN;
use crate::protocol::read_varint;

const OPCODE_MOB_SPAWN: u16 = 0x3641;
const OPCODE_MOB_GONE: u16 = 0x3642;
const OPCODE_PLAYER_GONE: u16 = 0x3647;
const OPCODE_HIT_NOTICE: u16 = 0x3803;
/// A hit notice's position: after the target, its skill (u32) and 5 bytes.
const HIT_POS_AFTER_TARGET: usize = 4 + 5;

/// An exact position isn't replaced by an estimate for this long.
const EXACT_KEPT_US: u64 = 3_000_000;

/// `0x3641`: varint entity, 3 bytes, u32 type, u8 flags, u8, then f32 × 3.
const SPAWN_POS_AFTER_ENTITY: usize = 3 + 4 + 2;

/// A step's (dx, dy, dz) in cm, after the entity and the flag byte `f` at `i`: each axis
/// only when its flag bit is set (per opcode, protocol.md).
fn step(opcode: u16, f: u8, p: &[u8], i: usize) -> Option<[f32; 3]> {
    let (mut q, bits, z_always) = match opcode {
        0x371d | 0x372a => (i + 1 + usize::from(f & 1), [2, 4, 8], false),
        0x3720 => (i + 1, [1, 2, 0], true),
        0x3725 | 0x3732 | 0x372d => (i + 1, [1, 2, 4], false),
        _ => return None,
    };
    let mut d = [0.0f32; 3];
    for (k, bit) in bits.iter().enumerate() {
        if (k == 2 && z_always) || (*bit != 0 && f & bit != 0) {
            d[k] = f32::from(*p.get(q)? as i8);
            q += 1;
        }
    }
    Some(d)
}

/// Where a keyframe's position starts, after the entity and the flag byte `f`.
fn keyframe_offset(opcode: u16, f: u8) -> Option<usize> {
    Some(match opcode {
        0x3729 | 0x3728 | 0x371e | 0x371b | 0x371c => 1 + usize::from(f & 1),
        0x371f | 0x3724 | 0x372f => 1,
        0x371a => 2 + usize::from(f & 1),
        _ => return None,
    })
}

/// Where you are, as far as the meter knows.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Me {
    pub pos: [f32; 3],
    /// From where you appeared or were hit; else where a mob you hit was.
    pub exact: bool,
}

#[derive(Debug, Default)]
pub struct Positions {
    at: HashMap<u64, [f32; 3]>,
    me: Option<(Me, u64)>,
    map: Option<u32>,
}

impl Positions {
    /// `you`: your entity, when known (for the hit notices).
    pub fn feed(&mut self, ts_us: u64, opcode: u16, p: &[u8], you: Option<u64>) {
        let mut i = 0;
        match opcode {
            OPCODE_SELF_SPAWN => {
                if let Some(pos) = parse_self_position(p) {
                    self.me = Some((Me { pos, exact: true }, ts_us));
                }
            }
            OPCODE_HIT_NOTICE => {
                let Some(you) = you else { return };
                let Some(attacker) = read_varint(p, &mut i) else { return };
                i += 1;
                let Some(target) = read_varint(p, &mut i) else { return };
                let Some(pos) = f32s(p, i + HIT_POS_AFTER_TARGET) else { return };
                if target != you {
                    self.at.insert(target, pos);
                }
                // Hit: you're there. Hitting: you're near it (an exact place stays first).
                let exact_now = self.me.is_some_and(|(m, at)| m.exact && ts_us.saturating_sub(at) < EXACT_KEPT_US);
                if target == you {
                    self.me = Some((Me { pos, exact: true }, ts_us));
                } else if attacker == you && !exact_now {
                    self.me = Some((Me { pos, exact: false }, ts_us));
                }
            }
            OPCODE_MAP_ENTERED => {
                let map = p.get(..4).and_then(|b| b.try_into().ok()).map(u32::from_le_bytes);
                if map.is_some() && map != self.map {
                    self.at.clear();
                    self.map = map;
                }
            }
            OPCODE_MOB_GONE | OPCODE_PLAYER_GONE => {
                if let Some(id) = read_varint(p, &mut i) {
                    self.at.remove(&id);
                }
            }
            OPCODE_MOB_SPAWN => {
                if let Some(id) = read_varint(p, &mut i) {
                    if let Some(pos) = f32s(p, i + SPAWN_POS_AFTER_ENTITY) {
                        self.at.insert(id, pos);
                    }
                }
            }
            _ => {
                let Some(id) = read_varint(p, &mut i) else { return };
                let Some(&f) = p.get(i) else { return };
                if let Some(pos) = keyframe_offset(opcode, f).and_then(|off| f32s(p, i + off)) {
                    self.at.insert(id, pos);
                } else if let (Some(d), Some(at)) = (step(opcode, f, p, i), self.at.get_mut(&id)) {
                    for k in 0..3 {
                        at[k] += d[k];
                    }
                }
            }
        }
    }

    /// The map you're on (from the last zone change), when known.
    pub fn map(&self) -> Option<u32> {
        self.map
    }

    pub fn of(&self, id: u64) -> Option<[f32; 3]> {
        self.at.get(&id).copied()
    }

    /// Your position: exact while fresh, else the last one known, as rough.
    pub fn me(&self, now_us: u64) -> Option<Me> {
        self.me.map(|(m, at)| Me { exact: m.exact && now_us.saturating_sub(at) < POSITION_FRESH_US, ..m })
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }
}

/// Metres on the ground and the compass bearing (degrees clockwise from north, which is
/// -y) from `from` to `to`.
pub fn distance_and_bearing(from: [f32; 3], to: [f32; 3]) -> (u32, u16) {
    let (dx, dy) = (f64::from(to[0] - from[0]), f64::from(to[1] - from[1]));
    let bearing = dx.atan2(-dy).to_degrees().rem_euclid(360.0);
    ((dx.hypot(dy) / 100.0).round() as u32, bearing.round() as u16 % 360)
}

/// Three finite, world-sized floats.
fn f32s(p: &[u8], i: usize) -> Option<[f32; 3]> {
    let f = |k: usize| p.get(k..k + 4).and_then(|b| b.try_into().ok()).map(f32::from_le_bytes);
    let v = [f(i)?, f(i + 4)?, f(i + 8)?];
    v.iter().all(|x| x.is_finite() && x.abs() < 1e7).then_some(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spawn(id: u8, x: f32, y: f32) -> Vec<u8> {
        let mut p = vec![id, 0, 0, 0];
        p.extend(2_400_009u32.to_le_bytes());
        p.extend([0x40, 0x02]);
        for v in [x, y, 100.0f32, 0.0] {
            p.extend(v.to_le_bytes());
        }
        p
    }

    #[test]
    fn spawns_and_keyframes_move_a_mob() {
        let mut ps = Positions::default();
        ps.feed(0, OPCODE_MOB_SPAWN, &spawn(7, 1000.0, 2000.0), None);
        assert_eq!(ps.of(7), Some([1000.0, 2000.0, 100.0]));
        // A mob's keyframe (0x371c, flags 0x04: no byte before the position).
        let mut k = vec![7, 0x04];
        for v in [1500.0f32, 2000.0, 100.0] {
            k.extend(v.to_le_bytes());
        }
        k.extend([0, 0, 0, 0, 1]);
        ps.feed(1, 0x371c, &k, None);
        assert_eq!(ps.of(7), Some([1500.0, 2000.0, 100.0]));
        // A step (0x371d, flags 0x07: speed, dx and dy): 15 cm east, 11 cm south.
        ps.feed(2, 0x371d, &[7, 0x07, 0x01, 0x0f, 0x0b, 0x5a, 0x28, 0x5a, 0x28], None);
        assert_eq!(ps.of(7), Some([1515.0, 2011.0, 100.0]));
        ps.feed(3, OPCODE_MOB_GONE, &[7, 3], None);
        assert_eq!(ps.of(7), None);
    }

    fn hit(attacker: u8, target: u8, x: f32) -> Vec<u8> {
        let mut p = vec![attacker, 0, target];
        p.extend(14_040_240u32.to_le_bytes());
        p.extend([0; 5]);
        for v in [x, 500.0f32, 100.0] {
            p.extend(v.to_le_bytes());
        }
        p.extend([1, 0, 0, 0, 0, 1, 0]);
        p
    }

    #[test]
    fn hits_say_where_you_are() {
        const S: u64 = 1_000_000;
        let mut ps = Positions::default();
        // You (9) hit a mob (7): you're about where it is.
        ps.feed(S, OPCODE_HIT_NOTICE, &hit(9, 7, 1000.0), Some(9));
        assert_eq!(ps.me(S), Some(Me { pos: [1000.0, 500.0, 100.0], exact: false }));
        assert_eq!(ps.of(7), Some([1000.0, 500.0, 100.0]));
        // It hits you: exactly there, and your next hit doesn't replace that at once.
        ps.feed(2 * S, OPCODE_HIT_NOTICE, &hit(7, 9, 1200.0), Some(9));
        ps.feed(3 * S, OPCODE_HIT_NOTICE, &hit(9, 7, 1000.0), Some(9));
        assert_eq!(ps.me(3 * S), Some(Me { pos: [1200.0, 500.0, 100.0], exact: true }));
        // A minute on: still the best there is, but rough.
        assert_eq!(ps.me(70 * S), Some(Me { pos: [1200.0, 500.0, 100.0], exact: false }));
    }

    #[test]
    fn distance_and_compass_bearing() {
        // 30 m east and 40 m north (north is -y): 50 m, bearing 37°.
        assert_eq!(distance_and_bearing([0.0, 0.0, 0.0], [3000.0, -4000.0, 0.0]), (50, 37));
    }
}
