//! Buffs and debuffs ("effects"): opcodes 0x382a (applied), 0x382b (still active) and
//! 0x382c (removed), and the spans they make; and the effects already on you as you appear
//! (in `0x3633`: see [`parse_spawn_effects`]).
//!
//! One effect entry:
//!
//! ```text
//! u8 kind (0x13 with a skill, 0x11 without), varint instance, u32 effect,
//! u64 duration_ms, u64 time_ms, varint source, u8,
//! [kind 0x13: u32 skill], …
//! ```
//!
//! - `0x382a`: varint target, varint count (always 1 so far), entry.
//! - `0x382b`: varint target, entry. Sent about once a second for long-running effects.
//! - `0x382c`: varint target, varint count, then per effect either `00, varint instance,
//!   u8` or `07, varint instance, u8, varint source, u32 skill, u32`.
//!
//! `instance` identifies one application of an effect on one target; the remove message
//! and heal-over-time ticks (`0x3805`) refer to it. Cast again while it's still on, an
//! effect is often topped up rather than applied anew: a still-active message for the same
//! instance carries a longer duration, counted from when it was first applied (the raid
//! capture of 2026-11-13: a 30 s buff of yours recast 13.6 s in came back as 43,600 ms).
//! So a still-active message's duration is taken, or the buff would run out early. `effect` is a 9-digit ID that starts
//! with the skill's family (Recuperation 1812·00101, Earth's Promise 1878·00011). Worked out
//! from our live dummy tests: Recuperation's three 8 s applications each ended 8.1 s later
//! with a remove for the same instance, and Earth's Promise's 5 s debuffs 5.0 s later.

use std::collections::HashMap;

use serde::Serialize;

use crate::protocol::read_varint;

pub const OPCODE_BUFF_APPLY: u16 = 0x382a;
pub const OPCODE_BUFF_ACTIVE: u16 = 0x382b;
pub const OPCODE_BUFF_REMOVE: u16 = 0x382c;

/// Entry kinds.
const KIND_WITH_SKILL: u8 = 0x13;
const REMOVE_SHORT: u8 = 0x00;
const REMOVE_LONG: u8 = 0x07;
/// Closed spans older than this (relative to the newest) are dropped.
const KEEP_CLOSED_US: u64 = 30 * 60 * 1_000_000;

/// One effect, as the apply and still-active messages describe it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Effect {
    pub instance: u64,
    /// Which effect (9 digits; the first four are the skill family).
    pub effect: u32,
    /// The skill behind it, when the entry carries one.
    pub skill: Option<u32>,
    /// Who applied it.
    pub source: u64,
    pub duration_ms: u32,
    /// Server time of the message, in milliseconds since the Unix epoch.
    pub time_ms: u64,
}

fn u32_at(p: &[u8], i: &mut usize) -> Option<u32> {
    let v = u32::from_le_bytes(p.get(*i..*i + 4)?.try_into().ok()?);
    *i += 4;
    Some(v)
}

fn entry(p: &[u8], i: &mut usize) -> Option<Effect> {
    let kind = *p.get(*i)?;
    *i += 1;
    let instance = read_varint(p, i)?;
    let effect = u32_at(p, i)?;
    // A u64; durations so far fit in the low half.
    let duration_ms = u32_at(p, i)?;
    *i += 4;
    let time_ms = u64::from_le_bytes(p.get(*i..*i + 8)?.try_into().ok()?);
    *i += 8;
    let source = read_varint(p, i)?;
    *i += 1;
    let skill = if kind == KIND_WITH_SKILL { Some(u32_at(p, i)?) } else { None };
    Some(Effect {
        instance,
        effect,
        skill,
        source,
        duration_ms,
        time_ms,
    })
}

/// `0x382a`: the target and the effect applied to it. Only the first entry is read: the
/// entry's tail varies in length, and every apply seen so far carries exactly one.
pub fn parse_apply(p: &[u8]) -> Option<(u64, Effect)> {
    let mut i = 0;
    let target = read_varint(p, &mut i)?;
    let count = read_varint(p, &mut i)?;
    if count == 0 {
        return None;
    }
    Some((target, entry(p, &mut i)?))
}

