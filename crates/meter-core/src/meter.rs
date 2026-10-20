//! The DPS meter: groups player damage into encounters and summarises them.
//!
//! Feed it every game message in order; ask for a [`Snapshot`] whenever the display
//! refreshes. Pure bookkeeping — no threads, no I/O — so it runs the same on live
//! traffic and on recorded sessions.
//!
//! An encounter is *your* fight: the targets you hit, and everyone's damage on those
//! targets (including hits from just before you joined). Players fighting other things
//! nearby don't count, unless the scope is set to [`Scope::Everyone`]. [`Scope::Party`]
//! narrows it to your party or group: the targets any of you hit, and only your damage.

use std::collections::{HashMap, HashSet, VecDeque};

use serde::{Deserialize, Serialize};

use crate::buffs::EffectTracker;
use crate::classes;
use crate::damage::{self, DamageEvent, Position};
use crate::entities::{Change, Registry};
use crate::protocol::{OPCODE_DAMAGE, OPCODE_DOT};

/// No activity in your fight for this long ends the encounter.
pub const DEFAULT_IDLE_TIMEOUT_US: u64 = 15_000_000;
/// Hits by others are remembered this long, so joining a fight late still shows
/// what happened to that target before you arrived.
const LOOKBACK_US: u64 = 30_000_000;
const LOOKBACK_MAX_EVENTS: usize = 50_000;
/// Player skill IDs are 8 digits (class prefix + skill); mob skills are 7.
const PLAYER_SKILL_MIN: u32 = 10_000_000;
const MAX_SKILLS_PER_ROW: usize = 12;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Scope {
    /// Targets you hit, and everyone's damage on them.
    #[default]
    Mine,
    /// Targets you or your party hit, and only your party's damage. On your own, just you.
    Party,
    /// All player damage nearby.
    Everyone,
}

/// One player hit, reduced to what the meter needs.
#[derive(Debug, Clone, Copy)]
struct Hit {
    ts_us: u64,
    target: u64,
    attacker: u64,
    skill: u32,
    damage: u64,
    /// Which of the attacker's casts it belongs to (0 for ticks).
    cast_id: u8,
    is_dot: bool,
    crit: bool,
    perfect: bool,
    double: bool,
    position: Position,
    /// Hits it stands for: its parts when it's a multi-hit, otherwise 1.
    hit_count: u32,
}

/// How a player's (or one skill's) hits landed. Each count is out of `hits`, except
/// `casts` and `multi_hits`.
#[derive(Debug, Default, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Outcomes {
    /// Casts (direct hits grouped by cast ID; ticks don't count).
    pub casts: u64,
    pub crits: u64,
    pub back: u64,
    pub front: u64,
    pub perfect: u64,
    pub double: u64,
    /// Hits that were multi-hits (two or more parts).
    pub multi: u64,
    /// Hits counting every part of a multi-hit (what the game's own counter shows).
    pub multi_hits: u64,
}

impl Outcomes {
    fn add(&mut self, h: &Hit, new_cast: bool) {
        self.casts += u64::from(new_cast);
        self.crits += u64::from(h.crit);
        self.back += u64::from(h.position == Position::Back);
        self.front += u64::from(h.position == Position::Front);
        self.perfect += u64::from(h.perfect);
        self.double += u64::from(h.double);
        self.multi += u64::from(h.hit_count >= 2);
        self.multi_hits += u64::from(h.hit_count);
    }

    fn merge(&mut self, o: &Outcomes) {
        self.casts += o.casts;
        self.crits += o.crits;
        self.back += o.back;
        self.front += o.front;
        self.perfect += o.perfect;
        self.double += o.double;
        self.multi += o.multi;
        self.multi_hits += o.multi_hits;
    }
}

#[derive(Debug, Default, Clone)]
struct SkillStats {
    hits: u64,
    damage: u64,
    max: u64,
    min: u64,
    outcomes: Outcomes,
}

impl SkillStats {
    fn add(&mut self, damage: u64, outcomes: &Outcomes, hits: u64, max: u64, min: u64) {
        self.min = if self.hits == 0 { min } else { self.min.min(min) };
        self.hits += hits;
        self.damage += damage;
        self.max = self.max.max(max);
        self.outcomes.merge(outcomes);
    }
}

