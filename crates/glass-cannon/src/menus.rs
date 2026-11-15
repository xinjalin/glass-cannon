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
//! Windows that aren't full screen (Attendance, …) have their close X at their own top
//! right, wherever the window is, on its title bar. So the top of the screen (its top 60 %,
//! from 30 % across to the right) is looked over for an X-like mark (a little fainter than
//! the corner's: 125, its strokes 150, each point the brightest a pixel up or down, as a
//! window's X can be thin and not quite square) that sits on a title bar: a long straight
//! edge above it and one below it, running 40 % of the screen's height to its left, one
//! along 90 % of that, the other (the line under a title bar can fade at its ends) 70 %.
//! On a screenshot of the Attendance window its X scores 145, with edges 30 px above and 29
//! below; Force Settings' 138, edges along 100 % and 84 %; 294 gameplay screenshots have
//! 57 X-like marks, none with an edge along even 60 %. Kept only for that look, as the
//! corner.
//!
//! A pop-up over a menu dims the screen, its mark too, past telling, so once seen the menu
//! stays open until the cursor hides (closing the menu hides it).
//!
//! Pop-ups over the game itself (the banners at login: a picture, and under it a bar with
//! "Don't show again today" and its close mark) dim everything else nearly to black, and
//! their bar is one flat grey. So a band across the middle of the screen (90 % of its
//! height wide, from 40 % to 95 % down) is read too and looked over for such a bar: flat
//! (within 3 of one grey, 25 to 90 of 255) for at least 45 % of the screen's height across,
//! 3.5 to 9 % of it tall (its text breaks the rows through it), with an edge along its top
//! and the dimmed screen under it. On a 2000×1126 screenshot of the "LOOKBOOK" banner: the
//! bar at 46-47, rows 793-862, 800 px wide, above it the picture at about 110, under it 5
//! (found with our meter over its right end, left out); 294 gameplay screenshots have no
//! such bar. Kept only for that look, as the corner.
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

/// The top of the screen looked over for a window's close X: from this share of its width
/// across to the right, and this share of its height down.
const WINDOW_LEFT: f64 = 0.3;
const WINDOW_DOWN: f64 = 0.6;
/// A window's X: its score and strokes (see [`Corner::x_score`]), its arms' half-length as
/// shares of the screen's height.
const WINDOW_X_SCORE: f32 = 125.0;
const WINDOW_X_BRIGHT: f32 = 150.0;
const WINDOW_ARM: (f64, f64) = (0.006, 0.02);
/// Its strokes may be thin and a little off the diagonal (a window's X is narrower or taller
/// than square): each point along them is the brightest within this many pixels up or down.
const WINDOW_X_SLACK: i32 = 1;
/// The most the window check may take: past it, no window is taken to be there (it's
/// looked for again in a moment). It runs where the overlay decides whether a widget takes
/// the mouse, so it mustn't hold that up.
const WINDOW_BUDGET: std::time::Duration = std::time::Duration::from_millis(40);
/// Its title bar: straight edges (a step of this much brightness two rows apart, along at
/// least this share of their length, the fainter one at least the second share: a line
/// under a title bar may fade at its ends) within this share of the height above and
/// below the X, this long (as a share of the height) to its left.
const BAR_STEP: i32 = 15;
const BAR_STRAIGHT: f64 = 0.9;
const BAR_STRAIGHT_FAINTER: f64 = 0.7;
const BAR_REACH: f64 = 0.04;
const BAR_SPAN: f64 = 0.4;

/// The band across the middle looked over for a pop-up's bar: its width (as a share of the
/// game's height, centred) and its top and bottom (shares of the height down).
const BAND_W: f64 = 0.9;
const BAND_TOP: f64 = 0.4;
const BAND_BOTTOM: f64 = 0.95;
/// The bar: this flat (of 255: each pixel from the last, and the most it drifts from its
/// first: the bar shades a little), this grey, at least this long (a share of the height),
/// and this tall; the share of its flat rows' pixels that must be on its grey (our widgets
/// over it are left out); its edges within this share of the height above and below (they
/// blur over a few rows); under it, the dimmed screen: this dark, along this share of it.
const POPUP_FLAT: i32 = 3;
const POPUP_DRIFT: i32 = 8;
const POPUP_EDGE: f64 = 0.012;
const POPUP_GREY: (u8, u8) = (25, 90);
const POPUP_SPAN: f64 = 0.45;
const POPUP_TALL: (f64, f64) = (0.035, 0.09);
const POPUP_ON: f64 = 0.85;
const POPUP_DIM: u8 = 15;
const POPUP_DIM_SHARE: f64 = 0.7;

