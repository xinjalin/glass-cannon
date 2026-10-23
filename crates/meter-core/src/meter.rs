//! The DPS meter: groups player damage into encounters and summarises them.
//!
//! Feed it every game message in order; ask for a [`Snapshot`] whenever the display
//! refreshes. Pure bookkeeping — no threads, no I/O — so it runs the same on live
//! traffic and on recorded sessions.
//!
//! An encounter is *your* fight: the targets you hit, and only your damage on them.
//! [`Scope::Party`] widens it to your party or group (the targets any of you hit, and all
//! of your damage), and [`Scope::Everyone`] to all player damage nearby. Everyone's hits on
//! your targets are kept (including hits from just before you joined), so switching the
//! scope mid-fight shows them.

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

use serde::{Deserialize, Serialize};

use crate::buffs::EffectTracker;
use crate::classes;
use crate::damage::{self, DamageEvent, Position};
use crate::dungeon::{Instance, Maps, OPCODE_MAP_ENTERED};
use crate::entities::{parse_death, Change, Death, PartyMember, Registry, OPCODE_DEATH};
use crate::cast::{parse_cast, parse_cast_start, parse_cast_stop, OPCODE_CAST, OPCODE_CAST_START, OPCODE_CAST_STOP};
use crate::cooldowns::{CastCooldowns, CasterCooldown, CooldownView, Cooldowns};
use crate::pvp::{PvpSession, PvpTracker};
use crate::fieldboss::{parse_world_map, FieldBoss, OPCODE_WORLD_MAP};
use crate::state::{self, Stagger, REMOVED_DIED};
use crate::hp;
use crate::protocol::{OPCODE_DAMAGE, OPCODE_DOT};

/// While a boss in the fight is alive and in combat, quiet this long ends it instead.
const BOSS_IDLE_TIMEOUT_US: u64 = 90_000_000;
/// Hits on something that died this recently don't start a new fight (damage over time
/// still ticking on a corpse).
const AFTER_DEATH_US: u64 = 10_000_000;
/// Deaths remembered this long, then forgotten (entity IDs get reused).
const DEATHS_KEPT_US: u64 = 120_000_000;
/// A death recap lists this many of the last hits.
const RECAP_HITS: usize = 12;
/// Pings kept for the median.
const PINGS_KEPT: usize = 6;
/// Damage over this long sets how fast a target is going down (its time to kill).
const KILL_RATE_US: u64 = 10_000_000;

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
    /// Targets you hit, and only your damage on them.
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
    /// Their first and last hit.
    span: Option<(u64, u64)>,
}

impl PlayerStats {
    fn add(&mut self, h: &Hit, new_cast: bool) {
        self.span = Some(widen(self.span, (h.ts_us, h.ts_us)));
        let mut one = Outcomes::default();
        one.add(h, new_cast);
        self.damage += h.damage;
        self.hits += 1;
        self.max_hit = self.max_hit.max(h.damage);
        self.outcomes.merge(&one);
        self.skills.entry(h.skill).or_default().add(h.damage, &one, 1, h.damage, h.damage);
    }

    fn merge(&mut self, other: &PlayerStats) {
        if let Some(span) = other.span {
            self.span = Some(widen(self.span, span));
        }
        self.damage += other.damage;
        self.hits += other.hits;
        self.max_hit = self.max_hit.max(other.max_hit);
        self.outcomes.merge(&other.outcomes);
        for (id, s) in &other.skills {
            self.skills.entry(*id).or_default().add(s.damage, &s.outcomes, s.hits, s.max, s.min);
        }
    }
}

fn widen(span: Option<(u64, u64)>, (first, last): (u64, u64)) -> (u64, u64) {
    span.map_or((first, last), |(a, b)| (a.min(first), b.max(last)))
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
    /// The map it's on and when you entered it.
    instance: Option<(u32, u64)>,
    /// Players who died during it.
    deaths: Vec<(u64, Death)>,
    /// Players who came back to life during it: (when, who).
    revived: Vec<(u64, u64)>,
    /// Its bosses' casts, in order.
    boss_casts: Vec<BossCast>,
    /// Your party's players (you too) who did something in it, as the party was then: so
    /// it still shows them after the party breaks up.
    party: HashSet<u64>,
    end: Option<FightEnd>,
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
            instance: None,
            deaths: Vec::new(),
            revived: Vec::new(),
            boss_casts: Vec::new(),
            party: HashSet::new(),
            end: None,
        }
    }

    /// When its first hit landed.
    fn start_us(&self) -> u64 {
        self.targets.values().map(|t| t.first_us).min().unwrap_or(self.last_activity_us)
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
    /// In damage taken: the mob type that used it (names a mob skill the data doesn't).
    #[serde(default)]
    pub source_type: Option<u32>,
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
    /// The map the fight was on, and when you entered it (which stay on that map: a
    /// dungeon run). Unknown in fights saved before this was read.
    #[serde(default)]
    pub map: Option<u32>,
    #[serde(default)]
    pub instance_us: Option<u64>,
    /// Damage taken from mobs, per player (most first); the skills are what hit them.
    #[serde(default)]
    pub taken: Vec<PlayerRow>,
    #[serde(default)]
    pub total_healing: u64,
    #[serde(default)]
    pub total_taken: u64,
    /// How it ended (None while it's going on, and in fights saved before this was read).
    #[serde(default)]
    pub end: Option<FightEnd>,
    /// Deaths of the players shown, in order.
    #[serde(default)]
    pub deaths: Vec<DeathView>,
    /// When the boss goes berserk (µs since the Unix epoch), if the game said.
    #[serde(default)]
    pub berserk_us: Option<u64>,
    /// The main target's HP now and in full, when known (live fights).
    #[serde(default)]
    pub target_hp: Option<u64>,
    #[serde(default)]
    pub target_max_hp: Option<u64>,
    /// Its bosses' casts, in order.
    #[serde(default)]
    pub boss_casts: Vec<BossCastView>,
}

/// The bosses' casts in the fight shown, for the cast timeline.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct BossCasts {
    pub active: bool,
    pub start_us: u64,
    /// Seconds into the fight now (its length, once over).
    pub now_s: f64,
    pub target_type: Option<u32>,
    pub casts: Vec<BossCastView>,
}

/// A boss's cast during a fight.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct BossCast {
    at_us: u64,
    caster: u64,
    skill: u32,
    /// Its cast time (0: none, it went off at once).
    cast_ms: u32,
    went_off: bool,
    /// Stopped before it went off.
    stopped: bool,
}

/// A boss's cast, for the cast timeline.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BossCastView {
    /// Seconds into the fight it began.
    pub at_s: f64,
    pub boss: String,
    pub boss_type: Option<u32>,
    pub skill: u32,
    /// Filled in by the app from its skill table.
    #[serde(default)]
    pub skill_name: Option<String>,
    pub cast_ms: u32,
    pub stopped: bool,
}

/// What you're targeting (or fighting), for the target frame.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TargetView {
    pub entity: u64,
    /// Its label: a player's name, or a mob's "#id" (the app names mobs by `mob_type`).
    pub name: String,
    pub mob_type: Option<u32>,
    /// Set by the app from its mob data.
    pub boss: bool,
    pub hp: Option<u64>,
    pub max_hp: Option<u64>,
    /// When it goes berserk (µs since 1970).
    pub berserk_us: Option<u64>,
    /// Who it's attacking, and whether that's you.
    pub attacking: Option<String>,
    pub attacking_you: bool,
    pub stagger: Option<Stagger>,
    /// At the rate it lost HP over the last 10 s.
    pub time_to_kill_s: Option<f64>,
    /// You picked it (otherwise it's the fight's main target).
    pub selected: bool,
    /// What it's casting, if the cast has a cast time and hasn't gone off yet.
    pub cast: Option<CastBar>,
}

/// Tickets and Abyss artifacts the game sent (see [`crate::contents`]).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Contents {
    /// `tickets` is every ticket (a login's list), not just those that changed.
    pub full_list: bool,
    pub tickets: Vec<crate::contents::Ticket>,
    pub sieges: Vec<crate::contents::Siege>,
    pub artifacts: Vec<crate::contents::Artifact>,
}

/// A cast under way.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct CastBar {
    pub skill: u32,
    /// Filled in by the app from its skill table.
    pub skill_name: Option<String>,
    pub cast_ms: u32,
    pub remaining_ms: u32,
}

/// Why a fight ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FightEnd {
    /// Every boss in it died; or, with no boss, everyone left combat with the targets dead.
    Kill,
    /// A boss healed back to full, or everyone left combat with a boss alive.
    Wipe,
    /// Nobody did anything for a while.
    Idle,
    /// You changed map.
    Zone,
    /// Ended by hand.
    Manual,
}

