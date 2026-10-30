//! Global hotkeys, which work while the game has the focus, and closing with the game.
//!
//! - arrange the overlay's widgets (again to save and finish), as the launcher's arrange
//!   button; only while the overlay is over the game. Ctrl+Shift+F10 unless changed.
//! - show or hide the overlay's widgets for now (not saved: the overlay itself stays on,
//!   and turns on if it was off). Alt+E unless changed.
//!
//! Each is set in the settings (keybinds) as keys joined by "+": modifiers (Ctrl, Shift,
//! Alt, Win) then a key named as the browser names it (`KeyboardEvent.code`: "F10", "KeyH",
//! "Digit1"…), or one modifier key alone ("AltRight"), which counts when tapped: pressed
//! and let go quickly with nothing else pressed meanwhile (holding Alt to click something
//! in game isn't a tap). A lone modifier is the game's too (Alt shows its cursor), so the
//! default is a combination, as Questlog's overlay has.

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::{hud, lock, AppState};

/// The actions: (id, what it does, its default keys).
pub const ACTIONS: [(&str, &str, &str); 2] =
    [("arrange", "arrange the overlay", "Ctrl+Shift+F10"), ("show_hide", "show or hide the overlay", "Alt+KeyE")];

/// The keys for each action, as set in the settings (missing or unreadable: the default).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bindings {
    #[serde(default)]
    pub arrange: String,
    #[serde(default)]
    pub show_hide: String,
}

impl Bindings {
    /// The keys an action uses now.
    pub fn get(&self, action: &str) -> &str {
        let set = match action {
            "arrange" => &self.arrange,
            "show_hide" => &self.show_hide,
            _ => return "",
        };
        if parse(set).is_some() {
            set
        } else {
            ACTIONS.iter().find(|a| a.0 == action).map_or("", |a| a.2)
        }
    }

    /// Sets an action's keys ("" for its default). Err: unknown action, keys that can't be
    /// used, or keys another action already uses.
    pub fn set(&mut self, action: &str, keys: &str) -> Result<(), String> {
        let keys = if keys.is_empty() { ACTIONS.iter().find(|a| a.0 == action).map_or("", |a| a.2) } else { keys };
        let parsed = parse(keys).ok_or_else(|| format!("{keys} can't be used as a hotkey"))?;
        if let Some(other) = ACTIONS.iter().find(|a| a.0 != action && parse(self.get(a.0)) == Some(parsed)) {
            return Err(format!("{} already uses {keys}", other.1));
        }
        match action {
            "arrange" => self.arrange = keys.into(),
            "show_hide" => self.show_hide = keys.into(),
            _ => return Err(format!("no hotkey called {action}")),
        }
        Ok(())
    }
}

const VK_SHIFT: u8 = 0x10;
const VK_CONTROL: u8 = 0x11;
const VK_MENU: u8 = 0x12;
const VK_LWIN: u8 = 0x5B;
const VK_RWIN: u8 = 0x5C;

/// Modifiers, as bits.
const CTRL: u8 = 1;
const SHIFT: u8 = 2;
const ALT: u8 = 4;
const WIN: u8 = 8;

/// A hotkey: modifiers that must be held (exactly) and the key; or a lone modifier key,
/// tapped (`mods` 0).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Keys {
    mods: u8,
    vk: u8,
    tap: bool,
}

/// The virtual key for a `KeyboardEvent.code`, and whether it's a modifier.
fn vk(code: &str) -> Option<(u8, bool)> {
    let modifier = match code {
        "ShiftLeft" => Some(0xA0),
        "ShiftRight" => Some(0xA1),
        "ControlLeft" => Some(0xA2),
        "ControlRight" => Some(0xA3),
        "AltLeft" => Some(0xA4),
        "AltRight" => Some(0xA5),
        "MetaLeft" => Some(VK_LWIN),
        "MetaRight" => Some(VK_RWIN),
        _ => None,
    };
    if let Some(v) = modifier {
        return Some((v, true));
    }
    let one = |rest: &str, base: u8, max: u8| rest.parse::<u8>().ok().filter(|n| *n <= max).map(|n| base + n);
    let v = if let Some(c) = code.strip_prefix("Key").filter(|c| c.len() == 1 && c.as_bytes()[0].is_ascii_uppercase()) {
        Some(c.as_bytes()[0])
    } else if let Some(d) = code.strip_prefix("Digit") {
        one(d, 0x30, 9)
    } else if let Some(d) = code.strip_prefix("Numpad").filter(|d| d.len() == 1) {
        one(d, 0x60, 9)
    } else if let Some(f) = code.strip_prefix('F').filter(|f| !f.is_empty() && f.bytes().all(|b| b.is_ascii_digit())) {
        f.parse::<u8>().ok().filter(|n| (1..=24).contains(n)).map(|n| 0x6F + n)
    } else {
        Some(match code {
            "Space" => 0x20,
            "Enter" => 0x0D,
            "Tab" => 0x09,
            "Backspace" => 0x08,
            "Insert" => 0x2D,
            "Delete" => 0x2E,
            "Home" => 0x24,
            "End" => 0x23,
            "PageUp" => 0x21,
            "PageDown" => 0x22,
            "ArrowLeft" => 0x25,
            "ArrowUp" => 0x26,
            "ArrowRight" => 0x27,
            "ArrowDown" => 0x28,
            "Pause" => 0x13,
            "ScrollLock" => 0x91,
            "PrintScreen" => 0x2C,
            "ContextMenu" => 0x5D,
            "Backquote" => 0xC0,
            "Minus" => 0xBD,
            "Equal" => 0xBB,
            "BracketLeft" => 0xDB,
            "BracketRight" => 0xDD,
            "Backslash" => 0xDC,
            "Semicolon" => 0xBA,
            "Quote" => 0xDE,
            "Comma" => 0xBC,
            "Period" => 0xBE,
            "Slash" => 0xBF,
            "NumpadMultiply" => 0x6A,
            "NumpadAdd" => 0x6B,
            "NumpadSubtract" => 0x6D,
            "NumpadDecimal" => 0x6E,
            "NumpadDivide" => 0x6F,
            _ => return None,
        })
    };
    v.map(|v| (v, false))
}

