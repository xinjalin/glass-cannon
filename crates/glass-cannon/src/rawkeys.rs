//! The keyboard, two more ways than the keyboard state other apps read
//! (`GetAsyncKeyState`), which some games keep their keys from:
//!
//! - Windows' raw input: a hidden window of ours asks for every key press and release,
//!   even while another window (the game) has the focus;
//! - a low-level keyboard hook (as voice chat apps use for push-to-talk), which Windows
//!   calls with every key before any window gets it.
//!
//! A key is held while any of them says so. Which of them see keys in game is counted, for
//! the settings to show. Both run on a thread of their own that waits on its messages:
//! Windows holds every key until a low-level hook has answered, so it must answer at once
//! (a thread that slept between looks would make every key in game late).

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// Keys held, as one source says.
struct Table([AtomicU64; 4]);

impl Table {
    const fn new() -> Self {
        Self([AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0)])
    }
    fn get(&self, vk: u8) -> bool {
        self.0[usize::from(vk / 64)].load(Ordering::Relaxed) & (1 << (vk % 64)) != 0
    }
    fn set(&self, vk: u8, on: bool) {
        let bit = 1u64 << (vk % 64);
        if on {
            self.0[usize::from(vk / 64)].fetch_or(bit, Ordering::Relaxed);
        } else {
            self.0[usize::from(vk / 64)].fetch_and(!bit, Ordering::Relaxed);
        }
    }
    /// Sets a side's key and the plain key with it (held while either side is). True for a
    /// fresh press.
    fn key(&self, key: u8, plain: Option<u8>, press: bool) -> bool {
        let fresh = press && !self.get(key);
        self.set(key, press);
        if let Some(p) = plain {
            let other = key ^ 1;
            self.set(p, press || self.get(other));
        }
        fresh
    }
}

static RAW: Table = Table::new();
static HOOK: Table = Table::new();
/// Key presses seen through each, in game; and how many of the raw ones the keyboard state
/// saw too. Also through each, anywhere.
static RAW_IN_GAME: AtomicU64 = AtomicU64::new(0);
static HOOK_IN_GAME: AtomicU64 = AtomicU64::new(0);
static STATE_SAW: AtomicU64 = AtomicU64::new(0);
static RAW_ANY: AtomicU64 = AtomicU64::new(0);
/// Windows took our asking for raw input, and the hook.
static RAW_ON: AtomicBool = AtomicBool::new(false);
static HOOK_ON: AtomicBool = AtomicBool::new(false);
static HOOK_ANY: AtomicU64 = AtomicU64::new(0);

/// The game is in front (kept by the overlay's watcher), for the counts.
static GAME_FRONT: AtomicBool = AtomicBool::new(false);

const VK_SHIFT: u8 = 0x10;
const VK_CONTROL: u8 = 0x11;
const VK_MENU: u8 = 0x12;

/// The key is held, as raw input or the hook last said.
pub fn down(vk: u8) -> bool {
    RAW.get(vk) || HOOK.get(vk)
}

pub fn set_game_front(on: bool) {
    GAME_FRONT.store(on, Ordering::Relaxed);
}

/// The keyboard state showed a key held while the game was in front.
static STATE_IN_GAME: AtomicBool = AtomicBool::new(false);

pub fn note_state_key() {
    if GAME_FRONT.load(Ordering::Relaxed) {
        STATE_IN_GAME.store(true, Ordering::Relaxed);
    }
}

/// Some way of reading the keyboard has seen a key while the game was in front: until
/// then, the meter can't tell Alt (the game's cursor) from a game menu.
pub fn keys_seen_in_game() -> bool {
    RAW_IN_GAME.load(Ordering::Relaxed) > 0 || HOOK_IN_GAME.load(Ordering::Relaxed) > 0 || STATE_IN_GAME.load(Ordering::Relaxed)
}

/// Key presses seen, for the settings.
#[derive(serde::Serialize)]
pub struct Counts {
    /// In game: through raw input, the hook, and the keyboard state (of the raw ones).
    raw: u64,
    hook: u64,
    state: u64,
    /// Anywhere: through raw input and the hook.
    raw_any: u64,
    hook_any: u64,
    /// Windows took raw input and the hook.
    raw_on: bool,
    hook_on: bool,
}

