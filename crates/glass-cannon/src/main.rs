//! Glass Cannon: an always-on-top DPS overlay for Aion 2 with a settings pop-out.
//!
//! The overlay opens and starts capturing straight away. A background engine thread
//! captures the game's traffic (or replays a recording) and feeds the meter; the overlay
//! polls `snapshot` to draw it. Finished fights go to a local history. The settings
//! window (appearance, status checks, meter options, tools) opens from the overlay's menu
//! button. No admin rights and no network access: the game data ships in the release.
//!
//! Options:
//!   --replay <session folder>   play back a recorded capture
//!   --speed <n>                 replay speed multiplier (default 1)
//!   --process <name>            game executable name to look for (default AION2)

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, State, WebviewWindow, WindowEvent};

use meter_core::live::{self, LiveEvent, LiveOptions};
use meter_core::meter::{EncounterView, Meter, Scope, Snapshot, DEFAULT_IDLE_TIMEOUT_US};
use meter_core::recorder::Side;
use meter_core::session;

mod analysis;
mod clickthrough;
mod fonts;
mod panels;
mod ping;
mod gamedata;
mod layout;
mod placement;
mod runs;
mod settings;
mod store;
mod style;

use gamedata::GameData;
use store::{Appearance, Fight, FightDetails, History, Instances, KnownPlayers, Profile, Settings};

const RETRY_AFTER: Duration = Duration::from_secs(5);
const OVERLAY: &str = "main";
const SETTINGS: &str = "settings";
const ANALYSIS: &str = "analysis";

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
enum Phase {
    Idle,
    Starting,
    Waiting,
    Capturing,
    Replay,
    Error,
}

#[derive(Debug, Clone, Serialize)]
struct Status {
    phase: Phase,
    message: String,
    dropped: u64,
}

struct Engine {
    meter: Mutex<Meter>,
    status: Mutex<Status>,
    started: AtomicBool,
    /// Replays run on the recording's clock instead of the wall clock.
    replay_clock: Option<AtomicU64>,
    /// Set when new game messages reach the meter, for the "as it happens" refresh.
    changed: (Mutex<bool>, Condvar),
}

impl Engine {
    fn now(&self) -> u64 {
        match &self.replay_clock {
            Some(c) => c.load(Ordering::Relaxed),
            None => live::now_us(),
        }
    }

    fn meter(&self) -> MutexGuard<'_, Meter> {
        lock(&self.meter)
    }

    fn set_status(&self, phase: Phase, message: impl Into<String>) {
        let mut s = lock(&self.status);
        s.phase = phase;
        s.message = message.into();
    }

    fn update_status(&self, f: impl FnOnce(&mut Status)) {
        f(&mut lock(&self.status));
    }

    fn mark_changed(&self) {
        *lock(&self.changed.0) = true;
        self.changed.1.notify_one();
    }

    /// Waits up to `timeout` for new messages; true if there were some.
    fn wait_changed(&self, timeout: Duration) -> bool {
        let guard = lock(&self.changed.0);
        let (mut changed, _) = self
            .changed
            .1
            .wait_timeout_while(guard, timeout, |c| !*c)
            .unwrap_or_else(|e| e.into_inner());
        std::mem::take(&mut *changed)
    }
}