/// `0x382b`: the target and one effect that's still active on it.
pub fn parse_active(p: &[u8]) -> Option<(u64, Effect)> {
    let mut i = 0;
    let target = read_varint(p, &mut i)?;
    Some((target, entry(p, &mut i)?))
}

/// `0x382c`: the target and the instances removed from it. Stops at an entry kind it
/// doesn't know, keeping the ones before it.
pub fn parse_remove(p: &[u8]) -> Option<(u64, Vec<u64>)> {
    let mut i = 0;
    let target = read_varint(p, &mut i)?;
    let count = read_varint(p, &mut i)?;
    let mut instances = Vec::new();
    for _ in 0..count {
        let Some(&kind) = p.get(i) else { break };
        i += 1;
        let Some(instance) = read_varint(p, &mut i) else { break };
        match kind {
            REMOVE_SHORT => i += 1,
            REMOVE_LONG => {
                i += 1;
                if read_varint(p, &mut i).is_none() {
                    break;
                }
                i += 8; // skill, and a u32 we don't use
            }
            _ => break,
        }
        if i > p.len() {
            break;
        }
        instances.push(instance);
    }
    Some((target, instances))
}

/// An effect with no end: its time left reads as all ones (in an apply as in a spawn's list).
pub const NO_END_MS: u32 = u32::MAX;

/// The effects already on you as you appear (`0x3633`: at login, and on entering a zone or
/// a dungeon), which no apply message is sent for: a stance turned on before (a Chanter's
/// mantras), a scroll or food still running, and what your passive skills, gear and
/// Theostones give you. A list inside the message:
///
/// ```text
/// varint count, then per effect:
///   u8 kind (0x10, +1 with a source, +2 with a skill), varint instance, u32 effect,
///   u64 time left (ms; all ones: no end), u64 when it ends (ms since 1970; 2099-12-31: never),
///   [kind & 1: varint source], u8 level, [kind & 2: u32 skill],
///   [the 1st, 9th, 17th... effect: u8, a flag bit for it and the seven after],
///   3 × f32 where it was applied
/// ```
///
/// Each with whether it names a source. Where the list starts isn't fixed (your name,
/// stats and more come first), so it's found by its shape: the place from which a count
/// and that many effects read through, each with a kind and an end time that can be one
/// (the longest such list, should two places do). Checked on a login of 2026-11-19: 35
/// effects, read to the byte where the next list starts; the two with no source were the
/// character's two mantras, turned on before, and the one with an end a 30-day effect with
/// 26.9 days left, ending on the hour. All 34 such messages in our captures have a list
/// that reads through.
pub fn parse_spawn_effects(p: &[u8]) -> Vec<(Effect, bool)> {
    const KINDS: std::ops::RangeInclusive<u8> = 0x10..=0x13;
    const ENDS: std::ops::RangeInclusive<u64> = 1_600_000_000_000..=4_200_000_000_000;
    let read = |from: usize| -> Option<Vec<(Effect, bool)>> {
        let mut i = from;
        let count = read_varint(p, &mut i).filter(|n| (1..=250).contains(n))?;
        let mut out = Vec::with_capacity(count as usize);
        for n in 0..count {
            let kind = *p.get(i).filter(|k| KINDS.contains(k))?;
            i += 1;
            let instance = read_varint(p, &mut i)?;
            let effect = u32_at(p, &mut i).filter(|e| *e > 0)?;
            let left = u64::from_le_bytes(p.get(i..i + 8)?.try_into().ok()?);
            let time_ms = u64::from_le_bytes(p.get(i + 8..i + 16)?.try_into().ok()?);
            i += 16;
            if !ENDS.contains(&time_ms) {
                return None;
            }
            let source = if kind & 1 != 0 { Some(read_varint(p, &mut i)?) } else { None };
            i += 1;
            let skill = if kind & 2 != 0 { Some(u32_at(p, &mut i)?) } else { None };
            i += usize::from(n % 8 == 0) + 12;
            if i > p.len() {
                return None;
            }
            let duration_ms = u32::try_from(left).unwrap_or(NO_END_MS);
            out.push((Effect { instance, effect, skill, source: source.unwrap_or(0), duration_ms, time_ms }, source.is_some()));
        }
        Some(out)
    };
    (0..p.len()).filter_map(read).max_by_key(Vec::len).unwrap_or_default()
}

