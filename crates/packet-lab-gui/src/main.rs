//! Packet Lab: watch and inspect Aion 2's message stream, live from the game or from a
//! recording, against what `docs/opcodes.md` says each opcode is.
//!
//! A worker thread feeds decoded messages (server to client; what the game sends is
//! encrypted) into a store of the latest [`KEEP`]; the page asks for counts by opcode, the
//! messages that match its filter, and one message's bytes, a few times a second.
//!
//! Options:
//!   --hidden    start without showing the window (for scripted checks)

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod catalog;
mod store;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{Manager, State};

use meter_core::live::{self, LiveEvent, LiveOptions};
use meter_core::recorder::Side;
use meter_core::session::{self, TimedMessage};

use store::{Find, MessageView, Query, RowsView, StatView, Store};

/// A recording's messages are added this many at a time when loaded at once, so the page
/// shows them coming in.
const LOAD_CHUNK: usize = 5_000;
/// Flows with fewer messages than this aren't the game's.
const MIN_GAME_MESSAGES: usize = 20;
/// A game connection's messages are mostly opcodes the notes know; other connections
/// (HTTPS and the like) only decode into noise.
const MIN_KNOWN_SHARE: f64 = 0.8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
enum Kind {
    Idle,
    Live,
    Recording,
}

/// What's feeding the store.
struct Source {
    kind: Kind,
    /// The process or recording.
    label: String,
    /// What it's doing ("capturing on …", "waiting for …", an error).
    message: String,
    /// Set to stop its worker.
    stop: Arc<AtomicBool>,
}

