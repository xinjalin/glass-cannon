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
pub const WIDGETS: [&str; 11] = ["meter", "boss-warnings", "boss-timers", "cubes", "target", "player", "party", "cooldowns", "party-cooldowns", "timers", "pets"];

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

/// The target frame: the target, with your best against it (a boss) and your DPS now.
#[derive(Serialize)]
pub struct TargetFrame {
    #[serde(flatten)]
    target: TargetView,
    best: Option<crate::store::Best>,
    my_dps: Option<f64>,
}

/// What you're targeting (or fighting), named, with your best against it.
#[tauri::command]
pub fn target(app: State<'_, Arc<AppState>>) -> Option<TargetFrame> {
    let mut snap = app.engine.meter().snapshot();
    lock(&app.data).annotate(&mut snap);
    let my_dps = snap.encounter.as_ref().and_then(|e| e.players.iter().find(|p| p.is_self)).map(|p| p.dps);
    let target = snap.target?;
    let best = match (target.boss, target.mob_type) {
        (true, Some(kind)) => {
            let profile = lock(&app.profile);
            lock(&app.bests).get(profile.name.as_deref(), kind)
        }
        _ => None,
    };
    Some(TargetFrame { target, best, my_dps })
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

/// Whether the overlay steps aside for game menus (from the cursor).
#[tauri::command]
pub fn set_hide_for_menus(app: State<'_, Arc<AppState>>, on: bool) {
    let mut s = lock(&app.settings);
    s.hide_for_menus = on;
    s.save(&app.config_dir);
}

/// Whether the app closes when the game does.
#[tauri::command]
pub fn set_close_with_game(app: State<'_, Arc<AppState>>, on: bool) {
    let mut s = lock(&app.settings);
    s.close_with_game = on;
    s.save(&app.config_dir);
}

/// Whether the overlay steps aside while the world map is open.
#[tauri::command]
pub fn set_hide_on_world_map(app: State<'_, Arc<AppState>>, on: bool) {
    let mut s = lock(&app.settings);
    s.hide_on_world_map = on;
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
    /// The dungeon you're in: its time limit, or the time after its last boss.
    pub dungeon: Option<meter_core::dungeon::DungeonClock>,
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
    let dungeon = app.engine.meter().dungeon_clock();
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
    ContentsView { odyle, tickets, tickets_ms: c.tickets_ms, abyss, abyss_ms: c.abyss_ms, dungeon }
}

#[tauri::command]
pub fn rename_ticket(app: State<'_, Arc<AppState>>, id: u32, name: String) {
    lock(&app.contents).rename_ticket(id, &name);
}

/// A dummy run for the history window, its skills named.
#[derive(Serialize)]
pub struct DummyRunRow {
    #[serde(flatten)]
    run: crate::dummy::DummyRun,
    skill_names: Vec<Option<String>>,
}

/// Your training dummy runs (newest first), and dummy mode's settings.
#[derive(Serialize)]
pub struct DummyRuns {
    runs: Vec<DummyRunRow>,
    on: bool,
    run_s: u32,
    lengths: &'static [u32],
    label: String,
}

#[tauri::command]
pub fn dummy_runs(app: State<'_, Arc<AppState>>) -> DummyRuns {
    let data = lock(&app.data);
    let runs = lock(&app.dummy)
        .runs()
        .iter()
        .map(|r| DummyRunRow { skill_names: r.skills.iter().map(|s| data.skill_meta(s.skill).0).collect(), run: r.clone() })
        .collect();
    let s = lock(&app.settings);
    DummyRuns { runs, on: s.dummy_mode, run_s: s.dummy_run_s, lengths: &crate::dummy::RUN_LENGTHS, label: s.dummy_label.clone() }
}

/// Two runs compared (`a` the earlier), with skill names.
#[derive(Serialize)]
pub struct DummyComparison {
    #[serde(flatten)]
    comparison: crate::dummy::Comparison,
    skill_names: Vec<Option<String>>,
}

#[tauri::command]
pub fn compare_dummy_runs(app: State<'_, Arc<AppState>>, a: u64, b: u64) -> Result<DummyComparison, String> {
    let book = lock(&app.dummy);
    let find = |id: u64| book.runs().iter().find(|r| r.ended_ms == id).ok_or("no such run");
    let (mut x, mut y) = (find(a)?, find(b)?);
    if x.ended_ms > y.ended_ms {
        std::mem::swap(&mut x, &mut y);
    }
    let comparison = crate::dummy::compare(x, y);
    let data = lock(&app.data);
    let skill_names = comparison.skills.iter().map(|(s, _, _)| data.skill_meta(*s).0).collect();
    Ok(DummyComparison { comparison, skill_names })
}

#[tauri::command]
pub fn label_dummy_run(app: State<'_, Arc<AppState>>, id: u64, label: String) -> bool {
    lock(&app.dummy).label(id, &label)
}

#[tauri::command]
pub fn delete_dummy_run(app: State<'_, Arc<AppState>>, id: u64) {
    lock(&app.dummy).remove(id);
}

#[tauri::command]
pub fn set_dummy_mode(app: State<'_, Arc<AppState>>, on: bool) {
    let mut s = lock(&app.settings);
    s.dummy_mode = on;
    s.save(&app.config_dir);
}

#[tauri::command]
pub fn set_dummy_run_length(app: State<'_, Arc<AppState>>, seconds: u32) -> Result<(), String> {
    if !crate::dummy::RUN_LENGTHS.contains(&seconds) {
        return Err(format!("{seconds} s isn't one of the run lengths"));
    }
    let mut s = lock(&app.settings);
    s.dummy_run_s = seconds;
    s.save(&app.config_dir);
    Ok(())
}

/// The label new dummy runs get (empty: none).
#[tauri::command]
pub fn set_dummy_label(app: State<'_, Arc<AppState>>, label: String) {
    let mut s = lock(&app.settings);
    s.dummy_label = label.trim().chars().take(40).collect();
    s.save(&app.config_dir);
}

/// A pet for the pet tracker: its progress, where it's found, and its mobs in view.
#[derive(Serialize)]
pub struct PetRow {
    id: u32,
    name: String,
    genus: Option<String>,
    /// 0: not summoned yet; None until the game has sent your pets.
    level: Option<u32>,
    /// Toward its next level.
    souls: Option<u32>,
    /// Souls its next level takes (None at the top level).
    need: Option<u32>,
    found_in: Vec<String>,
    tracked: bool,
    /// Its mobs in view now.
    in_view: u32,
}

#[derive(Serialize)]
pub struct PetTracker {
    /// The game has sent this character's pets (at login, or saved from before).
    synced: bool,
    character: Option<String>,
    nearest: bool,
    pets: Vec<PetRow>,
}

#[tauri::command]
pub fn pet_tracker(app: State<'_, Arc<AppState>>) -> PetTracker {
    // Each pet's mobs in view: how many.
    let (state, mobs) = {
        let mut meter = app.engine.meter();
        let state = meter.pets().state();
        let positions = meter.positions();
        // Mobs in view (the registry keeps those that left, to name past fights), wild ones
        // only: players' summons give no souls.
        let registry = meter.registry();
        let mobs: Vec<u32> =
            registry.mobs().filter(|(id, _)| registry.owner_of(*id).is_none() && positions.of(*id).is_some()).map(|(_, kind)| kind).collect();
        (state, mobs)
    };
    let data = lock(&app.data);
    let mut near: std::collections::HashMap<u32, u32> = std::collections::HashMap::new();
    for kind in mobs {
        if let Some(pet) = data.pet_of_mob(kind) {
            *near.entry(pet).or_insert(0) += 1;
        }
    }
    let character = lock(&app.profile).name.clone();
    let s = lock(&app.settings);
    let tracked = character.as_ref().and_then(|c| s.tracked_pets.get(c)).cloned().unwrap_or_default();
    let pets = data
        .all_pets()
        .map(|p| {
            let level = state.as_ref().map(|st| st.levels.get(&p.id).copied().unwrap_or(0));
            let souls = state.as_ref().map(|st| st.souls.get(&p.id).copied().unwrap_or(0));
            let need = level.and_then(|l| meter_core::pets::SOULS_PER_LEVEL.get(l as usize).copied());
            let in_view = near.get(&p.id).copied().unwrap_or(0);
            PetRow {
                id: p.id,
                name: p.name.to_string(),
                genus: p.genus.map(str::to_string),
                level,
                souls,
                need,
                found_in: p.found_in.to_vec(),
                tracked: tracked.contains(&p.id),
                in_view,
            }
        })
        .collect();
    PetTracker { synced: state.is_some(), character, nearest: s.pet_tracker_nearest, pets }
}

/// The pet map window's data: where each pet's mobs spawn, per map (`pet-map.json`).
#[tauri::command]
pub fn pet_map(app: State<'_, Arc<AppState>>) -> Option<serde_json::Value> {
    lock(&app.data).pet_map()
}

/// A pet map's picture (its `image.file`), as a data URL.
#[tauri::command]
pub fn pet_map_image(app: State<'_, Arc<AppState>>, file: String) -> Option<String> {
    lock(&app.data).pet_map_image(&file)
}

/// The map window's gathering side: where each material is, per map (`gather-map.json`).
#[tauri::command]
pub fn gather_map(app: State<'_, Arc<AppState>>) -> Option<serde_json::Value> {
    lock(&app.data).gather_map()
}

/// A material's icon (its `icon`), as a data URL.
#[tauri::command]
pub fn gather_icon(app: State<'_, Arc<AppState>>, file: String) -> Option<String> {
    lock(&app.data).gather_icon(&file)
}

/// Where you are for the pet map: the map (the game's ID) and x, y in metres.
#[derive(Serialize)]
pub struct PetMapMe {
    map: Option<u32>,
    at: Option<[f32; 2]>,
    exact: bool,
}

#[tauri::command]
pub fn pet_map_me(app: State<'_, Arc<AppState>>) -> PetMapMe {
    let now = app.engine.now();
    let meter = app.engine.meter();
    let positions = meter.positions();
    let me = positions.me(now);
    PetMapMe {
        map: positions.map(),
        at: me.map(|m| [m.pos[0] / 100.0, m.pos[1] / 100.0]),
        exact: me.is_some_and(|m| m.exact),
    }
}

/// Tracks a pet for your character, or stops.
#[tauri::command]
pub fn set_pet_tracked(app: State<'_, Arc<AppState>>, id: u32, on: bool) -> Result<(), String> {
    let character = lock(&app.profile).name.clone().ok_or("log in to a character first")?;
    let mut s = lock(&app.settings);
    let list = s.tracked_pets.entry(character).or_default();
    list.retain(|p| *p != id);
    if on {
        list.push(id);
    }
    s.save(&app.config_dir);
    Ok(())
}

#[tauri::command]
pub fn set_pet_tracker_nearest(app: State<'_, Arc<AppState>>, on: bool) {
    let mut s = lock(&app.settings);
    s.pet_tracker_nearest = on;
    s.save(&app.config_dir);
}

/// Your HP, MP and cast now (the player bars widget), the cast's skill named.
#[tauri::command]
pub fn self_bars(app: State<'_, Arc<AppState>>) -> Option<meter_core::meter::SelfBars> {
    let mut bars = app.engine.meter().self_bars()?;
    if let Some(c) = bars.cast.as_mut() {
        c.skill_name = lock(&app.data).skill_meta(c.skill).0;
    }
    Some(bars)
}

/// Your party as frames (the party frames widget).
#[tauri::command]
pub fn party_frames(app: State<'_, Arc<AppState>>) -> Vec<meter_core::meter::PartyFrame> {
    app.engine.meter().party_frames()
}

/// A piece of a player's gear, named.
#[derive(Serialize)]
pub struct GearRow {
    slot: u8,
    enhance: u8,
    item: u32,
    #[serde(flatten)]
    info: Option<crate::gamedata::GearEntry>,
}

/// Your party's members' gear, by name (those who've been in view; the party view's
/// tooltips).
#[tauri::command]
pub fn party_gear(app: State<'_, Arc<AppState>>) -> std::collections::BTreeMap<String, Vec<GearRow>> {
    let meter = app.engine.meter();
    let data = lock(&app.data);
    meter
        .registry()
        .party()
        .iter()
        .filter_map(|m| {
            let gear = meter.gear_of(&m.name)?;
            let rows = gear.iter().map(|g| GearRow { slot: g.slot, enhance: g.enhance, item: g.item, info: data.gear(g.item).cloned() }).collect();
            Some((m.name.clone(), rows))
        })
        .collect()
}
