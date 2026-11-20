//! An arena match's two full-screen screens: the line-up ("BLUE VS RED") as it begins and
//! the result ("Victory" / "Defeat", the score table) as it ends. The overlay steps aside
//! for both.
//!
//! Worked out from two matches in one capture (2026-11-20: a 5v5 Arena of Cooperation and
//! a 1v1, both in the Fire Temple Arena). A match is its own game-server connection:
//!
//! - [`OPCODE_ARENA_LINEUP`] as you arrive, with both teams;
//! - [`OPCODE_ARENA_LOADED`] as each player finishes loading;
//! - [`OPCODE_ARENA_START_AT`] once everyone has: the fight starts 20 s later (both
//!   matches), as the dungeon state's step 2 (`0x6100`, with 7 for its map) says when it
//!   does. Nobody moved in the first 10 s of those 20 in either match (ten players in one,
//!   the first at 10.55 s): that's taken as the line-up screen's time, [`INTRO_US`];
//! - [`OPCODE_ARENA_KILL`] for each kill (a 1v1 is rounds: you're put back with a new
//!   `0x4002` for the same map);
//! - [`OPCODE_ARENA_RESULT`] with the dungeon's "complete" as it ends: the result screen
//!   stays until you leave (30 s at most, the state's step 3), which is the next `0x4002`,
//!   for another map.

use crate::dungeon::{OPCODE_DUNGEON_STEP, OPCODE_MAP_ENTERED};

/// The match's line-up, as you arrive: both teams' players (8-byte character ID, name,
/// class, rating…). Not read: only that it came.
pub const OPCODE_ARENA_LINEUP: u16 = 0xe319;
/// Players who've finished loading: u8 count, count × 8-byte character ID. Those before
/// you as you arrive, then each as they do.
pub const OPCODE_ARENA_LOADED: u16 = 0xe31c;
/// Everyone has loaded: u64, when the fight starts (server time, ms since 1970; 20 s on).
pub const OPCODE_ARENA_START_AT: u16 = 0xe31d;
/// A kill: the killer's 8-byte character ID, then the killed player's.
pub const OPCODE_ARENA_KILL: u16 = 0xe325;
/// The match's result, for you: u8, u32 your rating after it, u8, u8, your 8-byte character
/// ID, u8 kills, u8 deaths, u16.
pub const OPCODE_ARENA_RESULT: u16 = 0xe328;

/// The dungeon state (`0x6100`) for the fight having started.
const STEP_STARTED: u8 = 2;
/// How long the line-up screen is taken to stay once everyone has loaded.
pub const INTRO_US: u64 = 10_000_000;
/// The longest the wait for everyone to load is taken to be (the game's own limit before
/// the fight is 110 s): past it something was missed, and the overlay comes back.
const WAIT_MAX_US: u64 = 120_000_000;
/// The longest the result screen is taken to stay (the game sends you out after 30 s).
const RESULT_MAX_US: u64 = 40_000_000;

/// A screen of the game's that covers everything, in an arena match.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArenaScreen {
    /// The two teams' line-up, before the fight.
    Lineup,
    /// The result, after it.
    Result,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum Stage {
    /// Not in a match (or in one we didn't see begin).
    #[default]
    Out,
    /// Arrived (when): waiting for everyone to load.
    Waiting(u64),
    /// Everyone's loaded: the line-up shows until then.
    Intro(u64),
    Fighting,
    /// Over (when).
    Result(u64),
}

/// Follows an arena match. Feed it every message.
#[derive(Debug, Default, Clone)]
pub struct Arena {
    stage: Stage,
    /// The arena's map, once known: what tells a new round from having left.
    map: Option<u32>,
}

