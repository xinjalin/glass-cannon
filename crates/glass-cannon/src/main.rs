//! Glass Cannon: a DPS meter for Aion 2, in an overlay over the game.
//!
//! The app opens to a small launcher and starts capturing straight away. A background
//! engine thread captures the game's traffic (or replays a recording) and feeds the meter;
//! the overlay (one window over the game's picture, holding the meter and the other
//! widgets) polls `snapshot` to draw it. Finished fights go to a local history. The
//! launcher starts and stops the overlay and opens the analysis, history and settings
//! windows. No admin rights and no network access: the game data ships in the release.
//!
//! Options:
//!   --replay <session folder>   play back a recorded capture
//!   --speed <n>                 replay speed multiplier (default 1)
//!   --process <name>            game executable name to look for (default AION2)
//!   --hidden                    start without showing the launcher (for scripted checks)

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State, WindowEvent};

use meter_core::live::{self, LiveEvent, LiveOptions};
use meter_core::meter::{EncounterView, Meter, Scope, Snapshot, DEFAULT_IDLE_TIMEOUT_US};
use meter_core::recorder::Side;
use meter_core::session;

mod analysis;
mod dummy;
mod fonts;
mod game_window;
mod gamedata;
mod hotkeys;
mod hud;
mod launcher;
mod layout;
mod ping;
mod placement;
mod runs;
mod settings;
mod store;
mod style;
mod widgets;

use gamedata::GameData;
use store::{Appearance, Fight, FightDetails, History, Instances, KnownPlayers, Profile, Settings};

const RETRY_AFTER: Duration = Duration::from_secs(5);
const LAUNCHER: &str = "launcher";
const SETTINGS: &str = "settings";
const ANALYSIS: &str = "analysis";
const HISTORY: &str = "history";
const PET_MAP: &str = "petmap";

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
    /// Whether the running capture is saving packets (settings changes apply next start).
    recording_now: AtomicBool,
    /// The overlay's widgets' boxes, and whether they're being arranged.
    hud_rects: Mutex<Vec<hud::HitRect>>,
    hud_panels: Mutex<Vec<hud::HitRect>>,
    hud_editing: AtomicBool,
    /// A hotkey is being picked in the settings: none go off meanwhile.
    hotkeys_paused: AtomicBool,
    /// Where each hotkey stands (registered, taken by another app…), for the settings.
    hotkey_states: Mutex<std::collections::BTreeMap<&'static str, &'static str>>,
    /// The overlay's widgets are hidden for now by the hotkey (not saved).
    hud_hidden: AtomicBool,
    /// Why the widgets are stepping aside now: "", "map" or "hotkey".
    step_aside: Mutex<&'static str>,
    /// The game's window is up, and the overlay is over it (kept by the HUD's watcher).
    game_found: AtomicBool,
    hud_shown: AtomicBool,
    /// Player names saved by an earlier run, until we know whether they're from this login.
    known_players: Mutex<Option<KnownPlayers>>,
    /// How many named players were last saved to `players.json`.
    players_saved: AtomicUsize,
    /// PvP sessions.
    pvp: Mutex<store::PvpLog>,
    /// Field bosses from the world map.
    field_bosses: Mutex<store::FieldBosses>,
    /// Your best against each boss.
    bests: Mutex<store::Bests>,
    pets: Mutex<store::PetBook>,
    /// Content tickets and Abyss artifacts, as last sent.
    contents: Mutex<store::KnownContents>,
    /// The history window has been placed once.
    history_placed: AtomicBool,
    pet_map_placed: AtomicBool,
    /// Where every window was last (saved as they move, at most once a second).
    placements: Mutex<placement::Placements>,
    /// Your training dummy runs, and dummy mode's state on the meter.
    dummy: Mutex<dummy::DummyBook>,
    dummy_tracker: Mutex<dummy::DummyTracker>,
    /// The meter showed a fight on a dummy last time: it counts your damage only.
    dummy_active: AtomicBool,
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
    meter_view: store::MeterView,
    /// The game's own ping (live capture only).
    ping_ms: Option<u32>,
    /// For the alerts the overlay shows before events.
    timer_region: String,
    alerts: Vec<String>,
    alert_lead_min: u32,
    hide_after_s: u32,
    boss_hp: bool,
    boss_casts: bool,
    /// Your best against the boss being fought.
    best: Option<store::Best>,
    /// What the boss cast next the last time you fought it, from where this fight is.
    boss_next: Option<NextCast>,
    /// The boss's next casts and whether its cast now is dangerous (the overlay's boss
    /// alerts), and whether they sound.
    boss_ahead: BossAhead,
    alert_sound: bool,
    /// Hidden cubes in view.
    cubes: Vec<meter_core::cubes::CubeView>,
    /// Pet souls gained since the last snapshot, with each pet's progress.
    souls: Vec<meter_core::pets::SoulGain>,
    /// Dummy mode, while the fight shown is on a training dummy.
    dummy: Option<dummy::DummyLive>,
    /// World PvP: the PvP session going on (fights with players outside your party).
    pvp: Option<meter_core::pvp::PvpSession>,
}

