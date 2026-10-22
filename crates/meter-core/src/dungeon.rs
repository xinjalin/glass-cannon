//! Which map you're on, and a dungeon's progress: entering it, its start (the countdown
//! ending) and its completion. Layouts are in `docs/protocol.md`.
//!
//! Worked out from a Vakron Sky Island run (2026-10-22) with screenshots: the game opens a
//! new connection to the dungeon's server and sends `0x4002` with the map, `0x6100` step 2
//! as the countdown ends and step 3 with `0x6101` at "Expedition Complete".

use serde::{Deserialize, Serialize};

/// You're on a map: u32 map ID, u8. First thing on every game server connection.
pub const OPCODE_MAP_ENTERED: u16 = 0x4002;
/// A dungeon's progress: u32 map ID, u8 step (1 entered, 2 started, 3 complete), two u64
/// times in milliseconds.
pub const OPCODE_DUNGEON_STEP: u16 = 0x6100;
/// A dungeon was completed: u8, u32 map ID, u8 1, u8.
pub const OPCODE_DUNGEON_COMPLETE: u16 = 0x6101;

const STEP_STARTED: u8 = 2;
const STEP_COMPLETE: u8 = 3;

/// One stay on a map (a dungeon run, when the map is a dungeon's).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Instance {
    pub map: u32,
    /// When you entered (capture time, µs since the Unix epoch): what tells one stay
    /// from the next.
    pub entered_us: u64,
    /// When the dungeon started (its countdown ended), if it's a dungeon.
    pub started_us: Option<u64>,
    /// When it was completed, if it was.
    pub completed_us: Option<u64>,
}

/// Follows the map you're on. Feed it every message; it says when what it knows changed.
#[derive(Debug, Default, Clone)]
pub struct Maps {
    current: Option<Instance>,
}

impl Maps {
    pub fn current(&self) -> Option<&Instance> {
        self.current.as_ref()
    }

    /// Returns the instance when this message changed it.
    pub fn feed(&mut self, ts_us: u64, opcode: u16, p: &[u8]) -> Option<Instance> {
        match opcode {
            OPCODE_MAP_ENTERED => {
                let map = u32_at(p, 0)?;
                self.current = Some(Instance { map, entered_us: ts_us, started_us: None, completed_us: None });
            }
            OPCODE_DUNGEON_STEP => {
                let (map, step) = (u32_at(p, 0)?, *p.get(4)?);
                let i = self.current.as_mut().filter(|i| i.map == map)?;
                match step {
                    STEP_STARTED if i.started_us.is_none() => i.started_us = Some(ts_us),
                    STEP_COMPLETE if i.completed_us.is_none() => i.completed_us = Some(ts_us),
                    _ => return None,
                }
            }
            OPCODE_DUNGEON_COMPLETE => {
                let map = u32_at(p, 1)?;
                let i = self.current.as_mut().filter(|i| i.map == map && i.completed_us.is_none())?;
                i.completed_us = Some(ts_us);
            }
            _ => return None,
        }
        self.current
    }
}

fn u32_at(p: &[u8], i: usize) -> Option<u32> {
    Some(u32::from_le_bytes(p.get(i..i + 4)?.try_into().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::tests::hex;

    #[test]
    fn follows_a_dungeon_run() {
        // The Vakron Sky Island run: entered, the countdown ends, "Expedition Complete",
        // then back in the world (map 1110).
        let mut m = Maps::default();
        let entered = m.feed(1_000, OPCODE_MAP_ENTERED, &hex("08 28 09 00 00 01")).unwrap();
        assert_eq!((entered.map, entered.entered_us, entered.started_us), (600_072, 1_000, None));
        m.feed(1_100, OPCODE_DUNGEON_STEP, &hex("08 28 09 00 01 8c 08 c4 62 a1 01 00 00 bc 7d c4 62 a1 01 00 00"));
        let started = m.feed(23_000, OPCODE_DUNGEON_STEP, &hex("08 28 09 00 02 ef 7d c4 62 a1 01 00 00 6f 6c fb 62 a1 01 00 00"));
        assert_eq!(started.unwrap().started_us, Some(23_000));
        let done = m.feed(716_000, OPCODE_DUNGEON_STEP, &hex("08 28 09 00 03 b9 12 cf 62 a1 01 00 00 79 3a d8 62 a1 01 00 00"));
        assert_eq!(done.unwrap().completed_us, Some(716_000));
        // Its completion message too, which changes nothing more.
        assert!(m.feed(716_000, OPCODE_DUNGEON_COMPLETE, &hex("00 08 28 09 00 01 00")).is_none());
        let world = m.feed(780_000, OPCODE_MAP_ENTERED, &hex("56 04 00 00 00 01")).unwrap();
        assert_eq!((world.map, world.started_us), (1110, None));
        // A step for another map isn't this one's.
        assert!(m.feed(781_000, OPCODE_DUNGEON_STEP, &hex("08 28 09 00 02")).is_none());
    }
}
