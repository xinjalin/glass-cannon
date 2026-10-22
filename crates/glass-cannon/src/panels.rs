//! The side windows: fight history, cooldowns and world timers. Each opens beside the
//! meter the first time, stays where you put it, and is opened again at start if it was
//! open when the app closed.

use std::sync::Arc;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use meter_core::fieldboss::FieldBoss;
use meter_core::meter::TargetView;
use meter_core::pvp::PvpSession;

use crate::layout::Side;
use crate::settings::place_beside;
use crate::store::TIMER_REGIONS;
use crate::{lock, AppState, OVERLAY};

pub const PANELS: [&str; 4] = ["history", "cooldowns", "timers", "target"];
/// The smallest each may be squeezed to (logical pixels) to fit beside the meter.
const MIN_SIZE: (f64, f64) = (260.0, 200.0);

fn check(name: &str) -> Result<(), String> {
    PANELS.contains(&name).then_some(()).ok_or_else(|| format!("no window called {name}"))
}

/// Shows a side window (beside the meter the first time), or hides it if it's showing.
#[tauri::command]
pub fn toggle_panel(handle: AppHandle, app: State<'_, Arc<AppState>>, name: String) -> Result<(), String> {
    check(&name)?;
    let w = handle.get_webview_window(&name).ok_or("window missing")?;
    let open = !w.is_visible().unwrap_or(false);
    if open {
        show(&handle, &w, !lock(&app.panels_placed).contains(&name));
        lock(&app.panels_placed).insert(name.clone());
    } else {
        w.hide().map_err(|e| e.to_string())?;
    }
    let mut s = lock(&app.settings);
    s.panels.retain(|p| p != &name);
    if open {
        s.panels.push(name);
    }
    s.save(&app.config_dir);
    Ok(())
}

fn show(handle: &AppHandle, w: &tauri::WebviewWindow, first: bool) {
    if first {
        if let Ok(size) = w.outer_size() {
            place_beside(
                handle,
                w,
                (size.width as i32, size.height as i32),
                MIN_SIZE,
                &[Side::Left, Side::Right, Side::Below, Side::Above],
            );
        }
    }
    let _ = w.show();
    let _ = w.unminimize();
}

/// Opens the windows that were open last time.
pub fn reopen(handle: &AppHandle, app: &AppState) {
    let open = lock(&app.settings).panels.clone();
    for name in open.iter().filter(|n| PANELS.contains(&n.as_str())) {
        if let Some(w) = handle.get_webview_window(name) {
            show(handle, &w, true);
            lock(&app.panels_placed).insert(name.clone());
        }
    }
}

/// Which side windows are showing (for the meter's buttons).
#[tauri::command]
pub fn panels_open(handle: AppHandle) -> Vec<String> {
    PANELS
        .iter()
        .filter(|n| handle.get_webview_window(n).is_some_and(|w| w.is_visible().unwrap_or(false)))
        .map(|n| n.to_string())
        .collect()
}

/// Shows a saved fight on the meter (picked in the history window).
#[tauri::command]
pub fn show_fight(handle: AppHandle, index: usize) -> Result<(), String> {
    handle.emit_to(OVERLAY, "show-fight", index).map_err(|e| e.to_string())
}

#[derive(Serialize)]
pub struct CooldownRow {
    pub skill: u32,
    pub name: Option<String>,
    /// Which skill's icon to show.
    pub icon_key: Option<u32>,
    pub remaining_ms: u32,
    pub duration_ms: u32,
    pub charges: Option<u8>,
}

/// Your skills that aren't ready, soonest first, with names and icons.
#[tauri::command]
pub fn cooldowns(app: State<'_, Arc<AppState>>) -> Vec<CooldownRow> {
    let now = app.engine.now();
    let list = app.engine.meter().cooldowns(now);
    let data = lock(&app.data);
    list.into_iter()
        .map(|c| {
            let (name, icon_key) = data.skill_meta(c.skill);
            CooldownRow {
                skill: c.skill,
                name,
                icon_key,
                remaining_ms: c.remaining_ms,
                duration_ms: c.duration_ms,
                charges: c.charges,
            }
        })
        .collect()
}

#[derive(Serialize)]
pub struct PvpList {
    pub current: Option<PvpSession>,
    /// Newest first.
    pub sessions: Vec<PvpSession>,
}

