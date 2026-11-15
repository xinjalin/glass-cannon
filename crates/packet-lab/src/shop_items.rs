//! `packet-lab shop-items`: takes the items the Black Cloud Merchant sells from a scraped item
//! database and writes `gamedata/shop-items.json`, which names its offers in the black
//! market window.
//!
//! Kept, by item ID: pets ("Pet: Fungie", with the pet's ID in `pets.json` when one has the
//! same name), skins ("Blue Dawn (Skin: Greatsword) (Bound)": the name, and what it's a skin
//! for), and Theostones. That's what its pages offered in our capture of 2026-11-14 (pets
//! and skins) and what the guides say its Special page also has; an item it offers that
//! isn't here shows as its number. Each keeps its icon's file name too, for
//! `packet-lab shop-icons`, and its grade where our data tells it (see [`grade_of`]).

use std::collections::{BTreeMap, BTreeSet};
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
    // Cosmetics it may offer on its Special page.
    let bare = name.trim_end_matches(" (Bound)");
    if let Some(emote) = bare.strip_prefix("Emote: ") {
        e.insert("kind".into(), json!("emote"));
        e.insert("name".into(), json!(emote));
        return Some(e);
    }
    if bare.ends_with(" Wings") {
        e.insert("kind".into(), json!("wings"));
        e.insert("name".into(), json!(bare));
        return Some(e);
    }
    if bare.contains("Skin Chest") {
        e.insert("kind".into(), json!("skin chest"));
        e.insert("name".into(), json!(bare));
        return Some(e);
    }
    None
}

/// Item grades, lowest first (the game's colours: grey, green, blue, gold, orange).
const GRADES: [&str; 5] = ["Common", "Rare", "Epic", "Unique", "Legendary"];

/// An item's grade, which the scrape doesn't say for these. A Theostone's is in our
/// Theostone data (by name). A skin's is the gear's that shares its icon (most do: the skin
/// looks like it): when they all agree, or when three grades share it, the middle one
/// (Chivalrous Will's Common, Rare and Epic twins: it's Rare). Two grades, or no twin:
/// none, rather than a guess. Checked against the game's card colours (2026-11-16: four
/// skins green, Rare; a Theostone blue, Epic) and a few items on questlog.gg (Silver Flux
/// Epic, Chivalrous Will Rare).
fn grade_of(e: &Map<String, Value>, twins: &BTreeMap<String, BTreeSet<usize>>, theostones: &BTreeMap<String, String>) -> Option<String> {
    match e["kind"].as_str()? {
        "theostone" => theostones.get(e["name"].as_str()?.trim_end_matches(" (Bound)")).cloned(),
        "skin" => {
            let grades: Vec<usize> = twins.get(e["icon"].as_str()?)?.iter().copied().collect();
            let at = match grades.len() {
                1 => grades[0],
                n if n >= 3 => grades[n / 2],
                _ => return None,
            };
            Some(GRADES[at].to_string())
        }
        _ => None,
    }
}

/// Each icon's gear grades (by their place in [`GRADES`]), from `gear.json`.
fn gear_twins(gear: &Path) -> Result<BTreeMap<String, BTreeSet<usize>>> {
    let doc: Value = serde_json::from_str(&fs::read_to_string(gear).with_context(|| format!("reading {}", gear.display()))?)?;
    let mut twins: BTreeMap<String, BTreeSet<usize>> = BTreeMap::new();
    for piece in doc["gear"].as_object().into_iter().flatten().map(|(_, p)| p) {
        let (Some(icon), Some(grade)) = (piece["icon"].as_str(), piece["grade"].as_str()) else { continue };
        if let Some(at) = GRADES.iter().position(|g| *g == grade) {
            twins.entry(icon.to_string()).or_default().insert(at);
        }
    }
    Ok(twins)
}

/// Each Theostone's grade by name, from `theostones.json`.
fn theostone_grades(path: &Path) -> Result<BTreeMap<String, String>> {
    let doc: Value = serde_json::from_str(&fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?)?;
    Ok(doc["theostones"]
        .as_object()
        .into_iter()
        .flatten()
        .filter_map(|(_, t)| Some((t["name"].as_str()?.to_string(), t["grade"].as_str()?.to_string())))
        .collect())
}

pub fn run(items: &Path, pets: &Path, gear: &Path, theostones: &Path, out: &Path, scraped: &str) -> Result<()> {
    let twins = gear_twins(gear)?;
    let theostones = theostone_grades(theostones)?;
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
        .filter_map(|(id, item)| {
            let mut e = kind(item["name"].as_str()?, &by_name)?;
            // Its icon's file name, without the extension (`packet-lab shop-icons` fetches it).
            if let Some(icon) = item["icon"].as_str() {
                e.insert("icon".into(), json!(icon.trim_end_matches(".webp").trim_end_matches(".png")));
            }
            if let Some(grade) = grade_of(&e, &twins, &theostones) {
                e.insert("grade".into(), json!(grade));
            }
            Some((id.parse::<u32>().ok()?, Value::Object(e)))
        })
        .collect();
    let doc = json!({
        "source": {
            "from": "a scraped Aion 2 item database (collected from public fan databases; the bundle names no source site)",
            "scraped": scraped,
            "note": "Aion 2 game data; names belong to NCSOFT. Not covered by Glass Cannon's MIT licence. Only the pets, skins, Theostones and cosmetics (wings, emotes, skin chests): what the Black Cloud Merchant sells. Made with packet-lab shop-items.",
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
        // Cosmetics.
        let wings = kind("Black Fragment Wings", &pets).unwrap();
        assert_eq!((wings["kind"].as_str(), wings["name"].as_str()), (Some("wings"), Some("Black Fragment Wings")));
        let emote = kind("Emote: Gaze (Bound)", &pets).unwrap();
        assert_eq!((emote["kind"].as_str(), emote["name"].as_str()), (Some("emote"), Some("Gaze")));
        assert_eq!(kind("Weapon Skin Chest: Eternal Sun (Bound)", &pets).unwrap()["kind"], "skin chest");
        assert!(kind("Elder Greatsword", &pets).is_none());
        // Grades: a skin's from the gear sharing its icon, a Theostone's by name.
        let twins = BTreeMap::from([
            ("Icon_A".to_string(), BTreeSet::from([1])),
            ("Icon_B".to_string(), BTreeSet::from([0, 1, 2])),
            ("Icon_C".to_string(), BTreeSet::from([0, 1])),
        ]);
        let stones = BTreeMap::from([("Theostone: Boreas's Encouragement".to_string(), "Epic".to_string())]);
        let skin = |icon: &str| {
            let mut e = kind("Blue Dawn (Skin: Greatsword)", &pets).unwrap();
            e.insert("icon".into(), json!(icon));
            grade_of(&e, &twins, &stones)
        };
        assert_eq!(skin("Icon_A").as_deref(), Some("Rare"));
        assert_eq!(skin("Icon_B").as_deref(), Some("Rare"), "three grades: the middle one");
        assert_eq!(skin("Icon_C"), None, "two grades: no guess");
        assert_eq!(skin("Icon_D"), None, "no twin");
        let stone = kind("Theostone: Boreas's Encouragement (Bound)", &pets).unwrap();
        assert_eq!(grade_of(&stone, &twins, &stones).as_deref(), Some("Epic"));
        // A pet the pet data doesn't have keeps its name, with no pet ID.
        assert!(kind("Pet: Void", &pets).unwrap().get("pet").is_none());
    }
}
