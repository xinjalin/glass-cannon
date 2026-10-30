//! Game menus (inventory, character, skills…), from the mouse cursor: they send the server
//! nothing (checked on a recording that opened each in turn), but with the game's action
//! controls the cursor is hidden while you play and shows when a menu opens. Holding a key
//! (Alt) shows it too, to click something without a menu: then it's a peek, not a menu.
//! With the classic controls the cursor always shows, so this does nothing useful there:
//! the setting is off by default and says so.
//!
//! The idea is from SkeeveAN's Aion DPS Meter notes (`docs/plans/menu-detect-cursor.md`);
//! this is our own code.

/// The cursor showing this long with no key held is a menu.
const MENU_AFTER_MS: u64 = 250;
/// A menu stays this long after the cursor hides (no flicker between two menus).
const MENU_KEPT_MS: u64 = 300;
/// A key held longer than this is stuck, not a peek.
const STUCK_KEY_MS: u64 = 20_000;

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
}

impl MenuDetector {
    /// `now_ms`: any clock; `cursor`: the cursor shows; `key`: some key is held.
    pub fn update(&mut self, now_ms: u64, cursor: bool, key: bool) -> bool {
        self.key_since = if key { Some(self.key_since.unwrap_or(now_ms)) } else { None };
        let peeking = self.key_since.is_some_and(|t| now_ms.saturating_sub(t) < STUCK_KEY_MS);
        if !cursor {
            self.bare_since = None;
            if self.open && now_ms.saturating_sub(self.last_seen) > MENU_KEPT_MS {
                self.open = false;
            }
            return self.open;
        }
        if peeking {
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

/// Some key (or mouse button) is held, the lock keys left out.
pub fn any_key_held() -> bool {
    #[link(name = "user32")]
    extern "system" {
        fn GetAsyncKeyState(vk: i32) -> i16;
    }
    const LOCKS: [i32; 3] = [0x14, 0x90, 0x91];
    // SAFETY: a plain query of the keyboard's state.
    (1..=254).filter(|vk| !LOCKS.contains(vk)).any(|vk| unsafe { GetAsyncKeyState(vk) } < 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_menu_opens_and_closes() {
        let mut m = MenuDetector::default();
        assert!(!m.update(0, false, false));
        // The cursor shows: a menu once it's stayed 250 ms.
        assert!(!m.update(100, true, false));
        assert!(m.update(400, true, false));
        // It hides: still open for a moment, then closed.
        assert!(m.update(500, false, false));
        assert!(!m.update(800, false, false));
    }

    #[test]
    fn holding_alt_is_a_peek() {
        let mut m = MenuDetector::default();
        for t in (0..2000).step_by(50) {
            assert!(!m.update(t, true, true));
        }
        // A key stuck for 20 s no longer counts.
        assert!(!m.update(20_000, true, true));
        assert!(m.update(21_000, true, true));
    }
}
