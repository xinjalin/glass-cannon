// Glass Cannon settings, in tabs (appearance, overlay, alerts, meter, keybinds, general),
// with a fuzzy search over all of them.

const tauri = window.__TAURI__;
const invoke = tauri.core.invoke;

const $ = (id) => document.getElementById(id);
const FONT_MIN = 10;
const FONT_MAX = 18;
const OPACITY_MIN = 0;
const OPACITY_MAX = 100;
const OPACITY_STEP = 5;
// Window width in em: one column of settings and the margins (wide enough for the tabs on
// one line).
const WIDTH_EM = 40;

const SCOPE_HINTS = {
  mine: "just you: your damage on the targets you hit",
  party: "your party's targets, and only your party's damage",
  everyone: "every player fighting nearby",
};

let state = null;
let fitted = "";
const DEFAULT_FONT = "Cascadia Mono";
let fitQueued = false;

function setCheck(id, check) {
  const row = $(id);
  row.className = `check ${check.level}`;
  const value = row.querySelector(".value");
  value.textContent = check.text;
  value.title = check.text;
}

function setToggle(id, on) {
  $(id).setAttribute("aria-pressed", String(on));
  $(id).querySelector(".state").textContent = on ? "[on]" : "[off]";
}

// Sizes the window to its content (asked of the app, which also keeps it on screen).
// Runs when the font size changes and whenever the content's height does (hints change
// length), at most once a frame.
function fitWindow() {
  if (fitQueued) return;
  fitQueued = true;
  requestAnimationFrame(() => {
    fitQueued = false;
    measureAndFit();
  });
}

// The window fits the tallest tab (they share one grid cell), so it keeps its size from tab
// to tab; search results scroll in that size instead.
function measureAndFit() {
  if (searching()) return;
  const fs = parseFloat(getComputedStyle(document.documentElement).fontSize);
  const width = Math.ceil(WIDTH_EM * fs);
  const panels = $("panels");
  // Measure the tabs at the width they'll have once the window is resized.
  panels.style.width = `${width - 2.2 * fs}px`;
  const outer = (el) => {
    const cs = getComputedStyle(el);
    return el.offsetHeight + parseFloat(cs.marginTop) + parseFloat(cs.marginBottom);
  };
  const height = Math.ceil(
    outer($("bar")) +
      outer($("search-row")) +
      outer($("tabs")) +
      panels.offsetHeight +
      outer($("message")) +
      outer($("footer")) +
      parseFloat(getComputedStyle($("panel")).paddingBottom) +
      0.6 * fs,
  );
  panels.style.width = "";
  const size = `${width}x${height}`;
  if (size === fitted) return;
  fitted = size;
  invoke("fit_settings", { width, height }).catch(() => {});
}

function el(tag, className, text) {
  const e = document.createElement(tag);
  if (className) e.className = className;
  if (text !== undefined) e.textContent = text;
  return e;
}

function say(text, kind = "") {
  $("message").className = `message ${kind}`;
  $("message").textContent = text;
}

function drawAppearance(a) {
  // While a colour is being picked, what's saved may be a moment behind the picker.
  if (picking && picking.changed) a = { ...a, [picking.field]: pickedValue() };
  applyAppearance(a);
  for (const b of document.querySelectorAll("[data-theme-choice]")) {
    b.setAttribute("role", "radio");
    b.setAttribute("aria-checked", String(b.dataset.themeChoice === a.theme));
  }
  for (const b of document.querySelectorAll("[data-highlight-choice]")) {
    b.setAttribute("role", "radio");
    b.setAttribute("aria-checked", String(b.dataset.highlightChoice === a.highlight));
  }
  for (const b of document.querySelectorAll("[data-bar-choice]")) {
    b.setAttribute("role", "radio");
    b.setAttribute("aria-checked", String(b.dataset.barChoice === a.bar_color));
  }
  drawCustom(document.querySelector("[data-theme-custom]"), a.theme);
  drawCustom(document.querySelector("[data-bar-custom]"), a.bar_color);
  for (const b of document.querySelectorAll("[data-backdrop-choice]")) {
    b.setAttribute("role", "radio");
    b.setAttribute("aria-checked", String(b.dataset.backdropChoice === a.backdrop));
  }
  $("font-value").textContent = `${a.font_size}px`;
  $("font-name").textContent = a.font || DEFAULT_FONT;
  $("font-down").disabled = a.font_size <= FONT_MIN;
  $("font-up").disabled = a.font_size >= FONT_MAX;
  // "clear" has no tint to adjust.
  const clear = a.backdrop === "clear";
  $("opacity-value").textContent = clear ? "none" : `${a.opacity}%`;
  $("opacity-down").disabled = clear || a.opacity <= OPACITY_MIN;
  $("opacity-up").disabled = clear || a.opacity >= OPACITY_MAX;
  setToggle("info-bar", a.info_bar);
  fitWindow();
}

