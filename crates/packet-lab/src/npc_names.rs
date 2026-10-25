//! `packet-lab npc-names`: takes the NPCs from a scraped NPC database and writes
//! `gamedata/npc-names.json`, which names the NPCs (quest givers, merchants, guards…) that
//! the mob lists in `npcs.json` don't have, by the type ID the game spawns them with
//! (`0x3641`: Elvida is 2950865).
//!
//! Kept: each NPC's name, nothing else.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};

pub fn run(npcs: &Path, out: &Path, scraped: &str) -> Result<()> {
    let text = fs::read_to_string(npcs).with_context(|| format!("reading {}", npcs.display()))?;
    let Value::Object(all) = serde_json::from_str(&text)? else { bail!("{} isn't a JSON object by NPC ID", npcs.display()) };
    let kept: BTreeMap<u32, &str> = all
        .iter()
        .filter_map(|(id, n)| Some((id.parse::<u32>().ok()?, n["name"].as_str().filter(|s| !s.trim().is_empty())?)))
        .collect();
    let doc = json!({
        "source": {
            "from": "a scraped Aion 2 NPC database (collected from public fan databases; the bundle names no source site)",
            "scraped": scraped,
            "note": "Aion 2 game data; names belong to NCSOFT. Not covered by Glass Cannon's MIT licence. Made with packet-lab npc-names.",
        },
        "npcs": kept,
    });
    if let Some(dir) = out.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::write(out, serde_json::to_string_pretty(&doc)? + "\n").with_context(|| format!("writing {}", out.display()))?;
    println!("done: {} NPC names saved to {}", kept.len(), out.display());
    Ok(())
}
