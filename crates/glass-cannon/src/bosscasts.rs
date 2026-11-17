//! The boss casts window: every boss, by content (expeditions, transcendence, raids…),
//! dungeon and name, with the distinct casts the meter has seen it make. Each cast can be
//! renamed (names.json, as anywhere) and set to show or not in the boss warnings and the
//! boss timers; left on auto, it goes by the general rule (a long cast, or one that marks
//! the ground, or one our data says to warn of), or as the app's defaults have it.
//!
//! A boss cast mapping (your names and choices) can be exported to a file and imported
//! from one; one made the app's `gamedata/boss-cast-defaults.json` is everyone's start.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use serde::Serialize;
use tauri::State;

use crate::gamedata::{CastChoice, CastMapping};

use crate::gamedata::GameData;
use crate::store::{BossCastBook, SeenCast};
use crate::{lock, AppState};

/// Content types, in the order the window's tabs show them. Bosses outside these (field
/// bosses, and any the data doesn't place) go last.
const KINDS: [&str; 8] = ["Expedition", "Transcendence", "Sanctuary", "Arena", "Sealed Dungeon", "Stronghold", "Dimensional Invasion", "Field boss"];
const OTHER: &str = "Other";

#[derive(Debug, Serialize, PartialEq)]
pub struct CastSetup {
    pub skill: u32,
    /// Its name (yours, the game's, or our label), when there's one.
    pub name: Option<String>,
    /// Your name for it, when you gave one.
    pub renamed: bool,
    pub cast_ms: u32,
    pub lands_ms: u32,
    pub seen: u32,
    pub fights: u32,
    pub last_ms: u64,
    /// Your choices (None: auto), and what auto does for each (the app's default, else the
    /// rule).
    pub warn: Option<bool>,
    pub timer: Option<bool>,
    pub auto_warn: bool,
    pub auto_timer: bool,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct BossSetup {
    pub name: String,
    /// Its mob types (one per mode) and the modes.
    pub types: Vec<u32>,
    pub modes: Vec<String>,
    pub casts: Vec<CastSetup>,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct PlaceSetup {
    pub name: String,
    pub bosses: Vec<BossSetup>,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct KindSetup {
    pub kind: String,
    pub places: Vec<PlaceSetup>,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct BossCastsView {
    pub kinds: Vec<KindSetup>,
}

/// One boss being gathered: its types and modes, and its casts over every mode.
#[derive(Default)]
struct Gather {
    types: BTreeSet<u32>,
    modes: BTreeSet<String>,
    casts: BTreeMap<u32, SeenCast>,
}

pub fn view(data: &GameData, book: &BossCastBook, warn: &BTreeMap<u32, bool>, timer: &BTreeMap<u32, bool>) -> BossCastsView {
    // kind -> place -> boss name -> gathered.
    let mut tree: BTreeMap<String, BTreeMap<String, BTreeMap<String, Gather>>> = BTreeMap::new();
    let mut add = |kind: &str, place: &str, mode: Option<&str>, boss: u32| {
        let name = data.npc_label(boss).unwrap_or_else(|| format!("Mob {boss}"));
        let g = tree.entry(kind.to_string()).or_default().entry(place.to_string()).or_default().entry(name).or_default();
        g.types.insert(boss);
        if let Some(m) = mode {
            g.modes.insert(m.to_string());
        }
        if let Some(casts) = book.bosses.get(&boss) {
            for (skill, s) in casts {
                let e = g.casts.entry(*skill).or_default();
                e.cast_ms = e.cast_ms.max(s.cast_ms);
                e.lands_ms = e.lands_ms.max(s.lands_ms);
                e.seen += s.seen;
                e.fights += s.fights;
                e.last_ms = e.last_ms.max(s.last_ms);
            }
        }
    };
    let mut placed = BTreeSet::new();
    for (boss, place) in data.dungeon_bosses() {
        add(place.kind.unwrap_or(OTHER), place.name, place.mode, boss);
        placed.insert(boss);
    }
    for (boss, place) in data.field_boss_types() {
        if placed.insert(boss) {
            add("Field boss", place.as_deref().unwrap_or("Field bosses"), None, boss);
        }
    }
    // Bosses the meter saw casting that the data doesn't place.
    for &boss in book.bosses.keys() {
        if placed.contains(&boss) {
            continue;
        }
        match data.dungeon(boss) {
            Some(p) => add(p.kind.unwrap_or(OTHER), p.name, p.mode, boss),
            None => add(OTHER, "Elsewhere", None, boss),
        }
    }
    let warn_casts: BTreeSet<u32> = data.warn_casts().iter().copied().collect();
    let (default_warns, default_timers) = (data.default_cast_warns(), data.default_cast_timers());
    let mut kinds: Vec<KindSetup> = tree
        .into_iter()
        .map(|(kind, places)| KindSetup {
            kind,
            places: places
                .into_iter()
                .map(|(name, bosses)| PlaceSetup {
                    name,
                    bosses: bosses
                        .into_iter()
                        .map(|(name, g)| BossSetup {
                            name,
                            types: g.types.into_iter().collect(),
                            modes: g.modes.into_iter().collect(),
                            casts: g
                                .casts
                                .into_iter()
                                .map(|(skill, s)| CastSetup {
                                    skill,
                                    name: data.boss_skill_name(skill),
                                    renamed: data.skill_renamed(skill),
                                    cast_ms: s.cast_ms,
                                    lands_ms: s.lands_ms,
                                    seen: s.seen,
                                    fights: s.fights,
                                    last_ms: s.last_ms,
                                    warn: warn.get(&skill).copied(),
                                    timer: timer.get(&skill).copied(),
                                    auto_warn: default_warns
                                        .get(&skill)
                                        .copied()
                                        .unwrap_or_else(|| warn_casts.contains(&skill) || crate::dangerous(&BTreeMap::new(), skill, s.cast_ms, s.lands_ms)),
                                    auto_timer: default_timers.get(&skill).copied().unwrap_or_else(|| {
                                        warn.get(&skill).copied().or_else(|| default_warns.get(&skill).copied()).unwrap_or_else(|| warn_casts.contains(&skill) || crate::dangerous(&BTreeMap::new(), skill, s.cast_ms, s.lands_ms))
                                    }),
                                })
                                .collect(),
                        })
                        .collect(),
                })
                .collect(),
        })
        .collect();
    let order = |k: &str| KINDS.iter().position(|x| *x == k).unwrap_or(KINDS.len());
    kinds.sort_by_key(|k| order(&k.kind));
    BossCastsView { kinds }
}

/// Your boss cast mapping: your names for boss skills, and your warn and timer choices.
fn mapping(data: &GameData, warn: &BTreeMap<u32, bool>, timer: &BTreeMap<u32, bool>) -> CastMapping {
    let mut casts: BTreeMap<u32, CastChoice> = BTreeMap::new();
    for (id, name) in data.boss_skill_renames() {
        casts.entry(id).or_default().name = Some(name);
    }
    for (id, on) in warn {
        casts.entry(*id).or_default().warn = Some(*on);
    }
    for (id, on) in timer {
        casts.entry(*id).or_default().timer = Some(*on);
    }
    let made = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    CastMapping { source: Some(serde_json::json!({ "made_by": "Glass Cannon's boss casts window", "version": env!("CARGO_PKG_VERSION"), "unix_time": made })), casts }
}

/// Saves your boss cast mapping to a file in the `exports` folder beside the app (as
/// recordings go to `captures`), named for the time, and shows it in Explorer; its path.
#[tauri::command]
pub fn export_boss_casts(app: State<'_, Arc<AppState>>) -> Result<String, String> {
    let (warn, timer) = {
        let s = lock(&app.settings);
        (s.boss_skill_alerts.clone(), s.boss_skill_timers.clone())
    };
    let m = mapping(&lock(&app.data), &warn, &timer);
    if m.casts.is_empty() {
        return Err("nothing to export yet: name a cast, or set one to warn or not".into());
    }
    let dir = app.exe_dir.join("exports");
    std::fs::create_dir_all(&dir).map_err(|e| format!("couldn't make {}: {e}", dir.display()))?;
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let path = dir.join(format!("glass-cannon-boss-casts-{secs}.json"));
    let json = serde_json::to_vec_pretty(&m).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| format!("couldn't save {}: {e}", path.display()))?;
    let _ = std::process::Command::new("explorer").arg(format!("/select,{}", path.display())).spawn();
    Ok(path.display().to_string())
}

/// What an import changed.
#[derive(Debug, Serialize, PartialEq)]
pub struct Imported {
    pub names: usize,
    pub warns: usize,
    pub timers: usize,
}

/// Takes in a boss cast mapping (a file's text): its names and choices go over yours.
#[tauri::command]
pub fn import_boss_casts(app: State<'_, Arc<AppState>>, json: String) -> Result<Imported, String> {
    let m: CastMapping = serde_json::from_str(&json).map_err(|_| "that isn't a boss cast mapping (a file exported from this window)".to_string())?;
    // Boss and mob skills only (players' skills are 11xxxxxx and up).
    let casts: Vec<(u32, CastChoice)> = m.casts.into_iter().filter(|(id, _)| *id < 10_000_000).collect();
    if casts.is_empty() {
        return Err("no boss casts in that file".into());
    }
    let names: BTreeMap<u32, String> = casts.iter().filter_map(|(id, c)| Some((*id, c.name.clone()?))).collect();
    let named = if names.is_empty() { 0 } else { lock(&app.data).rename_skills(&names).map_err(|e| format!("{e:#}"))? };
    let mut s = lock(&app.settings);
    let (mut warns, mut timers) = (0, 0);
    for (id, c) in &casts {
        if let Some(on) = c.warn {
            s.boss_skill_alerts.insert(*id, on);
            warns += 1;
        }
        if let Some(on) = c.timer {
            s.boss_skill_timers.insert(*id, on);
            timers += 1;
        }
    }
    s.save(&app.config_dir);
    Ok(Imported { names: named, warns, timers })
}

#[tauri::command]
pub fn boss_casts_setup(app: State<'_, Arc<AppState>>) -> BossCastsView {
    let (warn, timer) = {
        let s = lock(&app.settings);
        (s.boss_skill_alerts.clone(), s.boss_skill_timers.clone())
    };
    view(&lock(&app.data), &lock(&app.boss_casts), &warn, &timer)
}

/// Sets a boss skill for the boss timers: always (true), never (false), or as its warning
/// goes (None).
#[tauri::command]
pub fn set_boss_skill_timer(app: State<'_, Arc<AppState>>, skill: u32, timer: Option<bool>) {
    let mut s = lock(&app.settings);
    match timer {
        Some(on) => s.boss_skill_timers.insert(skill, on),
        None => s.boss_skill_timers.remove(&skill),
    };
    s.save(&app.config_dir);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn bosses_by_content_dungeon_and_name_with_their_casts() {
        let dir = std::env::temp_dir().join(format!("gc-bosscasts-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("skills.json"), r#"{ "skills": { "1803010": { "name": "Ludra's Trial" }, "1803200": { "name": "Attack" } } }"#).unwrap();
        fs::write(
            dir.join("npcs.json"),
            r#"{ "npcs": { "2301014": { "name": "Eternal Ludra", "boss": true }, "2311014": { "name": "Eternal Ludra", "boss": true }, "2301006": { "name": "Rahu", "boss": true }, "2300100": { "name": "A Mob" } } }"#,
        )
        .unwrap();
        fs::write(
            dir.join("dungeons.json"),
            r#"{ "dungeons": { "ReforgedAbyss": { "name": "Abyssal Forge: Ludra", "type": "Sanctuary" } },
                 "mobs": { "2301014": { "dungeon": "ReforgedAbyss", "mode": "Conquest" }, "2311014": { "dungeon": "ReforgedAbyss", "mode": "Exploration" },
                           "2301006": { "dungeon": "ReforgedAbyss", "mode": "Conquest" }, "2300100": { "dungeon": "ReforgedAbyss", "mode": "Conquest" } } }"#,
        )
        .unwrap();
        let data = GameData::load_for_tests(dir.clone());
        let mut book = BossCastBook::load(&dir, &crate::store::History::load(&dir));
        let seen = |cast_ms, lands_ms| SeenCast { cast_ms, lands_ms, seen: 1, fights: 1, last_ms: 5 };
        // Ludra in two modes: her casts together.
        book.bosses.insert(2_301_014, [(1_803_010, seen(4000, 0))].into());
        book.bosses.insert(2_311_014, [(1_803_200, seen(1000, 0)), (1_803_010, seen(3000, 1200))].into());
        let v = view(&data, &book, &[(1_803_200, false)].into(), &[(1_803_010, true)].into());
        assert_eq!(v.kinds.len(), 1);
        let place = &v.kinds[0].places[0];
        assert_eq!((v.kinds[0].kind.as_str(), place.name.as_str()), ("Sanctuary", "Abyssal Forge: Ludra"));
        // Bosses only (not "A Mob"), by name.
        assert_eq!(place.bosses.iter().map(|b| b.name.as_str()).collect::<Vec<_>>(), ["Eternal Ludra", "Rahu"]);
        let ludra = &place.bosses[0];
        assert_eq!((ludra.types.clone(), ludra.modes.clone()), (vec![2_301_014, 2_311_014], vec!["Conquest".to_string(), "Exploration".to_string()]));
        let trial = &ludra.casts[0];
        assert_eq!((trial.skill, trial.name.as_deref(), trial.cast_ms, trial.lands_ms, trial.seen), (1_803_010, Some("Ludra's Trial"), 4000, 1200, 2));
        assert_eq!((trial.warn, trial.timer, trial.auto_warn), (None, Some(true), true));
        // A short cast the game calls "Attack": no name, auto quiet, your "never" kept.
        let attack = &ludra.casts[1];
        assert_eq!((attack.name.as_deref(), attack.warn, attack.auto_warn), (None, Some(false), false));
        // Rahu, never fought: listed with no casts.
        assert!(place.bosses[1].casts.is_empty());
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn the_apps_mapping_names_and_sets_casts_and_yours_wins() {
        let dir = std::env::temp_dir().join(format!("gc-castmap-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("skills.json"), r#"{ "skills": { "1803200": { "name": "Attack" }, "1803010": { "name": "Ludra's Trial" } } }"#).unwrap();
        fs::write(dir.join("npcs.json"), r#"{ "npcs": { "2301014": { "name": "Eternal Ludra", "boss": true } } }"#).unwrap();
        fs::write(
            dir.join("boss-cast-defaults.json"),
            r#"{ "casts": { "1803200": { "name": "tail swipe", "warn": true }, "1803010": { "timer": false }, "1803300": { "name": "bad\nname" } } }"#,
        )
        .unwrap();
        let data = GameData::load_for_tests(dir.clone());
        // Its name over the game's "Attack"; one that won't do left out.
        assert_eq!(data.boss_skill_name(1_803_200).as_deref(), Some("tail swipe"));
        assert_eq!(data.boss_skill_name(1_803_300), None);
        assert_eq!(data.default_cast_warns(), [(1_803_200, true)].into());
        assert_eq!(data.default_cast_timers(), [(1_803_010, false)].into());
        let mut book = BossCastBook::load(&dir, &crate::store::History::load(&dir));
        let seen = |cast_ms| SeenCast { cast_ms, lands_ms: 0, seen: 1, fights: 1, last_ms: 0 };
        book.bosses.insert(2_301_014, [(1_803_200, seen(1000)), (1_803_010, seen(4000))].into());
        // Auto is the app's choice where it has one; yours over it.
        let v = view(&data, &book, &BTreeMap::new(), &[(1_803_010, true)].into());
        let casts = &v.kinds[0].places[0].bosses[0].casts;
        let swipe = casts.iter().find(|c| c.skill == 1_803_200).unwrap();
        assert_eq!((swipe.auto_warn, swipe.auto_timer, swipe.renamed), (true, true, false));
        let trial = casts.iter().find(|c| c.skill == 1_803_010).unwrap();
        assert_eq!((trial.auto_timer, trial.timer), (false, Some(true)));
        // Your mapping: your names and choices only.
        let m = mapping(&data, &[(1_803_200, false)].into(), &BTreeMap::new());
        assert_eq!(m.casts.keys().copied().collect::<Vec<_>>(), [1_803_200]);
        assert_eq!(m.casts[&1_803_200], CastChoice { name: None, warn: Some(false), timer: None });
        fs::remove_dir_all(dir).ok();
    }
}