function drawScope(scope, partySize, auto) {
  for (const b of document.querySelectorAll("[data-scope-choice]")) {
    b.setAttribute("role", "radio");
    b.setAttribute("aria-checked", String(auto ? b.dataset.scopeChoice === "auto" : b.dataset.scopeChoice === scope));
  }
  let hint = SCOPE_HINTS[scope] || "";
  if (auto) {
    hint = partySize > 1 ? `follows your party · now your party of ${partySize}` : "follows your party · now just you (not in a party)";
  } else if (scope === "party") {
    hint += partySize > 1 ? ` · ${partySize} in your party` : " · not in a party";
  }
  $("scope-hint").textContent = hint;
}

async function refresh() {
  try {
    state = await invoke("settings_status");
    $("version").textContent = `v${state.version}`;
    setCheck("check-npcap", state.npcap);
    setCheck("check-data", state.data);
    setCheck("check-details", state.details);
    markGeneral();
    drawAppearance(state.appearance);
    drawScope(state.scope, state.party_size, state.auto_scope);
    for (const b of document.querySelectorAll("[data-refresh-choice]")) {
      b.setAttribute("role", "radio");
      b.setAttribute("aria-checked", String(Number(b.dataset.refreshChoice) === state.refresh_ms));
    }
    setToggle("record", state.record);
    setToggle("boss-hp", state.boss_hp);
    setToggle("boss-casts", state.boss_casts);
    setToggle("boss-mechanics", state.boss_mechanics);
    setToggle("boss-on-you", state.boss_on_you);
    setToggle("pet-souls", state.pet_souls);
    setToggle("alert-sound", state.alert_sound);
    setToggle("hide-on-world-map", state.hide_on_world_map);
    setToggle("close-with-game", state.close_with_game);
    setToggle("hide-for-menus", state.hide_for_menus);
    // The last checks, on hover: how X-like and arrow-like the top-right corner was with the
    // cursor up.
    const menuBtn = $("hide-for-menus");
    if (!menuBtn.dataset.text) menuBtn.dataset.text = menuBtn.title;
    menuBtn.title =
      menuBtn.dataset.text +
      (state.menu_checks && state.menu_checks.length
        ? `\n\nLast checks (X from ${state.menu_share[0]}, arrow from ${state.menu_share[1]}): ${state.menu_checks.map(([x, a]) => `X ${x} / arrow ${a}`).join(" · ")}`
        : "");
    drawHotkeys();
    $("arrange-overlay").querySelector(".state").textContent = state.arranging ? "[ done ]" : "[ arrange ]";
    $("arrange-overlay").setAttribute("aria-pressed", String(state.arranging));
    for (const b of document.querySelectorAll("[data-widget]")) {
      const on = state.widgets.includes(b.dataset.widget);
      b.setAttribute("aria-pressed", String(on));
      b.querySelector(".state").textContent = on ? "[on]" : "[off]";
    }
    for (const b of document.querySelectorAll("[data-party-cd-choice]")) {
      b.setAttribute("role", "radio");
      b.setAttribute("aria-checked", String((b.dataset.partyCdChoice === "class") === state.party_cooldowns_by_class));
    }
    for (const b of document.querySelectorAll("[data-bars-choice]")) {
      b.setAttribute("role", "radio");
      b.setAttribute("aria-checked", String(b.dataset.barsChoice === state.player_bars_style));
    }
    for (const b of document.querySelectorAll("[data-region-choice]")) {
      b.setAttribute("role", "radio");
      b.setAttribute("aria-checked", String(b.dataset.regionChoice === state.timer_region));
    }
    for (const b of document.querySelectorAll("[data-lead-choice]")) {
      b.setAttribute("role", "radio");
      b.setAttribute("aria-checked", String(Number(b.dataset.leadChoice) === state.alert_lead_min));
    }
    for (const b of document.querySelectorAll("[data-fade-choice]")) {
      b.setAttribute("role", "radio");
      b.setAttribute("aria-checked", String(Number(b.dataset.fadeChoice) === state.hide_after_s));
    }
    // Capture is already running, so a changed choice applies next time.
    const pending = state.record !== state.recording_now;
    $("record-hint").textContent = pending
      ? `applies next time you open the meter (now ${state.recording_now ? "recording" : "not recording"})`
      : "save packets to the captures folder";
    $("record-hint").className = pending ? "hint warn" : "hint";
  } catch (e) {
    say(String(e), "error");
  }
}

// Appearance changes are sent one after another, so a later one is never overtaken by an
// earlier one still on its way (the app may run two at once).
let appearanceQueue = Promise.resolve();
function queueAppearance(command, change) {
  appearanceQueue = appearanceQueue.then(async () => {
    try {
      const a = await invoke(command, { change });
      if (state) state.appearance = a;
      drawAppearance(a);
    } catch (e) {
      say(String(e), "error");
    }
  });
  return appearanceQueue;
}

// Changes apply at once here; the overlay picks them up on its next refresh.
function setAppearance(change) {
  return queueAppearance("set_appearance", change);
}

