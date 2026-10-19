//! Combat timeline, buff-span tracking, and the buffs-per-skill tables they derive.
//!
//! The meter's core bookkeeping ([`crate::meter`]) aggregates each hit into per-skill
//! totals. This module keeps the *temporal* layer those totals drop:
//!
//! - [`TimelineEvent`] records every hit together with the buffs that were active on the
//!   caster and the target at that instant, so a fight can be drawn as a rotation timeline.
//! - [`BuffSpan`] tracks each buff as an open/closed `[start, end]` interval, so buffs can
//!   be drawn as a buff timeline.
//!
//! From those two, [`buffs_per_skill`] and [`buff_overlap`] derive the "which buffs were up
//! during which skill" breakdown. [`BuffTracker`] is the small state machine that maintains
//! the spans as buff apply/remove messages arrive.

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};

/// A buff's identity: the buff's skill code and the entity that applied it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BuffKey {
    /// The buff's skill code (the same ID space as [`crate::damage::DamageEvent::skill`]).
    pub skill: u32,
    /// The entity that applied the buff.
    pub source: u64,
}

/// Whether a buff helps its target or hinders it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BuffKind {
    /// A beneficial effect on the player.
    Buff,
    /// A harmful effect on the target.
    Debuff,
}

/// Where a buff comes from, relative to the player being analysed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BuffSource {
    /// The player's own buff.
    #[serde(rename = "self")]
    SelfBuff,
    /// A party member's buff.
    #[serde(rename = "party")]
    Party,
    /// A debuff on the target.
    #[serde(rename = "target")]
    Target,
    /// Anything else (kept for forward compatibility).
    #[serde(rename = "other")]
    Other,
}

/// A compact buff reference carried on each [`TimelineEvent`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuffChip {
    pub skill: u32,
    pub source: u64,
}

impl BuffChip {
    pub fn key(self) -> BuffKey {
        BuffKey { skill: self.skill, source: self.source }
    }
}

/// One hit on the rotation timeline, with the buffs that were active at that instant.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TimelineEvent {
    /// Capture time in microseconds since the Unix epoch.
    pub time_us: u64,
    /// The attacker's entity ID.
    pub player: u64,
    /// The target's entity ID.
    pub target: u64,
    /// The skill's ID.
    pub skill: u32,
    /// 1, 2, 3… within a multi-hit skill.
    pub hit: u32,
    pub damage: u64,
    /// A damage-over-time tick rather than a direct hit.
    pub is_dot: bool,
    pub is_crit: bool,
    pub is_back: bool,
    pub is_front: bool,
    pub is_perfect: bool,
    pub is_double: bool,
    pub is_parry: bool,
    /// Buffs on the caster at cast time.
    pub self_buffs: Vec<BuffChip>,
    /// Party-provided buffs on the caster.
    pub party_buffs: Vec<BuffChip>,
    /// Debuffs on the target at cast time.
    pub target_debuffs: Vec<BuffChip>,
}

/// One buff's active interval on one target.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BuffSpan {
    pub start_us: u64,
    pub end_us: u64,
    pub skill: u32,
    pub target: u64,
    pub source: u64,
}

/// A target's HP at a point in time (for the boss HP curve).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BossHpSample {
    pub time_us: u64,
    pub entity: u64,
    pub hp: u64,
}

/// Per-skill metadata for the timeline.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TimelineSkillMeta {
    pub skill: u32,
    /// Display name, filled in from the skill table (`None` = unknown).
    pub name: Option<String>,
    /// Which skill ID's icon to show (a variant may share its base skill's icon).
    pub icon_key: Option<u32>,
}

/// Per-target metadata for the timeline.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TimelineTargetMeta {
    pub entity: u64,
    pub name: Option<String>,
    pub is_boss: bool,
    pub first_hit_us: Option<u64>,
    pub death_us: Option<u64>,
}