/// A boss's next cast, as it came the last time you fought it.
#[derive(Serialize)]
struct NextCast {
    skill: u32,
    skill_name: Option<String>,
    cast_ms: u32,
    /// How long after it went off its marked areas landed (0: none).
    lands_ms: u32,
    /// Worth a warning: see [`dangerous`].
    danger: bool,
    /// Seconds from now.
    in_s: f64,
}

/// The boss's casts ahead, and whether the one it's casting now is dangerous.
#[derive(Serialize, Default)]
struct BossAhead {
    /// The live cast's: a warning, not just a bar.
    cast_danger: bool,
    /// Its next casts that matter (at most [`UPCOMING`], within [`UPCOMING_S`]).
    upcoming: Vec<NextCast>,
}

/// Bosses cast these this often or more in a fight as routine; they're not worth a warning.
const ROUTINE_CASTS: usize = 8;
const UPCOMING: usize = 5;
const UPCOMING_S: f64 = 60.0;
/// A cast this long (or one that marks the ground) is dangerous, unless you say otherwise.
/// 2.5 s: at 2 s, bosses' short casts of 2.0-2.2 s chimed every few seconds in a fight.
/// (analysis.js's `looksDangerous` says the same for the [boss] tab.)
const DANGER_CAST_MS: u32 = 2500;

/// Whether a boss skill gets a warning: as you marked it (the analysis window's [boss]
/// tab), else when it marks the ground or takes [`DANGER_CAST_MS`] or more to cast.
fn dangerous(marks: &std::collections::BTreeMap<u32, bool>, skill: u32, cast_ms: u32, lands_ms: u32) -> bool {
    marks.get(&skill).copied().unwrap_or(lands_ms > 0 || cast_ms >= DANGER_CAST_MS)
}

/// The boss casts the meter shows: not routine (cast [`ROUTINE_CASTS`] times or more in
/// the last fight against it, or in this one so far). Drops a routine live cast, and gives
/// what it cast next the last time (with a cast time or landing areas: a mechanic), from
/// where this fight is.
fn boss_casts_to_show(
    history: &store::History,
    data: &GameData,
    marks: &std::collections::BTreeMap<u32, bool>,
    enc: &mut meter_core::meter::EncounterView,
) -> BossAhead {
    let kind = enc.boss_casts.iter().find_map(|c| c.boss_type).or(enc.target_type);
    let last = kind.and_then(|kind| {
        history.newest_first().find(|f| f.view.start_us != enc.start_us && f.view.boss_casts.iter().any(|c| c.boss_type == Some(kind)))
    });
    let casts = last.map_or(&[][..], |f| &f.view.boss_casts[..]);
    let tally = |list: &[meter_core::meter::BossCastView]| {
        let mut n: HashMap<u32, usize> = HashMap::new();
        for c in list {
            *n.entry(c.skill).or_default() += 1;
        }
        n
    };
    let (before, now_counts) = (tally(casts), tally(&enc.boss_casts));
    let routine = |skill: u32| [&before, &now_counts].iter().any(|n| n.get(&skill).copied().unwrap_or(0) >= ROUTINE_CASTS);
    // Its landing areas last time, for a cast that hasn't gone off yet.
    let lands_before = |skill: u32| casts.iter().filter(|c| c.skill == skill).map(|c| c.lands_ms).max().unwrap_or(0);
    if enc.boss_cast.as_ref().is_some_and(|c| routine(c.skill) && marks.get(&c.skill) != Some(&true)) {
        enc.boss_cast = None;
    }
    let cast_danger = enc
        .boss_cast
        .as_ref()
        .is_some_and(|c| dangerous(marks, c.skill, c.cast_ms, c.lands_ms.max(lands_before(c.skill))));
    let Some(now) = enc.boss_now_s else {
        return BossAhead { cast_danger, upcoming: Vec::new() };
    };
    let upcoming = casts
        .iter()
        .filter(|c| c.at_s > now && c.at_s - now <= UPCOMING_S)
        .filter(|c| marks.get(&c.skill) == Some(&true) || (!routine(c.skill) && (c.cast_ms > 0 || c.lands_ms > 0)))
        .take(UPCOMING)
        .map(|c| NextCast {
            skill: c.skill,
            skill_name: data.boss_skill_name(c.skill),
            cast_ms: c.cast_ms,
            lands_ms: c.lands_ms,
            danger: dangerous(marks, c.skill, c.cast_ms, c.lands_ms),
            in_s: c.at_s - now,
        })
        .collect();
    BossAhead { cast_danger, upcoming }
}

