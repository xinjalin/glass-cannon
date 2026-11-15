//! `packet-lab wings`: takes the wings from a scraped wing database and writes
//! `gamedata/wings.json`, which names the wings in your collection (the game's wing list,
//! `0x5672`, gives wing IDs) and ties the black market's wing items to them by name.
//!
//! Kept: each wing's name, grade, faction and icon name, by wing ID; its icon is fetched
//! when it isn't in the data folder yet.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use anyhow::{bail, Context, Result};
use serde_json::{json, Map, Value};

pub fn run(wings: &Path, out: &Path, scraped: &str) -> Result<()> {
    let text = fs::read_to_string(wings).with_context(|| format!("reading {}", wings.display()))?;
    let Value::Object(all) = serde_json::from_str(&text)? else { bail!("{} isn't a JSON object by wing ID", wings.display()) };
    let mut kept: BTreeMap<u32, Value> = BTreeMap::new();
    let mut icons = BTreeSet::new();
    for (id, w) in &all {
        let (Ok(id), Some(name)) = (id.parse::<u32>(), w["name"].as_str()) else { continue };
        let mut e = Map::new();
        e.insert("name".into(), json!(name));
        let props = &w["properties"];
        if let Some(grade) = props["Grade"].as_str().or(w["type"].as_str()) {
            e.insert("grade".into(), json!(grade));
        }
        if let Some(faction) = props["Faction"].as_str() {
            e.insert("faction".into(), json!(faction));
        }
        let icon = w["icon"].as_str().map(|i| i.trim_end_matches(".webp").trim_end_matches(".png")).filter(|i| crate::shop_icons::plain(i));
        if let Some(icon) = icon {
            e.insert("icon".into(), json!(icon));
            icons.insert(icon.to_string());
        }
        kept.insert(id, Value::Object(e));
    }
    if kept.is_empty() {
        bail!("no wings in {}", wings.display());
    }
    let doc = json!({
        "source": {
            "from": "a scraped Aion 2 wing database (collected from public fan databases; the bundle names no source site)",
            "scraped": scraped,
            "note": "Aion 2 game data; names belong to NCSOFT. Not covered by Glass Cannon's MIT licence. Made with packet-lab wings.",
        },
        "wings": kept,
    });
    let dir = out.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(dir)?;
    fs::write(out, serde_json::to_string(&doc)? + "\n").with_context(|| format!("writing {}", out.display()))?;
    println!("done: {} wings saved to {}", kept.len(), out.display());
    crate::shop_icons::fetch_all(&icons, dir)
}
