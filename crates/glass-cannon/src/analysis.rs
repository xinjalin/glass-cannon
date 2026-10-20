//! The analysis window: one fight in full (every player and skill, the rotation, buffs
//! and debuffs) for the live fight or a saved one. Opened by right-clicking a player in the
//! overlay, or from its menu.

use std::collections::BTreeMap;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use meter_core::meter::Analysis;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, State};

use crate::settings::overlay_monitor;
use crate::store::Appearance;
use crate::{lock, AppState, ANALYSIS};

/// Default window size in logical pixels; smaller screens get less.
const WIDTH: f64 = 1200.0;
const HEIGHT: f64 = 780.0;

/// Which fight and player the window shows.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Selection {
    /// A saved fight's history index; `None` is the live (or last) fight.
    pub fight: Option<usize>,
    pub player: Option<u64>,
}

#[derive(Serialize)]
pub struct SkillMeta {
    name: Option<String>,
    icon_key: Option<u32>,
}

/// One fight, with names filled in for the page.
#[derive(Serialize)]
pub struct AnalysisData {
    #[serde(flatten)]
    analysis: Analysis,
    /// Names and icons for every skill in the hits, rows and effects.
    skills: BTreeMap<u32, SkillMeta>,
    /// Targets the game data marks as bosses.
    bosses: Vec<u64>,
    /// Still going (only the live fight can be).
    live: bool,
    /// When a saved fight ended (ms since the Unix epoch).
    ended_ms: Option<u64>,
    appearance: Appearance,
}

/// Shows the window on a fight and (optionally) a player.
#[tauri::command]
pub fn open_analysis(
    handle: AppHandle,
    app: State<'_, Arc<AppState>>,
    fight: Option<usize>,
    player: Option<u64>,
) -> Result<(), String> {
    let selection = Selection { fight, player };
    *lock(&app.analysis_selection) = selection;
    let w = handle.get_webview_window(ANALYSIS).ok_or("no analysis window")?;
    // The first time, size it to the screen and put it in the middle of the overlay's monitor.
    if !app.analysis_placed.swap(true, Ordering::SeqCst) {
        if let Some(m) = overlay_monitor(&handle) {
            let scale = m.scale_factor();
            let area = m.work_area();
            let max = area.size.to_logical::<f64>(scale);
            let size = LogicalSize::new(WIDTH.min(max.width - 40.0), HEIGHT.min(max.height - 40.0));
            let _ = w.set_size(size);
            let phys = size.to_physical::<i32>(scale);
            let x = area.position.x + (area.size.width as i32 - phys.width) / 2;
            let y = area.position.y + (area.size.height as i32 - phys.height) / 2;
            let _ = w.set_position(PhysicalPosition::new(x, y));
        }
    }
    w.show().map_err(|e| e.to_string())?;
    let _ = w.unminimize();
    let _ = w.set_focus();
    let _ = w.emit("analysis-select", selection);
    Ok(())
}

/// What the window should show (it asks when it first loads).
#[tauri::command]
pub fn analysis_selection(app: State<'_, Arc<AppState>>) -> Selection {
    *lock(&app.analysis_selection)
}

/// Changes whenever the fight's data does, so the page only refetches then.
#[tauri::command]
pub fn analysis_stamp(app: State<'_, Arc<AppState>>, fight: Option<usize>) -> String {
    match fight {
        Some(i) => format!("saved-{i}"),
        None => {
            let (finished, hits) = app.engine.meter().analysis_stamp();
            format!("live-{finished}-{hits}")
        }
    }
}

/// The fight in full, with names filled in.
#[tauri::command]
pub fn analysis_data(app: State<'_, Arc<AppState>>, fight: Option<usize>) -> Option<AnalysisData> {
    let (mut a, live, ended_ms) = match fight {
        None => {
            let now = app.engine.now();
            let a = app.engine.meter().analysis(now)?;
            let live = a.view.active;
            (a, live, None)
        }
        Some(index) => {
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
    let ids = a
        .hits
        .iter()
        .map(|h| h.skill)
        .chain(a.effects.iter().filter_map(|e| e.skill))
        .chain(a.view.players.iter().flat_map(|p| p.skills.iter().map(|s| s.skill)));
    let mut skills = BTreeMap::new();
    for id in ids {
        skills.entry(id).or_insert_with(|| {
            let (name, icon_key) = data.skill_meta(id);
            SkillMeta { name, icon_key }
        });
    }
    drop(data);
    let appearance = lock(&app.settings).appearance.clone();
    Some(AnalysisData {
        analysis: a,
        skills,
        bosses,
        live,
        ended_ms,
        appearance,
    })
}
