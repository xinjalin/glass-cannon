//! Game menus (inventory, character, the main menu…), with the game's action controls,
//! where the cursor is hidden while you play and shows when a menu opens. Alt (the game's
//! cursor key) shows it too, and from outside the game the two cursors are the same (the
//! same cursor, at the screen's middle, the mouse let go the same way), and the game keeps
//! its keys from other apps. What tells them apart: every full-screen menu has a close X
//! at the top right, on the screen's corner diagonal (as far in from the right as down
//! from the top: 2.8 to 4.4 % of the screen's height, by UI scale, the inventory's a
//! little in from the others'); a few windows (Season Mission) have a back arrow there
//! instead, pointing left. Alt shows neither.
//!
//! So while the cursor shows (and only then), a small square at the top-right corner of
//! the game's picture (8.5 % of its height a side) is read and looked over for the X (thin,
//! bright strokes along both diagonals with darker gaps between them) or the arrow (a solid
//! head and tail, dark all round). The square's pixels are kept only for that look: nothing
//! is saved or sent (the app never goes online), and nothing is read while the cursor is
//! hidden, the setting off or the game not in front. With the classic controls the cursor
//! always shows: the corner is then looked at every half second, and the mark decides.
//!
//! Measured on screenshots at two UI scales, of the inventory and the main menu: the X
//! scores 163-201; 880 squares elsewhere on them (the world, text, icons) at most 129. The
//! Season Mission's arrow 149; 1,000 squares elsewhere at most 68.
//!
//! Reward screens (a Monster Cube's, Shugo's and others' after content) have neither: their
//! top right has your currencies. They have a gold button at the bottom middle instead:
//! **Confirm** once the cards are turned (on a 2000×1125 screenshot: 224×44 px, its middle
//! 91 % of the way down, so about 20 % of the screen's height wide and 4 % tall), and
//! before that **Complete Selection**, the same size, just right of the middle beside a dark
//! **Cancel Selection**. So a strip at the bottom middle (70 % of the height wide, the
//! lowest 22 % of it) is read too and looked over for a solid gold bar of about that size
//! anywhere in it, with no gold just around it. Kept only for that look, as the corner.
//!
//! A pop-up over a menu dims the screen, its mark too, past telling, so once seen the menu
//! stays open until the cursor hides (closing the menu hides it).
//!
//! The cursor idea is from SkeeveAN's Aion DPS Meter notes (`docs/plans/menu-detect-cursor.md`);
//! this is our own code.

