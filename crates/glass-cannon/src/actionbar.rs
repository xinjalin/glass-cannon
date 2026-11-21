//! The action bars: up to six per class, each a widget of its own (bar 1 is "action-bar",
//! the others "action-bar-2" to "action-bar-6"), each set up from a panel beside it while
//! you arrange the overlay. A bar tracks your skills' cooldowns, the buffs on you, both (a
//! skill shows the buff it gave you while that runs, else its cooldown), or the debuffs on
//! you, and has a look of its own: icons (square or round, of a size, with spacing, in a
//! row or a column) or timer bars; the countdown, with tenths under 3 s, coloured or not;
//! skills on cooldown in grey or not; a glow when ready; its colours and opacity. It shows
//! always, or only what's running, ready or missing, and can hide out of combat.
//!
//! The skills to choose from are your class's own, as the skill details have them: Active
//! and Stigma (a stigma you haven't equipped can be left off). Buffs are the named effects
//! players (you included) have put on you; debuffs, those anything else has. An effect with
//! no set length (an aura), or one of an hour or more, shows as its icon alone.
//!
//! Everything here comes from what the meter already reads: your casts and cooldown
//! changes (`0x3802`, `0x3847`), your skill list (`0x5100`: your loadout's, its stigmas the
//! equipped ones, sent again as you switch loadouts, and changed as you spec or reset a
//! stigma in the skill window: `0x3907`, `0x3844`) and the effects on you
//! (`0x382a`-`0x382c`).

use std::collections::BTreeMap;
use std::ops::RangeInclusive;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

use crate::{lock, AppState};

/// The bars a class can have.
pub const BARS: u8 = 6;
/// What a bar tracks.
pub const TRACKS: [&str; 4] = ["cooldowns", "buffs", "both", "debuffs"];
/// What of it shows: all, or only what's running, ready, or missing (a buff not on you).
pub const SHOWS: [&str; 4] = ["always", "running", "ready", "missing"];
pub const LOOKS: [&str; 2] = ["icons", "bars"];
pub const SHAPES: [&str; 2] = ["square", "round"];
/// The colours a bar can take: a name here, or a colour of your own (`#rrggbb`).
pub const BAR_COLOURS: [&str; 8] = ["green", "blue", "red", "amber", "lavender", "rose", "mono", "theme"];
pub const ICON_SIZES: RangeInclusive<u16> = 20..=96;
pub const SPACINGS: RangeInclusive<u16> = 0..=24;
pub const BAR_WIDTHS: RangeInclusive<u16> = 120..=480;
pub const OPACITIES: RangeInclusive<u16> = 10..=100;
/// Marks among a bar's skills: a gap, and the start of a new row (no skill has these IDs).
pub const GAP: u32 = 1;
pub const ROW: u32 = 2;
const MAX_SKILLS: usize = 24;
const MAX_MARKS: usize = 24;
const MAX_EFFECTS: usize = 24;
/// Effects that have been on you this recently can be picked.
const EFFECTS_SEEN_US: u64 = 60 * 60 * 1_000_000;
/// An effect this long or longer shows as endless: its icon, no time.
const ENDLESS_MS: u32 = 60 * 60 * 1000;

fn is_mark(id: u32) -> bool {
    id == GAP || id == ROW
}

/// The widget a bar is.
pub fn widget_of(bar: u8) -> String {
    if bar == 1 {
        "action-bar".into()
    } else {
        format!("action-bar-{bar}")
    }
}

/// A skill's base: the specialties picked change its last four digits, not its cooldown.
fn base(skill: u32) -> u32 {
    skill - skill % 10_000
}

fn is_colour(c: &str) -> bool {
    BAR_COLOURS.contains(&c) || (c.len() == 7 && c.starts_with('#') && c[1..].chars().all(|h| h.is_ascii_hexdigit()))
}

fn clamp(range: &RangeInclusive<u16>, v: u16) -> u16 {
    v.clamp(*range.start(), *range.end())
}

/// The colours and background of the bars before they each had their own (the cooldown
/// widgets fall back to these).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct BarLook {
    pub background: bool,
    pub ready: String,
    pub running: String,
    pub cooling: String,
}

