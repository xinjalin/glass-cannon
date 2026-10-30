//! `packet-lab pet-map`: takes the mob spawn points of SkeeveAN's Aion DPS Meter (MIT) and
//! writes `gamedata/pet-map.json`, for the pet map window: on each world and Abyss map,
//! where the mobs that give each pet's souls spawn (metres, the game's own x and y: the
//! mob spawns of a recording in Altgard sit on these, 2.4 m off at the median).
//!
//! Which mob types give which pet comes from `gamedata/pet-spawns.json` (`pet-spawns`).

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde_json::{json, Value};

/// The maps kept: (path in the spawn file, name, the game's map IDs for it). Verteron and
/// Altgard's IDs are the field bosses' (Altgard's confirmed on a recording); Lower
/// Reshanta's by its name there.
const MAPS: [(&str, &str, &[u32]); 7] = [
    ("World/World_L/World_L_A", "Verteron", &[1010]),
    ("World/World_D/World_D_A", "Altgard", &[1110]),
    ("World/World_L/World_L_B", "Eltnen", &[]),
    ("World/World_D/World_D_B", "Morheim", &[]),
    ("Intersever/Abyss/Abyss_Reshanta_A", "Lower Reshanta", &[20]),
    ("Intersever/Abyss/Abyss_Reshanta_C", "Middle Reshanta", &[]),
    ("Intersever/Abyss/Abyss_Reshanta_D", "Upper Reshanta", &[]),
];

fn read(path: &Path) -> Result<Value> {
    let text = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    Ok(serde_json::from_str(&text)?)
}

pub fn run(spawns: &Path, pet_spawns: &Path, out: &Path, commit: &str) -> Result<()> {
    let spawns = read(spawns)?;
    let monsters: BTreeMap<String, u64> = read(pet_spawns)?["monsters"]
        .as_object()
        .context("pet-spawns.json has no monsters")?
        .iter()
        .filter_map(|(k, v)| Some((k.clone(), v.as_u64()?)))
        .collect();
    let mut maps = Vec::new();
    let mut points = 0;
    for (path, name, ids) in MAPS {
        let types = spawns["maps"][path].as_object().with_context(|| format!("spawns.json has no {path}"))?;
        let mut pets: BTreeMap<u64, Vec<[i64; 2]>> = BTreeMap::new();
        for (mob, at) in types {
            let Some(&pet) = monsters.get(mob) else { continue };
            for p in at.as_array().into_iter().flatten() {
                let (Some(x), Some(y)) = (p[0].as_f64(), p[1].as_f64()) else { continue };
                pets.entry(pet).or_default().push([(x / 100.0).round() as i64, (y / 100.0).round() as i64]);
            }
        }
        for v in pets.values_mut() {
            v.sort_unstable();
            v.dedup();
            points += v.len();
        }
        maps.push(json!({ "key": path, "name": name, "ids": ids, "pets": pets }));
    }
    let doc = json!({
        "source": {
            "from": "SkeeveAN's Aion DPS Meter, Client/assets/aion2/pets/spawns.json (https://github.com/SkeeveAN/Aion-DPS-Meter)",
            "commit": commit,
            "licence": "MIT, Copyright (c) 2026 SkeeveTV",
            "note": "Aion 2 game data; names belong to NCSOFT. Positions in metres. Made with packet-lab pet-map.",
        },
        "maps": maps,
    });
    if let Some(dir) = out.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::write(out, serde_json::to_string(&doc)? + "\n").with_context(|| format!("writing {}", out.display()))?;
    println!("done: {points} spawn points on {} maps saved to {}", MAPS.len(), out.display());
    Ok(())
}
