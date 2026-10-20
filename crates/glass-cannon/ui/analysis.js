// Glass Cannon analysis: one fight in full. Overview (tiles, DPS over time, players),
// skills, the rotation timeline and buffs. Names come from network traffic, so text is set
// with textContent (or drawn on a canvas), never innerHTML.

const tauri = window.__TAURI__;
const invoke = tauri.core.invoke;
const win = tauri.window.getCurrentWindow();

const POLL_MS = 1000;
const MAX_SERIES = 5; // players drawn in the DPS chart (validated colour slots)
const SMOOTH_S = 5; // DPS-over-time trailing average
const OPENER_CASTS = 24;
const LANE_H = 28;
const AXIS_H = 26;
const HIT = { CRIT: 0x01, BACK: 0x02, FRONT: 0x04, PERFECT: 0x08, DOUBLE: 0x10, DOT: 0x20, FIRST: 0x40 };

const $ = (id) => document.getElementById(id);

let selection = { fight: null, player: null };
let data = null; // what analysis_data returned
let stamp = "";
let tab = "overview";
let target = null; // null = every target
let sort = { key: "damage", dir: -1 };
const zoom = { rotation: null, buffs: null }; // [t0, t1] in ms, or null = fit
let derived = null;

// ---------------------------------------------------------------- helpers

function el(tag, className, text) {
  const e = document.createElement(tag);
  if (className) e.className = className;
  if (text !== undefined) e.textContent = text;
  return e;
}

function fmt(n) {
  if (n >= 1e6) return (n / 1e6).toFixed(2) + "M";
  if (n >= 1e4) return (n / 1e3).toFixed(1) + "K";
  return Math.round(n).toLocaleString("en-US");
}

const pct = (n, of) => (of ? `${((n / of) * 100).toFixed(1)}%` : "–");

function clock(ms) {
  const s = Math.max(0, Math.floor(ms / 1000));
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
}

function gap(ms) {
  if (ms == null) return "–";
  const s = ms / 1000;
  return s >= 60 ? `${Math.floor(s / 60)}m ${Math.round(s % 60)}s` : `${s.toFixed(1)}s`;
}

const css = (name) => getComputedStyle(document.documentElement).getPropertyValue(name).trim();
const seriesColor = (i) => css(`--series-${i + 1}`);

function name(id) {
  return (data && data.names[id]) || `#${id}`;
}

function skillName(id) {
  const m = data && data.skills[id];
  return (m && m.name) || String(id);
}

// Skill icons as images (for canvases) and data URLs, loaded once each.
const iconUrls = new Map();
const iconImages = new Map();
let redrawQueued = false;

function skillIconUrl(skill) {
  const key = data && data.skills[skill] && data.skills[skill].icon_key;
  if (key == null) return null;
  if (!iconUrls.has(key)) {
    iconUrls.set(key, null);
    invoke("skill_icon", { skill: key })
      .then((url) => {
        iconUrls.set(key, url);
        if (url) {
          const img = new Image();
          img.onload = queueRedraw;
          img.src = url;
          iconImages.set(key, img);
        }
        queueRedraw();
      })
      .catch(() => {});
  }
  return iconUrls.get(key);
}

function skillIconImage(skill) {
  skillIconUrl(skill);
  const key = data.skills[skill] && data.skills[skill].icon_key;
  const img = iconImages.get(key);
  return img && img.complete ? img : null;
}

const classIcons = new Map();
function classIconUrl(code) {
  if (code == null) return null;
  if (!classIcons.has(code)) {
    classIcons.set(code, null);
    invoke("class_icon", { code })
      .then((url) => {
        classIcons.set(code, url);
        queueRedraw();
      })
      .catch(() => {});
  }
  return classIcons.get(code);
}

function iconOrBlank(url, className) {
  if (url) {
    const img = el("img", className);
    img.src = url;
    img.alt = "";
    return img;
  }
  return el("span", `${className || ""} blank`);
}

function queueRedraw() {
  if (redrawQueued) return;
  redrawQueued = true;
  requestAnimationFrame(() => {
    redrawQueued = false;
    render();
  });
}

// ---------------------------------------------------------------- deriving stats

function emptyStats() {
  return { damage: 0, hits: 0, multiHits: 0, casts: 0, crit: 0, back: 0, front: 0, perfect: 0, double: 0, multi: 0, min: Infinity, max: 0 };
}

function addHit(s, h) {
  s.damage += h.damage;
  s.hits += 1;
  s.multiHits += h.n;
  if (h.flags & HIT.FIRST) s.casts += 1;
  if (h.flags & HIT.CRIT) s.crit += 1;
  if (h.flags & HIT.BACK) s.back += 1;
  if (h.flags & HIT.FRONT) s.front += 1;
  if (h.flags & HIT.PERFECT) s.perfect += 1;
  if (h.flags & HIT.DOUBLE) s.double += 1;
  if (h.n >= 2) s.multi += 1;
  s.min = Math.min(s.min, h.damage);
  s.max = Math.max(s.max, h.damage);
}