pub fn counts() -> Counts {
    let l = |a: &AtomicU64| a.load(Ordering::Relaxed);
    Counts {
        raw: l(&RAW_IN_GAME),
        hook: l(&HOOK_IN_GAME),
        state: l(&STATE_SAW),
        raw_any: l(&RAW_ANY),
        hook_any: l(&HOOK_ANY),
        raw_on: RAW_ON.load(Ordering::Relaxed),
        hook_on: HOOK_ON.load(Ordering::Relaxed),
    }
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
    fn SetWindowsHookExW(id: i32, proc: unsafe extern "system" fn(i32, usize, isize) -> isize, instance: isize, thread: u32) -> isize;
    fn CallNextHookEx(hook: isize, code: i32, wparam: usize, lparam: isize) -> isize;
    fn GetMessageW(msg: *mut Msg, hwnd: isize, min: u32, max: u32) -> i32;
    fn DispatchMessageW(msg: *const Msg) -> isize;
}

#[repr(C)]
struct Msg {
    hwnd: isize,
    message: u32,
    wparam: usize,
    lparam: isize,
    time: u32,
    pt: [i32; 2],
    private: u32,
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
                if RAW.key(key, plain, flags & RI_KEY_BREAK == 0) {
                    RAW_ANY.fetch_add(1, Ordering::Relaxed);
                    if GAME_FRONT.load(Ordering::Relaxed) {
                        RAW_IN_GAME.fetch_add(1, Ordering::Relaxed);
                        // SAFETY: a plain query of the keyboard's state.
                        if unsafe { GetAsyncKeyState(i32::from(key)) } < 0 {
                            STATE_SAW.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                }
            }
        }
    }
    // SAFETY: the default handling of our own window's message.
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

const WH_KEYBOARD_LL: i32 = 13;
const LLKHF_UP: u32 = 0x80;

/// The low-level hook: (vk, scan, flags, time, extra), as Windows hands it over.
#[repr(C)]
struct KbdLl {
    vk: u32,
    scan: u32,
    flags: u32,
    time: u32,
    extra: usize,
}

unsafe extern "system" fn hook(code: i32, wparam: usize, lparam: isize) -> isize {
    if code >= 0 && lparam != 0 {
        // SAFETY: for HC_ACTION, lparam points at the key's KBDLLHOOKSTRUCT.
        let k = unsafe { &*(lparam as *const KbdLl) };
        if let Ok(vk) = u8::try_from(k.vk) {
            // The hook names the side already (Left/Right Alt…); the plain key with it.
            let plain = match vk {
                0xA4 | 0xA5 => Some(VK_MENU),
                0xA2 | 0xA3 => Some(VK_CONTROL),
                0xA0 | 0xA1 => Some(VK_SHIFT),
                _ => None,
            };
            if HOOK.key(vk, plain, k.flags & LLKHF_UP == 0) {
                HOOK_ANY.fetch_add(1, Ordering::Relaxed);
                if GAME_FRONT.load(Ordering::Relaxed) {
                    HOOK_IN_GAME.fetch_add(1, Ordering::Relaxed);
                }
            }
        }
    }
    // SAFETY: passing the key on, untouched.
    unsafe { CallNextHookEx(0, code, wparam, lparam) }
}

/// Starts reading the keyboard, both ways, on a thread of its own. Call once.
pub fn start() {
    let _ = std::thread::Builder::new().name("keys".into()).spawn(|| {
        let raw = start_raw();
        // SAFETY: a low-level hook calling our function above; this thread's loop runs it.
        let hooked = unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, hook, GetModuleHandleW(std::ptr::null()), 0) } != 0;
        RAW_ON.store(raw, Ordering::Relaxed);
        HOOK_ON.store(hooked, Ordering::Relaxed);
        if !raw && !hooked {
            return;
        }
        let mut msg = Msg { hwnd: 0, message: 0, wparam: 0, lparam: 0, time: 0, pt: [0; 2], private: 0 };
        // SAFETY: waiting on this thread's own messages, and handing our window its own.
        while unsafe { GetMessageW(&mut msg, 0, 0, 0) } > 0 {
            unsafe { DispatchMessageW(&msg) };
        }
    });
}

fn start_raw() -> bool {
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
