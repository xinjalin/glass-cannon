//! Field bosses, from the world map (`0x9101`, sent when you open it).
//!
//! ```text
//! u16 result, u32 map, varint count, then per boss:
//!   u8 flags, varint boss ID, [flags 1: 3 × f32 where it spawns], u64 time (ms since 1970)
//! ```
//!
//! The time is when it spawned while it's up, else when it next spawns. A few entries
//! have one more byte before the time (Questlog's list calls one an "is spawned" flag,
//! but most entries don't have it); the time is found either way. Checked on the map of
//! Altgard (1110): 24 bosses, up since hours ago or back within two hours.

use serde::{Deserialize, Serialize};

use crate::protocol::read_varint;

pub const OPCODE_WORLD_MAP: u16 = 0x9101;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FieldBoss {
    pub id: u32,
    pub map: u32,
    pub pos: Option<[f32; 3]>,
    /// When it spawned (up) or will (down), ms since 1970.
    pub time_ms: u64,
}

const TIMES: std::ops::Range<u64> = 1_700_000_000_000..2_200_000_000_000;

fn time_at(p: &[u8], i: usize) -> Option<u64> {
    let v = u64::from_le_bytes(p.get(i..i + 8)?.try_into().ok()?);
    TIMES.contains(&v).then_some(v)
}

pub fn parse_world_map(p: &[u8]) -> Option<Vec<FieldBoss>> {
    if p.get(..2)? != [0, 0] {
        return None;
    }
    let map = u32::from_le_bytes(p.get(2..6)?.try_into().ok()?);
    let mut i = 6;
    let n = read_varint(p, &mut i)?;
    let mut out = Vec::new();
    for _ in 0..n.min(256) {
        let flags = *p.get(i)?;
        i += 1;
        let id = u32::try_from(read_varint(p, &mut i)?).ok()?;
        let pos = if flags & 1 != 0 {
            let f = |k: usize| p.get(i + k..i + k + 4).and_then(|b| b.try_into().ok()).map(f32::from_le_bytes);
            let pos = [f(0)?, f(4)?, f(8)?];
            i += 12;
            Some(pos)
        } else {
            None
        };
        let time_ms = match time_at(p, i) {
            Some(t) => t,
            None => {
                i += 1;
                time_at(p, i)?
            }
        };
        i += 8;
        out.push(FieldBoss { id, map, pos, time_ms });
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn altgards_field_bosses() {
        let hex = include_str!("../testdata/world-map-1110.hex");
        let p: Vec<u8> = hex.split_whitespace().map(|b| u8::from_str_radix(b, 16).unwrap()).collect();
        let bosses = parse_world_map(&p).unwrap();
        assert_eq!(bosses.len(), 24);
        assert!(bosses.iter().all(|b| b.map == 1110));
        assert_eq!((bosses[0].id, bosses[0].time_ms), (111_003, 1_792_631_524_233));
        assert!(bosses[0].pos.is_some());
        // Opened at 20:52 (1792664511 s): 13 up (one for 9 h), 11 back within 2 h 5 min.
        let opened = 1_792_664_511_000;
        assert!(bosses.iter().all(|b| b.time_ms < opened + 7_500_000));
        assert_eq!(bosses.iter().filter(|b| b.time_ms > opened).count(), 11);
    }
}