/// Everything the timeline window needs for one fight.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct BossTimeline {
    pub events: Vec<TimelineEvent>,
    pub buff_spans: Vec<BuffSpan>,
    pub hp_samples: Vec<BossHpSample>,
    pub skills: HashMap<u32, TimelineSkillMeta>,
    pub targets: HashMap<u64, TimelineTargetMeta>,
}

/// How often one buff was up across a skill's casts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BuffUptime {
    pub key: BuffKey,
    pub kind: BuffKind,
    pub source: BuffSource,
    /// Casts that had this buff up.
    pub casts: usize,
    /// `casts / total casts * 100`, rounded down.
    pub percent: u8,
    /// Per-cast bitmap (`true` = the buff was up on that cast), oldest first.
    pub up: Vec<bool>,
}

/// One skill's buff and debuff uptimes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkillBuffs {
    pub skill: u32,
    pub casts: usize,
    pub damage: u64,
    /// Buffs that were up on every cast.
    pub always: Vec<BuffUptime>,
    /// Beneficial buffs, most uptime first.
    pub buffs: Vec<BuffUptime>,
    /// Target debuffs, most uptime first.
    pub debuffs: Vec<BuffUptime>,
}

/// The buffs-per-skill table for one player.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct BuffsPerSkill {
    /// Total casts counted, across all skills.
    pub casts: usize,
    /// One entry per skill, highest damage first.
    pub skills: Vec<SkillBuffs>,
}

/// How much of a skill's casts and damage happened while one buff was up.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct BuffOverlap {
    pub casts_with: usize,
    pub skill_casts: usize,
    pub damage_with: u64,
    pub skill_damage: u64,
    /// The other buffs that were co-active, with their uptime among these casts.
    pub other_buffs: Vec<BuffUptime>,
}

/// Tracks open buff spans per `(target, key)` as apply/remove messages arrive.
#[derive(Debug, Clone, Default)]
pub struct BuffTracker {
    open: HashMap<(u64, BuffKey), BuffSpan>,
    closed: Vec<BuffSpan>,
}

impl BuffTracker {
    /// A buff was applied (or refreshed) on `target` at `ts_us`.
    pub fn apply(&mut self, ts_us: u64, target: u64, skill: u32, source: u64) {
        let key = BuffKey { skill, source };
        match self.open.get_mut(&(target, key)) {
            Some(span) => span.end_us = ts_us,
            None => {
                self.open.insert(
                    (target, key),
                    BuffSpan { start_us: ts_us, end_us: ts_us, skill, target, source },
                );
            }
        }
    }

    /// A buff was removed from `target` at `ts_us`.
    pub fn remove(&mut self, ts_us: u64, target: u64, skill: u32, source: u64) {
        let key = BuffKey { skill, source };
        if let Some(mut span) = self.open.remove(&(target, key)) {
            span.end_us = ts_us;
            self.closed.push(span);
        }
    }

    /// Closes every still-open span at `ts_us` (for example at the end of a fight).
    pub fn close_all(&mut self, ts_us: u64) {
        for (_, mut span) in self.open.drain() {
            span.end_us = ts_us;
            self.closed.push(span);
        }
    }

    /// The buffs currently open on `target`.
    pub fn active_on(&self, target: u64) -> impl Iterator<Item = BuffKey> + '_ {
        self.open.keys().filter_map(move |(t, key)| (*t == target).then_some(*key))
    }

    /// Every closed span, in the order they closed.
    pub fn spans(&self) -> &[BuffSpan] {
        &self.closed
    }
}