// Everything the views need, for the chosen target filter.
function derive() {
  const hits = target == null ? data.hits : data.hits.filter((h) => h.target === target);
  const players = new Map();
  let first = Infinity;
  let last = 0;
  for (const h of hits) {
    first = Math.min(first, h.t);
    last = Math.max(last, h.t);
    let p = players.get(h.player);
    if (!p) {
      p = { ...emptyStats(), skills: new Map() };
      players.set(h.player, p);
    }
    addHit(p, h);
    let s = p.skills.get(h.skill);
    if (!s) {
      s = { ...emptyStats(), castTimes: [] };
      p.skills.set(h.skill, s);
    }
    addHit(s, h);
    if (h.flags & HIT.FIRST && !(h.flags & HIT.DOT)) s.castTimes.push(h.t);
  }
  const durationMs = target == null ? data.view.duration_s * 1000 : Math.max(1000, last - first);
  // Casts with their hits, per player, in time order.
  const casts = new Map();
  const open = new Map();
  for (const h of hits) {
    if (h.flags & HIT.DOT) continue;
    const key = `${h.player}/${h.skill}/${h.cast}`;
    let c = open.get(key);
    if (h.flags & HIT.FIRST || !c) {
      c = { player: h.player, skill: h.skill, t: h.t, damage: 0, hits: 0, crit: false, back: false, front: false };
      open.set(key, c);
      if (!casts.has(h.player)) casts.set(h.player, []);
      casts.get(h.player).push(c);
    }
    c.damage += h.damage;
    c.hits += h.n;
    c.crit = c.crit || !!(h.flags & HIT.CRIT);
    c.back = c.back || !!(h.flags & HIT.BACK);
    c.front = c.front || !!(h.flags & HIT.FRONT);
  }
  // Players in the full fight's order, so each keeps its colour whatever the filter.
  const order = data.view.players.map((p) => p.id).filter((id) => players.has(id));
  return { hits, players, order, durationMs, casts, endMs: data.view.duration_s * 1000 };
}

function selectedPlayer() {
  return derived && derived.players.get(selection.player);
}

// ---------------------------------------------------------------- header, chips, tabs

function drawHeader() {
  const v = data.view;
  const when = data.ended_ms ? ` · ${new Date(data.ended_ms).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}` : "";
  const boss = v.target_boss || (v.target_id != null && data.bosses.includes(v.target_id)) ? "BOSS " : "";
  $("fight-label").textContent = `${boss}${name(v.target_id) || v.target || "?"}${v.target_count > 1 ? ` +${v.target_count - 1}` : ""} · ${clock(v.duration_s * 1000)} · ${fmt(v.total_damage)}${when}`;
  $("live").hidden = !data.live;
}

function chip(label, sub, checked, onClick, iconUrl) {
  const b = el("button", "chip");
  b.type = "button";
  b.setAttribute("role", "radio");
  b.setAttribute("aria-checked", String(checked));
  if (iconUrl !== undefined) b.append(iconOrBlank(iconUrl, ""));
  b.append(el("span", "", label));
  if (sub) b.append(el("span", "sub", sub));
  b.addEventListener("click", onClick);
  return b;
}

function drawChips() {
  const players = $("players");
  players.replaceChildren();
  for (const p of data.view.players) {
    const stats = derived.players.get(p.id);
    if (!stats) continue;
    const dps = stats.damage / (derived.durationMs / 1000);
    players.append(
      chip(p.name, fmt(dps), p.id === selection.player, () => selectPlayer(p.id), classIconUrl(p.class_code)),
    );
  }
  const targets = $("targets");
  targets.replaceChildren(chip("all", null, target == null, () => setTarget(null)));
  const ids = [...new Set(data.hits.map((h) => h.target))];
  const byDamage = new Map();
  for (const h of data.hits) byDamage.set(h.target, (byDamage.get(h.target) || 0) + h.damage);
  ids.sort((a, b) => byDamage.get(b) - byDamage.get(a));
  if (ids.length > 1) {
    for (const id of ids.slice(0, 8)) {
      targets.append(chip(name(id), fmt(byDamage.get(id)), target === id, () => setTarget(id)));
    }
  }
}

function drawTabs() {
  for (const b of document.querySelectorAll("#tabs [data-tab]")) {
    b.setAttribute("aria-selected", String(b.dataset.tab === tab));
  }
  for (const s of document.querySelectorAll(".tab")) s.hidden = !data || s.id !== `tab-${tab}`;
}

// ---------------------------------------------------------------- overview

function tile(label, value, detail) {
  const t = el("div", "tile");
  t.append(el("div", "label", label), el("div", "value", value));
  if (detail) t.append(el("div", "detail", detail));
  return t;
}

function drawTiles() {
  const p = selectedPlayer();
  const box = $("tiles");
  box.replaceChildren();
  if (!p) return;
  const secs = derived.durationMs / 1000;
  const all = [...derived.players.values()].reduce((a, s) => a + s.damage, 0);
  box.append(
    tile("dps", fmt(p.damage / secs), `${clock(derived.durationMs)} fight`),
    tile("damage", fmt(p.damage), `${pct(p.damage, all)} of everyone's`),
    tile("casts", String(p.casts), `${(p.casts / (secs / 60)).toFixed(1)} per minute`),
    tile("hits", String(p.multiHits), `${p.hits} events · multi ${pct(p.multi, p.hits)}`),
    tile("crit", pct(p.crit, p.hits), `${p.crit} hits`),
    tile("back attack", pct(p.back, p.hits), `front ${pct(p.front, p.hits)}`),
    tile("perfect", pct(p.perfect, p.hits), `double ${pct(p.double, p.hits)}`),
    tile("max hit", fmt(p.max), `min ${fmt(p.min === Infinity ? 0 : p.min)}`),
  );
}