/// Everything the windows talk to. Built before any window exists, so a page can never
/// ask too early.
pub struct AppState {
    engine: Arc<Engine>,
    data: Mutex<GameData>,
    history: Mutex<History>,
    /// Full analyses of recent fights.
    details: FightDetails,
    /// Dungeon runs' start and completion, as the game reported them.
    instances: Mutex<Instances>,
    /// What the analysis window shows.
    analysis_selection: Mutex<analysis::Selection>,
    /// The analysis window has been sized and placed once.
    analysis_placed: AtomicBool,
    profile: Mutex<Profile>,
    settings: Mutex<Settings>,
    config_dir: PathBuf,
    exe_dir: PathBuf,
    args: Args,
    overlay_placed: AtomicBool,
    /// Whether the running capture is saving packets (settings changes apply next start).
    recording_now: AtomicBool,
    /// The overlay wasn't restored from last time, so its first fit sets the height exactly.
    overlay_new: AtomicBool,
    /// Click-through is on (the meter only answers on its lock button).
    locked: AtomicBool,
    /// Where the overlay's lock button is.
    lock_button: Mutex<Option<clickthrough::ButtonRect>>,
    /// Player names saved by an earlier run, until we know whether they're from this login.
    known_players: Mutex<Option<KnownPlayers>>,
    /// How many named players were last saved to `players.json`.
    players_saved: AtomicUsize,
    /// PvP sessions.
    pvp: Mutex<store::PvpLog>,
    /// Field bosses from the world map.
    field_bosses: Mutex<store::FieldBosses>,
    /// Side windows placed beside the meter once already.
    panels_placed: Mutex<std::collections::HashSet<String>>,
    /// Where every window was last (saved as they move, at most once a second).
    placements: Mutex<placement::Placements>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

#[derive(Serialize)]
struct View {
    status: Status,
    meter: Snapshot,
    /// How many skill names are loaded (0 = no game data yet).
    skills_loaded: usize,
    /// Why none are loaded, when that's the case.
    skills_problem: Option<String>,
    /// Your name came from memory (the game hasn't said who you are yet this session).
    name_remembered: bool,
    history_count: usize,
    /// Whose damage counts follows your party (not chosen by hand).
    auto_scope: bool,
    appearance: Appearance,
    /// How often the overlay should ask again, in milliseconds.
    refresh_ms: u32,
    locked: bool,
    meter_view: store::MeterView,
    /// The game's own ping (live capture only).
    ping_ms: Option<u32>,
    /// For the alerts the overlay shows before events.
    timer_region: String,
    alerts: Vec<String>,
    alert_lead_min: u32,
    hide_after_s: u32,
    boss_hp: bool,
}

/// The current meter with names filled in. Also moves finished fights into the history.
/// Skill lists come only for the players in `expanded` (the rows open in the overlay):
/// the rest would only be thrown away, several times a second.
#[tauri::command]
fn snapshot(app: State<'_, Arc<AppState>>, expanded: Option<Vec<u64>>) -> View {
    let now = app.engine.now();
    let auto_scope = !lock(&app.settings).manual_scope;
    let (mut snap, finished, known, entered, ping_ms, pvp, field_bosses) = {
        let mut meter = app.engine.meter();
        meter.tick(now);
        let known = remember_players(&app, &mut meter);
        // Whose damage counts follows your party, unless chosen by hand: the party now, or
        // the one in the fight shown (it still shows them after the party breaks up).
        let scope = lock(&app.settings).scope_for(party_size(&meter));
        if meter.scope() != scope {
            meter.set_scope(scope);
        }
        let ping = if app.args.replay.is_none() { ping::game_ping_ms(&meter) } else { None };
        (meter.snapshot(), meter.take_finished(), known, meter.take_instances(), ping, meter.take_pvp(), meter.take_field_bosses())
    };
    lock(&app.field_bosses).update(field_bosses);
    if !pvp.is_empty() {
        let mut log = lock(&app.pvp);
        for s in pvp {
            log.record(s);
        }
    }
    if !entered.is_empty() {
        let mut instances = lock(&app.instances);
        for i in entered {
            instances.record(i);
        }
    }
    if let (Some(open), Some(enc)) = (&expanded, snap.encounter.as_mut()) {
        let rows = enc.players.iter_mut().chain(enc.healing.iter_mut()).chain(enc.taken.iter_mut());
        for row in rows.filter(|r| !open.contains(&r.id)) {
            row.skills.clear();
        }
    }
    if let Some(k) = known {
        k.save(&app.config_dir);
    }
    let mut data = lock(&app.data);
    data.reload_if_empty();
    let mut profile = lock(&app.profile);
    let mut history = lock(&app.history);
    for f in finished {
        let mut view = f.view;
        data.annotate_view(&mut view);
        let self_name = f.self_name.or_else(|| profile.name.clone());
        if let Some(name) = &self_name {
            profile.name_self_row(&mut view, name);
        }
        let ended_ms = view.end_us / 1000;
        if let Some(analysis) = &f.analysis {
            app.details.save(ended_ms, analysis);
        }
        history.push(Fight { ended_ms, self_name, view });
    }
    data.annotate(&mut snap);
    let name_remembered = profile.apply(&mut snap);
    let settings = lock(&app.settings).clone();
    View {
        status: lock(&app.engine.status).clone(),
        meter: snap,
        skills_loaded: data.skill_count(),
        skills_problem: data.problem().map(str::to_string),
        name_remembered,
        history_count: history.len(),
        auto_scope,
        appearance: settings.appearance,
        refresh_ms: settings.refresh_ms,
        locked: settings.locked,
        meter_view: settings.meter_view,
        ping_ms,
        timer_region: settings.timer_region,
        alerts: settings.alerts,
        alert_lead_min: settings.alert_lead_min,
        hide_after_s: settings.hide_after_s,
        boss_hp: settings.boss_hp,
    }
}

/// Names players from the last run of the meter if it was in this login, and returns
/// the names to save when more have been learned. Not for replays: their entity IDs are
/// from another login.
fn remember_players(app: &AppState, meter: &mut Meter) -> Option<KnownPlayers> {
    if app.args.replay.is_some() {
        return None;
    }
    let self_id = meter.registry().self_id()?;
    let now_ms = live::now_us() / 1000;
    let saved = lock(&app.known_players).take();
    if let Some(saved) = saved {
        if let Some(names) = saved.for_login(self_id, now_ms) {
            meter.remember_names(names);
        }
    }
    let count = meter.registry().players().count();
    (app.players_saved.swap(count, Ordering::SeqCst) != count).then(|| KnownPlayers {
        self_id,
        saved_ms: now_ms,
        players: meter.registry().players().map(|(id, n)| (id, n.to_string())).collect(),
    })
}

/// Ends the fight now; the next hit starts a new one.
#[tauri::command]
fn end_fight(app: State<'_, Arc<AppState>>) {
    let now = app.engine.now();
    app.engine.meter().end_fight(now);
}

/// What the meter's rows show (damage, healing or damage taken); saved for next time.
#[tauri::command]
fn set_meter_view(app: State<'_, Arc<AppState>>, view: store::MeterView) {
    let mut s = lock(&app.settings);
    s.meter_view = view;
    s.save(&app.config_dir);
}

/// How often the overlay redraws (one of the set rates); saved for next time.
#[tauri::command]
fn set_refresh(app: State<'_, Arc<AppState>>, ms: u32) -> Result<(), String> {
    if !store::REFRESH_RATES.contains(&ms) {
        return Err(format!("unsupported refresh rate {ms} ms"));
    }
    let mut s = lock(&app.settings);
    s.refresh_ms = ms;
    s.save(&app.config_dir);
    Ok(())
}

/// One skill's icon as a data URL, fetched the first time the overlay shows it.
#[tauri::command]
fn skill_icon(app: State<'_, Arc<AppState>>, skill: u32) -> Option<String> {
    lock(&app.data).skill_icon(skill)
}

#[tauri::command]
fn class_icon(app: State<'_, Arc<AppState>>, code: u32) -> Option<String> {
    lock(&app.data).class_icon(code)
}

/// Names a skill (an empty name clears it). Saved to the local game data.
#[tauri::command]
fn rename_skill(app: State<'_, Arc<AppState>>, skill: u32, name: String) -> Result<(), String> {
    lock(&app.data).rename_skill(skill, &name).map_err(|e| format!("{e:#}"))
}

/// Names a mob type (e.g. one NOIA2 has no English name for).
#[tauri::command]
fn rename_npc(app: State<'_, Arc<AppState>>, type_id: u32, name: String) -> Result<(), String> {
    lock(&app.data).rename_npc(type_id, &name).map_err(|e| format!("{e:#}"))
}

#[tauri::command]
fn reset(app: State<'_, Arc<AppState>>) {
    app.engine.meter().reset();
}

/// Whose damage counts (mine, party or everyone); saved for next time.
/// Whose damage counts: a scope chosen by hand, or `None` to follow your party.
#[tauri::command]
fn set_scope(app: State<'_, Arc<AppState>>, scope: Option<Scope>) {
    let mut s = lock(&app.settings);
    s.manual_scope = scope.is_some();
    if let Some(scope) = scope {
        s.scope = scope;
    }
    s.save(&app.config_dir);
    let settings = s.clone();
    drop(s);
    // Settings released before the meter: the snapshot takes them the other way round.
    let mut meter = app.engine.meter();
    let scope = settings.scope_for(party_size(&meter));
    meter.set_scope(scope);
}

/// The party size the auto scope follows: your party now, or the party that was in the
/// fight shown, whichever is larger.
fn party_size(meter: &Meter) -> usize {
    meter.registry().party().len().max(meter.shown_party_size())
}

#[tauri::command]
fn history_list(app: State<'_, Arc<AppState>>) -> runs::HistoryList {
    let fights = lock(&app.history).summaries(&app.details);
    runs::history_list(fights, &lock(&app.data), &lock(&app.instances))
}

/// Forgets every finished fight, dungeon run and PvP session: the history, the fights'
/// saved analyses, the dungeon runs' records and the PvP log. Only the dungeon you're in
/// now, if you are, is kept, so its fights still to come are still grouped as a run.
#[tauri::command]
fn clear_history(handle: AppHandle, app: State<'_, Arc<AppState>>) {
    let here = app.engine.meter().instance().map(|i| i.entered_us);
    lock(&app.history).clear();
    app.details.clear();
    lock(&app.instances).clear_except(here);
    lock(&app.pvp).clear();
    // An analysis of a saved fight (or run) goes back to the live fight: its history index
    // would otherwise point at whatever fight takes that place next.
    let selection = {
        let mut s = lock(&app.analysis_selection);
        let saved = s.fight.is_some() || s.run.is_some();
        s.fight = None;
        s.run = None;
        saved.then_some(*s)
    };
    if let (Some(selection), Some(w)) = (selection, handle.get_webview_window(ANALYSIS)) {
        let _ = w.emit("analysis-select", selection);
    }
    let _ = handle.emit("history-cleared", ());
}

#[tauri::command]
fn history_fight(app: State<'_, Arc<AppState>>, index: usize) -> Option<EncounterView> {
    lock(&app.history).fight(index).map(|f| f.view.clone())
}

#[derive(Clone)]
struct Args {
    replay: Option<PathBuf>,
    speed: f64,
    process: String,
}

fn parse_args() -> Args {
    let mut args = Args {
        replay: None,
        speed: 1.0,
        process: "AION2".into(),
    };
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--replay" => args.replay = it.next().map(PathBuf::from),
            "--speed" => {
                args.speed = it
                    .next()
                    .and_then(|s| s.parse::<f64>().ok())
                    .unwrap_or(1.0)
                    .max(0.01)
            }
            "--process" => args.process = it.next().unwrap_or(args.process),
            _ => {}
        }
    }
    args
}

