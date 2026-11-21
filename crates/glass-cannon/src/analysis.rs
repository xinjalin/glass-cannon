//! The analysis window: one fight in full (every player and skill, the rotation, buffs
//! and debuffs) for the live fight or a saved one. Opened from the launcher, or by
//! right-clicking a player on the meter.

use std::collections::BTreeMap;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use meter_core::meter::Analysis;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::settings::{anchor_monitor, fully_on_screen, outer_rect, place_beside, SIDES};
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
    /// The skill this ID is part of (its row in the skills tab and its rotation lane).
    group: u32,
    /// An extra hit its skill triggered: part of that skill's cast, not a cast of its own.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    triggered: bool,
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
    /// What killed players against this boss over the session: this fight's deaths and
    /// those of the other fights against it within [`NIGHT_MS`] of it.
    killers: Killers,
    appearance: Appearance,
}

/// Fights against the same boss this close in time are one session's.
const NIGHT_MS: u64 = 12 * 3_600_000;

/// What killed players against a boss, over a session's fights.
#[derive(Debug, Default, Serialize)]
struct Killers {
    /// The fights counted (this one too), and their deaths.
    fights: usize,
    deaths: usize,
    /// Most deaths first.
    rows: Vec<KillerRow>,
}

#[derive(Debug, Serialize)]
struct KillerRow {
    /// The killing hit's skill, and what the move is called.
    skill: u32,
    name: Option<String>,
    deaths: usize,
    /// In how many of the fights it killed someone.
    fights: usize,
    /// Its deaths in the fight shown.
    here: usize,
}

