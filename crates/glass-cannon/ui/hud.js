// The overlay over the game: shows the widgets chosen in the settings, places them,
// arranges them (launcher → arrange), fades the fight widgets outside combat, and draws the
// boss warnings and timers from what the meter widget sends up. Text from the game is set
// with textContent, never innerHTML.

const tauri = window.__TAURI__;
const invoke = tauri.core.invoke;
const $ = (id) => document.getElementById(id);

// Where each widget goes at first: its top left as a fraction of the overlay, and its size
// in CSS pixels (the meter's height follows its rows).
const DEFAULTS = {
  meter: { x: 0.72, y: 0.28, w: 420, h: 230 },
  "boss-warnings": { x: 0.3, y: 0.3, w: 0.4, h: 120 },
  "boss-timers": { x: 0.015, y: 0.47, w: 300, h: 150 },
  cubes: { x: 0.72, y: 0.06, w: 280, h: 150 },
  target: { x: 0.37, y: 0.02, w: 380, h: 170 },
  cooldowns: { x: 0.6, y: 0.66, w: 300, h: 230 },
  "party-cooldowns": { x: 0.015, y: 0.68, w: 330, h: 220 },
  timers: { x: 0.015, y: 0.05, w: 330, h: 280 },
  pets: { x: 0.79, y: 0.5, w: 320, h: 230 },
};

// The side widgets: their page, label, whether they take the mouse (the world timers have
// buttons; the others only show), and whether they fade outside combat (all but the world
// timers, which you read between fights).
const SIDE_WIDGETS = {
  target: { page: "target.html", label: "target", passive: true, fades: true },
  cooldowns: { page: "cooldowns.html", label: "cooldowns", passive: true, fades: true },
  "party-cooldowns": { page: "party-cooldowns.html", label: "party cooldowns", passive: true, fades: true },
  timers: { page: "timers.html", label: "world timers", passive: false, fades: false },
  pets: { page: "pets.html", label: "pet tracker", passive: false, fades: false },
};
// Every widget's name in the arrange menu, in the settings' order.
const LABELS = [
  ["meter", "meter"],
  ["boss-warnings", "boss warnings"],
  ["boss-timers", "boss timers"],
  ["cubes", "hidden cubes"],
  ["target", "target frame"],
  ["cooldowns", "cooldowns"],
  ["party-cooldowns", "party cooldowns"],
  ["timers", "world timers"],
  ["pets", "pet tracker"],
];
// The grids widgets can snap to while arranging (CSS pixels; 0: none), and the default.
const GRIDS = [0, 8, 16, 32, 64];
const DEFAULT_GRID = 16;
// Widths up to 1 are fractions of the overlay's width (the warnings span its middle).
const px = (w) => (w <= 1 ? w * innerWidth : w);

// Widget places by name, and "_grid" (the grid they snap to).
let layout = {};
let shown = new Set();
const grid = () => (GRIDS.includes(layout._grid) ? layout._grid : DEFAULT_GRID);
const snap = (v) => (grid() ? Math.round(v / grid()) * grid() : v);
// Heights the widgets asked for to fit (the meter's rows), kept apart from the saved
// layout so loading it doesn't undo them.
const fitted = {};
let editing = false;

function place() {
  for (const el of document.querySelectorAll(".widget")) {
    const id = el.dataset.widget;
    const p = { ...DEFAULTS[id], ...(layout[id] || {}) };
    // A widget that fits its content: the meter exactly; the side widgets up to the
    // height you gave them (they scroll beyond it). While arranging, the side widgets show
    // that full height, the most they'll take, so they're placed clear of each other.
    if (fitted[id]) p.h = el.dataset.side === undefined ? fitted[id] : editing ? p.h : Math.min(fitted[id], p.h);
    const w = px(p.w);
    const x = Math.min(Math.max(0, p.x * innerWidth), innerWidth - Math.min(w, innerWidth));
    const y = Math.min(Math.max(0, p.y * innerHeight), innerHeight - 40);
    Object.assign(el.style, { left: `${x}px`, top: `${y}px`, width: `${w}px`, height: `${p.h}px` });
  }
  reportRects();
}

