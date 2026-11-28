//! Your inventory: every stack you carry and how many, currencies among them (Kina, Abyss
//! Points and the rest are items too), and what each change adds or takes. Worked out on
//! our captures of 2026-11-28 (an Abyss farming run and an hour in town); nobody else's
//! notes had these.
//!
//! ```text
//! 0x5611  everything you carry, at login and with each new game server:
//!         5 bytes, then stacks
//! 0x561b  stacks that changed: u8 n, then n × (u8 what: 0 added, 1 count changed,
//!         2 removed; u8 has details; a stack; details…)
//! a stack u64 ID (its own, kept as its count changes), u32 item, u64 count, u8, u8,
//!         three zero bytes, then a name's length and the name (a crafted piece's
//!         maker), and more by the kind of item
//! ```
//!
//! What follows a stack varies in length (a piece of gear brings its stats), and we don't
//! read it: stacks are found by their shape instead ([`stacks`]). On the two captures that
//! finds exactly the `n` stacks each of 2,961 `0x561b` messages says it has, gear and
//! crafted pieces included, and all 215 of a login's.
//!
//! Checked against the game's own Abyss Points counter (`0xffab`: u64 this week's points,
//! u64 4,500,000): over 418 kills it rose by 36,485, and so did the Abyss Points stack
//! (930100003), kill for kill.

use std::collections::HashMap;

use serde::Serialize;

pub const OPCODE_INVENTORY: u16 = 0x5611;
pub const OPCODE_INVENTORY_CHANGED: u16 = 0x561b;

/// Currencies, as items.
pub const KINA_BOUND: u32 = 930_100_001;
pub const ABYSS_POINTS: u32 = 930_100_003;
pub const KINA: u32 = 930_100_031;

const ADDED: u8 = 0;
const REMOVED: u8 = 2;
/// A stack as far as we read it: ID, item, count, two bytes, three zero bytes.
const STACK_LEN: usize = 25;

/// How many of an item you have changed: by how much, and what you have now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ItemChange {
    pub item: u32,
    /// Gained (above 0) or spent, lost or used up (below).
    pub delta: i64,
    /// What you carry now, over every stack of it.
    pub total: u64,
}

/// A stack found in a payload: where it starts, its ID, item and count.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Stack {
    at: usize,
    id: u64,
    item: u32,
    count: u64,
}

/// Every stack in `p` from `from` on, by shape: a stack ID (never 0; its fifth and eighth
/// bytes are), an item ID (nine digits), a count (under 2^40), two bytes and three zeros.
fn stacks(p: &[u8], from: usize) -> Vec<Stack> {
    let mut found = Vec::new();
    let mut i = from;
    while let Some(s) = p.get(i..i + STACK_LEN) {
        let id = u64::from_le_bytes(s[..8].try_into().unwrap());
        let item = u32::from_le_bytes(s[8..12].try_into().unwrap());
        let count = u64::from_le_bytes(s[12..20].try_into().unwrap());
        if id != 0 && s[4] == 0 && s[7] == 0 && (100_000_000..1_000_000_000).contains(&item) && count < 1 << 40 && s[22..25] == [0, 0, 0] {
            found.push(Stack { at: i, id, item, count });
            i += STACK_LEN;
        } else {
            i += 1;
        }
    }
    found
}

/// What you carry, kept up from the game's messages.
#[derive(Debug, Default)]
pub struct Inventory {
    /// Each stack's item and count, by the stack's ID.
    stacks: HashMap<u64, (u32, u64)>,
    /// The game has sent everything you carry (this run of the meter).
    known: bool,
    changes: Vec<ItemChange>,
}

impl Inventory {
    pub fn feed(&mut self, opcode: u16, p: &[u8]) {
        match opcode {
            OPCODE_INVENTORY => {
                // Everything, again: what changed while we weren't told isn't a gain.
                let all = stacks(p, 0);
                if all.is_empty() {
                    return;
                }
                self.stacks = all.into_iter().map(|s| (s.id, (s.item, s.count))).collect();
                self.known = true;
            }
            OPCODE_INVENTORY_CHANGED => self.changed(p),
            _ => {}
        }
    }