impl Default for BarLook {
    fn default() -> Self {
        Self { background: true, ready: "green".into(), running: "blue".into(), cooling: "red".into() }
    }
}

impl BarLook {
    /// Repairs a hand-edited settings file.
    pub fn sanitized(mut self) -> Self {
        let fixed = Self::default();
        for (now, d) in [(&mut self.ready, fixed.ready), (&mut self.running, fixed.running), (&mut self.cooling, fixed.cooling)] {
            if !is_colour(now) {
                *now = d;
            }
        }
        self
    }
}

/// One class's action bar as it was before there were several (read to carry it over).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ActionBarSetup {
    #[serde(default)]
    pub skills: Vec<u32>,
    #[serde(default)]
    pub buffs: Vec<u32>,
    #[serde(default)]
    pub style: String,
}

/// How a bar looks.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct BarStyle {
    /// One of [`LOOKS`].
    pub look: String,
    /// Icons' size and the space between them, in CSS pixels.
    pub icon_size: u16,
    pub spacing: u16,
    /// A column, not a row.
    pub vertical: bool,
    /// One of [`SHAPES`].
    pub shape: String,
    /// The time left on each icon.
    pub countdown: bool,
    /// Tenths of a second in the last 3 s.
    pub tenths: bool,
    /// The countdown coloured: amber in its last 10 s, red in its last 3.
    pub colours: bool,
    /// A skill on cooldown in grey.
    pub grey: bool,
    /// A glow when a skill's ready again.
    pub glow: bool,
    /// The bars look: each bar's width (CSS pixels) and the track behind it.
    pub bar_width: u16,
    pub background: bool,
    /// The bars' colours: a ready skill, a buff running, a skill on cooldown (see
    /// [`BAR_COLOURS`]).
    pub ready: String,
    pub running: String,
    pub cooling: String,
    /// In percent.
    pub opacity: u16,
}

impl Default for BarStyle {
    fn default() -> Self {
        Self {
            look: LOOKS[0].into(),
            icon_size: 40,
            spacing: 3,
            vertical: false,
            shape: SHAPES[0].into(),
            countdown: true,
            tenths: true,
            colours: true,
            grey: false,
            glow: true,
            bar_width: 240,
            background: true,
            ready: "green".into(),
            running: "blue".into(),
            cooling: "red".into(),
            opacity: 100,
        }
    }
}

impl BarStyle {
    fn sanitized(mut self) -> Self {
        let fixed = Self::default();
        if !LOOKS.contains(&self.look.as_str()) {
            self.look = fixed.look;
        }
        if !SHAPES.contains(&self.shape.as_str()) {
            self.shape = fixed.shape;
        }
        self.icon_size = clamp(&ICON_SIZES, self.icon_size);
        self.spacing = clamp(&SPACINGS, self.spacing);
        self.bar_width = clamp(&BAR_WIDTHS, self.bar_width);
        self.opacity = clamp(&OPACITIES, self.opacity);
        for (now, d) in [(&mut self.ready, fixed.ready), (&mut self.running, fixed.running), (&mut self.cooling, fixed.cooling)] {
            if !is_colour(now) {
                *now = d;
            } else {
                *now = now.to_ascii_lowercase();
            }
        }
        self
    }

    /// Its colours, as the cooldown widgets take them.
    pub fn look(&self) -> BarLook {
        BarLook { background: self.background, ready: self.ready.clone(), running: self.running.clone(), cooling: self.cooling.clone() }
    }
}

/// What a bar sounds, each "none", "chime" or one of your sound files (settings → sound):
/// when a skill on it is ready again, when an effect on it comes on you, and when one ends.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct BarSounds {
    pub ready: String,
    pub gained: String,
    pub ended: String,
}

impl Default for BarSounds {
    fn default() -> Self {
        let none = || crate::sounds::NONE.to_string();
        Self { ready: none(), gained: none(), ended: none() }
    }
}

impl BarSounds {
    fn sanitized(mut self) -> Self {
        for choice in [&mut self.ready, &mut self.gained, &mut self.ended] {
            if !crate::sounds::is_choice(choice) {
                *choice = crate::sounds::NONE.into();
            }
        }
        self
    }
}

