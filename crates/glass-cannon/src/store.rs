//! Small files the app keeps in its settings folder (%APPDATA%\io.github.xinjalin.glass-cannon):
//!
//! - `settings.json`  settings choices (record fights, show everyone)
//! - `profile.json`   your character's last-seen name and class
//! - `history.jsonl`  finished fights, one per line (newest last)
//! - `players.json`   names of players seen this login, for when the meter restarts

use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use meter_core::meter::{Analysis, EncounterView, FightEnd, Scope, Snapshot};
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    /// Also save each session's packets to `captures/`.
    #[serde(default)]
    pub record: bool,
    /// Whose damage counts when chosen by hand: yours ("mine"), your party's, or
    /// everyone's nearby. Only used with `manual_scope`.
    #[serde(default)]
    pub scope: Scope,
    /// The scope was chosen by hand. Otherwise it follows your party: your party's fight
    /// while you're in one, yours on your own.
    #[serde(default)]
    pub manual_scope: bool,
    /// Older settings files: `true` becomes `scope: "everyone"`.
    #[serde(default, skip_serializing)]
    everyone: bool,
    #[serde(default)]
    pub appearance: Appearance,
    /// How often the overlay redraws, in milliseconds: one of [`REFRESH_RATES`]
    /// ([`REALTIME`]: as soon as new data arrives).
    #[serde(default)]
    pub refresh_ms: u32,
    /// Click-through: the meter ignores the mouse except for its lock button.
    #[serde(default)]
    pub locked: bool,
    /// What the meter's rows show.
    #[serde(default)]
    pub meter_view: MeterView,
    /// The windows left open (history, cooldowns, timers), opened again at start.
    #[serde(default)]
    pub panels: Vec<String>,
    /// Whose event schedule the timers follow: one of [`TIMER_REGIONS`].
    #[serde(default = "default_timer_region")]
    pub timer_region: String,
    /// Events (their timer IDs) and field bosses ("boss:<id>") to be told about before
    /// they start, and how many minutes before.
    #[serde(default)]
    pub alerts: Vec<String>,
    #[serde(default = "default_alert_lead")]
    pub alert_lead_min: u32,
    /// The meter fades away this many seconds after combat ends (0: it doesn't), and comes
    /// back in the next fight or under the mouse.
    #[serde(default)]
    pub hide_after_s: u32,
    /// The boss's HP and enrage timer under the target's name.
    #[serde(default = "default_true")]
    pub boss_hp: bool,
    /// Under that, what the boss is casting, else what it cast next last time.
    #[serde(default = "default_true")]
    pub boss_casts: bool,
    /// A notice for each pet soul from a kill, with the pet's progress.
    #[serde(default = "default_true")]
    pub pet_souls: bool,
}

fn default_alert_lead() -> u32 {
    5
}
fn default_true() -> bool {
    true
}

/// Regions with their own event times (the global client's by time zone, Korea, Taiwan).
pub const TIMER_REGIONS: [&str; 6] = ["as", "naw", "eu", "la", "kr", "tw"];

fn default_timer_region() -> String {
    "as".into()
}

/// No settings file: the same as an empty one, so each setting gets its own default (some
/// are on unless turned off).
impl Default for Settings {
    fn default() -> Self {
        serde_json::from_str("{}").expect("every setting has a default")
    }
}

impl Settings {
    /// Whose damage counts now, for a party of `party` people (you included; 0 alone).
    pub fn scope_for(&self, party: usize) -> Scope {
        match (self.manual_scope, party > 1) {
            (true, _) => self.scope,
            (false, true) => Scope::Party,
            (false, false) => Scope::Mine,
        }
    }
}

/// What the meter's rows show: damage dealt, healing done or damage taken.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MeterView {
    #[default]
    Damage,
    Healing,
    Taken,
    /// Who's in your party or group, with their combat power and gear score.
    Party,
}

pub const REFRESH_RATES: [u32; 5] = [REALTIME, 200, 500, 1000, 2000];
/// The "as it happens" rate: the overlay redraws when new data arrives instead of on a
/// timer, at most this often (ten times a second).
pub const REALTIME: u32 = 100;
pub const DEFAULT_REFRESH_MS: u32 = 500;

pub const THEMES: [&str; 6] = ["rose", "lavender", "mint", "sky", "amber", "mono"];
pub const HIGHLIGHTS: [&str; 3] = ["self", "all", "off"];
pub const BACKDROPS: [&str; 3] = ["frosted", "tinted", "clear"];
/// Bar colours: follow the theme, each player's class, or one of the theme colours.
pub const BAR_COLORS: [&str; 8] = ["theme", "class", "rose", "lavender", "mint", "sky", "amber", "mono"];
pub const FONT_SIZES: std::ops::RangeInclusive<u8> = 10..=18;
pub const OPACITIES: std::ops::RangeInclusive<u8> = 0..=100;

