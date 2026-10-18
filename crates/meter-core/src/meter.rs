//! The DPS meter: groups player damage into encounters and summarises them.
//!
//! Feed it every game message in order; ask for a [`Snapshot`] whenever the display
//! refreshes. Pure bookkeeping — no threads, no I/O — so it runs the same on live
//! traffic and on recorded sessions.
//!
//! An encounter is *your* fight: the targets you hit, and everyone's damage on those
//! targets (including hits from just before you joined). Players fighting other things
//! nearby don't count, unless the scope is set to [`Scope::Everyone`].

use std::collections::{HashMap, HashSet, VecDeque};

use serde::Serialize;

use crate::damage::{self, DamageEvent};
use crate::entities::{Change, Registry};
use crate::protocol::OPCODE_DAMAGE;

/// No activity in your fight for this long ends the encounter.
pub const DEFAULT_IDLE_TIMEOUT_US: u64 = 15_000_000;
/// Hits by others are remembered this long, so joining a fight late still shows
/// what happened to that target before you arrived.
const LOOKBACK_US: u64 = 30_000_000;
const LOOKBACK_MAX_EVENTS: usize = 50_000;
/// Player skill IDs are 8 digits (class prefix + skill); mob skills are 7.
const PLAYER_SKILL_MIN: u32 = 10_000_000;
const MAX_SKILLS_PER_ROW: usize = 12;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Scope {
    /// Targets you hit, and everyone's damage on them.
    Mine,
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
}

#[derive(Debug, Default, Clone)]
struct SkillStats {
    hits: u64,
    damage: u64,
    max: u64,
}

#[derive(Debug, Default, Clone)]
struct PlayerStats {
    damage: u64,
    hits: u64,
    max_hit: u64,
    skills: HashMap<u32, SkillStats>,
}

impl PlayerStats {
    fn add(&mut self, h: &Hit) {
        self.damage += h.damage;
        self.hits += 1;
        self.max_hit = self.max_hit.max(h.damage);
        let s = self.skills.entry(h.skill).or_default();
        s.damage += h.damage;
        s.hits += 1;
        s.max = s.max.max(h.damage);
    }