/// The top of the screen's brightness, row by row.
pub struct Area {
    w: usize,
    h: usize,
    luma: Vec<u8>,
}

impl Area {
    fn at(&self, x: f64, y: f64) -> f32 {
        let (x, y) = (x.round(), y.round());
        if x < 0.0 || y < 0.0 || x >= self.w as f64 || y >= self.h as f64 {
            return 0.0;
        }
        f32::from(self.luma[y as usize * self.w + x as usize])
    }

    /// Blanks boxes of the area, (left, top, right, bottom) in its pixels: our widgets.
    pub fn blank(&mut self, boxes: &[(f64, f64, f64, f64)]) {
        let (w, h) = (self.w as f64, self.h as f64);
        for &(l, t, r, b) in boxes {
            let (l, t) = (l.max(0.0).floor() as usize, t.max(0.0).floor() as usize);
            let (r, b) = (r.min(w).ceil() as usize, b.min(h).ceil() as usize);
            for y in t..b {
                self.luma[y * self.w + l.min(r)..y * self.w + r].fill(0);
            }
        }
    }

    /// How much of row `y`, from `x0` to `x1`, is a straight edge (a step in brightness to
    /// the row two below).
    fn straight(&self, y: usize, x0: usize, x1: usize) -> f64 {
        if y + 2 >= self.h || x1 <= x0 {
            return 0.0;
        }
        let (a, b) = (&self.luma[y * self.w..(y + 1) * self.w], &self.luma[(y + 2) * self.w..(y + 3) * self.w]);
        let n = (x0..x1).filter(|&x| (i32::from(a[x]) - i32::from(b[x])).abs() >= BAR_STEP).count();
        n as f64 / (x1 - x0) as f64
    }

    /// A title bar around (cx, cy): a straight edge above and one below, near, long to the
    /// left.
    fn on_title_bar(&self, cx: usize, cy: usize, s: usize, height: f64) -> bool {
        let span = (BAR_SPAN * height) as usize;
        let reach = (BAR_REACH * height) as usize;
        let Some(x0) = cx.checked_sub(2 * s + span) else { return false };
        let x1 = cx - 2 * s;
        let best = |y: &dyn Fn(usize) -> Option<usize>| (s..=reach).filter_map(y).map(|y| self.straight(y, x0, x1)).fold(0.0, f64::max);
        let above = best(&|d| cy.checked_sub(d));
        let below = best(&|d| Some(cy + d));
        above.max(below) >= BAR_STRAIGHT && above.min(below) >= BAR_STRAIGHT_FAINTER
    }

    /// Row `y`'s longest flat grey stretch: (from, to, its grey), if at least `span` long.
    /// Blanked pixels (our widgets) go along with it but don't count as on it.
    fn flat_run(&self, y: usize, span: usize) -> Option<(usize, usize, i32)> {
        let row = &self.luma[y * self.w..(y + 1) * self.w];
        let mut best: Option<(usize, usize, i32)> = None;
        let mut x = 0;
        while x < self.w {
            let v = i32::from(row[x]);
            if !(i32::from(POPUP_GREY.0)..=i32::from(POPUP_GREY.1)).contains(&v) {
                x += 1;
                continue;
            }
            // Blanked pixels go along with it, but it ends at its last pixel of the grey.
            let (start, mut at, mut end, mut on, mut last) = (x, x, x, 0usize, v);
            while at < self.w {
                let p = i32::from(row[at]);
                if p != 0 {
                    if (p - last).abs() > POPUP_FLAT || (p - v).abs() > POPUP_DRIFT {
                        break;
                    }
                    last = p;
                    on += 1;
                    end = at + 1;
                }
                at += 1;
            }
            let len = end - start;
            if len >= span && on as f64 >= POPUP_ON * len as f64 && best.is_none_or(|(a, b, _)| len > b - a) {
                best = Some((start, end, v));
            }
            x = at.max(x + 1);
        }
        best
    }

