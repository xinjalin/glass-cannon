//! The attempts window: each character's tries at every boss (`store::Attempts`), grouped
//! by the dungeon or raid and mode the boss is in.

use std::collections::BTreeMap;
use std::sync::Arc;

use serde::Serialize;
use tauri::State;

use crate::gamedata::GameData;
use crate::store::{Attempts, Tally};
use crate::{lock, AppState};

/// One boss's tries.
#[derive(Debug, Serialize)]
pub struct BossAttempts {
    pub boss: u32,
    pub name: String,
    pub attempts: u32,
    #[serde(flatten)]
    pub tally: Tally,
}

/// A dungeon or raid in one mode (or the bosses no dungeon has: field bosses and others).
#[derive(Debug, Serialize)]
pub struct PlaceAttempts {
    pub name: String,
    /// Expedition, Transcendence, Sanctuary (a raid)…; None for the bosses outside them.
    pub kind: Option<String>,
    pub mode: Option<String>,
    pub attempts: u32,
    pub kills: u32,
    pub wipes: u32,
    pub fight_s: f64,
    /// The latest try at any of its bosses (ms since 1970).
    pub last_ms: u64,
    /// In the order they were first tried.
    pub bosses: Vec<BossAttempts>,
}

#[derive(Debug, Serialize)]
pub struct CharacterAttempts {
    /// "" when the character wasn't known.
    pub name: String,
    pub attempts: u32,
    pub kills: u32,
    pub wipes: u32,
    /// Latest tried first.
    pub places: Vec<PlaceAttempts>,
}

#[derive(Debug, Serialize)]
pub struct AttemptsView {
    /// The character you're playing, when known (shown first).
    pub you: Option<String>,
    /// Most tries first.
    pub characters: Vec<CharacterAttempts>,
}

/// Groups each character's bosses by where they are.
pub fn view(attempts: &Attempts, data: &GameData, you: Option<String>) -> AttemptsView {
    let mut characters: Vec<CharacterAttempts> = attempts
        .all()
        .iter()
        .map(|(name, bosses)| {
            let mut places: BTreeMap<(String, Option<String>), PlaceAttempts> = BTreeMap::new();
            for (&boss, tally) in bosses {
                let place = data.dungeon(boss);
                let key = place.as_ref().map_or((String::new(), None), |p| (p.name.to_string(), p.mode.map(str::to_string)));
                let p = places.entry(key).or_insert_with(|| PlaceAttempts {
                    name: place.as_ref().map_or_else(|| "field bosses & others".to_string(), |p| p.name.to_string()),
                    kind: place.as_ref().and_then(|p| p.kind.map(str::to_string)),
                    mode: place.as_ref().and_then(|p| p.mode.map(str::to_string)),
                    attempts: 0,
                    kills: 0,
                    wipes: 0,
                    fight_s: 0.0,
                    last_ms: 0,
                    bosses: Vec::new(),
                });
                p.attempts += tally.attempts();
                p.kills += tally.kills;
                p.wipes += tally.wipes;
                p.fight_s += tally.fight_s;
                p.last_ms = p.last_ms.max(tally.last_ms);
                p.bosses.push(BossAttempts {
                    boss,
                    name: data.npc_label(boss).unwrap_or_else(|| format!("Mob {boss}")),
                    attempts: tally.attempts(),
                    tally: tally.clone(),
                });
            }
            let mut places: Vec<PlaceAttempts> = places.into_values().collect();
            for p in &mut places {
                p.bosses.sort_by_key(|b| (b.tally.first_ms, b.boss));
            }
            places.sort_by(|a, b| b.last_ms.cmp(&a.last_ms));
            CharacterAttempts {
                name: name.clone(),
                attempts: places.iter().map(|p| p.attempts).sum(),
                kills: places.iter().map(|p| p.kills).sum(),
                wipes: places.iter().map(|p| p.wipes).sum(),
                places,
            }
        })
        .collect();
    characters.sort_by(|a, b| b.attempts.cmp(&a.attempts).then(a.name.cmp(&b.name)));
    AttemptsView { you, characters }
}

#[tauri::command]
pub fn attempts_list(app: State<'_, Arc<AppState>>) -> AttemptsView {
    let you = lock(&app.profile).name.clone();
    view(&lock(&app.attempts), &lock(&app.data), you)
}

/// Forgets one character's attempts.
#[tauri::command]
pub fn clear_attempts(app: State<'_, Arc<AppState>>, character: String) {
    lock(&app.attempts).clear(&character);
}
