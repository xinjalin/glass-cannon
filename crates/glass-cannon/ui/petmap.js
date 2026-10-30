// The pet map: each world and Abyss map's picture with its zones (from Questlog's map),
// and where the mobs that give each pet's souls spawn (from Aion DPS Meter's data), as
// dots in their genus' colour. Pick a pet (in the list, or by clicking a dot) to light up
// its spawns and see the areas it's found in; [+] tracks it (the pet tracker widget shows
// the ones you track). The list has this map's pets; a search looks through every pet,
// by name, genus or area. You show as a ring on the map you're on, once the meter knows
// where you are.

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

const matches = (p, q) => p && (p.name.toLowerCase().includes(q) || (p.genus || "").toLowerCase().includes(q) || p.found_in.some((a) => a.toLowerCase().includes(q)));

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
      return !q || matches(p, q);
    });
}

// The list's pets: this map's, or with a search, every pet that matches (those on this
// map first): [id, points here].
function listedPets() {
  const q = search.trim().toLowerCase();
  if (!q) return shownPets();
  const m = current();
  return [...pets.values()]
    .filter((p) => matches(p, q) && (!onlyNeeded || needs(p) || p.id === selected))
    .map((p) => [p.id, (m && m.pets[p.id]) || []]);
}

// The dots' colours, by genus (as Questlog's map has them).
const GENUS = { Fera: "#e5484d", Natura: "#46a758", Cogni: "#3e8ee8", Varian: "#9b6ef3", Special: "#f5b62e" };
const genusColour = (id) => GENUS[(pets.get(id) || {}).genus] || "#ebe1cd";

function petName(id) {
  const p = pets.get(id);
  return p ? p.name : `pet ${id}`;
}

// --- The map ---------------------------------------------------------------

// The picture of the map showing, as Questlog draws its big ones: the image is decoded
// once into levels (full size, half, quarter… down to 512 px), each cut into chunks of
// 2048 px, and only the chunks in view are drawn, from the level nearest the zoom. Only
// the map showing is kept; another map's picture is let go.
const CHUNK_PX = 2048;
const SMALLEST_LEVEL_PX = 512;
let picture = null; // { key, levels: [{ width, chunks: [{ bitmap, u0, v0, u1, v1 }] }] }
let pictureLoading = null;

function dropPicture() {
  if (picture) for (const l of picture.levels) for (const c of l.chunks) c.bitmap.close();
  picture = null;
}

async function loadPicture(m) {
  if (!m || !m.image || (picture && picture.key === m.key) || pictureLoading === m.key) return;
  pictureLoading = m.key;
  dropPicture();
  draw();
  try {
    const url = await invoke("pet_map_image", { file: m.image.file });
    if (!url || current() !== m) return;
    // Through an image (the page may load data URLs as images, not fetch them).
    const img = new Image();
    img.src = url;
    await img.decode();
    const full = await createImageBitmap(img);
    const levels = [];
    for (let k = 1; ; k *= 2) {
      const w = Math.ceil(full.width / k);
      const h = Math.ceil(full.height / k);
      const chunks = [];
      const step = CHUNK_PX * k;
      for (let y = 0; y < full.height; y += step) {
        for (let x = 0; x < full.width; x += step) {
          const cw = Math.min(step, full.width - x);
          const ch = Math.min(step, full.height - y);
          const bitmap = await createImageBitmap(full, x, y, cw, ch, {
            resizeWidth: Math.max(1, Math.round(cw / k)),
            resizeHeight: Math.max(1, Math.round(ch / k)),
            resizeQuality: "high",
          });
          chunks.push({ bitmap, u0: x / full.width, v0: y / full.height, u1: (x + cw) / full.width, v1: (y + ch) / full.height });
        }
      }
      levels.push({ width: w, chunks });
      if (Math.max(w, h) <= SMALLEST_LEVEL_PX) break;
    }
    full.close();
    if (current() !== m) {
      for (const l of levels) for (const c of l.chunks) c.bitmap.close();
      return;
    }
    picture = { key: m.key, levels };
    draw();
  } catch {
    // No picture: the dots and zones still show.
  } finally {
    if (pictureLoading === m.key) pictureLoading = null;
  }
}

function drawPicture(m, w, h) {
  if (!picture || picture.key !== m.key || !m.image) return;
  const { x, y, w: mw, h: mh } = m.image;
  const dpr = window.devicePixelRatio || 1;
  // The level whose pixels are nearest the screen's.
  const perScreenPx = picture.levels[0].width / mw / (view.scale * dpr);
  const k = Math.min(Math.max(Math.floor(Math.log2(perScreenPx)), 0), picture.levels.length - 1);
  ctx.globalAlpha = 0.9;
  for (const c of picture.levels[k].chunks) {
    const [x0, y0] = toPx(x + c.u0 * mw, y + c.v0 * mh);
    const [x1, y1] = toPx(x + c.u1 * mw, y + c.v1 * mh);
    if (x1 < 0 || y1 < 0 || x0 > w || y0 > h) continue;
    ctx.drawImage(c.bitmap, Math.round(x0), Math.round(y0), Math.round(x1) - Math.round(x0), Math.round(y1) - Math.round(y0));
  }
  ctx.globalAlpha = 1;
}

