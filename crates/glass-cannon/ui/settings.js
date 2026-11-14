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
  drawClassColours(a.class_palette || {});
  drawTextStyle(a);
  setToggle("info-bar", a.info_bar);
  setToggle("show-cp", a.combat_power !== false);
  setToggle("show-odyle", a.odyle !== false);
  setToggle("fill-tint", !!a.fill_tint);
  for (const b of document.querySelectorAll("[data-row-style-choice]")) {
    b.setAttribute("role", "radio");
    b.setAttribute("aria-checked", String(b.dataset.rowStyleChoice === (a.row_style || "blocks")));
  }
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
    setToggle("click-through", state.click_through);
    setToggle("meter-fit", state.meter_fit);
    setToggle("black-market-auto", state.black_market_auto);
    for (const b of document.querySelectorAll("[data-party-sort]")) {
      b.setAttribute("role", "radio");
      b.setAttribute("aria-checked", String(b.dataset.partySort === state.party_sort));
    }
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
// Each class's colour (wherever class colours show: the meter's rows with class bars, the
// party frames…): the built-in one, or one picked; [reset] puts the built-in ones back.
const CLASSES = [
  [11, "Gladiator", "#f2b27c"],
  [12, "Templar", "#f2908a"],
  [13, "Assassin", "#c6a9f4"],
  [14, "Ranger", "#8fd0f2"],
  [15, "Sorcerer", "#8fe0c4"],
  [16, "Elementalist", "#f3a6d1"],
  [17, "Cleric", "#e8dd8c"],
  [18, "Chanter", "#abe39c"],
  [19, "Fighter", "#f6aa6e"],
];

// The swatches are made once and then only updated in place: making them again replaces
// the colour inputs, which closes a picker that's open (and the settings refresh every
// few seconds).
let classPalette = {};
let classSwatches = null;

function makeClassColours() {
  const items = CLASSES.map(([code, name]) => {
    const item = el("span", "class-swatch");
    const b = el("button", "swatch");
    b.type = "button";
    const input = el("input");
    input.type = "color";
    input.tabIndex = -1;
    input.setAttribute("aria-label", `${name}'s colour`);
    // Picked (the picker closes with it): saved.
    input.addEventListener("change", () => setAppearance({ classPalette: { ...classPalette, [code]: input.value } }));
    b.append(input);
    b.addEventListener("click", () => {
      try {
        input.showPicker();
      } catch {
        input.click();
      }
    });
    item.append(b, el("span", "class-swatch-name", name.slice(0, 3)));
    return { code, name, b, input, item };
  });
  const reset = el("button", "chip", "[reset]");
  reset.type = "button";
  reset.title = "Every class back to its built-in colour";
  reset.addEventListener("click", () => setAppearance({ classPalette: {} }));
  $("class-colours").replaceChildren(...items.map((i) => i.item), reset);
  return { items, reset };
}

function drawClassColours(palette) {
  classPalette = palette;
  if (!classSwatches) classSwatches = makeClassColours();
  for (const { code, name, b, input } of classSwatches.items) {
    const builtIn = CLASSES.find(([c]) => c === code)[2];
    const own = isHexColor(palette[code]);
    const colour = own ? palette[code] : builtIn;
    b.style.setProperty("--swatch", colour);
    b.classList.toggle("own", own);
    b.title = `${name}: ${colour}${own ? " (yours)" : ""} · click to pick another colour`;
    if (input.value !== colour) input.value = colour;
  }
  classSwatches.reset.hidden = !Object.keys(palette).length;
}

// The text's colour and its edge's colour: preset swatches and one to pick. Made once and
// updated in place (making them again closes a picker that's open).
const TEXT_COLOURS = [
  ["white", "#ffffff"],
  ["yellow", "#ffe066"],
  ["cyan", "#7fe3ff"],
  ["green", "#9be89b"],
  ["rose", "#f7b6b0"],
  ["grey", "#c8c8c8"],
];
const EDGE_COLOURS = [
  ["black", "#000000"],
  ["dark grey", "#333333"],
  ["white", "#ffffff"],
  ["navy", "#10204a"],
];
const TEXT_OP = { min: 40, max: 100, step: 10 };
const colourRows = {};

