// Glass Cannon settings: appearance, the overlay's widgets, status checks, meter options,
// tools.

const tauri = window.__TAURI__;
const invoke = tauri.core.invoke;

const $ = (id) => document.getElementById(id);
const FONT_MIN = 10;
const FONT_MAX = 18;
const OPACITY_MIN = 0;
const OPACITY_MAX = 100;
const OPACITY_STEP = 5;
const FROST_STEP = 10;
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
  // Frost only means something with the frosted background.
  const frosted = a.backdrop === "frosted";
  $("frost-value").textContent = frosted ? `${a.frost}%` : "–";
  $("frost-down").disabled = !frosted || a.frost <= 0;
  $("frost-up").disabled = !frosted || a.frost >= 100;
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
    drawAppearance(state.appearance);
    drawScope(state.scope, state.party_size, state.auto_scope);
    for (const b of document.querySelectorAll("[data-refresh-choice]")) {
      b.setAttribute("role", "radio");
      b.setAttribute("aria-checked", String(Number(b.dataset.refreshChoice) === state.refresh_ms));
    }
    setToggle("record", state.record);
    setToggle("boss-hp", state.boss_hp);
    setToggle("boss-casts", state.boss_casts);
    setToggle("pet-souls", state.pet_souls);
    setToggle("alert-sound", state.alert_sound);
    setToggle("hide-on-world-map", state.hide_on_world_map);
    $("arrange-overlay").querySelector(".state").textContent = state.arranging ? "[ done ]" : "[ arrange ]";
    $("arrange-overlay").setAttribute("aria-pressed", String(state.arranging));
    for (const b of document.querySelectorAll("[data-widget]")) {
      const on = state.widgets.includes(b.dataset.widget);
      b.setAttribute("aria-pressed", String(on));
      b.querySelector(".state").textContent = on ? "[on]" : "[off]";
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
$("frost-down").addEventListener("click", () => {
  setAppearance({ frost: Math.max(0, Math.ceil(state.appearance.frost / FROST_STEP) * FROST_STEP - FROST_STEP) });
});
$("frost-up").addEventListener("click", () => {
  setAppearance({ frost: Math.min(100, Math.floor(state.appearance.frost / FROST_STEP) * FROST_STEP + FROST_STEP) });
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

$("alert-sound").addEventListener("click", async () => {
  await invoke("set_alert_sound", { on: !(state && state.alert_sound) });
  refresh();
});

$("pet-souls").addEventListener("click", async () => {
  await invoke("set_pet_souls", { on: !(state && state.pet_souls) });
  refresh();
});

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

// ---- The pet tracker's picker: every pet (search one, or an area it's found in), to track
// the ones you're working on.
let petData = null;
let petNeededOnly = true;

// Portraits, by pet (asked for once each).
const petPortraits = new Map();
function petPortrait(p) {
  if (!petPortraits.has(p.id)) {
    petPortraits.set(p.id, null);
    invoke("pet_icon", { pet: p.id })
      .then((url) => {
        petPortraits.set(p.id, url);
        const img = document.querySelector(`#pet-pick [data-pet="${p.id}"]`);
        if (url && img) img.replaceWith(petPortrait(p));
      })
      .catch(() => {});
  }
  const url = petPortraits.get(p.id);
  const img = url ? el("img", "pp-icon") : el("span", "pp-icon blank", p.name[0]);
  if (url) {
    img.src = url;
    img.alt = "";
  }
  img.dataset.pet = p.id;
  return img;
}

function drawPetPick() {
  const list = $("pet-pick");
  if (!petData) return;
  const q = $("pet-search").value.trim().toLowerCase();
  const needed = (p) => p.level == null || p.need != null;
  const match = (p) => !q || p.name.toLowerCase().includes(q) || (p.genus || "").toLowerCase().includes(q) || p.found_in.some((w) => w.toLowerCase().includes(q));
  const pets = petData.pets
    .filter((p) => match(p) && (!petNeededOnly || needed(p) || p.tracked))
    .sort((a, b) => b.tracked - a.tracked || a.name.localeCompare(b.name));
  list.replaceChildren(
    ...pets.map((p) => {
      const li = el("li");
      const text = el("span", "pp-text");
      const name = el("span", "pp-name", p.name);
      const level = p.level == null ? "Lv ?" : p.need == null ? "max" : `Lv ${p.level} · ${p.souls}/${p.need}`;
      text.append(name, el("span", "pp-meta", [p.genus, level].filter(Boolean).join(" · ")));
      // Where it's found, on hover over the pet.
      li.title = p.found_in.length ? `${p.name}, found in:\n${p.found_in.join("\n")}` : `${p.name}: from an item, an event or a dungeon`;
      const b = el("button", "", p.tracked ? "[tracked]" : "[track]");
      b.type = "button";
      b.setAttribute("aria-pressed", String(p.tracked));
      b.addEventListener("click", async () => {
        await invoke("set_pet_tracked", { id: p.id, on: !p.tracked }).catch((e) => say(String(e), "error"));
        loadPets();
      });
      li.append(petPortrait(p), text, b);
      return li;
    }),
  );
  $("pet-pick-hint").textContent = !petData.character
    ? "log in to a character to track its pets"
    : `${pets.length} of ${petData.pets.length} pets · tracked ones show in the pet tracker widget`;
}

// The list follows your pets as the game sends them (at login, entering a dungeon, a soul):
// asked for again every few seconds while the settings are open, and redrawn only when that
// changed something.
const PET_POLL_MS = 3000;
let petKey = "";
async function loadPets() {
  try {
    petData = await invoke("pet_tracker");
  } catch {
    return;
  }
  const key = JSON.stringify([petData.character, petData.synced, petData.pets.map((p) => [p.id, p.level, p.souls, p.tracked])]);
  if (key === petKey) return;
  petKey = key;
  drawPetPick();
}
setInterval(() => {
  if (!document.hidden) loadPets();
}, PET_POLL_MS);

$("pet-search").addEventListener("input", drawPetPick);
$("pet-needed").addEventListener("click", () => {
  petNeededOnly = !petNeededOnly;
  $("pet-needed").setAttribute("aria-pressed", String(petNeededOnly));
  drawPetPick();
});
loadPets();
