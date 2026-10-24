//! Where the windows open: where you last left them. The overlay otherwise opens on the
//! game's monitor (or the primary one), near the right edge; the others beside it.
//!
//! Every window's position and size are kept in `windows.json` as you move or resize it
//! (written at most once a second), so they come back next time even after a crash.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::{Monitor, PhysicalPosition, PhysicalSize, Runtime, WebviewWindow, Window};

/// The overlay's placement from before every window's was kept (read if there's no newer).
const STATE_FILE: &str = "window.json";
const WINDOWS_FILE: &str = "windows.json";
/// Gap from the monitor's right edge, in logical pixels.
const EDGE_MARGIN: f64 = 32.0;
/// Top of the overlay, as a fraction of the monitor height (clear of the minimap).
const TOP_FRACTION: f64 = 0.3;

/// Saved in physical pixels.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Placement {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// Every window's last placement, by label.
pub struct Placements {
    path: PathBuf,
    by_label: BTreeMap<String, Placement>,
    changed: bool,
}

impl Placements {
    pub fn load(config_dir: &Path) -> Self {
        let path = config_dir.join(WINDOWS_FILE);
        let mut by_label: BTreeMap<String, Placement> =
            fs::read(&path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        if !by_label.contains_key(crate::OVERLAY) {
            let old = fs::read(config_dir.join(STATE_FILE)).ok().and_then(|b| serde_json::from_slice(&b).ok());
            if let Some(p) = old {
                by_label.insert(crate::OVERLAY.into(), p);
            }
        }
        Self { path, by_label, changed: false }
    }

    pub fn get(&self, label: &str) -> Option<Placement> {
        self.by_label.get(label).copied()
    }

    pub fn set(&mut self, label: &str, p: Placement) {
        if self.by_label.get(label) != Some(&p) {
            self.by_label.insert(label.into(), p);
            self.changed = true;
        }
    }

    /// Writes the file if anything moved since the last time.
    pub fn save_if_changed(&mut self) {
        if !self.changed {
            return;
        }
        self.changed = false;
        if let Some(dir) = self.path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        if let Ok(json) = serde_json::to_vec_pretty(&self.by_label) {
            let tmp = self.path.with_extension("json.tmp");
            if fs::write(&tmp, json).is_ok() {
                let _ = fs::rename(&tmp, &self.path);
            }
        }
    }
}

/// Where a window is now (its outside: position and size), if it's showing. A minimised
/// window reports a position far off-screen, and a hidden one isn't anywhere to keep.
pub fn current<R: Runtime>(window: &Window<R>) -> Option<Placement> {
    if window.is_minimized().unwrap_or(false) || !window.is_visible().unwrap_or(false) {
        return None;
    }
    let (pos, size) = (window.outer_position().ok()?, window.outer_size().ok()?);
    Some(Placement { x: pos.x, y: pos.y, width: size.width, height: size.height })
}

/// Puts a window back where it was saved, pulled fully onto the screen it's mostly on (a
/// screen can have changed size, or the window been left half off it); with `size`, at its
/// saved size too. False when no screen has it any more.
pub fn restore<R: Runtime>(window: &WebviewWindow<R>, p: Placement, size: bool) -> bool {
    let now = window.outer_size().ok();
    let (width, height) = match (size, now) {
        (false, Some(s)) => (s.width, s.height),
        _ => (p.width, p.height),
    };
    let saved = Placement { width, height, ..p };
    let r = crate::layout::Rect::new(p.x, p.y, width as i32, height as i32);
    let monitors = window.available_monitors().unwrap_or_default();
    let Some(m) = monitors
        .iter()
        .filter(|m| overlaps(m, &saved))
        .max_by_key(|m| r.overlap(&crate::settings::work_area(m)))
    else {
        return false;
    };
    let r = crate::layout::keep_inside(r, crate::settings::work_area(m));
    if size {
        // Saved as the window's outside; set_size takes the inside, so take off the frame
        // (or the window would grow by it every time it opens).
        let frame = match (window.outer_size(), window.inner_size()) {
            (Ok(o), Ok(i)) => (o.width as i32 - i.width as i32, o.height as i32 - i.height as i32),
            _ => (0, 0),
        };
        let _ = window.set_size(PhysicalSize::new((r.w - frame.0).max(1) as u32, (r.h - frame.1).max(1) as u32));
    }
    let _ = window.set_position(PhysicalPosition::new(r.x, r.y));
    true
}

/// Positions the overlay before it is shown. Returns where it was put when that was an
/// automatic choice (not a restored position), so a later game launch can move it.
pub fn place<R: Runtime>(window: &WebviewWindow<R>, saved: Option<Placement>) -> Option<PhysicalPosition<i32>> {
    if saved.is_some_and(|p| restore(window, p, true)) {
        return None;
    }
    let monitors = window.available_monitors().unwrap_or_default();
    let target = game_monitor(&monitors)
        .or_else(|| window.primary_monitor().ok().flatten())
        .or_else(|| monitors.first().cloned())?;
    let pos = right_edge(&target, window.outer_size().ok()?);
    let _ = window.set_position(pos);
    Some(pos)
}

/// Moves an automatically placed window to the game's monitor once the game appears,
/// unless the user has moved it in the meantime.
pub fn follow_game<R: Runtime>(window: &WebviewWindow<R>, placed_at: PhysicalPosition<i32>) -> bool {
    if window.outer_position().ok() != Some(placed_at) {
        return true; // the user moved it; leave it alone
    }
    let monitors = window.available_monitors().unwrap_or_default();
    let Some(target) = game_monitor(&monitors) else {
        return false; // no game window yet; keep waiting
    };
    if let Ok(size) = window.outer_size() {
        let _ = window.set_position(right_edge(&target, size));
    }
    true
}

fn right_edge(m: &Monitor, size: PhysicalSize<u32>) -> PhysicalPosition<i32> {
    let margin = (EDGE_MARGIN * m.scale_factor()) as i32;
    let x = m.position().x + m.size().width as i32 - size.width as i32 - margin;
    let y = m.position().y + (m.size().height as f64 * TOP_FRACTION) as i32;
    PhysicalPosition::new(x.max(m.position().x), y)
}

fn overlaps(m: &Monitor, p: &Placement) -> bool {
    let (mx, my) = (m.position().x, m.position().y);
    let (mw, mh) = (m.size().width as i32, m.size().height as i32);
    // At least 80×40 physical pixels of the window must be on this monitor.
    let w = (p.x + p.width as i32).min(mx + mw) - p.x.max(mx);
    let h = (p.y + p.height as i32).min(my + mh) - p.y.max(my);
    w >= 80 && h >= 40
}

/// The monitor showing the game's main window.
fn game_monitor(monitors: &[Monitor]) -> Option<Monitor> {
    let r = crate::game_window::game_rect("AION2")?;
    let (cx, cy) = ((r.left + r.right) / 2, (r.top + r.bottom) / 2);
    monitors
        .iter()
        .find(|m| {
            let (mx, my) = (m.position().x, m.position().y);
            cx >= mx && cx < mx + m.size().width as i32 && cy >= my && cy < my + m.size().height as i32
        })
        .cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placements_are_kept_and_the_old_overlay_file_still_counts() {
        let dir = std::env::temp_dir().join(format!("gc-placements-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        // From before every window's was kept: just the overlay's.
        fs::write(dir.join(STATE_FILE), r#"{ "x": 10, "y": 20, "width": 400, "height": 250 }"#).unwrap();
        let mut p = Placements::load(&dir);
        assert_eq!(p.get(crate::OVERLAY), Some(Placement { x: 10, y: 20, width: 400, height: 250 }));
        let target = Placement { x: 600, y: 140, width: 340, height: 220 };
        p.set("target", target);
        p.save_if_changed();
        let again = Placements::load(&dir);
        assert_eq!(again.get("target"), Some(target));
        assert_eq!(again.get(crate::OVERLAY).map(|o| o.x), Some(10), "kept in the new file too");
        assert_eq!(again.get("history"), None);
        fs::remove_dir_all(dir).ok();
    }
}
