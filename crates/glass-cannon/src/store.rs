//! Small files the app keeps in its settings folder (%APPDATA%\io.github.xinjalin.glass-cannon):
//!
//! - `settings.json`  settings choices (record fights, show everyone)
//! - `profile.json`   your character's last-seen name and class
//! - `history.jsonl`  finished fights, one per line (newest last)

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use meter_core::meter::{EncounterView, Scope, Snapshot};
use serde::{de::DeserializeOwned, Deserialize, Serialize};

/// Fights kept in memory and shown in the history list.
const HISTORY_LIMIT: usize = 200;

fn load<T: DeserializeOwned + Default>(path: &Path) -> T {
    fs::read(path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

fn save<T: Serialize>(path: &Path, value: &T) {
    if let Some(dir) = path.parent() {
        let _ = fs::create_dir_all(dir);
    }
    if let Ok(json) = serde_json::to_vec_pretty(value) {
        let tmp = path.with_extension("tmp");
        if fs::write(&tmp, json).is_ok() {
            let _ = fs::rename(&tmp, path);
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Settings {
    /// Also save each session's packets to `captures/`.
    #[serde(default)]
    pub record: bool,
    /// Whose damage counts: your targets ("mine"), your party, or everyone nearby.
    #[serde(default)]
    pub scope: Scope,
    /// Older settings files: `true` becomes `scope: "everyone"`.
    #[serde(default, skip_serializing)]
    everyone: bool,
    #[serde(default)]
    pub appearance: Appearance,
}

pub const THEMES: [&str; 6] = ["rose", "lavender", "mint", "sky", "amber", "mono"];
pub const HIGHLIGHTS: [&str; 2] = ["self", "all"];
pub const BACKDROPS: [&str; 3] = ["frosted", "tinted", "clear"];
/// Bar colours: follow the theme, each player's class, or one of the theme colours.
pub const BAR_COLORS: [&str; 8] = ["theme", "class", "rose", "lavender", "mint", "sky", "amber", "mono"];
pub const FONT_SIZES: std::ops::RangeInclusive<u8> = 10..=18;
pub const OPACITIES: std::ops::RangeInclusive<u8> = 0..=100;

/// How the windows look. Unknown or out-of-range values fall back to the defaults.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Appearance {
    /// Accent colour: one of [`THEMES`].
    #[serde(default = "default_theme")]
    pub theme: String,
    /// The colour of the bars and filled rows, independent of the theme: one of [`BAR_COLORS`].
    #[serde(default = "default_bar_color")]
    pub bar_color: String,
    /// Older settings files: `true` becomes `bar_color: "class"`.
    #[serde(default, skip_serializing)]
    class_colors: bool,
    /// "self": only your row is filled; "all": every row is filled in its colour.
    #[serde(default = "default_highlight")]
    pub highlight: String,
    /// Text size in pixels.
    #[serde(default = "default_font_size")]
    pub font_size: u8,
    /// How opaque the dark tint over the background is, in percent.
    #[serde(default = "default_opacity")]
    pub opacity: u8,
    /// "frosted": blurred glass; "tinted": see-through tint, no blur; "clear": nothing at all.
    #[serde(default = "default_backdrop")]
    pub backdrop: String,
    /// Only the target name and the meter bars: no title bar, frame or border.
    #[serde(default)]
    pub minimal: bool,
    /// A thin strip at the top with the local time and ping.
    #[serde(default)]
    pub info_bar: bool,
}

fn default_theme() -> String {
    "rose".into()
}
fn default_bar_color() -> String {
    "theme".into()
}
fn default_highlight() -> String {
    "self".into()
}
fn default_font_size() -> u8 {
    13
}
fn default_opacity() -> u8 {
    62
}
fn default_backdrop() -> String {
    "frosted".into()
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            theme: default_theme(),
            bar_color: default_bar_color(),
            class_colors: false,
            highlight: default_highlight(),
            font_size: default_font_size(),
            opacity: default_opacity(),
            backdrop: default_backdrop(),
            minimal: false,
            info_bar: false,
        }
    }
}

/// Changes to apply; anything left as `None` stays as it is.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppearanceChange {
    pub theme: Option<String>,
    pub bar_color: Option<String>,
    pub highlight: Option<String>,
    pub font_size: Option<u8>,
    pub opacity: Option<u8>,
    pub backdrop: Option<String>,
    pub minimal: Option<bool>,
    pub info_bar: Option<bool>,
}

impl Appearance {
    /// Applies the given changes, keeping every value valid.
    pub fn update(&mut self, c: AppearanceChange) {
        if let Some(t) = c.theme.filter(|t| THEMES.contains(&t.as_str())) {
            self.theme = t;
        }
        if let Some(b) = c.bar_color.filter(|b| BAR_COLORS.contains(&b.as_str())) {
            self.bar_color = b;
        }
        if let Some(h) = c.highlight.filter(|h| HIGHLIGHTS.contains(&h.as_str())) {
            self.highlight = h;
        }
        if let Some(f) = c.font_size {
            self.font_size = f.clamp(*FONT_SIZES.start(), *FONT_SIZES.end());
        }
        if let Some(o) = c.opacity {
            self.opacity = o.min(*OPACITIES.end());
        }
        if let Some(b) = c.backdrop.filter(|b| BACKDROPS.contains(&b.as_str())) {
            self.backdrop = b;
        }
        if let Some(v) = c.minimal {
            self.minimal = v;
        }
        if let Some(v) = c.info_bar {
            self.info_bar = v;
        }
    }

    /// Repairs values from a hand-edited or older settings file.
    pub fn sanitized(mut self) -> Self {
        let fixed = Self::default();
        if !THEMES.contains(&self.theme.as_str()) {
            self.theme = fixed.theme;
        }
        if std::mem::take(&mut self.class_colors) && self.bar_color == "theme" {
            self.bar_color = "class".into();
        }
        if !BAR_COLORS.contains(&self.bar_color.as_str()) {
            self.bar_color = fixed.bar_color;
        }
        if !HIGHLIGHTS.contains(&self.highlight.as_str()) {
            self.highlight = fixed.highlight;
        }
        if !FONT_SIZES.contains(&self.font_size) {
            self.font_size = fixed.font_size;
        }
        if !OPACITIES.contains(&self.opacity) {
            self.opacity = fixed.opacity;
        }
        if !BACKDROPS.contains(&self.backdrop.as_str()) {
            self.backdrop = fixed.backdrop;
        }
        self
    }
}

impl Settings {
    pub fn load(dir: &Path) -> Self {
        let mut s: Settings = load(&dir.join("settings.json"));
        s.appearance = s.appearance.sanitized();
        if std::mem::take(&mut s.everyone) && s.scope == Scope::Mine {
            s.scope = Scope::Everyone;
        }
        s
    }

    pub fn save(&self, dir: &Path) {
        save(&dir.join("settings.json"), self);
    }
}

/// Your character as last seen, so the meter can name you before the game says who you
/// are (it only does that at login, on a zone change, or when you first hit a real mob).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Profile {
    pub name: Option<String>,
    pub class_code: Option<u32>,
    #[serde(skip)]
    path: PathBuf,
}