    /// A pop-up's bar in the band: flat grey rows, long and a few percent of the height
    /// tall (rows through its text needn't be flat), an edge along its top, the dimmed
    /// screen under it. `height`: the game picture's height.
    pub fn has_popup_bar(&self, height: f64) -> bool {
        let span = (POPUP_SPAN * height) as usize;
        let (lo, hi) = ((POPUP_TALL.0 * height) as usize, (POPUP_TALL.1 * height) as usize);
        let rows: Vec<Option<(usize, usize, i32)>> = (0..self.h).map(|y| self.flat_run(y, span)).collect();
        let edge = ((POPUP_EDGE * height) as usize).max(3);
        let same = |a: (usize, usize, i32), b: (usize, usize, i32)| {
            let near = |p: usize, q: usize| p.abs_diff(q) as f64 <= 0.02 * height;
            near(a.0, b.0) && near(a.1, b.1) && (a.2 - b.2).abs() <= POPUP_DRIFT
        };
        // How much of row `y` from x0 to x1 is off the grey by an edge's step, or dim.
        let off = |y: usize, x0: usize, x1: usize, grey: i32| (x0..x1).filter(|&x| (i32::from(self.luma[y * self.w + x]) - grey).abs() >= BAR_STEP).count() as f64 / (x1 - x0) as f64;
        let dim = |y: usize, x0: usize, x1: usize| (x0..x1).filter(|&x| self.luma[y * self.w + x] <= POPUP_DIM).count() as f64 / (x1 - x0) as f64;
        for top in 1..self.h {
            let Some(first) = rows[top] else { continue };
            // Its first flat row (the one above isn't), with its top edge just above: the
            // picture over it.
            if rows[top - 1].is_some_and(|r| same(r, first)) {
                continue;
            }
            let (x0, x1, grey) = first;
            if !(top.saturating_sub(edge)..top).any(|y| off(y, x0, x1, grey) >= 0.7) {
                continue;
            }
            // Its bottom: the last row of its grey within the tallest it can be, then the
            // dimmed screen.
            let last = (top..(top + hi).min(self.h)).rev().find(|&y| rows[y].is_some_and(|r| same(r, first)));
            let Some(bottom) = last else { continue };
            let tall = bottom - top + 1;
            let flat = (top..=bottom).filter(|&y| rows[y].is_some_and(|r| same(r, first))).count();
            if tall < lo || flat * 5 < tall * 2 {
                continue;
            }
            if (bottom + 1..(bottom + 1 + edge).min(self.h)).any(|y| dim(y, x0, x1) >= POPUP_DIM_SHARE) {
                return true;
            }
        }
        false
    }

    /// The brightest within [`WINDOW_X_SLACK`] pixels up or down.
    fn near(&self, x: f64, y: f64) -> f32 {
        (-WINDOW_X_SLACK..=WINDOW_X_SLACK).map(|k| self.at(x, y + f64::from(k))).fold(0.0, f32::max)
    }

