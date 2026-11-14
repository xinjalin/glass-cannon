//! Who is who: your own character, nearby players, and mobs, keyed by entity ID.
//!
//! Fed with every message in order. Names arrive when an entity comes into view (or
//! after a zone change), so a label can start as a bare ID and gain a name later.
//! Layouts are in `docs/protocol.md`.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::hp::{parse_party_vitals, OPCODE_PARTY_VITALS};
use crate::protocol::read_varint;

pub const OPCODE_SELF_SPAWN: u16 = 0x3633;
/// Your character's stats (varint entity, a list of stats, max HP); names your entity.
pub const OPCODE_SELF_STATS: u16 = 0x364a;
pub const OPCODE_MOB_SPAWN: u16 = 0x3641;
pub const OPCODE_PLAYER_SPAWN: u16 = 0x3645;
/// Something died: the entity, the skill and who did it (a player's entity, server and
/// name, or a mob's type). A kill by you names you without a zone change. See [`Death`].
pub const OPCODE_DEATH: u16 = 0x8d04;
/// Your party's full roster, resent whenever someone joins or leaves.
pub const OPCODE_PARTY_ROSTER: u16 = 0x9200;
/// You left the party (or it broke up).
pub const OPCODE_PARTY_LEFT: u16 = 0x9213;
/// A cross-server group's full roster (expeditions and other matched groups).
pub const OPCODE_GROUP_ROSTER: u16 = 0x9702;
/// One member joining that group.
pub const OPCODE_GROUP_JOIN: u16 = 0x970b;
/// Someone joined your party, or a member came back online: one party roster entry
/// (Questlog's PartyAddMember and PartyReconnectMember). In the capture of 2026-10-24 two
/// joined a party this way before a dungeon, after its roster.
pub const OPCODE_PARTY_ADD_MEMBER: u16 = 0x920d;
pub const OPCODE_PARTY_RECONNECT: u16 = 0x9222;
/// You left the group.
pub const OPCODE_GROUP_LEFT: u16 = 0x971d;
/// A member left the party: u32 character ID, 2 zero bytes, u16 server. Taken to mean
/// you're out when it carries your own ID (being kicked; not seen in a capture yet).
pub const OPCODE_PARTY_MEMBER_LEFT: u16 = 0x9221;
/// A member left the group (same layout as [`OPCODE_PARTY_MEMBER_LEFT`]).
pub const OPCODE_GROUP_MEMBER_LEFT: u16 = 0x971e;
/// A group member readied up or stopped: u32 character ID, 2 zero bytes, u16 server, u8
/// ready (1 or 0).
pub const OPCODE_GROUP_READY: u16 = 0x9716;
/// You readied up or stopped: `00 00`, u8 ready.
pub const OPCODE_GROUP_SELF_READY: u16 = 0x9715;
/// The party's leader changed: u32 character ID, 2 zero bytes, u16 server.
pub const OPCODE_PARTY_LEADER: u16 = 0x9211;
/// You were kicked from the party (u8: by vote).
pub const OPCODE_PARTY_KICKED: u16 = 0x9219;
/// The party broke up (u8 reason).
pub const OPCODE_PARTY_DISBANDED: u16 = 0x921f;
/// You left the party (u64 party, u8 reason): seen leaving after an expedition's run.
pub const OPCODE_PARTY_LEFT_NOTICE: u16 = 0x931f;
/// The party finder group removed you (u8 reason).
pub const OPCODE_GROUP_KICKED: u16 = 0x972a;
/// The Force (a 20-player raid group) broke up, or you left it.
pub const OPCODE_FORCE_DISBANDED: u16 = 0x961a;
/// The Force's members (a 20-player raid group: several parties): its full roster, a
/// member added, one member, the member list, a member back online. Each member is in
/// them as a party roster's are: u32 character ID, `00 00`, u16 server, u8 length, name
/// (a 20-member roster in the capture of 2026-10-23).
pub const OPCODE_FORCE_INFO: u16 = 0x9602;
pub const OPCODE_FORCE_ADD_MEMBER: u16 = 0x960a;
pub const OPCODE_FORCE_RECONNECT: u16 = 0x961b;
pub const OPCODE_FORCE_MEMBER: u16 = 0x961d;
pub const OPCODE_FORCE_MEMBER_LIST: u16 = 0x961e;
/// A member left the Force: u64 member (u32 character ID, `00 00`, u16 server), u64 party,
/// u8 reason.
pub const OPCODE_FORCE_REMOVE_MEMBER: u16 = 0x9622;
pub const OPCODE_FORCE_LEFT: u16 = 0x962e;
/// One party or group member's level, gear score, legion and combat power (not sent for
/// you). Resent often.
pub const OPCODE_PARTY_MEMBER: u16 = 0x921c;
/// In a mob spawn, after the entity ID: a mob made in a party member's likeness (bosses
/// such as Tiere and Thamon copy each member), followed by that member's name.
const PLAYER_COPY_MARK: [u8; 3] = [0x1c, 0x00, 0x01];
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
    /// Your Force's members (character ID -> name); empty when you're not in one.
    force: HashMap<u32, String>,
    /// Players the game sends party HP for (`0x921b`, your party members alone): who's in
    /// your party when their name or the roster isn't known (they were sent before the
    /// meter started). Forgotten when the party changes.
    party_ids: HashSet<u64>,
    /// Mobs whose spawn we missed (it came before the meter started) that have a stagger
    /// gauge: bosses, as far as we can tell (a raid boss stays up through every wipe, so
    /// it may never spawn again).
    unseen_bosses: HashSet<u64>,
    /// Combat power by entity, as players' spawns (and yours) give it.
    combat_power: HashMap<u64, u32>,
    /// Other players' class and rank, as their spawns give them.
    spawn_info: HashMap<u64, SpawnInfo>,
    /// In a dungeon (its state sent since the map last changed): a party-finder group's
    /// ready marks are over then.
    in_dungeon: bool,
}

/// What a player's spawn says of them, besides their name and combat power.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SpawnInfo {
    /// Their class, as the meter's class codes (see [`crate::classes`]).
    pub class_code: Option<u32>,
    /// Their rank: 1 the highest, 19 the lowest (see [`parse_spawn_rank`]).
    pub rank: Option<u32>,
}

