//! `packet-lab gear`: takes the equipment (weapons, armour, accessories) from a scraped item
//! database and writes `gamedata/gear.json`, which names the gear players wear (the party
//! view's tooltips).
//!
//! Kept: each piece's name, grade, item level and icon name, by the ID players' spawns
//! (`0x3645`) give. `gear-icons` adds the icon names to a `gear.json` made before, and
//! fetches the icons.

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
        if let Some(icon) = it["icon"].as_str().map(icon_name).filter(|i| crate::shop_icons::plain(i)) {
            e.insert("icon".into(), json!(icon));
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

/// An icon's name without its extension.
fn icon_name(icon: &str) -> &str {
    icon.trim_end_matches(".webp").trim_end_matches(".png")
}

/// `packet-lab gear-icons`: each piece of gear's icon name, from a scraped item database
/// (by item ID), into `gear.json` (nothing else in it changes), and the icons not in
/// `dir` yet fetched (see `shop_icons`).
pub fn icons(items: &Path, gear: &Path, dir: &Path) -> Result<()> {
    let text = fs::read_to_string(items).with_context(|| format!("reading {}", items.display()))?;
    let Value::Object(all) = serde_json::from_str(&text)? else { bail!("{} isn't a JSON object by item ID", items.display()) };
    let mut doc: Value = serde_json::from_str(&fs::read_to_string(gear).with_context(|| format!("reading {}", gear.display()))?)?;
    let Some(pieces) = doc["gear"].as_object_mut() else { bail!("{} has no gear", gear.display()) };
    let mut wanted = std::collections::BTreeSet::new();
    let mut named = 0;
    for (id, piece) in pieces.iter_mut() {
        let Some(icon) = all.get(id).and_then(|it| it["icon"].as_str()).map(icon_name).filter(|i| crate::shop_icons::plain(i)) else { continue };
        piece["icon"] = json!(icon);
        wanted.insert(icon.to_string());
        named += 1;
    }
    let total = pieces.len();
    fs::write(gear, serde_json::to_string(&doc)? + "\n").with_context(|| format!("writing {}", gear.display()))?;
    println!("{named} of {total} pieces of gear given their icon in {}", gear.display());
    crate::shop_icons::fetch_all(&wanted, dir)
}

