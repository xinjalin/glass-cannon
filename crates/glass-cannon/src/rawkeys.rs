//! The keyboard through Windows' raw input: a hidden window of ours asks for every key
//! press and release, even while another window (the game) has the focus. Some games keep
//! their keys from the keyboard state other apps read (`GetAsyncKeyState`); raw input comes
//! straight from the keyboard, so it sees them all the same. Which of the two sees keys
//! in game is counted, for the settings to show.
//!
//! Run on the hotkeys thread, whose message loop delivers the window's messages.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

static DOWN: [AtomicU64; 4] = [AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0)];
/// The game is in front (kept by the overlay's watcher), for the counts.
static GAME_FRONT: AtomicBool = AtomicBool::new(false);
/// Key presses seen in game through raw input, and how many the keyboard state saw too.
static IN_GAME: AtomicU64 = AtomicU64::new(0);
static STATE_SAW: AtomicU64 = AtomicU64::new(0);

const VK_SHIFT: u8 = 0x10;
const VK_CONTROL: u8 = 0x11;
const VK_MENU: u8 = 0x12;

/// The key is held, as raw input last said.
pub fn down(vk: u8) -> bool {
    DOWN[usize::from(vk / 64)].load(Ordering::Relaxed) & (1 << (vk % 64)) != 0
}

fn set(vk: u8, on: bool) {
    let bit = 1u64 << (vk % 64);
    if on {
        DOWN[usize::from(vk / 64)].fetch_or(bit, Ordering::Relaxed);
    } else {
        DOWN[usize::from(vk / 64)].fetch_and(!bit, Ordering::Relaxed);
    }
}

pub fn set_game_front(on: bool) {
    GAME_FRONT.store(on, Ordering::Relaxed);
}

/// (key presses seen in game through raw input, how many of them the keyboard state saw).
pub fn counts() -> (u64, u64) {
    (IN_GAME.load(Ordering::Relaxed), STATE_SAW.load(Ordering::Relaxed))
}

#[repr(C)]
struct WndClass {
    style: u32,
    proc: unsafe extern "system" fn(isize, u32, usize, isize) -> isize,
    cls_extra: i32,
    wnd_extra: i32,
    instance: isize,
    icon: isize,
    cursor: isize,
    background: isize,
    menu_name: *const u16,
    class_name: *const u16,
}

#[repr(C)]
struct RawInputDevice {
    usage_page: u16,
    usage: u16,
    flags: u32,
    target: isize,
}

#[link(name = "user32")]
extern "system" {
    fn RegisterClassW(class: *const WndClass) -> u16;
    fn CreateWindowExW(
        ex_style: u32,
        class: *const u16,
        title: *const u16,
        style: u32,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        parent: isize,
        menu: isize,
        instance: isize,
        param: *const std::ffi::c_void,
    ) -> isize;
    fn DefWindowProcW(hwnd: isize, msg: u32, wparam: usize, lparam: isize) -> isize;
    fn RegisterRawInputDevices(devices: *const RawInputDevice, count: u32, size: u32) -> i32;
    fn GetRawInputData(raw: isize, command: u32, data: *mut u8, size: *mut u32, header_size: u32) -> u32;
    fn GetAsyncKeyState(vk: i32) -> i16;
}

#[link(name = "kernel32")]
extern "system" {
    fn GetModuleHandleW(name: *const u16) -> isize;
}

const WM_INPUT: u32 = 0x00FF;
const RID_INPUT: u32 = 0x1000_0003;
const RIM_TYPEKEYBOARD: u32 = 1;
const RIDEV_INPUTSINK: u32 = 0x100;
const RI_KEY_BREAK: u16 = 1;
const RI_KEY_E0: u16 = 2;
/// The raw input header: type, size, device, wParam.
const HEADER: usize = 4 + 4 + std::mem::size_of::<isize>() * 2;

