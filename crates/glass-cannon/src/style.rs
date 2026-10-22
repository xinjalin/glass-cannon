//! Native window styling on Windows: the background effect, rounded corners, and a border
//! in the theme's colour (or none at all in minimal mode).
//!
//! "Frosted" is Windows 11's acrylic backdrop. Windows switches acrylic off whenever a window
//! loses focus, which an overlay always does, so the windows are subclassed to always draw
//! as active (WM_NCACTIVATE is passed on as "active"); focus itself works as usual. On older
//! Windows, which has no backdrop attribute, the legacy acrylic accent is used instead.
//! (The plain blur-behind accent renders solid black on current Windows 11.)

use std::ffi::c_void;

use tauri::{Runtime, WebviewWindow};

use crate::store::Appearance;

const DWMWA_WINDOW_CORNER_PREFERENCE: u32 = 33;
const DWMWA_BORDER_COLOR: u32 = 34;
const DWMWA_SYSTEMBACKDROP_TYPE: u32 = 38;
const DWMWCP_ROUND: u32 = 2;
const DWMSBT_NONE: u32 = 1;
const DWMSBT_TRANSIENTWINDOW: u32 = 3;
/// DWMWA_COLOR_NONE: no border at all.
const NO_BORDER: u32 = 0xFFFF_FFFE;
const WM_NCACTIVATE: u32 = 0x0086;
const WCA_ACCENT_POLICY: u32 = 0x13;
const ACCENT_DISABLED: u32 = 0;
const ACCENT_ENABLE_ACRYLICBLURBEHIND: u32 = 4;

#[repr(C)]
struct AccentPolicy {
    state: u32,
    flags: u32,
    gradient: u32,
    animation: u32,
}

#[repr(C)]
struct CompositionData {
    attribute: u32,
    data: *mut c_void,
    size: usize,
}

type SubclassProc = unsafe extern "system" fn(isize, u32, usize, isize, usize, usize) -> isize;
type SetWindowCompositionAttribute = unsafe extern "system" fn(isize, *mut CompositionData) -> i32;

#[link(name = "dwmapi")]
extern "system" {
    fn DwmSetWindowAttribute(hwnd: isize, attribute: u32, value: *const u32, size: u32) -> i32;
}

#[link(name = "comctl32")]
extern "system" {
    fn SetWindowSubclass(hwnd: isize, proc: SubclassProc, id: usize, data: usize) -> i32;
    fn DefSubclassProc(hwnd: isize, msg: u32, wparam: usize, lparam: isize) -> isize;
}

#[link(name = "kernel32")]
extern "system" {
    fn GetModuleHandleA(name: *const u8) -> isize;
    fn GetProcAddress(module: isize, name: *const u8) -> *const c_void;
}

/// Keeps the window's frame (and so its acrylic) in the active state.
unsafe extern "system" fn always_active(
    hwnd: isize,
    msg: u32,
    wparam: usize,
    lparam: isize,
    _id: usize,
    _data: usize,
) -> isize {
    let wparam = if msg == WM_NCACTIVATE { 1 } else { wparam };
    DefSubclassProc(hwnd, msg, wparam, lparam)
}

/// The undocumented accent API; looked up at runtime because user32.lib doesn't export it.
fn set_accent(hwnd: isize, state: u32) {
    // SAFETY: both names are NUL-terminated, and the function's signature is stable since
    // Windows 7. The policy outlives the call.
    unsafe {
        let user32 = GetModuleHandleA(c"user32.dll".as_ptr().cast());
        let f = GetProcAddress(user32, c"SetWindowCompositionAttribute".as_ptr().cast());
        if f.is_null() {
            return;
        }
        let f: SetWindowCompositionAttribute = std::mem::transmute(f);
        // Acrylic needs a non-zero tint alpha to show; the page draws the real tint.
        let mut policy = AccentPolicy { state, flags: 0, gradient: 0x0100_0000, animation: 0 };
        let mut data = CompositionData {
            attribute: WCA_ACCENT_POLICY,
            data: &mut policy as *mut AccentPolicy as *mut c_void,
            size: std::mem::size_of::<AccentPolicy>(),
        };
        f(hwnd, &mut data);
    }
}