/// A player's death, with what led to it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DeathView {
    pub entity: u64,
    pub name: String,
    /// Seconds into the fight.
    pub at_s: f64,
    /// A player killer's name or a mob's label (the app names mobs by `killer_type`).
    pub killer: Option<String>,
    pub killer_type: Option<u32>,
    pub skill: u32,
    /// Filled in by the app from its skill table.
    pub skill_name: Option<String>,
    /// From the last time they were at full HP in this fight (or its start) to the death.
    pub time_to_death_s: f64,
    /// Damage taken in that time: direct hits, and damage over time.
    pub direct: u64,
    pub over_time: u64,
    /// Healing received in that time.
    pub healed: u64,
    /// The last hits taken, oldest first.
    pub hits: Vec<RecapHit>,
    /// The last heal received in the fight.
    pub last_heal: Option<RecapHit>,
    /// How long until they were brought back, if it was during the fight.
    #[serde(default)]
    pub revived_after_s: Option<f64>,
}

/// One hit (or heal) in a death recap.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RecapHit {
    /// Seconds before the death.
    pub before_s: f64,
    pub skill: u32,
    pub skill_name: Option<String>,
    pub from: String,
    pub from_type: Option<u32>,
    pub amount: u64,
    pub dot: bool,
    /// HP right after it, when known.
    pub hp: Option<u64>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Snapshot {
    pub self_name: Option<String>,
    pub self_id: Option<u64>,
    pub scope: Scope,
    /// People in your party or group, you included (0 when on your own).
    pub party_size: usize,
    /// Your party or group, you included, in roster order (empty when on your own).
    pub party: Vec<PartyView>,
    /// What you're targeting, or fighting.
    pub target: Option<TargetView>,
    /// You're in combat (the game says so, or a fight is going on).
    pub in_combat: bool,
    /// The current encounter, or the last one once it has ended.
    pub encounter: Option<EncounterView>,
    pub encounters_completed: u32,
}

