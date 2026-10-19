//! The font families installed on this PC, for the font picker in the settings. Read
//! from Windows each time the picker opens, so newly installed fonts show up.

use std::collections::BTreeMap;
use std::ffi::c_void;

use serde::Serialize;

const DEFAULT_CHARSET: u8 = 1;
const FIXED_PITCH: u8 = 1;
const LF_FACESIZE: usize = 32;
/// Font names go into CSS, so they're kept short and free of characters that could
/// break out of a quoted string.
const MAX_NAME_CHARS: usize = 64;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FontFamily {
    pub name: String,
    /// Fixed-pitch: the meter's columns line up best with these.
    pub mono: bool,
}

#[repr(C)]
#[derive(Default)]
struct LogFontW {
    height: i32,
    width: i32,
    escapement: i32,
    orientation: i32,
    weight: i32,
    italic: u8,
    underline: u8,
    strike_out: u8,
    char_set: u8,
    out_precision: u8,
    clip_precision: u8,
    quality: u8,
    pitch_and_family: u8,
    face_name: [u16; LF_FACESIZE],
}

type EnumProc = unsafe extern "system" fn(*const LogFontW, *const c_void, u32, isize) -> i32;

#[link(name = "gdi32")]
extern "system" {
    fn EnumFontFamiliesExW(hdc: isize, logfont: *const LogFontW, proc_: EnumProc, lparam: isize, flags: u32) -> i32;
}

#[link(name = "user32")]
extern "system" {
    fn GetDC(hwnd: isize) -> isize;
    fn ReleaseDC(hwnd: isize, hdc: isize) -> i32;
}

/// Called once per family (and once per character set within it).
unsafe extern "system" fn collect(font: *const LogFontW, _metrics: *const c_void, _kind: u32, data: isize) -> i32 {
    // SAFETY: Windows passes a valid LOGFONTW, and `data` is the map `installed` lent us.
    let (font, families) = unsafe { (&*font, &mut *(data as *mut BTreeMap<String, bool>)) };
    let len = font.face_name.iter().position(|&c| c == 0).unwrap_or(LF_FACESIZE);
    let name = String::from_utf16_lossy(&font.face_name[..len]);
    // "@Name" is the vertical-writing twin of a CJK font.
    if !name.starts_with('@') && usable_name(&name) {
        let mono = font.pitch_and_family & 0x3 == FIXED_PITCH;
        families.entry(name).and_modify(|m| *m |= mono).or_insert(mono);
    }
    1
}

/// Installed font families, alphabetically.
pub fn installed() -> Vec<FontFamily> {
    let mut families: BTreeMap<String, bool> = BTreeMap::new();
    let query = LogFontW {
        char_set: DEFAULT_CHARSET,
        ..LogFontW::default()
    };
    // SAFETY: a screen DC, released below; the callback only touches `families`, which
    // outlives the call.
    unsafe {
        let hdc = GetDC(0);
        if hdc == 0 {
            return Vec::new();
        }
        EnumFontFamiliesExW(hdc, &query, collect, &mut families as *mut _ as isize, 0);
        ReleaseDC(0, hdc);
    }
    let mut list: Vec<FontFamily> = families.into_iter().map(|(name, mono)| FontFamily { name, mono }).collect();
    list.sort_by_cached_key(|f| f.name.to_lowercase());
    list
}

/// A name that's safe to put in a quoted CSS font-family.
pub fn usable_name(name: &str) -> bool {
    !name.trim().is_empty()
        && name.chars().count() <= MAX_NAME_CHARS
        && !name
            .chars()
            .any(|c| c.is_control() || matches!(c, '"' | '\'' | '\\' | ';' | '{' | '}' | '<' | '>'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_the_windows_fonts() {
        let fonts = installed();
        assert!(fonts.iter().any(|f| f.name == "Consolas" && f.mono), "{fonts:?}");
        assert!(fonts.iter().any(|f| f.name == "Arial" && !f.mono));
        assert!(fonts.iter().all(|f| !f.name.starts_with('@')));
    }

    #[test]
    fn names_are_checked_before_they_reach_css() {
        assert!(usable_name("Cascadia Mono"));
        assert!(usable_name("\u{5fae}\u{8f6f}\u{96c5}\u{9ed1}"));
        assert!(!usable_name(""));
        assert!(!usable_name("x\"; } body { color: red"));
        assert!(!usable_name("back\\slash"));
        assert!(!usable_name(&"a".repeat(65)));
    }
}