    fn changed(&mut self, p: &[u8]) {
        let found = stacks(p, 1);
        // Not what it says it holds: a shape we don't know. Leave it.
        if found.is_empty() || p.first().map(|n| usize::from(*n)) != Some(found.len()) {
            return;
        }
        // An item's change over the whole message: a stack moved (taken away here, added
        // there) is no gain.
        let mut deltas: Vec<(u32, i64)> = Vec::new();
        for s in found {
            // Its "what" is two bytes before the stack.
            let what = if s.at >= 2 { p[s.at - 2] } else { continue };
            let count = if what == REMOVED { 0 } else { s.count };
            let before = self.stacks.get(&s.id).map(|(_, n)| *n);
            let delta = match before {
                Some(n) => count as i64 - n as i64,
                // A new stack is all gain. One we never saw, changing: how many it had
                // isn't known, so neither is the change.
                None if what == ADDED => count as i64,
                None => 0,
            };
            if count == 0 {
                self.stacks.remove(&s.id);
            } else {
                self.stacks.insert(s.id, (s.item, count));
            }
            match deltas.iter_mut().find(|(item, _)| *item == s.item) {
                Some((_, d)) => *d += delta,
                None => deltas.push((s.item, delta)),
            }
        }
        for (item, delta) in deltas.into_iter().filter(|(_, d)| *d != 0) {
            let total = self.total(item);
            self.changes.push(ItemChange { item, delta, total });
        }
    }

    /// How many of an item you carry, over every stack of it (0 when none, or not known).
    pub fn total(&self, item: u32) -> u64 {
        self.stacks.values().filter(|(i, _)| *i == item).map(|(_, n)| *n).sum()
    }

    /// The game has sent everything you carry, so [`Self::total`] is the whole count.
    pub fn known(&self) -> bool {
        self.known
    }