// A row of colour swatches (with [theme] first when `theme` is given) and a picker.
function makeColourRow(boxId, presets, theme, set) {
  const items = [];
  if (theme) {
    const t = el("button", "chip", "[theme]");
    t.type = "button";
    t.title = "The theme's text colour";
    t.dataset.value = "theme";
    t.addEventListener("click", () => set("theme"));
    items.push(t);
  }
  for (const [name, hex] of presets) {
    const b = el("button", "swatch");
    b.type = "button";
    b.title = name;
    b.dataset.value = hex;
    b.style.setProperty("--swatch", hex);
    b.style.boxShadow = "inset 0 0 0 1px rgba(127, 127, 127, 0.6)";
    b.addEventListener("click", () => set(hex));
    items.push(b);
  }
  const custom = el("button", "swatch custom");
  custom.type = "button";
  const input = el("input");
  input.type = "color";
  input.tabIndex = -1;
  input.setAttribute("aria-label", "A colour of your own");
  input.addEventListener("change", () => set(input.value));
  custom.append(input);
  custom.addEventListener("click", () => {
    try {
      input.showPicker();
    } catch {
      input.click();
    }
  });
  items.push(custom);
  $(boxId).replaceChildren(...items);
  return { items, custom, input, presets: presets.map(([, h]) => h) };
}

function markColourRow(row, value) {
  const preset = value === "theme" || row.presets.includes(value);
  for (const b of row.items) {
    b.setAttribute("role", "radio");
    b.setAttribute("aria-checked", String(b === row.custom ? !preset : b.dataset.value === value));
  }
  if (!preset && isHexColor(value)) {
    row.custom.style.setProperty("--swatch", value);
    if (row.input.value !== value) row.input.value = value;
    row.custom.title = `${value} · click to pick another colour`;
  } else {
    row.custom.style.removeProperty("--swatch");
    row.custom.title = "Pick any colour";
  }
}

function drawTextStyle(a) {
  if (!colourRows.text) {
    colourRows.text = makeColourRow("text-colours", TEXT_COLOURS, true, (v) => setAppearance({ textColor: v }));
    colourRows.edge = makeColourRow("edge-colours", EDGE_COLOURS, false, (v) => setAppearance({ edgeColor: v }));
  }
  markColourRow(colourRows.text, a.text_color || "theme");
  markColourRow(colourRows.edge, a.edge_color || "#000000");
  const op = a.text_opacity || 100;
  $("text-op-value").textContent = `${op}%`;
  $("text-op-down").disabled = op <= TEXT_OP.min;
  $("text-op-up").disabled = op >= TEXT_OP.max;
  for (const b of document.querySelectorAll("[data-text-edge]")) {
    b.setAttribute("role", "radio");
    b.setAttribute("aria-checked", String(b.dataset.textEdge === (a.text_edge || "theme")));
  }
  // The edge's colour only matters with an edge of your own.
  for (const b of colourRows.edge.items) b.disabled = !a.text_edge || a.text_edge === "theme" || a.text_edge === "none";
}

$("text-op-down").addEventListener("click", () => setAppearance({ textOpacity: (state.appearance.text_opacity || 100) - TEXT_OP.step }));
$("text-op-up").addEventListener("click", () => setAppearance({ textOpacity: (state.appearance.text_opacity || 100) + TEXT_OP.step }));
for (const b of document.querySelectorAll("[data-text-edge]")) {
  b.addEventListener("click", () => setAppearance({ textEdge: b.dataset.textEdge }));
}

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
$("show-cp").addEventListener("click", () => setAppearance({ combatPower: state.appearance.combat_power === false }));
$("show-odyle").addEventListener("click", () => setAppearance({ odyle: state.appearance.odyle === false }));
$("fill-tint").addEventListener("click", () => setAppearance({ fillTint: !state.appearance.fill_tint }));
for (const b of document.querySelectorAll("[data-row-style-choice]")) {
  b.addEventListener("click", () => setAppearance({ rowStyle: b.dataset.rowStyleChoice }));
}

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


$("black-market-auto").addEventListener("click", async () => {
  await invoke("set_black_market_auto", { on: !(state && state.black_market_auto) });
  refresh();
});

$("meter-fit").addEventListener("click", async () => {
  await invoke("set_meter_fit", { on: !(state && state.meter_fit) });
  refresh();
});

for (const b of document.querySelectorAll("[data-party-sort]")) {
  b.addEventListener("click", async () => {
    await invoke("set_party_sort", { sort: b.dataset.partySort }).catch(() => {});
    refresh();
  });
}

