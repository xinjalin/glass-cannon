// Glass Cannon settings pop-out: status checks, appearance, options, tools.

const tauri = window.__TAURI__;
const invoke = tauri.core.invoke;

const $ = (id) => document.getElementById(id);
const FONT_MIN = 10;
const FONT_MAX = 18;
const OPACITY_MIN = 0;
const OPACITY_MAX = 100;
const OPACITY_STEP = 5;

let busy = false;
let state = null;

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

function say(text, kind = "") {
  $("message").className = `message ${kind}`;
  $("message").textContent = text;
}

function drawAppearance(a) {
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
  for (const b of document.querySelectorAll("[data-backdrop-choice]")) {
    b.setAttribute("role", "radio");
    b.setAttribute("aria-checked", String(b.dataset.backdropChoice === a.backdrop));
  }
  $("font-value").textContent = `${a.font_size}px`;
  $("font-down").disabled = a.font_size <= FONT_MIN;
  $("font-up").disabled = a.font_size >= FONT_MAX;
  // "clear" has no tint to adjust.
  const clear = a.backdrop === "clear";
  $("opacity-value").textContent = clear ? "none" : `${a.opacity}%`;
  $("opacity-down").disabled = clear || a.opacity <= OPACITY_MIN;
  $("opacity-up").disabled = clear || a.opacity >= OPACITY_MAX;
  setToggle("minimal", a.minimal);
  setToggle("info-bar", a.info_bar);
}

async function refresh() {
  try {
    state = await invoke("settings_status");
    $("version").textContent = `v${state.version}`;
    setCheck("check-npcap", state.npcap);
    setCheck("check-game", state.game);
    setCheck("check-data", state.data);
    drawAppearance(state.appearance);
    setToggle("everyone", state.everyone);
    setToggle("record", state.record);
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

// Changes apply at once here; the overlay picks them up on its next refresh.
async function setAppearance(change) {
  try {
    const a = await invoke("set_appearance", { change });
    if (state) state.appearance = a;
    drawAppearance(a);
  } catch (e) {
    say(String(e), "error");
  }
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
$("opacity-down").addEventListener("click", () =>
  setAppearance({ opacity: state.appearance.opacity - OPACITY_STEP }),
);
$("opacity-up").addEventListener("click", () =>
  setAppearance({ opacity: state.appearance.opacity + OPACITY_STEP }),
);
for (const b of document.querySelectorAll("[data-bar-choice]")) {
  b.addEventListener("click", () => setAppearance({ barColor: b.dataset.barChoice }));
}
$("minimal").addEventListener("click", () => setAppearance({ minimal: !state.appearance.minimal }));
$("info-bar").addEventListener("click", () => setAppearance({ infoBar: !state.appearance.info_bar }));

$("everyone").addEventListener("click", async () => {
  await invoke("set_everyone", { everyone: !(state && state.everyone) });
  refresh();
});

$("record").addEventListener("click", async () => {
  await invoke("set_record", { record: !(state && state.record) });
  refresh();
});

$("update").addEventListener("click", async () => {
  if (busy) return;
  busy = true;
  $("update").disabled = true;
  $("update").querySelector(".state").textContent = "[ … ]";
  say("downloading game data (about 10 MB)…");
  try {
    say(`updated: ${await invoke("update_game_data")}`, "ok");
  } catch (e) {
    say(`update failed: ${e}`, "error");
  }
  busy = false;
  $("update").disabled = false;
  $("update").querySelector(".state").textContent = "[ run ]";
  refresh();
});

$("history").addEventListener("click", () => invoke("open_history").catch((e) => say(String(e), "error")));

$("shortcut").addEventListener("click", async () => {
  try {
    say(await invoke("create_shortcuts"), "ok");
  } catch (e) {
    say(`couldn't add the shortcut: ${e}`, "error");
  }
});

$("close").addEventListener("click", () => invoke("hide_settings"));
document.addEventListener("keydown", (e) => {
  if (e.key === "Escape") invoke("hide_settings");
});

refresh();
setInterval(refresh, 2000);
