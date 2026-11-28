//! The farming widget: what you've gained since you last reset it (Abyss Points, Kina,
//! souls, materials, anything that goes into your inventory) and at what rate.
//!
//! Gains come from the meter's inventory ([`meter_core::inventory`]): every change in how
//! many of an item you carry. Only what you gain counts toward a rate; what you spend is
//! kept beside it (buying something doesn't undo an hour's farming).
//!
//! It lives in memory: a run starts with the first gain after the app starts or after
//! [reset], and is gone when the app closes.

use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;

use serde::Serialize;
use tauri::State;

use meter_core::inventory::{ItemChange, ABYSS_POINTS, KINA, KINA_BOUND};

use crate::gamedata::GameData;
use crate::{lock, AppState};

/// The items the widget lists until you choose your own.
pub const DEFAULT_ITEMS: [u32; 3] = [ABYSS_POINTS, KINA_BOUND, KINA];
/// The stretches a rate can be measured over, in minutes (0: since the run began).
pub const WINDOWS_MIN: [u32; 4] = [0, 5, 15, 60];
/// What a rate is given per.
pub const PER: [&str; 2] = ["hour", "minute"];
/// The longest stretch gains are remembered one by one, for the rates over a window.
const KEPT_MS: u64 = 60 * 60 * 1000;
/// Gains remembered one by one, at most (a kill is two or three).
const KEPT: usize = 50_000;
/// A rate over less than this says nothing yet.
const MIN_RATE_MS: u64 = 20_000;
/// Other items listed with [all], at most.
const OTHERS: usize = 40;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct Tally {
    gained: u64,
    spent: u64,
    /// What you carried after its last change.
    have: u64,
}

/// A farming run: everything gained and spent since it began.
#[derive(Debug, Default)]
pub struct Farm {
    /// When the run's first change came (ms since 1970).
    started_ms: Option<u64>,
    tallies: BTreeMap<u32, Tally>,
    /// The last hour's gains: when, what, how many.
    recent: VecDeque<(u64, u32, u64)>,
}