/// The corner square's side, as a share of the game's height.
const CORNER: f64 = 0.085;
/// The strip at the bottom middle looked over for a reward screen's gold button: its
/// width and height, as shares of the game's height (from the bottom up).
const STRIP_W: f64 = 0.7;
const STRIP_H: f64 = 0.22;
/// The button's size, as shares of the game's height: its height and width may be
/// between these (UI scales), and the share of each of its rows that must be gold (its
/// white text and icons take some: 0.67 at the least on the screenshot).
const BUTTON_H: (f64, f64) = (0.022, 0.07);
const BUTTON_W: (f64, f64) = (0.1, 0.32);
const BUTTON_GOLD: f64 = 0.5;
/// Where the X's middle can be, in from the right and down from the top (the same), as a
/// share of the game's height; and its arms' half-length as a share of that.
const X_FROM: f64 = 0.022;
const X_TO: f64 = 0.05;
const ARM_FROM: f64 = 0.28;
const ARM_TO: f64 = 0.45;
/// An X: the strokes this much brighter than the brightest gap, and this bright (of 255).
pub const X_SCORE: f32 = 150.0;
const X_BRIGHT: f32 = 160.0;
/// Where the back arrow's tip can be (in from the right, down from the top) and its head's
/// half-height, as shares of the game's height.
const TIP_IN: (f64, f64) = (0.035, 0.075);
const TIP_DOWN: (f64, f64) = (0.02, 0.055);
const HEAD: (f64, f64) = (0.008, 0.017);
/// Points of the arrow, from its tip in head half-heights (x right, y down): inside its
/// head and tail, and just outside it all round.
const ARROW_ON: [(f64, f64); 8] = [(0.4, -0.1), (0.9, -0.3), (1.2, -0.75), (0.9, 0.3), (1.2, 0.75), (1.3, 0.0), (2.3, -0.2), (2.7, -0.15)];
const ARROW_OFF: [(f64, f64); 13] = [
    (0.3, -0.8),
    (0.3, 0.8),
    (-0.6, 0.0),
    (-0.4, -0.6),
    (-0.4, 0.6),
    (2.6, -0.9),
    (2.6, 0.9),
    (1.9, -0.9),
    (3.4, 0.0),
    (1.6, -1.25),
    (1.6, 1.25),
    (0.8, -1.1),
    (0.8, 1.1),
];
/// An arrow: its dimmest point this much brighter than the brightest point around it.
pub const ARROW_SCORE: f32 = 110.0;

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
    /// The square's side, in pixels.
    pub fn side(&self) -> usize {
        self.side
    }

    /// Blanks boxes of the square, (left, top, right, bottom) in its pixels: what's there
    /// isn't the game's (our own widgets over it). Blank is black, which neither the X's
    /// strokes nor the arrow's head can be.
    pub fn blank(&mut self, boxes: &[(f64, f64, f64, f64)]) {
        let side = self.side as f64;
        for &(l, t, r, b) in boxes {
            let (l, t) = (l.max(0.0).floor() as usize, t.max(0.0).floor() as usize);
            let (r, b) = (r.min(side).ceil() as usize, b.min(side).ceil() as usize);
            for y in t..b {
                self.luma[y * self.side + l.min(r)..y * self.side + r].fill(0);
            }
        }
    }

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

    /// How arrow-like the best spot is: its dimmest point minus the brightest around it.
    pub fn arrow_score(&self, height: f64) -> f32 {
        let side = self.side as f64;
        let mut best = f32::MIN;
        let (a_from, a_to) = (((HEAD.0 * height).floor() as i32).max(5), (HEAD.1 * height).ceil() as i32);
        let (y_from, y_to) = ((TIP_DOWN.0 * height).floor() as i32, (TIP_DOWN.1 * height).ceil() as i32);
        let (x_from, x_to) = ((side - TIP_IN.1 * height).floor() as i32, (side - TIP_IN.0 * height).ceil() as i32);
        for a in a_from..=a_to {
            let a = f64::from(a);
            for ty in y_from..=y_to {
                for tx in x_from..=x_to {
                    let (tx, ty) = (f64::from(tx), f64::from(ty));
                    let on = ARROW_ON.iter().map(|(u, v)| self.at(tx + u * a, ty + v * a)).fold(f32::MAX, f32::min);
                    if on - best <= 0.0 {
                        continue;
                    }
                    let off = ARROW_OFF.iter().map(|(u, v)| self.at(tx + u * a, ty + v * a)).fold(0.0, f32::max);
                    best = best.max(on - off);
                }
            }
        }
        best
    }

    /// A menu's mark in the corner: its close X, or a window's back arrow.
    pub fn has_menu_mark(&self, height: f64) -> bool {
        self.has_close_x(height) || self.arrow_score(height) >= ARROW_SCORE
    }
}

/// A strip of the screen, in colour, row by row.
pub struct Strip {
    w: usize,
    h: usize,
    rgb: Vec<[u8; 3]>,
}

/// The Confirm button's gold (and its lighter bottom edge), not the game's world or its
/// white text.
fn gold([r, g, b]: [u8; 3]) -> bool {
    let (r, g, b) = (i32::from(r), i32::from(g), i32::from(b));
    r >= 150 && g >= 100 && r - b >= 80 && g <= r + 10
}

impl Strip {
    #[cfg(test)]
    pub fn width(&self) -> usize {
        self.w
    }

    /// Blanks boxes of the strip, (left, top, right, bottom) in its pixels: our own
    /// widgets over the game.
    pub fn blank(&mut self, boxes: &[(f64, f64, f64, f64)]) {
        let (w, h) = (self.w as f64, self.h as f64);
        for &(l, t, r, b) in boxes {
            let (l, t) = (l.max(0.0).floor() as usize, t.max(0.0).floor() as usize);
            let (r, b) = (r.min(w).ceil() as usize, b.min(h).ceil() as usize);
            for y in t..b {
                self.rgb[y * self.w + l.min(r)..y * self.w + r].fill([0, 0, 0]);
            }
        }
    }