/// Tallies deaths by what killed: `here` is the fight shown, `others` the session's other
/// fights against the boss.
fn killers(here: &[meter_core::meter::DeathView], others: &[&[meter_core::meter::DeathView]], name: impl Fn(u32) -> Option<String>) -> Killers {
    let mut rows: BTreeMap<u32, KillerRow> = BTreeMap::new();
    for (i, deaths) in std::iter::once(here).chain(others.iter().copied()).enumerate() {
        let mut seen = std::collections::BTreeSet::new();
        for d in deaths {
            let row = rows.entry(d.skill).or_insert_with(|| KillerRow { skill: d.skill, name: name(d.skill), deaths: 0, fights: 0, here: 0 });
            row.deaths += 1;
            row.here += usize::from(i == 0);
            row.fights += usize::from(seen.insert(d.skill));
        }
    }
    let mut rows: Vec<KillerRow> = rows.into_values().collect();
    rows.sort_by(|a, b| b.deaths.cmp(&a.deaths).then(b.here.cmp(&a.here)).then(a.skill.cmp(&b.skill)));
    Killers { fights: 1 + others.len(), deaths: rows.iter().map(|r| r.deaths).sum(), rows }
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

/// The first time: where you left it last time, else beside the launcher (in game, in the
/// middle of the game's screen) at the default size. Later it stays where you put it, at
/// the size you gave it, unless it isn't fully on a screen any more (a monitor was
/// unplugged): then it's placed again, keeping a size bigger than the default.
fn place(handle: &AppHandle, w: &tauri::WebviewWindow, first: bool) {
    if first {
        let saved = lock(&handle.state::<Arc<AppState>>().placements).get(ANALYSIS);
        if saved.is_some_and(|p| crate::placement::restore(w, p, true)) {
            return;
        }
    }
    let (Some(mon), Some(now)) = (anchor_monitor(handle), outer_rect(w)) else {
        return;
    };
    if !first && fully_on_screen(w) {
        return;
    }
    let scale = mon.scale_factor();
    let default = ((WIDTH * scale) as i32, (HEIGHT * scale) as i32);
    let size = if first { default } else { (now.w.max(default.0), now.h.max(default.1)) };
    place_beside(handle, w, size, MIN, &SIDES);
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
    // Which of the casts before each death the overlay warns of.
    let marks = crate::cast_marks(&app, &data);
    for c in a.view.deaths.iter_mut().flat_map(|d| d.casts.iter_mut()) {
        c.warned = crate::dangerous(&marks, c.skill, c.cast_ms, c.lands_ms);
    }
    // What killed against this boss over the session (the history's other fights against
    // it; none for a run, whose fights are all in this view already).
    let killers = {
        let history = lock(&app.history);
        let at = ended_ms.unwrap_or_else(crate::hotkeys::now_ms);
        let others: Vec<&[meter_core::meter::DeathView]> = match a.view.target_type.filter(|_| run_info.is_none()) {
            Some(boss) => history
                .newest_first()
                .filter(|f| f.view.target_type == Some(boss) && Some(f.ended_ms) != ended_ms && f.ended_ms.abs_diff(at) <= NIGHT_MS)
                .map(|f| f.view.deaths.as_slice())
                .collect(),
            None => Vec::new(),
        };
        killers(&a.view.deaths, &others, |skill| data.move_name(skill))
    };
    let mut bosses = Vec::new();
    for (id, type_id) in &a.mob_types {
        if let Some((name, boss)) = data.npc(*type_id) {
            a.names.insert(*id, name);
            if boss {
                bosses.push(*id);
            }
        }
    }
    // Mobs and NPCs with no name show as "Unknown", not a number; players not named yet
    // keep their "#ID", so they stay apart.
    let players: std::collections::HashSet<u64> =
        a.view.players.iter().chain(&a.view.healing).chain(&a.view.taken).map(|p| p.id).collect();
    for (id, name) in a.names.iter_mut() {
        if !players.contains(id) && crate::gamedata::is_stand_in(name) {
            *name = crate::gamedata::UNNAMED.to_string();
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
    let groups = data.skill_groups();
    // Each skill's own ID too, for its row's name and icon.
    let ids: Vec<u32> = ids.collect();
    let ids = ids.iter().copied().chain(ids.iter().map(|id| groups.group(*id)));
    let mut skills = BTreeMap::new();
    for id in ids {
        skills.entry(id).or_insert_with(|| {
            let (name, icon_key) = data.skill_meta(id);
            // A mob's skill the data has no name for: after the mob that used it.
            let name = name.or_else(|| {
                let by = a.taken.iter().find(|h| h.skill == id)?.player;
                let (mob, _) = data.npc(*a.mob_types.get(&by)?)?;
                Some(format!("{mob}'s attack"))
            });
            SkillMeta { name, icon_key, group: groups.group(id), triggered: groups.triggered(id) }
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
        killers,
        appearance,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use meter_core::meter::DeathView;

    fn death(skill: u32) -> DeathView {
        let json = format!(
            r#"{{"entity":1,"name":"x","at_s":1.0,"killer":null,"killer_type":null,"skill":{skill},"skill_name":null,"time_to_death_s":1.0,"direct":1,"over_time":0,"healed":0,"hits":[],"last_heal":null}}"#
        );
        serde_json::from_str(&json).unwrap()
    }

    #[test]
    fn what_killed_over_a_session() {
        // This fight: two deaths to 7, one to 9. Two more fights: 7 once, 5 twice in one.
        let here = [death(7), death(9), death(7)];
        let (a, b) = ([death(7)], [death(5), death(5)]);
        let k = killers(&here, &[&a, &b], |skill| (skill == 7).then(|| "seven".to_string()));
        assert_eq!((k.fights, k.deaths), (3, 6));
        let rows: Vec<_> = k.rows.iter().map(|r| (r.skill, r.name.as_deref(), r.deaths, r.fights, r.here)).collect();
        assert_eq!(rows, [(7, Some("seven"), 3, 2, 2), (5, None, 2, 1, 0), (9, None, 1, 1, 1)]);
        // On its own: this fight's.
        let k = killers(&here, &[], |_| None);
        assert_eq!((k.fights, k.deaths, k.rows.len()), (1, 3, 2));
    }
}