/// How the windows look. Unknown or out-of-range values fall back to the defaults.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Appearance {
    /// Accent colour: one of [`THEMES`], or a colour of your own (`#rrggbb`).
    #[serde(default = "default_theme")]
    pub theme: String,
    /// The colour of the bars and filled rows, independent of the theme: one of
    /// [`BAR_COLORS`], or a colour of your own (`#rrggbb`).
    #[serde(default = "default_bar_color")]
    pub bar_color: String,
    /// Older settings files: `true` becomes `bar_color: "class"`.
    #[serde(default, skip_serializing)]
    class_colors: bool,
    /// "self": only your row is filled; "all": every row is filled in its colour; "off":
    /// no row is filled, and the bars show the bar colour.
    #[serde(default = "default_highlight")]
    pub highlight: String,
    /// Text size in pixels.
    #[serde(default = "default_font_size")]
    pub font_size: u8,
    /// An installed font family; empty for the built-in monospace (Cascadia Mono).
    #[serde(default)]
    pub font: String,
    /// How opaque the dark tint over the background is, in percent.
    #[serde(default = "default_opacity")]
    pub opacity: u8,
    /// "frosted": blurred glass; "tinted": see-through tint, no blur; "clear": nothing at all.
    #[serde(default = "default_backdrop")]
    pub backdrop: String,
    /// Only the target name and the meter bars: no title bar, frame or border.
    #[serde(default)]
    pub minimal: bool,
    /// A thin strip at the top with the local time.
    #[serde(default)]
    pub info_bar: bool,
}

/// A colour picked by hand: `#` and six hex digits.
fn is_hex_color(s: &str) -> bool {
    s.len() == 7 && s.starts_with('#') && s[1..].chars().all(|c| c.is_ascii_hexdigit())
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
            font: String::new(),
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
    /// Any number: out-of-range sizes are clamped rather than refused.
    pub font_size: Option<i64>,
    pub font: Option<String>,
    /// Any number: clamped to 0-100.
    pub opacity: Option<i64>,
    pub backdrop: Option<String>,
    pub minimal: Option<bool>,
    pub info_bar: Option<bool>,
}

impl Appearance {
    /// Applies the given changes, keeping every value valid.
    pub fn update(&mut self, c: AppearanceChange) {
        if let Some(t) = c.theme.filter(|t| THEMES.contains(&t.as_str()) || is_hex_color(t)) {
            self.theme = t.to_ascii_lowercase();
        }
        if let Some(b) = c.bar_color.filter(|b| BAR_COLORS.contains(&b.as_str()) || is_hex_color(b)) {
            self.bar_color = b.to_ascii_lowercase();
        }
        if let Some(h) = c.highlight.filter(|h| HIGHLIGHTS.contains(&h.as_str())) {
            self.highlight = h;
        }
        if let Some(f) = c.font.filter(|f| f.is_empty() || crate::fonts::usable_name(f)) {
            self.font = f;
        }
        if let Some(f) = c.font_size {
            self.font_size = f.clamp(i64::from(*FONT_SIZES.start()), i64::from(*FONT_SIZES.end())) as u8;
        }
        if let Some(o) = c.opacity {
            self.opacity = o.clamp(i64::from(*OPACITIES.start()), i64::from(*OPACITIES.end())) as u8;
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
        if !self.font.is_empty() && !crate::fonts::usable_name(&self.font) {
            self.font = fixed.font.clone();
        }
        if !THEMES.contains(&self.theme.as_str()) && !is_hex_color(&self.theme) {
            self.theme = fixed.theme;
        }
        if std::mem::take(&mut self.class_colors) && self.bar_color == "theme" {
            self.bar_color = "class".into();
        }
        if !BAR_COLORS.contains(&self.bar_color.as_str()) && !is_hex_color(&self.bar_color) {
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
        if !REFRESH_RATES.contains(&s.refresh_ms) {
            s.refresh_ms = DEFAULT_REFRESH_MS;
        }
        s
    }

    pub fn save(&self, dir: &Path) {
        save(&dir.join("settings.json"), self);
    }
}

/// How long remembered player names stay usable. Entity IDs only last a login; this
/// just stops a very old file from being trusted if your own ID ever came round again.
const KNOWN_PLAYERS_MAX_AGE_MS: u64 = 12 * 60 * 60 * 1000;

/// Players named during this login. The game names a player only when they come into
/// view (or you change zone), so without this a restarted meter would show everyone
/// already around you as a number.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct KnownPlayers {
    /// Your entity ID when these were seen: entity IDs only hold within one login.
    pub self_id: u64,
    pub saved_ms: u64,
    pub players: std::collections::BTreeMap<u64, String>,
}

impl KnownPlayers {
    pub fn load(dir: &Path) -> Self {
        load(&dir.join("players.json"))
    }

