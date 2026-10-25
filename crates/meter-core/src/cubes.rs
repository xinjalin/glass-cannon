//! Hidden cubes in view: where each is, and how far and which way from you.
//!
//! A cube is a world object (`0x3634`, Questlog's EnvObjSpawn) of one of [`CUBES`]' types;
//! it leaves view or is opened with `0x3635` (reason 3, Questlog's "Dead", when opened).
//! The game never sends your own movement, so your position is the one in your last
//! `0x3633` (login, teleport, zone change), trusted for [`POSITION_FRESH_US`] as Questlog
//! does: distance and direction are only known for a while after you arrive somewhere.
//!
//! Coordinates are centimetres. Questlog takes y as growing southwards, so north is -y.

use std::collections::HashMap;

use serde::Serialize;

use crate::dungeon::OPCODE_MAP_ENTERED;
use crate::entities::OPCODE_SELF_SPAWN;
use crate::protocol::read_varint;

/// A world object came into view: varint entity, u8 flags, then its description (u8 flags,
/// [1: string], u32 object type, [2: u8], [4: u8], [8: u8, u32], [16: u8]) and f32 × 3
/// position, then more we don't read.
pub const OPCODE_OBJECT_SPAWN: u16 = 0x3634;
/// A world object left view: varint entity, u8 state, u8 reason ([`REASON_OPENED`] when it
/// was used up), varint.
pub const OPCODE_OBJECT_DESPAWN: u16 = 0x3635;
/// The object was used (the game's "Dead" appear reason).
pub const REASON_OPENED: u8 = 3;

/// Hidden cubes' object types and their look (from Questlog's list; type 1301199, a red
/// one, is in our captures: seen twice, opened once).
pub const CUBES: [(u32, Look); 19] = [
    (1_200_036, Look::Gold),
    (1_200_037, Look::Gold),
    (1_200_039, Look::Red),
    (1_200_040, Look::Red),
    (1_200_041, Look::Red),
    (1_200_667, Look::Gold),
    (1_201_502, Look::Gold),
    (1_201_503, Look::Red),
    (1_201_504, Look::Red),
    (1_201_505, Look::Gold),
    (1_300_081, Look::Gold),
    (1_300_082, Look::Gold),
    (1_300_754, Look::Gold),
    (1_301_199, Look::Red),
    (1_301_200, Look::Red),
    (1_301_242, Look::Gold),
    (1_301_243, Look::Gold),
    (1_301_244, Look::Red),
    (1_301_245, Look::Red),
];

/// Your position from `0x3633` counts this long (as in Questlog).
pub const POSITION_FRESH_US: u64 = 60_000_000;
/// An opened cube stays in the list this long, marked opened.
const OPENED_KEPT_US: u64 = 8_000_000;
/// Your position in `0x3633`: this far after your name, after a u16 of flags whose high
/// byte is this (in 37 of the 40 in our captures; the other three have no position there).
const SELF_POS_AFTER_NAME: usize = 164;
const SELF_POS_FLAGS_HIGH: u8 = 0x92;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Look {
    Gold,
    Red,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Cube {
    look: Look,
    pos: [f32; 3],
    seen_us: u64,
    opened_us: Option<u64>,
}

/// A cube for the overlay.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CubeView {
    pub entity: u64,
    pub look: Look,
    /// When it came into view (µs since 1970).
    pub seen_us: u64,
    pub opened: bool,
    /// From your last known position, while it's fresh: metres, the compass bearing
    /// (degrees clockwise from north) and how far above you it is (metres).
    pub distance_m: Option<u32>,
    pub bearing: Option<u16>,
    pub rise_m: Option<i32>,
}

#[derive(Debug, Default)]
pub struct Cubes {
    cubes: HashMap<u64, Cube>,
    /// Where you last appeared, and when.
    me: Option<([f32; 3], u64)>,
    map: Option<u32>,
}