for (const b of document.querySelectorAll("[data-theme-choice]")) {
  b.addEventListener("click", () => setAppearance({ theme: b.dataset.themeChoice }));
}
for (const b of document.querySelectorAll("[data-highlight-choice]")) {
  b.addEventListener("click", () => setAppearance({ highlight: b.dataset.highlightChoice }));
}
for (const b of document.querySelectorAll("[data-backdrop-choice]")) {
  b.addEventListener("click", () => setAppearance({ backdrop: b.dataset.backdropChoice }));
}
$("font-down").addEventListener("click", () => setAppearance({ fontSize: state.appearance.font_size - 1 }));
$("font-up").addEventListener("click", () => setAppearance({ fontSize: state.appearance.font_size + 1 }));
// The tint moves in steps of 5, snapping to them first (the default is 62), within 0-100.
$("opacity-down").addEventListener("click", () => {
  const o = state.appearance.opacity;
  setAppearance({ opacity: Math.max(OPACITY_MIN, Math.ceil(o / OPACITY_STEP) * OPACITY_STEP - OPACITY_STEP) });
});
$("opacity-up").addEventListener("click", () => {
  const o = state.appearance.opacity;
  setAppearance({ opacity: Math.min(OPACITY_MAX, Math.floor(o / OPACITY_STEP) * OPACITY_STEP + OPACITY_STEP) });
});
// The last swatch of each row: a colour of your own, picked in the system's colour dialog.
function drawCustom(button, value) {
  const picked = isHexColor(value);
  button.setAttribute("role", "radio");
  button.setAttribute("aria-checked", String(picked));
  const input = button.querySelector("input");
  if (picked) {
    button.style.setProperty("--swatch", value);
    // Not while its picker is open: setting it would move or close the picker.
    if (!picking || picking.input !== input) input.value = value;
  } else button.style.removeProperty("--swatch");
  button.title = picked ? `${value} · click to pick another colour` : "Pick any colour";
}
// The custom colour picker that's open, if one is: { input, field, key, start, before,
// changed }. Its colour is saved and shown in every window as it changes (a few times a
// second at most); this window's frame is restyled once it closes, since restyling it
// closes the picker. The picker doesn't always say when it closes (clicking another window
// may not), so a click or key here, or this window losing focus, also counts. Closing it
// without picking anything changes nothing; cancelling it (back to the colour it opened
// with) puts back what was set before, a preset included.
let picking = null;
let pickTimer = null;
let pickChange = null;
const PICK_SAVE_MS = 120;

// The open picker's colour as a setting: back at the colour it opened with (cancelled),
// what was set before, a preset included.
function pickedValue() {
  const { input, start, before } = picking;
  return input.value === start && before ? before : input.value;
}

function savePick() {
  clearTimeout(pickTimer);
  pickTimer = null;
  const change = pickChange;
  pickChange = null;
  if (change) queueAppearance("pick_colour", change);
}

async function donePicking() {
  if (!picking) return;
  const { key, changed } = picking;
  const value = pickedValue();
  picking = null;
  clearTimeout(pickTimer);
  pickTimer = null;
  pickChange = null;
  if (!changed) return;
  // The picker's last colour, with every window (this one too) restyled.
  await setAppearance({ [key]: value });
}

for (const [kind, key] of [["theme", "theme"], ["bar", "barColor"]]) {
  const button = document.querySelector(`[data-${kind}-custom]`);
  const input = button.querySelector("input");
  const field = kind === "theme" ? "theme" : "bar_color";
  const open = () => ({ input, field, key, start: input.value, before: state && state.appearance[field], changed: false });
  button.addEventListener("click", () => {
    picking = open();
    input.click();
  });
  input.addEventListener("input", () => {
    if (!picking) picking = open();
    picking.changed = true;
    if (state) drawAppearance(state.appearance);
    button.style.setProperty("--swatch", input.value);
    pickChange = { [key]: pickedValue() };
    if (!pickTimer) pickTimer = setTimeout(savePick, PICK_SAVE_MS);
  });
  input.addEventListener("change", donePicking);
}
window.addEventListener("blur", donePicking);
document.addEventListener("pointerdown", donePicking, true);
document.addEventListener("keydown", donePicking, true);
for (const b of document.querySelectorAll("[data-bar-choice]")) {
  b.addEventListener("click", () => setAppearance({ barColor: b.dataset.barChoice }));
}
$("info-bar").addEventListener("click", () => setAppearance({ infoBar: !state.appearance.info_bar }));

for (const b of document.querySelectorAll("[data-scope-choice]")) {
  b.addEventListener("click", async () => {
    const choice = b.dataset.scopeChoice;
    await invoke("set_scope", { scope: choice === "auto" ? null : choice });
    refresh();
  });
}

for (const b of document.querySelectorAll("[data-refresh-choice]")) {
  b.addEventListener("click", async () => {
    await invoke("set_refresh", { ms: Number(b.dataset.refreshChoice) }).catch((e) => say(String(e), "error"));
    refresh();
  });
}

$("boss-hp").addEventListener("click", async () => {
  await invoke("set_boss_hp", { on: !(state && state.boss_hp) });
  refresh();
});

$("boss-casts").addEventListener("click", async () => {
  await invoke("set_boss_casts", { on: !(state && state.boss_casts) });
  refresh();
});

$("boss-mechanics").addEventListener("click", async () => {
  await invoke("set_boss_mechanics", { on: !(state && state.boss_mechanics) });
  refresh();
});

for (const b of document.querySelectorAll("[data-widget]")) {
  b.addEventListener("click", async () => {
    await invoke("toggle_widget", { name: b.dataset.widget }).catch((e) => say(String(e), "error"));
    refresh();
  });
}