    pub fn save(&self, dir: &Path) {
        save(&dir.join("players.json"), self);
    }

    /// The names, if they're from the login you're in (same entity ID, not too old).
    pub fn for_login(self, self_id: u64, now_ms: u64) -> Option<std::collections::BTreeMap<u64, String>> {
        let fresh = now_ms.saturating_sub(self.saved_ms) <= KNOWN_PLAYERS_MAX_AGE_MS;
        (self.self_id == self_id && fresh && !self.players.is_empty()).then_some(self.players)
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

/// Fights kept with their full analysis (the rest of the history keeps its summary).
const DETAILS_LIMIT: usize = 50;

const INSTANCES_LIMIT: usize = 500;
/// PvP sessions kept.
const PVP_LIMIT: usize = 300;

/// Field bosses as world maps last showed them, and names you gave them
/// (`fieldbosses.json`).
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct FieldBosses {
    #[serde(skip)]
    path: PathBuf,
    #[serde(default)]
    pub names: BTreeMap<u32, String>,
    #[serde(default)]
    pub bosses: BTreeMap<u32, meter_core::fieldboss::FieldBoss>,
    /// When each was last read from a world map (ms since 1970).
    #[serde(default)]
    pub seen_ms: BTreeMap<u32, u64>,
}

impl FieldBosses {
    pub fn load(dir: &Path) -> Self {
        let path = dir.join("fieldbosses.json");
        let mut f: Self = fs::read_to_string(&path).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        f.path = path;
        f
    }

    fn save(&self) {
        if let Some(dir) = self.path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        if let Ok(s) = serde_json::to_string_pretty(self) {
            let _ = fs::write(&self.path, s);
        }
    }

    pub fn update(&mut self, seen: Vec<meter_core::fieldboss::FieldBoss>) {
        if seen.is_empty() {
            return;
        }
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64);
        for b in seen {
            self.seen_ms.insert(b.id, now);
            self.bosses.insert(b.id, b);
        }
        self.save();
    }

    pub fn rename(&mut self, id: u32, name: &str) {
        let name = name.trim();
        if name.is_empty() {
            self.names.remove(&id);
        } else {
            self.names.insert(id, name.chars().take(40).collect());
        }
        self.save();
    }
}

/// Content tickets and Abyss artifacts as the game last sent them, and the names you gave
/// tickets (`contents.json`). Kept so the world timers show them after a restart, until the
/// game sends them again (at login, entering the Abyss, opening its map).
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct KnownContents {
    #[serde(skip)]
    path: PathBuf,
    #[serde(default)]
    pub tickets: BTreeMap<u32, meter_core::contents::Ticket>,
    #[serde(default)]
    pub ticket_names: BTreeMap<u32, String>,
    /// When the tickets were last sent (ms since 1970).
    #[serde(default)]
    pub tickets_ms: u64,
    #[serde(default)]
    pub sieges: BTreeMap<u32, meter_core::contents::Siege>,
    #[serde(default)]
    pub artifacts: BTreeMap<u32, meter_core::contents::Artifact>,
    #[serde(default)]
    pub abyss_ms: u64,
}

impl KnownContents {
    pub fn load(dir: &Path) -> Self {
        let path = dir.join("contents.json");
        let mut c: Self = fs::read_to_string(&path).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        c.path = path;
        c
    }

    fn save(&self) {
        if let Some(dir) = self.path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        if let Ok(s) = serde_json::to_string_pretty(self) {
            let _ = fs::write(&self.path, s);
        }
    }

    pub fn update(&mut self, c: meter_core::meter::Contents) {
        if c == meter_core::meter::Contents::default() {
            return;
        }
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64);
        if c.full_list {
            self.tickets.clear();
        }
        if !c.tickets.is_empty() {
            self.tickets.extend(c.tickets.into_iter().map(|t| (t.id, t)));
            self.tickets_ms = now;
        }
        if !c.sieges.is_empty() || !c.artifacts.is_empty() {
            self.sieges.extend(c.sieges.into_iter().map(|s| (s.event, s)));
            self.artifacts.extend(c.artifacts.into_iter().map(|a| (a.id, a)));
            self.abyss_ms = now;
        }
        self.save();
    }

    pub fn rename_ticket(&mut self, id: u32, name: &str) {
        let name = name.trim();
        if name.is_empty() {
            self.ticket_names.remove(&id);
        } else {
            self.ticket_names.insert(id, name.chars().take(40).collect());
        }
        self.save();
    }
}

/// PvP sessions, oldest first (`pvp.jsonl`).
pub struct PvpLog {
    path: PathBuf,
    sessions: Vec<meter_core::pvp::PvpSession>,
}

impl PvpLog {
    pub fn load(dir: &Path) -> Self {
        let path = dir.join("pvp.jsonl");
        let mut sessions: Vec<meter_core::pvp::PvpSession> = fs::read_to_string(&path)
            .unwrap_or_default()
            .lines()
            .filter_map(|l| serde_json::from_str(l).ok())
            .collect();
        let extra = sessions.len().saturating_sub(PVP_LIMIT);
        sessions.drain(..extra);
        Self { path, sessions }
    }

