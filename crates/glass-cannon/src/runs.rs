//! Dungeon runs: fights in a row in the same dungeon, grouped for the history.
//!
//! A run starts at a fight on a dungeon's mob and goes on while the fights keep coming
//! (no gap longer than [`RUN_GAP_MS`]) and stay in that dungeon. Fights on mobs the data
//! doesn't place (adds, summons) between two of the dungeon's fights belong to the run;
//! ones after its last dungeon fight don't. A fight in another dungeon ends it.
//!
//! Going straight back into the same dungeon isn't a gap or another dungeon, so a run also
//! ends when a boss it already fought comes round again (other than a retry of the boss
//! just fought, after a wipe): the run ended at its last boss, and the next one starts with
//! the fights after that. Against 266 dungeon fights recorded by another meter (which reads
//! instance entry from the game), this grouping agreed on 255.

use std::collections::{HashMap, HashSet};
use std::ops::Range;

use serde::Serialize;

use crate::gamedata::{DungeonPlace, GameData};
use crate::store::FightSummary;

/// The longest quiet spell inside a run: walking between packs and bosses takes a while.
pub const RUN_GAP_MS: u64 = 10 * 60 * 1000;

/// What grouping needs to know about one fight.
#[derive(Debug, Clone, Copy)]
pub struct Leg<'a> {
    pub start_ms: u64,
    pub end_ms: u64,
    /// The dungeon its mob is in (an ID), if the data places it.
    pub dungeon: Option<&'a str>,
    /// The boss it was on (its mob type), if it was a boss fight.
    pub boss: Option<u32>,
}

/// The run being built.
struct Open<'a> {
    dungeon: &'a str,
    first: usize,
    /// The last fight on the dungeon's mobs.
    last: usize,
    /// When the latest fight in it ended.
    end_ms: u64,
    bosses: HashSet<u32>,
    /// The latest boss fought, and where.
    last_boss: Option<(u32, usize)>,
}

impl<'a> Open<'a> {
    fn start(dungeon: &'a str, first: usize, legs: &[Leg]) -> Self {
        let mut run = Open { dungeon, first, last: first, end_ms: 0, bosses: HashSet::new(), last_boss: None };
        for (i, leg) in legs.iter().enumerate().skip(first) {
            run.add(i, leg);
        }
        run
    }

    fn add(&mut self, i: usize, leg: &Leg) {
        self.end_ms = self.end_ms.max(leg.end_ms);
        if leg.dungeon.is_some() {
            self.last = i;
        }
        if let Some(b) = leg.boss {
            self.bosses.insert(b);
            self.last_boss = Some((b, i));
        }
    }

    /// Its fights, if there are at least two.
    fn close(self, runs: &mut Vec<(Range<usize>, &'a str)>) {
        if self.last > self.first {
            runs.push((self.first..self.last + 1, self.dungeon));
        }
    }
}

/// Groups fights (oldest first) into runs of two or more: each run's fights as a range of
/// indexes into `legs`, and its dungeon.
pub fn group<'a>(legs: &[Leg<'a>]) -> Vec<(Range<usize>, &'a str)> {
    let mut runs = Vec::new();
    let mut open: Option<Open<'a>> = None;
    for (i, leg) in legs.iter().enumerate() {
        let continues = open.as_ref().is_some_and(|run| {
            leg.start_ms.saturating_sub(run.end_ms) <= RUN_GAP_MS && leg.dungeon.is_none_or(|d| d == run.dungeon)
        });
        if !continues {
            if let Some(run) = open.take() {
                run.close(&mut runs);
            }
            open = leg.dungeon.map(|d| Open::start(d, i, &legs[..=i]));
            continue;
        }
        let run = open.as_mut().expect("checked above");
        let again = leg
            .boss
            .is_some_and(|b| run.bosses.contains(&b) && run.last_boss.is_some_and(|(last, _)| last != b));
        if !again {
            run.add(i, leg);
            continue;
        }
        // Back in for another run: this one ended at its last boss, and the next starts at
        // the first of the dungeon's fights after it.
        let (_, boss_at) = run.last_boss.expect("a boss was fought");
        let dungeon = run.dungeon;
        let next = (boss_at + 1..=i).find(|&j| legs[j].dungeon == Some(dungeon)).unwrap_or(i);
        let mut done = open.take().expect("checked above");
        done.last = boss_at;
        done.close(&mut runs);
        open = Some(Open::start(dungeon, next, &legs[..=i]));
    }
    if let Some(run) = open {
        run.close(&mut runs);
    }
    runs
}