/// The current meter with names filled in. Also moves finished fights into the history.
/// Skill lists come only for the players in `expanded` (the rows open in the overlay):
/// the rest would only be thrown away, several times a second.
#[tauri::command]
fn snapshot(app: State<'_, Arc<AppState>>, expanded: Option<Vec<u64>>) -> View {
    let now = app.engine.now();
    let auto_scope = !lock(&app.settings).manual_scope;
    let (mut snap, finished, known, entered, ping_ms, pvp, field_bosses, contents, cubes, souls) = {
        let mut meter = app.engine.meter();
        meter.tick(now);
        let known = remember_players(&app, &mut meter);
        // Whose damage counts follows your party, unless chosen by hand: the party now, or
        // the one in the fight shown (it still shows them after the party breaks up).
        // A fight on a training dummy counts your damage only (dummy mode).
        let mut scope = lock(&app.settings).scope_for(party_size(&meter));
        if auto_scope && app.dummy_active.load(Ordering::SeqCst) {
            scope = meter_core::meter::Scope::Mine;
        }
        if meter.scope() != scope {
            meter.set_scope(scope);
        }
        let ping = if app.args.replay.is_none() { ping::game_ping_ms(&meter) } else { None };
        let contents = meter.take_contents();
        let cubes = meter.cubes(now);
        // Your pets as last saved, until the game sends its list (and saved as they change).
        let name = meter.registry().self_name().map(str::to_string).or_else(|| lock(&app.profile).name.clone());
        if let Some(name) = name {
            lock(&app.pets).sync(&name, meter.pets());
        }
        let souls = meter.take_soul_gains();
        let pvp_now = meter.pvp();
        (meter.snapshot(), meter.take_finished(), known, meter.take_instances(), ping, meter.take_pvp(), meter.take_field_bosses(), contents, cubes, (souls, pvp_now))
    };
    let (mut souls, pvp_now) = souls;
    lock(&app.field_bosses).update(field_bosses);
    lock(&app.contents).update(contents);
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
            let s = lock(&app.settings);
            if s.dummy_mode && view.target_type.is_some_and(|t| data.is_dummy(t)) {
                if let Some(run) = dummy::DummyRun::from_fight(analysis, s.dummy_run_s, ended_ms, self_name.clone(), &s.dummy_label) {
                    lock(&app.dummy).add(run);
                }
            }
        }
        lock(&app.bests).record(&view, ended_ms, self_name.as_deref());
        history.push(Fight { ended_ms, self_name, view });
    }
    data.annotate(&mut snap);
    // Your best against the boss you're fighting (a boss once named above).
    let best = snap
        .encounter
        .as_ref()
        .filter(|e| e.target_boss)
        .and_then(|e| e.target_type)
        .and_then(|t| lock(&app.bests).get(profile.name.as_deref(), t));
    if !lock(&app.settings).pet_souls {
        souls.clear();
    }
    for g in &mut souls {
        g.name = data.pet_name(g.pet);
    }
    let mut boss_next = None;
    let mut boss_ahead = BossAhead::default();
    if let Some(enc) = snap.encounter.as_mut().filter(|e| e.active && e.target_boss) {
        let marks = lock(&app.settings).boss_skill_alerts.clone();
        boss_ahead = boss_casts_to_show(&history, &data, &marks, enc);
        boss_next = boss_ahead.upcoming.first().map(|c| NextCast {
            skill: c.skill,
            skill_name: c.skill_name.clone(),
            cast_ms: c.cast_ms,
            lands_ms: c.lands_ms,
            danger: c.danger,
            in_s: c.in_s,
        });
    }
    let name_remembered = profile.apply(&mut snap);
    let settings = lock(&app.settings).clone();
    let dummy = snap
        .encounter
        .as_ref()
        .filter(|e| settings.dummy_mode && e.target_type.is_some_and(|t| data.is_dummy(t)))
        .and_then(|e| {
            let book = lock(&app.dummy);
            lock(&app.dummy_tracker).view(e, now, settings.dummy_run_s, &book, profile.name.as_deref(), &settings.dummy_label)
        });
    app.dummy_active.store(dummy.is_some() && snap.encounter.as_ref().is_some_and(|e| e.active), Ordering::SeqCst);
    // The target frame shows the boss's HP, enrage and cast (and your best): the meter
    // leaves them out while it's on.
    let target_frame = settings.widgets.iter().any(|w| w == "target");
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
        meter_view: settings.meter_view,
        ping_ms,
        timer_region: settings.timer_region,
        alerts: settings.alerts,
        alert_lead_min: settings.alert_lead_min,
        hide_after_s: settings.hide_after_s,
        boss_hp: settings.boss_hp && !target_frame,
        boss_casts: settings.boss_casts && !target_frame,
        best,
        boss_next,
        boss_ahead,
        alert_sound: settings.alert_sound,
        cubes,
        souls,
        dummy,
        pvp: pvp_now,
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
fn pet_icon(app: State<'_, Arc<AppState>>, pet: u32) -> Option<String> {
    lock(&app.data).pet_icon(pet)
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
/// fight shown, whichever is larger (in a Force, at least a party's).
fn party_size(meter: &Meter) -> usize {
    let force = if meter.registry().in_force() { 2 } else { 0 };
    meter.registry().party().len().max(meter.shown_party_size()).max(force)
}

#[tauri::command]
fn history_list(app: State<'_, Arc<AppState>>) -> runs::HistoryList {
    let history = lock(&app.history);
    let mut fights = history.summaries(&app.details);
    let bests = lock(&app.bests);
    for f in &mut fights {
        let character = history.fight(f.index).and_then(|x| x.self_name.as_deref());
        if let Some(b) = f.target_type.and_then(|t| bests.get(character, t)) {
            f.best_dps = b.dps_ms == f.ended_ms;
            f.fastest = b.kill_ms == f.ended_ms;
        }
    }
    drop(bests);
    drop(history);
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
    lock(&app.bests).clear();
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
    hidden: bool,
}

fn parse_args() -> Args {
    let mut args = Args {
        replay: None,
        speed: 1.0,
        process: "AION2".into(),
        hidden: false,
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
            "--hidden" => args.hidden = true,
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
    let placements = placement::Placements::load(&config_dir);
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
        recording_now: AtomicBool::new(false),
        hud_rects: Mutex::new(Vec::new()),
        hud_panels: Mutex::new(Vec::new()),
        hud_editing: AtomicBool::new(false),
        hotkeys_paused: AtomicBool::new(false),
        hotkey_states: Mutex::new(std::collections::BTreeMap::new()),
        hud_hidden: AtomicBool::new(false),
        step_aside: Mutex::new(""),
        game_found: AtomicBool::new(false),
        hud_shown: AtomicBool::new(false),
        known_players: Mutex::new(Some(known_players)),
        players_saved: AtomicUsize::new(0),
        pvp: Mutex::new(store::PvpLog::load(&config_dir_for_pvp)),
        field_bosses: Mutex::new(store::FieldBosses::load(&config_dir_for_pvp)),
        contents: Mutex::new(store::KnownContents::load(&config_dir_for_pvp)),
        bests: Mutex::new(store::Bests::load(&config_dir_for_pvp)),
        pets: Mutex::new(store::PetBook::load(&config_dir_for_pvp)),
        history_placed: AtomicBool::new(false),
        pet_map_placed: AtomicBool::new(false),
        placements: Mutex::new(placements),
        dummy: Mutex::new(dummy::DummyBook::load(&config_dir_for_pvp)),
        dummy_tracker: Mutex::new(dummy::DummyTracker::default()),
        dummy_active: AtomicBool::new(false),
    });

    tauri::Builder::default()
        .manage(Arc::clone(&state))
        .invoke_handler(tauri::generate_handler![
            snapshot,
            reset,
            end_fight,
            set_meter_view,
            set_refresh,
            set_scope,
            skill_icon,
            class_icon,
            pet_icon,
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
            settings::create_shortcuts,
            settings::open_history,
            settings::toggle_settings,
            settings::hide_settings,
            widgets::toggle_widget,
            widgets::widgets_shown,
            widgets::show_fight,
            widgets::cooldowns,
            widgets::pvp_list,
            widgets::timer_region,
            widgets::set_timer_region,
            widgets::target,
            widgets::field_bosses,
            widgets::contents,
            widgets::rename_ticket,
            widgets::rename_field_boss,
            widgets::set_alert,
            widgets::set_alert_lead,
            widgets::set_hide_after,
            widgets::set_boss_hp,
            widgets::set_boss_casts,
            widgets::set_pet_souls,
            widgets::dummy_runs,
            widgets::pet_tracker,
            widgets::set_pet_tracked,
            widgets::set_pet_tracker_nearest,
            widgets::compare_dummy_runs,
            widgets::label_dummy_run,
            widgets::delete_dummy_run,
            widgets::set_dummy_mode,
            widgets::set_dummy_run_length,
            widgets::set_dummy_label,
            widgets::set_boss_skill_alert,
            widgets::boss_skill_alerts,
            widgets::set_alert_sound,
            widgets::set_hide_on_world_map,
            widgets::set_close_with_game,
            widgets::pet_map,
            widgets::pet_map_me,
            widgets::pet_map_image,
            settings::open_pet_map,
            settings::fit_pet_map,
            settings::reset_settings,
            settings::set_hotkey,
            settings::pause_hotkeys,
            widgets::self_bars,
            widgets::party_frames,
            widgets::party_gear,
            widgets::timer_settings,
            widgets::set_timer_favourite,
            widgets::set_timer_favourites_only,
            widgets::party_cooldowns,
            launcher::launcher_state,
            launcher::set_launcher_on_top,
            hud::hud_rects,
            hud::hud_edit,
            hud::set_overlay,
            hud::hud_layout,
            hud::set_hud_layout,
        ])
        .setup(move |app| {
            restyle_windows(app.handle(), &lock(&state.settings).appearance.clone());
            // Capturing starts straight away, with the launcher.
            start_engine(&state);
            if !state.args.hidden {
                launcher::show(app.handle())?;
            }
            // The overlay never takes the focus from the game (but while being arranged).
            if let Some(h) = app.get_webview_window(hud::HUD) {
                let _ = h.set_focusable(false);
            }
            hud::watch(app.handle().clone(), Arc::clone(&state));
            hotkeys::start(app.handle().clone(), Arc::clone(&state));
            hotkeys::close_with_game(app.handle().clone(), Arc::clone(&state));
            notify_changes(app.handle().clone(), Arc::clone(&state));
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
                    // Closing the launcher closes the app.
                    LAUNCHER => {
                        let mut placements = lock(&app.placements);
                        if let Some(p) = placement::current(window) {
                            placements.set(LAUNCHER, p);
                        }
                        placements.save_if_changed();
                        drop(placements);
                        window.app_handle().exit(0);
                    }
                    // Closing any other window only tucks it away.
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
    // The launcher keeps its own look (styled once, at start).
    for label in [SETTINGS, ANALYSIS, HISTORY, PET_MAP] {
        if Some(label) == skip {
            continue;
        }
        if let Some(w) = handle.get_webview_window(label) {
            style::apply(&w, appearance);
        }
    }
    let _ = handle.emit("appearance", appearance);
}

/// The saved appearance, for a page as it loads.
#[tauri::command]
fn appearance(app: State<'_, Arc<AppState>>) -> Appearance {
    lock(&app.settings).appearance.clone()
}

/// With the update rate on "as it happens", tells the meter whenever new game messages
/// arrive (at most every [`store::REALTIME`] ms), instead of it asking on a timer.
fn notify_changes(handle: AppHandle, app: Arc<AppState>) {
    thread::Builder::new()
        .name("notify".into())
        .spawn(move || loop {
            if !app.engine.wait_changed(Duration::from_secs(1)) {
                continue;
            }
            if lock(&app.settings).refresh_ms == store::REALTIME {
                let _ = handle.emit_to(hud::HUD, "meter-changed", ());
                thread::sleep(Duration::from_millis(store::REALTIME.into()));
            }
        })
        .expect("starting the notify thread");
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


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_boss_cast_warns_when_long_or_marking_the_ground_unless_marked() {
        let none = std::collections::BTreeMap::new();
        // Short casts (the 2.0-2.2 s ones that chimed all fight) don't; 2.5 s and more do.
        assert!(!dangerous(&none, 1, 2000, 0));
        assert!(!dangerous(&none, 1, 2167, 0));
        assert!(dangerous(&none, 1, 2500, 0));
        assert!(dangerous(&none, 1, 4500, 0));
        // Marking the ground does, however short.
        assert!(dangerous(&none, 1, 700, 2150));
        // What you marked in the [boss] tab wins.
        let marks = std::collections::BTreeMap::from([(1, true), (2, false)]);
        assert!(dangerous(&marks, 1, 500, 0));
        assert!(!dangerous(&marks, 2, 4500, 1000));
    }
}
