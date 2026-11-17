//! How the vitals arcs widget looks (settings → overlay → vitals arcs): the arcs' look,
//! roundness and thickness, whether they always show, their numbers (what, how big, on what, at which end) and the cast's look.
//! Each change is saved and sent to the widget at once.

use std::collections::{BTreeMap, BTreeSet};
use std::ops::RangeInclusive;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

use crate::{lock, AppState};

/// The arcs' looks: thick with a dark track; thin lines with a glow; cut into cells; thick,
/// dark at the bottom and bright at the fill's end; a light frame with the fill inside it;
/// thin with notches outside them at a quarter, half and three quarters.
pub const STYLES: [&str; 6] = ["solid", "thin", "segmented", "gradient", "framed", "ticks"];
/// An effect over any look: none, or flames along each side arc in its own colour (an
/// outer arc's outward, an inner arc's inward).
pub const EFFECTS: [&str; 2] = ["none", "flames"];
/// What's behind the numbers: a dark outline, a soft shadow, a dark plate, nothing.
pub const TEXT_BACKINGS: [&str; 4] = ["outline", "shadow", "plate", "none"];
/// Which end of the arcs the numbers go by.
pub const TEXT_PLACES: [&str; 2] = ["top", "bottom"];
/// The numbers: value and max, the share in percent, or the share by each arc's end with
/// the values under them.
pub const NUMBERS: [&str; 3] = ["value", "percent", "both"];
/// The cast: along an arc under your feet, or a straight bar with its time at each end.
pub const CASTS: [&str; 2] = ["arc", "bar"];
/// Where a side arc can go: a side, and outside (the thicker line) or inside it.
pub const SPOTS: [&str; 4] = ["left-outer", "left-inner", "right-outer", "right-inner"];
/// The numbers' size, in percent of the overlay's text.
pub const TEXT_SIZES: RangeInclusive<u16> = 50..=200;
/// How round the arcs are, in percent: 0 straight, 50 as they were, 100 the roundest.
pub const ROUNDNESS: RangeInclusive<u16> = 0..=100;
/// The side arcs, each of which can be turned off (only HP and stamina, say).
pub const SIDE_ARCS: [&str; 4] = ["hp", "stamina", "mp", "flight"];
/// The arcs (and the shield over the HP arc) that can have a colour of their own.
pub const COLOURED: [&str; 6] = ["hp", "stamina", "mp", "flight", "cast", "shield"];
/// How thick the arcs are, in percent of each look's own.
pub const WIDTHS: RangeInclusive<u16> = 50..=300;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct VitalsSetup {
    pub style: String,
    /// See [`EFFECTS`].
    pub effect: String,
    pub roundness: u16,
    pub text_size: u16,
    pub text_backing: String,
    pub text_at: String,
    pub numbers: String,
    pub cast: String,
    /// Each side arc's spot (see [`SPOTS`]): one arc to a spot, all four placed.
    pub arc_spots: BTreeMap<String, String>,
    pub width: u16,
    /// The arcs' tracks show even with nothing to fill them (no value yet, no cast).
    pub always_show: bool,
    /// An arc hides while it's full (HP with no shield on it), and comes back as it drops.
    pub hide_full: bool,
    /// Your cast shows under your feet.
    pub show_cast: bool,
    /// Colours of your own ("#rrggbb"), by arc (see [`COLOURED`]); the rest keep theirs.
    pub colours: BTreeMap<String, String>,
    /// HP lost stays as a pale trail for a moment, then shrinks away.
    pub trail: bool,
    /// The HP arc turns yellow at half HP and red at a fifth.
    pub hp_warn: bool,
    /// The side arcs turned off (see [`SIDE_ARCS`]); the others show.
    pub hidden_arcs: BTreeSet<String>,
}

