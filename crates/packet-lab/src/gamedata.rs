//! `packet-lab update-data`: fills the overlay's `gamedata` folder from two open-source
//! (GPL-3.0) Aion 2 meters:
//!
//! - NOIA2 (https://github.com/ZDYoung0519/NOIA2): skill names and icons, class icons,
//!   boss lists, and fallback mob names
//! - RATmeter (https://github.com/Kuroukihime/AIon2-Dps-Meter): English mob names for the
//!   global client, with boss and training-dummy flags
//!
//! ```text
//! gamedata/
//!   skills.json        skill names (keyed by the skill IDs the game sends)
//!   1801.png …         skill icons (named after the first four digits of the ID)
//!   npcs.json          English mob/boss names, boss and dummy flags
//!   class_11.webp …    class icons, by class code
//! ```
//!
//! Runs only when you ask (from a terminal or "update game data" in the overlay's settings). The
//! folder sits next to the programs inside `target/`, is never committed, and the overlay
//! itself never goes online. Names you typed yourself in the overlay are kept.

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};

const REPO: &str = "ZDYoung0519/NOIA2";
const BRANCH: &str = "main";
const SKILL_NAMES_DIR: &str = "src/i18n/locales/aion2skills";
const SKILL_ICONS_DIR: &str = "public/aion2/skill";
const NPC_NAMES: &str = "npc_names.json";
const MOBS: &str = "src/games/aion2/data/mobs.json";
const BOSS_IDS: &str = "src-tauri/data/boss_ids.json";
const CLASS_ICONS_DIR: &str = "public/aion2/class";
const RAT_REPO: &str = "Kuroukihime/AIon2-Dps-Meter";
const RAT_BRANCH: &str = "master";
const RAT_MOBS: &str = "AionDpsMeter.Core/GameData/Assets/mobs.json";
/// Class code (first two digits of a class's skill IDs) → NOIA2's class icon name.
const CLASS_ICONS: [(u32, &str); 9] = [
    (11, "gladiator"),
    (12, "templar"),
    (13, "assassin"),
    (14, "ranger"),
    (15, "sorcerer"),
    (16, "elementalist"),
    (17, "cleric"),
    (18, "chanter"),
    (19, "fighter"),
];
const LANGUAGES: [&str; 4] = ["en", "ko", "zh-TW", "zh-CN"];
const USER_AGENT: &str = concat!("glass-cannon-update/", env!("CARGO_PKG_VERSION"));
const MAX_JSON: u64 = 8 << 20;
const MAX_ICON: u64 = 256 << 10;

/// A `gamedata` folder next to this program, where the overlay (built into the same
/// folder) looks first.
pub fn default_dir() -> Result<PathBuf> {
    let exe = std::env::current_exe().context("finding this program's folder")?;
    let dir = exe.parent().context("this program has no folder")?;
    Ok(dir.join("gamedata"))
}

struct Source {
    agent: ureq::Agent,
    sha: String,
    rat_sha: String,
}

impl Source {
    fn raw(&self, path: &str) -> String {
        format!("https://raw.githubusercontent.com/{REPO}/{}/{path}", self.sha)
    }

    fn rat_raw(&self, path: &str) -> String {
        format!("https://raw.githubusercontent.com/{RAT_REPO}/{}/{path}", self.rat_sha)
    }
}

fn latest_commit(agent: &ureq::Agent, repo: &str, branch: &str) -> Result<String> {
    let commit = get_json(agent, &format!("https://api.github.com/repos/{repo}/commits/{branch}"))?;
    Ok(commit["sha"].as_str().context("no commit SHA in GitHub's reply")?.to_string())
}

pub fn run(dir: &Path, lang: &str, with_icons: bool) -> Result<()> {
    if !LANGUAGES.contains(&lang) {
        bail!("language must be one of {}", LANGUAGES.join(", "));
    }
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(30))
        .user_agent(USER_AGENT)
        .build();
    // Pin one commit per project so everything comes from a consistent snapshot.
    let sha = latest_commit(&agent, REPO, BRANCH)?;
    let rat_sha = latest_commit(&agent, RAT_REPO, RAT_BRANCH)?;
    println!("progress: NOIA2 {}, RATmeter {}", &sha[..12], &rat_sha[..12]);
    let src = Source { agent, sha, rat_sha };

    fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    let files: HashSet<String> = if with_icons { repo_files(&src)? } else { HashSet::new() };

    let skills = update_skills(&src, dir, lang, &files)?;
    let npcs = update_npcs(&src, dir)?;
    let classes = if with_icons { update_class_icons(&src, dir, &files)? } else { 0 };

    println!("done: {skills} skill names, {npcs} mobs and bosses, {classes} class icons");
    println!("saved to {}", dir.display());
    Ok(())
}