/// Starts capturing (or replaying), once. Safe to call again.
fn start_engine(app: &Arc<AppState>) {
    if app.engine.started.swap(true, Ordering::SeqCst) {
        return;
    }
    let settings = lock(&app.settings).clone();
    app.engine.meter().set_scope(settings.scope_for(0));
    app.recording_now.store(settings.record && app.args.replay.is_none(), Ordering::SeqCst);
    app.engine.set_status(Phase::Starting, "starting…");
    let state = Arc::clone(app);
    thread::Builder::new()
        .name("engine".into())
        .spawn(move || match &state.args.replay {
            Some(path) => run_replay(&state.engine, path, state.args.speed),
            None => run_live(&state, settings.record),
        })
        .expect("starting the engine thread");
}

fn run_live(app: &AppState, record: bool) {
    let engine = &app.engine;
    let opts = LiveOptions {
        process: app.args.process.clone(),
        // Next to the program, never wherever Windows happened to start it.
        out_dir: app.exe_dir.join("captures"),
        devices: Vec::new(),
        duration: None,
        record,
        decode: true,
    };
    let stop = AtomicBool::new(false);
    loop {
        let result = live::run(&opts, &stop, |ev| match ev {
            LiveEvent::Waiting(msg) => engine.set_status(Phase::Waiting, msg),
            LiveEvent::Started { devices, .. } => {
                engine.set_status(Phase::Capturing, format!("capturing on {}", devices.join(", ")))
            }
            LiveEvent::Warning(msg) => engine.update_status(|s| s.message = msg),
            LiveEvent::Status(st) => engine.update_status(|s| s.dropped = st.dropped),
            LiveEvent::Messages(msgs) => {
                let mut meter = engine.meter();
                for m in &msgs {
                    meter.feed(m.ts_us, m.opcode, &m.payload);
                }
                drop(meter);
                engine.mark_changed();
            }
            LiveEvent::Finished { .. } => {}
        });
        match result {
            Ok(_) => return,
            Err(e) => {
                engine.set_status(Phase::Error, format!("{e:#}"));
                thread::sleep(RETRY_AFTER);
            }
        }
    }
}

