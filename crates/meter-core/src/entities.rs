//! Who is who: your own character, nearby players, and mobs, keyed by entity ID.
//!
//! Fed with every message in order. Names arrive when an entity comes into view (or
//! after a zone change), so a label can start as a bare ID and gain a name later.
//! Layouts are in `docs/protocol.md`.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::protocol::read_varint;

pub const OPCODE_SELF_SPAWN: u16 = 0x3633;
pub const OPCODE_SELF_TICK: u16 = 0x364a;
pub const OPCODE_MOB_SPAWN: u16 = 0x3641;
pub const OPCODE_PLAYER_SPAWN: u16 = 0x3645;
/// A player engaging a mob: carries the player's entity ID and name. Fires on the first
/// hit of every real fight (not on training dummies), so it names you without a zone change.
pub const OPCODE_ENGAGE: u16 = 0x8d04;
/// Your party's full roster, resent whenever someone joins or leaves.
pub const OPCODE_PARTY_ROSTER: u16 = 0x9200;
/// You left the party (or it broke up).
pub const OPCODE_PARTY_LEFT: u16 = 0x9213;
/// A cross-server group's full roster (expeditions and other matched groups).
pub const OPCODE_GROUP_ROSTER: u16 = 0x9702;
/// One member joining that group.
pub const OPCODE_GROUP_JOIN: u16 = 0x970b;
/// You left the group.
pub const OPCODE_GROUP_LEFT: u16 = 0x971d;
/// One party or group member's level, gear score, legion and combat power (not sent for
/// you). Resent often.
pub const OPCODE_PARTY_MEMBER: u16 = 0x921c;
/// In a mob spawn, after the entity ID: an object that belongs to a player, followed by
/// that player's name (a Divine Barrier on a party member, for example).
const NAMED_OBJECT_MARK: [u8; 3] = [0x1c, 0x00, 0x01];
/// Levels, gear scores and combat power as rosters carry them; anything outside is a
/// different field.
const LEVELS: std::ops::RangeInclusive<u32> = 1..=99;
const GEAR_SCORES: std::ops::RangeInclusive<u32> = 100..=9_999;
const COMBAT_POWER: std::ops::RangeInclusive<u32> = 1_000..=10_000_000;

/// Bytes between the entity ID and the name length in player/self spawns.
const SPAWN_NAME_SKIP: usize = 5;
const MAX_NAME_BYTES: usize = 64;
/// Bytes between the entity ID and the type ID in mob spawns.
const MOB_TYPE_SKIP: usize = 3;
/// Mob type IDs seen so far are 7 digits. Anything far outside is a layout we don't know.
const MOB_TYPE_RANGE: std::ops::RangeInclusive<u32> = 100_000..=99_999_999;
/// Server IDs seen in rosters are 4 digits (1501..2507 so far).
const SERVER_RANGE: std::ops::RangeInclusive<u16> = 1000..=9999;
/// Character names in rosters, in bytes.
const ROSTER_NAME_BYTES: std::ops::RangeInclusive<usize> = 2..=36;
/// In mob spawns: eight `ff` bytes and a u64 time (2100-01-01 KST, "never"), followed by
/// the summoner's entity ID for Elementalist spirits and the mob's own ID otherwise.
const SUMMON_MARK: [u8; 16] = [
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x80, 0x75, 0xd5, 0x2a, 0xbb, 0x03, 0x00, 0x00,
];
/// Bytes after the varint following [`SUMMON_MARK`] in which a spirit's owner appears
/// again as a u32.
const SUMMON_OWNER_ECHO: usize = 24;
/// Other summons (Cleric, Sorcerer): u8, u8 `0x02`, 12 bytes (a position), then this tag
/// and the owner as a u32.
const SUMMON_OWNER_TAG: [u8; 3] = [0x07, 0x02, 0x06];
const SUMMON_OWNER_TAG_AT: usize = 14;
/// Skills a spirit only ever uses on its owner (each basic attack also lands on the owner
/// as 16990002/3, for example), so a spirit summoned before the meter started still finds
/// its owner. In our captures, every use by a spirit with a known owner hit that owner.
const SPIRIT_ECHO_SKILLS: [u32; 9] = [
    16_107_000, 16_257_000, 16_780_002, 16_780_003, 16_780_004, 16_780_005, 16_780_006, 16_990_002, 16_990_003,
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    /// Our own entity ID (and name, when it came from a full self spawn).
    SelfIdentified { id: u64, name: Option<String> },
    /// A new login: previous entity IDs no longer mean anything.
    Relogin { old_id: u64, new_id: u64 },
    PlayerNamed { id: u64, name: String },
    MobSeen { id: u64, type_id: u32 },
    /// A summoned creature (spirit, servant) and the player it belongs to.
    SummonSeen { id: u64, owner: u64 },
    /// Your party or group changed; `members` includes you (0 once you leave).
    PartyChanged { members: usize },
}

