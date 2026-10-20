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

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

use serde::{Deserialize, Serialize};

use crate::buffs::EffectTracker;
use crate::classes;
use crate::damage::{self, DamageEvent, Position};
use crate::entities::{Change, Registry};
use crate::hp;
use crate::protocol::{OPCODE_DAMAGE, OPCODE_DOT};

/// No activity in your fight for this long ends the encounter.
pub const DEFAULT_IDLE_TIMEOUT_US: u64 = 15_000_000;
/// Hits by others are remembered this long, so joining a fight late still shows
/// what happened to that target before you arrived.
const LOOKBACK_US: u64 = 30_000_000;
const LOOKBACK_MAX_EVENTS: usize = 50_000;
/// Player skill IDs are 8 digits (class prefix + skill); mob skills are 7.
const PLAYER_SKILL_MIN: u32 = 10_000_000;
/// Skills summoned creatures (and some mobs) use: 8 digits like a class skill, but not one.
const CREATURE_SKILLS: std::ops::Range<u32> = 16_990_000..17_000_000;
/// How long a self-heal waits for the HP update that shows what it restored. In our
/// captures that update follows it in the same millisecond; anything later is something
/// else (regeneration, a tick).
const SELF_HEAL_WAIT_US: u64 = 50_000;
const MAX_SKILLS_PER_ROW: usize = 12;
/// HP samples are kept this long, so a long boss fight still has its whole HP line.
const HP_KEEP_US: u64 = 60 * 60_000_000;
/// How often (in samples) entities that haven't changed HP for that long are forgotten.
const HP_PRUNE_EVERY: u32 = 4096;

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

impl Hit {
    /// A heal or a mob's hit: an amount and who it went from and to, no outcomes.
    fn flow(ts_us: u64, from: u64, to: u64, skill: u32, amount: u64) -> Self {
        Hit {
            ts_us,
            target: to,
            attacker: from,
            skill,
            damage: amount,
            cast_id: 0,
            is_dot: true,
            crit: false,
            perfect: false,
            double: false,
            position: Position::Other(0),
            hit_count: 1,
        }
    }
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
    /// Every recorded hit in order, for the analysis views.
    hits: Vec<(Hit, bool)>,
    /// Heals during the fight: `attacker` is the healer, `target` who was healed.
    heals: Vec<Hit>,
    /// Damage players took from mobs: `attacker` is the mob, `target` the player.
    taken: Vec<Hit>,
    /// Everyone whose damage was recorded, so a mob's hit on them counts as damage taken.
    attackers: HashSet<u64>,
}

impl Encounter {
    fn new(ts_us: u64) -> Self {
        Self {
            last_activity_us: ts_us,
            targets: HashMap::new(),
            engaged: HashSet::new(),
            last_cast: HashMap::new(),
            hits: Vec::new(),
            heals: Vec::new(),
            taken: Vec::new(),
            attackers: HashSet::new(),
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
        self.attackers.insert(h.attacker);
        self.hits.push((*h, new_cast));
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
    /// Healing done, per healer (most first). Here `damage`, `dps` and `share` mean
    /// healing, healing per second and share of all healing; the skills are the heals.
    #[serde(default)]
    pub healing: Vec<PlayerRow>,
    /// Damage taken from mobs, per player (most first); the skills are what hit them.
    #[serde(default)]
    pub taken: Vec<PlayerRow>,
    #[serde(default)]
    pub total_healing: u64,
    #[serde(default)]
    pub total_taken: u64,
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
    /// Everything the analysis views need, kept with the fight's history.
    pub analysis: Option<Analysis>,
}

/// One counted hit, for the rotation and buff views. Times are milliseconds from the
/// fight's first counted hit.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct TimelineHit {
    pub t: u32,
    pub player: u64,
    pub target: u64,
    pub skill: u32,
    /// The cast it belongs to (see [`TimelineHit::FIRST`]).
    pub cast: u8,
    pub damage: u64,
    /// [`TimelineHit::CRIT`] and the other bits below.
    pub flags: u8,
    /// Hits it stands for: a multi-hit's parts, otherwise 1.
    pub n: u8,
}

impl TimelineHit {
    pub const CRIT: u8 = 0x01;
    pub const BACK: u8 = 0x02;
    pub const FRONT: u8 = 0x04;
    pub const PERFECT: u8 = 0x08;
    pub const DOUBLE: u8 = 0x10;
    pub const DOT: u8 = 0x20;
    /// The first hit of a cast (casts are counted on these).
    pub const FIRST: u8 = 0x40;
}

/// A buff or debuff during the fight, on a player or a counted target. Times are
/// milliseconds from the fight's first counted hit, clipped to the fight.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TimelineEffect {
    pub start: u32,
    pub end: u32,
    /// Already up when the fight started.
    pub before: bool,
    /// Still up when the fight ended (or now, while it's going).
    pub open: bool,
    pub target: u64,
    pub source: u64,
    pub skill: Option<u32>,
    pub effect: u32,
}

