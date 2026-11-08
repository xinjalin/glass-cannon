//! The weekly Odyle checklist: each character's Odyle energy and its weekly sources, and
//! how much of each is left this week (`checklist.json`).
//!
//! From the game: the energy itself (ticket [`ODYLE_ENERGY`]) and the weekly buy of Odyle
//! Energy for Kina at the trade guild (shop counter [`ODYLE_PURCHASE`], 20 a week), each
//! read when the game sends them (the energy at login and as it changes, the counter at
//! login and when a shop opens). Ticked by hand: the two weekly conversions into Odyle
//! Energy (16 and 4 a week, as SkeeveAN's Aion DPS Meter lists them), which no meter has
//! found in the game's messages yet. Everything weekly starts again at the game's weekly
//! reset, Wednesday 07:00 UTC (16:00 in Korea).

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use meter_core::contents::{ODYLE_ENERGY, ODYLE_PURCHASE, ODYLE_PURCHASE_MAX};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::{lock, AppState};

const WEEK_MS: u64 = 7 * 24 * 3_600_000;
/// The first weekly reset after 1970: Wednesday 7 January, 07:00 UTC.
const FIRST_RESET_MS: u64 = 6 * 24 * 3_600_000 + 7 * 3_600_000;

/// The weekly sources ticked by hand: key, what it is, how many a week.
pub const MANUAL: [(&str, &str, u32); 2] = [
    ("convert16", "weekly conversion (16 a week)", 16),
    ("convert4", "weekly conversion (4 a week)", 4),
];

/// The latest weekly reset at or before `now_ms`.
pub fn last_reset(now_ms: u64) -> u64 {
    if now_ms < FIRST_RESET_MS {
        return 0;
    }
    FIRST_RESET_MS + (now_ms - FIRST_RESET_MS) / WEEK_MS * WEEK_MS
}

fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

/// One character's week.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Week {
    /// Odyle energy as last sent: basic and charged, and when.
    #[serde(default)]
    pub energy: Option<(u32, u32)>,
    #[serde(default)]
    pub energy_ms: u64,
    /// The weekly Kina buy: how many bought, when that count resets (the game's), when read.
    #[serde(default)]
    pub bought: Option<u32>,
    #[serde(default)]
    pub bought_reset_ms: u64,
    #[serde(default)]
    pub bought_ms: u64,
    /// The sources ticked by hand ([`MANUAL`]'s keys), and the week they count for (its
    /// reset).
    #[serde(default)]
    pub manual: BTreeMap<String, u32>,
    #[serde(default)]
    pub manual_week_ms: u64,
}

pub struct Checklist {
    path: PathBuf,
    /// Character name -> its week.
    by_character: BTreeMap<String, Week>,
}

impl Checklist {
    pub fn load(dir: &Path) -> Self {
        let path = dir.join("checklist.json");
        let by_character = fs::read_to_string(&path).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        Self { path, by_character }
    }

    fn save(&self) {
        if let Some(dir) = self.path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        if let Ok(s) = serde_json::to_string_pretty(&self.by_character) {
            let _ = fs::write(&self.path, s);
        }
    }

    /// Takes what the game sent about `character`'s Odyle energy and the weekly buy.
    pub fn update(&mut self, character: Option<&str>, c: &meter_core::meter::Contents, now_ms: u64) {
        let Some(character) = character else { return };
        let odyle = c.tickets.iter().find(|t| t.id == ODYLE_ENERGY);
        let buy = c.shop.iter().find(|s| s.id == ODYLE_PURCHASE);
        if odyle.is_none() && buy.is_none() {
            return;
        }
        let w = self.by_character.entry(character.to_string()).or_default();
        if let Some(t) = odyle {
            let before = w.energy.unwrap_or((0, 0));
            w.energy = Some((t.count.unwrap_or(before.0), t.charged.unwrap_or(before.1)));
            w.energy_ms = now_ms;
        }
        if let Some(s) = buy {
            (w.bought, w.bought_reset_ms, w.bought_ms) = (Some(s.bought), s.reset_ms, now_ms);
        }
        self.save();
    }

    /// Counts a hand-ticked source up or down (0 to its weekly amount), for this week.
    pub fn step(&mut self, character: &str, key: &str, delta: i32, now_ms: u64) -> Result<(), String> {
        let (_, _, max) = MANUAL.iter().find(|(k, _, _)| *k == key).ok_or_else(|| format!("no source called {key}"))?;
        let w = self.by_character.entry(character.to_string()).or_default();
        let week = last_reset(now_ms);
        if w.manual_week_ms < week {
            w.manual.clear();
            w.manual_week_ms = week;
        }
        let n = w.manual.entry(key.to_string()).or_insert(0);
        *n = (i64::from(*n) + i64::from(delta)).clamp(0, i64::from(*max)) as u32;
        self.save();
        Ok(())
    }
}

