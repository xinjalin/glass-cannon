//! Dummy mode: timed runs on the training dummy (the Training Scarecrow).
//!
//! Hitting a dummy switches the meter to dummy mode (if it's on in the settings): your
//! damage only, a run of a set length from your first hit, and your pace against your
//! best and last runs of that length. When the fight ends, a run you kept up to the end
//! is saved (`dummy-runs.json`); one you stopped early is dropped. A run is *clean* when
//! nobody else buffed you or debuffed the dummy during it, so it compares fairly with your
//! other clean runs. The history window lists the runs and compares any two.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use meter_core::meter::{Analysis, EncounterView, TimelineHit};

/// Run lengths offered in the settings (seconds), and the default.
pub const RUN_LENGTHS: [u32; 5] = [30, 60, 120, 180, 300];
pub const DEFAULT_RUN_S: u32 = 60;
/// Runs kept.
const KEEP: usize = 300;
/// A run counts as kept up to the end if your last hit is this close to it (ms).
const END_SLACK_MS: u32 = 3000;
/// Skills listed per run.
const SKILLS_KEPT: usize = 12;

/// One skill in a run.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RunSkill {
    pub skill: u32,
    pub damage: u64,
    pub hits: u32,
    pub crits: u32,
}

/// A finished run.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DummyRun {
    /// When the fight ended (ms since 1970); also its ID.
    pub ended_ms: u64,
    pub character: Option<String>,
    /// Yours to group runs by (a gear set, a rotation…).
    #[serde(default)]
    pub label: String,
    pub run_s: u32,
    pub damage: u64,
    pub dps: f64,
    pub hits: u32,
    pub crits: u32,
    /// Hits from behind.
    pub back: u32,
    pub casts: u32,
    /// Average damage of a crit and of any other hit (not damage over time).
    pub crit_avg: f64,
    pub normal_avg: f64,
    pub clean: bool,
    /// Who buffed you or debuffed the dummy (names) when it isn't clean.
    #[serde(default)]
    pub helpers: Vec<String>,
    /// Your damage by the end of each second.
    pub curve: Vec<u64>,
    pub skills: Vec<RunSkill>,
}

impl DummyRun {
    /// Scores your run from a finished fight on a dummy, or None when you stopped before
    /// `run_s` (or didn't hit it).
    pub fn from_fight(analysis: &Analysis, run_s: u32, ended_ms: u64, character: Option<String>, label: &str) -> Option<Self> {
        let me = analysis.view.players.iter().find(|p| p.is_self)?.id;
        let mine: Vec<&TimelineHit> = analysis.hits.iter().filter(|h| h.player == me).collect();
        let t0 = mine.first()?.t;
        let len_ms = run_s * 1000;
        if mine.last()?.t - t0 + END_SLACK_MS < len_ms {
            return None;
        }
        let inside: Vec<&TimelineHit> = mine.into_iter().filter(|h| h.t - t0 < len_ms).collect();
        let mut curve = vec![0u64; run_s as usize];
        let mut skills: BTreeMap<u32, RunSkill> = BTreeMap::new();
        let (mut damage, mut hits, mut crits, mut back, mut casts) = (0u64, 0u32, 0u32, 0u32, 0u32);
        let (mut crit_dmg, mut normal_dmg, mut normal_hits) = (0u64, 0u64, 0u32);
        for h in &inside {
            damage += h.damage;
            let sec = ((h.t - t0) / 1000) as usize;
            curve[sec] += h.damage;
            let n = u32::from(h.n.max(1));
            let s = skills.entry(h.skill).or_insert(RunSkill { skill: h.skill, damage: 0, hits: 0, crits: 0 });
            s.damage += h.damage;
            s.hits += n;
            if h.flags & TimelineHit::FIRST != 0 {
                casts += 1;
            }
            if h.flags & TimelineHit::DOT != 0 {
                continue;
            }
            hits += n;
            if h.flags & TimelineHit::BACK != 0 {
                back += n;
            }
            if h.flags & TimelineHit::CRIT != 0 {
                crits += n;
                s.crits += n;
                crit_dmg += h.damage;
            } else {
                normal_dmg += h.damage;
                normal_hits += n;
            }
        }
        for i in 1..curve.len() {
            curve[i] += curve[i - 1];
        }
        // Clean: no effect from anyone else on you, or on the dummy, during the run.
        let dummies: Vec<u64> = inside.iter().map(|h| h.target).collect();
        let (start, end) = (t0, t0 + len_ms);
        let mut helpers: Vec<String> = analysis
            .effects
            .iter()
            .filter(|e| e.source != me && e.source != 0 && e.start < end && e.end > start)
            .filter(|e| e.target == me || dummies.contains(&e.target))
            // Only other players: a mob's or the world's effects aren't someone's help.
            .filter(|e| !dummies.contains(&e.source) && !analysis.mob_types.contains_key(&e.source))
            .map(|e| analysis.names.get(&e.source).cloned().unwrap_or_else(|| format!("#{}", e.source)))
            .collect();
        helpers.sort();
        helpers.dedup();
        let mut skills: Vec<RunSkill> = skills.into_values().collect();
        skills.sort_by(|a, b| b.damage.cmp(&a.damage));
        skills.truncate(SKILLS_KEPT);
        let avg = |d: u64, n: u32| if n == 0 { 0.0 } else { d as f64 / f64::from(n) };
        Some(Self {
            ended_ms,
            character,
            label: label.to_string(),
            run_s,
            damage,
            dps: damage as f64 / f64::from(run_s),
            hits,
            crits,
            back,
            casts,
            crit_avg: avg(crit_dmg, crits),
            normal_avg: avg(normal_dmg, normal_hits),
            clean: helpers.is_empty(),
            helpers,
            curve,
            skills,
        })
    }
}

