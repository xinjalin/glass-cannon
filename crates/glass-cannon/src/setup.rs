//! Sharing a setup: your overlay's layout (where each widget is, which show, which fade),
//! your theme (the appearance), and your settings (each widget's, and the app's) saved to a
//! file in the `exports` folder beside the app, and taken in from one, each part or not.
//!
//! What's yours alone never goes in a file: the pets you track (by character), hotkeys,
//! recording, the dummy's label, and how the launcher and the app start and stop.
//!
//! The action bars are each class's own. A file has every class's bars its maker set up,
//! and names the class they were on (the bars they saw). Taken in, the file's classes'
//! bars replace yours and your other classes' stay; and when you're on another class than
//! its maker was, your class's bars take the look of theirs (what each bar tracks, what
//! shows, when it hides, its style) with your own skills on them.

use std::collections::BTreeMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use tauri::{AppHandle, Emitter, State};

use crate::actionbar::BarSetup;
use crate::store::Settings;
use crate::{lock, AppState};

/// The parts of a setup.
pub const PARTS: [&str; 3] = ["layout", "theme", "settings"];

/// The settings that go with the layout: which widgets show, and which fade.
const LAYOUT_KEYS: [&str; 2] = ["widgets", "fading"];
/// The theme: the appearance.
const THEME_KEYS: [&str; 1] = ["appearance"];
/// The settings shared: each widget's, and the app's, not what's yours alone.
const SETTINGS_KEYS: &[&str] = &[
    "scope",
    "manual_scope",
    "refresh_ms",
    "meter_view",
    "timer_region",
    "alerts",
    "alert_lead_min",
    "timer_favourites",
    "timer_favourites_only",
    "hide_after_s",
    "meter_hide_after_s",
    "boss_hp",
    "boss_casts",
    "pet_souls",
    "boss_skill_alerts",
    "boss_skill_timers",
    "boss_mechanics",
    "party_cooldowns_by_class",
    "black_market_auto",
    "meter_fit",
    "party_sort",
    "cooldowns_look",
    "cooldowns_ready_right",
    "cooldowns_size",
    "cooldowns_scale",
    "cooldowns_bar_look",
    "action_bar_look",
    "party_look",
    "timer_pins",
    "pins_look",
    "bars",
    "vitals",
    "cast_bar",
    "player_cast",
    "target",
    "alert_sound",
    "boss_on_you",
    "hide_for_menus",
    "click_through",
    "dummy_mode",
    "dummy_run_s",
    "pet_tracker_nearest",
];

/// A shared setup, as the file has it: each part there or not.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct SharedSetup {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<Value>,
    /// `hud`: the widgets' places and sizes (hud.json); `widgets` and `fading`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout: Option<Map<String, Value>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme: Option<Map<String, Value>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings: Option<Map<String, Value>>,
}

fn pick(all: &Map<String, Value>, keys: &[&str]) -> Map<String, Value> {
    keys.iter().filter_map(|k| all.get(*k).map(|v| ((*k).to_string(), v.clone()))).collect()
}

/// Your setup's parts, as a file has them; `class`, the class you're on (its bars the ones
/// you see).
pub fn shared(settings: &Settings, layout: Value, parts: &[String], class: Option<u32>) -> SharedSetup {
    let all = match serde_json::to_value(settings) {
        Ok(Value::Object(m)) => m,
        _ => Map::new(),
    };
    let want = |p: &str| parts.iter().any(|x| x == p);
    let made = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    SharedSetup {
        source: Some(serde_json::json!({ "made_by": "Glass Cannon", "version": env!("CARGO_PKG_VERSION"), "unix_time": made, "class": class })),
        layout: want("layout").then(|| {
            let mut m = pick(&all, &LAYOUT_KEYS);
            m.insert("hud".into(), layout);
            m
        }),
        theme: want("theme").then(|| pick(&all, &THEME_KEYS)),
        settings: want("settings").then(|| pick(&all, SETTINGS_KEYS)),
    }
}

