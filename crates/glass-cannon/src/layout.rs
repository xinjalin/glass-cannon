//! Where a window goes next to the meter: beside it (left, right, below or above, in a
//! given order of preference), shrunk down to its minimum size if that's what it takes,
//! and always inside the monitor's work area. Only when no side has room even for the
//! minimum size does it overlap the meter, and then as little as it can.
//!
//! Plain rectangles in physical pixels, so it can be tested without windows.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self { x, y, w, h }
    }
    fn right(&self) -> i32 {
        self.x + self.w
    }
    fn bottom(&self) -> i32 {
        self.y + self.h
    }
    /// Overlapping area with another rectangle.
    pub fn overlap(&self, o: &Rect) -> i64 {
        let w = (self.right().min(o.right()) - self.x.max(o.x)).max(0) as i64;
        let h = (self.bottom().min(o.bottom()) - self.y.max(o.y)).max(0) as i64;
        w * h
    }
    /// Entirely inside `o`.
    pub fn inside(&self, o: &Rect) -> bool {
        self.x >= o.x && self.y >= o.y && self.right() <= o.right() && self.bottom() <= o.bottom()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
    Below,
    Above,
}

/// Where to put a window of `size` (shrinkable to `min`) beside `avoid` (the meter),
/// inside `area` (the monitor's work area), `gap` apart. `sides` is the order of
/// preference: the first side with room for the full size wins; failing that, the side
/// with room for the biggest shrunk window.
pub fn beside(area: Rect, avoid: Rect, size: (i32, i32), min: (i32, i32), gap: i32, sides: &[Side]) -> Rect {
    // Never bigger than the screen, never smaller than the minimum (unless the screen is).
    let size = (size.0.min(area.w), size.1.min(area.h));
    let min = (min.0.min(size.0), min.1.min(size.1));
    let clamp = |v: i32, lo: i32, hi: i32| v.max(lo).min(hi.max(lo));

    let mut best: Option<(i64, Rect)> = None;
    for side in sides {
        // The free box on this side of the meter.
        let free = match side {
            Side::Left => Rect::new(area.x, area.y, avoid.x - gap - area.x, area.h),
            Side::Right => Rect::new(avoid.right() + gap, area.y, area.right() - avoid.right() - gap, area.h),
            Side::Below => Rect::new(area.x, avoid.bottom() + gap, area.w, area.bottom() - avoid.bottom() - gap),
            Side::Above => Rect::new(area.x, area.y, area.w, avoid.y - gap - area.y),
        };
        let (w, h) = (size.0.min(free.w), size.1.min(free.h));
        if w < min.0 || h < min.1 {
            continue;
        }
        // Next to the meter: level with its top, or flush with its right edge.
        let (x, y) = match side {
            Side::Left => (avoid.x - gap - w, clamp(avoid.y, area.y, area.bottom() - h)),
            Side::Right => (avoid.right() + gap, clamp(avoid.y, area.y, area.bottom() - h)),
            Side::Below => (clamp(avoid.right() - w, area.x, area.right() - w), avoid.bottom() + gap),
            Side::Above => (clamp(avoid.right() - w, area.x, area.right() - w), avoid.y - gap - h),
        };
        let r = Rect::new(x, y, w, h);
        if (w, h) == size {
            return r;
        }
        let a = w as i64 * h as i64;
        if best.is_none_or(|(b, _)| a > b) {
            best = Some((a, r));
        }
    }
    if let Some((_, r)) = best {
        return r;
    }
    // No room anywhere: the full size, in whichever corner covers the meter least.
    let corners = [
        (area.x, area.y),
        (area.right() - size.0, area.y),
        (area.x, area.bottom() - size.1),
        (area.right() - size.0, area.bottom() - size.1),
    ];
    corners
        .into_iter()
        .map(|(x, y)| Rect::new(x, y, size.0, size.1))
        .min_by_key(|r| r.overlap(&avoid))
        .expect("four corners")
}