/// Builds the buffs-per-skill table from one player's timeline events.
pub fn buffs_per_skill(events: &[TimelineEvent]) -> BuffsPerSkill {
    let mut by_skill: BTreeMap<u32, Vec<&TimelineEvent>> = BTreeMap::new();
    for e in events {
        by_skill.entry(e.skill).or_default().push(e);
    }

    let mut skills: Vec<SkillBuffs> = Vec::with_capacity(by_skill.len());
    for (skill, casts) in by_skill {
        let total = casts.len();
        let mut damage = 0u64;
        for e in &casts {
            damage += e.damage;
        }

        let mut always = Vec::new();
        let mut buffs = Vec::new();
        let mut debuffs = Vec::new();
        for (key, kind, source) in collect_chips(&casts) {
            let uptime = uptime_for(&casts, key, kind, source);
            if uptime.casts == total {
                always.push(uptime);
            } else if kind == BuffKind::Debuff {
                debuffs.push(uptime);
            } else {
                buffs.push(uptime);
            }
        }
        always.sort_by_key(|u| (u.key.skill, u.key.source));
        buffs.sort_by(|a, b| b.percent.cmp(&a.percent).then_with(|| a.key.skill.cmp(&b.key.skill)));
        debuffs.sort_by(|a, b| b.percent.cmp(&a.percent).then_with(|| a.key.skill.cmp(&b.key.skill)));
        skills.push(SkillBuffs { skill, casts: total, damage, always, buffs, debuffs });
    }
    skills.sort_by(|a, b| b.damage.cmp(&a.damage).then(a.skill.cmp(&b.skill)));

    BuffsPerSkill { casts: events.len(), skills }
}

/// How much of `events` (one skill's casts) happened while `key` was up.
pub fn buff_overlap(events: &[TimelineEvent], key: BuffKey) -> BuffOverlap {
    let casts: Vec<&TimelineEvent> = events.iter().collect();
    let mut casts_with = 0usize;
    let mut damage_with = 0u64;
    let mut skill_damage = 0u64;
    for e in &casts {
        skill_damage += e.damage;
        if chip_present(e, key) {
            casts_with += 1;
            damage_with += e.damage;
        }
    }
    let other_buffs = collect_chips(&casts)
        .into_iter()
        .filter(|(k, _, _)| *k != key)
        .map(|(k, kind, source)| uptime_for(&casts, k, kind, source))
        .collect();
    BuffOverlap {
        casts_with,
        skill_casts: casts.len(),
        damage_with,
        skill_damage,
        other_buffs,
    }
}

/// Every distinct buff seen across the events, with its classification.
fn collect_chips(events: &[&TimelineEvent]) -> Vec<(BuffKey, BuffKind, BuffSource)> {
    let mut seen: Vec<(BuffKey, BuffKind, BuffSource)> = Vec::new();
    for e in events {
        for c in &e.self_buffs {
            push_unique(&mut seen, (c.key(), BuffKind::Buff, BuffSource::SelfBuff));
        }
        for c in &e.party_buffs {
            push_unique(&mut seen, (c.key(), BuffKind::Buff, BuffSource::Party));
        }
        for c in &e.target_debuffs {
            push_unique(&mut seen, (c.key(), BuffKind::Debuff, BuffSource::Target));
        }
    }
    seen
}

fn push_unique(v: &mut Vec<(BuffKey, BuffKind, BuffSource)>, item: (BuffKey, BuffKind, BuffSource)) {
    if !v.contains(&item) {
        v.push(item);
    }
}

/// True if `key` is among the buffs carried on this event.
fn chip_present(e: &TimelineEvent, key: BuffKey) -> bool {
    e.self_buffs.iter().any(|c| c.key() == key)
        || e.party_buffs.iter().any(|c| c.key() == key)
        || e.target_debuffs.iter().any(|c| c.key() == key)
}

