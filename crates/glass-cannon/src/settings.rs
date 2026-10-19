//! The settings pop-out: status checks, options, game data updates, shortcuts.

use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::process::Command;
use std::sync::Arc;

use serde::Serialize;
use tauri::{AppHandle, Manager, PhysicalPosition, State};

use crate::store::{Appearance, AppearanceChange};
use crate::{lock, notify_overlay, restyle_windows, show_overlay, AppState, OVERLAY, SETTINGS};

/// Child processes run without flashing a console window.
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
/// Gap between the overlay and the settings window, in logical pixels.
const GAP: f64 = 10.0;

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
    everyone: bool,
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
                level: "warn",
                text: "not downloaded".into(),
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
        everyone: s.everyone,
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

/// Saves the "record fights" choice. Capture is already running, so it applies the next
/// time the meter starts.
#[tauri::command]
pub fn set_record(app: State<'_, Arc<AppState>>, record: bool) {
    let mut s = lock(&app.settings);
    s.record = record;
    s.save(&app.config_dir);
}

/// Runs `packet-lab update-data` (next to this program) and reloads the data.
#[tauri::command]
pub async fn update_game_data(app: State<'_, Arc<AppState>>) -> Result<String, String> {
    let app = Arc::clone(&app);
    tauri::async_runtime::spawn_blocking(move || refresh_game_data(&app))
        .await
        .map_err(|e| e.to_string())?
}

/// Downloads the game data next to the exe and loads it. The data isn't shipped with the
/// meter (its sources are GPL-3.0 projects), so this also runs by itself on first start.
pub fn refresh_game_data(app: &AppState) -> Result<String, String> {
    app.downloading.store(true, Ordering::SeqCst);
    let result = download_game_data(&app.exe_dir);
    if result.is_ok() {
        lock(&app.data).reload();
    }
    app.downloading.store(false, Ordering::SeqCst);
    result
}

fn download_game_data(exe_dir: &Path) -> Result<String, String> {
    let exe = exe_dir.join("packet-lab.exe");
    if !exe.is_file() {
        return Err(format!("{} not found next to the meter", exe.display()));
    }
    let output = Command::new(&exe)
        .arg("update-data")
        .arg("--out")
        .arg(exe_dir.join("gamedata"))
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("couldn't run packet-lab: {e}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let reason = stderr.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("unknown error");
        return Err(reason.trim_start_matches("error: ").to_string());
    }
    Ok(stdout
        .lines()
        .find_map(|l| l.strip_prefix("done: "))
        .unwrap_or("game data updated")
        .to_string())
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
    if let (Some(o), Ok(size)) = (handle.get_webview_window(OVERLAY), s.outer_size()) {
        if let (Ok(pos), Ok(osize), Ok(Some(mon))) = (o.outer_position(), o.outer_size(), o.current_monitor()) {
            let gap = (GAP * mon.scale_factor()) as i32;
            // Beside the overlay: to its left if there's room on its monitor, else to its right.
            let left = pos.x - size.width as i32 - gap;
            let x = if left >= mon.position().x { left } else { pos.x + osize.width as i32 + gap };
            let _ = s.set_position(PhysicalPosition::new(x, pos.y));
        }
    }
    s.show().map_err(|e| e.to_string())?;
    s.set_focus().map_err(|e| e.to_string())
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
