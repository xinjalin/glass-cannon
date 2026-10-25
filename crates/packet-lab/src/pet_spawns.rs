//! `packet-lab pet-spawns`: takes the pet data of SkeeveAN's Aion DPS Meter (MIT) and
//! writes `gamedata/pet-spawns.json`, for the overlay's pet tracker.
//!
//! Kept: which mob types give which pet's souls (`pets.json`'s `monsters`, mob type → pet
//! ID), and the areas each pet is found in (`regions.json`: groups of subzones with their
//! pets), as "Altamia Canyon · Verteron".

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde_json::{json, Value};

fn read(path: &Path) -> Result<Value> {
    let text = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    Ok(serde_json::from_str(&text)?)
}

/// "verteron" → "Verteron".
fn title(id: &str) -> String {
    let mut c = id.chars();
    c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
}

pub fn run(pets: &Path, regions: &Path, out: &Path, commit: &str) -> Result<()> {
    let pets = read(pets)?;
    let regions = read(regions)?;
    let monsters: BTreeMap<u32, u64> = pets["monsters"]
        .as_object()
        .context("pets.json has no monsters")?
        .iter()
        .filter_map(|(k, v)| Some((k.parse().ok()?, v.as_u64()?)))
        .collect();
    let mut found: BTreeMap<u64, Vec<String>> = BTreeMap::new();
    for group in regions["groups"].as_array().context("regions.json has no groups")? {
        let world = title(group["id"].as_str().unwrap_or_default());
        for region in group["regions"].as_array().into_iter().flatten() {
            let Some(name) = region["names"]["en"].as_str() else { continue };
            for pet in region["pets"].as_array().into_iter().flatten().filter_map(Value::as_u64) {
                found.entry(pet).or_default().push(format!("{name} · {world}"));
            }
        }
    }
    let doc = json!({
        "source": {
            "from": "SkeeveAN's Aion DPS Meter, Client/assets/aion2/pets/pets.json and regions.json (https://github.com/SkeeveAN/Aion-DPS-Meter)",
            "commit": commit,
            "licence": "MIT, Copyright (c) 2026 SkeeveTV",
            "note": "Aion 2 game data; names belong to NCSOFT. Made with packet-lab pet-spawns.",
        },
        "monsters": monsters,
        "found_in": found,
    });
    if let Some(dir) = out.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::write(out, serde_json::to_string_pretty(&doc)? + "\n").with_context(|| format!("writing {}", out.display()))?;
    println!("done: {} mob types and {} pets' areas saved to {}", monsters.len(), found.len(), out.display());
    Ok(())
}