// The app lets the mouse through everywhere but the widgets that take it, so it needs
// their boxes (in device pixels from the overlay's top left).
let rectsKey = "";
function reportRects() {
  const dpr = window.devicePixelRatio || 1;
  const box = (el) => {
    const r = el.getBoundingClientRect();
    return { x: r.left * dpr, y: r.top * dpr, width: r.width * dpr, height: r.height * dpr };
  };
  const shown = [...document.querySelectorAll(".widget:not([hidden])")];
  const rects = shown.filter((el) => !el.classList.contains("passive")).map(box);
  // The widgets with a panel get a frosted plate behind it (when frosted is chosen), but
  // not while they're faded.
  const faded = document.documentElement.classList.contains("faded") && !editing;
  const panels = shown.filter((el) => el.querySelector("iframe") && !(faded && el.classList.contains("fades"))).map(box);
  const key = JSON.stringify([rects, panels].map((l) => l.map((r) => Object.values(r).map(Math.round))));
  if (key === rectsKey) return;
  rectsKey = key;
  invoke("hud_rects", { rects, panels }).catch(() => {});
}

async function loadLayout() {
  const saved = await invoke("hud_layout").catch(() => null);
  layout = saved && typeof saved === "object" ? saved : {};
  place();
}

function saveLayout() {
  invoke("set_hud_layout", { layout }).catch(() => {});
}

window.addEventListener("resize", place);

// ---------------------------------------------------------------- arranging

function setEditing(on) {
  editing = on;
  document.documentElement.classList.toggle("editing", on);
  $("arrange-menu").hidden = !on;
  drawMenu();
  // On or off with arranging (drawMenu only draws while arranging).
  drawGrid();
  place();
}

// The arrange menu: a switch per widget, the grid sizes.
function drawMenu() {
  if (!editing) return;
  $("menu-widgets").replaceChildren(
    ...LABELS.map(([id, label]) => {
      const b = document.createElement("button");
      b.type = "button";
      b.className = "menu-switch";
      b.setAttribute("aria-pressed", String(shown.has(id)));
      b.append(Object.assign(document.createElement("span"), { textContent: label }));
      b.append(Object.assign(document.createElement("span"), { className: "state", textContent: shown.has(id) ? "[on]" : "[off]" }));
      b.addEventListener("click", () => invoke("toggle_widget", { name: id }).catch(() => {}));
      return b;
    }),
  );
  $("menu-grid").replaceChildren(
    ...GRIDS.map((g) => {
      const b = document.createElement("button");
      b.type = "button";
      b.textContent = g ? `[${g}]` : "[off]";
      b.setAttribute("aria-pressed", String(g === grid()));
      b.title = g ? `Widgets snap to a ${g} px grid` : "Widgets move freely";
      b.addEventListener("click", () => {
        layout._grid = g;
        saveLayout();
        drawGrid();
        drawMenu();
      });
      return b;
    }),
  );
  drawGrid();
}

// The grid shows while arranging.
function drawGrid() {
  const g = grid();
  document.documentElement.classList.toggle("gridded", editing && g > 0);
  document.documentElement.style.setProperty("--grid", `${g}px`);
}

// The menu moves by its title.
$("menu-head").addEventListener("pointerdown", (e) => {
  if (e.button !== 0) return;
  const menu = $("arrange-menu");
  const r = menu.getBoundingClientRect();
  const start = { x: e.clientX, y: e.clientY, left: r.left, top: r.top };
  $("menu-head").setPointerCapture(e.pointerId);
  const move = (m) => {
    const left = Math.min(Math.max(0, start.left + m.clientX - start.x), innerWidth - r.width);
    const top = Math.min(Math.max(0, start.top + m.clientY - start.y), innerHeight - r.height);
    Object.assign(menu.style, { left: `${left}px`, top: `${top}px`, transform: "none" });
  };
  const up = () => {
    $("menu-head").removeEventListener("pointermove", move);
    $("menu-head").removeEventListener("pointerup", up);
  };
  $("menu-head").addEventListener("pointermove", move);
  $("menu-head").addEventListener("pointerup", up);
});

