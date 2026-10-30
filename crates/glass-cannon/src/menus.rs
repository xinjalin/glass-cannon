//! Game menus (inventory, character, skills…), from the mouse cursor: they send the server
//! nothing (checked on a recording that opened each in turn), but with the game's action
//! controls the cursor is hidden while you play and shows when a menu opens. Alt (the
//! game's cursor key) shows it too, and the two look the same from outside the game: the
//! same cursor, at the screen's middle, the mouse let go the same way (checked by watching
//! the cursor while pressing Alt and opening menus), and the game keeps its keys from
//! other apps. What differs is how long: Alt shows it for a moment (0.2-1.3 s there), a
//! menu for as long as it's open. So the cursor has to stay a while to be a menu. With the
//! classic controls the cursor always shows, so this does nothing useful there: the
//! setting is off by default and says so.
//!
//! The idea is from SkeeveAN's Aion DPS Meter notes (`docs/plans/menu-detect-cursor.md`);
//! this is our own code.

/// The cursor showing this long is a menu (an Alt press is shorter).
const MENU_AFTER_MS: u64 = 2000;
/// A menu stays this long after the cursor hides (no flicker between two menus).
const MENU_KEPT_MS: u64 = 300;

/// Decides from the cursor, a few times a second, whether a menu is open.
#[derive(Debug, Default)]
pub struct MenuDetector {
    /// Since when the cursor has shown.
    shown_since: Option<u64>,
    /// The menu is open; and when the cursor last showed while it was.
    open: bool,
    last_seen: u64,
}

impl MenuDetector {
    /// `now_ms`: any clock; `cursor`: the cursor shows.
    pub fn update(&mut self, now_ms: u64, cursor: bool) -> bool {
        if !cursor {
            self.shown_since = None;
            if self.open && now_ms.saturating_sub(self.last_seen) > MENU_KEPT_MS {
                self.open = false;
            }
            return self.open;
        }
        let since = *self.shown_since.get_or_insert(now_ms);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_menu_opens_and_closes() {
        let mut m = MenuDetector::default();
        assert!(!m.update(0, false));
        // The cursor shows: a menu once it's stayed 2 s.
        assert!(!m.update(100, true));
        assert!(!m.update(1500, true));
        assert!(m.update(2200, true));
        // It hides: still open for a moment, then closed.
        assert!(m.update(2300, false));
        assert!(!m.update(2600, false));
    }

    #[test]
    fn alt_presses_are_too_short() {
        // As watched in game: Alt shows the cursor for 0.2 to 1.3 s.
        let mut m = MenuDetector::default();
        for (from, to) in [(0, 1300), (3000, 3650), (5000, 5200)] {
            for t in (from..=to).step_by(50) {
                assert!(!m.update(t, true));
            }
            assert!(!m.update(to + 50, false));
        }
    }
}