impl Profile {
    pub fn load(dir: &Path) -> Self {
        let path = dir.join("profile.json");
        Profile { path: path.clone(), ..load(&path) }
    }

    /// Learns from a snapshot that knows your name, or fills in the remembered name when
    /// it doesn't (only if your class matches, so another character isn't mislabelled).
    /// Returns true when the remembered name was used.
    pub fn apply(&mut self, snap: &mut Snapshot) -> bool {
        let my_class = snap
            .encounter
            .as_ref()
            .and_then(|e| e.players.iter().find(|p| p.is_self))
            .and_then(|p| p.class_code);
        match &snap.self_name {
            Some(name) => {
                let class_code = my_class.or(if self.name.as_ref() == Some(name) { self.class_code } else { None });
                if self.name.as_ref() != Some(name) || self.class_code != class_code {
                    self.name = Some(name.clone());
                    self.class_code = class_code;
                    save(&self.path, self);
                }
                false
            }
            None => {
                let Some(name) = self.name.clone() else { return false };
                if snap.self_id.is_none() {
                    return false;
                }
                if my_class.is_some() && self.class_code.is_some() && my_class != self.class_code {
                    return false;
                }
                snap.self_name = Some(name.clone());
                if let Some(enc) = snap.encounter.as_mut() {
                    self.name_self_row(enc, &name);
                }
                true
            }
        }
    }