impl Default for VitalsSetup {
    fn default() -> Self {
        Self {
            style: STYLES[0].into(),
            effect: EFFECTS[0].into(),
            roundness: 50,
            text_size: 100,
            text_backing: TEXT_BACKINGS[0].into(),
            text_at: TEXT_PLACES[0].into(),
            numbers: NUMBERS[0].into(),
            cast: CASTS[0].into(),
            arc_spots: default_spots(),
            width: 100,
            always_show: false,
            hide_full: false,
            show_cast: true,
            colours: BTreeMap::new(),
            trail: false,
            hp_warn: false,
            hidden_arcs: BTreeSet::new(),
        }
    }
}

/// Changes to make; `None` leaves a setting as it is.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VitalsChange {
    pub style: Option<String>,
    pub effect: Option<String>,
    /// Any number: clamped.
    pub roundness: Option<i64>,
    pub text_size: Option<i64>,
    pub text_backing: Option<String>,
    pub text_at: Option<String>,
    pub numbers: Option<String>,
    pub cast: Option<String>,

    pub width: Option<i64>,
    pub always_show: Option<bool>,
    pub hide_full: Option<bool>,
    pub show_cast: Option<bool>,
    /// The colours of your own, all of them (an empty map: the built-in ones).
    pub colours: Option<BTreeMap<String, String>>,
    pub trail: Option<bool>,
    pub hp_warn: Option<bool>,
    /// A side arc to show or hide, with `arc_shown`, or to move, with `spot` (the arc there
    /// moves to where this one was).
    pub arc: Option<String>,
    pub arc_shown: Option<bool>,
    pub spot: Option<String>,
}

/// As at first: HP and stamina on the left, MP and flight power on the right.
fn default_spots() -> BTreeMap<String, String> {
    SIDE_ARCS.iter().zip(SPOTS).map(|(a, s)| ((*a).into(), s.into())).collect()
}

/// Each side arc in a spot of its own.
fn valid_spots(s: &BTreeMap<String, String>) -> bool {
    let arcs: BTreeSet<&str> = s.keys().map(String::as_str).collect();
    let spots: BTreeSet<&str> = s.values().map(String::as_str).collect();
    arcs == SIDE_ARCS.into_iter().collect() && spots == SPOTS.into_iter().collect()
}

fn is_hex_colour(c: &str) -> bool {
    c.len() == 7 && c.starts_with('#') && c[1..].chars().all(|c| c.is_ascii_hexdigit())
}

/// The colours kept: known arcs, real colours.
fn valid_colours(c: BTreeMap<String, String>) -> BTreeMap<String, String> {
    c.into_iter().filter(|(k, v)| COLOURED.contains(&k.as_str()) && is_hex_colour(v)).map(|(k, v)| (k, v.to_ascii_lowercase())).collect()
}

fn pick(choices: &[&str], v: Option<String>, now: &mut String) {
    if let Some(v) = v.filter(|v| choices.contains(&v.as_str())) {
        *now = v;
    }
}

fn clamp(range: &RangeInclusive<u16>, v: i64) -> u16 {
    v.clamp(i64::from(*range.start()), i64::from(*range.end())) as u16
}

impl VitalsSetup {
    pub fn update(&mut self, c: VitalsChange) {
        pick(&STYLES, c.style, &mut self.style);
        pick(&EFFECTS, c.effect, &mut self.effect);
        pick(&TEXT_BACKINGS, c.text_backing, &mut self.text_backing);
        pick(&TEXT_PLACES, c.text_at, &mut self.text_at);
        pick(&NUMBERS, c.numbers, &mut self.numbers);
        pick(&CASTS, c.cast, &mut self.cast);
        if let Some(r) = c.roundness {
            self.roundness = clamp(&ROUNDNESS, r);
        }
        if let Some(t) = c.text_size {
            self.text_size = clamp(&TEXT_SIZES, t);
        }
        if let Some(w) = c.width {
            self.width = clamp(&WIDTHS, w);
        }
        if let Some(a) = c.always_show {
            self.always_show = a;
        }
        if let Some(h) = c.hide_full {
            self.hide_full = h;
        }
        if let Some(s) = c.show_cast {
            self.show_cast = s;
        }
        if let Some(c) = c.colours {
            self.colours = valid_colours(c);
        }
        if let Some(t) = c.trail {
            self.trail = t;
        }
        if let Some(w) = c.hp_warn {
            self.hp_warn = w;
        }
        let arc = c.arc.filter(|a| SIDE_ARCS.contains(&a.as_str()));
        if let (Some(arc), Some(spot)) = (&arc, c.spot.filter(|s| SPOTS.contains(&s.as_str()))) {
            // Swapped with the arc there.
            let was = self.arc_spots.get(arc).cloned().unwrap_or_default();
            if let Some(other) = self.arc_spots.iter().find(|(_, s)| **s == spot).map(|(a, _)| a.clone()) {
                self.arc_spots.insert(other, was);
            }
            self.arc_spots.insert(arc.clone(), spot);
        }
        if let (Some(arc), Some(shown)) = (arc, c.arc_shown) {
            if shown {
                self.hidden_arcs.remove(&arc);
            } else {
                self.hidden_arcs.insert(arc);
            }
        }
    }

