//! `packet-lab shop-icons` and `gear-icons`: icons for the items the Black Cloud Merchant
//! sells (skins, Theostones, wings…; pets have their portraits, `pet_<id>.png`) and for the
//! gear players wear, from NCSOFT's own game asset server, by the icon names
//! `gamedata/shop-items.json` and `gamedata/gear.json` keep (from a scraped item database).
//!
//! Shrunk to 48 pixels and saved in `gamedata` under the icon's own name, `<icon>.png`: a
//! skin and the gear it looks like share theirs, so each is fetched and kept once. An icon
//! already there isn't fetched again, so a run that stopped can be run again to finish. A
//! few are fetched at a time; each is written under a temporary name and renamed once
//! whole, so a run stopped part way leaves no half-written icon.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use serde_json::Value;

/// The icons' size: the black market window shows them at about two lines' height.
const SIZE: u32 = 48;
/// Downloads at a time: quicker than one by one, and still light on the server.
const AT_ONCE: usize = 4;

/// The file an icon is saved as (the app reads it by the same name).
pub fn file_for(icon: &str) -> String {
    format!("{icon}.png")
}

/// An icon name that's safe as a file name (letters, digits and underscores).
pub fn plain(icon: &str) -> bool {
    !icon.is_empty() && icon.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

pub fn run(shop_items: &Path, dir: &Path) -> Result<()> {
    let text = fs::read_to_string(shop_items).with_context(|| format!("reading {}", shop_items.display()))?;
    let doc: Value = serde_json::from_str(&text)?;
    let Some(items) = doc["items"].as_object() else { bail!("{} has no items", shop_items.display()) };
    // Pets with a portrait don't need one.
    let icons: BTreeSet<String> = items
        .values()
        .filter(|i| !(i["kind"] == "pet" && i.get("pet").is_some()))
        .filter_map(|i| i["icon"].as_str())
        .filter(|icon| plain(icon))
        .map(str::to_string)
        .collect();
    fetch_all(&icons, dir)
}

/// Fetches the icons not in `dir` yet, a few at a time.
pub fn fetch_all(icons: &BTreeSet<String>, dir: &Path) -> Result<()> {
    fetch_all_sized(icons, dir, SIZE)
}

/// The same, shrunk to `size` pixels.
pub fn fetch_all_sized(icons: &BTreeSet<String>, dir: &Path, size: u32) -> Result<()> {
    let wanted = icons.len();
    let had = icons.iter().filter(|icon| dir.join(file_for(icon)).exists()).count();
    let queue: Mutex<Vec<&str>> = Mutex::new(icons.iter().map(String::as_str).filter(|icon| !dir.join(file_for(icon)).exists()).collect());
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(30))
        .user_agent(crate::gamedata::USER_AGENT)
        .build();
    let (saved, failed) = (AtomicUsize::new(0), AtomicUsize::new(0));
    std::thread::scope(|s| {
        for _ in 0..AT_ONCE {
            s.spawn(|| loop {
                let Some(icon) = queue.lock().expect("the queue").pop() else { return };
                let out = dir.join(file_for(icon));
                let part = dir.join(format!("{}.part", file_for(icon)));
                let got = crate::item_icons::fetch_sized(&agent, icon, &part, size)
                    .and_then(|()| fs::rename(&part, &out).with_context(|| format!("naming {}", out.display())));
                match got {
                    Ok(()) => {
                        saved.fetch_add(1, Ordering::Relaxed);
                    }
                    Err(e) => {
                        let _ = fs::remove_file(&part);
                        failed.fetch_add(1, Ordering::Relaxed);
                        eprintln!("{icon}: {e:#}");
                    }
                }
            });
        }
    });
    println!(
        "done: {} icons saved to {} ({had} there already, {} failed, {wanted} wanted)",
        saved.into_inner(),
        dir.display(),
        failed.into_inner()
    );
    Ok(())
}
