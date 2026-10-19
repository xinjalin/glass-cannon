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

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, State, WebviewWindow, WindowEvent};

use meter_core::buff::{BuffBreakdown, BuffSpan};
use meter_core::live::{self, LiveEvent, LiveOptions};
use meter_core::meter::{EncounterView, Meter, RotationEvent, Scope, SkillRow, Snapshot, DEFAULT_IDLE_TIMEOUT_US};
use meter_core::recorder::Side;
use meter_core::session;

mod fonts;
mod gamedata;
mod placement;
mod settings;
mod store;
mod style;

use gamedata::GameData;
use store::{Appearance, Fight, FightSummary, History, Profile, Settings};

const RETRY_AFTER: Duration = Duration::from_secs(5);
const OVERLAY: &str = "main";
const SETTINGS: &str = "settings";
const DETAIL: &str = "detail";

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
    /// Round trip to the game server.
    ping_ms: Option<u32>,
}

struct Engine {
    meter: Mutex<Meter>,
    status: Mutex<Status>,
    started: AtomicBool,
    /// Replays run on the recording's clock instead of the wall clock.
    replay_clock: Option<AtomicU64>,
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
}

/// Everything the windows talk to. Built before any window exists, so a page can never
/// ask too early.
pub struct AppState {
    engine: Arc<Engine>,
    data: Mutex<GameData>,
    history: Mutex<History>,
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
    /// The player the detail window is showing.
    detail_player: Mutex<Option<u64>>,
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
    appearance: Appearance,
}

/// The current meter with names filled in. Also moves finished fights into the history.
#[tauri::command]
fn snapshot(app: State<'_, Arc<AppState>>) -> View {
    let now = app.engine.now();
    let (mut snap, finished) = {
        let mut meter = app.engine.meter();
        meter.tick(now);
        (meter.snapshot(), meter.take_finished())
    };
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
        history.push(Fight { ended_ms, self_name, view });
    }
    data.annotate(&mut snap);
    let name_remembered = profile.apply(&mut snap);
    View {
        status: lock(&app.engine.status).clone(),
        meter: snap,
        skills_loaded: data.skill_count(),
        skills_problem: data.problem().map(str::to_string),
        name_remembered,
        history_count: history.len(),
        appearance: lock(&app.settings).appearance.clone(),
    }
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
#[tauri::command]
fn set_scope(app: State<'_, Arc<AppState>>, scope: Scope) {
    app.engine.meter().set_scope(scope);
    let mut s = lock(&app.settings);
    s.scope = scope;
    s.save(&app.config_dir);
}

#[tauri::command]
fn history_list(app: State<'_, Arc<AppState>>) -> Vec<FightSummary> {
    lock(&app.history).summaries()
}

#[tauri::command]
fn history_fight(app: State<'_, Arc<AppState>>, index: usize) -> Option<EncounterView> {
    lock(&app.history).fight(index).map(|f| f.view.clone())
}

/// What the detail window shows for one player: full skill list, rotation and buffs.
#[derive(Serialize)]
struct Detail {
    name: String,
    is_self: bool,
    skills: Vec<SkillRow>,
    rotation: Vec<RotationEvent>,
    buffs_per_skill: BuffBreakdown,
    buff_spans: Vec<BuffSpan>,
    /// Buff abnormal id → name, for the buffs shown here.
    buffs: HashMap<u32, String>,
    appearance: Appearance,
}

/// Opens the detail window for one player and remembers which player it's showing.
#[tauri::command]
fn open_detail(handle: AppHandle, app: State<'_, Arc<AppState>>, player_id: u64) -> Result<(), String> {
    *lock(&app.detail_player) = Some(player_id);
    let w = handle.get_webview_window(DETAIL).ok_or("no detail window")?;
    w.show().map_err(|e| e.to_string())?;
    w.set_focus().map_err(|e| e.to_string())
}

