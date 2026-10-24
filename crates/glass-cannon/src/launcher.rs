//! The launcher: the small window the app opens to. It shows whether the meter is
//! capturing, the fight going on (or the last one), and starts and stops the overlay; the
//! analysis, history and settings open from it. Closing it closes the app.

use std::sync::atomic::Ordering;
use std::sync::Arc;

use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use meter_core::meter::{EncounterView, FightEnd};

use crate::{lock, ping, AppState, Status, LAUNCHER};

#[derive(Serialize)]
pub struct LauncherState {
    version: &'static str,
    status: Status,
    /// The game's own ping (live capture only).
    ping_ms: Option<u32>,
    /// The game's window is up (or a replay stands in for it).
    game: bool,
    /// The overlay is on, and whether it's over the game now.
    overlay_on: bool,
    overlay_showing: bool,
    /// How many of its widgets show.
    widgets: usize,
    /// The fight going on, else the last one.
    fight: Option<FightLine>,
    /// Fights in the history.
    fights: usize,
}

#[derive(Serialize)]
pub struct FightLine {
    live: bool,
    target: Option<String>,
    boss: bool,
    duration_s: f64,
    my_dps: Option<f64>,
    total_dps: f64,
    players: usize,
    end: Option<FightEnd>,
    /// How far the boss's HP has gone (0-1), in a boss fight whose HP is known.
    progress: Option<f64>,
    /// The players' DPS as fractions of the top one, in the meter's order (at most
    /// [`BARS`]): the cover's skyline.
    bars: Vec<f64>,
}

const BARS: usize = 16;

impl FightLine {
    fn of(view: &EncounterView, live: bool) -> Self {
        let top = view.players.iter().map(|p| p.dps).fold(0.0, f64::max);
        let progress = match (view.target_boss, view.target_hp, view.target_max_hp) {
            (true, Some(hp), Some(max)) if max > 0 => Some(1.0 - hp.min(max) as f64 / max as f64),
            _ => None,
        };
        Self {
            live,
            target: view.target.clone(),
            boss: view.target_boss,
            duration_s: view.duration_s,
            my_dps: view.players.iter().find(|p| p.is_self).map(|p| p.dps),
            total_dps: view.total_dps,
            players: view.players.len(),
            end: view.end,
            progress,
            bars: view.players.iter().take(BARS).map(|p| if top > 0.0 { p.dps / top } else { 0.0 }).collect(),
        }
    }
}

/// What the launcher shows; it asks about once a second.
#[tauri::command]
pub fn launcher_state(app: State<'_, Arc<AppState>>) -> LauncherState {
    let (mut snap, ping_ms) = {
        let meter = app.engine.meter();
        let ping = if app.args.replay.is_none() { ping::game_ping_ms(&meter) } else { None };
        (meter.snapshot(), ping)
    };
    let live = snap.encounter.as_ref().is_some_and(|e| e.active);
    if live {
        lock(&app.data).annotate(&mut snap);
    }
    let history = lock(&app.history);
    let fight = match snap.encounter.as_ref().filter(|_| live) {
        Some(e) => Some(FightLine::of(e, true)),
        None => history.newest_first().next().map(|f| FightLine::of(&f.view, false)),
    };
    let settings = lock(&app.settings);
    LauncherState {
        version: env!("CARGO_PKG_VERSION"),
        status: lock(&app.engine.status).clone(),
        ping_ms,
        game: app.game_found.load(Ordering::SeqCst),
        overlay_on: settings.overlay_on,
        overlay_showing: app.hud_shown.load(Ordering::SeqCst),
        widgets: settings.widgets.len(),
        fight,
        fights: history.len(),
    }
}

/// Shows the launcher where you left it, else in the middle of the main screen.
pub fn show(handle: &AppHandle) -> tauri::Result<()> {
    let Some(w) = handle.get_webview_window(LAUNCHER) else { return Ok(()) };
    crate::style::launcher(&w);
    let saved = lock(&handle.state::<Arc<AppState>>().placements).get(LAUNCHER);
    if !saved.is_some_and(|p| crate::placement::restore(&w, p, false)) {
        w.center()?;
    }
    w.show()?;
    w.set_focus()
}
