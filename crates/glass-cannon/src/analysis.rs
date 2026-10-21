//! The analysis window: one fight in full (every player and skill, the rotation, buffs
//! and debuffs) for the live fight or a saved one. Opened by right-clicking a player in the
//! overlay, or from its menu.

use std::collections::BTreeMap;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use meter_core::meter::Analysis;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::layout::Side;
use crate::settings::{outer_rect, overlay_monitor, place_beside, work_area};
use crate::store::Appearance;
use crate::{lock, AppState, ANALYSIS};

/// Default window size in logical pixels; smaller screens get less.
const WIDTH: f64 = 1200.0;
const HEIGHT: f64 = 780.0;
/// As in tauri.conf.json: the smallest the window can be.
const MIN: (f64, f64) = (760.0, 480.0);

/// Which fight (or dungeon run) and player the window shows.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Selection {
    /// A saved fight's history index; `None` is the live (or last) fight.
    pub fight: Option<usize>,
    /// A dungeon run: its first and last fights' history indexes (shown instead of `fight`).
    #[serde(default)]
    pub run: Option<(usize, usize)>,
    pub player: Option<u64>,
}

/// About a dungeon run shown as one analysis.
#[derive(Serialize)]
pub struct RunInfo {
    /// Fights in the run.
    fights: usize,
    /// Of those, the ones with a saved analysis (the newest fights only), which are what's
    /// shown.
    shown: usize,
    /// Where each shown fight starts on the joined timeline (ms).
    starts: Vec<u32>,
    /// First pull to last kill.
    wall_s: f64,
}

#[derive(Serialize)]
pub struct SkillMeta {
    name: Option<String>,
    icon_key: Option<u32>,
}

#[derive(Serialize)]
pub struct EffectMeta {
    name: Option<String>,
    icon_key: Option<u32>,
    /// The name is this effect's own, not its skill's.
    own: bool,
    /// You named it.
    custom: bool,
}

/// One fight, with names filled in for the page.
#[derive(Serialize)]
pub struct AnalysisData {
    #[serde(flatten)]
    analysis: Analysis,
    /// Names and icons for every skill in the hits, rows and effects.
    skills: BTreeMap<u32, SkillMeta>,
    /// Names and icons for effects: ones you named, and ones their skill doesn't name (no
    /// skill, or one the data lacks).
    effect_meta: BTreeMap<u32, EffectMeta>,
    /// For the skill tooltips: each class skill's cooldown, cost, description and
    /// specialties, with the ones this ID shows were chosen.
    skill_details: BTreeMap<u32, serde_json::Value>,
    /// Targets the game data marks as bosses.
    bosses: Vec<u64>,
    /// The dungeon (and mode) the main target is in, if any.
    place: Option<crate::runs::Place>,
    /// Set when this is a whole dungeon run.
    run: Option<RunInfo>,
    /// Still going (only the live fight can be).
    live: bool,
    /// When a saved fight ended (ms since the Unix epoch).
    ended_ms: Option<u64>,
    appearance: Appearance,
}

/// Shows the window on a fight and (optionally) a player, or hides it when it's already
/// showing that: the meter's right-click and menu button open and close it.
#[tauri::command]
pub fn open_analysis(
    handle: AppHandle,
    app: State<'_, Arc<AppState>>,
    fight: Option<usize>,
    run: Option<(usize, usize)>,
    player: Option<u64>,
) -> Result<(), String> {
    let w = handle.get_webview_window(ANALYSIS).ok_or("no analysis window")?;
    let now = *lock(&app.analysis_selection);
    let showing = w.is_visible().unwrap_or(false) && !w.is_minimized().unwrap_or(false);
    if showing && now.fight == fight && now.run == run && player.is_none_or(|p| now.player == Some(p)) {
        return w.hide().map_err(|e| e.to_string());
    }
    let selection = Selection { fight, run, player: player.or(now.player) };
    *lock(&app.analysis_selection) = selection;
    place(&handle, &w, !app.analysis_placed.swap(true, Ordering::SeqCst));
    w.show().map_err(|e| e.to_string())?;
    let _ = w.unminimize();
    let _ = w.set_focus();
    let _ = w.emit("analysis-select", selection);
    Ok(())
}

