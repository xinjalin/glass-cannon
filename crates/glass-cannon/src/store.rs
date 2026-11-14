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

/// Fights kept in memory and shown in the history list, boss fights and the rest each.
const HISTORY_LIMIT: usize = 200;

/// Which of `items` (oldest first) fall outside the newest `limit` boss fights and the
/// newest `limit` other fights, so a run of short fights can't push the boss fights out.
fn beyond_limit<T>(items: &[T], limit: usize, boss: impl Fn(&T) -> bool) -> Vec<bool> {
    let (mut bosses, mut others) = (0, 0);
    let mut drop: Vec<bool> = items
        .iter()
        .rev()
        .map(|i| {
            let seen = if boss(i) { &mut bosses } else { &mut others };
            *seen += 1;
            *seen > limit
        })
        .collect();
    drop.reverse();
    drop
}

/// `path` with `.suffix` after its whole name (`bests.json` -> `bests.json.bad`).
fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".");
    name.push(suffix);
    path.with_file_name(name)
}

/// Writes a file whole or not at all: to a temporary file beside it, then renamed over
/// it. A crash (or the PC losing power) in the middle leaves the file as it was, rather
/// than cut short.
pub(crate) fn write_atomic(path: &Path, bytes: impl AsRef<[u8]>) {
    if let Some(dir) = path.parent() {
        let _ = fs::create_dir_all(dir);
    }
    let tmp = with_suffix(path, "tmp");
    if fs::write(&tmp, bytes).is_ok() {
        let _ = fs::rename(&tmp, path);
    }
}

/// Reads a JSON file. One that's there but can't be read (damaged, or from a version this
/// one doesn't understand) is set aside as `<name>.bad`, so that starting over with
/// nothing doesn't write over it.
pub(crate) fn read_json<T: DeserializeOwned>(path: &Path) -> Option<T> {
    let bytes = fs::read(path).ok()?;
    match serde_json::from_slice(&bytes) {
        Ok(value) => Some(value),
        Err(_) => {
            set_aside(path);
            None
        }
    }
}

fn set_aside(path: &Path) {
    let _ = fs::rename(path, with_suffix(path, "bad"));
}

fn load<T: DeserializeOwned + Default>(path: &Path) -> T {
    read_json(path).unwrap_or_default()
}

fn save<T: Serialize>(path: &Path, value: &T) {
    if let Ok(json) = serde_json::to_vec_pretty(value) {
        write_atomic(path, json);
    }
}

/// A `.jsonl` log read back: what could be read, and how many lines there were.
fn read_lines<T: DeserializeOwned>(path: &Path) -> (Vec<T>, usize) {
    let text = fs::read_to_string(path).unwrap_or_default();
    let lines = text.lines().filter(|l| !l.trim().is_empty());
    (lines.clone().filter_map(|l| serde_json::from_str(l).ok()).collect(), lines.count())
}

