//! Where the overlay opens: where you last left it, otherwise on the game's monitor
//! (or the primary one), near the right edge.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::{Monitor, PhysicalPosition, PhysicalSize, Runtime, WebviewWindow, Window};

const STATE_FILE: &str = "window.json";
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

pub fn state_path(config_dir: &Path) -> PathBuf {
    config_dir.join(STATE_FILE)
}

pub fn load(path: &Path) -> Option<Placement> {
    serde_json::from_slice(&fs::read(path).ok()?).ok()
}

/// Saves the window's position and size so the next start opens in the same place.
pub fn save<R: Runtime>(window: &Window<R>, path: &Path) {
    let (Ok(pos), Ok(size)) = (window.outer_position(), window.outer_size()) else {
        return;
    };
    // A minimised window reports a position far off-screen; don't remember that.
    if window.is_minimized().unwrap_or(false) {
        return;
    }
    let p = Placement {
        x: pos.x,
        y: pos.y,
        width: size.width,
        height: size.height,
    };
    if let Some(dir) = path.parent() {
        let _ = fs::create_dir_all(dir);
    }
    if let Ok(json) = serde_json::to_vec_pretty(&p) {
        let _ = fs::write(path, json);
    }
}

/// Positions the window before it is shown. Returns where it was put when that was
/// an automatic choice (not a restored position), so a later game launch can move it.
pub fn place<R: Runtime>(window: &WebviewWindow<R>, saved: Option<Placement>) -> Option<PhysicalPosition<i32>> {
    let monitors = window.available_monitors().unwrap_or_default();
    if let Some(p) = saved {
        // Back where it was, pulled fully onto the screen it's mostly on (a screen can
        // have changed size, or the window been left half off it).
        let r = crate::layout::Rect::new(p.x, p.y, p.width as i32, p.height as i32);
        let home = monitors
            .iter()
            .filter(|m| overlaps(m, &p))
            .max_by_key(|m| r.overlap(&crate::settings::work_area(m)));
        if let Some(m) = home {
            let r = crate::layout::keep_inside(r, crate::settings::work_area(m));
            // Saved as the window's outside; set_size takes the inside, so take off the frame
            // (or the meter would grow by it every time it opens).
            let frame = match (window.outer_size(), window.inner_size()) {
                (Ok(o), Ok(i)) => (o.width as i32 - i.width as i32, o.height as i32 - i.height as i32),
                _ => (0, 0),
            };
            let _ = window.set_size(PhysicalSize::new((r.w - frame.0).max(1) as u32, (r.h - frame.1).max(1) as u32));
            let _ = window.set_position(PhysicalPosition::new(r.x, r.y));
            return None;
        }
    }
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
    let pids: HashSet<u32> = meter_core::conns::find_processes("AION2")
        .ok()?
        .into_iter()
        .map(|p| p.pid)
        .collect();
    let r = win32::largest_window(&pids)?;
    let (cx, cy) = ((r.left + r.right) / 2, (r.top + r.bottom) / 2);
    monitors
        .iter()
        .find(|m| {
            let (mx, my) = (m.position().x, m.position().y);
            cx >= mx && cx < mx + m.size().width as i32 && cy >= my && cy < my + m.size().height as i32
        })
        .cloned()
}

mod win32 {
    use std::collections::HashSet;

    type Hwnd = isize;

    #[repr(C)]
    #[derive(Debug, Default, Clone, Copy)]
    pub struct Rect {
        pub left: i32,
        pub top: i32,
        pub right: i32,
        pub bottom: i32,
    }

    #[link(name = "user32")]
    extern "system" {
        fn EnumWindows(callback: unsafe extern "system" fn(Hwnd, isize) -> i32, lparam: isize) -> i32;
        fn GetWindowThreadProcessId(hwnd: Hwnd, pid: *mut u32) -> u32;
        fn IsWindowVisible(hwnd: Hwnd) -> i32;
        fn IsIconic(hwnd: Hwnd) -> i32;
        fn GetWindowRect(hwnd: Hwnd, rect: *mut Rect) -> i32;
    }

    struct Search<'a> {
        pids: &'a HashSet<u32>,
        best: Option<(i64, Rect)>,
    }

    unsafe extern "system" fn visit(hwnd: Hwnd, lparam: isize) -> i32 {
        // SAFETY: lparam is the &mut Search passed to EnumWindows below, alive for the call.
        let search = &mut *(lparam as *mut Search);
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, &mut pid);
        if search.pids.contains(&pid) && IsWindowVisible(hwnd) != 0 && IsIconic(hwnd) == 0 {
            let mut r = Rect::default();
            if GetWindowRect(hwnd, &mut r) != 0 {
                let area = (r.right - r.left) as i64 * (r.bottom - r.top) as i64;
                if search.best.is_none_or(|(a, _)| area > a) {
                    search.best = Some((area, r));
                }
            }
        }
        1 // keep enumerating
    }

    /// The largest visible top-level window owned by any of `pids`.
    pub fn largest_window(pids: &HashSet<u32>) -> Option<Rect> {
        if pids.is_empty() {
            return None;
        }
        let mut search = Search { pids, best: None };
        // SAFETY: the callback only touches `search`, which outlives the call.
        unsafe { EnumWindows(visit, &mut search as *mut Search as isize) };
        search.best.map(|(_, r)| r)
    }
}
