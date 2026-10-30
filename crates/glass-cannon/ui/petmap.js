// The pet map: on each world and Abyss map, where the mobs that give each pet's souls
// spawn (from Aion DPS Meter's data), as dots: together they trace the map's zones. Pick
// a pet (in the list, or by clicking a dot) to light up its spawns and see the areas it's
// found in; you show as a ring on the map you're on, once the meter knows where you are.

const ME_POLL_MS = 1000;
const PETS_POLL_MS = 5000;
// Pixels around the pointer a dot counts as under it.
const HIT_PX = 7;

let data = null; // { maps: [{ key, name, ids, pets: { id: [[x, y]] } }] }
let pets = new Map(); // id -> the pet tracker's row (name, level, need, found_in, tracked)
let me = null; // { map, at, exact }
let mapIndex = 0;
let selected = null;
let onlyNeeded = true;
let search = "";
// The view: the centre (metres) and pixels per metre.
let view = null;

const canvas = $("map");
const ctx = canvas.getContext("2d");

const needs = (p) => !p || p.level == null || p.need != null;
const current = () => (data ? data.maps[mapIndex] : null);
const onMap = (m) => m && me && me.map != null && m.ids.includes(me.map);

// Portraits, by pet (asked for once each).
const portraits = new Map();
function portrait(id, name) {
  if (!portraits.has(id)) {
    portraits.set(id, null);
    invoke("pet_icon", { pet: id })
      .then((url) => {
        portraits.set(id, url);
        if (url) drawList();
      })
      .catch(() => {});
  }
  const url = portraits.get(id);
  if (url) {
    const img = el("img", "pet-icon");
    img.src = url;
    img.alt = "";
    return img;
  }
  return el("span", "pet-icon blank", (name || "?")[0]);
}

function pips(level) {
  const box = el("span", "pet-pips");
  for (let i = 1; i <= 3; i++) box.append(el("span", "pip" + (level != null && level >= i ? " on" : ""), ""));
  return box;
}

// The pets on this map that the filters let through: [id, points].
function shownPets() {
  const m = current();
  if (!m) return [];
  const q = search.trim().toLowerCase();
  return Object.entries(m.pets)
    .map(([id, pts]) => [Number(id), pts])
    .filter(([id]) => {
      const p = pets.get(id);
      if (onlyNeeded && !needs(p) && id !== selected) return false;
      return !q || (p && p.name.toLowerCase().includes(q));
    });
}

function petName(id) {
  const p = pets.get(id);
  return p ? p.name : `pet ${id}`;
}

// --- The map ---------------------------------------------------------------

function bounds(m) {
  let [x0, y0, x1, y1] = [Infinity, Infinity, -Infinity, -Infinity];
  for (const pts of Object.values(m.pets)) {
    for (const [x, y] of pts) {
      if (x < x0) x0 = x;
      if (x > x1) x1 = x;
      if (y < y0) y0 = y;
      if (y > y1) y1 = y;
    }
  }
  return [x0, y0, x1, y1];
}

function fitView() {
  const m = current();
  if (!m) return;
  const [x0, y0, x1, y1] = bounds(m);
  const w = canvas.clientWidth || 1;
  const h = canvas.clientHeight || 1;
  const scale = Math.min(w / Math.max(1, x1 - x0), h / Math.max(1, y1 - y0)) * 0.9;
  view = { cx: (x0 + x1) / 2, cy: (y0 + y1) / 2, scale };
}

// Map metres to canvas pixels (north up: the game's y grows southwards, as the screen's).
const toPx = (x, y) => [(x - view.cx) * view.scale + canvas.clientWidth / 2, (y - view.cy) * view.scale + canvas.clientHeight / 2];
const toMap = (px, py) => [(px - canvas.clientWidth / 2) / view.scale + view.cx, (py - canvas.clientHeight / 2) / view.scale + view.cy];

function colours() {
  const cs = getComputedStyle(document.documentElement);
  const v = (n) => cs.getPropertyValue(n).trim();
  return { accent: v("--accent"), ok: v("--ok"), muted: v("--muted"), text: v("--text"), faint: v("--faint") };
}