impl Cubes {
    pub fn feed(&mut self, ts_us: u64, opcode: u16, p: &[u8]) {
        match opcode {
            OPCODE_OBJECT_SPAWN => {
                self.cubes.retain(|_, c| c.opened_us.is_none_or(|t| ts_us.saturating_sub(t) <= OPENED_KEPT_US));
                if let Some((entity, kind, pos)) = parse_object_spawn(p) {
                    if let Some(look) = CUBES.iter().find(|(t, _)| *t == kind).map(|(_, l)| *l) {
                        self.cubes.insert(entity, Cube { look, pos, seen_us: ts_us, opened_us: None });
                    }
                }
            }
            OPCODE_OBJECT_DESPAWN => {
                let mut i = 0;
                let Some(entity) = read_varint(p, &mut i) else { return };
                let Some(&reason) = p.get(i + 1) else { return };
                if reason == REASON_OPENED {
                    if let Some(c) = self.cubes.get_mut(&entity) {
                        c.opened_us = Some(ts_us);
                    }
                } else {
                    self.cubes.remove(&entity);
                }
            }
            OPCODE_SELF_SPAWN => {
                if let Some(pos) = parse_self_position(p) {
                    self.me = Some((pos, ts_us));
                }
            }
            OPCODE_MAP_ENTERED => {
                let map = p.get(..4).and_then(|b| b.try_into().ok()).map(u32::from_le_bytes);
                if map.is_some() && map != self.map {
                    self.cubes.clear();
                    self.map = map;
                }
            }
            _ => {}
        }
    }

    /// The cubes in view (opened ones for a few seconds), the first seen first.
    pub fn view(&self, now_us: u64) -> Vec<CubeView> {
        let me = self.me.filter(|(_, at)| now_us.saturating_sub(*at) < POSITION_FRESH_US).map(|(p, _)| p);
        let mut out: Vec<CubeView> = self
            .cubes
            .iter()
            .filter(|(_, c)| c.opened_us.is_none_or(|t| now_us.saturating_sub(t) <= OPENED_KEPT_US))
            .map(|(entity, c)| {
                let (distance_m, bearing, rise_m) = match me {
                    Some(m) => {
                        let (dx, dy, dz) = (f64::from(c.pos[0] - m[0]), f64::from(c.pos[1] - m[1]), f64::from(c.pos[2] - m[2]));
                        // North is -y: the bearing turns clockwise from it towards +x (east).
                        let bearing = dx.atan2(-dy).to_degrees().rem_euclid(360.0);
                        (Some((dx.hypot(dy) / 100.0).round() as u32), Some(bearing.round() as u16 % 360), Some((dz / 100.0).round() as i32))
                    }
                    None => (None, None, None),
                };
                CubeView {
                    entity: *entity,
                    look: c.look,
                    seen_us: c.seen_us,
                    opened: c.opened_us.is_some(),
                    distance_m,
                    bearing,
                    rise_m,
                }
            })
            .collect();
        out.sort_by_key(|c| (c.seen_us, c.entity));
        out
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }
}

/// (entity, object type, position) of a `0x3634`.
fn parse_object_spawn(p: &[u8]) -> Option<(u64, u32, [f32; 3])> {
    let mut i = 0;
    let entity = read_varint(p, &mut i)?;
    i += 1; // flags (for fields after the position)
    let desc = *p.get(i)?;
    i += 1;
    if desc & 1 != 0 {
        let n = usize::try_from(read_varint(p, &mut i)?).ok()?;
        i += n;
    }
    let kind = u32::from_le_bytes(p.get(i..i + 4)?.try_into().ok()?);
    i += 4;
    for (bit, n) in [(2, 1), (4, 1), (8, 5), (16, 1)] {
        if desc & bit != 0 {
            i += n;
        }
    }
    Some((entity, kind, f32s(p, i)?))
}

/// Your position in your `0x3633`: varint entity, 5 bytes, u8 length, your name, then
/// [`SELF_POS_AFTER_NAME`] bytes on, after its flags.
fn parse_self_position(p: &[u8]) -> Option<[f32; 3]> {
    let mut i = 0;
    read_varint(p, &mut i)?;
    let name_end = i + 6 + usize::from(*p.get(i + 5)?);
    let at = name_end + SELF_POS_AFTER_NAME;
    if *p.get(at - 1)? != SELF_POS_FLAGS_HIGH {
        return None;
    }
    f32s(p, at).filter(|v| v.iter().all(|x| x.is_finite()))
}

