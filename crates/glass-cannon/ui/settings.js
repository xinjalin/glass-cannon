// Glass Cannon settings pop-out: appearance, status checks, meter options, tools.

const tauri = window.__TAURI__;
const invoke = tauri.core.invoke;

const $ = (id) => document.getElementById(id);
const FONT_MIN = 10;
const FONT_MAX = 18;
const OPACITY_MIN = 0;
const OPACITY_MAX = 100;
const OPACITY_STEP = 5;
// Window width in em: two 29em columns, the gap between them and the margins.
const WIDTH_EM = 61.5;

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

function measureAndFit() {
  const fs = parseFloat(getComputedStyle(document.documentElement).fontSize);
  const width = Math.ceil(WIDTH_EM * fs);
  const columns = $("columns");
  // Measure the columns at the width they'll have once the window is resized.
  columns.style.width = `${width - 2.2 * fs}px`;
  const panel = getComputedStyle($("panel"));
  const height = Math.ceil(
    $("bar").offsetHeight +
      columns.offsetHeight +
      $("footer").offsetHeight +
      parseFloat(panel.paddingBottom) +
      0.6 * fs,
  );
  columns.style.width = "";
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
  setToggle("minimal", a.minimal);
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
    setCheck("check-game", state.game);
    setCheck("check-data", state.data);
    setCheck("check-details", state.details);
    drawAppearance(state.appearance);
    drawScope(state.scope, state.party_size, state.auto_scope);
    for (const b of document.querySelectorAll("[data-refresh-choice]")) {
      b.setAttribute("role", "radio");
      b.setAttribute("aria-checked", String(Number(b.dataset.refreshChoice) === state.refresh_ms));
    }
    setToggle("locked", state.locked);
    setToggle("record", state.record);
    setToggle("boss-hp", state.boss_hp);
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
$("minimal").addEventListener("click", () => setAppearance({ minimal: !state.appearance.minimal }));
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

for (const b of document.querySelectorAll("[data-fade-choice]")) {
  b.addEventListener("click", async () => {
    await invoke("set_hide_after", { seconds: Number(b.dataset.fadeChoice) });
    refresh();
  });
}

$("locked").addEventListener("click", async () => {
  await invoke("set_locked", { locked: !(state && state.locked) });
  refresh();
});

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

$("history").addEventListener("click", () => invoke("open_history").catch((e) => say(String(e), "error")));
for (const name of ["cooldowns", "party-cooldowns", "timers", "target"]) {
  $(`open-${name}`).addEventListener("click", () => invoke("toggle_panel", { name }).catch((e) => say(String(e), "error")));
}

$("shortcut").addEventListener("click", async () => {
  try {
    say(await invoke("create_shortcuts"), "ok");
  } catch (e) {
    say(`couldn't add the shortcut: ${e}`, "error");
  }
});

$("close").addEventListener("click", () => invoke("hide_settings"));
document.addEventListener("keydown", (e) => {
  if (e.key === "Escape" && $("font-menu").hidden) invoke("hide_settings");
});

// A change from another window (or "reset appearance") shows here too.
tauri.event.listen("appearance", (e) => {
  if (state) state.appearance = e.payload;
  drawAppearance(e.payload);
});

new ResizeObserver(fitWindow).observe($("columns"));
refresh();
setInterval(refresh, 2000);