/// Rewrites a `.jsonl` log with just `items`, whole or not at all. The logs are added to
/// a line at a time and only trimmed in memory, so without this they'd grow for ever (and
/// all of it be read at every start). With `keep_old`, the file as it was is kept beside
/// it as `<name>.bak` first: it had lines this version couldn't read.
fn rewrite_lines<'a, T: Serialize + 'a>(path: &Path, items: impl IntoIterator<Item = &'a T>, keep_old: bool) {
    if keep_old {
        let _ = fs::copy(path, with_suffix(path, "bak"));
    }
    let text: String = items.into_iter().filter_map(|i| serde_json::to_string(i).ok()).map(|l| l + "\n").collect();
    write_atomic(path, text);
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
    /// What the meter's rows show.
    #[serde(default)]
    pub meter_view: MeterView,
    /// The overlay's widgets that show (see [`crate::widgets::WIDGETS`]).
    #[serde(default = "default_widgets")]
    pub widgets: Vec<String>,
    /// Older settings files: the side windows left open, which are widgets now.
    #[serde(default, skip_serializing)]
    panels: Option<Vec<String>>,
    /// Whose event schedule the timers follow: one of [`TIMER_REGIONS`].
    #[serde(default = "default_timer_region")]
    pub timer_region: String,
    /// Events (their timer IDs) and field bosses ("boss:<id>") to be told about before
    /// they start, and how many minutes before.
    #[serde(default)]
    pub alerts: Vec<String>,
    #[serde(default = "default_alert_lead")]
    pub alert_lead_min: u32,
    /// Events and field bosses you starred in the world timers, and whether only those show.
    #[serde(default)]
    pub timer_favourites: Vec<String>,
    #[serde(default)]
    pub timer_favourites_only: bool,
    /// The overlay's fight widgets fade away this many seconds after combat ends (0: they
    /// don't), and come back in the next fight (the meter under the mouse too).
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
    /// The overlay is on (the launcher starts and stops it): it shows over the game
    /// whenever the game is in front.
    #[serde(default = "default_true")]
    pub overlay_on: bool,
    /// Boss skills you marked: true always warns, false never (the rest go by how
    /// dangerous they look).
    #[serde(default)]
    pub boss_skill_alerts: BTreeMap<u32, bool>,
    /// Dungeon bosses' mechanics in the boss warnings (`gamedata/boss-mechanics.json`).
    #[serde(default = "default_true")]
    pub boss_mechanics: bool,
    /// The party cooldowns widget grouped by class, then skill, then player (else by player).
    #[serde(default)]
    pub party_cooldowns_by_class: bool,
    /// The black market window opens by itself when the merchant is opened in game.
    #[serde(default = "default_true")]
    pub black_market_auto: bool,
    /// The meter's height fits what it shows (its rows, boss lines, open skill lists); off,
    /// it's the height you give it while arranging.
    #[serde(default = "default_true")]
    pub meter_fit: bool,
    /// The party view's order: the roster's, or by combat power or gear score, highest or
    /// lowest first (one of [`PARTY_SORTS`]).
    #[serde(default = "default_party_sort")]
    pub party_sort: String,
    /// The widgets that fade outside combat (settings → fade outside combat; the arrange
    /// menu's fade column picks them). The others always show.
    #[serde(default = "default_fading")]
    pub fading: Vec<String>,
    /// The cooldowns widget's look: "list" (a row each) or "timeline" (icons sliding along a
    /// bar to Ready as their cooldowns run down).
    #[serde(default = "default_cooldowns_look")]
    pub cooldowns_look: String,
    /// The timeline's Ready end on the right (else the left).
    #[serde(default)]
    pub cooldowns_ready_right: bool,
    /// The timeline's size, in percent (its icons, times and labels).
    #[serde(default = "default_cooldowns_size")]
    pub cooldowns_size: u16,
    /// The timeline's scale: "percent" (how much of each cooldown is left) or "time" (the
    /// time left, marked 1, 10, 30, 1m, 2m, 5m).
    #[serde(default = "default_cooldowns_scale")]
    pub cooldowns_scale: String,
    /// How the action bars' bars look (the bars look): their background and colours.
    #[serde(default)]
    pub action_bar_look: crate::actionbar::BarLook,
    /// Each class's action bar (by class code): its skills, buffs and look.
    #[serde(default)]
    pub action_bars: BTreeMap<String, crate::actionbar::ActionBarSetup>,
    /// How the vitals arcs widget looks.
    #[serde(default)]
    pub vitals: crate::vitals::VitalsSetup,
    /// A sound with each boss warning in the overlay.
    #[serde(default = "default_true")]
    pub alert_sound: bool,
    /// The boss warnings say when the boss turns to attack you.
    #[serde(default = "default_true")]
    pub boss_on_you: bool,
    /// The overlay steps aside while the world map is open.
    #[serde(default = "default_true")]
    pub hide_on_world_map: bool,
    /// The launcher kept on top of other windows (its green light).
    #[serde(default)]
    pub launcher_on_top: bool,
    /// Dummy mode: hitting a training dummy times a run of `dummy_run_s` seconds.
    #[serde(default = "default_true")]
    pub dummy_mode: bool,
    #[serde(default = "default_dummy_run_s")]
    pub dummy_run_s: u32,
    /// The label new dummy runs get.
    #[serde(default)]
    pub dummy_label: String,
    /// The pets each character tracks (the pet tracker widget).
    #[serde(default)]
    pub tracked_pets: BTreeMap<String, Vec<u32>>,
    /// The pet tracker lists every pet you still need, those in view first.
    #[serde(default)]
    pub pet_tracker_nearest: bool,
    /// Close the app a few seconds after the game closes.
    #[serde(default)]
    pub close_with_game: bool,
    /// The overlay steps aside while a game menu is open (its close X or back arrow in the
    /// corner while the cursor shows).
    #[serde(default = "default_true")]
    pub hide_for_menus: bool,
    /// No widget takes the mouse while you play, not even the meter, the world timers and
    /// the pet tracker (the others never do); arranging them still does.
    #[serde(default)]
    pub click_through: bool,
    /// The hotkeys' keys (settings → keybinds).
    #[serde(default)]
    pub hotkeys: crate::hotkeys::Bindings,
}


/// The party view's orders.
pub const PARTY_SORTS: [&str; 5] = ["roster", "cp_desc", "cp_asc", "gs_desc", "gs_asc"];

fn default_party_sort() -> String {
    PARTY_SORTS[0].into()
}

/// The widgets that fade outside combat unless chosen otherwise: those of the fight.
fn default_fading() -> Vec<String> {
    ["meter", "boss-timers", "target", "cooldowns", "party-cooldowns"].map(String::from).to_vec()
}

/// The cooldowns widget's looks.
pub const COOLDOWNS_LOOKS: [&str; 2] = ["list", "timeline"];

fn default_cooldowns_look() -> String {
    COOLDOWNS_LOOKS[0].into()
}

/// The timeline's sizes, in percent.
pub const COOLDOWNS_SIZES: std::ops::RangeInclusive<u16> = 70..=250;

fn default_cooldowns_size() -> u16 {
    100
}

/// The timeline's scales.
pub const COOLDOWNS_SCALES: [&str; 2] = ["percent", "time"];

fn default_cooldowns_scale() -> String {
    COOLDOWNS_SCALES[0].into()
}

fn default_dummy_run_s() -> u32 {
    crate::dummy::DEFAULT_RUN_S
}

