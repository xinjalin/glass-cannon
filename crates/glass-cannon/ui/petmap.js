// The map: each world and Abyss map's picture with its zones (from Questlog's map), and
// one of two sides at a time:
// - pets: where the mobs that give each pet's souls spawn (from Aion DPS Meter's data), as
//   dots in their genus' colour. Pick a pet (in the list, or by clicking a dot) to light up
//   its spawns and see the areas it's found in (picked in the list, the map moves to its
//   spawns); [+] tracks it (the pet tracker widget shows
//   the ones you track). The list has this map's pets; a search looks through every pet,
//   by name, genus or area.
// - gathering: where each material (ore, gems, herbs, logs, food, Odyle) and collectable
//   (hidden cubes, Empyrean traces) is (from Questlog's map), as dots in their kind's
//   colour. ✓ shows or hides one on the map (or a whole kind); pick one to light it up and
//   see which areas have the most.
// You show as a ring on the map you're on, once the meter knows where you are.

const ME_POLL_MS = 1000;
const PETS_POLL_MS = 5000;
// Pixels around the pointer a dot counts as under it.
const HIT_PX = 7;
// What's remembered between openings (only this window's own choices).
const MODE_KEY = "map.mode";
const HIDDEN_KEY = "map.hidden";

let data = null; // { maps: [{ key, name, ids, pets: { id: [[x, y]] }, image, zones }] }
let gather = null; // { kinds: [{ key, name }], items: [{ name, kind, icon, points: { mapId: [[x, y]] } }] }
let pets = new Map(); // id -> the pet tracker's row (name, level, need, found_in, tracked)
let me = null; // { map, at, exact }
let mapIndex = 0;
let mode = remembered(MODE_KEY) === "gather" ? "gather" : "pets";
let selected = null; // the pet picked
let picked = null; // the material picked
let hidden = new Set(); // the materials not shown on the map (by name)
// Which pets show: "all", "missing" (no copies: never summoned), "needed" (not at max
// level) or "tracked"; and the genera turned off. Remembered.
const SHOWS = ["all", "missing", "needed", "tracked"];
const SHOW_KEY = "map.petShow";
const GENERA_KEY = "map.generaOff";
let show = SHOWS.includes(remembered(SHOW_KEY)) ? remembered(SHOW_KEY) : "needed";
let generaOff = new Set();
try {
  generaOff = new Set(JSON.parse(remembered(GENERA_KEY) || "[]"));
} catch {
  // All on, then.
}
// The game has sent this character's pets (else levels are unknown, and every pet counts
// as missing and needed).
let petsKnown = false;
let search = "";
// The view: the centre (metres) and pixels per metre.
let view = null;

const canvas = $("map");
const ctx = canvas.getContext("2d");

const needs = (p) => !p || p.level == null || p.need != null;
const missing = (p) => !p || p.level == null || p.level === 0;
// The filters: how far you've got with it, and its genus. The pet picked always shows.
function passes(id, p) {
  if (id === selected) return true;
  if (p && p.genus && generaOff.has(p.genus)) return false;
  if (show === "missing") return missing(p);
  if (show === "needed") return needs(p);
  if (show === "tracked") return !!(p && p.tracked);
  return true;
}
const current = () => (data ? data.maps[mapIndex] : null);
const onMap = (m) => m && me && me.map != null && m.ids.includes(me.map);

function remembered(key) {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}
function remember(key, value) {
  try {
    localStorage.setItem(key, value);
  } catch {
    // Not kept: fine.
  }
}

// Portraits, by pet (asked for once each).
const portraits = new Map();
function portrait(id, name) {
  if (!portraits.has(id)) {
    portraits.set(id, null);
    invoke("pet_icon", { pet: id })
      .then((url) => {
        if (!url) return portraits.delete(id);
        portraits.set(id, url);
        drawList();
      })
      .catch(() => portraits.delete(id));
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

// Materials' icons, by file (asked for once each).
const itemIcons = new Map();
function icon(item) {
  const file = item.icon;
  if (file && !itemIcons.has(file)) {
    itemIcons.set(file, null);
    invoke("gather_icon", { file })
      .then((url) => {
        if (!url) return itemIcons.delete(file);
        itemIcons.set(file, url);
        drawList();
      })
      .catch(() => itemIcons.delete(file));
  }
  const url = file && itemIcons.get(file);
  if (url) {
    const img = el("img", "pet-icon");
    img.src = url;
    img.alt = "";
    return img;
  }
  const blank = el("span", "pet-icon blank", "");
  blank.style.background = kindColour(item.kind);
  return blank;
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
      return passes(id, p) && (!q || matches(p, q));
    });
}