// The zones: their borders, and (below) their names: the big ones always, the small ones
// once zoomed in.
const SMALL_NAMES_FROM = 0.12; // px per metre
function drawZones(m, c) {
  if (!m.zones) return;
  ctx.lineJoin = "round";
  for (const z of m.zones) {
    ctx.strokeStyle = z.rank === 1 ? "rgba(60, 40, 20, 0.55)" : "rgba(60, 40, 20, 0.3)";
    ctx.lineWidth = z.rank === 1 ? 1.4 : 1;
    ctx.beginPath();
    for (const poly of z.polygons) {
      poly.forEach(([x, y], i) => {
        const [px, py] = toPx(x, y);
        if (i) ctx.lineTo(px, py);
        else ctx.moveTo(px, py);
      });
      ctx.closePath();
    }
    ctx.stroke();
  }
}

// Their names, over the dots.
function drawZoneNames(m) {
  if (!m.zones) return;
  const fs = parseFloat(getComputedStyle(document.documentElement).fontSize) || 13;
  ctx.textAlign = "center";
  ctx.textBaseline = "middle";
  // Big zones first; a name that would overlap one already drawn is left out.
  const drawn = [];
  const order = [...m.zones].sort((a, b) => a.rank - b.rank);
  for (const z of order) {
    if (!z.label || (z.rank !== 1 && view.scale < SMALL_NAMES_FROM)) continue;
    const [px, py] = toPx(z.label[0], z.label[1]);
    ctx.font = `${z.rank === 1 ? 600 : 400} ${Math.round(fs * (z.rank === 1 ? 0.95 : 0.8))}px ${getComputedStyle(document.body).fontFamily}`;
    const half = ctx.measureText(z.name).width / 2 + 3;
    const box = [px - half, py - fs * 0.6, px + half, py + fs * 0.6];
    if (drawn.some((d) => box[0] < d[2] && box[2] > d[0] && box[1] < d[3] && box[3] > d[1])) continue;
    drawn.push(box);
    ctx.lineWidth = 3;
    ctx.strokeStyle = "rgba(240, 228, 205, 0.85)";
    ctx.strokeText(z.name, px, py);
    ctx.fillStyle = "rgba(45, 30, 15, 0.95)";
    ctx.fillText(z.name, px, py);
  }
  ctx.textAlign = "start";
  ctx.textBaseline = "alphabetic";
}

