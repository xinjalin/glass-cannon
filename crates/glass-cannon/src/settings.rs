//! The settings pop-out: appearance, status checks, meter options, tools.

use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use meter_core::meter::Scope;
use serde::Serialize;
use tauri::{AppHandle, LogicalSize, Manager, Monitor, PhysicalPosition, PhysicalSize, Runtime, State, WebviewWindow};

use crate::layout::{self, Rect, Side};

use crate::store::{Appearance, AppearanceChange};
use crate::{hud, lock, restyle_windows, restyle_windows_but, AppState, HISTORY, LAUNCHER, SETTINGS};

/// Child processes run without flashing a console window.
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
/// Gap between the launcher and a window beside it, in logical pixels.
const GAP: f64 = 10.0;
/// Space kept free around a window that has to shrink to fit a small screen.
const SCREEN_MARGIN: f64 = 16.0;
/// The smallest the settings window gets to fit beside the launcher (logical pixels); its
/// content scrolls.
const MIN_SETTINGS: (f64, f64) = (420.0, 320.0);
/// The same for the history window.
const MIN_HISTORY: (f64, f64) = (320.0, 240.0);
/// Where a window goes beside the launcher, in order of preference.
pub(crate) const SIDES: [Side; 4] = [Side::Left, Side::Right, Side::Below, Side::Above];

#[derive(Serialize)]
pub struct Check {
    /// "ok", "warn" or "bad".
    level: &'static str,
    text: String,
}

#[derive(Serialize)]
pub struct SettingsStatus {
    version: &'static str,
    npcap: Check,
    game: Check,
    data: Check,
    /// Skill details for the analysis window's tooltips.
    details: Check,
    record: bool,
    /// Whether the capture running now is recording (the switch applies on next start).
    recording_now: bool,
    scope: Scope,
    /// The scope follows your party (not chosen by hand).
    auto_scope: bool,
    /// People in your party or group, you included (0 when on your own).
    party_size: usize,
    appearance: Appearance,
    refresh_ms: u32,
    boss_hp: bool,
    boss_casts: bool,
    pet_souls: bool,
    /// The overlay's widgets that show.
    widgets: Vec<String>,
    alert_sound: bool,
    hide_after_s: u32,
}

#[tauri::command]
pub fn settings_status(app: State<'_, Arc<AppState>>) -> SettingsStatus {
    let npcap = match meter_core::npcap::Npcap::load() {
        Ok(np) => Check {
            level: "ok",
            text: np
                .version()
                .split(',')
                .next()
                .unwrap_or("ready")
                .trim()
                .trim_start_matches("Npcap version ")
                .to_string(),
        },
        Err(_) => Check {
            level: "bad",
            text: "not installed (npcap.com)".into(),
        },
    };
    let game = match meter_core::conns::find_processes(&app.args.process) {
        Ok(p) if !p.is_empty() => Check { level: "ok", text: "running".into() },
        _ => Check {
            level: "warn",
            text: "not running".into(),
        },
    };
    let (data, details) = {
        let mut d = lock(&app.data);
        d.reload_if_empty();
        let data = match d.skill_count() {
            0 => Check {
                level: "bad",
                text: "missing: reinstall from the release zip".into(),
            },
            n => Check {
                level: "ok",
                text: format!("{} skills · {} mobs", thousands(n), thousands(d.named_npc_count())),
            },
        };
        // Cooldowns, descriptions and specialties for the analysis tooltips: the meter
        // works without them, so a missing file is only a warning.
        let details = match (d.detail_count(), d.theostone_count()) {
            (0, _) => Check {
                level: "warn",
                text: "missing: no skill tooltips".into(),
            },
            (n, 0) => Check {
                level: "ok",
                text: format!("{} skills", thousands(n)),
            },
            (n, t) => Check {
                level: "ok",
                text: format!("{} skills · {t} theostones", thousands(n)),
            },
        };
        (data, details)
    };
    let s = lock(&app.settings).clone();
    // Once: a second lock of the meter in the same expression would wait on the first.
    let party_size = app.engine.meter().registry().party().len();
    SettingsStatus {
        version: env!("CARGO_PKG_VERSION"),
        npcap,
        game,
        data,
        details,
        record: s.record,
        recording_now: app.recording_now.load(std::sync::atomic::Ordering::SeqCst),
        scope: s.scope_for(party_size),
        auto_scope: !s.manual_scope,
        party_size,
        appearance: s.appearance,
        refresh_ms: s.refresh_ms,
        boss_hp: s.boss_hp,
        boss_casts: s.boss_casts,
        pet_souls: s.pet_souls,
        widgets: s.widgets,
        alert_sound: s.alert_sound,
        hide_after_s: s.hide_after_s,
    }
}

