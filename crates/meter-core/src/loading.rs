//! The game's loading screen, between maps. The overlay steps aside for it.
//!
//! Worked out from two captures (2026-11-20 and 2026-11-21: 19 loads, into the world, a
//! dungeon, a raid and arena matches). Every load goes the same way:
//!
//! - To another server (a dungeon, an arena, back to the world): [`OPCODE_SERVER_MOVE`] on
//!   the connection you're leaving, a new connection 0.35 to 0.6 s later, and on it, 1.3 to
//!   1.6 s after it began, [`OPCODE_MAP_PREPARE`].
//! - On the same server (a teleport, the next part of a dungeon): [`OPCODE_MAP_PREPARE`]
//!   alone.
//! - Then nothing but the clock (`0x3600`) for 3.3 to 7 s, while the game loads, and
//!   [`OPCODE_MAP_ENTERED`] with everything around you. The game's own "this player has
//!   loaded" for you (an arena match's `0xe31c`) came 1.2 to 1.3 s after it in both
//!   matches: the screen is taken to stay [`SETTLE_US`] past it.
//!
//! A 1v1's next round puts you back the same way in 0.75 s (two of them): taken as no
//! loading screen to wait out, so the overlay is back as you arrive.

use crate::dungeon::OPCODE_MAP_ENTERED;

/// You're being moved to another game server: u16, then the server's address as text and
/// its port (not read: only that it came). The connection ends 0.1 s later.
pub const OPCODE_SERVER_MOVE: u16 = 0x3616;
/// The map to load: u32 (a count of this connection's loads, from 1), u32 map ID, more
/// (not read: only that it came). Questlog's PrepareEnterMap.
pub const OPCODE_MAP_PREPARE: u16 = 0x3621;

/// How long the loading screen is taken to stay once you're on the map.
pub const SETTLE_US: u64 = 1_500_000;
/// A load shorter than this (a round's re-placing) has no screen to wait out.
const QUICK_US: u64 = 2_000_000;
/// The longest a load is taken to last: past it a message was missed, and the overlay
/// comes back.
const LOAD_MAX_US: u64 = 60_000_000;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum Stage {
    #[default]
    Playing,
    /// Loading, since then.
    Loading(u64),
    /// On the map: the screen stays until then.
    Settling(u64),
}

/// Follows the game's loads. Feed it every message.
#[derive(Debug, Default, Clone)]
pub struct Loading {
    stage: Stage,
}

impl Loading {
    pub fn feed(&mut self, ts_us: u64, opcode: u16) {
        match (opcode, self.stage) {
            // (The map's own message after a server move is the same load going on.)
            (OPCODE_SERVER_MOVE | OPCODE_MAP_PREPARE, Stage::Loading(since)) if ts_us.saturating_sub(since) < LOAD_MAX_US => {}
            (OPCODE_SERVER_MOVE | OPCODE_MAP_PREPARE, _) => self.stage = Stage::Loading(ts_us),
            (OPCODE_MAP_ENTERED, Stage::Loading(since)) => {
                let quick = ts_us.saturating_sub(since) < QUICK_US;
                self.stage = if quick { Stage::Playing } else { Stage::Settling(ts_us + SETTLE_US) };
            }
            _ => {}
        }
    }

    /// The loading screen is up.
    pub fn up(&self, now_us: u64) -> bool {
        match self.stage {
            Stage::Playing => false,
            Stage::Loading(since) => now_us < since + LOAD_MAX_US,
            Stage::Settling(until) => now_us < until,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const S: u64 = 1_000_000;

    #[test]
    fn a_load_to_another_server_and_one_on_the_same() {
        let mut l = Loading::default();
        assert!(!l.up(S));
        // Into an arena match, as in the capture: the move, the new connection's map, you're
        // there.
        l.feed(10 * S, OPCODE_SERVER_MOVE);
        assert!(l.up(10 * S));
        l.feed(11 * S + 900_000, OPCODE_MAP_PREPARE);
        assert!(l.up(15 * S));
        l.feed(16 * S + 500_000, OPCODE_MAP_ENTERED);
        assert!(l.up(17 * S + 900_000), "the screen stays a moment once you're on the map");
        assert!(!l.up(18 * S));
        // A teleport on the same server.
        l.feed(100 * S, OPCODE_MAP_PREPARE);
        assert!(l.up(103 * S));
        l.feed(106 * S + 600_000, OPCODE_MAP_ENTERED);
        assert!(l.up(108 * S) && !l.up(108 * S + 100_000));
    }

    #[test]
    fn a_rounds_re_placing_and_a_missed_message() {
        // A 1v1's next round: on the map again 0.75 s later, nothing to wait out.
        let mut l = Loading::default();
        l.feed(10 * S, OPCODE_MAP_PREPARE);
        assert!(l.up(10 * S + 500_000));
        l.feed(10 * S + 750_000, OPCODE_MAP_ENTERED);
        assert!(!l.up(10 * S + 750_000));
        // The map's message never comes: the load has its limit, and the next one starts
        // from its own time.
        l.feed(100 * S, OPCODE_MAP_PREPARE);
        assert!(l.up(159 * S) && !l.up(160 * S));
        l.feed(200 * S, OPCODE_MAP_PREPARE);
        assert!(l.up(259 * S) && !l.up(260 * S));
        // A map's message with no load before it (the meter started mid-load): nothing.
        let mut l = Loading::default();
        l.feed(5 * S, OPCODE_MAP_ENTERED);
        assert!(!l.up(5 * S));
    }
}
