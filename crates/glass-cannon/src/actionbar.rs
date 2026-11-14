//! The action bar widget: the skills you choose, with their cooldowns (a sweep, the time
//! left, charges, a glow once ready), and the buffs on you you choose to follow, with their
//! time left. Each class keeps its own (settings → overlay → action bar), as icons (square
//! or round) or as timer bars.
//!
//! Everything here comes from what the meter already reads: your casts and cooldown
//! changes (`0x3802`, `0x3847`) and the buffs on you (`0x382a`-`0x382c`).

use std::collections::BTreeMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::{lock, AppState};

/// The looks: icons, square or round, or a bar each with its time.
pub const STYLES: [&str; 3] = ["square", "round", "bars"];
/// Buffs that have been on you this recently can be picked.
const BUFFS_SEEN_US: u64 = 60 * 60 * 1_000_000;

/// One class's action bar.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ActionBarSetup {
    /// Base skill IDs (`xxyy0000`), in the bar's order.
    #[serde(default)]
    pub skills: Vec<u32>,
    /// Effect IDs, in order.
    #[serde(default)]
    pub buffs: Vec<u32>,
    /// One of [`STYLES`] ("square" when unset).
    #[serde(default)]
    pub style: String,
}

/// The colours a bar can take: a name here, or a colour of your own (`#rrggbb`).
pub const BAR_COLOURS: [&str; 8] = ["green", "blue", "red", "amber", "lavender", "rose", "mono", "theme"];

/// How the bars look (the bars look; the same for every class): the track behind each bar,
/// and the colour of a ready skill, a buff that's running, and a skill on cooldown.
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

fn is_colour(c: &str) -> bool {
    BAR_COLOURS.contains(&c) || (c.len() == 7 && c.starts_with('#') && c[1..].chars().all(|h| h.is_ascii_hexdigit()))
}

/// Changes to the bars' look; `None` leaves it as it is.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct BarLookChange {
    pub background: Option<bool>,
    pub ready: Option<String>,
    pub running: Option<String>,
    pub cooling: Option<String>,
}