fn uptime_for(
    events: &[&TimelineEvent],
    key: BuffKey,
    kind: BuffKind,
    source: BuffSource,
) -> BuffUptime {
    let total = events.len();
    let mut up = vec![false; total];
    let mut casts = 0usize;
    for (i, e) in events.iter().enumerate() {
        if chip_present(e, key) {
            up[i] = true;
            casts += 1;
        }
    }
    let percent = if total == 0 { 0 } else { (casts * 100 / total) as u8 };
    BuffUptime { key, kind, source, casts, percent, up }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(skill: u32, damage: u64, self_buffs: &[u32], target_debuffs: &[u32]) -> TimelineEvent {
        TimelineEvent {
            time_us: 0,
            player: 1,
            target: 2,
            skill,
            hit: 1,
            damage,
            is_dot: false,
            is_crit: false,
            is_back: false,
            is_front: false,
            is_perfect: false,
            is_double: false,
            is_parry: false,
            self_buffs: self_buffs.iter().map(|s| BuffChip { skill: *s, source: 1 }).collect(),
            party_buffs: Vec::new(),
            target_debuffs: target_debuffs.iter().map(|s| BuffChip { skill: *s, source: 1 }).collect(),
        }
    }

    #[test]
    fn tracker_applies_refreshes_and_removes() {
        let mut t = BuffTracker::default();
        t.apply(10, 100, 1000, 7);
        t.apply(20, 100, 1000, 7); // refresh
        t.remove(30, 100, 1000, 7);
        assert_eq!(
            t.spans(),
            &[BuffSpan { start_us: 10, end_us: 30, skill: 1000, target: 100, source: 7 }]
        );
    }

    #[test]
    fn tracker_close_all_closes_open_spans() {
        let mut t = BuffTracker::default();
        t.apply(10, 100, 1000, 7);
        t.close_all(50);
        assert_eq!(
            t.spans(),
            &[BuffSpan { start_us: 10, end_us: 50, skill: 1000, target: 100, source: 7 }]
        );
        assert_eq!(t.active_on(100).count(), 0);
    }

    #[test]
    fn active_on_returns_open_buffs() {
        let mut t = BuffTracker::default();
        t.apply(10, 100, 1000, 7);
        t.apply(11, 100, 1001, 8);
        t.apply(12, 999, 1002, 7);
        let mut keys: Vec<BuffKey> = t.active_on(100).collect();
        keys.sort_by_key(|k| k.skill);
        assert_eq!(
            keys,
            vec![BuffKey { skill: 1000, source: 7 }, BuffKey { skill: 1001, source: 8 }]
        );
    }

    #[test]
    fn buffs_per_skill_counts_uptime() {
        // Skill 1000 cast twice: once with buff 900 up, once without.
        let events = vec![event(1000, 500, &[900], &[]), event(1000, 600, &[], &[])];
        let table = buffs_per_skill(&events);
        assert_eq!(table.casts, 2);
        assert_eq!(table.skills.len(), 1);
        let sb = &table.skills[0];
        assert_eq!((sb.skill, sb.casts, sb.damage), (1000, 2, 1100));
        assert_eq!(sb.buffs.len(), 1);
        assert_eq!(sb.buffs[0].key, BuffKey { skill: 900, source: 1 });
        assert_eq!(sb.buffs[0].casts, 1);
        assert_eq!(sb.buffs[0].percent, 50);
        assert_eq!(sb.buffs[0].up, vec![true, false]);
        assert!(sb.always.is_empty() && sb.debuffs.is_empty());
    }

    #[test]
    fn always_buff_is_separated() {
        let events = vec![event(1000, 100, &[900], &[]), event(1000, 200, &[900], &[])];
        let sb = &buffs_per_skill(&events).skills[0];
        assert_eq!(sb.always.len(), 1);
        assert_eq!(sb.always[0].percent, 100);
        assert!(sb.buffs.is_empty());
    }

    #[test]
    fn debuffs_are_classified_separately() {
        let events = vec![event(1000, 100, &[], &[700]), event(1000, 200, &[], &[])];
        let sb = &buffs_per_skill(&events).skills[0];
        assert_eq!(sb.debuffs.len(), 1);
        assert_eq!(sb.debuffs[0].kind, BuffKind::Debuff);
        assert_eq!(sb.debuffs[0].percent, 50);
    }

    #[test]
    fn buff_overlap_counts_damage_with_buff() {
        let events = vec![
            event(1000, 100, &[900], &[]),
            event(1000, 400, &[900], &[]),
            event(1000, 500, &[], &[]),
        ];
        let ov = buff_overlap(&events, BuffKey { skill: 900, source: 1 });
        assert_eq!(ov.skill_casts, 3);
        assert_eq!(ov.casts_with, 2);
        assert_eq!(ov.skill_damage, 1000);
        assert_eq!(ov.damage_with, 500);
    }
}