function setupCanvas(canvas, height) {
  const dpr = window.devicePixelRatio || 1;
  const width = canvas.parentElement.clientWidth;
  canvas.style.height = `${height}px`;
  canvas.width = Math.max(1, Math.round(width * dpr));
  canvas.height = Math.max(1, Math.round(height * dpr));
  const ctx = canvas.getContext("2d");
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  return { ctx, width, height };
}

function niceStep(max, ticks) {
  const raw = max / ticks;
  const mag = 10 ** Math.floor(Math.log10(raw || 1));
  for (const m of [1, 2, 2.5, 5, 10]) if (raw <= m * mag) return m * mag;
  return 10 * mag;
}

function timeStep(spanMs, px) {
  const want = spanMs / Math.max(2, px / 90);
  for (const s of [1000, 2000, 5000, 10000, 15000, 30000, 60000, 120000, 300000, 600000]) if (s >= want) return s;
  return 600000;
}

const fontPx = () => parseFloat(getComputedStyle(document.documentElement).fontSize) || 13;
const canvasFont = (scale = 0.85) => `${Math.round(fontPx() * scale)}px ${css("--mono")}`;

// Per-second damage for the top players, as a 5 s trailing average.
function dpsSeries() {
  const seconds = Math.max(1, Math.ceil(derived.endMs / 1000) + 1);
  const ids = derived.order.slice(0, MAX_SERIES);
  return ids.map((id, i) => {
    const perSec = new Float64Array(seconds);
    for (const h of derived.hits) if (h.player === id) perSec[Math.min(seconds - 1, Math.floor(h.t / 1000))] += h.damage;
    const dps = new Float64Array(seconds);
    let sum = 0;
    for (let s = 0; s < seconds; s++) {
      sum += perSec[s];
      if (s >= SMOOTH_S) sum -= perSec[s - SMOOTH_S];
      dps[s] = sum / Math.min(SMOOTH_S, s + 1);
    }
    return { id, colour: seriesColor(i), dps };
  });
}

let dpsHover = null;

function drawDpsChart() {
  const box = $("dps-chart");
  const canvas = box.querySelector("canvas");
  const { ctx, width, height } = setupCanvas(canvas, box.clientHeight || 200);
  ctx.clearRect(0, 0, width, height);
  const series = dpsSeries();
  const left = 4.5 * fontPx();
  const bottom = height - AXIS_H;
  const right = width - 8;
  const top = 8;
  let max = 1;
  for (const s of series) for (const v of s.dps) max = Math.max(max, v);
  const step = niceStep(max, 4);
  max = Math.ceil(max / step) * step;
  const seconds = series.length ? series[0].dps.length : 1;
  const x = (s) => left + ((right - left) * s) / Math.max(1, seconds - 1);
  const y = (v) => bottom - ((bottom - top) * v) / max;

  // Recessive grid and axes.
  ctx.font = canvasFont();
  ctx.fillStyle = css("--faint");
  ctx.strokeStyle = css("--line-faint");
  ctx.lineWidth = 1;
  ctx.textAlign = "right";
  ctx.textBaseline = "middle";
  for (let v = 0; v <= max; v += step) {
    ctx.beginPath();
    ctx.moveTo(left, Math.round(y(v)) + 0.5);
    ctx.lineTo(right, Math.round(y(v)) + 0.5);
    ctx.stroke();
    ctx.fillText(fmt(v), left - 6, y(v));
  }
  ctx.textAlign = "center";
  ctx.textBaseline = "top";
  const tstep = timeStep(seconds * 1000, right - left) / 1000;
  for (let s = 0; s < seconds; s += tstep) ctx.fillText(clock(s * 1000), x(s), bottom + 6);

  for (const s of series) {
    ctx.strokeStyle = s.colour;
    ctx.lineWidth = s.id === selection.player ? 2.5 : 2;
    ctx.lineJoin = "round";
    ctx.beginPath();
    s.dps.forEach((v, i) => (i ? ctx.lineTo(x(i), y(v)) : ctx.moveTo(x(i), y(v))));
    ctx.stroke();
  }

  // Crosshair and tooltip.
  const tip = box.querySelector(".tooltip");
  if (dpsHover != null && series.length) {
    const s = Math.max(0, Math.min(seconds - 1, Math.round(((dpsHover - left) / (right - left)) * (seconds - 1))));
    ctx.strokeStyle = css("--muted");
    ctx.lineWidth = 1;
    ctx.beginPath();
    ctx.moveTo(Math.round(x(s)) + 0.5, top);
    ctx.lineTo(Math.round(x(s)) + 0.5, bottom);
    ctx.stroke();
    tip.replaceChildren(el("div", "t-head", clock(s * 1000)));
    const rows = series.map((ser) => ({ ser, v: ser.dps[s] })).sort((a, b) => b.v - a.v);
    for (const { ser, v } of rows) {
      ctx.fillStyle = ser.colour;
      ctx.strokeStyle = css("--ink");
      ctx.lineWidth = 2;
      ctx.beginPath();
      ctx.arc(x(s), y(v), 4, 0, Math.PI * 2);
      ctx.fill();
      ctx.stroke();
      const row = el("div", "t-row");
      const label = el("span");
      const dot = el("span", "dot");
      dot.style.background = ser.colour;
      label.append(dot, document.createTextNode(name(ser.id)));
      row.append(label, el("span", "", `${fmt(v)}/s`));
      tip.append(row);
    }
    tip.hidden = false;
    placeTooltip(tip, box, x(s), top + 10);
  } else {
    tip.hidden = true;
  }

  const legend = $("dps-legend");
  legend.replaceChildren();
  for (const s of series) {
    const item = el("span", "item");
    const line = el("span", "line");
    line.style.background = s.colour;
    item.append(line, document.createTextNode(name(s.id)));
    legend.append(item);
  }
}

