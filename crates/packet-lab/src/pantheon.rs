//! `packet-lab pantheon`: takes the Pantheon decorations (artworks, statues, colossi) from
//! SkeeveAN's Aion DPS Meter (MIT, read from the game's text tables) and writes
//! `gamedata/pantheon.json`, which names the decorations in your collection: the game's
//! decoration list (`0x8aaf`) gives their IDs, the same as this file's.
//!
//! Kept: each one's kind, English name, side, icon name and stat effects ("Destiny +3");
//! its icon is fetched when it isn't in the data folder yet.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use anyhow::{bail, Context, Result};
use serde_json::{json, Map, Value};

pub fn run(pantheon: &Path, out: &Path, commit: &str) -> Result<()> {
    let text = fs::read_to_string(pantheon).with_context(|| format!("reading {}", pantheon.display()))?;
    let doc: Value = serde_json::from_str(&text)?;
    let Some(decorations) = doc["decorations"].as_object() else { bail!("{} has no decorations", pantheon.display()) };
    // Stats by key, in English ("Death" -> "Death [Triniel]").
    let stat = |k: &str| doc["stats"][k]["en"].as_str().unwrap_or(k).to_string();
    let mut kept: BTreeMap<u32, Value> = BTreeMap::new();
    let mut icons = BTreeSet::new();
    for (id, d) in decorations {
        let (Ok(id), Some(name)) = (id.parse::<u32>(), d["n"]["en"].as_str()) else { continue };
        let mut e = Map::new();
        e.insert("kind".into(), json!(d["k"].as_str().unwrap_or("frame")));
        e.insert("name".into(), json!(name));
        if let Some(side) = d["r"].as_str() {
            e.insert("side".into(), json!(side));
        }
        if let Some(icon) = d["i"].as_str().filter(|i| crate::shop_icons::plain(i)) {
            e.insert("icon".into(), json!(icon));
            icons.insert(icon.to_string());
        }
        let effects: Vec<Value> = d["e"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|x| Some(json!([stat(x[0].as_str()?), x[1].as_i64()?])))
            .collect();
        if !effects.is_empty() {
            e.insert("effects".into(), json!(effects));
        }
        kept.insert(id, Value::Object(e));
    }
    if kept.is_empty() {
        bail!("no decorations in {}", pantheon.display());
    }
    let doc = json!({
        "source": {
            "from": "SkeeveAN's Aion DPS Meter, Backend/src/data/aion2/pantheon.json (https://github.com/SkeeveAN/Aion-DPS-Meter)",
            "commit": commit,
            "licence": "MIT, Copyright (c) 2026 SkeeveTV",
            "note": "Names from the game's own text tables; Aion 2 game data, names and art belong to NCSOFT. Made with packet-lab pantheon.",
        },
        "decorations": kept,
    });
    let dir = out.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(dir)?;
    fs::write(out, serde_json::to_string(&doc)? + "\n").with_context(|| format!("writing {}", out.display()))?;
    println!("done: {} decorations saved to {}", kept.len(), out.display());
    crate::shop_icons::fetch_all(&icons, dir)
}
