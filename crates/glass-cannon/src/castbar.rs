//! How the cast bar widget looks (overlay → cast bar): a bar of its own for the cast you're
//! making (a charged skill you're holding), with its icon, name and time, in a look of its
//! own and with the vitals arcs' effects. Each change is saved and sent to the widget at
//! once. Also whether the player bars widget shows the cast too.

use std::ops::RangeInclusive;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

use crate::{lock, AppState};

/// The bar's looks, as the vitals arcs': thick on a dark track; a thin glowing line; cut
/// into cells; dark at its start and bright where the fill ends; a light frame with the
/// fill inside it.
pub const STYLES: [&str; 5] = ["solid", "thin", "segmented", "gradient", "framed"];
/// The effect is the vitals arcs' own, with its strength, brightness, size and speed
/// (whatever it's changed to); otherwise one of [`crate::vitals::EFFECTS`].
pub const EFFECT_MATCH: &str = "match";
/// Where the skill's name and the time go: over the bar, on it, under it.
pub const TEXT_PLACES: [&str; 3] = ["above", "inside", "below"];
/// The time shown: what's left, what's left of the whole, or none.
pub const TIMES: [&str; 3] = ["left", "both", "none"];
/// The bar's ends.
pub const ENDS: [&str; 2] = ["round", "square"];
/// How thick the bar is, and how big its text, in percent of their own.
pub const THICKNESSES: RangeInclusive<u16> = 25..=400;
pub const TEXT_SIZES: RangeInclusive<u16> = 50..=300;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct CastBarSetup {
    pub style: String,
    /// [`EFFECT_MATCH`], or one of the vitals arcs' effects.
    pub effect: String,
    /// The effect's own settings (see the vitals arcs'), when it isn't the vitals arcs'.
    pub effect_strength: u16,
    pub effect_brightness: u16,
    pub effect_size: u16,
    pub effect_speed: u16,
    pub thickness: u16,
    pub text_size: u16,
    pub text_at: String,
    pub time: String,
    pub ends: String,
    /// The skill's icon beside the bar, and its name.
    pub icon: bool,
    pub name: bool,
    /// A notch at each of a charged skill's levels.
    pub marks: bool,
    /// The empty bar shows between casts.
    pub always_show: bool,
    /// With the empty bar showing between casts, the effect runs along it then too.
    pub effect_always: bool,
    /// A colour of your own ("#rrggbb"); none: the theme's.
    pub colour: Option<String>,
}

impl Default for CastBarSetup {
    fn default() -> Self {
        Self {
            style: STYLES[0].into(),
            effect: EFFECT_MATCH.into(),
            effect_strength: 100,
            effect_brightness: 100,
            effect_size: 100,
            effect_speed: 100,
            thickness: 100,
            text_size: 100,
            text_at: TEXT_PLACES[1].into(),
            time: TIMES[0].into(),
            ends: ENDS[0].into(),
            icon: true,
            name: true,
            marks: true,
            always_show: false,
            effect_always: false,
            colour: None,
        }
    }
}

/// Changes to make; `None` leaves a setting as it is.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CastBarChange {
    pub style: Option<String>,
    pub effect: Option<String>,
    /// Any number: clamped.
    pub effect_strength: Option<i64>,
    pub effect_brightness: Option<i64>,
    pub effect_size: Option<i64>,
    pub effect_speed: Option<i64>,
    pub thickness: Option<i64>,
    pub text_size: Option<i64>,
    pub text_at: Option<String>,
    pub time: Option<String>,
    pub ends: Option<String>,
    pub icon: Option<bool>,
    pub name: Option<bool>,
    pub marks: Option<bool>,
    pub always_show: Option<bool>,
    pub effect_always: Option<bool>,
    /// A colour of your own; an empty one: the theme's again.
    pub colour: Option<String>,
}

fn is_effect(e: &str) -> bool {
    e == EFFECT_MATCH || crate::vitals::EFFECTS.contains(&e)
}

fn is_hex_colour(c: &str) -> bool {
    c.len() == 7 && c.starts_with('#') && c[1..].chars().all(|c| c.is_ascii_hexdigit())
}