    fn gold_share(&self, y: usize, from: usize, to: usize) -> f64 {
        let row = &self.rgb[y * self.w..(y + 1) * self.w];
        let n = row[from..to].iter().filter(|p| gold(**p)).count();
        n as f64 / (to - from).max(1) as f64
    }

    /// A reward screen's gold button in the strip: rows of gold as wide as the button
    /// (gaps of a few pixels are its text), starting and ending at the same places, as many
    /// as it is tall, not at the strip's sides, and no gold just above, below or beside
    /// them. `height`: the game picture's height, which the button's size goes by.
    pub fn has_confirm_button(&self, height: f64) -> bool {
        let (wide_lo, wide_hi) = (BUTTON_W.0 * height, BUTTON_W.1 * height);
        let (tall_lo, tall_hi) = ((BUTTON_H.0 * height) as usize, (BUTTON_H.1 * height).ceil() as usize);
        let gap = ((0.006 * height) as usize).max(2);
        let margin = ((0.01 * height) as usize).max(2);
        let slack = ((0.01 * height) as usize).max(2);
        let mut y = margin;
        while y + margin < self.h {
            // A row whose longest gold run could be the button's top.
            let Some((x0, x1)) = self.longest_run(y, gap) else {
                y += 1;
                continue;
            };
            let wide = (x1 - x0) as f64;
            if wide < wide_lo || wide > wide_hi || x0 == 0 || x1 >= self.w {
                y += 1;
                continue;
            }
            // Down while the rows are gold across it.
            let mut end = y + 1;
            while end < self.h && self.gold_share(end, x0, x1) >= BUTTON_GOLD {
                end += 1;
            }
            let tall = end - y;
            // Its full width, from all its rows (the top row's ends can be faded).
            let (mut x0, mut x1) = (x0, x1);
            for yy in y..end {
                if let Some((a, b)) = self.longest_run(yy, gap).filter(|&(a, b)| a < x1 && b > x0) {
                    (x0, x1) = (x0.min(a), x1.max(b));
                }
            }
            let wide = (x1 - x0) as f64;
            let around_clear = wide <= wide_hi
                && end + margin <= self.h
                && self.gold_share(y - margin, x0, x1) < 0.2
                && self.gold_share(end + margin - 1, x0, x1) < 0.2
                && x0 >= slack
                && x1 + slack <= self.w
                && (y..end).all(|yy| !gold(self.rgb[yy * self.w + x0 - slack]) && !gold(self.rgb[yy * self.w + x1 + slack - 1]));
            if (tall_lo..=tall_hi).contains(&tall) && around_clear {
                return true;
            }
            y = end.max(y + 1);
        }
        false
    }