/// "Ctrl+Shift+F10", "AltRight"…; None when it can't be a hotkey.
fn parse(keys: &str) -> Option<Keys> {
    let parts: Vec<&str> = keys.split('+').collect();
    let (last, mods) = parts.split_last()?;
    let mut bits = 0;
    for m in mods {
        let bit = match *m {
            "Ctrl" => CTRL,
            "Shift" => SHIFT,
            "Alt" => ALT,
            "Win" => WIN,
            _ => return None,
        };
        if bits & bit != 0 {
            return None;
        }
        bits |= bit;
    }
    let (vk, modifier) = vk(last)?;
    // A modifier only alone; a plain key only with a modifier (else it'd fire while typing
    // or playing), F keys and the like excepted.
    let plain_ok = (0x70..=0x87).contains(&vk) || matches!(vk, 0x13 | 0x91 | 0x2C);
    match (modifier, bits) {
        (true, 0) => Some(Keys { mods: 0, vk, tap: true }),
        (true, _) => None,
        (false, 0) if !plain_ok => None,
        (false, _) => Some(Keys { mods: bits, vk, tap: false }),
    }
}

#[link(name = "user32")]
extern "system" {
    fn GetAsyncKeyState(vk: i32) -> i16;
}

/// Whether a key is held now, from the keyboard.
fn down(vk: u8) -> bool {
    // SAFETY: a plain query of the keyboard's state.
    unsafe { GetAsyncKeyState(i32::from(vk)) < 0 }
}

/// The modifiers held now.
fn mods_held(down: &dyn Fn(u8) -> bool) -> u8 {
    let mut m = 0;
    if down(VK_CONTROL) {
        m |= CTRL;
    }
    if down(VK_SHIFT) {
        m |= SHIFT;
    }
    if down(VK_MENU) {
        m |= ALT;
    }
    if down(VK_LWIN) || down(VK_RWIN) {
        m |= WIN;
    }
    m
}

/// Every key (and mouse button) held now, as 256 bits.
fn all_held(down: &dyn Fn(u8) -> bool) -> [u64; 4] {
    let mut held = [0u64; 4];
    for v in 1..=254u8 {
        if down(v) {
            held[usize::from(v / 64)] |= 1 << (v % 64);
        }
    }
    held
}

/// A tap is at most this long.
const TAP_MS: u128 = 600;
const POLL: Duration = Duration::from_millis(10);
const PAUSED_AT_MOST: Duration = Duration::from_secs(30);

/// Watches one hotkey: whether it went off since the last look.
#[derive(Default)]
struct Watch {
    was_down: bool,
    /// For a tap: when it went down, the keys held then, and nothing else pressed since.
    since: Option<(Instant, [u64; 4], bool)>,
}

impl Watch {
    /// `at`: now; `down`: whether a key is held now.
    fn fired(&mut self, k: Keys, at: Instant, down: &dyn Fn(u8) -> bool) -> bool {
        let now = down(k.vk);
        let was = std::mem::replace(&mut self.was_down, now);
        if !k.tap {
            return now && !was && mods_held(down) == k.mods;
        }
        match (was, now) {
            (false, true) => {
                self.since = Some((at, all_held(down), true));
                false
            }
            (true, true) => {
                if let Some((_, at_start, clean)) = &mut self.since {
                    let held = all_held(down);
                    if held.iter().zip(at_start.iter()).any(|(h, s)| h & !s != 0) {
                        *clean = false;
                    }
                }
                false
            }
            (true, false) => self.since.take().is_some_and(|(t, _, clean)| clean && at.duration_since(t).as_millis() <= TAP_MS),
            (false, false) => false,
        }
    }
}

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
    fn UnregisterHotKey(hwnd: isize, id: i32) -> i32;
    fn PeekMessageW(msg: *mut Msg, hwnd: isize, min: u32, max: u32, remove: u32) -> i32;
}

