//! Who is who: your own character, nearby players, and mobs, keyed by entity ID.
//!
//! Fed with every message in order. Names arrive when an entity comes into view (or
//! after a zone change), so a label can start as a bare ID and gain a name later.
//! Layouts are in `docs/protocol.md`.

use std::collections::HashMap;

use crate::protocol::read_varint;

pub const OPCODE_SELF_SPAWN: u16 = 0x3633;
pub const OPCODE_SELF_TICK: u16 = 0x364a;
pub const OPCODE_MOB_SPAWN: u16 = 0x3641;
pub const OPCODE_PLAYER_SPAWN: u16 = 0x3645;

/// Bytes between the entity ID and the name length in player/self spawns.
const SPAWN_NAME_SKIP: usize = 5;
const MAX_NAME_BYTES: usize = 64;
/// Bytes between the entity ID and the type ID in mob spawns.
const MOB_TYPE_SKIP: usize = 3;
/// Mob type IDs seen so far are 7 digits. Anything far outside is a layout we don't know.
const MOB_TYPE_RANGE: std::ops::RangeInclusive<u32> = 100_000..=99_999_999;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    /// Our own entity ID (and name, when it came from a full self spawn).
    SelfIdentified { id: u64, name: Option<String> },
    /// A new login: previous entity IDs no longer mean anything.
    Relogin { old_id: u64, new_id: u64 },
    PlayerNamed { id: u64, name: String },
    MobSeen { id: u64, type_id: u32 },
}

#[derive(Debug, Default, Clone)]
pub struct Registry {
    self_id: Option<u64>,
    self_name: Option<String>,
    players: HashMap<u64, String>,
    mobs: HashMap<u64, u32>,
}

impl Registry {
    pub fn self_id(&self) -> Option<u64> {
        self.self_id
    }

    pub fn self_name(&self) -> Option<&str> {
        self.self_name.as_deref()
    }

    pub fn is_self(&self, id: u64) -> bool {
        self.self_id == Some(id)
    }

    pub fn player_name(&self, id: u64) -> Option<&str> {
        if self.is_self(id) {
            return self.self_name.as_deref();
        }
        self.players.get(&id).map(String::as_str)
    }

    pub fn mob_type(&self, id: u64) -> Option<u32> {
        self.mobs.get(&id).copied()
    }

    pub fn players(&self) -> impl Iterator<Item = (u64, &str)> {
        self.players.iter().map(|(id, n)| (*id, n.as_str()))
    }

    pub fn mobs(&self) -> impl Iterator<Item = (u64, u32)> + '_ {
        self.mobs.iter().map(|(id, t)| (*id, *t))
    }

    /// A display name: your name, a player's name, "Mob <type>", or "#<id>".
    pub fn label(&self, id: u64) -> String {
        if self.is_self(id) {
            return match &self.self_name {
                Some(n) => n.clone(),
                None => format!("You (#{id})"),
            };
        }
        if let Some(n) = self.players.get(&id) {
            return n.clone();
        }
        if let Some(t) = self.mobs.get(&id) {
            return format!("Mob {t}");
        }
        format!("#{id}")
    }

    /// Updates from one message. Returns what changed, if anything.
    pub fn observe(&mut self, opcode: u16, payload: &[u8]) -> Option<Change> {
        match opcode {
            OPCODE_SELF_SPAWN => {
                let (id, name) = parse_named_spawn(payload)?;
                if let Some(change) = self.set_self(id) {
                    self.self_name = Some(name);
                    return Some(change);
                }
                let renamed = self.self_name.as_deref() != Some(&name);
                self.self_name = Some(name.clone());
                renamed.then_some(Change::SelfIdentified { id, name: Some(name) })
            }
            OPCODE_SELF_TICK => {
                let id = parse_self_tick(payload)?;
                self.set_self(id)
            }
            OPCODE_PLAYER_SPAWN => {
                let (id, name) = parse_named_spawn(payload)?;
                if self.is_self(id) || self.players.get(&id) == Some(&name) {
                    return None;
                }
                self.mobs.remove(&id);
                self.players.insert(id, name.clone());
                Some(Change::PlayerNamed { id, name })
            }
            OPCODE_MOB_SPAWN => {
                let (id, type_id) = parse_mob_spawn(payload)?;
                if self.mobs.get(&id) == Some(&type_id) {
                    return None;
                }
                self.players.remove(&id);
                self.mobs.insert(id, type_id);
                Some(Change::MobSeen { id, type_id })
            }
            _ => None,
        }
    }

    fn set_self(&mut self, id: u64) -> Option<Change> {
        match self.self_id {
            Some(old) if old == id => None,
            Some(old) => {
                // Entity IDs are per login, so everything we knew is stale.
                *self = Registry::default();
                self.self_id = Some(id);
                Some(Change::Relogin {
                    old_id: old,
                    new_id: id,
                })
            }
            None => {
                self.self_id = Some(id);
                Some(Change::SelfIdentified {
                    id,
                    name: self.self_name.clone(),
                })
            }
        }
    }
}