#[derive(Debug, Default, Clone)]
struct PlayerStats {
    damage: u64,
    hits: u64,
    max_hit: u64,
    outcomes: Outcomes,
    skills: HashMap<u32, SkillStats>,
}

impl PlayerStats {
    fn add(&mut self, h: &Hit, new_cast: bool) {
        let mut one = Outcomes::default();
        one.add(h, new_cast);
        self.damage += h.damage;
        self.hits += 1;
        self.max_hit = self.max_hit.max(h.damage);
        self.outcomes.merge(&one);
        self.skills.entry(h.skill).or_default().add(h.damage, &one, 1, h.damage, h.damage);
    }

    fn merge(&mut self, other: &PlayerStats) {
        self.damage += other.damage;
        self.hits += other.hits;
        self.max_hit = self.max_hit.max(other.max_hit);
        self.outcomes.merge(&other.outcomes);
        for (id, s) in &other.skills {
            self.skills.entry(*id).or_default().add(s.damage, &s.outcomes, s.hits, s.max, s.min);
        }
    }
}

/// All player damage on one target during an encounter.
#[derive(Debug, Clone)]
struct TargetLog {
    first_us: u64,
    last_us: u64,
    total: u64,
    players: HashMap<u64, PlayerStats>,
}

#[derive(Debug, Clone)]
struct Encounter {
    /// Last time something happened in your fight.
    last_activity_us: u64,
    targets: HashMap<u64, TargetLog>,
    /// Targets you have hit.
    engaged: HashSet<u64>,
    /// Each attacker's latest cast ID per skill: a new ID is a new cast.
    last_cast: HashMap<(u64, u32), u8>,
}

impl Encounter {
    fn new(ts_us: u64) -> Self {
        Self {
            last_activity_us: ts_us,
            targets: HashMap::new(),
            engaged: HashSet::new(),
            last_cast: HashMap::new(),
        }
    }

