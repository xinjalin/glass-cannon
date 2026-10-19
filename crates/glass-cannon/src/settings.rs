//! The settings pop-out: appearance, status checks, meter options, tools.

use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;

use meter_core::meter::Scope;
use serde::Serialize;
use tauri::{AppHandle, LogicalSize, Manager, Monitor, PhysicalPosition, Runtime, State, WebviewWindow};

use crate::store::{Appearance, AppearanceChange};
use crate::{lock, notify_overlay, restyle_windows, show_overlay, AppState, OVERLAY, SETTINGS};

/// Child processes run without flashing a console window.
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
/// Gap between the overlay and the settings window, in logical pixels.
const GAP: f64 = 10.0;
/// Space kept free around a window that has to shrink to fit a small screen.
const SCREEN_MARGIN: f64 = 16.0;

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
    record: bool,
    /// Whether the capture running now is recording (the switch applies on next start).
    recording_now: bool,
    scope: Scope,
    /// People in your party or group, you included (0 when on your own).
    party_size: usize,
    appearance: Appearance,
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
    let data = {
        let mut d = lock(&app.data);
        d.reload_if_empty();
        match d.skill_count() {
            0 => Check {
                level: "bad",
                text: "missing: reinstall from the release zip".into(),
            },
            n => Check {
                level: "ok",
                text: format!("{} skills · {} mobs", thousands(n), thousands(d.named_npc_count())),
            },
        }
    };
    let s = lock(&app.settings).clone();
    SettingsStatus {
        version: env!("CARGO_PKG_VERSION"),
        npcap,
        game,
        data,
        record: s.record,
        recording_now: app.recording_now.load(std::sync::atomic::Ordering::SeqCst),
        scope: s.scope,
        party_size: app.engine.meter().registry().party().len(),
        appearance: s.appearance,
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

/// Puts every appearance setting back to its default, and the overlay back to its
/// default size.
#[tauri::command]
pub fn reset_appearance(handle: AppHandle, app: State<'_, Arc<AppState>>) -> Appearance {
    let mut s = lock(&app.settings);
    s.appearance = Appearance::default();
    s.save(&app.config_dir);
    restyle_windows(&handle, &s.appearance);
    notify_overlay(&handle, "reset-size");
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

/// Opens the overlay's history list.
#[tauri::command]
pub fn open_history(handle: AppHandle) -> Result<(), String> {
    show_overlay(&handle).map_err(|e| e.to_string())?;
    notify_overlay(&handle, "show-history");
    Ok(())
}

/// Shows the settings next to the overlay, or hides them if they're already open.
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
    let (width, height) = match overlay_monitor(&handle) {
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

/// The monitor the overlay is on (or the primary one).
fn overlay_monitor<R: Runtime>(handle: &AppHandle<R>) -> Option<Monitor> {
    let o = handle.get_webview_window(OVERLAY)?;
    o.current_monitor().ok().flatten().or_else(|| o.primary_monitor().ok().flatten())
}

/// Beside the overlay (to its left if it fits, else its right), entirely within the
/// overlay's monitor, leaving out the taskbar.
fn place_settings<R: Runtime>(handle: &AppHandle<R>, s: &WebviewWindow<R>) {
    let (Some(o), Some(mon)) = (handle.get_webview_window(OVERLAY), overlay_monitor(handle)) else {
        return;
    };
    let (Ok(pos), Ok(osize), Ok(size)) = (o.outer_position(), o.outer_size(), s.outer_size()) else {
        return;
    };
    let area = mon.work_area();
    let (left_edge, top_edge) = (area.position.x, area.position.y);
    let right_edge = left_edge + area.size.width as i32;
    let bottom_edge = top_edge + area.size.height as i32;
    let (w, h) = (size.width as i32, size.height as i32);
    let gap = (GAP * mon.scale_factor()) as i32;

    let left = pos.x - w - gap;
    let right = pos.x + osize.width as i32 + gap;
    let x = if left >= left_edge {
        left
    } else if right + w <= right_edge {
        right
    } else {
        // No room on either side: overlap the overlay rather than leave the screen.
        right_edge - w
    };
    let x = x.clamp(left_edge, (right_edge - w).max(left_edge));
    let y = pos.y.clamp(top_edge, (bottom_edge - h).max(top_edge));
    let _ = s.set_position(PhysicalPosition::new(x, y));
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