/// Beside the meter on its monitor: the first time at the default size, and again later
/// whenever it would cover the meter or isn't fully on a screen (a monitor was unplugged,
/// the meter moved). Otherwise it stays where you put it, at the size you gave it.
fn place(handle: &AppHandle, w: &tauri::WebviewWindow, first: bool) {
    let (Some(o), Some(mon), Some(now)) = (handle.get_webview_window(crate::OVERLAY), overlay_monitor(handle), outer_rect(w))
    else {
        return;
    };
    let Some(meter) = outer_rect(&o) else { return };
    let on_screen = w
        .available_monitors()
        .unwrap_or_default()
        .iter()
        .any(|m| now.inside(&work_area(m)));
    if !first && on_screen && now.overlap(&meter) == 0 {
        return;
    }
    // The default size, or bigger if you made it bigger (a window shrunk to fit one screen
    // gets its size back on a roomier one).
    let scale = mon.scale_factor();
    let default = ((WIDTH * scale) as i32, (HEIGHT * scale) as i32);
    let size = if first { default } else { (now.w.max(default.0), now.h.max(default.1)) };
    // The side of the meter with more room first.
    let area = work_area(&mon);
    let room_left = meter.x - area.x;
    let room_right = area.x + area.w - (meter.x + meter.w);
    let sides = if room_left >= room_right {
        [Side::Left, Side::Right, Side::Below, Side::Above]
    } else {
        [Side::Right, Side::Left, Side::Below, Side::Above]
    };
    place_beside(handle, w, size, MIN, &sides);
}

/// The meter is showing another fight (a saved one, or the live one again): an open
/// analysis window follows it.
#[tauri::command]
pub fn follow_fight(handle: AppHandle, app: State<'_, Arc<AppState>>, fight: Option<usize>) {
    let selection = {
        let mut s = lock(&app.analysis_selection);
        if s.fight == fight && s.run.is_none() {
            return;
        }
        s.fight = fight;
        s.run = None;
        *s
    };
    if let Some(w) = handle.get_webview_window(ANALYSIS) {
        if w.is_visible().unwrap_or(false) {
            let _ = w.emit("analysis-select", selection);
        }
    }
}

/// The player picked in the window, so a right-click on them in the meter closes it.
#[tauri::command]
pub fn analysis_pick(app: State<'_, Arc<AppState>>, player: Option<u64>) {
    lock(&app.analysis_selection).player = player;
}

/// What the window should show (it asks when it first loads).
#[tauri::command]
pub fn analysis_selection(app: State<'_, Arc<AppState>>) -> Selection {
    *lock(&app.analysis_selection)
}

/// Changes whenever the fight's data does, so the page only refetches then.
#[tauri::command]
pub fn analysis_stamp(app: State<'_, Arc<AppState>>, fight: Option<usize>, run: Option<(usize, usize)>) -> String {
    match (run, fight) {
        (Some((a, b)), _) => format!("run-{a}-{b}"),
        (None, Some(i)) => format!("saved-{i}"),
        (None, None) => {
            let (finished, hits) = app.engine.meter().analysis_stamp();
            format!("live-{finished}-{hits}")
        }
    }
}