fn f32s(p: &[u8], i: usize) -> Option<[f32; 3]> {
    let f = |k: usize| p.get(k..k + 4).and_then(|b| b.try_into().ok()).map(f32::from_le_bytes);
    Some([f(i)?, f(i + 4)?, f(i + 8)?])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::tests::hex;

    const S: u64 = 1_000_000;

    fn spawn(entity: u8, kind: u32, x: f32, y: f32, z: f32) -> Vec<u8> {
        let mut p = vec![entity, 0, 0];
        p.extend(kind.to_le_bytes());
        for v in [x, y, z] {
            p.extend(v.to_le_bytes());
        }
        p.extend([0; 20]);
        p
    }

    fn me_at(x: f32, y: f32, z: f32) -> Vec<u8> {
        // Entity, 5 bytes, a 4-letter name, then the state's flags and position.
        let mut p = vec![0x10, 0, 0, 0, 0, 0, 4];
        p.extend(b"Nene");
        p.extend(vec![0; SELF_POS_AFTER_NAME - 2]);
        p.extend([0x80, SELF_POS_FLAGS_HIGH]);
        for v in [x, y, z] {
            p.extend(v.to_le_bytes());
        }
        p.extend([0; 40]);
        p
    }

    #[test]
    fn a_cube_in_view_and_which_way() {
        let mut c = Cubes::default();
        c.feed(0, OPCODE_MAP_ENTERED, &hex("56 04 00 00 00 01"));
        c.feed(S, OPCODE_SELF_SPAWN, &me_at(10_000.0, 10_000.0, 500.0));
        // A red cube 30 m north-east-ish and 2 m up; a barrel that isn't a cube.
        c.feed(2 * S, OPCODE_OBJECT_SPAWN, &spawn(7, 1_301_199, 13_000.0, 7_000.0, 700.0));
        c.feed(2 * S, OPCODE_OBJECT_SPAWN, &spawn(8, 1_550_000, 0.0, 0.0, 0.0));
        let v = c.view(3 * S);
        assert_eq!(v.len(), 1);
        assert_eq!((v[0].look, v[0].distance_m, v[0].bearing, v[0].rise_m), (Look::Red, Some(42), Some(45), Some(2)));
        // A minute after you appeared: no longer where you are.
        assert_eq!(c.view(70 * S)[0].distance_m, None);
        // Opened: shown a few seconds, then gone. Leaving view: gone at once.
        c.feed(71 * S, OPCODE_OBJECT_DESPAWN, &[7, 0, REASON_OPENED, 0]);
        assert!(c.view(72 * S)[0].opened);
        assert!(c.view(80 * S).is_empty());
        c.feed(81 * S, OPCODE_OBJECT_SPAWN, &spawn(9, 1_200_036, 1.0, 1.0, 1.0));
        c.feed(82 * S, OPCODE_OBJECT_DESPAWN, &[9, 0, 1, 0]);
        assert!(c.view(82 * S).is_empty());
        // Another map: none.
        c.feed(83 * S, OPCODE_OBJECT_SPAWN, &spawn(9, 1_200_036, 1.0, 1.0, 1.0));
        c.feed(84 * S, OPCODE_MAP_ENTERED, &hex("08 28 09 00 00 01"));
        assert!(c.view(84 * S).is_empty());
    }

    #[test]
    fn a_real_cube_spawn() {
        // A red cube (type 1301199) coming into view in the Force session (2026-10-23).
        let p = hex("be 8f 01 04 00 cf da 13 00 13 cd 9e 47 f4 02 13 48 00 64 60 46 00 00 00 00 00 b0 1a 43 01 01 01 a6 22 a6 68 a1 01 00 00 00 00 00 00 00 00 00 00 2e 9f 28 3c 07 00 00");
        let (entity, kind, pos) = parse_object_spawn(&p).unwrap();
        assert_eq!((entity, kind), (18_366, 1_301_199));
        assert_eq!(pos.map(|v| v.round() as i32), [81_306, 150_540, 14_361]);
    }
}