// The list's pets: this map's, or with a search, every pet that matches (those on this
// map first): [id, points here].
function listedPets() {
  const q = search.trim().toLowerCase();
  if (!q) return shownPets();
  const m = current();
  return [...pets.values()]
    .filter((p) => matches(p, q) && passes(p.id, p))
    .map((p) => [p.id, (m && m.pets[p.id]) || []]);
}

// The dots' colours, by genus (as Questlog's map has them).
const GENUS = { Fera: "#e5484d", Natura: "#46a758", Cogni: "#3e8ee8", Varian: "#9b6ef3", Special: "#f5b62e" };
const genusColour = (id) => GENUS[(pets.get(id) || {}).genus] || "#ebe1cd";

function petName(id) {
  const p = pets.get(id);
  return p ? p.name : `pet ${id}`;
}

// --- Gathering -------------------------------------------------------------

// The dots' colours, by kind.
const KINDS = {
  metal: "#a9b4c2",
  jewelry: "#3ecfd6",
  herb: "#46a758",
  tree: "#a0703c",
  berry: "#e5484d",
  vegetable: "#f08a24",
  shellfish: "#f08fb0",
  od: "#9b6ef3",
  "hidden-cube": "#f5b62e",
  monolith: "#3e8ee8",
};
// The collectables start hidden (there are a lot of them); the materials show.
const HIDDEN_KINDS = ["hidden-cube", "monolith"];
const kindColour = (kind) => KINDS[kind] || "#ebe1cd";
const kindName = (kind) => ((gather && gather.kinds.find((k) => k.key === kind)) || { name: kind }).name;

// This map's materials: [item, points here], those it has.
const itemsCache = new Map();
function itemsOn(m) {
  if (!gather || !m) return [];
  if (!itemsCache.has(m.key)) {
    const list = gather.items.map((it) => [it, m.ids.flatMap((id) => it.points[id] || [])]).filter(([, pts]) => pts.length);
    itemsCache.set(m.key, list);
  }
  return itemsCache.get(m.key);
}

const itemMatches = (it, q) => !q || it.name.toLowerCase().includes(q) || kindName(it.kind).toLowerCase().includes(q);

// The materials on the map: shown ones the search lets through (and the one picked): [name, points].
function shownItems() {
  const q = search.trim().toLowerCase();
  return itemsOn(current())
    .filter(([it]) => it.name === picked || (!hidden.has(it.name) && itemMatches(it, q)))
    .map(([it, pts]) => [it.name, pts]);
}

function setHidden(names, hide) {
  for (const n of names) {
    if (hide) hidden.add(n);
    else hidden.delete(n);
  }
  remember(HIDDEN_KEY, JSON.stringify([...hidden]));
  drawList();
  draw();
}

// Which zone a spot is in: the smallest area that has it, else the region.
function inside(poly, x, y) {
  let on = false;
  for (let i = 0, j = poly.length - 1; i < poly.length; j = i++) {
    const [xi, yi] = poly[i];
    const [xj, yj] = poly[j];
    if (yi > y !== yj > y && x < ((xj - xi) * (y - yi)) / (yj - yi) + xi) on = !on;
  }
  return on;
}
function zoneOf(m, x, y) {
  let best = null;
  for (const z of m.zones || []) {
    if (!z.polygons.some((p) => inside(p, x, y))) continue;
    if (!best || z.rank > best.rank) best = z;
  }
  return best ? best.name : null;
}