fn default_alert_lead() -> u32 {
    5
}
fn default_true() -> bool {
    true
}
fn default_widgets() -> Vec<String> {
    ["meter", "boss-warnings", "boss-timers", "cubes"].map(String::from).to_vec()
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

pub const THEMES: [&str; 7] = ["night", "rose", "lavender", "mint", "sky", "amber", "mono"];
pub const HIGHLIGHTS: [&str; 3] = ["self", "all", "off"];
pub const BACKDROPS: [&str; 3] = ["frosted", "tinted", "clear"];
/// How the meter's rows show each player's share: a strip of blocks beside the numbers, or
/// the row itself filled that far.
pub const ROW_STYLES: [&str; 2] = ["blocks", "fill"];
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
    /// An installed font family (Calibri Light unless chosen); empty for the built-in
    /// monospace (Cascadia Mono).
    #[serde(default = "default_font")]
    pub font: String,
    /// How opaque the dark tint over the background is, in percent.
    #[serde(default = "default_opacity")]
    pub opacity: u8,
    /// "frosted": blurred glass; "tinted": see-through tint, no blur; "clear": nothing at all.
    #[serde(default = "default_backdrop")]
    pub backdrop: String,
    /// A thin strip at the top with the local time.
    #[serde(default)]
    pub info_bar: bool,
    /// One of [`ROW_STYLES`].
    #[serde(default = "default_row_style")]
    pub row_style: String,
    /// The rows' fill as see-through as the tint (else solid).
    #[serde(default)]
    pub fill_tint: bool,
    /// Each player's combat power by their name on the meter.
    #[serde(default = "default_true")]
    pub combat_power: bool,
    /// Your Odyle energy on the meter's top strip.
    #[serde(default = "default_true")]
    pub odyle: bool,
    /// Colours of your own for classes (by class code, "11" to "19": `#rrggbb`), in place of
    /// the built-in ones, wherever class colours show.
    #[serde(default)]
    pub class_palette: BTreeMap<String, String>,
    /// The text's colour: "theme" (the theme's), or a colour of your own (`#rrggbb`).
    #[serde(default = "default_theme_word")]
    pub text_color: String,
    /// How opaque the text is, in percent.
    #[serde(default = "default_text_opacity")]
    pub text_opacity: u8,
    /// The text's edge: one of [`TEXT_EDGES`] ("theme": the theme's soft shadow).
    #[serde(default = "default_theme_word")]
    pub text_edge: String,
    /// The edge's colour (`#rrggbb`).
    #[serde(default = "default_edge_color")]
    pub edge_color: String,
}

/// The text's edges: the theme's, none, a drop shadow, an outline, raised, depressed.
pub const TEXT_EDGES: [&str; 6] = ["theme", "none", "shadow", "outline", "raised", "depressed"];
pub const TEXT_OPACITIES: std::ops::RangeInclusive<u8> = 40..=100;

fn default_theme_word() -> String {
    "theme".into()
}
fn default_text_opacity() -> u8 {
    100
}
fn default_edge_color() -> String {
    "#000000".into()
}

/// The class codes a colour can be set for.
pub const CLASS_CODES: std::ops::RangeInclusive<u32> = 11..=19;

/// Keeps the class colours that are a class and a colour.
fn valid_palette(p: BTreeMap<String, String>) -> BTreeMap<String, String> {
    p.into_iter()
        .filter(|(k, v)| k.parse::<u32>().is_ok_and(|c| CLASS_CODES.contains(&c)) && is_hex_color(v))
        .map(|(k, v)| (k, v.to_ascii_lowercase()))
        .collect()
}

/// A colour picked by hand: `#` and six hex digits.
fn is_hex_color(s: &str) -> bool {
    s.len() == 7 && s.starts_with('#') && s[1..].chars().all(|c| c.is_ascii_hexdigit())
}

fn default_theme() -> String {
    "night".into()
}
fn default_bar_color() -> String {
    "theme".into()
}
fn default_highlight() -> String {
    "self".into()
}
fn default_font_size() -> u8 {
    14
}
fn default_font() -> String {
    "Calibri Light".into()
}
fn default_opacity() -> u8 {
    62
}
fn default_backdrop() -> String {
    "frosted".into()
}
fn default_row_style() -> String {
    ROW_STYLES[0].into()
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            theme: default_theme(),
            bar_color: default_bar_color(),
            class_colors: false,
            highlight: default_highlight(),
            font_size: default_font_size(),
            font: default_font(),
            opacity: default_opacity(),
            backdrop: default_backdrop(),
            info_bar: false,
            row_style: default_row_style(),
            fill_tint: false,
            combat_power: true,
            odyle: true,
            class_palette: BTreeMap::new(),
            text_color: default_theme_word(),
            text_opacity: default_text_opacity(),
            text_edge: default_theme_word(),
            edge_color: default_edge_color(),
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
    pub info_bar: Option<bool>,
    pub row_style: Option<String>,
    pub fill_tint: Option<bool>,
    pub combat_power: Option<bool>,
    pub odyle: Option<bool>,
    /// The whole set of class colours of your own (an empty one: the built-in colours).
    pub class_palette: Option<BTreeMap<String, String>>,
    pub text_color: Option<String>,
    /// Any number: clamped.
    pub text_opacity: Option<i64>,
    pub text_edge: Option<String>,
    pub edge_color: Option<String>,
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
        if let Some(v) = c.info_bar {
            self.info_bar = v;
        }
        if let Some(r) = c.row_style.filter(|r| ROW_STYLES.contains(&r.as_str())) {
            self.row_style = r;
        }
        if let Some(v) = c.fill_tint {
            self.fill_tint = v;
        }
        if let Some(v) = c.combat_power {
            self.combat_power = v;
        }
        if let Some(v) = c.odyle {
            self.odyle = v;
        }
        if let Some(p) = c.class_palette {
            self.class_palette = valid_palette(p);
        }
        if let Some(t) = c.text_color.filter(|t| t == "theme" || is_hex_color(t)) {
            self.text_color = t.to_ascii_lowercase();
        }
        if let Some(o) = c.text_opacity {
            self.text_opacity = o.clamp(i64::from(*TEXT_OPACITIES.start()), i64::from(*TEXT_OPACITIES.end())) as u8;
        }
        if let Some(e) = c.text_edge.filter(|e| TEXT_EDGES.contains(&e.as_str())) {
            self.text_edge = e;
        }
        if let Some(e) = c.edge_color.filter(|e| is_hex_color(e)) {
            self.edge_color = e.to_ascii_lowercase();
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
        if !ROW_STYLES.contains(&self.row_style.as_str()) {
            self.row_style = fixed.row_style;
        }
        self.class_palette = valid_palette(std::mem::take(&mut self.class_palette));
        if self.text_color != "theme" && !is_hex_color(&self.text_color) {
            self.text_color = fixed.text_color.clone();
        }
        if !TEXT_OPACITIES.contains(&self.text_opacity) {
            self.text_opacity = fixed.text_opacity;
        }
        if !TEXT_EDGES.contains(&self.text_edge.as_str()) {
            self.text_edge = fixed.text_edge.clone();
        }
        if !is_hex_color(&self.edge_color) {
            self.edge_color = fixed.edge_color.clone();
        }
        self
    }
}

