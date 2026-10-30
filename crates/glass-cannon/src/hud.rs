//! The overlay (HUD): one transparent window over the game's picture that holds the
//! widgets (the meter, boss alerts…). It follows the game window, shows only while the game
//! (or one of our windows) is in front, never takes the focus from the game, and lets the
//! mouse through except over a widget. The launcher turns it on and off; settings, history
//! and analysis are windows. Replaying a recording, it covers the launcher's screen instead.
//!
//! Like Questlog's overlay: the page reports where its widgets are, and a watcher checks
//! the pointer against them and makes the window take the mouse only there.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use serde::Deserialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, Runtime, State, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

use crate::game_window::{Front, GameWindow, Rect, Tracker};
use crate::{lock, AppState, LAUNCHER};

pub const HUD: &str = "hud";
const LAYOUT_FILE: &str = "hud.json";
/// How often the pointer is checked while the HUD shows, and the game window looked for
/// (every few checks).
const WATCH: Duration = Duration::from_millis(50);
const FIND_EVERY: u32 = 4;
const WATCH_HIDDEN: Duration = Duration::from_millis(250);

/// A widget's box in the HUD's page, in physical pixels from its top left.
#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq)]
pub struct HitRect {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

impl HitRect {
    fn contains(&self, x: f64, y: f64) -> bool {
        (self.x..=self.x + self.width).contains(&x) && (self.y..=self.y + self.height).contains(&y)
    }
}

/// Where the widgets are; the HUD reports it whenever its layout changes: those that take
/// the mouse, and every widget's panel (for its frosted plate).
#[tauri::command]
pub fn hud_rects(app: State<'_, Arc<AppState>>, rects: Vec<HitRect>, panels: Option<Vec<HitRect>>) {
    *lock(&app.hud_rects) = rects;
    *lock(&app.hud_panels) = panels.unwrap_or_default();
}

/// Arranging the widgets: the whole HUD takes the mouse (and the keyboard) until done.
#[tauri::command]
pub fn hud_edit<R: Runtime>(handle: AppHandle<R>, app: State<'_, Arc<AppState>>, on: bool) {
    app.hud_editing.store(on, Ordering::SeqCst);
    if let Some(w) = handle.get_webview_window(HUD) {
        let _ = w.set_focusable(on);
    }
    let _ = handle.emit("hud-edit", on);
}

/// The overlay on or off (the launcher's button); saved for next time.
#[tauri::command]
pub fn set_overlay<R: Runtime>(handle: AppHandle<R>, app: State<'_, Arc<AppState>>, on: bool) {
    let mut s = lock(&app.settings);
    s.overlay_on = on;
    s.save(&app.config_dir);
    drop(s);
    if !on {
        hud_edit(handle, app, false);
    }
}

/// The widgets' places and sizes, as the HUD saved them (`hud.json`; empty at first).
#[tauri::command]
pub fn hud_layout(app: State<'_, Arc<AppState>>) -> Value {
    read_layout(&app.config_dir)
}

#[tauri::command]
pub fn set_hud_layout(app: State<'_, Arc<AppState>>, layout: Value) {
    let path = layout_path(&app.config_dir);
    if let Some(dir) = path.parent() {
        let _ = fs::create_dir_all(dir);
    }
    if let Ok(s) = serde_json::to_vec_pretty(&layout) {
        let tmp = path.with_extension("json.tmp");
        if fs::write(&tmp, s).is_ok() {
            let _ = fs::rename(&tmp, &path);
        }
    }
}

fn layout_path(dir: &Path) -> PathBuf {
    dir.join(LAYOUT_FILE)
}

fn read_layout(dir: &Path) -> Value {
    fs::read(layout_path(dir)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or(Value::Null)
}

/// Keeps the HUD over the game's picture and its click-through in step with the pointer.
/// Runs for the life of the app. Never holds a lock while it talks to the window.
pub fn watch<R: Runtime>(handle: AppHandle<R>, app: Arc<AppState>) {
    thread::Builder::new()
        .name("hud".into())
        .spawn(move || {
            let mut tracker = Tracker::new(&app.args.process);
            let (mut shown, mut placed, mut ignoring): (bool, Option<Rect>, Option<bool>) = (false, None, None);
            let mut show_wanted = false;
            // What the page was last told about the world map.
            let mut map_sent: Option<bool> = None;
            let mut plates = Plates::default();
            let mut tick = 0u32;
            // The game itself in front (not one of our windows), for the menu check.
            let mut game_in_front = false;
            let mut menus = crate::menus::MenuDetector::default();
            let started = std::time::Instant::now();
            loop {
                let Some(w) = handle.get_webview_window(HUD) else { return };
                let on = lock(&app.settings).overlay_on;
                let editing = app.hud_editing.load(Ordering::SeqCst);
                if tick.is_multiple_of(FIND_EVERY) || !shown {
                    let game = if app.args.replay.is_some() { replay_screen(&handle) } else { tracker.find() };
                    app.game_found.store(game.is_some(), Ordering::SeqCst);
                    game_in_front = game.is_some_and(|g| g.front == crate::game_window::Front::Game);
                    show_wanted = on && (game.is_some_and(|g| g.shows_overlay()) || (editing && shown));
                    if let Some(g) = game.filter(|_| show_wanted) {
                        if placed != Some(g.client) {
                            let _ = w.set_position(PhysicalPosition::new(g.client.left, g.client.top));
                            let _ = w.set_size(PhysicalSize::new(g.client.width() as u32, g.client.height() as u32));
                            placed = Some(g.client);
                        }
                    }
                }
                tick = tick.wrapping_add(1);
                if show_wanted != shown {
                    let ok = if show_wanted { w.show() } else { w.hide() };
                    if ok.is_ok() {
                        shown = show_wanted;
                        app.hud_shown.store(shown, Ordering::SeqCst);
                        if shown {
                            let _ = w.set_always_on_top(true);
                        }
                    }
                }
                // The world map: the widgets step aside in place, and their plates with them.
                // The window stays up, so they all come back at once (a window shown again
                // redraws each widget in its own time).
                // A game menu (settings → hide for game menus): as the map, from the cursor.
                let menu_check = shown && !editing && game_in_front && app.args.replay.is_none() && lock(&app.settings).hide_for_menus;
                let menu = menu_check && {
                    let cursor = crate::menus::cursor_showing();
                    let key = cursor && crate::menus::any_key_held();
                    menus.update(started.elapsed().as_millis() as u64, cursor, key)
                };
                let map = shown && !editing && (on_world_map(&app) || menu);
                if map_sent != Some(map) && handle.emit_to(HUD, "hud-map", map).is_ok() {
                    map_sent = Some(map);
                }
                let (frosted, frost) = {
                    let a = &lock(&app.settings).appearance;
                    (a.backdrop == "frosted", a.frost)
                };
                // No frost at all: clear glass, no blur to put behind the widgets.
                let wanted: Vec<HitRect> =
                    if shown && frosted && !map && crate::style::frosted(frost) { lock(&app.hud_panels).clone() } else { Vec::new() };
                plates.update(&handle, &w, &wanted, frost);
                if shown {
                    // The map takes every click while it's open.
                    let ignore = !editing && (map || !pointer_on_widget(&handle, &w, &app));
                    if ignoring != Some(ignore) && w.set_ignore_cursor_events(ignore).is_ok() {
                        ignoring = Some(ignore);
                    }
                }
                thread::sleep(if shown { WATCH } else { WATCH_HIDDEN });
            }
        })
        .expect("starting the HUD thread");
}

/// The world map is open and the overlay steps aside for it (settings → hide on the world
/// map): back within about 1.5 s of closing it, as the game stops asking for its contents.
pub fn on_world_map(app: &AppState) -> bool {
    // The setting first, on its own: the snapshot holds the meter while it reads the
    // settings, so holding both here the other way round could deadlock.
    let hide = lock(&app.settings).hide_on_world_map;
    hide && app.engine.meter().world_map_open(app.engine.now())
}

/// Replaying: the launcher's screen stands in for the game's picture.
fn replay_screen<R: Runtime>(handle: &AppHandle<R>) -> Option<GameWindow> {
    let m = handle.get_webview_window(LAUNCHER)?.current_monitor().ok().flatten()?;
    let (p, s) = (m.position(), m.size());
    let r = Rect { left: p.x, top: p.y, right: p.x + s.width as i32, bottom: p.y + s.height as i32 };
    Some(GameWindow { window: r, client: r, minimized: false, front: Front::Game })
}

/// "Frosted" in the overlay: the HUD is one window over the whole game, which Windows
/// can't blur under just its widgets, so each widget gets a plate: a small empty window
/// with Windows' frosted backdrop, just under the HUD, exactly behind the widget's panel,
/// letting the mouse through. The widget's own tint is then lighter, as the windows' was.
struct Plates<R: Runtime> {
    windows: Vec<WebviewWindow<R>>,
    /// Where each is (physical pixels), None while parked.
    at: Vec<Option<(i32, i32, u32, u32)>>,
    /// Shown once already (from then on they're only moved).
    shown: Vec<bool>,
    /// The frost setting each one was last given.
    frost: Vec<Option<u8>>,
}

impl<R: Runtime> Default for Plates<R> {
    fn default() -> Self {
        Self { windows: Vec::new(), at: Vec::new(), shown: Vec::new(), frost: Vec::new() }
    }
}

/// Where a plate waits while it isn't wanted (the map open, the game out of front): off
/// every screen. Moved there rather than hidden, because a hidden plate shown again has
/// Windows build its blur anew, which flashes.
const PARKED: PhysicalPosition<i32> = PhysicalPosition { x: -20_000, y: -20_000 };

impl<R: Runtime> Plates<R> {
    /// `frost`: the frost setting (the kind of blur and its tint).
    fn update(&mut self, handle: &AppHandle<R>, hud: &WebviewWindow<R>, wanted: &[HitRect], frost: u8) {
        let origin = hud.inner_position().ok();
        while self.windows.len() < wanted.len() {
            let label = format!("plate-{}", self.windows.len());
            let built = WebviewWindowBuilder::new(handle, &label, WebviewUrl::App("plate.html".into()))
                .title("Glass Cannon")
                .decorations(false)
                .transparent(true)
                .shadow(false)
                .resizable(false)
                .skip_taskbar(true)
                .always_on_top(true)
                .focusable(false)
                .visible(false)
                .build();
            let Ok(w) = built else { return };
            crate::style::plate(&w);
            let _ = w.set_ignore_cursor_events(true);
            self.windows.push(w);
            self.at.push(None);
            self.shown.push(false);
            self.frost.push(None);
        }
        for (i, w) in self.windows.iter().enumerate() {
            if self.frost[i] != Some(frost) {
                crate::style::set_plate_frost(w, frost);
                self.frost[i] = Some(frost);
                // A new kind of blur can bring the plate to the front: back under the HUD.
                below(w, hud);
            }
            let want = origin.and_then(|o| wanted.get(i).map(|r| (o.x + r.x as i32, o.y + r.y as i32, r.width as u32, r.height as u32)));
            if want == self.at[i] {
                continue;
            }
            match want {
                Some((x, y, width, height)) if width > 0 && height > 0 => {
                    let _ = w.set_position(PhysicalPosition::new(x, y));
                    let _ = w.set_size(PhysicalSize::new(width, height));
                    if !self.shown[i] {
                        self.shown[i] = w.show().is_ok();
                    }
                    below(w, hud);
                }
                _ => {
                    let _ = w.set_position(PARKED);
                }
            }
            self.at[i] = want;
        }
    }
}

/// Puts `plate` just under `hud` (both stay on top of the game).
fn below<R: Runtime>(plate: &WebviewWindow<R>, hud: &WebviewWindow<R>) {
    #[link(name = "user32")]
    extern "system" {
        fn SetWindowPos(hwnd: isize, after: isize, x: i32, y: i32, cx: i32, cy: i32, flags: u32) -> i32;
    }
    const SWP_NOSIZE: u32 = 0x1;
    const SWP_NOMOVE: u32 = 0x2;
    const SWP_NOACTIVATE: u32 = 0x10;
    if let (Ok(p), Ok(h)) = (plate.hwnd(), hud.hwnd()) {
        // SAFETY: both are our own windows' handles.
        unsafe { SetWindowPos(p.0 as isize, h.0 as isize, 0, 0, 0, 0, SWP_NOSIZE | SWP_NOMOVE | SWP_NOACTIVATE) };
    }
}

fn pointer_on_widget<R: Runtime>(handle: &AppHandle<R>, w: &tauri::WebviewWindow<R>, app: &AppState) -> bool {
    let (Ok(cursor), Ok(origin)) = (handle.cursor_position(), w.inner_position()) else {
        return false;
    };
    let (x, y) = (cursor.x - f64::from(origin.x), cursor.y - f64::from(origin.y));
    lock(&app.hud_rects).iter().any(|r| r.contains(x, y))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_widget_takes_the_pointer_over_its_box_only() {
        let r = HitRect { x: 100.0, y: 50.0, width: 300.0, height: 200.0 };
        assert!(r.contains(100.0, 50.0) && r.contains(400.0, 250.0));
        assert!(!r.contains(99.0, 60.0) && !r.contains(200.0, 251.0));
    }

    #[test]
    fn the_layout_is_kept() {
        let dir = std::env::temp_dir().join(format!("gc-hud-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        assert_eq!(read_layout(&dir), Value::Null);
        fs::write(layout_path(&dir), r#"{"meter":{"x":0.7,"y":0.3,"w":420}}"#).unwrap();
        assert_eq!(read_layout(&dir)["meter"]["w"], 420);
        fs::remove_dir_all(dir).ok();
    }
}