// Arrange the overlay; again (or [ done ] on the overlay) saves the layout and stops.
$("arrange-overlay").addEventListener("click", async () => {
  await invoke("hud_edit", { on: !(state && state.arranging) }).catch((e) => say(String(e), "error"));
  refresh();
});

$("hide-on-world-map").addEventListener("click", async () => {
  await invoke("set_hide_on_world_map", { on: !(state && state.hide_on_world_map) });
  refresh();
});

$("hide-for-menus").addEventListener("click", async () => {
  await invoke("set_hide_for_menus", { on: !(state && state.hide_for_menus) });
  refresh();
});

// Keybinds: click one, then press its new keys (as the app names them: modifiers, then
// the key's code, "Ctrl+Shift+F10"; or one modifier alone, tapped, "AltRight"). Esc
// keeps the old ones, Delete puts back the default. The hotkeys pause meanwhile.
let bindingAction = null;
const modsDown = new Set();
const MODIFIERS = ["ControlLeft", "ControlRight", "ShiftLeft", "ShiftRight", "AltLeft", "AltRight", "MetaLeft", "MetaRight"];

function prettyKeys(keys) {
  const side = { Left: "Left", Right: "Right" };
  const names = { Control: "Ctrl", Shift: "Shift", Alt: "Alt", Meta: "Win" };
  return keys
    .split("+")
    .map((k) => {
      const m = /^(Control|Shift|Alt|Meta)(Left|Right)$/.exec(k);
      if (m) return `${side[m[2]]} ${names[m[1]]}`;
      return k.replace(/^Key(.)$/, "$1").replace(/^Digit(.)$/, "$1").replace(/^Numpad(.)$/, "Num $1").replace(/^Arrow/, "");
    })
    .join("+");
}

function drawHotkeys() {
  const rows = state.hotkeys.map((h) => {
    const b = el("button", "opt");
    b.type = "button";
    b.setAttribute("aria-pressed", String(bindingAction === h.action));
    b.title = `Works while the game is up · click, then press new keys: with Ctrl, Shift, Alt or Win, or an F key (default: ${prettyKeys(h.default)}) · now: ${h.state}`;
    if (h.state.startsWith("taken")) b.classList.add("warn");
    b.append(el("span", "key", h.what), el("span", "state", bindingAction === h.action ? "[press keys…]" : `[${prettyKeys(h.keys)}]${h.state.startsWith("taken") ? " taken" : ""}`));
    b.addEventListener("click", () => (bindingAction === h.action ? stopPicking() : startPicking(h.action)));
    return b;
  });
  $("hotkeys").replaceChildren(...rows);
  // New rows: filter them like the rest while searching.
  if (searching()) runSearch();
}

function startPicking(action) {
  bindingAction = action;
  modsDown.clear();
  invoke("pause_hotkeys", { on: true }).catch(() => {});
  $("hotkeys-hint").textContent = "press the new keys (with Ctrl, Shift, Alt or Win, or an F key) · Esc keeps the old ones · Delete puts back the default";
  $("hotkeys-hint").hidden = false;
  drawHotkeys();
}

function stopPicking(error) {
  bindingAction = null;
  modsDown.clear();
  invoke("pause_hotkeys", { on: false }).catch(() => {});
  // Only shown while picking, or to say why keys weren't taken.
  $("hotkeys-hint").textContent = error || "";
  $("hotkeys-hint").hidden = !error;
  refresh();
}

async function pickKeys(keys) {
  const action = bindingAction;
  try {
    await invoke("set_hotkey", { action, keys });
    stopPicking();
  } catch (e) {
    stopPicking(String(e));
  }
}

window.addEventListener(
  "keydown",
  (e) => {
    if (!bindingAction) return;
    e.preventDefault();
    e.stopPropagation();
    if (e.repeat) return;
    if (MODIFIERS.includes(e.code)) {
      modsDown.add(e.code);
      return;
    }
    const mods = [e.ctrlKey && "Ctrl", e.shiftKey && "Shift", e.altKey && "Alt", e.metaKey && "Win"].filter(Boolean);
    if (!mods.length && e.code === "Escape") return stopPicking();
    if (!mods.length && e.code === "Delete") return pickKeys("");
    pickKeys([...mods, e.code].join("+"));
  },
  true,
);
window.addEventListener(
  "keyup",
  (e) => {
    if (!bindingAction) return;
    e.preventDefault();
    e.stopPropagation();
    // One modifier alone, pressed and let go (Right Alt brings a Left Ctrl with it on
    // some keyboards).
    const alone = [...modsDown].filter((c) => !(c === "ControlLeft" && modsDown.has("AltRight")));
    if (alone.length === 1 && alone[0] === e.code) pickKeys(e.code);
  },
  true,
);
window.addEventListener("blur", () => {
  if (bindingAction) stopPicking();
});

$("close-with-game").addEventListener("click", async () => {
  await invoke("set_close_with_game", { on: !(state && state.close_with_game) });
  refresh();
});