/// Your runs, newest first.
pub struct DummyBook {
    path: PathBuf,
    runs: Vec<DummyRun>,
}

impl DummyBook {
    pub fn load(dir: &Path) -> Self {
        let path = dir.join("dummy-runs.json");
        let runs = fs::read_to_string(&path).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        Self { path, runs }
    }

    fn save(&self) {
        if let Some(dir) = self.path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        if let Ok(s) = serde_json::to_string(&self.runs) {
            let _ = fs::write(&self.path, s);
        }
    }

    pub fn add(&mut self, run: DummyRun) {
        self.runs.insert(0, run);
        self.runs.truncate(KEEP);
        self.save();
    }

    pub fn runs(&self) -> &[DummyRun] {
        &self.runs
    }

    /// Gives a run a label (empty: none).
    pub fn label(&mut self, ended_ms: u64, label: &str) -> bool {
        let Some(r) = self.runs.iter_mut().find(|r| r.ended_ms == ended_ms) else { return false };
        r.label = label.trim().to_string();
        self.save();
        true
    }

    pub fn remove(&mut self, ended_ms: u64) {
        self.runs.retain(|r| r.ended_ms != ended_ms);
        self.save();
    }

    /// Your best and last runs of this length (clean ones for the best).
    pub fn best_and_last(&self, character: Option<&str>, run_s: u32) -> (Option<&DummyRun>, Option<&DummyRun>) {
        let mine = || self.runs.iter().filter(move |r| r.run_s == run_s && r.character.as_deref() == character);
        let best = mine().filter(|r| r.clean).max_by(|a, b| a.dps.total_cmp(&b.dps));
        (best, mine().next())
    }
}

/// Dummy mode on the meter, while a fight on a dummy is going on (or just ended).
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct DummyLive {
    pub run_s: u32,
    /// Seconds since your first hit, up to `run_s`.
    pub elapsed_s: f64,
    pub done: bool,
    /// The fight ended before the run's time was up: it isn't saved.
    pub cancelled: bool,
    pub damage: u64,
    pub dps: f64,
    /// Your damage against your best clean run's and your last run's at the same point
    /// (+0.05 = 5 % ahead).
    pub vs_best: Option<f64>,
    pub vs_last: Option<f64>,
    pub best_dps: Option<f64>,
    pub label: String,
}