    pub fn record(&mut self, s: meter_core::pvp::PvpSession) {
        if let Some(dir) = self.path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        if let (Ok(line), Ok(mut f)) = (serde_json::to_string(&s), OpenOptions::new().create(true).append(true).open(&self.path)) {
            let _ = writeln!(f, "{line}");
        }
        self.sessions.push(s);
    }

    /// Newest first.
    pub fn list(&self) -> Vec<meter_core::pvp::PvpSession> {
        self.sessions.iter().rev().cloned().collect()
    }

    pub fn clear(&mut self) {
        self.sessions.clear();
        let _ = fs::remove_file(&self.path);
    }
}

/// Dungeon runs as the game reported them: when you entered, when the countdown ended and
/// when it was completed. Kept in `instances.jsonl`, one line per change (the last line for
/// an entry wins).
pub struct Instances {
    path: PathBuf,
    by_entered: BTreeMap<u64, meter_core::dungeon::Instance>,
}

impl Instances {
    pub fn load(dir: &Path) -> Self {
        let path = dir.join("instances.jsonl");
        let mut by_entered = BTreeMap::new();
        for line in fs::read_to_string(&path).unwrap_or_default().lines() {
            if let Ok(i) = serde_json::from_str::<meter_core::dungeon::Instance>(line) {
                by_entered.insert(i.entered_us, i);
            }
        }
        while by_entered.len() > INSTANCES_LIMIT {
            by_entered.pop_first();
        }
        Self { path, by_entered }
    }

    /// Saves a dungeon run's progress (stays without a countdown aren't dungeons).
    pub fn record(&mut self, i: meter_core::dungeon::Instance) {
        if i.started_us.is_none() || self.by_entered.get(&i.entered_us) == Some(&i) {
            return;
        }
        self.by_entered.insert(i.entered_us, i);
        if let Some(dir) = self.path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        if let (Ok(line), Ok(mut f)) = (serde_json::to_string(&i), OpenOptions::new().create(true).append(true).open(&self.path)) {
            let _ = writeln!(f, "{line}");
        }
    }

    pub fn get(&self, entered_us: u64) -> Option<&meter_core::dungeon::Instance> {
        self.by_entered.get(&entered_us)
    }

    /// Forgets every dungeon run but the one entered at `keep` (the one you're in).
    pub fn clear_except(&mut self, keep: Option<u64>) {
        self.by_entered.retain(|entered, _| Some(*entered) == keep);
        let lines: String = self
            .by_entered
            .values()
            .filter_map(|i| serde_json::to_string(i).ok())
            .map(|l| l + "
")
            .collect();
        if lines.is_empty() {
            let _ = fs::remove_file(&self.path);
        } else {
            let _ = fs::write(&self.path, lines);
        }
    }
}

/// Full analyses of recent fights, one file each in `fights/`, named by when they ended.
pub struct FightDetails {
    dir: PathBuf,
}

impl FightDetails {
    pub fn new(config_dir: &Path) -> Self {
        Self {
            dir: config_dir.join("fights"),
        }
    }

    fn path(&self, ended_ms: u64) -> PathBuf {
        self.dir.join(format!("{ended_ms}.json"))
    }

    pub fn save(&self, ended_ms: u64, analysis: &Analysis) {
        if fs::create_dir_all(&self.dir).is_err() {
            return;
        }
        if let Ok(json) = serde_json::to_vec(analysis) {
            let tmp = self.dir.join(format!("{ended_ms}.json.tmp"));
            if fs::write(&tmp, json).is_ok() {
                let _ = fs::rename(&tmp, self.path(ended_ms));
            }
        }
        self.prune();
    }

    pub fn load(&self, ended_ms: u64) -> Option<Analysis> {
        serde_json::from_slice(&fs::read(self.path(ended_ms)).ok()?).ok()
    }

    pub fn has(&self, ended_ms: u64) -> bool {
        self.path(ended_ms).is_file()
    }

    /// Deletes every saved analysis.
    pub fn clear(&self) {
        let Ok(entries) = fs::read_dir(&self.dir) else { return };
        for e in entries.flatten() {
            if e.file_name().to_str().is_some_and(|n| n.ends_with(".json")) {
                let _ = fs::remove_file(e.path());
            }
        }
    }