const WM_HOTKEY: u32 = 0x0312;
const PM_REMOVE: u32 = 1;
const MOD_ALT: u32 = 0x1;
const MOD_CONTROL: u32 = 0x2;
const MOD_SHIFT: u32 = 0x4;
const MOD_WIN: u32 = 0x8;
const MOD_NOREPEAT: u32 = 0x4000;
/// A hotkey another app holds is tried again this often.
const RETRY: Duration = Duration::from_secs(5);

/// `RegisterHotKey`'s modifiers for ours.
fn win_mods(mods: u8) -> u32 {
    let mut m = MOD_NOREPEAT;
    for (ours, theirs) in [(CTRL, MOD_CONTROL), (SHIFT, MOD_SHIFT), (ALT, MOD_ALT), (WIN, MOD_WIN)] {
        if mods & ours != 0 {
            m |= theirs;
        }
    }
    m
}

/// Where each hotkey stands, for the settings.
pub fn state_of(app: &AppState, action: &str) -> &'static str {
    lock(&app.hotkey_states).get(action).copied().unwrap_or("waiting for the game")
}

/// Milliseconds since 1970, for the menu check.
pub fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

/// What a hotkey does.
fn act(handle: &AppHandle, app: &AppState, action: &str) {
    match action {
        "arrange" => {
            let editing = app.hud_editing.load(Ordering::SeqCst);
            if editing || app.hud_shown.load(Ordering::SeqCst) {
                hud::hud_edit(handle.clone(), handle.state::<Arc<AppState>>(), !editing);
            }
        }
        _ => hud::show_hide(handle, app),
    }
}