    /// The changes since last asked, oldest first.
    pub fn take_changes(&mut self) -> Vec<ItemChange> {
        std::mem::take(&mut self.changes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(s: &str) -> Vec<u8> {
        (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect()
    }

    /// A stack with nothing after it but the zeros and reason bytes of a plain one.
    fn plain(what: u8, id: u64, item: u32, count: u64) -> Vec<u8> {
        let mut e = vec![what, 0];
        e.extend(id.to_le_bytes());
        e.extend(item.to_le_bytes());
        e.extend(count.to_le_bytes());
        e.extend([1, 2, 0, 0, 0, 0]);
        e.extend([0; 9]);
        e.extend([0x0b, 0]);
        e
    }

    fn message(entries: &[Vec<u8>]) -> Vec<u8> {
        let mut p = vec![entries.len() as u8];
        for e in entries {
            p.extend(e);
        }
        p.push(0);
        p
    }

    // Two kills in the Abyss (2026-11-28): the Abyss Points stack at 1,631, then 1,893.
    const AP_1631: &str = "010100c692050000330000233370375f060000000000000102000000000000000000000000000b0000";
    const AP_1893: &str = "010100c6920500003300002333703765070000000000000102000000000000000000000000000b0000";

    #[test]
    fn a_kills_abyss_points_are_the_stacks_rise() {
        let mut inv = Inventory::default();
        // The first sight of the stack: how many it had before isn't known.
        inv.feed(OPCODE_INVENTORY_CHANGED, &hex(AP_1631));
        assert!(inv.take_changes().is_empty());
        assert_eq!(inv.total(ABYSS_POINTS), 1631);
        inv.feed(OPCODE_INVENTORY_CHANGED, &hex(AP_1893));
        assert_eq!(inv.take_changes(), [ItemChange { item: ABYSS_POINTS, delta: 262, total: 1893 }]);
        assert!(inv.take_changes().is_empty());
        assert!(!inv.known());
    }

    #[test]
    fn a_soul_is_picked_up_and_used() {
        // A Soul: Marsh Targon (534660175) added, then removed a tenth of a second later
        // (it goes to the pet).
        let mut inv = Inventory::default();
        inv.feed(OPCODE_INVENTORY_CHANGED, &hex("0100009dc80a00003200004f44de1f0100000000000000040000000000000000000000000000050000"));
        assert_eq!(inv.take_changes(), [ItemChange { item: 534_660_175, delta: 1, total: 1 }]);
        inv.feed(OPCODE_INVENTORY_CHANGED, &hex("0102009dc80a00003200004f44de1f0000000000000000040000000000000000000000000000002600"));
        assert_eq!(inv.take_changes(), [ItemChange { item: 534_660_175, delta: -1, total: 0 }]);
    }

    #[test]
    fn a_message_with_two_stacks_and_one_with_gear() {
        let mut inv = Inventory::default();
        inv.feed(OPCODE_INVENTORY, &{
            let mut all = vec![0x10, 0x02, 0x02, 0x00, 0x00];
            all.extend(&plain(0, 0x0001_ab00_0004_aa9e, 516_140_001, 3)[2..]);
            all.extend(&plain(0, 0x0001_aa00_0005_3972, KINA, 10_000_000)[2..]);
            all
        });
        assert!(inv.known());
        assert_eq!((inv.total(KINA), inv.total(516_140_001)), (10_000_000, 3));
        assert!(inv.take_changes().is_empty(), "what you had already is no gain");
        // Sold at the market (as recorded): the item's stack removed, the Kina stack up to
        // 9,124,265... here down from 10,000,000, so spent.
        inv.feed(
            OPCODE_INVENTORY_CHANGED,
            &hex("0202009eaa040000ab0100e1abc31e0000000000000000012e00000000000000000000000000000e0001007239050000aa01003f337037a9398b000000000001000000000000000000000000004c0000"),
        );
        assert_eq!(
            inv.take_changes(),
            [ItemChange { item: 516_140_001, delta: -3, total: 0 }, ItemChange { item: KINA, delta: 9_124_265 - 10_000_000, total: 9_124_265 }]
        );
        // A piece of gear looted (110160025), its stats after the stack: one stack, one gain.
        inv.feed(
            OPCODE_INVENTORY_CHANGED,
            &hex("010001785808000042000099e890060100000000000000012e00000000000000000000000000000000000000000000000000000014000000000100000000000000000000000000000000030000000000000000000000000000000000000000050000"),
        );
        assert_eq!(inv.take_changes(), [ItemChange { item: 110_160_025, delta: 1, total: 1 }]);
    }

    #[test]
    fn a_stack_moved_is_no_gain_and_everything_again_is_no_change() {
        let mut inv = Inventory::default();
        inv.feed(OPCODE_INVENTORY_CHANGED, &message(&[plain(0, 0x0000_3200_0000_0011, 533_460_001, 500)]));
        assert_eq!(inv.take_changes().len(), 1);
        // Taken out of one stack and put in a new one, in one message.
        inv.feed(OPCODE_INVENTORY_CHANGED, &message(&[plain(2, 0x0000_3200_0000_0011, 533_460_001, 0), plain(0, 0x0000_3200_0000_0012, 533_460_001, 500)]));
        assert!(inv.take_changes().is_empty());
        assert_eq!(inv.total(533_460_001), 500);
        // Split: 200 of it into a new stack.
        inv.feed(OPCODE_INVENTORY_CHANGED, &message(&[plain(1, 0x0000_3200_0000_0012, 533_460_001, 300), plain(0, 0x0000_3200_0000_0013, 533_460_001, 200)]));
        assert!(inv.take_changes().is_empty());
        // A new game server sends everything again, with more than we last saw (earned
        // while we weren't told): not a gain.
        let mut all = vec![0x10, 0x02, 0x01, 0x00, 0x00];
        all.extend(&plain(0, 0x0000_3200_0000_0012, 533_460_001, 900)[2..]);
        inv.feed(OPCODE_INVENTORY, &all);
        assert!(inv.take_changes().is_empty());
        assert_eq!(inv.total(533_460_001), 900);
        // And from there changes count again.
        inv.feed(OPCODE_INVENTORY_CHANGED, &message(&[plain(1, 0x0000_3200_0000_0012, 533_460_001, 899)]));
        assert_eq!(inv.take_changes(), [ItemChange { item: 533_460_001, delta: -1, total: 899 }]);
    }

    #[test]
    fn a_message_that_isnt_what_it_says_is_left() {
        let mut inv = Inventory::default();
        // Says two stacks, holds one.
        let mut p = message(&[plain(0, 0x0000_3200_0000_0011, 533_460_001, 5)]);
        p[0] = 2;
        inv.feed(OPCODE_INVENTORY_CHANGED, &p);
        inv.feed(OPCODE_INVENTORY_CHANGED, &[]);
        inv.feed(OPCODE_INVENTORY_CHANGED, &[1, 2, 3]);
        inv.feed(OPCODE_INVENTORY, &[0x10, 0x02]);
        assert!(inv.take_changes().is_empty() && !inv.known() && inv.total(533_460_001) == 0);
    }
}