// The areas with the most of a material on this map, most first.
const areasCache = new Map();
function mostlyIn(m, name, pts) {
  const key = `${m.key}|${name}`;
  if (!areasCache.has(key)) {
    const count = new Map();
    for (const [x, y] of pts) {
      const z = zoneOf(m, x, y);
      if (z) count.set(z, (count.get(z) || 0) + 1);
    }
    areasCache.set(
      key,
      [...count.entries()].sort((a, b) => b[1] - a[1]).slice(0, 4).map(([z, n]) => `${z} ×${n}`),
    );
  }
  return areasCache.get(key);
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
function drawZones(m) {
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

// The view that frames a box (metres), at least `span` metres across.
function framing([x0, y0, x1, y1], span, margin) {
  const w = canvas.clientWidth || 1;
  const h = canvas.clientHeight || 1;
  const scale = Math.min(w / Math.max(span, x1 - x0), h / Math.max(span, y1 - y0)) * margin;
  return { cx: (x0 + x1) / 2, cy: (y0 + y1) / 2, scale };
}

function fitView() {
  stopGlide();
  const m = current();
  if (!m) return;
  view = framing(bounds(m), 1, 0.9);
}

// A short glide of the view to another (cancelled by a drag, the wheel or a new one).
const GLIDE_MS = 280;
let glide = null;
function stopGlide() {
  if (glide) cancelAnimationFrame(glide);
  glide = null;
}
function glideTo(to) {
  stopGlide();
  if (!view) {
    view = to;
    draw();
    return;
  }
  const from = { ...view };
  const start = performance.now();
  const step = (t) => {
    const k = Math.min(1, (t - start) / GLIDE_MS);
    const e = 1 - (1 - k) ** 3;
    // The zoom eases on a log scale, so zooming in and out feel the same.
    view = {
      cx: from.cx + (to.cx - from.cx) * e,
      cy: from.cy + (to.cy - from.cy) * e,
      scale: Math.exp(Math.log(from.scale) + (Math.log(to.scale) - Math.log(from.scale)) * e),
    };
    draw();
    glide = k < 1 ? requestAnimationFrame(step) : null;
  };
  glide = requestAnimationFrame(step);
}

// A pet picked in the list: the map moves to its spawns: on this map when it has some,
// else on the map with the most of them. They're framed, but not closer than SPAWN_SPAN
// metres across (a few points close together).
const SPAWN_SPAN = 400;
function showSpawns(id) {
  if (!data) return;
  let i = mapIndex;
  if (!((current() && current().pets[id]) || []).length) {
    let most = 0;
    data.maps.forEach((m, k) => {
      const n = (m.pets[id] || []).length;
      if (n > most) [most, i] = [n, k];
    });
    if (!most) return;
  }
  if (i !== mapIndex) switchMap(i);
  const pts = data.maps[i].pets[id];
  const box = [Infinity, Infinity, -Infinity, -Infinity];
  for (const [x, y] of pts) {
    box[0] = Math.min(box[0], x);
    box[1] = Math.min(box[1], y);
    box[2] = Math.max(box[2], x);
    box[3] = Math.max(box[3], y);
  }
  glideTo(framing(box, SPAWN_SPAN, 0.8));
}

// Map metres to canvas pixels (north up: the game's y grows southwards, as the screen's).
const toPx = (x, y) => [(x - view.cx) * view.scale + canvas.clientWidth / 2, (y - view.cy) * view.scale + canvas.clientHeight / 2];
const toMap = (px, py) => [(px - canvas.clientWidth / 2) / view.scale + view.cx, (py - canvas.clientHeight / 2) / view.scale + view.cy];

// One layer of dots with a dark rim, to read on the map's parchment: the spots of the
// entries `filter` lets through.
function dots(shown, filter, r, fill, alpha) {
  const w = canvas.clientWidth;
  const h = canvas.clientHeight;
  ctx.beginPath();
  for (const [key, pts] of shown) {
    if (!filter(key)) continue;
    for (const [x, y] of pts) {
      const [px, py] = toPx(x, y);
      if (px < -r || py < -r || px > w + r || py > h + r) continue;
      ctx.moveTo(px + r, py);
      ctx.arc(px, py, r, 0, Math.PI * 2);
    }
  }
  ctx.globalAlpha = alpha;
  ctx.lineWidth = 1;
  ctx.strokeStyle = "rgba(20, 14, 8, 0.8)";
  ctx.stroke();
  ctx.fillStyle = fill;
  ctx.fill();
  ctx.globalAlpha = 1;
}

// Every other pet's spawns, then the tracked ones, by genus; the one picked on top (the
// others fade back).
function drawPets() {
  const shown = shownPets();
  const tracked = (id) => (pets.get(id) || {}).tracked;
  const faded = selected != null ? 0.45 : 0.85;
  for (const g of [...Object.keys(GENUS), null]) {
    const ofGenus = (id) => ((pets.get(id) || {}).genus in GENUS ? (pets.get(id) || {}).genus : null) === g;
    const fill = g ? GENUS[g] : "#ebe1cd";
    dots(shown, (id) => id !== selected && !tracked(id) && ofGenus(id), 1.9, fill, faded);
    dots(shown, (id) => id !== selected && tracked(id) && ofGenus(id), 2.6, fill, Math.min(1, faded + 0.15));
  }
  if (selected != null) {
    dots(shown, (id) => id === selected, 5, "#fff", 1);
    dots(shown, (id) => id === selected, 3.6, genusColour(selected), 1);
  }
}

// The materials shown, by kind; the one picked on top (the others fade back).
function drawGather() {
  const shown = shownItems();
  const kindOf = new Map(itemsOn(current()).map(([it]) => [it.name, it.kind]));
  const faded = picked != null ? 0.4 : 0.9;
  for (const kind of Object.keys(KINDS)) dots(shown, (n) => n !== picked && kindOf.get(n) === kind, 2.1, KINDS[kind], faded);
  if (picked != null) {
    dots(shown, (n) => n === picked, 5, "#fff", 1);
    dots(shown, (n) => n === picked, 3.6, kindColour(kindOf.get(picked)), 1);
  }
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
  drawPicture(m, w, h);
  drawZones(m);
  if (mode === "gather") drawGather();
  else drawPets();
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

// The pets (or materials) with a dot under the pointer, nearest first.
function dotsAt(px, py) {
  const m = current();
  if (!m || !view) return [];
  const found = new Map();
  for (const [key, pts] of mode === "gather" ? shownItems() : shownPets()) {
    for (const [x, y] of pts) {
      const [qx, qy] = toPx(x, y);
      const d = Math.hypot(qx - px, qy - py);
      if (d <= HIT_PX && (!found.has(key) || d < found.get(key))) found.set(key, d);
    }
  }
  return [...found.entries()].sort((a, b) => a[1] - b[1]).map(([key]) => key);
}

const nameOf = (key) => (mode === "gather" ? key : petName(key));
const pickedNow = () => (mode === "gather" ? picked : selected);

let drag = null;
const wrap = $("map-wrap");
wrap.addEventListener("pointerdown", (e) => {
  if (!view) return;
  stopGlide();
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
  const under = dotsAt(e.clientX - r.left, e.clientY - r.top);
  const tip = $("tip");
  if (!under.length) {
    tip.hidden = true;
    return;
  }
  tip.replaceChildren(el("span", "", under.slice(0, 3).map(nameOf).join(", ")));
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
  const under = dotsAt(e.clientX - r.left, e.clientY - r.top);
  // A dot picks its pet or material (the next one under it, if that one's picked already)
  // and shows it in the list; the map itself, away from the dots, lets go of the one picked.
  const now = pickedNow();
  if (under.length) pick(under[0] === now && under.length > 1 ? under[1] : under[0]);
  else if (now != null) select(null);
});
wrap.addEventListener("pointerleave", () => ($("tip").hidden = true));
wrap.addEventListener(
  "wheel",
  (e) => {
    if (!view) return;
    e.preventDefault();
    stopGlide();
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

// The list's row: picks the pet (or material), or lets go of it. A pet picked moves the
// map to its spawns.
function select(key) {
  if (mode === "gather") picked = picked === key ? null : key;
  else selected = selected === key ? null : key;
  drawList();
  draw();
  if (mode === "pets" && selected != null && selected === key) showSpawns(key);
}

// From the map: picks it and scrolls the list to it.
function pick(key) {
  if (mode === "gather") picked = key;
  else selected = key;
  drawList();
  draw();
  const row = $("pets").querySelector(`[data-key="${CSS.escape(String(key))}"]`);
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
  if (mode === "gather") drawGatherList();
  else drawPetList();
}

function drawPetList() {
  const m = current();
  const rows = listedPets()
    .map(([id, pts]) => [id, pts, pets.get(id)])
    .sort(([a, , pa], [b, , pb]) => needs(pb) - needs(pa) || petName(a).localeCompare(petName(b)));
  $("pets").replaceChildren(
    ...rows.map(([id, pts, p]) => {
      const li = el("li", "map-pet" + (needs(p) ? "" : " done") + (p && p.tracked ? " tracked" : ""));
      li.setAttribute("aria-selected", String(id === selected));
      li.dataset.key = id;
      // Track it (the pet tracker widget shows the ones you track).
      const track = el("button", "map-track", p && p.tracked ? "✓" : "+");
      track.type = "button";
      track.setAttribute("aria-pressed", String(!!(p && p.tracked)));
      track.title = p && p.tracked ? "Tracked (the pet tracker widget shows it) · click to stop" : "Track it in the pet tracker widget";
      // A pet at max level has nothing left to track (one tracked stops when it gets there).
      if (!needs(p) && !p.tracked) {
        track.disabled = true;
        track.title = "At max level: nothing left to track";
      }
      track.addEventListener("click", (e) => {
        e.stopPropagation();
        if (!track.disabled) setTracked(id, !(p && p.tracked));
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
  const none = { all: "no pets here", missing: "no pets here you're missing", needed: "no pets you still need here", tracked: "no tracked pets here" };
  if (!rows.length) $("pets").append(el("li", "empty", search.trim() ? "no pet matches" : m ? none[show] : "no map data"));
  const areas = $("areas");
  const p = selected != null ? pets.get(selected) : null;
  areas.hidden = !p;
  if (p) {
    areas.replaceChildren(el("b", "", p.name), ` · ${p.found_in.length ? `found in ${p.found_in.join(", ")}` : "where it's found isn't known"}`);
    if (p.level != null) areas.append(` · ${p.need == null ? "max level" : `level ${p.level}, ${p.souls}/${p.need} souls`}`);
  }
}

// The show/hide box of a row: ✓ shown, – some shown, empty hidden.
function showBox(state, title, onClick) {
  const b = el("button", "map-track map-show", state === "all" ? "✓" : state === "some" ? "–" : "");
  b.type = "button";
  b.setAttribute("aria-pressed", String(state === "all"));
  b.title = title;
  b.addEventListener("click", (e) => {
    e.stopPropagation();
    onClick();
  });
  return b;
}

// By kind: a kind with more than one material has a row of its own (to show or hide them
// all), its materials under it.
function drawGatherList() {
  const m = current();
  const q = search.trim().toLowerCase();
  const rows = [];
  for (const k of gather ? gather.kinds : []) {
    const items = itemsOn(m).filter(([it]) => it.kind === k.key && itemMatches(it, q));
    if (!items.length) continue;
    if (items.length > 1) {
      const names = items.map(([it]) => it.name);
      const shownCount = names.filter((n) => !hidden.has(n)).length;
      const state = shownCount === names.length ? "all" : shownCount ? "some" : "none";
      const li = el("li", "map-kind" + (state === "none" ? " off" : ""));
      const swatch = el("span", "genus-dot");
      swatch.style.background = kindColour(k.key);
      const line = el("span", "pet-line");
      line.append(swatch, el("span", "pet-name", k.name));
      const total = items.reduce((n, [, pts]) => n + pts.length, 0);
      li.append(showBox(state, state === "all" ? `Hide every ${k.name.toLowerCase()} on the map` : `Show every ${k.name.toLowerCase()} on the map`, () => setHidden(names, state === "all")), line, el("span", "map-count", `×${total}`));
      li.title = `${k.name}: ${total} spots on this map`;
      li.addEventListener("click", () => setHidden(names, state === "all"));
      rows.push(li);
    }
    for (const [it, pts] of items) {
      const off = hidden.has(it.name);
      const li = el("li", "map-pet map-item" + (items.length > 1 ? " sub" : "") + (off ? " off" : ""));
      li.setAttribute("aria-selected", String(it.name === picked));
      li.dataset.key = it.name;
      const box = showBox(off ? "none" : "all", off ? "Show it on the map" : "Hide it on the map", () => setHidden([it.name], !off));
      const line = el("span", "pet-line");
      const swatch = el("span", "genus-dot");
      swatch.style.background = kindColour(it.kind);
      line.append(swatch, el("span", "pet-name", it.name));
      li.append(box, icon(it), line, el("span", "map-count", `×${pts.length}`));
      li.title = `${it.name} (${kindName(it.kind)}): ${pts.length} spots on this map` + (off ? "\nhidden on the map" : "");
      li.addEventListener("click", () => select(it.name));
      rows.push(li);
    }
  }
  $("pets").replaceChildren(...rows);
  if (!rows.length) $("pets").append(el("li", "empty", !gather ? "no gathering data" : q ? "nothing matches" : "nothing to gather on this map"));
  const areas = $("areas");
  const it = picked != null ? itemsOn(m).find(([i]) => i.name === picked) : null;
  areas.hidden = !it;
  if (it) {
    const [item, pts] = it;
    const most = mostlyIn(m, item.name, pts);
    areas.replaceChildren(el("b", "", item.name), ` · ${kindName(item.kind)} · ${pts.length} spots here`);
    if (most.length) areas.append(` · most in ${most.join(", ")}`);
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
      b.addEventListener("click", () => switchMap(i));
      return b;
    }),
  );
  const here = data.maps.find(onMap);
  $("where").textContent = here ? `· you're in ${here.name}` : "";
}

// Shows another map, all of it.
function switchMap(i) {
  mapIndex = i;
  loadPicture(current());
  fitView();
  drawTabs();
  drawList();
  draw();
}

// Pets or gathering: the side shown, on the map and in the list.
function drawMode() {
  for (const b of $("mode").querySelectorAll("button")) b.setAttribute("aria-selected", String(b.dataset.mode === mode));
  $("pet-filters").hidden = mode !== "pets";
  const s = $("search");
  s.placeholder = mode === "gather" ? "search a material" : "search a pet";
  s.title = mode === "gather" ? "A material, by name or kind" : "Any pet, by name, genus or area (not only this map's)";
  $("map").setAttribute("aria-label", mode === "gather" ? "Where each material is gathered" : "Where the mobs that give pet souls spawn");
}
for (const b of $("mode").querySelectorAll("button")) {
  b.addEventListener("click", () => {
    if (mode === b.dataset.mode) return;
    mode = b.dataset.mode;
    remember(MODE_KEY, mode);
    search = "";
    $("search").value = "";
    $("tip").hidden = true;
    drawMode();
    drawList();
    draw();
  });
}
drawMode();

$("search").addEventListener("input", (e) => {
  search = e.target.value;
  drawList();
  draw();
});
function drawFilters() {
  for (const b of document.querySelectorAll("#pet-filters [data-show]")) b.setAttribute("aria-checked", String(b.dataset.show === show));
  for (const b of document.querySelectorAll("#pet-filters [data-genus]")) {
    b.setAttribute("aria-pressed", String(!generaOff.has(b.dataset.genus)));
    b.title = `${b.dataset.genus} pets: ${generaOff.has(b.dataset.genus) ? "hidden · click to show" : "shown · click to hide"}`;
  }
  // Levels unknown: missing and needed can't tell yet.
  $("pets-unknown").hidden = petsKnown || show === "all" || show === "tracked";
}
for (const b of document.querySelectorAll("#pet-filters [data-show]")) {
  b.addEventListener("click", () => {
    show = b.dataset.show;
    remember(SHOW_KEY, show);
    drawFilters();
    drawList();
    draw();
  });
}
for (const b of document.querySelectorAll("#pet-filters [data-genus]")) {
  b.querySelector(".genus-dot").style.background = GENUS[b.dataset.genus];
  b.addEventListener("click", () => {
    const g = b.dataset.genus;
    if (generaOff.has(g)) generaOff.delete(g);
    else generaOff.add(g);
    remember(GENERA_KEY, JSON.stringify([...generaOff]));
    drawFilters();
    drawList();
    draw();
  });
}
drawFilters();

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
    petsKnown = !!t.synced;
    drawFilters();
  } catch {
    return;
  }
  if (mode !== "pets") return;
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

async function loadGather() {
  try {
    gather = await invoke("gather_map");
  } catch {
    gather = null;
  }
  if (!gather || !gather.items) gather = null;
  let names = null;
  try {
    names = JSON.parse(remembered(HIDDEN_KEY));
  } catch {
    names = null;
  }
  hidden = new Set(Array.isArray(names) ? names : gather ? gather.items.filter((it) => HIDDEN_KINDS.includes(it.kind)).map((it) => it.name) : []);
}

(async () => {
  try {
    data = await invoke("pet_map");
  } catch {
    data = null;
  }
  await loadGather();
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