/// Someone leaving the party: what the HP-sent members are forgotten on even when the
/// roster isn't known.
const PARTY_EXITS: [u16; 10] = [
    OPCODE_PARTY_MEMBER_LEFT,
    OPCODE_GROUP_MEMBER_LEFT,
    OPCODE_PARTY_LEFT,
    OPCODE_GROUP_LEFT,
    OPCODE_PARTY_KICKED,
    OPCODE_PARTY_DISBANDED,
    OPCODE_PARTY_LEFT_NOTICE,
    OPCODE_GROUP_KICKED,
    OPCODE_FORCE_DISBANDED,
    OPCODE_FORCE_LEFT,
];

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
    /// In a party-finder group that hasn't started: whether they've readied up. None
    /// outside such a group, once inside, and for the group's leader (who starts it).
    #[serde(default)]
    pub ready: Option<bool>,
    /// Leads the party or group (the first slot of a group, or named by a leader change).
    #[serde(default)]
    pub leader: bool,
    /// Their place in a party-finder group (1 the leader's): a Sanctuary's room of ten has
    /// two parties, slots 1 to 5 and 6 to 10. None outside such a group.
    #[serde(default)]
    pub slot: Option<u8>,
    /// Their class, as a group's roster says it (see [`class_of_byte`]); the meter's party
    /// view says it as `class_code`, this or what their skills say.
    #[serde(default, skip_serializing)]
    pub roster_class: Option<u32>,
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

    /// You, or a player in your party or Force: their name is on its roster (rosters
    /// carry names, not entity IDs), or, when their name or a whole roster isn't known, the
    /// game sends their party HP. A roster without you is only part of one (a member
    /// coming back online, before the full roster).
    pub fn in_party(&self, id: u64) -> bool {
        if self.is_self(id) {
            return true;
        }
        let whole = self.self_name.as_deref().is_some_and(|me| self.party.iter().any(|m| m.name == me));
        match self.player_name(id) {
            Some(n) if whole || !self.force.is_empty() => {
                self.party.iter().any(|m| m.name == n) || self.force.values().any(|f| f == n)
            }
            _ => self.party_ids.contains(&id),
        }
    }

    /// You're in a Force.
    pub fn in_force(&self) -> bool {
        !self.force.is_empty()
    }

    /// How many are in your Force (0: none).
    pub fn force_size(&self) -> usize {
        self.force.len()
    }

    /// The entity of the player (or you) with this name, if we've seen them.
    pub fn entity_named(&self, name: &str) -> Option<u64> {
        if self.self_name.as_deref() == Some(name) {
            return self.self_id;
        }
        self.players.iter().find(|(_, n)| n.as_str() == name).map(|(id, _)| *id)
    }

    /// A player's combat power (or yours), from their spawn.
    pub fn combat_power(&self, id: u64) -> Option<u32> {
        self.combat_power.get(&id).copied()
    }

    /// Another player's class and rank, from their spawn.
    pub fn spawn_info(&self, id: u64) -> SpawnInfo {
        self.spawn_info.get(&id).copied().unwrap_or_default()
    }

    pub fn mob_type(&self, id: u64) -> Option<u32> {
        self.mobs.get(&id).copied()
    }

    /// A boss we never saw spawn (so its type isn't known): a mob with a stagger gauge.
    /// Only bosses have one (Deus 2026-10-24, the raid 2026-11-06).
    pub fn unseen_boss(&self, id: u64) -> bool {
        self.unseen_bosses.contains(&id) && !self.mobs.contains_key(&id) && !self.players.contains_key(&id)
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

    /// A display name: your name, a player's name, "Mob <type>", "Boss" (one we never saw
    /// spawn), or "#<id>".
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
        if self.unseen_boss(id) {
            return "Boss".to_string();
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
        // A new map; a dungeon's state comes after its map (a town's never does). In one, a
        // group has started: nobody's readying up.
        match opcode {
            crate::dungeon::OPCODE_MAP_ENTERED => self.in_dungeon = false,
            crate::dungeon::OPCODE_DUNGEON_STEP => {
                self.in_dungeon = true;
                for m in &mut self.party {
                    m.ready = None;
                }
            }
            _ => {}
        }
        let change = self.observe_message(opcode, payload);
        // Joins only add to the party; anything else that changes it may have dropped someone.
        let join = matches!(opcode, OPCODE_GROUP_JOIN | OPCODE_PARTY_ADD_MEMBER | OPCODE_PARTY_RECONNECT);
        if (matches!(change, Some(Change::PartyChanged { .. })) && !join) || PARTY_EXITS.contains(&opcode) {
            self.party_ids.clear();
        }
        change
    }

    fn observe_message(&mut self, opcode: u16, payload: &[u8]) -> Option<Change> {
        match opcode {
            crate::state::OPCODE_STAGGER => {
                let (mob, _) = crate::state::parse_stagger(payload)?;
                if !self.is_self(mob) && !self.mobs.contains_key(&mob) && !self.players.contains_key(&mob) {
                    self.unseen_bosses.insert(mob);
                }
                None
            }
            OPCODE_PARTY_VITALS => {
                let (id, _) = parse_party_vitals(payload)?;
                if !self.is_self(id) && !self.mobs.contains_key(&id) {
                    self.party_ids.insert(id);
                }
                None
            }
            OPCODE_SELF_SPAWN => {
                let (id, name) = parse_named_spawn(payload)?;
                let cp = parse_self_combat_power(payload);
                let change = self.set_self(id);
                if let Some(cp) = cp {
                    self.combat_power.insert(id, cp);
                }
                if let Some(change) = change {
                    self.self_name = Some(name);
                    return Some(change);
                }
                let renamed = self.self_name.as_deref() != Some(&name);
                self.self_name = Some(name.clone());
                renamed.then_some(Change::SelfIdentified { id, name: Some(name) })
            }
            OPCODE_SELF_STATS => {
                let id = parse_self_stats(payload)?;
                self.set_self(id)
            }
            OPCODE_PLAYER_SPAWN => {
                let (id, name) = parse_named_spawn(payload)?;
                if let Some(cp) = parse_spawn_combat_power(payload) {
                    self.combat_power.insert(id, cp);
                }
                self.spawn_info.insert(id, SpawnInfo { class_code: parse_spawn_class(payload), rank: parse_spawn_rank(payload) });
                if self.is_self(id) || self.players.get(&id) == Some(&name) {
                    return None;
                }
                self.mobs.remove(&id);
                self.owners.remove(&id);
                self.players.insert(id, name.clone());
                Some(Change::PlayerNamed { id, name })
            }
            OPCODE_DEATH => {
                let d = parse_death(payload)?;
                let (id, name) = (d.killer?, d.killer_name?);
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
                let mut members = parse_roster(payload, opcode == OPCODE_GROUP_ROSTER);
                if self.in_dungeon {
                    for m in &mut members {
                        m.ready = None;
                    }
                }
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
                        m.roster_class = m.roster_class.or(old.roster_class);
                        // Party rosters don't say; group rosters do.
                        if opcode == OPCODE_PARTY_ROSTER {
                            m.ready = old.ready;
                            m.leader = old.leader;
                        }
                    }
                }
                if members == self.party {
                    return None;
                }
                let names_changed = members.iter().map(|m| &m.name).ne(self.party.iter().map(|m| &m.name));
                self.party = members;
                names_changed.then_some(Change::PartyChanged { members: self.party.len() })
            }
            OPCODE_GROUP_JOIN | OPCODE_PARTY_ADD_MEMBER | OPCODE_PARTY_RECONNECT => {
                let before = self.party.len();
                for mut m in parse_roster(payload, opcode == OPCODE_GROUP_JOIN) {
                    if self.in_dungeon {
                        m.ready = None;
                    }
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
            OPCODE_GROUP_READY => {
                let (character, _) = parse_member_id(payload)?;
                let ready = *payload.get(8)? != 0;
                let m = self.party.iter_mut().find(|m| m.character == character && m.ready.is_some())?;
                m.ready = Some(ready);
                None
            }
            OPCODE_GROUP_SELF_READY => {
                let ready = *payload.get(2)? != 0;
                let me = self.self_name.clone()?;
                let m = self.party.iter_mut().find(|m| m.name == me && m.ready.is_some())?;
                m.ready = Some(ready);
                None
            }
            OPCODE_PARTY_MEMBER_LEFT | OPCODE_GROUP_MEMBER_LEFT => {
                let (character, _) = parse_member_id(payload)?;
                let me = self.self_name.as_deref();
                let mine = self.party.iter().any(|m| m.character == character && Some(m.name.as_str()) == me);
                let before = self.party.len();
                if mine {
                    self.party.clear();
                } else {
                    self.party.retain(|m| m.character != character);
                }
                (self.party.len() != before).then_some(Change::PartyChanged { members: self.party.len() })
            }
            OPCODE_PARTY_LEADER => {
                let (character, _) = parse_member_id(payload)?;
                for m in &mut self.party {
                    m.leader = m.character == character;
                }
                None
            }
            OPCODE_PARTY_LEFT
            | OPCODE_GROUP_LEFT
            | OPCODE_PARTY_KICKED
            | OPCODE_PARTY_DISBANDED
            | OPCODE_PARTY_LEFT_NOTICE
            | OPCODE_GROUP_KICKED
            | OPCODE_FORCE_DISBANDED
            | OPCODE_FORCE_LEFT => {
                // Your own leave requests answer with a u16 result: only 0 means you left
                // (as Questlog reads them).
                if matches!(opcode, OPCODE_PARTY_LEFT | OPCODE_GROUP_LEFT) && payload.get(..2)? != [0, 0] {
                    return None;
                }
                if matches!(opcode, OPCODE_FORCE_DISBANDED | OPCODE_FORCE_LEFT) {
                    self.force.clear();
                }
                if self.party.is_empty() {
                    return None;
                }
                self.party.clear();
                Some(Change::PartyChanged { members: 0 })
            }
            OPCODE_FORCE_INFO => {
                self.force = force_members(payload).collect();
                None
            }
            OPCODE_FORCE_ADD_MEMBER | OPCODE_FORCE_RECONNECT | OPCODE_FORCE_MEMBER | OPCODE_FORCE_MEMBER_LIST => {
                self.force.extend(force_members(payload));
                None
            }
            OPCODE_FORCE_REMOVE_MEMBER => {
                let (character, _) = parse_member_id(payload)?;
                self.force.remove(&character);
                None
            }
            // A boss's copy of a party member: a mob, though it carries their name.
            OPCODE_MOB_SPAWN if parse_player_copy(payload).is_some() => {
                let (id, _, type_id) = parse_player_copy(payload)?;
                self.owners.remove(&id);
                self.players.remove(&id);
                (self.mobs.insert(id, type_id) != Some(type_id)).then_some(Change::MobSeen { id, type_id })
            }
            OPCODE_MOB_SPAWN => {
                // Summons with a named layout have no type we can read, but still an owner.
                let summon = parse_summon_owner(payload);
                let mob = parse_mob_spawn(payload);
                let id = summon.map(|(id, _)| id).or(mob.map(|(id, _)| id))?;
                self.players.remove(&id);
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

/// Combat power as players have it now (level-cap characters 100K-400K; new ones less).
/// Where a spawn's could be read two ways, the one in this band is taken.
const LIKELY_COMBAT_POWER: std::ops::RangeInclusive<u64> = 40_000..=2_000_000;

/// A player's combat power in their spawn (`0x3645`): a u64 just before the message's end,
/// followed by a byte (how they died) and maybe one more. Its last flag is a bit packed
/// into an earlier flag byte when one has a bit free, else a byte of its own, so the u64
/// is 9 or 10 bytes from the end. Checked on our captures (2026-11-07, the Abyss;
/// 2026-11-13, the Ludra raid; 46,526 spawns): one of the two reads is a combat power in
/// all but 2; both are in 279, each the other shifted a byte (×256), and the one in
/// [`LIKELY_COMBAT_POWER`] is right (the party roster's agrees). The field's place is
/// Questlog's lead (its CharAppear_NT); the offsets and the check are ours.
pub fn parse_spawn_combat_power(p: &[u8]) -> Option<u32> {
    spawn_combat_power_at(p).map(|(_, cp)| cp)
}

/// Where a spawn's combat power starts, and it.
fn spawn_combat_power_at(p: &[u8]) -> Option<(usize, u32)> {
    let at = |from_end: usize| -> Option<(usize, u64)> {
        let start = p.len().checked_sub(from_end)?;
        Some((start, u64::from_le_bytes(p.get(start..start + 8)?.try_into().ok()?)))
    };
    let ok = |(_, v): &(usize, u64)| (*v as u32 as u64) == *v && COMBAT_POWER.contains(&(*v as u32));
    let (nine, ten) = (at(9).filter(ok), at(10).filter(ok));
    let (start, v) = match (nine, ten) {
        (Some(a), Some(b)) => [a, b].into_iter().find(|(_, v)| LIKELY_COMBAT_POWER.contains(v))?,
        (a, b) => a.or(b)?,
    };
    Some((start, v as u32))
}

/// The ranks a spawn can give: 1 the highest.
pub const RANKS: std::ops::RangeInclusive<u32> = 1..=19;

/// A player's rank in their spawn (`0x3645`): the u32 just before their combat power. 1 to
/// 19, 19 the lowest: on our captures (2026-11-07, the Abyss; 2026-11-13, the Ludra raid;
/// 4,090 players) 19 is the most common by far (1,232), and the lower the rank, the higher
/// the players' combat power (a median of 225K at 5, 176K at 10, 119K at 19). Which of the
/// game's ranks each number is hasn't been checked against the game.
pub fn parse_spawn_rank(p: &[u8]) -> Option<u32> {
    let (start, _) = spawn_combat_power_at(p)?;
    u32_at(p, start.checked_sub(4)?).filter(|r| RANKS.contains(r))
}

/// The classes in a spawn's class byte, by (byte - 1) / 4 from 1 (its low bits are the
/// player's faction and look).
const SPAWN_CLASSES: [u32; 8] = [11, 12, 14, 13, 16, 15, 17, 18];

/// A player's class in their spawn (`0x3645`): the byte right after their name (see
/// [`class_of_byte`]). On the Abyss capture, against the classes the meter learnt from 335
/// enemy players' skills, it agrees for 324 (97 %); on both captures, against the skills
/// 2,600 players cast, each value's most common class (Templars' buffs aside) is its own.
pub fn parse_spawn_class(p: &[u8]) -> Option<u32> {
    let mut i = 0;
    read_varint(p, &mut i)?;
    i += SPAWN_NAME_SKIP;
    let len = *p.get(i)? as usize;
    class_of_byte(*p.get(i + 1 + len)?)
}

/// A class as spawns and group rosters give it (the byte after a name): 5 to 36, four to a
/// class. A group roster's agrees with the same players' spawns for all 27 players in both
/// captures (2026-11-07 and 11-13: expedition groups and Ludra rooms).
pub fn class_of_byte(b: u8) -> Option<u32> {
    let k = u32::from(b).checked_sub(1)? / 4;
    SPAWN_CLASSES.get(usize::try_from(k.checked_sub(1)?).ok()?).copied()
}

/// Your combat power in your own spawn (`0x3633`): a u64 81 bytes from the end, then your
/// highest combat power (a u64, never lower). Checked on our captures: 115 of 118 (the
/// other 3 read as nothing, not as a wrong number).
pub fn parse_self_combat_power(p: &[u8]) -> Option<u32> {
    let start = p.len().checked_sub(81)?;
    let now = u64::from_le_bytes(p.get(start..start + 8)?.try_into().ok()?);
    let best = u64::from_le_bytes(p.get(start + 8..start + 16)?.try_into().ok()?);
    let cp = u32::try_from(now).ok().filter(|c| COMBAT_POWER.contains(c))?;
    (best >= now && best <= u64::from(*COMBAT_POWER.end())).then_some(cp)
}

/// `0x8d04`: varint mob, u32 skill, varint player, varint server, u8 length, UTF-8 name.
/// A death (`0x8d04`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Death {
    pub entity: u64,
    /// The skill that did it (0 when none, e.g. a mob killed by a script).
    pub skill: u32,
    /// Who did it: a player's or mob's entity.
    pub killer: Option<u64>,
    /// A player killer's name.
    pub killer_name: Option<String>,
    /// A mob killer's type.
    pub killer_type: Option<u32>,
}

/// `0x8d04`: varint entity, u32 skill, varint killer, u16 killer's server, u8 length + the
/// killer's name, u8 length + their legion, u16, varint killer's mob type, u32, u8, u8.
/// Checked on Vakron Sky Island: all 101 arrive when the entity's HP is 0.
pub fn parse_death(p: &[u8]) -> Option<Death> {
    let mut i = 0;
    let entity = read_varint(p, &mut i)?;
    let skill = u32_at(p, i)?;
    i += 4;
    let killer = read_varint(p, &mut i)?;
    i += 2; // the killer's server
    let len = *p.get(i)? as usize;
    i += 1;
    let name = std::str::from_utf8(p.get(i..i + len)?).ok()?;
    i += len;
    if len > MAX_NAME_BYTES || name.chars().any(char::is_control) {
        return None;
    }
    let legion = *p.get(i)? as usize;
    i += 1 + legion + 2;
    let killer_type = read_varint(p, &mut i).and_then(|t| u32::try_from(t).ok()).filter(|t| *t > 0);
    Some(Death {
        entity,
        skill,
        killer: (killer > 0).then_some(killer),
        killer_name: (!name.is_empty()).then(|| name.to_string()),
        killer_type,
    })
}

/// A roster: each member's entry is u32 character ID, 2 zero bytes, u16 server, u8 length
/// and their name, then other fields. In `group` rosters (`0x9702`, `0x970b`) the entry
/// starts two bytes earlier: u8 flags (`0x0c` always set, `0x02` ready, `0x10` an
/// expedition's group started) and u8 slot (1 is the leader). A Sanctuary's room (slots up
/// to 10) has `0x10` set all along, readying up included (the captures of 2026-11-07 and
/// 11-13): its start is the dungeon's (see `Registry::observe`).
pub fn parse_roster(p: &[u8], group: bool) -> Vec<PartyMember> {
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
    let head_at = |start: usize| start.checked_sub(2).map(|f| (p[f], p[f + 1])).filter(|(flags, _)| group && flags & 0x0c == 0x0c);
    // A Sanctuary's room: more than one party's slots.
    let room = found.iter().any(|(start, _, _)| head_at(*start).is_some_and(|(_, slot)| slot > 5));
    let mut members: Vec<PartyMember> = Vec::new();
    for (k, (start, name, end)) in found.iter().enumerate() {
        if members.iter().any(|m| &m.name == name) {
            continue;
        }
        let next = found.get(k + 1).map_or(p.len(), |f| f.0);
        let server = u16::from_le_bytes([p[start + 6], p[start + 7]]);
        let (level, gear_score, combat_power) = roster_stats(&p[*end..next], server);
        let head = head_at(*start);
        let leader = head.is_some_and(|(_, slot)| slot == 1);
        let ready = head.filter(|(flags, slot)| (room || flags & 0x10 == 0) && *slot > 1).map(|(flags, _)| flags & 0x02 != 0);
        members.push(PartyMember {
            character: u32_at(p, *start).expect("an entry has 9 bytes before its name"),
            name: name.clone(),
            server,
            level,
            gear_score,
            combat_power,
            legion: None,
            ready,
            leader,
            slot: head.map(|(_, slot)| slot),
            // A group roster's entry: its class right after the name, as in a spawn.
            roster_class: if group { p.get(*end).and_then(|b| class_of_byte(*b)) } else { None },
        });
    }
    members
}

/// u32 character ID, 2 zero bytes, u16 server.
fn parse_member_id(p: &[u8]) -> Option<(u32, u16)> {
    if p.get(4..6)? != [0, 0] {
        return None;
    }
    Some((u32_at(p, 0)?, u16::from_le_bytes(p.get(6..8)?.try_into().ok()?)))
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
        ready: None,
        leader: false,
        slot: None,
        roster_class: None,
    })
}

/// A mob spawn for an object that belongs to a player: varint entity ID,
/// [`NAMED_OBJECT_MARK`], u8 length and the player's name.
pub fn parse_player_copy(p: &[u8]) -> Option<(u64, String, u32)> {
    let mut i = 0;
    let id = read_varint(p, &mut i)?;
    if p.get(i..i + 3)? != PLAYER_COPY_MARK {
        return None;
    }
    let len = *p.get(i + 3)? as usize;
    if !ROSTER_NAME_BYTES.contains(&len) {
        return None;
    }
    let name = std::str::from_utf8(p.get(i + 4..i + 4 + len)?).ok()?;
    let type_id = u32_at(p, i + 4 + len)?;
    name.chars().all(char::is_alphanumeric).then(|| (id, name.to_string(), type_id))
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

/// `0x364a`: varint entity ID (ours), then the stats.
pub fn parse_self_stats(p: &[u8]) -> Option<u64> {
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

/// The members in a Force message: every place it reads as a roster entry (u32 character
/// ID, `00 00`, u16 server 1000-2999, u8 length 2-24, a name of letters and digits).
fn force_members(p: &[u8]) -> impl Iterator<Item = (u32, String)> + '_ {
    (0..p.len().saturating_sub(9)).filter_map(move |i| {
        if p[i + 4..i + 6] != [0, 0] {
            return None;
        }
        let server = u16::from_le_bytes([p[i + 6], p[i + 7]]);
        let len = usize::from(p[i + 8]);
        if !(1000..3000).contains(&server) || !(2..=24).contains(&len) {
            return None;
        }
        let name = std::str::from_utf8(p.get(i + 9..i + 9 + len)?).ok()?;
        name.chars().all(char::is_alphanumeric).then(|| (u32::from_le_bytes(p[i..i + 4].try_into().expect("4 bytes")), name.to_string()))
    })
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
    const AJA_COPY: &str = "b3 d1 05 1c 00 01 03 41 6a 61 ac 8f 2c 00 00 02 39 ba 80 46 f5 af 7f 46 00 52 1c 47";

    #[test]
    fn combat_power_from_a_players_spawn() {
        // The ends of real spawns (the raid, 2026-11-13), behind some made-up start.
        let with_end = |end: &str| [vec![0x55; 40], hex(end)].concat();
        // The last flag packed into an earlier byte: the u64 9 from the end.
        assert_eq!(parse_spawn_combat_power(&with_end("00 0a 00 00 00 ab c6 02 00 00 00 00 00 00")), Some(181_931));
        // The last flag a byte of its own: 10 from the end.
        assert_eq!(parse_spawn_combat_power(&with_end("0a 00 00 00 99 bc 02 00 00 00 00 00 00 01")), Some(179_353));
        // Both read as a combat power (1,016 or 260,309): the likely one.
        assert_eq!(parse_spawn_combat_power(&with_end("00 00 d5 f8 03 00 00 00 00 00 00 00")), Some(260_309));
        // Neither.
        assert_eq!(parse_spawn_combat_power(&with_end("ff ff ff ff ff ff ff ff ff ff ff ff")), None);
        assert_eq!(parse_spawn_combat_power(&[1, 2, 3]), None);
    }

    #[test]
    fn rank_from_a_players_spawn() {
        let with_end = |end: &str| [vec![0x55; 40], hex(end)].concat();
        // The u32 before the combat power, wherever that is.
        assert_eq!(parse_spawn_rank(&with_end("0a 00 00 00 ab c6 02 00 00 00 00 00 00")), Some(10));
        assert_eq!(parse_spawn_rank(&with_end("13 00 00 00 99 bc 02 00 00 00 00 00 00 01")), Some(19));
        // Not a rank: none.
        assert_eq!(parse_spawn_rank(&with_end("00 00 00 01 ab c6 02 00 00 00 00 00 00")), None);
    }

    #[test]
    fn class_from_a_players_spawn() {
        let spawn = |class: u8| [encode_varint(9463), vec![0; SPAWN_NAME_SKIP], vec![4], b"Name".to_vec(), vec![class, 0, 0, 0]].concat();
        // Four values to a class, in the game's order.
        assert_eq!([5, 8, 9, 13, 17, 21, 25, 29, 36].map(|v| parse_spawn_class(&spawn(v))), [11, 11, 12, 14, 13, 16, 15, 17, 18].map(Some));
        assert_eq!(parse_spawn_class(&spawn(4)), None);
        assert_eq!(parse_spawn_class(&spawn(37)), None);
    }

    #[test]
    fn your_combat_power_from_your_spawn() {
        let spawn = |now: u64, best: u64| [vec![0x55; 30], now.to_le_bytes().to_vec(), best.to_le_bytes().to_vec(), vec![0; 65]].concat();
        assert_eq!(parse_self_combat_power(&spawn(152_973, 152_973)), Some(152_973));
        assert_eq!(parse_self_combat_power(&spawn(148_497, 148_598)), Some(148_497));
        // Not a combat power and its best: nothing.
        assert_eq!(parse_self_combat_power(&spawn(152_973, 5)), None);
        assert_eq!(parse_self_combat_power(&spawn(3, 3)), None);
    }

    #[test]
    fn spawns_give_each_player_their_combat_power() {
        let mut r = Registry::default();
        let mut p = with_name("fd 63 15 b0 a4 00 07", "Silke", "");
        p.extend(hex("00 0a 00 00 00 ab c6 02 00 00 00 00 00 00"));
        r.observe(OPCODE_PLAYER_SPAWN, &p);
        assert_eq!(r.combat_power(r.entity_named("Silke").unwrap()), Some(181_931));
    }

    #[test]
    fn group_rosters_carry_gear_score_and_combat_power() {
        let m = parse_roster(&hex(VAKRON_ROSTER), true);
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
        // Their slots in the group, and each one's class.
        assert_eq!(m.iter().map(|m| m.slot).collect::<Vec<_>>(), (1..=5).map(Some).collect::<Vec<_>>());
        assert!(m.iter().all(|m| m.roster_class.is_some()), "{:?}", m.iter().map(|m| m.roster_class).collect::<Vec<_>>());
    }

    #[test]
    fn a_sanctuary_rooms_ready_marks_last_until_the_dungeon() {
        // A room of ten as the captures have it: 0x10 set all along, 0x02 for ready.
        let room = |ready_from: u8| -> Vec<u8> {
            let mut p = vec![0; 6];
            for slot in 1..=10u8 {
                let flags = if slot >= ready_from { 0x1e } else { 0x1c };
                p.extend([flags, slot]);
                p.extend((1000 + u32::from(slot)).to_le_bytes());
                p.extend([0, 0]);
                p.extend(2503u16.to_le_bytes());
                let name = format!("Member{slot}");
                p.push(name.len() as u8);
                p.extend(name.as_bytes());
                p.extend([24, 0, 0, 0, 0, 0, 0, 0]);
            }
            p
        };
        let mut r = Registry::default();
        r.observe(OPCODE_GROUP_ROSTER, &room(6));
        let ready: Vec<_> = r.party().iter().map(|m| m.ready).collect();
        assert_eq!(ready[0], None, "the leader doesn't ready up");
        assert_eq!(ready[1..5], [Some(false); 4]);
        assert_eq!(ready[5..], [Some(true); 5]);
        assert_eq!(r.party().iter().map(|m| m.slot.unwrap()).collect::<Vec<_>>(), (1..=10).collect::<Vec<_>>());
        // Started: into the dungeon, where the rosters keep coming.
        r.observe(crate::dungeon::OPCODE_MAP_ENTERED, &[0x12, 0x27, 0x09, 0, 0]);
        r.observe(crate::dungeon::OPCODE_DUNGEON_STEP, &[0x12, 0x27, 0x09, 0, 1]);
        r.observe(OPCODE_GROUP_ROSTER, &room(2));
        assert!(r.party().iter().all(|m| m.ready.is_none()));
        // Back in town: readying up again.
        r.observe(crate::dungeon::OPCODE_MAP_ENTERED, &[0x56, 0x04, 0, 0, 0]);
        r.observe(OPCODE_GROUP_ROSTER, &room(3));
        assert_eq!(r.party()[1].ready, Some(false));
        assert_eq!(r.party()[2].ready, Some(true));
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

    /// From the Vakron capture, in the party finder before the start: the group's roster
    /// (Aja leads; robi03 and you not ready), TAQE joining, and the ready messages.
    const ROOM_ROSTER: &str = "e6 76 04 00 06 38 30 6b 20 33 62 05 08 28 09 00 00 03 80 62 01 00 00 00 e5 05 0f 01 00 05 0c 01 80 62 01 00 00 00 e5 05 03 41 6a 61 09 00 00 00 2d 00 00 00 63 06 00 00 e5 05 e5 05 04 52 3e 01 00 00 00 00 00 00 01 01 0c 02 a7 de 03 00 00 00 df 05 06 72 6f 62 69 30 33 1d 00 00 00 2d 00 00 00 10 07 00 00 df 05 df 05 04 73 47 01 00 00 00 00 00 00 01 01 0c 03 58 de 03 00 00 00 c7 09 0c 4e 65 6e 65 6e 65 54 6f 77 61 77 61 24 00 00 00 2d 00 00 00 e7 07 00 00 c7 09 c7 09 04 d2 58 01 00 00 00 00 00 00 01 02 00 04 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 04 00 00 00 00 00 00 00 00 00 00 00 00 05 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 04 00 00 00 00 00 00 00 00 00 00 00 00 02";
    const TAQE_JOINS: &str = "0c 04 0f 1a 04 00 00 00 e4 05 04 54 41 51 45 0e 00 00 00 2d 00 00 00 c3 05 00 00 01 e4 05 e4 05 04 f6 f9 00 00 00 00 00 00 00 01 01";

    #[test]
    fn group_members_ready_up() {
        let mut r = Registry::default();
        r.observe(OPCODE_SELF_SPAWN, &with_name("cb 1c 5f 81 c1 28 37", "NeneneTowawa", ""));
        r.observe(OPCODE_GROUP_ROSTER, &hex(ROOM_ROSTER));
        let ready = |r: &Registry| r.party().iter().map(|m| (m.name.clone(), m.ready)).collect::<Vec<_>>();
        let s = |n: &str| n.to_string();
        assert_eq!(ready(&r), [(s("Aja"), None), (s("robi03"), Some(false)), (s("NeneneTowawa"), Some(false))]);
        r.observe(OPCODE_GROUP_READY, &hex("a7 de 03 00 00 00 df 05 01"));
        r.observe(OPCODE_GROUP_SELF_READY, &hex("00 00 01"));
        assert!(r.observe(OPCODE_GROUP_JOIN, &hex(TAQE_JOINS)).is_some());
        assert_eq!(r.party()[3].ready, Some(false));
        r.observe(OPCODE_GROUP_READY, &hex("0f 1a 04 00 00 00 e4 05 01"));
        assert_eq!(
            ready(&r),
            [(s("Aja"), None), (s("robi03"), Some(true)), (s("NeneneTowawa"), Some(true)), (s("TAQE"), Some(true))]
        );
        // Inside the dungeon the group roster marks everyone as in: no ready marks.
        r.observe(OPCODE_GROUP_ROSTER, &hex(VAKRON_ROSTER));
        assert!(r.party().iter().all(|m| m.ready.is_none()));
    }

    #[test]
    fn members_leaving_and_being_removed() {
        let mut r = Registry::default();
        r.observe(OPCODE_SELF_SPAWN, &with_name("cb 1c 5f 81 c1 28 37", "NeneneTowawa", ""));
        r.observe(OPCODE_GROUP_ROSTER, &hex(VAKRON_ROSTER));
        // robi03 leaves (from the end of the Vakron run).
        assert_eq!(
            r.observe(OPCODE_GROUP_MEMBER_LEFT, &hex("a7 de 03 00 00 00 df 05")),
            Some(Change::PartyChanged { members: 4 })
        );
        assert!(r.party().iter().all(|m| m.name != "robi03"));
        assert_eq!(r.observe(OPCODE_PARTY_MEMBER_LEFT, &hex("a7 de 03 00 00 00 df 05")), None);
        // The same message with your own ID: you're out (kicked), so the party is gone.
        assert_eq!(
            r.observe(OPCODE_PARTY_MEMBER_LEFT, &hex("58 de 03 00 00 00 c7 09")),
            Some(Change::PartyChanged { members: 0 })
        );
        assert!(r.party().is_empty());
    }

    #[test]
    fn leader_changes_and_the_party_ending() {
        let mut r = Registry::default();
        r.observe(OPCODE_SELF_SPAWN, &with_name("cb 1c 5f 81 c1 28 37", "NeneneTowawa", ""));
        r.observe(OPCODE_GROUP_ROSTER, &hex(ROOM_ROSTER));
        let leader = |r: &Registry| r.party().iter().find(|m| m.leader).map(|m| m.name.clone());
        assert_eq!(leader(&r).as_deref(), Some("Aja"), "the group's first slot leads");
        // Vakron Sky Island, near the end: you became the leader, then the party broke up.
        r.observe(OPCODE_PARTY_LEADER, &hex("58 de 03 00 00 00 c7 09"));
        assert_eq!(leader(&r).as_deref(), Some("NeneneTowawa"));
        assert_eq!(r.observe(OPCODE_PARTY_DISBANDED, &hex("08")), Some(Change::PartyChanged { members: 0 }));
        // Removed from a party finder group (the expedition capture), or kicked from a party.
        for (opcode, payload) in [(OPCODE_GROUP_KICKED, "00"), (OPCODE_PARTY_KICKED, "00")] {
            r.observe(OPCODE_GROUP_ROSTER, &hex(ROOM_ROSTER));
            assert_eq!(r.observe(opcode, &hex(payload)), Some(Change::PartyChanged { members: 0 }));
        }
    }

    #[test]
    fn a_bosss_copy_of_a_player_is_a_mob() {
        // Tiere's copy of Aja (Vakron Sky Island): it attacks the party with mob skills.
        let mut r = Registry::default();
        r.observe(OPCODE_PLAYER_SPAWN, &with_name("8b 26 01 20 a2 01 07", "Aja", ""));
        assert_eq!(parse_player_copy(&hex(AJA_COPY)), Some((92_339, "Aja".to_string(), 2_920_364)));
        r.observe(OPCODE_MOB_SPAWN, &hex(AJA_COPY));
        assert_eq!(r.mob_type(92_339), Some(2_920_364));
        assert_eq!(r.player_name(92_339), None, "not a player");
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
        r.observe(OPCODE_SELF_STATS, &hex("ff 18 00 00 00 00 00 00 00 00 00"));
        assert_eq!(r.self_id(), Some(3199));
        assert_eq!(r.label(3199), "You (#3199)");
        // Repeated ticks change nothing.
        assert_eq!(r.observe(OPCODE_SELF_STATS, &hex("ff 18 00 00")), None);
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
    fn a_kill_names_the_killer() {
        // From the zone-change capture: we (3199) killed mob 19776 with 18010340.
        let mut p = hex("c0 9a 01 e4 d0 12 01 ff 18 c7 09");
        p.push(12);
        p.extend_from_slice(b"NeneneTowawa");
        p.extend(hex("00 00"));
        let d = parse_death(&p).unwrap();
        assert_eq!((d.entity, d.skill, d.killer, d.killer_name.as_deref()), (19_776, 18_010_340, Some(3199), Some("NeneneTowawa")));

        let mut r = Registry::default();
        r.observe(OPCODE_SELF_STATS, &hex("ff 18 00"));
        assert_eq!(r.label(3199), "You (#3199)");
        r.observe(OPCODE_DEATH, &p);
        assert_eq!(r.label(3199), "NeneneTowawa");
        assert_eq!(r.self_name(), Some("NeneneTowawa"));
    }

    #[test]
    fn deaths_in_a_dungeon() {
        // Vakron Sky Island: a mob killed with nobody named, and a party member (8829)
        // killed by a mob-type object (2920364, a Protection Circle barrier).
        let mob = parse_death(&hex("f7 bf 05 f4 ae bb 00 00 00 00 00 00 00 00 00 00 00 00 00 01 02")).unwrap();
        assert_eq!((mob.entity, mob.killer, mob.killer_name, mob.killer_type), (90_103, None, None, None));
        let member = parse_death(&hex("fd 44 b3 7b 18 00 ae ba 02 00 00 00 00 00 00 ac 9f b2 01 00 00 00 00 01 02")).unwrap();
        assert_eq!((member.entity, member.skill, member.killer, member.killer_type), (8829, 1_604_531, Some(40_238), Some(2_920_364)));
    }

    #[test]
    fn relogin_clears_stale_ids() {
        let mut r = Registry::default();
        r.observe(OPCODE_SELF_STATS, &hex("ff 18 00"));
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
        let names: Vec<String> = parse_roster(&p, false).into_iter().map(|m| m.name).collect();
        assert_eq!(names, ["\u{7c92}\u{5bfb}\u{6b22}", "Pingu", "NeneneTowawa"]);
        assert!(parse_roster(&hex("00 00 00 00 00 00 00 00 00 00"), false).is_empty());
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
        // Joining the party itself: one roster entry.
        r.observe(OPCODE_PLAYER_SPAWN, &with_name("e4 4f 01 20 a2 01 07", "Latecomer", ""));
        assert_eq!(
            r.observe(OPCODE_PARTY_ADD_MEMBER, &roster(&[(4, 2504, "Latecomer")])),
            Some(Change::PartyChanged { members: 4 })
        );
        assert!(r.in_party(10212));
        // Back online: already a member.
        assert_eq!(r.observe(OPCODE_PARTY_RECONNECT, &roster(&[(4, 2504, "Latecomer")])), None);
        // A leave that failed changes nothing.
        assert_eq!(r.observe(OPCODE_PARTY_LEFT, &hex("05 00")), None);
        assert_eq!(r.observe(OPCODE_PARTY_LEFT, &hex("00 00")), Some(Change::PartyChanged { members: 0 }));
        assert!(!r.in_party(10210));
        assert!(r.in_party(3659));
    }

    #[test]
    fn a_boss_we_never_saw_spawn() {
        // Eternal Ludra in the 2026-11-06 raid: up before the meter started, so no spawn,
        // but its stagger gauge comes (entity 36018).
        let mut r = Registry::default();
        r.observe(OPCODE_SELF_SPAWN, &with_name("cb 1c 5f 81 c1 28 37", "NeneneTowawa", ""));
        assert_eq!(r.label(36_018), "#36018");
        r.observe(crate::state::OPCODE_STAGGER, &hex("b2 99 02 03 01 94 11 00 00 8a 11 00 00 02"));
        assert!(r.unseen_boss(36_018));
        assert_eq!(r.label(36_018), "Boss");
        // A player's gauge (a boss's copy of one) or a mob whose spawn we saw isn't one.
        r.observe(OPCODE_PLAYER_SPAWN, &with_name("e2 4f 01 20 a2 01 07", "Pingu", ""));
        r.observe(crate::state::OPCODE_STAGGER, &hex("e2 4f 03 01 94 11 00 00 8a 11 00 00 02"));
        assert!(!r.unseen_boss(10_210));
    }

    /// Party HP (`0x921b`): an entity, HP, max HP, MP, max MP.
    fn party_hp(id: u64) -> Vec<u8> {
        let mut p = encode_varint(id);
        p.extend(encode_varint(9_000));
        p.extend(encode_varint(10_000));
        p.extend(500u32.to_le_bytes());
        p.extend(1_000u32.to_le_bytes());
        p
    }

    #[test]
    fn party_hp_tells_members_from_before_the_meter_started() {
        // As in the 2026-10-24 session started ten minutes in: the members' spawns came
        // before the meter, so their names aren't known; their party HP still comes.
        let mut r = Registry::default();
        r.observe(OPCODE_SELF_SPAWN, &with_name("cb 1c 5f 81 c1 28 37", "NeneneTowawa", ""));
        r.observe(OPCODE_PARTY_ROSTER, &roster(&[(182272, 2503, "Pingu"), (253528, 2503, "NeneneTowawa")]));
        assert!(!r.in_party(8967));
        r.observe(OPCODE_PARTY_VITALS, &party_hp(8967));
        assert!(r.in_party(8967));
        assert!(!r.in_party(10601));
        // Once named, the roster says: a stranger who was sent party HP isn't kept.
        r.observe(OPCODE_PLAYER_SPAWN, &with_name("e2 4f 01 20 a2 01 07", "Pingu", ""));
        r.observe(OPCODE_PLAYER_SPAWN, &with_name("e3 4f 01 20 a2 01 07", "Stranger", ""));
        r.observe(OPCODE_PARTY_VITALS, &party_hp(10211));
        assert!(r.in_party(10210));
        assert!(!r.in_party(10211));
        // Without a roster (none sent since the meter started), party HP alone counts.
        let mut r = Registry::default();
        r.observe(OPCODE_SELF_SPAWN, &with_name("cb 1c 5f 81 c1 28 37", "NeneneTowawa", ""));
        r.observe(OPCODE_PLAYER_SPAWN, &with_name("e2 4f 01 20 a2 01 07", "Pingu", ""));
        r.observe(OPCODE_PARTY_VITALS, &party_hp(10210));
        r.observe(OPCODE_PARTY_VITALS, &party_hp(3659));
        assert!(r.in_party(10210));
        assert!(r.in_party(3659));
        // A member back online (2026-11-01, just before the full roster): one entry, not us,
        // so not the whole party.
        r.observe(OPCODE_PARTY_RECONNECT, &roster(&[(4, 2504, "Latecomer")]));
        assert_eq!(r.party().len(), 1);
        assert!(r.in_party(10210));
        // Someone leaving: who's in it is learnt again from the next party HP.
        r.observe(OPCODE_PARTY_MEMBER_LEFT, &hex("00 00 00 00 00 00 00 00"));
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

    #[test]
    fn a_forces_roster() {
        // A Force's full roster (2026-10-23): 20 members, as party roster entries.
        let p = hex(include_str!("../testdata/force-roster.hex"));
        let members: HashMap<u32, String> = force_members(&p).collect();
        assert_eq!(members.len(), 20);
        let mut r = Registry::default();
        r.observe(OPCODE_FORCE_INFO, &p);
        assert_eq!(r.force_size(), 20);
    }

}
