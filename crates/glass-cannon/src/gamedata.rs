//! Skill names and icons, mob/boss names and class icons, from the first of these folders
//! that has them: `gamedata` next to the program, `skills` next to the program (older
//! layout), then the app's settings folder.
//!
//! ```text
//! <folder>\
//!   skills.json    { "skills": { "18010340": { "name": "…", "icon": "1801.png" } } }
//!   npcs.json      { "npcs":   { "2400009":  { "name": "…", "boss": true } } }
//!   1801.png …     skill icons
//!   class_18.webp  class icons, by class code
//! ```
//!
//! The release ships this folder; `packet-lab update-data` builds it. Names you type in
//! the overlay are kept apart, in `names.json` in the settings folder, so a new release's
//! data never overwrites them. The app itself never downloads anything.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use meter_core::meter::{EncounterView, Snapshot};
use serde::{de::DeserializeOwned, Deserialize, Serialize};

const SKILLS_FILE: &str = "skills.json";
const NPCS_FILE: &str = "npcs.json";
const MAX_ICON_BYTES: u64 = 256 * 1024;
const MAX_NAME_CHARS: usize = 60;
/// How often an empty catalog tries the files again.
const RELOAD_EVERY: Duration = Duration::from_secs(5);

#[derive(Debug, Default, Serialize, Deserialize)]
struct SkillFile {
    /// Where imported data came from (kept as-is).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    source: Option<serde_json::Value>,
    #[serde(default)]
    skills: BTreeMap<u32, Entry>,
}

/// Your own names for skills and mob types (`names.json` in the settings folder).
#[derive(Debug, Default, Serialize, Deserialize)]
struct Names {
    #[serde(default)]
    skills: BTreeMap<u32, String>,
    #[serde(default)]
    npcs: BTreeMap<u32, String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct NpcFile {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    source: Option<serde_json::Value>,
    #[serde(default)]
    npcs: BTreeMap<u32, Entry>,
}

/// One skill or one mob type.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Entry {
    #[serde(default)]
    name: String,
    /// File name of an icon in the same folder (skills only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    icon: Option<String>,
    /// A boss (mobs only).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    boss: bool,
    /// A training dummy (mobs only).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    dummy: bool,
    /// Named by you in the overlay (older data folders kept your names in here).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    custom: bool,
}

pub struct GameData {
    /// Folders to look in, in order.
    candidates: Vec<PathBuf>,
    /// The folder in use.
    dir: PathBuf,
    /// Where your own names are saved.
    names_path: Option<PathBuf>,
    names: Names,
    skills: SkillFile,
    npcs: NpcFile,
    /// Why no skills are loaded (missing files, unreadable, bad JSON…).
    problem: Option<String>,
    last_attempt: Option<Instant>,
}

impl GameData {
    /// Loads from the first folder whose skills file has skills in it, with your own
    /// names (from `names_path`) on top.
    pub fn load(candidates: Vec<PathBuf>, names_path: Option<PathBuf>) -> Self {
        let mut d = Self {
            dir: candidates.first().cloned().unwrap_or_default(),
            candidates,
            names: names_path.as_deref().and_then(|p| read_json(p).ok()).unwrap_or_default(),
            names_path,
            skills: SkillFile::default(),
            npcs: NpcFile::default(),
            problem: None,
            last_attempt: None,
        };
        d.reload();
        d
    }

    #[cfg(test)]
    fn load_first(candidates: Vec<PathBuf>) -> Self {
        Self::load(candidates, None)
    }

    /// Reads the files again.
    pub fn reload(&mut self) {
        self.last_attempt = Some(Instant::now());
        let mut problems = Vec::new();
        for dir in &self.candidates {
            match read_json::<SkillFile>(&dir.join(SKILLS_FILE)) {
                Ok(f) if !f.skills.is_empty() => {
                    self.dir = dir.clone();
                    self.skills = f;
                    // Mob names are optional (older folders only have skills).
                    self.npcs = read_json::<NpcFile>(&dir.join(NPCS_FILE)).unwrap_or_default();
                    self.problem = None;
                    self.adopt_old_custom_names();
                    self.apply_names();
                    return;
                }
                Ok(_) => problems.push(format!("{} has no skills in it", dir.join(SKILLS_FILE).display())),
                Err(e) => problems.push(format!("{e:#}")),
            }
        }
        self.problem = Some(problems.join("; "));
    }

