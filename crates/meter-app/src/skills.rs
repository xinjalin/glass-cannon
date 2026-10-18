//! Skill names and icons, from the first of these folders that has them:
//! a `skills` folder next to the program, then the app's settings folder.
//!
//! ```text
//! <folder>\
//!   skills.json      { "skills": { "18010340": { "name": "…", "icon": "18010340.png" } } }
//!   18010340.png     icons, referenced by file name
//! ```
//!
//! `packet-lab import-skills` fills it; names you type in the overlay are saved to the
//! same file (marked `custom`, so a re-import keeps them). The overlay never downloads.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use meter_core::meter::Snapshot;
use serde::{Deserialize, Serialize};

const FILE: &str = "skills.json";
const MAX_ICON_BYTES: u64 = 256 * 1024;
const MAX_NAME_CHARS: usize = 60;
/// How often an empty catalog tries the file again.
const RELOAD_EVERY: Duration = Duration::from_secs(5);

#[derive(Debug, Default, Serialize, Deserialize)]
struct SkillFile {
    /// Where imported data came from (kept as-is).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    source: Option<serde_json::Value>,
    #[serde(default)]
    skills: BTreeMap<u32, SkillEntry>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct SkillEntry {
    #[serde(default)]
    name: String,
    /// File name of an icon in the same folder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    icon: Option<String>,
    /// Named by you in the overlay.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    custom: bool,
}

pub struct Catalog {
    /// Folders to look in, in order.
    candidates: Vec<PathBuf>,
    /// The folder in use (where renames are saved).
    dir: PathBuf,
    file: SkillFile,
    /// Why the last load found no skills (missing file, unreadable, bad JSON…).
    problem: Option<String>,
    last_attempt: Option<Instant>,
}

impl Catalog {
    /// No skills and nowhere to save yet; replaced by [`Catalog::load`] at startup.
    pub fn empty() -> Self {
        Self {
            candidates: Vec::new(),
            dir: PathBuf::new(),
            file: SkillFile::default(),
            problem: Some("the app is still starting".into()),
            last_attempt: None,
        }
    }

    /// Loads from a single folder.
    #[cfg(test)]
    pub fn load(dir: PathBuf) -> Self {
        Self::load_first(vec![dir])
    }

    /// Loads from the first folder whose skills file has skills in it.
    pub fn load_first(candidates: Vec<PathBuf>) -> Self {
        let mut c = Self {
            dir: candidates.last().cloned().unwrap_or_default(),
            candidates,
            file: SkillFile::default(),
            problem: None,
            last_attempt: None,
        };
        c.reload();
        c
    }

    fn reload(&mut self) {
        self.last_attempt = Some(Instant::now());
        let mut problems = Vec::new();
        for dir in &self.candidates {
            let path = dir.join(FILE);
            let loaded = fs::read(&path)
                .map_err(|e| format!("can't read {}: {e}", path.display()))
                .and_then(|b| {
                    serde_json::from_slice::<SkillFile>(&b)
                        .map_err(|e| format!("{} is not valid: {e}", path.display()))
                });
            match loaded {
                Ok(f) if !f.skills.is_empty() => {
                    self.dir = dir.clone();
                    self.file = f;
                    self.problem = None;
                    return;
                }
                Ok(_) => problems.push(format!("{} has no skills in it", path.display())),
                Err(e) => problems.push(e),
            }
        }
        self.problem = Some(problems.join("; "));
    }

    /// Tries the file again (at most every few seconds) while no skills are loaded,
    /// so a late import or a slow start fixes itself.
    pub fn reload_if_empty(&mut self) {
        if !self.file.skills.is_empty() || self.candidates.is_empty() {
            return;
        }
        if self.last_attempt.is_some_and(|t| t.elapsed() < RELOAD_EVERY) {
            return;
        }
        self.reload();
    }

