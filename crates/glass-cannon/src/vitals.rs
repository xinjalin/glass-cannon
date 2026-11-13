//! How the vitals arcs widget looks (settings → overlay → vitals arcs): the arcs' look,
//! roundness and thickness, whether they always show, their numbers (what, how big, on what, at which end) and the cast's look.
//! Each change is saved and sent to the widget at once.

use std::ops::RangeInclusive;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

use crate::{lock, AppState};

/// The arcs' looks: thick with a dark track; thin lines with a glow; cut into cells.
pub const STYLES: [&str; 3] = ["solid", "thin", "segmented"];
/// What's behind the numbers: a dark outline, a soft shadow, a dark plate, nothing.
pub const TEXT_BACKINGS: [&str; 4] = ["outline", "shadow", "plate", "none"];
/// Which end of the arcs the numbers go by.
pub const TEXT_PLACES: [&str; 2] = ["top", "bottom"];
/// The numbers: value and max, the share in percent, or the share by each arc's end with
/// the values under them.
pub const NUMBERS: [&str; 3] = ["value", "percent", "both"];
/// The cast: along an arc under your feet, or a straight bar with its time at each end.
pub const CASTS: [&str; 2] = ["arc", "bar"];
/// The numbers' size, in percent of the overlay's text.
pub const TEXT_SIZES: RangeInclusive<u16> = 50..=200;
/// How round the arcs are, in percent: 0 straight, 50 as they were, 100 the roundest.
pub const ROUNDNESS: RangeInclusive<u16> = 0..=100;
/// How thick the arcs are, in percent of each look's own.
pub const WIDTHS: RangeInclusive<u16> = 50..=300;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct VitalsSetup {
    pub style: String,
    pub roundness: u16,
    pub text_size: u16,
    pub text_backing: String,
    pub text_at: String,
    pub numbers: String,
    pub cast: String,
    pub width: u16,
    /// The arcs' tracks show even with nothing to fill them (no value yet, no cast).
    pub always_show: bool,
    /// Your cast shows under your feet.
    pub show_cast: bool,
}

impl Default for VitalsSetup {
    fn default() -> Self {
        Self {
            style: STYLES[0].into(),
            roundness: 50,
            text_size: 100,
            text_backing: TEXT_BACKINGS[0].into(),
            text_at: TEXT_PLACES[0].into(),
            numbers: NUMBERS[0].into(),
            cast: CASTS[0].into(),
            width: 100,
            always_show: false,
            show_cast: true,
        }
    }
}

/// Changes to make; `None` leaves a setting as it is.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VitalsChange {
    pub style: Option<String>,
    /// Any number: clamped.
    pub roundness: Option<i64>,
    pub text_size: Option<i64>,
    pub text_backing: Option<String>,
    pub text_at: Option<String>,
    pub numbers: Option<String>,
    pub cast: Option<String>,
    pub width: Option<i64>,
    pub always_show: Option<bool>,
    pub show_cast: Option<bool>,
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
        if let Some(c) = c.show_cast {
            self.show_cast = c;
        }
    }

    /// Repairs values from a hand-edited settings file.
    pub fn sanitized(mut self) -> Self {
        let fixed = Self::default();
        let keep = |choices: &[&str], v: String, d: String| if choices.contains(&v.as_str()) { v } else { d };
        self.style = keep(&STYLES, self.style, fixed.style);
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
        assert!(v.show_cast, "the cast shows unless turned off");
        v.update(VitalsChange { show_cast: Some(false), ..Default::default() });
        assert!(!v.show_cast);
    }

    #[test]
    fn a_settings_file_without_it_or_with_junk_reads_as_the_defaults() {
        let v: VitalsSetup = serde_json::from_str("{}").unwrap();
        assert_eq!(v, VitalsSetup::default());
        let v: VitalsSetup = serde_json::from_str(r#"{ "style": "wavy", "text_size": 7, "roundness": 30, "width": 9 }"#).unwrap();
        let v = v.sanitized();
        assert_eq!((v.style.as_str(), v.text_size, v.roundness, v.width), ("solid", 100, 30, 100));
    }
}