    /// Tries again (at most every few seconds) while nothing is loaded.
    pub fn reload_if_empty(&mut self) {
        if !self.skills.skills.is_empty() || self.candidates.is_empty() {
            return;
        }
        if self.last_attempt.is_some_and(|t| t.elapsed() < RELOAD_EVERY) {
            return;
        }
        self.reload();
    }

    pub fn skill_count(&self) -> usize {
        self.skills.skills.len()
    }

    /// Mob types with an English (or your own) name.
    pub fn named_npc_count(&self) -> usize {
        self.npcs.npcs.values().filter(|e| !e.name.is_empty()).count()
    }

    pub fn problem(&self) -> Option<&str> {
        self.problem.as_deref()
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// A skill's entry: its exact ID first, then its base skill (variants differ in the
    /// last digits, e.g. 11100048 -> 11100000).
    fn resolve_skill(&self, id: u32) -> Option<(u32, &Entry)> {
        [id, id - id % 100, id - id % 10_000]
            .into_iter()
            .find_map(|k| self.skills.skills.get(&k).map(|e| (k, e)))
    }

    /// Fills in skill names and icon keys, and the target's name and boss flag.
    pub fn annotate(&self, snap: &mut Snapshot) {
        if let Some(enc) = snap.encounter.as_mut() {
            self.annotate_view(enc);
        }
    }

    pub fn annotate_view(&self, enc: &mut EncounterView) {
        for row in enc.players.iter_mut().flat_map(|p| p.skills.iter_mut()) {
            if let Some((key, e)) = self.resolve_skill(row.skill) {
                if !e.name.is_empty() {
                    row.name = Some(e.name.clone());
                }
                if e.icon.is_some() {
                    row.icon_key = Some(key);
                }
            }
        }
        if let Some(e) = enc.target_type.and_then(|t| self.npcs.npcs.get(&t)) {
            if !e.name.is_empty() {
                enc.target = Some(e.name.clone());
            }
            enc.target_boss = e.boss;
        }
    }

    /// Names a skill (an empty name goes back to the data's name). Saved to names.json.
    pub fn rename_skill(&mut self, skill: u32, name: &str) -> Result<()> {
        let name = check_name(name)?;
        set_or_clear(&mut self.names.skills, skill, name);
        self.save_names()
    }

    /// Names a mob type. Saved to names.json.
    pub fn rename_npc(&mut self, type_id: u32, name: &str) -> Result<()> {
        let name = check_name(name)?;
        set_or_clear(&mut self.names.npcs, type_id, name);
        self.save_names()
    }

    fn save_names(&mut self) -> Result<()> {
        let Some(path) = &self.names_path else {
            bail!("no settings folder to save names in");
        };
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
        }
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, serde_json::to_vec_pretty(&self.names)?)?;
        fs::rename(&tmp, path)?;
        // Re-read the data so a cleared name falls back to the original.
        self.reload();
        Ok(())
    }

    /// Older versions saved your names inside the data files (marked `custom`). The first
    /// time there's no names.json, move them over so a new release's data can't lose them.
    fn adopt_old_custom_names(&mut self) {
        let Some(path) = &self.names_path else { return };
        if path.exists() {
            return;
        }
        let custom = |m: &BTreeMap<u32, Entry>| -> BTreeMap<u32, String> {
            m.iter()
                .filter(|(_, e)| e.custom && !e.name.is_empty())
                .map(|(id, e)| (*id, e.name.clone()))
                .collect()
        };
        let (skills, npcs) = (custom(&self.skills.skills), custom(&self.npcs.npcs));
        if skills.is_empty() && npcs.is_empty() {
            return;
        }
        self.names.skills.extend(skills);
        self.names.npcs.extend(npcs);
        if let Ok(json) = serde_json::to_vec_pretty(&self.names) {
            let _ = fs::write(path, json);
        }
    }

    /// Puts your names over the data's.
    fn apply_names(&mut self) {
        for (id, name) in &self.names.skills {
            let e = self.skills.skills.entry(*id).or_default();
            e.name = name.clone();
            e.custom = true;
        }
        for (id, name) in &self.names.npcs {
            let e = self.npcs.npcs.entry(*id).or_default();
            e.name = name.clone();
            e.custom = true;
        }
    }

    /// A skill's icon as a `data:` URL, so the page needs no file access.
    pub fn skill_icon(&self, skill: u32) -> Option<String> {
        let file = self.skills.skills.get(&skill)?.icon.as_deref()?;
        self.icon_data_url(file)
    }

    /// A class icon (class_<code>.webp or .png) as a `data:` URL.
    pub fn class_icon(&self, code: u32) -> Option<String> {
        ["webp", "png"]
            .iter()
            .find_map(|ext| self.icon_data_url(&format!("class_{code}.{ext}")))
    }

    fn icon_data_url(&self, file: &str) -> Option<String> {
        // Only plain file names inside the data folder.
        if file.contains(['/', '\\', ':']) || file.starts_with('.') {
            return None;
        }
        let mime = match Path::new(file).extension()?.to_str()?.to_ascii_lowercase().as_str() {
            "png" => "image/png",
            "webp" => "image/webp",
            "jpg" | "jpeg" => "image/jpeg",
            _ => return None,
        };
        let path = self.dir.join(file);
        if fs::metadata(&path).ok()?.len() > MAX_ICON_BYTES {
            return None;
        }
        Some(format!("data:{mime};base64,{}", base64(&fs::read(path).ok()?)))
    }
}

fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let bytes = fs::read(path).with_context(|| format!("can't read {}", path.display()))?;
    serde_json::from_slice(&bytes).with_context(|| format!("{} is not valid", path.display()))
}

fn check_name(name: &str) -> Result<&str> {
    let name = name.trim();
    if name.chars().count() > MAX_NAME_CHARS || name.chars().any(char::is_control) {
        bail!("names are up to {MAX_NAME_CHARS} characters, without line breaks");
    }
    Ok(name)
}

fn set_or_clear(map: &mut BTreeMap<u32, String>, id: u32, name: &str) {
    if name.is_empty() {
        map.remove(&id);
    } else {
        map.insert(id, name.to_string());
    }
}

fn base64(data: &[u8]) -> String {
    const ABC: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ABC[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let d = std::env::temp_dir().join(format!("glass-cannon-{tag}-{}-{nanos}", std::process::id()));
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn view_with(target_type: Option<u32>, skills: &[u32]) -> EncounterView {
        serde_json::from_value(serde_json::json!({
            "active": true, "duration_s": 1.0, "total_damage": 0, "total_dps": 0.0,
            "target": "Mob 2400009", "target_id": 1, "target_type": target_type, "target_count": 1,
            "start_us": 0, "end_us": 0,
            "players": [{
                "id": 1, "name": "x", "is_self": true, "class_code": 18, "class_name": "Chanter",
                "damage": 0, "dps": 0.0, "share": 0.0, "hits": 0, "max_hit": 0,
                "skills": skills.iter().map(|s| serde_json::json!({
                    "skill": s, "name": null, "icon_key": null, "damage": 0, "hits": 0, "max": 0, "share": 0.0
                })).collect::<Vec<_>>()
            }]
        }))
        .unwrap()
    }

    #[test]
    fn base64_matches_known_values() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn annotates_skills_target_and_boss() {
        let dir = temp_dir("annotate");
        fs::write(dir.join("1110.png"), b"png").unwrap();
        fs::write(
            dir.join(SKILLS_FILE),
            r#"{ "skills": { "11100000": { "name": "Ruinous Blow", "icon": "1110.png" } } }"#,
        )
        .unwrap();
        fs::write(dir.join(NPCS_FILE), r#"{ "npcs": { "2400009": { "name": "Hardened Drana", "boss": true } } }"#).unwrap();
        let d = GameData::load_first(vec![dir.clone()]);
        let mut v = view_with(Some(2_400_009), &[11_100_048, 3_000_021]);
        d.annotate_view(&mut v);
        assert_eq!(v.players[0].skills[0].name.as_deref(), Some("Ruinous Blow"));
        assert_eq!(v.players[0].skills[0].icon_key, Some(11_100_000));
        assert_eq!(v.players[0].skills[1].name, None);
        assert_eq!(v.target.as_deref(), Some("Hardened Drana"));
        assert!(v.target_boss);
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn first_folder_with_skills_wins_and_problems_are_reported() {
        let missing = std::env::temp_dir().join("glass-cannon-definitely-missing");
        let good = temp_dir("good");
        fs::write(good.join(SKILLS_FILE), r#"{ "skills": { "7": { "name": "Seven" } } }"#).unwrap();
        let d = GameData::load_first(vec![missing.clone(), good.clone()]);
        assert_eq!(d.skill_count(), 1);
        assert_eq!(d.dir(), good.as_path());
        assert!(d.problem().is_none());
        let none = GameData::load_first(vec![missing]);
        assert!(none.problem().unwrap().contains("can't read"));
        fs::remove_dir_all(good).ok();
    }

    #[test]
    fn your_names_live_apart_from_the_data() {
        let dir = temp_dir("rename");
        let names = dir.join("settings").join("names.json");
        let skills = r#"{ "source": { "x": 1 }, "skills": { "5": { "name": "Imported" } } }"#;
        fs::write(dir.join(SKILLS_FILE), skills).unwrap();
        let mut d = GameData::load(vec![dir.clone()], Some(names.clone()));
        d.rename_skill(5, "Mine").unwrap();
        d.rename_npc(2_400_009, "Drana").unwrap();
        // The data file is untouched; the names survive a fresh copy of it.
        assert_eq!(fs::read_to_string(dir.join(SKILLS_FILE)).unwrap(), skills);
        let again = GameData::load(vec![dir.clone()], Some(names.clone()));
        assert_eq!(again.named_npc_count(), 1);
        assert_eq!(again.skills.skills[&5].name, "Mine");
        // Clearing goes back to the data's name.
        d.rename_skill(5, "").unwrap();
        assert_eq!(d.skills.skills[&5].name, "Imported");
        assert!(d.rename_skill(1, "line\nbreak").is_err());
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn old_custom_names_move_to_names_json() {
        let dir = temp_dir("adopt");
        let names = dir.join("names.json");
        fs::write(
            dir.join(SKILLS_FILE),
            r#"{ "skills": { "5": { "name": "Typed", "custom": true }, "6": { "name": "Data" } } }"#,
        )
        .unwrap();
        GameData::load(vec![dir.clone()], Some(names.clone()));
        let saved: Names = read_json(&names).unwrap();
        assert_eq!(saved.skills.get(&5).map(String::as_str), Some("Typed"));
        assert!(!saved.skills.contains_key(&6));
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn icon_paths_are_confined() {
        let dir = temp_dir("icons");
        fs::write(dir.join("class_18.webp"), b"RIFF").unwrap();
        fs::write(dir.join(SKILLS_FILE), r#"{ "skills": { "1": { "name": "One", "icon": "..\\x.png" } } }"#).unwrap();
        let d = GameData::load_first(vec![dir.clone()]);
        assert_eq!(d.skill_icon(1), None);
        assert_eq!(d.class_icon(18).as_deref(), Some("data:image/webp;base64,UklGRg=="));
        assert_eq!(d.class_icon(11), None);
        fs::remove_dir_all(dir).ok();
    }
}