/// The keys a raw keyboard event stands for: the side-specific key, and the plain one.
fn keys_of(vk: u16, make: u16, flags: u16) -> Option<(u8, Option<u8>)> {
    let vk = u8::try_from(vk).ok().filter(|v| *v != 0xFF && *v != 0)?;
    let e0 = flags & RI_KEY_E0 != 0;
    Some(match vk {
        VK_MENU => (if e0 { 0xA5 } else { 0xA4 }, Some(VK_MENU)),
        VK_CONTROL => (if e0 { 0xA3 } else { 0xA2 }, Some(VK_CONTROL)),
        VK_SHIFT => (if make == 0x36 { 0xA1 } else { 0xA0 }, Some(VK_SHIFT)),
        v => (v, None),
    })
}

unsafe extern "system" fn proc(hwnd: isize, msg: u32, wparam: usize, lparam: isize) -> isize {
    if msg == WM_INPUT {
        let mut buf = [0u8; 64];
        let mut size = buf.len() as u32;
        // SAFETY: Windows fills our buffer, no more than `size` bytes, for this message.
        let got = unsafe { GetRawInputData(lparam, RID_INPUT, buf.as_mut_ptr(), &mut size, HEADER as u32) };
        let u16_at = |i: usize| u16::from_le_bytes([buf[i], buf[i + 1]]);
        if got != u32::MAX && got as usize >= HEADER + 8 && u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]) == RIM_TYPEKEYBOARD {
            let (make, flags, vk) = (u16_at(HEADER), u16_at(HEADER + 2), u16_at(HEADER + 6));
            if let Some((key, plain)) = keys_of(vk, make, flags) {
                let press = flags & RI_KEY_BREAK == 0;
                let fresh = press && !down(key);
                set(key, press);
                if let Some(p) = plain {
                    // The plain key is held while either side is.
                    let other = if key % 2 == 0 { key + 1 } else { key - 1 };
                    set(p, press || down(other));
                }
                if fresh && GAME_FRONT.load(Ordering::Relaxed) {
                    IN_GAME.fetch_add(1, Ordering::Relaxed);
                    // SAFETY: a plain query of the keyboard's state.
                    if unsafe { GetAsyncKeyState(i32::from(key)) } < 0 {
                        STATE_SAW.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
        }
    }
    // SAFETY: the default handling of our own window's message.
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

/// Makes the hidden window and asks for the keyboard. Call once, on the thread whose
/// message loop runs; false if Windows said no.
pub fn start() -> bool {
    let name: Vec<u16> = "GlassCannonRawKeys\0".encode_utf16().collect();
    // SAFETY: registering our own window class and making a window of it that's never
    // shown; the strings outlive the calls.
    unsafe {
        let instance = GetModuleHandleW(std::ptr::null());
        let class = WndClass {
            style: 0,
            proc,
            cls_extra: 0,
            wnd_extra: 0,
            instance,
            icon: 0,
            cursor: 0,
            background: 0,
            menu_name: std::ptr::null(),
            class_name: name.as_ptr(),
        };
        if RegisterClassW(&class) == 0 {
            return false;
        }
        const WS_POPUP: u32 = 0x8000_0000;
        let hwnd = CreateWindowExW(0, name.as_ptr(), name.as_ptr(), WS_POPUP, 0, 0, 0, 0, 0, 0, instance, std::ptr::null());
        if hwnd == 0 {
            return false;
        }
        let device = RawInputDevice { usage_page: 0x01, usage: 0x06, flags: RIDEV_INPUTSINK, target: hwnd };
        RegisterRawInputDevices(&device, 1, std::mem::size_of::<RawInputDevice>() as u32) != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sides_and_plain_keys() {
        assert_eq!(keys_of(0x12, 0x38, RI_KEY_E0), Some((0xA5, Some(VK_MENU))));
        assert_eq!(keys_of(0x12, 0x38, 0), Some((0xA4, Some(VK_MENU))));
        assert_eq!(keys_of(0x10, 0x36, 0), Some((0xA1, Some(VK_SHIFT))));
        assert_eq!(keys_of(0x45, 0x12, 0), Some((0x45, None)));
        assert_eq!(keys_of(0xFF, 0, 0), None);
    }
}