    /// Repairs values from a hand-edited settings file.
    pub fn sanitized(mut self) -> Self {
        let fixed = Self::default();
        let keep = |choices: &[&str], v: String, d: String| if choices.contains(&v.as_str()) { v } else { d };
        self.style = keep(&STYLES, self.style, fixed.style);
        self.effect = keep(&EFFECTS, self.effect, fixed.effect);
        self.text_backing = keep(&TEXT_BACKINGS, self.text_backing, fixed.text_backing);
        self.text_at = keep(&TEXT_PLACES, self.text_at, fixed.text_at);
        self.numbers = keep(&NUMBERS, self.numbers, fixed.numbers);
        self.cast = keep(&CASTS, self.cast, fixed.cast);
        self.roundness = self.roundness.min(*ROUNDNESS.end());
        if !TEXT_SIZES.contains(&self.text_size) {
            self.text_size = fixed.text_size;
        }
        if !WIDTHS.contains(&self.width) {
            self.width = fixed.width;
        }
        self.colours = valid_colours(self.colours);
        self.hidden_arcs.retain(|a| SIDE_ARCS.contains(&a.as_str()));
        if !valid_spots(&self.arc_spots) {
            self.arc_spots = fixed.arc_spots;
        }
        self
    }
}

#[tauri::command]
pub fn vitals_setup(app: State<'_, Arc<AppState>>) -> VitalsSetup {
    lock(&app.settings).vitals.clone()
}

/// Changes the vitals arcs' look; saved, and sent to the widget.
#[tauri::command]
pub fn set_vitals(handle: AppHandle, app: State<'_, Arc<AppState>>, change: VitalsChange) -> VitalsSetup {
    let mut s = lock(&app.settings);
    s.vitals.update(change);
    s.save(&app.config_dir);
    let setup = s.vitals.clone();
    drop(s);
    let _ = handle.emit("vitals-setup", &setup);
    setup
}