#[derive(Debug, Default, Clone)]
pub struct Registry {
    self_id: Option<u64>,
    self_name: Option<String>,
    players: HashMap<u64, String>,
    mobs: HashMap<u64, u32>,
    /// Summoned creatures and who summoned them.
    owners: HashMap<u64, u64>,
    /// Your party or group, you included, in roster order. Empty when you're on your own.
    party: Vec<PartyMember>,
    /// Objects that belong to a player (by name), such as a barrier on them.
    objects: HashMap<u64, String>,
}

/// Someone in your party or group, as the rosters and member updates describe them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PartyMember {
    /// A character ID that lasts across logins (not an entity ID).
    pub character: u32,
    pub name: String,
    pub server: u16,
    pub level: Option<u32>,
    pub gear_score: Option<u32>,
    pub combat_power: Option<u32>,
    pub legion: Option<String>,
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

    /// Your party or group (you included), in roster order.
    pub fn party(&self) -> &[PartyMember] {
        &self.party
    }

    /// You, or a player whose name is on your party's roster. Rosters carry names, not
    /// entity IDs, so a member counts once their name is known.
    pub fn in_party(&self, id: u64) -> bool {
        self.is_self(id) || self.player_name(id).is_some_and(|n| self.party.iter().any(|m| m.name == n))
    }

    /// The entity of the player (or you) with this name, if we've seen them.
    pub fn entity_named(&self, name: &str) -> Option<u64> {
        if self.self_name.as_deref() == Some(name) {
            return self.self_id;
        }
        self.players.iter().find(|(_, n)| n.as_str() == name).map(|(id, _)| *id)
    }

    /// For an object that belongs to a player (a barrier on them), that player's entity.
    pub fn object_player(&self, id: u64) -> Option<u64> {
        self.objects.get(&id).and_then(|name| self.entity_named(name))
    }

    pub fn mob_type(&self, id: u64) -> Option<u32> {
        self.mobs.get(&id).copied()
    }

    /// The player who summoned this creature, if it is one.
    pub fn owner_of(&self, id: u64) -> Option<u64> {
        self.owners.get(&id).copied()
    }

    /// Who a hit by `id` counts for: the summoner for a summoned creature, else `id`.
    pub fn credited(&self, id: u64) -> u64 {
        self.owner_of(id).unwrap_or(id)
    }

    /// Learns a spirit's owner from a hit with one of [`SPIRIT_ECHO_SKILLS`]. Only needed
    /// for spirits summoned before we saw their spawn.
    pub fn observe_hit(&mut self, attacker: u64, target: u64, skill: u32) -> Option<Change> {
        if !SPIRIT_ECHO_SKILLS.contains(&skill)
            || attacker == target
            || self.owners.contains_key(&attacker)
            || self.player_name(attacker).is_some()
            || self.is_self(attacker)
            || self.mobs.contains_key(&target)
            || self.owners.contains_key(&target)
        {
            return None;
        }
        self.owners.insert(attacker, target);
        Some(Change::SummonSeen { id: attacker, owner: target })
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

    /// Names a player from an earlier record, unless the game has already said who (or
    /// what) that entity is.
    pub fn remember(&mut self, id: u64, name: String) {
        if self.is_self(id) || self.mobs.contains_key(&id) {
            return;
        }
        self.players.entry(id).or_insert(name);
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
                self.owners.remove(&id);
                self.players.insert(id, name.clone());
                Some(Change::PlayerNamed { id, name })
            }
            OPCODE_ENGAGE => {
                let (id, name) = parse_engage(payload)?;
                if self.is_self(id) {
                    if self.self_name.as_deref() == Some(&name) {
                        return None;
                    }
                    self.self_name = Some(name.clone());
                    return Some(Change::SelfIdentified { id, name: Some(name) });
                }
                if self.players.get(&id) == Some(&name) {
                    return None;
                }
                self.mobs.remove(&id);
                self.owners.remove(&id);
                self.players.insert(id, name.clone());
                Some(Change::PlayerNamed { id, name })
            }
            OPCODE_PARTY_ROSTER | OPCODE_GROUP_ROSTER => {
                let mut members = parse_roster(payload);
                // A roster without us is someone else's group (a party-finder post).
                let ours = match &self.self_name {
                    Some(me) => members.iter().any(|m| &m.name == me),
                    None => !members.is_empty(),
                };
                if !ours {
                    return None;
                }
                // Keep what member updates told us that this roster doesn't say.
                for m in &mut members {
                    if let Some(old) = self.party.iter().find(|o| o.character == m.character) {
                        m.level = m.level.or(old.level);
                        m.gear_score = m.gear_score.or(old.gear_score);
                        m.combat_power = m.combat_power.or(old.combat_power);
                        m.legion = m.legion.take().or_else(|| old.legion.clone());
                    }
                }
                if members == self.party {
                    return None;
                }
                let names_changed = members.iter().map(|m| &m.name).ne(self.party.iter().map(|m| &m.name));
                self.party = members;
                names_changed.then_some(Change::PartyChanged { members: self.party.len() })
            }
            OPCODE_GROUP_JOIN => {
                let before = self.party.len();
                for m in parse_roster(payload) {
                    if !self.party.iter().any(|o| o.character == m.character) {
                        self.party.push(m);
                    }
                }
                (self.party.len() != before).then_some(Change::PartyChanged { members: self.party.len() })
            }
            OPCODE_PARTY_MEMBER => {
                let u = parse_member_update(payload)?;
                let m = self.party.iter_mut().find(|m| m.character == u.character)?;
                m.level = u.level.or(m.level);
                m.gear_score = u.gear_score.or(m.gear_score);
                m.combat_power = u.combat_power.or(m.combat_power);
                m.legion = u.legion.or_else(|| m.legion.take());
                None
            }
            OPCODE_PARTY_LEFT | OPCODE_GROUP_LEFT => {
                if self.party.is_empty() {
                    return None;
                }
                self.party.clear();
                Some(Change::PartyChanged { members: 0 })
            }
            OPCODE_MOB_SPAWN if parse_named_object(payload).is_some() => {
                let (id, name) = parse_named_object(payload)?;
                self.mobs.remove(&id);
                self.owners.remove(&id);
                self.players.remove(&id);
                self.objects.insert(id, name);
                None
            }
            OPCODE_MOB_SPAWN => {
                // Summons with a named layout have no type we can read, but still an owner.
                let summon = parse_summon_owner(payload);
                let mob = parse_mob_spawn(payload);
                let id = summon.map(|(id, _)| id).or(mob.map(|(id, _)| id))?;
                self.players.remove(&id);
                self.objects.remove(&id);
                let owner_changed = match summon {
                    Some((_, owner)) => self.owners.insert(id, owner) != Some(owner),
                    // An entity ID can be reused by something that isn't a summon.
                    None => self.owners.remove(&id).is_some(),
                };
                let type_changed = mob.is_some_and(|(_, t)| self.mobs.insert(id, t) != Some(t));
                match (summon, mob) {
                    (Some((id, owner)), _) if owner_changed => Some(Change::SummonSeen { id, owner }),
                    (_, Some((id, type_id))) if type_changed => Some(Change::MobSeen { id, type_id }),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    fn set_self(&mut self, id: u64) -> Option<Change> {
        match self.self_id {
            Some(old) if old == id => None,
            Some(old) => {
                // Entity IDs are per login, so everything we knew is stale. The party
                // isn't keyed by entity ID, so it carries over.
                let party = std::mem::take(&mut self.party);
                *self = Registry::default();
                self.self_id = Some(id);
                self.party = party;
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

/// `0x8d04`: varint mob, u32 skill, varint player, varint server, u8 length, UTF-8 name.
pub fn parse_engage(p: &[u8]) -> Option<(u64, String)> {
    let mut i = 0;
    read_varint(p, &mut i)?; // the mob
    i += 4; // skill used
    let id = read_varint(p, &mut i)?;
    read_varint(p, &mut i)?; // server
    let len = *p.get(i)? as usize;
    i += 1;
    if id == 0 || len == 0 || len > MAX_NAME_BYTES {
        return None;
    }
    let name = std::str::from_utf8(p.get(i..i + len)?).ok()?;
    if name.chars().any(char::is_control) {
        return None;
    }
    Some((id, name.to_string()))
}

/// Rosters (`0x9200`, `0x9702`, `0x970b`): each member is a u32 character ID, two zero
/// bytes, a u16 server ID, a u8 length and the UTF-8 name, among other fields we don't
/// use. Entries aren't at fixed offsets, so this scans for that shape.
pub fn parse_roster(p: &[u8]) -> Vec<PartyMember> {
    // Where each entry starts and its name ends; an entry's other fields run to the next.
    let mut found: Vec<(usize, String, usize)> = Vec::new();
    let mut i = 0;
    while i < p.len() {
        match roster_entry(p, i) {
            Some((name, end)) => {
                found.push((i, name, end));
                i = end;
            }
            None => i += 1,
        }
    }
    let mut members: Vec<PartyMember> = Vec::new();
    for (k, (start, name, end)) in found.iter().enumerate() {
        if members.iter().any(|m| &m.name == name) {
            continue;
        }
        let next = found.get(k + 1).map_or(p.len(), |f| f.0);
        let server = u16::from_le_bytes([p[start + 6], p[start + 7]]);
        let (level, gear_score, combat_power) = roster_stats(&p[*end..next], server);
        members.push(PartyMember {
            character: u32_at(p, *start).expect("an entry has 9 bytes before its name"),
            name: name.clone(),
            server,
            level,
            gear_score,
            combat_power,
            legion: None,
        });
    }
    members
}

fn u32_at(p: &[u8], i: usize) -> Option<u32> {
    Some(u32::from_le_bytes(p.get(i..i + 4)?.try_into().ok()?))
}

/// Level, gear score and combat power from the bytes after a roster entry's name. Group
/// rosters (`0x9702`): u32, level, gear score, the server again (sometimes a byte later),
/// three bytes, combat power. Party rosters (`0x9200`) put a varying amount first, then
/// level, a zero, gear score and combat power in a row.
fn roster_stats(body: &[u8], server: u16) -> (Option<u32>, Option<u32>, Option<u32>) {
    let (level, gear) = (u32_at(body, 4), u32_at(body, 8));
    if level.is_some_and(|l| LEVELS.contains(&l)) && gear.is_some_and(|g| GEAR_SCORES.contains(&g)) {
        let echo = server.to_le_bytes();
        if let Some(at) = (12..=13).find(|&j| body.get(j..j + 2) == Some(&echo[..])) {
            if let Some(cp) = u32_at(body, at + 5).filter(|c| COMBAT_POWER.contains(c)) {
                return (level, gear, Some(cp));
            }
        }
    }
    for i in 0..body.len().saturating_sub(15) {
        let [l, z, g, c] = [i, i + 4, i + 8, i + 12].map(|j| u32_at(body, j).unwrap_or(u32::MAX));
        if LEVELS.contains(&l) && z == 0 && GEAR_SCORES.contains(&g) && COMBAT_POWER.contains(&c) {
            return (Some(l), Some(g), Some(c));
        }
    }
    (None, None, None)
}

/// `0x921c`: u32 character ID, 2 zero bytes, u16 server, u32, 12 bytes, 12 bytes (a
/// position), u32 level, u32, u32 gear score, u8 length + legion name, u32, u8, u32
/// combat power.
pub fn parse_member_update(p: &[u8]) -> Option<PartyMember> {
    let level = u32_at(p, 36).filter(|l| LEVELS.contains(l))?;
    let gear_score = u32_at(p, 44).filter(|g| GEAR_SCORES.contains(g));
    let len = *p.get(48)? as usize;
    let legion = std::str::from_utf8(p.get(49..49 + len)?)
        .ok()
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let combat_power = u32_at(p, 49 + len + 5).filter(|c| COMBAT_POWER.contains(c));
    Some(PartyMember {
        character: u32_at(p, 0)?,
        name: String::new(),
        server: u16::from_le_bytes(p.get(6..8)?.try_into().ok()?),
        level: Some(level),
        gear_score,
        combat_power,
        legion,
    })
}

/// A mob spawn for an object that belongs to a player: varint entity ID,
/// [`NAMED_OBJECT_MARK`], u8 length and the player's name.
pub fn parse_named_object(p: &[u8]) -> Option<(u64, String)> {
    let mut i = 0;
    let id = read_varint(p, &mut i)?;
    if p.get(i..i + 3)? != NAMED_OBJECT_MARK {
        return None;
    }
    let len = *p.get(i + 3)? as usize;
    if !ROSTER_NAME_BYTES.contains(&len) {
        return None;
    }
    let name = std::str::from_utf8(p.get(i + 4..i + 4 + len)?).ok()?;
    name.chars().all(char::is_alphanumeric).then(|| (id, name.to_string()))
}

/// A roster entry starting at `i`: the name and where it ends.
fn roster_entry(p: &[u8], i: usize) -> Option<(String, usize)> {
    let head = p.get(i..i + 9)?;
    if head[4..6] != [0, 0] {
        return None;
    }
    let server = u16::from_le_bytes([head[6], head[7]]);
    let len = head[8] as usize;
    if !SERVER_RANGE.contains(&server) || !ROSTER_NAME_BYTES.contains(&len) {
        return None;
    }
    let name = std::str::from_utf8(p.get(i + 9..i + 9 + len)?).ok()?;
    name.chars()
        .all(char::is_alphanumeric)
        .then(|| (name.to_string(), i + 9 + len))
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

/// `0x3641` of a summoned creature: (its entity ID, its summoner's). Two layouts:
///
/// - Elementalist spirits: [`SUMMON_MARK`] is followed by the owner's varint entity ID
///   (other mobs have their own ID there), and the same ID comes again as a u32 a few
///   bytes later.
/// - Other summons: their own ID after the mark, then u8, u8 `0x02`, 12 bytes,
///   `07 02 06` and the owner as a u32.
pub fn parse_summon_owner(p: &[u8]) -> Option<(u64, u64)> {
    let mut i = 0;
    let id = read_varint(p, &mut i)?;
    let mut j = p.windows(SUMMON_MARK.len()).position(|w| w == SUMMON_MARK)? + SUMMON_MARK.len();
    let who = read_varint(p, &mut j)?;
    if who != id {
        let echo = u32::try_from(who).ok()?.to_le_bytes();
        let after = p.get(j..p.len().min(j + SUMMON_OWNER_ECHO))?;
        return after.windows(4).any(|w| w == echo).then_some((id, who));
    }
    let tag = p.get(j + SUMMON_OWNER_TAG_AT..j + SUMMON_OWNER_TAG_AT + 7)?;
    if *p.get(j + 1)? != 0x02 || tag[..3] != SUMMON_OWNER_TAG {
        return None;
    }
    let owner = u64::from(u32::from_le_bytes(tag[3..7].try_into().ok()?));
    (owner != 0 && owner != id).then_some((id, owner))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::protocol::tests::encode_varint;

    pub(crate) fn hex(s: &str) -> Vec<u8> {
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

    // From the Vakron Sky Island capture (2026-10-22): a party of five in an expedition.
    const VAKRON_ROSTER: &str = "e6 76 04 00 06 38 30 6b 20 33 62 05 08 28 09 00 00 03 80 62 01 00 00 00 e5 05 ff 01 02 05 1e 01 80 62 01 00 00 00 e5 05 03 41 6a 61 09 00 00 00 2d 00 00 00 63 06 00 00 e5 05 9e 11 04 52 3e 01 00 00 00 00 00 00 ad 00 00 00 00 00 00 00 01 00 1e 02 a7 de 03 00 00 00 df 05 06 72 6f 62 69 30 33 1d 00 00 00 2d 00 00 00 10 07 00 00 df 05 9e 11 04 73 47 01 00 00 00 00 00 00 3b 00 00 00 00 00 00 00 01 00 1e 03 58 de 03 00 00 00 c7 09 0c 4e 65 6e 65 6e 65 54 6f 77 61 77 61 24 00 00 00 2d 00 00 00 e7 07 00 00 c7 09 9e 11 04 d2 58 01 00 00 00 00 00 00 8f 00 00 00 00 00 00 00 01 00 1e 04 0f 1a 04 00 00 00 e4 05 04 54 41 51 45 0e 00 00 00 2d 00 00 00 c3 05 00 00 07 e4 05 9e 11 04 f6 f9 00 00 00 00 00 00 00 59 00 00 00 00 00 00 00 01 00 1e 05 5b 3c 03 00 00 00 e1 05 07 43 68 61 6e 64 72 61 22 00 00 00 2d 00 00 00 57 06 00 00 e1 05 9e 11 04 1a 39 01 00 00 00 00 00 00 6d 00 00 00 00 00 00 00 01 00 00 07";
    const ROBI_UPDATE: &str = "a7 de 03 00 00 00 df 05 1d 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 ac 4a 15 47 ba ee 62 47 00 68 21 47 2d 00 00 00 00 00 00 00 10 07 00 00 00 00 00 00 00 01 73 47 01 00 00 00 00 00 00";
    const CHANDRA_UPDATE: &str = "5b 3c 03 00 00 00 e1 05 22 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 ac 4a 15 47 ba ee 62 47 00 68 21 47 2d 00 00 00 00 00 00 00 57 06 00 00 0c 45 74 65 72 6e 61 6c 57 69 6e 47 73 01 00 00 00 01 1a 39 01 00 00 00 00 00 00";
    const AJA_BARRIER: &str = "b3 d1 05 1c 00 01 03 41 6a 61 ac 8f 2c 00 00 02 39 ba 80 46 f5 af 7f 46 00 52 1c 47";

    #[test]
    fn group_rosters_carry_gear_score_and_combat_power() {
        let m = parse_roster(&hex(VAKRON_ROSTER));
        let got: Vec<_> = m.iter().map(|m| (m.name.as_str(), m.level, m.gear_score, m.combat_power)).collect();
        assert_eq!(
            got,
            vec![
                ("Aja", Some(45), Some(1635), Some(81_490)),
                ("robi03", Some(45), Some(1808), Some(83_827)),
                // In game: gear score 2,023 and combat power 88.27K.
                ("NeneneTowawa", Some(45), Some(2023), Some(88_274)),
                // The server comes a byte later in this entry.
                ("TAQE", Some(45), Some(1475), Some(63_990)),
                ("Chandra", Some(45), Some(1623), Some(80_154)),
            ]
        );
        assert_eq!((m[2].character, m[2].server), (253_528, 2503));
    }

    #[test]
    fn member_updates_fill_in_the_party() {
        let u = parse_member_update(&hex(ROBI_UPDATE)).unwrap();
        assert_eq!((u.character, u.level, u.gear_score, u.combat_power, u.legion), (253_607, Some(45), Some(1808), Some(83_827), None));
        let c = parse_member_update(&hex(CHANDRA_UPDATE)).unwrap();
        assert_eq!((c.gear_score, c.combat_power, c.legion.as_deref()), (Some(1623), Some(80_154), Some("EternalWinGs")));

        let mut r = Registry::default();
        r.observe(OPCODE_SELF_SPAWN, &with_name("cb 1c 5f 81 c1 28 37", "NeneneTowawa", ""));
        assert!(r.observe(OPCODE_GROUP_ROSTER, &hex(VAKRON_ROSTER)).is_some());
        assert_eq!(r.party().len(), 5);
        r.observe(OPCODE_PARTY_MEMBER, &hex(CHANDRA_UPDATE));
        let chandra = r.party().iter().find(|m| m.name == "Chandra").unwrap();
        assert_eq!(chandra.legion.as_deref(), Some("EternalWinGs"));
        // The roster again (resent often) keeps the legion it doesn't carry.
        assert!(r.observe(OPCODE_GROUP_ROSTER, &hex(VAKRON_ROSTER)).is_none());
        assert_eq!(r.party()[4].legion.as_deref(), Some("EternalWinGs"));
    }

    #[test]
    fn a_barrier_belongs_to_the_player_it_protects() {
        let mut r = Registry::default();
        r.observe(OPCODE_PLAYER_SPAWN, &with_name("8b 26 01 20 a2 01 07", "Aja", ""));
        assert_eq!(parse_named_object(&hex(AJA_BARRIER)), Some((92_339, "Aja".to_string())));
        r.observe(OPCODE_MOB_SPAWN, &hex(AJA_BARRIER));
        assert_eq!(r.object_player(92_339), r.entity_named("Aja"));
        assert!(r.object_player(92_339).is_some());
        assert_eq!(r.mob_type(92_339), None, "not a mob");
    }

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
    fn engage_names_us_and_others() {
        // From the zone-change capture: mob 19776 engaged by us (3199) with 18010340.
        let mut p = hex("c0 9a 01 e4 d0 12 01 ff 18 c7 09");
        p.push(12);
        p.extend_from_slice(b"NeneneTowawa");
        p.extend(hex("00 00"));
        assert_eq!(parse_engage(&p), Some((3199, "NeneneTowawa".into())));

        let mut r = Registry::default();
        r.observe(OPCODE_SELF_TICK, &hex("ff 18 00"));
        assert_eq!(r.label(3199), "You (#3199)");
        r.observe(OPCODE_ENGAGE, &p);
        assert_eq!(r.label(3199), "NeneneTowawa");
        assert_eq!(r.self_name(), Some("NeneneTowawa"));
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

    /// A roster entry as the game writes it: character ID, 00 00, server, name.
    fn member(char_id: u32, server: u16, name: &str) -> Vec<u8> {
        let mut p = char_id.to_le_bytes().to_vec();
        p.extend([0, 0]);
        p.extend(server.to_le_bytes());
        p.push(name.len() as u8);
        p.extend_from_slice(name.as_bytes());
        // Level, class and other fields follow.
        p.extend(hex("24 00 00 00 2d 00 00 00 e7 07 00 00"));
        p
    }

    fn roster(members: &[(u32, u16, &str)]) -> Vec<u8> {
        let mut p = hex("08 50 07 14 00 40 d3 24 9a 05 00 00 00 02 01");
        for (id, server, name) in members {
            p.extend(hex("c7 09 c7 09 24"));
            p.extend(member(*id, *server, name));
        }
        p
    }

    #[test]
    fn parses_roster_names() {
        let p = roster(&[(173846, 2503, "\u{7c92}\u{5bfb}\u{6b22}"), (182272, 2503, "Pingu"), (253528, 2503, "NeneneTowawa")]);
        let names: Vec<String> = parse_roster(&p).into_iter().map(|m| m.name).collect();
        assert_eq!(names, ["\u{7c92}\u{5bfb}\u{6b22}", "Pingu", "NeneneTowawa"]);
        assert!(parse_roster(&hex("00 00 00 00 00 00 00 00 00 00")).is_empty());
    }

    #[test]
    fn tracks_party_membership() {
        let mut r = Registry::default();
        r.observe(OPCODE_SELF_SPAWN, &with_name("cb 1c 5f 81 c1 28 37", "NeneneTowawa", ""));
        r.observe(OPCODE_PLAYER_SPAWN, &with_name("e2 4f 01 20 a2 01 07", "Pingu", ""));
        r.observe(OPCODE_PLAYER_SPAWN, &with_name("e3 4f 01 20 a2 01 07", "Stranger", ""));
        let p = roster(&[(182272, 2503, "Pingu"), (253528, 2503, "NeneneTowawa")]);
        assert_eq!(r.observe(OPCODE_PARTY_ROSTER, &p), Some(Change::PartyChanged { members: 2 }));
        assert!(r.in_party(3659));
        assert!(r.in_party(10210));
        assert!(!r.in_party(10211));
        // Someone else's group (without us) is ignored.
        let other = roster(&[(1, 2507, "Latifa"), (2, 2507, "Kyle")]);
        assert_eq!(r.observe(OPCODE_GROUP_ROSTER, &other), None);
        // A member joins, then we leave.
        assert_eq!(
            r.observe(OPCODE_GROUP_JOIN, &member(3, 2504, "Stranger")),
            Some(Change::PartyChanged { members: 3 })
        );
        assert!(r.in_party(10211));
        assert_eq!(r.observe(OPCODE_PARTY_LEFT, &hex("00 00")), Some(Change::PartyChanged { members: 0 }));
        assert!(!r.in_party(10210));
        assert!(r.in_party(3659));
    }

    /// Spirit 21466 spawning for player 1802 (`20261020-185747`, 32.3 s).
    pub(crate) const SPIRIT_SPAWN: &str = "da a7 01 1f 10 00 c7 8e 2c 00 40 02 80 c2 99 c7 40 08 05 c8 00 1f 0e 47         45 04 18 43 1a 6c 01 d0 6f d0 6f 6f 0c 00 00 6f 0c 00 00 00 00 00 00 00 00 00 00 00 00 00 00 d4 3d 02 00         64 00 00 00 f0 49 02 00 01 00 00 00 00 00 00 00 a0 86 01 00 00 00 00 00 87 69 09 00 01 02 01 11 01 ac 31         9a 09 ff ff ff ff ff ff ff ff 80 75 d5 2a bb 03 00 00 8a 0e 0f 02 60 49 99 c7 43 f8 04 c8 00 e6 0d 47 07         02 06 0a 07 00 00 02 cd 00 fa 05 00 00 d0 00 73 01 00 00 2d 00 00 00 e5 1d 03 00 00";

    /// Morzetic's (14637) Cleric servant 40634, named layout (`20261020-184817`, 0 s).
    const SERVANT_SPAWN: &str = "ba bd 02 5f 00 01 08 4d 6f 72 7a 65 74 69 63 ca 90 2c 00 40 02 80 96 98 c7         40 3a 05 c8 00 e6 0d 47 66 8d 13 43 ed 68 01 9e 96 01 9e 96 01 8f 12 00 00 8f 12 00 00 00 00 00 00 00 00         00 00 00 00 00 00 d4 3d 02 00 64 00 00 00 f0 49 02 00 01 00 00 00 00 00 00 00 a0 86 01 00 00 00 00 00 a0         9b 0a 00 01 02 01 11 01 81 96 98 00 ff ff ff ff ff ff ff ff 80 75 d5 2a bb 03 00 00 ba bd 02 01 02 80 96         98 c7 40 3a 05 c8 00 e6 0d 47 07 02 06 2d 39 00 00 38 00 00 00 00 00 c7 09 09 42 4f 52 41 50 41 4e 44 41         02 00 02 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 02 cd 00 ff 07 00 00 d0 00 6d 01 00 00 2d 00 00 00         00";

    /// An ordinary mob, 35692 of type 2340352 (`20261018-183523`): its own ID after the mark.
    const MOB_SPAWN: &str = "ec 96 02 04 20 00 00 b6 23 00 00 02 cb 18 d0 43 79 8d de c4 00 30 52 46 00 10         ef 42 00 55 01 af 24 af 24 64 00 00 00 64 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 64 00         00 00 64 00 00 00 01 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 06 01 11 01 81 96 98 00 ff         ff ff ff ff ff ff ff 80 75 d5 2a bb 03 00 00 ec 96 02 01 00 cb 18 d0 43 79 8d de c4 00 30 52 46 06 00 14         00 00 00 ab f2 e5 6b 00";

    #[test]
    fn spirit_spawn_names_its_owner() {
        let p = hex(SPIRIT_SPAWN);
        assert_eq!(parse_summon_owner(&p), Some((21466, 1802)));
        let mut r = Registry::default();
        assert_eq!(r.observe(OPCODE_MOB_SPAWN, &p), Some(Change::SummonSeen { id: 21466, owner: 1802 }));
        assert_eq!(r.owner_of(21466), Some(1802));
        assert_eq!(r.credited(21466), 1802);
        assert_eq!(r.credited(1802), 1802);
        assert_eq!(r.mob_type(21466), Some(2_920_135));
        // The same spawn again changes nothing.
        assert_eq!(r.observe(OPCODE_MOB_SPAWN, &p), None);
    }

    #[test]
    fn other_summons_name_their_owner_after_the_position() {
        let p = hex(SERVANT_SPAWN);
        assert_eq!(parse_summon_owner(&p), Some((40634, 14637)));
        let mut r = Registry::default();
        r.observe(OPCODE_MOB_SPAWN, &p);
        assert_eq!(r.owner_of(40634), Some(14637));
        // A named layout: no type we can read.
        assert_eq!(r.mob_type(40634), None);
    }

    #[test]
    fn ordinary_mobs_have_no_owner() {
        let p = hex(MOB_SPAWN);
        assert_eq!(parse_summon_owner(&p), None);
        assert_eq!(parse_mob_spawn(&p), Some((35692, 2_340_352)));
        // A spirit's ID taken over by a mob loses its owner.
        let mut spirit = hex(SPIRIT_SPAWN);
        spirit.splice(0..3, encode_varint(35692));
        let mut r = Registry::default();
        r.observe(OPCODE_MOB_SPAWN, &spirit);
        assert_eq!(r.owner_of(35692), Some(1802));
        r.observe(OPCODE_MOB_SPAWN, &p);
        assert_eq!(r.owner_of(35692), None);
        // Truncated before the owner's echo: not trusted.
        assert_eq!(parse_summon_owner(&hex(SPIRIT_SPAWN)[..128]), None);
    }

    #[test]
    fn spirit_hits_on_its_owner_name_the_owner() {
        // 20261020-184817: spirit 41727 (summoned before the capture) hits 55 with 16990002
        // after each basic attack.
        let mut r = Registry::default();
        assert_eq!(r.observe_hit(41727, 20644, 100_028), None);
        assert_eq!(r.observe_hit(41727, 41727, 16_990_002), None);
        assert_eq!(r.observe_hit(41727, 55, 16_990_002), Some(Change::SummonSeen { id: 41727, owner: 55 }));
        assert_eq!(r.observe_hit(41727, 55, 16_990_002), None);
        assert_eq!(r.credited(41727), 55);
        // Never for named players, or onto mobs.
        r.observe(OPCODE_PLAYER_SPAWN, &with_name("fd 63 15 b0 a4 00 07", "Player", ""));
        assert_eq!(r.observe_hit(12797, 55, 16_107_000), None);
        r.observe(OPCODE_MOB_SPAWN, &hex(MOB_SPAWN));
        assert_eq!(r.observe_hit(9, 35692, 16_107_000), None);
    }
}