    fn merge(&mut self, other: &PlayerStats) {
        self.damage += other.damage;
        self.hits += other.hits;
        self.max_hit = self.max_hit.max(other.max_hit);
        for (id, s) in &other.skills {
            let t = self.skills.entry(*id).or_default();
            t.damage += s.damage;
            t.hits += s.hits;
            t.max = t.max.max(s.max);
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
}

impl Encounter {
    fn new(ts_us: u64) -> Self {
        Self {
            last_activity_us: ts_us,
            targets: HashMap::new(),
            engaged: HashSet::new(),
        }
    }

    fn record(&mut self, h: &Hit) {
        let t = self.targets.entry(h.target).or_insert(TargetLog {
            first_us: h.ts_us,
            last_us: h.ts_us,
            total: 0,
            players: HashMap::new(),
        });
        t.first_us = t.first_us.min(h.ts_us);
        t.last_us = t.last_us.max(h.ts_us);
        t.total += h.damage;
        t.players.entry(h.attacker).or_default().add(h);
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SkillRow {
    pub skill: u32,
    /// Display name, filled in by the app from its skill table (None = unknown).
    pub name: Option<String>,
    /// Which skill ID's icon to show (a variant may share its base skill's icon).
    pub icon_key: Option<u32>,
    pub damage: u64,
    pub hits: u64,
    pub max: u64,
    /// Fraction of this player's damage.
    pub share: f64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PlayerRow {
    pub id: u64,
    pub name: String,
    pub is_self: bool,
    /// First two digits of the player's skill IDs; seems to identify the class.
    pub class_code: Option<u32>,
    pub damage: u64,
    pub dps: f64,
    /// Fraction of everyone's damage.
    pub share: f64,
    pub hits: u64,
    pub max_hit: u64,
    pub skills: Vec<SkillRow>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct EncounterView {
    /// Still in combat (not yet idle).
    pub active: bool,
    pub duration_s: f64,
    pub total_damage: u64,
    pub total_dps: f64,
    /// The most-damaged target.
    pub target: Option<String>,
    pub target_count: usize,
    /// Highest damage first.
    pub players: Vec<PlayerRow>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Snapshot {
    pub self_name: Option<String>,
    pub self_id: Option<u64>,
    pub scope: Scope,
    /// The current encounter, or the last one once it has ended.
    pub encounter: Option<EncounterView>,
    pub encounters_completed: u32,
}

pub struct Meter {
    registry: Registry,
    scope: Scope,
    idle_timeout_us: u64,
    current: Option<Encounter>,
    last: Option<Encounter>,
    /// Recent player hits outside any encounter, for late joins.
    lookback: VecDeque<Hit>,
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
            scope: Scope::Mine,
            idle_timeout_us,
            current: None,
            last: None,
            lookback: VecDeque::new(),
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

    /// Feeds one game message.
    pub fn feed(&mut self, ts_us: u64, opcode: u16, payload: &[u8]) {
        if opcode != OPCODE_DAMAGE {
            if let Some(Change::Relogin { .. }) = self.registry.observe(opcode, payload) {
                // Entity IDs from the previous login mean nothing now.
                self.current = None;
                self.last = None;
                self.lookback.clear();
            }
            return;
        }
        let Some(ev) = damage::parse(payload) else { return };
        self.tick(ts_us);
        self.on_damage(ts_us, &ev);
    }

    fn on_damage(&mut self, ts_us: u64, ev: &DamageEvent) {
        let Some(dmg) = ev.damage else { return };
        if ev.is_self() {
            return;
        }
        let me = self.registry.self_id();
        let by_me = me == Some(ev.attacker);
        let on_me = me == Some(ev.target);
        let player = self.is_player(ev.attacker, ev.skill);
        // Without knowing who we are yet, every player hit counts.
        let everyone = self.scope == Scope::Everyone || me.is_none();

        let hit = Hit {
            ts_us,
            target: ev.target,
            attacker: ev.attacker,
            skill: ev.skill,
            damage: dmg,
        };

        // Does this start or continue your fight?
        let engaged = self.current.as_ref().is_some_and(|e| e.engaged.contains(&ev.target));
        let relevant = by_me || on_me || engaged || (everyone && player);
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
        if by_me || (self.scope == Scope::Everyone && player) {
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

        let mut per_player: HashMap<u64, PlayerStats> = HashMap::new();
        for (_, t) in &counted {
            for (id, p) in &t.players {
                per_player.entry(*id).or_default().merge(p);
            }
        }
        let total: u64 = per_player.values().map(|p| p.damage).sum();

        let mut players: Vec<PlayerRow> = per_player
            .iter()
            .map(|(id, p)| self.row(*id, p, total, duration_s))
            .collect();
        players.sort_by(|a, b| b.damage.cmp(&a.damage).then(a.id.cmp(&b.id)));

        let target = counted
            .iter()
            .max_by_key(|(id, t)| (t.total, std::cmp::Reverse(**id)))
            .map(|(id, _)| self.registry.label(**id));

        Some(EncounterView {
            active,
            duration_s,
            total_damage: total,
            total_dps: total as f64 / duration_s,
            target,
            target_count: counted.len(),
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
                share: ratio(s.damage, p.damage),
            })
            .collect();
        skills.sort_by(|a, b| b.damage.cmp(&a.damage).then(a.skill.cmp(&b.skill)));
        let class_code = skills
            .iter()
            .find(|s| s.skill >= PLAYER_SKILL_MIN)
            .map(|s| s.skill / 1_000_000);
        skills.truncate(MAX_SKILLS_PER_ROW);
        PlayerRow {
            id,
            name: self.registry.label(id),
            is_self: self.registry.is_self(id),
            class_code,
            damage: p.damage,
            dps: p.damage as f64 / duration_s,
            share: ratio(p.damage, total),
            hits: p.hits,
            max_hit: p.max_hit,
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
    use crate::entities::{OPCODE_PLAYER_SPAWN, OPCODE_SELF_SPAWN, OPCODE_SELF_TICK};
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

        m.feed(30 * S, OPCODE_DAMAGE, &hit(2, ME, 18_010_340, 50));
        let e = m.snapshot().encounter.unwrap();
        assert!(e.active);
        assert_eq!(e.total_damage, 50);
    }

    #[test]
    fn reset_clears_display() {
        let mut m = meter_as_me();
        m.feed(S, OPCODE_DAMAGE, &hit(1, ME, 18_010_340, 100));
        m.reset();
        assert_eq!(m.snapshot().encounter, None);
    }
}
