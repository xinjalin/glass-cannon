//! Game menus (inventory, character, skills…), from the mouse cursor: they send the server
//! nothing (checked on a recording that opened each in turn), but with the game's action
//! controls the cursor is hidden while you play and shows when a menu opens. Holding a key
//! (Alt) shows it too, to click something without a menu: then it's a peek, not a menu.
//! With the classic controls the cursor always shows, so this does nothing useful there:
//! the setting is off by default and says so.
//!
//! Alt is the game's cursor key, and it can toggle the cursor on and leave it there with no
//! key held: a cursor that comes up within a moment of Alt being pressed (or while Alt is
//! pressed with it showing) is your cursor, not a menu, until it goes away.
//!
//! The idea is from SkeeveAN's Aion DPS Meter notes (`docs/plans/menu-detect-cursor.md`);
//! this is our own code.

/// The cursor showing this long with no key held is a menu.
const MENU_AFTER_MS: u64 = 250;
/// A menu stays this long after the cursor hides (no flicker between two menus).
const MENU_KEPT_MS: u64 = 300;
/// A key held longer than this is stuck, not a peek.
const STUCK_KEY_MS: u64 = 20_000;
/// The cursor coming up this soon after Alt is pressed or let go is the game's cursor.
const ALT_CURSOR_MS: u64 = 1000;

/// Decides from the cursor and the keys, a few times a second, whether a menu is open.
#[derive(Debug, Default)]
pub struct MenuDetector {
    /// Since when the cursor has shown with no key held.
    bare_since: Option<u64>,
    /// Since when a key has been held.
    key_since: Option<u64>,
    /// The menu is open; and when the cursor last showed while it was.
    open: bool,
    last_seen: u64,
    /// The cursor showing now is the game's cursor, from Alt.
    alt_cursor: bool,
}

impl MenuDetector {
    /// `now_ms`: any clock; `cursor`: the cursor shows; `key`: some key is held; `alt_ms`:
    /// when Alt was last pressed (the same clock).
    pub fn update(&mut self, now_ms: u64, cursor: bool, key: bool, alt_ms: Option<u64>) -> bool {
        self.key_since = if key { Some(self.key_since.unwrap_or(now_ms)) } else { None };
        let peeking = self.key_since.is_some_and(|t| now_ms.saturating_sub(t) < STUCK_KEY_MS);
        if cursor && alt_ms.is_some_and(|t| now_ms.saturating_sub(t) <= ALT_CURSOR_MS) {
            self.alt_cursor = true;
        }
        if !cursor {
            self.alt_cursor = false;
            self.bare_since = None;
            if self.open && now_ms.saturating_sub(self.last_seen) > MENU_KEPT_MS {
                self.open = false;
            }
            return self.open;
        }
        if peeking || self.alt_cursor {
            self.bare_since = None;
            self.open = false;
            return false;
        }
        let since = *self.bare_since.get_or_insert(now_ms);
        if now_ms.saturating_sub(since) >= MENU_AFTER_MS {
            self.open = true;
        }
        if self.open {
            self.last_seen = now_ms;
        }
        self.open
    }
}

/// The cursor is showing (anywhere).
pub fn cursor_showing() -> bool {
    #[repr(C)]
    struct CursorInfo {
        size: u32,
        flags: u32,
        cursor: isize,
        pos: [i32; 2],
    }
    #[link(name = "user32")]
    extern "system" {
        fn GetCursorInfo(info: *mut CursorInfo) -> i32;
    }
    const CURSOR_SHOWING: u32 = 0x1;
    let mut info = CursorInfo { size: std::mem::size_of::<CursorInfo>() as u32, flags: 0, cursor: 0, pos: [0; 2] };
    // SAFETY: a plain query into our own struct, sized as Windows wants.
    let ok = unsafe { GetCursorInfo(&mut info) } != 0;
    ok && info.flags & CURSOR_SHOWING != 0 && info.cursor != 0
}

/// Some key (or mouse button) is held, the lock keys left out (raw input or the keyboard
/// state: see `rawkeys`).
pub fn any_key_held() -> bool {
    const LOCKS: [u8; 3] = [0x14, 0x90, 0x91];
    (1..=254u8).filter(|vk| !LOCKS.contains(vk)).any(crate::hotkeys::down)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_menu_opens_and_closes() {
        let mut m = MenuDetector::default();
        assert!(!m.update(0, false, false, None));
        // The cursor shows: a menu once it's stayed 250 ms.
        assert!(!m.update(100, true, false, None));
        assert!(m.update(400, true, false, None));
        // It hides: still open for a moment, then closed.
        assert!(m.update(500, false, false, None));
        assert!(!m.update(800, false, false, None));
    }

    #[test]
    fn holding_alt_is_a_peek() {
        let mut m = MenuDetector::default();
        for t in (0..2000).step_by(50) {
            assert!(!m.update(t, true, true, None));
        }
        // A key stuck for 20 s no longer counts.
        assert!(!m.update(20_000, true, true, None));
        assert!(m.update(21_000, true, true, None));
    }

    #[test]
    fn alt_toggling_the_cursor_is_no_menu() {
        let mut m = MenuDetector::default();
        // Alt tapped at 1000: the cursor comes up and stays, no key held.
        assert!(!m.update(1000, false, false, Some(1000)));
        for t in (1050..10_000).step_by(50) {
            assert!(!m.update(t, true, false, Some(1000)));
        }
        // Alt again: the cursor goes. Then a menu opens (no Alt): a menu.
        assert!(!m.update(10_050, false, false, Some(10_000)));
        assert!(!m.update(20_000, true, false, Some(10_000)));
        assert!(m.update(20_300, true, false, Some(10_000)));
    }
}
