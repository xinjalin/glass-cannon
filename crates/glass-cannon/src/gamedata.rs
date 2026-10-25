//! Skill names and icons, mob/boss names, class icons and skill details, from the first of
//! these folders that has them: `gamedata` next to the program, `skills` next to the
//! program (older layout), the app's settings folder, then the repository's `gamedata`
//! (for a build in `target/release`).
//!
//! ```text
//! <folder>\
//!   skills.json    { "skills": { "18010340": { "name": "…", "icon": "1801.png" } } }
//!   npcs.json      { "npcs":   { "2400009":  { "name": "…", "boss": true } } }
//!   npc-names.json { "npcs":   { "2950865": "Elvida" } }   NPCs the mob lists don't name
//!   1801.png …     skill icons
//!   class_18.webp  class icons, by class code
//!   skill-details.json  { "skills": { "18010000": { "cooldown_s": 15, "specs": […] } } }
//! ```
//!
//! The release ships this folder (the repository's `gamedata`); `packet-lab update-data`
//! and `packet-lab skill-details` build it. Names you type in
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
const DETAILS_FILE: &str = "skill-details.json";
const THEOSTONES_FILE: &str = "theostones.json";
const DUNGEONS_FILE: &str = "dungeons.json";
const PETS_FILE: &str = "pets.json";
const NPC_NAMES_FILE: &str = "npc-names.json";
const ITEM_ICONS_FILE: &str = "item-icons.json";
/// Player skills have 8-digit IDs; mobs' are shorter.
const PLAYER_SKILL_MIN: u32 = 10_000_000;
const FIELD_BOSSES_FILE: &str = "field-bosses.json";

/// Field bosses by their world-map ID: map, mob type, level and where (field-bosses.json).
#[derive(Debug, Default, Deserialize)]
struct FieldBossFile {
    #[serde(default)]
    maps: BTreeMap<u32, String>,
    #[serde(default)]
    bosses: BTreeMap<u32, FieldBossInfo>,
}

#[derive(Debug, Deserialize)]
struct FieldBossInfo {
    npc: u32,
    level: Option<u32>,
    #[serde(rename = "where")]
    place: Option<String>,
}
const MAX_ICON_BYTES: u64 = 256 * 1024;
const MAX_NAME_CHARS: usize = 60;
/// Theostone (item) skills as the game sends them: 7 digits starting 30.
const THEOSTONE_SKILLS: std::ops::RangeInclusive<u32> = 3_000_000..=3_099_999;
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

/// Skill details by base skill ID, kept as they are in the file (the page shows them).
#[derive(Debug, Default, Deserialize)]
struct DetailsFile {
    #[serde(default)]
    skills: BTreeMap<u32, serde_json::Value>,
}

/// What each Theostone does, by the skill ID the game sends for it.
#[derive(Debug, Default, Deserialize)]
struct TheostoneFile {
    #[serde(default)]
    theostones: BTreeMap<u32, Theostone>,
}

/// Icons for consumables' buffs (scrolls, potions, food), by skill ID.
#[derive(Debug, Default, Deserialize)]
struct ItemIconFile {
    #[serde(default)]
    icons: BTreeMap<u32, String>,
}

/// NPCs' names (quest givers, merchants…) by type, for those the mob lists don't name
/// (optional).
#[derive(Debug, Default, Deserialize)]
struct NpcNameFile {
    #[serde(default)]
    npcs: BTreeMap<u32, String>,
}

/// Pets' names, by the ID their souls come with (optional).
#[derive(Debug, Default, Deserialize)]
struct PetFile {
    #[serde(default)]
    pets: BTreeMap<u32, PetEntry>,
}

#[derive(Debug, Deserialize)]
struct PetEntry {
    name: String,
}

/// Which dungeon (and mode) each dungeon mob type is in.
#[derive(Debug, Default, Deserialize)]
struct DungeonFile {
    #[serde(default)]
    dungeons: BTreeMap<String, DungeonEntry>,
    #[serde(default)]
    mobs: BTreeMap<u32, DungeonMob>,
}