/// Your settings with a setup's parts over them (only the keys each part may have);
/// the layout's places, when it has them. Which parts were taken. `class`, the class
/// you're on.
pub fn merged(settings: &Settings, setup: &SharedSetup, parts: &[String], class: Option<u32>) -> Result<(Settings, Option<Value>, Vec<String>), String> {
    let mut all = match serde_json::to_value(settings) {
        Ok(Value::Object(m)) => m,
        _ => return Err("couldn't read your settings".into()),
    };
    let want = |p: &str| parts.iter().any(|x| x == p);
    let mut taken = Vec::new();
    let mut hud = None;
    let mut take = |part: &str, from: &Option<Map<String, Value>>, keys: &[&str]| {
        if let (true, Some(m)) = (want(part), from) {
            for k in keys {
                if let Some(v) = m.get(*k) {
                    all.insert((*k).to_string(), v.clone());
                }
            }
            taken.push(part.to_string());
        }
    };
    take("layout", &setup.layout, &LAYOUT_KEYS);
    take("theme", &setup.theme, &THEME_KEYS);
    take("settings", &setup.settings, SETTINGS_KEYS);
    if want("layout") {
        hud = setup.layout.as_ref().and_then(|m| m.get("hud")).filter(|v| v.is_object()).cloned();
    }
    if taken.is_empty() {
        return Err("none of the parts picked are in that file".into());
    }
    let mut s: Settings = serde_json::from_value(Value::Object(all)).map_err(|_| "that setup's settings don't read".to_string())?;
    if taken.iter().any(|p| p == "settings") && setup.settings.as_ref().is_some_and(|m| m.contains_key("bars")) {
        let maker = setup.source.as_ref().and_then(|v| v.get("class")).and_then(Value::as_u64).and_then(|c| u32::try_from(c).ok());
        s.bars = bars_merged(&settings.bars, std::mem::take(&mut s.bars), maker, class);
    }
    Ok((s, hud, taken))
}

type ClassBars = BTreeMap<String, BTreeMap<u8, BarSetup>>;

/// Your bars with a file's over them: its classes' replace yours, your others stay; and on
/// another class than the file's maker, yours take the look of theirs (all but the skills,
/// each class's own), a bar of theirs you haven't got added with no skills.
fn bars_merged(mine: &ClassBars, theirs: ClassBars, maker: Option<u32>, class: Option<u32>) -> ClassBars {
    let looks = match (maker, class) {
        (Some(m), Some(c)) if m != c => theirs.get(&m.to_string()).cloned().map(|b| (c.to_string(), b)),
        _ => None,
    };
    let mut out = mine.clone();
    out.extend(theirs);
    if let Some((c, looks)) = looks {
        let yours = out.entry(c).or_default();
        for (n, look) in looks {
            let skills = yours.get(&n).map(|b| b.skills.clone()).unwrap_or_default();
            yours.insert(n, BarSetup { skills, ..look });
        }
    }
    out
}

/// Saves your setup's parts (`layout`, `theme`, `settings`) to the exports folder beside the
/// app, and shows the file in Explorer; its path.
#[tauri::command]
pub fn export_setup(app: State<'_, Arc<AppState>>, parts: Vec<String>) -> Result<String, String> {
    let parts: Vec<String> = parts.into_iter().filter(|p| PARTS.contains(&p.as_str())).collect();
    if parts.is_empty() {
        return Err("pick something to export".into());
    }
    let class = lock(&app.profile).class_code;
    let setup = shared(&lock(&app.settings), crate::hud::read_layout(&app.config_dir), &parts, class);
    let dir = app.exe_dir.join("exports");
    std::fs::create_dir_all(&dir).map_err(|e| format!("couldn't make {}: {e}", dir.display()))?;
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let path = dir.join(format!("glass-cannon-setup-{secs}.json"));
    let json = serde_json::to_vec_pretty(&setup).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| format!("couldn't save {}: {e}", path.display()))?;
    let _ = std::process::Command::new("explorer").arg(format!("/select,{}", path.display())).spawn();
    Ok(path.display().to_string())
}

