//! The overlay (HUD): one transparent window over the game's picture that holds the
//! widgets (the meter, boss alerts…). It follows the game window, shows only while the game
//! (or one of our windows) is in front, never takes the focus from the game, and lets the
//! mouse through except over a widget. Settings, history and analysis stay windows.
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
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, Runtime, State};

use crate::game_window::{Rect, Tracker};
use crate::{lock, AppState, OVERLAY};

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

/// Where the widgets are; the HUD reports it whenever its layout changes.
#[tauri::command]
pub fn hud_rects(app: State<'_, Arc<AppState>>, rects: Vec<HitRect>) {
    *lock(&app.hud_rects) = rects;
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

/// The overlay on or off (settings → overlay): the meter moves into it, or back out.
#[tauri::command]
pub fn set_overlay(handle: AppHandle, app: State<'_, Arc<AppState>>, on: bool) {
    let mut s = lock(&app.settings);
    s.overlay = on;
    s.save(&app.config_dir);
    drop(s);
    if let Some(main) = handle.get_webview_window(OVERLAY) {
        let _ = if on { main.hide() } else { main.show() };
    }
    if !on {
        app.hud_editing.store(false, Ordering::SeqCst);
    }
    crate::panels::overlay_switched(&handle, &app, on);
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
            // With the overlay on, the meter's window comes back while there's no game
            // window (so the app is never out of reach), and goes when the game appears.
            let mut meter_shown: Option<bool> = None;
            let mut tick = 0u32;
            loop {
                let Some(w) = handle.get_webview_window(HUD) else { return };
                let on = lock(&app.settings).overlay;
                let editing = app.hud_editing.load(Ordering::SeqCst);
                if tick.is_multiple_of(FIND_EVERY) || !shown {
                    let game = if on { tracker.find() } else { None };
                    show_wanted = game.is_some_and(|g| g.shows_overlay()) || (on && editing && shown);
                    let meter_wanted = !on || game.is_none();
                    if on && meter_shown != Some(meter_wanted) {
                        if let Some(main) = handle.get_webview_window(OVERLAY) {
                            if (if meter_wanted { main.show() } else { main.hide() }).is_ok() {
                                meter_shown = Some(meter_wanted);
                            }
                        }
                    }
                    if !on {
                        meter_shown = None;
                    }
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
                        if shown {
                            let _ = w.set_always_on_top(true);
                        }
                    }
                }
                if shown {
                    let ignore = !editing && !pointer_on_widget(&handle, &w, &app);
                    if ignoring != Some(ignore) && w.set_ignore_cursor_events(ignore).is_ok() {
                        ignoring = Some(ignore);
                    }
                }
                thread::sleep(if shown { WATCH } else { WATCH_HIDDEN });
            }
        })
        .expect("starting the HUD thread");
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
