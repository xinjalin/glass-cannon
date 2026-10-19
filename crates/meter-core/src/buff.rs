//! Buff tracking and the analysis tables built on top of the damage stream.
//!
//! The meter's core bookkeeping ([`crate::meter`]) turns every hit into per-skill totals.
//! This module adds the two *temporal* layers those totals drop, and the views derived
//! from them:
//!
//! - A [`BuffSpan`] is one effect's active `[start, end]` interval on an entity, maintained
//!   by [`BuffTracker`] as buff apply/remove messages arrive. Spans give the buff timeline.
//! - A [`BuffSnapshot`] records which effects were up on the caster and target at the moment
//!   of a hit, so [`buff_breakdown`] and [`buff_overlap`] can answer "which buffs were up
//!   during which skill" without scanning spans for every cast.
//!
//! Naming and layout follow this crate's conventions (`_us` timestamps, `u64` entity ids).
//! The buff message layout (`0x382a`/`0x382b`/`0x382c`) is decoded here from our own
//! captures; see `docs/protocol.md`.

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};

use crate::protocol::read_varint;

/// A buff was applied (`0x382a`).
pub const OPCODE_BUFF_APPLY: u16 = 0x382a;
/// A buff's state was refreshed (`0x382b`).
pub const OPCODE_BUFF_REFRESH: u16 = 0x382b;
/// A buff was removed (`0x382c`).
pub const OPCODE_BUFF_REMOVE: u16 = 0x382c;

/// A buff's identity: the buff's skill code and the entity that applied it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BuffId {
    /// The buff's id (the "abnormal id" the game uses; maps to a name via the game data).
    pub skill: u32,
    /// The entity that applied the effect.
    pub source: u64,
}

/// One decoded buff change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuffChange {
    pub target: u64,
    pub skill: u32,
    /// The entity that applied it (0 = unknown, e.g. on remove).
    pub source: u64,
    /// True for apply/refresh, false for remove.
    pub applied: bool,
}

/// Decodes a buff message (apply, refresh or remove) into a [`BuffChange`].
///
/// Layout (worked out from our own captures, see `docs/protocol.md`):
/// `varint target, varint … , varint …, varint buff_id, …` — the buff id is the 4th varint
/// for apply/remove and the 3rd for refresh. The source (who applied it) follows a fixed
/// `4e a1 01 00 00` marker in apply/refresh; remove doesn't carry one.
pub fn parse_buff(opcode: u16, payload: &[u8]) -> Option<BuffChange> {
    if !matches!(opcode, OPCODE_BUFF_APPLY | OPCODE_BUFF_REFRESH | OPCODE_BUFF_REMOVE) {
        return None;
    }
    let mut i = 0;
    let target = read_varint(payload, &mut i)?;
    read_varint(payload, &mut i)?;
    if opcode != OPCODE_BUFF_REFRESH {
        read_varint(payload, &mut i)?;
    }
    let skill = read_varint(payload, &mut i)? as u32;
    let applied = opcode != OPCODE_BUFF_REMOVE;
    let source = if applied { find_source(payload) } else { 0 };
    Some(BuffChange { target, skill, source, applied })
}

fn find_source(payload: &[u8]) -> u64 {
    const MARKER: [u8; 5] = [0x4e, 0xa1, 0x01, 0x00, 0x00];
    let Some(idx) = payload.windows(MARKER.len()).position(|w| w == MARKER) else {
        return 0;
    };
    let mut i = idx + MARKER.len();
    read_varint(payload, &mut i).unwrap_or(0)
}

/// Whether an effect helps its target or hinders it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BuffKind {
    /// A beneficial effect.
    Buff,
    /// A harmful effect.
    Debuff,
}

/// Where an effect came from, relative to the player being analysed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BuffSource {
    /// Cast by the player themself.
    #[serde(rename = "self")]
    SelfBuff,
    /// Cast by another player (a party or group member).
    #[serde(rename = "party")]
    Party,
    /// On the target (usually a debuff).
    #[serde(rename = "target")]
    Target,
    /// Anything else, kept for forward compatibility.
    #[serde(rename = "other")]
    Other,
}