fn run_replay(engine: &Engine, path: &Path, speed: f64) {
    let loaded = session::resolve_flow(path).and_then(|flow| session::read_messages(&flow, Side::In));
    let messages = match loaded {
        Ok(m) => m.messages,
        Err(e) => {
            engine.set_status(Phase::Error, format!("replay: {e:#}"));
            return;
        }
    };
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    engine.set_status(Phase::Replay, format!("replaying {name} at {speed}×"));
    let clock = engine.replay_clock.as_ref().expect("replay mode has a clock");
    let Some(t0) = messages.first().map(|m| m.ts_us) else {
        engine.set_status(Phase::Error, "replay: no game messages in that session");
        return;
    };
    let started = Instant::now();
    let mut last = t0;
    for m in &messages {
        let due = Duration::from_secs_f64((m.ts_us - t0) as f64 / 1e6 / speed);
        if let Some(wait) = due.checked_sub(started.elapsed()) {
            thread::sleep(wait);
        }
        clock.store(m.ts_us, Ordering::Relaxed);
        engine.meter().feed(m.ts_us, m.opcode, &m.payload);
        engine.mark_changed();
        last = m.ts_us;
    }
    // Let the last fight end (and reach the history).
    clock.store(last + DEFAULT_IDLE_TIMEOUT_US + 1, Ordering::Relaxed);
    engine.set_status(Phase::Replay, format!("replay of {name} finished"));
}