// A widget can be moved (and resized by its corner) while arranging, snapping to the grid.
function setUp(el) {
  const grip = document.createElement("span");
  grip.className = "grip";
  el.append(grip);
  el.addEventListener("pointerdown", (e) => {
    if (!editing || e.button !== 0) return;
    const id = el.dataset.widget;
    const start = { x: e.clientX, y: e.clientY, left: el.offsetLeft, top: el.offsetTop, w: el.offsetWidth, h: el.offsetHeight };
    const resizing = e.target === grip;
    el.setPointerCapture(e.pointerId);
    const move = (m) => {
      const dx = m.clientX - start.x;
      const dy = m.clientY - start.y;
      const p = { ...DEFAULTS[id], ...(layout[id] || {}) };
      if (resizing) {
        p.w = Math.max(160, snap(start.left + start.w + dx) - start.left);
        if (id !== "meter") p.h = Math.max(60, snap(start.top + start.h + dy) - start.top);
      } else {
        p.x = Math.max(0, snap(start.left + dx)) / innerWidth;
        p.y = Math.max(0, snap(start.top + dy)) / innerHeight;
      }
      layout[id] = p;
      place();
    };
    const up = () => {
      el.removeEventListener("pointermove", move);
      el.removeEventListener("pointerup", up);
      saveLayout();
    };
    el.addEventListener("pointermove", move);
    el.addEventListener("pointerup", up);
  });
}

for (const el of document.querySelectorAll(".widget")) setUp(el);

// The widgets chosen in the settings: the meter and the boss widgets show or hide (the
// meter keeps running hidden: the boss widgets go by what it sends up); the side widgets
// are made as they're chosen, and taken away.
function showWidgets(names) {
  const want = new Set(names || []);
  shown = want;
  for (const el of document.querySelectorAll(".widget:not([data-side])")) el.hidden = !want.has(el.dataset.widget);
  for (const el of document.querySelectorAll(".widget[data-side]")) {
    if (!want.has(el.dataset.widget)) el.remove();
  }
  for (const name of want) {
    const w = SIDE_WIDGETS[name];
    if (!w || document.querySelector(`.widget[data-widget="${name}"]`)) continue;
    const el = document.createElement("section");
    el.className = `widget${w.passive ? " passive" : ""}${w.fades ? " fades" : ""}`;
    el.dataset.widget = name;
    el.dataset.side = "";
    el.dataset.label = w.label;
    const frame = document.createElement("iframe");
    frame.src = w.page;
    frame.title = w.label;
    el.append(frame);
    $("widgets").append(el);
    setUp(el);
  }
  drawMenu();
  place();
}

tauri.event.listen("hud-widgets", (e) => showWidgets(e.payload));

// The world map is open (settings → hide on the world map): every widget steps aside at
// once, and comes back at once.
tauri.event.listen("hud-map", (e) => document.documentElement.classList.toggle("map-hidden", !!e.payload));

$("edit-done").addEventListener("click", () => invoke("hud_edit", { on: false }));
$("edit-reset").addEventListener("click", () => {
  layout = { _grid: layout._grid };
  saveLayout();
  place();
});
tauri.event.listen("hud-edit", (e) => setEditing(!!e.payload));

// ---------------------------------------------------------------- from the meter widget

// The meter's page sends its height (it fits its rows), whether the fight widgets are
// faded (settings → fade outside combat), and, each refresh, the boss's cast and what's
// ahead.
let boss = null;
let bossAt = 0;
window.addEventListener("message", (e) => {
  const m = e.data || {};
  if (m.type === "gc-fit" && m.widget) {
    fitted[m.widget] = Math.ceil(m.height);
    place();
  } else if (m.type === "gc-cubes") {
    drawCubes(m.cubes || [], !!m.sound);
  } else if (m.type === "gc-fade") {
    document.documentElement.classList.toggle("faded", !!m.faded);
    reportRects();
  } else if (m.type === "gc-boss") {
    boss = m.boss;
    bossAt = performance.now();
    if (boss && boss.appearance) applyAppearance(boss.appearance);
  }
});