/// A weekly source, as the window shows it.
#[derive(Debug, Serialize, PartialEq)]
pub struct Source {
    pub key: String,
    pub label: String,
    /// How many done this week (None: not known yet), of how many.
    pub done: Option<u32>,
    pub max: u32,
    /// Read from the game (else ticked by hand).
    pub auto: bool,
    /// When the game last said (ms since 1970).
    pub seen_ms: Option<u64>,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct CharacterWeek {
    pub name: String,
    pub energy: Option<(u32, u32)>,
    pub energy_ms: Option<u64>,
    pub sources: Vec<Source>,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct ChecklistView {
    /// The character you're playing, when known (shown first).
    pub you: Option<String>,
    pub characters: Vec<CharacterWeek>,
    /// When this week started and when the next reset is.
    pub week_ms: u64,
    pub next_reset_ms: u64,
}

pub fn view(c: &Checklist, you: Option<String>, now_ms: u64) -> ChecklistView {
    let week = last_reset(now_ms);
    let mut names: Vec<&String> = c.by_character.keys().collect();
    // You first, even before the game has sent anything about you.
    if let Some(me) = you.as_ref().filter(|me| !c.by_character.contains_key(*me)) {
        names.insert(0, me);
    }
    let empty = Week::default();
    let characters = names
        .into_iter()
        .map(|name| {
            let w = c.by_character.get(name).unwrap_or(&empty);
            // A count from before this week's reset is over: none bought yet this week.
            let bought = w.bought.map(|b| if w.bought_reset_ms <= now_ms { 0 } else { b });
            let mut sources = vec![Source {
                key: "buy".into(),
                label: "Kina buy at the trade guild".into(),
                done: bought,
                max: ODYLE_PURCHASE_MAX,
                auto: true,
                seen_ms: (w.bought_ms > 0).then_some(w.bought_ms),
            }];
            let this_week = w.manual_week_ms >= week;
            for (key, label, max) in MANUAL {
                sources.push(Source {
                    key: key.into(),
                    label: label.into(),
                    done: Some(if this_week { w.manual.get(key).copied().unwrap_or(0) } else { 0 }),
                    max,
                    auto: false,
                    seen_ms: None,
                });
            }
            CharacterWeek { name: name.clone(), energy: w.energy, energy_ms: (w.energy_ms > 0).then_some(w.energy_ms), sources }
        })
        .collect::<Vec<_>>();
    let mut characters = characters;
    if let Some(me) = &you {
        characters.sort_by_key(|ch| &ch.name != me);
    }
    ChecklistView { you, characters, week_ms: week, next_reset_ms: week + WEEK_MS }
}

#[tauri::command]
pub fn checklist(app: State<'_, Arc<AppState>>) -> ChecklistView {
    let you = lock(&app.profile).name.clone();
    view(&lock(&app.checklist), you, now_ms())
}

/// Ticks a hand-counted weekly source up (1) or down (-1) for a character.
#[tauri::command]
pub fn step_checklist(app: State<'_, Arc<AppState>>, character: String, key: String, delta: i32) -> Result<(), String> {
    lock(&app.checklist).step(&character, &key, delta.clamp(-100, 100), now_ms())
}

/// Called with what the meter read since its last snapshot.
pub fn take(app: &AppState, c: &meter_core::meter::Contents) {
    let me = lock(&app.profile).name.clone();
    lock(&app.checklist).update(me.as_deref(), c, now_ms());
}

#[cfg(test)]
mod tests {
    use super::*;
    use meter_core::contents::{ShopCounter, Ticket};

    const HOUR: u64 = 3_600_000;
    /// Wednesday 2026-10-28 07:00 UTC (a reset, from our capture).
    const RESET: u64 = 1_793_170_800_000;

    fn temp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("meter-checklist-{tag}-{}-{}", std::process::id(), now_ms()));
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn the_week_starts_wednesday_at_seven_utc() {
        assert_eq!(last_reset(RESET), RESET);
        assert_eq!(last_reset(RESET - 1), RESET - WEEK_MS);
        assert_eq!(last_reset(RESET + 3 * 24 * HOUR), RESET);
    }

    #[test]
    fn a_characters_week() {
        let dir = temp_dir("week");
        let mut c = Checklist::load(&dir);
        let now = RESET - 24 * HOUR;
        let sent = meter_core::meter::Contents {
            tickets: vec![Ticket { id: ODYLE_ENERGY, count: Some(355), charged: Some(1345) }],
            shop: vec![ShopCounter { id: ODYLE_PURCHASE, bought: 12, reset_ms: RESET }, ShopCounter { id: 1169, bought: 3, reset_ms: RESET }],
            ..Default::default()
        };
        c.update(Some("Nene"), &sent, now);
        c.step("Nene", "convert16", 5, now).unwrap();
        c.step("Nene", "convert4", 9, now).unwrap();
        assert!(c.step("Nene", "nothing", 1, now).is_err());

        let v = view(&Checklist::load(&dir), Some("Nene".into()), now);
        let me = &v.characters[0];
        assert_eq!((me.name.as_str(), me.energy), ("Nene", Some((355, 1345))));
        let done: Vec<Option<u32>> = me.sources.iter().map(|s| s.done).collect();
        assert_eq!(done, [Some(12), Some(5), Some(4)], "the buy from the game; by hand, never past the week's amount");
        assert_eq!(v.next_reset_ms, RESET);

        // After the reset: a new week, nothing done yet.
        let v = view(&Checklist::load(&dir), Some("Nene".into()), RESET + HOUR);
        let done: Vec<Option<u32>> = v.characters[0].sources.iter().map(|s| s.done).collect();
        assert_eq!(done, [Some(0), Some(0), Some(0)]);
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn you_show_first_before_the_game_says_anything() {
        let dir = temp_dir("you");
        let mut c = Checklist::load(&dir);
        let sent = meter_core::meter::Contents { shop: vec![ShopCounter { id: ODYLE_PURCHASE, bought: 20, reset_ms: RESET }], ..Default::default() };
        c.update(Some("Alt"), &sent, RESET - HOUR);
        let v = view(&c, Some("Nene".into()), RESET - HOUR);
        let names: Vec<&str> = v.characters.iter().map(|ch| ch.name.as_str()).collect();
        assert_eq!(names, ["Nene", "Alt"]);
        assert_eq!(v.characters[0].sources[0].done, None, "not known yet");
        fs::remove_dir_all(dir).ok();
    }
}