for (const b of document.querySelectorAll("[data-bars-choice]")) {
  b.addEventListener("click", async () => {
    await invoke("set_player_bars_style", { style: b.dataset.barsChoice }).catch((e) => say(String(e), "error"));
    refresh();
  });
}
for (const b of document.querySelectorAll("[data-region-choice]")) {
  b.addEventListener("click", async () => {
    await invoke("set_timer_region", { region: b.dataset.regionChoice }).catch((e) => say(String(e), "error"));
    refresh();
  });
}
for (const b of document.querySelectorAll("[data-lead-choice]")) {
  b.addEventListener("click", async () => {
    await invoke("set_alert_lead", { minutes: Number(b.dataset.leadChoice) });
    refresh();
  });
}

$("boss-on-you").addEventListener("click", async () => {
  await invoke("set_boss_on_you", { on: !(state && state.boss_on_you) });
  refresh();
});

$("alert-sound").addEventListener("click", async () => {
  await invoke("set_alert_sound", { on: !(state && state.alert_sound) });
  refresh();
});

$("pet-souls").addEventListener("click", async () => {
  await invoke("set_pet_souls", { on: !(state && state.pet_souls) });
  refresh();
});

for (const b of document.querySelectorAll("[data-party-cd-choice]")) {
  b.addEventListener("click", async () => {
    await invoke("set_party_cooldowns_by_class", { on: b.dataset.partyCdChoice === "class" });
    refresh();
  });
}

for (const b of document.querySelectorAll("[data-fade-choice]")) {
  b.addEventListener("click", async () => {
    await invoke("set_hide_after", { seconds: Number(b.dataset.fadeChoice) });
    refresh();
  });
}

$("record").addEventListener("click", async () => {
  await invoke("set_record", { record: !(state && state.record) });
  refresh();
});

// ---------------------------------------------------------------- font picker

let fonts = null; // installed families, loaded when the menu first opens
let menuItems = []; // the options shown, in order
let activeIndex = -1;

async function openFontMenu() {
  if (!fonts) fonts = await invoke("list_fonts").catch(() => []);
  $("font-menu").hidden = false;
  $("font-button").setAttribute("aria-expanded", "true");
  $("font-filter").value = "";
  drawFontList("");
  placeFontMenu();
  $("font-filter").focus();
}

function closeFontMenu() {
  if ($("font-menu").hidden) return;
  $("font-menu").hidden = true;
  $("font-button").setAttribute("aria-expanded", "false");
  $("font-button").focus();
}

// Under the button (or above it if there's more room there), right-aligned with it and
// inside the window.
function placeFontMenu() {
  const menu = $("font-menu");
  const b = $("font-button").getBoundingClientRect();
  const margin = 8;
  const below = innerHeight - b.bottom - margin;
  const above = b.top - margin;
  const down = below >= 200 || below >= above;
  menu.style.maxHeight = `${Math.max(120, (down ? below : above) - 4)}px`;
  menu.style.top = down ? `${b.bottom + 4}px` : "";
  menu.style.bottom = down ? "" : `${innerHeight - b.top + 4}px`;
  const left = Math.min(b.right - menu.offsetWidth, innerWidth - menu.offsetWidth - margin);
  menu.style.left = `${Math.max(margin, left)}px`;
}

function drawFontList(filter) {
  const list = $("font-list");
  list.replaceChildren();
  menuItems = [];
  const q = filter.trim().toLowerCase();
  const current = state ? state.appearance.font : "";
  const add = (name, label) => {
    const item = el("li", "menu-item", label);
    item.setAttribute("role", "option");
    item.setAttribute("aria-selected", String(name === current));
    // Each font shows in itself (names come from Windows and are checked by the app).
    if (name) item.style.fontFamily = `"${name}", monospace`;
    item.addEventListener("click", () => chooseFont(name));
    item.addEventListener("mousemove", () => setActive(menuItems.indexOf(item), false));
    list.append(item);
    menuItems.push(item);
  };
  const group = (title) => list.append(el("li", "menu-group", title));
  if (!q || DEFAULT_FONT.toLowerCase().includes(q)) {
    group("default");
    add("", `${DEFAULT_FONT} (built in)`);
  }
  // The built-in default is listed once, above.
  const matches = (fonts || []).filter(
    (f) => f.name !== DEFAULT_FONT && (!q || f.name.toLowerCase().includes(q)),
  );
  const mono = matches.filter((f) => f.mono);
  const other = matches.filter((f) => !f.mono);
  if (mono.length) {
    group("monospace");
    for (const f of mono) add(f.name, f.name);
  }
  if (other.length) {
    group("other fonts");
    for (const f of other) add(f.name, f.name);
  }
  if (!menuItems.length) list.append(el("li", "menu-empty", "no fonts match"));
  const selected = menuItems.findIndex((i) => i.getAttribute("aria-selected") === "true");
  setActive(selected >= 0 ? selected : 0, true);
}

function setActive(index, scroll) {
  activeIndex = index;
  menuItems.forEach((item, i) => item.classList.toggle("active", i === index));
  if (scroll && menuItems[index]) menuItems[index].scrollIntoView({ block: "nearest" });
}

function chooseFont(name) {
  closeFontMenu();
  setAppearance({ font: name });
}