// ---------------------------------------------------------------- boss warnings and timers

const PRE_WARN_S = 5;
const TIMER_SPAN_S = 30;
// A cast's name; most boss skills have none in any data we have (name one in the analysis
// window's [boss] tab), so then the boss's name. With `long`, how long it takes to cast
// too (the timers' bars; the warnings count it down instead).
function castName(c, long = true) {
  if (c.skill_name) return c.skill_name;
  const who = boss && boss.name ? boss.name : "the boss";
  if (!long) return who;
  const what = c.cast_ms ? `${(c.cast_ms / 1000).toFixed(1)} s cast` : "a cast";
  return `${who} · ${what}`;
}

// A short two-note chime, once per warning (settings → alert sound).
let audio = null;
// The boss warnings' rising pair, and the hidden cubes' falling one.
const BOSS_NOTES = [880, 1320];
const CUBE_NOTES = [1568, 1175];
function chime(notes = BOSS_NOTES) {
  try {
    audio = audio || new AudioContext();
    const t = audio.currentTime;
    for (const [i, f] of notes.entries()) {
      const o = audio.createOscillator();
      const g = audio.createGain();
      o.frequency.value = f;
      g.gain.setValueAtTime(0.0001, t + i * 0.12);
      g.gain.exponentialRampToValueAtTime(0.25, t + i * 0.12 + 0.02);
      g.gain.exponentialRampToValueAtTime(0.0001, t + i * 0.12 + 0.11);
      o.connect(g).connect(audio.destination);
      o.start(t + i * 0.12);
      o.stop(t + i * 0.12 + 0.12);
    }
  } catch {
    // No sound, then.
  }
}

// Shrinks a line's font until it fits `width` (the box's padding included), down to a
// third of its size; it never wraps.
function fitLine(el, width) {
  el.style.fontSize = "";
  const box = el.parentElement;
  const pad = box.offsetWidth - box.clientWidth + parseFloat(getComputedStyle(box).paddingLeft) * 2;
  const room = width - pad;
  const full = parseFloat(getComputedStyle(el).fontSize);
  if (el.scrollWidth > room && room > 0) {
    el.style.fontSize = `${Math.max(full / 3, (full * room) / el.scrollWidth)}px`;
  }
}

// A warning stays this long once it's over, dimmed, so it can be read to the end.
const WARNING_HOLD_MS = 1500;
let warnedFor = "";
let lastShown = null;
function drawWarnings(elapsed) {
  const box = $("warning");
  const live = boss && boss.cast;
  let show = null;
  if (live && boss.ahead.cast_danger) {
    const castLeft = live.cast_left_ms - elapsed;
    const landsLeft = live.lands_left_ms - elapsed;
    const key = `cast:${live.skill}:${boss.castKey}`;
    if (castLeft > 0) show = { cls: "now", what: "casting", title: castName(live, false), time: `${(castLeft / 1000).toFixed(1)} s`, key };
    else if (landsLeft > 0) show = { cls: "now lands", what: "lands · move", title: castName(live, false), time: `${(landsLeft / 1000).toFixed(1)} s`, key };
  }
  if (!show && boss) {
    const next = boss.ahead.upcoming.find((c) => c.danger);
    const left = next && next.in_s - elapsed / 1000;
    if (next && left > 0 && left <= PRE_WARN_S) show = { cls: "soon", what: "coming", title: castName(next, false), time: `in ${Math.ceil(left)} s`, key: "" };
  }
  const now = performance.now();
  if (show) {
    lastShown = { ...show, at: now };
  } else if (lastShown && now - lastShown.at < WARNING_HOLD_MS && lastShown.cls !== "soon") {
    // Just over: the same card, dimmed, a moment longer.
    show = { ...lastShown, cls: `${lastShown.cls} over`, time: lastShown.cls.includes("lands") ? "landed" : "now", key: "" };
  }
  box.hidden = !show;
  if (!show) return;
  box.className = show.cls;
  $("warning-what").textContent = show.what;
  const nameEl = $("warning-name");
  if (nameEl.textContent !== show.title || nameEl.dataset.cls !== show.cls) {
    nameEl.textContent = show.title;
    nameEl.dataset.cls = show.cls;
    fitLine(nameEl, box.parentElement.clientWidth);
  }
  $("warning-time").textContent = show.time;
  if (show.key && show.key !== warnedFor) {
    warnedFor = show.key;
    // Only with the warning to see: not while the boss warnings widget is turned off.
    if (boss.sound && !box.closest(".widget").hidden) chime(BOSS_NOTES);
  }
}

