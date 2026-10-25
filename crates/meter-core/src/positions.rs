//! Where the mobs and players in view are, and where you are.
//!
//! Mobs come into view with their position (`0x3641`), and the game sends where other
//! players and mobs move: keyframes with a position (mobs' `0x371c` once a second; see
//! protocol.md, "Other players and mobs moving") and small deltas between them, which this
//! leaves out (a keyframe is never more than a second or so behind). It never sends your
//! own movement: your position is the one in your last `0x3633` (login, teleport, zone
//! change), trusted for [`crate::cubes::POSITION_FRESH_US`].

use std::collections::HashMap;

use crate::cubes::{parse_self_position, POSITION_FRESH_US};
use crate::dungeon::OPCODE_MAP_ENTERED;
use crate::entities::OPCODE_SELF_SPAWN;
use crate::protocol::read_varint;

const OPCODE_MOB_SPAWN: u16 = 0x3641;
const OPCODE_MOB_GONE: u16 = 0x3642;
const OPCODE_PLAYER_GONE: u16 = 0x3647;

/// `0x3641`: varint entity, 3 bytes, u32 type, u8 flags, u8, then f32 × 3.
const SPAWN_POS_AFTER_ENTITY: usize = 3 + 4 + 2;

/// Where a keyframe's position starts, after the entity and the flag byte `f`.
fn keyframe_offset(opcode: u16, f: u8) -> Option<usize> {
    Some(match opcode {
        0x3729 | 0x3728 | 0x371e | 0x371b | 0x371c => 1 + usize::from(f & 1),
        0x371f | 0x3724 | 0x372f => 1,
        0x371a => 2 + usize::from(f & 1),
        _ => return None,
    })
}

#[derive(Debug, Default)]
pub struct Positions {
    at: HashMap<u64, [f32; 3]>,
    me: Option<([f32; 3], u64)>,
    map: Option<u32>,
}

impl Positions {
    pub fn feed(&mut self, ts_us: u64, opcode: u16, p: &[u8]) {
        let mut i = 0;
        match opcode {
            OPCODE_SELF_SPAWN => {
                if let Some(pos) = parse_self_position(p) {
                    self.me = Some((pos, ts_us));
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
                let Some(off) = p.get(i).and_then(|&f| keyframe_offset(opcode, f)) else { return };
                if let Some(pos) = f32s(p, i + off) {
                    self.at.insert(id, pos);
                }
            }
        }
    }

    pub fn of(&self, id: u64) -> Option<[f32; 3]> {
        self.at.get(&id).copied()
    }

    /// Your position, while it's fresh.
    pub fn me(&self, now_us: u64) -> Option<[f32; 3]> {
        self.me.filter(|(_, at)| now_us.saturating_sub(*at) < POSITION_FRESH_US).map(|(p, _)| p)
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
        ps.feed(0, OPCODE_MOB_SPAWN, &spawn(7, 1000.0, 2000.0));
        assert_eq!(ps.of(7), Some([1000.0, 2000.0, 100.0]));
        // A mob's keyframe (0x371c, flags 0x04: no byte before the position).
        let mut k = vec![7, 0x04];
        for v in [1500.0f32, 2000.0, 100.0] {
            k.extend(v.to_le_bytes());
        }
        k.extend([0, 0, 0, 0, 1]);
        ps.feed(1, 0x371c, &k);
        assert_eq!(ps.of(7), Some([1500.0, 2000.0, 100.0]));
        ps.feed(2, OPCODE_MOB_GONE, &[7, 3]);
        assert_eq!(ps.of(7), None);
    }

    #[test]
    fn distance_and_compass_bearing() {
        // 30 m east and 40 m north (north is -y): 50 m, bearing 37°.
        assert_eq!(distance_and_bearing([0.0, 0.0, 0.0], [3000.0, -4000.0, 0.0]), (50, 37));
    }
}