#[derive(Debug, Deserialize)]
struct DungeonEntry {
    name: String,
    #[serde(rename = "type")]
    kind: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DungeonMob {
    dungeon: String,
    mode: Option<String>,
}

/// Where a mob type is: its dungeon (ID and name), the dungeon's type and the mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DungeonPlace<'a> {
    pub id: &'a str,
    pub name: &'a str,
    pub kind: Option<&'a str>,
    pub mode: Option<&'a str>,
}

#[derive(Debug, Clone, Deserialize)]
struct Theostone {
    grade: Option<String>,
    effect: String,
    cooldown_s: Option<f64>,
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
    /// Cooldowns, costs, descriptions and specialties by base skill ID (optional).
    details: DetailsFile,
    /// Theostone effects and grades (optional).
    theostones: TheostoneFile,
    /// Dungeon mobs, for the history's dungeon runs (optional).
    dungeons: DungeonFile,
    /// Field bosses on the world map (optional).
    field_bosses: FieldBossFile,
    /// Pets' names (optional).
    pets: PetFile,
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
            details: DetailsFile::default(),
            theostones: TheostoneFile::default(),
            dungeons: DungeonFile::default(),
            field_bosses: FieldBossFile::default(),
            pets: PetFile::default(),
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
                    // Mob names and skill details are optional (older folders only have skills).
                    self.npcs = read_json::<NpcFile>(&dir.join(NPCS_FILE)).unwrap_or_default();
                    self.details = read_json::<DetailsFile>(&dir.join(DETAILS_FILE)).unwrap_or_default();
                    self.theostones = read_json::<TheostoneFile>(&dir.join(THEOSTONES_FILE)).unwrap_or_default();
                    self.dungeons = read_json::<DungeonFile>(&dir.join(DUNGEONS_FILE)).unwrap_or_default();
                    self.field_bosses = read_json::<FieldBossFile>(&dir.join(FIELD_BOSSES_FILE)).unwrap_or_default();
                    self.pets = read_json::<PetFile>(&dir.join(PETS_FILE)).unwrap_or_default();
                    // NPCs' names fill in the types the mob lists don't name (your own names
                    // still go on top, below).
                    let npc_names = read_json::<NpcNameFile>(&dir.join(NPC_NAMES_FILE)).unwrap_or_default();
                    for (id, name) in npc_names.npcs {
                        let e = self.npcs.npcs.entry(id).or_default();
                        if e.name.is_empty() {
                            e.name = name;
                        }
                    }
                    // Consumables' icons fill in skills that have none.
                    let items = read_json::<ItemIconFile>(&dir.join(ITEM_ICONS_FILE)).unwrap_or_default();
                    for (id, file) in items.icons {
                        if let Some(e) = self.skills.skills.get_mut(&id) {
                            e.icon.get_or_insert(file);
                        }
                    }
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

    /// Skills with details (cooldown, cost, description, specialties) for the tooltips.
    pub fn detail_count(&self) -> usize {
        self.details.skills.len()
    }

