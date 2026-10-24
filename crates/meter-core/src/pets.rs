//! Pet souls: each kill of a pet's kind gives its soul, and souls summon the pet and then
//! level it up. Message names are Questlog's, each checked on our captures (2026-10-23/24).
//!
//! ```text
//! 0x9000  your pets at login: u32, varint n, n × (u32 key, u32 pet, u32 level),
//!         varint n, n × (u32 pet, u32 souls), …
//! 0x9009  a pet summoned: varint entity, u32 pet, u32 level, …
//! 0x900d  souls gained: varint n, n × (u32 pet, u32 souls)   (1 a kill in our captures)
//! 0x900e  souls spent: the same                              (5 to summon a pet)
//! ```
//!
//! A pet takes [`SOULS_PER_LEVEL`] souls to summon (level 1), then to reach levels 2 and 3
//! (Questlog's table; "Souls to summon: 5" in the scraped pet data). Its souls in the list
//! are those toward its next level: on 2026-10-24 Ursus was level 1 with 24, and its next
//! soul was spent at once (25, level 2). Pets from reward boxes or the shop come in the
//! list and with `0x9009` as well.
//!
//! The list comes at login, on entering or leaving a dungeon and with some other moves,
//! but not with every zone change (on 2026-10-24: three zone changes and 19 souls with
//! none, then one with a respawn 20 minutes later), so a meter started in between can know
//! no pet: the app keeps each character's [`PetState`] and hands it back ([`Pets::seed`]).

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};

use crate::protocol::read_varint;

pub const OPCODE_PET_LIST: u16 = 0x9000;
pub const OPCODE_PET_SUMMONED: u16 = 0x9009;
pub const OPCODE_SOULS_GAINED: u16 = 0x900d;
pub const OPCODE_SOULS_SPENT: u16 = 0x900e;

/// Souls to go from level 0 (not summoned) to 1, 1 to 2 and 2 to 3 (the highest).
pub const SOULS_PER_LEVEL: [u32; 3] = [5, 25, 75];

/// A pet's souls went up: what it has now and what its next level takes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SoulGain {
    pub pet: u32,
    pub gained: u32,
    /// Known only once the game's pet list has come (or a saved one was handed back): until
    /// then the gain alone is known.
    pub known: bool,
    pub souls: u32,
    /// Its level (0: not summoned yet), and the souls its next level takes (None at the top).
    pub level: u32,
    pub next_level_souls: Option<u32>,
    /// Filled in by the app from its pet data.
    pub name: Option<String>,
}

/// Every pet's level and souls toward its next one, for keeping between runs.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PetState {
    pub levels: BTreeMap<u32, u32>,
    pub souls: BTreeMap<u32, u32>,
}

#[derive(Debug, Default)]
pub struct Pets {
    level: HashMap<u32, u32>,
    souls: HashMap<u32, u32>,
    gains: Vec<SoulGain>,
    /// The game's list (or a saved state) has come.
    known: bool,
    /// Counts changes, so the app saves only when there's something new.
    revision: u64,
}

impl Pets {
    pub fn feed(&mut self, opcode: u16, p: &[u8]) {
        match opcode {
            OPCODE_PET_LIST => {
                if let Some((levels, souls)) = parse_pet_list(p) {
                    self.level = levels;
                    self.souls = souls;
                    self.known = true;
                    self.revision += 1;
                }
            }
            OPCODE_PET_SUMMONED => {
                let mut i = 0;
                if read_varint(p, &mut i).is_none() {
                    return;
                }
                if let (Some(pet), Some(level)) = (u32_at(p, i), u32_at(p, i + 4)) {
                    let l = self.level.entry(pet).or_default();
                    *l = (*l).max(level);
                    self.revision += 1;
                }
            }
            OPCODE_SOULS_GAINED | OPCODE_SOULS_SPENT => {
                for (pet, n) in parse_souls(p).unwrap_or_default() {
                    self.revision += 1;
                    let souls = self.souls.entry(pet).or_default();
                    if opcode == OPCODE_SOULS_SPENT {
                        *souls = souls.saturating_sub(n);
                        // Spending what the next level takes is that level (summoning sends
                        // its own message too).
                        let level = self.level.entry(pet).or_default();
                        if SOULS_PER_LEVEL.get(*level as usize) == Some(&n) {
                            *level += 1;
                        }
                        continue;
                    }
                    *souls += n;
                    let level = self.level.get(&pet).copied().unwrap_or(0);
                    self.gains.push(SoulGain {
                        pet,
                        gained: n,
                        known: self.known,
                        souls: *souls,
                        level,
                        next_level_souls: SOULS_PER_LEVEL.get(level as usize).copied(),
                        name: None,
                    });
                }
            }
            _ => {}
        }
    }

    /// Every pet's level and souls, once known.
    pub fn state(&self) -> Option<PetState> {
        self.known.then(|| PetState {
            levels: self.level.iter().map(|(k, v)| (*k, *v)).collect(),
            souls: self.souls.iter().map(|(k, v)| (*k, *v)).collect(),
        })
    }