function draw() {
  const m = current();
  const dpr = window.devicePixelRatio || 1;
  const w = canvas.clientWidth;
  const h = canvas.clientHeight;
  if (canvas.width !== Math.round(w * dpr) || canvas.height !== Math.round(h * dpr)) {
    canvas.width = Math.round(w * dpr);
    canvas.height = Math.round(h * dpr);
  }
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  ctx.clearRect(0, 0, w, h);
  if (!m || !view) return;
  const c = colours();
  const shown = shownPets();
  const dot = (x, y, r) => {
    const [px, py] = toPx(x, y);
    if (px < -r || py < -r || px > w + r || py > h + r) return;
    ctx.moveTo(px + r, py);
    ctx.arc(px, py, r, 0, Math.PI * 2);
  };
  // Every other pet's spawns, faint, then the tracked ones, then the one picked on top.
  ctx.globalAlpha = 0.5;
  ctx.fillStyle = c.muted;
  ctx.beginPath();
  for (const [id, pts] of shown) if (id !== selected && !(pets.get(id) || {}).tracked) for (const [x, y] of pts) dot(x, y, 1.6);
  ctx.fill();
  ctx.globalAlpha = 0.85;
  ctx.fillStyle = c.ok;
  ctx.beginPath();
  for (const [id, pts] of shown) if (id !== selected && (pets.get(id) || {}).tracked) for (const [x, y] of pts) dot(x, y, 2.2);
  ctx.fill();
  ctx.globalAlpha = 1;
  if (selected != null && m.pets[selected]) {
    ctx.fillStyle = c.accent;
    ctx.beginPath();
    for (const [x, y] of m.pets[selected]) dot(x, y, 3.5);
    ctx.fill();
  }
  // You.
  if (onMap(m) && me.at) {
    const [px, py] = toPx(me.at[0], me.at[1]);
    ctx.strokeStyle = c.text;
    ctx.lineWidth = 2;
    ctx.globalAlpha = me.exact ? 1 : 0.6;
    ctx.beginPath();
    ctx.arc(px, py, 6, 0, Math.PI * 2);
    ctx.stroke();
    ctx.font = `${getComputedStyle(document.documentElement).fontSize} ${getComputedStyle(document.body).fontFamily}`;
    ctx.fillStyle = c.text;
    ctx.fillText(me.exact ? "you" : "you ≈", px + 9, py + 4);
    ctx.globalAlpha = 1;
  }
}

// The pets with a dot under the pointer, nearest first.
function petsAt(px, py) {
  const m = current();
  if (!m || !view) return [];
  const found = new Map();
  for (const [id, pts] of shownPets()) {
    for (const [x, y] of pts) {
      const [qx, qy] = toPx(x, y);
      const d = Math.hypot(qx - px, qy - py);
      if (d <= HIT_PX && (!found.has(id) || d < found.get(id))) found.set(id, d);
    }
  }
  return [...found.entries()].sort((a, b) => a[1] - b[1]).map(([id]) => id);
}

let drag = null;
const wrap = $("map-wrap");
wrap.addEventListener("pointerdown", (e) => {
  if (!view) return;
  drag = { x: e.clientX, y: e.clientY, cx: view.cx, cy: view.cy, moved: false };
  wrap.setPointerCapture(e.pointerId);
});
wrap.addEventListener("pointermove", (e) => {
  const r = canvas.getBoundingClientRect();
  if (drag) {
    const dx = e.clientX - drag.x;
    const dy = e.clientY - drag.y;
    if (Math.abs(dx) + Math.abs(dy) > 3) drag.moved = true;
    if (drag.moved) {
      wrap.classList.add("dragging");
      view.cx = drag.cx - dx / view.scale;
      view.cy = drag.cy - dy / view.scale;
      $("tip").hidden = true;
      draw();
      return;
    }
  }
  const under = petsAt(e.clientX - r.left, e.clientY - r.top);
  const tip = $("tip");
  if (!under.length) {
    tip.hidden = true;
    return;
  }
  tip.replaceChildren(el("span", "", under.slice(0, 3).map(petName).join(", ")));
  if (under.length > 3) tip.append(el("span", "dim", ` +${under.length - 3}`));
  tip.hidden = false;
  const x = e.clientX - r.left + 12;
  const y = e.clientY - r.top + 12;
  tip.style.left = `${Math.min(x, r.width - tip.offsetWidth - 4)}px`;
  tip.style.top = `${Math.min(y, r.height - tip.offsetHeight - 4)}px`;
});
wrap.addEventListener("pointerup", (e) => {
  const was = drag;
  drag = null;
  wrap.classList.remove("dragging");
  if (!was || was.moved) return;
  const r = canvas.getBoundingClientRect();
  const under = petsAt(e.clientX - r.left, e.clientY - r.top);
  if (under.length) select(under[0] === selected && under.length > 1 ? under[1] : under[0]);
});
wrap.addEventListener("pointerleave", () => ($("tip").hidden = true));
wrap.addEventListener(
  "wheel",
  (e) => {
    if (!view) return;
    e.preventDefault();
    const r = canvas.getBoundingClientRect();
    const [mx, my] = toMap(e.clientX - r.left, e.clientY - r.top);
    const k = Math.exp(-e.deltaY * 0.0015);
    view.scale = Math.min(Math.max(view.scale * k, 0.01), 20);
    // Keep the spot under the pointer where it is.
    const [nx, ny] = toMap(e.clientX - r.left, e.clientY - r.top);
    view.cx += mx - nx;
    view.cy += my - ny;
    draw();
  },
  { passive: false },
);
wrap.addEventListener("dblclick", () => {
  fitView();
  draw();
});
new ResizeObserver(() => {
  if (!view) fitView();
  draw();
}).observe(wrap);

