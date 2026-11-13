//! `packet-lab boss-skills`: takes the skill names of SkeeveAN's Aion DPS Meter (MIT, read
//! from the game's text tables) and writes `gamedata/boss-skills.json`: the game's own
//! names for the mob and boss skills our `skills.json` only calls "Attack", doesn't have,
//! or names differently, for the boss warnings, cast marks and death recaps.
//!
//! Differently: `skills.json`'s names are NOIA2's, and some are translations that aren't
//! the game's English (Eternal Ludra's "Ludra's Trial", as NCSOFT's own patch notes call
//! it, was "Purple Bolt"). The game's text wins. Where the game's text only says "Attack"
//! and ours has a name, ours stays.
//!
//! A file of its own, so `update-data` (which rewrites `skills.json`) never drops them.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde_json::{json, Value};

/// Player skills have 8-digit IDs; mobs' are shorter.
const PLAYER_SKILL_MIN: u32 = 10_000_000;

fn read(path: &Path) -> Result<Value> {
    let text = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    serde_json::from_str(&text).with_context(|| format!("{} isn't JSON", path.display()))
}

/// The mob skills `names` (skill ID → name) has a real name for, where `ours` (skills.json's
/// `skills`) has none or another.
fn game_names(names: &Value, ours: &Value) -> BTreeMap<u32, String> {
    fn named(n: Option<&str>) -> Option<&str> {
        n.map(str::trim).filter(|n| !n.is_empty() && *n != "Attack")
    }
    names
        .as_object()
        .into_iter()
        .flatten()
        .filter_map(|(id, name)| {
            let num: u32 = id.parse().ok()?;
            let name = named(name.as_str())?;
            (num < PLAYER_SKILL_MIN && named(ours[id]["name"].as_str()) != Some(name)).then(|| (num, name.to_string()))
        })
        .collect()
}

pub fn run(names: &Path, skills: &Path, out: &Path, commit: &str) -> Result<()> {
    let names = read(names)?;
    let ours = read(skills)?;
    let found = game_names(&names, &ours["skills"]);
    let doc = json!({
        "source": {
            "from": "SkeeveAN's Aion DPS Meter, Backend/src/data/aion2/skill_names.json (https://github.com/SkeeveAN/Aion-DPS-Meter)",
            "commit": commit,
            "licence": "MIT, Copyright (c) 2026 SkeeveTV",
            "note": "Aion 2 game data; names belong to NCSOFT. The game's own names for the mob and boss skills skills.json doesn't name, or names differently (the game's wins). Made with packet-lab boss-skills.",
        },
        "skills": found,
    });
    if let Some(dir) = out.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::write(out, serde_json::to_string_pretty(&doc)? + "\n").with_context(|| format!("writing {}", out.display()))?;
    println!("done: {} boss and mob skill names saved to {}", found.len(), out.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_games_names_for_mob_skills_we_name_otherwise_or_not_at_all() {
        let names = json!({
            "1801969": "Abyssal Gaze", "1801840": "Attack", "1218170": "Rake", "1803010": "Ludra's Trial",
            "1604426": "Lodestone Prison", "18010000": "Onslaught", "x": "y", "1000001": " ", "1000340": "Attack",
            "1802900": "Wheel of Torment"
        });
        let ours = json!({
            "1218170": { "name": "Claw Swipe" }, "1604426": { "name": "Attack" }, "1803010": { "name": "Purple Bolt" },
            "1000340": { "name": "Deadshot" }, "1802900": { "name": "Wheel of Torment" }
        });
        let got = game_names(&names, &ours);
        assert_eq!(
            got.into_iter().collect::<Vec<_>>(),
            vec![
                // Ours was another name: the game's.
                (1_218_170, "Rake".into()),
                // Ours was only "Attack".
                (1_604_426, "Lodestone Prison".into()),
                // Not in ours.
                (1_801_969, "Abyssal Gaze".into()),
                (1_803_010, "Ludra's Trial".into()),
                // Left out: the same name already, the game's "Attack" (ours stays), a
                // player skill.
            ]
        );
    }
}