    /// A window's close X anywhere in the area: an X-like mark on a title bar. `height`: the
    /// game picture's height.
    pub fn has_window_close(&self, height: f64) -> bool {
        let (lo, hi) = (((WINDOW_ARM.0 * height).round() as usize).max(5), (WINDOW_ARM.1 * height).round() as usize);
        let q = lo as f64 * 0.6;
        // Between an X's arms it's dark: at most this bright, for its strokes (255 at most)
        // to stand the score above it.
        let dark = 255.0 - WINDOW_X_SCORE;
        let g0 = lo as f64 * 0.75;
        let started = std::time::Instant::now();
        for cy in 4..self.h.saturating_sub(4) {
            // Not in tests, which run many at once unoptimised: they check the looking itself.
            if !cfg!(test) && started.elapsed() > WINDOW_BUDGET {
                return false;
            }
            for cx in 4..self.w.saturating_sub(4) {
                let (x, y) = (cx as f64, cy as f64);
                // A cheap first look: a bright crossing, bright a little out along both
                // diagonals, dark a little out straight up, down and to the sides (bright
                // ground passes the rest everywhere).
                if self.at(x, y) < 130.0
                    || [(g0, 0.0), (-g0, 0.0), (0.0, g0), (0.0, -g0)].iter().any(|(dx, dy)| self.at(x + dx, y + dy) > dark)
                    || [(q, q), (-q, -q), (q, -q), (-q, q)].iter().any(|(dx, dy)| self.near(x + dx, y + dy) < 120.0)
                {
                    continue;
                }
                for s in lo..=hi.max(lo) {
                    let g = s as f64 * 0.75;
                    let gap = [(g, 0.0), (-g, 0.0), (0.0, g), (0.0, -g)].iter().map(|(dx, dy)| self.at(x + dx, y + dy)).fold(0.0, f32::max);
                    if gap > dark {
                        continue;
                    }
                    let (mut on, mut n) = (0.0, 0.0);
                    for t in -(s as i32)..=s as i32 {
                        if t.abs() < 2 {
                            continue;
                        }
                        let t = f64::from(t);
                        on += self.near(x + t, y + t) + self.near(x + t, y - t);
                        n += 2.0;
                    }
                    let on = on / n;
                    if on - gap >= WINDOW_X_SCORE && on >= WINDOW_X_BRIGHT {
                        if self.on_title_bar(cx, cy, s, height) {
                            return true;
                        }
                        break;
                    }
                }
            }
        }
        false
    }
}

/// Reads the top of a screen area (left, top, width, height) for [`Area::has_window_close`].
pub fn read_top(area: (i32, i32, i32, i32)) -> Option<Area> {
    let (left, top, width, height) = area;
    let (x, w, h) = top_box(width, height);
    if w < 64 || h < 64 {
        return None;
    }
    let px = read_screen(left + x, top, w, h)?;
    let luma = px.iter().map(|[r, g, b]| ((u32::from(*r) * 299 + u32::from(*g) * 587 + u32::from(*b) * 114) / 1000) as u8).collect();
    Some(Area { w: w as usize, h: h as usize, luma })
}

/// The band across the middle (see [`BAND_W`]), as brightness.
pub fn read_band(area: (i32, i32, i32, i32)) -> Option<Area> {
    let (left, top, width, height) = area;
    let (x, y, w, h) = band_box(width, height);
    if w < 64 || h < 64 {
        return None;
    }
    let px = read_screen(left + x, top + y, w, h)?;
    let luma = px.iter().map(|[r, g, b]| ((u32::from(*r) * 299 + u32::from(*g) * 587 + u32::from(*b) * 114) / 1000) as u8).collect();
    Some(Area { w: w as usize, h: h as usize, luma })
}

/// The band's left and top within the game's picture, its width and height.
pub fn band_box(width: i32, height: i32) -> (i32, i32, i32, i32) {
    let w = ((f64::from(height) * BAND_W).round() as i32).min(width);
    let (y0, y1) = ((f64::from(height) * BAND_TOP).round() as i32, (f64::from(height) * BAND_BOTTOM).round() as i32);
    ((width - w) / 2, y0, w, y1 - y0)
}