/// One fight in full: every player's skills (not capped), every counted hit and the
/// effects on the people involved.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Analysis {
    pub view: EncounterView,
    pub hits: Vec<TimelineHit>,
    pub effects: Vec<TimelineEffect>,
    /// Display names for every entity mentioned (players, targets, effect sources).
    pub names: BTreeMap<u64, String>,
    /// Mob types of the targets, for looking up proper names.
    pub mob_types: BTreeMap<u64, u32>,
    /// HP over the fight for each counted target the game sent HP for.
    #[serde(default)]
    pub target_hp: Vec<TargetHp>,
    /// Each heal in the healing rows: `player` healed `target`.
    #[serde(default)]
    pub heals: Vec<TimelineHit>,
    /// Each hit in the damage-taken rows: `player` is the mob, `target` who took it.
    #[serde(default)]
    pub taken: Vec<TimelineHit>,
}

/// A target's HP during a fight (from `0x8d00`). Mob and dummy HP isn't in damage units:
/// it falls by 4.5 per point of damage, so only the fraction of `max` is meaningful.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TargetHp {
    pub target: u64,
    /// From the mob's spawn, or the highest HP seen.
    pub max: u64,
    /// Oldest first; the first one is at 0 when the target's HP was known before the fight.
    pub samples: Vec<HpPoint>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct HpPoint {
    /// Milliseconds from the fight's first counted hit.
    pub t: u32,
    pub hp: u64,
}

/// Every entity's HP over time (`0x8d00`), and its max HP when known.
#[derive(Debug, Default)]
struct HpLog {
    entities: HashMap<u64, EntityHp>,
    since_prune: u32,
}

#[derive(Debug, Default)]
struct EntityHp {
    /// From the mob's spawn, or the highest HP seen.
    max: u64,
    /// (capture time µs, HP), oldest first.
    samples: VecDeque<(u64, u64)>,
}

impl HpLog {
    /// The entity's latest HP, if we've had one.
    fn last(&self, entity: u64) -> Option<u64> {
        self.entities.get(&entity)?.samples.back().map(|(_, hp)| *hp)
    }

    fn record(&mut self, ts_us: u64, entity: u64, hp: u64, max: Option<u64>) {
        let e = self.entities.entry(entity).or_default();
        e.max = e.max.max(hp).max(max.unwrap_or(0));
        e.samples.push_back((ts_us, hp));
        while e.samples.front().is_some_and(|(t, _)| ts_us.saturating_sub(*t) > HP_KEEP_US) {
            e.samples.pop_front();
        }
        self.since_prune += 1;
        if self.since_prune >= HP_PRUNE_EVERY {
            self.since_prune = 0;
            self.entities
                .retain(|_, e| e.samples.back().is_some_and(|(t, _)| ts_us.saturating_sub(*t) <= HP_KEEP_US));
        }
    }

