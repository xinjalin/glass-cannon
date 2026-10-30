//! Which map you're on, and a dungeon's progress: entering it, its start (the countdown
//! ending) and its completion. Layouts are in `docs/protocol.md`.
//!
//! Worked out from a Vakron Sky Island run (2026-10-22) with screenshots: the game opens a
//! new connection to the dungeon's server and sends `0x4002` with the map, `0x6100` step 2
//! as the countdown ends and step 3 with `0x6101` at "Expedition Complete".
//!
//! In Fire Temple (six runs, 2026-10-23) the game sends `0x4002` and step 2 again twice
//! mid-run, as you're moved on to the next part: the same stay. A new run of the same
//! dungeon starts with step 1, or after it was completed.

use serde::{Deserialize, Serialize};

/// You're on a map: u32 map ID, u8. First thing on every game server connection.
pub const OPCODE_MAP_ENTERED: u16 = 0x4002;
/// A dungeon's state (Questlog's DungeonState): u32 map ID, u8 state (1 entered, 2 started,
/// 3 complete, 4 over), u64 start and u64 end of its time limit (ms since 1970).
pub const OPCODE_DUNGEON_STEP: u16 = 0x6100;
/// A dungeon's result (Questlog's DungeonCompleteResult): u8 flags, u32 map ID, u8 1 when
/// cleared, then its rewards. Only a clear counts, as in Questlog: it came with every
/// state 3 in our captures.
pub const OPCODE_DUNGEON_COMPLETE: u16 = 0x6101;

/// A dungeon's objective (map event) progress: u32 event ID, u8 state (2 running, 3 done),
/// varint stage, varint step, then the step's objectives. In both Vakron Sky Island runs
/// the stage goes 1 -> 2 just after the first boss, 2 -> 3 after the second; Fire Temple
/// stays at stage 1 and its step goes up instead. So the stages are split at the middle
/// bosses' deaths ([`crate::state::OPCODE_MIDBOSS_KILLED`]), which both send.
pub const OPCODE_MAP_EVENT_PROGRESS: u16 = 0x922d;
/// Joining a map event: u32 event ID, u8 length, its name, then as in a progress message.
pub const OPCODE_MAP_EVENT_JOIN: u16 = 0x922e;
/// Stages kept per stay.
pub const MAX_STAGES: usize = 6;

const STEP_ENTERED: u8 = 1;
const STEP_STARTED: u8 = 2;

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
    /// When each stage of the dungeon began (0: not reached): its start, then each middle
    /// boss's death.
    #[serde(default)]
    pub stages: [u64; MAX_STAGES],
}

/// A dungeon's clock, as its last `0x6100` set it. On our captures (2026-10-25): phase 1
/// the 30 s before it starts, 2 its time limit (60 min, 30 min in some, 14 min in an
/// event), 3 the 10 min after its last boss, 4 the 10 s before you're sent out. Open maps
/// send an end of 0: no limit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct DungeonClock {
    pub map: u32,
    pub phase: u8,
    /// Server time, ms since 1970.
    pub start_ms: u64,
    pub end_ms: u64,
}

/// Follows the map you're on. Feed it every message; it says when what it knows changed.
#[derive(Debug, Default, Clone)]
pub struct Maps {
    current: Option<Instance>,
    clock: Option<DungeonClock>,
}

impl Maps {
    pub fn current(&self) -> Option<&Instance> {
        self.current.as_ref()
    }

    /// The dungeon's clock, while you're in a dungeon with a time limit.
    pub fn clock(&self) -> Option<DungeonClock> {
        self.clock
    }

    /// This `0x4002` is you moving on inside the dungeon you're in, not a new stay (the
    /// mobs keep their entity IDs: Kromede's Desire left and came back as the same one).
    pub fn moving_within(&self, p: &[u8]) -> bool {
        u32_at(p, 0).is_some_and(|map| self.current.is_some_and(|i| i.map == map && i.completed_us.is_none()))
    }

    /// Returns the instance when this message changed it.
    pub fn feed(&mut self, ts_us: u64, opcode: u16, p: &[u8]) -> Option<Instance> {
        if opcode == OPCODE_DUNGEON_STEP && p.len() >= 21 {
            let (start_ms, end_ms) = (u64_at(p, 5)?, u64_at(p, 13)?);
            let clock = DungeonClock { map: u32_at(p, 0)?, phase: p[4], start_ms, end_ms };
            self.clock = (end_ms > start_ms).then_some(clock);
        }
        match opcode {
            OPCODE_MAP_ENTERED => {
                if self.moving_within(p) {
                    return None;
                }
                self.current = Some(Instance::new(u32_at(p, 0)?, ts_us));
                self.clock = None;
            }
            OPCODE_DUNGEON_STEP => {
                let (map, step) = (u32_at(p, 0)?, *p.get(4)?);
                let i = self.current.as_mut().filter(|i| i.map == map)?;
                match step {
                    STEP_ENTERED if i.started_us.is_some() => *i = Instance::new(map, ts_us),
                    STEP_STARTED if i.started_us.is_none() => {
                        i.started_us = Some(ts_us);
                        i.stages[0] = ts_us;
                    }
                    _ => return None,
                }
            }
            OPCODE_DUNGEON_COMPLETE => {
                let map = u32_at(p, 1)?;
                if *p.get(5)? != 1 {
                    return None;
                }
                let i = self.current.as_mut().filter(|i| i.map == map && i.completed_us.is_none())?;
                i.completed_us = Some(ts_us);
            }
            crate::state::OPCODE_MIDBOSS_KILLED => {
                let i = self.current.as_mut().filter(|i| i.started_us.is_some() && i.completed_us.is_none())?;
                let next = i.stages.iter().position(|t| *t == 0)?;
                i.stages[next] = ts_us;
            }
            _ => return None,
        }
        self.current
    }
}

