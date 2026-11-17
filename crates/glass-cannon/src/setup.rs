//! Sharing a setup: your overlay's layout (where each widget is, which show, which fade),
//! your theme (the appearance), and your settings (each widget's, and the app's) saved to a
//! file in the `exports` folder beside the app, and taken in from one, each part or not.
//!
//! What's yours alone never goes in a file: the pets you track (by character), hotkeys,
//! recording, the dummy's label, and how the launcher and the app start and stop.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use tauri::{AppHandle, Emitter, State};

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

/// Your setup's parts, as a file has them.
pub fn shared(settings: &Settings, layout: Value, parts: &[String]) -> SharedSetup {
    let all = match serde_json::to_value(settings) {
        Ok(Value::Object(m)) => m,
        _ => Map::new(),
    };
    let want = |p: &str| parts.iter().any(|x| x == p);
    let made = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    SharedSetup {
        source: Some(serde_json::json!({ "made_by": "Glass Cannon", "version": env!("CARGO_PKG_VERSION"), "unix_time": made })),
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
/// the layout's places, when it has them. Which parts were taken.
pub fn merged(settings: &Settings, setup: &SharedSetup, parts: &[String]) -> Result<(Settings, Option<Value>, Vec<String>), String> {
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
    let s: Settings = serde_json::from_value(Value::Object(all)).map_err(|_| "that setup's settings don't read".to_string())?;
    Ok((s, hud, taken))
}

/// Saves your setup's parts (`layout`, `theme`, `settings`) to the exports folder beside the
/// app, and shows the file in Explorer; its path.
#[tauri::command]
pub fn export_setup(app: State<'_, Arc<AppState>>, parts: Vec<String>) -> Result<String, String> {
    let parts: Vec<String> = parts.into_iter().filter(|p| PARTS.contains(&p.as_str())).collect();
    if parts.is_empty() {
        return Err("pick something to export".into());
    }
    let setup = shared(&lock(&app.settings), crate::hud::read_layout(&app.config_dir), &parts);
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
    let (merged, hud, taken) = merged(&lock(&app.settings), &setup, &parts)?;
    merged.save(&app.config_dir);
    // Read back as a start would, so anything out of range is put right.
    let loaded = Settings::load(&app.config_dir);
    let appearance = loaded.appearance.clone();
    *lock(&app.settings) = loaded;
    // The windows' and widgets' look, at once.
    let _ = handle.emit("appearance", &appearance);
    if let Some(hud) = hud {
        crate::hud::write_layout(&app.config_dir, &hud);
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
        let file = shared(&mine, serde_json::json!({ "meter": { "x": 0.5 } }), &PARTS.map(String::from));
        let text = serde_json::to_string(&file).unwrap();
        // Never the characters' pets.
        assert!(!text.contains("NeneneTowawa"));
        assert_eq!(file.layout.as_ref().unwrap()["hud"]["meter"]["x"], 0.5);
        // Taken in by someone else, the parts picked only.
        let back: SharedSetup = serde_json::from_str(&text).unwrap();
        let theirs = Settings::default();
        let (s, hud, taken) = merged(&theirs, &back, &["theme".into(), "layout".into()]).unwrap();
        assert_eq!(taken, ["layout", "theme"]);
        assert_eq!(s.appearance.theme, "mint");
        assert_eq!(s.widgets, ["meter", "vitals"]);
        assert!(hud.is_some());
        assert!(!s.vitals.hp_warn, "settings weren't picked");
        assert_eq!(s.hide_after_s, theirs.hide_after_s);
        let (s, _, _) = merged(&theirs, &back, &["settings".into()]).unwrap();
        assert!(s.vitals.hp_warn);
        assert!(s.tracked_pets.is_empty());
        // A file without what's picked.
        let only_theme = SharedSetup { theme: back.theme, ..SharedSetup::default() };
        assert!(merged(&theirs, &only_theme, &["layout".into()]).is_err());
    }
}