/// One bar.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct BarSetup {
    /// One of [`TRACKS`].
    pub tracks: String,
    /// Base skill IDs (`xxyy0000`), in the bar's order, with [`GAP`]s and [`ROW`]s among
    /// them (cooldowns and both).
    pub skills: Vec<u32>,
    /// Effect IDs, in order (buffs and debuffs), unless `all_effects`.
    pub effects: Vec<u32>,
    /// Every buff (or debuff) on you, not only those picked.
    pub all_effects: bool,
    /// One of [`SHOWS`].
    pub show: String,
    /// Out of combat, it hides.
    pub hide_idle: bool,
    /// A stigma you haven't equipped stays off it.
    pub hide_unequipped: bool,
    pub style: BarStyle,
    pub sounds: BarSounds,
}

impl Default for BarSetup {
    fn default() -> Self {
        Self {
            tracks: TRACKS[0].into(),
            skills: Vec::new(),
            effects: Vec::new(),
            all_effects: false,
            show: SHOWS[0].into(),
            hide_idle: false,
            hide_unequipped: false,
            style: BarStyle::default(),
            sounds: BarSounds::default(),
        }
    }
}

impl BarSetup {
    pub fn sanitized(mut self) -> Self {
        if !TRACKS.contains(&self.tracks.as_str()) {
            self.tracks = TRACKS[0].into();
        }
        if !SHOWS.contains(&self.show.as_str()) {
            self.show = SHOWS[0].into();
        }
        self.skills = dedup(self.skills.into_iter().map(|s| if is_mark(s) { s } else { base(s) }).collect());
        let mut effects: Vec<u32> = Vec::new();
        for e in self.effects {
            if !is_mark(e) && !effects.contains(&e) && effects.len() < MAX_EFFECTS {
                effects.push(e);
            }
        }
        self.effects = effects;
        self.style = self.style.sanitized();
        self.sounds = self.sounds.sanitized();
        self
    }
}

/// The skills without repeats, first kept, at most [`MAX_SKILLS`]; gaps and rows (marks) as
/// many as given, up to [`MAX_MARKS`].
fn dedup(list: Vec<u32>) -> Vec<u32> {
    let mut out: Vec<u32> = Vec::new();
    let (mut skills, mut marks) = (0, 0);
    for id in list {
        if is_mark(id) {
            if marks < MAX_MARKS {
                marks += 1;
                out.push(id);
            }
        } else if !out.contains(&id) && skills < MAX_SKILLS {
            skills += 1;
            out.push(id);
        }
    }
    out
}

/// Each class's bars (by class code, then bar), carried over from the one bar each class had
/// before: its skills on bar 1, and its buffs on bar 2 (bar 1 with no skills), in its look
/// and colours.
pub fn carried_over(old: &BTreeMap<String, ActionBarSetup>, look: &BarLook) -> BTreeMap<String, BTreeMap<u8, BarSetup>> {
    let mut out = BTreeMap::new();
    for (class, o) in old {
        let style = BarStyle {
            look: if o.style == "bars" { "bars".into() } else { "icons".into() },
            shape: if o.style == "round" { "round".into() } else { "square".into() },
            background: look.background,
            ready: look.ready.clone(),
            running: look.running.clone(),
            cooling: look.cooling.clone(),
            ..BarStyle::default()
        };
        let mut bars = BTreeMap::new();
        if !o.skills.is_empty() {
            bars.insert(1, BarSetup { skills: o.skills.clone(), style: style.clone(), ..BarSetup::default() }.sanitized());
        }
        if !o.buffs.is_empty() {
            let at = if bars.is_empty() { 1 } else { 2 };
            bars.insert(at, BarSetup { tracks: "buffs".into(), effects: o.buffs.clone(), style, ..BarSetup::default() }.sanitized());
        }
        if !bars.is_empty() {
            out.insert(class.clone(), bars);
        }
    }
    out
}

// ---------------------------------------------------------------- views