    /// Puts the remembered name on your row in a fight view, if it has no name yet.
    pub fn name_self_row(&self, enc: &mut EncounterView, name: &str) {
        for p in enc.players.iter_mut().filter(|p| p.is_self) {
            if p.name.starts_with("You (#") {
                p.name = name.to_string();
            }
        }
    }
}

/// A finished fight, as saved.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fight {
    /// When it ended (ms since the Unix epoch).
    pub ended_ms: u64,
    pub self_name: Option<String>,
    pub view: EncounterView,
}

/// One line of the history list.
#[derive(Debug, Clone, Serialize)]
pub struct FightSummary {
    /// Position in the list (pass to `history_fight`).
    pub index: usize,
    pub ended_ms: u64,
    pub target: Option<String>,
    pub target_boss: bool,
    pub duration_s: f64,
    pub total_dps: f64,
    pub players: usize,
    pub my_dps: Option<f64>,
}

pub struct History {
    path: PathBuf,
    fights: Vec<Fight>,
}

impl History {
    pub fn load(dir: &Path) -> Self {
        let path = dir.join("history.jsonl");
        let mut fights: Vec<Fight> = fs::read_to_string(&path)
            .unwrap_or_default()
            .lines()
            .filter_map(|l| serde_json::from_str(l).ok())
            .collect();
        let excess = fights.len().saturating_sub(HISTORY_LIMIT);
        fights.drain(..excess);
        Self { path, fights }
    }

