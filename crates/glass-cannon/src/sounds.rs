//! Sounds: what each alert plays. The app has a chime of its own for each (a boss warning,
//! the boss turning to you, a hidden cube, a world timer); any of them can play a sound
//! file of yours in its place, or nothing. So can a boss cast's warning (the boss casts
//! window) and an action bar (a skill ready again, an effect coming on you or ending).
//!
//! A sound file you pick (settings → sound) is copied to the `sounds` folder beside the
//! settings under a cleaned-up name, and anything that sounds can use it. The files are
//! yours alone: no export carries them. All of it is under one switch (alert sound) and one
//! volume.

use std::collections::BTreeMap;
use std::ops::RangeInclusive;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::store::Settings;
use crate::{lock, AppState};

/// A sound choice that plays nothing, and one that plays the app's chime; any other is one
/// of your sound files, by name.
pub const NONE: &str = "none";
pub const CHIME: &str = "chime";
/// The alerts with a sound to choose: a boss warning (a cast with none of its own), the
/// boss turning to you, a hidden cube, and a world timer's alert. Each plays its chime
/// unless set otherwise.
pub const ALERTS: [&str; 4] = ["boss", "on_you", "cube", "timer"];
/// How loud, in percent of full.
pub const VOLUMES: RangeInclusive<u16> = 5..=100;

/// The sound files taken in: what the overlay's browser decodes.
const SOUND_TYPES: [&str; 5] = ["wav", "mp3", "ogg", "flac", "m4a"];
/// The largest sound file taken in (an alert's sound is a second or two).
const MAX_SOUND_BYTES: usize = 2 * 1024 * 1024;
const MAX_SOUND_NAME: usize = 60;

fn sounds_dir(config_dir: &Path) -> PathBuf {
    config_dir.join("sounds")
}

/// A sound file's name as it's kept: the file's own name with nothing a path could be made
/// of (letters, digits, spaces, `-`, `_`, `(`, `)` and one `.` before its type), its type
/// one of [`SOUND_TYPES`]. None when nothing is left of it, or it isn't a sound file.
fn sound_name(name: &str) -> Option<String> {
    let file = name.rsplit(['/', '\\']).next()?;
    let (stem, ext) = file.rsplit_once('.')?;
    let ext = ext.to_ascii_lowercase();
    if !SOUND_TYPES.contains(&ext.as_str()) {
        return None;
    }
    let stem: String = stem.chars().filter(|c| c.is_alphanumeric() || matches!(c, ' ' | '-' | '_' | '(' | ')')).collect();
    let stem: String = stem.trim().chars().take(MAX_SOUND_NAME).collect();
    let stem = stem.trim_end();
    (!stem.is_empty()).then(|| format!("{stem}.{ext}"))
}

/// Whether `choice` reads as a sound choice: nothing, the chime, or a sound file's name as
/// kept (whether the file is there is another matter: one that's gone plays the chime).
pub fn is_choice(choice: &str) -> bool {
    choice == NONE || choice == CHIME || sound_name(choice).as_deref() == Some(choice)
}

/// The sound files in the folder, by name.
pub fn sound_files(config_dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(sounds_dir(config_dir))
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| sound_name(n).as_deref() == Some(n.as_str()))
        .collect();
    names.sort_by_key(|n| n.to_lowercase());
    names
}

/// Keeps a sound file's bytes in the sounds folder under `name` (cleaned up); the name it
/// has there. The same file again is the one already kept; another file of that name gets
/// a number after it, so a sound something already uses never changes under it.
fn keep_sound(config_dir: &Path, name: &str, bytes: &[u8]) -> Result<String, String> {
    let clean = sound_name(name).ok_or_else(|| format!("not a sound file this can play ({})", SOUND_TYPES.join(", ")))?;
    if bytes.is_empty() {
        return Err("that file is empty".into());
    }
    if bytes.len() > MAX_SOUND_BYTES {
        return Err(format!("that file is too big: {} MB at most", MAX_SOUND_BYTES / 1024 / 1024));
    }
    let dir = sounds_dir(config_dir);
    std::fs::create_dir_all(&dir).map_err(|e| format!("couldn't make {}: {e}", dir.display()))?;
    let (stem, ext) = clean.rsplit_once('.').expect("a sound's name has its type");
    for n in 1..100 {
        let name = if n == 1 { clean.clone() } else { format!("{stem} {n}.{ext}") };
        let path = dir.join(&name);
        match std::fs::read(&path) {
            Ok(kept) if kept == bytes => return Ok(name),
            Ok(_) => continue,
            Err(_) => {
                std::fs::write(&path, bytes).map_err(|e| format!("couldn't save {}: {e}", path.display()))?;
                return Ok(name);
            }
        }
    }
    Err("too many sounds of that name".into())
}