/// One thing on a bar as it stands now.
#[derive(Debug, Serialize, PartialEq)]
pub struct Slot {
    /// "skill", "gap", "row", "buff" or "debuff".
    pub kind: &'static str,
    pub id: u32,
    pub name: Option<String>,
    pub icon_key: Option<u32>,
    /// A skill's cooldown left (0 when ready), or an effect's time left.
    pub remaining_ms: u32,
    pub duration_ms: u32,
    /// Charges left, for skills that have them (when known: after a cast).
    pub charges: Option<u8>,
    /// How many of an effect are on you (from different casts or casters).
    pub stacks: u32,
    /// An effect with no set length (or an hour or more): its icon, no time.
    pub endless: bool,
    /// On cooldown, or an effect on you (a skill on the both bar: its buff running).
    pub running: bool,
    /// A skill on the both bar showing its buff, not its cooldown.
    pub buff: bool,
    /// An effect picked that isn't on you.
    pub missing: bool,
}

impl Slot {
    fn mark(id: u32) -> Self {
        Slot {
            kind: if id == ROW { "row" } else { "gap" },
            id,
            name: None,
            icon_key: None,
            remaining_ms: 0,
            duration_ms: 0,
            charges: None,
            stacks: 0,
            endless: false,
            running: false,
            buff: false,
            missing: false,
        }
    }

    /// Whether it shows with the bar's `show` (gaps and rows always).
    fn shown(&self, show: &str) -> bool {
        if self.kind == "gap" || self.kind == "row" {
            return true;
        }
        match show {
            "running" => self.running,
            "ready" => self.kind == "skill" && !self.running,
            "missing" => self.missing || (self.kind == "skill" && !self.running),
            _ => true,
        }
    }
}

#[derive(Debug, Serialize, PartialEq)]
pub struct BarView {
    pub bar: u8,
    /// Your class, when known (each has its own bars).
    pub class: Option<u32>,
    /// This bar is set up for your class.
    pub exists: bool,
    pub tracks: String,
    pub hide_idle: bool,
    pub in_combat: bool,
    pub style: BarStyle,
    pub sounds: BarSounds,
    /// Every buff (or debuff) on you is on it, not only those picked.
    pub all_effects: bool,
    pub slots: Vec<Slot>,
    /// Everything on the bar, shown or not, for its sounds.
    pub watch: Vec<Watch>,
}

/// One thing on a bar as its sounds go by it: a skill on cooldown, or an effect on you (a
/// skill on the both bar: its buff running). The bar hears a skill stop cooling (ready),
/// and an effect come on or go.
#[derive(Debug, Serialize, PartialEq)]
pub struct Watch {
    pub id: u32,
    pub cooling: bool,
    pub on: bool,
}

impl Watch {
    fn of(s: &Slot) -> Option<Self> {
        let skill = s.kind == "skill";
        (s.kind != "gap" && s.kind != "row").then_some(Watch { id: s.id, cooling: skill && !s.buff && s.remaining_ms > 0, on: if skill { s.buff } else { s.running } })
    }
}

/// Your class, as last seen (your profile).
fn class(app: &AppState) -> Option<u32> {
    lock(&app.profile).class_code
}

fn setup_of(app: &AppState, class: Option<u32>, bar: u8) -> Option<BarSetup> {
    let class = class?;
    lock(&app.settings).bars.get(&class.to_string())?.get(&bar).cloned()
}

/// An effect on you, as a slot: its longest-lasting instance, and how many there are.
fn effect_slot(data: &crate::gamedata::GameData, kind: &'static str, effect: u32, on: &[&meter_core::buffs::EffectSpan], now: u64) -> Slot {
    let (name, icon_key) = data.effect_meta(effect);
    let longest = on.iter().max_by_key(|s| s.start_us + u64::from(s.duration_ms) * 1000);
    let skill = longest.and_then(|s| s.skill).map(|s| data.skill_meta(s));
    let (remaining_ms, duration_ms) = longest.map_or((0, 0), |s| {
        let ends = s.start_us + u64::from(s.duration_ms) * 1000;
        ((ends.saturating_sub(now) / 1000) as u32, s.duration_ms)
    });
    Slot {
        kind,
        id: effect,
        name: name.or_else(|| skill.as_ref().and_then(|s| s.0.clone())),
        icon_key: icon_key.or_else(|| skill.and_then(|s| s.1)),
        remaining_ms,
        duration_ms,
        charges: None,
        stacks: on.len() as u32,
        endless: !on.is_empty() && (duration_ms == 0 || duration_ms >= ENDLESS_MS),
        running: !on.is_empty(),
        buff: false,
        missing: on.is_empty(),
    }
}