function placeTooltip(tip, box, x, y) {
  const w = tip.offsetWidth;
  const left = x + 14 + w > box.clientWidth ? x - 14 - w : x + 14;
  tip.style.left = `${Math.max(0, left)}px`;
  tip.style.top = `${y}px`;
}

function th(label, key, left) {
  const h = el("th", left ? "left" : "", label);
  if (key) {
    h.dataset.key = key;
    if (sort.key === key) h.classList.add("sorted");
  }
  return h;
}

function barCell(text, fraction) {
  const td = el("td", "bar-cell");
  const bar = el("span", "bar");
  bar.style.width = `${Math.max(0, Math.min(1, fraction)) * 100}%`;
  td.append(bar, el("span", "", text));
  return td;
}

function drawPlayersTable() {
  const table = $("players-table");
  table.replaceChildren();
  const head = el("tr");
  head.append(th("#"), th("player", null, true), th("dps"), th("damage"), th("share"), th("casts"), th("crit"), th("back"), th("front"), th("perfect"), th("double"), th("max"));
  table.append(head);
  const secs = derived.durationMs / 1000;
  const all = [...derived.players.values()].reduce((a, s) => a + s.damage, 0);
  const top = derived.order.length ? derived.players.get(derived.order[0]).damage : 1;
  derived.order.forEach((id, i) => {
    const s = derived.players.get(id);
    const p = data.view.players.find((r) => r.id === id);
    const tr = el("tr", `clickable${id === selection.player ? " selected" : ""}`);
    const who = el("td", "left");
    const cell = el("div", "cell-name");
    cell.append(iconOrBlank(classIconUrl(p && p.class_code), ""), el("span", "", name(id)));
    who.append(cell);
    tr.append(
      el("td", "muted", String(i + 1)),
      who,
      barCell(fmt(s.damage / secs), s.damage / top),
      el("td", "", fmt(s.damage)),
      el("td", "", pct(s.damage, all)),
      el("td", "", String(s.casts)),
      el("td", "", pct(s.crit, s.hits)),
      el("td", "", pct(s.back, s.hits)),
      el("td", "", pct(s.front, s.hits)),
      el("td", "", pct(s.perfect, s.hits)),
      el("td", "", pct(s.double, s.hits)),
      el("td", "", fmt(s.max)),
    );
    tr.addEventListener("click", () => selectPlayer(id));
    table.append(tr);
  });
}

// ---------------------------------------------------------------- skills

const SKILL_COLUMNS = [
  ["damage", "damage", (s) => s.damage],
  ["%", "share", (s, p) => s.damage / p.damage],
  ["casts", "casts", (s) => s.casts],
  ["multi", "multi", (s) => s.multi / s.hits],
  ["crit", "crit", (s) => s.crit / s.hits],
  ["hits", "hits", (s) => s.hits],
  ["min", "min", (s) => s.min],
  ["max", "max", (s) => s.max],
  ["avg", "avg", (s) => s.damage / s.hits],
  ["double", "double", (s) => s.double / s.hits],
  ["perfect", "perfect", (s) => s.perfect / s.hits],
  ["back", "back", (s) => s.back / s.hits],
  ["front", "front", (s) => s.front / s.hits],
  ["cast gap", "gap", (s) => castGap(s) ?? Infinity],
];

function castGap(s) {
  const t = s.castTimes;
  if (t.length < 2) return null;
  return (t[t.length - 1] - t[0]) / (t.length - 1);
}

function drawSkills() {
  const p = selectedPlayer();
  const table = $("skills-table");
  table.replaceChildren();
  $("skills-legend").textContent = `skills · ${name(selection.player)}`;
  const summary = $("skills-summary");
  summary.replaceChildren();
  if (!p) return;
  const secs = derived.durationMs / 1000;
  const add = (label, value) => {
    const s = el("span", "", `${label} `);
    s.append(el("b", "", value));
    summary.append(s);
  };
  add("dps", fmt(p.damage / secs));
  add("casts", String(p.casts));
  add("hits", `${p.multiHits}`);
  add("crit", pct(p.crit, p.hits));
  add("back", pct(p.back, p.hits));
  add("front", pct(p.front, p.hits));
  add("perfect", pct(p.perfect, p.hits));
  add("double", pct(p.double, p.hits));

  const head = el("tr");
  head.append(th("skill", "name", true));
  for (const [label, key] of SKILL_COLUMNS) head.append(th(label, key));
  table.append(head);
  const col = SKILL_COLUMNS.find((c) => c[1] === sort.key);
  const rows = [...p.skills.entries()].map(([id, s]) => ({ id, s }));
  rows.sort((a, b) => {
    if (sort.key === "name") return sort.dir * skillName(a.id).localeCompare(skillName(b.id));
    const va = col ? col[2](a.s, p) : 0;
    const vb = col ? col[2](b.s, p) : 0;
    return sort.dir * (va - vb) || b.s.damage - a.s.damage;
  });
  const top = Math.max(...rows.map((r) => r.s.damage), 1);
  for (const { id, s } of rows) {
    const tr = el("tr");
    const who = el("td", "left");
    const cell = el("div", "cell-name");
    cell.title = `skill ${id}`;
    cell.append(iconOrBlank(skillIconUrl(id), ""), el("span", "", skillName(id)));
    who.append(cell);
    tr.append(
      who,
      barCell(fmt(s.damage), s.damage / top),
      el("td", "", pct(s.damage, p.damage)),
      el("td", "", String(s.casts)),
      el("td", "", pct(s.multi, s.hits)),
      el("td", "", pct(s.crit, s.hits)),
      el("td", "", String(s.hits)),
      el("td", "", fmt(s.min === Infinity ? 0 : s.min)),
      el("td", "", fmt(s.max)),
      el("td", "", fmt(s.damage / s.hits)),
      el("td", "", pct(s.double, s.hits)),
      el("td", "", pct(s.perfect, s.hits)),
      el("td", "", pct(s.back, s.hits)),
      el("td", "", pct(s.front, s.hits)),
      el("td", "", gap(castGap(s))),
    );
    table.append(tr);
  }
}

