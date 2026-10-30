//! Global hotkeys, which work while the game has the focus, and closing with the game.
//!
//! - Ctrl+Shift+F10: arrange the overlay's widgets (again to save and finish), as the
//!   launcher's arrange button; only while the overlay is over the game.
//! - Ctrl+Shift+F12: show or hide the overlay, as the launcher's play button.
//!
//! Windows' `RegisterHotKey` on a thread of its own, whose message loop gets `WM_HOTKEY`.

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use tauri::{AppHandle, Manager};

use crate::{hud, lock, AppState};

const MOD_CONTROL: u32 = 0x2;
const MOD_SHIFT: u32 = 0x4;
const MOD_NOREPEAT: u32 = 0x4000;
const VK_F10: u32 = 0x79;
const VK_F12: u32 = 0x7B;
const WM_HOTKEY: u32 = 0x0312;

const ARRANGE: i32 = 1;
const SHOW_HIDE: i32 = 2;

/// The hotkeys, for the settings to list.
pub const LIST: [(&str, &str); 2] = [("Ctrl+Shift+F10", "arrange the overlay"), ("Ctrl+Shift+F12", "show or hide the overlay")];

#[repr(C)]
struct Msg {
    hwnd: isize,
    message: u32,
    wparam: usize,
    lparam: isize,
    time: u32,
    pt: [i32; 2],
    private: u32,
}

#[link(name = "user32")]
extern "system" {
    fn RegisterHotKey(hwnd: isize, id: i32, modifiers: u32, vk: u32) -> i32;
    fn GetMessageW(msg: *mut Msg, hwnd: isize, min: u32, max: u32) -> i32;
}

pub fn start(handle: AppHandle, app: Arc<AppState>) {
    let _ = thread::Builder::new().name("hotkeys".into()).spawn(move || {
        // SAFETY: thread hotkeys (no window); this thread's queue gets their messages.
        let ok = unsafe {
            [(ARRANGE, VK_F10), (SHOW_HIDE, VK_F12)]
                .iter()
                .filter(|(id, vk)| RegisterHotKey(0, *id, MOD_CONTROL | MOD_SHIFT | MOD_NOREPEAT, *vk) != 0)
                .count()
        };
        if ok == 0 {
            return;
        }
        let mut msg = Msg { hwnd: 0, message: 0, wparam: 0, lparam: 0, time: 0, pt: [0; 2], private: 0 };
        // SAFETY: a plain message loop on this thread's own queue.
        while unsafe { GetMessageW(&mut msg, 0, 0, 0) } > 0 {
            if msg.message != WM_HOTKEY {
                continue;
            }
            match msg.wparam as i32 {
                ARRANGE => {
                    let editing = app.hud_editing.load(Ordering::SeqCst);
                    if editing || app.hud_shown.load(Ordering::SeqCst) {
                        hud::hud_edit(handle.clone(), handle.state::<Arc<AppState>>(), !editing);
                    }
                }
                SHOW_HIDE => {
                    let on = lock(&app.settings).overlay_on;
                    hud::set_overlay(handle.clone(), handle.state::<Arc<AppState>>(), !on);
                }
                _ => {}
            }
        }
    });
}

/// The game gone this long closes the app (settings → close with the game).
const GAME_GONE: Duration = Duration::from_secs(5);

/// Closes the app once the game it saw has gone, if that's on. Not for replays.
pub fn close_with_game(handle: AppHandle, app: Arc<AppState>) {
    if app.args.replay.is_some() {
        return;
    }
    let _ = thread::Builder::new().name("close-with-game".into()).spawn(move || {
        let (mut seen, mut gone): (bool, Option<Instant>) = (false, None);
        loop {
            thread::sleep(Duration::from_secs(1));
            if app.game_found.load(Ordering::SeqCst) {
                seen = true;
                gone = None;
            } else if seen {
                let since = *gone.get_or_insert_with(Instant::now);
                if since.elapsed() >= GAME_GONE && lock(&app.settings).close_with_game {
                    handle.exit(0);
                    return;
                }
            }
        }
    });
}