$("font-button").addEventListener("click", () => ($("font-menu").hidden ? openFontMenu() : closeFontMenu()));
$("font-filter").addEventListener("input", (e) => drawFontList(e.target.value));
$("font-filter").addEventListener("keydown", (e) => {
  if (e.key === "ArrowDown" || e.key === "ArrowUp") {
    e.preventDefault();
    if (!menuItems.length) return;
    const step = e.key === "ArrowDown" ? 1 : -1;
    setActive((activeIndex + step + menuItems.length) % menuItems.length, true);
  } else if (e.key === "Enter") {
    e.preventDefault();
    if (menuItems[activeIndex]) menuItems[activeIndex].click();
  } else if (e.key === "Escape") {
    // Only the menu closes, not the settings window.
    e.preventDefault();
    e.stopPropagation();
    closeFontMenu();
  } else if (e.key === "Tab") {
    closeFontMenu();
  }
});
document.addEventListener("mousedown", (e) => {
  if (!$("font-menu").hidden && !e.target.closest("#font-menu, #font-button")) closeFontMenu();
});
window.addEventListener("resize", () => {
  if (!$("font-menu").hidden) placeFontMenu();
});

// ---------------------------------------------------------------- reset

// Two clicks: the first asks, the second (within a few seconds) resets.
let resetArmed = null;
function disarmReset() {
  clearTimeout(resetArmed);
  resetArmed = null;
  $("reset").querySelector(".state").textContent = "[ reset ]";
}
$("reset").addEventListener("click", async () => {
  if (!resetArmed) {
    $("reset").querySelector(".state").textContent = "[ sure? ]";
    resetArmed = setTimeout(disarmReset, 3000);
    return;
  }
  disarmReset();
  try {
    const a = await invoke("reset_appearance");
    if (state) state.appearance = a;
    drawAppearance(a);
    say("appearance back to the defaults", "ok");
  } catch (e) {
    say(String(e), "error");
  }
});

// The same two clicks for every setting and keybind.
let resetAllArmed = null;
function disarmResetAll() {
  clearTimeout(resetAllArmed);
  resetAllArmed = null;
  $("reset-all").querySelector(".state").textContent = "[ reset ]";
}
$("reset-all").addEventListener("click", async () => {
  if (!resetAllArmed) {
    $("reset-all").querySelector(".state").textContent = "[ sure? ]";
    resetAllArmed = setTimeout(disarmResetAll, 3000);
    return;
  }
  disarmResetAll();
  try {
    await invoke("reset_settings");
    await refresh();
    say("settings and keybinds back to the defaults", "ok");
  } catch (e) {
    say(String(e), "error");
  }
});

$("shortcut").addEventListener("click", async () => {
  try {
    say(await invoke("create_shortcuts"), "ok");
  } catch (e) {
    say(`couldn't add the shortcut: ${e}`, "error");
  }
});

$("close").addEventListener("click", () => invoke("hide_settings"));
document.addEventListener("keydown", (e) => {
  if (e.key !== "Escape" || !$("font-menu").hidden) return;
  // A search first: Esc clears it, the next one closes.
  if (searching()) {
    e.preventDefault();
    clearSearch();
    return;
  }
  invoke("hide_settings");
});

// A change from another window (or "reset appearance") shows here too.
tauri.event.listen("appearance", (e) => {
  if (state) state.appearance = e.payload;
  drawAppearance(e.payload);
});

// ---------------------------------------------------------------- tabs

const TAB_KEY = "settings-tab";
const tabButtons = [...document.querySelectorAll("#tabs [role=tab]")];
const panels = [...document.querySelectorAll(".tab-panel")];

function storedTab() {
  try {
    return localStorage.getItem(TAB_KEY);
  } catch {
    return null;
  }
}

function showTab(name, focus = false) {
  if (!panels.some((p) => p.dataset.tab === name)) name = panels[0].dataset.tab;
  for (const b of tabButtons) {
    const on = b.dataset.tab === name;
    b.setAttribute("aria-selected", String(on));
    b.tabIndex = on ? 0 : -1;
    if (on && focus) b.focus();
  }
  for (const p of panels) p.classList.toggle("active", p.dataset.tab === name);
  try {
    localStorage.setItem(TAB_KEY, name);
  } catch {
    // Not remembered, then.
  }
}

function currentTab() {
  const b = tabButtons.find((x) => x.getAttribute("aria-selected") === "true");
  return b ? b.dataset.tab : panels[0].dataset.tab;
}

function stepTab(step, focus) {
  const i = tabButtons.findIndex((b) => b.dataset.tab === currentTab());
  showTab(tabButtons[(i + step + tabButtons.length) % tabButtons.length].dataset.tab, focus);
}

for (const b of tabButtons) {
  b.addEventListener("click", () => {
    // A tab picked while searching: back to that tab's settings.
    if (searching()) clearSearch(false);
    showTab(b.dataset.tab);
  });
}
$("tabs").addEventListener("keydown", (e) => {
  if (e.key === "ArrowRight" || e.key === "ArrowLeft") {
    e.preventDefault();
    stepTab(e.key === "ArrowRight" ? 1 : -1, true);
  } else if (e.key === "Home" || e.key === "End") {
    e.preventDefault();
    const b = e.key === "Home" ? tabButtons[0] : tabButtons[tabButtons.length - 1];
    showTab(b.dataset.tab, true);
  }
});

