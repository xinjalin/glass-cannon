//! `packet-lab pet-icons`: each pet's portrait for the pet tracker, from NCSOFT's own game
//! asset server (the icon file names come from a scraped pet database), shrunk to 64
//! pixels and saved in `gamedata` as `pet_<id>.png`.

use std::fs;
use std::path::Path;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use serde_json::Value;

pub fn run(pets: &Path, dir: &Path) -> Result<()> {
    let text = fs::read_to_string(pets).with_context(|| format!("reading {}", pets.display()))?;
    let Value::Object(all) = serde_json::from_str(&text)? else { bail!("{} isn't a JSON object by pet ID", pets.display()) };
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(30))
        .user_agent(crate::gamedata::USER_AGENT)
        .build();
    let (mut saved, mut failed) = (0, 0);
    for (id, p) in &all {
        let (Ok(id), Some(icon)) = (id.parse::<u32>(), p["icon"].as_str()) else { continue };
        let icon = icon.trim_end_matches(".webp").trim_end_matches(".png");
        match crate::item_icons::fetch(&agent, icon, &dir.join(format!("pet_{id}.png"))) {
            Ok(()) => saved += 1,
            Err(e) => {
                failed += 1;
                eprintln!("{id} {icon}: {e:#}");
            }
        }
    }
    println!("done: {saved} pet portraits saved to {} ({failed} failed)", dir.display());
    Ok(())
}
