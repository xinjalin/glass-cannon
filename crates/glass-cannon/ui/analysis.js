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
const HP_STRIP_H = 22; // target HP strip under the DPS chart
const HIT = { CRIT: 0x01, BACK: 0x02, FRONT: 0x04, PERFECT: 0x08, DOUBLE: 0x10, DOT: 0x20, FIRST: 0x40 };

const $ = (id) => document.getElementById(id);

let selection = { fight: null, player: null };
let data = null; // what analysis_data returned
let stamp = "";
let tab = "overview";
let target = null; // null = every target
let sort = { key: "damage", dir: -1 };
// Each timeline's zoom: null = the whole fight, or { a, span } in ms. `follow` keeps a
// window that reaches the newest data pinned to it, so a live fight scrolls into view.
const zoom = { rotation: null, buffs: null };
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

// Per-second rates: small ones (a little healing) keep a decimal instead of showing 0.
const fmtRate = (n) => (n > 0 && n < 10 ? n.toFixed(1) : fmt(n));
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
  return iconUrlFor(data && data.skills[skill] && data.skills[skill].icon_key);
}

function iconUrlFor(key) {
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
  return iconImageFor(data.skills[skill] && data.skills[skill].icon_key);
}

function iconImageFor(key) {
  iconUrlFor(key);
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
  return { hits, players, order, durationMs, casts, endMs: data.view.duration_s * 1000, hp: targetHp() };
}

// The chosen target's HP (or the main target's) as a fraction of its max, or null when
// the game sent none. Mob HP isn't in damage units, so only the fraction is shown.
function targetHp() {
  const id = target ?? data.view.target_id;
  const h = (data.target_hp || []).find((x) => x.target === id);
  if (!h || !h.max || !h.samples.length) return null;
  return { id, points: h.samples.map((s) => ({ t: s.t, f: Math.min(1, s.hp / h.max) })) };
}

// HP is a step function: the last sample at or before `t` (null before the first one).
function hpAt(hp, t) {
  let at = null;
  for (const p of hp.points) {
    if (p.t > t) break;
    at = p;
  }
  return at;
}

const hpText = (p) => (p ? `${(p.f * 100).toFixed(1)}%` : "–");

// Draws HP as a recessive stepped area between y(1) (full) and y(0) (dead), from its first
// sample to `tEnd`. The caller clips to the plot.
function drawHpArea(ctx, hp, x, y, tEnd, fillAlpha, lineColour) {
  const pts = hp.points;
  const last = pts[pts.length - 1];
  const xEnd = x(Math.max(last.t, tEnd));
  const steps = () =>
    pts.forEach((p, i) => {
      if (i) ctx.lineTo(x(p.t), y(pts[i - 1].f));
      else ctx.moveTo(x(p.t), y(p.f));
      ctx.lineTo(x(p.t), y(p.f));
    });
  ctx.save();
  ctx.beginPath();
  steps();
  ctx.lineTo(xEnd, y(last.f));
  ctx.lineTo(xEnd, y(0));
  ctx.lineTo(x(pts[0].t), y(0));
  ctx.closePath();
  ctx.fillStyle = css("--muted");
  ctx.globalAlpha = fillAlpha;
  ctx.fill();
  ctx.beginPath();
  steps();
  ctx.lineTo(xEnd, y(last.f));
  ctx.strokeStyle = lineColour;
  ctx.globalAlpha = 1;
  ctx.lineWidth = 1;
  ctx.stroke();
  ctx.restore();
}

function hpTooltipRow(tip, hp, t) {
  const row = el("div", "t-row");
  const label = el("span");
  label.append(el("span", "hp-swatch"), document.createTextNode(`hp · ${name(hp.id)}`));
  row.append(label, el("span", "", hpText(hpAt(hp, t))));
  tip.append(row);
}

function selectedPlayer() {
  return derived && derived.players.get(selection.player);
}

// ---------------------------------------------------------------- header, chips, tabs

function drawHeader() {
  const v = data.view;
  const when = data.ended_ms ? ` · ${new Date(data.ended_ms).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}` : "";
  const boss = v.target_boss || (v.target_id != null && data.bosses.includes(v.target_id)) ? "BOSS " : "";
  const where = data.place ? ` · ${[data.place.name, data.place.mode].filter(Boolean).join(" ")}` : "";
  $("fight-label").textContent = `${boss}${name(v.target_id) || v.target || "?"}${v.target_count > 1 ? ` +${v.target_count - 1}` : ""}${where} · ${clock(v.duration_s * 1000)} · ${fmt(v.total_damage)}${when}`;
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
  return perSecondSeries(derived.hits, derived.order.slice(0, MAX_SERIES), (h) => h.player);
}

