//! `packet-lab import-skills`: fills the overlay's local skill folder with names and
//! icons from the NOIA2 project (https://github.com/ZDYoung0519/NOIA2, GPL-3.0).
//!
//! Runs only when you ask. The data goes into a `skills` folder next to the programs
//! (inside `target/`, never committed), and the overlay itself never goes online. NOIA2's skill tables are keyed
//! by the same skill IDs the game sends (e.g. 18010340 = Onslaught); its icons are named
//! after the first four digits of the ID (class + skill number), e.g. 1801.png.

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{bail, Context, Result};
use serde_json::{json, Map, Value};

const REPO: &str = "ZDYoung0519/NOIA2";
const BRANCH: &str = "main";
const NAMES_DIR: &str = "src/i18n/locales/aion2skills";
const ICONS_DIR: &str = "public/aion2/skill";
const LANGUAGES: [&str; 4] = ["en", "ko", "zh-TW", "zh-CN"];
const USER_AGENT: &str = concat!("aion-meter-import/", env!("CARGO_PKG_VERSION"));
const MAX_JSON: u64 = 8 << 20;
const MAX_ICON: u64 = 256 << 10;

/// A `skills` folder next to this program, which is where the overlay (built into the
/// same folder) looks first.
pub fn default_dir() -> Result<PathBuf> {
    let exe = std::env::current_exe().context("finding this program's folder")?;
    let dir = exe.parent().context("this program has no folder")?;
    Ok(dir.join("skills"))
}

pub fn run(dir: &Path, lang: &str, with_icons: bool) -> Result<()> {
    if !LANGUAGES.contains(&lang) {
        bail!("language must be one of {}", LANGUAGES.join(", "));
    }
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(30))
        .user_agent(USER_AGENT)
        .build();

    // Pin one commit so names and icons come from the same snapshot.
    let commit: Value = get_json(&agent, &format!("https://api.github.com/repos/{REPO}/commits/{BRANCH}"))?;
    let sha = commit["sha"].as_str().context("no commit SHA in GitHub's reply")?.to_string();
    println!("NOIA2 commit {}", &sha[..12]);

    let names: Map<String, Value> = match get_json(
        &agent,
        &format!("https://raw.githubusercontent.com/{REPO}/{sha}/{NAMES_DIR}/{lang}.json"),
    )? {
        Value::Object(m) => m,
        _ => bail!("the {lang} skill table isn't a JSON object"),
    };

    let available: HashSet<String> = if with_icons {
        let tree: Value = get_json(
            &agent,
            &format!("https://api.github.com/repos/{REPO}/git/trees/{sha}?recursive=1"),
        )?;
        tree["tree"]
            .as_array()
            .context("no file list in GitHub's reply")?
            .iter()
            .filter_map(|e| e["path"].as_str()?.strip_prefix(&format!("{ICONS_DIR}/")).map(str::to_string))
            .collect()
    } else {
        HashSet::new()
    };

    // Build the table: every skill with a name, plus the icon file it would use.
    let mut skills: BTreeMap<u32, Value> = BTreeMap::new();
    let mut wanted: HashSet<String> = HashSet::new();
    for (id, name) in &names {
        let (Ok(num), Some(name)) = (id.parse::<u32>(), name.as_str()) else {
            continue;
        };
        let icon = icon_file(id).filter(|f| available.contains(f));
        if let Some(f) = &icon {
            wanted.insert(f.clone());
        }
        let mut entry = json!({ "name": name });
        if let Some(f) = icon {
            entry["icon"] = json!(f);
        }
        skills.insert(num, entry);
    }

    // Keep names you typed yourself in the overlay.
    fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    let file = dir.join("skills.json");
    let mut kept = 0;
    if let Ok(old) = fs::read(&file) {
        if let Ok(old) = serde_json::from_slice::<Value>(&old) {
            for (id, e) in old["skills"].as_object().into_iter().flatten() {
                if e["custom"].as_bool() == Some(true) {
                    if let Ok(num) = id.parse::<u32>() {
                        let entry = skills.entry(num).or_insert_with(|| json!({}));
                        entry["name"] = e["name"].clone();
                        entry["custom"] = json!(true);
                        kept += 1;
                    }
                }
            }
        }
    }

    let mut fetched = 0;
    let mut failed = 0;
    let mut wanted: Vec<String> = wanted.into_iter().collect();
    wanted.sort();
    for (i, f) in wanted.iter().enumerate() {
        let path = dir.join(f);
        if path.is_file() {
            continue;
        }
        let url = format!("https://raw.githubusercontent.com/{REPO}/{sha}/{ICONS_DIR}/{f}");
        match get_bytes(&agent, &url, MAX_ICON) {
            Ok(bytes) if bytes.starts_with(b"\x89PNG") => {
                fs::write(&path, bytes)?;
                fetched += 1;
            }
            _ => failed += 1,
        }
        if (i + 1) % 50 == 0 {
            println!("  icons {}/{}", i + 1, wanted.len());
        }
    }

    let out = json!({
        "source": {
            "project": format!("https://github.com/{REPO}"),
            "commit": sha,
            "license": "GPL-3.0",
            "language": lang,
            "note": "Imported with packet-lab import-skills. Skill names and icons are from the NOIA2 project; game names and art belong to NCSoft.",
        },
        "skills": skills,
    });
    let tmp = dir.join("skills.json.tmp");
    fs::write(&tmp, serde_json::to_vec_pretty(&out)?)?;
    fs::rename(&tmp, &file)?;

    let with_icon = out["skills"]
        .as_object()
        .map(|m| m.values().filter(|e| e.get("icon").is_some()).count())
        .unwrap_or(0);
    println!(
        "{} skill names ({lang}), {with_icon} with icons; {fetched} icons downloaded, {failed} failed, {kept} of your own names kept",
        out["skills"].as_object().map_or(0, |m| m.len())
    );
    println!("saved to {}", dir.display());
    println!("restart the overlay to load them");
    Ok(())
}

/// NOIA2 names icons after 6-digit IDs as-is, otherwise after the first four digits.
fn icon_file(id: &str) -> Option<String> {
    let code = match id.len() {
        6 => id,
        n if n >= 4 => &id[..4],
        _ => return None,
    };
    Some(format!("{code}.png"))
}

fn get_bytes(agent: &ureq::Agent, url: &str, limit: u64) -> Result<Vec<u8>> {
    let resp = agent
        .get(url)
        .set("Accept", "application/vnd.github+json")
        .call()
        .with_context(|| format!("downloading {url}"))?;
    let mut bytes = Vec::new();
    resp.into_reader().take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        bail!("{url} is larger than expected");
    }
    Ok(bytes)
}

fn get_json(agent: &ureq::Agent, url: &str) -> Result<Value> {
    let bytes = get_bytes(agent, url, MAX_JSON)?;
    serde_json::from_slice(&bytes).with_context(|| format!("{url} isn't valid JSON"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icon_names_follow_noia2_convention() {
        assert_eq!(icon_file("18010340").as_deref(), Some("1801.png"));
        assert_eq!(icon_file("100010").as_deref(), Some("100010.png"));
        assert_eq!(icon_file("1216760").as_deref(), Some("1216.png"));
        assert_eq!(icon_file("17"), None);
    }
}