/// Shows the overlay, placing it the first time.
fn show_overlay(handle: &AppHandle) -> tauri::Result<()> {
    let Some(window) = handle.get_webview_window(OVERLAY) else {
        return Ok(());
    };
    let app = handle.state::<Arc<AppState>>();
    if !app.overlay_placed.swap(true, Ordering::SeqCst) {
        let saved = lock(&app.placements).get(OVERLAY);
        if let Some(placed_at) = placement::place(&window, saved) {
            app.overlay_new.store(true, Ordering::SeqCst);
            // Not restored from last time: move to the game's monitor once it starts.
            let w: WebviewWindow = window.clone();
            thread::spawn(move || {
                while !placement::follow_game(&w, placed_at) {
                    thread::sleep(Duration::from_secs(2));
                }
            });
        }
    }
    window.show()?;
    window.set_focus()?;
    Ok(())
}

/// Sets the overlay's height for five rows (the page measures it for the current font
/// size and layout). `exact` after a layout change; otherwise only ever grows it, so a
/// taller window you made yourself stays. `width` (after a reset) sets that too.
/// `min_width` is what the title bar needs for its longest text: the window grows to it on
/// first start, after a layout change or a reset, but a width you chose is left alone.
/// Keeps the window on its monitor.
#[tauri::command]
fn fit_overlay(
    handle: AppHandle,
    app: State<'_, Arc<AppState>>,
    height: f64,
    exact: bool,
    width: Option<f64>,
    min_width: Option<f64>,
) -> Result<(), String> {
    let w = handle.get_webview_window(OVERLAY).ok_or("no overlay")?;
    let first_run = app.overlay_new.swap(false, Ordering::SeqCst);
    let scale = w.scale_factor().map_err(|e| e.to_string())?;
    let now = w.inner_size().map_err(|e| e.to_string())?.to_logical::<f64>(scale);
    let mut wanted = if exact || first_run { height } else { now.height.max(height) };
    let monitor = w.current_monitor().ok().flatten();
    if let Some(m) = &monitor {
        wanted = wanted.min(m.work_area().size.height as f64 / m.scale_factor() - 20.0);
    }
    let mut new_width = width.unwrap_or(now.width);
    if let Some(min) = min_width.filter(|_| exact || first_run || width.is_some()) {
        new_width = new_width.max(min);
    }
    if let Some(m) = &monitor {
        new_width = new_width.min(m.work_area().size.width as f64 / m.scale_factor() - 20.0);
    }
    if (wanted - now.height).abs() < 1.0 && (new_width - now.width).abs() < 1.0 {
        return Ok(());
    }
    w.set_size(LogicalSize::new(new_width, wanted)).map_err(|e| e.to_string())?;
    // A taller or wider window mustn't run off the bottom or the right of the screen.
    if let (Some(m), Ok(pos)) = (monitor, w.outer_position()) {
        let area = m.work_area();
        let bottom = area.position.y + area.size.height as i32;
        let right = area.position.x + area.size.width as i32;
        let tall = (wanted * scale).ceil() as i32;
        let wide = (new_width * scale).ceil() as i32;
        let y = if pos.y + tall > bottom { (bottom - tall).max(area.position.y) } else { pos.y };
        let x = if pos.x + wide > right { (right - wide).max(area.position.x) } else { pos.x };
        if (x, y) != (pos.x, pos.y) {
            let _ = w.set_position(PhysicalPosition::new(x, y));
        }
    }
    Ok(())
}