/// A bar as it stands now (the widget polls it).
#[tauri::command]
pub fn action_bar(app: State<'_, Arc<AppState>>, bar: u8) -> BarView {
    let class = class(&app);
    let Some(setup) = setup_of(&app, class, bar) else {
        return BarView { bar, class, exists: false, tracks: TRACKS[0].into(), hide_idle: false, in_combat: false, style: BarStyle::default(), sounds: BarSounds::default(), all_effects: false, slots: Vec::new(), watch: Vec::new() };
    };
    let now = app.engine.now();
    let (cooldowns, effects, from_player, in_combat, equipped) = {
        let m = app.engine.meter();
        let effects = m.your_effects(now);
        let from_player: Vec<bool> = effects.iter().map(|s| m.is_known_player(s.source)).collect();
        // Your skill list is your loadout's (the game sends it again as you switch), its
        // equipped stigmas the only ones in it; until it comes, none is taken to be off.
        let equipped: Option<Vec<u32>> = (!m.your_skills().is_empty()).then(|| m.your_skills().iter().map(|id| base(*id)).collect());
        (m.cooldowns(now), effects, from_player, m.in_combat_now(), equipped)
    };
    let data = lock(&app.data);
    let mut slots = Vec::new();
    match setup.tracks.as_str() {
        "buffs" | "debuffs" => {
            let kind = if setup.tracks == "buffs" { "buff" } else { "debuff" };
            let wanted = setup.tracks == "buffs";
            let mine: Vec<&meter_core::buffs::EffectSpan> = effects.iter().zip(&from_player).filter(|(_, p)| **p == wanted).map(|(s, _)| s).collect();
            let ids: Vec<u32> = if setup.all_effects {
                let mut ids: Vec<u32> = Vec::new();
                for s in &mine {
                    if !ids.contains(&s.effect) {
                        ids.push(s.effect);
                    }
                }
                ids
            } else {
                setup.effects.clone()
            };
            for id in ids {
                let on: Vec<&meter_core::buffs::EffectSpan> = mine.iter().copied().filter(|s| s.effect == id).collect();
                slots.push(effect_slot(&data, kind, id, &on, now));
            }
            if setup.all_effects {
                // The soonest out first; endless ones last.
                slots.sort_by_key(|s| if s.endless { u32::MAX } else { s.remaining_ms });
            }
        }
        _ => {
            let both = setup.tracks == "both";
            for &skill in &setup.skills {
                if is_mark(skill) {
                    slots.push(Slot::mark(skill));
                    continue;
                }
                if setup.hide_unequipped && data.is_stigma(skill) && equipped.as_ref().is_some_and(|e| !e.contains(&skill)) {
                    continue;
                }
                let cd = cooldowns.iter().find(|c| base(c.skill) == skill);
                let (name, icon_key) = data.skill_meta(cd.map_or(skill, |c| c.skill));
                let remaining_ms = cd.map_or(0, |c| c.remaining_ms);
                let mut slot = Slot {
                    kind: "skill",
                    id: skill,
                    name,
                    icon_key,
                    remaining_ms,
                    duration_ms: cd.map_or(0, |c| c.duration_ms),
                    charges: cd.and_then(|c| c.charges),
                    stacks: 0,
                    endless: false,
                    running: remaining_ms > 0,
                    buff: false,
                    missing: false,
                };
                // Both: the buff the skill gave you, while it runs.
                if both {
                    let on: Vec<&meter_core::buffs::EffectSpan> = effects.iter().filter(|s| s.skill.is_some_and(|k| base(k) == skill)).collect();
                    if let Some(first) = on.first() {
                        let e = effect_slot(&data, "skill", first.effect, &on, now);
                        slot.remaining_ms = e.remaining_ms;
                        slot.duration_ms = e.duration_ms;
                        slot.stacks = e.stacks;
                        slot.endless = e.endless;
                        slot.running = true;
                        slot.buff = true;
                    }
                }
                slots.push(slot);
            }
        }
    }
    let watch = slots.iter().filter_map(Watch::of).collect();
    slots.retain(|s| s.shown(&setup.show));
    BarView { bar, class, exists: true, tracks: setup.tracks.clone(), hide_idle: setup.hide_idle, in_combat, style: setup.style, sounds: setup.sounds, all_effects: setup.all_effects, slots, watch }
}

