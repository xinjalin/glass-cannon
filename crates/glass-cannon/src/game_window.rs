//! The game's window: where its picture is on screen, and whether it's what you're looking
//! at. The overlay (HUD) covers exactly that picture and shows only while the game, or one
//! of our own windows, is in front.

use std::collections::HashSet;
use std::time::{Duration, Instant};

/// How often the game's process list is read again (finding processes is the slow part).
const PIDS_EVERY: Duration = Duration::from_secs(5);

/// A rectangle on screen, in physical pixels (laid out as Win32's RECT).
#[repr(C)]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Rect {
    pub fn width(&self) -> i32 {
        self.right - self.left
    }

    pub fn height(&self) -> i32 {
        self.bottom - self.top
    }
}

/// The game's main window as it is now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GameWindow {
    /// Its whole window, frame and all.
    pub window: Rect,
    /// The picture: the window's inside, in screen coordinates.
    pub client: Rect,
    pub minimized: bool,
    /// What's in front: the game, one of our windows, or something else.
    pub front: Front,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Front {
    Game,
    Ours,
    Other,
}

impl GameWindow {
    /// Whether the overlay should show over it: the game is up and in front (or one of our
    /// windows is, opened from the overlay), and big enough to be the game's picture.
    pub fn shows_overlay(&self) -> bool {
        !self.minimized && self.front != Front::Other && self.client.width() >= 320 && self.client.height() >= 240
    }
}

/// Finds the game's window (the largest top-level window of its processes), reading its
/// process list again every few seconds.
pub struct Tracker {
    process: String,
    pids: HashSet<u32>,
    pids_at: Option<Instant>,
}

impl Tracker {
    pub fn new(process: &str) -> Self {
        Self { process: process.to_string(), pids: HashSet::new(), pids_at: None }
    }

    pub fn find(&mut self) -> Option<GameWindow> {
        if self.pids_at.is_none_or(|t| t.elapsed() >= PIDS_EVERY) {
            self.pids = meter_core::conns::find_processes(&self.process)
                .map(|ps| ps.into_iter().map(|p| p.pid).collect())
                .unwrap_or_default();
            self.pids_at = Some(Instant::now());
        }
        win32::game_window(&self.pids)
    }
}

mod win32 {
    use std::collections::HashSet;

    use super::{Front, GameWindow, Rect};

    type Hwnd = isize;

    #[repr(C)]
    #[derive(Default)]
    struct Point {
        x: i32,
        y: i32,
    }

    #[link(name = "user32")]
    extern "system" {
        fn EnumWindows(callback: unsafe extern "system" fn(Hwnd, isize) -> i32, lparam: isize) -> i32;
        fn GetWindowThreadProcessId(hwnd: Hwnd, pid: *mut u32) -> u32;
        fn IsWindowVisible(hwnd: Hwnd) -> i32;
        fn IsIconic(hwnd: Hwnd) -> i32;
        fn GetWindowRect(hwnd: Hwnd, rect: *mut Rect) -> i32;
        fn GetClientRect(hwnd: Hwnd, rect: *mut Rect) -> i32;
        fn ClientToScreen(hwnd: Hwnd, point: *mut Point) -> i32;
        fn GetForegroundWindow() -> Hwnd;
    }

    struct Search<'a> {
        pids: &'a HashSet<u32>,
        best: Option<(i64, Hwnd, Rect)>,
    }

    unsafe extern "system" fn visit(hwnd: Hwnd, lparam: isize) -> i32 {
        // SAFETY: lparam is the &mut Search passed to EnumWindows below, alive for the call.
        let search = &mut *(lparam as *mut Search);
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, &mut pid);
        if search.pids.contains(&pid) && IsWindowVisible(hwnd) != 0 {
            let mut r = Rect::default();
            if GetWindowRect(hwnd, &mut r) != 0 {
                // A minimised window keeps its size elsewhere; count it so it's still found.
                let area = if IsIconic(hwnd) != 0 { 1 } else { (r.right - r.left) as i64 * (r.bottom - r.top) as i64 };
                if search.best.is_none_or(|(a, _, _)| area > a) {
                    search.best = Some((area, hwnd, r));
                }
            }
        }
        1 // keep enumerating
    }

    /// The largest visible top-level window owned by any of `pids`, with its picture's
    /// place on screen and what's in front.
    pub fn game_window(pids: &HashSet<u32>) -> Option<GameWindow> {
        if pids.is_empty() {
            return None;
        }
        let mut search = Search { pids, best: None };
        // SAFETY: the callback only touches `search`, which outlives the call.
        unsafe { EnumWindows(visit, &mut search as *mut Search as isize) };
        let (_, hwnd, window) = search.best?;
        let mut inside = Rect::default();
        let mut origin = Point::default();
        // SAFETY: plain Win32 calls writing into locals; hwnd came from EnumWindows.
        let (minimized, front) = unsafe {
            GetClientRect(hwnd, &mut inside);
            ClientToScreen(hwnd, &mut origin);
            let fg = GetForegroundWindow();
            let mut fg_pid = 0u32;
            GetWindowThreadProcessId(fg, &mut fg_pid);
            let front = if fg == hwnd {
                Front::Game
            } else if fg_pid == std::process::id() {
                Front::Ours
            } else {
                Front::Other
            };
            (IsIconic(hwnd) != 0, front)
        };
        let client = Rect {
            left: origin.x,
            top: origin.y,
            right: origin.x + inside.right,
            bottom: origin.y + inside.bottom,
        };
        Some(GameWindow { window, client, minimized, front })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn game(front: Front, minimized: bool, w: i32, h: i32) -> GameWindow {
        let r = Rect { left: 0, top: 0, right: w, bottom: h };
        GameWindow { window: r, client: r, minimized, front }
    }

    #[test]
    fn the_overlay_shows_while_the_game_is_in_front() {
        assert!(game(Front::Game, false, 2560, 1440).shows_overlay());
        // Our settings or analysis window in front: still over the game.
        assert!(game(Front::Ours, false, 2560, 1440).shows_overlay());
        // A browser in front, the game minimised, or a launcher-sized window: hidden.
        assert!(!game(Front::Other, false, 2560, 1440).shows_overlay());
        assert!(!game(Front::Game, true, 2560, 1440).shows_overlay());
        assert!(!game(Front::Game, false, 300, 200).shows_overlay());
    }
}