    /// Keeps the newest [`DETAILS_LIMIT`] files.
    fn prune(&self) {
        let Ok(entries) = fs::read_dir(&self.dir) else { return };
        let mut stamps: Vec<u64> = entries
            .filter_map(|e| e.ok()?.file_name().to_str()?.strip_suffix(".json")?.parse().ok())
            .collect();
        stamps.sort_unstable();
        let excess = stamps.len().saturating_sub(DETAILS_LIMIT);
        for ended_ms in &stamps[..excess] {
            let _ = fs::remove_file(self.path(*ended_ms));
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

/// Your best against one boss.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Best {
    /// Your highest DPS in a kill, and when that fight ended (ms since 1970).
    pub dps: f64,
    pub dps_ms: u64,
    /// The fastest kill (the fight's length), and when it ended.
    pub kill_s: f64,
    pub kill_ms: u64,
    pub kills: u32,
}

/// Your bests against each boss, per character as Questlog keeps them (`bests.json`), from
/// every kill you were in. Kept apart from the history, which only keeps the last fights.
pub struct Bests {
    path: PathBuf,
    /// Character name -> mob type -> best. Bests saved before they were per character
    /// are under "" and count for any character without its own.
    by_character: BTreeMap<String, BTreeMap<u32, Best>>,
}

#[derive(Serialize, Deserialize)]
struct BestsFile {
    characters: BTreeMap<String, BTreeMap<u32, Best>>,
}

impl Bests {
    pub fn load(dir: &Path) -> Self {
        let path = dir.join("bests.json");
        let text = fs::read_to_string(&path).unwrap_or_default();
        let by_character = match serde_json::from_str::<BestsFile>(&text) {
            Ok(f) => f.characters,
            // The first version: one map by mob type.
            Err(_) => serde_json::from_str::<BTreeMap<u32, Best>>(&text)
                .map(|old| BTreeMap::from([(String::new(), old)]))
                .unwrap_or_default(),
        };
        Self { path, by_character }
    }

    fn save(&self) {
        if let Some(dir) = self.path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        let file = BestsFile { characters: self.by_character.clone() };
        if let Ok(s) = serde_json::to_string_pretty(&file) {
            let _ = fs::write(&self.path, s);
        }
    }

    /// Your best against `boss` as `character` (or from before bests were per character).
    pub fn get(&self, character: Option<&str>, boss: u32) -> Option<Best> {
        [character.unwrap_or(""), ""].iter().find_map(|c| self.by_character.get(*c)?.get(&boss).copied())
    }

    /// Counts a finished fight if it was a boss kill you were in.
    pub fn record(&mut self, view: &EncounterView, ended_ms: u64, character: Option<&str>) {
        let (Some(boss), true, Some(FightEnd::Kill)) = (view.target_type, view.target_boss, view.end) else { return };
        let Some(me) = view.players.iter().find(|p| p.is_self).filter(|p| p.dps > 0.0) else { return };
        let b = self
            .by_character
            .entry(character.unwrap_or("").to_string())
            .or_default()
            .entry(boss)
            .or_insert(Best { dps: 0.0, dps_ms: 0, kill_s: f64::INFINITY, kill_ms: 0, kills: 0 });
        b.kills += 1;
        if me.dps > b.dps {
            (b.dps, b.dps_ms) = (me.dps, ended_ms);
        }
        if view.duration_s < b.kill_s {
            (b.kill_s, b.kill_ms) = (view.duration_s, ended_ms);
        }
        self.save();
    }

    pub fn clear(&mut self) {
        self.by_character.clear();
        let _ = fs::remove_file(&self.path);
    }
}

/// Each character's pets: level and souls toward the next one (`pets.json`), so a meter
/// started after the game sent its pet list (at login or a zone change) still knows them.
pub struct PetBook {
    path: PathBuf,
    by_character: BTreeMap<String, meter_core::pets::PetState>,
    /// The meter's pet revision last saved.
    saved: u64,
}

impl PetBook {
    pub fn load(dir: &Path) -> Self {
        let path = dir.join("pets.json");
        let by_character = fs::read_to_string(&path).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        Self { path, by_character, saved: 0 }
    }

    /// Hands a saved state to a meter that doesn't know your pets yet, and saves its state
    /// when it has changed.
    pub fn sync(&mut self, character: &str, pets: &mut meter_core::pets::Pets) {
        if !pets.known() {
            if let Some(state) = self.by_character.get(character) {
                pets.seed(state);
                self.saved = pets.revision();
            }
            return;
        }
        if pets.revision() == self.saved {
            return;
        }
        if let Some(state) = pets.state() {
            self.by_character.insert(character.to_string(), state);
            self.saved = pets.revision();
            if let Some(dir) = self.path.parent() {
                let _ = fs::create_dir_all(dir);
            }
            if let Ok(s) = serde_json::to_string(&self.by_character) {
                let _ = fs::write(&self.path, s);
            }
        }
    }
}

/// One line of the history list.
#[derive(Debug, Clone, Serialize)]
pub struct FightSummary {
    /// Position in the list (pass to `history_fight`).
    pub index: usize,
    pub started_ms: u64,
    pub ended_ms: u64,
    pub target: Option<String>,
    /// The main target's mob type.
    pub target_type: Option<u32>,
    /// When you entered the map it was on (the same for a dungeon run's fights).
    pub instance_us: Option<u64>,
    pub target_boss: bool,
    pub duration_s: f64,
    pub total_damage: u64,
    pub total_dps: f64,
    pub players: usize,
    pub my_dps: Option<f64>,
    #[serde(skip)]
    pub my_damage: Option<u64>,
    /// A full analysis is saved for it (the newest fights only).
    pub has_details: bool,
    /// Where it was, when its mob is a dungeon's (filled in by [`crate::runs`]).
    pub dungeon: Option<crate::runs::Place>,
    /// The dungeon run it's part of: a position in the list's runs.
    pub run: Option<usize>,
    /// How it ended (unknown for fights saved before this was read).
    pub end: Option<FightEnd>,
    /// Deaths of the players shown.
    pub deaths: usize,
    /// Your best DPS against its boss, or its fastest kill (filled in for the list).
    pub best_dps: bool,
    pub fastest: bool
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

    /// Newest first. `details` says which fights have a saved analysis.
    pub fn summaries(&self, details: &FightDetails) -> Vec<FightSummary> {
        self.fights
            .iter()
            .enumerate()
            .rev()
            .map(|(index, f)| FightSummary {
                index,
                started_ms: f.view.start_us / 1000,
                ended_ms: f.ended_ms,
                target: f.view.target.clone(),
                target_type: f.view.target_type,
                instance_us: f.view.instance_us,
                target_boss: f.view.target_boss,
                duration_s: f.view.duration_s,
                total_damage: f.view.total_damage,
                total_dps: f.view.total_dps,
                players: f.view.players.len(),
                my_dps: f.view.players.iter().find(|p| p.is_self).map(|p| p.dps),
                my_damage: f.view.players.iter().find(|p| p.is_self).map(|p| p.damage),
                has_details: details.has(f.ended_ms),
                dungeon: None,
                run: None,
                end: f.view.end,
                deaths: f.view.deaths.len(),
                best_dps: false,
                fastest: false,
            })
            .collect()
    }

    pub fn fight(&self, index: usize) -> Option<&Fight> {
        self.fights.get(index)
    }

    pub fn newest_first(&self) -> impl Iterator<Item = &Fight> {
        self.fights.iter().rev()
    }

    pub fn clear(&mut self) {
        self.fights.clear();
        let _ = fs::remove_file(&self.path);
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
            party: Vec::new(),
            target: None,
            in_combat: false,
            encounter: Some(view),
            encounters_completed: 0,
        })
        .unwrap()
    }

