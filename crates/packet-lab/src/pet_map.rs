//! `packet-lab pet-map`: takes the mob spawn points of SkeeveAN's Aion DPS Meter (MIT) and
//! writes `gamedata/pet-map.json`, for the pet map window: on each world and Abyss map,
//! where the mobs that give each pet's souls spawn (metres, the game's own x and y: the
//! mob spawns of a recording in Altgard sit on these, 2.4 m off at the median).
//!
//! Which mob types give which pet comes from `gamedata/pet-spawns.json` (`pet-spawns`).
//!
//! With `--regions` and `--images`, also each map's picture and its zones, from Questlog's
//! interactive map (questlog.gg/aion-2/en/map): its `map.getRegions` (zone outlines and
//! where their names go) and its map images (one picture per map, or a map's tiles as
//! `<key>_<x>_<y>.webp`). The pictures are shrunk to about 2 m a pixel and saved as
//! `petmap_<key>.jpg`. Questlog's y runs the other way (its y is minus the game's);
//! with that turned round, its zones and pictures sit on the spawn points.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use anyhow::{bail, Context, Result};
use image::imageops::FilterType;
use image::{GenericImage, RgbImage};
use serde_json::{json, Value};

/// A map: its path in the spawn file and file names, its name, the game's map ID, and
/// where its picture lies (cm, as Questlog has it: left, top, columns, rows, sector size).
struct Map {
    path: &'static str,
    key: &'static str,
    name: &'static str,
    id: u32,
    sectors: [f64; 5],
}

const MAPS: [Map; 7] = [
    Map { path: "World/World_L/World_L_A", key: "World_L_A", name: "Verteron", id: 1010, sectors: [-408e3, -408e3, 8.0, 8.0, 102e3] },
    Map { path: "World/World_D/World_D_A", key: "World_D_A", name: "Altgard", id: 1110, sectors: [-408e3, -408e3, 8.0, 8.0, 102e3] },
    Map { path: "World/World_L/World_L_B", key: "World_L_B", name: "Eltnen", id: 1011, sectors: [-408e3, -408e3, 8.0, 8.0, 102e3] },
    Map { path: "World/World_D/World_D_B", key: "World_D_B", name: "Morheim", id: 1111, sectors: [-408e3, -408e3, 8.0, 8.0, 102e3] },
    Map {
        path: "Intersever/Abyss/Abyss_Reshanta_A",
        key: "Abyss_Reshanta_A",
        name: "Lower Reshanta",
        id: 20,
        sectors: [-204e3, -204e3, 4.0, 4.0, 102e3],
    },
    Map {
        path: "Intersever/Abyss/Abyss_Reshanta_C",
        key: "Abyss_Reshanta_C",
        name: "Middle Reshanta",
        id: 22,
        sectors: [-102e3, -102e3, 2.0, 2.0, 102e3],
    },
    Map {
        path: "Intersever/Abyss/Abyss_Reshanta_D",
        key: "Abyss_Reshanta_D",
        name: "Upper Reshanta",
        id: 23,
        sectors: [-102e3, -102e3, 2.0, 2.0, 102e3],
    },
];

/// Metres a pixel of a saved picture, and the sizes it's kept within.
const METRES_PER_PX: f64 = 2.0;
const MIN_PX: u32 = 1024;
const MAX_PX: u32 = 4096;
/// A Questlog map tile's size.
const TILE_PX: u32 = 1024;

fn read(path: &Path) -> Result<Value> {
    let text = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    Ok(serde_json::from_str(&text)?)
}

/// A map's picture, whole: its one image, or its tiles put together.
fn load_picture(dir: &Path, m: &Map) -> Result<RgbImage> {
    let whole = dir.join(format!("{}.webp", m.key));
    if whole.exists() {
        return Ok(image::open(&whole).with_context(|| format!("reading {}", whole.display()))?.to_rgb8());
    }
    let (cols, rows) = (m.sectors[2] as u32, m.sectors[3] as u32);
    let mut out = RgbImage::new(cols * TILE_PX, rows * TILE_PX);
    for x in 0..cols {
        for y in 0..rows {
            let tile = dir.join(format!("{}_{x:02}_{y:02}.webp", m.key));
            let img = image::open(&tile).with_context(|| format!("reading {}", tile.display()))?.to_rgb8();
            let img = if img.width() == TILE_PX && img.height() == TILE_PX {
                img
            } else {
                image::imageops::resize(&img, TILE_PX, TILE_PX, FilterType::Triangle)
            };
            out.copy_from(&img, x * TILE_PX, y * TILE_PX)?;
        }
    }
    Ok(out)
}