impl Arena {
    pub fn feed(&mut self, ts_us: u64, opcode: u16, p: &[u8]) {
        match opcode {
            OPCODE_ARENA_LINEUP => *self = Arena { stage: Stage::Waiting(ts_us), map: None },
            OPCODE_ARENA_START_AT if matches!(self.stage, Stage::Waiting(_)) => self.stage = Stage::Intro(ts_us + INTRO_US),
            OPCODE_DUNGEON_STEP if p.get(4) == Some(&STEP_STARTED) && matches!(self.stage, Stage::Waiting(_) | Stage::Intro(_)) => {
                self.stage = Stage::Fighting;
            }
            // Also for a match joined before the meter was started.
            OPCODE_ARENA_RESULT => self.stage = Stage::Result(ts_us),
            OPCODE_MAP_ENTERED if self.stage != Stage::Out => {
                let Some(map) = p.get(..4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]])) else { return };
                match self.map {
                    // A new round, on the same map.
                    Some(arena) if arena == map => {}
                    // Arriving: it comes just after the line-up.
                    None if !matches!(self.stage, Stage::Result(_)) => self.map = Some(map),
                    // Another map: you've left.
                    _ => *self = Arena::default(),
                }
            }
            _ => {}
        }
    }

    /// The full-screen screen the game is showing now, if it is.
    pub fn screen(&self, now_us: u64) -> Option<ArenaScreen> {
        match self.stage {
            Stage::Waiting(since) if now_us < since + WAIT_MAX_US => Some(ArenaScreen::Lineup),
            Stage::Intro(until) if now_us < until => Some(ArenaScreen::Lineup),
            Stage::Result(since) if now_us < since + RESULT_MAX_US => Some(ArenaScreen::Result),
            _ => None,
        }
    }

    /// In a match (from its line-up until you've left).
    pub fn in_match(&self) -> bool {
        self.stage != Stage::Out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::tests::hex;

    const S: u64 = 1_000_000;
    const ARENA: &str = "3c 00 00 00 00 01";
    const WORLD: &str = "56 04 00 00 00 01";

    /// The messages of the 1v1 in the capture (the line-up cut to its start, the other
    /// player's ID made up), at their times.
    fn arrive(a: &mut Arena) {
        a.feed(5 * S, OPCODE_ARENA_LINEUP, &hex("b2 d4 01 01 00 00 00 0b 00 00 00"));
        a.feed(5 * S, OPCODE_ARENA_LOADED, &hex("00"));
        a.feed(5 * S, OPCODE_MAP_ENTERED, &hex(ARENA));
        a.feed(5 * S, OPCODE_DUNGEON_STEP, &hex("07 00 00 00 01 a7 92 e1 f7 a1 01 00 00 57 40 e3 f7 a1 01 00 00"));
    }

    #[test]
    fn the_lineup_and_the_result_of_a_match() {
        let mut a = Arena::default();
        assert_eq!((a.screen(4 * S), a.in_match()), (None, false));
        arrive(&mut a);
        assert_eq!(a.screen(5 * S), Some(ArenaScreen::Lineup));
        // Everyone's loaded: the line-up for ten seconds more, then the wait in the arena.
        a.feed(6 * S, OPCODE_ARENA_LOADED, &hex("01 11 22 03 00 00 00 e0 05"));
        assert_eq!(a.screen(60 * S), Some(ArenaScreen::Lineup), "still waiting for someone to load");
        a.feed(7 * S, OPCODE_ARENA_START_AT, &hex("45 fe e1 f7 a1 01 00 00"));
        assert_eq!((a.screen(16 * S), a.screen(17 * S)), (Some(ArenaScreen::Lineup), None));
        a.feed(27 * S, OPCODE_DUNGEON_STEP, &hex("07 00 00 00 02 77 fe e1 f7 a1 01 00 00 37 26 eb f7 a1 01 00 00"));
        assert_eq!(a.screen(27 * S), None);
        // A round lost: put back on the same map, the fight's state sent again.
        a.feed(63 * S, OPCODE_ARENA_KILL, &hex("11 22 03 00 00 00 e0 05 58 de 03 00 00 00 c7 09"));
        a.feed(69 * S, OPCODE_MAP_ENTERED, &hex(ARENA));
        a.feed(69 * S, OPCODE_DUNGEON_STEP, &hex("07 00 00 00 02 77 fe e1 f7 a1 01 00 00 37 26 eb f7 a1 01 00 00"));
        assert_eq!((a.screen(70 * S), a.in_match()), (None, true));
        // Over: the result until you've left.
        a.feed(144 * S, OPCODE_ARENA_RESULT, &hex("01 bd 05 00 00 02 01 58 de 03 00 00 00 c7 09 01 03 00 00"));
        a.feed(144 * S, OPCODE_DUNGEON_STEP, &hex("07 00 00 00 03 e2 c7 e3 f7 a1 01 00 00 12 3d e4 f7 a1 01 00 00"));
        assert_eq!(a.screen(148 * S), Some(ArenaScreen::Result));
        a.feed(154 * S, OPCODE_MAP_ENTERED, &hex(WORLD));
        assert_eq!((a.screen(154 * S), a.in_match()), (None, false));
    }

    #[test]
    fn a_missed_message_brings_the_overlay_back() {
        // The fight starts without "everyone's loaded" having come.
        let mut a = Arena::default();
        arrive(&mut a);
        a.feed(27 * S, OPCODE_DUNGEON_STEP, &hex("07 00 00 00 02 77 fe e1 f7 a1 01 00 00 37 26 eb f7 a1 01 00 00"));
        assert_eq!(a.screen(27 * S), None);
        // Nor the fight's start: the wait has its limit.
        let mut a = Arena::default();
        arrive(&mut a);
        assert_eq!((a.screen(124 * S), a.screen(126 * S)), (Some(ArenaScreen::Lineup), None));
        // Leaving before the fight.
        a.feed(130 * S, OPCODE_MAP_ENTERED, &hex(WORLD));
        assert!(!a.in_match());
        // A result with nothing before it (the meter started mid-match): shown, until the
        // next map or its own limit.
        let mut a = Arena::default();
        a.feed(10 * S, OPCODE_ARENA_RESULT, &hex("01 b3 05 00 00 02 01 58 de 03 00 00 00 c7 09 00 01 00 00"));
        assert_eq!((a.screen(11 * S), a.screen(51 * S)), (Some(ArenaScreen::Result), None));
        a.feed(20 * S, OPCODE_MAP_ENTERED, &hex(WORLD));
        assert_eq!(a.screen(21 * S), None);
        // A dungeon's own start isn't a match's.
        let mut a = Arena::default();
        a.feed(S, OPCODE_MAP_ENTERED, &hex("08 28 09 00 00 01"));
        a.feed(2 * S, OPCODE_DUNGEON_STEP, &hex("08 28 09 00 02 ef 7d c4 62 a1 01 00 00 6f 6c fb 62 a1 01 00 00"));
        assert_eq!((a.screen(2 * S), a.in_match()), (None, false));
    }
}