    /// The longest run of gold in a row, gaps of up to `gap` pixels bridged: [start, end).
    fn longest_run(&self, y: usize, gap: usize) -> Option<(usize, usize)> {
        let row = &self.rgb[y * self.w..(y + 1) * self.w];
        let mut best: Option<(usize, usize)> = None;
        let mut run: Option<(usize, usize)> = None; // start, last gold
        for (x, p) in row.iter().enumerate() {
            if gold(*p) {
                run = Some(match run {
                    Some((s, last)) if x - last <= gap + 1 => (s, x),
                    _ => (x, x),
                });
                let (s, last) = run.unwrap();
                if best.is_none_or(|(bs, be)| last + 1 - s > be - bs) {
                    best = Some((s, last + 1));
                }
            }
        }
        best
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
        // Once a menu's mark is seen, the menu stays open until the cursor hides: a pop-up
        // over it dims the whole screen, the mark too, past telling (on a screenshot, the
        // X's strokes at 36 of 255), and closing the menu hides the cursor. Nothing more
        // is read meanwhile.
        if due && !self.open {
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
    let (left, top, width, height) = area;
    let side = (f64::from(height) * CORNER).round() as i32;
    if side < 16 || width < side {
        return None;
    }
    let px = read_screen(left + width - side, top, side, side)?;
    let luma = px.iter().map(|[r, g, b]| ((u32::from(*r) * 299 + u32::from(*g) * 587 + u32::from(*b) * 114) / 1000) as u8).collect();
    Some(Corner { side: side as usize, luma })
}

/// Reads the strip at the bottom middle of a screen area (left, top, width, height), where
/// a reward screen's Confirm button is.
pub fn read_bottom(area: (i32, i32, i32, i32)) -> Option<Strip> {
    let (left, top, width, height) = area;
    let (w, h) = (((f64::from(height) * STRIP_W).round() as i32).min(width), (f64::from(height) * STRIP_H).round() as i32);
    if w < 32 || h < 16 {
        return None;
    }
    let x = left + (width - w) / 2;
    let rgb = read_screen(x, top + height - h, w, h)?;
    Some(Strip { w: w as usize, h: h as usize, rgb })
}

/// Where [`read_bottom`]'s strip is within the game's picture: (left, top), for blanking our
/// widgets in it.
pub fn bottom_origin(width: i32, height: i32) -> (f64, f64) {
    let (w, h) = (((f64::from(height) * STRIP_W).round() as i32).min(width), (f64::from(height) * STRIP_H).round() as i32);
    (f64::from((width - w) / 2), f64::from(height - h))
}

/// A rectangle of the screen's pixels as RGB, row by row.
fn read_screen(x: i32, y: i32, w: i32, h: i32) -> Option<Vec<[u8; 3]>> {
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
            width: w,
            height: -h, // top-down
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
            if BitBlt(mem, 0, 0, w, h, screen, x, y, SRCCOPY) != 0 {
                GdiFlush();
                let n = (w * h) as usize;
                let px = std::slice::from_raw_parts(bits, n * 4);
                out = Some(px.chunks_exact(4).map(|p| [p[2], p[1], p[0]]).collect());
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
    fn a_back_arrow() {
        // As in the Season Mission at 1440 high: tip 64 px in, 47 down, head 16 px each way.
        let mut c = corner(None);
        let (tx, ty, a) = (c.side as f64 - 64.0, 47.0, 16.0);
        for y in 0..c.side {
            for x in 0..c.side {
                let (u, v) = ((x as f64 - tx) / a, (y as f64 - ty) / a);
                let head = (0.0..=1.3).contains(&u) && v.abs() <= u * 0.8;
                let tail = (1.3..=2.85).contains(&u) && (-0.35..=0.15).contains(&v);
                if head || tail {
                    c.luma[y * c.side + x] = 210;
                }
            }
        }
        assert!(c.arrow_score(1440.0) >= ARROW_SCORE, "{}", c.arrow_score(1440.0));
        assert!(c.has_menu_mark(1440.0));
        assert!(corner(None).arrow_score(1440.0) < ARROW_SCORE);
    }

    #[test]
    fn our_own_widget_in_the_corner_is_no_x() {
        // A widget's X-like glyph (its close button, say) in the corner: an X to the eye…
        let mut c = corner(Some((48.0, 19.0)));
        assert!(c.has_close_x(1440.0));
        // …but it's ours, so it's blanked and not taken for a menu.
        let side = c.side() as f64;
        c.blank(&[(side - 80.0, -8.0, side + 8.0, 80.0)]);
        assert!(!c.has_menu_mark(1440.0));
        // A box past the square changes nothing; one partly in only blanks its part.
        let mut c = corner(Some((48.0, 19.0)));
        c.blank(&[(side + 10.0, 0.0, side + 50.0, 40.0), (-30.0, -30.0, 5.0, 5.0)]);
        assert!(c.has_close_x(1440.0));
    }

    /// The bottom strip of a 1125-high screen: the dark world, maybe a reward screen's gold
    /// Confirm button (224×44, its middle 91 % down) with white text, maybe other gold.
    fn strip(button: bool, paint: impl Fn(usize, usize) -> Option<[u8; 3]>) -> Strip {
        let height = 1125.0;
        let (w, h) = ((height * STRIP_W).round() as usize, (height * STRIP_H).round() as usize);
        let top = 1125 - h;
        let mut rgb = vec![[0x30, 0x36, 0x3e]; w * h];
        for y in 0..h {
            for x in 0..w {
                let (sx, sy) = (x as i64 - w as i64 / 2 + 1000, (y + top) as i64);
                let on_button = button && (888..1112).contains(&sx) && (1003..1047).contains(&sy);
                if on_button {
                    // Gold, white letters in its middle rows, a lighter bottom edge.
                    rgb[y * w + x] = if (1015..1035).contains(&sy) && sx % 9 < 3 && (950..1060).contains(&sx) {
                        [0xff, 0xf7, 0xe4]
                    } else if sy >= 1044 {
                        [0xe7, 0xd5, 0x74]
                    } else {
                        [0xc9, 0x91, 0x10]
                    };
                }
                if let Some(c) = paint(sx as usize, sy as usize) {
                    rgb[y * w + x] = c;
                }
            }
        }
        Strip { w, h, rgb }
    }

    #[test]
    fn a_reward_screens_confirm_button() {
        assert!(strip(true, |_, _| None).has_confirm_button(1125.0));
        // No button: the world.
        assert!(!strip(false, |_, _| None).has_confirm_button(1125.0));
        // Gold across the whole strip (a sunset, a gold floor): not a button.
        let all_gold = |_: usize, y: usize| (y >= 1000).then_some([0xd0, 0x98, 0x20]);
        assert!(!strip(false, all_gold).has_confirm_button(1125.0));
        // A small gold icon in the middle: too narrow.
        let icon = |x: usize, y: usize| ((985..1015).contains(&x) && (1003..1047).contains(&y)).then_some([0xd0, 0x98, 0x20]);
        assert!(!strip(false, icon).has_confirm_button(1125.0));
        // A gold bar too tall to be the button.
        let tall = |x: usize, y: usize| ((900..1100).contains(&x) && (920..1100).contains(&y)).then_some([0xd0, 0x98, 0x20]);
        assert!(!strip(false, tall).has_confirm_button(1125.0));
        // A gold bar running off the strip's side (a boss's golden ground across the floor).
        let floor = |_: usize, y: usize| ((1010..1050).contains(&y)).then_some([0xd0, 0x98, 0x20]);
        assert!(!strip(false, floor).has_confirm_button(1125.0));
    }

    #[test]
    fn a_reward_screen_before_its_cards_are_turned() {
        // As a Monster Cube's "Not This Tile?" (measured on a 2000×1122 screenshot): a dark
        // Cancel Selection left of the middle (771-997), the gold Selection Complete right of
        // it (1003-1226, 1032-1082), with its key's F in a dark box, its text, and a key icon
        // and its count on the gold.
        let buttons = |x: usize, y: usize| {
            let row = (1032..1082).contains(&y);
            if row && (771..997).contains(&x) {
                Some([0x2a, 0x3a, 0x5c])
            } else if row && (1003..1226).contains(&x) {
                let key_box = (1014..1030).contains(&x) && (1048..1066).contains(&y);
                let text = (1036..1162).contains(&x) && (1048..1066).contains(&y) && x % 7 < 2;
                let icon = (1180..1200).contains(&x) && (1042..1072).contains(&y);
                Some(if key_box { [0x30, 0x2a, 0x20] } else if text || icon { [0xf0, 0xe8, 0xd8] } else { [0xe0, 0xa8, 0x20] })
            } else {
                None
            }
        };
        assert!(strip(false, buttons).has_confirm_button(1125.0));
    }

    #[test]
    fn our_own_widget_over_the_button_is_left_out() {
        let mut s = strip(true, |_, _| None);
        let w = s.width() as f64;
        s.blank(&[(0.0, 0.0, w, 400.0)]);
        assert!(!s.has_confirm_button(1125.0));
    }

    #[test]
    fn a_bright_patch_is_no_x() {
        let mut c = corner(None);
        c.luma.iter_mut().for_each(|v| *v = 240);
        assert!(!c.has_close_x(1440.0));
        assert!(!c.has_menu_mark(1440.0));
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
        // A pop-up dims the X: still open while the cursor shows (and nothing read).
        assert!(m.update(1400, true, &mut || unreachable!()));
        assert!(m.update(5000, true, &mut || unreachable!()));
        // The cursor hides: kept a moment, then gone.
        assert!(m.update(5050, false, &mut || unreachable!()));
        assert!(!m.update(5400, false, &mut || unreachable!()));
    }
}