/// A map's zones from Questlog's regions, in metres with the game's y.
fn zones(regions: &Value, id: u32) -> Vec<Value> {
    let flip = |p: &Value| -> Option<[i64; 2]> { Some([(p[0].as_f64()? / 100.0).round() as i64, (-p[1].as_f64()? / 100.0).round() as i64]) };
    regions["result"]["data"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|r| r["mapWorldId"].as_str() == Some(&id.to_string()))
        .filter_map(|r| {
            let polygons: Vec<Vec<[i64; 2]>> = r["polygons"]
                .as_array()?
                .iter()
                .map(|poly| {
                    let mut pts: Vec<[i64; 2]> = poly.as_array().into_iter().flatten().filter_map(flip).collect();
                    pts.dedup();
                    pts
                })
                .filter(|p| p.len() >= 3)
                .collect();
            Some(json!({
                "name": r["name"].as_str()?,
                "rank": r["rank"].as_u64().unwrap_or(2),
                "label": flip(&r["label"]),
                "polygons": polygons,
            }))
        })
        .collect()
}

pub fn run(spawns: &Path, pet_spawns: &Path, regions: Option<&Path>, images: Option<&Path>, out: &Path, commit: &str) -> Result<()> {
    let spawns = read(spawns)?;
    let monsters: BTreeMap<String, u64> = read(pet_spawns)?["monsters"]
        .as_object()
        .context("pet-spawns.json has no monsters")?
        .iter()
        .filter_map(|(k, v)| Some((k.clone(), v.as_u64()?)))
        .collect();
    let regions = regions.map(read).transpose()?;
    if regions.is_some() != images.is_some() {
        bail!("--regions and --images go together");
    }
    let dir = out.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(dir)?;
    let mut maps = Vec::new();
    let mut points = 0;
    for m in &MAPS {
        let types = spawns["maps"][m.path].as_object().with_context(|| format!("spawns.json has no {}", m.path))?;
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
        let mut entry = json!({ "key": m.path, "name": m.name, "ids": [m.id], "pets": pets });
        if let (Some(regions), Some(images)) = (&regions, images) {
            let [left, top, cols, rows, size] = m.sectors;
            let (w, h) = (cols * size / 100.0, rows * size / 100.0);
            let px = ((w / METRES_PER_PX).round() as u32).clamp(MIN_PX, MAX_PX);
            let picture = load_picture(images, m)?;
            let small = image::imageops::resize(&picture, px, (f64::from(px) * h / w).round() as u32, FilterType::Triangle);
            let file = format!("petmap_{}.jpg", m.key);
            let mut bytes = Vec::new();
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, 82).encode_image(&small)?;
            fs::write(dir.join(&file), &bytes)?;
            entry["image"] = json!({ "file": file, "x": left / 100.0, "y": top / 100.0, "w": w, "h": h });
            entry["zones"] = Value::Array(zones(regions, m.id));
            println!("{}: {}x{} picture ({} KB), {} zones", m.name, small.width(), small.height(), bytes.len() / 1024, entry["zones"].as_array().map_or(0, Vec::len));
        }
        maps.push(entry);
    }
    let mut source = json!({
        "from": "SkeeveAN's Aion DPS Meter, Client/assets/aion2/pets/spawns.json (https://github.com/SkeeveAN/Aion-DPS-Meter)",
        "commit": commit,
        "licence": "MIT, Copyright (c) 2026 SkeeveTV",
        "note": "Aion 2 game data; names belong to NCSOFT. Positions in metres. Made with packet-lab pet-map.",
    });
    if regions.is_some() {
        source["pictures_and_zones"] =
            json!("Questlog's interactive map (questlog.gg/aion-2/en/map): its map images and map.getRegions, no licence; the pictures are NCSOFT's artwork.");
    }
    let doc = json!({ "source": source, "maps": maps });
    fs::write(out, serde_json::to_string(&doc)? + "\n").with_context(|| format!("writing {}", out.display()))?;
    println!("done: {points} spawn points on {} maps saved to {}", MAPS.len(), out.display());
    Ok(())
}