    fn record(&mut self, h: &Hit) {
        // A cast counts once, on the target its first hit landed on.
        let new_cast = !h.is_dot && self.last_cast.insert((h.attacker, h.skill), h.cast_id) != Some(h.cast_id);
        let t = self.targets.entry(h.target).or_insert(TargetLog {
            first_us: h.ts_us,
            last_us: h.ts_us,
            total: 0,
            players: HashMap::new(),
        });
        t.first_us = t.first_us.min(h.ts_us);
        t.last_us = t.last_us.max(h.ts_us);
        t.total += h.damage;
        t.players.entry(h.attacker).or_default().add(h, new_cast);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SkillRow {
    pub skill: u32,
    /// Display name, filled in by the app from its skill table (None = unknown).
    pub name: Option<String>,
    /// Which skill ID's icon to show (a variant may share its base skill's icon).
    pub icon_key: Option<u32>,
    pub damage: u64,
    pub hits: u64,
    pub max: u64,
    /// Smallest single hit.
    #[serde(default)]
    pub min: u64,
    /// Fraction of this player's damage.
    pub share: f64,
    #[serde(default)]
    pub outcomes: Outcomes,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PlayerRow {
    pub id: u64,
    pub name: String,
    pub is_self: bool,
    /// First two digits of the player's skill IDs, which identify the class.
    pub class_code: Option<u32>,
    /// e.g. "Chanter".
    pub class_name: Option<String>,
    pub damage: u64,
    pub dps: f64,
    /// Fraction of everyone's damage.
    pub share: f64,
    pub hits: u64,
    pub max_hit: u64,
    #[serde(default)]
    pub outcomes: Outcomes,
    pub skills: Vec<SkillRow>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EncounterView {
    /// Still in combat (not yet idle).
    pub active: bool,
    pub duration_s: f64,
    pub total_damage: u64,
    pub total_dps: f64,
    /// The most-damaged target.
    pub target: Option<String>,
    /// Its entity ID, and its mob type when known (for looking up a proper name).
    pub target_id: Option<u64>,
    pub target_type: Option<u32>,
    /// Set by the app when its mob data marks the target as a boss.
    #[serde(default)]
    pub target_boss: bool,
    pub target_count: usize,
    /// Capture time of the first and last counted hit (µs since the Unix epoch).
    pub start_us: u64,
    pub end_us: u64,
    /// Highest damage first.
    pub players: Vec<PlayerRow>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Snapshot {
    pub self_name: Option<String>,
    pub self_id: Option<u64>,
    pub scope: Scope,
    /// People in your party or group, you included (0 when on your own).
    pub party_size: usize,
    /// The current encounter, or the last one once it has ended.
    pub encounter: Option<EncounterView>,
    pub encounters_completed: u32,
}

/// A fight that has ended, as it looked at the end.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct FinishedFight {
    pub view: EncounterView,
    pub self_name: Option<String>,
}

pub struct Meter {
    registry: Registry,
    /// Fights that ended since the last [`Meter::take_finished`].
    finished: Vec<FinishedFight>,
    scope: Scope,
    idle_timeout_us: u64,
    current: Option<Encounter>,
    last: Option<Encounter>,
    /// Recent player hits outside any encounter, for late joins.
    lookback: VecDeque<Hit>,
    /// Buffs and debuffs on everything in view.
    effects: EffectTracker,
    completed: u32,
}

impl Default for Meter {
    fn default() -> Self {
        Self::new(DEFAULT_IDLE_TIMEOUT_US)
    }
}

impl Meter {
    pub fn new(idle_timeout_us: u64) -> Self {
        Self {
            registry: Registry::default(),
            finished: Vec::new(),
            scope: Scope::Mine,
            idle_timeout_us,
            current: None,
            last: None,
            lookback: VecDeque::new(),
            effects: EffectTracker::default(),
            completed: 0,
        }
    }

    pub fn registry(&self) -> &Registry {
        &self.registry
    }

    pub fn encounters_completed(&self) -> u32 {
        self.completed
    }

    pub fn scope(&self) -> Scope {
        self.scope
    }

    pub fn set_scope(&mut self, scope: Scope) {
        self.scope = scope;
    }

    /// Fights that have ended since the last call, oldest first.
    pub fn take_finished(&mut self) -> Vec<FinishedFight> {
        std::mem::take(&mut self.finished)
    }

    /// Feeds one game message.
    /// Buff and debuff spans seen so far.
    pub fn effects(&self) -> &EffectTracker {
        &self.effects
    }

    pub fn feed(&mut self, ts_us: u64, opcode: u16, payload: &[u8]) {
        if self.effects.feed(ts_us, opcode, payload) {
            return;
        }
        if opcode == OPCODE_DOT {
            if let Some(ev) = damage::parse_dot(payload) {
                self.tick(ts_us);
                self.on_damage(ts_us, &ev);
            }
            return;
        }
        if opcode != OPCODE_DAMAGE {
            if let Some(Change::Relogin { .. }) = self.registry.observe(opcode, payload) {
                // Entity IDs from the previous login mean nothing now.
                self.current = None;
                self.last = None;
                self.lookback.clear();
                self.effects.clear();
            }
            return;
        }
        let Some(ev) = damage::parse(payload) else { return };
        self.tick(ts_us);
        self.on_damage(ts_us, &ev);
    }

    fn on_damage(&mut self, ts_us: u64, ev: &DamageEvent) {
        let Some(dmg) = ev.damage else { return };
        // Heal-over-time ticks carry the amount healed, not damage.
        if ev.is_self() || ev.is_heal() {
            return;
        }
        let me = self.registry.self_id();
        let by_me = me == Some(ev.attacker);
        let on_me = me == Some(ev.target);
        // In party scope, a party member's hit makes the target yours too.
        let by_party = self.scope == Scope::Party && self.registry.in_party(ev.attacker);
        let on_party = self.scope == Scope::Party && self.registry.in_party(ev.target);
        let player = self.is_player(ev.attacker, ev.skill);
        // Without knowing who we are yet, every player hit counts.
        let everyone = self.scope == Scope::Everyone || me.is_none();

        let hit = Hit {
            ts_us,
            target: ev.target,
            attacker: ev.attacker,
            skill: ev.skill,
            damage: dmg,
            cast_id: ev.cast_id,
            is_dot: ev.is_dot,
            crit: ev.is_crit(),
            perfect: ev.is_perfect(),
            double: ev.is_double(),
            position: ev.position(),
            hit_count: ev.hit_count(),
        };

        // Does this start or continue your fight?
        let engaged = self.current.as_ref().is_some_and(|e| e.engaged.contains(&ev.target));
        let relevant = by_me || on_me || by_party || on_party || engaged || (everyone && player);
        if !relevant {
            if player {
                self.remember(hit);
            }
            return;
        }

        if self.current.is_none() {
            let mut enc = Encounter::new(ts_us);
            // Bring in recent hits so a late join shows the whole fight on its targets.
            for h in self.lookback.drain(..) {
                enc.record(&h);
            }
            self.current = Some(enc);
        }
        let enc = self.current.as_mut().expect("encounter exists");
        enc.last_activity_us = enc.last_activity_us.max(ts_us);
        // Only your own hits make a target yours. Before we know who we are, everything
        // is shown, but none of it sticks once we do.
        if by_me || by_party || (self.scope == Scope::Everyone && player) {
            enc.engaged.insert(ev.target);
        }
        if player {
            enc.record(&hit);
        }
    }

    fn remember(&mut self, hit: Hit) {
        if let Some(enc) = self.current.as_mut() {
            // During a fight, keep everything: you may hit their target next.
            enc.record(&hit);
            return;
        }
        self.lookback.push_back(hit);
        while self
            .lookback
            .front()
            .is_some_and(|h| hit.ts_us.saturating_sub(h.ts_us) > LOOKBACK_US)
            || self.lookback.len() > LOOKBACK_MAX_EVENTS
        {
            self.lookback.pop_front();
        }
    }

    /// Ends the current encounter if it has been idle too long. Call periodically.
    pub fn tick(&mut self, now_us: u64) {
        if let Some(enc) = &self.current {
            if now_us.saturating_sub(enc.last_activity_us) > self.idle_timeout_us {
                if let Some(view) = self.view(enc, false) {
                    self.finished.push(FinishedFight {
                        view,
                        self_name: self.registry.self_name().map(str::to_string),
                    });
                }
                self.last = self.current.take();
                self.completed += 1;
            }
        }
    }

    /// Clears the display; the next hit starts a fresh encounter.
    pub fn reset(&mut self) {
        self.current = None;
        self.last = None;
        self.lookback.clear();
    }

    pub fn snapshot(&self) -> Snapshot {
        let (enc, active) = match (&self.current, &self.last) {
            (Some(e), _) => (Some(e), true),
            (None, Some(e)) => (Some(e), false),
            (None, None) => (None, false),
        };
        Snapshot {
            self_name: self.registry.self_name().map(str::to_string),
            self_id: self.registry.self_id(),
            scope: self.scope,
            party_size: self.registry.party().len(),
            encounter: enc.and_then(|e| self.view(e, active)),
            encounters_completed: self.completed,
        }
    }

    fn is_player(&self, id: u64, skill: u32) -> bool {
        let r = &self.registry;
        r.is_self(id)
            || r.player_name(id).is_some()
            || (r.mob_type(id).is_none() && skill >= PLAYER_SKILL_MIN)
    }

    fn view(&self, e: &Encounter, active: bool) -> Option<EncounterView> {
        let everyone = self.scope == Scope::Everyone || self.registry.self_id().is_none();
        let counted: Vec<(&u64, &TargetLog)> = e
            .targets
            .iter()
            .filter(|(id, _)| everyone || e.engaged.contains(id))
            .collect();
        if counted.is_empty() {
            return None;
        }

        let first = counted.iter().map(|(_, t)| t.first_us).min()?;
        let last = counted.iter().map(|(_, t)| t.last_us).max()?;
        let duration_s = ((last - first) as f64 / 1e6).max(1.0);

        // Party scope counts only your party's damage (just yours when on your own).
        let party_only = self.scope == Scope::Party && self.registry.self_id().is_some();
        let mut per_player: HashMap<u64, PlayerStats> = HashMap::new();
        for (_, t) in &counted {
            for (id, p) in &t.players {
                if !party_only || self.registry.in_party(*id) {
                    per_player.entry(*id).or_default().merge(p);
                }
            }
        }
        if per_player.is_empty() {
            return None;
        }
        let total: u64 = per_player.values().map(|p| p.damage).sum();

        let mut players: Vec<PlayerRow> = per_player
            .iter()
            .map(|(id, p)| self.row(*id, p, total, duration_s))
            .collect();
        players.sort_by(|a, b| b.damage.cmp(&a.damage).then(a.id.cmp(&b.id)));

        let main_target = counted
            .iter()
            .max_by_key(|(id, t)| (t.total, std::cmp::Reverse(**id)))
            .map(|(id, _)| **id);

        Some(EncounterView {
            active,
            duration_s,
            total_damage: total,
            total_dps: total as f64 / duration_s,
            target: main_target.map(|id| self.registry.label(id)),
            target_id: main_target,
            target_type: main_target.and_then(|id| self.registry.mob_type(id)),
            target_boss: false,
            target_count: counted.len(),
            start_us: first,
            end_us: last,
            players,
        })
    }

    fn row(&self, id: u64, p: &PlayerStats, total: u64, duration_s: f64) -> PlayerRow {
        let mut skills: Vec<SkillRow> = p
            .skills
            .iter()
            .map(|(skill, s)| SkillRow {
                skill: *skill,
                name: None,
                icon_key: None,
                damage: s.damage,
                hits: s.hits,
                max: s.max,
                min: s.min,
                share: ratio(s.damage, p.damage),
                outcomes: s.outcomes,
            })
            .collect();
        skills.sort_by(|a, b| b.damage.cmp(&a.damage).then(a.skill.cmp(&b.skill)));
        let class_code = skills.iter().find_map(|s| classes::class_code_of_skill(s.skill));
        skills.truncate(MAX_SKILLS_PER_ROW);
        PlayerRow {
            id,
            name: self.registry.label(id),
            is_self: self.registry.is_self(id),
            class_code,
            class_name: class_code.and_then(classes::class_name).map(str::to_string),
            damage: p.damage,
            dps: p.damage as f64 / duration_s,
            share: ratio(p.damage, total),
            hits: p.hits,
            max_hit: p.max_hit,
            outcomes: p.outcomes,
            skills,
        }
    }
}

fn ratio(part: u64, whole: u64) -> f64 {
    if whole == 0 {
        0.0
    } else {
        part as f64 / whole as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::{OPCODE_PARTY_ROSTER, OPCODE_PLAYER_SPAWN, OPCODE_SELF_SPAWN, OPCODE_SELF_TICK};
    use crate::protocol::tests::encode_varint;

    const S: u64 = 1_000_000;
    const ME: u64 = 3199;
    const DUMMY: u64 = 38937;

    /// A minimal 0x3804 payload with a damage value (flags 0x04).
    fn hit(target: u64, attacker: u64, skill: u32, dmg: u64) -> Vec<u8> {
        let mut p = encode_varint(target);
        p.extend(0x04u16.to_le_bytes());
        p.extend(encode_varint(attacker));
        p.extend(skill.to_le_bytes());
        p.extend(1u16.to_le_bytes()); // seq
        p.extend(0u32.to_le_bytes()); // unknown_a
        p.extend(1u32.to_le_bytes()); // hit
        p.extend(encode_varint(0)); // unknown_b
        p.extend(encode_varint(dmg));
        p.extend(1u16.to_le_bytes());
        p
    }

    /// A 0x3804 payload with a cast ID, result byte, optional outcome block
    /// (special, position) and optional multi-hit parts.
    fn hit_full(skill: u32, dmg: u64, cast_id: u8, result: u8, block: Option<(u8, u8)>, parts: usize) -> Vec<u8> {
        let mut flags = 0x04u16;
        if block.is_some() {
            flags |= 0x02;
        }
        if parts > 0 {
            flags |= 0x20;
        }
        let mut p = encode_varint(DUMMY);
        p.extend(flags.to_le_bytes());
        p.extend(encode_varint(ME));
        p.extend(skill.to_le_bytes());
        p.extend([cast_id, result]);
        if let Some((special, position)) = block {
            p.extend([special, 0, position]);
        }
        p.extend(0u32.to_le_bytes()); // unknown_a
        p.extend(1u32.to_le_bytes()); // hit
        p.extend(encode_varint(0)); // unknown_b
        p.extend(encode_varint(dmg));
        if parts > 0 {
            p.extend(encode_varint(parts as u64));
            for _ in 0..parts {
                p.extend(encode_varint(dmg / parts as u64));
            }
        }
        p.extend(1u16.to_le_bytes());
        p
    }

    #[test]
    fn counts_casts_crits_positions_and_multi_hits() {
        let mut m = meter_as_me();
        // Cast 7: two hits, one a crit from behind. Cast 8: a perfect from the front.
        // Cast 9: a double strike from the side, a multi-hit with 4 parts.
        m.feed(S, OPCODE_DAMAGE, &hit_full(18_010_340, 1000, 7, 2, Some((0, 2)), 0));
        m.feed(S, OPCODE_DAMAGE, &hit_full(18_010_340, 1700, 7, 3, Some((0, 1)), 0));
        m.feed(2 * S, OPCODE_DAMAGE, &hit_full(18_010_340, 1100, 8, 2, Some((0x04, 2)), 0));
        m.feed(3 * S, OPCODE_DAMAGE, &hit_full(18_010_340, 4000, 9, 2, Some((0x08, 0)), 4));
        let e = m.snapshot().encounter.unwrap();
        let me = &e.players[0];
        let want = Outcomes {
            casts: 3,
            crits: 1,
            back: 1,
            front: 2,
            perfect: 1,
            double: 1,
            multi: 1,
            multi_hits: 7,
        };
        assert_eq!(me.outcomes, want);
        assert_eq!(me.skills[0].outcomes, want);
        assert_eq!((me.skills[0].hits, me.skills[0].min, me.skills[0].max), (4, 1000, 4000));
    }

    #[test]
    fn heal_ticks_are_not_damage() {
        let mut m = meter_as_me();
        m.feed(S, OPCODE_DAMAGE, &hit(DUMMY, ME, 18_010_340, 1000));
        // The dummy "healing" you via your own skill (seen with lifesteal-like effects).
        let mut heal = encode_varint(ME);
        heal.push(0x0b);
        heal.extend(encode_varint(DUMMY));
        heal.extend(encode_varint(42));
        heal.extend(0u32.to_le_bytes());
        heal.extend(encode_varint(783));
        heal.extend(encode_varint(1));
        heal.extend(18_730_002u32.to_le_bytes());
        m.feed(2 * S, OPCODE_DOT, &heal);
        let e = m.snapshot().encounter.unwrap();
        assert_eq!(e.total_damage, 1000);
        assert_eq!(e.players.len(), 1);
    }

    fn spawn(id: u64, name: &str) -> Vec<u8> {
        let mut p = encode_varint(id);
        p.extend([0u8; 5]);
        p.push(name.len() as u8);
        p.extend(name.as_bytes());
        p
    }

    fn meter_as_me() -> Meter {
        let mut m = Meter::default();
        m.feed(0, OPCODE_SELF_SPAWN, &spawn(ME, "NeneneTowawa"));
        m
    }

    #[test]
    fn totals_dps_names_and_classes() {
        let mut m = meter_as_me();
        m.feed(0, OPCODE_PLAYER_SPAWN, &spawn(15772, "Other"));
        m.feed(10 * S, OPCODE_DAMAGE, &hit(DUMMY, ME, 18_010_340, 3000));
        m.feed(12 * S, OPCODE_DAMAGE, &hit(DUMMY, 15772, 12_010_340, 9000));
        m.feed(20 * S, OPCODE_DAMAGE, &hit(DUMMY, ME, 18_040_350, 7000));

        let snap = m.snapshot();
        assert_eq!(snap.self_name.as_deref(), Some("NeneneTowawa"));
        let e = snap.encounter.unwrap();
        assert!(e.active);
        assert_eq!(e.duration_s, 10.0);
        assert_eq!(e.total_damage, 19_000);
        let me = e.players.iter().find(|p| p.is_self).unwrap();
        assert_eq!(me.name, "NeneneTowawa");
        assert_eq!((me.damage, me.dps, me.class_code), (10_000, 1000.0, Some(18)));
        assert_eq!(me.class_name.as_deref(), Some("Chanter"));
        assert_eq!(me.skills[0].skill, 18_040_350);
        assert_eq!(e.players[1].name, "Other");
        assert_eq!(e.players[1].class_code, Some(12));
    }

    #[test]
    fn players_on_other_targets_are_left_out() {
        let mut m = meter_as_me();
        m.feed(S, OPCODE_DAMAGE, &hit(DUMMY, ME, 18_010_340, 1000));
        // Someone at the next dummy.
        m.feed(2 * S, OPCODE_DAMAGE, &hit(55555, 777, 11_010_340, 50_000));
        m.feed(3 * S, OPCODE_DAMAGE, &hit(DUMMY, ME, 18_010_340, 1000));

        let e = m.snapshot().encounter.unwrap();
        assert_eq!(e.players.len(), 1);
        assert_eq!(e.total_damage, 2000);

        m.set_scope(Scope::Everyone);
        let e = m.snapshot().encounter.unwrap();
        assert_eq!(e.total_damage, 52_000);
    }

    /// A `0x9200` party roster with the given names.
    fn roster(names: &[&str]) -> Vec<u8> {
        let mut p = vec![0x08, 0x50, 0x05, 0x00];
        for (i, name) in names.iter().enumerate() {
            p.extend((100 + i as u32).to_le_bytes());
            p.extend([0, 0, 0xc7, 0x09, name.len() as u8]);
            p.extend(name.as_bytes());
            p.extend([0x24, 0, 0, 0]);
        }
        p
    }

    #[test]
    fn party_scope_counts_only_the_party() {
        let mut m = meter_as_me();
        m.feed(0, OPCODE_PLAYER_SPAWN, &spawn(15772, "Pingu"));
        m.feed(0, OPCODE_PLAYER_SPAWN, &spawn(777, "Stranger"));
        m.feed(0, OPCODE_PARTY_ROSTER, &roster(&["Pingu", "NeneneTowawa"]));
        m.set_scope(Scope::Party);
        assert_eq!(m.snapshot().party_size, 2);

        // Our party member starts on a target we never touch: it's still our fight.
        m.feed(S, OPCODE_DAMAGE, &hit(55555, 15772, 12_010_340, 4000));
        m.feed(2 * S, OPCODE_DAMAGE, &hit(55555, 777, 11_010_340, 9000));
        m.feed(3 * S, OPCODE_DAMAGE, &hit(DUMMY, ME, 18_010_340, 1000));
        let e = m.snapshot().encounter.unwrap();
        let names: Vec<_> = e.players.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["Pingu", "NeneneTowawa"]);
        assert_eq!(e.total_damage, 5000);

        // Switching back mid-fight keeps the fight's targets, now with everyone's damage.
        m.set_scope(Scope::Mine);
        let e = m.snapshot().encounter.unwrap();
        assert_eq!((e.players.len(), e.total_damage), (3, 14_000));
    }

    #[test]
    fn late_join_includes_earlier_hits_on_your_target() {
        let mut m = meter_as_me();
        // Another player starts on the boss before we arrive.
        m.feed(S, OPCODE_DAMAGE, &hit(DUMMY, 777, 11_010_340, 5000));
        m.feed(5 * S, OPCODE_DAMAGE, &hit(DUMMY, 777, 11_010_340, 5000));
        assert_eq!(m.snapshot().encounter, None);
        m.feed(6 * S, OPCODE_DAMAGE, &hit(DUMMY, ME, 18_010_340, 1000));

        let e = m.snapshot().encounter.unwrap();
        assert_eq!(e.total_damage, 11_000);
        assert_eq!(e.duration_s, 5.0);
        // And their later hits on it keep counting.
        m.feed(8 * S, OPCODE_DAMAGE, &hit(DUMMY, 777, 11_010_340, 5000));
        assert_eq!(m.snapshot().encounter.unwrap().total_damage, 16_000);
    }

    #[test]
    fn mobs_and_self_effects_are_not_counted() {
        let mut m = meter_as_me();
        // A mob (7-digit skill) hits us: the fight starts, but mobs get no row.
        m.feed(S, OPCODE_DAMAGE, &hit(ME, 18088, 1_216_760, 435));
        m.feed(S, OPCODE_DAMAGE, &hit(ME, ME, 18_160_021, 50));
        assert_eq!(m.snapshot().encounter, None);
        m.feed(2 * S, OPCODE_DAMAGE, &hit(18088, ME, 18_010_340, 900));
        let e = m.snapshot().encounter.unwrap();
        assert_eq!(e.players.len(), 1);
        assert_eq!(e.total_damage, 900);
    }

    #[test]
    fn without_knowing_who_we_are_everything_counts() {
        let mut m = Meter::default();
        m.feed(S, OPCODE_DAMAGE, &hit(1, 10, 18_010_340, 100));
        m.feed(S, OPCODE_DAMAGE, &hit(2, 20, 11_010_340, 200));
        assert_eq!(m.snapshot().encounter.unwrap().total_damage, 300);
        // The self tick arrives: from now on it's our fight only.
        m.feed(2 * S, OPCODE_SELF_TICK, &encode_varint(10));
        assert_eq!(m.snapshot().self_id, Some(10));
    }

    #[test]
    fn strangers_fight_is_dropped_once_we_know_who_we_are() {
        let mut m = Meter::default();
        // Others fight while we don't know our ID yet.
        m.feed(S, OPCODE_DAMAGE, &hit(25002, 5788, 14_010_340, 1000));
        assert!(m.snapshot().encounter.is_some());
        m.feed(2 * S, OPCODE_SELF_TICK, &encode_varint(ME));
        m.feed(3 * S, OPCODE_DAMAGE, &hit(25002, 5788, 14_010_340, 1000));
        assert_eq!(m.snapshot().encounter, None);
        // Our own hit starts a real fight.
        m.feed(4 * S, OPCODE_DAMAGE, &hit(DUMMY, ME, 18_010_340, 500));
        let e = m.snapshot().encounter.unwrap();
        assert_eq!(e.total_damage, 500);
    }

    #[test]
    fn idle_gap_ends_the_encounter_and_next_hit_starts_fresh() {
        let mut m = meter_as_me();
        m.feed(S, OPCODE_DAMAGE, &hit(1, ME, 18_010_340, 100));
        m.feed(2 * S, OPCODE_DAMAGE, &hit(1, ME, 18_010_340, 100));
        m.tick(10 * S);
        assert!(m.snapshot().encounter.unwrap().active);
        m.tick(18 * S);
        let ended = m.snapshot();
        assert!(!ended.encounter.as_ref().unwrap().active);
        assert_eq!(ended.encounter.unwrap().total_damage, 200);
        assert_eq!(ended.encounters_completed, 1);
        let finished = m.take_finished();
        assert_eq!(finished.len(), 1);
        assert_eq!(finished[0].view.total_damage, 200);
        assert_eq!((finished[0].view.start_us, finished[0].view.end_us), (S, 2 * S));
        assert_eq!(finished[0].self_name.as_deref(), Some("NeneneTowawa"));
        assert!(m.take_finished().is_empty());

        m.feed(30 * S, OPCODE_DAMAGE, &hit(2, ME, 18_010_340, 50));
        let e = m.snapshot().encounter.unwrap();
        assert!(e.active);
        assert_eq!(e.total_damage, 50);
    }

    #[test]
    fn dot_ticks_count() {
        let mut m = meter_as_me();
        m.feed(S, OPCODE_DAMAGE, &hit(DUMMY, ME, 18_010_340, 1000));
        // A tick from us on the dummy: target, flags 0x0a, attacker, unknown, u32, damage, skill.
        let mut dot = encode_varint(DUMMY);
        dot.push(0x0a);
        dot.extend(encode_varint(ME));
        dot.extend(encode_varint(1));
        dot.extend(0u32.to_le_bytes());
        dot.extend(encode_varint(250));
        dot.extend(18_730_002u32.to_le_bytes());
        m.feed(2 * S, OPCODE_DOT, &dot);
        let e = m.snapshot().encounter.unwrap();
        assert_eq!(e.total_damage, 1250);
        assert!(e.players[0].skills.iter().any(|s| s.skill == 18_730_002));
    }

    #[test]
    fn reset_clears_display() {
        let mut m = meter_as_me();
        m.feed(S, OPCODE_DAMAGE, &hit(1, ME, 18_010_340, 100));
        m.reset();
        assert_eq!(m.snapshot().encounter, None);
    }
}