impl Settings {
    pub fn load(dir: &Path) -> Self {
        let mut s: Settings = load(&dir.join("settings.json"));
        s.appearance = s.appearance.sanitized();
        s.vitals = std::mem::take(&mut s.vitals).sanitized();
        s.action_bar_look = std::mem::take(&mut s.action_bar_look).sanitized();
        if std::mem::take(&mut s.everyone) && s.scope == Scope::Mine {
            s.scope = Scope::Everyone;
        }
        if !REFRESH_RATES.contains(&s.refresh_ms) {
            s.refresh_ms = DEFAULT_REFRESH_MS;
        }
        // Side windows left open are widgets now (the history stays a window).
        for name in s.panels.take().unwrap_or_default() {
            if crate::widgets::WIDGETS.contains(&name.as_str()) && !s.widgets.contains(&name) {
                s.widgets.push(name);
            }
        }
        s.widgets.retain(|w| crate::widgets::WIDGETS.contains(&w.as_str()));
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

/// Fights kept with their full analysis, boss fights and the rest each (the rest of the
/// history keeps its summary).
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
        let mut f: Self = read_json(&path).unwrap_or_default();
        f.path = path;
        f
    }

    fn save(&self) {
        if let Ok(s) = serde_json::to_string_pretty(self) {
            write_atomic(&self.path, s);
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
        let mut c: Self = read_json(&path).unwrap_or_default();
        c.path = path;
        c
    }

    fn save(&self) {
        if let Ok(s) = serde_json::to_string_pretty(self) {
            write_atomic(&self.path, s);
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
        let (mut sessions, lines): (Vec<meter_core::pvp::PvpSession>, usize) = read_lines(&path);
        let extra = sessions.len().saturating_sub(PVP_LIMIT);
        sessions.drain(..extra);
        // Older sessions than are kept: off the file too.
        if extra > 0 {
            rewrite_lines(&path, &sessions, lines > sessions.len() + extra);
        }
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
        let extra = self.sessions.len().saturating_sub(2 * PVP_LIMIT);
        if extra > 0 {
            self.sessions.drain(..extra + PVP_LIMIT);
            rewrite_lines(&self.path, &self.sessions, false);
        }
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
        let (changes, lines): (Vec<meter_core::dungeon::Instance>, usize) = read_lines(&path);
        let read = changes.len();
        let mut by_entered: BTreeMap<u64, meter_core::dungeon::Instance> = changes.into_iter().map(|i| (i.entered_us, i)).collect();
        while by_entered.len() > INSTANCES_LIMIT {
            by_entered.pop_first();
        }
        // A line per change: once the file is mostly superseded lines, just the runs.
        if read > 2 * by_entered.len() + 50 {
            rewrite_lines(&path, by_entered.values(), lines > read);
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
            write_atomic(&self.path, lines);
        }
    }
}

/// Full analyses of recent fights, one file each in `fights/`, named by when they ended:
/// `<ms>.boss.json` for a boss fight, `<ms>.json` for the rest.
pub struct FightDetails {
    dir: PathBuf,
}

impl FightDetails {
    pub fn new(config_dir: &Path) -> Self {
        Self {
            dir: config_dir.join("fights"),
        }
    }

    fn file(&self, ended_ms: u64, boss: bool) -> PathBuf {
        self.dir.join(if boss { format!("{ended_ms}.boss.json") } else { format!("{ended_ms}.json") })
    }

    /// The fight's file, whichever kind it was saved as.
    fn path(&self, ended_ms: u64) -> Option<PathBuf> {
        [true, false].into_iter().map(|boss| self.file(ended_ms, boss)).find(|p| p.is_file())
    }

    /// `boss`: the fight was against a boss, kept apart from the rest.
    pub fn save(&self, ended_ms: u64, analysis: &Analysis, boss: bool) {
        if fs::create_dir_all(&self.dir).is_err() {
            return;
        }
        if let Ok(json) = serde_json::to_vec(analysis) {
            let tmp = self.dir.join(format!("{ended_ms}.json.tmp"));
            if fs::write(&tmp, json).is_ok() {
                let _ = fs::rename(&tmp, self.file(ended_ms, boss));
            }
        }
        self.prune();
    }

    pub fn load(&self, ended_ms: u64) -> Option<Analysis> {
        serde_json::from_slice(&fs::read(self.path(ended_ms)?).ok()?).ok()
    }

    pub fn has(&self, ended_ms: u64) -> bool {
        self.path(ended_ms).is_some()
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

    /// Keeps the newest [`DETAILS_LIMIT`] boss fights and as many others.
    fn prune(&self) {
        let Ok(entries) = fs::read_dir(&self.dir) else { return };
        let mut saved: Vec<(u64, bool)> = entries
            .filter_map(|e| {
                let name = e.ok()?.file_name().into_string().ok()?;
                let stem = name.strip_suffix(".json")?;
                let (stamp, boss) = stem.strip_suffix(".boss").map_or((stem, false), |s| (s, true));
                Some((stamp.parse().ok()?, boss))
            })
            .collect();
        saved.sort_unstable();
        let drop = beyond_limit(&saved, DETAILS_LIMIT, |(_, boss)| *boss);
        for (&(ended_ms, boss), _) in saved.iter().zip(drop).filter(|(_, d)| *d) {
            let _ = fs::remove_file(self.file(ended_ms, boss));
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
            Err(_) => match serde_json::from_str::<BTreeMap<u32, Best>>(&text) {
                Ok(old) => BTreeMap::from([(String::new(), old)]),
                Err(_) => {
                    if !text.is_empty() {
                        set_aside(&path);
                    }
                    BTreeMap::new()
                }
            },
        };
        Self { path, by_character }
    }

    fn save(&self) {
        let file = BestsFile { characters: self.by_character.clone() };
        if let Ok(s) = serde_json::to_string_pretty(&file) {
            write_atomic(&self.path, s);
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

/// One boss's attempts as one character.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Tally {
    pub kills: u32,
    pub wipes: u32,
    /// When the first and the latest attempt ended (ms since 1970).
    pub first_ms: u64,
    pub last_ms: u64,
    /// The first kill, when.
    #[serde(default)]
    pub first_kill_ms: Option<u64>,
    /// The fastest kill (the fight's length).
    #[serde(default)]
    pub fastest_kill_s: Option<f64>,
    /// The best wipe: the lowest the boss got (a share of its HP), and when it ended.
    #[serde(default)]
    pub best_wipe: Option<f64>,
    #[serde(default)]
    pub best_wipe_ms: u64,
    /// Time in its fights, seconds.
    #[serde(default)]
    pub fight_s: f64,
}

impl Tally {
    pub fn attempts(&self) -> u32 {
        self.kills + self.wipes
    }
}

/// Every boss kill and wipe, per character and boss (`attempts.json`): how many tries a
/// boss took, and the best of the wipes. Kept apart from the history, which only keeps the
/// last fights, and not cleared with it.
pub struct Attempts {
    path: PathBuf,
    /// Character name ("" when it wasn't known) -> boss mob type -> its attempts.
    by_character: BTreeMap<String, BTreeMap<u32, Tally>>,
}

#[derive(Serialize, Deserialize)]
struct AttemptsFile {
    characters: BTreeMap<String, BTreeMap<u32, Tally>>,
}

impl Attempts {
    /// Loads `attempts.json`; with none yet, counts the fights in `history` (so the tries
    /// before this was kept count too) and saves them.
    pub fn load(dir: &Path, history: &History) -> Self {
        let path = dir.join("attempts.json");
        let saved = read_json::<AttemptsFile>(&path);
        let mut a = Self { path, by_character: saved.as_ref().map(|f| f.characters.clone()).unwrap_or_default() };
        if saved.is_none() {
            for f in &history.fights {
                a.count(&f.view, f.ended_ms, f.self_name.as_deref());
            }
            if !a.by_character.is_empty() {
                a.save();
            }
        }
        a
    }

    fn save(&self) {
        let file = AttemptsFile { characters: self.by_character.clone() };
        if let Ok(s) = serde_json::to_string_pretty(&file) {
            write_atomic(&self.path, s);
        }
    }

    /// Counts a finished fight if it was a boss kill or wipe; true if it counted.
    fn count(&mut self, view: &EncounterView, ended_ms: u64, character: Option<&str>) -> bool {
        let (Some(boss), true) = (view.target_type, view.target_boss) else { return false };
        let kill = match view.end {
            Some(FightEnd::Kill) => true,
            Some(FightEnd::Wipe) => false,
            _ => return false,
        };
        let t = self.by_character.entry(character.unwrap_or("").to_string()).or_default().entry(boss).or_default();
        if t.attempts() == 0 || ended_ms < t.first_ms {
            t.first_ms = ended_ms;
        }
        t.last_ms = t.last_ms.max(ended_ms);
        t.fight_s += view.duration_s;
        if kill {
            t.kills += 1;
            if t.first_kill_ms.is_none_or(|ms| ended_ms < ms) {
                t.first_kill_ms = Some(ended_ms);
            }
            if t.fastest_kill_s.is_none_or(|s| view.duration_s < s) {
                t.fastest_kill_s = Some(view.duration_s);
            }
        } else {
            t.wipes += 1;
            if let Some(low) = view.target_low_hp {
                if t.best_wipe.is_none_or(|b| low < b) {
                    (t.best_wipe, t.best_wipe_ms) = (Some(low), ended_ms);
                }
            }
        }
        true
    }

    /// Counts a finished fight if it was a boss kill or wipe, and saves.
    pub fn record(&mut self, view: &EncounterView, ended_ms: u64, character: Option<&str>) {
        if self.count(view, ended_ms, character) {
            self.save();
        }
    }

    /// `character`'s attempts at `boss`.
    pub fn get(&self, character: Option<&str>, boss: u32) -> Option<&Tally> {
        self.by_character.get(character.unwrap_or(""))?.get(&boss)
    }

    /// Every character's attempts: name ("" when it wasn't known) -> boss -> attempts.
    pub fn all(&self) -> &BTreeMap<String, BTreeMap<u32, Tally>> {
        &self.by_character
    }

    /// Forgets `character`'s attempts.
    pub fn clear(&mut self, character: &str) {
        if self.by_character.remove(character).is_some() {
            self.save();
        }
    }
}

/// Each character's pets: level and souls toward the next one (`pets.json`), so a meter
/// started after the game sent its pet list (at login, entering or leaving a dungeon, and
/// some other moves) still knows them.
pub struct PetBook {
    path: PathBuf,
    by_character: BTreeMap<String, meter_core::pets::PetState>,
    /// The meter's pet revision last saved.
    saved: u64,
}

impl PetBook {
    pub fn load(dir: &Path) -> Self {
        let path = dir.join("pets.json");
        let by_character = read_json(&path).unwrap_or_default();
        Self { path, by_character, saved: 0 }
    }

    /// A pet's level as your characters last had it, the highest of them (pets are a
    /// collection your characters share): 0 when none has it.
    pub fn level(&self, pet: u32) -> u32 {
        self.by_character.values().filter_map(|s| s.levels.get(&pet).copied()).max().unwrap_or(0)
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
            if let Ok(s) = serde_json::to_string(&self.by_character) {
                write_atomic(&self.path, s);
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
    pub fastest: bool,
    /// How low its target got (a share of its HP; 0 when killed), when known.
    pub low_hp: Option<f64>,
    /// Your best wipe against its boss: it got lowest (filled in for the list).
    pub best_try: bool,
}

pub struct History {
    path: PathBuf,
    fights: Vec<Fight>,
    /// Lines in the file, and whether some couldn't be read (by this version).
    on_disk: usize,
    unread: bool,
}

impl History {
    pub fn load(dir: &Path) -> Self {
        let path = dir.join("history.jsonl");
        let (mut fights, lines): (Vec<Fight>, usize) = read_lines(&path);
        let read = fights.len();
        Self::trim(&mut fights);
        let mut history = Self { path, fights, on_disk: lines, unread: lines > read };
        // Fights older than are kept: off the file too.
        if read > history.fights.len() {
            history.compact();
        }
        history
    }

    /// Rewrites the file with the fights kept (see [`rewrite_lines`]).
    fn compact(&mut self) {
        rewrite_lines(&self.path, &self.fights, self.unread);
        self.on_disk = self.fights.len();
        self.unread = false;
    }

    /// Keeps the newest [`HISTORY_LIMIT`] boss fights and as many others.
    fn trim(fights: &mut Vec<Fight>) {
        let mut drop = beyond_limit(fights, HISTORY_LIMIT, |f| f.view.target_boss).into_iter();
        fights.retain(|_| !drop.next().unwrap_or(false));
    }

    pub fn push(&mut self, fight: Fight) {
        if let Some(dir) = self.path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        if let Ok(line) = serde_json::to_string(&fight) {
            if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&self.path) {
                let _ = writeln!(f, "{line}");
                self.on_disk += 1;
            }
        }
        self.fights.push(fight);
        Self::trim(&mut self.fights);
        if self.on_disk > self.fights.len() + HISTORY_LIMIT {
            self.compact();
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
                low_hp: f.view.target_low_hp,
                best_try: false,
            })
            .collect()
    }

    /// Fills in how low each boss got, for boss fights saved before that was kept, from
    /// their saved analysis (the newest fights have one).
    pub fn fill_low_hp(&mut self, details: &FightDetails) {
        for f in self.fights.iter_mut().filter(|f| f.view.target_boss && f.view.target_low_hp.is_none()) {
            let Some(a) = details.load(f.ended_ms) else { continue };
            let Some(hp) = a.target_hp.iter().find(|h| Some(h.target) == f.view.target_id && h.max > 0) else { continue };
            let Some(low) = hp.samples.iter().map(|p| p.hp).min() else { continue };
            f.view.target_low_hp = Some(if f.view.end == Some(FightEnd::Kill) { 0.0 } else { low as f64 / hp.max as f64 });
        }
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
        (self.on_disk, self.unread) = (0, false);
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
    fn open_side_windows_become_widgets() {
        let dir = temp_dir("widgets");
        assert_eq!(Settings::load(&dir).widgets, ["meter", "boss-warnings", "boss-timers", "cubes"]);
        fs::write(dir.join("settings.json"), r#"{ "panels": ["history", "timers", "target"] }"#).unwrap();
        let s = Settings::load(&dir);
        assert_eq!(s.widgets, ["meter", "boss-warnings", "boss-timers", "cubes", "timers", "target"]);
        assert!(s.overlay_on, "the overlay is on unless turned off");
        s.save(&dir);
        let saved = fs::read_to_string(dir.join("settings.json")).unwrap();
        assert!(!saved.contains("panels") && saved.contains("widgets"));
        fs::write(dir.join("settings.json"), r#"{ "widgets": ["target", "nonsense"], "overlay_on": false }"#).unwrap();
        let s = Settings::load(&dir);
        assert_eq!((s.widgets, s.overlay_on), (vec!["target".to_string()], false));
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn missing_or_odd_refresh_rates_become_the_default() {
        let dir = temp_dir("refresh");
        assert_eq!(Settings::load(&dir).refresh_ms, DEFAULT_REFRESH_MS);
        fs::write(dir.join("settings.json"), r#"{ "refresh_ms": 7, "locked": true }"#).unwrap();
        assert_eq!(Settings::load(&dir).refresh_ms, DEFAULT_REFRESH_MS);
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
            info_bar: Some(true),
            row_style: Some("fill".into()),
            fill_tint: Some(true),
            combat_power: Some(false),
            odyle: Some(false),
            class_palette: None,
            text_color: Some("#AABBCC".into()),
            text_opacity: Some(10),
            text_edge: Some("outline".into()),
            edge_color: Some("not a colour".into()),
        });
        assert_eq!((s.appearance.text_color.as_str(), s.appearance.text_opacity, s.appearance.text_edge.as_str(), s.appearance.edge_color.as_str()), ("#aabbcc", 40, "outline", "#000000"));
        assert_eq!((s.appearance.row_style.as_str(), s.appearance.fill_tint, s.appearance.combat_power, s.appearance.odyle), ("fill", true, false, false));
        // Class colours: only real classes and colours are kept.
        s.appearance.update(AppearanceChange {
            class_palette: Some([("12", "#FF0000"), ("99", "#00ff00"), ("13", "blue")].map(|(k, v)| (k.to_string(), v.to_string())).into()),
            ..Default::default()
        });
        assert_eq!(s.appearance.class_palette, [("12".to_string(), "#ff0000".to_string())].into());
        // An unknown row style is ignored.
        s.appearance.update(AppearanceChange { row_style: Some("wavy".into()), ..Default::default() });
        assert_eq!(s.appearance.row_style, "fill");
        s.save(&dir);
        let again = Settings::load(&dir);
        assert!(again.record);
        assert_eq!(again.appearance, s.appearance);
        // Older settings files: combat power and Odyle energy on, rows as blocks.
        let old: Appearance = serde_json::from_str("{}").unwrap();
        assert_eq!((old.row_style.as_str(), old.fill_tint, old.combat_power, old.odyle), ("blocks", false, true, true));
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
        // A raid's boss pulls, then more short fights than are kept (as on 2026-11-07, when
        // PvP fights pushed the raid's out).
        details.save(1, &analysis, true);
        details.save(2, &analysis, true);
        for ended_ms in 3..=(DETAILS_LIMIT as u64 + 4) {
            details.save(ended_ms, &analysis, false);
        }
        assert!(details.has(1) && details.has(2), "the boss fights stay");
        assert!(!details.has(3) && !details.has(4));
        assert!(details.has(5));
        assert_eq!(details.load(DETAILS_LIMIT as u64 + 4), Some(analysis.clone()));
        assert_eq!(details.load(1), Some(analysis));
        // A fight saved before boss fights were kept apart still loads.
        fs::copy(dir.join("fights/5.json"), dir.join("fights/0.json")).unwrap();
        assert!(details.has(0));
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn a_boss_fight_outlasts_the_short_fights_in_the_history() {
        let dir = temp_dir("history-bosses");
        let mut h = History::load(&dir);
        let mut boss = snap(None, Some(18)).encounter.unwrap();
        boss.target_boss = true;
        h.push(Fight { ended_ms: 0, self_name: None, view: boss });
        for i in 1..=(HISTORY_LIMIT as u64 + 1) {
            h.push(Fight { ended_ms: i, self_name: None, view: snap(None, Some(18)).encounter.unwrap() });
        }
        assert_eq!(h.len(), HISTORY_LIMIT + 1);
        assert_eq!(h.fight(0).map(|f| f.ended_ms), Some(0), "the boss fight stays");
        assert_eq!(h.fight(1).map(|f| f.ended_ms), Some(2), "the oldest other fight goes");
        let again = History::load(&dir);
        assert_eq!((again.len(), again.fight(0).map(|f| f.ended_ms)), (HISTORY_LIMIT + 1, Some(0)));
        fs::remove_dir_all(dir).ok();
    }

    fn lines(path: &Path) -> usize {
        fs::read_to_string(path).unwrap_or_default().lines().count()
    }

    #[test]
    fn the_history_file_is_trimmed_too() {
        let dir = temp_dir("history-file");
        let file = dir.join("history.jsonl");
        let plain = |i: u64| Fight { ended_ms: i, self_name: None, view: snap(None, Some(18)).encounter.unwrap() };
        // A file from before it was trimmed: three times what's kept.
        let old: String = (0..3 * HISTORY_LIMIT as u64).map(|i| serde_json::to_string(&plain(i)).unwrap() + "\n").collect();
        fs::write(&file, old).unwrap();
        let mut h = History::load(&dir);
        assert_eq!((h.len(), lines(&file)), (HISTORY_LIMIT, HISTORY_LIMIT), "loading drops the old fights from the file");
        assert!(!dir.join("history.jsonl.bak").exists(), "nothing was unreadable: no copy kept");
        // While running: it grows by a line a fight, and is cut back before it doubles.
        for i in 0..2 * HISTORY_LIMIT as u64 {
            h.push(plain(10_000 + i));
            assert!(lines(&file) <= 2 * HISTORY_LIMIT, "{} lines", lines(&file));
        }
        let again = History::load(&dir);
        assert_eq!(again.len(), HISTORY_LIMIT);
        assert_eq!(again.newest_first().next().map(|f| f.ended_ms), Some(10_000 + 2 * HISTORY_LIMIT as u64 - 1));
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn lines_this_version_cannot_read_are_kept_in_a_copy_before_a_trim() {
        let dir = temp_dir("history-bak");
        let file = dir.join("history.jsonl");
        let plain = |i: u64| serde_json::to_string(&Fight { ended_ms: i, self_name: None, view: snap(None, Some(18)).encounter.unwrap() }).unwrap();
        let mut old: String = (0..HISTORY_LIMIT as u64 + 5).map(|i| plain(i) + "\n").collect();
        old.push_str("{\"from\":\"a newer version\"}\n");
        fs::write(&file, &old).unwrap();
        let h = History::load(&dir);
        assert_eq!((h.len(), lines(&file)), (HISTORY_LIMIT, HISTORY_LIMIT));
        assert_eq!(fs::read_to_string(dir.join("history.jsonl.bak")).unwrap(), old);
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn a_file_that_cannot_be_read_is_set_aside_not_written_over() {
        let dir = temp_dir("bad-file");
        // Cut short, as by a crash in the middle of an old (not atomic) save.
        let cut = r#"{"characters":{"Nene":{"7":{"kills":3,"wipes":"#;
        fs::write(dir.join("attempts.json"), cut).unwrap();
        fs::write(dir.join("bests.json"), cut).unwrap();
        fs::write(dir.join("settings.json"), "not json").unwrap();
        let history = History::load(&dir);
        let mut attempts = Attempts::load(&dir, &history);
        let bests = Bests::load(&dir);
        let settings = Settings::load(&dir);
        assert!(attempts.all().is_empty() && bests.by_character.is_empty());
        assert_eq!(settings.refresh_ms, DEFAULT_REFRESH_MS, "the defaults, then");
        for name in ["attempts.json", "bests.json", "settings.json"] {
            assert!(!dir.join(name).exists(), "{name} is out of the way");
        }
        assert_eq!(fs::read_to_string(dir.join("attempts.json.bad")).unwrap(), cut, "and kept as it was");
        assert_eq!(fs::read_to_string(dir.join("settings.json.bad")).unwrap(), "not json");
        // The next save starts a new file; the old one is still there.
        attempts.record(&boss_try(FightEnd::Kill, None, 60.0), 1, Some("Nene"));
        assert!(dir.join("attempts.json").exists() && dir.join("attempts.json.bad").exists());
        assert!(!dir.join("attempts.json.tmp").exists(), "written whole, under its own name");
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn the_pvp_log_and_the_dungeon_runs_are_trimmed_on_the_file_too() {
        let dir = temp_dir("logs");
        let run = |entered_us: u64, stage: u64| meter_core::dungeon::Instance {
            map: 600_072,
            entered_us,
            started_us: Some(entered_us + 1),
            completed_us: None,
            stages: [stage, 0, 0, 0, 0, 0],
        };
        // One run, saved again at each of 200 changes.
        let mut i = Instances::load(&dir);
        for stage in 1..=200 {
            i.record(run(5, stage));
        }
        let file = dir.join("instances.jsonl");
        assert_eq!(lines(&file), 200);
        let again = Instances::load(&dir);
        assert_eq!((lines(&file), again.get(5).map(|r| r.stages[0])), (1, Some(200)), "just the run as it ended");
        fs::remove_dir_all(dir).ok();
    }

    /// A boss fight against mob type 7: how it ended, how low the boss got, how long.
    fn boss_try(end: FightEnd, low: Option<f64>, secs: f64) -> EncounterView {
        let mut v = snap(None, Some(18)).encounter.unwrap();
        (v.target_boss, v.target_type, v.end, v.target_low_hp, v.duration_s) = (true, Some(7), Some(end), low, secs);
        v
    }

    #[test]
    fn attempts_count_each_characters_kills_and_wipes() {
        let dir = temp_dir("attempts");
        let mut a = Attempts::load(&dir, &History::load(&dir));
        a.record(&boss_try(FightEnd::Wipe, Some(0.8), 60.0), 1000, Some("Nene"));
        a.record(&boss_try(FightEnd::Wipe, Some(0.6), 90.0), 2000, Some("Nene"));
        a.record(&boss_try(FightEnd::Wipe, Some(0.7), 80.0), 3000, Some("Nene"));
        a.record(&boss_try(FightEnd::Kill, Some(0.0), 200.0), 4000, Some("Nene"));
        a.record(&boss_try(FightEnd::Kill, Some(0.0), 150.0), 5000, Some("Nene"));
        // Another character's, and fights that aren't tries: idle, or not a boss.
        a.record(&boss_try(FightEnd::Wipe, Some(0.9), 30.0), 6000, Some("Alt"));
        a.record(&boss_try(FightEnd::Idle, Some(0.5), 30.0), 7000, Some("Nene"));
        let mut trash = boss_try(FightEnd::Kill, None, 10.0);
        trash.target_boss = false;
        a.record(&trash, 8000, Some("Nene"));

        let t = a.get(Some("Nene"), 7).unwrap().clone();
        assert_eq!((t.kills, t.wipes, t.attempts()), (2, 3, 5));
        assert_eq!((t.best_wipe, t.best_wipe_ms), (Some(0.6), 2000), "the wipe that got lowest");
        assert_eq!((t.fastest_kill_s, t.first_kill_ms), (Some(150.0), Some(4000)));
        assert_eq!((t.first_ms, t.last_ms, t.fight_s), (1000, 5000, 580.0));
        assert_eq!(a.get(Some("Alt"), 7).map(Tally::attempts), Some(1));
        // Kept, and kept apart from the history.
        let again = Attempts::load(&dir, &History::load(&dir));
        assert_eq!(again.get(Some("Nene"), 7), Some(&t));
        let mut again = again;
        again.clear("Alt");
        assert!(Attempts::load(&dir, &History::load(&dir)).get(Some("Alt"), 7).is_none());
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn attempts_start_from_the_history() {
        let dir = temp_dir("attempts-seed");
        let mut h = History::load(&dir);
        h.push(Fight { ended_ms: 10, self_name: Some("Nene".into()), view: boss_try(FightEnd::Wipe, Some(0.5), 60.0) });
        h.push(Fight { ended_ms: 20, self_name: Some("Nene".into()), view: boss_try(FightEnd::Kill, Some(0.0), 60.0) });
        let a = Attempts::load(&dir, &h);
        assert_eq!(a.get(Some("Nene"), 7).map(|t| (t.kills, t.wipes)), Some((1, 1)));
        // Only the first time: the next start reads the file, not the history again.
        h.push(Fight { ended_ms: 30, self_name: Some("Nene".into()), view: boss_try(FightEnd::Wipe, Some(0.4), 60.0) });
        assert_eq!(Attempts::load(&dir, &h).get(Some("Nene"), 7).map(|t| t.wipes), Some(1));
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
        assert_eq!(a.theme, "night");
        assert_eq!(a.bar_color, "theme");
        assert_eq!(a.highlight, "self");
        assert_eq!(a.font_size, 18);
        // An unusable font name is refused: the default stays.
        assert_eq!(a.font, "Calibri Light");
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