/// A muted version of each theme's accent for the window border, as `0xRRGGBB`.
fn border_rgb(theme: &str) -> u32 {
    match theme {
        "lavender" => 0x9c88cf,
        "mint" => 0x6fb592,
        "sky" => 0x6fa3cc,
        "amber" => 0xc99a52,
        "mono" => 0xa8a8a8,
        hex if hex.len() == 7 && hex.starts_with('#') => match u32::from_str_radix(&hex[1..], 16) {
            // A picked colour, made readable as the pages make it, a little darker (as the
            // built-in themes' frames are than their accents).
            Ok(rgb) => darker(readable(rgb)),
            Err(_) => 0xc98a86,
        },
        _ => 0xc98a86, // rose
    }
}

/// Lightness every picked colour is raised to, keeping its hue (theme.js does the same).
const MIN_LIGHTNESS: f64 = 0.62;

fn readable(rgb: u32) -> u32 {
    let [r, g, b] = [rgb >> 16 & 0xff, rgb >> 8 & 0xff, rgb & 0xff].map(|c| f64::from(c) / 255.0);
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let l = (max + min) / 2.0;
    if l >= MIN_LIGHTNESS {
        return rgb;
    }
    let d = max - min;
    let s = if d == 0.0 { 0.0 } else { d / (1.0 - (2.0 * l - 1.0).abs()) };
    let h = if d == 0.0 {
        0.0
    } else if max == r {
        ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    let c = (1.0 - (2.0 * MIN_LIGHTNESS - 1.0).abs()) * s;
    let x = c * (1.0 - (h % 2.0 - 1.0).abs());
    let m = MIN_LIGHTNESS - c / 2.0;
    let (r1, g1, b1) = match h as u32 % 6 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let to = |v: f64| ((v + m) * 255.0).round().clamp(0.0, 255.0) as u32;
    to(r1) << 16 | to(g1) << 8 | to(b1)
}

fn darker(rgb: u32) -> u32 {
    let to = |c: u32| (f64::from(c) * 0.83).round() as u32;
    to(rgb >> 16 & 0xff) << 16 | to(rgb >> 8 & 0xff) << 8 | to(rgb & 0xff)
}


/// Styles the overlay (`is_overlay`) or the settings window for the given appearance.
/// The settings window always stays frosted and framed so it's readable.
pub fn apply<R: Runtime>(window: &WebviewWindow<R>, a: &Appearance, is_overlay: bool) {
    let minimal = is_overlay && a.minimal;
    let frosted = !is_overlay || a.backdrop == "frosted";
    // The drop shadow outlines the window, which minimal mode is meant to hide.
    let _ = window.set_shadow(!minimal);

    let Ok(hwnd) = window.hwnd() else { return };
    let hwnd = hwnd.0 as isize;
    let border = if minimal {
        NO_BORDER
    } else {
        let rgb = border_rgb(&a.theme);
        // COLORREF is 0x00BBGGRR.
        (rgb & 0xff) << 16 | (rgb & 0xff00) | (rgb >> 16 & 0xff)
    };
    // Subclassing has to happen on the thread that owns the window.
    let _ = window.run_on_main_thread(move || {
        // SAFETY: plain DWM and subclass calls on our own window with 4-byte values.
        unsafe {
            SetWindowSubclass(hwnd, always_active, 1, 0);
            let backdrop = if frosted { DWMSBT_TRANSIENTWINDOW } else { DWMSBT_NONE };
            let modern = DwmSetWindowAttribute(hwnd, DWMWA_SYSTEMBACKDROP_TYPE, &backdrop, 4) == 0;
            if !modern {
                set_accent(hwnd, if frosted { ACCENT_ENABLE_ACRYLICBLURBEHIND } else { ACCENT_DISABLED });
            }
            DwmSetWindowAttribute(hwnd, DWMWA_WINDOW_CORNER_PREFERENCE, &DWMWCP_ROUND, 4);
            DwmSetWindowAttribute(hwnd, DWMWA_BORDER_COLOR, &border, 4);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picked_colours_frame_the_window_readably() {
        // As theme.js makes them: #102040 -> #648bd8, #202020 -> #9e9e9e; light ones stay.
        assert_eq!(readable(0x102040), 0x648bd8);
        assert_eq!(readable(0x202020), 0x9e9e9e);
        assert_eq!(readable(0xf2a8a4), 0xf2a8a4);
        assert_eq!(border_rgb("#f2a8a4"), 0xc98b88);
        assert_eq!(border_rgb("rose"), 0xc98a86);
    }
}
