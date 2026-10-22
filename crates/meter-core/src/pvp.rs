//! PvP: fights between you and players outside your party.
//!
//! Every hit between you and such a player (either way) is part of a session; 90 seconds
//! without one ends it. A session is kept when it lasted at least 15 seconds or someone
//! died. It counts damage dealt and taken per opponent, the most damage you did to each
//! in 5 seconds (burst), opponents who died (your kills when the killing blow was yours),
//! and your deaths. Not checked against a capture yet: none of ours has PvP in it.

use std::collections::{BTreeMap, VecDeque};

use serde::{Deserialize, Serialize};

/// Quiet this long ends a session.
const SESSION_GAP_US: u64 = 90_000_000;
/// Sessions shorter than this, with nobody dying, aren't kept (a stray hit).
const MIN_SESSION_US: u64 = 15_000_000;
/// The window for burst damage.
const BURST_US: u64 = 5_000_000;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Opponent {
    pub name: String,
    pub class_code: Option<u32>,
    pub dealt: u64,
    pub taken: u64,
    /// The most damage you did them in 5 seconds.
    pub burst: u64,
    /// Times they died in the session, and how many of those you landed the killing blow.
    pub deaths: u32,
    pub kills: u32,
    #[serde(skip)]
    window: VecDeque<(u64, u64)>,
}

/// A finished (or the current) PvP session.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct PvpSession {
    pub started_us: u64,
    pub ended_us: u64,
    pub dealt: u64,
    pub taken: u64,
    pub kills: u32,
    pub deaths: u32,
    /// Who killed you last.
    pub last_killer: Option<String>,
    /// Most damage dealt and taken first.
    pub opponents: Vec<Opponent>,
}

#[derive(Debug, Default)]
pub struct PvpTracker {
    current: Option<(PvpSession, BTreeMap<u64, Opponent>)>,
    finished: Vec<PvpSession>,
}

impl PvpTracker {
    /// A hit between you and a player outside your party. `outgoing`: you hit them.
    pub fn hit(&mut self, ts_us: u64, other: u64, name: String, class_code: Option<u32>, damage: u64, outgoing: bool) {
        self.tick(ts_us);
        let (s, opponents) = self.current.get_or_insert_with(|| {
            (PvpSession { started_us: ts_us, ..PvpSession::default() }, BTreeMap::new())
        });
        s.ended_us = ts_us;
        let o = opponents.entry(other).or_default();
        o.name = name;
        o.class_code = class_code.or(o.class_code);
        if outgoing {
            o.dealt += damage;
            o.window.push_back((ts_us, damage));
            while o.window.front().is_some_and(|(t, _)| ts_us.saturating_sub(*t) > BURST_US) {
                o.window.pop_front();
            }
            o.burst = o.burst.max(o.window.iter().map(|(_, d)| d).sum());
        } else {
            o.taken += damage;
        }
    }

    /// Someone died: an opponent in the session (a kill when `by_you`), or you.
    pub fn death(&mut self, ts_us: u64, entity: u64, is_you: bool, by_you: bool, killer: Option<String>) {
        self.tick(ts_us);
        let Some((s, opponents)) = self.current.as_mut() else { return };
        if is_you {
            s.deaths += 1;
            s.last_killer = killer;
            s.ended_us = ts_us;
        } else if let Some(o) = opponents.get_mut(&entity) {
            o.deaths += 1;
            o.kills += u32::from(by_you);
            s.ended_us = ts_us;
        }
    }

    /// Ends the session once it has been quiet long enough.
    pub fn tick(&mut self, now_us: u64) {
        if self.current.as_ref().is_some_and(|(s, _)| now_us.saturating_sub(s.ended_us) > SESSION_GAP_US) {
            self.end();
        }
    }

    pub fn end(&mut self) {
        let Some((mut s, opponents)) = self.current.take() else { return };
        s.opponents = opponents.into_values().collect();
        s.opponents.sort_by(|a, b| (b.dealt + b.taken).cmp(&(a.dealt + a.taken)).then(a.name.cmp(&b.name)));
        s.dealt = s.opponents.iter().map(|o| o.dealt).sum();
        s.taken = s.opponents.iter().map(|o| o.taken).sum();
        s.kills = s.opponents.iter().map(|o| o.kills).sum();
        let died = s.deaths > 0 || s.opponents.iter().any(|o| o.deaths > 0);
        if s.ended_us - s.started_us >= MIN_SESSION_US || died {
            self.finished.push(s);
        }
    }

    /// The session going on, if any.
    pub fn current(&self) -> Option<PvpSession> {
        let (s, opponents) = self.current.as_ref()?;
        let mut s = s.clone();
        s.opponents = opponents.values().cloned().collect();
        s.dealt = s.opponents.iter().map(|o| o.dealt).sum();
        s.taken = s.opponents.iter().map(|o| o.taken).sum();
        s.kills = s.opponents.iter().map(|o| o.kills).sum();
        Some(s)
    }

    /// Sessions that ended since the last call.
    pub fn take_finished(&mut self) -> Vec<PvpSession> {
        std::mem::take(&mut self.finished)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const S: u64 = 1_000_000;

    #[test]
    fn a_session_with_a_kill_and_a_death() {
        let mut p = PvpTracker::default();
        p.hit(S, 7, "Rival".into(), Some(11), 4000, true);
        p.hit(2 * S, 7, "Rival".into(), None, 1500, false);
        p.hit(4 * S, 7, "Rival".into(), None, 6000, true);
        p.hit(9 * S, 7, "Rival".into(), None, 3000, true);
        p.death(9 * S, 7, false, true, None);
        p.hit(20 * S, 8, "Other".into(), Some(14), 9000, false);
        p.death(21 * S, 1, true, false, Some("Other".into()));
        assert!(p.take_finished().is_empty());
        p.tick(200 * S);
        let s = &p.take_finished()[0];
        assert_eq!((s.dealt, s.taken, s.kills, s.deaths), (13_000, 10_500, 1, 1));
        assert_eq!(s.last_killer.as_deref(), Some("Other"));
        let rival = s.opponents.iter().find(|o| o.name == "Rival").unwrap();
        // 4000 + 6000 within 5 s; the 3000 came 5 s after the 6000.
        assert_eq!((rival.burst, rival.class_code, rival.deaths, rival.kills), (10_000, Some(11), 1, 1));
    }

    #[test]
    fn a_stray_hit_isnt_a_session() {
        let mut p = PvpTracker::default();
        p.hit(S, 7, "Rival".into(), None, 100, true);
        p.tick(200 * S);
        assert!(p.take_finished().is_empty());
    }
}