fn repo_files(src: &Source) -> Result<HashSet<String>> {
    let tree = get_json(
        &src.agent,
        &format!("https://api.github.com/repos/{REPO}/git/trees/{}?recursive=1", src.sha),
    )?;
    Ok(tree["tree"]
        .as_array()
        .context("no file list in GitHub's reply")?
        .iter()
        .filter_map(|e| e["path"].as_str().map(str::to_string))
        .collect())
}

fn update_skills(src: &Source, dir: &Path, lang: &str, files: &HashSet<String>) -> Result<usize> {
    let names = match get_json(&src.agent, &src.raw(&format!("{SKILL_NAMES_DIR}/{lang}.json")))? {
        Value::Object(m) => m,
        _ => bail!("the {lang} skill table isn't a JSON object"),
    };
    let mut skills: BTreeMap<u32, Value> = BTreeMap::new();
    let mut wanted: HashSet<String> = HashSet::new();
    for (id, name) in &names {
        let (Ok(num), Some(name)) = (id.parse::<u32>(), name.as_str()) else {
            continue;
        };
        let icon = icon_file(id).filter(|f| files.contains(&format!("{SKILL_ICONS_DIR}/{f}")));
        let mut entry = json!({ "name": name });
        if let Some(f) = icon {
            wanted.insert(f.clone());
            entry["icon"] = json!(f);
        }
        skills.insert(num, entry);
    }
    let kept = keep_custom(&dir.join("skills.json"), "skills", &mut skills);

    let mut wanted: Vec<String> = wanted.into_iter().collect();
    wanted.sort();
    let (mut fetched, mut failed) = (0, 0);
    for (i, f) in wanted.iter().enumerate() {
        let path = dir.join(f);
        if !path.is_file() {
            match get_bytes(&src.agent, &src.raw(&format!("{SKILL_ICONS_DIR}/{f}")), MAX_ICON) {
                Ok(b) if b.starts_with(b"\x89PNG") => {
                    fs::write(&path, b)?;
                    fetched += 1;
                }
                _ => failed += 1,
            }
        }
        if (i + 1) % 50 == 0 || i + 1 == wanted.len() {
            println!("progress: skill icons {}/{}", i + 1, wanted.len());
        }
    }
    let count = skills.len();
    write_json(
        &dir.join("skills.json"),
        &json!({ "source": source_note(src, lang), "skills": skills }),
    )?;
    println!("progress: {count} skill names ({lang}); {fetched} new icons, {failed} failed, {kept} of your own names kept");
    Ok(count)
}

fn update_npcs(src: &Source, dir: &Path) -> Result<usize> {
    println!("progress: mob and boss names");
    let rat = get_json(&src.agent, &src.rat_raw(RAT_MOBS))?;
    let names = get_json(&src.agent, &src.raw(NPC_NAMES))?;
    let mobs = get_json(&src.agent, &src.raw(MOBS))?;
    let boss_ids = get_json(&src.agent, &src.raw(BOSS_IDS))?;

    let mut npcs: BTreeMap<u32, Value> = BTreeMap::new();
    // RATmeter first: English names from the global client.
    for (id, v) in rat.as_object().into_iter().flatten() {
        let Ok(num) = id.parse::<u32>() else { continue };
        let mut entry = json!({ "name": v["name"].as_str().unwrap_or("") });
        if v["isBoss"].as_bool() == Some(true) {
            entry["boss"] = json!(true);
        }
        if v["isDummy"].as_bool() == Some(true) {
            entry["dummy"] = json!(true);
        }
        npcs.insert(num, entry);
    }
    // NOIA2's English names fill any gaps.
    for (id, v) in names.as_object().into_iter().flatten() {
        if let (Ok(num), Some(en)) = (id.parse::<u32>(), v["en"].as_str()) {
            let entry = npcs.entry(num).or_insert_with(|| json!({ "name": "" }));
            if entry["name"].as_str().is_none_or(str::is_empty) {
                entry["name"] = json!(en);
            }
        }
    }
    let mut bosses: HashSet<u32> = boss_ids["boss_ids"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_u64().map(|n| n as u32))
        .collect();
    for m in mobs.as_array().into_iter().flatten() {
        if m["boss"].as_bool() == Some(true) {
            if let Some(code) = m["code"].as_u64() {
                bosses.insert(code as u32);
            }
        }
    }
    for id in &bosses {
        npcs.entry(*id).or_insert_with(|| json!({ "name": "" }))["boss"] = json!(true);
    }
    let kept = keep_custom(&dir.join("npcs.json"), "npcs", &mut npcs);
    let named = npcs.values().filter(|v| v["name"].as_str().is_some_and(|s| !s.is_empty())).count();
    let count = npcs.len();
    let mut source = source_note(src, "en");
    source["mob_names"] = json!({
        "project": format!("https://github.com/{RAT_REPO}"),
        "commit": src.rat_sha,
        "license": "GPL-3.0",
    });
    write_json(&dir.join("npcs.json"), &json!({ "source": source, "npcs": npcs }))?;
    println!("progress: {named} English mob names, {} bosses, {kept} of your own names kept", bosses.len());
    Ok(count)
}

