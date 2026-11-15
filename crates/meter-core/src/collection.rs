//! Your collection: the skins (appearances) and wings you've unlocked. Message names and
//! layouts are Questlog's, each checked on our captures (2026-10-25/26).
//!
//! ```text
//! 0x8d5b  SkinInfo_NT, at login: varint n, n × (i32 skin, u16 count, u16 instances),
//!         then totals by kind (not used)
//! 0x8d5c  a skin unlocked (or its count up): the same list, then its kind's total
//! 0x5672  WingList_NT, at login: varint n, n × (i32 wing, u8 enchant level)
//! ```
//!
//! A skin's ID is its kind, the art's set number and its slot: 1 0SSS 1TT a weapon (TT as
//! in its gear's item ID: 01 greatsword … 08 staff), 3 0SSS 1PP armour (01 breastplate …
//! 07 cloak), 4 0SSS 1PP a whole outfit, 5 0SSS 101/103 earrings/necklace. Looting the
//! Strange Karma Bow (item 110450035, art `WP_BW_0014`) unlocked 10014104, and its helm
//! (210360027, `GM_0063…Helmet`) 30063103 (2026-10-26). The app ties them to items.
//!
//! Like pets, the lists come at login (and some other moves), so the app keeps each
//! character's [`CollectionState`] and hands it back ([`Collection::seed`]).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::protocol::read_varint;

pub const OPCODE_SKIN_LIST: u16 = 0x8d5b;
pub const OPCODE_SKIN_ADDED: u16 = 0x8d5c;
pub const OPCODE_WING_LIST: u16 = 0x5672;

/// Your skins (by skin ID, with the game's count) and wings (by wing ID, with their
/// enchant level), for keeping between runs.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CollectionState {
    pub skins: BTreeMap<u32, u16>,
    pub wings: BTreeMap<u32, u8>,
    /// Which lists the game has sent (a state saved with only one leaves the other unknown).
    pub skins_known: bool,
    pub wings_known: bool,
}

#[derive(Debug, Default)]
pub struct Collection {
    state: CollectionState,
    /// Counts changes, so the app saves only when there's something new.
    revision: u64,
}

impl Collection {
    pub fn feed(&mut self, opcode: u16, p: &[u8]) {
        match opcode {
            OPCODE_SKIN_LIST => {
                if let Some(skins) = parse_skins(p) {
                    self.state.skins = skins.into_iter().collect();
                    self.state.skins_known = true;
                    self.revision += 1;
                }
            }
            OPCODE_SKIN_ADDED => {
                if let Some(skins) = parse_skins(p) {
                    self.state.skins.extend(skins);
                    self.revision += 1;
                }
            }
            OPCODE_WING_LIST => {
                if let Some(wings) = parse_wings(p) {
                    self.state.wings = wings.into_iter().collect();
                    self.state.wings_known = true;
                    self.revision += 1;
                }
            }
            _ => {}
        }
    }

    /// Your collection, once the game has sent either list.
    pub fn state(&self) -> Option<CollectionState> {
        self.known().then(|| self.state.clone())
    }

    pub fn known(&self) -> bool {
        self.state.skins_known || self.state.wings_known
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// A saved state for a meter that started after the game's lists: what's unlocked
    /// since start goes on top of it; a list the game has sent since stays as it is.
    pub fn seed(&mut self, saved: &CollectionState) {
        if !self.state.skins_known {
            let added = std::mem::replace(&mut self.state.skins, saved.skins.clone());
            self.state.skins.extend(added);
            self.state.skins_known = saved.skins_known;
        }
        if !self.state.wings_known {
            self.state.wings = saved.wings.clone();
            self.state.wings_known = saved.wings_known;
        }
        self.revision += 1;
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }
}

fn u32_at(p: &[u8], i: usize) -> Option<u32> {
    Some(u32::from_le_bytes(p.get(i..i + 4)?.try_into().ok()?))
}

fn parse_skins(p: &[u8]) -> Option<Vec<(u32, u16)>> {
    let mut i = 0;
    let n = read_varint(p, &mut i)? as usize;
    if n > 20_000 || p.len() < i + 8 * n {
        return None;
    }
    (0..n)
        .map(|k| {
            let at = i + 8 * k;
            Some((u32_at(p, at)?, u16::from_le_bytes([p[at + 4], p[at + 5]])))
        })
        .collect()
}

fn parse_wings(p: &[u8]) -> Option<Vec<(u32, u8)>> {
    let mut i = 0;
    let n = read_varint(p, &mut i)? as usize;
    if n > 2_000 || p.len() < i + 5 * n {
        return None;
    }
    (0..n).map(|k| Some((u32_at(p, i + 5 * k)?, p[i + 5 * k + 4]))).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(s: &str) -> Vec<u8> {
        s.split_whitespace().map(|b| u8::from_str_radix(b, 16).unwrap()).collect()
    }

    #[test]
    fn the_lists_and_an_unlock_are_read() {
        let mut c = Collection::default();
        assert!(c.state().is_none());
        // Two skins (a starter greatsword, 6 of it; a dagger), then totals by kind.
        c.feed(OPCODE_SKIN_LIST, &hex("02 cd 9a 98 00 06 00 00 00 b9 9e 98 00 01 00 00 00 01 01 28 00 00 00"));
        // Two wings, the second at +3.
        c.feed(OPCODE_WING_LIST, &hex("02 a4 67 65 02 00 d0 68 65 02 03"));
        // Looted: skin 10007103, its kind's total after it.
        c.feed(OPCODE_SKIN_ADDED, &hex("01 3f b2 98 00 01 00 00 00 01 0a 00 00 00 00"));
        let s = c.state().unwrap();
        assert_eq!(s.skins.into_iter().collect::<Vec<_>>(), [(10_001_101, 6), (10_002_105, 1), (10_007_103, 1)]);
        assert_eq!(s.wings.into_iter().collect::<Vec<_>>(), [(40_200_100, 0), (40_200_400, 3)]);
        assert!(s.skins_known && s.wings_known);
        // Cut short: ignored.
        c.feed(OPCODE_SKIN_LIST, &hex("05 cd 9a 98 00"));
        assert_eq!(c.state().unwrap().skins.len(), 3);
    }

    #[test]
    fn a_saved_state_fills_in_what_the_game_hasnt_sent() {
        let saved = CollectionState { skins: [(1, 1)].into(), wings: [(9, 2)].into(), skins_known: true, wings_known: true };
        // Started after login: one unlock seen, then the saved lists.
        let mut c = Collection::default();
        c.feed(OPCODE_SKIN_ADDED, &hex("01 3f b2 98 00 01 00 00 00"));
        assert!(!c.known());
        c.seed(&saved);
        let s = c.state().unwrap();
        assert_eq!(s.skins.keys().copied().collect::<Vec<_>>(), [1, 10_007_103]);
        assert_eq!(s.wings.keys().copied().collect::<Vec<_>>(), [9]);
        // The game's own list since wins.
        let mut c = Collection::default();
        c.feed(OPCODE_WING_LIST, &hex("01 a4 67 65 02 00"));
        c.seed(&saved);
        assert_eq!(c.state().unwrap().wings.keys().copied().collect::<Vec<_>>(), [40_200_100]);
    }
}