/// Where a fight was: a dungeon and the mode it was in.
#[derive(Debug, Clone, Serialize, PartialEq, Eq, Hash)]
pub struct Place {
    pub name: String,
    /// The dungeon's type (Expedition, Transcendence…).
    pub kind: Option<String>,
    /// Exploration, Conquest, Stage 2…
    pub mode: Option<String>,
}

impl From<&DungeonPlace<'_>> for Place {
    fn from(p: &DungeonPlace<'_>) -> Self {
        Place { name: p.name.to_string(), kind: p.kind.map(str::to_string), mode: p.mode.map(str::to_string) }
    }
}

/// One dungeon run in the history list.
#[derive(Debug, Clone, Serialize)]
pub struct Run {
    pub place: Place,
    pub started_ms: u64,
    pub ended_ms: u64,
    /// Time spent fighting: its fights' lengths added up.
    pub fight_s: f64,
    pub fights: usize,
    pub bosses: usize,
    pub total_damage: u64,
    /// The most players in any of its fights.
    pub players: usize,
    /// Your damage over the time you spent fighting.
    pub my_dps: Option<f64>,
}

/// The history list: fights newest first, and the dungeon runs they make up.
#[derive(Debug, Serialize)]
pub struct HistoryList {
    pub fights: Vec<FightSummary>,
    pub runs: Vec<Run>,
}

/// Marks each fight on a dungeon mob with where it was, and groups the fights into runs.
/// `fights` is newest first (as the history lists them).
pub fn history_list(mut fights: Vec<FightSummary>, data: &GameData) -> HistoryList {
    let mut ids: Vec<Option<&str>> = Vec::with_capacity(fights.len());
    for f in fights.iter_mut() {
        let place = f.target_type.and_then(|t| data.dungeon(t));
        ids.push(place.as_ref().map(|p| p.id));
        f.dungeon = place.as_ref().map(Place::from);
    }
    // Grouping goes oldest first.
    let legs: Vec<Leg> = fights
        .iter()
        .zip(&ids)
        .rev()
        .map(|(f, d)| Leg {
            start_ms: f.started_ms,
            end_ms: f.ended_ms,
            dungeon: *d,
            boss: f.target_type.filter(|_| f.target_boss),
        })
        .collect();
    let n = fights.len();
    let mut runs = Vec::new();
    for (range, _) in group(&legs) {
        // Back to newest-first positions.
        let members: Vec<usize> = range.map(|i| n - 1 - i).collect();
        let run = runs.len();
        for &i in &members {
            fights[i].run = Some(run);
        }
        runs.push(summarise(members.iter().map(|&i| &fights[i])));
    }
    HistoryList { fights, runs }
}