/// Changes how the windows look; saved for next time. Missing values stay as they are.
#[tauri::command]
pub fn set_appearance(handle: AppHandle, app: State<'_, Arc<AppState>>, change: AppearanceChange) -> Appearance {
    let mut s = lock(&app.settings);
    s.appearance.update(change);
    s.save(&app.config_dir);
    restyle_windows(&handle, &s.appearance);
    s.appearance.clone()
}

/// A colour being picked in the settings window: saved and shown in every window as it
/// changes. The settings window's own frame isn't restyled, since that closes its colour
/// picker; [`set_appearance`] does it once the picker is closed.
#[tauri::command]
pub fn pick_colour(handle: AppHandle, app: State<'_, Arc<AppState>>, change: AppearanceChange) -> Appearance {
    let mut s = lock(&app.settings);
    s.appearance.update(change);
    s.save(&app.config_dir);
    restyle_windows_but(&handle, &s.appearance, Some(SETTINGS));
    s.appearance.clone()
}

/// Puts every appearance setting back to its default (the overlay's layout has its own
/// reset, while arranging it).
#[tauri::command]
pub fn reset_appearance(handle: AppHandle, app: State<'_, Arc<AppState>>) -> Appearance {
    let mut s = lock(&app.settings);
    s.appearance = Appearance::default();
    s.save(&app.config_dir);
    restyle_windows(&handle, &s.appearance);
    s.appearance.clone()
}

/// Installed font families for the font picker.
#[tauri::command]
pub fn list_fonts() -> Vec<crate::fonts::FontFamily> {
    crate::fonts::installed()
}

/// Saves the "record fights" choice. Capture is already running, so it applies the next
/// time the meter starts.
#[tauri::command]
pub fn set_record(app: State<'_, Arc<AppState>>, record: bool) {
    let mut s = lock(&app.settings);
    s.record = record;
    s.save(&app.config_dir);
}

