//! Locking the meter: clicks go through it to the game, except on its lock button.
//!
//! Windows can only make a whole window ignore the mouse, so while the meter is locked a
//! watcher checks where the pointer is and lets the window take the mouse again only
//! while the pointer is over the lock button.

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use serde::Deserialize;
use tauri::{AppHandle, Manager, Runtime, State};

use crate::{lock, AppState, OVERLAY};

/// How often the pointer is checked while locked (fast enough that the button reacts
/// straight away, and too little work to notice).
const WATCH_LOCKED: Duration = Duration::from_millis(50);
const WATCH_UNLOCKED: Duration = Duration::from_millis(250);
/// Extra room around the button that still counts as over it, in physical pixels.
const SLOP: f64 = 2.0;

/// The lock button's box in the overlay's page, in physical pixels from its top left.
#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq)]
pub struct ButtonRect {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

impl ButtonRect {
    fn contains(&self, x: f64, y: f64) -> bool {
        (self.x - SLOP..=self.x + self.width + SLOP).contains(&x)
            && (self.y - SLOP..=self.y + self.height + SLOP).contains(&y)
    }
}

/// Locks or unlocks the meter; saved for next time.
#[tauri::command]
pub fn set_locked(app: State<'_, Arc<AppState>>, locked: bool) {
    app.locked.store(locked, Ordering::SeqCst);
    let mut s = lock(&app.settings);
    s.locked = locked;
    s.save(&app.config_dir);
}

/// Where the lock button is; the overlay reports it whenever its layout changes.
#[tauri::command]
pub fn lock_button_rect(app: State<'_, Arc<AppState>>, rect: ButtonRect) {
    *lock(&app.lock_button) = Some(rect);
}

/// Keeps the overlay's click-through in step with the lock and the pointer. Runs for the
/// life of the app. Never holds a lock while it talks to the window: those calls wait for
/// the UI thread, which may be waiting for the same lock.
pub fn watch<R: Runtime>(handle: AppHandle<R>, app: Arc<AppState>) {
    thread::Builder::new()
        .name("click-through".into())
        .spawn(move || {
            let mut ignoring = false;
            loop {
                let locked = app.locked.load(Ordering::SeqCst);
                let Some(w) = handle.get_webview_window(OVERLAY) else { return };
                let ignore = locked && !pointer_on_button(&handle, &w, &app);
                if ignore != ignoring && w.set_ignore_cursor_events(ignore).is_ok() {
                    ignoring = ignore;
                }
                thread::sleep(if locked { WATCH_LOCKED } else { WATCH_UNLOCKED });
            }
        })
        .expect("starting the click-through thread");
}

fn pointer_on_button<R: Runtime>(handle: &AppHandle<R>, w: &tauri::WebviewWindow<R>, app: &AppState) -> bool {
    let rect = *lock(&app.lock_button);
    let Some(rect) = rect else { return false };
    let (Ok(cursor), Ok(origin)) = (handle.cursor_position(), w.inner_position()) else {
        return false;
    };
    rect.contains(cursor.x - f64::from(origin.x), cursor.y - f64::from(origin.y))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_button_takes_the_pointer_with_a_little_slop() {
        let r = ButtonRect {
            x: 100.0,
            y: 50.0,
            width: 20.0,
            height: 18.0,
        };
        assert!(r.contains(110.0, 60.0));
        assert!(r.contains(98.5, 49.0));
        assert!(r.contains(121.0, 69.0));
        assert!(!r.contains(97.0, 60.0));
        assert!(!r.contains(110.0, 71.0));
    }
}