    #[test]
    fn known_players_only_come_back_for_the_same_login() {
        let dir = temp_dir("players");
        assert_eq!(KnownPlayers::load(&dir).for_login(3199, 0), None);
        let k = KnownPlayers {
            self_id: 3199,
            saved_ms: 1_000_000,
            players: [(15772, "Pingu".to_string())].into(),
        };
        k.save(&dir);
        let names = KnownPlayers::load(&dir).for_login(3199, 1_000_000 + 60_000).unwrap();
        assert_eq!(names.get(&15772).map(String::as_str), Some("Pingu"));
        // A new login gives you a new entity ID; their IDs changed too.
        assert_eq!(KnownPlayers::load(&dir).for_login(4100, 1_060_000), None);
        // Too old to trust.
        assert_eq!(KnownPlayers::load(&dir).for_login(3199, 1_000_000 + KNOWN_PLAYERS_MAX_AGE_MS + 1), None);
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn missing_or_odd_refresh_rates_become_the_default() {
        let dir = temp_dir("refresh");
        assert_eq!(Settings::load(&dir).refresh_ms, DEFAULT_REFRESH_MS);
        fs::write(dir.join("settings.json"), r#"{ "refresh_ms": 7, "locked": true }"#).unwrap();
        let s = Settings::load(&dir);
        assert_eq!((s.refresh_ms, s.locked), (DEFAULT_REFRESH_MS, true));
        fs::write(dir.join("settings.json"), r#"{ "refresh_ms": 2000 }"#).unwrap();
        assert_eq!(Settings::load(&dir).refresh_ms, 2000);
        fs::write(dir.join("settings.json"), format!(r#"{{ "refresh_ms": {REALTIME} }}"#)).unwrap();
        assert_eq!(Settings::load(&dir).refresh_ms, REALTIME);
        fs::remove_dir_all(dir).ok();
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
        let list = again.summaries(&FightDetails::new(&dir));
        assert_eq!(list[0].ended_ms, 2);
        assert_eq!(list[0].index, 2);
        assert_eq!(list[0].my_dps, Some(10.0));
        assert_eq!(again.fight(0).unwrap().view.total_dps, 0.0);
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn pets_kept_per_character() {
        use meter_core::pets::{Pets, OPCODE_PET_LIST, OPCODE_SOULS_GAINED};
        let dir = temp_dir("pets");
        let mut book = PetBook::load(&dir);
        // The game's list: Vivel (1188) at level 1 with 8 souls.
        let mut list = vec![0x24, 0x04, 0, 0, 1];
        for v in [1188u32, 1188, 1] {
            list.extend(v.to_le_bytes());
        }
        list.push(1);
        for v in [1188u32, 8] {
            list.extend(v.to_le_bytes());
        }
        let mut pets = Pets::default();
        pets.feed(OPCODE_PET_LIST, &list);
        book.sync("NeneneTowawa", &mut pets);
        // Next run, started after the list: the saved state, then a soul on top.
        let mut book = PetBook::load(&dir);
        let mut later = Pets::default();
        book.sync("NeneneTowawa", &mut later);
        let soul = [1, 0xa4, 0x04, 0, 0, 1, 0, 0, 0];
        later.feed(OPCODE_SOULS_GAINED, &soul);
        let g = later.take_gains();
        assert_eq!((g[0].known, g[0].souls, g[0].next_level_souls), (true, 9, Some(25)));
        // Another character knows nothing of them.
        let mut other = Pets::default();
        book.sync("Alt", &mut other);
        assert!(!other.known());
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn bests_per_boss() {
        let dir = temp_dir("bests");
        let mut b = Bests::load(&dir);
        let kill = |dps: f64, secs: f64, end| {
            let mut view = snap(None, Some(18)).encounter.unwrap();
            (view.target_type, view.target_boss, view.end, view.duration_s) = (Some(2_300_812), true, end, secs);
            view.players[0].is_self = true;
            view.players[0].dps = dps;
            view
        };
        let me = Some("NeneneTowawa");
        b.record(&kill(20_000.0, 120.0, Some(FightEnd::Kill)), 1, me);
        b.record(&kill(25_000.0, 130.0, Some(FightEnd::Kill)), 2, me);
        b.record(&kill(90_000.0, 30.0, Some(FightEnd::Wipe)), 3, me); // a wipe isn't a best
        let best = Bests::load(&dir).get(me, 2_300_812).unwrap();
        assert_eq!((best.dps, best.dps_ms, best.kill_s, best.kill_ms, best.kills), (25_000.0, 2, 120.0, 1, 2));
        // Another character has its own.
        assert!(Bests::load(&dir).get(Some("Alt"), 2_300_812).is_none());
        b.record(&kill(10_000.0, 200.0, Some(FightEnd::Kill)), 4, Some("Alt"));
        assert_eq!(Bests::load(&dir).get(Some("Alt"), 2_300_812).unwrap().dps, 10_000.0);
        assert_eq!(Bests::load(&dir).get(me, 2_300_812).unwrap().dps, 25_000.0);
        b.clear();
        assert!(Bests::load(&dir).get(me, 2_300_812).is_none());
        // A file from before bests were per character counts for anyone.
        fs::write(dir.join("bests.json"), r#"{"2300812":{"dps":5.0,"dps_ms":1,"kill_s":9.0,"kill_ms":1,"kills":1}}"#).unwrap();
        assert_eq!(Bests::load(&dir).get(me, 2_300_812).unwrap().dps, 5.0);
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn clearing_the_history_forgets_the_fights_and_their_analyses() {
        let dir = temp_dir("clear");
        let mut h = History::load(&dir);
        h.push(Fight { ended_ms: 7, self_name: None, view: snap(None, Some(18)).encounter.unwrap() });
        let details = FightDetails::new(&dir);
        fs::create_dir_all(dir.join("fights")).unwrap();
        fs::write(dir.join("fights/7.json"), "{}").unwrap();
        fs::write(dir.join("fights/notes.txt"), "not ours").unwrap();
        assert!(details.has(7));
        h.clear();
        details.clear();
        assert_eq!(h.len(), 0);
        assert_eq!(History::load(&dir).len(), 0, "gone from disk too");
        assert!(!details.has(7));
        assert!(dir.join("fights/notes.txt").exists(), "only analyses are deleted");

        // Dungeon runs: the past ones go, the one you're in stays.
        let run = |entered_us| meter_core::dungeon::Instance { map: 600_072, entered_us, started_us: Some(entered_us + 1), completed_us: None, stages: [0; meter_core::dungeon::MAX_STAGES] };
        let mut instances = Instances::load(&dir);
        instances.record(run(100));
        instances.record(run(200));
        instances.clear_except(Some(200));
        let again = Instances::load(&dir);
        assert!(again.get(100).is_none() && again.get(200).is_some());
        Instances::load(&dir).clear_except(None);
        assert!(Instances::load(&dir).get(200).is_none());
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
            font: Some("Consolas".into()),
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
    fn fight_details_are_saved_and_pruned() {
        let dir = temp_dir("details");
        let details = FightDetails::new(&dir);
        let view: EncounterView = serde_json::from_value(serde_json::json!({
            "active": false, "duration_s": 10.0, "total_damage": 5, "total_dps": 0.5,
            "target": null, "target_id": null, "target_type": null, "target_count": 1,
            "start_us": 0, "end_us": 10, "players": []
        }))
        .unwrap();
        let analysis = Analysis {
            view,
            hits: Vec::new(),
            effects: Vec::new(),
            names: Default::default(),
            mob_types: Default::default(),
            target_hp: Vec::new(),
            heals: Vec::new(),
            taken: Vec::new(),
        };
        for ended_ms in 1..=(DETAILS_LIMIT as u64 + 2) {
            details.save(ended_ms, &analysis);
        }
        assert!(!details.has(1) && !details.has(2));
        assert!(details.has(3));
        assert_eq!(details.load(DETAILS_LIMIT as u64 + 2), Some(analysis));
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn a_new_install_gets_the_defaults() {
        let s = Settings::load(&temp_dir("no-settings"));
        assert!(s.boss_hp);
        assert_eq!((s.alert_lead_min, s.timer_region.as_str(), s.hide_after_s), (5, "as", 0));
    }

    #[test]
    fn the_scope_follows_your_party_unless_chosen() {
        let mut s = Settings::default();
        assert_eq!((s.scope_for(0), s.scope_for(1), s.scope_for(5)), (Scope::Mine, Scope::Mine, Scope::Party));
        s.manual_scope = true;
        s.scope = Scope::Everyone;
        assert_eq!(s.scope_for(5), Scope::Everyone);
        // Settings from before this follow the party.
        let dir = temp_dir("auto-scope");
        fs::write(dir.join("settings.json"), r#"{ "scope": "everyone" }"#).unwrap();
        assert_eq!(Settings::load(&dir).scope_for(5), Scope::Party);
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn dungeon_runs_are_kept() {
        use meter_core::dungeon::Instance;
        let dir = temp_dir("instances");
        let mut i = Instances::load(&dir);
        let entered = Instance { map: 600_072, entered_us: 1, started_us: None, completed_us: None, stages: [0; meter_core::dungeon::MAX_STAGES] };
        i.record(entered);
        assert!(i.get(1).is_none(), "no countdown: not a dungeon");
        i.record(Instance { started_us: Some(2), ..entered });
        i.record(Instance { started_us: Some(2), completed_us: Some(9), ..entered });
        let again = Instances::load(&dir);
        assert_eq!(again.get(1).and_then(|i| i.completed_us), Some(9));
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
    fn colours_of_your_own() {
        let mut a = Appearance::default();
        a.update(AppearanceChange {
            theme: Some("#3A7BD5".into()),
            bar_color: Some("#00ff88".into()),
            highlight: Some("off".into()),
            ..AppearanceChange::default()
        });
        assert_eq!((a.theme.as_str(), a.bar_color.as_str(), a.highlight.as_str()), ("#3a7bd5", "#00ff88", "off"));
        // Not quite a colour: kept as it was.
        for bad in ["#12345", "#1234567", "3a7bd5x", "#ggg000", "red"] {
            a.update(AppearanceChange { theme: Some(bad.into()), ..AppearanceChange::default() });
        }
        assert_eq!(a.theme, "#3a7bd5");
        assert_eq!(a.clone().sanitized(), a);
    }

    #[test]
    fn appearance_stays_valid() {
        let mut a = Appearance::default();
        a.update(AppearanceChange {
            theme: Some("neon".into()),
            bar_color: Some("plaid".into()),
            highlight: Some("rainbow".into()),
            font_size: Some(99),
            font: Some("x\"; }".into()),
            opacity: Some(250),
            backdrop: Some("fog".into()),
            ..AppearanceChange::default()
        });
        assert_eq!(a.theme, "rose");
        assert_eq!(a.bar_color, "theme");
        assert_eq!(a.highlight, "self");
        assert_eq!(a.font_size, 18);
        assert_eq!(a.font, "");
        assert_eq!(a.opacity, 100);
        // A stepper that overshoots below zero (the old 62% default, minus 5s) clamps.
        a.update(AppearanceChange {
            opacity: Some(-3),
            font_size: Some(-1),
            ..AppearanceChange::default()
        });
        assert_eq!((a.opacity, a.font_size), (0, 10));
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
