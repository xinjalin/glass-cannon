//! `packet-lab gather-map`: takes the gathering spots of Questlog's interactive map
//! (questlog.gg/aion-2/en/map, its `map.getMarkers`) and writes `gamedata/gather-map.json`,
//! for the map window's gathering side: on each world and Abyss map, where each material
//! (ore, gems, herbs, logs, food, Odyle) and collectable (hidden cubes, Empyrean traces)
//! is, in metres with the game's y (Questlog's y is minus the game's, as for its zones).
//!
//! With `--icons`, a folder of Questlog's item icons (`<icon name>.webp`, from its
//! `cdn.questlog.gg/aion-2/assets/…` paths), each material's icon is saved next to the
//! output as `gather_<icon name>.png` at 64 px.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use image::imageops::FilterType;
use serde_json::{json, Value};

/// The maps the pet map shows (the game's map IDs); the instance maps are left out.
const MAPS: [&str; 7] = ["1010", "1011", "1110", "1111", "20", "22", "23"];

/// Questlog's kinds of gathering spot, in the order the list shows them, with their names.
const KINDS: [(&str, &str); 10] = [
    ("metal", "Ore"),
    ("jewelry", "Gems"),
    ("herb", "Herbs"),
    ("tree", "Logs"),
    ("berry", "Berries"),
    ("vegetable", "Vegetables"),
    ("shellfish", "Shellfish"),
    ("od", "Odyle"),
    ("hidden-cube", "Hidden cubes"),
    ("monolith", "Empyrean traces"),
];

const ICON_PX: u32 = 64;

fn read(path: &Path) -> Result<Value> {
    let text = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    Ok(serde_json::from_str(&text)?)
}

/// An icon's name from Questlog's path (`/assets/…/Icon_X.Icon_X` → `Icon_X`).
fn icon_name(path: &str) -> Option<&str> {
    let last = path.rsplit('/').next()?;
    let name = last.split('.').next()?;
    (!name.is_empty() && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')).then_some(name)
}

/// A material: its kind, icon, and spots by map.
#[derive(Default)]
struct Item {
    kind: String,
    icon: Option<String>,
    points: BTreeMap<String, Vec<[i64; 2]>>,
}

pub fn run(markers: &Path, icons: Option<&Path>, out: &Path) -> Result<()> {
    let markers = read(markers)?;
    let list = markers["result"]["data"]["json"].as_array().or_else(|| markers["result"]["data"].as_array()).context("no markers in the file")?;
    let mut items: BTreeMap<String, Item> = BTreeMap::new();
    for m in list {
        if !matches!(m["mapType"].as_str(), Some("gatherable" | "collectable")) {
            continue;
        }
        let (Some(kind), Some(name), Some(map)) = (m["mapCategory"].as_str(), m["name"].as_str(), m["mapWorldId"].as_str()) else { continue };
        if !KINDS.iter().any(|(k, _)| *k == kind) || !MAPS.contains(&map) {
            continue;
        }
        let item = items.entry(name.to_string()).or_default();
        item.kind = kind.to_string();
        if item.icon.is_none() {
            item.icon = m["icon"].as_str().and_then(icon_name).map(str::to_string);
        }
        let at = item.points.entry(map.to_string()).or_default();
        for p in m["coordinates"].as_array().into_iter().flatten() {
            let (Some(x), Some(y)) = (p[0].as_f64(), p[1].as_f64()) else { continue };
            at.push([(x / 100.0).round() as i64, (-y / 100.0).round() as i64]);
        }
    }
    let dir = out.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(dir)?;
    let mut saved_icons = BTreeMap::new();
    let mut points = 0;
    let mut out_items = Vec::new();
    for (kind, _) in KINDS {
        for (name, item) in items.iter_mut().filter(|(_, i)| i.kind == kind) {
            for v in item.points.values_mut() {
                v.sort_unstable();
                v.dedup();
                points += v.len();
            }
            let mut entry = json!({ "name": name, "kind": kind, "points": item.points });
            if let (Some(icons), Some(icon)) = (icons, &item.icon) {
                let file = format!("gather_{icon}.png");
                if !saved_icons.contains_key(icon) {
                    let src = icons.join(format!("{icon}.webp"));
                    let img = image::open(&src).with_context(|| format!("reading {}", src.display()))?;
                    img.resize(ICON_PX, ICON_PX, FilterType::Lanczos3).save(dir.join(&file))?;
                    saved_icons.insert(icon.clone(), file.clone());
                }
                entry["icon"] = json!(file);
            }
            out_items.push(entry);
        }
    }
    let doc = json!({
        "source": {
            "from": "Questlog's interactive map (questlog.gg/aion-2/en/map): map.getMarkers and its item icons",
            "licence": "none: used without a licence; names and icons belong to NCSOFT",
            "note": "Positions in metres, the game's x and y. Made with packet-lab gather-map.",
        },
        "kinds": KINDS.iter().map(|(key, name)| json!({ "key": key, "name": name })).collect::<Vec<_>>(),
        "items": out_items,
    });
    fs::write(out, serde_json::to_string(&doc)? + "\n").with_context(|| format!("writing {}", out.display()))?;
    println!("done: {points} spots of {} materials ({} icons) saved to {}", items.len(), saved_icons.len(), out.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icon_names() {
        assert_eq!(icon_name("/assets/Game/UI/Item/ETC/Icon_Item_Gather_Ore_A_c_002.Icon_Item_Gather_Ore_A_c_002"), Some("Icon_Item_Gather_Ore_A_c_002"));
        assert_eq!(icon_name("/assets/x/a b.name"), None);
        assert_eq!(icon_name(""), None);
    }
}
