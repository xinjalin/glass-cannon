//! Game menus (inventory, character, skills…), with the game's action controls, where
//! the cursor is hidden while you play and shows when a menu opens. Alt (the game's cursor
//! key) shows it too, and from outside the game the two cursors are the same (the same
//! cursor, at the screen's middle, the mouse let go the same way), and the game keeps its
//! keys from other apps. What differs is the picture: a full-screen menu covers or dims
//! most of it, Alt only puts a cursor over the same world.
//!
//! So a few points of the game's picture are read: 24 single pixels (6 × 4, the meter's
//! own widgets left out), only while the game is in front and the setting is on: every
//! half second while the cursor is hidden, and once as it comes up. A menu is most of
//! those points changing as the cursor comes up. Each reading is 24 brightness numbers
//! kept until the next one: no picture is made, saved or sent (the app never goes online).
//! With the classic controls the cursor always shows, so this does nothing there: the
//! setting is off by default and says so.
//!
//! The cursor idea is from SkeeveAN's Aion DPS Meter notes (`docs/plans/menu-detect-cursor.md`);
//! this is our own code.

/// Points across and down.
pub const COLS: usize = 6;
pub const ROWS: usize = 4;
/// How often the points are read while the cursor is hidden (to compare with).
const READ_EVERY_MS: u64 = 500;
/// The reading to compare with is at most this old when the cursor comes up.
const COMPARE_WITHIN_MS: u64 = 900;
/// The cursor's reading is taken this long after it comes up (a menu opening fades in).
const READ_AFTER_MS: u64 = 300;
/// A point changed: its brightness moved by more than this (of 255).
const CHANGED_BY: u8 = 28;
/// A menu: at least this share of the points changed.
pub const MENU_SHARE: f32 = 0.6;
/// Too few points left (the widgets covering the rest): no telling.
const FEWEST_POINTS: usize = 8;
/// A menu stays this long after the cursor hides (no flicker between two menus).
const MENU_KEPT_MS: u64 = 300;

/// The points' brightness (None: left out, under a widget).
pub type Reading = [Option<u8>; COLS * ROWS];

/// The share of points (read both times) whose brightness changed.
pub fn changed_share(before: &Reading, after: &Reading) -> Option<f32> {
    let pairs: Vec<(u8, u8)> = before.iter().zip(after).filter_map(|(a, b)| Some(((*a)?, (*b)?))).collect();
    if pairs.len() < FEWEST_POINTS {
        return None;
    }
    let changed = pairs.iter().filter(|(a, b)| a.abs_diff(*b) > CHANGED_BY).count();
    Some(changed as f32 / pairs.len() as f32)
}

/// Decides, a few times a second, whether a menu is open.
#[derive(Debug, Default)]
pub struct MenuDetector {
    /// The last reading while the cursor was hidden, and when.
    before: Option<(u64, Reading)>,
    /// Since when the cursor has shown; and what its reading said (once taken).
    shown_since: Option<u64>,
    menu: Option<bool>,
    open: bool,
    last_seen: u64,
}

