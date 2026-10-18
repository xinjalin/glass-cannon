//! Aion Meter: an always-on-top DPS overlay.
//!
//! A background engine thread captures the game's traffic (or replays a recorded
//! session) and feeds the meter; the window polls `snapshot` to draw it.
//! No admin rights, no network access of its own, no auto-update.
//!
//! Options:
//!   --replay <session folder>   play back a recorded capture instead of going live
//!   --speed <n>                 replay speed multiplier (default 1)
//!   --process <name>            game executable name to look for (default AION2)
//!   --record                    also save live captures to --out (default "captures")

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{Manager, State, WindowEvent};

use meter_core::live::{self, LiveEvent, LiveOptions};
use meter_core::meter::{Meter, Scope, Snapshot, DEFAULT_IDLE_TIMEOUT_US};
use meter_core::recorder::Side;
use meter_core::session;

mod placement;
mod skills;

const RETRY_AFTER: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
enum Phase {
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
        self.meter.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn set_status(&self, phase: Phase, message: impl Into<String>) {
        let mut s = self.status.lock().unwrap_or_else(|e| e.into_inner());
        s.phase = phase;
        s.message = message.into();
    }

    fn update_status(&self, f: impl FnOnce(&mut Status)) {
        f(&mut self.status.lock().unwrap_or_else(|e| e.into_inner()));
    }
}

#[derive(Serialize)]
struct View {
    status: Status,
    meter: Snapshot,
    /// How many skill names are loaded (0 = nothing imported yet).
    skills_loaded: usize,
    /// Why none are loaded, when that's the case.
    skills_problem: Option<String>,
}

/// The current meter, with skill names filled in, so the page needs nothing else.
#[tauri::command]
fn snapshot(engine: State<'_, Arc<Engine>>, catalog: State<'_, Mutex<skills::Catalog>>) -> View {
    let now = engine.now();
    let mut snap = {
        let mut meter = engine.meter();
        meter.tick(now);
        meter.snapshot()
    };
    let mut catalog = catalog.lock().unwrap_or_else(|e| e.into_inner());
    catalog.reload_if_empty();
    catalog.annotate(&mut snap);
    View {
        status: engine.status.lock().unwrap_or_else(|e| e.into_inner()).clone(),
        meter: snap,
        skills_loaded: catalog.len(),
        skills_problem: catalog.problem().map(str::to_string),
    }
}

/// One skill's icon as a data URL, fetched the first time the overlay shows it.
#[tauri::command]
fn skill_icon(catalog: State<'_, Mutex<skills::Catalog>>, skill: u32) -> Option<String> {
    catalog.lock().unwrap_or_else(|e| e.into_inner()).icon(skill)
}

/// Names a skill (an empty name clears it). Saved to the local skills file.
#[tauri::command]
fn rename_skill(catalog: State<'_, Mutex<skills::Catalog>>, skill: u32, name: String) -> Result<(), String> {
    catalog
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .rename(skill, &name)
        .map_err(|e| format!("{e:#}"))
}

#[tauri::command]
fn reset(engine: State<'_, Arc<Engine>>) {
    engine.meter().reset();
}

#[tauri::command]
fn set_everyone(engine: State<'_, Arc<Engine>>, everyone: bool) {
    engine
        .meter()
        .set_scope(if everyone { Scope::Everyone } else { Scope::Mine });
}

struct Args {
    replay: Option<PathBuf>,
    speed: f64,
    process: String,
    record: bool,
    out: PathBuf,
}

fn parse_args() -> Args {
    let mut args = Args {
        replay: None,
        speed: 1.0,
        process: "AION2".into(),
        record: false,
        out: PathBuf::from("captures"),
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
            "--record" => args.record = true,
            "--out" => args.out = it.next().map(PathBuf::from).unwrap_or(args.out),
            _ => {}
        }
    }
    args
}

fn run_live(engine: &Engine, args: &Args) {
    let opts = LiveOptions {
        process: args.process.clone(),
        out_dir: args.out.clone(),
        devices: Vec::new(),
        duration: None,
        record: args.record,
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
    // Let the last fight show as ended.
    clock.store(last + DEFAULT_IDLE_TIMEOUT_US + 1, Ordering::Relaxed);
    engine.set_status(Phase::Replay, format!("replay of {name} finished"));
}

fn main() {
    let args = parse_args();
    let engine = Arc::new(Engine {
        meter: Mutex::new(Meter::default()),
        status: Mutex::new(Status {
            phase: Phase::Starting,
            message: "starting…".into(),
            dropped: 0,
        }),
        replay_clock: args.replay.as_ref().map(|_| AtomicU64::new(0)),
    });

    let worker = Arc::clone(&engine);
    thread::Builder::new()
        .name("engine".into())
        .spawn(move || match &args.replay {
            Some(path) => run_replay(&worker, path, args.speed),
            None => run_live(&worker, &args),
        })
        .expect("starting the engine thread");

    tauri::Builder::default()
        .manage(engine)
        // Registered before the window exists, so the page can always ask for it;
        // filled from disk in `setup`.
        .manage(Mutex::new(skills::Catalog::empty()))
        .invoke_handler(tauri::generate_handler![
            snapshot,
            reset,
            set_everyone,
            skill_icon,
            rename_skill
        ])
        .setup(|app| {
            let window = app
                .get_webview_window("main")
                .expect("the main window is defined in tauri.conf.json");
            let config_dir = app.path().app_config_dir()?;
            // A `skills` folder next to the program first, then the settings folder.
            let mut skill_dirs = Vec::new();
            if let Some(exe_dir) = std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf)) {
                skill_dirs.push(exe_dir.join("skills"));
            }
            skill_dirs.push(config_dir.join("skills"));
            let catalog = skills::Catalog::load_first(skill_dirs);
            write_startup_log(&config_dir, &catalog);
            *app
                .state::<Mutex<skills::Catalog>>()
                .lock()
                .unwrap_or_else(|e| e.into_inner()) = catalog;
            let state_file = placement::state_path(&config_dir);
            // The window starts hidden so it never flashes in the wrong place.
            let auto = placement::place(&window, placement::load(&state_file));
            window.show()?;
            if let Some(placed_at) = auto {
                // Not restored from last time: move to the game's monitor once it starts.
                thread::spawn(move || {
                    while !placement::follow_game(&window, placed_at) {
                        thread::sleep(Duration::from_secs(2));
                    }
                });
            }
            app.manage(StateFile(state_file));
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { .. } = event {
                if let Some(StateFile(path)) = window.try_state::<StateFile>().as_deref() {
                    placement::save(window, path);
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("running Aion Meter");
}

/// Where the window position is remembered between runs.
struct StateFile(PathBuf);

/// The app has no console, so record what it found at startup in `meter.log`.
fn write_startup_log(config_dir: &Path, catalog: &skills::Catalog) {
    let line = |k: &str, v: String| format!("{k:<14} {v}\n");
    let mut log = String::new();
    log += &line("version", env!("CARGO_PKG_VERSION").to_string());
    log += &line("started", format!("{} (unix µs)", live::now_us()));
    log += &line("exe", std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_default());
    log += &line("working dir", std::env::current_dir().map(|p| p.display().to_string()).unwrap_or_default());
    log += &line("config dir", config_dir.display().to_string());
    log += &line("skills dir", catalog.dir().display().to_string());
    log += &line("skills loaded", catalog.len().to_string());
    if let Some(p) = catalog.problem() {
        log += &line("skills problem", p.to_string());
    }
    let _ = std::fs::create_dir_all(config_dir);
    let _ = std::fs::write(config_dir.join("meter.log"), log);
}