fn pick(choices: &[&str], v: Option<String>, now: &mut String) {
    if let Some(v) = v.filter(|v| choices.contains(&v.as_str())) {
        *now = v;
    }
}

fn clamp(range: &RangeInclusive<u16>, v: i64) -> u16 {
    v.clamp(i64::from(*range.start()), i64::from(*range.end())) as u16
}

impl CastBarSetup {
    pub fn update(&mut self, c: CastBarChange) {
        use crate::vitals::{EFFECT_BRIGHTNESSES, EFFECT_SIZES, EFFECT_SPEEDS, EFFECT_STRENGTHS};
        pick(&STYLES, c.style, &mut self.style);
        pick(&TEXT_PLACES, c.text_at, &mut self.text_at);
        pick(&TIMES, c.time, &mut self.time);
        pick(&ENDS, c.ends, &mut self.ends);
        if let Some(e) = c.effect.filter(|e| is_effect(e)) {
            self.effect = e;
        }
        for (range, v, now) in [
            (&EFFECT_STRENGTHS, c.effect_strength, &mut self.effect_strength),
            (&EFFECT_BRIGHTNESSES, c.effect_brightness, &mut self.effect_brightness),
            (&EFFECT_SIZES, c.effect_size, &mut self.effect_size),
            (&EFFECT_SPEEDS, c.effect_speed, &mut self.effect_speed),
            (&THICKNESSES, c.thickness, &mut self.thickness),
            (&TEXT_SIZES, c.text_size, &mut self.text_size),
        ] {
            if let Some(v) = v {
                *now = clamp(range, v);
            }
        }
        for (v, now) in [(c.icon, &mut self.icon), (c.name, &mut self.name), (c.marks, &mut self.marks), (c.always_show, &mut self.always_show), (c.effect_always, &mut self.effect_always)] {
            if let Some(v) = v {
                *now = v;
            }
        }
        if let Some(colour) = c.colour {
            if colour.is_empty() {
                self.colour = None;
            } else if is_hex_colour(&colour) {
                self.colour = Some(colour.to_ascii_lowercase());
            }
        }
    }

    /// Repairs values from a hand-edited settings file (or someone's setup).
    pub fn sanitized(mut self) -> Self {
        use crate::vitals::{EFFECT_BRIGHTNESSES, EFFECT_SIZES, EFFECT_SPEEDS, EFFECT_STRENGTHS};
        let fixed = Self::default();
        let keep = |choices: &[&str], v: String, d: String| if choices.contains(&v.as_str()) { v } else { d };
        self.style = keep(&STYLES, self.style, fixed.style);
        self.text_at = keep(&TEXT_PLACES, self.text_at, fixed.text_at);
        self.time = keep(&TIMES, self.time, fixed.time);
        self.ends = keep(&ENDS, self.ends, fixed.ends);
        if !is_effect(&self.effect) {
            self.effect = fixed.effect;
        }
        for (range, now, d) in [
            (&EFFECT_STRENGTHS, &mut self.effect_strength, fixed.effect_strength),
            (&EFFECT_BRIGHTNESSES, &mut self.effect_brightness, fixed.effect_brightness),
            (&EFFECT_SIZES, &mut self.effect_size, fixed.effect_size),
            (&EFFECT_SPEEDS, &mut self.effect_speed, fixed.effect_speed),
            (&THICKNESSES, &mut self.thickness, fixed.thickness),
            (&TEXT_SIZES, &mut self.text_size, fixed.text_size),
        ] {
            if !range.contains(now) {
                *now = d;
            }
        }
        self.colour = self.colour.filter(|c| is_hex_colour(c)).map(|c| c.to_ascii_lowercase());
        self
    }
}

/// What the cast bar's settings show: its look, and whether the player bars widget shows
/// the cast too (the vitals arcs' own is theirs: `show_cast`).
#[derive(Debug, Clone, Serialize)]
pub struct CastBarView {
    #[serde(flatten)]
    pub setup: CastBarSetup,
    pub player_cast: bool,
}

fn view(s: &crate::store::Settings) -> CastBarView {
    CastBarView { setup: s.cast_bar.clone(), player_cast: s.player_cast }
}

#[tauri::command]
pub fn cast_bar_setup(app: State<'_, Arc<AppState>>) -> CastBarView {
    view(&lock(&app.settings))
}