/// A party member as the meter shows them.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PartyView {
    #[serde(flatten)]
    pub member: PartyMember,
    pub is_self: bool,
    /// From the skills we've seen them use (unknown until they fight).
    pub class_code: Option<u32>,
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
    /// How long the game said it lasts (0: no set length, or a fight saved before this).
    #[serde(default)]
    pub duration_ms: u32,
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

    /// From the spawn, or the highest HP seen.
    fn max(&self, entity: u64) -> Option<u64> {
        self.entities.get(&entity).map(|e| e.max).filter(|m| *m > 0)
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
    /// The map you're on (a dungeon run, when it's a dungeon's).
    maps: Maps,
    /// Stays on a map that started or changed since [`Meter::take_instances`].
    instance_changes: Vec<Instance>,
    /// Each player's class, from their skills.
    classes: HashMap<u64, u32>,
    /// Mob types that are bosses (from the app's mob data).
    bosses: HashSet<u32>,
    /// Who's in combat, as the game last said, and when.
    combat: HashMap<u64, (bool, u64)>,
    /// When things died (pruned after [`DEATHS_KEPT_US`]).
    died: HashMap<u64, u64>,
    /// When bosses go berserk (ms since 1970).
    berserk: HashMap<u64, u64>,
    /// The game's pings coming back: (arrival, when the client sent it on its clock).
    pings: VecDeque<(u64, i64)>,
    /// Your skills' cooldowns.
    cooldowns: Cooldowns,
    /// Everyone's long cooldowns, for your party's.
    cast_cooldowns: CastCooldowns,
    /// Fights with players outside your party.
    pvp: PvpTracker,
    /// Your target, what each mob is attacking, and their stagger gauges.
    selected: Option<u64>,
    aggro: HashMap<u64, u64>,
    stagger: HashMap<u64, Stagger>,
    /// Field bosses read from a world map since the last [`Meter::take_field_bosses`].
    field_bosses: Vec<FieldBoss>,
    /// Tickets and Abyss artifacts read since the last [`Meter::take_contents`].
    contents: Contents,
    /// Casts under way: caster -> (skill, when it started, cast time in ms).
    casting: HashMap<u64, (u32, u64, u32)>,
    /// The newest message's time.
    latest_us: u64,
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
            maps: Maps::default(),
            instance_changes: Vec::new(),
            classes: HashMap::new(),
            bosses: HashSet::new(),
            combat: HashMap::new(),
            died: HashMap::new(),
            berserk: HashMap::new(),
            pings: VecDeque::new(),
            cooldowns: Cooldowns::default(),
            cast_cooldowns: CastCooldowns::default(),
            pvp: PvpTracker::default(),
            selected: None,
            aggro: HashMap::new(),
            stagger: HashMap::new(),
            field_bosses: Vec::new(),
            contents: Contents::default(),
            casting: HashMap::new(),
            latest_us: 0,
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

    /// Which mob types are bosses: a boss's death ends the fight, and while one is alive
    /// and in combat the fight waits longer before going idle.
    pub fn set_boss_types(&mut self, types: impl IntoIterator<Item = u32>) {
        self.bosses = types.into_iter().collect();
    }

    /// The game's latest pings: when each came back (capture time, µs) and when the client
    /// sent it, on its clock (see [`state::CLIENT_CLOCK_BASE_MS`]).
    pub fn pings(&self) -> impl Iterator<Item = &(u64, i64)> {
        self.pings.iter()
    }

    /// Your skills that aren't ready at `now_us`, soonest first.
    pub fn cooldowns(&self, now_us: u64) -> Vec<CooldownView> {
        self.cooldowns.active(now_us)
    }

    /// How many of your party (you included) took part in the fight shown (the one going,
    /// or the last): a fight from before the party broke up still counts as a party's.
    pub fn shown_party_size(&self) -> usize {
        self.current.as_ref().or(self.last.as_ref()).map_or(0, |e| e.party.len())
    }

    /// The bosses' casts in the fight shown (the one going, or the last): whether it's
    /// still going, how far in it is, and the main target's mob type.
    pub fn boss_casts(&self) -> Option<BossCasts> {
        // The fight going, or the last one while the new one has nothing to show yet.
        let (e, active, view) = [(self.current.as_ref(), true), (self.last.as_ref(), false)]
            .into_iter()
            .find_map(|(e, active)| e.and_then(|e| self.view(e, active).map(|v| (e, active, v))))?;
        let start = e.start_us();
        Some(BossCasts {
            active,
            start_us: start,
            now_s: if active { self.latest_us.saturating_sub(start) as f64 / 1e6 } else { view.duration_s },
            target_type: view.target_type,
            casts: view.boss_casts,
        })
    }

    /// Your skills (base ids), once the game has sent their list.
    pub fn your_skills(&self) -> &[u32] {
        self.cooldowns.known()
    }

    /// Your party's (you included) long cooldowns seen this session: ready or time left.
    pub fn party_cooldowns(&self, now_us: u64) -> Vec<CasterCooldown> {
        let r = &self.registry;
        self.cast_cooldowns.of(now_us, |e| r.is_self(e) || (!r.party().is_empty() && r.player_name(e).is_some() && r.in_party(e)))
    }

    /// The PvP session going on, if any.
    pub fn pvp(&self) -> Option<PvpSession> {
        self.pvp.current()
    }

    /// PvP sessions that ended since the last call.
    pub fn take_pvp(&mut self) -> Vec<PvpSession> {
        self.pvp.take_finished()
    }

    /// Field bosses from world maps opened since the last call.
    pub fn take_field_bosses(&mut self) -> Vec<FieldBoss> {
        std::mem::take(&mut self.field_bosses)
    }

    /// Tickets (a full list replaces what was known) and Abyss artifacts read since the
    /// last call.
    pub fn take_contents(&mut self) -> Contents {
        std::mem::take(&mut self.contents)
    }

    /// Stays on a map (dungeon runs) that began or changed since the last call, oldest
    /// first; the same stay can come more than once as it starts and completes.
    pub fn take_instances(&mut self) -> Vec<Instance> {
        std::mem::take(&mut self.instance_changes)
    }

    /// The map you're on now, if the game has said.
    pub fn instance(&self) -> Option<&Instance> {
        self.maps.current()
    }

    /// Fights that have ended since the last call, oldest first.
    pub fn take_finished(&mut self) -> Vec<FinishedFight> {
        std::mem::take(&mut self.finished)
    }

    /// Buff and debuff spans seen so far.
    pub fn effects(&self) -> &EffectTracker {
        &self.effects
    }

    /// Feeds one game message.
    pub fn feed(&mut self, ts_us: u64, opcode: u16, payload: &[u8]) {
        self.latest_us = self.latest_us.max(ts_us);
        self.cooldowns.feed(ts_us, opcode, payload, self.registry.self_id());
        if opcode == OPCODE_CAST {
            if let Some(c) = parse_cast(payload) {
                // The cast went off, so its cast time is over.
                if self.casting.get(&c.caster).is_some_and(|(skill, _, _)| *skill == c.skill) {
                    self.casting.remove(&c.caster);
                }
                if c.skill < PLAYER_SKILL_MIN && self.is_boss(c.caster) {
                    if let Some(enc) = self.current.as_mut() {
                        match enc.boss_casts.iter_mut().rev().find(|b| b.caster == c.caster && b.skill == c.skill && b.cast_ms > 0 && !b.went_off && !b.stopped) {
                            Some(started) => started.went_off = true,
                            None => enc.boss_casts.push(BossCast { at_us: ts_us, caster: c.caster, skill: c.skill, cast_ms: 0, went_off: true, stopped: false }),
                        }
                    }
                }
                if c.skill >= PLAYER_SKILL_MIN {
                    self.cast_cooldowns.cast(ts_us, c.caster, c.skill, c.cooldown_ms);
                }
            }
        }
        if self.effects.feed(ts_us, opcode, payload) {
            return;
        }
        if opcode == hp::OPCODE_HP {
            if let Some(u) = hp::parse_hp(payload) {
                let before = self.hp.last(u.entity);
                self.hp.record(ts_us, u.entity, u.hp, None);
                if let Some(rise) = before.and_then(|b| u.hp.checked_sub(b)).filter(|r| *r > 0) {
                    self.settle_self_heal(ts_us, u.entity, rise);
                    if self.boss_reset(u.entity, u.hp) {
                        self.finish(FightEnd::Wipe, ts_us);
                    }
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
            if opcode == OPCODE_MAP_ENTERED && self.current.is_some() && !self.maps.moving_within(payload) {
                self.finish(FightEnd::Zone, ts_us);
            }
            if let Some(i) = self.maps.feed(ts_us, opcode, payload) {
                self.instance_changes.push(i);
            }
            match opcode {
                state::OPCODE_COMBAT_STATE => {
                    if let Some(s) = state::parse_combat_state(payload) {
                        self.combat.insert(s.entity, (s.in_combat, ts_us));
                        if let Some(b) = s.berserk_ms {
                            self.berserk.insert(s.entity, b);
                        }
                        self.check_end(ts_us);
                    }
                }
                state::OPCODE_NPC_REMOVED => {
                    if let Some((entity, REMOVED_DIED)) = state::parse_npc_removed(payload) {
                        self.died.entry(entity).or_insert(ts_us);
                        self.check_end(ts_us);
                    }
                }
                state::OPCODE_PING => {
                    if let Some(sent) = state::parse_ping(payload) {
                        self.pings.push_back((ts_us, sent));
                        if self.pings.len() > PINGS_KEPT {
                            self.pings.pop_front();
                        }
                    }
                }
                OPCODE_DEATH => {
                    if let Some(d) = parse_death(payload) {
                        self.on_death(ts_us, d);
                    }
                }
                state::OPCODE_YOUR_TARGET => {
                    if let Some(t) = state::parse_your_target(payload) {
                        self.selected = t;
                    }
                }
                state::OPCODE_MOB_TARGET => {
                    if let Some((mob, target)) = state::parse_mob_target(payload) {
                        self.aggro.insert(mob, target);
                    }
                }
                state::OPCODE_STAGGER => {
                    if let Some((mob, s)) = state::parse_stagger(payload) {
                        self.stagger.insert(mob, s);
                    }
                }
                OPCODE_WORLD_MAP => {
                    if let Some(bosses) = parse_world_map(payload) {
                        self.field_bosses.extend(bosses);
                    }
                }
                crate::contents::OPCODE_TICKETS => {
                    if let Some(list) = crate::contents::parse_tickets(payload) {
                        self.contents.full_list = true;
                        self.contents.tickets = list;
                    }
                }
                crate::contents::OPCODE_TICKET_UPDATE => {
                    if let Some(t) = crate::contents::parse_ticket_update(payload) {
                        self.contents.tickets.retain(|x| x.id != t.id);
                        self.contents.tickets.push(t);
                    }
                }
                crate::contents::OPCODE_ABYSS_MAP | crate::contents::OPCODE_ABYSS_ENTERED => {
                    if let Some((sieges, artifacts)) = crate::contents::parse_abyss(opcode, payload) {
                        self.contents.sieges.extend(sieges);
                        self.contents.artifacts.extend(artifacts);
                    }
                }
                OPCODE_CAST_START => {
                    if let Some(c) = parse_cast_start(payload).filter(|c| c.cast_ms > 0) {
                        self.casting.insert(c.caster, (c.skill, ts_us, c.cast_ms));
                        if c.skill < PLAYER_SKILL_MIN && self.is_boss(c.caster) {
                            if let Some(enc) = self.current.as_mut() {
                                enc.boss_casts.push(BossCast {
                                    at_us: ts_us,
                                    caster: c.caster,
                                    skill: c.skill,
                                    cast_ms: c.cast_ms,
                                    went_off: false,
                                    stopped: false,
                                });
                            }
                        }
                    }
                }
                OPCODE_CAST_STOP => {
                    if let Some((caster, skill)) = parse_cast_stop(payload) {
                        if self.casting.get(&caster).is_some_and(|(s, _, _)| *s == skill) {
                            self.casting.remove(&caster);
                        }
                        if let Some(enc) = self.current.as_mut() {
                            if let Some(b) = enc.boss_casts.iter_mut().rev().find(|b| b.caster == caster && b.skill == skill && !b.went_off && !b.stopped) {
                                b.stopped = true;
                            }
                        }
                    }
                }
                state::OPCODE_REVIVED => {
                    if let Some(entity) = state::parse_revived(payload) {
                        self.died.remove(&entity);
                        if let Some(enc) = self.current.as_mut() {
                            enc.revived.push((ts_us, entity));
                        }
                    }
                }
                state::OPCODE_MIDBOSS_KILLED => {
                    if let Some(kind) = state::parse_midboss_killed(payload) {
                        self.on_boss_killed(ts_us, kind);
                    }
                }
                _ => {}
            }
            if let Some(Change::Relogin { .. }) = self.registry.observe(opcode, payload) {
                // Entity IDs from the previous login mean nothing now.
                self.current = None;
                self.last = None;
                self.lookback.clear();
                self.effects.clear();
                self.hp = HpLog::default();
                self.cooldowns.clear();
                self.cast_cooldowns.clear();
                self.casting.clear();
                self.pvp.end();
            }
            return;
        }
        let Some(ev) = damage::parse(payload) else { return };
        self.tick(ts_us);
        self.on_damage(ts_us, &ev);
    }

    fn on_damage(&mut self, ts_us: u64, ev: &DamageEvent) {
        self.registry.observe_hit(ev.attacker, ev.target, ev.skill);
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
        let attacker = ev.attacker;
        // A summon's hits count for its owner. Hits between a player and their own
        // summons (a spirit's echo on its owner, Spirit Communion) aren't damage.
        let by = self.registry.credited(attacker);
        let on = self.registry.credited(ev.target);
        if by == on {
            return;
        }
        self.record_pvp(ts_us, by, on, ev.skill, dmg);
        // Between members of your party (a Chanter's effect on an ally, say): not damage.
        if !self.registry.party().is_empty() && self.registry.in_party(by) && self.registry.in_party(on) {
            return;
        }
        let me = self.registry.self_id();
        let by_me = me == Some(by);
        let on_me = me == Some(ev.target);
        // In party scope, a party member's hit makes the target yours too.
        let by_party = self.scope == Scope::Party && self.registry.in_party(by);
        let on_party = self.scope == Scope::Party && self.registry.in_party(ev.target);
        // Spirits attack with mob-style skill IDs, so they're players by their owner.
        let player = by != attacker || self.is_player(attacker, ev.skill);
        // Without knowing who we are yet, every player hit counts.
        let everyone = self.scope == Scope::Everyone || me.is_none();

        let hit = Hit {
            ts_us,
            target: ev.target,
            attacker,
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
        // A player's skill ticking on a player (a shield or mantra) isn't a mob's damage.
        let on_player = !player
            && classes::class_code_of_skill(ev.skill).is_none()
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

        if player {
            if let Some(code) = classes::class_code_of_skill(ev.skill) {
                self.classes.insert(by, code);
            }
        }
        // Nor does a 1-damage hit (touching an object in a dungeon), a mob's hit (it counts
        // as damage taken once your side is fighting), or a hit on a corpse. A corpse doesn't
        // join a fight it wasn't in either: in Fire Temple a hit landing just after Black
        // Smoke Murute died made it look alive in the next fight.
        let corpse = self.died.get(&ev.target).is_some_and(|d| ts_us.saturating_sub(*d) < AFTER_DEATH_US);
        match &self.current {
            None if dmg <= 1 || corpse || !player => return,
            Some(enc) if corpse && !enc.targets.contains_key(&ev.target) => return,
            _ => {}
        }
        if self.current.is_none() {
            let mut enc = Encounter::new(ts_us);
            enc.instance = self.maps.current().map(|i| (i.map, i.entered_us));
            // Bring in recent hits so a late join shows the whole fight on its targets.
            for h in self.lookback.drain(..) {
                enc.record(&h);
            }
            self.current = Some(enc);
        }
        let party_member = player && self.registry.in_party(by);
        let enc = self.current.as_mut().expect("encounter exists");
        enc.last_activity_us = enc.last_activity_us.max(ts_us);
        // Only your own hits make a target yours. Before we know who we are, everything
        // is shown, but none of it sticks once we do.
        if by_me || by_party || (self.scope == Scope::Everyone && player) {
            enc.engaged.insert(ev.target);
        }
        if player {
            enc.record(&hit);
            if party_member {
                enc.party.insert(by);
            }
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
        self.pvp.tick(now_us);
        let Some(enc) = &self.current else { return };
        let quiet = now_us.saturating_sub(enc.last_activity_us);
        if quiet <= self.idle_timeout_us {
            return;
        }
        // A boss still fighting (between phases, or out of reach) gets longer.
        if !self.boss_fighting(enc) || quiet > BOSS_IDLE_TIMEOUT_US {
            self.finish(FightEnd::Idle, now_us);
        }
    }

    /// A hit between you and a player outside your party goes in the PvP session.
    fn record_pvp(&mut self, ts_us: u64, by: u64, on: u64, skill: u32, damage: u64) {
        let Some(me) = self.registry.self_id() else { return };
        let enemy = |id: u64| id != me && self.registry.player_name(id).is_some() && !self.registry.in_party(id);
        let (other, outgoing) = match (by == me, on == me) {
            (true, _) if enemy(on) => (on, true),
            (_, true) if enemy(by) => (by, false),
            _ => return,
        };
        let class = self.classes.get(&other).copied().or_else(|| (!outgoing).then(|| classes::class_code_of_skill(skill)).flatten());
        if let Some(code) = class {
            self.classes.insert(other, code);
        }
        let name = self.registry.label(other);
        self.pvp.hit(ts_us, other, name, class, damage, outgoing);
    }

    fn is_boss(&self, entity: u64) -> bool {
        self.registry.mob_type(entity).is_some_and(|t| self.bosses.contains(&t))
    }

    /// Something died: a player's death goes in the fight's recap, and it may end the fight.
    fn on_death(&mut self, ts_us: u64, d: Death) {
        let me = self.registry.self_id();
        let by_you = me.is_some() && d.killer.map(|k| self.registry.credited(k)) == me;
        let killer = d.killer_name.clone().or_else(|| d.killer.map(|k| self.registry.label(k)));
        self.pvp.death(ts_us, d.entity, me == Some(d.entity), by_you, killer);
        self.died.insert(d.entity, ts_us);
        self.died.retain(|_, at| ts_us.saturating_sub(*at) < DEATHS_KEPT_US);
        // Players the game has named (an unnamed "player" may be a mob we missed the spawn of).
        if self.registry.is_self(d.entity) || self.registry.player_name(d.entity).is_some() {
            if let Some(enc) = self.current.as_mut() {
                enc.deaths.push((ts_us, d));
            }
            return;
        }
        self.check_end(ts_us);
    }

    /// The game says a boss of this type was killed: in case its own death was missed (it
    /// died out of view), it counts as dead for ending the fight.
    fn on_boss_killed(&mut self, ts_us: u64, kind: u32) {
        let Some(enc) = &self.current else { return };
        let killed: Vec<u64> =
            enc.targets.keys().copied().filter(|t| self.registry.mob_type(*t) == Some(kind)).collect();
        for t in killed {
            self.died.entry(t).or_insert(ts_us);
        }
        self.check_end(ts_us);
    }

    /// Whether you and your party in this fight have all left combat since it started:
    /// None until the game has said for at least one of you.
    fn party_out_of_combat(&self, enc: &Encounter) -> Option<bool> {
        let start = enc.start_us();
        let mut known = false;
        for id in enc.attackers.iter().copied().chain(self.registry.self_id()) {
            if !self.registry.in_party(id) {
                continue;
            }
            match self.combat.get(&id) {
                Some((true, _)) => return Some(false),
                Some((false, at)) if *at >= start => known = true,
                _ => {}
            }
        }
        known.then_some(true)
    }

    /// A boss of the fight is alive and still in combat.
    fn boss_fighting(&self, enc: &Encounter) -> bool {
        self.counted_targets(enc)
            .into_iter()
            .any(|t| self.is_boss(t) && !self.died.contains_key(&t) && self.combat.get(&t).is_some_and(|(c, _)| *c))
    }

    /// Ends the fight when the game says it's over: its bosses died, or everyone left combat
    /// (unless a boss is still in combat: in Fire Temple the whole party leaves combat at
    /// Kromede's Desire's phase changes while it stays in).
    fn check_end(&mut self, ts_us: u64) {
        let Some(enc) = &self.current else { return };
        let targets = self.counted_targets(enc);
        let dead = |t: &u64| self.died.get(t).is_some_and(|d| enc.targets.get(t).is_some_and(|l| *d >= l.first_us));
        let bosses: Vec<u64> = targets.iter().copied().filter(|t| self.is_boss(*t)).collect();
        let out = self.party_out_of_combat(enc) == Some(true);
        let end = if !bosses.is_empty() && bosses.iter().all(dead) {
            Some(FightEnd::Kill)
        } else if out && !bosses.is_empty() {
            (!self.boss_fighting(enc)).then_some(FightEnd::Wipe)
        } else if out {
            Some(if targets.iter().all(dead) { FightEnd::Kill } else { FightEnd::Idle })
        } else {
            None
        };
        if let Some(end) = end {
            self.finish(end, ts_us);
        }
    }

    /// A boss in the fight that took damage is back to full HP: it reset.
    fn boss_reset(&self, entity: u64, hp: u64) -> bool {
        let Some(enc) = &self.current else { return false };
        let max = self.hp.max(entity);
        max.is_some_and(|m| hp >= m)
            && self.is_boss(entity)
            && !self.died.contains_key(&entity)
            && enc.targets.get(&entity).is_some_and(|t| t.total > 0)
            && self.counted_targets(enc).contains(&entity)
    }

    fn finish(&mut self, end: FightEnd, now_us: u64) {
        let Some(enc) = self.current.as_mut() else { return };
        enc.end = Some(end);
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

    /// Ends the current encounter now, as if it had gone idle: it goes to the finished
    /// fights and stays on show, and the next hit starts a new one.
    pub fn end_fight(&mut self, now_us: u64) {
        self.finish(FightEnd::Manual, now_us);
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
            party: self.party_view(),
            target: self.target_view(),
            in_combat: self.current.is_some()
                || self.registry.self_id().is_some_and(|me| self.combat.get(&me).is_some_and(|(c, _)| *c)),
            encounter: enc.and_then(|e| self.view(e, active)),
            encounters_completed: self.completed,
        }
    }

    /// Your party with what we know of each member's class.
    fn party_view(&self) -> Vec<PartyView> {
        let me = self.registry.self_name();
        self.registry
            .party()
            .iter()
            .map(|m| {
                let entity = self.registry.entity_named(&m.name);
                PartyView {
                    member: m.clone(),
                    is_self: me == Some(m.name.as_str()),
                    class_code: entity.and_then(|id| self.classes.get(&id).copied()),
                }
            })
            .collect()
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
            .filter(|(h, _)| {
                let by = self.registry.credited(h.attacker);
                counted.contains(&h.target) && players.contains(&by) && by != self.registry.credited(h.target)
            })
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
                    player: self.registry.credited(h.attacker),
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
                duration_ms: s.duration_ms,
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

        // Whose damage counts: yours in mine scope, your party's in party scope (just yours
        // when on your own), everyone's in everyone scope or before we know who we are.
        let me = self.registry.self_id();
        let shown = |id: u64| match self.scope {
            _ if me.is_none() => true,
            Scope::Mine => me == Some(id),
            Scope::Party => self.registry.in_party(id) || e.party.contains(&id),
            Scope::Everyone => true,
        };
        let mut per_player: HashMap<u64, PlayerStats> = HashMap::new();
        for (target, t) in &counted {
            for (id, p) in &t.players {
                // Summons are folded into their owner (also hits from before we knew).
                let by = self.registry.credited(*id);
                if by == self.registry.credited(**target) {
                    continue;
                }
                if shown(by) {
                    per_player.entry(by).or_default().merge(p);
                }
            }
        }
        // The fight lasts from the first to the last counted hit, so time someone else spent
        // on the target before you arrived doesn't lower your DPS.
        let (first, last) = per_player.values().filter_map(|p| p.span).reduce(|a, b| widen(Some(a), b))?;
        let duration_s = ((last - first) as f64 / 1e6).max(1.0);
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

        // Healing and damage taken for the people whose damage counts: in everyone scope,
        // everyone dealing damage here, and anyone healing them.
        let everyone = self.scope == Scope::Everyone || me.is_none();
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
        let (mut taken, total_taken) = self.flow_rows(
            e.taken.iter().filter(|h| shown(h.target) && in_fight(h.target)),
            |h| h.target,
            &per_player,
            duration_s,
            skill_cap,
        );
        // Which mob used each skill that hit them (for naming skills the data doesn't).
        for row in &mut taken {
            for s in &mut row.skills {
                s.source_type = e
                    .taken
                    .iter()
                    .find(|h| h.target == row.id && h.skill == s.skill)
                    .and_then(|h| self.registry.mob_type(h.attacker));
            }
        }

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
            map: e.instance.map(|(map, _)| map),
            instance_us: e.instance.map(|(_, at)| at),
            healing,
            taken,
            total_healing,
            total_taken,
            end: e.end,
            deaths: e.deaths.iter().filter(|(_, d)| shown(d.entity)).map(|(at, d)| self.death_view(e, *at, d)).collect(),
            berserk_us: main_target.and_then(|t| self.berserk.get(&t)).map(|ms| ms * 1000),
            target_hp: main_target.filter(|_| active).and_then(|t| self.hp.last(t)),
            target_max_hp: main_target.filter(|_| active).and_then(|t| self.hp.max(t)),
            boss_casts: e
                .boss_casts
                .iter()
                .map(|c| BossCastView {
                    at_s: c.at_us.saturating_sub(first) as f64 / 1e6,
                    boss: self.registry.label(c.caster),
                    boss_type: self.registry.mob_type(c.caster),
                    skill: c.skill,
                    skill_name: None,
                    cast_ms: c.cast_ms,
                    stopped: c.stopped,
                })
                .collect(),
        })
    }

    /// Your target if you've picked one that's alive, else the live fight's main target.
    fn target_view(&self) -> Option<TargetView> {
        let alive = |t: &u64| !self.died.contains_key(t) && !self.registry.is_self(*t);
        let picked = self.selected.filter(alive);
        let fighting = self.current.as_ref().and_then(|e| {
            e.targets.iter().filter(|(t, _)| alive(t)).max_by_key(|(id, t)| (t.total, std::cmp::Reverse(**id))).map(|(id, _)| *id)
        });
        let entity = picked.or(fighting)?;
        let hp = self.hp.last(entity);
        // Its time to kill: its HP over the damage it took in the last 10 s.
        let time_to_kill_s = self.current.as_ref().and_then(|e| {
            let last = e.hits.last()?.0.ts_us;
            let recent: u64 = e
                .hits
                .iter()
                .rev()
                .take_while(|(h, _)| last.saturating_sub(h.ts_us) <= KILL_RATE_US)
                .filter(|(h, _)| h.target == entity)
                .map(|(h, _)| h.damage)
                .sum();
            let rate = recent as f64 / (KILL_RATE_US as f64 / 1e6);
            (rate > 0.0).then(|| hp.unwrap_or(0) as f64 / rate).filter(|_| hp.is_some())
        });
        let attacking = self.aggro.get(&entity).copied().filter(|t| *t != 0);
        Some(TargetView {
            entity,
            name: self.registry.label(entity),
            mob_type: self.registry.mob_type(entity),
            boss: false,
            hp,
            max_hp: self.hp.max(entity),
            berserk_us: self.berserk.get(&entity).map(|ms| ms * 1000),
            attacking: attacking.map(|t| self.registry.label(t)),
            attacking_you: attacking.is_some() && attacking == self.registry.self_id(),
            stagger: self.stagger.get(&entity).copied(),
            time_to_kill_s,
            selected: picked.is_some(),
            cast: self.casting.get(&entity).and_then(|&(skill, start, cast_ms)| {
                let end = start + u64::from(cast_ms) * 1000;
                (end > self.latest_us).then(|| CastBar {
                    skill,
                    skill_name: None,
                    cast_ms,
                    remaining_ms: ((end - self.latest_us) / 1000) as u32,
                })
            }),
        })
    }

    /// What led to a player's death: the hits and heals since they were last at full HP.
    fn death_view(&self, e: &Encounter, at: u64, d: &Death) -> DeathView {
        let start = e.start_us();
        let hp = self.hp.entities.get(&d.entity);
        let full = hp
            .and_then(|h| h.samples.iter().rev().find(|(t, v)| (start..at).contains(t) && *v >= h.max).map(|(t, _)| *t))
            .unwrap_or(start);
        let hp_after = |ts: u64| hp.and_then(|h| h.samples.iter().find(|(t, _)| *t >= ts && *t <= at).map(|(_, v)| *v));
        let secs = |us: u64| at.saturating_sub(us) as f64 / 1e6;
        let recap = |h: &Hit| RecapHit {
            before_s: secs(h.ts_us),
            skill: h.skill,
            skill_name: None,
            from: self.registry.label(h.attacker),
            from_type: self.registry.mob_type(h.attacker),
            amount: h.damage,
            dot: h.is_dot,
            hp: hp_after(h.ts_us),
        };
        let taken: Vec<&Hit> = e.taken.iter().filter(|h| h.target == d.entity && (full..=at).contains(&h.ts_us)).collect();
        let healed = e.heals.iter().filter(|h| h.target == d.entity && (full..=at).contains(&h.ts_us)).map(|h| h.damage).sum();
        DeathView {
            entity: d.entity,
            name: self.registry.label(d.entity),
            at_s: at.saturating_sub(start) as f64 / 1e6,
            killer: d.killer_name.clone().or_else(|| d.killer.map(|k| self.registry.label(k))),
            killer_type: d.killer_type.or_else(|| d.killer.and_then(|k| self.registry.mob_type(k))),
            skill: d.skill,
            skill_name: None,
            time_to_death_s: secs(full),
            direct: taken.iter().filter(|h| !h.is_dot).map(|h| h.damage).sum(),
            over_time: taken.iter().filter(|h| h.is_dot).map(|h| h.damage).sum(),
            healed,
            hits: taken.iter().rev().take(RECAP_HITS).rev().map(|h| recap(h)).collect(),
            last_heal: e.heals.iter().rev().find(|h| h.target == d.entity && h.ts_us <= at).map(recap),
            revived_after_s: e
                .revived
                .iter()
                .find(|(t, who)| *who == d.entity && *t >= at)
                .map(|(t, _)| (t - at) as f64 / 1e6),
        }
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
                source_type: None,
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
    use crate::entities::{OPCODE_PARTY_ROSTER, OPCODE_PLAYER_SPAWN, OPCODE_SELF_SPAWN, OPCODE_SELF_STATS};
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
        // Mine: only your damage.
        assert_eq!(e.duration_s, 10.0);
        assert_eq!((e.total_damage, e.players.len()), (10_000, 1));
        let me = &e.players[0];
        assert_eq!(me.name, "NeneneTowawa");
        assert_eq!((me.damage, me.dps, me.class_code), (10_000, 1000.0, Some(18)));
        assert_eq!(me.class_name.as_deref(), Some("Chanter"));
        assert_eq!(me.skills[0].skill, 18_040_350);

        m.set_scope(Scope::Everyone);
        let e = m.snapshot().encounter.unwrap();
        assert_eq!(e.total_damage, 19_000);
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

        // Mine: just yours, on the same targets.
        m.set_scope(Scope::Mine);
        let e = m.snapshot().encounter.unwrap();
        assert_eq!((e.players.len(), e.total_damage), (1, 1000));
        // Everyone: all the damage on them.
        m.set_scope(Scope::Everyone);
        let e = m.snapshot().encounter.unwrap();
        assert_eq!((e.players.len(), e.total_damage), (3, 14_000));
    }

    #[test]
    fn late_join_keeps_earlier_hits_on_your_target() {
        let mut m = meter_as_me();
        // Another player starts on the boss before we arrive.
        m.feed(S, OPCODE_DAMAGE, &hit(DUMMY, 777, 11_010_340, 5000));
        m.feed(5 * S, OPCODE_DAMAGE, &hit(DUMMY, 777, 11_010_340, 5000));
        assert_eq!(m.snapshot().encounter, None);
        m.feed(6 * S, OPCODE_DAMAGE, &hit(DUMMY, ME, 18_010_340, 1000));

        // Mine: just your hit, and their time on it before you came doesn't count.
        let e = m.snapshot().encounter.unwrap();
        assert_eq!((e.total_damage, e.duration_s), (1000, 1.0));
        // Everyone: their earlier hits are there too.
        m.set_scope(Scope::Everyone);
        let e = m.snapshot().encounter.unwrap();
        assert_eq!((e.total_damage, e.duration_s), (11_000, 5.0));
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
        m.feed(2 * S, OPCODE_SELF_STATS, &encode_varint(10));
        assert_eq!(m.snapshot().self_id, Some(10));
    }

    #[test]
    fn strangers_fight_is_dropped_once_we_know_who_we_are() {
        let mut m = Meter::default();
        // Others fight while we don't know our ID yet.
        m.feed(S, OPCODE_DAMAGE, &hit(25002, 5788, 14_010_340, 1000));
        assert!(m.snapshot().encounter.is_some());
        m.feed(2 * S, OPCODE_SELF_STATS, &encode_varint(ME));
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
        assert_eq!(healing, [("NeneneTowawa", 1576)], "mine: only your healing");
        assert_eq!(e.total_healing, 1576);
        let self_heal = e.healing[0].skills.iter().find(|s| s.skill == 18_170_010).unwrap();
        assert_eq!(self_heal.damage, 600, "what HP rose by, not the 1000 the message said");
        assert!(!e.healing[0].skills.iter().any(|s| s.skill == 18_720_001), "a shield isn't a heal");
        assert_eq!(e.taken.len(), 1);
        assert_eq!((e.taken[0].id, e.taken[0].damage, e.total_taken), (ME, 800, 800));
        assert_eq!(e.taken[0].skills[0].skill, 100_018);
        assert_eq!(e.taken[0].class_code, Some(18), "class from your damage, not the mob's skill");

        let a = m.analysis(6 * S).unwrap();
        assert_eq!(a.heals.len(), 2, "mine: your two heals, not Pingu's");
        assert_eq!(a.taken.iter().map(|h| (h.t, h.damage)).collect::<Vec<_>>(), [(1000, 500), (2000, 300)]);
        assert!(a.names.contains_key(&MOB));

        // Everyone: Pingu's chant on you and the stranger's heal count too.
        m.set_scope(Scope::Everyone);
        let e = m.snapshot().encounter.unwrap();
        assert_eq!(e.healing.len(), 3);
        let pingu = e.healing.iter().find(|p| p.name == "Pingu").unwrap();
        assert_eq!((pingu.damage, pingu.skills[0].skill), (200, 18_730_002));
    }

    /// Mob 17151 (type 2400002) coming into view with 76,643 of 119,700 HP (from test D),
    /// on a meter that takes its type as a boss's.
    fn meter_with_boss() -> Meter {
        let mut m = meter_as_me();
        m.set_boss_types([2_400_002]);
        let spawn = [
            0xff, 0x85, 0x01, 0x05, 0x20, 0x00, 0x02, 0x9f, 0x24, 0x00, 0x40, 0x02, 0xcc, 0x98, 0xa6, 0xc7, 0xd3, 0xef,
            0xaa, 0xc7, 0x00, 0xd4, 0xe1, 0x46, 0x15, 0xdd, 0x3f, 0x43, 0x70, 0x88, 0x01, 0xe3, 0xd6, 0x04, 0x94, 0xa7,
            0x07, 0x64, 0x00, 0x00, 0x00,
        ];
        m.feed(0, crate::entities::OPCODE_MOB_SPAWN, &spawn);
        m
    }
    const BOSS: u64 = 17151;

    /// A death (0x8d04): entity, skill, killer, server, no name or legion, then the rest.
    fn death(entity: u64, skill: u32, killer: u64) -> Vec<u8> {
        let mut p = encode_varint(entity);
        p.extend(skill.to_le_bytes());
        p.extend(encode_varint(killer));
        p.extend([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 2]);
        p
    }

    /// In or out of combat (0x8d21).
    fn combat(entity: u64, in_combat: bool) -> Vec<u8> {
        let mut p = encode_varint(entity);
        p.extend([0, u8::from(in_combat)]);
        p
    }

    fn ends(m: &mut Meter) -> Vec<FightEnd> {
        m.take_finished().into_iter().filter_map(|f| f.view.end).collect()
    }

    #[test]
    fn a_bosss_death_ends_the_fight() {
        let mut m = meter_with_boss();
        m.feed(S, OPCODE_DAMAGE, &hit(BOSS, ME, 18_010_340, 5000));
        m.feed(3 * S, OPCODE_DEATH, &death(BOSS, 18_010_340, ME));
        assert_eq!(ends(&mut m), [FightEnd::Kill]);
        // Damage still ticking on the corpse doesn't start another fight.
        m.feed(4 * S, OPCODE_DAMAGE, &hit(BOSS, ME, 18_010_340, 300));
        assert!(m.snapshot().encounter.is_some_and(|e| !e.active));
    }

    #[test]
    fn a_mob_hitting_you_after_a_kill_starts_nothing() {
        let mut m = meter_with_boss();
        m.feed(S, OPCODE_DAMAGE, &hit(BOSS, ME, 18_010_340, 5000));
        m.feed(3 * S, OPCODE_DEATH, &death(BOSS, 18_010_340, ME));
        assert_eq!(ends(&mut m), [FightEnd::Kill]);
        // Fire Temple: a mob hits you, then a late hit lands on the dead boss.
        m.feed(3 * S + 100_000, OPCODE_DAMAGE, &hit(ME, DUMMY, 100_018, 1935));
        m.feed(3 * S + 200_000, OPCODE_DAMAGE, &hit(BOSS, ME, 18_010_340, 895));
        assert!(m.snapshot().encounter.is_some_and(|e| !e.active));
        m.tick(60 * S);
        assert!(ends(&mut m).is_empty());
    }

    #[test]
    fn a_boss_back_to_full_hp_is_a_wipe() {
        let mut m = meter_with_boss();
        m.feed(S, OPCODE_DAMAGE, &hit(BOSS, ME, 18_010_340, 5000));
        m.feed(2 * S, hp::OPCODE_HP, &hp_update(BOSS, 71_643));
        assert!(ends(&mut m).is_empty());
        m.feed(5 * S, hp::OPCODE_HP, &hp_update(BOSS, 119_700));
        assert_eq!(ends(&mut m), [FightEnd::Wipe]);
    }

    #[test]
    fn a_boss_still_fighting_gets_longer() {
        let mut m = meter_with_boss();
        m.feed(S, state::OPCODE_COMBAT_STATE, &combat(BOSS, true));
        m.feed(S, OPCODE_DAMAGE, &hit(BOSS, ME, 18_010_340, 5000));
        // Out of reach for 30 s (a phase change): still the same fight.
        m.tick(31 * S);
        assert!(m.snapshot().encounter.unwrap().active);
        m.tick(100 * S);
        assert_eq!(ends(&mut m), [FightEnd::Idle]);
    }

    #[test]
    fn a_party_out_of_combat_while_the_boss_stays_in_is_no_wipe() {
        let mut m = meter_with_boss();
        m.feed(0, state::OPCODE_COMBAT_STATE, &combat(ME, true));
        m.feed(0, state::OPCODE_COMBAT_STATE, &combat(BOSS, true));
        m.feed(S, OPCODE_DAMAGE, &hit(BOSS, ME, 18_010_340, 5000));
        // A phase change: you're out of combat, the boss isn't.
        m.feed(2 * S, state::OPCODE_COMBAT_STATE, &combat(ME, false));
        assert!(ends(&mut m).is_empty());
        m.feed(8 * S, state::OPCODE_COMBAT_STATE, &combat(ME, true));
        m.feed(9 * S, OPCODE_DAMAGE, &hit(BOSS, ME, 18_010_340, 5000));
        // It left combat too: a wipe.
        m.feed(10 * S, state::OPCODE_COMBAT_STATE, &combat(BOSS, false));
        m.feed(10 * S, state::OPCODE_COMBAT_STATE, &combat(ME, false));
        assert_eq!(ends(&mut m), [FightEnd::Wipe]);
    }

    #[test]
    fn trash_ends_when_you_leave_combat() {
        let mut m = meter_as_me();
        m.feed(S, state::OPCODE_COMBAT_STATE, &combat(ME, true));
        m.feed(S, OPCODE_DAMAGE, &hit(DUMMY, ME, 18_010_340, 5000));
        m.feed(2 * S, OPCODE_DEATH, &death(DUMMY, 18_010_340, ME));
        // The mob died, but you're still fighting (more may be coming).
        assert!(ends(&mut m).is_empty());
        m.feed(5 * S, state::OPCODE_COMBAT_STATE, &combat(ME, false));
        assert_eq!(ends(&mut m), [FightEnd::Kill]);
    }

    #[test]
    fn changing_map_ends_the_fight() {
        let mut m = meter_as_me();
        m.feed(S, OPCODE_DAMAGE, &hit(DUMMY, ME, 18_010_340, 5000));
        m.feed(2 * S, OPCODE_MAP_ENTERED, &[0x56, 0x04, 0, 0, 1]);
        assert_eq!(ends(&mut m), [FightEnd::Zone]);
        // Moved on inside a dungeon (Fire Temple sends its map again): the same fight.
        let fire_temple = [0xd6, 0x27, 0x09, 0x00, 0x00, 0x01];
        m.feed(3 * S, OPCODE_MAP_ENTERED, &fire_temple);
        m.feed(4 * S, OPCODE_DAMAGE, &hit(DUMMY, ME, 18_010_340, 5000));
        m.feed(5 * S, OPCODE_MAP_ENTERED, &fire_temple);
        assert!(ends(&mut m).is_empty());
        assert!(m.snapshot().encounter.unwrap().active);
    }

    #[test]
    fn deaths_are_recapped_and_party_hits_on_each_other_dont_count() {
        let mut m = meter_with_boss();
        m.feed(0, OPCODE_PLAYER_SPAWN, &spawn(15772, "Pingu"));
        m.feed(0, OPCODE_PARTY_ROSTER, &roster(&["Pingu", "NeneneTowawa"]));
        m.set_scope(Scope::Party);
        m.feed(S, OPCODE_DAMAGE, &hit(BOSS, ME, 18_010_340, 5000));
        // Pingu's effect on you isn't damage.
        m.feed(S, OPCODE_DAMAGE, &hit(ME, 15772, 12_010_340, 777));
        m.feed(S, hp::OPCODE_HP, &hp_update(15772, 9000));
        m.feed(2 * S, OPCODE_DAMAGE, &hit(15772, BOSS, 100_018, 4000));
        m.feed(2 * S, hp::OPCODE_HP, &hp_update(15772, 5000));
        m.feed(3 * S, OPCODE_DOT, &heal(15772, ME, 18_120_120, 1000));
        m.feed(4 * S, OPCODE_DAMAGE, &hit(15772, BOSS, 100_019, 6000));
        m.feed(4 * S, hp::OPCODE_HP, &hp_update(15772, 0));
        m.feed(4 * S, OPCODE_DEATH, &death(15772, 100_019, BOSS));
        let e = m.snapshot().encounter.unwrap();
        assert!(e.active, "a party member's death doesn't end the fight");
        assert_eq!(e.total_damage, 5000);
        let d = &e.deaths[0];
        assert_eq!((d.name.as_str(), d.skill, d.at_s), ("Pingu", 100_019, 3.0));
        assert_eq!((d.time_to_death_s, d.direct, d.healed), (3.0, 10_000, 1000));
        let hits: Vec<_> = d.hits.iter().map(|h| (h.before_s, h.amount, h.hp)).collect();
        assert_eq!(hits, [(2.0, 4000, Some(5000)), (0.0, 6000, Some(0))]);
        assert_eq!(d.last_heal.as_ref().map(|h| h.amount), Some(1000));
        assert_eq!(d.revived_after_s, None);
        // Brought back 6 s later: alive again, and the recap says when.
        m.feed(10 * S, state::OPCODE_REVIVED, &encode_varint(15772));
        assert!(!m.died.contains_key(&15772));
        assert_eq!(m.snapshot().encounter.unwrap().deaths[0].revived_after_s, Some(6.0));
    }

    /// A cast start (0x3809) with a cast time, aimed at `target`.
    fn cast_start(caster: u64, skill: u32, target: u64, cast_ms: u64) -> Vec<u8> {
        let mut p = encode_varint(caster);
        p.push(0);
        p.extend(skill.to_le_bytes());
        p.extend([1, 0]);
        p.extend(encode_varint(target));
        p.extend([0, 0, 0, 0]);
        p.extend(encode_varint(cast_ms));
        p.push(1);
        p
    }

    #[test]
    fn the_target_frame_shows_a_cast_until_it_goes_off() {
        let mut m = meter_with_boss();
        m.feed(S, OPCODE_DAMAGE, &hit(BOSS, ME, 18_010_340, 5000));
        m.feed(2 * S, crate::cast::OPCODE_CAST_START, &cast_start(BOSS, 1_604_380, ME, 3000));
        m.feed(3 * S, OPCODE_DAMAGE, &hit(BOSS, ME, 18_010_340, 5000));
        let cast = m.snapshot().target.unwrap().cast.unwrap();
        assert_eq!((cast.skill, cast.cast_ms, cast.remaining_ms), (1_604_380, 3000, 2000));
        // It went off.
        let mut done = encode_varint(BOSS);
        done.push(0);
        done.extend(1_604_380u32.to_le_bytes());
        done.extend([0x40, 0]);
        done.extend(encode_varint(ME));
        m.feed(4 * S, OPCODE_CAST, &done);
        assert_eq!(m.snapshot().target.unwrap().cast, None);
        // Another one, stopped (interrupted) before it finished.
        m.feed(5 * S, crate::cast::OPCODE_CAST_START, &cast_start(BOSS, 1_604_410, ME, 5000));
        assert!(m.snapshot().target.unwrap().cast.is_some());
        let mut stop = encode_varint(BOSS);
        stop.extend([8, 0x18]);
        stop.extend(1_604_410u32.to_le_bytes());
        stop.push(0);
        m.feed(6 * S, crate::cast::OPCODE_CAST_STOP, &stop);
        assert_eq!(m.snapshot().target.unwrap().cast, None);
    }

    #[test]
    fn a_fight_keeps_its_party_after_the_party_breaks_up() {
        let mut m = meter_with_boss();
        m.feed(0, OPCODE_PLAYER_SPAWN, &spawn(15772, "Pingu"));
        m.feed(0, OPCODE_PARTY_ROSTER, &roster(&["Pingu", "NeneneTowawa"]));
        m.set_scope(Scope::Party);
        m.feed(S, OPCODE_DAMAGE, &hit(BOSS, ME, 18_010_340, 5000));
        m.feed(S, OPCODE_DAMAGE, &hit(BOSS, 15772, 12_010_340, 3000));
        m.feed(3 * S, OPCODE_DEATH, &death(BOSS, 18_010_340, ME));
        assert_eq!(ends(&mut m), [FightEnd::Kill]);
        // The dungeon's done and the party breaks up: you left it.
        m.feed(10 * S, crate::entities::OPCODE_PARTY_LEFT, &[0, 0]);
        assert!(m.registry().party().is_empty());
        let e = m.snapshot().encounter.unwrap();
        assert_eq!((e.players.len(), e.total_damage), (2, 8000), "Pingu still shows");
        assert_eq!(m.shown_party_size(), 2);
    }

    #[test]
    fn force_members_count_as_your_party() {
        let mut m = meter_with_boss();
        m.feed(0, OPCODE_PLAYER_SPAWN, &spawn(15772, "Pingu"));
        m.feed(0, OPCODE_PLAYER_SPAWN, &spawn(777, "Stranger"));
        // A Force roster: members as in a party roster (character ID, 00 00, server,
        // name), among other bytes; a legion's name alone isn't a member.
        let mut force = vec![0, 0x35, 0x29, 0, 0];
        force.extend([0xcf, 0xec, 0x02, 0x00, 0, 0, 0xc7, 0x09, 5]);
        force.extend(b"Pingu");
        force.extend([0x0f, 0, 0, 0, 9]);
        force.extend(b"LegionOne");
        m.feed(0, crate::entities::OPCODE_FORCE_INFO, &force);
        assert!(m.registry().in_force());
        assert!(m.registry().in_party(15772) && !m.registry().in_party(777));
        m.set_scope(Scope::Party);
        m.feed(S, OPCODE_DAMAGE, &hit(BOSS, ME, 18_010_340, 5000));
        m.feed(S, OPCODE_DAMAGE, &hit(BOSS, 15772, 12_010_340, 3000));
        m.feed(S, OPCODE_DAMAGE, &hit(BOSS, 777, 11_010_340, 9000));
        assert_eq!(m.snapshot().encounter.unwrap().total_damage, 8000, "the Force's damage, not a stranger's");
        assert_eq!(m.registry().force_size(), 1);
        // Removed from it (by character ID, as 0x9622 says), then back, then leaving it.
        let remove = [0xcf, 0xec, 0x02, 0x00, 0, 0, 0xc7, 0x09, 0x3f, 0x28, 0x0a, 0, 0x83, 0x24, 0xd5, 0, 3];
        m.feed(2 * S, crate::entities::OPCODE_FORCE_REMOVE_MEMBER, &remove);
        assert!(!m.registry().in_party(15772));
        m.feed(3 * S, crate::entities::OPCODE_FORCE_INFO, &force);
        m.feed(4 * S, crate::entities::OPCODE_FORCE_LEFT, &[0; 9]);
        assert!(!m.registry().in_force() && !m.registry().in_party(15772));
    }

    #[test]
    fn a_fight_keeps_its_bosss_casts() {
        let mut m = meter_with_boss();
        m.feed(S, OPCODE_DAMAGE, &hit(BOSS, ME, 18_010_340, 5000));
        // A cast with a cast time that goes off, one that's stopped, one that's instant.
        m.feed(2 * S, crate::cast::OPCODE_CAST_START, &cast_start(BOSS, 1_604_380, ME, 3000));
        let went_off = |skill: u32| {
            let mut p = encode_varint(BOSS);
            p.push(0);
            p.extend(skill.to_le_bytes());
            p.extend([0x40, 0]);
            p.extend(encode_varint(ME));
            p
        };
        m.feed(5 * S, OPCODE_CAST, &went_off(1_604_380));
        m.feed(6 * S, crate::cast::OPCODE_CAST_START, &cast_start(BOSS, 1_604_410, ME, 5000));
        let mut stop = encode_varint(BOSS);
        stop.extend([8, 0x18]);
        stop.extend(1_604_410u32.to_le_bytes());
        stop.push(0);
        m.feed(7 * S, crate::cast::OPCODE_CAST_STOP, &stop);
        m.feed(9 * S, OPCODE_CAST, &went_off(1_604_100));
        let casts = m.snapshot().encounter.unwrap().boss_casts;
        let got: Vec<_> = casts.iter().map(|c| (c.at_s, c.skill, c.cast_ms, c.stopped)).collect();
        assert_eq!(got, [(1.0, 1_604_380, 3000, false), (5.0, 1_604_410, 5000, true), (8.0, 1_604_100, 0, false)]);
        assert_eq!(casts[0].boss_type, Some(2_400_002));
    }

    #[test]
    fn the_game_saying_a_middle_boss_was_killed_ends_the_fight() {
        let mut m = meter_with_boss();
        m.feed(S, OPCODE_DAMAGE, &hit(BOSS, ME, 18_010_340, 5000));
        // Its own death went unseen (out of view); the dungeon's notice is enough.
        m.feed(3 * S, state::OPCODE_MIDBOSS_KILLED, &2_400_002u32.to_le_bytes());
        assert_eq!(ends(&mut m), [FightEnd::Kill]);
    }

    #[test]
    fn fighting_a_player_outside_your_party_is_pvp() {
        let mut m = meter_as_me();
        m.feed(0, OPCODE_PLAYER_SPAWN, &spawn(15772, "Pingu"));
        m.feed(0, OPCODE_PLAYER_SPAWN, &spawn(777, "Rival"));
        m.feed(0, OPCODE_PARTY_ROSTER, &roster(&["Pingu", "NeneneTowawa"]));
        m.feed(S, OPCODE_DAMAGE, &hit(777, ME, 18_010_340, 5000));
        m.feed(2 * S, OPCODE_DAMAGE, &hit(ME, 777, 11_010_340, 3000));
        // A party member's hits on the rival aren't yours (Questlog counts only yours too).
        m.feed(2 * S, OPCODE_DAMAGE, &hit(777, 15772, 12_010_340, 9000));
        m.feed(20 * S, OPCODE_DAMAGE, &hit(777, ME, 18_010_340, 4000));
        m.feed(20 * S, OPCODE_DEATH, &death(777, 18_010_340, ME));
        let now = m.pvp().unwrap();
        assert_eq!((now.dealt, now.taken, now.kills), (9000, 3000, 1));
        m.tick(200 * S);
        let s = &m.take_pvp()[0];
        assert_eq!((s.dealt, s.taken, s.kills, s.deaths), (9000, 3000, 1, 0));
        assert_eq!((s.opponents[0].name.as_str(), s.opponents[0].class_code), ("Rival", Some(11)));
    }

    #[test]
    fn reset_clears_display() {
        let mut m = meter_as_me();
        m.feed(S, OPCODE_DAMAGE, &hit(1, ME, 18_010_340, 100));
        m.reset();
        assert_eq!(m.snapshot().encounter, None);
    }

    // 20261020-184817: Elementalist 55's spirit 41727 hits the dummy 20644 with 100028,
    // echoes onto 55 with 16990002, and 55 casts Spirit Communion (16770001) on it.
    const SPIRIT_HIT: &str = "a4 a1 01 26 00 ff c5 02 bc 86 01 00 16 03 00 00 02 4f a0 98 00 01 00 00 00 c2 63         ee 35 04 99 01 99 01 99 01 99 01 01 00";
    const SPIRIT_ECHO: &str = "37 04 00 ff c5 02 32 3f 03 01 17 02 93 af 44 65 01 00 00 00 c2 63 14 01 00";
    const COMMUNION: &str = "ff c5 02 04 00 37 d1 e3 ff 00 68 02 af fd f4 63 01 00 00 00 c2 63 af 04 01 00";

    #[test]
    fn spirit_damage_counts_for_its_owner() {
        use crate::entities::tests::hex;
        let mut m = Meter::default();
        m.set_scope(Scope::Everyone);
        // A hit by an owner's own big skill, recorded before the spirit's owner is known.
        m.feed(0, OPCODE_DAMAGE, &hit(20644, 41727, 16_110_004, 22_000));
        m.feed(S / 10, OPCODE_DAMAGE, &hex(SPIRIT_ECHO));
        m.feed(S, OPCODE_DAMAGE, &hex(SPIRIT_HIT));
        m.feed(2 * S, OPCODE_DAMAGE, &hex(COMMUNION));
        m.feed(3 * S, OPCODE_DAMAGE, &hit(20644, 55, 16_740_001, 3_000));

        let e = m.snapshot().encounter.unwrap();
        assert_eq!(e.players.len(), 1);
        let owner = &e.players[0];
        assert_eq!((owner.id, owner.name.as_str()), (55, "#55"));
        assert_eq!(owner.class_name.as_deref(), Some("Elementalist"));
        // The echo (20) and Spirit Communion (559) aren't damage.
        assert_eq!(owner.damage, 22_000 + 6_894 + 3_000);
        let skills: Vec<u32> = owner.skills.iter().map(|s| s.skill).collect();
        assert_eq!(skills, [16_110_004, 100_028, 16_740_001]);
        assert_eq!((e.target_id, e.target_count), (Some(20644), 1));

        let a = m.analysis(4 * S).unwrap();
        assert_eq!(a.hits.len(), 3);
        assert!(a.hits.iter().all(|h| h.player == 55));
    }

    #[test]
    fn your_spirit_makes_its_target_yours() {
        use crate::entities::tests::{hex, SPIRIT_SPAWN};
        let mut m = meter_as_me();
        // Spirit 21466 belongs to 1802; make that us.
        let mut spawn = hex(SPIRIT_SPAWN);
        let at = spawn.windows(2).position(|w| w == [0x8a, 0x0e]).unwrap();
        spawn.splice(at..at + 2, encode_varint(ME));
        let tail = spawn.windows(4).position(|w| w == [0x0a, 0x07, 0x00, 0x00]).unwrap();
        spawn.splice(tail..tail + 4, (ME as u32).to_le_bytes());
        m.feed(0, hp::OPCODE_MOB_SPAWN, &spawn);
        assert_eq!(m.registry().owner_of(21466), Some(ME));

        m.feed(S, OPCODE_DAMAGE, &hit(DUMMY, 21466, 100_044, 1_602));
        let e = m.snapshot().encounter.unwrap();
        assert_eq!(e.players.len(), 1);
        assert!(e.players[0].is_self);
        assert_eq!(e.players[0].damage, 1_602);
    }
}