/// The detail window's data, for the remembered player.
#[tauri::command]
fn detail_state(app: State<'_, Arc<AppState>>) -> Option<Detail> {
    let player_id = (*lock(&app.detail_player))?;
    let now = app.engine.now();
    let (name, is_self, mut skills, rotation, buffs_per_skill, buff_spans) = {
        let meter = app.engine.meter();
        (
            meter.registry().label(player_id),
            meter.registry().is_self(player_id),
            meter.full_skills(player_id),
            meter.rotation(player_id),
            meter.buffs_per_skill(player_id),
            meter.buff_spans(now),
        )
    };
    let mut buffs: HashMap<u32, String> = HashMap::new();
    {
        let data = lock(&app.data);
        data.annotate_skills(&mut skills);
        for s in &buff_spans {
            if let Some(n) = data.buff_name(s.skill) {
                buffs.entry(s.skill).or_insert_with(|| n.to_string());
            }
        }
        for sb in &buffs_per_skill.skills {
            for cov in sb.always.iter().chain(&sb.buffs).chain(&sb.debuffs) {
                if let Some(n) = data.buff_name(cov.id.skill) {
                    buffs.entry(cov.id.skill).or_insert_with(|| n.to_string());
                }
            }
        }
    }
    let appearance = lock(&app.settings).appearance.clone();
    Some(Detail { name, is_self, skills, rotation, buffs_per_skill, buff_spans, buffs, appearance })
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
    app.engine.meter().set_scope(settings.scope);
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
            LiveEvent::Status(st) => engine.update_status(|s| {
                s.dropped = st.dropped;
                s.ping_ms = st.ping_ms;
            }),
            LiveEvent::Messages(msgs) => {
                let mut meter = engine.meter();
                for m in &msgs {
                    meter.feed(m.ts_us, m.opcode, &m.payload);
                }
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
        let state_file = placement::state_path(&app.config_dir);
        if let Some(placed_at) = placement::place(&window, placement::load(&state_file)) {
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
/// taller window you made yourself stays. `width` (after a reset) sets that too. Keeps
/// the window on its monitor.
#[tauri::command]
fn fit_overlay(
    handle: AppHandle,
    app: State<'_, Arc<AppState>>,
    height: f64,
    exact: bool,
    width: Option<f64>,
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
    let new_width = width.unwrap_or(now.width);
    if (wanted - now.height).abs() < 1.0 && (new_width - now.width).abs() < 1.0 {
        return Ok(());
    }
    w.set_size(LogicalSize::new(new_width, wanted)).map_err(|e| e.to_string())?;
    // A taller window mustn't run off the bottom of the screen.
    if let (Some(m), Ok(pos)) = (monitor, w.outer_position()) {
        let area = m.work_area();
        let bottom = area.position.y + area.size.height as i32;
        let tall = (wanted * scale).ceil() as i32;
        if pos.y + tall > bottom {
            let y = (bottom - tall).max(area.position.y);
            let _ = w.set_position(PhysicalPosition::new(pos.x, y));
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
        vec![exe_dir.join("gamedata"), exe_dir.join("skills"), config_dir.join("gamedata")],
        Some(config_dir.join("names.json")),
    );
    write_startup_log(&config_dir, &data);

    let replay = args.replay.is_some();
    let state = Arc::new(AppState {
        engine: Arc::new(Engine {
            meter: Mutex::new(Meter::default()),
            status: Mutex::new(Status {
                phase: Phase::Idle,
                message: "not started".into(),
                dropped: 0,
                ping_ms: None,
            }),
            started: AtomicBool::new(false),
            replay_clock: replay.then(|| AtomicU64::new(0)),
        }),
        data: Mutex::new(data),
        history: Mutex::new(History::load(&config_dir)),
        profile: Mutex::new(Profile::load(&config_dir)),
        settings: Mutex::new(Settings::load(&config_dir)),
        config_dir,
        exe_dir,
        args,
        overlay_placed: AtomicBool::new(false),
        recording_now: AtomicBool::new(false),
        overlay_new: AtomicBool::new(false),
        detail_player: Mutex::new(None),
    });

    tauri::Builder::default()
        .manage(Arc::clone(&state))
        .invoke_handler(tauri::generate_handler![
            snapshot,
            reset,
            set_scope,
            skill_icon,
            class_icon,
            rename_skill,
            rename_npc,
            history_list,
            history_fight,
            open_detail,
            detail_state,
            settings::settings_status,
            settings::set_record,
            settings::set_appearance,
            settings::fit_settings,
            settings::reset_appearance,
            settings::list_fonts,
            fit_overlay,
            settings::create_shortcuts,
            settings::open_history,
            settings::toggle_settings,
            settings::hide_settings,
        ])
        .setup(move |app| {
            restyle_windows(app.handle(), &lock(&state.settings).appearance.clone());
            // The meter is the app: start capturing and show it straight away.
            start_engine(&state);
            show_overlay(app.handle())?;
            Ok(())
        })
        .on_window_event(|window, event| {
            let WindowEvent::CloseRequested { api, .. } = event else {
                return;
            };
            let app = window.state::<Arc<AppState>>();
            match window.label() {
                OVERLAY => {
                    placement::save(window, &placement::state_path(&app.config_dir));
                    window.app_handle().exit(0);
                }
                // Closing the settings only tucks them away.
                _ => {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("running Glass Cannon");
}

/// Applies the background effect, corners and border to both windows.
fn restyle_windows(handle: &AppHandle, appearance: &Appearance) {
    for label in [OVERLAY, SETTINGS, DETAIL] {
        if let Some(w) = handle.get_webview_window(label) {
            style::apply(&w, appearance, label == OVERLAY);
        }
    }
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
    if let Some(p) = data.problem() {
        log += &line("data problem", p.to_string());
    }
    let _ = std::fs::create_dir_all(config_dir);
    let _ = std::fs::write(config_dir.join("meter.log"), log);
}