impl MenuDetector {
    /// `now_ms`: any clock; `cursor`: the cursor shows; `read`: reads the points now (only
    /// called when needed). Returns whether a menu is open, and the share changed when
    /// this call decided.
    pub fn update(&mut self, now_ms: u64, cursor: bool, read: &mut dyn FnMut() -> Reading) -> (bool, Option<f32>) {
        if !cursor {
            self.shown_since = None;
            self.menu = None;
            if self.before.is_none_or(|(t, _)| now_ms.saturating_sub(t) >= READ_EVERY_MS) {
                self.before = Some((now_ms, read()));
            }
            if self.open && now_ms.saturating_sub(self.last_seen) > MENU_KEPT_MS {
                self.open = false;
            }
            return (self.open, None);
        }
        let since = *self.shown_since.get_or_insert(now_ms);
        let mut decided = None;
        if self.menu.is_none() && now_ms.saturating_sub(since) >= READ_AFTER_MS {
            let after = read();
            let share = self.before.filter(|(t, _)| since.saturating_sub(*t) <= COMPARE_WITHIN_MS).and_then(|(_, b)| changed_share(&b, &after));
            // No telling (nothing fresh to compare with, or too few points): not a menu,
            // so the cursor alone never hides the widgets.
            self.menu = Some(share.is_some_and(|s| s >= MENU_SHARE));
            decided = share;
        }
        match self.menu {
            Some(true) => {
                self.open = true;
                self.last_seen = now_ms;
            }
            Some(false) => self.open = false,
            // Not decided yet: as it was (a menu just closed stays a moment).
            None => {}
        }
        (self.open, decided)
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

/// Reads the brightness of 6 × 4 points of a screen area (left, top, width, height),
/// spread evenly across it, in one go (Windows shrinks the area straight to those points);
/// `skip(col, row)` leaves a point out. Nothing else of the screen is kept.
pub fn read_points(area: (i32, i32, i32, i32), skip: &dyn Fn(usize, usize) -> bool) -> Reading {
    #[repr(C)]
    struct BitmapInfoHeader {
        size: u32,
        width: i32,
        height: i32,
        planes: u16,
        bit_count: u16,
        compression: u32,
        size_image: u32,
        x_ppm: i32,
        y_ppm: i32,
        clr_used: u32,
        clr_important: u32,
    }
    #[link(name = "user32")]
    extern "system" {
        fn GetDC(hwnd: isize) -> isize;
        fn ReleaseDC(hwnd: isize, dc: isize) -> i32;
    }
    #[link(name = "gdi32")]
    extern "system" {
        fn CreateCompatibleDC(dc: isize) -> isize;
        fn CreateDIBSection(dc: isize, info: *const BitmapInfoHeader, usage: u32, bits: *mut *mut u8, section: isize, offset: u32) -> isize;
        fn SelectObject(dc: isize, obj: isize) -> isize;
        fn DeleteObject(obj: isize) -> i32;
        fn DeleteDC(dc: isize) -> i32;
        fn SetStretchBltMode(dc: isize, mode: i32) -> i32;
        fn StretchBlt(d: isize, x: i32, y: i32, w: i32, h: i32, s: isize, sx: i32, sy: i32, sw: i32, sh: i32, rop: u32) -> i32;
        fn GdiFlush() -> i32;
    }
    const COLORONCOLOR: i32 = 3;
    const SRCCOPY: u32 = 0x00CC_0020;
    let mut out: Reading = [None; COLS * ROWS];
    let (left, top, width, height) = area;
    if width < COLS as i32 || height < ROWS as i32 {
        return out;
    }
    // SAFETY: GDI calls on a screen DC and our own small bitmap, each released below; the
    // bits are read only while the bitmap is alive and after GDI has finished writing.
    unsafe {
        let screen = GetDC(0);
        if screen == 0 {
            return out;
        }
        let mem = CreateCompatibleDC(screen);
        let info = BitmapInfoHeader {
            size: std::mem::size_of::<BitmapInfoHeader>() as u32,
            width: COLS as i32,
            height: -(ROWS as i32), // top-down
            planes: 1,
            bit_count: 32,
            compression: 0,
            size_image: 0,
            x_ppm: 0,
            y_ppm: 0,
            clr_used: 0,
            clr_important: 0,
        };
        let mut bits: *mut u8 = std::ptr::null_mut();
        let bmp = if mem != 0 { CreateDIBSection(mem, &info, 0, &mut bits, 0, 0) } else { 0 };
        if bmp != 0 && !bits.is_null() {
            let old = SelectObject(mem, bmp);
            SetStretchBltMode(mem, COLORONCOLOR);
            if StretchBlt(mem, 0, 0, COLS as i32, ROWS as i32, screen, left, top, width, height, SRCCOPY) != 0 {
                GdiFlush();
                let px = std::slice::from_raw_parts(bits, COLS * ROWS * 4);
                for row in 0..ROWS {
                    for col in 0..COLS {
                        if skip(col, row) {
                            continue;
                        }
                        let i = (row * COLS + col) * 4;
                        let (b, g, r) = (u32::from(px[i]), u32::from(px[i + 1]), u32::from(px[i + 2]));
                        out[row * COLS + col] = Some(((r * 299 + g * 587 + b * 114) / 1000) as u8);
                    }
                }
            }
            SelectObject(mem, old);
        }
        if bmp != 0 {
            DeleteObject(bmp);
        }
        if mem != 0 {
            DeleteDC(mem);
        }
        ReleaseDC(0, screen);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat(v: u8) -> Reading {
        [Some(v); COLS * ROWS]
    }

    #[test]
    fn a_menu_is_most_points_changing() {
        let mut m = MenuDetector::default();
        let world = flat(120);
        let mut menu_pic = flat(30);
        menu_pic[0] = Some(120);
        // The cursor hidden: the world is read to compare with.
        assert_eq!(m.update(0, false, &mut || world), (false, None));
        // A menu: the cursor comes up and the picture goes dark.
        assert!(!m.update(100, true, &mut || menu_pic).0);
        let (open, share) = m.update(400, true, &mut || menu_pic);
        assert!(open && share.unwrap() > 0.9);
        // Closed: gone a moment after the cursor.
        assert!(m.update(500, false, &mut || world).0);
        assert!(!m.update(900, false, &mut || world).0);
    }

    #[test]
    fn alt_keeps_the_picture() {
        let mut m = MenuDetector::default();
        let world = flat(120);
        let mut with_cursor = world;
        with_cursor[3] = Some(250);
        m.update(0, false, &mut || world);
        for t in (100..3000).step_by(50) {
            assert!(!m.update(t, true, &mut || with_cursor).0);
        }
    }

    #[test]
    fn no_telling_is_no_menu() {
        // Nothing to compare with, or the widgets covering nearly everything.
        let mut m = MenuDetector::default();
        assert!(!m.update(400, true, &mut || flat(10)).0);
        let mut few: Reading = [None; COLS * ROWS];
        few[0] = Some(1);
        assert_eq!(changed_share(&flat(200), &few), None);
    }
}
