//! `packet-lab item-icons`: icons for the buffs consumables give (scrolls, potions, food).
//!
//! Their skills are named in `gamedata/skills.json` but have no icon there (the icon sets
//! we use cover class skills). A scraped item database gives each item's icon file name;
//! items are matched to skills by name, and each icon is downloaded from NCSOFT's own game
//! asset server, shrunk to [`SIZE`] pixels and saved in `gamedata`, with
//! `gamedata/item-icons.json` saying which skill uses which file. Only skills that have
//! no icon yet are filled in.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::Path;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use image::imageops::FilterType;
use serde_json::{json, Map, Value};

/// NCSOFT's game asset server: `<icon name>.png`.
const ASSETS: &str = "https://assets.playnccdn.com/static-aion2-gamedata/resources";
/// Icons are shown at about 2em; this keeps them sharp on a 2× screen and small on disk.
const SIZE: u32 = 64;
const MAX_ICON_BYTES: u64 = 512 * 1024;

pub fn run(items: &Path, dir: &Path, fetched: &str) -> Result<()> {
    let items = read_object(items)?;
    let skills = read_object(&dir.join("skills.json"))?;
    let skills = skills.get("skills").and_then(Value::as_object).context("skills.json has no skills")?;
    // Each usable item's icon, by item name (bound and tradable copies share it).
    let mut icon_of: HashMap<String, String> = HashMap::new();
    for item in items.values() {
        let (Some(name), Some(icon)) = (item["name"].as_str(), item["icon"].as_str()) else { continue };
        if !icon.starts_with("Icon_Item_Usable_") {
            continue;
        }
        let name = name.trim_end_matches(" (Bound)").trim_end_matches(" (Tradable)");
        icon_of.entry(name.to_string()).or_insert_with(|| icon.trim_end_matches(".webp").trim_end_matches(".png").to_string());
    }
    // Skills with a name but no icon, whose name is a usable item's.
    let mut wanted: BTreeMap<u32, String> = BTreeMap::new();
    for (id, s) in skills {
        let (Ok(id), Some(name)) = (id.parse::<u32>(), s["name"].as_str()) else { continue };
        if s.get("icon").is_none() {
            if let Some(icon) = icon_of.get(name) {
                wanted.insert(id, icon.clone());
            }
        }
    }
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(30))
        .user_agent(crate::gamedata::USER_AGENT)
        .build();
    let mut files: BTreeMap<String, bool> = BTreeMap::new();
    for icon in wanted.values() {
        if files.contains_key(icon) {
            continue;
        }
        let file = format!("{icon}.png");
        let ok = match fetch(&agent, icon, &dir.join(&file)) {
            Ok(()) => true,
            Err(e) => {
                println!("skipped {icon}: {e:#}");
                false
            }
        };
        files.insert(icon.clone(), ok);
    }
    let icons: BTreeMap<u32, String> = wanted
        .into_iter()
        .filter(|(_, icon)| files.get(icon) == Some(&true))
        .map(|(id, icon)| (id, format!("{icon}.png")))
        .collect();
    let saved = files.values().filter(|ok| **ok).count();
    let doc = json!({
        "source": {
            "icons": ASSETS,
            "names": "item icon names from a scraped item database (collected from public fan databases)",
            "fetched": fetched,
            "note": "Aion 2 artwork, belonging to NCSOFT. Not covered by Glass Cannon's MIT licence. Made with packet-lab item-icons.",
        },
        "icons": icons,
    });
    let out = dir.join("item-icons.json");
    fs::write(&out, serde_json::to_string_pretty(&doc)? + "\n").with_context(|| format!("writing {}", out.display()))?;
    println!("done: {saved} icons for {} skills saved to {}", icons.len(), dir.display());
    Ok(())
}

/// Downloads one icon and saves it at [`SIZE`] pixels.
pub(crate) fn fetch(agent: &ureq::Agent, icon: &str, out: &Path) -> Result<()> {
    let bytes = crate::gamedata::get_bytes(agent, &format!("{ASSETS}/{icon}.png"), MAX_ICON_BYTES)?;
    if !bytes.starts_with(b"\x89PNG") {
        bail!("not a PNG");
    }
    let img = image::load_from_memory_with_format(&bytes, image::ImageFormat::Png)?;
    img.resize_exact(SIZE, SIZE, FilterType::Lanczos3)
        .save_with_format(out, image::ImageFormat::Png)
        .with_context(|| format!("writing {}", out.display()))
}

fn read_object(path: &Path) -> Result<Map<String, Value>> {
    let bytes = fs::read(path).with_context(|| format!("can't read {}", path.display()))?;
    match serde_json::from_slice(&bytes)? {
        Value::Object(m) => Ok(m),
        _ => bail!("{} isn't a JSON object", path.display()),
    }
}
