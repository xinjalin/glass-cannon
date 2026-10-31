//! `packet-lab dungeons`: takes the dungeon mobs from a scraped monster database and writes
//! `gamedata/dungeons.json`, which the meter uses to group the history into dungeon runs.
//!
//! Kept: each dungeon's name and type (Expedition, Transcendence…), and for each mob type
//! that belongs to one, which dungeon and which mode (Exploration, Conquest, Stage 2…). A
//! dungeon's mobs have one ID per mode (Atiel: 2300601 in Exploration, 2340601 in Stage 2),
//! so the mob a fight was on says where it was.
//!
//! The scrape sometimes files a mob under the wrong dungeon (Draupnir's Conquest [Hard]
//! bosses under Dying Dramata's Nest). A mob's IDs in a dungeon's other modes differ only
//! in the third digit (Lakshmi: 2300401, 2310401, 2320401, 2330401), so a mob whose such
//! siblings of the same name, two or more, are all in one other dungeon is put there.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use anyhow::{bail, Context, Result};
use serde_json::{json, Map, Value};

pub fn run(monsters: &Path, dungeons: &Path, out: &Path, scraped: &str) -> Result<()> {
    let monsters = read_object(monsters)?;
    let dungeons = read_object(dungeons)?;
    let mut mobs: BTreeMap<u32, Value> = BTreeMap::new();
    let mut used: BTreeMap<String, Value> = BTreeMap::new();
    for (id, m) in &monsters {
        let Ok(id) = id.parse::<u32>() else { continue };
        let Some(dungeon) = m["references"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|r| r["category"] == "dungeons")
            .and_then(|r| r["id"].as_str())
        else {
            continue;
        };
        let Some(d) = dungeons.get(dungeon) else { continue };
        let mut entry = json!({ "dungeon": dungeon });
        if let Some(mode) = m["details"]["Dungeon"].as_str().and_then(mode) {
            entry["mode"] = json!(mode);
        }
        mobs.insert(id, entry);
        used.entry(dungeon.to_string()).or_insert_with(|| {
            let mut e = Map::new();
            e.insert("name".into(), d["name"].clone());
            if let Some(kind) = d["type"].as_str() {
                e.insert("type".into(), json!(kind));
            }
            Value::Object(e)
        });
    }
    let name = |id: u32| monsters.get(&id.to_string()).and_then(|m| m["name"].as_str());
    for (id, from, to) in misfiled(&mobs, name) {
        println!("{id}: filed under {from}, its other modes are in {to}: moved");
        mobs.get_mut(&id).expect("listed")["dungeon"] = json!(to);
    }
    let doc = json!({
        "source": {
            "from": "a scraped Aion 2 monster database (collected from public fan databases; the bundle names no source site)",
            "scraped": scraped,
            "note": "Aion 2 game data; names belong to NCSOFT. Not covered by Glass Cannon's MIT licence. Made with packet-lab dungeons.",
        },
        "dungeons": used,
        "mobs": mobs,
    });
    if let Some(dir) = out.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::write(out, serde_json::to_string_pretty(&doc)? + "\n").with_context(|| format!("writing {}", out.display()))?;
    println!("done: {} dungeons, {} mob types saved to {}", doc["dungeons"].as_object().map_or(0, Map::len), mobs.len(), out.display());
    Ok(())
}

/// Mobs whose other modes (two or more, the same name) are all in another dungeon: (ID,
/// dungeon, theirs).
fn misfiled<'a>(mobs: &BTreeMap<u32, Value>, name: impl Fn(u32) -> Option<&'a str>) -> Vec<(u32, String, String)> {
    let dungeon = |id: u32| mobs.get(&id).and_then(|m| m["dungeon"].as_str());
    mobs.keys()
        .filter_map(|&id| {
            let own = dungeon(id)?;
            let own_name = name(id)?;
            let theirs: Vec<&str> = siblings(id).filter(|&s| name(s) == Some(own_name)).filter_map(dungeon).collect();
            let other = *theirs.first()?;
            (theirs.len() >= 2 && other != own && theirs.iter().all(|d| *d == other)).then(|| (id, own.to_string(), other.to_string()))
        })
        .collect()
}

/// The same mob's IDs in the other modes: the third digit changed (2320401 → 2300401…).
fn siblings(id: u32) -> impl Iterator<Item = u32> {
    let text = id.to_string();
    (b'0'..=b'9').filter_map(move |d| {
        let mut b = text.clone().into_bytes();
        (b.len() == 7 && b[2] != d).then(|| {
            b[2] = d;
            String::from_utf8(b).ok()?.parse().ok()
        })?
    })
}

fn read_object(path: &Path) -> Result<Map<String, Value>> {
    let bytes = fs::read(path).with_context(|| format!("can't read {}", path.display()))?;
    match serde_json::from_slice(&bytes)? {
        Value::Object(m) => Ok(m),
        _ => bail!("{} isn't a JSON object", path.display()),
    }
}

/// The mode from a mob's dungeon line, which reads like
/// "$L13 Deus Research Base span ·  stage2": "Stage 2" (or "Conquest [Hard]", or
/// "Conquest · Phase 1" when there are more parts).
fn mode(line: &str) -> Option<String> {
    let parts: Vec<String> = line
        .split("span ·")
        .skip(1)
        .map(|p| p.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|p| !p.is_empty())
        .map(|p| match p.strip_prefix("stage") {
            Some(n) if n.chars().all(|c| c.is_ascii_digit()) && !n.is_empty() => format!("Stage {n}"),
            _ => p,
        })
        .collect();
    (!parts.is_empty()).then(|| parts.join(" · "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modes_read_cleanly() {
        assert_eq!(mode("$L13 Deus Research Base span ·  stage2").as_deref(), Some("Stage 2"));
        assert_eq!(mode("$L13 Fire Temple span ·  Conquest [Hard]").as_deref(), Some("Conquest [Hard]"));
        assert_eq!(mode("$L13 Krao Cave span ·  Conquest span ·  Phase   1").as_deref(), Some("Conquest · Phase 1"));
        assert_eq!(mode("$L13 Somewhere"), None);
    }

    #[test]
    fn a_mob_filed_under_another_dungeon_goes_back_with_its_other_modes() {
        let mob = |d: &str| json!({ "dungeon": d });
        let mobs: BTreeMap<u32, Value> = [
            // Lakshmi: Draupnir in three modes, Dying Dramata's Nest in Conquest [Hard].
            (2_300_401, mob("Draupnir")),
            (2_310_401, mob("Draupnir")),
            (2_320_401, mob("DeadDramataNest")),
            (2_330_401, mob("Draupnir")),
            // A mob with one other mode elsewhere stays.
            (2_300_582, mob("Fire_Temple_Arena")),
            (2_310_582, mob("BakronIsland")),
            // So does one whose siblings are other mobs.
            (2_300_109, mob("SunkenTemple")),
            (2_310_109, mob("KraoCave")),
            (2_330_109, mob("KraoCave")),
        ]
        .into();
        let name = |id: u32| Some(if id % 1000 == 109 && id != 2_300_109 { "Condensed Krao" } else if id % 1000 == 109 { "Laser" } else { "Lakshmi" });
        assert_eq!(misfiled(&mobs, name), [(2_320_401, "DeadDramataNest".to_string(), "Draupnir".to_string())]);
    }
}
