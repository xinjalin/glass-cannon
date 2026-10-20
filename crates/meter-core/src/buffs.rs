//! Buffs and debuffs ("effects"): opcodes 0x382a (applied), 0x382b (still active) and
//! 0x382c (removed), and the spans they make.
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
//! and heal-over-time ticks (`0x3805`) refer to it. `effect` is a 9-digit ID that starts
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

/// One effect's time on one target. `end_us` is `None` while it's still active.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EffectSpan {
    pub target: u64,
    pub instance: u64,
    pub effect: u32,
    pub skill: Option<u32>,
    pub source: u64,
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

    fn open(&mut self, ts_us: u64, target: u64, e: Effect, restart: bool) {
        let span = EffectSpan {
            target,
            instance: e.instance,
            effect: e.effect,
            skill: e.skill,
            source: e.source,
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