impl Farm {
    pub fn add(&mut self, now_ms: u64, changes: &[ItemChange]) {
        if changes.is_empty() {
            return;
        }
        self.started_ms.get_or_insert(now_ms);
        for c in changes {
            let t = self.tallies.entry(c.item).or_default();
            t.have = c.total;
            if c.delta > 0 {
                t.gained += c.delta as u64;
                self.recent.push_back((now_ms, c.item, c.delta as u64));
            } else {
                t.spent += c.delta.unsigned_abs();
            }
        }
        while self.recent.len() > KEPT || self.recent.front().is_some_and(|(at, _, _)| now_ms.saturating_sub(*at) > KEPT_MS) {
            self.recent.pop_front();
        }
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// How long the run has gone on.
    fn elapsed_ms(&self, now_ms: u64) -> u64 {
        self.started_ms.map_or(0, |s| now_ms.saturating_sub(s))
    }

    /// An item's gain per hour over the last `window_min` minutes (0: over the whole run).
    /// None until there's enough time to go by.
    fn per_hour(&self, item: u32, window_min: u32, now_ms: u64) -> Option<f64> {
        let elapsed = self.elapsed_ms(now_ms);
        let window_ms = u64::from(window_min) * 60_000;
        let (gained, over_ms) = if window_min == 0 || elapsed <= window_ms {
            (self.tallies.get(&item).map_or(0, |t| t.gained), elapsed)
        } else {
            let from = now_ms - window_ms;
            (self.recent.iter().filter(|(at, i, _)| *i == item && *at >= from).map(|(_, _, n)| *n).sum(), window_ms)
        };
        (over_ms >= MIN_RATE_MS).then(|| gained as f64 * 3_600_000.0 / over_ms as f64)
    }
}

#[derive(Debug, Serialize, PartialEq)]
pub struct FarmRow {
    item: u32,
    name: String,
    grade: Option<String>,
    /// One of the items always listed.
    pinned: bool,
    gained: u64,
    spent: u64,
    /// Per hour or per minute, as set; None until there's enough time to go by.
    rate: Option<f64>,
    /// How many you carry now (None before its first change, with the game's list not
    /// in yet).
    have: Option<u64>,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct FarmView {
    /// Seconds since the run's first gain (None: nothing yet).
    elapsed_s: Option<f64>,
    /// One of [`WINDOWS_MIN`] and of [`PER`].
    window_min: u32,
    per: String,
    /// Everything gained is listed, not just the pinned items.
    all: bool,
    /// The pinned items, in your order, then (with `all`) the rest by how many you gained.
    rows: Vec<FarmRow>,
    /// Items gained that aren't listed (without `all`, or beyond what fits).
    more: usize,
}

/// How the widget is set: the items always listed, the stretch its rates go by (one of
/// [`WINDOWS_MIN`]), what they're per (one of [`PER`]), and whether everything gained is
/// listed.
#[derive(Debug, Clone, Copy)]
pub struct Shown<'a> {
    pub pinned: &'a [u32],
    pub window_min: u32,
    pub per: &'a str,
    pub all: bool,
}

/// What the widget shows now.
pub fn view(farm: &Farm, shown: Shown, inventory: Option<&meter_core::inventory::Inventory>, data: &GameData, now_ms: u64) -> FarmView {
    let Shown { pinned, window_min, per, all } = shown;
    let scale = if per == "minute" { 1.0 / 60.0 } else { 1.0 };
    let row = |item: u32, is_pinned: bool| {
        let t = farm.tallies.get(&item).copied();
        let (name, grade) = data.item_name(item);
        // What you carry: as of its last change, else from the game's list.
        let have = t.map(|t| t.have).or_else(|| inventory.filter(|i| i.known()).map(|i| i.total(item)));
        FarmRow {
            item,
            name,
            grade,
            pinned: is_pinned,
            gained: t.map_or(0, |t| t.gained),
            spent: t.map_or(0, |t| t.spent),
            rate: farm.per_hour(item, window_min, now_ms).map(|r| r * scale),
            have,
        }
    };
    let mut rows: Vec<FarmRow> = pinned.iter().map(|&i| row(i, true)).collect();
    let mut others: Vec<FarmRow> = farm.tallies.iter().filter(|(i, t)| !pinned.contains(i) && t.gained > 0).map(|(&i, _)| row(i, false)).collect();
    others.sort_by(|a, b| b.gained.cmp(&a.gained).then_with(|| a.name.cmp(&b.name)));
    let more = if all { others.len().saturating_sub(OTHERS) } else { others.len() };
    if all {
        rows.extend(others.into_iter().take(OTHERS));
    }
    FarmView { elapsed_s: farm.started_ms.map(|_| farm.elapsed_ms(now_ms) as f64 / 1000.0), window_min, per: per.to_string(), all, rows, more }
}

#[tauri::command]
pub fn farm_view(app: State<'_, Arc<AppState>>) -> FarmView {
    let now = crate::hotkeys::now_ms();
    let meter = app.engine.meter();
    let farm = lock(&app.farm);
    let data = lock(&app.data);
    let s = lock(&app.settings);
    let shown = Shown { pinned: &s.farm_items, window_min: s.farm_window_min, per: &s.farm_per, all: s.farm_all };
    view(&farm, shown, Some(meter.inventory()), &data, now)
}

/// Starts the run over: nothing gained, no time gone.
#[tauri::command]
pub fn farm_reset(app: State<'_, Arc<AppState>>) {
    lock(&app.farm).reset();
}

/// Always lists an item (at the end of the pinned ones), or stops.
#[tauri::command]
pub fn set_farm_pinned(app: State<'_, Arc<AppState>>, item: u32, on: bool) {
    let mut s = lock(&app.settings);
    s.farm_items.retain(|i| *i != item);
    if on {
        s.farm_items.push(item);
    }
    s.save(&app.config_dir);
}

/// The stretch the rates are measured over (minutes; 0: the whole run).
#[tauri::command]
pub fn set_farm_window(app: State<'_, Arc<AppState>>, minutes: u32) -> Result<(), String> {
    if !WINDOWS_MIN.contains(&minutes) {
        return Err(format!("no such stretch: {minutes} minutes"));
    }
    let mut s = lock(&app.settings);
    s.farm_window_min = minutes;
    s.save(&app.config_dir);
    Ok(())
}

#[tauri::command]
pub fn set_farm_per(app: State<'_, Arc<AppState>>, per: String) -> Result<(), String> {
    if !PER.contains(&per.as_str()) {
        return Err(format!("rates are per hour or per minute, not per {per}"));
    }
    let mut s = lock(&app.settings);
    s.farm_per = per;
    s.save(&app.config_dir);
    Ok(())
}

/// Lists everything gained, or only the pinned items.
#[tauri::command]
pub fn set_farm_all(app: State<'_, Arc<AppState>>, on: bool) {
    let mut s = lock(&app.settings);
    s.farm_all = on;
    s.save(&app.config_dir);
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIN: u64 = 60_000;
    const SOUL: u32 = 534_660_175;

    fn gain(item: u32, delta: i64, total: u64) -> ItemChange {
        ItemChange { item, delta, total }
    }

    fn rows(v: &FarmView) -> Vec<(u32, u64, u64, Option<i64>)> {
        v.rows.iter().map(|r| (r.item, r.gained, r.spent, r.rate.map(|x| x.round() as i64))).collect()
    }

    #[test]
    fn gains_add_up_and_give_a_rate_since_the_first_one() {
        let data = GameData::load(Vec::new(), None);
        let mut farm = Farm::default();
        let t0 = 1_000_000_000;
        // Nothing yet: the pinned items show, with nothing gained and no time gone.
        let v = view(&farm, Shown { pinned: &DEFAULT_ITEMS, window_min: 0, per: "hour", all: false }, None, &data, t0);
        assert_eq!(v.elapsed_s, None);
        assert_eq!(rows(&v), [(ABYSS_POINTS, 0, 0, None), (KINA_BOUND, 0, 0, None), (KINA, 0, 0, None)]);
        // A kill: points and bound Kina, and a soul that's used at once.
        farm.add(t0, &[gain(ABYSS_POINTS, 90, 1631), gain(KINA_BOUND, 892, 3_141_998), gain(SOUL, 1, 1)]);
        farm.add(t0 + 100, &[gain(SOUL, -1, 0)]);
        // Too soon for a rate.
        assert_eq!(rows(&view(&farm, Shown { pinned: &DEFAULT_ITEMS, window_min: 0, per: "hour", all: false }, None, &data, t0 + 5_000))[0], (ABYSS_POINTS, 90, 0, None));
        farm.add(t0 + 10 * MIN, &[gain(ABYSS_POINTS, 510, 2141), gain(KINA, -160_000, 9_000_000)]);
        let v = view(&farm, Shown { pinned: &DEFAULT_ITEMS, window_min: 0, per: "hour", all: false }, None, &data, t0 + 10 * MIN);
        assert_eq!(v.elapsed_s, Some(600.0));
        // 600 points in 10 minutes: 3,600 an hour. What you spend is no loss of rate.
        assert_eq!(rows(&v), [(ABYSS_POINTS, 600, 0, Some(3600)), (KINA_BOUND, 892, 0, Some(5352)), (KINA, 0, 160_000, Some(0))]);
        assert_eq!((v.rows[0].have, v.rows[2].have), (Some(2141), Some(9_000_000)));
        // The soul isn't pinned: counted as one more, listed with [all].
        assert_eq!((v.more, v.all), (1, false));
        let all = view(&farm, Shown { pinned: &DEFAULT_ITEMS, window_min: 0, per: "minute", all: true }, None, &data, t0 + 10 * MIN);
        assert_eq!(all.more, 0);
        assert_eq!(rows(&all)[3], (SOUL, 1, 1, Some(0)));
        // Per minute: 60 points a minute.
        assert_eq!(all.rows[0].rate.map(|r| r.round() as i64), Some(60));
    }

    #[test]
    fn a_rate_over_the_last_minutes_forgets_what_came_before() {
        let data = GameData::load(Vec::new(), None);
        let mut farm = Farm::default();
        let t0 = 1_000_000_000;
        farm.add(t0, &[gain(ABYSS_POINTS, 6000, 6000)]);
        farm.add(t0 + 28 * MIN, &[gain(ABYSS_POINTS, 100, 6100)]);
        let now = t0 + 30 * MIN;
        let rate = |window| view(&farm, Shown { pinned: &[ABYSS_POINTS], window_min: window, per: "hour", all: false }, None, &data, now).rows[0].rate.map(|r| r.round() as i64);
        // The whole run: 6,100 in half an hour.
        assert_eq!(rate(0), Some(12_200));
        // The last 5 minutes: only the 100.
        assert_eq!(rate(5), Some(1200));
        // An hour's window on a half-hour run is the run.
        assert_eq!(rate(60), Some(12_200));
        // Reset: everything goes, the pins stay listed.
        farm.reset();
        let v = view(&farm, Shown { pinned: &[ABYSS_POINTS], window_min: 0, per: "hour", all: false }, None, &data, now);
        assert_eq!((v.elapsed_s, rows(&v)), (None, vec![(ABYSS_POINTS, 0, 0, None)]));
    }

    #[test]
    fn old_gains_are_dropped_from_the_window_but_not_from_the_total() {
        let mut farm = Farm::default();
        let t0 = 1_000_000_000;
        farm.add(t0, &[gain(ABYSS_POINTS, 10, 10)]);
        farm.add(t0 + 2 * KEPT_MS, &[gain(ABYSS_POINTS, 5, 15)]);
        assert_eq!(farm.recent.len(), 1);
        assert_eq!(farm.tallies[&ABYSS_POINTS].gained, 15);
    }
}