/// Takes in a setup (a file's text): the parts picked that it has go over yours, then the
/// overlay and the windows start again with them. Which parts it took.
#[tauri::command]
pub fn import_setup(handle: AppHandle, app: State<'_, Arc<AppState>>, json: String, parts: Vec<String>) -> Result<Vec<String>, String> {
    let setup: SharedSetup = serde_json::from_str(&json).map_err(|_| "that isn't a setup (a file exported from Glass Cannon)".to_string())?;
    let class = lock(&app.profile).class_code;
    let (merged, hud, taken) = merged(&lock(&app.settings), &setup, &parts, class)?;
    merged.save(&app.config_dir);
    // Read back as a start would, so anything out of range is put right.
    let loaded = Settings::load(&app.config_dir);
    let appearance = loaded.appearance.clone();
    *lock(&app.settings) = loaded;
    // The windows' frames and every page's look, at once.
    crate::restyle_windows(&handle, &appearance);
    if let Some(hud) = hud {
        crate::hud::write_layout(&app.config_dir, &crate::hud::clean_layout(&hud));
    }
    let _ = handle.emit("setup-imported", &taken);
    Ok(taken)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_setup_shares_its_parts_and_not_whats_yours() {
        let mut mine = Settings::default();
        mine.appearance.theme = "mint".into();
        mine.widgets = vec!["meter".into(), "vitals".into()];
        mine.vitals.hp_warn = true;
        mine.hide_after_s = 30;
        mine.tracked_pets.insert("NeneneTowawa".into(), vec![1002]);
        let file = shared(&mine, serde_json::json!({ "meter": { "x": 0.5 } }), &PARTS.map(String::from), None);
        let text = serde_json::to_string(&file).unwrap();
        // Never the characters' pets.
        assert!(!text.contains("NeneneTowawa"));
        assert_eq!(file.layout.as_ref().unwrap()["hud"]["meter"]["x"], 0.5);
        // Taken in by someone else, the parts picked only.
        let back: SharedSetup = serde_json::from_str(&text).unwrap();
        let theirs = Settings::default();
        let (s, hud, taken) = merged(&theirs, &back, &["theme".into(), "layout".into()], None).unwrap();
        assert_eq!(taken, ["layout", "theme"]);
        assert_eq!(s.appearance.theme, "mint");
        assert_eq!(s.widgets, ["meter", "vitals"]);
        assert!(hud.is_some());
        assert!(!s.vitals.hp_warn, "settings weren't picked");
        assert_eq!(s.hide_after_s, theirs.hide_after_s);
        let (s, _, _) = merged(&theirs, &back, &["settings".into()], None).unwrap();
        assert!(s.vitals.hp_warn);
        assert!(s.tracked_pets.is_empty());
        // A file without what's picked.
        let only_theme = SharedSetup { theme: back.theme, ..SharedSetup::default() };
        assert!(merged(&theirs, &only_theme, &["layout".into()], None).is_err());
    }

    fn bar(skills: &[u32], icon_size: u16, hide_idle: bool) -> BarSetup {
        let mut b = BarSetup { skills: skills.to_vec(), hide_idle, ..BarSetup::default() };
        b.style.icon_size = icon_size;
        b.style.spacing = 9;
        b.style.tenths = false;
        b
    }

    #[test]
    fn bars_go_by_class_and_take_the_makers_look_on_another() {
        // The maker: on class 1 with two bars; class 9 with one.
        let mut maker = Settings::default();
        maker.bars.insert("1".into(), [(1, bar(&[11_010_000], 64, true)), (2, bar(&[], 30, false))].into());
        maker.bars.insert("9".into(), [(1, bar(&[91_010_000], 50, false))].into());
        maker.timer_pins = vec!["shugo-festival".into()];
        maker.pins_look.train = true;
        maker.pins_look.within_min = 60;
        maker.party_look.columns = 2;
        let text = serde_json::to_string(&shared(&maker, Value::Null, &["settings".into()], Some(1))).unwrap();
        let file: SharedSetup = serde_json::from_str(&text).unwrap();

        // You: class 5 with a bar of your own, and class 3.
        let mut you = Settings::default();
        you.bars.insert("5".into(), [(1, BarSetup { skills: vec![51_010_000], ..BarSetup::default() })].into());
        you.bars.insert("3".into(), [(1, BarSetup { skills: vec![31_010_000], ..BarSetup::default() })].into());
        let (s, _, _) = merged(&you, &file, &["settings".into()], Some(5)).unwrap();
        // The class you're on: their look, your skills; their second bar too, empty.
        let b1 = &s.bars["5"][&1];
        assert_eq!(b1.skills, [51_010_000]);
        assert_eq!((b1.style.icon_size, b1.style.spacing, b1.style.tenths, b1.hide_idle), (64, 9, false, true));
        assert!(s.bars["5"][&2].skills.is_empty());
        assert_eq!(s.bars["5"][&2].style.icon_size, 30);
        // Their classes as they have them; your other classes kept.
        assert_eq!(s.bars["1"], maker.bars["1"]);
        assert_eq!(s.bars["9"], maker.bars["9"]);
        assert_eq!(s.bars["3"], you.bars["3"]);
        // The rest of the settings.
        assert_eq!(s.timer_pins, ["shugo-festival"]);
        assert!(s.pins_look.train);
        assert_eq!(s.pins_look.within_min, 60);
        assert_eq!(s.party_look.columns, 2);

        // On another class the file has: its skills for that class, in the maker's look.
        let (s, _, _) = merged(&you, &file, &["settings".into()], Some(9)).unwrap();
        assert_eq!(s.bars["9"][&1].skills, [91_010_000]);
        assert_eq!((s.bars["9"][&1].style.icon_size, s.bars["9"][&1].hide_idle), (64, true));
        assert_eq!(s.bars["5"], you.bars["5"]);
        // On the maker's own class: theirs as they are.
        let (s, _, _) = merged(&you, &file, &["settings".into()], Some(1)).unwrap();
        assert_eq!(s.bars["1"], maker.bars["1"]);
        assert_eq!(s.bars["9"], maker.bars["9"]);
        // A file without a class (or you not logged in): by class only.
        let (s, _, _) = merged(&you, &file, &["settings".into()], None).unwrap();
        assert_eq!(s.bars["9"], maker.bars["9"]);
        assert_eq!(s.bars["5"], you.bars["5"]);
    }
}