    /// The entity's HP during `start..=end`, in ms from `start`. The last sample before
    /// the fight is moved to 0, so the line starts where the target stood.
    fn during(&self, entity: u64, start: u64, end: u64) -> Option<TargetHp> {
        let e = self.entities.get(&entity)?;
        let before = e.samples.iter().take_while(|(t, _)| *t < start).last();
        let ms = |us: u64| (us.saturating_sub(start) / 1000).min(u64::from(u32::MAX)) as u32;
        let samples: Vec<HpPoint> = before
            .map(|(_, hp)| HpPoint { t: 0, hp: *hp })
            .into_iter()
            .chain(
                e.samples
                    .iter()
                    .filter(|(t, _)| (start..=end).contains(t))
                    .map(|(t, hp)| HpPoint { t: ms(*t), hp: *hp }),
            )
            .collect();
        (!samples.is_empty()).then_some(TargetHp {
            target: entity,
            max: e.max,
            samples,
        })
    }
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
    hp: HpLog,
    /// Self-heals (or shields: they look the same) waiting for an HP update.
    pending_heals: VecDeque<Hit>,
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
            hp: HpLog::default(),
            pending_heals: VecDeque::new(),
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
        if opcode == hp::OPCODE_HP {
            if let Some(u) = hp::parse_hp(payload) {
                let before = self.hp.last(u.entity);
                self.hp.record(ts_us, u.entity, u.hp, None);
                if let Some(rise) = before.and_then(|b| u.hp.checked_sub(b)).filter(|r| *r > 0) {
                    self.settle_self_heal(ts_us, u.entity, rise);
                }
            }
            return;
        }
        if opcode == hp::OPCODE_MOB_SPAWN {
            if let Some((entity, now, max)) = hp::parse_spawn_hp(payload) {
                self.hp.record(ts_us, entity, now, Some(max));
            }
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
                self.hp = HpLog::default();
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
        if ev.is_heal() {
            // The tick names the entity the effect sits on (a debuff on a mob can heal
            // whoever hits it), so the healer is whoever applied the effect.
            let healer = ev
                .instance
                .and_then(|i| self.effects.source_of(i, [ev.attacker, ev.target]))
                .unwrap_or(ev.attacker);
            if let Some(enc) = self.current.as_mut() {
                enc.heals.push(Hit::flow(ts_us, healer, ev.target, ev.skill, dmg));
            }
            return;
        }
        if ev.is_self() {
            // A heal a player casts on themselves (Healing Light, Blood Absorption…) comes
            // as a hit on themselves, and so does a shield (Steel Barrier). Only a heal
            // raises HP, so it waits for the HP update and counts what that restored.
            // Natural regeneration (Restore HP, 1010000) and items aren't player skills,
            // and mobs and summoned spirits use the 1699xxxx creature skills on themselves.
            let creature = CREATURE_SKILLS.contains(&ev.skill);
            if ev.skill >= PLAYER_SKILL_MIN && !creature && self.is_player(ev.attacker, ev.skill) {
                while self
                    .pending_heals
                    .front()
                    .is_some_and(|h| ts_us.saturating_sub(h.ts_us) > SELF_HEAL_WAIT_US)
                {
                    self.pending_heals.pop_front();
                }
                self.pending_heals
                    .push_back(Hit::flow(ts_us, ev.attacker, ev.target, ev.skill, dmg));
            }
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
        // A mob's hit on a player: damage taken, when it's someone we know is a player.
        let on_player = !player
            && (on_me
                || self.registry.player_name(ev.target).is_some()
                || self.current.as_ref().is_some_and(|e| e.attackers.contains(&ev.target)));
        if !relevant {
            if player {
                self.remember(hit);
            } else if on_player {
                if let Some(enc) = self.current.as_mut() {
                    enc.taken.push(hit);
                }
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
        } else if on_player {
            enc.taken.push(hit);
        }
    }

    /// HP rose by `rise`: if `entity` just healed themselves, that's what the heal did.
    fn settle_self_heal(&mut self, ts_us: u64, entity: u64, rise: u64) {
        let Some(i) = self
            .pending_heals
            .iter()
            .position(|h| h.target == entity && ts_us.saturating_sub(h.ts_us) <= SELF_HEAL_WAIT_US)
        else {
            return;
        };
        let mut heal = self.pending_heals.remove(i).expect("index from position");
        heal.damage = heal.damage.min(rise);
        if let Some(enc) = self.current.as_mut() {
            enc.heals.push(heal);
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
        let idle = self
            .current
            .as_ref()
            .is_some_and(|enc| now_us.saturating_sub(enc.last_activity_us) > self.idle_timeout_us);
        if idle {
            self.end_fight(now_us);
        }
    }

    /// Ends the current encounter now, as if it had gone idle: it goes to the finished
    /// fights and stays on show, and the next hit starts a new one.
    pub fn end_fight(&mut self, now_us: u64) {
        let Some(enc) = &self.current else { return };
        if let Some(view) = self.view(enc, false) {
            self.finished.push(FinishedFight {
                view,
                self_name: self.registry.self_name().map(str::to_string),
                analysis: self.analyse(enc, false, now_us),
            });
        }
        self.last = self.current.take();
        self.completed += 1;
    }

    /// Names players we were told about earlier (e.g. before the meter restarted, in the
    /// same login). Names the game sends always win over these.
    pub fn remember_names(&mut self, names: impl IntoIterator<Item = (u64, String)>) {
        for (id, name) in names {
            self.registry.remember(id, name);
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

    /// Changes whenever the fight shown by [`Meter::analysis`] does: (fights finished, hits
    /// in the current or last fight).
    pub fn analysis_stamp(&self) -> (u32, usize) {
        let events = self
            .current
            .as_ref()
            .or(self.last.as_ref())
            .map_or(0, |e| e.hits.len() + e.heals.len() + e.taken.len());
        (self.completed, events)
    }

    /// The current fight (or the last one) in full, for the analysis window.
    pub fn analysis(&self, now_us: u64) -> Option<Analysis> {
        let (enc, active) = match (&self.current, &self.last) {
            (Some(e), _) => (e, true),
            (None, Some(e)) => (e, false),
            (None, None) => return None,
        };
        self.analyse(enc, active, now_us)
    }

    fn analyse(&self, e: &Encounter, active: bool, now_us: u64) -> Option<Analysis> {
        let view = self.view_capped(e, active, None)?;
        let start = view.start_us;
        let end = if active { now_us.max(view.end_us) } else { view.end_us };
        let ms = |us: u64| (us.saturating_sub(start) / 1000).min(u64::from(u32::MAX)) as u32;
        let counted = self.counted_targets(e);
        let players: HashSet<u64> = view.players.iter().map(|p| p.id).collect();

        let hits: Vec<TimelineHit> = e
            .hits
            .iter()
            .filter(|(h, _)| counted.contains(&h.target) && players.contains(&h.attacker))
            .map(|(h, first)| {
                let mut flags = 0;
                for (on, bit) in [
                    (h.crit, TimelineHit::CRIT),
                    (h.position == Position::Back, TimelineHit::BACK),
                    (h.position == Position::Front, TimelineHit::FRONT),
                    (h.perfect, TimelineHit::PERFECT),
                    (h.double, TimelineHit::DOUBLE),
                    (h.is_dot, TimelineHit::DOT),
                    (*first, TimelineHit::FIRST),
                ] {
                    if on {
                        flags |= bit;
                    }
                }
                TimelineHit {
                    t: ms(h.ts_us),
                    player: h.attacker,
                    target: h.target,
                    skill: h.skill,
                    cast: h.cast_id,
                    damage: h.damage,
                    flags,
                    n: h.hit_count.min(255) as u8,
                }
            })
            .collect();

        // Heals by the healers shown, and hits on the players shown as taking damage.
        let healers: HashSet<u64> = view.healing.iter().map(|p| p.id).collect();
        let takers: HashSet<u64> = view.taken.iter().map(|p| p.id).collect();
        let flow = |h: &Hit| TimelineHit {
            t: ms(h.ts_us),
            player: h.attacker,
            target: h.target,
            skill: h.skill,
            cast: 0,
            damage: h.damage,
            flags: 0,
            n: 1,
        };
        let heals: Vec<TimelineHit> = e.heals.iter().filter(|h| healers.contains(&h.attacker)).map(flow).collect();
        let taken: Vec<TimelineHit> = e.taken.iter().filter(|h| takers.contains(&h.target)).map(flow).collect();

        let involved: HashSet<u64> = players.iter().chain(counted.iter()).copied().collect();
        let effects: Vec<TimelineEffect> = self
            .effects
            .spans(start, end)
            .into_iter()
            .filter(|s| involved.contains(&s.target))
            .map(|s| TimelineEffect {
                start: ms(s.start_us),
                end: ms(s.end_us.unwrap_or(end).min(end)),
                before: s.start_us < start,
                open: s.end_us.is_none_or(|e| e > end),
                target: s.target,
                source: s.source,
                skill: s.skill,
                effect: s.effect,
            })
            .collect();

        let mut names = BTreeMap::new();
        let mut mob_types = BTreeMap::new();
        let mentioned = involved
            .iter()
            .chain(effects.iter().map(|e| &e.source))
            .chain(heals.iter().chain(taken.iter()).flat_map(|h| [&h.player, &h.target]));
        for id in mentioned {
            names.insert(*id, self.registry.label(*id));
            if let Some(t) = self.registry.mob_type(*id) {
                mob_types.insert(*id, t);
            }
        }
        let mut target_hp: Vec<TargetHp> = counted.iter().filter_map(|id| self.hp.during(*id, start, end)).collect();
        target_hp.sort_by_key(|h| h.target);
        Some(Analysis {
            view,
            hits,
            effects,
            names,
            mob_types,
            target_hp,
            heals,
            taken,
        })
    }

    /// The targets a fight counts: the ones you engaged, or every one in Everyone scope.
    fn counted_targets(&self, e: &Encounter) -> HashSet<u64> {
        let everyone = self.scope == Scope::Everyone || self.registry.self_id().is_none();
        e.targets
            .keys()
            .filter(|id| everyone || e.engaged.contains(id))
            .copied()
            .collect()
    }

    fn view(&self, e: &Encounter, active: bool) -> Option<EncounterView> {
        self.view_capped(e, active, Some(MAX_SKILLS_PER_ROW))
    }

    /// The summary of a fight; each player's skill list is cut to `skill_cap` if given.
    fn view_capped(&self, e: &Encounter, active: bool, skill_cap: Option<usize>) -> Option<EncounterView> {
        let counted_ids = self.counted_targets(e);
        let counted: Vec<(&u64, &TargetLog)> = e.targets.iter().filter(|(id, _)| counted_ids.contains(id)).collect();
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
            .map(|(id, p)| self.row(*id, p, total, duration_s, skill_cap))
            .collect();
        players.sort_by(|a, b| b.damage.cmp(&a.damage).then(a.id.cmp(&b.id)));

        let main_target = counted
            .iter()
            .max_by_key(|(id, t)| (t.total, std::cmp::Reverse(**id)))
            .map(|(id, _)| **id);

        // Healing and damage taken for the people in this fight: in party scope your
        // party; otherwise everyone dealing damage here (and you), and anyone healing them.
        let everyone = self.scope == Scope::Everyone || self.registry.self_id().is_none();
        let shown = |id: u64| !party_only || self.registry.in_party(id);
        let in_fight = |id: u64| everyone || per_player.contains_key(&id) || self.registry.is_self(id);
        let (healing, total_healing) = self.flow_rows(
            e.heals
                .iter()
                .filter(|h| shown(h.attacker) && (in_fight(h.attacker) || in_fight(h.target))),
            |h| h.attacker,
            &per_player,
            duration_s,
            skill_cap,
        );
        let (taken, total_taken) = self.flow_rows(
            e.taken.iter().filter(|h| shown(h.target) && in_fight(h.target)),
            |h| h.target,
            &per_player,
            duration_s,
            skill_cap,
        );

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
            healing,
            taken,
            total_healing,
            total_taken,
        })
    }

    /// Rows for heals or damage taken, one per `key` (the healer, or who took the hits),
    /// most first, and their total. Classes come from the player's own damage when there
    /// is some: what hits you says nothing about your class.
    fn flow_rows<'a>(
        &self,
        events: impl Iterator<Item = &'a Hit>,
        key: impl Fn(&Hit) -> u64,
        damage: &HashMap<u64, PlayerStats>,
        duration_s: f64,
        skill_cap: Option<usize>,
    ) -> (Vec<PlayerRow>, u64) {
        let mut per: HashMap<u64, PlayerStats> = HashMap::new();
        for h in events {
            per.entry(key(h)).or_default().add(h, false);
        }
        let total: u64 = per.values().map(|p| p.damage).sum();
        let mut rows: Vec<PlayerRow> = per
            .iter()
            .map(|(id, p)| {
                let mut row = self.row(*id, p, total, duration_s, skill_cap);
                let class = damage
                    .get(id)
                    .and_then(|d| d.skills.keys().find_map(|s| classes::class_code_of_skill(*s)));
                if let Some(code) = class {
                    row.class_code = Some(code);
                    row.class_name = classes::class_name(code).map(str::to_string);
                }
                row
            })
            .collect();
        rows.sort_by(|a, b| b.damage.cmp(&a.damage).then(a.id.cmp(&b.id)));
        (rows, total)
    }

    fn row(&self, id: u64, p: &PlayerStats, total: u64, duration_s: f64, skill_cap: Option<usize>) -> PlayerRow {
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
        if let Some(cap) = skill_cap {
            skills.truncate(cap);
        }
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
    fn analysis_keeps_every_hit_with_its_outcome() {
        let mut m = meter_as_me();
        m.feed(10 * S, OPCODE_DAMAGE, &hit_full(18_010_340, 1000, 7, 2, Some((0, 2)), 0));
        m.feed(10 * S + 300_000, OPCODE_DAMAGE, &hit_full(18_010_340, 1700, 7, 3, Some((0, 1)), 0));
        m.feed(12 * S, OPCODE_DAMAGE, &hit_full(18_020_340, 4000, 8, 2, Some((0x08, 0)), 4));
        // Someone at the next dummy isn't part of your fight.
        m.feed(11 * S, OPCODE_DAMAGE, &hit(55555, 777, 11_010_340, 9000));
        let a = m.analysis(13 * S).unwrap();
        assert_eq!(a.hits.len(), 3);
        assert_eq!((a.hits[0].t, a.hits[1].t, a.hits[2].t), (0, 300, 2000));
        assert_eq!(a.hits[0].flags, TimelineHit::FRONT | TimelineHit::FIRST);
        assert_eq!(a.hits[1].flags, TimelineHit::CRIT | TimelineHit::BACK);
        assert_eq!(a.hits[2].flags, TimelineHit::DOUBLE | TimelineHit::FIRST);
        assert_eq!(a.hits[2].n, 4);
        assert_eq!(a.names.get(&ME).map(String::as_str), Some("NeneneTowawa"));
        // The view isn't capped: every skill is there.
        assert_eq!(a.view.players[0].skills.len(), 2);
    }

    #[test]
    fn analysis_includes_effects_in_the_fight() {
        let mut m = meter_as_me();
        // An 8 s buff on you, applied before the fight and removed during it.
        let mut apply = encode_varint(ME);
        apply.push(1);
        apply.push(0x13);
        apply.extend(encode_varint(2093));
        apply.extend(181_200_101u32.to_le_bytes());
        apply.extend(8000u64.to_le_bytes());
        apply.extend(0u64.to_le_bytes());
        apply.extend(encode_varint(ME));
        apply.push(0x0d);
        apply.extend(18_120_120u32.to_le_bytes());
        m.feed(9 * S, crate::buffs::OPCODE_BUFF_APPLY, &apply);
        m.feed(10 * S, OPCODE_DAMAGE, &hit_full(18_010_340, 1000, 7, 2, None, 0));
        let mut remove = encode_varint(ME);
        remove.extend([1, 0]);
        remove.extend(encode_varint(2093));
        remove.push(1);
        m.feed(15 * S, crate::buffs::OPCODE_BUFF_REMOVE, &remove);
        m.feed(16 * S, OPCODE_DAMAGE, &hit_full(18_010_340, 1000, 8, 2, None, 0));
        let a = m.analysis(16 * S).unwrap();
        assert_eq!(a.effects.len(), 1);
        let e = &a.effects[0];
        assert_eq!((e.start, e.end, e.before, e.open), (0, 5000, true, false));
        assert_eq!((e.target, e.source, e.skill), (ME, ME, Some(18_120_120)));
    }

    /// A real `0x8d00` for the dummy (38937, from 20261020-184342) with its HP replaced.
    fn dummy_hp(hp: u64) -> Vec<u8> {
        let mut p = vec![0x99, 0xb0, 0x02, 0x02, 0x01, 0x00];
        p.extend(hp.to_le_bytes());
        p
    }

    #[test]
    fn analysis_keeps_target_hp_clipped_to_the_fight() {
        let mut m = meter_as_me();
        m.feed(S, hp::OPCODE_HP, &dummy_hp(119_700));
        m.feed(8 * S, hp::OPCODE_HP, &dummy_hp(110_000));
        m.feed(10 * S, OPCODE_DAMAGE, &hit_full(18_010_340, 1000, 7, 2, None, 0));
        m.feed(10 * S + 100_000, hp::OPCODE_HP, &dummy_hp(105_500));
        m.feed(12 * S, OPCODE_DAMAGE, &hit_full(18_010_340, 1000, 8, 2, None, 0));
        m.feed(12 * S + 100_000, hp::OPCODE_HP, &dummy_hp(101_000));
        // Someone else's mob, never part of the fight.
        m.feed(12 * S, hp::OPCODE_HP, &[0xe1, 0x79, 0x02, 0x01, 0x00, 0xfc, 0x3f, 0, 0, 0, 0, 0, 0]);
        m.tick(40 * S);
        // After the fight.
        m.feed(41 * S, hp::OPCODE_HP, &dummy_hp(119_700));

        let a = m.analysis(41 * S).unwrap();
        assert_eq!(a.target_hp.len(), 1);
        let h = &a.target_hp[0];
        assert_eq!((h.target, h.max), (DUMMY, 119_700));
        let points: Vec<(u32, u64)> = h.samples.iter().map(|p| (p.t, p.hp)).collect();
        // The last value before the fight starts the line; nothing after its last hit.
        assert_eq!(points, [(0, 110_000), (100, 105_500)]);
        assert_eq!(m.take_finished()[0].analysis.as_ref().unwrap().target_hp, a.target_hp);
    }

    #[test]
    fn spawn_gives_a_mobs_max_hp() {
        let mut m = meter_as_me();
        // Mob 17151 comes into view hurt (from test D): 76,643 of 119,700.
        let spawn = [
            0xff, 0x85, 0x01, 0x05, 0x20, 0x00, 0x02, 0x9f, 0x24, 0x00, 0x40, 0x02, 0xcc, 0x98, 0xa6, 0xc7, 0xd3, 0xef,
            0xaa, 0xc7, 0x00, 0xd4, 0xe1, 0x46, 0x15, 0xdd, 0x3f, 0x43, 0x70, 0x88, 0x01, 0xe3, 0xd6, 0x04, 0x94, 0xa7,
            0x07, 0x64, 0x00, 0x00, 0x00,
        ];
        m.feed(S, crate::entities::OPCODE_MOB_SPAWN, &spawn);
        assert_eq!(m.registry().mob_type(17151), Some(2_400_002));
        m.feed(2 * S, OPCODE_DAMAGE, &hit(17151, ME, 18_010_340, 7586));
        let a = m.analysis(2 * S).unwrap();
        let h = &a.target_hp[0];
        assert_eq!((h.target, h.max, h.samples[0].hp), (17151, 119_700, 76_643));
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
    fn ending_a_fight_now_finishes_it_and_the_next_hit_starts_fresh() {
        let mut m = meter_as_me();
        m.end_fight(S); // nothing to end
        assert!(m.take_finished().is_empty());
        m.feed(S, OPCODE_DAMAGE, &hit(DUMMY, ME, 18_010_340, 100));
        m.feed(2 * S, OPCODE_DAMAGE, &hit(DUMMY, ME, 18_010_340, 100));
        m.end_fight(3 * S);
        let shown = m.snapshot();
        assert!(!shown.encounter.as_ref().unwrap().active);
        assert_eq!(shown.encounters_completed, 1);
        let finished = m.take_finished();
        assert_eq!(finished.len(), 1);
        assert_eq!(finished[0].view.total_damage, 200);
        assert!(finished[0].analysis.is_some());

        // A second later, well inside the idle timeout, the same target starts a new fight.
        m.feed(4 * S, OPCODE_DAMAGE, &hit(DUMMY, ME, 18_010_340, 50));
        let e = m.snapshot().encounter.unwrap();
        assert!(e.active);
        assert_eq!(e.total_damage, 50);
        assert_eq!(e.start_us, 4 * S);
    }

    #[test]
    fn remembered_names_fill_gaps_but_never_override_the_game() {
        let mut m = meter_as_me();
        m.feed(0, OPCODE_PLAYER_SPAWN, &spawn(15772, "Pingu"));
        m.remember_names([
            (15772, "Old name".to_string()),
            (2787, "Remembered".to_string()),
            (ME, "Not me".to_string()),
        ]);
        let r = m.registry();
        assert_eq!(r.player_name(15772), Some("Pingu"));
        assert_eq!(r.player_name(2787), Some("Remembered"));
        assert_eq!(r.player_name(ME), Some("NeneneTowawa"));
        // A fresh name from the game replaces a remembered one.
        m.feed(S, OPCODE_PLAYER_SPAWN, &spawn(2787, "Renamed"));
        assert_eq!(m.registry().player_name(2787), Some("Renamed"));
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

    /// An HP update (0x8d00): entity, mask 0x02, one u64 stat: 0 (HP).
    fn hp_update(entity: u64, hp: u64) -> Vec<u8> {
        let mut p = encode_varint(entity);
        p.extend([0x02, 1, 0]);
        p.extend(hp.to_le_bytes());
        p
    }

    /// A heal-over-time tick (0x3805, flags 0x0b): target, flags, healer, instance, u32,
    /// amount, the 0x01 varint, skill.
    fn heal(target: u64, healer: u64, skill: u32, amount: u64) -> Vec<u8> {
        let mut p = encode_varint(target);
        p.push(0x0b);
        p.extend(encode_varint(healer));
        p.extend(encode_varint(2093));
        p.extend(0u32.to_le_bytes());
        p.extend(encode_varint(amount));
        p.extend(encode_varint(0));
        p.extend(skill.to_le_bytes());
        p
    }

    #[test]
    fn heals_and_damage_taken_get_their_own_rows() {
        const MOB: u64 = 31865;
        let mut m = meter_as_me();
        m.feed(0, OPCODE_PLAYER_SPAWN, &spawn(15772, "Pingu"));
        m.feed(0, OPCODE_PLAYER_SPAWN, &spawn(2787, "Stranger"));
        // Before the fight: not counted.
        m.feed(0, OPCODE_DOT, &heal(ME, ME, 18_120_120, 50));
        m.feed(S, OPCODE_DAMAGE, &hit(MOB, ME, 18_010_340, 1000));
        // The mob hits you twice and Pingu once (Pingu isn't fighting it: not counted).
        m.feed(2 * S, OPCODE_DAMAGE, &hit(ME, MOB, 100_018, 500));
        m.feed(3 * S, OPCODE_DAMAGE, &hit(ME, MOB, 100_018, 300));
        m.feed(3 * S, OPCODE_DAMAGE, &hit(15772, MOB, 100_018, 999));
        // Your Recuperation, Pingu's chant on you, and a stranger healing a stranger.
        m.feed(4 * S, OPCODE_DOT, &heal(ME, ME, 18_120_120, 976));
        // A heal you cast on yourself comes as a hit on yourself, then HP rises: it counts
        // what HP rose by (600 of 1000; the rest was overheal). A shield looks the same but
        // HP doesn't rise. Regeneration and creature skills don't count.
        m.feed(4 * S, hp::OPCODE_HP, &hp_update(ME, 5000));
        m.feed(4 * S + 10, OPCODE_DAMAGE, &hit(ME, ME, 18_170_010, 1000));
        m.feed(4 * S + 10, hp::OPCODE_HP, &hp_update(ME, 5600));
        m.feed(4 * S + 300_000, OPCODE_DAMAGE, &hit(ME, ME, 18_720_001, 700));
        m.feed(4 * S + 600_000, OPCODE_DAMAGE, &hit(ME, ME, 1_010_000, 500));
        m.feed(4 * S + 600_000, hp::OPCODE_HP, &hp_update(ME, 6100));
        m.feed(4 * S + 700_000, OPCODE_DAMAGE, &hit(MOB, MOB, 16_990_004, 9_999));
        m.feed(5 * S, OPCODE_DOT, &heal(ME, 15772, 18_730_002, 200));
        m.feed(5 * S, OPCODE_DOT, &heal(2787, 2787, 17_090_140, 700));

        let e = m.snapshot().encounter.unwrap();
        assert_eq!(e.total_damage, 1000, "heals and hits on you aren't damage dealt");
        let healing: Vec<(&str, u64)> = e.healing.iter().map(|p| (p.name.as_str(), p.damage)).collect();
        assert_eq!(healing, [("NeneneTowawa", 1576), ("Pingu", 200)]);
        assert_eq!(e.total_healing, 1776);
        assert_eq!(e.healing[1].skills[0].skill, 18_730_002);
        let self_heal = e.healing[0].skills.iter().find(|s| s.skill == 18_170_010).unwrap();
        assert_eq!(self_heal.damage, 600, "what HP rose by, not the 1000 the message said");
        assert!(!e.healing[0].skills.iter().any(|s| s.skill == 18_720_001), "a shield isn't a heal");
        assert_eq!(e.taken.len(), 1);
        assert_eq!((e.taken[0].id, e.taken[0].damage, e.total_taken), (ME, 800, 800));
        assert_eq!(e.taken[0].skills[0].skill, 100_018);
        assert_eq!(e.taken[0].class_code, Some(18), "class from your damage, not the mob's skill");

        let a = m.analysis(6 * S).unwrap();
        assert_eq!(a.heals.len(), 3);
        assert_eq!(a.taken.iter().map(|h| (h.t, h.damage)).collect::<Vec<_>>(), [(1000, 500), (2000, 300)]);
        assert!(a.names.contains_key(&MOB));

        // Everyone: the stranger's heal counts too.
        m.set_scope(Scope::Everyone);
        let e = m.snapshot().encounter.unwrap();
        assert_eq!(e.healing.len(), 3);
    }

    #[test]
    fn reset_clears_display() {
        let mut m = meter_as_me();
        m.feed(S, OPCODE_DAMAGE, &hit(1, ME, 18_010_340, 100));
        m.reset();
        assert_eq!(m.snapshot().encounter, None);
    }
}
