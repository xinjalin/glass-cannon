//! How the target frame looks (settings → overlay → target frame): a player target's class
//! colour on their name or over the whole frame, their rank as a title or a number, and
//! which of the frame's parts show. Each change is saved and sent to the widget at once.

use std::collections::BTreeSet;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

use crate::{lock, AppState};

/// A player target's class colour: nowhere, on their name, or over the whole frame (to
/// tell at a glance who you're up against in PvP).
pub const CLASS_LOOKS: [&str; 3] = ["off", "name", "frame"];
/// A player's rank: the game's title for it (Supreme Commander … Ninth-Class Soldier), or
/// its number (1 the highest).
pub const RANK_STYLES: [&str; 2] = ["title", "number"];
/// The frame's parts that can be hidden.
pub const PARTS: [&str; 12] = ["percent", "class", "rank", "cp", "bar", "hp", "ttk", "cast", "attacking", "enrage", "best", "stagger"];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct TargetSetup {
    pub class_look: String,
    pub rank_style: String,
    /// The parts turned off (see [`PARTS`]); everything else shows.
    pub hidden: BTreeSet<String>,
}

impl Default for TargetSetup {
    fn default() -> Self {
        Self { class_look: CLASS_LOOKS[1].into(), rank_style: RANK_STYLES[0].into(), hidden: BTreeSet::new() }
    }
}

/// Changes to make; `None` leaves a setting as it is.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetChange {
    pub class_look: Option<String>,
    pub rank_style: Option<String>,
    /// A part to show or hide, with `shown`.
    pub part: Option<String>,
    pub shown: Option<bool>,
}

fn pick(choices: &[&str], v: Option<String>, now: &mut String) {
    if let Some(v) = v.filter(|v| choices.contains(&v.as_str())) {
        *now = v;
    }
}

impl TargetSetup {
    pub fn update(&mut self, c: TargetChange) {
        pick(&CLASS_LOOKS, c.class_look, &mut self.class_look);
        pick(&RANK_STYLES, c.rank_style, &mut self.rank_style);
        if let (Some(part), Some(shown)) = (c.part.filter(|p| PARTS.contains(&p.as_str())), c.shown) {
            if shown {
                self.hidden.remove(&part);
            } else {
                self.hidden.insert(part);
            }
        }
    }

    /// Repairs values from a hand-edited settings file.
    pub fn sanitized(mut self) -> Self {
        let fixed = Self::default();
        if !CLASS_LOOKS.contains(&self.class_look.as_str()) {
            self.class_look = fixed.class_look;
        }
        if !RANK_STYLES.contains(&self.rank_style.as_str()) {
            self.rank_style = fixed.rank_style;
        }
        self.hidden.retain(|p| PARTS.contains(&p.as_str()));
        self
    }
}

#[tauri::command]
pub fn target_setup(app: State<'_, Arc<AppState>>) -> TargetSetup {
    lock(&app.settings).target.clone()
}

/// Changes the target frame's look; saved, and sent to the widget.
#[tauri::command]
pub fn set_target_setup(handle: AppHandle, app: State<'_, Arc<AppState>>, change: TargetChange) -> TargetSetup {
    let mut s = lock(&app.settings);
    s.target.update(change);
    s.save(&app.config_dir);
    let setup = s.target.clone();
    drop(s);
    let _ = handle.emit("target-setup", &setup);
    setup
}

/// Puts the target frame's look back as it was at first.
#[tauri::command]
pub fn reset_target_setup(handle: AppHandle, app: State<'_, Arc<AppState>>) -> TargetSetup {
    let mut s = lock(&app.settings);
    s.target = TargetSetup::default();
    s.save(&app.config_dir);
    let setup = s.target.clone();
    drop(s);
    let _ = handle.emit("target-setup", &setup);
    setup
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changes_stay_valid() {
        let mut t = TargetSetup::default();
        assert_eq!((t.class_look.as_str(), t.rank_style.as_str()), ("name", "title"));
        t.update(TargetChange { class_look: Some("frame".into()), rank_style: Some("number".into()), ..Default::default() });
        assert_eq!((t.class_look.as_str(), t.rank_style.as_str()), ("frame", "number"));
        // Unknown choices and parts are ignored.
        t.update(TargetChange { class_look: Some("rainbow".into()), part: Some("hat".into()), shown: Some(false), ..Default::default() });
        assert_eq!(t.class_look, "frame");
        assert!(t.hidden.is_empty());
        // A part off, then on again.
        t.update(TargetChange { part: Some("stagger".into()), shown: Some(false), ..Default::default() });
        assert!(t.hidden.contains("stagger"));
        t.update(TargetChange { part: Some("stagger".into()), shown: Some(true), ..Default::default() });
        assert!(t.hidden.is_empty());
    }

    #[test]
    fn a_settings_file_without_it_or_with_junk_reads_as_the_defaults() {
        let t: TargetSetup = serde_json::from_str("{}").unwrap();
        assert_eq!(t, TargetSetup::default());
        let t: TargetSetup = serde_json::from_str(r#"{ "class_look": "x", "rank_style": "y", "hidden": ["cp", "hat"] }"#).unwrap();
        let t = t.sanitized();
        assert_eq!((t.class_look.as_str(), t.rank_style.as_str()), ("name", "title"));
        assert_eq!(t.hidden.iter().map(String::as_str).collect::<Vec<_>>(), ["cp"]);
    }
}