/// One effect's time on one target. `end_us` is `None` while it's still active.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EffectSpan {
    pub target: u64,
    pub instance: u64,
    pub effect: u32,
    pub skill: Option<u32>,
    pub source: u64,
    /// How long the game said it lasts (0 for ones without a set length).
    pub duration_ms: u32,
    /// Capture times, microseconds since the Unix epoch.
    pub start_us: u64,
    pub end_us: Option<u64>,
}

/// Builds effect spans from the apply / still-active / remove messages.
#[derive(Debug, Default, Clone)]
pub struct EffectTracker {
    open: HashMap<(u64, u64), EffectSpan>,
    closed: Vec<EffectSpan>,
}

impl EffectTracker {
    /// Feeds one message; returns true if it was an effect message.
    pub fn feed(&mut self, ts_us: u64, opcode: u16, payload: &[u8]) -> bool {
        match opcode {
            OPCODE_BUFF_APPLY => {
                if let Some((target, e)) = parse_apply(payload) {
                    self.open(ts_us, target, e, true);
                }
            }
            OPCODE_BUFF_ACTIVE => {
                // Effects that were already up when the capture started appear here first.
                if let Some((target, e)) = parse_active(payload) {
                    self.open(ts_us, target, e, false);
                }
            }
            OPCODE_BUFF_REMOVE => {
                if let Some((target, instances)) = parse_remove(payload) {
                    for instance in instances {
                        if let Some(mut span) = self.open.remove(&(target, instance)) {
                            span.end_us = Some(ts_us);
                            self.closed.push(span);
                        }
                    }
                    self.prune(ts_us);
                }
            }
            _ => return false,
        }
        true
    }

    /// You appear (`0x3633`), with the effects already on you (see
    /// [`parse_spawn_effects`]). Those with an end are taken, with the time they have left
    /// (scrolls, food, a skill's buff still running); of those with none, all but the ones
    /// with a source and no skill: those are what your passive skills, gear and Theostones
    /// give you (950 of the 1,014 endless ones in our captures' 34 lists, yours all but 2),
    /// which aren't buffs to anyone. What's left with no end are the stances you've turned
    /// on: a mantra on since before you logged in has no source, one turned on while
    /// playing keeps its skill.
    pub fn self_spawn(&mut self, ts_us: u64, payload: &[u8]) {
        let mut i = 0;
        let Some(me) = read_varint(payload, &mut i) else { return };
        for (mut e, sourced) in parse_spawn_effects(payload) {
            if e.duration_ms == NO_END_MS && sourced && e.skill.is_none() {
                continue;
            }
            if !sourced {
                e.source = me;
            }
            match self.open.get_mut(&(me, e.instance)) {
                // On you since before (a zone entered with it on): it ends when this says.
                Some(open) if open.effect == e.effect => {
                    let run_ms = u32::try_from(ts_us.saturating_sub(open.start_us) / 1000).unwrap_or(NO_END_MS);
                    open.duration_ms = e.duration_ms.saturating_add(run_ms);
                }
                _ => self.open(ts_us, me, e, true),
            }
        }
    }

