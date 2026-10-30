//! Other players' equipped gear, from their appearance (`0x3645`).
//!
//! The spawn holds the player's equipment as entries of a byte (`0x0e`, `0x0f` for the
//! weapon), the item ID (u32), its enhancement level (u8) and its slot (u8: 1 the weapon,
//! 2 helm, 3 pauldrons, 4 chest, 5 greaves, 6 gloves, 7 boots, 8 cloak, 9 necklace,
//! 10 and 11 earrings, …), slots in order. Worked out on our captures (2026-10-30): e.g.
//! "Enraged Kromede Sword" +19 in slot 1, then Nuakum and Vakron armour +11 to +13.
//! So a player's gear is known once they've been in view.

use std::collections::HashMap;

use serde::Serialize;

use crate::entities::{parse_named_spawn, OPCODE_PLAYER_SPAWN};

/// One equipped item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct GearPiece {
    pub slot: u8,
    pub item: u32,
    pub enhance: u8,
}

/// Equipment IDs: weapons 1xx, armour 2xx, accessories 3xx (nine digits).
const ITEM_IDS: std::ops::RangeInclusive<u32> = 100_000_000..=399_999_999;
const MAX_SLOT: u8 = 24;
const MAX_ENHANCE: u8 = 40;

/// The gear in a player's spawn, slots in order.
pub fn parse_gear(p: &[u8]) -> Vec<GearPiece> {
    let mut out: Vec<GearPiece> = Vec::new();
    let mut i = 0;
    while i + 7 <= p.len() {
        let tag = p[i];
        if tag == 0x0e || tag == 0x0f {
            let item = u32::from_le_bytes([p[i + 1], p[i + 2], p[i + 3], p[i + 4]]);
            let (enhance, slot) = (p[i + 5], p[i + 6]);
            let next_slot = out.last().map_or(1, |g| g.slot + 1);
            if ITEM_IDS.contains(&item) && enhance <= MAX_ENHANCE && (next_slot..=MAX_SLOT).contains(&slot) {
                out.push(GearPiece { slot, item, enhance });
                i += 7;
                continue;
            }
        }
        i += 1;
    }
    out
}

/// Players' gear by name, as last seen.
#[derive(Debug, Default)]
pub struct GearBook {
    by_name: HashMap<String, Vec<GearPiece>>,
}

impl GearBook {
    pub fn feed(&mut self, opcode: u16, p: &[u8]) {
        if opcode != OPCODE_PLAYER_SPAWN {
            return;
        }
        let Some((_, name)) = parse_named_spawn(p) else { return };
        let gear = parse_gear(p);
        if !gear.is_empty() {
            self.by_name.insert(name, gear);
        }
    }

    pub fn of(&self, name: &str) -> Option<&[GearPiece]> {
        self.by_name.get(name).map(|g| &g[..])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_equipment_in_slot_order() {
        // Bytes around the gear in a real spawn: a weapon +19 in slot 1, a helm +12 in 2,
        // then a stray 0x0e that isn't an entry, and pauldrons +12 in 3.
        let mut p = vec![0x48, 0x00, 0x92, 0xa7, 0x46];
        p.extend([0x0f]);
        p.extend(110_230_051u32.to_le_bytes());
        p.extend([0x13, 0x01, 0, 0, 0, 0, 0x9d]);
        p.extend([0x0e]);
        p.extend(210_330_053u32.to_le_bytes());
        p.extend([0x0c, 0x02, 0x0e, 0x01, 0x02]);
        p.extend([0x0e]);
        p.extend(210_430_053u32.to_le_bytes());
        p.extend([0x0c, 0x03, 0, 0]);
        let g = parse_gear(&p);
        assert_eq!(g.iter().map(|g| (g.slot, g.item, g.enhance)).collect::<Vec<_>>(), [(1, 110_230_051, 19), (2, 210_330_053, 12), (3, 210_430_053, 12)]);
    }
}
