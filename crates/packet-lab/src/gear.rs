//! `packet-lab gear`: takes the equipment (weapons, armour, accessories) from a scraped item
//! database and writes `gamedata/gear.json`, which names the gear players wear (the party
//! view's tooltips).
//!
//! Kept: each piece's name, grade and item level, by the ID players' spawns (`0x3645`) give.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use anyhow::{bail, Context, Result};
use serde_json::{json, Map, Value};

const KINDS: [&str; 3] = ["Weapons", "Armor", "Accessories"];

pub fn run(items: &Path, out: &Path, scraped: &str) -> Result<()> {
    let text = fs::read_to_string(items).with_context(|| format!("reading {}", items.display()))?;
    let Value::Object(all) = serde_json::from_str(&text)? else { bail!("{} isn't a JSON object by item ID", items.display()) };
    let mut kept: BTreeMap<u32, Value> = BTreeMap::new();
    for (id, it) in &all {
        let (Ok(id), Some(name)) = (id.parse::<u32>(), it["name"].as_str()) else { continue };
        if !it["category"].as_str().is_some_and(|c| KINDS.contains(&c)) {
            continue;
        }
        let mut e = Map::new();
        e.insert("name".into(), json!(name));
        if let Some(grade) = it["grade"].as_str() {
            e.insert("grade".into(), json!(grade));
        }
        if let Some(level) = it["itemLevel"].as_u64() {
            e.insert("level".into(), json!(level));
        }
        kept.insert(id, Value::Object(e));
    }
    let doc = json!({
        "source": {
            "from": "a scraped Aion 2 item database (collected from public fan databases; the bundle names no source site)",
            "scraped": scraped,
            "note": "Aion 2 game data; names belong to NCSOFT. Not covered by Glass Cannon's MIT licence. Made with packet-lab gear.",
        },
        "gear": kept,
    });
    if let Some(dir) = out.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::write(out, serde_json::to_string(&doc)? + "\n").with_context(|| format!("writing {}", out.display()))?;
    println!("done: {} pieces of gear saved to {}", kept.len(), out.display());
    Ok(())
}