/// Adds "Glass Cannon" to the Desktop and the Start menu.
#[tauri::command]
pub async fn create_shortcuts() -> Result<String, String> {
    let exe: PathBuf = std::env::current_exe().map_err(|e| e.to_string())?;
    // The path goes in through an environment variable, never spliced into the script.
    const SCRIPT: &str = r#"
$exe = $env:GLASS_CANNON_EXE
$shell = New-Object -ComObject WScript.Shell
foreach ($dir in @([Environment]::GetFolderPath('Desktop'), [Environment]::GetFolderPath('Programs'))) {
  $link = $shell.CreateShortcut((Join-Path $dir 'Glass Cannon.lnk'))
  $link.TargetPath = $exe
  $link.WorkingDirectory = Split-Path $exe
  $link.IconLocation = "$exe,0"
  $link.Description = 'Glass Cannon: DPS meter for Aion 2'
  $link.Save()
}
"#;
    let output = tauri::async_runtime::spawn_blocking(move || {
        Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", SCRIPT])
            .env("GLASS_CANNON_EXE", &exe)
            .creation_flags(CREATE_NO_WINDOW)
            .output()
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| format!("couldn't run PowerShell: {e}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    Ok("added to Desktop and Start menu".into())
}

/// Shows the fight history (the first time where you left it, else beside the launcher),
/// or hides it if it's showing.
#[tauri::command]
pub fn open_history(handle: AppHandle, app: State<'_, Arc<AppState>>) -> Result<(), String> {
    let w = handle.get_webview_window(HISTORY).ok_or("no history window")?;
    if w.is_visible().unwrap_or(false) && !w.is_minimized().unwrap_or(false) {
        return w.hide().map_err(|e| e.to_string());
    }
    if !app.history_placed.swap(true, Ordering::SeqCst) {
        let saved = lock(&app.placements).get(HISTORY);
        if !saved.is_some_and(|p| crate::placement::restore(&w, p, true)) {
            if let Ok(size) = w.outer_size() {
                place_beside(&handle, &w, (size.width as i32, size.height as i32), MIN_HISTORY, &SIDES);
            }
        }
    }
    w.show().map_err(|e| e.to_string())?;
    let _ = w.unminimize();
    w.set_focus().map_err(|e| e.to_string())
}

/// Shows the settings beside the launcher, or hides them if they're already open.
#[tauri::command]
pub fn toggle_settings(handle: AppHandle) -> Result<(), String> {
    let s = handle.get_webview_window(SETTINGS).ok_or("no settings window")?;
    if s.is_visible().unwrap_or(false) {
        return s.hide().map_err(|e| e.to_string());
    }
    place_settings(&handle, &s);
    s.show().map_err(|e| e.to_string())?;
    s.set_focus().map_err(|e| e.to_string())
}

/// Sizes the settings window to its content. The page asks whenever the font size
/// changes; on a small screen the window shrinks to fit and its content scrolls.
#[tauri::command]
pub fn fit_settings(handle: AppHandle, width: f64, height: f64) -> Result<(), String> {
    let s = handle.get_webview_window(SETTINGS).ok_or("no settings window")?;
    let (width, height) = match anchor_monitor(&handle) {
        Some(m) => {
            let area = m.work_area().size.to_logical::<f64>(m.scale_factor());
            (
                width.min(area.width - 2.0 * SCREEN_MARGIN),
                height.min(area.height - 2.0 * SCREEN_MARGIN),
            )
        }
        None => (width, height),
    };
    s.set_size(LogicalSize::new(width, height)).map_err(|e| e.to_string())?;
    if s.is_visible().unwrap_or(false) {
        place_settings(&handle, &s);
    }
    Ok(())
}

/// Whether the overlay is over the game now (then windows open on the game's screen).
fn in_game<R: Runtime>(handle: &AppHandle<R>) -> bool {
    handle.state::<Arc<AppState>>().hud_shown.load(Ordering::SeqCst)
}

/// The screen windows open on: the game's while the overlay is over it, else the
/// launcher's (or the primary one).
pub(crate) fn anchor_monitor<R: Runtime>(handle: &AppHandle<R>) -> Option<Monitor> {
    let o = handle.get_webview_window(if in_game(handle) { hud::HUD } else { LAUNCHER })?;
    o.current_monitor().ok().flatten().or_else(|| o.primary_monitor().ok().flatten())
}

/// Where you left it, else beside the launcher (its left if there's room, else its right,
/// below or above; in game, in the middle of the game's screen), entirely within that
/// screen, leaving out the taskbar. On a screen too small for that it shrinks (its content
/// scrolls) before it would cover the launcher.
fn place_settings<R: Runtime>(handle: &AppHandle<R>, s: &WebviewWindow<R>) {
    // Where you left it (its size follows its content), else beside the meter.
    let saved = lock(&handle.state::<Arc<AppState>>().placements).get(SETTINGS);
    if saved.is_some_and(|p| crate::placement::restore(s, p, false)) {
        return;
    }
    let Some(size) = s.outer_size().ok() else { return };
    let size = (size.width as i32, size.height as i32);
    place_beside(handle, s, size, MIN_SETTINGS, &SIDES);
}

/// Puts `w` (wanting `size` physical pixels, shrinkable to `min` logical ones) beside the
/// launcher on its monitor, in the order of `sides`; in game, in the middle of the game's
/// screen. Returns false when neither can be found.
pub(crate) fn place_beside<R: Runtime>(
    handle: &AppHandle<R>,
    w: &WebviewWindow<R>,
    size: (i32, i32),
    min: (f64, f64),
    sides: &[Side],
) -> bool {
    let Some(mon) = anchor_monitor(handle) else { return false };
    let area = work_area(&mon);
    let r = if in_game(handle) {
        let middle = Rect::new(area.x + (area.w - size.0) / 2, area.y + (area.h - size.1) / 2, size.0, size.1);
        layout::keep_inside(middle, area)
    } else {
        let Some(launcher) = handle.get_webview_window(LAUNCHER).and_then(|l| outer_rect(&l)) else {
            return false;
        };
        let scale = mon.scale_factor();
        let gap = (GAP * scale) as i32;
        let min = ((min.0 * scale) as i32, (min.1 * scale) as i32);
        layout::beside(area, launcher, size, min, gap, sides)
    };
    let (Ok(outer), Ok(inner)) = (w.outer_size(), w.inner_size()) else { return false };
    if (r.w, r.h) != (outer.width as i32, outer.height as i32) {
        // Sizes here are the window's outside; set_size takes the inside, so take off the
        // frame (or every placement would grow the window by it).
        let frame = (outer.width as i32 - inner.width as i32, outer.height as i32 - inner.height as i32);
        let _ = w.set_size(PhysicalSize::new((r.w - frame.0).max(1) as u32, (r.h - frame.1).max(1) as u32));
    }
    let _ = w.set_position(PhysicalPosition::new(r.x, r.y));
    true
}

/// A window's outer rectangle in physical pixels.
pub(crate) fn outer_rect<R: Runtime>(w: &WebviewWindow<R>) -> Option<Rect> {
    let (p, s) = (w.outer_position().ok()?, w.outer_size().ok()?);
    Some(Rect::new(p.x, p.y, s.width as i32, s.height as i32))
}

/// A monitor's work area (the screen less the taskbar) in physical pixels.
pub(crate) fn work_area(m: &Monitor) -> Rect {
    let a = m.work_area();
    Rect::new(a.position.x, a.position.y, a.size.width as i32, a.size.height as i32)
}

#[tauri::command]
pub fn hide_settings(handle: AppHandle) {
    if let Some(s) = handle.get_webview_window(SETTINGS) {
        let _ = s.hide();
    }
}

fn thousands(n: usize) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn thousands_separators() {
        assert_eq!(super::thousands(7105), "7,105");
        assert_eq!(super::thousands(923), "923");
        assert_eq!(super::thousands(1_234_567), "1,234,567");
    }
}