$("skills-table").addEventListener("click", (e) => {
  const h = e.target.closest("th[data-key]");
  if (!h) return;
  sort = h.dataset.key === sort.key ? { key: sort.key, dir: -sort.dir } : { key: h.dataset.key, dir: h.dataset.key === "name" ? 1 : -1 };
  drawSkills();
});

// ---------------------------------------------------------------- timelines (shared)

function viewWindow(which) {
  const end = Math.max(1000, derived.endMs);
  return zoom[which] || [0, end];
}

function zoomBy(which, factor, centreFrac = 0.5) {
  const end = Math.max(1000, derived.endMs);
  if (!factor) {
    zoom[which] = null;
  } else {
    const [t0, t1] = viewWindow(which);
    const centre = t0 + (t1 - t0) * centreFrac;
    let span = Math.min(end, Math.max(2000, (t1 - t0) * factor));
    let a = centre - span * centreFrac;
    a = Math.max(0, Math.min(end - span, a));
    zoom[which] = span >= end ? null : [a, a + span];
  }
  render();
}

function panBy(which, dxFrac) {
  const end = Math.max(1000, derived.endMs);
  const [t0, t1] = viewWindow(which);
  const span = t1 - t0;
  if (span >= end) return;
  const a = Math.max(0, Math.min(end - span, t0 - dxFrac * span));
  zoom[which] = [a, a + span];
  render();
}

for (const b of document.querySelectorAll("[data-zoom]")) {
  b.addEventListener("click", () => zoomBy(b.dataset.zoom, Number(b.dataset.step)));
}

// Wheel zoom and drag pan on a timeline canvas.
function attachTimeline(boxId, which, gutter, onHover) {
  const box = $(boxId);
  const canvas = box.querySelector("canvas");
  let drag = null;
  canvas.addEventListener("wheel", (e) => {
    e.preventDefault();
    const rect = canvas.getBoundingClientRect();
    const frac = Math.max(0, Math.min(1, (e.clientX - rect.left - gutter()) / (rect.width - gutter() - 8)));
    zoomBy(which, e.deltaY < 0 ? 0.8 : 1.25, frac);
  });
  canvas.addEventListener("mousedown", (e) => {
    drag = { x: e.clientX };
  });
  window.addEventListener("mouseup", () => (drag = null));
  canvas.addEventListener("mousemove", (e) => {
    const rect = canvas.getBoundingClientRect();
    if (drag && e.buttons & 1) {
      panBy(which, (e.clientX - drag.x) / (rect.width - gutter() - 8));
      drag.x = e.clientX;
      return;
    }
    onHover(e.clientX - rect.left, e.clientY - rect.top);
  });
  canvas.addEventListener("mouseleave", () => onHover(null, null));
}

function drawTimeAxis(ctx, x0, x1, y, t0, t1) {
  ctx.font = canvasFont();
  ctx.fillStyle = css("--faint");
  ctx.strokeStyle = css("--line-faint");
  ctx.textAlign = "center";
  ctx.textBaseline = "top";
  const step = timeStep(t1 - t0, x1 - x0);
  for (let t = Math.ceil(t0 / step) * step; t <= t1; t += step) {
    const x = x0 + ((x1 - x0) * (t - t0)) / (t1 - t0);
    ctx.beginPath();
    ctx.moveTo(Math.round(x) + 0.5, 0);
    ctx.lineTo(Math.round(x) + 0.5, y);
    ctx.stroke();
    ctx.fillText(clock(t), x, y + 6);
  }
}

function clipText(ctx, text, width) {
  if (ctx.measureText(text).width <= width) return text;
  let t = text;
  while (t.length > 1 && ctx.measureText(`${t}…`).width > width) t = t.slice(0, -1);
  return `${t}…`;
}

// ---------------------------------------------------------------- rotation

let rotationHover = null;
let rotationLayout = null;
const rotationGutter = () => 13 * fontPx();

function drawOpener() {
  const box = $("opener");
  box.replaceChildren();
  const casts = (derived.casts.get(selection.player) || []).slice(0, OPENER_CASTS);
  if (!casts.length) {
    box.append(el("span", "note", "no casts"));
    return;
  }
  casts.forEach((c, i) => {
    const step = el("span", "step");
    if (i) step.append(document.createTextNode("›"));
    const icon = iconOrBlank(skillIconUrl(c.skill), "");
    icon.title = `${skillName(c.skill)} · ${clock(c.t)} · ${fmt(c.damage)}`;
    step.append(icon);
    box.append(step);
  });
}