// A dot on [general] when one of its status checks isn't ok.
function markGeneral() {
  const levels = [...document.querySelectorAll("#tab-general .check")].map((c) => c.className);
  const bad = levels.some((l) => l.includes("bad"));
  const warn = levels.some((l) => l.includes("warn"));
  const dot = $("tab-button-general").querySelector(".tab-dot");
  dot.hidden = !bad && !warn;
  dot.classList.toggle("bad", bad);
  $("tab-button-general").title = bad || warn ? "A status check needs a look" : "";
}

// ---------------------------------------------------------------- search

// Fuzzy: each word typed must match a setting. In its name the letters only need to come
// in order, from the start of a word ("alrt snd" finds "alert sound"; one or two letters
// must be a word's start); in its description, keywords and section a word must start with
// what's typed. A word of 4 letters or more may also be one slip off a word of its name,
// keywords or section (a letter missing, extra, wrong or swapped: "aletr", "soudn").

const norm = (t) => (t || "").toLowerCase().replace(/[\[\]]/g, " ").replace(/\s+/g, " ").trim();
const isWordStart = (t, i) => i === 0 || !/[a-z0-9]/.test(t[i - 1]);

// The letters of `q` in order in `t`, starting at a word start: their positions and a
// score (a run of letters and word starts count most), or null.
function inOrder(q, t) {
  let best = null;
  for (let start = 0; start < t.length; start++) {
    if (t[start] !== q[0] || !isWordStart(t, start)) continue;
    const pos = [start];
    let score = 3;
    let i = start + 1;
    for (const ch of q.slice(1)) {
      // Prefer the next letter right after, then at a word start, then anywhere.
      let k = t[i] === ch ? i : -1;
      if (k < 0) {
        for (let j = i; j < t.length; j++) {
          if (t[j] === ch && isWordStart(t, j)) {
            k = j;
            break;
          }
        }
      }
      if (k < 0) k = t.indexOf(ch, i);
      if (k < 0) break;
      score += k === pos[pos.length - 1] + 1 ? 3 : isWordStart(t, k) ? 2 : 0;
      score -= Math.min(3, (k - pos[pos.length - 1] - 1) * 0.15);
      pos.push(k);
      i = k + 1;
    }
    if (pos.length === q.length && (!best || score > best.score)) best = { pos, score };
  }
  return best;
}

// Damerau-Levenshtein distance (with swaps), stopping early past `max`.
function slips(a, b, max) {
  if (Math.abs(a.length - b.length) > max) return max + 1;
  const d = Array.from({ length: a.length + 1 }, (_, i) => [i, ...Array(b.length).fill(0)]);
  for (let j = 1; j <= b.length; j++) d[0][j] = j;
  for (let i = 1; i <= a.length; i++) {
    let rowMin = Infinity;
    for (let j = 1; j <= b.length; j++) {
      const cost = a[i - 1] === b[j - 1] ? 0 : 1;
      d[i][j] = Math.min(d[i - 1][j] + 1, d[i][j - 1] + 1, d[i - 1][j - 1] + cost);
      if (i > 1 && j > 1 && a[i - 1] === b[j - 2] && a[i - 2] === b[j - 1]) d[i][j] = Math.min(d[i][j], d[i - 2][j - 2] + 1);
      rowMin = Math.min(rowMin, d[i][j]);
    }
    if (rowMin > max) return max + 1;
  }
  return d[a.length][b.length];
}

// A word of `words` that `token` is one slip from (the word whole, or its start as long as
// the token), or false.
function nearWord(token, words) {
  if (token.length < 4) return false;
  const max = token.length >= 7 ? 2 : 1;
  return words.some((w) => w.length >= 3 && (slips(token, w, max) <= max || (w.length > token.length && slips(token, w.slice(0, token.length), max) <= max)));
}

// How well one word typed matches a setting: a score, and the name's letters it matched.
function matchToken(token, item) {
  const exact = item.name.indexOf(token);
  if (exact >= 0 && isWordStart(item.name, exact)) {
    return { score: 12 + token.length, pos: [...Array(token.length).keys()].map((k) => exact + k) };
  }
  const order = token.length > 2 && inOrder(token, item.name);
  if (order) return { score: order.score, pos: order.pos };
  if (item.moreWords.some((w) => w.startsWith(token))) return { score: 4, pos: [] };
  if (token.length >= 3 && item.more.includes(token)) return { score: 2, pos: [] };
  if (nearWord(token, item.nameWords)) return { score: 3, pos: [] };
  if (nearWord(token, item.tagWords)) return { score: 1, pos: [] };
  return null;
}

// Every setting's row, with what's searched: its name, then its description, keywords,
// choices and where it is (section and tab). Read again each search, so the keybinds'
// rows (redrawn every refresh) and changed descriptions count.
function searchItems() {
  const items = [];
  for (const panel of panels) {
    for (const row of panel.querySelectorAll(".line, .opt, .check")) {
      const key = row.querySelector(".key");
      if (!key) continue;
      if (!key.dataset.text) key.dataset.text = key.textContent;
      const box = row.closest(".box");
      const legend = box ? box.querySelector("legend").textContent : "";
      const extra = [...row.querySelectorAll("[title]")].map((b) => `${b.textContent} ${b.title}`).join(" ");
      const name = norm(key.dataset.text);
      const tags = norm(`${row.dataset.keywords || ""} ${legend} ${panel.dataset.tab}`);
      const more = norm(`${row.title || ""} ${key.title || ""} ${extra} ${tags}`);
      items.push({
        row,
        key,
        panel,
        name,
        nameWords: name.split(/[^a-z0-9]+/).filter(Boolean),
        more,
        moreWords: more.split(/[^a-z0-9]+/).filter(Boolean),
        tagWords: tags.split(/[^a-z0-9]+/).filter(Boolean),
      });
    }
  }
  return items;
}