impl Instance {
    fn new(map: u32, entered_us: u64) -> Self {
        Instance { map, entered_us, started_us: None, completed_us: None, stages: [0; MAX_STAGES] }
    }
}

fn u32_at(p: &[u8], i: usize) -> Option<u32> {
    Some(u32::from_le_bytes(p.get(i..i + 4)?.try_into().ok()?))
}

fn u64_at(p: &[u8], i: usize) -> Option<u64> {
    Some(u64::from_le_bytes(p.get(i..i + 8)?.try_into().ok()?))
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
        // Its clock: the hour from when the countdown ended.
        let clock = m.clock().unwrap();
        assert_eq!((clock.phase, (clock.end_ms - clock.start_ms) / 1000), (2, 3600));
        // "Complete" as its state changes nothing: its result says it was cleared.
        assert!(m.feed(716_000, OPCODE_DUNGEON_STEP, &hex("08 28 09 00 03 b9 12 cf 62 a1 01 00 00 79 3a d8 62 a1 01 00 00")).is_none());
        assert!(m.feed(716_000, OPCODE_DUNGEON_COMPLETE, &hex("00 08 28 09 00 00 00")).is_none());
        let done = m.feed(716_000, OPCODE_DUNGEON_COMPLETE, &hex("00 08 28 09 00 01 00"));
        assert_eq!(done.unwrap().completed_us, Some(716_000));
        let world = m.feed(780_000, OPCODE_MAP_ENTERED, &hex("56 04 00 00 00 01")).unwrap();
        assert_eq!((world.map, world.started_us), (1110, None));
        assert_eq!(world.stages, [0; MAX_STAGES]);
        // A step for another map isn't this one's.
        assert!(m.feed(781_000, OPCODE_DUNGEON_STEP, &hex("08 28 09 00 02")).is_none());
    }

    #[test]
    fn a_dungeons_stages() {
        // Vakron Sky Island (2026-10-22): started 30 s in, Tiere died at 150 s, Thamon at 490 s.
        let mut m = Maps::default();
        let killed = |kind: u32| kind.to_le_bytes();
        m.feed(7, OPCODE_MAP_ENTERED, &hex("08 28 09 00 00 01"));
        // Not started yet: a middle boss doesn't count.
        assert!(m.feed(8, crate::state::OPCODE_MIDBOSS_KILLED, &killed(2_300_810)).is_none());
        assert_eq!(m.feed(30, OPCODE_DUNGEON_STEP, &hex("08 28 09 00 02")).unwrap().stages[0], 30);
        m.feed(150, crate::state::OPCODE_MIDBOSS_KILLED, &killed(2_300_810));
        let i = m.feed(490, crate::state::OPCODE_MIDBOSS_KILLED, &killed(2_300_811)).unwrap();
        assert_eq!(i.stages, [30, 150, 490, 0, 0, 0]);
        m.feed(723, OPCODE_DUNGEON_COMPLETE, &hex("00 08 28 09 00 01 00"));
        // Completed: no more stages.
        assert!(m.feed(730, crate::state::OPCODE_MIDBOSS_KILLED, &killed(2_300_811)).is_none());
    }

    #[test]
    fn moving_on_inside_a_dungeon_is_the_same_run() {
        // Fire Temple (2026-10-23): the map and its start again after each middle boss, then
        // a second run in the same connection, from step 1.
        let mut m = Maps::default();
        let ft = "d6 27 09 00";
        m.feed(7, OPCODE_MAP_ENTERED, &hex(&format!("{ft} 00 01")));
        m.feed(7, OPCODE_DUNGEON_STEP, &hex(&format!("{ft} 01")));
        m.feed(29, OPCODE_DUNGEON_STEP, &hex(&format!("{ft} 02")));
        m.feed(144, crate::state::OPCODE_MIDBOSS_KILLED, &2_300_311u32.to_le_bytes());
        assert!(m.feed(179, OPCODE_MAP_ENTERED, &hex(&format!("{ft} 00 01"))).is_none());
        assert!(m.feed(179, OPCODE_DUNGEON_STEP, &hex(&format!("{ft} 02"))).is_none());
        let i = m.feed(564, OPCODE_DUNGEON_COMPLETE, &hex(&format!("00 {ft} 01 00"))).unwrap();
        assert_eq!((i.map, i.entered_us, i.started_us, i.completed_us), (600_022, 7, Some(29), Some(564)));
        assert_eq!(i.stages[..3], [29, 144, 0]);
        // The next run: a new stay, whether the map comes first or step 1 does.
        assert_eq!(m.feed(642, OPCODE_MAP_ENTERED, &hex(&format!("{ft} 00 01"))).unwrap().entered_us, 642);
        m.feed(668, OPCODE_DUNGEON_STEP, &hex(&format!("{ft} 02")));
        let again = m.feed(700, OPCODE_DUNGEON_STEP, &hex(&format!("{ft} 01"))).unwrap();
        assert_eq!((again.entered_us, again.started_us), (700, None));
    }
}