/// The top area's left edge within the game's picture, its width and height.
pub fn top_box(width: i32, height: i32) -> (i32, i32, i32) {
    let x = (f64::from(width) * WINDOW_LEFT).round() as i32;
    (x, width - x, (f64::from(height) * WINDOW_DOWN).round() as i32)
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

/// The Black Cloud Merchant is a page of the Shop, and both have the close X in the corner,
/// so leaving the merchant (its Exit) back to the Shop doesn't take that away. What tells
/// them apart is at the top left: the Shop shows two signs there ("Black Cloud Merchants",
/// "Closet"), the merchant's page one ("Exit"), and its opening none. So a band where the
/// Shop's second sign is, past where the Exit sign ends (as shares of the game's height:
/// left, right, top, bottom), is looked at: bright, the Shop's back. On 2000×1125
/// screenshots: 40 % of a wider band bright on the Shop, 4 to 5 % on the merchant's page and
/// its opening (the Exit sign's edge); this one starts past it.
const SHOP_SIGN_BAND: (f64, f64, f64, f64) = (0.29, 0.55, 0.018, 0.05);
/// Bright: this bright (of 255), on at least this share of the band.
const SHOP_SIGN_BRIGHT: u8 = 150;
const SHOP_SIGN_SHARE: f64 = 0.25;

/// Where [`read_shop_sign`] looks within the game's picture: (left, top, width, height).
pub fn shop_sign_band(height: i32) -> (i32, i32, i32, i32) {
    let h = f64::from(height);
    let (l, r, t, b) = SHOP_SIGN_BAND;
    let at = |v: f64| (v * h).round() as i32;
    (at(l), at(t), at(r) - at(l), at(b) - at(t))
}

/// Whether the Shop's second sign shows (you're in the Shop, not at the merchant), our
/// own windows' boxes (`skip`, in the band's pixels) left out; None when it can't be read
/// or they cover it.
pub fn read_shop_sign(area: (i32, i32, i32, i32), skip: &[(f64, f64, f64, f64)]) -> Option<bool> {
    let (left, top, width, height) = area;
    let (x, y, w, h) = shop_sign_band(height);
    if w < 16 || h < 4 || x + w > width {
        return None;
    }
    let px = read_screen(left + x, top + y, w, h)?;
    let luma: Vec<u8> = px.iter().map(|[r, g, b]| ((u32::from(*r) * 299 + u32::from(*g) * 587 + u32::from(*b) * 114) / 1000) as u8).collect();
    shop_sign_lit(&luma, w as usize, skip)
}

/// Whether enough of the band (its pixels outside `skip`) is bright.
fn shop_sign_lit(luma: &[u8], w: usize, skip: &[(f64, f64, f64, f64)]) -> Option<bool> {
    let (mut seen, mut bright) = (0usize, 0usize);
    for (i, &l) in luma.iter().enumerate() {
        let (x, y) = ((i % w) as f64, (i / w) as f64);
        if skip.iter().any(|&(l0, t0, r0, b0)| x >= l0 && x < r0 && y >= t0 && y < b0) {
            continue;
        }
        seen += 1;
        if l >= SHOP_SIGN_BRIGHT {
            bright += 1;
        }
    }
    // Mostly covered: can't be told.
    (seen * 2 >= luma.len()).then_some(bright as f64 >= SHOP_SIGN_SHARE * seen as f64)
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

    /// The top of a 1125-high, 2000-wide screen (from 30 % across): a dark scene, maybe a
    /// window's title bar (a lighter band with straight edges) and an X on it.
    fn top(bar: bool, x: bool) -> Area {
        let (w, h) = (1400usize, 675usize);
        let mut luma: Vec<u8> = (0..w * h).map(|i| (20 + (i * 37 % 41)) as u8).collect();
        // The window: its title bar from 400 to 1350 across, rows 205-266.
        if bar {
            for y in 205..267 {
                for px in 400..1350 {
                    luma[y * w + px] = if y <= 207 || y >= 264 { 110 } else { 60 };
                }
            }
        }
        if x {
            // Near the bar's right end, as a window's close X.
            let (cx, cy) = (1320i64, 237i64);
            // An X 2 px thick, arms 9 px.
            for t in -9i64..=9 {
                for (dx, dy) in [(t, t), (t, -t)] {
                    for wdt in 0..2 {
                        let (px, py) = ((cx + dx + wdt) as usize, (cy + dy) as usize);
                        luma[py * w + px] = 230;
                    }
                }
            }
        }
        Area { w, h, luma }
    }

    #[test]
    fn a_windows_close_x_on_its_title_bar() {
        assert!(top(true, true).has_window_close(1125.0));
        // An X-like mark with no title bar (an icon in the world), or a bar with no X.
        assert!(!top(false, true).has_window_close(1125.0));
        assert!(!top(true, false).has_window_close(1125.0));
        // Our own widget over it: left out.
        let mut a = top(true, true);
        a.blank(&[(1300.0, 200.0, 1400.0, 280.0)]);
        assert!(!a.has_window_close(1125.0));
    }

    #[test]
    fn a_thin_tall_x_over_a_line_that_fades() {
        // As Force Settings': strokes 1 px, arms 6 px down for 5 across, and the line under
        // its title bar faint at its left end.
        let mut a = top(true, false);
        let (cx, cy) = (1320i64, 237i64);
        for t in -12i64..=12 {
            let dx = (t as f64 * 5.0 / 6.0).round() as i64;
            for px in [cx + dx, cx - dx] {
                a.luma[(cy + t) as usize * a.w + px as usize] = 210;
            }
        }
        for y in 264..267 {
            a.luma[y * a.w + 400..y * a.w + 950].fill(60);
        }
        assert!(a.has_window_close(1125.0));
        // No line under it at all: no window.
        for y in 264..267 {
            a.luma[y * a.w + 400..y * a.w + 1350].fill(60);
        }
        assert!(!a.has_window_close(1125.0));
    }

    /// A login banner over the band (as on a 2000×1126 screenshot, rows from the band's
    /// top): everything else dimmed to 5, the picture at 110 over the bar, the bar 46 to
    /// 48 (shading a little), its text through the middle rows, its edges blurred.
    fn banner() -> Area {
        let (w, h) = (1013usize, 620usize);
        let mut luma = vec![5u8; w * h];
        for y in 0..h {
            for x in 107..907 {
                luma[y * w + x] = match y {
                    100..=341 => 110,
                    342 => 80,
                    343..=410 => 46 + (x * 3 / 800) as u8,
                    411 => 30,
                    _ => 5,
                };
            }
        }
        // "Don't show again today": bright text through rows 365-390.
        for y in 365..391 {
            for x in (180..440).step_by(7) {
                luma[y * w + x] = 230;
            }
        }
        Area { w, h, luma }
    }

    #[test]
    fn a_popups_bar_over_the_dimmed_game() {
        assert!(banner().has_popup_bar(1126.0));
        // Our meter over its right end: left out, still a bar.
        let mut a = banner();
        a.blank(&[(833.0, 215.0, 1013.0, 443.0)]);
        assert!(a.has_popup_bar(1126.0));
        // The game not dimmed under it (a flat grey panel in the world): no pop-up.
        let mut a = banner();
        for y in 412..430 {
            a.luma[y * a.w..(y + 1) * a.w].fill(70);
        }
        assert!(!a.has_popup_bar(1126.0));
        // Too short to be the bar.
        let mut a = banner();
        for y in 343..410 {
            a.luma[y * a.w + 400..y * a.w + 907].fill(120);
        }
        assert!(!a.has_popup_bar(1126.0));
    }

    #[test]
    fn the_shops_second_sign_is_a_bright_band() {
        let (w, h) = (100usize, 10usize);
        let dark = vec![20u8; w * h];
        assert_eq!(shop_sign_lit(&dark, w, &[]), Some(false));
        // The sign over most of the band, as on the Shop's page.
        let mut lit = dark.clone();
        for y in 2..9 {
            lit[y * w + 10..y * w + 70].fill(210);
        }
        assert_eq!(shop_sign_lit(&lit, w, &[]), Some(true));
        // A bright widget of ours over a dark band (the merchant's page): left out, dark.
        let mut ours = dark.clone();
        for y in 0..10 {
            ours[y * w..y * w + 40].fill(230);
        }
        assert_eq!(shop_sign_lit(&ours, w, &[]), Some(true));
        assert_eq!(shop_sign_lit(&ours, w, &[(0.0, 0.0, 40.0, 10.0)]), Some(false));
        // Covered all over: can't be told.
        assert_eq!(shop_sign_lit(&lit, w, &[(0.0, 0.0, 100.0, 10.0)]), None);
        // The band's place on a 2000×1125 screen.
        assert_eq!(shop_sign_band(1125), (326, 20, 293, 36));
    }

    #[test]
    fn bright_ground_is_looked_over_quickly() {
        // The top of a 3440×1440 screen over bright ground (snow, white stone): nearly
        // everything is bright enough to start an X, and once took a minute to look over.
        let (w, h) = (2408usize, 864usize);
        let luma: Vec<u8> = (0..w * h).map(|i| (190 + ((i as u32).wrapping_mul(2_654_435_761) >> 27)) as u8).collect();
        let a = Area { w, h, luma };
        let t = std::time::Instant::now();
        assert!(!a.has_window_close(1440.0));
        assert!(t.elapsed() < std::time::Duration::from_secs(1), "took {:?}", t.elapsed());
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