/// Moves (and if need be shrinks) `r` to lie entirely inside `area`.
pub fn keep_inside(r: Rect, area: Rect) -> Rect {
    let (w, h) = (r.w.min(area.w), r.h.min(area.h));
    let x = r.x.max(area.x).min(area.right() - w);
    let y = r.y.max(area.y).min(area.bottom() - h);
    Rect::new(x, y, w, h)
}

#[cfg(test)]
mod tests {
    use super::*;

    // The three monitors this was tested on (work areas, taskbar excluded).
    const MAIN: Rect = Rect { x: 0, y: 0, w: 2560, h: 1392 };
    const PORTRAIT: Rect = Rect { x: 2560, y: 0, w: 1080, h: 1872 };
    const ALL: [Side; 4] = [Side::Left, Side::Right, Side::Below, Side::Above];

    #[test]
    fn beside_the_meter_on_the_preferred_side_when_it_fits() {
        // Meter at its default spot, right edge of the main screen.
        let meter = Rect::new(2108, 417, 420, 340);
        let r = beside(MAIN, meter, (1200, 780), (760, 480), 10, &ALL);
        assert_eq!(r, Rect::new(2108 - 10 - 1200, 417, 1200, 780));
        assert_eq!(r.overlap(&meter), 0);
        assert!(r.inside(&MAIN));
        // Meter at the left edge: the right side instead.
        let meter = Rect::new(20, 417, 420, 340);
        let r = beside(MAIN, meter, (1200, 780), (760, 480), 10, &ALL);
        assert_eq!((r.x, r.overlap(&meter)), (450, 0));
    }

    #[test]
    fn a_tall_window_moves_up_to_stay_on_screen() {
        let meter = Rect::new(2108, 1000, 420, 340);
        let r = beside(MAIN, meter, (1200, 780), (760, 480), 10, &ALL);
        assert_eq!(r.y, 1392 - 780);
        assert!(r.inside(&MAIN));
    }

    #[test]
    fn a_narrow_screen_puts_it_below_or_above() {
        // Portrait screen: no room either side of the meter for even the minimum width.
        let meter = Rect::new(2560 + 1080 - 452, 576, 420, 340);
        let r = beside(PORTRAIT, meter, (1200, 780), (760, 480), 10, &ALL);
        assert_eq!(r.overlap(&meter), 0);
        assert!(r.inside(&PORTRAIT));
        assert_eq!(r.y, 576 + 340 + 10, "below the meter");
        assert_eq!(r.w, 1080, "as wide as the screen allows");
    }

    #[test]
    fn it_shrinks_rather_than_overlaps() {
        // Room on the left for 900 of the 1200 pixels: shrink to fit (still above the minimum).
        let meter = Rect::new(910, 0, 420, 1392);
        let r = beside(Rect::new(0, 0, 1330, 1392), meter, (1200, 780), (760, 480), 10, &ALL);
        assert_eq!((r.x, r.w), (0, 900));
        assert_eq!(r.overlap(&meter), 0);
    }

    #[test]
    fn with_no_room_at_all_it_covers_the_meter_as_little_as_it_can() {
        let small = Rect::new(0, 0, 1000, 700);
        let meter = Rect::new(500, 200, 420, 340);
        let r = beside(small, meter, (1200, 780), (760, 480), 10, &ALL);
        assert!(r.inside(&small));
        assert_eq!((r.w, r.h), (1000, 700));
    }

    #[test]
    fn keep_inside_pulls_a_window_back_on_screen() {
        let r = keep_inside(Rect::new(2400, -50, 420, 340), MAIN);
        assert_eq!(r, Rect::new(2560 - 420, 0, 420, 340));
        let big = keep_inside(Rect::new(-100, 0, 3000, 2000), MAIN);
        assert_eq!(big, MAIN);
    }
}