/// Key combinations go through Windows' `RegisterHotKey` (as Questlog's overlay does
/// with its Alt+E): Windows sees them before the game does, whatever the game does with
/// its input. They're held only while the game is up (so they don't take the keys from
/// other apps the rest of the time), and let go while new keys are being picked in the
/// settings (so the settings see them). One that another app holds is tried again every
/// few seconds. A lone modifier can't be registered: it's read from the keyboard's state
/// instead, a hundred times a second.
pub fn start(handle: AppHandle, app: Arc<AppState>) {
    let _ = thread::Builder::new().name("hotkeys".into()).spawn(move || {
        let mut watches: [Watch; 2] = Default::default();
        let mut paused_since: Option<Instant> = None;
        // What each action has registered now (its keys), and when a refused one was tried.
        let mut held: [Option<Keys>; 2] = [None; 2];
        let mut refused: [Option<(Keys, Instant)>; 2] = [None; 2];
        let mut msg = Msg { hwnd: 0, message: 0, wparam: 0, lparam: 0, time: 0, pt: [0; 2], private: 0 };
        loop {
            thread::sleep(POLL);
            let bindings = lock(&app.settings).hotkeys.clone();
            // While a new key is being picked in the settings, nothing goes off (for a while
            // at most, should the settings close in the middle).
            let paused = app.hotkeys_paused.load(Ordering::SeqCst);
            paused_since = if paused { Some(paused_since.unwrap_or_else(Instant::now)) } else { None };
            if paused_since.is_some_and(|t| t.elapsed() > PAUSED_AT_MOST) {
                app.hotkeys_paused.store(false, Ordering::SeqCst);
            }
            let game = app.game_found.load(Ordering::SeqCst);
            for (k, (action, _, _)) in ACTIONS.iter().enumerate() {
                let keys = parse(bindings.get(action));
                let id = k as i32 + 1;
                let want = keys.filter(|k| !k.tap && game && !paused);
                if held[k] != want {
                    if held[k].take().is_some() {
                        // SAFETY: our own thread's hotkey.
                        unsafe { UnregisterHotKey(0, id) };
                    }
                    if let Some(w) = want {
                        let retry_due = refused[k].is_none_or(|(r, at)| r != w || at.elapsed() >= RETRY);
                        // SAFETY: a thread hotkey (no window); this thread's queue gets it.
                        if retry_due && unsafe { RegisterHotKey(0, id, win_mods(w.mods), u32::from(w.vk)) } != 0 {
                            held[k] = Some(w);
                            refused[k] = None;
                        } else if retry_due {
                            refused[k] = Some((w, Instant::now()));
                        }
                    }
                }
                let state = match keys {
                    Some(k) if k.tap => "read from the keyboard",
                    Some(_) if held[k].is_some() => "ready",
                    Some(_) if refused[k].is_some() => "taken by another app (trying again)",
                    _ => "waiting for the game",
                };
                lock(&app.hotkey_states).insert(action, state);
                // A lone modifier, tapped.
                if let Some(t) = keys.filter(|k| k.tap) {
                    if watches[k].fired(t, Instant::now(), &down) && !paused {
                        act(&handle, &app, action);
                    }
                }
            }
            // SAFETY: a plain look at this thread's own message queue.
            while unsafe { PeekMessageW(&mut msg, 0, 0, 0, PM_REMOVE) } != 0 {
                if msg.message == WM_HOTKEY && !paused {
                    if let Some((action, _, _)) = ACTIONS.get(msg.wparam.wrapping_sub(1)) {
                        act(&handle, &app, action);
                    }
                }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_read_as_the_settings_write_them() {
        assert_eq!(parse("Ctrl+Shift+F10"), Some(Keys { mods: CTRL | SHIFT, vk: 0x79, tap: false }));
        assert_eq!(parse("AltRight"), Some(Keys { mods: 0, vk: 0xA5, tap: true }));
        assert_eq!(parse("Alt+KeyH"), Some(Keys { mods: ALT, vk: b'H', tap: false }));
        assert_eq!(parse("F9"), Some(Keys { mods: 0, vk: 0x78, tap: false }));
        assert_eq!(parse("Ctrl+Digit3"), Some(Keys { mods: CTRL, vk: 0x33, tap: false }));
        // A plain letter would go off while typing; a modifier only alone; no repeats.
        assert_eq!(parse("KeyH"), None);
        assert_eq!(parse("Ctrl+AltRight"), None);
        assert_eq!(parse("Ctrl+Ctrl+F1"), None);
        assert_eq!(parse("Hyper+F1"), None);
        assert_eq!(parse(""), None);
    }

    /// Plays key states (the keys held at each look, 10 ms apart) to a watch: when it fired.
    fn play(keys: &str, looks: &[&[u8]]) -> Vec<usize> {
        let k = parse(keys).unwrap();
        let mut w = Watch::default();
        let t0 = Instant::now();
        let mut fired = Vec::new();
        for (i, held) in looks.iter().enumerate() {
            let down = |vk: u8| held.contains(&vk);
            if w.fired(k, t0 + POLL * i as u32, &down) {
                fired.push(i);
            }
        }
        fired
    }

    #[test]
    fn a_tap_counts_a_hold_or_a_combo_doesnt() {
        const RALT: &[u8] = &[0xA5, VK_MENU];
        // AltGr keyboards hold a Left Ctrl with it.
        const ALTGR: &[u8] = &[0xA5, VK_MENU, 0xA2, VK_CONTROL];
        assert_eq!(play("AltRight", &[&[], RALT, RALT, &[]]), vec![3]);
        assert_eq!(play("AltRight", &[&[], ALTGR, ALTGR, &[]]), vec![3]);
        // Something else pressed meanwhile (a click with Alt held): not a tap.
        assert_eq!(play("AltRight", &[&[], RALT, &[0xA5, VK_MENU, 0x01], RALT, &[]]), Vec::<usize>::new());
        // Held too long.
        let mut long: Vec<&[u8]> = vec![&[]];
        long.extend(std::iter::repeat_n(RALT, 70));
        long.push(&[]);
        assert!(play("AltRight", &long).is_empty());
        // A combo goes off as its key goes down, with exactly its modifiers.
        const CS: &[u8] = &[VK_CONTROL, VK_SHIFT];
        const CS_F10: &[u8] = &[VK_CONTROL, VK_SHIFT, 0x79];
        assert_eq!(play("Ctrl+Shift+F10", &[CS, CS_F10, CS_F10, CS]), vec![1]);
        assert!(play("Ctrl+Shift+F10", &[&[0x79], &[VK_CONTROL, 0x79]]).is_empty());
    }

    #[test]
    fn bindings_default_and_refuse_clashes() {
        let mut b = Bindings::default();
        assert_eq!(b.get("show_hide"), "Alt+KeyE");
        assert_eq!(b.get("arrange"), "Ctrl+Shift+F10");
        assert!(b.set("show_hide", "Ctrl+Shift+F10").is_err());
        assert!(b.set("show_hide", "KeyQ").is_err());
        b.set("show_hide", "Ctrl+Shift+F12").unwrap();
        assert_eq!(b.get("show_hide"), "Ctrl+Shift+F12");
        b.set("show_hide", "").unwrap();
        assert_eq!(b.get("show_hide"), "Alt+KeyE");
        // A hand-edited file with keys that can't be used: the default.
        b.arrange = "nonsense".into();
        assert_eq!(b.get("arrange"), "Ctrl+Shift+F10");
    }
}
