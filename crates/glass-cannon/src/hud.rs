//! The overlay (HUD): one transparent window over the game's picture that holds the
//! widgets (the meter, boss alerts…). It follows the game window, shows only while the game
//! (or one of our windows) is in front, never takes the focus from the game, and lets the
//! mouse through except over a widget. The launcher turns it on and off; settings, history
//! and analysis are windows. Replaying a recording, it covers the launcher's screen instead.
//!
//! Like Questlog's overlay: the page reports where its widgets are, and a watcher checks
//! the pointer against them and makes the window take the mouse only there.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use serde::Deserialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, Runtime, State, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

use crate::game_window::{Front, GameWindow, Rect, Tracker};
use crate::{lock, AppState, LAUNCHER};

pub const HUD: &str = "hud";
const LAYOUT_FILE: &str = "hud.json";
/// How often the pointer is checked while the HUD shows, and the game window looked for
/// (every few checks).
const WATCH: Duration = Duration::from_millis(50);
const FIND_EVERY: u32 = 4;
const WATCH_HIDDEN: Duration = Duration::from_millis(250);

/// A widget's box in the HUD's page, in physical pixels from its top left.
#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq)]
pub struct HitRect {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

impl HitRect {
    fn contains(&self, x: f64, y: f64) -> bool {
        (self.x..=self.x + self.width).contains(&x) && (self.y..=self.y + self.height).contains(&y)
    }
}

/// Where the widgets are; the HUD reports it whenever its layout changes: those that take
/// the mouse, every widget's panel (for its frosted plate), and every widget on screen
/// (left out of the corner the menu check reads).
#[tauri::command]
pub fn hud_rects(app: State<'_, Arc<AppState>>, rects: Vec<HitRect>, panels: Option<Vec<HitRect>>, drawn: Option<Vec<HitRect>>, meter: Option<HitRect>) {
    *lock(&app.hud_rects) = rects;
    *lock(&app.hud_panels) = panels.unwrap_or_default();
    *lock(&app.hud_drawn) = drawn.unwrap_or_default();
    *lock(&app.hud_meter) = meter;
}

/// How long the game's cursor stays hidden (you're back in the game) before you're taken to
/// have left the merchant: a moment's hiding (a loading screen) doesn't count.
const MERCHANT_LEFT_AFTER: Duration = Duration::from_millis(1000);
/// While the cursor shows, how often the top-right corner is looked at for the merchant's
/// close X, and how many looks in a row without it (once seen) mean you've left.
const MERCHANT_LOOK_EVERY: Duration = Duration::from_millis(500);
const MERCHANT_GONE_LOOKS: u8 = 2;

/// Telling that you've left the merchant. The cursor alone can't: the black market window
/// comes up in the middle of the screen, where the game keeps its hidden cursor, and over
/// our window it shows. So, while it shows, the top-right corner is looked at for the
/// merchant's close X (as for game menus): gone from there, once seen, you've left.
#[derive(Debug, Default)]
struct MerchantWatch {
    playing_since: Option<std::time::Instant>,
    looked_at: Option<std::time::Instant>,
    seen_mark: bool,
    misses: u8,
    /// Looks in a row with the Shop's second sign showing (see `menus::read_shop_sign`).
    shop_looks: u8,
}

impl MerchantWatch {
    /// `look`: whether the corner shows a menu's mark, and whether the Shop's second sign
    /// shows (each None: couldn't tell). The merchant's Exit goes back to the Shop, which
    /// has the X too: there, it's the sign that tells.
    fn left(&mut self, cursor: bool, look: impl FnOnce() -> (Option<bool>, Option<bool>)) -> bool {
        if !cursor {
            let since = *self.playing_since.get_or_insert_with(std::time::Instant::now);
            return since.elapsed() >= MERCHANT_LEFT_AFTER;
        }
        self.playing_since = None;
        if self.looked_at.is_some_and(|t| t.elapsed() < MERCHANT_LOOK_EVERY) {
            return false;
        }
        self.looked_at = Some(std::time::Instant::now());
        let (corner, shop) = look();
        match corner {
            Some(true) => {
                self.seen_mark = true;
                self.misses = 0;
            }
            Some(false) if self.seen_mark => self.misses += 1,
            _ => {}
        }
        self.shop_looks = if shop == Some(true) { self.shop_looks + 1 } else { 0 };
        self.misses >= MERCHANT_GONE_LOOKS || self.shop_looks >= MERCHANT_GONE_LOOKS
    }
}

/// Whether the Shop's second sign shows at the game's top left (you've left the merchant
/// for the Shop), our widgets left out; None when the black market window covers it.
fn merchant_shop_sign<R: Runtime>(handle: &AppHandle<R>, app: &AppState, area: Rect) -> Option<bool> {
    let (bx, by, bw, bh) = crate::menus::shop_sign_band(area.height());
    if let Some(bm) = handle.get_webview_window(crate::BLACK_MARKET).filter(|w| w.is_visible().unwrap_or(false)) {
        let (Ok(at), Ok(size)) = (bm.outer_position(), bm.outer_size()) else { return None };
        let (l, t) = (at.x - area.left, at.y - area.top);
        let (r, b) = (l + size.width as i32, t + size.height as i32);
        if r > bx && l < bx + bw && b > by && t < by + bh {
            return None;
        }
    }
    let skip = widgets_in_strip(&lock(&app.hud_drawn), f64::from(bx), f64::from(by));
    crate::menus::read_shop_sign((area.left, area.top, area.width(), area.height()), &skip)
}

/// Whether the game's top-right corner shows a menu's close X or back arrow, our widgets
/// left out; None when the black market window covers it, or a pop-up is up without the X
/// showing through its dimming (it can't be told then: buying asks first).
fn merchant_corner<R: Runtime>(handle: &AppHandle<R>, app: &AppState, area: Rect) -> Option<bool> {
    let mut corner = crate::menus::read_corner((area.left, area.top, area.width(), area.height()))?;
    let side = corner.side() as f64;
    if let Some(bm) = handle.get_webview_window(crate::BLACK_MARKET).filter(|w| w.is_visible().unwrap_or(false)) {
        let (Ok(at), Ok(size)) = (bm.outer_position(), bm.outer_size()) else { return None };
        let right = f64::from(area.right);
        let (l, t, r, b) = (f64::from(at.x), f64::from(at.y), f64::from(at.x) + f64::from(size.width), f64::from(at.y) + f64::from(size.height));
        if r > right - side && l < right && b > f64::from(area.top) && t < f64::from(area.top) + side {
            return None;
        }
    }
    corner.blank(&widgets_in_corner(&lock(&app.hud_drawn), f64::from(area.width()) - side, side));
    let height = f64::from(area.height());
    if corner.has_menu_mark(height) || corner.has_dimmed_close_x(height) {
        return Some(true);
    }
    let (w, h) = (area.width(), area.height());
    if let Some(mut band) = crate::menus::read_band((area.left, area.top, w, h)) {
        let (bx, by, _, _) = crate::menus::band_box(w, h);
        band.blank(&widgets_in_strip(&lock(&app.hud_drawn), f64::from(bx), f64::from(by)));
        if band.has_popup_bar(height) {
            return None;
        }
    }
    Some(false)
}

/// How far past its box a widget's glow and shadow can reach, in physical pixels.
const WIDGET_BLEED: f64 = 8.0;

/// The widgets' boxes in the menu check's corner square (its top left at `corner_x` across
/// the game's picture), each grown by its glow: (left, top, right, bottom).
fn widgets_in_corner(drawn: &[HitRect], corner_x: f64, side: f64) -> Vec<(f64, f64, f64, f64)> {
    drawn
        .iter()
        .map(|r| (r.x - WIDGET_BLEED - corner_x, r.y - WIDGET_BLEED, r.x + r.width + WIDGET_BLEED - corner_x, r.y + r.height + WIDGET_BLEED))
        .filter(|&(l, t, r, b)| r > 0.0 && b > 0.0 && l < side && t < side)
        .collect()
}

/// The widgets' boxes in the bottom strip the menu check reads (its top left at `ox`,
/// `oy` in the game's picture), each grown by its glow. Blanking clips them to the strip.
fn widgets_in_strip(drawn: &[HitRect], ox: f64, oy: f64) -> Vec<(f64, f64, f64, f64)> {
    drawn
        .iter()
        .map(|r| (r.x - WIDGET_BLEED - ox, r.y - WIDGET_BLEED - oy, r.x + r.width + WIDGET_BLEED - ox, r.y + r.height + WIDGET_BLEED - oy))
        .filter(|&(_, _, r, b)| r > 0.0 && b > 0.0)
        .collect()
}

/// Arranging the widgets: the whole HUD takes the mouse (and the keyboard) until done.
#[tauri::command]
pub fn hud_edit<R: Runtime>(handle: AppHandle<R>, app: State<'_, Arc<AppState>>, on: bool) {
    app.hud_editing.store(on, Ordering::SeqCst);
    if let Some(w) = handle.get_webview_window(HUD) {
        let _ = w.set_focusable(on);
    }
    let _ = handle.emit("hud-edit", on);
}

/// The overlay on or off (the launcher's button); saved for next time.
#[tauri::command]
pub fn set_overlay<R: Runtime>(handle: AppHandle<R>, app: State<'_, Arc<AppState>>, on: bool) {
    // Starting or stopping it from the launcher brings back widgets the hotkey hid.
    app.hud_hidden.store(false, Ordering::SeqCst);
    let mut s = lock(&app.settings);
    s.overlay_on = on;
    s.save(&app.config_dir);
    drop(s);
    if !on {
        hud_edit(handle, app, false);
    }
}

/// The show/hide hotkey: hides the widgets for now, or brings them back; with the
/// overlay off, turns it on (so the hotkey never leaves you without them). Never saved:
/// the next start shows them again.
pub fn show_hide(handle: &AppHandle, app: &AppState) {
    let on = lock(&app.settings).overlay_on;
    if !on {
        set_overlay(handle.clone(), handle.state::<Arc<AppState>>(), true);
        return;
    }
    let hidden = !app.hud_hidden.load(Ordering::SeqCst);
    if hidden && app.hud_editing.load(Ordering::SeqCst) {
        return;
    }
    app.hud_hidden.store(hidden, Ordering::SeqCst);
}

/// The widgets' places and sizes, as the HUD saved them (`hud.json`; empty at first).
#[tauri::command]
pub fn hud_layout(app: State<'_, Arc<AppState>>) -> Value {
    read_layout(&app.config_dir)
}

#[tauri::command]
pub fn set_hud_layout(app: State<'_, Arc<AppState>>, layout: Value) {
    write_layout(&app.config_dir, &layout);
}

/// A layout as kept: each widget's `x`, `y`, `w` and `h` when they're numbers, and the grid.
/// What someone's setup (or a hand-edited file) has that isn't is dropped, so the widget
/// takes its own place.
pub fn clean_layout(layout: &Value) -> Value {
    let Some(all) = layout.as_object() else { return Value::Object(serde_json::Map::new()) };
    let mut out = serde_json::Map::new();
    for (name, v) in all {
        if name == "_grid" {
            if v.as_f64().is_some_and(|g| g.is_finite() && (0.0..=200.0).contains(&g)) {
                out.insert(name.clone(), v.clone());
            }
            continue;
        }
        let Some(place) = v.as_object() else { continue };
        let kept: serde_json::Map<String, Value> = place
            .iter()
            .filter(|(k, n)| ["x", "y", "w", "h"].contains(&k.as_str()) && n.as_f64().is_some_and(|n| n.is_finite() && (0.0..=20_000.0).contains(&n)))
            .map(|(k, n)| (k.clone(), n.clone()))
            .collect();
        if !kept.is_empty() && name.len() <= 64 {
            out.insert(name.clone(), Value::Object(kept));
        }
    }
    Value::Object(out)
}

/// Saves the widgets' places and sizes (hud.json).
pub fn write_layout(dir: &Path, layout: &Value) {
    let path = layout_path(dir);
    if let Some(dir) = path.parent() {
        let _ = fs::create_dir_all(dir);
    }
    if let Ok(s) = serde_json::to_vec_pretty(&layout) {
        let tmp = path.with_extension("json.tmp");
        if fs::write(&tmp, s).is_ok() {
            let _ = fs::rename(&tmp, &path);
        }
    }
}

fn layout_path(dir: &Path) -> PathBuf {
    dir.join(LAYOUT_FILE)
}

pub fn read_layout(dir: &Path) -> Value {
    fs::read(layout_path(dir)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or(Value::Null)
}

/// Keeps the HUD over the game's picture and its click-through in step with the pointer.
/// Runs for the life of the app. Never holds a lock while it talks to the window.
pub fn watch<R: Runtime>(handle: AppHandle<R>, app: Arc<AppState>) {
    thread::Builder::new()
        .name("hud".into())
        .spawn(move || {
            let mut tracker = Tracker::new(&app.args.process);
            let (mut shown, mut placed, mut ignoring): (bool, Option<Rect>, Option<bool>) = (false, None, None);
            let mut show_wanted = false;
            // What the page was last told about the world map.
            let mut map_sent: Option<bool> = None;
            // What it was last told about keeping the meter over a lobby.
            let mut lobby_sent: Option<bool> = None;
            let mut plates = Plates::default();
            let mut tick = 0u32;
            // The game itself in front (not one of our windows), for the menu check.
            let mut game_in_front = false;
            let mut menus = crate::menus::MenuDetector::default();
            // Watching for you leaving the merchant.
            let mut merchant = MerchantWatch::default();
            loop {
                let Some(w) = handle.get_webview_window(HUD) else { return };
                let on = lock(&app.settings).overlay_on;
                let editing = app.hud_editing.load(Ordering::SeqCst);
                if tick.is_multiple_of(FIND_EVERY) || !shown {
                    let game = if app.args.replay.is_some() { replay_screen(&handle) } else { tracker.find() };
                    app.game_found.store(game.is_some(), Ordering::SeqCst);
                    game_in_front = game.is_some_and(|g| g.front == crate::game_window::Front::Game);
                    show_wanted = on && (game.is_some_and(|g| g.shows_overlay()) || (editing && shown));
                    if let Some(g) = game.filter(|_| show_wanted) {
                        if placed != Some(g.client) {
                            let _ = w.set_position(PhysicalPosition::new(g.client.left, g.client.top));
                            let _ = w.set_size(PhysicalSize::new(g.client.width() as u32, g.client.height() as u32));
                            placed = Some(g.client);
                        }
                    }
                }
                tick = tick.wrapping_add(1);
                if show_wanted != shown {
                    let ok = if show_wanted { w.show() } else { w.hide() };
                    if ok.is_ok() {
                        shown = show_wanted;
                        app.hud_shown.store(shown, Ordering::SeqCst);
                        if shown {
                            let _ = w.set_always_on_top(true);
                        }
                    }
                }
                // The world map: the widgets step aside in place, and their plates with them.
                // The window stays up, so they all come back at once (a window shown again
                // redraws each widget in its own time).
                // A game menu (settings → hide for game menus): as the map, from the cursor
                // with a menu's close X, or a window's back arrow, at the top right (see `menus`).
                // Leaving the merchant: its visit is over, and the black market window that came
                // up with it goes.
                if crate::settings::at_black_market(&app) && game_in_front {
                    let look = || placed.map_or((None, None), |area| (merchant_corner(&handle, &app, area), merchant_shop_sign(&handle, &app, area)));
                    if merchant.left(crate::menus::cursor_showing(), look) {
                        crate::settings::black_market_left(&handle, &app);
                        merchant = MerchantWatch::default();
                    }
                } else {
                    merchant.playing_since = None;
                }
                let menu_check = shown && !editing && game_in_front && app.args.replay.is_none() && lock(&app.settings).hide_for_menus;
                let menu = match placed.filter(|_| menu_check) {
                    Some(area) => {
                        let mut look = || {
                            let Some(mut corner) = crate::menus::read_corner((area.left, area.top, area.width(), area.height())) else { return false };
                            let height = f64::from(area.height());
                            // The screen there shows our own widgets over the game's: those
                            // aren't the game's, so they're left out.
                            let side = corner.side() as f64;
                            corner.blank(&widgets_in_corner(&lock(&app.hud_drawn), f64::from(area.width()) - side, side));
                            let byte = |v: f32| v.clamp(0.0, 255.0).round() as u8;
                            let (x, _) = corner.x_score(height);
                            let arrow = corner.arrow_score(height);
                            {
                                let mut checks = lock(&app.menu_checks);
                                checks.push_back((byte(x), byte(arrow)));
                                while checks.len() > 6 {
                                    checks.pop_front();
                                }
                            }
                            if corner.has_menu_mark(height) {
                                return true;
                            }
                            // A reward screen (a Monster Cube's, Shugo's…): no mark in the
                            // corner, a gold Confirm button at the bottom middle.
                            let (w, h) = (area.width(), area.height());
                            if let Some(mut strip) = crate::menus::read_bottom((area.left, area.top, w, h)) {
                                let (ox, oy) = crate::menus::bottom_origin(w, h);
                                strip.blank(&widgets_in_strip(&lock(&app.hud_drawn), ox, oy));
                                if strip.has_confirm_button(height) {
                                    return true;
                                }
                            }
                            // A pop-up over the game (a banner at login): the rest dimmed, its
                            // bar flat grey.
                            if let Some(mut band) = crate::menus::read_band((area.left, area.top, w, h)) {
                                let (bx, by, _, _) = crate::menus::band_box(w, h);
                                band.blank(&widgets_in_strip(&lock(&app.hud_drawn), f64::from(bx), f64::from(by)));
                                if band.has_popup_bar(height) {
                                    return true;
                                }
                            }
                            // A window that isn't full screen (Attendance…): its close X on
                            // its title bar, wherever it is in the top of the screen.
                            let Some(mut top) = crate::menus::read_top((area.left, area.top, w, h)) else { return false };
                            let (ox, _, _) = crate::menus::top_box(w, h);
                            top.blank(&widgets_in_strip(&lock(&app.hud_drawn), f64::from(ox), 0.0));
                            top.has_window_close(height)
                        };
                        menus.update(crate::hotkeys::now_ms(), crate::menus::cursor_showing(), &mut look)
                    }
                    None => false,
                };
                // Hidden by the hotkey: as for a menu, until it's pressed again. (The world map
                // is a menu here, by its close X: the game's asking for the map's contents,
                // as it was told by before, also comes now and then mid-fight, and the
                // widgets flickered with it.)
                let hidden = app.hud_hidden.load(Ordering::SeqCst);
                let map = shown && !editing && (menu || hidden);
                *lock(&app.step_aside) = match () {
                    _ if !map => "",
                    _ if hidden => "hotkey",
                    _ => "menu",
                };
                if map_sent != Some(map) && handle.emit_to(HUD, "hud-map", map).is_ok() {
                    map_sent = Some(map);
                }
                // A menu over a party-finder lobby (its group not started): the meter stays,
                // showing the party, to check everyone's combat power and gear score there.
                let lobby = map && !hidden && menu && in_lobby(&app);
                app.lobby_menu.store(lobby, Ordering::SeqCst);
                if lobby_sent != Some(lobby) && handle.emit_to(HUD, "hud-keep-meter", lobby).is_ok() {
                    lobby_sent = Some(lobby);
                }
                let frosted = lock(&app.settings).appearance.backdrop == "frosted";
                // Over a lobby's menu only the meter shows, and keeps its plate (without it, a
                // frosted meter is only its light tint, and looks like another theme).
                let wanted: Vec<HitRect> = match () {
                    _ if !shown || !frosted => Vec::new(),
                    _ if !map => lock(&app.hud_panels).clone(),
                    _ if lobby => lock(&app.hud_meter).iter().copied().collect(),
                    _ => Vec::new(),
                };
                plates.update(&handle, &w, &wanted);
                if shown {
                    // The map takes every click while it's open; so does the game with
                    // click-through on (settings), but while arranging.
                    let through = lock(&app.settings).click_through;
                    let ignore = !editing && (map || through || !pointer_on_widget(&handle, &w, &app));
                    if ignoring != Some(ignore) && w.set_ignore_cursor_events(ignore).is_ok() {
                        ignoring = Some(ignore);
                    }
                }
                thread::sleep(if shown { WATCH } else { WATCH_HIDDEN });
            }
        })
        .expect("starting the HUD thread");
}

/// In a party-finder group that hasn't started: someone in it has a ready mark (the game
/// sends them only then).
pub fn in_lobby(app: &AppState) -> bool {
    app.engine.meter().registry().party().iter().any(|m| m.ready.is_some())
}

/// Replaying: the launcher's screen stands in for the game's picture.
fn replay_screen<R: Runtime>(handle: &AppHandle<R>) -> Option<GameWindow> {
    let m = handle.get_webview_window(LAUNCHER)?.current_monitor().ok().flatten()?;
    let (p, s) = (m.position(), m.size());
    let r = Rect { left: p.x, top: p.y, right: p.x + s.width as i32, bottom: p.y + s.height as i32 };
    Some(GameWindow { window: r, client: r, minimized: false, front: Front::Game })
}

/// "Frosted" in the overlay: the HUD is one window over the whole game, which Windows
/// can't blur under just its widgets, so each widget gets a plate: a small empty window
/// with Windows' frosted backdrop, just under the HUD, exactly behind the widget's panel,
/// letting the mouse through. The widget's own tint is then lighter, as the windows' was.
struct Plates<R: Runtime> {
    windows: Vec<WebviewWindow<R>>,
    /// Where each is (physical pixels), None while parked.
    at: Vec<Option<(i32, i32, u32, u32)>>,
    /// Shown once already (from then on they're only moved).
    shown: Vec<bool>,
}

impl<R: Runtime> Default for Plates<R> {
    fn default() -> Self {
        Self { windows: Vec::new(), at: Vec::new(), shown: Vec::new() }
    }
}

/// Where a plate waits while it isn't wanted (the map open, the game out of front): off
/// every screen. Moved there rather than hidden, because a hidden plate shown again has
/// Windows build its blur anew, which flashes.
const PARKED: PhysicalPosition<i32> = PhysicalPosition { x: -20_000, y: -20_000 };

impl<R: Runtime> Plates<R> {
    fn update(&mut self, handle: &AppHandle<R>, hud: &WebviewWindow<R>, wanted: &[HitRect]) {
        let origin = hud.inner_position().ok();
        while self.windows.len() < wanted.len() {
            let label = format!("plate-{}", self.windows.len());
            let built = WebviewWindowBuilder::new(handle, &label, WebviewUrl::App("plate.html".into()))
                .title("Glass Cannon")
                .decorations(false)
                .transparent(true)
                .shadow(false)
                .resizable(false)
                .skip_taskbar(true)
                .always_on_top(true)
                .focusable(false)
                .visible(false)
                .build();
            let Ok(w) = built else { return };
            crate::style::plate(&w);
            let _ = w.set_ignore_cursor_events(true);
            // Its blur can bring it to the front: under the HUD.
            below(&w, hud);
            self.windows.push(w);
            self.at.push(None);
            self.shown.push(false);
        }
        for (i, w) in self.windows.iter().enumerate() {
            let want = origin.and_then(|o| wanted.get(i).map(|r| (o.x + r.x as i32, o.y + r.y as i32, r.width as u32, r.height as u32)));
            if want == self.at[i] {
                continue;
            }
            match want {
                Some((x, y, width, height)) if width > 0 && height > 0 => {
                    let _ = w.set_position(PhysicalPosition::new(x, y));
                    let _ = w.set_size(PhysicalSize::new(width, height));
                    if !self.shown[i] {
                        self.shown[i] = w.show().is_ok();
                    }
                    below(w, hud);
                }
                _ => {
                    let _ = w.set_position(PARKED);
                }
            }
            self.at[i] = want;
        }
    }
}

/// Puts `plate` just under `hud` (both stay on top of the game).
fn below<R: Runtime>(plate: &WebviewWindow<R>, hud: &WebviewWindow<R>) {
    #[link(name = "user32")]
    extern "system" {
        fn SetWindowPos(hwnd: isize, after: isize, x: i32, y: i32, cx: i32, cy: i32, flags: u32) -> i32;
    }
    const SWP_NOSIZE: u32 = 0x1;
    const SWP_NOMOVE: u32 = 0x2;
    const SWP_NOACTIVATE: u32 = 0x10;
    if let (Ok(p), Ok(h)) = (plate.hwnd(), hud.hwnd()) {
        // SAFETY: both are our own windows' handles.
        unsafe { SetWindowPos(p.0 as isize, h.0 as isize, 0, 0, 0, 0, SWP_NOSIZE | SWP_NOMOVE | SWP_NOACTIVATE) };
    }
}

fn pointer_on_widget<R: Runtime>(handle: &AppHandle<R>, w: &tauri::WebviewWindow<R>, app: &AppState) -> bool {
    let (Ok(cursor), Ok(origin)) = (handle.cursor_position(), w.inner_position()) else {
        return false;
    };
    let (x, y) = (cursor.x - f64::from(origin.x), cursor.y - f64::from(origin.y));
    lock(&app.hud_rects).iter().any(|r| r.contains(x, y))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_layout_keeps_only_places_that_are_numbers() {
        let given = serde_json::json!({
            "_grid": 8,
            "meter": { "x": 0.5, "y": 0.25, "w": 320, "h": 400 },
            "vitals": { "x": "left", "y": null, "w": 0.34, "h": 300, "z": 9 },
            "party": "top",
            "target": { "x": -4, "h": 1e12 },
            "timers": { "x": 0.1 }
        });
        let kept = clean_layout(&given);
        assert_eq!(kept["_grid"], 8);
        assert_eq!(kept["meter"], given["meter"]);
        assert_eq!(kept["vitals"], serde_json::json!({ "w": 0.34, "h": 300 }));
        assert!(kept.get("party").is_none() && kept.get("target").is_none());
        assert_eq!(kept["timers"], serde_json::json!({ "x": 0.1 }));
        // Not a layout at all.
        assert_eq!(clean_layout(&serde_json::json!([1, 2])), serde_json::json!({}));
        assert!(clean_layout(&serde_json::json!({ "_grid": "big" })).get("_grid").is_none());
    }

    #[test]
    fn leaving_the_merchant_is_its_close_x_gone_once_seen() {
        let mut m = MerchantWatch::default();
        // Each look as if the last were long ago.
        let look = |m: &mut MerchantWatch, mark: Option<bool>| {
            m.looked_at = None;
            m.left(true, || (mark, None))
        };
        // No mark seen yet (a merchant whose X isn't told): missing it means nothing.
        assert!(!look(&mut m, Some(false)));
        assert!(!look(&mut m, Some(false)));
        assert!(!look(&mut m, Some(false)));
        // Seen, then gone twice in a row: left. Covered by our window: can't tell.
        assert!(!look(&mut m, Some(true)));
        assert!(!look(&mut m, Some(false)));
        assert!(!look(&mut m, None));
        assert!(look(&mut m, Some(false)));
        // Between looks, nothing is read.
        let mut m = MerchantWatch::default();
        assert!(!m.left(true, || (Some(true), None)));
        assert!(!m.left(true, || unreachable!()));
        // The merchant's Exit: back in the Shop, the X still there, its second sign showing
        // in two looks in a row.
        let mut m = MerchantWatch::default();
        let shop = |m: &mut MerchantWatch, sign: Option<bool>| {
            m.looked_at = None;
            m.left(true, || (Some(true), sign))
        };
        assert!(!shop(&mut m, Some(false)));
        assert!(!shop(&mut m, Some(true)));
        assert!(!shop(&mut m, Some(false)), "a flash of it isn't enough");
        assert!(!shop(&mut m, Some(true)));
        assert!(shop(&mut m, Some(true)));
    }

    #[test]
    fn a_widget_takes_the_pointer_over_its_box_only() {
        let r = HitRect { x: 100.0, y: 50.0, width: 300.0, height: 200.0 };
        assert!(r.contains(100.0, 50.0) && r.contains(400.0, 250.0));
        assert!(!r.contains(99.0, 60.0) && !r.contains(200.0, 251.0));
    }

    #[test]
    fn the_layout_is_kept() {
        let dir = std::env::temp_dir().join(format!("gc-hud-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        assert_eq!(read_layout(&dir), Value::Null);
        fs::write(layout_path(&dir), r#"{"meter":{"x":0.7,"y":0.3,"w":420}}"#).unwrap();
        assert_eq!(read_layout(&dir)["meter"]["w"], 420);
        fs::remove_dir_all(dir).ok();
    }
}
