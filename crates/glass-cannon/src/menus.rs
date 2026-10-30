//! Game menus (inventory, character, the main menu…), with the game's action controls,
//! where the cursor is hidden while you play and shows when a menu opens. Alt (the game's
//! cursor key) shows it too, and from outside the game the two cursors are the same (the
//! same cursor, at the screen's middle, the mouse let go the same way), and the game keeps
//! its keys from other apps. What tells them apart: every full-screen menu has a close X
//! at the top right, on the screen's corner diagonal (as far in from the right as down
//! from the top: 2.8 to 4.4 % of the screen's height, by UI scale, the inventory's a
//! little in from the others'). Alt shows no X.
//!
//! So while the cursor shows (and only then), a small square at the top-right corner of
//! the game's picture (7 % of its height a side) is read and looked over for that X: thin,
//! bright strokes along both diagonals with darker gaps between them. The square's pixels
//! are kept only for that look: nothing is saved or sent (the app never goes online), and
//! nothing is read while the cursor is hidden, the setting off or the game not in front.
//! With the classic controls the cursor always shows, so leave the setting off there.
//!
//! Measured on screenshots at two UI scales, of the inventory and the main menu: the X
//! scores 163-201; 880 squares elsewhere on them (the world, text, icons) at most 129.
//!
//! The cursor idea is from SkeeveAN's Aion DPS Meter notes (`docs/plans/menu-detect-cursor.md`);
//! this is our own code.

/// The corner square's side, as a share of the game's height.
const CORNER: f64 = 0.07;
/// Where the X's middle can be, in from the right and down from the top (the same), as a
/// share of the game's height; and its arms' half-length as a share of that.
const X_FROM: f64 = 0.022;
const X_TO: f64 = 0.05;
const ARM_FROM: f64 = 0.28;
const ARM_TO: f64 = 0.45;
/// An X: the strokes this much brighter than the brightest gap, and this bright (of 255).
pub const X_SCORE: f32 = 150.0;
const X_BRIGHT: f32 = 160.0;
/// The corner is first read this long after the cursor comes up (a menu opening fades in),
/// then this often while it shows (a menu opened or closed with the cursor up).
const READ_AFTER_MS: u64 = 250;
const READ_EVERY_MS: u64 = 500;
/// A menu stays this long after the cursor hides (no flicker between two menus).
const MENU_KEPT_MS: u64 = 300;

/// The corner square's brightness, row by row.
pub struct Corner {
    side: usize,
    luma: Vec<u8>,
}

impl Corner {
    fn at(&self, x: f64, y: f64) -> f32 {
        let (x, y) = (x.round(), y.round());
        if x < 0.0 || y < 0.0 || x >= self.side as f64 || y >= self.side as f64 {
            return 0.0;
        }
        f32::from(self.luma[y as usize * self.side + x as usize])
    }

    /// How X-like the best spot is: (strokes minus the brightest gap, the strokes).
    /// `height`: the game picture's height, which the X's place and size go by.
    pub fn x_score(&self, height: f64) -> (f32, f32) {
        let mut best = (f32::MIN, 0.0);
        let side = self.side as f64;
        let (k_from, k_to) = ((X_FROM * height).floor() as i32, (X_TO * height).ceil() as i32);
        for k in k_from..=k_to {
            for dy in -2..=2 {
                for dx in -2..=2 {
                    let (cx, cy) = (side - f64::from(k) + f64::from(dx), f64::from(k + dy));
                    let (s_from, s_to) = (((ARM_FROM * f64::from(k)).round() as i32).max(5), (ARM_TO * f64::from(k)).round() as i32);
                    for s in s_from..=s_to {
                        let (mut on, mut n) = (0.0, 0.0);
                        for t in -s..=s {
                            if t.abs() < 2 {
                                continue;
                            }
                            let t = f64::from(t);
                            on += self.at(cx + t, cy + t) + self.at(cx + t, cy - t);
                            n += 2.0;
                        }
                        let on = on / n;
                        let q = f64::from(s) * 0.75;
                        let gap = [(q, 0.0), (-q, 0.0), (0.0, q), (0.0, -q)].iter().map(|(gx, gy)| self.at(cx + gx, cy + gy)).fold(0.0, f32::max);
                        if on - gap > best.0 {
                            best = (on - gap, on);
                        }
                    }
                }
            }
        }
        best
    }

    /// There's a menu's close X in the corner.
    pub fn has_close_x(&self, height: f64) -> bool {
        let (score, on) = self.x_score(height);
        score >= X_SCORE && on >= X_BRIGHT
    }
}

/// Decides, a few times a second, whether a menu is open.
#[derive(Debug, Default)]
pub struct MenuDetector {
    /// Since when the cursor has shown, and when the corner was last read.
    shown_since: Option<u64>,
    read_at: Option<u64>,
    open: bool,
    last_seen: u64,
}

