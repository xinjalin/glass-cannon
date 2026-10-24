//! The overlay's widgets: which show (settings → overlay), and what the side widgets
//! (cooldowns, world timers, target frame, party cooldowns) and the history window read.

use std::sync::Arc;

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use meter_core::fieldboss::FieldBoss;
use meter_core::meter::TargetView;
use meter_core::pvp::PvpSession;

use crate::store::TIMER_REGIONS;
use crate::{hud, lock, AppState};

/// Every widget the overlay has, in the settings' order.
pub const WIDGETS: [&str; 7] = ["meter", "boss-warnings", "boss-timers", "target", "cooldowns", "party-cooldowns", "timers"];

/// Shows a widget, or takes it away; saved for next time.
#[tauri::command]
pub fn toggle_widget(handle: AppHandle, app: State<'_, Arc<AppState>>, name: String) -> Result<(), String> {
    if !WIDGETS.contains(&name.as_str()) {
        return Err(format!("no widget called {name}"));
    }
    let mut s = lock(&app.settings);
    let show = !s.widgets.contains(&name);
    s.widgets.retain(|w| w != &name);
    if show {
        s.widgets.push(name);
    }
    s.save(&app.config_dir);
    let list = s.widgets.clone();
    drop(s);
    handle.emit_to(hud::HUD, "hud-widgets", list).map_err(|e| e.to_string())
}

/// The widgets that show.
#[tauri::command]
pub fn widgets_shown(app: State<'_, Arc<AppState>>) -> Vec<String> {
    lock(&app.settings).widgets.clone()
}

/// Shows a saved fight on the meter (picked in the history window).
#[tauri::command]
pub fn show_fight(handle: AppHandle, index: usize) -> Result<(), String> {
    handle.emit_to(hud::HUD, "show-fight", index).map_err(|e| e.to_string())
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
    pub favourites: Vec<String>,
    pub favourites_only: bool,
}

#[tauri::command]
pub fn timer_settings(app: State<'_, Arc<AppState>>) -> TimerSettings {
    let s = lock(&app.settings);
    let region = if TIMER_REGIONS.contains(&s.timer_region.as_str()) { s.timer_region.clone() } else { TIMER_REGIONS[0].into() };
    TimerSettings {
        region,
        alerts: s.alerts.clone(),
        alert_lead_min: s.alert_lead_min,
        favourites: s.timer_favourites.clone(),
        favourites_only: s.timer_favourites_only,
    }
}

/// Stars (or unstars) an event or field boss in the world timers.
#[tauri::command]
pub fn set_timer_favourite(app: State<'_, Arc<AppState>>, id: String, on: bool) {
    let mut s = lock(&app.settings);
    s.timer_favourites.retain(|f| f != &id);
    if on {
        s.timer_favourites.push(id);
    }
    s.save(&app.config_dir);
}

/// Shows only the starred events and field bosses, or all of them.
#[tauri::command]
pub fn set_timer_favourites_only(app: State<'_, Arc<AppState>>, on: bool) {
    let mut s = lock(&app.settings);
    s.timer_favourites_only = on;
    s.save(&app.config_dir);
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