fn update_class_icons(src: &Source, dir: &Path, files: &HashSet<String>) -> Result<usize> {
    let mut n = 0;
    for (code, name) in CLASS_ICONS {
        let remote = format!("{CLASS_ICONS_DIR}/{name}.webp");
        if ["webp", "png"].iter().any(|ext| dir.join(format!("class_{code}.{ext}")).is_file()) {
            n += 1;
            continue;
        }
        if !files.contains(&remote) {
            continue;
        }
        if let Ok(b) = get_bytes(&src.agent, &src.raw(&remote), MAX_ICON) {
            // Saved under its real format: one of NOIA2's ".webp" icons is a PNG.
            let ext = if b.starts_with(b"RIFF") {
                "webp"
            } else if b.starts_with(&[0x89, b'P', b'N', b'G']) {
                "png"
            } else {
                continue;
            };
            fs::write(dir.join(format!("class_{code}.{ext}")), b)?;
            n += 1;
        }
    }
    println!("progress: {n} class icons");
    Ok(n)
}

/// Copies entries you named yourself (`"custom": true`) from the existing file.
fn keep_custom(file: &Path, key: &str, into: &mut BTreeMap<u32, Value>) -> usize {
    let Some(old) = fs::read(file).ok().and_then(|b| serde_json::from_slice::<Value>(&b).ok()) else {
        return 0;
    };
    let mut kept = 0;
    for (id, e) in old[key].as_object().into_iter().flatten() {
        if e["custom"].as_bool() == Some(true) {
            if let Ok(num) = id.parse::<u32>() {
                let entry = into.entry(num).or_insert_with(|| json!({}));
                entry["name"] = e["name"].clone();
                entry["custom"] = json!(true);
                kept += 1;
            }
        }
    }
    kept
}

fn source_note(src: &Source, lang: &str) -> Value {
    json!({
        "project": format!("https://github.com/{REPO}"),
        "commit": src.sha,
        "license": "GPL-3.0",
        "language": lang,
        "note": "Imported with packet-lab update-data. Data from the NOIA2 project; game names and art belong to NCSoft.",
    })
}

fn write_json(path: &Path, v: &Value) -> Result<()> {
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, serde_json::to_vec_pretty(v)?)?;
    fs::rename(&tmp, path)?;
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
        assert_eq!(icon_file("17"), None);
    }

    #[test]
    fn custom_names_survive() {
        let dir = std::env::temp_dir().join(format!("gamedata-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let f = dir.join("npcs.json");
        fs::write(&f, r#"{ "npcs": { "2400009": { "name": "Mine", "custom": true }, "5": { "name": "x" } } }"#).unwrap();
        let mut into = BTreeMap::new();
        into.insert(2_400_009, json!({ "name": "Imported" }));
        assert_eq!(keep_custom(&f, "npcs", &mut into), 1);
        assert_eq!(into[&2_400_009]["name"], "Mine");
        assert!(!into.contains_key(&5));
        fs::remove_dir_all(dir).ok();
    }
}