struct AppState {
    catalog: std::collections::BTreeMap<u16, catalog::Entry>,
    store: Mutex<Store>,
    source: Mutex<Source>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl AppState {
    /// Stops the worker, empties the store, and starts a new source's record.
    fn begin(&self, kind: Kind, label: String) -> Arc<AtomicBool> {
        let stop = Arc::new(AtomicBool::new(false));
        let mut s = lock(&self.source);
        s.stop.store(true, Ordering::SeqCst);
        *s = Source { kind, label, message: "starting…".into(), stop: Arc::clone(&stop) };
        drop(s);
        lock(&self.store).clear();
        stop
    }

    /// The worker's news, if it's still the current source.
    fn say(&self, stop: &AtomicBool, message: impl Into<String>) {
        if !stop.load(Ordering::SeqCst) {
            lock(&self.source).message = message.into();
        }
    }

    fn add(&self, stop: &AtomicBool, messages: Vec<TimedMessage>) {
        if !stop.load(Ordering::SeqCst) {
            lock(&self.store).push(messages);
        }
    }
}

#[derive(Serialize)]
struct StatusView {
    kind: Kind,
    label: String,
    message: String,
    /// Messages seen, and kept (the latest [`store::KEEP`]).
    total: u64,
    kept: usize,
    /// Seconds from the first message to the last.
    span_s: f64,
    /// Messages a second over the last few seconds of the stream.
    rate: f64,
}

#[tauri::command]
fn catalog(app: State<'_, Arc<AppState>>) -> Vec<catalog::Entry> {
    app.catalog.values().cloned().collect()
}

#[tauri::command]
fn status(app: State<'_, Arc<AppState>>) -> StatusView {
    let (kind, label, message) = {
        let s = lock(&app.source);
        (s.kind, s.label.clone(), s.message.clone())
    };
    let store = lock(&app.store);
    StatusView { kind, label, message, total: store.total(), kept: store.kept(), span_s: store.span_s(), rate: store.rate() }
}

#[tauri::command]
fn stats(app: State<'_, Arc<AppState>>) -> Vec<StatView> {
    lock(&app.store).stats()
}

#[tauri::command]
fn rows(app: State<'_, Arc<AppState>>, query: Query) -> Result<RowsView, String> {
    let find = query.find.as_deref().map(Find::parse).transpose()?;
    Ok(lock(&app.store).rows(&query, find.as_ref()))
}

#[tauri::command]
fn message(app: State<'_, Arc<AppState>>, seq: u64, find: Option<String>) -> Result<Option<MessageView>, String> {
    let find = find.as_deref().map(Find::parse).transpose()?;
    Ok(lock(&app.store).message(seq, find.as_ref()))
}

#[tauri::command]
fn clear(app: State<'_, Arc<AppState>>) {
    lock(&app.store).clear();
}

#[tauri::command]
fn stop_source(app: State<'_, Arc<AppState>>) {
    let mut s = lock(&app.source);
    s.stop.store(true, Ordering::SeqCst);
    if s.kind == Kind::Live {
        s.message = "stopped".into();
    }
}

/// Captures the game's traffic live (Npcap, no admin rights), decoding its stream.
#[tauri::command]
fn start_live(app: State<'_, Arc<AppState>>, process: String) -> Result<(), String> {
    let process = process.trim().to_string();
    if process.is_empty() {
        return Err("give the game's process name (AION2)".into());
    }
    let state = Arc::clone(&app);
    let stop = app.begin(Kind::Live, process.clone());
    thread::Builder::new()
        .name("live".into())
        .spawn(move || {
            let opts = LiveOptions {
                process,
                out_dir: std::env::temp_dir().join("packet-lab"),
                devices: Vec::new(),
                duration: None,
                record: false,
                decode: true,
            };
            let result = live::run(&opts, &stop, |ev| match ev {
                LiveEvent::Waiting(m) => state.say(&stop, m),
                LiveEvent::Started { devices, .. } => state.say(&stop, format!("capturing on {}", devices.join(", "))),
                LiveEvent::Warning(m) => state.say(&stop, m),
                LiveEvent::Status(s) if s.dropped > 0 => state.say(&stop, format!("{} packets dropped", s.dropped)),
                LiveEvent::Messages(m) => state.add(&stop, m),
                _ => {}
            });
            if let Err(e) = result {
                state.say(&stop, format!("{e:#}"));
            }
        })
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Opens a recording (a session folder or one flow folder): all at once with `speed` 0,
/// else played at that speed.
#[tauri::command]
fn open_recording(app: State<'_, Arc<AppState>>, path: String, speed: f64) -> Result<(), String> {
    let path = PathBuf::from(path.trim().trim_matches('"'));
    let label = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let state = Arc::clone(&app);
    let stop = app.begin(Kind::Recording, label);
    thread::Builder::new()
        .name("recording".into())
        .spawn(move || {
            let messages = match read_recording(&path, &state.catalog) {
                Ok(m) => m,
                Err(e) => return state.say(&stop, format!("{e:#}")),
            };
            let n = messages.len();
            state.say(&stop, if speed > 0.0 { format!("playing {n} messages at {speed}×") } else { format!("loading {n} messages") });
            if speed > 0.0 {
                play(&state, &stop, messages, speed);
            } else {
                let mut it = messages.into_iter().peekable();
                while it.peek().is_some() && !stop.load(Ordering::SeqCst) {
                    state.add(&stop, it.by_ref().take(LOAD_CHUNK).collect());
                    thread::sleep(Duration::from_millis(15));
                }
            }
            state.say(&stop, format!("{n} messages"));
        })
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// A session's game connections (each server it was moved to), or one flow, in time order.
fn read_recording(path: &Path, known: &std::collections::BTreeMap<u16, catalog::Entry>) -> anyhow::Result<Vec<TimedMessage>> {
    if path.join("index.jsonl").is_file() {
        return Ok(session::read_messages(path, Side::In)?.messages);
    }
    let flows = path.join("flows");
    anyhow::ensure!(flows.is_dir(), "{} is neither a session folder nor a flow folder", path.display());
    let mut all = Vec::new();
    for entry in std::fs::read_dir(&flows)? {
        let dir = entry?.path();
        if !dir.file_name().is_some_and(|n| n.to_string_lossy().contains("-tcp-")) {
            continue;
        }
        if let Ok(m) = session::read_messages(&dir, Side::In) {
            let n = m.messages.len();
            let known_share = m.messages.iter().filter(|x| known.contains_key(&x.opcode)).count() as f64 / n.max(1) as f64;
            if n >= MIN_GAME_MESSAGES && known_share >= MIN_KNOWN_SHARE {
                all.extend(m.messages);
            }
        }
    }
    anyhow::ensure!(!all.is_empty(), "no game connection found in {}", path.display());
    all.sort_by_key(|m| m.ts_us);
    Ok(all)
}

/// Feeds a recording at `speed` times its own pace.
fn play(state: &AppState, stop: &AtomicBool, messages: Vec<TimedMessage>, speed: f64) {
    let Some(t0) = messages.first().map(|m| m.ts_us) else { return };
    let started = Instant::now();
    let mut batch = Vec::new();
    for m in messages {
        if stop.load(Ordering::SeqCst) {
            return;
        }
        let due = Duration::from_secs_f64((m.ts_us - t0) as f64 / 1e6 / speed);
        if let Some(wait) = due.checked_sub(started.elapsed()) {
            state.add(stop, std::mem::take(&mut batch));
            thread::sleep(wait);
        }
        batch.push(m);
    }
    state.add(stop, batch);
}

fn main() {
    let state = Arc::new(AppState {
        catalog: catalog::load(),
        store: Mutex::new(Store::default()),
        source: Mutex::new(Source {
            kind: Kind::Idle,
            label: String::new(),
            message: "pick a source: live from the game, or a recording".into(),
            stop: Arc::new(AtomicBool::new(true)),
        }),
    });
    tauri::Builder::default()
        .manage(state)
        .invoke_handler(tauri::generate_handler![catalog, status, stats, rows, message, clear, stop_source, start_live, open_recording])
        .setup(|app| {
            if !std::env::args().any(|a| a == "--hidden") {
                if let Some(w) = app.get_webview_window("main") {
                    w.show()?;
                }
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("running Packet Lab");
}