/// Puts the vitals arcs' look back as it was at first.
#[tauri::command]
pub fn reset_vitals(handle: AppHandle, app: State<'_, Arc<AppState>>) -> VitalsSetup {
    let mut s = lock(&app.settings);
    s.vitals = VitalsSetup::default();
    s.save(&app.config_dir);
    let setup = s.vitals.clone();
    drop(s);
    let _ = handle.emit("vitals-setup", &setup);
    setup
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changes_stay_valid() {
        let mut v = VitalsSetup::default();
        v.update(VitalsChange { style: Some("thin".into()), roundness: Some(250), text_size: Some(5), text_at: Some("bottom".into()), ..Default::default() });
        assert_eq!((v.style.as_str(), v.roundness, v.text_size, v.text_at.as_str()), ("thin", 100, 50, "bottom"));
        // Unknown choices are ignored.
        v.update(VitalsChange { style: Some("wavy".into()), numbers: Some("roman".into()), cast: Some("bar".into()), ..Default::default() });
        assert_eq!((v.style.as_str(), v.numbers.as_str(), v.cast.as_str()), ("thin", "value", "bar"));
        v.update(VitalsChange { width: Some(1000), always_show: Some(true), ..Default::default() });
        assert_eq!((v.width, v.always_show), (300, true));
        assert!(!v.hide_full);
        v.update(VitalsChange { hide_full: Some(true), ..Default::default() });
        assert!(v.hide_full);
        assert!(v.show_cast, "the cast shows unless turned off");
        v.update(VitalsChange { show_cast: Some(false), ..Default::default() });
        assert!(!v.show_cast);
        // Colours: known arcs and real colours only, all at once.
        let colours = [("hp", "#FF0000"), ("shield", "#ffffff"), ("wings", "#00ff00"), ("mp", "blue")];
        v.update(VitalsChange { colours: Some(colours.iter().map(|(k, c)| ((*k).into(), (*c).into())).collect()), ..Default::default() });
        assert_eq!(v.colours.iter().map(|(k, c)| (k.as_str(), c.as_str())).collect::<Vec<_>>(), [("hp", "#ff0000"), ("shield", "#ffffff")]);
        v.update(VitalsChange { colours: Some(BTreeMap::new()), ..Default::default() });
        assert!(v.colours.is_empty());
        // The newer looks, and the trail.
        v.update(VitalsChange { style: Some("framed".into()), trail: Some(true), ..Default::default() });
        assert_eq!((v.style.as_str(), v.trail), ("framed", true));
        // Flames, over any look; an unknown effect ignored.
        assert_eq!(v.effect, "none");
        v.update(VitalsChange { effect: Some("flames".into()), ..Default::default() });
        v.update(VitalsChange { effect: Some("lightning".into()), ..Default::default() });
        assert_eq!((v.style.as_str(), v.effect.as_str()), ("framed", "flames"));
        assert!(!v.hp_warn);
        v.update(VitalsChange { hp_warn: Some(true), ..Default::default() });
        assert!(v.hp_warn);
        // Only HP and stamina: MP and flight power off; an unknown arc ignored.
        for arc in ["mp", "flight", "wings"] {
            v.update(VitalsChange { arc: Some(arc.into()), arc_shown: Some(false), ..Default::default() });
        }
        assert_eq!(v.hidden_arcs.iter().map(String::as_str).collect::<Vec<_>>(), ["flight", "mp"]);
        v.update(VitalsChange { arc: Some("mp".into()), arc_shown: Some(true), ..Default::default() });
        // HP to the right, outside: MP, there before, takes HP's spot; stamina to the left,
        // outside, in HP's old spot again (MP inside it now); a spot that isn't one ignored.
        let spots = |v: &VitalsSetup| SIDE_ARCS.map(|a| v.arc_spots[a].clone());
        assert_eq!(spots(&v), ["left-outer", "left-inner", "right-outer", "right-inner"]);
        let to = |arc: &str, spot: &str| VitalsChange { arc: Some(arc.into()), spot: Some(spot.into()), ..Default::default() };
        v.update(to("hp", "right-outer"));
        assert_eq!(spots(&v), ["right-outer", "left-inner", "left-outer", "right-inner"]);
        v.update(to("stamina", "left-outer"));
        v.update(to("stamina", "up"));
        assert_eq!(spots(&v), ["right-outer", "left-outer", "left-inner", "right-inner"]);
        assert_eq!(v.hidden_arcs.iter().map(String::as_str).collect::<Vec<_>>(), ["flight"]);
    }

    #[test]
    fn spots_that_dont_place_each_arc_once_are_put_back() {
        let mut v = VitalsSetup::default();
        v.arc_spots.insert("mp".into(), "left-outer".into());
        assert_eq!(v.sanitized().arc_spots, default_spots());
    }

    #[test]
    fn a_settings_file_without_it_or_with_junk_reads_as_the_defaults() {
        let v: VitalsSetup = serde_json::from_str("{}").unwrap();
        assert_eq!(v, VitalsSetup::default());
        let v: VitalsSetup = serde_json::from_str(r#"{ "style": "wavy", "effect": "smoke", "text_size": 7, "roundness": 30, "width": 9 }"#).unwrap();
        let v = v.sanitized();
        assert_eq!((v.style.as_str(), v.effect.as_str(), v.text_size, v.roundness, v.width), ("solid", "none", 100, 30, 100));
    }
}
