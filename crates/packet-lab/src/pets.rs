//! `packet-lab pets`: takes the pets from a scraped pet database and writes
//! `gamedata/pets.json`, which names the pets in the meter's pet soul ticker.
//!
//! Kept: each pet's name and genus (Cogni, Natura, Varian, Special…), by the ID the
//! game sends with its souls (`0x900d`: the Black Dragon is 1001).

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use anyhow::{bail, Context, Result};
use serde_json::{json, Map, Value};

pub fn run(pets: &Path, out: &Path, scraped: &str) -> Result<()> {
    let text = fs::read_to_string(pets).with_context(|| format!("reading {}", pets.display()))?;
    let Value::Object(all) = serde_json::from_str(&text)? else { bail!("{} isn't a JSON object by pet ID", pets.display()) };
    let mut kept: BTreeMap<u32, Value> = BTreeMap::new();
    for (id, p) in &all {
        let (Ok(id), Some(name)) = (id.parse::<u32>(), p["name"].as_str()) else { continue };
        let mut e = Map::new();
        e.insert("name".into(), json!(name));
        if let Some(genus) = p["details"]["Genus"].as_str() {
            e.insert("genus".into(), json!(genus));
        }
        kept.insert(id, Value::Object(e));
    }
    let doc = json!({
        "source": {
            "from": "a scraped Aion 2 pet database (collected from public fan databases; the bundle names no source site)",
            "scraped": scraped,
            "note": "Aion 2 game data; names belong to NCSOFT. Not covered by Glass Cannon's MIT licence. Made with packet-lab pets.",
        },
        "pets": kept,
    });
    if let Some(dir) = out.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::write(out, serde_json::to_string_pretty(&doc)? + "\n").with_context(|| format!("writing {}", out.display()))?;
    println!("done: {} pets saved to {}", kept.len(), out.display());
    Ok(())
}