// ---------------------------------------------------------------- setting a bar up

/// A skill or effect that can go on a bar.
#[derive(Debug, Serialize, PartialEq)]
pub struct Choice {
    pub id: u32,
    pub name: Option<String>,
    pub icon_key: Option<u32>,
    /// On the bar.
    pub chosen: bool,
    /// A skill in your skill list (a stigma: equipped).
    pub learned: bool,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct BarSetupView {
    pub bar: u8,
    pub class: Option<u32>,
    /// The bars your class has, by number.
    pub bars: Vec<u8>,
    pub exists: bool,
    pub setup: BarSetup,
    /// Which stigmas you have equipped is known (your skill list: at login, and as you switch
    /// loadouts).
    pub stigmas_known: bool,
    /// Your class's Active and Stigma skills.
    pub active: Vec<Choice>,
    pub stigma: Vec<Choice>,
    /// Named effects on you in the last hour: from players, and from anything else.
    pub buffs: Vec<Choice>,
    pub debuffs: Vec<Choice>,
}

/// What a bar is and can have, for its settings panel.
#[tauri::command]
pub fn action_bar_setup(app: State<'_, Arc<AppState>>, bar: u8) -> BarSetupView {
    let class = class(&app);
    let saved = setup_of(&app, class, bar);
    let exists = saved.is_some();
    let setup = saved.unwrap_or_default();
    let bars: Vec<u8> = class.and_then(|c| lock(&app.settings).bars.get(&c.to_string()).map(|b| b.keys().copied().collect())).unwrap_or_default();
    let now = app.engine.now();
    let (learned, equipped, seen) = {
        let m = app.engine.meter();
        let learned: Vec<u32> = m.your_skills().iter().copied().chain(m.your_cast_skills()).map(base).collect();
        let equipped: Option<Vec<u32>> = (!m.your_skills().is_empty()).then(|| m.your_skills().iter().map(|id| base(*id)).collect());
        (learned, equipped, m.effects_seen_on_you_by(now, EFFECTS_SEEN_US))
    };
    let data = lock(&app.data);
    let (mut active, mut stigma) = (Vec::new(), Vec::new());
    if let Some(c) = class {
        for (id, is_stigma) in data.class_skills(c) {
            let (name, icon_key) = data.skill_meta(id);
            // A stigma: equipped (once that's known), else learned.
            let has = if is_stigma { equipped.as_ref().map_or(learned.contains(&id), |e| e.contains(&id)) } else { learned.contains(&id) };
            let choice = Choice { id, name, icon_key, chosen: setup.skills.contains(&id), learned: has };
            if is_stigma {
                stigma.push(choice);
            } else {
                active.push(choice);
            }
        }
    }
    // Effects with a name or an icon (the rest can't be told apart), the picked first.
    let effect_choice = |id: u32, skill: Option<u32>| {
        let (name, icon_key) = data.effect_meta(id);
        let skill = skill.map(|s| data.skill_meta(s));
        let name = name.or_else(|| skill.as_ref().and_then(|s| s.0.clone()));
        let icon_key = icon_key.or_else(|| skill.and_then(|s| s.1));
        (name.is_some() || icon_key.is_some()).then(|| Choice { id, name, icon_key, chosen: setup.effects.contains(&id), learned: true })
    };
    let mut buffs = Vec::new();
    let mut debuffs = Vec::new();
    for &id in &setup.effects {
        let skill = seen.iter().find(|(e, _, _)| *e == id).and_then(|(_, s, _)| *s);
        if let Some(c) = effect_choice(id, skill) {
            if setup.tracks == "debuffs" {
                debuffs.push(c);
            } else {
                buffs.push(c);
            }
        }
    }
    for &(id, skill, from_player) in &seen {
        if setup.effects.contains(&id) {
            continue;
        }
        if let Some(c) = effect_choice(id, skill) {
            if from_player {
                buffs.push(c);
            } else {
                debuffs.push(c);
            }
        }
    }
    let stigmas_known = equipped.is_some();
    BarSetupView { bar, class, bars, exists, setup, stigmas_known, active, stigma, buffs, debuffs }
}

/// Tells the bar widgets (and the cooldown widgets, which can take bar 1's colours) that
/// the bars changed.
fn changed(handle: &AppHandle, app: &AppState) {
    let class = class(app);
    let cooldowns = crate::widgets::cooldowns_look_of(&lock(&app.settings), class);
    let _ = handle.emit("cooldowns-look", cooldowns);
    let _ = handle.emit("action-bars", ());
}

/// Saves a bar for your class.
#[tauri::command]
pub fn set_action_bar_setup(handle: AppHandle, app: State<'_, Arc<AppState>>, bar: u8, setup: BarSetup) -> Result<BarSetup, String> {
    let class = class(&app).ok_or("the meter doesn't know your class yet: use a skill in game first")?;
    if !(1..=BARS).contains(&bar) {
        return Err(format!("no bar {bar}"));
    }
    let setup = setup.sanitized();
    let mut s = lock(&app.settings);
    s.bars.entry(class.to_string()).or_default().insert(bar, setup.clone());
    s.save(&app.config_dir);
    drop(s);
    changed(&handle, &app);
    Ok(setup)
}

/// A new bar for your class, the first number free, shown; its number.
#[tauri::command]
pub fn add_action_bar(handle: AppHandle, app: State<'_, Arc<AppState>>) -> Result<u8, String> {
    let class = class(&app).ok_or("the meter doesn't know your class yet: use a skill in game first")?;
    let mut s = lock(&app.settings);
    let bars = s.bars.entry(class.to_string()).or_default();
    let bar = (1..=BARS).find(|b| !bars.contains_key(b)).ok_or(format!("a class can have {BARS} bars"))?;
    bars.insert(bar, BarSetup::default());
    let widget = widget_of(bar);
    if !s.widgets.contains(&widget) {
        s.widgets.push(widget);
    }
    s.save(&app.config_dir);
    let list = s.widgets.clone();
    drop(s);
    let _ = handle.emit_to(crate::hud::HUD, "hud-widgets", list);
    changed(&handle, &app);
    Ok(bar)
}

/// Takes a bar away from your class (and its widget off the overlay).
#[tauri::command]
pub fn remove_action_bar(handle: AppHandle, app: State<'_, Arc<AppState>>, bar: u8) -> Result<(), String> {
    let class = class(&app).ok_or("the meter doesn't know your class yet")?;
    let mut s = lock(&app.settings);
    if let Some(bars) = s.bars.get_mut(&class.to_string()) {
        bars.remove(&bar);
    }
    let widget = widget_of(bar);
    s.widgets.retain(|w| *w != widget);
    s.save(&app.config_dir);
    let list = s.widgets.clone();
    drop(s);
    let _ = handle.emit_to(crate::hud::HUD, "hud-widgets", list);
    changed(&handle, &app);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_setup_stays_valid() {
        let s = BarSetup {
            tracks: "everything".into(),
            show: "sometimes".into(),
            skills: vec![18_010_340, GAP, 18_010_000, ROW, GAP, 18_020_000],
            effects: vec![5, GAP, 5, 7],
            style: BarStyle { look: "hexagons".into(), icon_size: 900, spacing: 99, opacity: 0, ready: "#A0FF10".into(), cooling: "purple-ish".into(), ..BarStyle::default() },
            sounds: BarSounds { ready: "Air Horn.wav".into(), gained: "../settings.json".into(), ended: "chime".into() },
            ..BarSetup::default()
        }
        .sanitized();
        // A sound: nothing, the chime, or a sound file's name as kept; anything else is none.
        assert_eq!((s.sounds.ready.as_str(), s.sounds.gained.as_str(), s.sounds.ended.as_str()), ("Air Horn.wav", "none", "chime"));
        assert_eq!((s.tracks.as_str(), s.show.as_str()), ("cooldowns", "always"));
        // Variants share a slot; gaps and rows stay, as many as put there.
        assert_eq!(s.skills, [18_010_000, GAP, ROW, GAP, 18_020_000]);
        assert_eq!(s.effects, [5, 7]);
        assert_eq!((s.style.look.as_str(), s.style.icon_size, s.style.spacing, s.style.opacity), ("icons", 96, 24, 10));
        assert_eq!((s.style.ready.as_str(), s.style.cooling.as_str()), ("#a0ff10", "red"));
        let old: BarSetup = serde_json::from_str("{}").unwrap();
        assert_eq!(old, BarSetup::default());
    }

    #[test]
    fn repeats_go_and_limits_hold() {
        assert_eq!(dedup(vec![30, 10, 30, 20, 10]), [30, 10, 20]);
        assert_eq!(dedup((10..60).collect()).len(), MAX_SKILLS);
        assert_eq!(dedup(vec![GAP; 40]).len(), MAX_MARKS);
    }

    #[test]
    fn the_one_bar_carries_over() {
        let look = BarLook { ready: "amber".into(), ..BarLook::default() };
        let old = BTreeMap::from([
            ("18".to_string(), ActionBarSetup { skills: vec![18_010_000, 18_020_000], buffs: vec![700], style: "round".into() }),
            ("12".to_string(), ActionBarSetup { skills: vec![], buffs: vec![800], style: "bars".into() }),
            ("14".to_string(), ActionBarSetup::default()),
        ]);
        let bars = carried_over(&old, &look);
        let chanter = &bars["18"];
        assert_eq!((chanter[&1].tracks.as_str(), chanter[&1].skills.len(), chanter[&1].style.shape.as_str(), chanter[&1].style.ready.as_str()), ("cooldowns", 2, "round", "amber"));
        assert_eq!((chanter[&2].tracks.as_str(), chanter[&2].effects.as_slice()), ("buffs", &[700][..]));
        assert_eq!((bars["12"][&1].tracks.as_str(), bars["12"][&1].style.look.as_str()), ("buffs", "bars"));
        assert!(!bars.contains_key("14"), "nothing to carry over");
    }

    #[test]
    fn what_shows() {
        let slot = |kind, id, running, missing| Slot { kind, id, running, missing, ..Slot::mark(GAP) };
        let ready = slot("skill", 18_010_000, false, false);
        let cooling = slot("skill", 18_020_000, true, false);
        let buff = slot("buff", 700, true, false);
        let gone = slot("buff", 701, false, true);
        // For the bar's sounds: a skill cooling (not one showing its buff), an effect on you.
        let watch = |s: &Slot| Watch::of(s).map(|w| (w.cooling, w.on));
        let cooling_for = Slot { remaining_ms: 4000, ..slot("skill", 18_020_000, true, false) };
        let buffed = Slot { remaining_ms: 4000, buff: true, ..slot("skill", 18_030_000, true, false) };
        assert_eq!((watch(&ready), watch(&cooling_for), watch(&buffed)), (Some((false, false)), Some((true, false)), Some((false, true))));
        assert_eq!((watch(&buff), watch(&gone), watch(&Slot::mark(GAP))), (Some((false, true)), Some((false, false)), None));
        let shown = |show: &str| [&ready, &cooling, &buff, &gone].iter().filter(|s| s.shown(show)).map(|s| s.id).collect::<Vec<_>>();
        assert_eq!(shown("always"), [18_010_000, 18_020_000, 700, 701]);
        assert_eq!(shown("running"), [18_020_000, 700]);
        assert_eq!(shown("ready"), [18_010_000]);
        assert_eq!(shown("missing"), [18_010_000, 701]);
        assert!(Slot::mark(ROW).shown("running"), "rows and gaps stay");
    }

    #[test]
    fn widgets_by_bar() {
        assert_eq!(widget_of(1), "action-bar");
        assert_eq!(widget_of(4), "action-bar-4");
    }
}