function drawRotation() {
  const box = $("rotation");
  const canvas = box.querySelector("canvas");
  const casts = derived.casts.get(selection.player) || [];
  const p = selectedPlayer();
  const lanes = p ? [...p.skills.entries()].filter(([, s]) => s.castTimes.length).sort((a, b) => b[1].damage - a[1].damage).map(([id]) => id) : [];
  const height = Math.max(LANE_H * 2, lanes.length * LANE_H) + AXIS_H;
  const { ctx, width } = setupCanvas(canvas, height);
  ctx.clearRect(0, 0, width, height);
  const gutter = rotationGutter();
  const [t0, t1] = viewWindow("rotation");
  const x = (t) => gutter + ((width - 8 - gutter) * (t - t0)) / (t1 - t0);
  const plotBottom = lanes.length * LANE_H;
  drawTimeAxis(ctx, gutter, width - 8, plotBottom, t0, t1);

  const icon = Math.round(LANE_H * 0.7);
  ctx.textBaseline = "middle";
  ctx.textAlign = "left";
  lanes.forEach((skill, i) => {
    const yMid = i * LANE_H + LANE_H / 2;
    if (i % 2) {
      ctx.fillStyle = "rgba(255,255,255,0.025)";
      ctx.fillRect(0, i * LANE_H, width, LANE_H);
    }
    const img = skillIconImage(skill);
    if (img) ctx.drawImage(img, 4, yMid - icon / 2, icon, icon);
    ctx.fillStyle = css("--text");
    ctx.font = canvasFont();
    ctx.fillText(clipText(ctx, skillName(skill), gutter - icon - 16), icon + 10, yMid);
  });

  rotationLayout = { lanes, x, gutter, marks: [] };
  const laneOf = new Map(lanes.map((s, i) => [s, i]));
  ctx.save();
  ctx.beginPath();
  ctx.rect(gutter, 0, width - gutter, plotBottom);
  ctx.clip();
  for (const c of casts) {
    const lane = laneOf.get(c.skill);
    if (lane == null || c.t < t0 - 2000 || c.t > t1 + 2000) continue;
    const cx = x(c.t);
    const cy = lane * LANE_H + LANE_H / 2;
    const img = skillIconImage(c.skill);
    if (img) {
      ctx.drawImage(img, cx - icon / 2, cy - icon / 2, icon, icon);
    } else {
      ctx.fillStyle = css("--accent");
      ctx.fillRect(cx - 3, cy - icon / 2, 6, icon);
    }
    if (c.crit) {
      ctx.strokeStyle = css("--accent-2");
      ctx.lineWidth = 1.5;
      ctx.strokeRect(cx - icon / 2 - 1.5, cy - icon / 2 - 1.5, icon + 3, icon + 3);
    }
    if (c.back) {
      ctx.fillStyle = css("--text");
      ctx.beginPath();
      ctx.arc(cx, cy + icon / 2 + 2.5, 1.8, 0, Math.PI * 2);
      ctx.fill();
    }
    rotationLayout.marks.push({ c, cx, cy });
  }
  ctx.restore();

  const tip = box.querySelector(".tooltip");
  const hit = rotationHover && nearestMark(rotationLayout.marks, rotationHover);
  if (hit) {
    const { c } = hit;
    tip.replaceChildren(el("div", "t-head", `${skillName(c.skill)} · ${clock(c.t)}`));
    const row = (a, b) => {
      const r = el("div", "t-row");
      r.append(el("span", "", a), el("span", "", b));
      tip.append(r);
    };
    row("damage", fmt(c.damage));
    row("hits", String(c.hits));
    row("crit", c.crit ? "yes" : "no");
    row("position", c.back ? "back" : c.front ? "front" : "side");
    tip.hidden = false;
    placeTooltip(tip, box, hit.cx, hit.cy + 12);
  } else {
    tip.hidden = true;
  }
}

function nearestMark(marks, [mx, my]) {
  let best = null;
  let bestD = 14 * 14;
  for (const m of marks) {
    const d = (m.cx - mx) ** 2 + (m.cy - my) ** 2;
    if (d < bestD) {
      bestD = d;
      best = m;
    }
  }
  return best;
}

attachTimeline("rotation", "rotation", rotationGutter, (mx, my) => {
  rotationHover = mx == null ? null : [mx, my];
  drawRotation();
});

// ---------------------------------------------------------------- buffs

let buffHover = null;
let buffLayout = null;
const buffGutter = () => 17 * fontPx();

function effectLabel(e) {
  return e.skill != null ? skillName(e.skill) : `effect ${e.effect}`;
}

