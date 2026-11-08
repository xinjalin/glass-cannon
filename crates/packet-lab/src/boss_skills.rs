//! `packet-lab boss-skills`: takes the skill names of SkeeveAN's Aion DPS Meter (MIT, read
//! from the game's text tables) and writes `gamedata/boss-skills.json`: names for the mob
//! and boss skills our `skills.json` only calls "Attack" or doesn't have, for the boss cast
//! marks and death recaps.
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

/// The mob skills `names` (skill ID → name) has a real name for and `ours` (skills.json's
/// `skills`) doesn't.
fn missing(names: &Value, ours: &Value) -> BTreeMap<u32, String> {
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
            (num < PLAYER_SKILL_MIN && named(ours[id]["name"].as_str()).is_none()).then(|| (num, name.to_string()))
        })
        .collect()
}

pub fn run(names: &Path, skills: &Path, out: &Path, commit: &str) -> Result<()> {
    let names = read(names)?;
    let ours = read(skills)?;
    let found = missing(&names, &ours["skills"]);
    let doc = json!({
        "source": {
            "from": "SkeeveAN's Aion DPS Meter, Backend/src/data/aion2/skill_names.json (https://github.com/SkeeveAN/Aion-DPS-Meter)",
            "commit": commit,
            "licence": "MIT, Copyright (c) 2026 SkeeveTV",
            "note": "Aion 2 game data; names belong to NCSOFT. Only mob and boss skills skills.json doesn't name. Made with packet-lab boss-skills.",
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
    fn only_mob_skills_we_dont_name() {
        let names = json!({
            "1801969": "Abyssal Gaze", "1801840": "Attack", "1218170": "Rake",
            "1604426": "Lodestone Prison", "18010000": "Onslaught", "x": "y", "1000001": " "
        });
        let ours = json!({ "1218170": { "name": "Claw Swipe" }, "1604426": { "name": "Attack" } });
        let got = missing(&names, &ours);
        assert_eq!(got.into_iter().collect::<Vec<_>>(), vec![(1_604_426, "Lodestone Prison".into()), (1_801_969, "Abyssal Gaze".into())]);
    }
}