fn exe_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .unwrap_or_default()
}

/// %APPDATA%\io.github.xinjalin.glass-cannon (the folder Tauri uses for this app's
/// identifier). Settings from before the rename (when the app was "Aion Meter") move over.
fn config_dir() -> PathBuf {
    let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(exe_dir);
    let dir = base.join("io.github.xinjalin.glass-cannon");
    let old = base.join("local.aion-meter");
    if !dir.exists() && old.is_dir() {
        let _ = std::fs::rename(&old, &dir);
    }
    dir
}

fn main() {
    let args = parse_args();
    let exe_dir = exe_dir();
    let config_dir = config_dir();
    let data = GameData::load(
        vec![
            exe_dir.join("gamedata"),
            exe_dir.join("skills"),
            config_dir.join("gamedata"),
            // A build from the repository (target/release): the repository's own copy.
            exe_dir.join("../../gamedata"),
        ],
        Some(config_dir.join("names.json")),
    );
    write_startup_log(&config_dir, &data);

    let replay = args.replay.is_some();
    let known_players = KnownPlayers::load(&config_dir);
    // Known from the start: the page can ask to be fitted before the overlay is placed.
    let placements = placement::Placements::load(&config_dir);
    let overlay_new = placements.get(OVERLAY).is_none();
    let config_dir_for_pvp = config_dir.clone();
    let state = Arc::new(AppState {
        engine: Arc::new(Engine {
            meter: Mutex::new({
                let mut m = Meter::default();
                m.set_boss_types(data.boss_types());
                m
            }),
            status: Mutex::new(Status {
                phase: Phase::Idle,
                message: "not started".into(),
                dropped: 0,
            }),
            started: AtomicBool::new(false),
            replay_clock: replay.then(|| AtomicU64::new(0)),
            changed: (Mutex::new(false), Condvar::new()),
        }),
        data: Mutex::new(data),
        history: Mutex::new(History::load(&config_dir)),
        details: FightDetails::new(&config_dir),
        instances: Mutex::new(Instances::load(&config_dir)),
        analysis_selection: Mutex::new(analysis::Selection::default()),
        analysis_placed: AtomicBool::new(false),
        profile: Mutex::new(Profile::load(&config_dir)),
        settings: Mutex::new(Settings::load(&config_dir)),
        config_dir,
        exe_dir,
        args,
        overlay_placed: AtomicBool::new(false),
        recording_now: AtomicBool::new(false),
        overlay_new: AtomicBool::new(overlay_new),
        locked: AtomicBool::new(false),
        lock_button: Mutex::new(None),
        known_players: Mutex::new(Some(known_players)),
        players_saved: AtomicUsize::new(0),
        pvp: Mutex::new(store::PvpLog::load(&config_dir_for_pvp)),
        field_bosses: Mutex::new(store::FieldBosses::load(&config_dir_for_pvp)),
        panels_placed: Mutex::new(std::collections::HashSet::new()),
        placements: Mutex::new(placements),
    });
    state.locked.store(lock(&state.settings).locked, Ordering::SeqCst);

    tauri::Builder::default()
        .manage(Arc::clone(&state))
        .invoke_handler(tauri::generate_handler![
            snapshot,
            reset,
            end_fight,
            set_meter_view,
            set_refresh,
            clickthrough::set_locked,
            clickthrough::lock_button_rect,
            set_scope,
            skill_icon,
            class_icon,
            rename_skill,
            rename_npc,
            history_list,
            clear_history,
            history_fight,
            settings::settings_status,
            settings::set_record,
            settings::set_appearance,
            settings::pick_colour,
            settings::fit_settings,
            analysis::open_analysis,
            analysis::follow_fight,
            analysis::analysis_pick,
            appearance,
            analysis::analysis_selection,
            analysis::analysis_stamp,
            analysis::analysis_data,
            settings::reset_appearance,
            settings::list_fonts,
            fit_overlay,
            settings::create_shortcuts,
            settings::open_history,
            settings::toggle_settings,
            settings::hide_settings,
            panels::toggle_panel,
            panels::panels_open,
            panels::show_fight,
            panels::cooldowns,
            panels::pvp_list,
            panels::timer_region,
            panels::set_timer_region,
            panels::target,
            panels::field_bosses,
            panels::rename_field_boss,
            panels::set_alert,
            panels::set_alert_lead,
            panels::set_hide_after,
            panels::set_boss_hp,
            panels::timer_settings,
            panels::party_cooldowns,
        ])
        .setup(move |app| {
            restyle_windows(app.handle(), &lock(&state.settings).appearance.clone());
            // The meter is the app: start capturing and show it straight away.
            start_engine(&state);
            show_overlay(app.handle())?;
            clickthrough::watch(app.handle().clone(), Arc::clone(&state));
            notify_changes(app.handle().clone(), Arc::clone(&state));
            panels::reopen(app.handle(), &state);
            save_placements(Arc::clone(&state));
            Ok(())
        })
        .on_window_event(|window, event| {
            let app = window.state::<Arc<AppState>>();
            match event {
                // Every window is put back where you leave it next time.
                WindowEvent::Moved(_) | WindowEvent::Resized(_) => {
                    if let Some(p) = placement::current(window) {
                        lock(&app.placements).set(window.label(), p);
                    }
                }
                WindowEvent::CloseRequested { api, .. } => match window.label() {
                    OVERLAY => {
                        let mut placements = lock(&app.placements);
                        if let Some(p) = placement::current(window) {
                            placements.set(OVERLAY, p);
                        }
                        placements.save_if_changed();
                        drop(placements);
                        window.app_handle().exit(0);
                    }
                    // Closing the settings only tucks them away.
                    _ => {
                        api.prevent_close();
                        let _ = window.hide();
                    }
                },
                _ => {}
            }
        })
        .run(tauri::generate_context!())
        .expect("running Glass Cannon");
}

