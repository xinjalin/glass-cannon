//! `packet-lab shop-items`: takes the items the Black Cloud Merchant sells from a scraped item
//! database and writes `gamedata/shop-items.json`, which names its offers in the black
//! market window.
//!
//! Kept, by item ID: pets ("Pet: Fungie", with the pet's ID in `pets.json` when one has the
//! same name), skins ("Blue Dawn (Skin: Greatsword) (Bound)": the name, and what it's a skin
//! for), and Theostones. That's what its pages offered in our capture of 2026-11-14 (pets
//! and skins) and what the guides say its Special page also has; an item it offers that
//! isn't here shows as its number.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use anyhow::{bail, Context, Result};
use serde_json::{json, Map, Value};

/// What kind of item, and the rest of its entry, from its name.
fn kind(name: &str, pets: &BTreeMap<String, u32>) -> Option<Map<String, Value>> {
    let mut e = Map::new();
    if let Some(pet) = name.strip_prefix("Pet: ") {
        e.insert("kind".into(), json!("pet"));
        e.insert("name".into(), json!(pet));
        if let Some(id) = pets.get(&pet.to_lowercase()) {
            e.insert("pet".into(), json!(id));
        }
        return Some(e);
    }
    if let Some((skin, rest)) = name.split_once(" (Skin: ") {
        e.insert("kind".into(), json!("skin"));
        e.insert("name".into(), json!(skin));
        let slot = rest.split(')').next().unwrap_or_default();
        e.insert("slot".into(), json!(slot));
        return Some(e);
    }
    if name.contains("Theostone") {
        e.insert("kind".into(), json!("theostone"));
        e.insert("name".into(), json!(name));
        return Some(e);
    }
    None
}

pub fn run(items: &Path, pets: &Path, out: &Path, scraped: &str) -> Result<()> {
    let text = fs::read_to_string(items).with_context(|| format!("reading {}", items.display()))?;
    let Value::Object(all) = serde_json::from_str(&text)? else { bail!("{} isn't a JSON object by item ID", items.display()) };
    let pets: Value = serde_json::from_str(&fs::read_to_string(pets).with_context(|| format!("reading {}", pets.display()))?)?;
    let by_name: BTreeMap<String, u32> = pets["pets"]
        .as_object()
        .into_iter()
        .flatten()
        .filter_map(|(id, p)| Some((p["name"].as_str()?.to_lowercase(), id.parse().ok()?)))
        .collect();
    let kept: BTreeMap<u32, Value> = all
        .iter()
        .filter_map(|(id, item)| Some((id.parse::<u32>().ok()?, Value::Object(kind(item["name"].as_str()?, &by_name)?))))
        .collect();
    let doc = json!({
        "source": {
            "from": "a scraped Aion 2 item database (collected from public fan databases; the bundle names no source site)",
            "scraped": scraped,
            "note": "Aion 2 game data; names belong to NCSOFT. Not covered by Glass Cannon's MIT licence. Only the pets, skins and Theostones (what the Black Cloud Merchant sells). Made with packet-lab shop-items.",
        },
        "items": kept,
    });
    if let Some(dir) = out.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::write(out, serde_json::to_string_pretty(&doc)? + "\n").with_context(|| format!("writing {}", out.display()))?;
    println!("done: {} shop items saved to {}", kept.len(), out.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pets_skins_and_theostones() {
        let pets = BTreeMap::from([("krall laborer".to_string(), 1001)]);
        let pet = kind("Pet: Krall laborer", &pets).unwrap();
        assert_eq!((pet["kind"].as_str(), pet["name"].as_str(), pet["pet"].as_u64()), (Some("pet"), Some("Krall laborer"), Some(1001)));
        let skin = kind("Blue Dawn (Skin: Greatsword) (Bound)", &pets).unwrap();
        assert_eq!((skin["kind"].as_str(), skin["name"].as_str(), skin["slot"].as_str()), (Some("skin"), Some("Blue Dawn"), Some("Greatsword")));
        assert_eq!(kind("Theostone: Nezekan's March", &pets).unwrap()["kind"], "theostone");
        assert!(kind("Elder Greatsword", &pets).is_none());
        // A pet the pet data doesn't have keeps its name, with no pet ID.
        assert!(kind("Pet: Void", &pets).unwrap().get("pet").is_none());
    }
}