// Lanes of effect spans: on the player (buffs), then on the target (debuffs).
function buffLanes() {
  const debuffTarget = target != null ? target : data.view.target_id;
  const lanes = new Map();
  for (const e of data.effects) {
    let kind = null;
    if (e.target === selection.player) kind = "buff";
    else if (e.target === debuffTarget) kind = "debuff";
    if (!kind) continue;
    const key = `${kind}/${e.skill ?? `e${e.effect}`}`;
    if (!lanes.has(key)) lanes.set(key, { kind, label: effectLabel(e), skill: e.skill, spans: [] });
    lanes.get(key).spans.push(e);
  }
  const end = Math.max(1000, derived.endMs);
  for (const lane of lanes.values()) {
    // Uptime: the union of the spans, as a share of the fight.
    const spans = [...lane.spans].sort((a, b) => a.start - b.start);
    let covered = 0;
    let curS = -1;
    let curE = -1;
    for (const s of spans) {
      if (s.start > curE) {
        if (curE > curS) covered += curE - curS;
        curS = s.start;
        curE = s.end;
      } else curE = Math.max(curE, s.end);
    }
    if (curE > curS) covered += curE - curS;
    lane.uptime = Math.min(1, covered / end);
  }
  const list = [...lanes.values()];
  list.sort((a, b) => (a.kind === b.kind ? b.uptime - a.uptime : a.kind === "buff" ? -1 : 1));
  // Different effects can share a skill name: tell them apart by the end of the effect ID.
  const seen = new Map();
  for (const lane of list) seen.set(lane.label, (seen.get(lane.label) || 0) + 1);
  for (const lane of list) {
    if (seen.get(lane.label) > 1) lane.label += ` ·${String(lane.spans[0].effect % 1000).padStart(3, "0")}`;
  }
  return { list, debuffTarget };
}

function drawBuffTimeline() {
  const box = $("buff-timeline");
  const canvas = box.querySelector("canvas");
  const { list, debuffTarget } = buffLanes();
  $("buffs-on-label").textContent = `on ${name(selection.player)}`;
  $("debuffs-on-label").textContent = `on ${debuffTarget != null ? name(debuffTarget) : "the target"}`;
  const laneH = Math.round(LANE_H * 0.8);
  const height = Math.max(laneH * 2, list.length * laneH) + AXIS_H;
  const { ctx, width } = setupCanvas(canvas, height);
  ctx.clearRect(0, 0, width, height);
  const gutter = buffGutter();
  const [t0, t1] = viewWindow("buffs");
  const x = (t) => gutter + ((width - 8 - gutter) * (t - t0)) / (t1 - t0);
  const plotBottom = list.length * laneH;
  drawTimeAxis(ctx, gutter, width - 8, plotBottom, t0, t1);
  if (!list.length) {
    ctx.fillStyle = css("--muted");
    ctx.font = canvasFont();
    ctx.textAlign = "center";
    ctx.fillText("no buffs or debuffs recorded for this player and target", (gutter + width) / 2, laneH);
  }
  const colours = { buff: css("--buff"), debuff: css("--debuff") };
  buffLayout = { bars: [], laneH };
  ctx.textBaseline = "middle";
  list.forEach((lane, i) => {
    const y = i * laneH;
    if (i % 2) {
      ctx.fillStyle = "rgba(255,255,255,0.025)";
      ctx.fillRect(0, y, width, laneH);
    }
    ctx.textAlign = "left";
    ctx.fillStyle = css("--text");
    ctx.font = canvasFont();
    const pctText = `${Math.round(lane.uptime * 100)}%`;
    ctx.fillText(clipText(ctx, lane.label, gutter - 5 * fontPx()), 4, y + laneH / 2);
    ctx.textAlign = "right";
    ctx.fillStyle = css("--muted");
    ctx.fillText(pctText, gutter - 10, y + laneH / 2);
    ctx.save();
    ctx.beginPath();
    ctx.rect(gutter, y, width - gutter, laneH);
    ctx.clip();
    for (const s of lane.spans) {
      const a = x(Math.max(s.start, t0));
      const b = x(Math.min(s.end, t1));
      if (b < gutter || a > width) continue;
      const w = Math.max(2, b - a);
      ctx.fillStyle = colours[lane.kind];
      roundRect(ctx, a, y + 4, w, laneH - 8, 3);
      ctx.fill();
      buffLayout.bars.push({ lane, s, a, b: a + w, y0: y, y1: y + laneH });
    }
    ctx.restore();
  });

  const tip = box.querySelector(".tooltip");
  const hit = buffHover && buffLayout.bars.find((r) => buffHover[0] >= r.a - 2 && buffHover[0] <= r.b + 2 && buffHover[1] >= r.y0 && buffHover[1] < r.y1);
  if (hit) {
    const { lane, s } = hit;
    tip.replaceChildren(el("div", "t-head", lane.label));
    const row = (a, b) => {
      const r = el("div", "t-row");
      r.append(el("span", "", a), el("span", "", b));
      tip.append(r);
    };
    row("on", name(s.target));
    row("from", name(s.source));
    row("time", `${s.before ? "before " : ""}${clock(s.start)} – ${s.open ? "still up" : clock(s.end)}`);
    row("length", gap(s.end - s.start));
    row("uptime", `${Math.round(lane.uptime * 100)}% of the fight`);
    tip.hidden = false;
    placeTooltip(tip, box, buffHover[0], hit.y1 + 4);
  } else {
    tip.hidden = true;
  }
}

function roundRect(ctx, x, y, w, h, r) {
  const rr = Math.min(r, w / 2, h / 2);
  ctx.beginPath();
  ctx.moveTo(x + rr, y);
  ctx.arcTo(x + w, y, x + w, y + h, rr);
  ctx.arcTo(x + w, y + h, x, y + h, rr);
  ctx.arcTo(x, y + h, x, y, rr);
  ctx.arcTo(x, y, x + w, y, rr);
  ctx.closePath();
}