/// Dummy mode's state between snapshots: the last reading inside the run, which stays
/// once the run's time is up.
#[derive(Default)]
pub struct DummyTracker {
    fight: Option<u64>,
    frozen: Option<(f64, u64)>,
}

impl DummyTracker {
    /// Dummy mode for the encounter shown, now (µs). `you` is your damage so far, and
    /// `first_hit_us` when you first hit.
    pub fn view(&mut self, enc: &EncounterView, now_us: u64, run_s: u32, book: &DummyBook, character: Option<&str>, label: &str) -> Option<DummyLive> {
        let you = enc.players.iter().find(|p| p.is_self)?;
        if self.fight != Some(enc.start_us) {
            self.fight = Some(enc.start_us);
            self.frozen = None;
        }
        let end_us = if enc.active { now_us } else { enc.end_us };
        let elapsed = end_us.saturating_sub(enc.start_us) as f64 / 1e6;
        let len = f64::from(run_s);
        let (elapsed_s, damage) = if elapsed < len {
            self.frozen = Some((elapsed, you.damage));
            (elapsed, you.damage)
        } else {
            self.frozen.unwrap_or((len, you.damage))
        };
        let done = elapsed >= len;
        let (best, last) = book.best_and_last(character, run_s);
        let at = |r: &DummyRun| {
            let sec = (elapsed_s.floor() as usize).min(r.curve.len().saturating_sub(1));
            r.curve.get(sec).copied().filter(|d| *d > 0).map(|d| damage as f64 / d as f64 - 1.0)
        };
        Some(DummyLive {
            run_s,
            elapsed_s: elapsed_s.min(len),
            done,
            cancelled: !enc.active && !done,
            damage,
            dps: if elapsed_s > 0.5 { damage as f64 / elapsed_s } else { 0.0 },
            vs_best: best.and_then(at),
            vs_last: last.and_then(at),
            best_dps: best.map(|r| r.dps),
            label: label.to_string(),
        })
    }
}

/// Two runs side by side (`a` the earlier one), with how much of the difference the crits
/// account for.
#[derive(Debug, Clone, Serialize)]
pub struct Comparison {
    pub dps_pct: f64,
    /// The difference with `b`'s crit rate replaced by `a`'s: what's left once crit luck
    /// is taken out.
    pub luck_free_pct: f64,
    pub crit_rate_a: f64,
    pub crit_rate_b: f64,
    pub verdict: &'static str,
    /// Per skill: (skill, damage in a, damage in b).
    pub skills: Vec<(u32, u64, u64)>,
}

/// A difference under this (after taking crit luck out) is called noise.
const NOISE_PCT: f64 = 1.0;

pub fn compare(a: &DummyRun, b: &DummyRun) -> Comparison {
    let rate = |r: &DummyRun| if r.hits == 0 { 0.0 } else { f64::from(r.crits) / f64::from(r.hits) };
    let (ra, rb) = (rate(a), rate(b));
    // Damage per hit at a crit rate: normal + rate × (crit - normal).
    let per_hit = |r: &DummyRun, rate: f64| r.normal_avg + rate * (r.crit_avg - r.normal_avg);
    let b_at_a_rate = if per_hit(b, rb) > 0.0 { b.dps * per_hit(b, ra) / per_hit(b, rb) } else { b.dps };
    let pct = |x: f64, y: f64| if y > 0.0 { (x / y - 1.0) * 100.0 } else { 0.0 };
    let dps_pct = pct(b.dps, a.dps);
    let luck_free_pct = pct(b_at_a_rate, a.dps);
    let verdict = if luck_free_pct.abs() < NOISE_PCT {
        if dps_pct.abs() < NOISE_PCT { "about the same" } else { "crit luck" }
    } else if luck_free_pct > 0.0 {
        "a real gain"
    } else {
        "a real loss"
    };
    let mut skills: BTreeMap<u32, (u64, u64)> = BTreeMap::new();
    for s in &a.skills {
        skills.entry(s.skill).or_default().0 += s.damage;
    }
    for s in &b.skills {
        skills.entry(s.skill).or_default().1 += s.damage;
    }
    let mut skills: Vec<(u32, u64, u64)> = skills.into_iter().map(|(k, (x, y))| (k, x, y)).collect();
    skills.sort_by_key(|(_, x, y)| std::cmp::Reverse(x.max(y).to_owned()));
    Comparison { dps_pct, luck_free_pct, crit_rate_a: ra, crit_rate_b: rb, verdict, skills }
}