impl MenuDetector {
    /// `now_ms`: any clock; `cursor`: the cursor shows; `look`: reads the corner and says
    /// whether there's an X (only called while the cursor shows, every so often). Returns
    /// whether a menu is open.
    pub fn update(&mut self, now_ms: u64, cursor: bool, look: &mut dyn FnMut() -> bool) -> bool {
        if !cursor {
            self.shown_since = None;
            self.read_at = None;
            if self.open && now_ms.saturating_sub(self.last_seen) > MENU_KEPT_MS {
                self.open = false;
            }
            return self.open;
        }
        let since = *self.shown_since.get_or_insert(now_ms);
        let due = match self.read_at {
            None => now_ms.saturating_sub(since) >= READ_AFTER_MS,
            Some(t) => now_ms.saturating_sub(t) >= READ_EVERY_MS,
        };
        if due {
            self.read_at = Some(now_ms);
            self.open = look();
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

/// Reads the top-right corner square of a screen area (left, top, width, height): its
/// brightness, nothing else kept.
pub fn read_corner(area: (i32, i32, i32, i32)) -> Option<Corner> {
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
        fn BitBlt(d: isize, x: i32, y: i32, w: i32, h: i32, s: isize, sx: i32, sy: i32, rop: u32) -> i32;
        fn GdiFlush() -> i32;
    }
    const SRCCOPY: u32 = 0x00CC_0020;
    let (left, top, width, height) = area;
    let side = (f64::from(height) * CORNER).round() as i32;
    if side < 16 || width < side {
        return None;
    }
    let mut out = None;
    // SAFETY: GDI calls on a screen DC and our own small bitmap, each released below; the
    // bits are read only while the bitmap is alive and after GDI has finished writing.
    unsafe {
        let screen = GetDC(0);
        if screen == 0 {
            return None;
        }
        let mem = CreateCompatibleDC(screen);
        let info = BitmapInfoHeader {
            size: std::mem::size_of::<BitmapInfoHeader>() as u32,
            width: side,
            height: -side, // top-down
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
            if BitBlt(mem, 0, 0, side, side, screen, left + width - side, top, SRCCOPY) != 0 {
                GdiFlush();
                let n = (side * side) as usize;
                let px = std::slice::from_raw_parts(bits, n * 4);
                let luma = px
                    .chunks_exact(4)
                    .map(|p| ((u32::from(p[2]) * 299 + u32::from(p[1]) * 587 + u32::from(p[0]) * 114) / 1000) as u8)
                    .collect();
                out = Some(Corner { side: side as usize, luma });
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

    /// A corner square for a 1440-high game with a dark, speckled world, and maybe an X
    /// (its middle `k` in from the corner, arms `s` long, strokes 2 px thick).
    fn corner(x: Option<(f64, f64)>) -> Corner {
        let side = (1440.0 * CORNER).round() as usize;
        let mut luma: Vec<u8> = (0..side * side).map(|i| (20 + (i * 37 % 41)) as u8).collect();
        if let Some((k, s)) = x {
            let (cx, cy) = (side as f64 - k, k);
            let mut t = -s;
            while t <= s {
                for (dx, dy) in [(t, t), (t, -t)] {
                    for w in [0.0, 1.0] {
                        let (x, y) = ((cx + dx + w).round() as usize, (cy + dy).round() as usize);
                        if x < side && y < side {
                            luma[y * side + x] = 230;
                        }
                    }
                }
                t += 0.5;
            }
        }
        Corner { side, luma }
    }

    #[test]
    fn a_close_x_at_each_ui_scale() {
        // The X's middle 2.8 to 4.4 % of 1440 in, its arms about 0.4 of that.
        for k in [40.0, 48.0, 56.0, 63.0] {
            assert!(corner(Some((k, (k * 0.4_f64).round()))).has_close_x(1440.0), "k {k}");
        }
        // No X: the world.
        assert!(!corner(None).has_close_x(1440.0));
    }

    #[test]
    fn a_bright_patch_is_no_x() {
        let mut c = corner(None);
        c.luma.iter_mut().for_each(|v| *v = 240);
        assert!(!c.has_close_x(1440.0));
    }

    #[test]
    fn the_x_decides_while_the_cursor_shows() {
        let mut m = MenuDetector::default();
        // Alt: the cursor, no X.
        assert!(!m.update(0, true, &mut || false));
        assert!(!m.update(300, true, &mut || false));
        // A menu opened with the cursor up: the X shows at the next look.
        assert!(!m.update(600, true, &mut || true));
        assert!(m.update(850, true, &mut || true));
        // The cursor hides: kept a moment, then gone.
        assert!(m.update(900, false, &mut || unreachable!()));
        assert!(!m.update(1300, false, &mut || unreachable!()));
    }
}

