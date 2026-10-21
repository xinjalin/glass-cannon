//! `packet-lab theostones`: takes the Theostones from a scraped item database (one JSON
//! object per item, with its in-game description) and writes `gamedata/theostones.json`,
//! what the analysis window's tooltips say about each Theostone the meter can name.
//!
//! Only Theostones already named in `gamedata/skills.json` are kept, matched by name (the
//! game sends a Theostone's effect as a 7-digit skill ID; the item database doesn't). Kept
//! per Theostone: its name, grade, effect and the effect's cooldown. The lines every
//! Theostone shares ("Grants …", "can be found in the Enhancement menu", "Only one …") are
//! dropped.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::Path;

use anyhow::{bail, Context, Result};
use serde_json::{json, Map, Value};

pub fn run(items: &Path, skills: &Path, out: &Path, scraped: &str) -> Result<()> {
    let all = read_object(items)?;
    // Our Theostone names, by the skill IDs the game sends (the data key less its last
    // digit). One Theostone can have more than one ID (Santras's Secret: 3000015 and 3001015).
    let ours = read_object(skills)?;
    let ours = ours.get("skills").and_then(Value::as_object).context("skills.json has no skills")?;
    let mut by_name: HashMap<&str, Vec<u32>> = HashMap::new();
    for (key, s) in ours {
        let (Ok(key), Some(name)) = (key.parse::<u32>(), s["name"].as_str()) else { continue };
        if (30_000_000..31_000_000).contains(&key) && name.starts_with("Theostone:") {
            by_name.entry(name).or_default().push(key / 10);
        }
    }
    let mut stones: BTreeMap<u32, Value> = BTreeMap::new();
    let mut found = 0;
    for item in all.values() {
        let Some(name) = item["name"].as_str() else { continue };
        let name = name.trim_end_matches(" (Bound)");
        let Some(skills) = by_name.remove(name) else { continue };
        if let Some(entry) = theostone(name, item) {
            found += 1;
            for skill in skills {
                stones.insert(skill, entry.clone());
            }
        }
    }
    let count = stones.len();
    let doc = json!({
        "source": {
            "from": "a scraped Aion 2 item database (collected from public fan databases; the bundle names no source site)",
            "scraped": scraped,
            "note": "Aion 2 game text and numbers belong to NCSOFT. Not covered by Glass Cannon's MIT licence. Made with packet-lab theostones.",
        },
        "theostones": stones,
    });
    if let Some(dir) = out.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::write(out, serde_json::to_string_pretty(&doc)? + "\n").with_context(|| format!("writing {}", out.display()))?;
    let missing = by_name.len();
    println!("done: {found} Theostones ({count} skill IDs) saved to {}; {missing} named ones aren't in the items", out.display());
    Ok(())
}

fn read_object(path: &Path) -> Result<Map<String, Value>> {
    let bytes = fs::read(path).with_context(|| format!("can't read {}", path.display()))?;
    match serde_json::from_slice(&bytes)? {
        Value::Object(m) => Ok(m),
        _ => bail!("{} isn't a JSON object", path.display()),
    }
}

/// One Theostone's tooltip data, or `None` when its description says nothing more than the
/// lines every Theostone has.
fn theostone(name: &str, item: &Value) -> Option<Value> {
    let desc = item["description"].as_str()?;
    let mut effect = Vec::new();
    let mut cooldown = None;
    for line in desc.lines().map(str::trim).filter(|l| !l.is_empty()) {
        if let Some(s) = line.strip_prefix("Effect Activation Cooldown:") {
            cooldown = s.trim().trim_end_matches('s').parse::<f64>().ok();
        } else if !(line.starts_with("Grants [")
            || line.starts_with("The obtained Theostone")
            || line.starts_with("Only one Theostone"))
        {
            effect.push(line);
        }
    }
    if effect.is_empty() {
        return None;
    }
    let mut e = Map::new();
    e.insert("name".into(), json!(name));
    if let Some(grade) = item["grade"].as_str() {
        e.insert("grade".into(), json!(grade));
    }
    e.insert("effect".into(), json!(effect.join("\n")));
    if let Some(s) = cooldown {
        e.insert("cooldown_s".into(), json!(s));
    }
    Some(Value::Object(e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_the_effect_and_its_cooldown() {
        let item = json!({
            "name": "Theostone: Fregion's Schemes (Bound)",
            "grade": "Unique",
            "description": "Grants [Theostone: Fregion's Schemes].\nThe obtained Theostone can be found in the Enhancement menu.\nHas a 3% chance to inflict [Blind] for 7s on attack\nEffect Activation Cooldown: 5s\nOnly one Theostone's effect applies when identical Theostones are engraved.",
        });
        let e = theostone("Theostone: Fregion's Schemes", &item).unwrap();
        assert_eq!(e["effect"], "Has a 3% chance to inflict [Blind] for 7s on attack");
        assert_eq!(e["cooldown_s"], 5.0);
        assert_eq!(e["grade"], "Unique");
        let empty = json!({ "description": "Grants [Theostone: X].\nEffect Activation Cooldown: 5s" });
        assert!(theostone("Theostone: X", &empty).is_none());
    }
}