    /// The dungeon a mob type belongs to, if any.
    pub fn dungeon(&self, mob_type: u32) -> Option<DungeonPlace<'_>> {
        let mob = self.dungeons.mobs.get(&mob_type)?;
        let (id, d) = self.dungeons.dungeons.get_key_value(&mob.dungeon)?;
        Some(DungeonPlace { id, name: &d.name, kind: d.kind.as_deref(), mode: mob.mode.as_deref() })
    }

    pub fn dungeon_mob_count(&self) -> usize {
        self.dungeons.mobs.len()
    }

    /// How many Theostones (by name) have their effect in the data.
    pub fn theostone_count(&self) -> usize {
        let effects: std::collections::HashSet<&str> = self.theostones.theostones.values().map(|t| t.effect.as_str()).collect();
        effects.len()
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
    /// last digits, e.g. 11100048 -> 11100000). Theostones come in as 7 digits (3001015)
    /// but the data lists them by their effect ID, one digit longer (30010151).
    fn resolve_skill(&self, id: u32) -> Option<(u32, &Entry)> {
        let theostone = THEOSTONE_SKILLS.contains(&id).then(|| id * 10 + 1);
        [Some(id), Some(id - id % 100), Some(id - id % 10_000), theostone]
            .into_iter()
            .flatten()
            .find_map(|k| self.skills.skills.get(&k).map(|e| (k, e)))
    }

    /// An effect's name and icon key, for buffs and debuffs that don't say which skill
    /// they came from. Effect IDs are a skill ID and one more digit (Recuperation 18120010
    /// → 181200101), and some are in the data as they are (Theostones).
    pub fn effect_meta(&self, effect: u32) -> (Option<String>, Option<u32>) {
        let found = self
            .skills
            .skills
            .get(&effect)
            .map(|e| (effect, e))
            .or_else(|| (effect >= 10_000_000).then(|| self.resolve_skill(effect / 10)).flatten());
        match found {
            Some((key, e)) => ((!e.name.is_empty()).then(|| e.name.clone()), e.icon.is_some().then_some(key)),
            None => (None, None),
        }
    }

    /// A name for this exact effect ID (one you gave it, or a Theostone's), not its
    /// skill's, and whether you gave it.
    pub fn effect_name(&self, effect: u32) -> Option<(String, bool)> {
        let e = self.skills.skills.get(&effect)?;
        (!e.name.is_empty()).then(|| (e.name.clone(), e.custom))
    }

    /// Fills in skill names and icon keys, and the target's name and boss flag.
    pub fn annotate(&self, snap: &mut Snapshot) {
        if let Some(enc) = snap.encounter.as_mut() {
            self.annotate_view(enc);
        }
        if let Some(t) = snap.target.as_mut() {
            if let Some((name, boss)) = t.mob_type.and_then(|m| self.npc(m)) {
                t.name = name;
                t.boss = boss;
            }
            // A mob's cast: most mob skills are only "Attack" in the data, which says nothing.
            if let Some(c) = t.cast.as_mut() {
                c.skill_name = self.boss_skill_name(c.skill);
            }
        }
    }

    /// A skill's name and icon key (for skills outside a view, e.g. the timelines).
    pub fn skill_meta(&self, skill: u32) -> (Option<String>, Option<u32>) {
        let resolved = self.resolve_skill(skill);
        let name = resolved.and_then(|(_, e)| (!e.name.is_empty()).then(|| e.name.clone()));
        (name, self.icon_key(skill, resolved))
    }

    /// What the data says about a player skill (cooldown, cost, description, its
    /// specialties), with the specialties this ID shows were chosen. A class skill's ID is
    /// its base skill plus the chosen specialties' numbers in its thousands, hundreds and
    /// tens digits, smallest first: Onslaught 18010000 with specialties 3 and 4 is
    /// 18010340. The last digit tells apart parts of one skill (chains, extra hits).
    /// A Theostone gets its grade and effect instead.
    pub fn skill_details(&self, skill: u32) -> Option<serde_json::Value> {
        // A Theostone: its grade and effect, in the shape the tooltips read (no specialties).
        if THEOSTONE_SKILLS.contains(&skill) {
            let t = self.theostones.theostones.get(&skill)?;
            let mut info = serde_json::json!({ "class": "Theostone", "desc": t.effect });
            if let Some(grade) = &t.grade {
                info["type"] = serde_json::json!(grade);
            }
            if let Some(s) = t.cooldown_s {
                info["cooldown_s"] = serde_json::json!(s);
            }
            return Some(info);
        }
        if !(10_000_000..100_000_000).contains(&skill) {
            return None;
        }
        let base = skill - skill % 10_000;
        let mut info = self.details.skills.get(&base)?.clone();
        let tail = skill % 10_000;
        let chosen: Vec<u32> = [tail / 1000, tail / 100 % 10, tail / 10 % 10]
            .into_iter()
            .filter(|d| *d > 0)
            .collect();
        info["base"] = serde_json::json!(base);
        info["chosen"] = serde_json::json!(chosen);
        Some(info)
    }

    /// A field boss's name, level, where it is and its map's name, by world-map ID.
    pub fn field_boss(&self, id: u32, map: u32) -> (Option<String>, Option<u32>, Option<String>, Option<String>) {
        let info = self.field_bosses.bosses.get(&id);
        let name = info.and_then(|b| self.npc(b.npc)).map(|(n, _)| n);
        let map_name = self.field_bosses.maps.get(&map).cloned();
        (name, info.and_then(|b| b.level), info.and_then(|b| b.place.clone()), map_name)
    }

    /// The mob types that are bosses: their death ends a fight, and HP back to full is a
    /// wipe. Training dummies are marked as bosses in the data but reset all the time.
    pub fn boss_types(&self) -> Vec<u32> {
        self.npcs
            .npcs
            .iter()
            .filter(|(_, e)| e.boss && !e.name.contains("Training Scarecrow"))
            .map(|(id, _)| *id)
            .collect()
    }

    /// A mob type's name and boss flag.
    pub fn npc(&self, type_id: u32) -> Option<(String, bool)> {
        let e = self.npcs.npcs.get(&type_id)?;
        (!e.name.is_empty()).then(|| (e.name.clone(), e.boss))
    }

    pub fn annotate_view(&self, enc: &mut EncounterView) {
        let rows = enc.players.iter_mut().chain(enc.healing.iter_mut()).chain(enc.taken.iter_mut());
        for row in rows.flat_map(|p| p.skills.iter_mut()) {
            let resolved = self.resolve_skill(row.skill);
            row.icon_key = self.icon_key(row.skill, resolved).or(row.icon_key);
            if row.name.is_none() {
                // A mob's skill the data has no name for: after the mob that used it.
                row.name = row.source_type.and_then(|t| self.npc(t)).map(|(mob, _)| format!("{mob}'s attack"));
            }
            if let Some((_, e)) = resolved {
                if !e.name.is_empty() {
                    row.name = Some(e.name.clone());
                }
            }
        }
        if let Some(e) = enc.target_type.and_then(|t| self.npcs.npcs.get(&t)) {
            if !e.name.is_empty() {
                enc.target = Some(e.name.clone());
            }
            enc.target_boss = e.boss;
        }
        // Death recaps: skills and mobs by name.
        let skill_name = |id: u32| self.resolve_skill(id).map(|(_, e)| e.name.clone()).filter(|n| !n.is_empty());
        let mob_name = |t: Option<u32>| t.and_then(|t| self.npc(t)).map(|(name, _)| name);
        for c in &mut enc.boss_casts {
            c.skill_name = self.boss_skill_name(c.skill);
            if let Some(name) = mob_name(c.boss_type) {
                c.boss = name;
            }
        }
        if let Some(c) = enc.boss_cast.as_mut() {
            c.skill_name = self.boss_skill_name(c.skill);
        }
        for d in &mut enc.deaths {
            d.skill_name = skill_name(d.skill);
            if let Some(name) = mob_name(d.killer_type) {
                d.killer = Some(name);
            }
            for h in d.hits.iter_mut().chain(d.last_heal.as_mut()) {
                h.skill_name = skill_name(h.skill);
                if let Some(name) = mob_name(h.from_type) {
                    h.from = name;
                }
            }
        }
    }

    /// A pet's name, by the ID its souls come with.
    pub fn pet_name(&self, pet: u32) -> Option<String> {
        self.pets.pets.get(&pet).map(|p| p.name.clone())
    }

    /// A boss skill's name: the data names most mob skills only "Attack", which says
    /// nothing, so that counts as no name (yours from names.json still does).
    pub fn boss_skill_name(&self, skill: u32) -> Option<String> {
        self.skill_meta(skill).0.filter(|n| n != "Attack" || self.names.skills.contains_key(&skill))
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
        match self.skills.skills.get(&skill).and_then(|e| e.icon.as_deref()) {
            Some(file) => self.icon_data_url(file),
            None => self.family_icon(skill).and_then(|f| self.icon_data_url(&f)),
        }
    }

    /// A skill without an icon of its own: its family's. Mob skills' icons are named after
    /// their ID's first four digits (1,782 of the 1,789 with icons in skills.json: Surging
    /// Thorn 1604390 has 1604.png), so 1604420 gets 1604.png too. A class's skill the data
    /// has no icon for (each class's Dodge, a Spiritmaster's spirits' attacks, basic
    /// attacks: 14 of the 771 player skills in our captures) gets its class's icon.
    fn family_icon(&self, skill: u32) -> Option<String> {
        if (1000..PLAYER_SKILL_MIN).contains(&skill) {
            let file = format!("{}.png", skill / 1000);
            return self.dir.join(&file).is_file().then_some(file);
        }
        let code = meter_core::classes::class_code_of_skill(skill)?;
        ["webp", "png"].iter().map(|ext| format!("class_{code}.{ext}")).find(|f| self.dir.join(f).is_file())
    }

    /// Which skill's icon to show for `skill`, given the entry it resolved to.
    fn icon_key(&self, skill: u32, resolved: Option<(u32, &Entry)>) -> Option<u32> {
        match resolved {
            Some((key, e)) if e.icon.is_some() => Some(key),
            _ => self.family_icon(skill).map(|_| skill),
        }
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
    fn npc_names_fill_in_what_the_mob_lists_dont_name() {
        let dir = temp_dir("npc-names");
        fs::write(dir.join(SKILLS_FILE), r#"{ "skills": { "11100000": { "name": "Ruinous Blow" } } }"#).unwrap();
        fs::write(dir.join(NPCS_FILE), r#"{ "npcs": { "2400009": { "name": "Hardened Drana", "boss": true } } }"#).unwrap();
        fs::write(dir.join(NPC_NAMES_FILE), r#"{ "npcs": { "2950865": "Elvida", "2400009": "Not this" } }"#).unwrap();
        let d = GameData::load_first(vec![dir.clone()]);
        assert_eq!(d.npc(2_950_865), Some(("Elvida".to_string(), false)));
        // A mob's own name stays, and it's still a boss.
        assert_eq!(d.npc(2_400_009), Some(("Hardened Drana".to_string(), true)));
        fs::remove_dir_all(dir).ok();
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
    fn theostones_and_effects_are_named_from_the_data() {
        let dir = temp_dir("effects");
        fs::write(dir.join("3001.png"), b"png").unwrap();
        fs::write(
            dir.join(SKILLS_FILE),
            r#"{ "skills": {
                "30010151": { "name": "Theostone: Santras's Secret", "icon": "3001.png" },
                "18330000": { "name": "Marchutan's Wrath" },
                "18120010": { "name": "Recuperation" }
            } }"#,
        )
        .unwrap();
        let d = GameData::load_first(vec![dir.clone()]);
        let theostone = (Some("Theostone: Santras's Secret".to_string()), Some(30_010_151));
        // From our captures: skill 3001015 applies effect 30010151.
        assert_eq!(d.skill_meta(3_001_015), theostone);
        assert_eq!(d.effect_meta(30_010_151), theostone);
        // Effects without a skill: the effect ID less its last digit is the skill.
        assert_eq!(d.effect_meta(183_300_071).0.as_deref(), Some("Marchutan's Wrath"));
        assert_eq!(d.effect_meta(181_200_101).0.as_deref(), Some("Recuperation"));
        // System effects have short IDs and no name.
        assert_eq!(d.effect_meta(231), (None, None));
        // Only Theostone-range skills get the extra digit.
        assert_eq!(d.skill_meta(1_833_000), (None, None));
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn consumables_get_their_icons() {
        let dir = temp_dir("item-icons");
        fs::write(
            dir.join(SKILLS_FILE),
            r#"{ "skills": { "2210105": { "name": "Courage Scroll" }, "18010000": { "name": "Onslaught", "icon": "1801.png" } } }"#,
        )
        .unwrap();
        fs::write(
            dir.join(ITEM_ICONS_FILE),
            r#"{ "icons": { "2210105": "Scroll.png", "18010000": "Other.png", "1": "Nothing.png" } }"#,
        )
        .unwrap();
        let d = GameData::load_first(vec![dir.clone()]);
        // A scroll's buff: the effect is the skill and one more digit.
        assert_eq!(d.effect_meta(22_101_051), (Some("Courage Scroll".to_string()), Some(2_210_105)));
        assert_eq!(d.skills.skills[&2_210_105].icon.as_deref(), Some("Scroll.png"));
        // A skill that has an icon keeps it.
        assert_eq!(d.skills.skills[&18_010_000].icon.as_deref(), Some("1801.png"));
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn class_skills_without_an_icon_get_their_classs() {
        let dir = temp_dir("class-icons");
        fs::write(dir.join(SKILLS_FILE), r#"{ "skills": { "18000100": { "name": "Dodge" } } }"#).unwrap();
        fs::write(dir.join("class_18.webp"), b"RIFF").unwrap();
        let d = GameData::load_first(vec![dir.clone()]);
        assert_eq!(d.skill_meta(18_000_100).1, Some(18_000_100));
        assert!(d.skill_icon(18_000_100).is_some_and(|u| u.starts_with("data:image/webp;base64,")));
        // No icon for a class we have none for.
        assert!(d.skill_icon(17_000_100).is_none());
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn dungeon_mobs_know_where_they_are() {
        let dir = temp_dir("dungeons");
        fs::write(dir.join(SKILLS_FILE), r#"{ "skills": { "18010000": { "name": "Onslaught" } } }"#).unwrap();
        fs::write(
            dir.join(DUNGEONS_FILE),
            r#"{ "dungeons": { "DeusCenter": { "name": "Deus Research Base", "type": "Transcendence" } },
                 "mobs": { "2340601": { "dungeon": "DeusCenter", "mode": "Stage 2" }, "1": { "dungeon": "Gone" } } }"#,
        )
        .unwrap();
        let d = GameData::load_first(vec![dir.clone()]);
        let place = DungeonPlace { id: "DeusCenter", name: "Deus Research Base", kind: Some("Transcendence"), mode: Some("Stage 2") };
        assert_eq!(d.dungeon(2_340_601), Some(place));
        assert_eq!(d.dungeon(2_400_010), None);
        assert_eq!(d.dungeon(1), None, "a mob in a dungeon the file doesn't list");
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn theostones_have_their_effect() {
        let dir = temp_dir("theostones");
        fs::write(dir.join(SKILLS_FILE), r#"{ "skills": { "18010000": { "name": "Onslaught" } } }"#).unwrap();
        fs::write(
            dir.join(THEOSTONES_FILE),
            r#"{ "theostones": {
                "3000015": { "name": "Theostone: Santras's Secret", "grade": "Rare", "effect": "Poison", "cooldown_s": 1.0 },
                "3001015": { "name": "Theostone: Santras's Secret", "grade": "Rare", "effect": "Poison", "cooldown_s": 1.0 }
            } }"#,
        )
        .unwrap();
        let d = GameData::load_first(vec![dir.clone()]);
        let info = d.skill_details(3_001_015).unwrap();
        assert_eq!((info["class"].as_str(), info["type"].as_str()), (Some("Theostone"), Some("Rare")));
        assert_eq!((info["desc"].as_str(), info["cooldown_s"].as_f64()), (Some("Poison"), Some(1.0)));
        assert!(info.get("chosen").is_none(), "Theostones have no specialties");
        assert!(d.skill_details(3_000_016).is_none());
        // Two IDs, one Theostone.
        assert_eq!(d.theostone_count(), 1);
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn skill_ids_carry_their_specialties() {
        let dir = temp_dir("details");
        fs::write(dir.join(SKILLS_FILE), r#"{ "skills": { "18010000": { "name": "Onslaught" } } }"#).unwrap();
        fs::write(
            dir.join(DETAILS_FILE),
            r#"{ "skills": { "18010000": { "name": "Onslaught", "specs": [ { "id": 18010030 } ] } } }"#,
        )
        .unwrap();
        let d = GameData::load_first(vec![dir.clone()]);
        // Captured as cast: specialties 3 and 4.
        let info = d.skill_details(18_010_340).unwrap();
        assert_eq!(info["base"], 18_010_000);
        assert_eq!(info["chosen"], serde_json::json!([3, 4]));
        assert_eq!(d.skill_details(18_010_000).unwrap()["chosen"], serde_json::json!([]));
        assert!(d.skill_details(18_020_340).is_none(), "no details for that skill");
        assert!(d.skill_details(3_001_015).is_none(), "not a class skill");
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