/// One effect's active interval on one entity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BuffSpan {
    pub start_us: u64,
    pub end_us: u64,
    pub skill: u32,
    /// The entity the effect is on.
    pub target: u64,
    /// The entity that applied it.
    pub source: u64,
}

/// The effects that were up on the caster and target at the moment of a hit.
///
/// Split by *who the effect is on* and *who applied it*, which is all the meter can tell
/// before the buff opcodes are decoded. `self_buffs`/`party_buffs` are on the caster;
/// `target_debuffs` are on the enemy it hit.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct BuffSnapshot {
    /// Effects on the caster applied by the caster.
    pub self_buffs: Vec<BuffId>,
    /// Effects on the caster applied by someone else.
    pub party_buffs: Vec<BuffId>,
    /// Effects on the target.
    pub target_debuffs: Vec<BuffId>,
}

/// One hit, reduced to what the buff analysis needs.
///
/// `hit` indexes the hit within a cast (1, 2, 3…), but some skills report every hit with
/// the same value, so a *cast* is detected by `hit` not increasing from the previous hit
/// of that skill (see [`cast_starts`]). DoT ticks are `is_dot` with `hit == 0`.
#[derive(Debug, Clone, PartialEq)]
pub struct SkillHit {
    pub skill: u32,
    pub hit: u32,
    pub is_dot: bool,
    pub damage: u64,
    pub buffs: BuffSnapshot,
}

/// How often one effect was up across a skill's casts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BuffCoverage {
    pub id: BuffId,
    pub kind: BuffKind,
    pub source: BuffSource,
    /// Casts that had this effect up.
    pub casts: usize,
    /// `casts / total casts * 100`, rounded down.
    pub percent: u8,
    /// Per-cast bitmap (`true` = the effect was up on that cast), oldest first.
    pub up: Vec<bool>,
}

/// One skill's effect coverage.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkillBuffBreakdown {
    pub skill: u32,
    pub casts: usize,
    /// Total damage of the skill, all hits and ticks included (matches the meter's totals).
    pub damage: u64,
    /// Effects that were up on every cast.
    pub always: Vec<BuffCoverage>,
    /// Beneficial effects, most uptime first.
    pub buffs: Vec<BuffCoverage>,
    /// Target effects, most uptime first.
    pub debuffs: Vec<BuffCoverage>,
}

/// The buffs-per-skill table for one player.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct BuffBreakdown {
    /// Total casts counted, across all skills.
    pub casts: usize,
    /// One entry per skill, highest damage first.
    pub skills: Vec<SkillBuffBreakdown>,
}

/// How much of a skill's casts and damage happened while one effect was up.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct BuffOverlap {
    pub casts_with: usize,
    pub skill_casts: usize,
    pub damage_with: u64,
    pub skill_damage: u64,
    /// The other effects that were co-active, with their coverage among these casts.
    pub other_buffs: Vec<BuffCoverage>,
}

/// Tracks effect spans per `(target, id)` as apply/remove messages arrive.
#[derive(Debug, Clone, Default)]
pub struct BuffTracker {
    /// Open spans keyed by `(target, skill)`. A target can only carry one of each buff id,
    /// so the source isn't part of the key.
    open: HashMap<(u64, u32), BuffSpan>,
    closed: Vec<BuffSpan>,
}

impl BuffTracker {
    /// An effect was applied (or refreshed) on `target` at `ts_us`.
    pub fn apply(&mut self, ts_us: u64, target: u64, skill: u32, source: u64) {
        match self.open.get_mut(&(target, skill)) {
            Some(span) => {
                span.end_us = ts_us;
                span.source = source;
            }
            None => {
                self.open.insert(
                    (target, skill),
                    BuffSpan { start_us: ts_us, end_us: ts_us, skill, target, source },
                );
            }
        }
    }

