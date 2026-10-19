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
        _ => 0xc98a86, // rose
    }
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