/// The fight in full, with names filled in.
#[tauri::command]
pub fn analysis_data(app: State<'_, Arc<AppState>>, fight: Option<usize>, run: Option<(usize, usize)>) -> Option<AnalysisData> {
    let mut run_info = None;
    let (mut a, live, ended_ms) = match (run, fight) {
        (Some((first, last)), _) => {
            // The run's fights that still have their full analysis, back to back.
            let fights: Vec<(u64, u64)> = {
                let history = lock(&app.history);
                (first..=last).filter_map(|i| history.fight(i)).map(|f| (f.view.start_us / 1000, f.ended_ms)).collect()
            };
            let parts: Vec<_> = fights.iter().filter_map(|(_, ended)| app.details.load(*ended)).collect();
            let shown = parts.len();
            let (a, starts) = meter_core::join::join(parts)?;
            let wall_ms = match (fights.first(), fights.last()) {
                (Some((start, _)), Some((_, end))) => end.saturating_sub(*start),
                _ => 0,
            };
            run_info = Some(RunInfo { fights: fights.len(), shown, starts, wall_s: wall_ms as f64 / 1000.0 });
            (a, false, fights.last().map(|(_, end)| *end))
        }
        (None, None) => {
            let now = app.engine.now();
            let a = app.engine.meter().analysis(now)?;
            let live = a.view.active;
            (a, live, None)
        }
        (None, Some(index)) => {
            let ended_ms = lock(&app.history).fight(index)?.ended_ms;
            (app.details.load(ended_ms)?, false, Some(ended_ms))
        }
    };

    // Your name, if the game hasn't sent it this session.
    // One guard: a lock in the `if let` itself would be held through the block, and taking
    // it again there would wait on ourselves forever (this runs on the UI thread).
    let profile = lock(&app.profile);
    if let Some(name) = &profile.name {
        profile.name_self_row(&mut a.view, name);
        for p in a.view.players.iter().filter(|p| p.is_self) {
            a.names.insert(p.id, p.name.clone());
        }
    }
    // Released before `data`: the tick locks data then profile, so holding both here in
    // the other order could deadlock with it.
    drop(profile);

    let data = lock(&app.data);
    data.annotate_view(&mut a.view);
    let mut bosses = Vec::new();
    for (id, type_id) in &a.mob_types {
        if let Some((name, boss)) = data.npc(*type_id) {
            a.names.insert(*id, name);
            if boss {
                bosses.push(*id);
            }
        }
    }
    let place = a.view.target_type.and_then(|t| data.dungeon(t)).as_ref().map(crate::runs::Place::from);
    let ids = a
        .hits
        .iter()
        .map(|h| h.skill)
        .chain(a.effects.iter().filter_map(|e| e.skill))
        .chain(a.heals.iter().chain(a.taken.iter()).map(|h| h.skill))
        .chain(a.view.players.iter().flat_map(|p| p.skills.iter().map(|s| s.skill)));
    let mut skills = BTreeMap::new();
    for id in ids {
        skills.entry(id).or_insert_with(|| {
            let (name, icon_key) = data.skill_meta(id);
            SkillMeta { name, icon_key }
        });
    }
    let skill_details: BTreeMap<u32, serde_json::Value> =
        skills.keys().filter_map(|id| data.skill_details(*id).map(|d| (*id, d))).collect();
    let mut effect_meta = BTreeMap::new();
    for e in &a.effects {
        if effect_meta.contains_key(&e.effect) {
            continue;
        }
        let skill = e.skill.and_then(|s| skills.get(&s));
        let (named, own) = (skill.is_some_and(|m| m.name.is_some()), data.effect_name(e.effect));
        if own.is_some() || !named {
            let (name, icon_key) = data.effect_meta(e.effect);
            let icon_key = icon_key.or_else(|| skill.and_then(|m| m.icon_key));
            let custom = own.as_ref().is_some_and(|(_, custom)| *custom);
            effect_meta.insert(
                e.effect,
                EffectMeta { own: own.is_some(), custom, name: own.map(|(n, _)| n).or(name), icon_key },
            );
        }
    }
    drop(data);
    let appearance = lock(&app.settings).appearance.clone();
    Some(AnalysisData {
        analysis: a,
        skills,
        effect_meta,
        skill_details,
        bosses,
        place,
        run: run_info,
        live,
        ended_ms,
        appearance,
    })
}