#[tauri::command]
pub fn pvp_list(app: State<'_, Arc<AppState>>) -> PvpList {
    let current = app.engine.meter().pvp();
    PvpList { current, sessions: lock(&app.pvp).list() }
}

/// The region whose event times the timers show.
#[tauri::command]
pub fn timer_region(app: State<'_, Arc<AppState>>) -> String {
    let r = lock(&app.settings).timer_region.clone();
    if TIMER_REGIONS.contains(&r.as_str()) { r } else { TIMER_REGIONS[0].to_string() }
}

#[tauri::command]
pub fn set_timer_region(app: State<'_, Arc<AppState>>, region: String) -> Result<(), String> {
    if !TIMER_REGIONS.contains(&region.as_str()) {
        return Err(format!("unknown region {region}"));
    }
    let mut s = lock(&app.settings);
    s.timer_region = region;
    s.save(&app.config_dir);
    Ok(())
}

/// What you're targeting (or fighting), named.
#[tauri::command]
pub fn target(app: State<'_, Arc<AppState>>) -> Option<TargetView> {
    let mut snap = app.engine.meter().snapshot();
    lock(&app.data).annotate(&mut snap);
    snap.target
}

#[derive(Serialize)]
pub struct FieldBossRow {
    #[serde(flatten)]
    pub boss: FieldBoss,
    /// The name you gave it, else the game data's.
    pub name: Option<String>,
    pub level: Option<u32>,
    /// Where it is ("Altgard · Sanctum Outpost"), and the map's name.
    pub place: Option<String>,
    pub map_name: Option<String>,
    /// When the world map last showed it.
    pub seen_ms: Option<u64>,
}

/// Field bosses as world maps last showed them.
#[tauri::command]
pub fn field_bosses(app: State<'_, Arc<AppState>>) -> Vec<FieldBossRow> {
    let f = lock(&app.field_bosses);
    let data = lock(&app.data);
    f.bosses
        .values()
        .map(|b| {
            let (name, level, place, map_name) = data.field_boss(b.id, b.map);
            FieldBossRow {
                boss: b.clone(),
                name: f.names.get(&b.id).cloned().or(name),
                level,
                place,
                map_name,
                seen_ms: f.seen_ms.get(&b.id).copied(),
            }
        })
        .collect()
}

#[tauri::command]
pub fn rename_field_boss(app: State<'_, Arc<AppState>>, id: u32, name: String) {
    lock(&app.field_bosses).rename(id, &name);
}

/// Be told (or not) before an event or field boss starts.
#[tauri::command]
pub fn set_alert(app: State<'_, Arc<AppState>>, id: String, on: bool) {
    let mut s = lock(&app.settings);
    s.alerts.retain(|a| a != &id);
    if on {
        s.alerts.push(id);
    }
    s.save(&app.config_dir);
}

#[tauri::command]
pub fn set_alert_lead(app: State<'_, Arc<AppState>>, minutes: u32) {
    let mut s = lock(&app.settings);
    s.alert_lead_min = minutes.clamp(1, 60);
    s.save(&app.config_dir);
}

/// Seconds after combat before the meter fades (0: never).
#[tauri::command]
pub fn set_hide_after(app: State<'_, Arc<AppState>>, seconds: u32) {
    let mut s = lock(&app.settings);
    s.hide_after_s = seconds.min(600);
    s.save(&app.config_dir);
}

#[tauri::command]
pub fn set_boss_hp(app: State<'_, Arc<AppState>>, on: bool) {
    let mut s = lock(&app.settings);
    s.boss_hp = on;
    s.save(&app.config_dir);
}

#[derive(Serialize)]
pub struct TimerSettings {
    pub region: String,
    pub alerts: Vec<String>,
    pub alert_lead_min: u32,
}

#[tauri::command]
pub fn timer_settings(app: State<'_, Arc<AppState>>) -> TimerSettings {
    let s = lock(&app.settings);
    let region = if TIMER_REGIONS.contains(&s.timer_region.as_str()) { s.timer_region.clone() } else { TIMER_REGIONS[0].into() };
    TimerSettings { region, alerts: s.alerts.clone(), alert_lead_min: s.alert_lead_min }
}