#[cfg(test)]
mod tests {
    use super::*;
    use meter_core::meter::{PlayerRow, TimelineEffect};

    fn hit(t: u32, player: u64, damage: u64, flags: u8) -> TimelineHit {
        TimelineHit { t, player, target: 9, skill: 11_000_000, cast: 0, damage, flags: flags | TimelineHit::FIRST, n: 1 }
    }

    fn analysis(hits: Vec<TimelineHit>, effects: Vec<TimelineEffect>) -> Analysis {
        let mut view: EncounterView = serde_json::from_str(
            r#"{"active":false,"duration_s":0,"total_damage":0,"total_dps":0,"target":null,"target_id":null,"target_type":null,"target_count":1,"start_us":0,"end_us":0,"players":[]}"#,
        )
        .unwrap();
        let mut me: PlayerRow = serde_json::from_str(
            r#"{"id":1,"name":"Me","is_self":true,"class_code":null,"class_name":null,"damage":0,"dps":0,"share":1,"hits":0,"max_hit":0,"skills":[]}"#,
        )
        .unwrap();
        me.is_self = true;
        view.players.push(me);
        Analysis { view, hits, effects, names: BTreeMap::from([(5, "Helper".to_string())]), mob_types: BTreeMap::new(), target_hp: vec![], heals: vec![], taken: vec![] }
    }

    #[test]
    fn a_full_run_is_scored_and_a_short_one_dropped() {
        // A hit every second for 10 s, every other one a crit (double damage).
        let hits: Vec<TimelineHit> = (0..10).map(|s| hit(1000 + s * 1000, 1, if s % 2 == 0 { 200 } else { 100 }, if s % 2 == 0 { TimelineHit::CRIT } else { 0 })).collect();
        let run = DummyRun::from_fight(&analysis(hits.clone(), vec![]), 10, 5, Some("Me".into()), "set A").unwrap();
        assert_eq!((run.damage, run.hits, run.crits, run.casts), (1500, 10, 5, 10));
        assert_eq!(run.curve[0], 200);
        assert_eq!(run.curve[9], 1500);
        assert!(run.clean && run.dps == 150.0 && run.label == "set A");
        assert_eq!((run.crit_avg, run.normal_avg), (200.0, 100.0));
        // Stopping at 5 s of a 30 s run: not saved.
        assert!(DummyRun::from_fight(&analysis(hits, vec![]), 30, 5, None, "").is_none());
    }

    #[test]
    fn someone_elses_buff_makes_it_not_clean() {
        let hits: Vec<TimelineHit> = (0..10).map(|s| hit(s * 1000, 1, 100, 0)).collect();
        let buff = TimelineEffect { start: 2000, end: 4000, before: false, open: false, target: 1, source: 5, skill: None, effect: 1, duration_ms: 0 };
        let run = DummyRun::from_fight(&analysis(hits, vec![buff]), 10, 5, None, "").unwrap();
        assert!(!run.clean);
        assert_eq!(run.helpers, vec!["Helper".to_string()]);
    }

    #[test]
    fn crit_luck_is_taken_out_of_a_comparison() {
        let run = |crits: u32| DummyRun {
            ended_ms: 0,
            character: None,
            label: String::new(),
            run_s: 10,
            damage: 0,
            // 10 hits of 100, crits 200: DPS follows the crits.
            dps: f64::from(10 * 100 + crits * 100) / 10.0,
            hits: 10,
            crits,
            back: 0,
            casts: 10,
            crit_avg: 200.0,
            normal_avg: 100.0,
            clean: true,
            helpers: vec![],
            curve: vec![],
            skills: vec![],
        };
        let c = compare(&run(2), &run(4));
        assert!(c.dps_pct > 15.0);
        assert!(c.luck_free_pct.abs() < 0.01);
        assert_eq!(c.verdict, "crit luck");
    }
}