/// Changes the cast bar's look; saved, and sent to the widget.
#[tauri::command]
pub fn set_cast_bar(handle: AppHandle, app: State<'_, Arc<AppState>>, change: CastBarChange) -> CastBarView {
    let mut s = lock(&app.settings);
    s.cast_bar.update(change);
    s.save(&app.config_dir);
    let v = view(&s);
    drop(s);
    let _ = handle.emit("cast-bar-setup", &v);
    v
}

/// Puts the cast bar's look back as it was at first.
#[tauri::command]
pub fn reset_cast_bar(handle: AppHandle, app: State<'_, Arc<AppState>>) -> CastBarView {
    let mut s = lock(&app.settings);
    s.cast_bar = CastBarSetup::default();
    s.save(&app.config_dir);
    let v = view(&s);
    drop(s);
    let _ = handle.emit("cast-bar-setup", &v);
    v
}

/// Whether the player bars widget shows your cast under your HP and MP (off: the cast bar
/// widget, or the vitals arcs, have it).
#[tauri::command]
pub fn set_player_cast(handle: AppHandle, app: State<'_, Arc<AppState>>, on: bool) -> CastBarView {
    let mut s = lock(&app.settings);
    s.player_cast = on;
    s.save(&app.config_dir);
    let v = view(&s);
    drop(s);
    let _ = handle.emit("cast-bar-setup", &v);
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changes_stay_valid() {
        let mut c = CastBarSetup::default();
        assert_eq!((c.effect.as_str(), c.text_at.as_str(), c.icon, c.colour.as_deref()), ("match", "inside", true, None));
        c.update(CastBarChange { style: Some("framed".into()), effect: Some("frost".into()), thickness: Some(9000), text_size: Some(1), ..Default::default() });
        assert_eq!((c.style.as_str(), c.effect.as_str(), c.thickness, c.text_size), ("framed", "frost", 400, 50));
        // Unknown choices are ignored.
        c.update(CastBarChange { style: Some("wavy".into()), effect: Some("lava".into()), text_at: Some("beside".into()), time: Some("both".into()), ..Default::default() });
        assert_eq!((c.style.as_str(), c.effect.as_str(), c.text_at.as_str(), c.time.as_str()), ("framed", "frost", "inside", "both"));
        c.update(CastBarChange { effect: Some("match".into()), effect_speed: Some(-4), icon: Some(false), always_show: Some(true), ..Default::default() });
        assert_eq!((c.effect.as_str(), c.effect_speed, c.icon, c.always_show, c.name, c.effect_always), ("match", 25, false, true, true, false));
        c.update(CastBarChange { effect_always: Some(true), ..Default::default() });
        assert!(c.effect_always && c.always_show);
        // A colour of your own, a bad one ignored, an empty one the theme's again.
        c.update(CastBarChange { colour: Some("#FFAA00".into()), ..Default::default() });
        assert_eq!(c.colour.as_deref(), Some("#ffaa00"));
        c.update(CastBarChange { colour: Some("orange".into()), ..Default::default() });
        assert_eq!(c.colour.as_deref(), Some("#ffaa00"));
        c.update(CastBarChange { colour: Some(String::new()), ..Default::default() });
        assert_eq!(c.colour, None);
    }

    #[test]
    fn a_hand_edited_file_is_repaired() {
        let odd: CastBarSetup = serde_json::from_str(
            r##"{ "style": "plaid", "effect": "lava", "thickness": 9, "text_size": 4000, "effect_size": 1, "text_at": "inside", "ends": "pointy", "colour": "url(x)", "marks": false }"##,
        )
        .unwrap();
        let c = odd.sanitized();
        let d = CastBarSetup::default();
        assert_eq!((c.style, c.effect, c.thickness, c.text_size, c.effect_size, c.ends, c.colour), (d.style, d.effect, 100, 100, 100, d.ends, None));
        assert!(!c.marks, "what's fine stays");
        // What's in range stays.
        let fine = CastBarSetup { style: "thin".into(), effect: "pulse".into(), thickness: 250, colour: Some("#12abef".into()), ..Default::default() };
        assert_eq!(fine.clone().sanitized(), fine);
    }
}