/// A kept sound's file, when `name` is one (a name as [`sound_name`] gives it: nothing
/// that leads out of the folder).
pub fn sound_path(config_dir: &Path, name: &str) -> Option<PathBuf> {
    (sound_name(name).as_deref() == Some(name)).then(|| sounds_dir(config_dir).join(name)).filter(|p| p.is_file())
}

/// A choice that can be saved: nothing, the chime, or a sound file that's there.
fn check_choice(config_dir: &Path, choice: &str) -> Result<(), String> {
    if choice == NONE || choice == CHIME || sound_path(config_dir, choice).is_some() {
        Ok(())
    } else {
        Err("no such sound".into())
    }
}

/// Every sound choice in the settings that names a file: the alerts', the boss casts' and
/// every class's action bars'.
fn file_choices(s: &Settings) -> impl Iterator<Item = &String> {
    let bars = s.bars.values().flat_map(|bars| bars.values()).flat_map(|b| [&b.sounds.ready, &b.sounds.gained, &b.sounds.ended]);
    s.sounds.values().chain(s.boss_skill_sounds.values()).chain(bars).filter(|c| *c != NONE && *c != CHIME)
}

/// How many things play each sound file.
fn uses(s: &Settings) -> BTreeMap<String, u32> {
    let mut n = BTreeMap::new();
    for name in file_choices(s) {
        *n.entry(name.clone()).or_default() += 1;
    }
    n
}

/// Takes a sound file out of every choice: an alert plays its chime again, a boss cast
/// its warning's sound, and an action bar nothing. Whether anything used it.
fn forget(s: &mut Settings, name: &str) -> bool {
    let before = uses(s).get(name).copied().unwrap_or(0);
    s.sounds.retain(|_, c| c != name);
    s.boss_skill_sounds.retain(|_, c| c != name);
    for bar in s.bars.values_mut().flat_map(|bars| bars.values_mut()) {
        for c in [&mut bar.sounds.ready, &mut bar.sounds.gained, &mut bar.sounds.ended] {
            if c == name {
                *c = NONE.into();
            }
        }
    }
    before > 0
}

/// Takes out every choice of a sound file that isn't in the folder (a setup taken in from
/// someone else names theirs). Whether any was.
pub fn drop_missing(s: &mut Settings, config_dir: &Path) -> bool {
    let kept = sound_files(config_dir);
    let gone: Vec<String> = uses(s).into_keys().filter(|name| !kept.contains(name)).collect();
    for name in &gone {
        forget(s, name);
    }
    !gone.is_empty()
}

/// What the overlay needs to sound the alerts.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct SoundSetup {
    /// Alerts sound at all (settings → alert sound).
    pub on: bool,
    /// In percent.
    pub volume: u16,
    /// The alerts set to something other than their chime ([`ALERTS`]).
    pub alerts: BTreeMap<String, String>,
    /// The boss casts with a sound of their own.
    pub casts: BTreeMap<u32, String>,
    /// Every sound file in use, to have ready before it's needed.
    pub files: Vec<String>,
}

pub fn setup(s: &Settings) -> SoundSetup {
    SoundSetup {
        on: s.alert_sound,
        volume: s.sound_volume.clamp(*VOLUMES.start(), *VOLUMES.end()),
        alerts: s.sounds.iter().filter(|(k, _)| ALERTS.contains(&k.as_str())).map(|(k, v)| (k.clone(), v.clone())).collect(),
        casts: s.boss_skill_sounds.clone(),
        files: uses(s).into_keys().collect(),
    }
}

/// One of your sound files, and how many things play it.
#[derive(Debug, Serialize, PartialEq)]
pub struct SoundFile {
    pub name: String,
    pub uses: u32,
}

/// The settings window's sound section.
#[derive(Debug, Serialize, PartialEq)]
pub struct SoundsView {
    pub on: bool,
    pub volume: u16,
    pub alerts: BTreeMap<String, String>,
    pub files: Vec<SoundFile>,
}