$("click-through").addEventListener("click", async () => {
  await invoke("set_click_through", { on: !(state && state.click_through) });
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

// ---------------------------------------------------------------- action bar

// Your class's bar as last read: what can go on it, and what's on it.
let abChoices = null;

// Skill and effect icons by the data's icon key, loaded once each.
const abIcons = new Map();
function abIcon(choice) {
  const key = choice.icon_key;
  if (key != null && !abIcons.has(key)) {
    abIcons.set(key, null);
    invoke("skill_icon", { skill: key })
      .then((url) => {
        abIcons.set(key, url);
        if (url) drawActionBar();
      })
      .catch(() => {});
  }
  const url = key != null ? abIcons.get(key) : null;
  if (!url) return el("span", "icon blank", (choice.name || "?")[0]);
  const img = el("img", "icon");
  img.src = url;
  img.alt = "";
  return img;
}

function drawActionBar() {
  const c = abChoices;
  if (!c) return;
  $("ab-class").textContent =
    c.class == null ? "use a skill in game first: each class has its own bar" : c.skills.length ? "" : "use your skills in game (or log in): they show here to choose from";
  for (const b of document.querySelectorAll("[data-ab-style]")) {
    b.setAttribute("role", "radio");
    b.setAttribute("aria-checked", String(b.dataset.abStyle === c.style));
  }
  const picks = (list, kind) =>
    list.map((choice) => {
      const b = el("button", "ab-pick");
      b.type = "button";
      b.setAttribute("aria-pressed", String(choice.chosen));
      b.title = `${choice.name || `#${choice.id}`}${choice.chosen ? " · on the bar: click to take it off" : " · click to put it on the bar"}`;
      b.append(abIcon(choice));
      if (choice.chosen) b.append(el("span", "ab-order", String(list.filter((x) => x.chosen).indexOf(choice) + 1)));
      b.addEventListener("click", () => toggleActionBar(kind, choice.id));
      return b;
    });
  drawBarLook(c.bar_look);
  $("ab-skills").replaceChildren(...picks(c.skills, "skills"));
  $("ab-buffs").replaceChildren(...picks(c.buffs, "buffs"));
  if (!c.buffs.length) $("ab-buffs").append(el("span", "note", "none on you in the last hour yet"));
}

// The bars look's colours: a ready skill, a buff running, a skill on cooldown; each a
// named colour, the theme's, or one picked.
const AB_COLOURS = { green: "#79dcaa", blue: "#6aa8f0", red: "#f87070", amber: "#f1c073", lavender: "#c8b4f7", rose: "#f2a8a4", mono: "#dcdcdc" };
const AB_KINDS = [
  ["ready", "ready", "A skill that's ready"],
  ["running", "buffs", "A buff that's running"],
  ["cooling", "cooldown", "A skill on cooldown"],
];

// Drawn again only when the look changes (drawing replaces the colour inputs, which would
// close a picker that's open).
let barLookDrawn = null;
function drawBarLook(l) {
  if (!l) return;
  const key = JSON.stringify(l);
  if (key === barLookDrawn) return;
  barLookDrawn = key;
  setToggle("ab-track", l.background);
  $("ab-colours").replaceChildren(
    ...AB_KINDS.map(([field, key, what]) => {
      const line = el("div", "line");
      line.dataset.keywords = "action bar colour color bars ready buff cooldown";
      line.append(el("span", "key", key));
      const sw = el("span", "swatches");
      sw.setAttribute("role", "radiogroup");
      sw.setAttribute("aria-label", `${what}: its bar's colour`);
      const theme = el("button", "chip", "[theme]");
      theme.type = "button";
      theme.title = "The bar colour (appearance)";
      theme.setAttribute("role", "radio");
      theme.setAttribute("aria-checked", String(l[field] === "theme"));
      theme.addEventListener("click", () => setBarLook({ [field]: "theme" }));
      sw.append(theme);
      for (const [name, hex] of Object.entries(AB_COLOURS)) {
        const b = el("button", "swatch");
        b.type = "button";
        b.title = name;
        b.style.setProperty("--swatch", hex);
        b.style.borderRadius = "0.2em";
        b.setAttribute("role", "radio");
        b.setAttribute("aria-checked", String(l[field] === name));
        b.addEventListener("click", () => setBarLook({ [field]: name }));
        sw.append(b);
      }
      const picked = /^#[0-9a-f]{6}$/i.test(l[field]);
      const custom = el("button", "swatch custom");
      custom.type = "button";
      custom.style.borderRadius = "0.2em";
      if (picked) custom.style.setProperty("--swatch", l[field]);
      custom.title = picked ? `${l[field]} · click to pick another colour` : "Pick any colour";
      custom.setAttribute("role", "radio");
      custom.setAttribute("aria-checked", String(picked));
      const input = el("input");
      input.type = "color";
      input.tabIndex = -1;
      input.setAttribute("aria-label", `${what}: a colour of your own`);
      input.value = picked ? l[field] : "#79dcaa";
      input.addEventListener("change", () => setBarLook({ [field]: input.value }));
      custom.append(input);
      // The input takes no clicks itself: the swatch opens its picker.
      custom.addEventListener("click", () => {
        try {
          input.showPicker();
        } catch {
          input.click();
        }
      });
      sw.append(custom);
      line.append(sw);
      return line;
    }),
  );
}

async function setBarLook(change) {
  try {
    const l = await invoke("set_action_bar_look", { change });
    if (abChoices) abChoices.bar_look = l;
    drawBarLook(l);
  } catch {
    // Unchanged, then.
  }
}

$("ab-track").addEventListener("click", () => abChoices && setBarLook({ background: !abChoices.bar_look.background }));

async function loadActionBar() {
  try {
    const next = await invoke("action_bar_choices");
    if (JSON.stringify(next) !== JSON.stringify(abChoices)) {
      abChoices = next;
      drawActionBar();
    }
  } catch {
    // Next time, then.
  }
}

async function saveActionBar(skills, buffs, style) {
  try {
    await invoke("set_action_bar", { skills, buffs, style });
  } catch (e) {
    $("ab-class").textContent = String(e);
  }
  loadActionBar();
}

function toggleActionBar(kind, id) {
  const c = abChoices;
  if (!c) return;
  const chosen = (list) => list.filter((x) => x.chosen).map((x) => x.id);
  const lists = { skills: chosen(c.skills), buffs: chosen(c.buffs) };
  const list = lists[kind];
  lists[kind] = list.includes(id) ? list.filter((x) => x !== id) : [...list, id];
  saveActionBar(lists.skills, lists.buffs, c.style);
}

for (const b of document.querySelectorAll("[data-ab-style]")) {
  b.addEventListener("click", () => {
    const c = abChoices;
    if (!c) return;
    const chosen = (list) => list.filter((x) => x.chosen).map((x) => x.id);
    saveActionBar(chosen(c.skills), chosen(c.buffs), b.dataset.abStyle);
  });
}

loadActionBar();
setInterval(() => {
  if (!document.hidden) loadActionBar();
}, 3000);

// ---------------------------------------------------------------- cooldowns look

// The cooldowns widget: a list, or a timeline with Ready on the left or the right, and its
// size.
let cdLook = null;
const CD_SIZE = { min: 70, max: 250, step: 10 };

function drawCdLook() {
  if (!cdLook) return;
  for (const b of document.querySelectorAll("[data-cd-look]")) {
    b.setAttribute("role", "radio");
    b.setAttribute("aria-checked", String(b.dataset.cdLook === cdLook.look));
  }
  for (const b of document.querySelectorAll("[data-cd-ready]")) {
    b.setAttribute("role", "radio");
    b.setAttribute("aria-checked", String((b.dataset.cdReady === "right") === cdLook.ready_right));
    // Only the timeline has a Ready end.
    b.disabled = cdLook.look !== "timeline";
  }
  const timeline = cdLook.look === "timeline";
  for (const b of document.querySelectorAll("[data-cd-scale]")) {
    b.setAttribute("role", "radio");
    b.setAttribute("aria-checked", String(b.dataset.cdScale === cdLook.scale));
    b.disabled = !timeline;
  }
  $("cd-size-value").textContent = `${cdLook.size}%`;
  $("cd-size-down").disabled = !timeline || cdLook.size <= CD_SIZE.min;
  $("cd-size-up").disabled = !timeline || cdLook.size >= CD_SIZE.max;
}

async function setCdLook(next) {
  try {
    cdLook = await invoke("set_cooldowns_look", { look: next.look, readyRight: next.ready_right, size: next.size, scale: next.scale });
  } catch {
    // Unchanged, then.
  }
  drawCdLook();
}

for (const b of document.querySelectorAll("[data-cd-look]")) {
  b.addEventListener("click", () => cdLook && setCdLook({ ...cdLook, look: b.dataset.cdLook }));
}
for (const b of document.querySelectorAll("[data-cd-ready]")) {
  b.addEventListener("click", () => cdLook && setCdLook({ ...cdLook, ready_right: b.dataset.cdReady === "right" }));
}
for (const b of document.querySelectorAll("[data-cd-scale]")) {
  b.addEventListener("click", () => cdLook && setCdLook({ ...cdLook, scale: b.dataset.cdScale }));
}
$("cd-size-down").addEventListener("click", () => cdLook && setCdLook({ ...cdLook, size: cdLook.size - CD_SIZE.step }));
$("cd-size-up").addEventListener("click", () => cdLook && setCdLook({ ...cdLook, size: cdLook.size + CD_SIZE.step }));
invoke("cooldowns_look")
  .then((l) => {
    cdLook = l;
    drawCdLook();
  })
  .catch(() => {});

// ---------------------------------------------------------------- vitals arcs

// How the vitals arcs look (vitals.rs); the widget follows each change at once.
let vitals = null;
const VT_ROUND_STEP = 10;
const VT_TEXT = { min: 50, max: 200, step: 10 };
const VT_WIDTH = { min: 50, max: 300, step: 25 };

function drawVitals() {
  const v = vitals;
  if (!v) return;
  for (const b of document.querySelectorAll("[data-vt]")) {
    b.setAttribute("role", "radio");
    b.setAttribute("aria-checked", String(v[b.dataset.vt] === b.dataset.vtValue));
  }
  $("vt-round-value").textContent = v.roundness === 0 ? "straight" : `${v.roundness}%`;
  $("vt-round-down").disabled = v.roundness <= 0;
  $("vt-round-up").disabled = v.roundness >= 100;
  $("vt-text-value").textContent = `${v.text_size}%`;
  $("vt-text-down").disabled = v.text_size <= VT_TEXT.min;
  $("vt-text-up").disabled = v.text_size >= VT_TEXT.max;
  $("vt-width-value").textContent = `${v.width}%`;
  $("vt-width-down").disabled = v.width <= VT_WIDTH.min;
  $("vt-width-up").disabled = v.width >= VT_WIDTH.max;
  drawArcColours(v.colours || {});
  setToggle("vt-always", v.always_show);
  setToggle("vt-trail", v.trail);
  setToggle("vt-cast", v.show_cast);
  // Its look only matters while it shows.
  for (const b of document.querySelectorAll('[data-vt="cast"]')) b.disabled = !v.show_cast;
}

// The arcs' colours: a swatch each to pick another, made once and updated in place
// (making them again closes a picker that's open). The built-in ones are panel.css's.
const ARC_COLOURS = [
  ["hp", "HP", "#ef6b4f"],
  ["stamina", "stamina", "#6dd37b"],
  ["mp", "MP", "#4ea2e6"],
  ["flight", "flight power", "#f2c84b"],
  ["cast", "cast", null],
  ["shield", "shield", "#ffffff"],
];
let arcSwatches = null;

function makeArcColours() {
  const items = ARC_COLOURS.map(([key, name]) => {
    const item = el("span", "class-swatch");
    const b = el("button", "swatch");
    b.type = "button";
    const input = el("input");
    input.type = "color";
    input.tabIndex = -1;
    input.setAttribute("aria-label", `The ${name} arc's colour`);
    input.addEventListener("change", () => vitals && setVitals({ colours: { ...(vitals.colours || {}), [key]: input.value } }));
    b.append(input);
    b.addEventListener("click", () => {
      try {
        input.showPicker();
      } catch {
        input.click();
      }
    });
    item.append(b, el("span", "class-swatch-name", key === "flight" ? "fly" : key === "stamina" ? "sta" : key === "shield" ? "shd" : name.toLowerCase()));
    return { key, name, b, input, item };
  });
  const reset = el("button", "chip", "[reset]");
  reset.type = "button";
  reset.title = "Every arc back to its built-in colour";
  reset.addEventListener("click", () => setVitals({ colours: {} }));
  $("vt-colours").replaceChildren(...items.map((i) => i.item), reset);
  return { items, reset };
}

function drawArcColours(colours) {
  if (!arcSwatches) arcSwatches = makeArcColours();
  // The cast's built-in colour is the theme's.
  const accent = getComputedStyle(document.documentElement).getPropertyValue("--accent").trim();
  for (const { key, name, b, input } of arcSwatches.items) {
    const builtIn = ARC_COLOURS.find(([k]) => k === key)[2] || (isHexColor(accent) ? accent : "#f2a8a4");
    const own = isHexColor(colours[key]);
    const colour = own ? colours[key] : builtIn;
    b.style.setProperty("--swatch", colour);
    b.classList.toggle("own", own);
    b.title = `${name}: ${colour}${own ? " (yours)" : key === "cast" ? " (the theme's)" : ""} · click to pick another colour`;
    if (input.value !== colour) input.value = colour;
  }
  arcSwatches.reset.hidden = !Object.keys(colours).length;
}

async function setVitals(change) {
  try {
    vitals = await invoke("set_vitals", { change });
  } catch {
    // Unchanged, then.
  }
  drawVitals();
}

for (const b of document.querySelectorAll("[data-vt]")) {
  const key = { style: "style", numbers: "numbers", text_backing: "textBacking", text_at: "textAt", cast: "cast" }[b.dataset.vt];
  b.addEventListener("click", () => setVitals({ [key]: b.dataset.vtValue }));
}
$("vt-round-down").addEventListener("click", () => vitals && setVitals({ roundness: vitals.roundness - VT_ROUND_STEP }));
$("vt-round-up").addEventListener("click", () => vitals && setVitals({ roundness: vitals.roundness + VT_ROUND_STEP }));
$("vt-text-down").addEventListener("click", () => vitals && setVitals({ textSize: vitals.text_size - VT_TEXT.step }));
$("vt-text-up").addEventListener("click", () => vitals && setVitals({ textSize: vitals.text_size + VT_TEXT.step }));
$("vt-width-down").addEventListener("click", () => vitals && setVitals({ width: vitals.width - VT_WIDTH.step }));
$("vt-width-up").addEventListener("click", () => vitals && setVitals({ width: vitals.width + VT_WIDTH.step }));
$("vt-always").addEventListener("click", () => vitals && setVitals({ alwaysShow: !vitals.always_show }));
$("vt-trail").addEventListener("click", () => vitals && setVitals({ trail: !vitals.trail }));
$("vt-cast").addEventListener("click", () => vitals && setVitals({ showCast: !vitals.show_cast }));
$("vt-reset").addEventListener("click", async () => {
  try {
    vitals = await invoke("reset_vitals");
  } catch {
    // Unchanged, then.
  }
  drawVitals();
});
invoke("vitals_setup")
  .then((v) => {
    vitals = v;
    drawVitals();
  })
  .catch(() => {});

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

// The overlay tab's widget settings: a tab each (cooldowns, party cooldowns, action bar,
// vitals arcs), the last one chosen remembered.
const SUBTAB_KEY = "settings-subtab";
const subtabButtons = [...document.querySelectorAll("#subtabs [role=tab]")];
const subpanels = [...document.querySelectorAll(".subpanel")];

function showSubtab(name, focus = false) {
  if (!subpanels.some((p) => p.dataset.subtab === name)) name = subpanels[0].dataset.subtab;
  for (const b of subtabButtons) {
    const on = b.dataset.subtab === name;
    b.setAttribute("aria-selected", String(on));
    b.tabIndex = on ? 0 : -1;
    if (on && focus) b.focus();
  }
  for (const p of subpanels) p.classList.toggle("active", p.dataset.subtab === name);
  try {
    localStorage.setItem(SUBTAB_KEY, name);
  } catch {
    // Not remembered, then.
  }
}

for (const b of subtabButtons) {
  b.addEventListener("click", () => showSubtab(b.dataset.subtab));
  // Arrow keys move along them, as along the tabs.
  b.addEventListener("keydown", (e) => {
    if (e.key !== "ArrowLeft" && e.key !== "ArrowRight") return;
    e.preventDefault();
    e.stopPropagation();
    const i = subtabButtons.indexOf(b);
    const next = subtabButtons[(i + (e.key === "ArrowRight" ? 1 : -1) + subtabButtons.length) % subtabButtons.length];
    showSubtab(next.dataset.subtab, true);
  });
}
try {
  showSubtab(localStorage.getItem(SUBTAB_KEY));
} catch {
  showSubtab(null);
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
  const sub = document.activeElement && document.activeElement.closest(".subpanel");
  $("search").value = "";
  runSearch();
  if (followFocus && focused) showTab(focused.dataset.tab);
  if (followFocus && sub) showSubtab(sub.dataset.subtab);
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