    /// An effect was removed from `target` at `ts_us`.
    pub fn remove(&mut self, ts_us: u64, target: u64, skill: u32) {
        if let Some(mut span) = self.open.remove(&(target, skill)) {
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

    /// The effects currently open on `target`.
    pub fn active_on(&self, target: u64) -> impl Iterator<Item = BuffId> + '_ {
        self.open
            .iter()
            .filter_map(move |((t, skill), span)| (*t == target).then_some(BuffId { skill: *skill, source: span.source }))
    }

    /// Every span for the buff timeline, open spans included (their `end_us` is `now_us`).
    ///
    /// Open spans must be visible while a fight is live, not only after it ends, so this
    /// builds a merged view on demand instead of returning just the closed ones.
    pub fn snapshot(&self, now_us: u64) -> Vec<BuffSpan> {
        let mut spans: Vec<BuffSpan> = self.closed.clone();
        spans.extend(self.open.values().map(|s| BuffSpan { end_us: now_us, ..s.clone() }));
        spans.sort_by_key(|s| (s.start_us, s.target, s.skill, s.source));
        spans
    }
}

/// Start indices (into `hits`, one skill's hits in time order) of each cast.
///
/// A cast begins when `hit` stops increasing from the previous hit of that skill. This
/// handles both multi-hit skills (1, 2, 3…, then back to 1) and skills whose hits always
/// report the same value (e.g. always 2). DoT ticks aren't casts.
fn cast_starts(hits: &[&SkillHit]) -> Vec<usize> {
    let mut starts = Vec::new();
    let mut prev: Option<u32> = None;
    for (i, h) in hits.iter().enumerate() {
        if h.is_dot {
            continue;
        }
        if prev.is_none_or(|p| h.hit <= p) {
            starts.push(i);
        }
        prev = Some(h.hit);
    }
    starts
}

/// Builds the buffs-per-skill table from one player's hits.
pub fn buff_breakdown(events: &[SkillHit]) -> BuffBreakdown {
    // Group by skill, keeping time order (events arrive ordered).
    let mut by_skill: BTreeMap<u32, Vec<&SkillHit>> = BTreeMap::new();
    for e in events {
        by_skill.entry(e.skill).or_default().push(e);
    }

    let mut skills: Vec<SkillBuffBreakdown> = Vec::with_capacity(by_skill.len());
    for (skill, hits) in by_skill {
        // The representative event of each cast is the hit that starts it.
        let starts = cast_starts(&hits);
        let casts: Vec<&SkillHit> = starts.iter().map(|&i| hits[i]).collect();
        let total = casts.len();
        // Damage counts every hit and tick, so it lines up with the meter's totals.
        let damage: u64 = hits.iter().map(|e| e.damage).sum();

        let mut always = Vec::new();
        let mut buffs = Vec::new();
        let mut debuffs = Vec::new();
        // Each list is handled on its own, so an effect seen as both a caster buff and a
        // target debuff gets two independent coverage rows instead of one merged one.
        for (id, kind, source, list) in distinct_effects(&casts) {
            let cov = coverage(&casts, id, kind, source, list);
            if cov.casts == total {
                always.push(cov);
            } else if kind == BuffKind::Debuff {
                debuffs.push(cov);
            } else {
                buffs.push(cov);
            }
        }
        always.sort_by_key(|c| (c.id.skill, c.id.source));
        buffs.sort_by(|a, b| b.percent.cmp(&a.percent).then_with(|| a.id.skill.cmp(&b.id.skill)));
        debuffs.sort_by(|a, b| b.percent.cmp(&a.percent).then_with(|| a.id.skill.cmp(&b.id.skill)));

        skills.push(SkillBuffBreakdown { skill, casts: total, damage, always, buffs, debuffs });
    }
    skills.sort_by(|a, b| b.damage.cmp(&a.damage).then(a.skill.cmp(&b.skill)));

    let casts = skills.iter().map(|s| s.casts).sum();
    BuffBreakdown { casts, skills }
}

/// How much of one player's damage happened while `id` was up, across all their skills.
pub fn buff_overlap(events: &[SkillHit], id: BuffId) -> BuffOverlap {
    let mut by_skill: BTreeMap<u32, Vec<&SkillHit>> = BTreeMap::new();
    for e in events {
        by_skill.entry(e.skill).or_default().push(e);
    }

    let mut casts_with = 0usize;
    let mut damage_with = 0u64;
    let mut skill_damage = 0u64;
    let mut skill_casts = 0usize;
    let mut cast_events: Vec<&SkillHit> = Vec::new();

    for (_, hits) in by_skill {
        let mut prev: Option<u32> = None;
        let mut current_has = false;
        for h in &hits {
            if h.is_dot {
                // Ticks ride the current cast; before any cast they count as no-buff.
                skill_damage += h.damage;
                if current_has {
                    damage_with += h.damage;
                }
                continue;
            }
            if prev.is_none_or(|p| h.hit <= p) {
                skill_casts += 1;
                current_has = chip_present(h, id);
                if current_has {
                    casts_with += 1;
                }
                cast_events.push(*h);
            }
            prev = Some(h.hit);
            skill_damage += h.damage;
            if current_has {
                damage_with += h.damage;
            }
        }
    }

    let other_buffs = distinct_effects(&cast_events)
        .into_iter()
        .filter(|(k, _, _, _)| *k != id)
        .map(|(k, kind, source, list)| coverage(&cast_events, k, kind, source, list))
        .collect();

    BuffOverlap {
        casts_with,
        skill_casts,
        damage_with,
        skill_damage,
        other_buffs,
    }
}

/// Which list an effect lives in, so coverage only checks that list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EffectList {
    SelfBuffs,
    PartyBuffs,
    TargetDebuffs,
}