    pub fn push(&mut self, fight: Fight) {
        if let Some(dir) = self.path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        if let Ok(line) = serde_json::to_string(&fight) {
            if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&self.path) {
                let _ = writeln!(f, "{line}");
            }
        }
        self.fights.push(fight);
        if self.fights.len() > HISTORY_LIMIT {
            self.fights.remove(0);
        }
    }

    pub fn len(&self) -> usize {
        self.fights.len()
    }

    /// Newest first.
    pub fn summaries(&self) -> Vec<FightSummary> {
        self.fights
            .iter()
            .enumerate()
            .rev()
            .map(|(index, f)| FightSummary {
                index,
                ended_ms: f.ended_ms,
                target: f.view.target.clone(),
                target_boss: f.view.target_boss,
                duration_s: f.view.duration_s,
                total_dps: f.view.total_dps,
                players: f.view.players.len(),
                my_dps: f.view.players.iter().find(|p| p.is_self).map(|p| p.dps),
            })
            .collect()
    }

    pub fn fight(&self, index: usize) -> Option<&Fight> {
        self.fights.get(index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let d = std::env::temp_dir().join(format!("meter-store-{tag}-{}-{nanos}", std::process::id()));
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn snap(self_name: Option<&str>, class: Option<u32>) -> Snapshot {
        serde_json::from_value::<EncounterView>(serde_json::json!({
            "active": true, "duration_s": 10.0, "total_damage": 100, "total_dps": 10.0,
            "target": null, "target_id": null, "target_type": null, "target_count": 1,
            "start_us": 0, "end_us": 10_000_000,
            "players": [{ "id": 3199, "name": "You (#3199)", "is_self": true, "class_code": class,
                "class_name": null, "damage": 100, "dps": 10.0, "share": 1.0, "hits": 1, "max_hit": 100,
                "skills": [] }]
        }))
        .map(|view| Snapshot {
            self_name: self_name.map(str::to_string),
            self_id: Some(3199),
            scope: Scope::Mine,
            party_size: 0,
            encounter: Some(view),
            encounters_completed: 0,
        })
        .unwrap()
    }

    #[test]
    fn profile_learns_and_fills_in_your_name() {
        let dir = temp_dir("profile");
        let mut p = Profile::load(&dir);
        assert!(!p.apply(&mut snap(Some("NeneneTowawa"), Some(18))));

        let mut p = Profile::load(&dir); // reloaded from disk
        let mut s = snap(None, Some(18));
        assert!(p.apply(&mut s));
        assert_eq!(s.self_name.as_deref(), Some("NeneneTowawa"));
        assert_eq!(s.encounter.unwrap().players[0].name, "NeneneTowawa");

        // A different class: probably another character, so don't guess.
        let mut other = snap(None, Some(11));
        assert!(!p.apply(&mut other));
        assert_eq!(other.self_name, None);
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn history_appends_and_reloads_newest_first() {
        let dir = temp_dir("history");
        let mut h = History::load(&dir);
        for i in 0..3u64 {
            let mut view = snap(None, Some(18)).encounter.unwrap();
            view.total_dps = i as f64;
            h.push(Fight { ended_ms: i, self_name: None, view });
        }
        let again = History::load(&dir);
        assert_eq!(again.len(), 3);
        let list = again.summaries();
        assert_eq!(list[0].ended_ms, 2);
        assert_eq!(list[0].index, 2);
        assert_eq!(list[0].my_dps, Some(10.0));
        assert_eq!(again.fight(0).unwrap().view.total_dps, 0.0);
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn settings_round_trip() {
        let dir = temp_dir("settings");
        let mut s = Settings { record: true, ..Settings::default() };
        s.appearance.update(AppearanceChange {
            theme: Some("mint".into()),
            bar_color: Some("amber".into()),
            highlight: Some("all".into()),
            font_size: Some(16),
            opacity: Some(0),
            backdrop: Some("clear".into()),
            minimal: Some(true),
            info_bar: Some(true),
        });
        s.save(&dir);
        let again = Settings::load(&dir);
        assert!(again.record);
        assert_eq!(again.appearance, s.appearance);
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn old_show_everyone_becomes_the_everyone_scope() {
        let dir = temp_dir("scope");
        fs::write(dir.join("settings.json"), r#"{ "everyone": true }"#).unwrap();
        assert_eq!(Settings::load(&dir).scope, Scope::Everyone);
        fs::write(dir.join("settings.json"), r#"{ "scope": "party" }"#).unwrap();
        assert_eq!(Settings::load(&dir).scope, Scope::Party);
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn appearance_stays_valid() {
        let mut a = Appearance::default();
        a.update(AppearanceChange {
            theme: Some("neon".into()),
            bar_color: Some("plaid".into()),
            highlight: Some("rainbow".into()),
            font_size: Some(99),
            opacity: Some(250),
            backdrop: Some("fog".into()),
            ..AppearanceChange::default()
        });
        assert_eq!(a.theme, "rose");
        assert_eq!(a.bar_color, "theme");
        assert_eq!(a.highlight, "self");
        assert_eq!(a.font_size, 18);
        assert_eq!(a.opacity, 100);
        assert_eq!(a.backdrop, "frosted");
        // An older settings file without appearance gets the defaults.
        let old: Settings = serde_json::from_str(r#"{ "record": true }"#).unwrap();
        assert_eq!(old.appearance, Appearance::default());
        let broken: Appearance =
            serde_json::from_str(r#"{ "theme": "x", "font_size": 40, "opacity": 101, "backdrop": "x" }"#).unwrap();
        assert_eq!(broken.sanitized(), Appearance::default());
        // The old class-colours switch carries over.
        let old: Appearance = serde_json::from_str(r#"{ "class_colors": true }"#).unwrap();
        assert_eq!(old.sanitized().bar_color, "class");
    }
}