function drawTimers(elapsed) {
  const list = $("timers");
  // The casts worth a warning only (2.5 s or more to cast, or marking the ground, unless
  // you marked them otherwise), as the warnings themselves.
  const items = boss ? boss.ahead.upcoming.filter((c) => c.danger).map((c) => ({ c, left: c.in_s - elapsed / 1000 })).filter((x) => x.left > 0) : [];
  while (list.children.length > items.length) list.lastChild.remove();
  items.forEach(({ c, left }, i) => {
    let li = list.children[i];
    if (!li) {
      li = document.createElement("li");
      li.append(Object.assign(document.createElement("span"), { className: "fill" }));
      li.append(Object.assign(document.createElement("span"), { className: "name" }));
      li.append(Object.assign(document.createElement("span"), { className: "left" }));
      list.append(li);
    }
    li.className = `${c.danger ? "danger" : ""} ${left <= PRE_WARN_S ? "soon" : ""}`;
    li.children[0].style.width = `${Math.max(0, Math.min(1, left / TIMER_SPAN_S)) * 100}%`;
    li.children[1].textContent = castName(c);
    li.children[2].textContent = left >= 60 ? `${Math.floor(left / 60)}:${String(Math.floor(left % 60)).padStart(2, "0")}` : left.toFixed(left < 10 ? 1 : 0);
  });
}

// ---------------------------------------------------------------- hidden cubes

// How long a cube that just came into view stands out (and its sound plays once).
const CUBE_NEW_MS = 6000;
// The cubes in view at the last update, by entity: one not among them is new.
let cubesSeen = new Set();

// An alert per hidden cube in view: gold or red, nearby, or taken once opened. The game
// never says where you are as you move, so it can't say which way the cube is: it only
// tells you one is close, with a sound the first time (settings → alert sound).
function drawCubes(cubes, sound) {
  const fresh = cubes.filter((c) => !c.opened && !cubesSeen.has(c.entity));
  cubesSeen = new Set(cubes.map((c) => c.entity));
  if (fresh.length && sound && !$("cubes").closest(".widget").hidden) chime(CUBE_NOTES);
  const list = $("cubes");
  const key = JSON.stringify(cubes.map((c) => [c.entity, c.look, c.opened]));
  if (key === list.dataset.key && !fresh.length) return;
  list.dataset.key = key;
  const isNew = new Set(fresh.map((c) => c.entity));
  list.replaceChildren(
    ...cubes.map((c) => {
      const li = document.createElement("li");
      li.className = `cube ${c.look}${c.opened ? " opened" : ""}${isNew.has(c.entity) ? " new" : ""}`;
      if (isNew.has(c.entity)) setTimeout(() => li.classList.remove("new"), CUBE_NEW_MS);
      li.append(Object.assign(document.createElement("span"), { className: "mark", textContent: "◆" }));
      li.append(Object.assign(document.createElement("span"), { className: "what", textContent: `${c.look} hidden cube` }));
      li.append(Object.assign(document.createElement("span"), { className: "state", textContent: c.opened ? "taken" : "nearby" }));
      return li;
    }),
  );
}

function frame() {
  const elapsed = boss ? performance.now() - bossAt : 0;
  drawWarnings(elapsed);
  drawTimers(elapsed);
  requestAnimationFrame(frame);
}

(async () => {
  const a = await invoke("appearance").catch(() => null);
  if (a) applyAppearance(a);
  tauri.event.listen("appearance", (e) => applyAppearance(e.payload));
  await loadLayout();
  showWidgets(await invoke("widgets_shown").catch(() => []));
  requestAnimationFrame(frame);
})();