    fn open(&mut self, ts_us: u64, target: u64, e: Effect, restart: bool) {
        let span = EffectSpan {
            target,
            instance: e.instance,
            effect: e.effect,
            skill: e.skill,
            source: e.source,
            duration_ms: e.duration_ms,
            start_us: ts_us,
            end_us: None,
        };
        let key = (target, e.instance);
        if restart || !self.open.contains_key(&key) {
            if let Some(mut old) = self.open.insert(key, span) {
                // Applied again without a remove: close the previous span here.
                old.end_us = Some(ts_us);
                self.closed.push(old);
            }
        } else if let Some(open) = self.open.get_mut(&key) {
            // Still active: topped up (cast again while it was on), its duration from when
            // it was first applied.
            open.duration_ms = e.duration_ms;
        }
    }

    fn prune(&mut self, now_us: u64) {
        let cutoff = now_us.saturating_sub(KEEP_CLOSED_US);
        self.closed.retain(|s| s.end_us.unwrap_or(u64::MAX) >= cutoff);
    }

    /// Effects active on `target` right now.
    pub fn active_on(&self, target: u64) -> impl Iterator<Item = &EffectSpan> {
        self.open.values().filter(move |s| s.target == target)
    }

    /// Who applied the effect `instance` that is on one of `entities` right now.
    pub fn source_of(&self, instance: u64, entities: [u64; 2]) -> Option<u64> {
        entities.iter().find_map(|e| self.open.get(&(*e, instance)).map(|s| s.source))
    }

    /// Every span that overlaps `[from_us, to_us]`, open ones included, by start time.
    pub fn spans(&self, from_us: u64, to_us: u64) -> Vec<EffectSpan> {
        let mut out: Vec<EffectSpan> = self
            .closed
            .iter()
            .chain(self.open.values())
            .filter(|s| s.start_us <= to_us && s.end_us.is_none_or(|e| e >= from_us))
            .cloned()
            .collect();
        out.sort_by_key(|s| (s.start_us, s.target, s.instance));
        out
    }