/// `0x3633` / `0x3645`: varint entity ID, 5 bytes, u8 length, UTF-8 name, …
pub fn parse_named_spawn(p: &[u8]) -> Option<(u64, String)> {
    let mut i = 0;
    let id = read_varint(p, &mut i)?;
    i += SPAWN_NAME_SKIP;
    let len = *p.get(i)? as usize;
    i += 1;
    if len == 0 || len > MAX_NAME_BYTES {
        return None;
    }
    let name = std::str::from_utf8(p.get(i..i + len)?).ok()?;
    if name.chars().any(char::is_control) {
        return None;
    }
    Some((id, name.to_string()))
}

/// `0x364a`: varint entity ID (ours), then mostly zeros.
pub fn parse_self_tick(p: &[u8]) -> Option<u64> {
    let mut i = 0;
    read_varint(p, &mut i)
}

/// `0x3641`: varint entity ID, 3 bytes, u32 type ID, …
pub fn parse_mob_spawn(p: &[u8]) -> Option<(u64, u32)> {
    let mut i = 0;
    let id = read_varint(p, &mut i)?;
    i += MOB_TYPE_SKIP;
    let type_id = u32::from_le_bytes(p.get(i..i + 4)?.try_into().ok()?);
    MOB_TYPE_RANGE.contains(&type_id).then_some((id, type_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(s: &str) -> Vec<u8> {
        s.split_whitespace()
            .map(|b| u8::from_str_radix(b, 16).unwrap())
            .collect()
    }

    fn with_name(prefix: &str, name: &str, suffix: &str) -> Vec<u8> {
        let mut p = hex(prefix);
        p.push(name.len() as u8);
        p.extend_from_slice(name.as_bytes());
        p.extend(hex(suffix));
        p
    }

    // Prefixes below are from our own captures.

    #[test]
    fn self_spawn_names_us() {
        let p = with_name("cb 1c 5f 81 c1 28 37", "NeneneTowawa", "00 00 00 02");
        assert_eq!(parse_named_spawn(&p), Some((3659, "NeneneTowawa".into())));

        let mut r = Registry::default();
        assert_eq!(
            r.observe(OPCODE_SELF_SPAWN, &p),
            Some(Change::SelfIdentified { id: 3659, name: None })
        );
        assert_eq!(r.label(3659), "NeneneTowawa");
        assert!(r.is_self(3659));
    }

    #[test]
    fn self_tick_identifies_us_without_a_name() {
        let mut r = Registry::default();
        r.observe(OPCODE_SELF_TICK, &hex("ff 18 00 00 00 00 00 00 00 00 00"));
        assert_eq!(r.self_id(), Some(3199));
        assert_eq!(r.label(3199), "You (#3199)");
        // Repeated ticks change nothing.
        assert_eq!(r.observe(OPCODE_SELF_TICK, &hex("ff 18 00 00")), None);
    }

    #[test]
    fn player_spawn_with_non_latin_name() {
        let p = with_name("fd 63 15 b0 a4 00 07", "黑魔導", "1c 00 00 00");
        let mut r = Registry::default();
        assert_eq!(
            r.observe(OPCODE_PLAYER_SPAWN, &p),
            Some(Change::PlayerNamed { id: 12797, name: "黑魔導".into() })
        );
        assert_eq!(r.label(12797), "黑魔導");
    }

    #[test]
    fn mob_spawn_type() {
        let p = hex("ee a6 02 04 20 00 09 9f 24 00 00 02 7e 3d b3 c7 60 02 8e c7");
        assert_eq!(parse_mob_spawn(&p), Some((37742, 2_400_009)));
        let mut r = Registry::default();
        r.observe(OPCODE_MOB_SPAWN, &p);
        assert_eq!(r.label(37742), "Mob 2400009");
        assert_eq!(r.label(1), "#1");
    }

    #[test]
    fn unknown_mob_layout_is_ignored() {
        // Type field far outside the 7-digit range.
        let p = hex("ee a6 02 04 20 00 07 b0 61 66 00");
        assert_eq!(parse_mob_spawn(&p), None);
    }

    #[test]
    fn relogin_clears_stale_ids() {
        let mut r = Registry::default();
        r.observe(OPCODE_SELF_TICK, &hex("ff 18 00"));
        r.observe(OPCODE_PLAYER_SPAWN, &with_name("e2 4f 01 20 a2 01 07", "Silke", ""));
        assert_eq!(r.label(10210), "Silke");

        let change = r.observe(OPCODE_SELF_SPAWN, &with_name("cb 1c 5f 81 c1 28 37", "NeneneTowawa", ""));
        assert_eq!(change, Some(Change::Relogin { old_id: 3199, new_id: 3659 }));
        assert_eq!(r.label(10210), "#10210");
        assert_eq!(r.label(3659), "NeneneTowawa");
    }

    #[test]
    fn rejects_garbage_names() {
        assert_eq!(parse_named_spawn(&hex("01 00 00 00 00 00 03 41 00 42")), None);
        assert_eq!(parse_named_spawn(&hex("01 00 00 00 00 00 00")), None);
        assert_eq!(parse_named_spawn(&hex("01 00 00 00 00 00 05 41 42")), None);
    }
}