    /// Why no skills are loaded, if that's the case.
    pub fn problem(&self) -> Option<&str> {
        self.problem.as_deref()
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn len(&self) -> usize {
        self.file.skills.len()
    }

    /// The entry for a skill: its exact ID first, then its base skill (variants differ
    /// in the last digits, e.g. 11100048 -> 11100000).
    fn resolve(&self, id: u32) -> Option<(u32, &SkillEntry)> {
        [id, id - id % 100, id - id % 10_000]
            .into_iter()
            .find_map(|k| self.file.skills.get(&k).map(|e| (k, e)))
    }

    /// Fills in names and icon keys on every skill row of a snapshot.
    pub fn annotate(&self, snap: &mut Snapshot) {
        let Some(enc) = snap.encounter.as_mut() else { return };
        for row in enc.players.iter_mut().flat_map(|p| p.skills.iter_mut()) {
            // A name you typed for this exact ID wins over a base-skill fallback.
            if let Some((key, e)) = self.resolve(row.skill) {
                if !e.name.is_empty() {
                    row.name = Some(e.name.clone());
                }
                if e.icon.is_some() {
                    row.icon_key = Some(key);
                }
            }
        }
    }

    /// The skill's icon as a `data:` URL, so the page needs no file access.
    pub fn icon(&self, skill: u32) -> Option<String> {
        let file = self.file.skills.get(&skill)?.icon.as_deref()?;
        self.icon_data_url(file)
    }

    /// Sets (or with an empty name, clears) a skill's name and saves the file.
    pub fn rename(&mut self, skill: u32, name: &str) -> Result<()> {
        let name = name.trim();
        if name.chars().count() > MAX_NAME_CHARS || name.chars().any(char::is_control) {
            bail!("skill names are up to {MAX_NAME_CHARS} characters, without line breaks");
        }
        if name.is_empty() {
            match self.file.skills.get_mut(&skill) {
                // Keep an imported icon; the next import restores the imported name.
                Some(e) if e.icon.is_some() => {
                    e.name.clear();
                    e.custom = false;
                }
                _ => {
                    self.file.skills.remove(&skill);
                }
            }
        } else {
            let e = self.file.skills.entry(skill).or_default();
            e.name = name.to_string();
            e.custom = true;
        }
        self.save()
    }

    fn save(&self) -> Result<()> {
        if self.dir.as_os_str().is_empty() {
            bail!("the skills folder isn't ready yet; try again in a moment");
        }
        fs::create_dir_all(&self.dir).with_context(|| format!("creating {}", self.dir.display()))?;
        let tmp = self.dir.join(format!("{FILE}.tmp"));
        fs::write(&tmp, serde_json::to_vec_pretty(&self.file)?)?;
        fs::rename(&tmp, self.dir.join(FILE))?;
        Ok(())
    }

    fn icon_data_url(&self, file: &str) -> Option<String> {
        // Only plain file names inside the skills folder.
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
        std::env::temp_dir().join(format!("meter-app-{tag}-{}-{nanos}", std::process::id()))
    }

    #[test]
    fn base64_matches_known_values() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn rename_saves_and_reloads() {
        let dir = temp_dir("skills");
        let mut c = Catalog::load(dir.clone());
        assert_eq!(c.len(), 0);
        c.rename(18_010_340, "  Cleave  ").unwrap();
        let again = Catalog::load(dir.clone());
        assert_eq!(again.resolve(18_010_340).unwrap().1.name, "Cleave");
        let saved = fs::read_to_string(dir.join(FILE)).unwrap();
        assert!(saved.contains("\"custom\": true"));
        let mut again = again;
        again.rename(18_010_340, "").unwrap();
        assert_eq!(Catalog::load(dir.clone()).len(), 0);
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn icons_load_as_data_urls_and_paths_are_confined() {
        let dir = temp_dir("icons");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("a.png"), b"png!").unwrap();
        fs::write(
            dir.join(FILE),
            r#"{ "source": { "project": "test" }, "skills": {
                "1": { "name": "One", "icon": "a.png" },
                "2": { "name": "Two", "icon": "..\\secret.png" },
                "3": { "name": "Three", "icon": "missing.png" }
            } }"#,
        )
        .unwrap();
        let c = Catalog::load(dir.clone());
        assert!(c.resolve(1).unwrap().1.icon.is_some());
        assert_eq!(c.icon(1).as_deref(), Some("data:image/png;base64,cG5nIQ=="));
        assert_eq!(c.icon(2), None);
        assert_eq!(c.icon(3), None);
        assert_eq!(c.icon(4), None);
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn rejects_bad_names() {
        let mut c = Catalog::load(temp_dir("bad"));
        assert!(c.rename(1, "line\nbreak").is_err());
        assert!(c.rename(1, &"x".repeat(61)).is_err());
    }

    #[test]
    fn rename_keeps_import_source() {
        let dir = std::env::temp_dir().join(format!("meter-app-source-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join(FILE),
            r#"{ "source": { "project": "x" }, "skills": { "5": { "name": "Imported" } } }"#,
        )
        .unwrap();
        let mut c = Catalog::load(dir.clone());
        c.rename(5, "Mine").unwrap();
        let saved = fs::read_to_string(dir.join(FILE)).unwrap();
        assert!(saved.contains("\"project\": \"x\""));
        assert!(saved.contains("\"Mine\""));
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn variants_fall_back_to_their_base_skill() {
        let dir = temp_dir("variants");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("1110.png"), b"png").unwrap();
        fs::write(
            dir.join(FILE),
            r#"{ "skills": {
                "11100000": { "name": "Ruinous Blow", "icon": "1110.png" },
                "11100140": { "name": "Ruinous Blow", "icon": "1110.png" }
            } }"#,
        )
        .unwrap();
        let c = Catalog::load(dir.clone());
        assert_eq!(c.resolve(11_100_140).unwrap().0, 11_100_140);
        assert_eq!(c.resolve(11_100_048).unwrap().0, 11_100_000);
        assert!(c.resolve(3_000_021).is_none());
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn first_folder_with_skills_wins() {
        let missing = temp_dir("missing");
        let empty = temp_dir("emptyfile");
        let good = temp_dir("good");
        fs::create_dir_all(&empty).unwrap();
        fs::create_dir_all(&good).unwrap();
        fs::write(empty.join(FILE), r#"{ "skills": {} }"#).unwrap();
        fs::write(good.join(FILE), r#"{ "skills": { "7": { "name": "Seven" } } }"#).unwrap();

        let c = Catalog::load_first(vec![missing.clone(), empty.clone(), good.clone()]);
        assert_eq!(c.len(), 1);
        assert_eq!(c.dir(), good.as_path());
        assert!(c.problem().is_none());

        let none = Catalog::load_first(vec![missing.clone(), empty.clone()]);
        assert_eq!(none.len(), 0);
        let p = none.problem().unwrap();
        assert!(p.contains("can't read") && p.contains("has no skills"));
        fs::remove_dir_all(empty).ok();
        fs::remove_dir_all(good).ok();
    }
}
