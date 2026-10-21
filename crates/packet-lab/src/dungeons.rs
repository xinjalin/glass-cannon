//! `packet-lab dungeons`: takes the dungeon mobs from a scraped monster database and writes
//! `gamedata/dungeons.json`, which the meter uses to group the history into dungeon runs.
//!
//! Kept: each dungeon's name and type (Expedition, Transcendence…), and for each mob type
//! that belongs to one, which dungeon and which mode (Exploration, Conquest, Stage 2…). A
//! dungeon's mobs have one ID per mode (Atiel: 2300601 in Exploration, 2340601 in Stage 2),
//! so the mob a fight was on says where it was.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use anyhow::{bail, Context, Result};
use serde_json::{json, Map, Value};

pub fn run(monsters: &Path, dungeons: &Path, out: &Path, scraped: &str) -> Result<()> {
    let monsters = read_object(monsters)?;
    let dungeons = read_object(dungeons)?;
    let mut mobs: BTreeMap<u32, Value> = BTreeMap::new();
    let mut used: BTreeMap<String, Value> = BTreeMap::new();
    for (id, m) in &monsters {
        let Ok(id) = id.parse::<u32>() else { continue };
        let Some(dungeon) = m["references"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|r| r["category"] == "dungeons")
            .and_then(|r| r["id"].as_str())
        else {
            continue;
        };
        let Some(d) = dungeons.get(dungeon) else { continue };
        let mut entry = json!({ "dungeon": dungeon });
        if let Some(mode) = m["details"]["Dungeon"].as_str().and_then(mode) {
            entry["mode"] = json!(mode);
        }
        mobs.insert(id, entry);
        used.entry(dungeon.to_string()).or_insert_with(|| {
            let mut e = Map::new();
            e.insert("name".into(), d["name"].clone());
            if let Some(kind) = d["type"].as_str() {
                e.insert("type".into(), json!(kind));
            }
            Value::Object(e)
        });
    }
    let doc = json!({
        "source": {
            "from": "a scraped Aion 2 monster database (collected from public fan databases; the bundle names no source site)",
            "scraped": scraped,
            "note": "Aion 2 game data; names belong to NCSOFT. Not covered by Glass Cannon's MIT licence. Made with packet-lab dungeons.",
        },
        "dungeons": used,
        "mobs": mobs,
    });
    if let Some(dir) = out.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::write(out, serde_json::to_string_pretty(&doc)? + "\n").with_context(|| format!("writing {}", out.display()))?;
    println!("done: {} dungeons, {} mob types saved to {}", doc["dungeons"].as_object().map_or(0, Map::len), mobs.len(), out.display());
    Ok(())
}

fn read_object(path: &Path) -> Result<Map<String, Value>> {
    let bytes = fs::read(path).with_context(|| format!("can't read {}", path.display()))?;
    match serde_json::from_slice(&bytes)? {
        Value::Object(m) => Ok(m),
        _ => bail!("{} isn't a JSON object", path.display()),
    }
}

/// The mode from a mob's dungeon line, which reads like
/// "$L13 Deus Research Base span ·  stage2": "Stage 2" (or "Conquest [Hard]", or
/// "Conquest · Phase 1" when there are more parts).
fn mode(line: &str) -> Option<String> {
    let parts: Vec<String> = line
        .split("span ·")
        .skip(1)
        .map(|p| p.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|p| !p.is_empty())
        .map(|p| match p.strip_prefix("stage") {
            Some(n) if n.chars().all(|c| c.is_ascii_digit()) && !n.is_empty() => format!("Stage {n}"),
            _ => p,
        })
        .collect();
    (!parts.is_empty()).then(|| parts.join(" · "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modes_read_cleanly() {
        assert_eq!(mode("$L13 Deus Research Base span ·  stage2").as_deref(), Some("Stage 2"));
        assert_eq!(mode("$L13 Fire Temple span ·  Conquest [Hard]").as_deref(), Some("Conquest [Hard]"));
        assert_eq!(mode("$L13 Krao Cave span ·  Conquest span ·  Phase   1").as_deref(), Some("Conquest · Phase 1"));
        assert_eq!(mode("$L13 Somewhere"), None);
    }
}
