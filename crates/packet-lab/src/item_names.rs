//! `packet-lab item-names`: takes the materials, consumables and currencies from a scraped
//! item database and writes `gamedata/items.json`, which names what the farming widget
//! counts (Abyss Points, Kina, souls, shards, crafting materials) by the item ID the game
//! sends with each change of your inventory (`0x561b`).
//!
//! Kept: each item's name, its grade when it isn't Common, and whether it's a currency.
//! Gear isn't here: `gear.json` names it already.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};

/// The scrape's categories for what's gathered, looted and spent rather than worn.
const KEPT: [&str; 2] = ["Materials", "Consumables"];

pub fn run(items: &Path, out: &Path, scraped: &str) -> Result<()> {
    let text = fs::read_to_string(items).with_context(|| format!("reading {}", items.display()))?;
    let Value::Object(all) = serde_json::from_str(&text)? else { bail!("{} isn't a JSON object by item ID", items.display()) };
    let mut currencies = 0;
    let kept: BTreeMap<u32, Value> = all
        .iter()
        .filter_map(|(id, item)| {
            let id = id.parse::<u32>().ok()?;
            let name = item["name"].as_str().map(str::trim).filter(|s| !s.is_empty())?;
            let badges: Vec<&str> = item["badges"].as_array().map(|b| b.iter().filter_map(Value::as_str).collect()).unwrap_or_default();
            let currency = badges.contains(&"Currency");
            if !currency && !item["category"].as_str().is_some_and(|c| KEPT.contains(&c)) {
                return None;
            }
            let mut entry = json!({ "name": name });
            if let Some(grade) = item["grade"].as_str().filter(|g| !g.is_empty() && *g != "Common") {
                entry["grade"] = json!(grade);
            }
            if currency {
                entry["currency"] = json!(true);
                currencies += 1;
            }
            Some((id, entry))
        })
        .collect();
    if kept.is_empty() {
        bail!("no materials or consumables in {}: is it the scrape's items.json?", items.display());
    }
    let doc = json!({
        "source": {
            "from": "a scraped Aion 2 item database (collected from public fan databases; the bundle names no source site)",
            "scraped": scraped,
            "kept": "materials, consumables and currencies: name, grade when not Common, and whether it is a currency",
            "note": "Aion 2 game data; names belong to NCSOFT. Not covered by Glass Cannon's MIT licence. Made with packet-lab item-names.",
        },
        "items": kept,
    });
    if let Some(dir) = out.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::write(out, serde_json::to_string(&doc)? + "\n").with_context(|| format!("writing {}", out.display()))?;
    println!("done: {} item names ({currencies} currencies) saved to {}", kept.len(), out.display());
    Ok(())
}