// Per-second amounts for each of `ids` (who an event belongs to: `keyOf`), as a 5 s
// trailing average. Colours go by position in `ids`.
function perSecondSeries(events, ids, keyOf) {
  const seconds = Math.max(1, Math.ceil(derived.endMs / 1000) + 1);
  return ids.map((id, i) => {
    const perSec = new Float64Array(seconds);
    for (const h of events) if (keyOf(h) === id) perSec[Math.min(seconds - 1, Math.floor(h.t / 1000))] += h.damage;
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
  drawSeriesChart($("dps-chart"), $("dps-legend"), dpsSeries(), dpsHover, derived.hp, selectPlayer);
}

// A per-second line chart (one line per series) with a crosshair tooltip at `hover`
// (x in the canvas), and the target's HP in a strip underneath when given. With `onPick`,
// clicking a line or its legend entry picks that series.
const chartLayouts = new WeakMap();
function drawSeriesChart(box, legendBox, series, hover, hp, onPick) {
  const canvas = box.querySelector("canvas");
  box.classList.toggle("has-hp", !!hp);
  const { ctx, width, height } = setupCanvas(canvas, box.clientHeight || 200);
  ctx.clearRect(0, 0, width, height);
  const left = 4.5 * fontPx();
  // The target's HP gets its own strip under the plot: same time axis, its own 0-100%.
  const strip = hp ? HP_STRIP_H : 0;
  const bottom = height - AXIS_H - strip;
  const right = width - 8;
  const top = 8;
  let max = 1;
  for (const s of series) for (const v of s.dps) max = Math.max(max, v);
  // Whole-number steps: a small maximum would otherwise label its gridlines 0, 0, 1, 1.
  const step = Math.max(1, niceStep(max, 4));
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
  for (let s = 0; s < seconds; s += tstep) {
    // The last label sits inside the plot rather than off its edge.
    const near = x(s) > right - 2 * fontPx();
    ctx.textAlign = near ? "right" : "center";
    ctx.fillText(clock(s * 1000), near ? right : x(s), bottom + strip + 6);
  }

  if (hp) {
    const sTop = bottom + 5;
    const sBottom = bottom + strip;
    const sy = (f) => sBottom - (sBottom - sTop) * f;
    ctx.strokeStyle = css("--line-faint");
    ctx.strokeRect(left + 0.5, Math.round(sTop) + 0.5, right - left - 1, sBottom - sTop);
    ctx.textAlign = "right";
    ctx.textBaseline = "middle";
    ctx.fillStyle = css("--faint");
    ctx.fillText("hp", left - 6, (sTop + sBottom) / 2);
    ctx.save();
    ctx.beginPath();
    ctx.rect(left, sTop, right - left, sBottom - sTop);
    ctx.clip();
    drawHpArea(ctx, hp, (t) => x(t / 1000), sy, derived.endMs, 0.28, css("--faint"));
    ctx.restore();
  }

  chartLayouts.set(box, { series, x, y, left, right, top, bottom, seconds, onPick });
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
  if (hover != null && series.length) {
    const s = Math.max(0, Math.min(seconds - 1, Math.round(((hover - left) / (right - left)) * (seconds - 1))));
    ctx.strokeStyle = css("--muted");
    ctx.lineWidth = 1;
    ctx.beginPath();
    ctx.moveTo(Math.round(x(s)) + 0.5, top);
    ctx.lineTo(Math.round(x(s)) + 0.5, bottom + strip);
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
    if (hp) hpTooltipRow(tip, hp, s * 1000);
    tip.hidden = false;
    placeTooltip(tip, box, x(s), top + 10);
  } else {
    tip.hidden = true;
  }

  const legend = legendBox;
  legend.replaceChildren();
  for (const s of series) {
    const item = el("span", `item${onPick ? " pick" : ""}${onPick && s.id === selection.player ? " on" : ""}`);
    const line = el("span", "line");
    line.style.background = s.colour;
    item.append(line, document.createTextNode(name(s.id)));
    if (onPick) item.addEventListener("click", () => onPick(s.id));
    legend.append(item);
  }
}

// The series whose line passes within a few pixels of (mx, my) on a chart, or null.
function lineAt(box, mx, my) {
  const L = chartLayouts.get(box);
  if (!L || !L.series.length || mx < L.left || mx > L.right || my < L.top || my > L.bottom) return null;
  const s = Math.max(0, Math.min(L.seconds - 1, Math.round(((mx - L.left) / (L.right - L.left)) * (L.seconds - 1))));
  let best = null;
  let bestD = 10;
  for (const ser of L.series) {
    const d = Math.abs(L.y(ser.dps[s]) - my);
    if (d < bestD) {
      bestD = d;
      best = ser;
    }
  }
  return best;
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
  head.append(th("skill", "name", true), th("spec"));
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
    cell.dataset.skill = id;
    cell.append(iconOrBlank(skillIconUrl(id), ""), el("span", "", skillName(id)));
    who.append(cell);
    const spec = el("td", "muted", specText(id));
    spec.dataset.skill = id;
    tr.append(
      who,
      spec,
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

// ---------------------------------------------------------------- healing / taken

const flowHover = { healing: null, taken: null };
const flowPick = { healing: null, taken: null }; // the row shown in detail
const FLOW = {
  // Heals: rows are healers (`player`); "healed" lists who they healed.
  healing: { rowOf: (h) => h.player, otherOf: (h) => h.target, unit: "healed", rate: "hps", others: "healed", skill: "heal" },
  // Damage taken: rows are who took it (`target`); "hit by" lists the mobs (`player`).
  taken: { rowOf: (h) => h.target, otherOf: (h) => h.player, unit: "taken", rate: "per s", others: "hit by", skill: "skill" },
};

function flowEvents(kind) {
  const all = data[kind === "healing" ? "heals" : "taken"] || [];
  // The target filter picks the mob for damage taken; heals aren't on a target.
  return kind === "taken" && target != null ? all.filter((h) => h.player === target) : all;
}

// Totals per key: amount, events and biggest, plus per-skill and per-other breakdowns.
function flowStats(events, keyOf, otherOf) {
  const rows = new Map();
  for (const h of events) {
    const k = keyOf(h);
    let r = rows.get(k);
    if (!r) {
      r = { amount: 0, n: 0, max: 0, skills: new Map(), others: new Map() };
      rows.set(k, r);
    }
    r.amount += h.damage;
    r.n += 1;
    r.max = Math.max(r.max, h.damage);
    for (const [map, key] of [[r.skills, h.skill], [r.others, otherOf(h)]]) {
      const s = map.get(key) || { amount: 0, n: 0, max: 0 };
      s.amount += h.damage;
      s.n += 1;
      s.max = Math.max(s.max, h.damage);
      map.set(key, s);
    }
  }
  return rows;
}

function drawFlow(kind) {
  const f = FLOW[kind];
  const events = flowEvents(kind);
  const stats = flowStats(events, f.rowOf, f.otherOf);
  const order = [...stats.keys()].sort((a, b) => stats.get(b).amount - stats.get(a).amount);
  const total = order.reduce((a, k) => a + stats.get(k).amount, 0);
  const secs = derived.durationMs / 1000;
  if (!stats.has(flowPick[kind])) {
    flowPick[kind] = stats.has(selection.player) ? selection.player : order[0] ?? null;
  }
  const pick = flowPick[kind];
  const row = stats.get(pick);
  const viewRows = data.view[kind] || [];
  const classOf = (id) => (viewRows.find((r) => r.id === id) || data.view.players.find((r) => r.id === id) || {}).class_code;

  const tiles = $(`${kind}-tiles`);
  tiles.replaceChildren(
    tile(`total ${f.unit}`, fmt(total), `${fmtRate(total / secs)}/s over ${clock(derived.durationMs)}`),
    tile(order.length === 1 ? (kind === "healing" ? "healer" : "player hit") : kind === "healing" ? "healers" : "players hit", String(order.length), order.length ? `most: ${name(order[0])}` : "none"),
  );
  if (row) {
    tiles.append(
      tile(name(pick), fmt(row.amount), `${pct(row.amount, total)} · ${fmtRate(row.amount / secs)}/s`),
      tile("biggest", fmt(row.max), `${row.n} ${kind === "healing" ? "ticks" : "hits"}`),
    );
  }

  // An empty chart says nothing: hide it when there's nothing to draw.
  const chartBox = $(`${kind}-chart`).closest(".box");
  chartBox.hidden = !events.length;
  if (events.length) {
    drawSeriesChart(
      $(`${kind}-chart`),
      $(`${kind}-legend`),
      perSecondSeries(events, order.slice(0, MAX_SERIES), f.rowOf),
      flowHover[kind],
      null,
    );
  }

  const table = $(`${kind}-table`);
  table.replaceChildren();
  const head = el("tr");
  head.append(th("#"), th(kind === "healing" ? "healer" : "player", null, true), th(f.rate), th(f.unit), th("share"), th(kind === "healing" ? "ticks" : "hits"), th("max"));
  table.append(head);
  const top = order.length ? stats.get(order[0]).amount : 1;
  order.forEach((id, i) => {
    const s = stats.get(id);
    const tr = el("tr", `clickable${id === pick ? " selected" : ""}`);
    const who = el("td", "left");
    const cell = el("div", "cell-name");
    cell.append(iconOrBlank(classIconUrl(classOf(id)), ""), el("span", "", name(id)));
    who.append(cell);
    tr.append(
      el("td", "muted", String(i + 1)),
      who,
      barCell(fmtRate(s.amount / secs), s.amount / top),
      el("td", "", fmt(s.amount)),
      el("td", "", pct(s.amount, total)),
      el("td", "", String(s.n)),
      el("td", "", fmt(s.max)),
    );
    tr.addEventListener("click", () => {
      flowPick[kind] = id;
      drawFlow(kind);
    });
    table.append(tr);
  });
  if (!order.length) {
    const tr = el("tr");
    const td = el("td", "muted left", kind === "healing" ? "no healing in this fight" : "no damage taken in this fight");
    td.colSpan = 7;
    tr.append(td);
    table.append(tr);
  }

  $(`${kind}-detail-legend`).textContent = row ? `${name(pick)} · by ${f.skill}` : "details";
  const breakdown = (tableId, map, labelOf, iconOf, firstHeader) => {
    const t = $(tableId);
    t.replaceChildren();
    if (!row) return;
    const h = el("tr");
    h.append(th(firstHeader, null, true), th(f.unit), th("share"), th(kind === "healing" ? "ticks" : "hits"), th("max"));
    t.append(h);
    const entries = [...map.entries()].sort((a, b) => b[1].amount - a[1].amount);
    const best = entries.length ? entries[0][1].amount : 1;
    for (const [key, s] of entries) {
      const tr = el("tr");
      const who = el("td", "left");
      const cell = el("div", "cell-name");
      if (iconOf) cell.dataset.skill = key;
      else cell.title = String(key);
      if (iconOf) cell.append(iconOrBlank(iconOf(key), ""));
      cell.append(el("span", "", labelOf(key)));
      who.append(cell);
      tr.append(who, barCell(fmt(s.amount), s.amount / best), el("td", "", pct(s.amount, row.amount)), el("td", "", String(s.n)), el("td", "", fmt(s.max)));
      t.append(tr);
    }
  };
  breakdown(`${kind}-skills`, row ? row.skills : new Map(), skillName, skillIconUrl, kind === "healing" ? "heal" : "skill");
  breakdown(`${kind}-others`, row ? row.others : new Map(), name, null, f.others);
}

// ---------------------------------------------------------------- timelines (shared)

const timelineEnd = () => Math.max(1000, derived.endMs);

function viewWindow(which) {
  const end = timelineEnd();
  const z = zoom[which];
  if (!z) return [0, end];
  const a = z.follow ? Math.max(0, end - z.span) : z.a;
  return [a, a + z.span];
}

// A window [a, a + span]; it follows the live edge when it ends there.
function setWindow(which, a, span) {
  const end = timelineEnd();
  if (span >= end) {
    zoom[which] = null;
    return;
  }
  a = Math.max(0, Math.min(end - span, a));
  // Within a few pixels' worth of the end counts as at the end.
  zoom[which] = { a, span, follow: a + span >= end - span * 0.01 };
}

function zoomBy(which, factor, centreFrac = 0.5) {
  const end = timelineEnd();
  if (!factor) {
    zoom[which] = null;
  } else {
    const [t0, t1] = viewWindow(which);
    const centre = t0 + (t1 - t0) * centreFrac;
    const span = Math.min(end, Math.max(2000, (t1 - t0) * factor));
    setWindow(which, centre - span * centreFrac, span);
  }
  render();
}

function panBy(which, dxFrac) {
  const [t0, t1] = viewWindow(which);
  const span = t1 - t0;
  if (span >= timelineEnd()) return;
  setWindow(which, t0 - dxFrac * span, span);
  render();
}

// The newest part of the fight at the current zoom (or the last 30 s), followed live.
function jumpToNow(which) {
  const end = timelineEnd();
  const span = zoom[which] ? zoom[which].span : Math.min(end, 30000);
  setWindow(which, end - span, span);
  render();
}

// "following" while a zoomed window is pinned to a live fight's newest data.
function drawFollow(which) {
  const z = zoom[which];
  const on = !!(z && z.follow && data.live);
  const mark = document.querySelector(`[data-follow-state="${which}"]`);
  if (mark) mark.hidden = !on;
}

for (const b of document.querySelectorAll("[data-zoom]")) {
  b.addEventListener("click", () => zoomBy(b.dataset.zoom, Number(b.dataset.step)));
}
for (const b of document.querySelectorAll("[data-now]")) {
  b.addEventListener("click", () => jumpToNow(b.dataset.now));
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
    onHover(e.clientX - rect.left, e.clientY - rect.top, e);
  });
  canvas.addEventListener("mouseleave", () => onHover(null, null, null));
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
    // A label at the right edge sits inside the plot rather than off it.
    const near = x > x1 - 2 * fontPx();
    ctx.textAlign = near ? "right" : "center";
    ctx.fillText(clock(t), near ? x1 : x, y + 6);
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
    icon.dataset.skill = c.skill;
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

  rotationLayout = { lanes, x, gutter, plotBottom, t0, t1, width, marks: [] };
  const laneOf = new Map(lanes.map((s, i) => [s, i]));
  ctx.save();
  ctx.beginPath();
  ctx.rect(gutter, 0, width - gutter, plotBottom);
  ctx.clip();
  // The target's HP behind the lanes: full at the top, dead at the bottom.
  const hp = derived.hp;
  if (hp && lanes.length) drawHpArea(ctx, hp, x, (f) => plotBottom - (plotBottom - 2) * f, derived.endMs, 0.07, css("--line-faint"));
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
    if (specText(c.skill)) tip.append(el("div", "t-row muted", `specialties ${specText(c.skill)}`));
    const row = (a, b) => {
      const r = el("div", "t-row");
      r.append(el("span", "", a), el("span", "", b));
      tip.append(r);
    };
    row("damage", fmt(c.damage));
    row("hits", String(c.hits));
    row("crit", c.crit ? "yes" : "no");
    row("position", c.back ? "back" : c.front ? "front" : "side");
    if (derived.hp) hpTooltipRow(tip, derived.hp, c.t);
    tip.hidden = false;
    placeTooltip(tip, box, hit.cx, hit.cy + 12);
  } else if (derived.hp && rotationHover && inRotationPlot(rotationHover)) {
    // Between casts: the time and the target's HP there.
    const [mx, my] = rotationHover;
    const L = rotationLayout;
    const t = L.t0 + ((L.t1 - L.t0) * (mx - L.gutter)) / (L.width - 8 - L.gutter);
    tip.replaceChildren(el("div", "t-head", clock(t)));
    hpTooltipRow(tip, derived.hp, t);
    tip.hidden = false;
    placeTooltip(tip, box, mx, my + 12);
  } else {
    tip.hidden = true;
  }
}

function inRotationPlot([mx, my]) {
  const L = rotationLayout;
  return L && mx >= L.gutter && mx <= L.width - 8 && my >= 0 && my <= L.plotBottom;
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

// The skill whose label (left of the lanes) is at (mx, my).
function rotationLabelAt(mx, my) {
  const L = rotationLayout;
  if (!L || mx == null || mx >= L.gutter || my < 0 || my >= L.plotBottom) return null;
  return L.lanes[Math.floor(my / LANE_H)] ?? null;
}

attachTimeline("rotation", "rotation", rotationGutter, (mx, my, e) => {
  const skill = rotationLabelAt(mx, my);
  rotationHover = mx == null || skill != null ? null : [mx, my];
  drawRotation();
  canvasTip($("rotation"), skill != null ? `skill:${skill}` : null, e, () => buildSkillTip(skill));
});

// ---------------------------------------------------------------- buffs

let buffHover = null;
let buffLayout = null;
const buffGutter = () => 21 * fontPx();

const effectMeta = (effect) => (data.effect_meta && data.effect_meta[effect]) || null;

// A name you gave the effect, else its skill's, else the effect's own (from its ID), else
// the bare number.
function effectLabel(e) {
  const own = effectMeta(e.effect);
  const skill = e.skill != null && data.skills[e.skill];
  if (own && own.name && (own.own || !(skill && skill.name))) return own.name;
  if (skill && skill.name) return skill.name;
  if (own && own.name) return own.name;
  return e.skill != null ? String(e.skill) : `effect ${e.effect}`;
}

function effectIconKey(e) {
  const own = effectMeta(e.effect);
  if (own && own.icon_key != null) return own.icon_key;
  return e.skill != null && data.skills[e.skill] ? data.skills[e.skill].icon_key : null;
}

// Lanes of effect spans, one per effect: on the player (buffs), then on the target
// (debuffs). Also kept by key for the tooltips.
let lanesByKey = new Map();
function buffLanes() {
  const debuffTarget = target != null ? target : data.view.target_id;
  const lanes = new Map();
  for (const e of data.effects) {
    let kind = null;
    if (e.target === selection.player) kind = "buff";
    else if (e.target === debuffTarget) kind = "debuff";
    if (!kind) continue;
    const key = `${kind}/${e.effect}`;
    if (!lanes.has(key)) {
      lanes.set(key, { key, kind, label: effectLabel(e), effect: e.effect, skill: e.skill, iconKey: effectIconKey(e), spans: [] });
    }
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
    // How long it lasts: what the game said, else the longest span that started and
    // ended in the fight (a shield that breaks, or a buff cast again, ends early).
    lane.lasts = Math.max(0, ...lane.spans.map((s) => s.duration_ms || 0));
    lane.longest = lane.lasts || Math.max(0, ...lane.spans.filter((s) => !s.before && !s.open).map((s) => s.end - s.start));
  }
  const list = [...lanes.values()];
  list.sort((a, b) => (a.kind === b.kind ? b.uptime - a.uptime : a.kind === "buff" ? -1 : 1));
  // One skill can put up several effects (Protection Circle: the circle, then Divine
  // Barrier) that the game data doesn't name apart. Tell them apart by how long they last,
  // or else by number; double-clicking the label names one.
  const groups = new Map();
  for (const lane of list) {
    const k = `${lane.kind}/${lane.label}`;
    if (!groups.has(k)) groups.set(k, []);
    groups.get(k).push(lane);
  }
  for (const group of groups.values()) {
    if (group.length < 2) continue;
    const lengths = group.map((l) => (l.longest ? seconds(l.longest / 1000) : null));
    const byLength = lengths.every(Boolean) && new Set(lengths).size === group.length;
    [...group].sort((a, b) => a.effect - b.effect).forEach((lane, i) => {
      lane.siblings = group.length;
      lane.label += byLength ? ` · ${seconds(lane.longest / 1000)}` : ` · #${i + 1}`;
    });
  }
  lanesByKey = new Map(list.map((l) => [l.key, l]));
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
  buffLayout = { bars: [], laneH, list, gutter };
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
    const icon = laneH - 6;
    const img = iconImageFor(lane.iconKey);
    if (img) ctx.drawImage(img, 4, y + 3, icon, icon);
    else {
      ctx.fillStyle = css("--line-faint");
      roundRect(ctx, 4, y + 3, icon, icon, 3);
      ctx.fill();
      ctx.fillStyle = css("--text");
    }
    ctx.fillText(clipText(ctx, lane.label, gutter - 5 * fontPx() - icon - 6), icon + 10, y + laneH / 2);
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

// The lane whose label (left of the bars) is at (mx, my).
function buffLabelAt(mx, my) {
  const L = buffLayout;
  if (!L || mx == null || mx >= L.gutter || my < 0) return null;
  return L.list[Math.floor(my / L.laneH)] || null;
}

attachTimeline("buff-timeline", "buffs", buffGutter, (mx, my, e) => {
  const lane = buffLabelAt(mx, my);
  buffHover = mx == null || lane ? null : [mx, my];
  drawBuffTimeline();
  canvasTip($("buff-timeline"), lane ? `effect:${lane.key}` : null, e, () => buildEffectTip(lane));
});

// Double-click a buff's label to name it (saved with your other names).
$("buff-timeline").querySelector("canvas").addEventListener("dblclick", (e) => {
  const box = $("buff-timeline");
  const rect = box.querySelector("canvas").getBoundingClientRect();
  const lane = buffLabelAt(e.clientX - rect.left, e.clientY - rect.top);
  if (!lane) return;
  hideSkillTip();
  const input = el("input", "lane-name");
  const own = effectMeta(lane.effect);
  input.value = own && own.custom ? own.name : "";
  input.placeholder = lane.label;
  input.maxLength = 60;
  Object.assign(input.style, {
    top: `${Math.floor((e.clientY - rect.top) / buffLayout.laneH) * buffLayout.laneH}px`,
    height: `${buffLayout.laneH}px`,
    width: `${buffLayout.gutter - 8}px`,
  });
  box.append(input);
  input.focus();
  let done = false;
  const finish = async (save) => {
    if (done) return;
    done = true;
    input.remove();
    if (save && input.value.trim() !== (own && own.custom ? own.name : "")) {
      await invoke("rename_skill", { skill: lane.effect, name: input.value }).catch((err) => sayCopied(String(err)));
      load(true);
    }
  };
  input.addEventListener("keydown", (k) => {
    k.stopPropagation();
    if (k.key === "Enter") finish(true);
    if (k.key === "Escape") finish(false);
  });
  input.addEventListener("blur", () => finish(true));
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
    head.dataset.skill = id;
    head.append(iconOrBlank(skillIconUrl(id), ""), el("span", "name", skillName(id)), el("span", "share", pct(s.damage, p.damage)));
    card.append(head, el("div", "sub", `${s.castTimes.length} casts · ${fmt(s.damage)}`));
    const rows = list.map((lane) => ({ lane, share: s.castTimes.filter((t) => up(lane, t)).length / s.castTimes.length })).filter((r) => r.share > 0);
    const always = rows.filter((r) => r.share >= 1);
    if (always.length) {
      card.append(el("div", "group", "ALWAYS UP"));
      const tags = el("div", "always");
      for (const r of always) {
        const t = el("span", "tag", r.lane.label);
        t.dataset.effect = r.lane.key;
        tags.append(t);
      }
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
        const label = el("span", "label", r.lane.label);
        label.dataset.effect = r.lane.key;
        line.append(label, track, el("span", "pct", `${Math.round(r.share * 100)}%`));
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
  // With no fight, nothing of the last one stays on show (title, players, targets).
  $("toolbar").hidden = !data;
  $("copy-text").disabled = $("copy-image").disabled = !data;
  if (!data) {
    $("fight-label").textContent = "";
    $("live").hidden = true;
    $("players").replaceChildren();
    $("targets").replaceChildren();
    return;
  }
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
    drawFollow("rotation");
  } else if (tab === "buffs") {
    drawBuffTimeline();
    drawBuffsPerSkill();
    drawFollow("buffs");
  } else if (tab === "healing" || tab === "taken") {
    drawFlow(tab);
  }
}

function selectPlayer(id) {
  selection.player = id;
  invoke("analysis_pick", { player: id }).catch(() => {});
  render();
}

function setTarget(id) {
  target = id;
  derived = derive();
  zoom.rotation = null;
  zoom.buffs = null;
  render();
}

// How long the last full fetch and redraw took: a long live fight is fetched less often
// so the window never spends more than about a tenth of its time on it.
let loadMs = 0;

async function load(force) {
  try {
    const s = await invoke("analysis_stamp", { fight: selection.fight });
    if (!force && s === stamp) return;
    stamp = s;
    const started = performance.now();
    const d = await invoke("analysis_data", { fight: selection.fight });
    data = d || null;
    if (data) {
      if (target != null && !data.hits.some((h) => h.target === target)) target = null;
      derived = derive();
      const ids = data.view.players.map((p) => p.id);
      if (!ids.includes(selection.player)) {
        const self = data.view.players.find((p) => p.is_self);
        selection.player = self ? self.id : ids[0];
      }
    }
    render();
    loadMs = performance.now() - started;
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
  e.target.style.cursor = lineAt($("dps-chart"), e.offsetX, e.offsetY) ? "pointer" : "";
});
$("dps-chart").querySelector("canvas").addEventListener("click", (e) => {
  const s = lineAt($("dps-chart"), e.offsetX, e.offsetY);
  if (s) selectPlayer(s.id);
});
$("dps-chart").querySelector("canvas").addEventListener("mouseleave", () => {
  dpsHover = null;
  drawDpsChart();
});

for (const k of ["healing", "taken"]) {
  const canvas = $(`${k}-chart`).querySelector("canvas");
  canvas.addEventListener("mousemove", (e) => {
    flowHover[k] = e.offsetX;
    drawFlow(k);
  });
  canvas.addEventListener("mouseleave", () => {
    flowHover[k] = null;
    drawFlow(k);
  });
}

// ---------------------------------------------------------------- copy (Discord)

const COPY_ROWS = 10;

// Columns a string takes in a monospace font: CJK and other wide characters take two.
function displayWidth(s) {
  let w = 0;
  for (const ch of s) {
    const c = ch.codePointAt(0);
    w += c >= 0x1100 && (c <= 0x115f || (c >= 0x2e80 && c <= 0xa4cf) || (c >= 0xac00 && c <= 0xd7a3) || (c >= 0xf900 && c <= 0xfaff) || (c >= 0xfe30 && c <= 0xfe4f) || (c >= 0xff00 && c <= 0xff60) || (c >= 0xffe0 && c <= 0xffe6) || c >= 0x20000) ? 2 : 1;
  }
  return w;
}

// Cuts to `width` columns, then pads to exactly that.
function fitColumns(s, width) {
  let out = "";
  for (const ch of s) {
    if (displayWidth(out + ch) > width) break;
    out += ch;
  }
  return out + " ".repeat(width - displayWidth(out));
}

function fightTitle() {
  const v = data.view;
  const boss = v.target_boss || (v.target_id != null && data.bosses.includes(v.target_id)) ? "BOSS " : "";
  const more = v.target_count > 1 ? ` +${v.target_count - 1}` : "";
  return `${boss}${name(v.target_id) || v.target || "?"}${more}`;
}

function fightWhen() {
  return new Date(data.ended_ms || Date.now()).toLocaleString([], { dateStyle: "medium", timeStyle: "short" });
}

// The fight as a Discord message: a title line and a monospace table in a code block.
function summaryText() {
  const v = data.view;
  const rows = v.players.slice(0, COPY_ROWS);
  const nameW = Math.min(18, Math.max(6, ...rows.map((p) => displayWidth(name(p.id)))));
  const lines = [];
  lines.push(` #  ${fitColumns("player", nameW)}     dps   damage  share   crit`);
  rows.forEach((p, i) => {
    const crit = p.hits ? `${Math.round((p.outcomes.crits / p.hits) * 100)}%` : "–";
    lines.push(
      `${String(i + 1).padStart(2)}  ${fitColumns(name(p.id), nameW)}  ${fmt(p.dps).padStart(6)}  ${fmt(p.damage).padStart(7)}  ${`${(p.share * 100).toFixed(1)}%`.padStart(5)}  ${crit.padStart(5)}`,
    );
  });
  if (v.players.length > rows.length) lines.push(`    … and ${v.players.length - rows.length} more`);
  const extra = [];
  if (v.total_healing) extra.push(`healing ${fmt(v.total_healing)} (most: ${name(v.healing[0].id)})`);
  if (v.total_taken) extra.push(`taken ${fmt(v.total_taken)} (most: ${name(v.taken[0].id)})`);
  if (extra.length) lines.push("", extra.join(" · "));
  const head = `**${fightTitle()}** · ${clock(v.duration_s * 1000)} · ${fmt(v.total_damage)} damage · ${fmt(v.total_dps)}/s · ${fightWhen()}`;
  return `${head}\n\`\`\`\n${lines.join("\n")}\n\`\`\`\n-# Glass Cannon`;
}

// ---------------------------------------------------------------- copy (image card)

// A summary card: title, the players as bars, and DPS over time for the top five.
async function summaryImage() {
  const v = data.view;
  const rows = v.players.slice(0, 8);
  const W = 760;
  const pad = 24;
  const rowH = 30;
  const chartH = 150;
  const H = pad + 64 + rows.length * rowH + 22 + chartH + 40 + pad;
  const scale = 2;
  const canvas = document.createElement("canvas");
  canvas.width = W * scale;
  canvas.height = H * scale;
  const ctx = canvas.getContext("2d");
  ctx.scale(scale, scale);
  const mono = css("--mono");
  const text = css("--text");
  const muted = css("--muted");
  const accent = css("--accent");

  ctx.fillStyle = `rgb(${css("--tint-rgb")})`;
  ctx.fillRect(0, 0, W, H);
  ctx.strokeStyle = css("--line");
  ctx.lineWidth = 1;
  roundRect(ctx, 0.5, 0.5, W - 1, H - 1, 10);
  ctx.stroke();

  ctx.textBaseline = "alphabetic";
  ctx.fillStyle = accent;
  ctx.font = `12px ${mono}`;
  ctx.fillText("─ glass cannon", pad, pad + 10);
  ctx.textAlign = "right";
  ctx.fillStyle = muted;
  ctx.fillText(fightWhen(), W - pad, pad + 10);
  ctx.textAlign = "left";
  ctx.fillStyle = text;
  ctx.font = `bold 20px ${mono}`;
  ctx.fillText(fightTitle(), pad, pad + 38);
  ctx.font = `13px ${mono}`;
  ctx.fillStyle = muted;
  const extra = [v.total_healing ? `healing ${fmt(v.total_healing)}` : "", v.total_taken ? `taken ${fmt(v.total_taken)}` : ""].filter(Boolean).join(" · ");
  ctx.fillText(`${clock(v.duration_s * 1000)} · ${fmt(v.total_damage)} damage · ${fmt(v.total_dps)}/s${extra ? ` · ${extra}` : ""}`, pad, pad + 58);

  // Player bars.
  const top = rows.length ? rows[0].dps || 1 : 1;
  const nameX = pad + 26;
  const barX = pad + 230;
  const barW = W - barX - pad - 150;
  let y = pad + 76;
  rows.forEach((p, i) => {
    ctx.fillStyle = muted;
    ctx.font = `13px ${mono}`;
    ctx.fillText(String(i + 1), pad, y + 19);
    ctx.fillStyle = p.is_self ? accent : text;
    ctx.font = `${p.is_self ? "bold " : ""}14px ${mono}`;
    ctx.fillText(clipText(ctx, name(p.id), barX - nameX - 10), nameX, y + 19);
    ctx.fillStyle = css("--line-faint");
    roundRect(ctx, barX, y + 8, barW, 14, 3);
    ctx.fill();
    ctx.fillStyle = accent;
    ctx.globalAlpha = p.is_self ? 1 : 0.55;
    roundRect(ctx, barX, y + 8, Math.max(4, (barW * p.dps) / top), 14, 3);
    ctx.fill();
    ctx.globalAlpha = 1;
    ctx.fillStyle = text;
    ctx.textAlign = "right";
    ctx.font = `14px ${mono}`;
    ctx.fillText(`${fmt(p.dps)}/s`, W - pad - 60, y + 19);
    ctx.fillStyle = muted;
    ctx.fillText(`${(p.share * 100).toFixed(0)}%`, W - pad, y + 19);
    ctx.textAlign = "left";
    y += rowH;
  });

  // DPS over time for the top five (the same lines as the overview).
  y += 22;
  const series = perSecondSeries(data.hits, data.view.players.slice(0, MAX_SERIES).map((p) => p.id), (h) => h.player);
  let max = 1;
  for (const s of series) for (const val of s.dps) max = Math.max(max, val);
  const seconds = series.length ? series[0].dps.length : 1;
  const left = pad;
  const right = W - pad;
  const x = (s) => left + ((right - left) * s) / Math.max(1, seconds - 1);
  const yv = (val) => y + chartH - (chartH * val) / max;
  ctx.strokeStyle = css("--line-faint");
  ctx.beginPath();
  ctx.moveTo(left, y + chartH + 0.5);
  ctx.lineTo(right, y + chartH + 0.5);
  ctx.stroke();
  for (const s of series) {
    ctx.strokeStyle = s.colour;
    ctx.lineWidth = 2;
    ctx.lineJoin = "round";
    ctx.beginPath();
    s.dps.forEach((val, i) => (i ? ctx.lineTo(x(i), yv(val)) : ctx.moveTo(x(i), yv(val))));
    ctx.stroke();
  }
  ctx.fillStyle = muted;
  ctx.font = `11px ${mono}`;
  ctx.fillText(`dps over time · peak ${fmt(max)}/s`, left, y - 6);
  // Legend: a line in each colour, then the name.
  let lx = left;
  const ly = y + chartH + 26;
  ctx.font = `12px ${mono}`;
  for (const s of series) {
    ctx.fillStyle = s.colour;
    ctx.fillRect(lx, ly - 5, 14, 2);
    ctx.fillStyle = muted;
    const label = clipText(ctx, name(s.id), 110);
    ctx.fillText(label, lx + 20, ly);
    lx += 20 + ctx.measureText(label).width + 22;
  }

  return new Promise((resolve) => canvas.toBlob(resolve, "image/png"));
}

let copiedTimer = null;
function sayCopied(message) {
  $("copied").textContent = message;
  clearTimeout(copiedTimer);
  copiedTimer = setTimeout(() => ($("copied").textContent = ""), 2500);
}

$("copy-text").addEventListener("click", async () => {
  if (!data) return;
  try {
    await navigator.clipboard.writeText(summaryText());
    sayCopied("copied: paste it in Discord");
  } catch (e) {
    sayCopied(`couldn't copy: ${e}`);
  }
});

$("copy-image").addEventListener("click", async () => {
  if (!data) return;
  try {
    // The item takes the promise, so the copy keeps the click's permission.
    await navigator.clipboard.write([new ClipboardItem({ "image/png": summaryImage() })]);
    sayCopied("image copied: paste it in Discord");
  } catch (e) {
    sayCopied(`couldn't copy: ${e}`);
  }
});

// ---------------------------------------------------------------- skill tooltips

// What the skill data says about a class skill (null when there's nothing).
const skillInfo = (id) => (data && data.skill_details && data.skill_details[id]) || null;

// The specialty a spec entry is (1-5, its position in the skill's list).
const specNumber = (spec) => Math.floor((spec.id % 10000) / 10);

// "3 · 4": the specialties a skill ID shows were chosen ("–" for none, "" for no data or a
// Theostone, which has none).
function specText(id) {
  const d = skillInfo(id);
  if (!d || !d.chosen) return "";
  return d.chosen.length ? d.chosen.join(" · ") : "–";
}

const seconds = (v) => `${Number(v.toFixed(1))}s`;
// One number, or "level 1 → top level" when it changes with the skill's level.
function byLevel(v, unit) {
  if (v == null) return null;
  return Array.isArray(v) ? `${unit(v[0])} → ${unit(v[1])}` : unit(v);
}

// The level 1 description with the numbers that grow with the skill's level shown as
// "1,384 → 4,386", so one text says both. Null when the two texts don't line up.
function mergeLevels(low, high) {
  // "1384-1384 damage" is one number.
  const tidy = (t) => t.replace(/(\d[\d.,]*)-\1(?![\d.,])/g, "$1");
  const num = /(\d[\d.,]*%?)/;
  const a = tidy(low).split(num);
  const b = tidy(high || low).split(num);
  if (a.length !== b.length || a.some((part, i) => i % 2 === 0 && part !== b[i])) return null;
  let scaled = false;
  const nodes = a.map((part, i) => {
    if (i % 2 === 0 || part === b[i]) return document.createTextNode(part);
    scaled = true;
    return el("b", "st-scale", `${part} → ${b[i]}`);
  });
  return { nodes, scaled };
}

// The skill data's description, kind, facts and specialties.
function appendSkillDetails(tip, d, compact) {
  const facts = [
    ["cooldown", byLevel(d.cooldown_s, seconds)],
    ["cast", byLevel(d.cast_s, seconds)],
    ["MP", byLevel(d.mp, (n) => fmt(n))],
    ["range", d.range],
  ].filter(([, v]) => v);
  if (facts.length) {
    const line = el("div", "st-facts");
    for (const [k, v] of facts) {
      const f = el("span");
      f.append(el("span", "k", `${k} `), document.createTextNode(v));
      line.append(f);
    }
    tip.append(line);
  }
  let scaled = Array.isArray(d.cooldown_s) || Array.isArray(d.cast_s) || Array.isArray(d.mp);
  if (d.desc) {
    const merged = mergeLevels(d.desc, d.desc_max);
    const p = el("div", "st-desc");
    if (merged) {
      p.append(...merged.nodes);
      scaled = scaled || merged.scaled;
    } else p.textContent = d.desc;
    tip.append(p);
  }
  if (d.properties && !compact) tip.append(el("div", "st-props", d.properties.split(" | ").join(" · ")));
  if (d.specs && d.specs.length && !compact) {
    const chosen = d.specs.filter((sp) => d.chosen.includes(specNumber(sp)));
    tip.append(el("div", "st-group", chosen.length ? `specialties · ${chosen.length} chosen` : "specialties · none chosen"));
    // The chosen ones in full; the others one line each, dimmed.
    for (const spec of d.specs) {
      const n = specNumber(spec);
      const on = d.chosen.includes(n);
      const row = el("div", `st-spec${on ? " on" : ""}`);
      row.append(el("span", "st-mark", on ? "✓" : String(n)), el("span", "st-spec-text", spec.desc));
      if (spec.unlock) row.append(el("span", "st-unlock", `lv ${spec.unlock}`));
      tip.append(row);
    }
  }
  return scaled;
}

function tipHead(tip, iconUrl, title, kind) {
  const head = el("div", "st-head");
  head.append(iconOrBlank(iconUrl, ""));
  const t = el("div", "st-title");
  t.append(el("div", "st-name", title), el("div", "st-kind", kind));
  head.append(t);
  tip.append(head);
}

function buildSkillTip(id) {
  const tip = $("skill-tip");
  tip.replaceChildren();
  const d = skillInfo(id);
  const kind = d ? [d.class, d.subtype || d.type, d.weapon].filter(Boolean).join(" · ") : "";
  tipHead(tip, skillIconUrl(id), skillName(id), kind || `skill ${id}`);
  const scaled = d ? appendSkillDetails(tip, d, false) : false;

  // From this fight: how often the selected player cast it.
  const s = selectedPlayer() && selectedPlayer().skills.get(id);
  if (s && s.castTimes.length) {
    const t = s.castTimes;
    let shortest = Infinity;
    for (let i = 1; i < t.length; i++) shortest = Math.min(shortest, t[i] - t[i - 1]);
    const parts = [`${t.length} cast${t.length === 1 ? "" : "s"}`];
    if (shortest < Infinity) parts.push(`shortest gap ${gap(shortest)}`);
    tip.append(el("div", "st-measured", `this fight · ${parts.join(" · ")}`));
  }
  if (scaled) tip.append(el("div", "st-note", `a → b: level 1 → ${d.max_level || "max"} (the game doesn't say which level)`));
  else if (!d) tip.append(el("div", "st-note", "no description in the game data"));
}

// A buff or debuff: what it is, how it went in this fight, and what its skill does.
function buildEffectTip(lane) {
  const tip = $("skill-tip");
  tip.replaceChildren();
  const on = lane.kind === "buff" ? name(selection.player) : name(lane.spans[0].target);
  tipHead(tip, iconUrlFor(lane.iconKey), lane.label, `${lane.kind} on ${on}`);
  const line = el("div", "st-facts");
  const fact = (k, v) => {
    const f = el("span");
    f.append(el("span", "k", `${k} `), document.createTextNode(v));
    line.append(f);
  };
  fact("uptime", `${Math.round(lane.uptime * 100)}%`);
  fact("applied", `${lane.spans.length}×`);
  if (lane.lasts) fact("lasts", seconds(lane.lasts / 1000));
  else if (lane.longest) fact("lasts", `up to ${gap(lane.longest)}`);
  tip.append(line);
  const sources = [...new Set(lane.spans.map((s) => s.source).filter((x) => x != null))];
  if (sources.length) {
    const more = sources.length > 3 ? ` +${sources.length - 3}` : "";
    tip.append(el("div", "st-measured", `from ${sources.slice(0, 3).map(name).join(", ")}${more}`));
  }
  const d = lane.skill != null ? skillInfo(lane.skill) : null;
  let scaled = false;
  if (d) {
    tip.append(el("div", "st-group", `from the skill ${skillName(lane.skill)}`));
    scaled = appendSkillDetails(tip, d, true);
  }
  if (lane.siblings) {
    tip.append(el("div", "st-measured", `one of ${lane.siblings} effects this skill puts up; the game data doesn't name them apart`));
  }
  if (scaled) tip.append(el("div", "st-note", `a → b: level 1 → ${d.max_level || "max"}`));
  tip.append(el("div", "st-note", `effect ${lane.effect} · double-click the label to name it`));
}

// The tooltip follows the pointer, kept inside the window.
function placeSkillTip(x, y) {
  const tip = $("skill-tip");
  const pad = 14;
  const w = tip.offsetWidth;
  const h = tip.offsetHeight;
  const left = x + pad + w > innerWidth ? x - pad - w : x + pad;
  tip.style.left = `${Math.max(4, left)}px`;
  tip.style.top = `${Math.max(4, Math.min(innerHeight - h - 4, y + pad))}px`;
}

let tipFor = null; // what the tooltip shows: "skill:<id>" or "effect:<lane key>"
let tipHost = null; // the element (or canvas box) it belongs to

function showTip(key, host, build, x, y) {
  if (tipFor !== key) {
    tipFor = key;
    build();
  }
  tipHost = host;
  $("skill-tip").hidden = false;
  placeSkillTip(x, y);
}

function hideSkillTip() {
  $("skill-tip").hidden = true;
  tipFor = null;
  tipHost = null;
}

// For a canvas: show the tooltip `key` (built by `build`) at the pointer, or hide it.
function canvasTip(box, key, e, build) {
  box.querySelector("canvas").style.cursor = key ? "help" : "";
  if (key && e) showTip(key, box, build, e.clientX, e.clientY);
  else if (tipHost === box) hideSkillTip();
}

// Any element with data-skill (or data-effect, a buff lane) shows its tooltip while the
// pointer is on it.
function tipTarget(e) {
  const host = e.target.closest && e.target.closest("[data-skill], [data-effect]");
  if (!host) return null;
  if (host.dataset.skill) {
    const id = Number(host.dataset.skill);
    return { host, key: `skill:${id}`, build: () => buildSkillTip(id) };
  }
  const lane = lanesByKey.get(host.dataset.effect);
  return lane ? { host, key: `effect:${lane.key}`, build: () => buildEffectTip(lane) } : null;
}
document.addEventListener("mouseover", (e) => {
  const t = tipTarget(e);
  if (t) showTip(t.key, t.host, t.build, e.clientX, e.clientY);
});
document.addEventListener("mouseout", (e) => {
  if (tipHost && tipHost.dataset && tipHost.contains(e.target) && !tipHost.contains(e.relatedTarget)) hideSkillTip();
});
document.addEventListener("mousemove", (e) => {
  if (!$("skill-tip").hidden && tipHost && tipHost.dataset && (tipHost.dataset.skill || tipHost.dataset.effect)) placeSkillTip(e.clientX, e.clientY);
});

$("close").addEventListener("click", () => win.hide());
$("max").addEventListener("click", () => win.toggleMaximize());
document.addEventListener("keydown", (e) => {
  if (e.key === "Escape") win.hide();
  const n = Number(e.key);
  if (n >= 1 && n <= 6 && !e.ctrlKey && !e.altKey) {
    tab = ["overview", "skills", "rotation", "buffs", "healing", "taken"][n - 1];
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

// A theme or font change in the settings shows here straight away.
tauri.event.listen("appearance", (e) => {
  applyAppearance(e.payload);
  queueRedraw();
});

(async () => {
  invoke("appearance").then((a) => {
    applyAppearance(a);
    queueRedraw();
  }).catch(() => {});
  selection = await invoke("analysis_selection").catch(() => selection);
  await load(true);
  // Live fights refresh every second, or less often when a refresh is costly.
  const poll = async () => {
    // A hidden window still says it's visible (document.hidden), so ask the window.
    const shown = await win.isVisible().catch(() => true);
    if (selection.fight == null && shown) await load(false);
    setTimeout(poll, Math.min(5000, Math.max(POLL_MS, loadMs * 10)));
  };
  setTimeout(poll, POLL_MS);
})();