fn summarise<'a>(fights: impl Iterator<Item = &'a FightSummary> + Clone) -> Run {
    // The place: where its bosses were (the mode can differ for trash), else most fights.
    let mut counts: HashMap<&Place, (usize, usize)> = HashMap::new();
    for f in fights.clone() {
        if let Some(p) = &f.dungeon {
            let c = counts.entry(p).or_default();
            c.0 += usize::from(f.target_boss);
            c.1 += 1;
        }
    }
    // Ties go to the later mode name, so the same history always reads the same.
    let place = counts
        .into_iter()
        .max_by(|(pa, a), (pb, b)| a.cmp(b).then_with(|| pa.mode.cmp(&pb.mode)))
        .map(|(p, _)| p.clone())
        .expect("a run has dungeon fights");
    let mut run = Run {
        place,
        started_ms: u64::MAX,
        ended_ms: 0,
        fight_s: 0.0,
        fights: 0,
        bosses: 0,
        total_damage: 0,
        players: 0,
        my_dps: None,
    };
    let (mut mine, mut secs, mut any_mine) = (0u64, 0.0, false);
    for f in fights {
        run.started_ms = run.started_ms.min(f.started_ms);
        run.ended_ms = run.ended_ms.max(f.ended_ms);
        run.fights += 1;
        run.bosses += usize::from(f.target_boss);
        run.total_damage += f.total_damage;
        run.players = run.players.max(f.players);
        secs += f.duration_s;
        if let Some(d) = f.my_damage {
            mine += d;
            any_mine = true;
        }
    }
    run.fight_s = secs;
    run.my_dps = (any_mine && secs > 0.0).then(|| mine as f64 / secs);
    run
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIN: u64 = 60_000;

    fn leg(start_min: u64, dungeon: Option<&'static str>) -> Leg<'static> {
        Leg { start_ms: start_min * MIN, end_ms: start_min * MIN + MIN, dungeon, boss: None }
    }

    fn boss(start_min: u64, dungeon: &'static str, boss: u32) -> Leg<'static> {
        Leg { boss: Some(boss), ..leg(start_min, Some(dungeon)) }
    }

    #[test]
    fn a_boss_coming_round_again_starts_the_next_run() {
        // Draupnir twice in a row, as recorded: trash, Saraswati (1), Lakshmi (2), Bakarma
        // (3) with a wipe and a retry, then straight back in: trash, Saraswati again.
        let d = "Draupnir";
        let legs = [
            leg(0, Some(d)),
            boss(1, d, 1),
            boss(3, d, 2),
            leg(5, Some(d)),
            boss(6, d, 3),
            boss(9, d, 3),
            leg(12, Some(d)),
            boss(13, d, 1),
            boss(15, d, 2),
        ];
        // The first run ends at the last Bakarma; the trash after it opens the second.
        assert_eq!(group(&legs), vec![(0..6, d), (6..9, d)]);
    }

    #[test]
    fn fights_in_one_dungeon_make_a_run() {
        let deus = Some("DeusCenter");
        let legs = [leg(0, None), leg(10, deus), leg(13, None), leg(16, deus), leg(19, deus), leg(22, None)];
        // The unplaced fight between Deus fights is in; the ones before and after aren't.
        assert_eq!(group(&legs), vec![(1..5, "DeusCenter")]);
    }

    #[test]
    fn a_long_break_or_another_dungeon_ends_a_run() {
        let (deus, krao) = (Some("DeusCenter"), Some("KraoCave"));
        let legs = [leg(0, deus), leg(3, deus), leg(30, deus), leg(33, deus), leg(35, krao), leg(37, krao)];
        assert_eq!(group(&legs), vec![(0..2, "DeusCenter"), (2..4, "DeusCenter"), (4..6, "KraoCave")]);
    }

    fn summary(index: usize, start_min: u64, target_type: u32, boss: bool) -> FightSummary {
        FightSummary {
            index,
            started_ms: start_min * MIN,
            ended_ms: start_min * MIN + MIN,
            target: None,
            target_type: Some(target_type),
            target_boss: boss,
            duration_s: 60.0,
            total_damage: 1000,
            total_dps: 1000.0 / 60.0,
            players: 2 + index,
            my_dps: Some(10.0),
            my_damage: Some(600),
            has_details: false,
            dungeon: None,
            run: None,
        }
    }

    #[test]
    fn the_list_marks_runs_newest_first() {
        let dir = std::env::temp_dir().join(format!("meter-runs-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("skills.json"), r#"{ "skills": { "18010000": { "name": "Onslaught" } } }"#).unwrap();
        std::fs::write(
            dir.join("dungeons.json"),
            r#"{ "dungeons": { "DeusCenter": { "name": "Deus Research Base", "type": "Transcendence" } },
                 "mobs": { "2340601": { "dungeon": "DeusCenter", "mode": "Stage 2" },
                           "2340602": { "dungeon": "DeusCenter", "mode": "Ordeal" } } }"#,
        )
        .unwrap();
        let data = GameData::load(vec![dir.clone()], None);
        // Oldest is index 0; the list comes newest first. A field mob, then a Deus run of
        // trash, an unplaced add and the boss.
        let mut fights = vec![
            summary(0, 0, 2_400_010, false),
            summary(1, 10, 2_340_602, false),
            summary(2, 12, 9_999_999, false),
            summary(3, 14, 2_340_601, true),
        ];
        fights.reverse();
        let list = history_list(fights, &data);
        let runs: Vec<Option<usize>> = list.fights.iter().map(|f| f.run).collect();
        assert_eq!(runs, [Some(0), Some(0), Some(0), None]);
        let run = &list.runs[0];
        // The boss fight's mode names the run, not the trash's.
        assert_eq!((run.place.name.as_str(), run.place.mode.as_deref()), ("Deus Research Base", Some("Stage 2")));
        assert_eq!((run.fights, run.bosses, run.total_damage, run.players), (3, 1, 3000, 5));
        assert_eq!((run.started_ms, run.ended_ms, run.fight_s), (10 * MIN, 15 * MIN, 180.0));
        assert_eq!(run.my_dps, Some(10.0));
        assert!(list.fights[1].dungeon.is_none(), "the unplaced add has no place of its own");
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn one_fight_is_not_a_run() {
        let legs = [leg(0, Some("DeusCenter")), leg(5, None), leg(8, Some("KraoCave"))];
        assert!(group(&legs).is_empty());
    }
}
