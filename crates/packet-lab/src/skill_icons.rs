//! `packet-lab skill-icons`: the official skill icons, for the action bars, cooldowns and the
//! rest, sharper than the icon sets that came with the skill names.
//!
//! A scrape of aion2.app's skill database names each class skill's icon (Onslaught
//! 18010000 has `ICON_CH_SKILL_001`); each is downloaded from NCSOFT's own game asset
//! server (256 px there), shrunk to [`SIZE`] pixels and saved in `gamedata` under its own
//! name, and `gamedata/skill-icons.json` says which skill (by its base ID, `ccxx0000`: every
//! variant of a skill shares its icon) uses which file. Icons already there are kept.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use anyhow::{bail, Context, Result};
use serde_json::{json, Map, Value};

/// Shown at up to about 3em (big action bar icons): sharp on a 2× screen, and still about
/// the size on disk of the 100 px icons they stand in for.
const SIZE: u32 = 96;

/// The class skills in the scrape and their icon names, by base skill ID.
pub fn icon_names(scrape: &Map<String, Value>) -> BTreeMap<u32, String> {
    scrape
        .iter()
        .filter_map(|(id, s)| {
            let id: u32 = id.parse().ok()?;
            // Class skills' bases (11 to 19, then the skill, then 0000); the rest (gliding,
            // the system's) aren't cast in fights.
            if !(11_000_000..20_000_000).contains(&id) || !id.is_multiple_of(10_000) {
                return None;
            }
            // The skill's own icon (its specialties have theirs further in).
            let icon = s.get("icon")?.as_str()?;
            let name = icon.rsplit('/').next()?.trim_end_matches(".webp").trim_end_matches(".png");
            crate::shop_icons::plain(name).then(|| (id, name.to_string()))
        })
        .collect()
}

pub fn run(scrape: &Path, dir: &Path, scraped: &str) -> Result<()> {
    let bytes = fs::read(scrape).with_context(|| format!("can't read {}", scrape.display()))?;
    let Value::Object(scrape) = serde_json::from_slice(&bytes)? else { bail!("{} isn't a JSON object", scrape.display()) };
    let names = icon_names(&scrape);
    if names.is_empty() {
        bail!("no class skill in the scrape names an icon");
    }
    let wanted: BTreeSet<String> = names.values().cloned().collect();
    crate::shop_icons::fetch_all_sized(&wanted, dir, SIZE)?;
    // Only those that came: a skill whose icon didn't keeps the one it had.
    let icons: BTreeMap<u32, String> = names
        .into_iter()
        .map(|(id, name)| (id, crate::shop_icons::file_for(&name)))
        .filter(|(_, file)| dir.join(file).is_file())
        .collect();
    let doc = json!({
        "source": {
            "icons": crate::item_icons::ASSETS,
            "names": "each class skill's icon name, from a scrape of aion2.app's skill database",
            "scraped": scraped,
            "size": SIZE,
            "note": "Aion 2 artwork, belonging to NCSOFT. Not covered by Glass Cannon's MIT licence. Made with packet-lab skill-icons.",
        },
        "icons": icons,
    });
    let out = dir.join("skill-icons.json");
    fs::write(&out, serde_json::to_string_pretty(&doc)? + "\n").with_context(|| format!("writing {}", out.display()))?;
    println!("done: icons for {} skills ({} files) in {}", icons.len(), icons.values().collect::<BTreeSet<_>>().len(), dir.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn class_skills_and_their_own_icons() {
        let scrape: Map<String, Value> = serde_json::from_str(
            r#"{
                "18010000": { "name": "Onslaught", "icon": "ICON_CH_SKILL_001.webp",
                              "specs": [ { "id": 18010010, "icon": "Item/ETC/Icon_Item_Usable_Stigma_CH_A_c_002" } ] },
                "18010010": { "name": "Onslaught spec", "icon": "ICON_CH_SKILL_002.webp" },
                "17300000": { "name": "Passive", "icon": "Skill/ICON_CL_SKILL_Passive_003.png" },
                "1001": { "name": "Glide", "icon": "ICON_CO_SKILL_004.webp" },
                "15060000": { "name": "Hellfire" },
                "15070000": { "name": "Odd", "icon": "../x y.webp" },
                "notanid": { "icon": "ICON_X.webp" }
            }"#,
        )
        .unwrap();
        let got = icon_names(&scrape);
        assert_eq!(
            got.into_iter().collect::<Vec<_>>(),
            [(17_300_000, "ICON_CL_SKILL_Passive_003".to_string()), (18_010_000, "ICON_CH_SKILL_001".to_string())]
        );
    }
}
