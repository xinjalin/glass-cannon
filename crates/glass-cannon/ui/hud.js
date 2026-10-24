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
  target: { x: 0.37, y: 0.02, w: 380, h: 170 },
  cooldowns: { x: 0.6, y: 0.66, w: 300, h: 230 },
  "party-cooldowns": { x: 0.015, y: 0.68, w: 330, h: 220 },
  timers: { x: 0.015, y: 0.05, w: 330, h: 280 },
};

// The side widgets: their page, label, whether they take the mouse (the world timers have
// buttons; the others only show), and whether they fade outside combat (all but the world
// timers, which you read between fights).
const SIDE_WIDGETS = {
  target: { page: "target.html", label: "target", passive: true, fades: true },
  cooldowns: { page: "cooldowns.html", label: "cooldowns", passive: true, fades: true },
  "party-cooldowns": { page: "party-cooldowns.html", label: "party cooldowns", passive: true, fades: true },
  timers: { page: "timers.html", label: "world timers", passive: false, fades: false },
};
// Widths up to 1 are fractions of the overlay's width (the warnings span its middle).
const px = (w) => (w <= 1 ? w * innerWidth : w);

let layout = {};
// Heights the widgets asked for to fit (the meter's rows), kept apart from the saved
// layout so loading it doesn't undo them.
const fitted = {};
let editing = false;

function place() {
  for (const el of document.querySelectorAll(".widget")) {
    const id = el.dataset.widget;
    const p = { ...DEFAULTS[id], ...(layout[id] || {}) };
    // A widget that fits its content: the meter exactly; the side widgets up to the
    // height you gave them (they scroll beyond it).
    if (fitted[id]) p.h = el.dataset.side !== undefined ? Math.min(fitted[id], p.h) : fitted[id];
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
  $("edit-bar").hidden = !on;
  place();
}

// A widget can be moved (and resized by its corner) while arranging.
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
        p.w = Math.max(160, start.w + dx);
        if (id !== "meter") p.h = Math.max(60, start.h + dy);
      } else {
        p.x = Math.max(0, start.left + dx) / innerWidth;
        p.y = Math.max(0, start.top + dy) / innerHeight;
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
  place();
}

tauri.event.listen("hud-widgets", (e) => showWidgets(e.payload));

$("edit-done").addEventListener("click", () => invoke("hud_edit", { on: false }));
$("edit-reset").addEventListener("click", () => {
  layout = {};
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
const castName = (c) => c.skill_name || (c.cast_ms ? `${(c.cast_ms / 1000).toFixed(1)} s cast` : "a cast");

// A short two-note chime, once per warning (settings → alert sound).
let audio = null;
function chime() {
  try {
    audio = audio || new AudioContext();
    const t = audio.currentTime;
    for (const [i, f] of [880, 1320].entries()) {
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

let warnedFor = "";
function drawWarnings(elapsed) {
  const box = $("warning");
  const live = boss && boss.cast;
  let show = null;
  if (live && boss.ahead.cast_danger) {
    const castLeft = live.cast_left_ms - elapsed;
    const landsLeft = live.lands_left_ms - elapsed;
    if (castLeft > 0) show = { cls: "now", title: castName(live), time: (castLeft / 1000).toFixed(1), key: `cast:${live.skill}:${boss.castKey}` };
    else if (landsLeft > 0) show = { cls: "now lands", title: castName(live), time: (landsLeft / 1000).toFixed(1), key: `cast:${live.skill}:${boss.castKey}` };
  }
  if (!show && boss) {
    const next = boss.ahead.upcoming.find((c) => c.danger);
    const left = next && next.in_s - elapsed / 1000;
    if (next && left > 0 && left <= PRE_WARN_S) show = { cls: "soon", title: castName(next), time: `in ${Math.ceil(left)}`, key: "" };
  }
  box.hidden = !show;
  if (!show) return;
  box.className = show.cls;
  const nameEl = $("warning-name");
  if (nameEl.textContent !== show.title || nameEl.dataset.cls !== show.cls) {
    nameEl.textContent = show.title;
    nameEl.dataset.cls = show.cls;
    fitLine(nameEl, box.parentElement.clientWidth);
  }
  $("warning-time").textContent = show.time;
  if (show.key && show.key !== warnedFor) {
    warnedFor = show.key;
    if (boss.sound) chime();
  }
}

function drawTimers(elapsed) {
  const list = $("timers");
  const items = boss ? boss.ahead.upcoming.map((c) => ({ c, left: c.in_s - elapsed / 1000 })).filter((x) => x.left > 0) : [];
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