attachTimeline("buff-timeline", "buffs", buffGutter, (mx, my) => {
  buffHover = mx == null ? null : [mx, my];
  drawBuffTimeline();
});

// Each skill's casts, and the share of them that landed with each effect up.
function drawBuffsPerSkill() {
  const box = $("buffs-per-skill");
  box.replaceChildren();
  const p = selectedPlayer();
  if (!p) return;
  const { list } = buffLanes();
  const up = (lane, t) => lane.spans.some((s) => s.start <= t && t <= s.end);
  const skills = [...p.skills.entries()].filter(([, s]) => s.castTimes.length).sort((a, b) => b[1].damage - a[1].damage);
  if (!skills.length || !list.length) {
    box.append(el("p", "note", list.length ? "no casts" : "no buffs or debuffs recorded for this player and target"));
    return;
  }
  for (const [id, s] of skills.slice(0, 18)) {
    const card = el("div", "card");
    const head = el("div", "card-head");
    head.append(iconOrBlank(skillIconUrl(id), ""), el("span", "name", skillName(id)), el("span", "share", pct(s.damage, p.damage)));
    card.append(head, el("div", "sub", `${s.castTimes.length} casts · ${fmt(s.damage)}`));
    const rows = list.map((lane) => ({ lane, share: s.castTimes.filter((t) => up(lane, t)).length / s.castTimes.length })).filter((r) => r.share > 0);
    const always = rows.filter((r) => r.share >= 1);
    if (always.length) {
      card.append(el("div", "group", "ALWAYS UP"));
      const tags = el("div", "always");
      for (const r of always) tags.append(el("span", "tag", r.lane.label));
      card.append(tags);
    }
    for (const kind of ["buff", "debuff"]) {
      const top = rows.filter((r) => r.lane.kind === kind && r.share < 1).sort((a, b) => b.share - a.share);
      if (!top.length) continue;
      card.append(el("div", "group", kind === "buff" ? `TOP BUFFS ON ${name(selection.player).toUpperCase()}` : "TOP DEBUFFS ON THE TARGET"));
      for (const r of top.slice(0, 3)) {
        const line = el("div", "uptime");
        const track = el("div", "track");
        const fill = el("div", `fill ${kind}`);
        fill.style.width = `${r.share * 100}%`;
        track.append(fill);
        line.append(el("span", "label", r.lane.label), track, el("span", "pct", `${Math.round(r.share * 100)}%`));
        card.append(line);
      }
      if (top.length > 3) card.append(el("div", "sub", `+${top.length - 3} more`));
    }
    box.append(card);
  }
}

// ---------------------------------------------------------------- render & data

function render() {
  drawTabs();
  $("empty").hidden = !!data;
  if (!data) return;
  drawHeader();
  drawChips();
  if (tab === "overview") {
    drawTiles();
    drawDpsChart();
    drawPlayersTable();
  } else if (tab === "skills") {
    drawSkills();
  } else if (tab === "rotation") {
    drawOpener();
    drawRotation();
  } else if (tab === "buffs") {
    drawBuffTimeline();
    drawBuffsPerSkill();
  }
}

function selectPlayer(id) {
  selection.player = id;
  render();
}

function setTarget(id) {
  target = id;
  derived = derive();
  zoom.rotation = null;
  zoom.buffs = null;
  render();
}

async function load(force) {
  try {
    const s = await invoke("analysis_stamp", { fight: selection.fight });
    if (!force && s === stamp) return;
    stamp = s;
    const d = await invoke("analysis_data", { fight: selection.fight });
    data = d || null;
    if (data) {
      applyAppearance(data.appearance);
      if (target != null && !data.hits.some((h) => h.target === target)) target = null;
      derived = derive();
      const ids = data.view.players.map((p) => p.id);
      if (!ids.includes(selection.player)) {
        const self = data.view.players.find((p) => p.is_self);
        selection.player = self ? self.id : ids[0];
      }
    }
    render();
  } catch (e) {
    $("fight-label").textContent = String(e);
  }
}

for (const b of document.querySelectorAll("#tabs [data-tab]")) {
  b.addEventListener("click", () => {
    tab = b.dataset.tab;
    render();
  });
}

$("dps-chart").querySelector("canvas").addEventListener("mousemove", (e) => {
  dpsHover = e.offsetX;
  drawDpsChart();
});
$("dps-chart").querySelector("canvas").addEventListener("mouseleave", () => {
  dpsHover = null;
  drawDpsChart();
});

$("close").addEventListener("click", () => win.hide());
$("max").addEventListener("click", () => win.toggleMaximize());
document.addEventListener("keydown", (e) => {
  if (e.key === "Escape") win.hide();
  const n = Number(e.key);
  if (n >= 1 && n <= 4 && !e.ctrlKey && !e.altKey) {
    tab = ["overview", "skills", "rotation", "buffs"][n - 1];
    render();
  }
});

new ResizeObserver(queueRedraw).observe($("scroll"));

tauri.event.listen("analysis-select", (e) => {
  const fightChanged = e.payload.fight !== selection.fight;
  selection = { fight: e.payload.fight, player: e.payload.player ?? selection.player };
  if (fightChanged) {
    target = null;
    zoom.rotation = null;
    zoom.buffs = null;
  }
  load(true);
});

(async () => {
  selection = await invoke("analysis_selection").catch(() => selection);
  await load(true);
  setInterval(() => {
    if (selection.fight == null) load(false);
  }, POLL_MS);
})();