impl BarLook {
    pub fn update(&mut self, c: BarLookChange) {
        if let Some(b) = c.background {
            self.background = b;
        }
        for (now, new) in [(&mut self.ready, c.ready), (&mut self.running, c.running), (&mut self.cooling, c.cooling)] {
            if let Some(v) = new.filter(|v| is_colour(v)) {
                *now = v.to_ascii_lowercase();
            }
        }
    }

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

/// A skill on the bar, as it stands now.
#[derive(Debug, Serialize, PartialEq)]
pub struct SlotView {
    pub skill: u32,
    pub name: Option<String>,
    pub icon_key: Option<u32>,
    /// 0 when ready.
    pub remaining_ms: u32,
    pub duration_ms: u32,
    /// Charges left, for skills that have them (when known: after a cast).
    pub charges: Option<u8>,
}

/// A buff followed, while it's on you.
#[derive(Debug, Serialize, PartialEq)]
pub struct BuffView {
    pub effect: u32,
    pub name: Option<String>,
    pub icon_key: Option<u32>,
    /// None for one with no set length.
    pub remaining_ms: Option<u32>,
    pub duration_ms: u32,
    /// How many of it are on you (from different casts or casters).
    pub stacks: u32,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct ActionBarView {
    /// Your class, when known (each has its own bar).
    pub class: Option<u32>,
    pub style: String,
    pub bar_look: BarLook,
    pub skills: Vec<SlotView>,
    /// Only the ones on you now.
    pub buffs: Vec<BuffView>,
    /// Nothing chosen yet for this class.
    pub unset: bool,
}

/// A skill or buff that can go on the bar.
#[derive(Debug, Serialize, PartialEq)]
pub struct Choice {
    pub id: u32,
    pub name: Option<String>,
    pub icon_key: Option<u32>,
    pub chosen: bool,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct Choices {
    pub class: Option<u32>,
    pub style: String,
    pub bar_look: BarLook,
    /// Your class's skills (from the game's list of them, and what you've cast), the chosen
    /// first, in their order.
    pub skills: Vec<Choice>,
    /// Buffs that have been on you in the last hour, the chosen first.
    pub buffs: Vec<Choice>,
}

/// A skill's base: the specialties picked change its last four digits, not its cooldown.
fn base(skill: u32) -> u32 {
    skill - skill % 10_000
}

/// Your class, as last seen (your profile).
fn class(app: &AppState) -> Option<u32> {
    lock(&app.profile).class_code
}

fn style_of(setup: &ActionBarSetup) -> String {
    if STYLES.contains(&setup.style.as_str()) {
        setup.style.clone()
    } else {
        STYLES[0].to_string()
    }
}

/// The chosen first (in their order), then the rest by ID.
fn ordered(chosen: &[u32], mut rest: Vec<u32>) -> Vec<(u32, bool)> {
    rest.retain(|id| !chosen.contains(id));
    rest.sort_unstable();
    rest.dedup();
    chosen.iter().map(|id| (*id, true)).chain(rest.into_iter().map(|id| (id, false))).collect()
}

fn setup_of(app: &AppState, class: Option<u32>) -> ActionBarSetup {
    class.and_then(|c| lock(&app.settings).action_bars.get(&c.to_string()).cloned()).unwrap_or_default()
}

#[tauri::command]
pub fn action_bar(app: State<'_, Arc<AppState>>) -> ActionBarView {
    let class = class(&app);
    let setup = setup_of(&app, class);
    let now = app.engine.now();
    let (cooldowns, effects) = {
        let m = app.engine.meter();
        (m.cooldowns(now), m.your_effects(now))
    };
    let data = lock(&app.data);
    let skills = setup
        .skills
        .iter()
        .map(|&skill| {
            let cd = cooldowns.iter().find(|c| base(c.skill) == skill);
            let (name, icon_key) = data.skill_meta(cd.map_or(skill, |c| c.skill));
            SlotView {
                skill,
                name,
                icon_key,
                remaining_ms: cd.map_or(0, |c| c.remaining_ms),
                duration_ms: cd.map_or(0, |c| c.duration_ms),
                charges: cd.and_then(|c| c.charges),
            }
        })
        .collect();
    let mut buffs = Vec::new();
    for &effect in &setup.buffs {
        let on: Vec<_> = effects.iter().filter(|s| s.effect == effect).collect();
        let Some(longest) = on.iter().max_by_key(|s| s.start_us + u64::from(s.duration_ms) * 1000) else { continue };
        let (name, icon_key) = data.effect_meta(effect);
        let skill = longest.skill.map(|s| data.skill_meta(s));
        let ends = longest.start_us + u64::from(longest.duration_ms) * 1000;
        buffs.push(BuffView {
            effect,
            name: name.or_else(|| skill.as_ref().and_then(|s| s.0.clone())),
            icon_key: icon_key.or_else(|| skill.and_then(|s| s.1)),
            remaining_ms: (longest.duration_ms > 0).then(|| (ends.saturating_sub(now) / 1000) as u32),
            duration_ms: longest.duration_ms,
            stacks: on.len() as u32,
        });
    }
    let bar_look = lock(&app.settings).action_bar_look.clone();
    ActionBarView { class, style: style_of(&setup), bar_look, skills, buffs, unset: setup.skills.is_empty() && setup.buffs.is_empty() }
}

/// What can go on your class's bar, for the settings.
#[tauri::command]
pub fn action_bar_choices(app: State<'_, Arc<AppState>>) -> Choices {
    let class = class(&app);
    let setup = setup_of(&app, class);
    let now = app.engine.now();
    let (known, cast, seen) = {
        let m = app.engine.meter();
        (m.your_skills().to_vec(), m.your_cast_skills(), m.effects_seen_on_you(now, BUFFS_SEEN_US))
    };
    // Your class's skills: player skills are `ccxxxxxx`, cc the class.
    let of_class = |s: &u32| class.is_none_or(|c| s / 1_000_000 == c);
    let skills: Vec<u32> = known.into_iter().chain(cast).map(base).filter(of_class).collect();
    let data = lock(&app.data);
    let skill_choices = ordered(&setup.skills, skills)
        .into_iter()
        .map(|(id, chosen)| {
            let (name, icon_key) = data.skill_meta(id);
            Choice { id, name, icon_key, chosen }
        })
        .collect();
    let skill_of: BTreeMap<u32, u32> = seen.iter().filter_map(|(e, s)| Some((*e, (*s)?))).collect();
    let buff_choices = ordered(&setup.buffs, seen.iter().map(|(e, _)| *e).collect())
        .into_iter()
        .map(|(id, chosen)| {
            let (name, icon_key) = data.effect_meta(id);
            let skill = skill_of.get(&id).map(|s| data.skill_meta(*s));
            Choice {
                id,
                name: name.or_else(|| skill.as_ref().and_then(|s| s.0.clone())),
                icon_key: icon_key.or_else(|| skill.and_then(|s| s.1)),
                chosen,
            }
        })
        .collect();
    let bar_look = lock(&app.settings).action_bar_look.clone();
    Choices { class, style: style_of(&setup), bar_look, skills: skill_choices, buffs: buff_choices }
}

/// The list without repeats, first kept, at most 24.
fn dedup(list: Vec<u32>) -> Vec<u32> {
    let mut out: Vec<u32> = Vec::new();
    for id in list {
        if !out.contains(&id) {
            out.push(id);
        }
    }
    out.truncate(24);
    out
}

/// Sets your class's bar: its skills and buffs in order, and its look.
#[tauri::command]
pub fn set_action_bar(app: State<'_, Arc<AppState>>, skills: Vec<u32>, buffs: Vec<u32>, style: String) -> Result<(), String> {
    let class = class(&app).ok_or("the meter doesn't know your class yet: use a skill in game first")?;
    if !STYLES.contains(&style.as_str()) {
        return Err(format!("no such look: {style}"));
    }
    let setup = ActionBarSetup { skills: dedup(skills.into_iter().map(base).collect()), buffs: dedup(buffs), style };
    let mut s = lock(&app.settings);
    s.action_bars.insert(class.to_string(), setup);
    s.save(&app.config_dir);
    Ok(())
}

/// Changes how the bars look (every class's); saved.
#[tauri::command]
pub fn set_action_bar_look(app: State<'_, Arc<AppState>>, change: BarLookChange) -> BarLook {
    let mut s = lock(&app.settings);
    s.action_bar_look.update(change);
    s.save(&app.config_dir);
    s.action_bar_look.clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bar_colours_stay_valid() {
        let mut l = BarLook::default();
        l.update(BarLookChange { ready: Some("#A0FF10".into()), cooling: Some("purple-ish".into()), background: Some(false), ..Default::default() });
        assert_eq!((l.ready.as_str(), l.running.as_str(), l.cooling.as_str(), l.background), ("#a0ff10", "blue", "red", false));
        let junk = BarLook { ready: "nope".into(), ..BarLook::default() }.sanitized();
        assert_eq!(junk.ready, "green");
        let old: BarLook = serde_json::from_str("{}").unwrap();
        assert_eq!(old, BarLook::default());
    }

    #[test]
    fn the_chosen_come_first_in_their_order() {
        let got = ordered(&[18_050_000, 18_010_000], vec![18_020_000, 18_010_000, 18_030_000, 18_020_000]);
        assert_eq!(got, [(18_050_000, true), (18_010_000, true), (18_020_000, false), (18_030_000, false)]);
    }

    #[test]
    fn variants_share_a_slot_and_repeats_go() {
        assert_eq!(base(18_010_340), 18_010_000);
        assert_eq!(dedup(vec![3, 1, 3, 2, 1]), [3, 1, 2]);
        assert_eq!(dedup((0..40).collect()).len(), 24);
    }

    #[test]
    fn an_unknown_look_is_square() {
        assert_eq!(style_of(&ActionBarSetup::default()), "square");
        assert_eq!(style_of(&ActionBarSetup { style: "round".into(), ..Default::default() }), "round");
        assert_eq!(style_of(&ActionBarSetup { style: "hexagon".into(), ..Default::default() }), "square");
    }
}
