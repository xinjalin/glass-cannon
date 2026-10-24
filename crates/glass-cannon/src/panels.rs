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

pub const PANELS: [&str; 5] = ["history", "cooldowns", "timers", "target", "party-cooldowns"];
/// The side windows that are widgets in the overlay when it's on (the history stays a
/// window: it's for looking back, not for the fight).
pub const WIDGET_PANELS: [&str; 4] = ["cooldowns", "timers", "target", "party-cooldowns"];

fn as_widget(app: &AppState, name: &str) -> bool {
    lock(&app.settings).overlay && WIDGET_PANELS.contains(&name)
}
/// The smallest each may be squeezed to (logical pixels) to fit beside the meter.
const MIN_SIZE: (f64, f64) = (260.0, 200.0);

fn check(name: &str) -> Result<(), String> {
    PANELS.contains(&name).then_some(()).ok_or_else(|| format!("no window called {name}"))
}

/// Shows a side window (beside the meter the first time), or hides it if it's showing.
#[tauri::command]
pub fn toggle_panel(handle: AppHandle, app: State<'_, Arc<AppState>>, name: String) -> Result<(), String> {
    check(&name)?;
    // In the overlay: the widget shows or goes (the window stays hidden).
    if as_widget(&app, &name) {
        let mut s = lock(&app.settings);
        let open = !s.panels.contains(&name);
        s.panels.retain(|p| p != &name);
        if open {
            s.panels.push(name);
        }
        s.save(&app.config_dir);
        let list = s.panels.clone();
        drop(s);
        let _ = handle.emit("hud-panels", list);
        return Ok(());
    }
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
    // The first time this session: where you left it last time, else beside the meter.
    let saved = first.then(|| lock(&handle.state::<Arc<AppState>>().placements).get(w.label())).flatten();
    if first && !saved.is_some_and(|p| crate::placement::restore(w, p, true)) {
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
    for name in open.iter().filter(|n| PANELS.contains(&n.as_str()) && !as_widget(app, n)) {
        if let Some(w) = handle.get_webview_window(name) {
            show(handle, &w, true);
            lock(&app.panels_placed).insert(name.clone());
        }
    }
}

/// Which side windows (or, in the overlay, widgets) are showing (for the meter's buttons
/// and the settings).
#[tauri::command]
pub fn panels_open(handle: AppHandle, app: State<'_, Arc<AppState>>) -> Vec<String> {
    let chosen = lock(&app.settings).panels.clone();
    PANELS
        .iter()
        .filter(|n| {
            if as_widget(&app, n) {
                chosen.iter().any(|c| c == *n)
            } else {
                handle.get_webview_window(n).is_some_and(|w| w.is_visible().unwrap_or(false))
            }
        })
        .map(|n| n.to_string())
        .collect()
}

/// The side windows shown as widgets in the overlay.
#[tauri::command]
pub fn hud_panels(app: State<'_, Arc<AppState>>) -> Vec<String> {
    let s = lock(&app.settings);
    s.panels.iter().filter(|n| WIDGET_PANELS.contains(&n.as_str())).cloned().collect()
}

/// Moving into or out of the overlay: the side windows hide (their widgets show), or come
/// back.
pub fn overlay_switched(handle: &AppHandle, app: &AppState, on: bool) {
    let chosen = lock(&app.settings).panels.clone();
    for name in WIDGET_PANELS {
        let Some(w) = handle.get_webview_window(name) else { continue };
        if on {
            let _ = w.hide();
        } else if chosen.iter().any(|c| c == name) {
            show(handle, &w, !lock(&app.panels_placed).contains(name));
            lock(&app.panels_placed).insert(name.to_string());
        }
    }
    let _ = handle.emit("hud-panels", chosen);
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

#[tauri::command]
pub fn set_boss_casts(app: State<'_, Arc<AppState>>, on: bool) {
    let mut s = lock(&app.settings);
    s.boss_casts = on;
    s.save(&app.config_dir);
}

/// Marks a boss skill: always warn (true), never (false), or as it looks (None).
#[tauri::command]
pub fn set_boss_skill_alert(app: State<'_, Arc<AppState>>, skill: u32, alert: Option<bool>) {
    let mut s = lock(&app.settings);
    match alert {
        Some(on) => s.boss_skill_alerts.insert(skill, on),
        None => s.boss_skill_alerts.remove(&skill),
    };
    s.save(&app.config_dir);
}

/// Your marks on boss skills, for the analysis window's [boss] tab.
#[tauri::command]
pub fn boss_skill_alerts(app: State<'_, Arc<AppState>>) -> std::collections::BTreeMap<u32, bool> {
    lock(&app.settings).boss_skill_alerts.clone()
}

#[tauri::command]
pub fn set_alert_sound(app: State<'_, Arc<AppState>>, on: bool) {
    let mut s = lock(&app.settings);
    s.alert_sound = on;
    s.save(&app.config_dir);
}

#[tauri::command]
pub fn set_pet_souls(app: State<'_, Arc<AppState>>, on: bool) {
    let mut s = lock(&app.settings);
    s.pet_souls = on;
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

/// Party cooldowns worth watching: 60 s or more, or a party-wide skill of 30 s or more.
const LONG_MS: u32 = 60_000;
const PARTY_SKILL_MS: u32 = 30_000;

#[derive(Serialize)]
pub struct MemberCooldowns {
    pub entity: u64,
    pub name: String,
    pub class_code: Option<u32>,
    pub is_self: bool,
    pub skills: Vec<PartySkill>,
}

#[derive(Serialize)]
pub struct PartySkill {
    pub skill: u32,
    pub name: Option<String>,
    pub icon_key: Option<u32>,
    pub remaining_ms: u32,
    pub duration_ms: u32,
    /// It affects the party (its description says so).
    pub party: bool,
}

/// Your party's long cooldowns (you included), each member's longest first. Your own
/// long-cooldown skills are listed from the game's list of your skills, ready until used.
#[tauri::command]
pub fn party_cooldowns(app: State<'_, Arc<AppState>>) -> Vec<MemberCooldowns> {
    let now = app.engine.now();
    let (list, labels, me, yours) = {
        let m = app.engine.meter();
        let list = m.party_cooldowns(now);
        let mut labels: Vec<(u64, String)> = list.iter().map(|c| (c.entity, m.registry().label(c.entity))).collect();
        let me = m.registry().self_id();
        if let Some(me) = me {
            labels.push((me, m.registry().label(me)));
        }
        (list, labels, me, m.your_skills().to_vec())
    };
    let data = lock(&app.data);
    let affects_party = |skill: u32| {
        data.skill_details(skill)
            .and_then(|d| d.get("desc").and_then(|v| v.as_str()).map(|s| s.to_lowercase().contains("party")))
            .unwrap_or(false)
    };
    let long = |duration_ms: u32, party: bool| duration_ms >= LONG_MS || (party && duration_ms >= PARTY_SKILL_MS);
    let ready_ones = yours.iter().filter_map(|&skill| {
        let cooldown_s = data.skill_details(skill)?.get("cooldown_s")?.as_f64()?;
        Some(meter_core::cooldowns::CasterCooldown {
            entity: me?,
            skill,
            remaining_ms: 0,
            duration_ms: (cooldown_s * 1000.0) as u32,
        })
    });
    // Skills used this session first, so a used skill isn't listed again as ready.
    let all: Vec<_> = list.into_iter().chain(ready_ones).collect();
    let mut out: Vec<MemberCooldowns> = Vec::new();
    for c in all {
        let party = affects_party(c.skill);
        if !long(c.duration_ms, party) {
            continue;
        }
        let (name, icon_key) = data.skill_meta(c.skill);
        let skill = PartySkill { skill: c.skill, name, icon_key, remaining_ms: c.remaining_ms, duration_ms: c.duration_ms, party };
        match out.iter_mut().find(|m| m.entity == c.entity) {
            Some(m) if m.skills.iter().any(|s| s.skill - s.skill % 10_000 == c.skill - c.skill % 10_000) => {}
            Some(m) => m.skills.push(skill),
            None => out.push(MemberCooldowns {
                entity: c.entity,
                name: labels.iter().find(|(e, _)| *e == c.entity).map(|(_, n)| n.clone()).unwrap_or_default(),
                class_code: meter_core::classes::class_code_of_skill(c.skill),
                is_self: me == Some(c.entity),
                skills: vec![skill],
            }),
        }
    }
    for m in &mut out {
        m.skills.sort_by_key(|s| std::cmp::Reverse(s.duration_ms));
    }
    // You first, then the others by name.
    out.sort_by(|a, b| b.is_self.cmp(&a.is_self).then(a.name.cmp(&b.name)));
    out
}

/// Odyle energy, the other content tickets and who holds the Abyss artifacts, for the world
/// timers window.
#[derive(Serialize)]
pub struct ContentsView {
    /// Basic and charged Odyle energy.
    pub odyle: Option<(u32, u32)>,
    pub tickets: Vec<TicketRow>,
    /// When the tickets were last sent (ms since 1970; 0 never).
    pub tickets_ms: u64,
    pub abyss: Vec<ArtifactRow>,
    pub abyss_ms: u64,
}

#[derive(Serialize)]
pub struct TicketRow {
    pub id: u32,
    /// The name you gave it (the game's data doesn't name them).
    pub name: Option<String>,
    pub count: Option<u32>,
    pub charged: Option<u32>,
}

#[derive(Serialize)]
pub struct ArtifactRow {
    pub id: u32,
    pub name: &'static str,
    /// 1 or 2.
    pub layer: u32,
    /// "Elyos", "Asmodians" or None (nobody).
    pub owner: Option<&'static str>,
    /// When the layer's holding resets (ms since 1970).
    pub until_ms: Option<u64>,
}

/// The artifacts by id (A2Tools' notes, from the game's AbyssArtifact table).
fn artifact_name(id: u32) -> &'static str {
    match id % 1000 {
        1 => "Reshanta's Root",
        2 => "Sulfur Tree Island",
        3 => "Archipelago of Siel's Wings",
        _ => "artifact",
    }
}

#[tauri::command]
pub fn contents(app: State<'_, Arc<AppState>>) -> ContentsView {
    use meter_core::contents::{ASMODIANS, ELYOS, ODYLE_ENERGY};
    let c = lock(&app.contents);
    let odyle = c.tickets.get(&ODYLE_ENERGY).map(|t| (t.count.unwrap_or(0), t.charged.unwrap_or(0)));
    let tickets = c
        .tickets
        .values()
        .filter(|t| t.id != ODYLE_ENERGY && (t.count.is_some() || t.charged.is_some()))
        .map(|t| TicketRow { id: t.id, name: c.ticket_names.get(&t.id).cloned(), count: t.count, charged: t.charged })
        .collect();
    let abyss = c
        .artifacts
        .values()
        .map(|a| {
            let layer = a.id / 1000;
            ArtifactRow {
                id: a.id,
                name: artifact_name(a.id),
                layer,
                owner: match a.owner {
                    ELYOS => Some("Elyos"),
                    ASMODIANS => Some("Asmodians"),
                    _ => None,
                },
                until_ms: c.sieges.get(&(layer * 1000 + 1)).and_then(|s| s.until_ms),
            }
        })
        .collect();
    ContentsView { odyle, tickets, tickets_ms: c.tickets_ms, abyss, abyss_ms: c.abyss_ms }
}

#[tauri::command]
pub fn rename_ticket(app: State<'_, Arc<AppState>>, id: u32, name: String) {
    lock(&app.contents).rename_ticket(id, &name);
}