    pub fn known(&self) -> bool {
        self.known
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// A saved state for a meter that started after the game's list: souls gained since
    /// start go on top of it.
    pub fn seed(&mut self, state: &PetState) {
        if self.known {
            return;
        }
        let gained = std::mem::take(&mut self.souls);
        self.level = state.levels.iter().map(|(k, v)| (*k, *v)).collect();
        self.souls = state.souls.iter().map(|(k, v)| (*k, *v)).collect();
        for (pet, n) in gained {
            *self.souls.entry(pet).or_default() += n;
        }
        self.known = true;
        self.revision += 1;
    }

    /// Souls gained since the last call, oldest first.
    pub fn take_gains(&mut self) -> Vec<SoulGain> {
        std::mem::take(&mut self.gains)
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }
}

type Levels = HashMap<u32, u32>;

fn parse_pet_list(p: &[u8]) -> Option<(Levels, HashMap<u32, u32>)> {
    let mut i = 4;
    let n = read_varint(p, &mut i)?;
    let mut levels = HashMap::new();
    for _ in 0..n.min(1024) {
        levels.insert(u32_at(p, i + 4)?, u32_at(p, i + 8)?);
        i += 12;
    }
    let n = read_varint(p, &mut i)?;
    let mut souls = HashMap::new();
    for _ in 0..n.min(1024) {
        souls.insert(u32_at(p, i)?, u32_at(p, i + 4)?);
        i += 8;
    }
    Some((levels, souls))
}

fn parse_souls(p: &[u8]) -> Option<Vec<(u32, u32)>> {
    let mut i = 0;
    let n = read_varint(p, &mut i)?;
    (0..n.min(64))
        .map(|k| {
            let at = i + 8 * k as usize;
            Some((u32_at(p, at)?, u32_at(p, at + 4)?))
        })
        .collect()
}

fn u32_at(p: &[u8], i: usize) -> Option<u32> {
    Some(u32::from_le_bytes(p.get(i..i + 4)?.try_into().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::tests::hex;

    #[test]
    fn a_meter_started_after_the_list_learns_it_from_a_saved_state() {
        let mut pets = Pets::default();
        // No list yet: the gain alone.
        pets.feed(OPCODE_SOULS_GAINED, &hex("01 a4 04 00 00 01 00 00 00"));
        assert!(!pets.take_gains()[0].known);
        assert!(pets.state().is_none());
        // Vivel (1188) saved at level 1 with 8: the soul since start goes on top.
        let saved = PetState { levels: [(1188, 1)].into(), souls: [(1188, 8)].into() };
        pets.seed(&saved);
        pets.feed(OPCODE_SOULS_GAINED, &hex("01 a4 04 00 00 01 00 00 00"));
        let g = pets.take_gains();
        assert_eq!((g[0].known, g[0].souls, g[0].level, g[0].next_level_souls), (true, 10, 1, Some(25)));
        // A list later replaces it; a second seed changes nothing.
        let rev = pets.revision();
        pets.seed(&PetState::default());
        assert_eq!(pets.revision(), rev);
        assert_eq!(pets.state().unwrap().souls[&1188], 10);
    }

    #[test]
    fn souls_toward_the_next_level() {
        let mut pets = Pets::default();
        // Login: pet 1004 at level 1 with 3 souls; 1091 not summoned, 4 souls.
        let mut list = vec![0x24, 0x04, 0, 0, 1];
        for v in [1004u32, 1004, 1] {
            list.extend(v.to_le_bytes());
        }
        list.push(2);
        for v in [1004u32, 3, 1091, 4] {
            list.extend(v.to_le_bytes());
        }
        pets.feed(OPCODE_PET_LIST, &list);
        // A kill of Red Spark Ignus (2026-10-23): one of 1091's souls. Five: summoned.
        pets.feed(OPCODE_SOULS_GAINED, &hex("01 43 04 00 00 01 00 00 00"));
        let g = pets.take_gains();
        assert_eq!((g[0].pet, g[0].souls, g[0].level, g[0].next_level_souls), (1091, 5, 0, Some(5)));
        pets.feed(OPCODE_SOULS_SPENT, &hex("01 43 04 00 00 05 00 00 00"));
        pets.feed(OPCODE_PET_SUMMONED, &hex("87 1d 43 04 00 00 01 00 00 00 05 09 00 00 00"));
        pets.feed(OPCODE_SOULS_GAINED, &hex("01 43 04 00 00 01 00 00 00"));
        let g = pets.take_gains();
        assert_eq!((g[0].souls, g[0].level, g[0].next_level_souls), (1, 1, Some(25)));
        // A pet at the top has no next level.
        pets.level.insert(1004, 3);
        let mut gain = vec![1];
        for v in [1004u32, 1] {
            gain.extend(v.to_le_bytes());
        }
        pets.feed(OPCODE_SOULS_GAINED, &gain);
        assert_eq!(pets.take_gains()[0].next_level_souls, None);
    }
}