function bounds(m) {
  let [x0, y0, x1, y1] = [Infinity, Infinity, -Infinity, -Infinity];
  // The zones when there are some (the whole map), else the spawn points.
  const all = m.zones && m.zones.length ? m.zones.flatMap((z) => z.polygons.flat()) : Object.values(m.pets).flat();
  for (const [x, y] of all) {
    if (x < x0) x0 = x;
    if (x > x1) x1 = x;
    if (y < y0) y0 = y;
    if (y > y1) y1 = y;
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
  drawPicture(m, w, h);
  drawZones(m, c);
  const shown = shownPets();
  const dot = (x, y, r) => {
    const [px, py] = toPx(x, y);
    if (px < -r || py < -r || px > w + r || py > h + r) return;
    ctx.moveTo(px + r, py);
    ctx.arc(px, py, r, 0, Math.PI * 2);
  };
  // Dots with a dark rim, to read on the map's parchment: every other pet's spawns, then
  // the tracked ones, then the one picked on top.
  const layer = (filter, r, fill, alpha) => {
    ctx.beginPath();
    for (const [id, pts] of shown) if (filter(id)) for (const [x, y] of pts) dot(x, y, r);
    ctx.globalAlpha = alpha;
    ctx.lineWidth = 1;
    ctx.strokeStyle = "rgba(20, 14, 8, 0.8)";
    ctx.stroke();
    ctx.fillStyle = fill;
    ctx.fill();
    ctx.globalAlpha = 1;
  };
  // By genus; a picked pet stands out (the others fade back).
  const tracked = (id) => (pets.get(id) || {}).tracked;
  const faded = selected != null ? 0.45 : 0.85;
  for (const g of [...Object.keys(GENUS), null]) {
    const ofGenus = (id) => ((pets.get(id) || {}).genus in GENUS ? (pets.get(id) || {}).genus : null) === g;
    const fill = g ? GENUS[g] : "#ebe1cd";
    layer((id) => id !== selected && !tracked(id) && ofGenus(id), 1.9, fill, faded);
    layer((id) => id !== selected && tracked(id) && ofGenus(id), 2.6, fill, Math.min(1, faded + 0.15));
  }
  if (selected != null) {
    layer((id) => id === selected, 5, "#fff", 1);
    layer((id) => id === selected, 3.6, genusColour(selected), 1);
  }
  drawZoneNames(m);
  // You.
  if (onMap(m) && me.at) {
    const [px, py] = toPx(me.at[0], me.at[1]);
    ctx.globalAlpha = me.exact ? 1 : 0.65;
    ctx.beginPath();
    ctx.arc(px, py, 6, 0, Math.PI * 2);
    ctx.lineWidth = 4;
    ctx.strokeStyle = "rgba(20, 14, 8, 0.85)";
    ctx.stroke();
    ctx.lineWidth = 2;
    ctx.strokeStyle = "#fff";
    ctx.stroke();
    ctx.font = `600 ${getComputedStyle(document.documentElement).fontSize} ${getComputedStyle(document.body).fontFamily}`;
    ctx.lineWidth = 3;
    ctx.strokeStyle = "rgba(20, 14, 8, 0.85)";
    ctx.strokeText(me.exact ? "you" : "you ≈", px + 10, py + 4);
    ctx.fillStyle = "#fff";
    ctx.fillText(me.exact ? "you" : "you ≈", px + 10, py + 4);
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
  // A dot picks its pet (the next one under it, if that one's picked already) and shows
  // it in the list; the map itself, away from the dots, lets go of the pet picked.
  if (under.length) pick(under[0] === selected && under.length > 1 ? under[1] : under[0]);
  else if (selected != null) select(null);
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

// The list's row: picks the pet, or lets go of it.
function select(id) {
  selected = selected === id ? null : id;
  drawList();
  draw();
}

// From the map: picks the pet and scrolls the list to it.
function pick(id) {
  selected = id;
  drawList();
  draw();
  const row = $("pets").querySelector(`[data-pet="${id}"]`);
  if (row) row.scrollIntoView({ block: "center", behavior: "smooth" });
}

async function setTracked(id, on) {
  try {
    await invoke("set_pet_tracked", { id, on });
  } catch (e) {
    $("areas").hidden = false;
    $("areas").textContent = String(e);
    return;
  }
  await loadPets();
}

function drawList() {
  const m = current();
  const rows = listedPets()
    .map(([id, pts]) => [id, pts, pets.get(id)])
    .sort(([a, , pa], [b, , pb]) => needs(pb) - needs(pa) || petName(a).localeCompare(petName(b)));
  $("pets").replaceChildren(
    ...rows.map(([id, pts, p]) => {
      const li = el("li", "map-pet" + (needs(p) ? "" : " done") + (p && p.tracked ? " tracked" : ""));
      li.setAttribute("aria-selected", String(id === selected));
      li.dataset.pet = id;
      // Track it (the pet tracker widget shows the ones you track).
      const track = el("button", "map-track", p && p.tracked ? "✓" : "+");
      track.type = "button";
      track.setAttribute("aria-pressed", String(!!(p && p.tracked)));
      track.title = p && p.tracked ? "Tracked (the pet tracker widget shows it) · click to stop" : "Track it in the pet tracker widget";
      track.addEventListener("click", (e) => {
        e.stopPropagation();
        setTracked(id, !(p && p.tracked));
      });
      const line = el("span", "pet-line");
      const swatch = el("span", "genus-dot");
      swatch.style.background = genusColour(id);
      line.append(swatch, el("span", "pet-name", petName(id)), pips(p ? p.level : null));
      li.append(track, portrait(id, p && p.name), line, el("span", "map-count", pts.length ? `×${pts.length}` : "elsewhere"));
      li.title =
        `${petName(id)}${p && p.genus ? ` (${p.genus})` : ""}: ` +
        (pts.length ? `${pts.length} spawn points on this map` : "not on this map") +
        (p && p.found_in.length ? `\nfound in ${p.found_in.join(", ")}` : "") +
        (p && p.tracked ? "\ntracked" : "");
      li.addEventListener("click", () => select(id));
      return li;
    }),
  );
  if (!rows.length) $("pets").append(el("li", "empty", search.trim() ? "no pet matches" : m ? "no pets you still need here" : "no map data"));
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
        loadPicture(current());
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

// The window grows to what the list and the map need at the font size.
function fitWindow() {
  requestAnimationFrame(() => {
    const fs = parseFloat(getComputedStyle(document.documentElement).fontSize) || 13;
    const side = document.querySelector(".pet-side").offsetWidth;
    invoke("fit_pet_map", { width: Math.ceil(side + 22 * fs), height: Math.ceil(26 * fs) }).catch(() => {});
  });
}
fitWindow();
tauri.event.listen("appearance", fitWindow);

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
  loadPicture(current());
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