// The name with the matched letters marked (built from text nodes: names are our own, but
// never set as HTML anyway).
function markKey(key, positions) {
  const text = key.dataset.text;
  if (!positions.length) {
    key.textContent = text;
    return;
  }
  // Positions are in the normalised name; it keeps the original's length unless it had
  // brackets or runs of spaces, which names don't.
  const at = new Set(positions);
  const parts = [];
  let run = "";
  let marked = false;
  for (let i = 0; i < text.length; i++) {
    const m = at.has(i);
    if (m !== marked && run) {
      parts.push(marked ? Object.assign(document.createElement("mark"), { textContent: run }) : document.createTextNode(run));
      run = "";
    }
    marked = m;
    run += text[i];
  }
  if (run) parts.push(marked ? Object.assign(document.createElement("mark"), { textContent: run }) : document.createTextNode(run));
  key.replaceChildren(...parts);
}

let bestRow = null;

function searching() {
  return norm($("search").value).length > 0;
}

function runSearch() {
  const tokens = norm($("search").value).split(" ").filter(Boolean);
  const box = $("panels");
  const on = tokens.length > 0;
  box.classList.toggle("searching", on);
  document.body.classList.toggle("searching", on);
  if (bestRow) bestRow.classList.remove("found");
  bestRow = null;
  let shown = 0;
  let bestScore = -Infinity;
  for (const item of searchItems()) {
    let score = 0;
    let pos = [];
    for (const t of tokens) {
      const m = matchToken(t, item);
      if (!m) {
        score = null;
        break;
      }
      score += m.score;
      pos = pos.concat(m.pos);
    }
    const hit = !on || score !== null;
    item.row.toggleAttribute("data-miss", !hit);
    markKey(item.key, on && hit ? pos : []);
    if (on && hit) {
      shown++;
      if (score > bestScore) {
        bestScore = score;
        bestRow = item.row;
      }
    }
  }
  // Hints show under a setting that matched; sections and tabs with nothing left go.
  for (const hint of document.querySelectorAll(".tab-panel .hint")) {
    const prev = hint.previousElementSibling;
    const keep = !on || hint.id === "hotkeys-hint" || (prev && prev.matches(".line, .opt") && !prev.hasAttribute("data-miss"));
    hint.toggleAttribute("data-miss", !keep);
  }
  for (const b of document.querySelectorAll(".tab-panel .box")) {
    b.toggleAttribute("data-miss", on && !b.querySelector(".line:not([data-miss]), .opt:not([data-miss]), .check:not([data-miss])"));
  }
  for (const p of panels) p.toggleAttribute("data-miss", on && !p.querySelector(".box:not([data-miss])"));
  if (bestRow && on) bestRow.classList.add("found");
  $("no-results").hidden = !on || shown > 0;
  $("search-count").textContent = on ? (shown === 1 ? "1 setting" : `${shown} settings`) : "";
  if (!on) fitWindow();
}

// Clears the search. With a setting focused in the results, its tab opens, so it stays in
// view.
function clearSearch(followFocus = true) {
  const focused = document.activeElement && document.activeElement.closest(".tab-panel");
  $("search").value = "";
  runSearch();
  if (followFocus && focused) showTab(focused.dataset.tab);
}

// The control to go to for a row: the row itself when it's a button, else its first one.
function controlOf(row) {
  return row.matches("button") ? row : row.querySelector("button, input");
}

$("search").addEventListener("input", () => {
  runSearch();
  $("scroll").scrollTop = 0;
});
$("search").addEventListener("keydown", (e) => {
  if ((e.key === "Enter" || e.key === "ArrowDown") && bestRow) {
    // To the best match (Enter doesn't switch anything: Space or Enter again does).
    e.preventDefault();
    const c = controlOf(bestRow);
    if (c) {
      c.focus();
      c.scrollIntoView({ block: "nearest" });
    }
  }
});
document.addEventListener("keydown", (e) => {
  if (bindingAction) return;
  const typing = e.target.matches && e.target.matches("input, textarea");
  if ((e.ctrlKey && e.key.toLowerCase() === "f") || (e.key === "/" && !typing)) {
    e.preventDefault();
    $("search").focus();
    $("search").select();
  } else if (e.ctrlKey && (e.key === "Tab" || e.key === "PageDown" || e.key === "PageUp")) {
    e.preventDefault();
    if (searching()) clearSearch(false);
    stepTab(e.key === "PageUp" || (e.key === "Tab" && e.shiftKey) ? -1 : 1, false);
  }
});

showTab(storedTab() || "appearance");
new ResizeObserver(fitWindow).observe($("panels"));
refresh();
setInterval(refresh, 2000);