#[tauri::command]
pub fn sounds(app: State<'_, Arc<AppState>>) -> SoundsView {
    let s = lock(&app.settings);
    let used = uses(&s);
    let setup = setup(&s);
    SoundsView {
        on: setup.on,
        volume: setup.volume,
        alerts: setup.alerts,
        files: sound_files(&app.config_dir).into_iter().map(|name| SoundFile { uses: used.get(&name).copied().unwrap_or(0), name }).collect(),
    }
}

/// Takes in a sound file you picked (its name and bytes): kept in the sounds folder, for
/// anything that sounds to use. Its name there.
#[tauri::command]
pub fn add_sound(handle: AppHandle, app: State<'_, Arc<AppState>>, name: String, bytes: Vec<u8>) -> Result<String, String> {
    let kept = keep_sound(&app.config_dir, &name, &bytes)?;
    let _ = handle.emit("sounds-changed", ());
    Ok(kept)
}

/// Deletes a sound you added; what used it goes back to its chime (an action bar: to
/// nothing).
#[tauri::command]
pub fn delete_sound(handle: AppHandle, app: State<'_, Arc<AppState>>, name: String) -> Result<(), String> {
    let path = sound_path(&app.config_dir, &name).ok_or("no such sound")?;
    std::fs::remove_file(&path).map_err(|e| format!("couldn't delete {}: {e}", path.display()))?;
    let mut s = lock(&app.settings);
    if forget(&mut s, &name) {
        s.save(&app.config_dir);
    }
    drop(s);
    let _ = handle.emit("sounds-changed", ());
    Ok(())
}

/// A kept sound's bytes, to play (the overlay, and a window's ▶).
#[tauri::command]
pub fn sound_bytes(app: State<'_, Arc<AppState>>, name: String) -> Result<tauri::ipc::Response, String> {
    let path = sound_path(&app.config_dir, &name).ok_or("no such sound")?;
    std::fs::read(&path).map(tauri::ipc::Response::new).map_err(|e| format!("couldn't read {}: {e}", path.display()))
}

/// Sets what an alert plays: nothing, its chime, or one of your sounds.
#[tauri::command]
pub fn set_sound_choice(handle: AppHandle, app: State<'_, Arc<AppState>>, alert: String, sound: String) -> Result<(), String> {
    if !ALERTS.contains(&alert.as_str()) {
        return Err(format!("no alert called {alert}"));
    }
    check_choice(&app.config_dir, &sound)?;
    let mut s = lock(&app.settings);
    if sound == CHIME {
        s.sounds.remove(&alert);
    } else {
        s.sounds.insert(alert, sound);
    }
    s.save(&app.config_dir);
    drop(s);
    let _ = handle.emit("sounds-changed", ());
    Ok(())
}

/// Sets how loud the alerts are, in percent.
#[tauri::command]
pub fn set_sound_volume(handle: AppHandle, app: State<'_, Arc<AppState>>, volume: u16) {
    let mut s = lock(&app.settings);
    s.sound_volume = volume.clamp(*VOLUMES.start(), *VOLUMES.end());
    s.save(&app.config_dir);
    drop(s);
    let _ = handle.emit("sounds-changed", ());
}