/// Writes the windows' placements once a second when one moved.
fn save_placements(app: Arc<AppState>) {
    thread::Builder::new()
        .name("placements".into())
        .spawn(move || loop {
            thread::sleep(Duration::from_secs(1));
            lock(&app.placements).save_if_changed();
        })
        .expect("starting the placements thread");
}

/// Applies the background effect, corners and border to every window, and tells each
/// page so the theme and font change everywhere at once.
fn restyle_windows(handle: &AppHandle, appearance: &Appearance) {
    restyle_windows_but(handle, appearance, None);
}

/// Restyles every window but `skip` (its page still gets the new appearance).
fn restyle_windows_but(handle: &AppHandle, appearance: &Appearance, skip: Option<&str>) {
    for label in [OVERLAY, SETTINGS, ANALYSIS].into_iter().chain(panels::PANELS) {
        if Some(label) == skip {
            continue;
        }
        if let Some(w) = handle.get_webview_window(label) {
            style::apply(&w, appearance, label == OVERLAY);
        }
    }
    let _ = handle.emit("appearance", appearance);
}

/// The saved appearance, for a page as it loads.
#[tauri::command]
fn appearance(app: State<'_, Arc<AppState>>) -> Appearance {
    lock(&app.settings).appearance.clone()
}

/// With the update rate on "as it happens", tells the overlay whenever new game messages
/// arrive (at most every [`store::REALTIME`] ms), instead of it asking on a timer.
fn notify_changes(handle: AppHandle, app: Arc<AppState>) {
    thread::Builder::new()
        .name("notify".into())
        .spawn(move || loop {
            if !app.engine.wait_changed(Duration::from_secs(1)) {
                continue;
            }
            if lock(&app.settings).refresh_ms == store::REALTIME {
                notify_overlay(&handle, "meter-changed");
                thread::sleep(Duration::from_millis(store::REALTIME.into()));
            }
        })
        .expect("starting the notify thread");
}