    pub fn clear(&mut self) {
        self.open.clear();
        self.closed.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(s: &str) -> Vec<u8> {
        s.split_whitespace()
            .map(|b| u8::from_str_radix(b, 16).unwrap())
            .collect()
    }

    // From the live dummy tests (you = 11792, the dummy = 25002).
    const RECUPERATION_APPLY: &str = "90 5c 01 13 ad 10 e5 e4 cc 0a 40 1f 00 00 00 00 00 00 3d 3b 6a 58 a1 01 00 00 \
                                      90 5c 0d b8 7d 14 01 00 21 23 96 c7 39 12 01 c8 00 ed 0d 47";
    const RECUPERATION_REMOVE: &str = "90 5c 01 00 ad 10 01";

    #[test]
    fn the_effects_on_you_as_you_log_in() {
        // The list from a login (2026-11-19), with what comes before and after it in the
        // message; the character (11555) had turned two mantras on before.
        let list = hex(include_str!("../testdata/self-spawn-effects.hex"));
        let all = parse_spawn_effects(&list);
        assert_eq!(all.len(), 35);
        assert_eq!(all.iter().filter(|(_, sourced)| !sourced).map(|(e, _)| (e.effect, e.instance, e.duration_ms)).collect::<Vec<_>>(), [
            (181_600_201, 20, NO_END_MS),
            (181_900_401, 21, NO_END_MS),
        ]);
        // One with an end: 26.9 days left, ending on the hour.
        let timed: Vec<&Effect> = all.iter().map(|(e, _)| e).filter(|e| e.duration_ms != NO_END_MS).collect();
        assert_eq!(timed.iter().map(|e| (e.effect, e.duration_ms, e.time_ms, e.source)).collect::<Vec<_>>(), [(20_100, 2_320_783_884, 1_797_404_400_049, 11_555)]);
        // The rest: passive skills, gear and Theostones, yours, with no end.
        assert!(all.iter().filter(|(_, sourced)| *sourced).all(|(e, _)| e.source == 11_555));
        assert_eq!(all.last().unwrap().0.effect, 188_000_001);
        // No list: nothing (a short message, one of other things).
        assert!(parse_spawn_effects(&list[..20]).is_empty());
        assert!(parse_spawn_effects(&[0x13; 64]).is_empty());
        // (An apply's count and effect are laid out the same: a list of one.)
        assert_eq!(parse_spawn_effects(&hex(RECUPERATION_APPLY)).iter().map(|(e, _)| e.effect).collect::<Vec<_>>(), [181_200_101]);

        // As you appear: your entity, the rest of the message, the list somewhere in it.
        let mut spawn = vec![0xa3, 0x5a, 0, 0, 0, 0, 0, 2, b'M', b'e'];
        spawn.extend(&list);
        let mut t = EffectTracker::default();
        t.self_spawn(5_000_000, &spawn);
        let mut on: Vec<(u32, u64, u32, u64)> = t.active_on(11_555).map(|s| (s.effect, s.source, s.duration_ms, s.start_us)).collect();
        on.sort_unstable();
        // The mantras (yours, though the list names no source) and the one with an end; not
        // the 32 from passives and gear.
        assert_eq!(on, [(20_100, 11_555, 2_320_783_884, 5_000_000), (181_600_201, 11_555, NO_END_MS, 5_000_000), (181_900_401, 11_555, NO_END_MS, 5_000_000)]);
        // A mantra turned off is removed as any effect is.
        t.feed(6_000_000, OPCODE_BUFF_REMOVE, &[0xa3, 0x5a, 1, 0, 20, 1]);
        assert_eq!(t.active_on(11_555).count(), 2);
        // A zone entered with them still on: the same spans, the timed one ending when the
        // new list says (a second less left, a second on).
        let mut later = spawn.clone();
        let at = later.windows(4).position(|w| w == 20_100u32.to_le_bytes()).unwrap() + 4;
        later[at..at + 8].copy_from_slice(&2_320_782_884u64.to_le_bytes());
        t.self_spawn(6_000_000, &later);
        let mut on: Vec<(u32, u32, u64)> = t.active_on(11_555).map(|s| (s.effect, s.duration_ms, s.start_us)).collect();
        on.sort_unstable();
        assert_eq!(on, [(20_100, 2_320_783_884, 5_000_000), (181_600_201, NO_END_MS, 6_000_000), (181_900_401, NO_END_MS, 5_000_000)]);

        // A mantra turned on while playing is listed with its source and skill at the next
        // zone: taken too (a list of one, as the game lays it out).
        let mut zone = vec![0xa3, 0x5a, 0, 0, 0, 0, 0, 2, b'M', b'e', 0xee, 0xee, 1, 0x13, 7];
        zone.extend(181_900_301u32.to_le_bytes());
        zone.extend([0xff; 8]);
        zone.extend(4_102_412_400_000u64.to_le_bytes());
        zone.extend([0xa3, 0x5a, 20]);
        zone.extend(18_190_030u32.to_le_bytes());
        zone.extend([0; 13]);
        let mut t = EffectTracker::default();
        t.self_spawn(1_000_000, &zone);
        assert_eq!(t.active_on(11_555).map(|s| (s.effect, s.skill, s.source)).collect::<Vec<_>>(), [(181_900_301, Some(18_190_030), 11_555)]);
    }

    #[test]
    fn a_buff_topped_up_while_on_lasts_longer() {
        // Recuperation (8 s) applied, then still active 5 s in with its duration from then
        // on 13 s: cast again, and topped up.
        let apply = hex(RECUPERATION_APPLY);
        let mut active = apply.clone();
        active.remove(2); // no count in a still-active message
        active[9..17].copy_from_slice(&13_000u64.to_le_bytes()); // its duration
        let mut t = EffectTracker::default();
        t.feed(1_000_000, OPCODE_BUFF_APPLY, &apply);
        t.feed(6_000_000, OPCODE_BUFF_ACTIVE, &active);
        let span = t.active_on(11792).next().unwrap();
        assert_eq!((span.start_us, span.duration_ms), (1_000_000, 13_000), "the same start, a longer duration");
    }

    #[test]
    fn buff_apply() {
        let (target, e) = parse_apply(&hex(RECUPERATION_APPLY)).unwrap();
        assert_eq!(target, 11792);
        assert_eq!(
            e,
            Effect {
                instance: 2093,
                effect: 181_200_101,
                skill: Some(18_120_120),
                source: 11792,
                duration_ms: 8000,
                time_ms: 0x0000_01a1_586a_3b3d,
            }
        );
    }

    #[test]
    fn debuff_from_another_player() {
        // Earth's Promise (5 s) on the dummy, applied by another Chanter (8355).
        let (target, e) = parse_apply(&hex(
            "aa c3 01 01 13 86 bc 09 cb 99 31 0b 88 13 00 00 00 00 00 00 17 aa 6c 58 a1 01 00 00 \
             a3 41 10 61 8f 1e 01 00 ed 43 96 c7 ca 68 01 c8 00 ed 0d 47",
        ))
        .unwrap();
        assert_eq!((target, e.source, e.skill), (25002, 8355, Some(18_780_001)));
        assert_eq!((e.effect, e.duration_ms, e.instance), (187_800_011, 5000, 155_142));
    }

    #[test]
    fn effect_without_a_skill() {
        let (_, e) = parse_apply(&hex(
            "ef ce 02 01 11 02 92 96 98 00 08 07 00 00 00 00 00 00 bf 18 6f 58 a1 01 00 00 \
             ef ce 02 01 00 80 b5 97 c7 c0 65 04 c8 00 3c 0e 47",
        ))
        .unwrap();
        assert_eq!((e.instance, e.effect, e.duration_ms, e.skill), (2, 10_000_018, 1800, None));
    }

    #[test]
    fn still_active() {
        let (target, e) = parse_active(&hex(
            "90 5c 13 f5 0b d3 ff d2 0a 60 09 00 00 00 00 00 00 0c 11 6a 58 a1 01 00 00 \
             90 5c 0a 95 19 15 01 02 4f 0d 96 c7 a0 fc 00 c8 00 ed 0d 47",
        ))
        .unwrap();
        assert_eq!((target, e.instance, e.skill), (11792, 1525, Some(18_160_021)));
    }

    #[test]
    fn removes_short_and_long_entries() {
        assert_eq!(parse_remove(&hex(RECUPERATION_REMOVE)), Some((11792, vec![2093])));
        assert_eq!(
            parse_remove(&hex("a4 a1 01 02 00 c6 f4 0c 01 00 c5 f4 0c 01")),
            Some((20644, vec![211_526, 211_525]))
        );
        let long = hex(
            "8a 0e 04 07 d3 02 0b 8a 0e e2 b7 f8 00 5d d4 27 61 07 cf 02 0b 8a 0e e2 b7 f8 00 5d d4 27 61 \
             07 c7 02 0b 8a 0e e2 b7 f8 00 5d d4 27 61 07 cb 02 0b 8a 0e e2 b7 f8 00 5d d4 27 61",
        );
        assert_eq!(parse_remove(&long), Some((1802, vec![339, 335, 327, 331])));
    }

    #[test]
    fn tracker_makes_spans() {
        let mut t = EffectTracker::default();
        assert!(t.feed(1_000_000, OPCODE_BUFF_APPLY, &hex(RECUPERATION_APPLY)));
        assert_eq!(t.active_on(11792).count(), 1);
        // While it's up, the span is open.
        assert_eq!(t.spans(0, 2_000_000)[0].end_us, None);
        assert!(t.feed(9_100_000, OPCODE_BUFF_REMOVE, &hex(RECUPERATION_REMOVE)));
        assert_eq!(t.active_on(11792).count(), 0);
        let spans = t.spans(0, 20_000_000);
        assert_eq!(spans.len(), 1);
        assert_eq!((spans[0].start_us, spans[0].end_us), (1_000_000, Some(9_100_000)));
        assert_eq!((spans[0].skill, spans[0].source), (Some(18_120_120), 11792));
        // Outside the window: nothing.
        assert!(t.spans(10_000_000, 20_000_000).is_empty());
        assert!(!t.feed(0, 0x3804, &[]));
    }
}