/// Every distinct effect seen across the casts, with its classification and list.
fn distinct_effects(casts: &[&SkillHit]) -> Vec<(BuffId, BuffKind, BuffSource, EffectList)> {
    let mut seen: Vec<(BuffId, BuffKind, BuffSource, EffectList)> = Vec::new();
    for c in casts {
        for id in &c.buffs.self_buffs {
            push_unique(&mut seen, (*id, BuffKind::Buff, BuffSource::SelfBuff, EffectList::SelfBuffs));
        }
        for id in &c.buffs.party_buffs {
            push_unique(&mut seen, (*id, BuffKind::Buff, BuffSource::Party, EffectList::PartyBuffs));
        }
        for id in &c.buffs.target_debuffs {
            push_unique(&mut seen, (*id, BuffKind::Debuff, BuffSource::Target, EffectList::TargetDebuffs));
        }
    }
    seen
}

fn push_unique(
    v: &mut Vec<(BuffId, BuffKind, BuffSource, EffectList)>,
    item: (BuffId, BuffKind, BuffSource, EffectList),
) {
    if !v.contains(&item) {
        v.push(item);
    }
}

/// True if `id` is among the effects carried on this hit.
fn chip_present(h: &SkillHit, id: BuffId) -> bool {
    h.buffs.self_buffs.contains(&id)
        || h.buffs.party_buffs.contains(&id)
        || h.buffs.target_debuffs.contains(&id)
}