// --- The list --------------------------------------------------------------

function select(id) {
  selected = selected === id ? null : id;
  drawList();
  draw();
}

function drawList() {
  const m = current();
  const rows = shownPets()
    .map(([id, pts]) => [id, pts, pets.get(id)])
    .sort(([a, , pa], [b, , pb]) => needs(pb) - needs(pa) || petName(a).localeCompare(petName(b)));
  $("pets").replaceChildren(
    ...rows.map(([id, pts, p]) => {
      const li = el("li", "map-pet" + (needs(p) ? "" : " done") + (p && p.tracked ? " tracked" : ""));
      li.setAttribute("aria-selected", String(id === selected));
      const line = el("span", "pet-line");
      line.append(el("span", "pet-name", petName(id)), pips(p ? p.level : null));
      li.append(portrait(id, p && p.name), line, el("span", "map-count", `×${pts.length}`));
      li.title = `${petName(id)}${p && p.genus ? ` (${p.genus})` : ""}: ${pts.length} spawn points on this map` + (p && p.tracked ? " · tracked" : "");
      li.addEventListener("click", () => select(id));
      return li;
    }),
  );
  if (!rows.length) $("pets").append(el("li", "empty", m ? "no pets you still need here" : "no map data"));
  const areas = $("areas");
  const p = selected != null ? pets.get(selected) : null;
  areas.hidden = !p;
  if (p) {
    areas.replaceChildren(el("b", "", p.name), ` · ${p.found_in.length ? `found in ${p.found_in.join(", ")}` : "where it's found isn't known"}`);
    if (p.level != null) areas.append(` · ${p.need == null ? "max level" : `level ${p.level}, ${p.souls}/${p.need} souls`}`);
  }
}

function drawTabs() {
  if (!data) return;
  $("tabs").replaceChildren(
    ...data.maps.map((m, i) => {
      const b = el("button", "", `[${m.name}${onMap(m) ? " •" : ""}]`);
      b.type = "button";
      b.setAttribute("role", "tab");
      b.setAttribute("aria-selected", String(i === mapIndex));
      if (onMap(m)) b.title = "You're here";
      b.addEventListener("click", () => {
        mapIndex = i;
        fitView();
        drawTabs();
        drawList();
        draw();
      });
      return b;
    }),
  );
  const here = data.maps.find(onMap);
  $("where").textContent = here ? `· you're in ${here.name}` : "";
}

$("search").addEventListener("input", (e) => {
  search = e.target.value;
  drawList();
  draw();
});
$("needed").addEventListener("click", () => {
  onlyNeeded = !onlyNeeded;
  $("needed").setAttribute("aria-pressed", String(onlyNeeded));
  drawList();
  draw();
});

// --- Data ------------------------------------------------------------------

async function loadPets() {
  try {
    const t = await invoke("pet_tracker");
    pets = new Map(t.pets.map((p) => [p.id, p]));
  } catch {
    return;
  }
  drawList();
  draw();
}

async function loadMe() {
  const was = me ? `${me.map}` : "";
  try {
    me = await invoke("pet_map_me");
  } catch {
    return;
  }
  if (`${me.map}` !== was) drawTabs();
  draw();
}

(async () => {
  try {
    data = await invoke("pet_map");
  } catch {
    data = null;
  }
  if (!data || !data.maps || !data.maps.length) {
    data = null;
    drawList();
    return;
  }
  await Promise.all([loadPets(), loadMe()]);
  // Start on the map you're on, if it's one of these.
  const here = data.maps.findIndex(onMap);
  if (here >= 0) mapIndex = here;
  fitView();
  drawTabs();
  drawList();
  draw();
})();

setInterval(() => {
  if (!document.hidden && data) loadMe();
}, ME_POLL_MS);
setInterval(() => {
  if (!document.hidden && data) loadPets();
}, PETS_POLL_MS);