/// Sets a boss cast's warning sound: one of your sounds, or as the boss warnings go
/// (None).
#[tauri::command]
pub fn set_boss_skill_sound(handle: AppHandle, app: State<'_, Arc<AppState>>, skill: u32, sound: Option<String>) -> Result<(), String> {
    if let Some(name) = &sound {
        sound_path(&app.config_dir, name).ok_or("no such sound")?;
    }
    let mut s = lock(&app.settings);
    match sound {
        Some(name) => s.boss_skill_sounds.insert(skill, name),
        None => s.boss_skill_sounds.remove(&skill),
    };
    s.save(&app.config_dir);
    drop(s);
    let _ = handle.emit("sounds-changed", ());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actionbar::BarSetup;
    use std::fs;

    #[test]
    fn your_sounds_are_kept_by_a_safe_name() {
        // The file's own name, whatever folder it came from; only sound files.
        assert_eq!(sound_name("C:\\Users\\me\\Sounds\\Air Horn (loud).WAV").as_deref(), Some("Air Horn (loud).wav"));
        assert_eq!(sound_name("../../settings.json"), None);
        assert_eq!(sound_name("..\\..\\evil.mp3").as_deref(), Some("evil.mp3"));
        assert_eq!(sound_name("a/b:c*?.ogg").as_deref(), Some("bc.ogg"));
        assert_eq!(sound_name("...mp3"), None);
        assert_eq!(sound_name("notes.txt"), None);
        assert_eq!(sound_name("noext"), None);
        // A choice: nothing, the chime, or a name as kept.
        assert!(is_choice(NONE) && is_choice(CHIME) && is_choice("Air Horn (loud).wav"));
        assert!(!is_choice("") && !is_choice("../horn.wav") && !is_choice("HORN.WAV") && !is_choice("horn"));

        let dir = std::env::temp_dir().join(format!("gc-sounds-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        assert_eq!(keep_sound(&dir, "C:\\x\\horn.wav", b"RIFFone").as_deref(), Ok("horn.wav"));
        // The same file again is the one kept; another of that name gets a number.
        assert_eq!(keep_sound(&dir, "horn.wav", b"RIFFone").as_deref(), Ok("horn.wav"));
        assert_eq!(keep_sound(&dir, "horn.wav", b"RIFFtwo").as_deref(), Ok("horn 2.wav"));
        assert_eq!(fs::read(dir.join("sounds/horn.wav")).unwrap(), b"RIFFone");
        assert!(keep_sound(&dir, "big.wav", &vec![0; MAX_SOUND_BYTES + 1]).is_err());
        assert!(keep_sound(&dir, "empty.wav", b"").is_err());
        assert!(keep_sound(&dir, "readme.txt", b"hello").is_err());
        // Only names of kept sounds lead to a file: nothing outside the folder.
        fs::write(dir.join("settings.json"), "{}").unwrap();
        fs::write(dir.join("sounds/notes.txt"), "not a sound").unwrap();
        assert_eq!(sound_files(&dir), ["horn 2.wav", "horn.wav"]);
        assert!(sound_path(&dir, "horn.wav").is_some());
        assert_eq!(sound_path(&dir, "../settings.json"), None);
        assert_eq!(sound_path(&dir, "..\\settings.json"), None);
        assert_eq!(sound_path(&dir, "notes.txt"), None);
        assert_eq!(sound_path(&dir, "gone.wav"), None);
        assert!(check_choice(&dir, "horn.wav").is_ok() && check_choice(&dir, NONE).is_ok() && check_choice(&dir, CHIME).is_ok());
        assert!(check_choice(&dir, "gone.wav").is_err());
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn what_the_overlay_sounds_and_a_deleted_file_forgotten() {
        let mut s = Settings::default();
        assert_eq!(setup(&s), SoundSetup { on: s.alert_sound, volume: 100, ..Default::default() });
        s.sounds = [("cube".to_string(), "horn.wav".to_string()), ("timer".to_string(), NONE.to_string()), ("made up".to_string(), "x.wav".to_string())].into();
        s.boss_skill_sounds = [(1_803_010, "horn.wav".to_string()), (1_803_250, "run.mp3".to_string())].into();
        let mut bar = BarSetup::default();
        bar.sounds.ready = "horn.wav".into();
        bar.sounds.gained = CHIME.into();
        s.bars.entry("18".into()).or_default().insert(1, bar);
        s.sound_volume = 250;
        let v = setup(&s);
        assert_eq!(v.volume, 100);
        assert_eq!(v.alerts, [("cube".to_string(), "horn.wav".to_string()), ("timer".to_string(), NONE.to_string())].into());
        assert_eq!(v.casts.len(), 2);
        // Each file once, whatever uses it; "none" and the chime aren't files.
        assert_eq!(v.files, ["horn.wav", "run.mp3", "x.wav"]);
        assert_eq!(uses(&s)["horn.wav"], 3);
        // Deleted: the alert chimes again, the cast goes by the boss warnings, the bar is quiet.
        assert!(forget(&mut s, "horn.wav"));
        assert!(!s.sounds.contains_key("cube") && s.sounds["timer"] == NONE);
        assert_eq!(s.boss_skill_sounds.keys().copied().collect::<Vec<_>>(), [1_803_250]);
        assert_eq!((s.bars["18"][&1].sounds.ready.as_str(), s.bars["18"][&1].sounds.gained.as_str()), (NONE, CHIME));
        assert!(!forget(&mut s, "horn.wav"));
        // A setup taken in: choices of files that aren't here go; the rest stay.
        let dir = std::env::temp_dir().join(format!("gc-sounds-missing-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        keep_sound(&dir, "run.mp3", b"ID3").unwrap();
        assert!(drop_missing(&mut s, &dir));
        assert_eq!(uses(&s).into_keys().collect::<Vec<_>>(), ["run.mp3"]);
        assert!(!drop_missing(&mut s, &dir));
        fs::remove_dir_all(dir).ok();
    }
}