fn coverage(
    casts: &[&SkillHit],
    id: BuffId,
    kind: BuffKind,
    source: BuffSource,
    list: EffectList,
) -> BuffCoverage {
    let total = casts.len();
    let mut up = vec![false; total];
    let mut n = 0usize;
    for (i, c) in casts.iter().enumerate() {
        let present = match list {
            EffectList::SelfBuffs => c.buffs.self_buffs.contains(&id),
            EffectList::PartyBuffs => c.buffs.party_buffs.contains(&id),
            EffectList::TargetDebuffs => c.buffs.target_debuffs.contains(&id),
        };
        if present {
            up[i] = true;
            n += 1;
        }
    }
    let percent = if total == 0 { 0 } else { (n * 100 / total) as u8 };
    BuffCoverage { id, kind, source, casts: n, percent, up }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hit(skill: u32, damage: u64, self_buffs: &[u32], target_debuffs: &[u32]) -> SkillHit {
        SkillHit {
            skill,
            hit: 1,
            is_dot: false,
            damage,
            buffs: BuffSnapshot {
                self_buffs: self_buffs.iter().map(|s| BuffId { skill: *s, source: 1 }).collect(),
                party_buffs: Vec::new(),
                target_debuffs: target_debuffs.iter().map(|s| BuffId { skill: *s, source: 2 }).collect(),
            },
        }
    }

    fn multi_hit(skill: u32, damage: u64) -> SkillHit {
        SkillHit { hit: 2, ..hit(skill, damage, &[], &[]) }
    }

    fn hex(s: &str) -> Vec<u8> {
        s.split_whitespace().map(|b| u8::from_str_radix(b, 16).unwrap()).collect()
    }

    #[test]
    fn tracker_applies_refreshes_and_removes() {
        let mut t = BuffTracker::default();
        t.apply(10, 100, 1000, 7);
        t.apply(20, 100, 1000, 7); // refresh
        t.remove(30, 100, 1000);
        assert_eq!(
            t.snapshot(99),
            vec![BuffSpan { start_us: 10, end_us: 30, skill: 1000, target: 100, source: 7 }]
        );
    }

    #[test]
    fn snapshot_includes_open_spans() {
        let mut t = BuffTracker::default();
        t.apply(10, 100, 1000, 7);
        t.apply(15, 100, 1001, 8);
        t.remove(20, 100, 1001);
        let spans = t.snapshot(50);
        assert_eq!(
            spans,
            vec![
                BuffSpan { start_us: 10, end_us: 50, skill: 1000, target: 100, source: 7 },
                BuffSpan { start_us: 15, end_us: 20, skill: 1001, target: 100, source: 8 },
            ]
        );
    }

    #[test]
    fn active_on_returns_open_effects() {
        let mut t = BuffTracker::default();
        t.apply(10, 100, 1000, 7);
        t.apply(11, 100, 1001, 8);
        t.apply(12, 999, 1002, 7);
        let mut ids: Vec<BuffId> = t.active_on(100).collect();
        ids.sort_by_key(|k| k.skill);
        assert_eq!(ids, vec![BuffId { skill: 1000, source: 7 }, BuffId { skill: 1001, source: 8 }]);
    }

    #[test]
    fn parses_apply_refresh_and_remove() {
        // Real captured bytes (target 3199 = the player).
        let apply = hex(
            "ff 18 01 11 c5 0c e8 ef ec 0a 00 00 00 00 00 00 00 00 e6 f7 17 4e a1 01 00 00 \
             ff 18 01 00 7a dc 99 c7 95 c2 04 c8 00 ed 0d 47",
        );
        let ev = parse_buff(OPCODE_BUFF_APPLY, &apply).unwrap();
        assert_eq!((ev.target, ev.skill, ev.source, ev.applied), (3199, 1605, 3199, true));

        let refresh = hex(
            "ff 18 13 95 09 d3 ff d2 0a 60 09 00 00 00 00 00 00 33 fc 17 4e a1 01 00 00 \
             ff 18 0a 95 19 15 01 02 7a dc 99 c7 95 c2 04 c8 00 ed 0d 47",
        );
        let ev = parse_buff(OPCODE_BUFF_REFRESH, &refresh).unwrap();
        assert_eq!((ev.target, ev.skill, ev.source, ev.applied), (3199, 1173, 3199, true));

        let remove = hex("ff 18 01 00 c5 0c 01");
        let ev = parse_buff(OPCODE_BUFF_REMOVE, &remove).unwrap();
        assert_eq!((ev.target, ev.skill, ev.source, ev.applied), (3199, 1605, 0, false));
    }

    #[test]
    fn parses_debuff_from_another_player() {
        // Real captured bytes: player 13714 debuffs dummy 38937.
        let apply = hex(
            "99 b0 02 01 13 ee bf 06 e5 07 5f 0a 10 27 00 00 00 00 00 00 aa 1a 18 4e a1 01 00 00 \
             92 6b 0a d4 80 09 01 00 c2 0b 95 c7 69 75 01 c8 81 f5 0d 47",
        );
        let ev = parse_buff(OPCODE_BUFF_APPLY, &apply).unwrap();
        assert_eq!((ev.target, ev.skill, ev.source), (38937, 106478, 13714));
    }

    #[test]
    fn multi_hit_is_one_cast() {
        // One cast of skill 1000 = first hit + one follow-up hit. The buff was up for the
        // whole cast, so coverage must be 100%, not 50% from counting the follow-up.
        let events = vec![hit(1000, 500, &[900], &[]), multi_hit(1000, 500)];
        let table = buff_breakdown(&events);
        assert_eq!(table.casts, 1);
        assert_eq!(table.skills.len(), 1);
        let sb = &table.skills[0];
        assert_eq!((sb.skill, sb.casts, sb.damage), (1000, 1, 1000));
        assert_eq!(sb.always.len(), 1);
        assert_eq!(sb.always[0].percent, 100);
        assert_eq!(sb.always[0].casts, 1);
    }

    #[test]
    fn uptime_is_per_cast() {
        let events = vec![hit(1000, 500, &[900], &[]), hit(1000, 600, &[], &[])];
        let table = buff_breakdown(&events);
        assert_eq!(table.casts, 2);
        let sb = &table.skills[0];
        assert_eq!(sb.buffs.len(), 1);
        assert_eq!(sb.buffs[0].id, BuffId { skill: 900, source: 1 });
        assert_eq!(sb.buffs[0].casts, 1);
        assert_eq!(sb.buffs[0].percent, 50);
        assert_eq!(sb.buffs[0].up, vec![true, false]);
    }

    #[test]
    fn same_id_as_buff_and_debuff_stays_separate() {
        // Skill 900 sits in self_buffs on one cast and in target_debuffs on another. The two
        // rows must not merge: each list is counted on its own.
        let events = vec![
            hit(1000, 100, &[900], &[]),
            hit(1000, 200, &[], &[900]),
        ];
        let sb = &buff_breakdown(&events).skills[0];
        assert_eq!(sb.buffs.len(), 1);
        assert_eq!(sb.debuffs.len(), 1);
        assert_eq!(sb.buffs[0].casts, 1);
        assert_eq!(sb.debuffs[0].casts, 1);
        assert_eq!(sb.buffs[0].percent, 50);
        assert_eq!(sb.debuffs[0].percent, 50);
    }

    #[test]
    fn buff_overlap_counts_damage_with_effect() {
        let events = vec![
            hit(1000, 100, &[900], &[]),
            hit(1000, 400, &[900], &[]),
            hit(1000, 500, &[], &[]),
        ];
        let ov = buff_overlap(&events, BuffId { skill: 900, source: 1 });
        assert_eq!(ov.skill_casts, 3);
        assert_eq!(ov.casts_with, 2);
        assert_eq!(ov.skill_damage, 1000);
        assert_eq!(ov.damage_with, 500);
    }

    #[test]
    fn cast_starts_when_hit_does_not_increase() {
        // Some skills report every hit as `hit == 2` (single-hit skills whose first hit
        // index never appears). Each event is its own cast, not zero casts.
        let events = vec![
            SkillHit { hit: 2, ..hit(1000, 100, &[900], &[]) },
            SkillHit { hit: 2, ..hit(1000, 100, &[], &[]) },
        ];
        let table = buff_breakdown(&events);
        assert_eq!(table.casts, 2);
        assert_eq!(table.skills[0].casts, 2);
        assert_eq!(table.skills[0].buffs[0].percent, 50);
    }

    #[test]
    fn hit_reset_starts_a_new_cast() {
        // 1,2,3 then 1,2 is two casts of a three- then two-hit skill.
        let events = vec![
            SkillHit { hit: 1, ..hit(1000, 10, &[], &[]) },
            SkillHit { hit: 2, ..hit(1000, 20, &[], &[]) },
            SkillHit { hit: 3, ..hit(1000, 30, &[], &[]) },
            SkillHit { hit: 1, ..hit(1000, 40, &[], &[]) },
            SkillHit { hit: 2, ..hit(1000, 50, &[], &[]) },
        ];
        let table = buff_breakdown(&events);
        assert_eq!(table.casts, 2);
        assert_eq!(table.skills[0].casts, 2);
        assert_eq!(table.skills[0].damage, 150);
    }
}