/// Something for the overlay to react to.
fn notify_overlay(handle: &AppHandle, event: &str) {
    let _ = handle.emit_to(OVERLAY, event, ());
}

/// The app has no console, so record what it found at startup in `meter.log`.
fn write_startup_log(config_dir: &Path, data: &GameData) {
    let line = |k: &str, v: String| format!("{k:<14} {v}\n");
    let mut log = String::new();
    log += &line("version", env!("CARGO_PKG_VERSION").to_string());
    log += &line("started", format!("{} (unix us)", live::now_us()));
    log += &line("exe", std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_default());
    log += &line("working dir", std::env::current_dir().map(|p| p.display().to_string()).unwrap_or_default());
    log += &line("config dir", config_dir.display().to_string());
    log += &line("data dir", data.dir().display().to_string());
    log += &line("skills loaded", data.skill_count().to_string());
    log += &line("mob names", data.named_npc_count().to_string());
    log += &line("skill details", data.detail_count().to_string());
    log += &line("theostones", data.theostone_count().to_string());
    log += &line("dungeon mobs", data.dungeon_mob_count().to_string());
    if let Some(p) = data.problem() {
        log += &line("data problem", p.to_string());
    }
    let _ = std::fs::create_dir_all(config_dir);
    let _ = std::fs::write(config_dir.join("meter.log"), log);
}

