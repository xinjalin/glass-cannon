// The vitals arcs widget: arcs either side of your character, as GT Companion's center HUD
// has them, with no box. At first on the left your HP (outer) with your stamina inside it,
// on the right your MP with your flight power inside it (each can go to any side, outside or
// in, or be turned off), each filling from the bottom; your cast
// under your feet. Drawn to the widget's size (arrange it around your character), in the
// look chosen in the settings (overlay → vitals arcs): solid, thin, segmented, gradient,
// framed or ticked arcs, a damage trail on the HP arc, how
// round and thick they are, whether their tracks always show, the numbers (value, percent or both; their size,
// what's behind them, at the arcs' top or bottom ends), and the cast as an arc or a straight
// bar with its time at each end. Max HP is the game's; until it sends it, the highest HP
// seen stands in. Stamina and flight power read full until the game sends them (it sends
// them only as they change).

// Smooth enough for a cast.
const POLL_MS = 100;
// Below this share of HP, the HP arc glows.
const LOW_HP = 0.3;
// HP warning colours (settings → vitals arcs): the HP arc yellow from half HP, red from a
// fifth.
const HP_WARN = [
  [0.2, "#e5383b"],
  [0.5, "#f5c542"],
];
// Stamina and flight power are sent in hundredths of what the game shows.
const HUNDREDTHS = 100;

// ---------------------------------------------------------------- the look

// As the settings have it (vitals.rs); these until they're read.
let setup = { style: "solid", effect: "none", roundness: 50, text_size: 100, text_backing: "outline", text_at: "top", numbers: "value", cast: "arc", width: 100, always_show: false, show_cast: true, trail: false, hp_warn: false, hidden_arcs: [], arc_spots: {}, hide_full: false };

// The side arcs hidden for now because they're full (hide when full).
const fullNow = new Set();

// Each side arc's spot: a side, outside or in (as at first, unless moved in the settings).
const SPOTS = { hp: "left-outer", stamina: "left-inner", mp: "right-outer", flight: "right-inner" };
const spotOf = (k) => (setup.arc_spots || {})[k] || SPOTS[k];

// A side arc turned off in the settings (only HP and stamina, say).
const arcOff = (k) => (setup.hidden_arcs || []).includes(k);

// At roundness 50 (as the arcs always were) each side arc bends this far either side of its
// middle, and the cast's arc this far either side of straight down; at 100, twice as far;
// at 0, straight. Angles in degrees.
const SIDE_BEND = 28;
const CAST_BEND = 32;
// The cast's circle at roundness 50, as a share of the side arcs'.
const CAST_R = 0.42;
// Segmented arcs: cells of this much of the arc (of 100), less the gap between them.
const CELL = 5;
const CELL_GAP = 1.2;
// Ticks: marks across the side arcs at these shares.
const TICKS = [0.25, 0.5, 0.75];
// The damage trail: how long the HP lost stays before it shrinks away (the shrinking is
// panel.css's).
const TRAIL_HOLD_MS = 600;

const SVG = "http://www.w3.org/2000/svg";
const ARCS = ["hp", "stamina", "mp", "flight", "cast"];

function svgEl(tag, cls) {
  const e = document.createElementNS(SVG, tag);
  if (cls) e.setAttribute("class", cls);
  return e;
}

// Each arc's track, fill and numbers (its value; its percent when both are shown, or the
// cast bar's time left), and the cast bar's whole time; plates behind the numbers.
const parts = {};
let plates;
let ticks;
function build() {
  const svg = $("arcs");
  plates = svgEl("g");
  const defs = svgEl("defs");
  const lines = svgEl("g");
  ticks = svgEl("g", "arc-ticks");
  const texts = svgEl("g");
  svg.replaceChildren(defs, plates, lines, ticks, texts);
  for (const k of ARCS) {
    // The framed look's frame, under the track.
    const frame = svgEl("path", `arc-frame ${k}`);
    const track = svgEl("path", `arc-track ${k}`);
    const fill = svgEl("path", `arc-fill ${k}`);
    fill.setAttribute("pathLength", "100");
    track.setAttribute("pathLength", "100");
    lines.append(frame, track);
    // The HP arc's damage trail, under its fill.
    let trail = null;
    if (k === "hp") {
      trail = svgEl("path", "arc-trail hp");
      trail.setAttribute("pathLength", "100");
      lines.append(trail);
    }
    lines.append(fill);
    // The gradient look's: dark at the arc's start, its colour where the fill ends (moved
    // there as it fills).
    const grad = svgEl("linearGradient", `arc-grad ${k}`);
    grad.id = `grad-${k}`;
    grad.setAttribute("gradientUnits", "userSpaceOnUse");
    grad.append(svgEl("stop", "g0"), svgEl("stop", "g1"));
    grad.lastChild.setAttribute("offset", "1");
    defs.append(grad);
    const value = svgEl("text", `arc-text ${k}`);
    const pct = svgEl("text", `arc-text arc-pct ${k}`);
    texts.append(value, pct);
    parts[k] = { frame, track, trail, fill, grad, value, pct };
  }
  // The shield, over the HP arc's fill.
  parts.shield = svgEl("path", "arc-shield");
  parts.shield.setAttribute("pathLength", "100");
  lines.append(parts.shield);
  parts.castEnd = svgEl("text", "arc-text cast cast-end");
  texts.append(parts.castEnd);
}

function applySetup() {
  const svg = $("arcs");
  // Ticks are the thin look with marks.
  const look = setup.style === "ticks" ? "look-thin look-ticks" : `look-${setup.style}`;
  svg.setAttribute("class", `arcs ${look} effect-${setup.effect || "none"} backing-${setup.text_backing}${svg.classList.contains("low") ? " low" : ""}`);
  svg.style.setProperty("--vt", String(setup.text_size / 100));
  // Colours of your own; the rest as built in (panel.css).
  for (const k of ["hp", "stamina", "mp", "flight", "cast", "shield"]) {
    const c = (setup.colours || {})[k];
    if (isHexColor(c)) svg.style.setProperty(`--arc-${k}`, c);
    else svg.style.removeProperty(`--arc-${k}`);
  }
  hpColourNow = undefined;
  layoutArcs();
  draw(lastBars);
  flameColours();
  runFlames();
}

// The HP arc's colour as its HP has it (HP warning colours): null, its own.
let hpColourNow;
function hpColour(hp) {
  const warn = setup.hp_warn && hp != null ? HP_WARN.find(([at]) => hp <= at) : null;
  const colour = warn ? warn[1] : null;
  if (colour === hpColourNow) return;
  hpColourNow = colour;
  const svg = $("arcs");
  const own = (setup.colours || {}).hp;
  if (colour) svg.style.setProperty("--arc-hp", colour);
  else if (isHexColor(own)) svg.style.setProperty("--arc-hp", own);
  else svg.style.removeProperty("--arc-hp");
  // The flames in it too.
  flameColours();
}

// ---------------------------------------------------------------- geometry

const fix = ([x, y]) => `${x.toFixed(1)} ${y.toFixed(1)}`;
const rad = (deg) => (deg * Math.PI) / 180;

function geometry() {
  const svg = $("arcs");
  const w = svg.clientWidth;
  const h = svg.clientHeight;
  const fs = parseFloat(getComputedStyle(document.documentElement).fontSize) || 13;
  const ts = (fs * setup.text_size) / 100;
  const thin = setup.style === "thin" || setup.style === "ticks";
  // The arcs' thickness, as the look has it times the width chosen.
  const k = (setup.width || 100) / 100;
  const outer = (thin ? Math.max(2, fs * 0.2) : Math.max(5, fs * 0.6)) * k;
  const inner = (thin ? Math.max(2, fs * 0.16) : Math.max(3, fs * 0.38)) * k;
  const gap = thin ? Math.max(4, fs * 0.5) : inner * 0.7;
  // Room past the glow.
  // And past the ticks look's notches (see drawTicks).
  // And past the flames effect's tongues (see the flames, below).
  const pad = outer / 2 + (thin ? 4 : 1) + (setup.style === "ticks" ? fs * 0.95 : 0) + (setup.effect === "flames" ? fs * FLAME_ROOM * k : 0);
  // Room for the numbers past the arcs' ends (the same at both, so the arcs stay centred),
  // from the overlay's text size alone: the numbers' own settings never move the arcs.
  const room = fs * 1.6;
  const len = Math.max(20, h - 2 * pad - 2 * room);
  const bend = Math.min(2 * SIDE_BEND, (SIDE_BEND * setup.roundness) / 50);
  return { w, h, fs, ts, outer, inner, gap, pad, len, bend, cx: w / 2, cy: h / 2 };
}

// A side arc, `d` in from the outer one's line, bottom to top: its path and its two ends.
// Round, it's part of a circle bulging outward with the outer arc `len` tall; straight, a
// line.
function sideArc(g, left, d) {
  const inward = left ? 1 : -1;
  const edge = left ? g.pad : g.w - g.pad;
  if (g.bend < 0.5) {
    const x = edge + inward * d;
    const bottom = [x, g.cy + g.len / 2];
    const top = [x, g.cy - g.len / 2];
    return { d: `M ${fix(bottom)} L ${fix(top)}`, top, bottom };
  }
  const t = rad(g.bend);
  const big = g.len / (2 * Math.sin(t));
  const cx = edge + inward * big;
  const r = big - d;
  const x = cx - inward * r * Math.cos(t);
  const bottom = [x, g.cy + r * Math.sin(t)];
  const top = [x, g.cy - r * Math.sin(t)];
  return { d: `M ${fix(bottom)} A ${r.toFixed(1)} ${r.toFixed(1)} 0 0 ${left ? 1 : 0} ${fix(top)}`, top, bottom };
}

// The cast: an arc under your feet (as round as the side arcs), or a straight bar there.
function castShape(g) {
  const base = Math.min(g.w / 2 - g.pad, g.len / (2 * Math.sin(rad(SIDE_BEND)))) * CAST_R;
  const low = g.cy + base;
  const half = base * Math.sin(rad(CAST_BEND));
  const left = [g.cx - half, low];
  const right = [g.cx + half, low];
  const bend = Math.min(2 * CAST_BEND, (CAST_BEND * setup.roundness) / 50);
  if (setup.cast === "bar" || bend < 0.5) {
    const bar = setup.cast === "bar";
    // The bar a little wider than the arc, level with its lowest point.
    const b = bar ? half * 1.25 : half;
    return { d: `M ${fix([g.cx - b, low])} L ${fix([g.cx + b, low])}`, left: [g.cx - b, low], right: [g.cx + b, low], low, bar };
  }
  // The same width whatever the bend: a circle through both ends with its lowest point at
  // `low`.
  const r = half / Math.sin(rad(bend));
  const top = low - r * (1 - Math.cos(rad(bend)));
  return { d: `M ${fix([left[0], top])} A ${r.toFixed(1)} ${r.toFixed(1)} 0 0 0 ${fix([right[0], top])}`, left: [left[0], top], right: [right[0], top], low, bar: false };
}

function place(t, [x, y], anchor) {
  t.setAttribute("x", x.toFixed(1));
  t.setAttribute("y", y.toFixed(1));
  t.setAttribute("text-anchor", anchor);
}

// Lays the arcs and their numbers out for the widget's size and the look.
function layoutArcs() {
  const svg = $("arcs");
  const g = geometry();
  if (!g.w || !g.h) return;
  svg.setAttribute("viewBox", `0 0 ${g.w} ${g.h}`);
  const step = g.outer / 2 + g.gap + g.inner / 2;
  // Each arc in its spot; on a side whose outer arc is off, the inner one moves out to its
  // place (two arcs, one each side, say).
  const arcs = {};
  const sides = [];
  // Each side arc's layer as drawn (an inner arc alone on its side is drawn outside).
  const layers = {};
  for (const side of ["left", "right"]) {
    const left = side === "left";
    const at = (layer) => Object.keys(SPOTS).find((k) => spotOf(k) === `${side}-${layer}`);
    const [o, i] = [at("outer"), at("inner")];
    const outer = [sideArc(g, left, 0), g.outer];
    if (o) [arcs[o], layers[o]] = [outer, "outer"];
    if (i) [arcs[i], layers[i]] = [[sideArc(g, left, step), g.inner], "inner"];
    const shown = [o, i].filter((k) => k && !arcOff(k));
    if (shown.length === 1) [arcs[shown[0]], layers[shown[0]]] = [outer, "outer"];
    if (shown.length) sides.push([shown[0], shown[1], left]);
  }
  const cast = castShape(g);
  arcs.cast = [cast, cast.bar ? Math.max(2, g.inner * 0.6) : g.inner];
  const framed = setup.style === "framed";
  // Framed: the fill inset in its track, the frame just outside it.
  const fillWidth = (width) => (framed ? Math.max(1.5, width - Math.max(2, width * 0.3)) : width);
  for (const [k, [shape, width]] of Object.entries(arcs)) {
    const p = parts[k];
    for (const e of [p.frame, p.track, p.trail, p.fill]) if (e) e.setAttribute("d", shape.d);
    p.track.style.strokeWidth = `${width}px`;
    p.fill.style.strokeWidth = `${fillWidth(width)}px`;
    if (p.trail) p.trail.style.strokeWidth = `${fillWidth(width)}px`;
    p.frame.style.strokeWidth = `${width + 3}px`;
    p.track.style.strokeDasharray = setup.style === "segmented" && k !== "cast" ? `${CELL - CELL_GAP} ${CELL_GAP}` : "";
  }
  parts.shield.setAttribute("d", arcs.hp[0].d);
  parts.shield.style.strokeWidth = `${fillWidth(arcs.hp[1])}px`;
  drawTicks(arcs, g, sides.map(([o]) => o));
  layoutFlames(arcs, layers, g);

  // The numbers, the outer arc's over the inner's, beside the chosen ends and inward of
  // both arcs' ends: the arcs bend outward from their ends, so nothing of them is further
  // in, and the numbers never cross them. With both, the percents go past the ends (over
  // the top ones, under the bottom ones), either side of the gap between the two arcs,
  // and the values a line further along the arcs, clear of them.
  const ts = g.ts;
  const both = setup.numbers === "both";
  for (const [outerKey, innerKey, left] of sides) {
    // One arc alone on its side: its numbers placed as the outer's, by it.
    const o = arcs[outerKey][0];
    const i = innerKey ? arcs[innerKey][0] : o;
    const inward = left ? 1 : -1;
    const anchor = left ? "start" : "end";
    const away = left ? "end" : "start";
    const end = setup.text_at === "bottom" ? "bottom" : "top";
    // Between the two arcs' ends, and a little either side of it.
    const mid = (o[end][0] + i[end][0]) / 2;
    const apart = ts * 0.15;
    const x = i[end][0] + inward * ((innerKey ? g.inner : g.outer) / 2 + ts * 0.35);
    if (end === "bottom") {
      // Upward from the outer arc's bottom end, as the top's mirrored: the outer arc's
      // value lowest, by its end (the lower one), the inner arc's over it, by its own.
      const y = o.bottom[1] + ts * 0.75 - (both ? ts : 0);
      place(parts[outerKey].value, [x, y], anchor);
      const py = Math.max(o.bottom[1], i.bottom[1]) + g.outer / 2 + ts * 0.9;
      if (!innerKey) place(parts[outerKey].pct, [mid, py], "middle");
      else {
        place(parts[innerKey].value, [x, y - ts], anchor);
        place(parts[outerKey].pct, [mid - inward * apart, py], away);
        place(parts[innerKey].pct, [mid + inward * apart, py], anchor);
      }
    } else {
      // Downward from the outer arc's top end.
      const y = o.top[1] - ts * 0.15 + (both ? ts : 0);
      place(parts[outerKey].value, [x, y], anchor);
      const py = Math.min(o.top[1], i.top[1]) - g.outer / 2 - ts * 0.45;
      if (!innerKey) place(parts[outerKey].pct, [mid, py], "middle");
      else {
        place(parts[innerKey].value, [x, y + ts], anchor);
        place(parts[outerKey].pct, [mid - inward * apart, py], away);
        place(parts[innerKey].pct, [mid + inward * apart, py], anchor);
      }
    }
  }
  if (cast.bar) {
    // The skill over the bar, the time left under its left end, the whole time under its
    // right.
    place(parts.cast.value, [g.cx, cast.low - ts * 0.55], "middle");
    place(parts.cast.pct, [cast.left[0], cast.low + ts * 1.0], "start");
    place(parts.castEnd, [cast.right[0], cast.low + ts * 1.0], "end");
  } else {
    place(parts.cast.value, [g.cx, cast.low + g.inner + ts], "middle");
  }
  drawPlates();
}

// The ticks look's marks: notches like a ruler's on the outside of each side's outer arc
// (`outers`: HP and MP at first), clear of both arcs, at a quarter, half (longer) and three
// quarters of the way up.
function drawTicks(arcs, g, outers) {
  const marks = [];
  if (setup.style === "ticks") {
    for (const k of outers) {
      const path = parts[k].track;
      const len = path.getTotalLength();
      const from = arcs[k][1] / 2 + Math.max(2, g.fs * 0.15);
      for (const t of TICKS) {
        const [a, b] = [path.getPointAtLength(len * t - 0.5), path.getPointAtLength(len * t + 0.5)];
        // Across the arc (its direction there, turned a quarter), away from the middle.
        const [dx, dy] = [b.x - a.x, b.y - a.y];
        const n = Math.hypot(dx, dy) || 1;
        const [mx, my] = [(a.x + b.x) / 2, (a.y + b.y) / 2];
        let [nx, ny] = [-dy / n, dx / n];
        if ((mx + nx - g.cx) ** 2 < (mx - g.cx) ** 2) [nx, ny] = [-nx, -ny];
        const long = g.fs * (t === 0.5 ? 0.75 : 0.45);
        const line = svgEl("line", `arc-tick ${k}`);
        line.dataset.at = String(t);
        line.setAttribute("x1", (mx + nx * from).toFixed(1));
        line.setAttribute("y1", (my + ny * from).toFixed(1));
        line.setAttribute("x2", (mx + nx * (from + long)).toFixed(1));
        line.setAttribute("y2", (my + ny * (from + long)).toFixed(1));
        marks.push(line);
      }
    }
  }
  ticks.replaceChildren(...marks);
}

// The gradient look: an arc's gradient from its start to where its fill ends.
function placeGradient(k, share) {
  const p = parts[k];
  const len = p.fill.getTotalLength();
  if (!len) return;
  const a = p.fill.getPointAtLength(0);
  const b = p.fill.getPointAtLength(len * Math.max(0.02, Math.min(1, share)));
  for (const [attr, v] of [["x1", a.x], ["y1", a.y], ["x2", b.x], ["y2", b.y]]) p.grad.setAttribute(attr, v.toFixed(1));
}

// The damage trail: the HP arc's fill as it was before a hit, held a moment, then let go
// down to the HP now (panel.css shrinks it). Up again, it follows at once (the fill
// covers it).
let trailAt = null;
let trailTimer = null;
let hpNow = null;
function drawTrail(pct) {
  const t = parts.hp.trail;
  hpNow = pct;
  if (!setup.trail || pct == null) {
    t.style.display = "none";
    trailAt = null;
    clearTimeout(trailTimer);
    trailTimer = null;
    return;
  }
  t.style.display = "";
  if (trailAt == null || pct >= trailAt) {
    trailAt = pct;
    clearTimeout(trailTimer);
    trailTimer = null;
  } else if (!trailTimer) {
    trailTimer = setTimeout(() => {
      trailTimer = null;
      if (hpNow != null && trailAt != null && hpNow < trailAt) {
        trailAt = hpNow;
        setTrail();
      }
    }, TRAIL_HOLD_MS);
  }
  setTrail();
}

function setTrail() {
  parts.hp.trail.style.strokeDasharray = setup.style === "segmented" ? cells(0, trailAt) : `${trailAt.toFixed(2)} 100`;
}

// ---------------------------------------------------------------- drawing

// A segmented arc's fill from `from` to `to` (in hundredths of the arc): the cells' parts
// in that stretch, so the gaps stay where the track's are. A dash of nothing first, so
// the gaps and dashes alternate from there.
function cells(from, to) {
  const dashes = [0];
  let pos = 0;
  for (let at = Math.floor(from / CELL) * CELL; at < to - 0.001; at += CELL) {
    const a = Math.max(from, at);
    const b = Math.min(to, at + CELL - CELL_GAP);
    if (b - a <= 0.05) continue;
    dashes.push((a - pos).toFixed(2), (b - a).toFixed(2));
    pos = b;
  }
  dashes.push(100);
  return dashes.join(" ");
}

function arc(k, share, value, percent, shielded = false) {
  const p = parts[k];
  // Hide when full: a side arc at the top (HP with no shield on it) goes until it drops.
  const full = setup.hide_full && k !== "cast" && share != null && share >= 0.999 && !shielded;
  if (full) fullNow.add(k);
  else fullNow.delete(k);
  // Turned off, or full: none of it.
  flameShare[k] = arcOff(k) || full || share == null ? null : Math.max(0, Math.min(1, share));
  if (arcOff(k) || full) {
    for (const e of [p.frame, p.track, p.fill, p.value, p.pct]) e.style.display = "none";
    return;
  }
  const known = share != null;
  // Always shown: the empty track stays up with nothing to fill it.
  p.track.style.display = known || setup.always_show ? "" : "none";
  // The framed look's frame goes with its track.
  p.frame.style.display = setup.style === "framed" ? p.track.style.display : "none";
  for (const e of [p.fill, p.value, p.pct]) e.style.display = known ? "" : "none";
  if (!known) return;
  const pct = Math.max(0, Math.min(1, share)) * 100;
  p.fill.style.strokeDasharray = setup.style === "segmented" && k !== "cast" ? cells(0, pct) : `${pct.toFixed(2)} 100`;
  if (setup.style === "gradient") placeGradient(k, share);
  p.value.textContent = setup.numbers === "percent" ? percent : value;
  p.pct.textContent = setup.numbers === "both" ? percent : "";
}

// "value/max", in the game's units.
const pair = (v, max, scale = 1) => `${Math.round(v / scale).toLocaleString("en-US")}/${Math.round(max / scale).toLocaleString("en-US")}`;

function drawArcs(b) {
  const share = (v, max) => (v != null && max ? v / max : null);
  const side = (cls, v, max, scale, shielded) => {
    const s = share(v, max);
    arc(cls, s, s != null ? pair(v, max, scale) : "", s != null ? `${Math.round(Math.min(1, s) * 100)}` : "", shielded);
  };
  const hp = share(b && b.hp, b && b.hp_max);
  // The shield: on along the arc from the HP's end (a dash after a gap the HP's length).
  const span = b && !arcOff("hp") ? shieldSpan(b.hp, b.hp_max, b.shield) : null;
  side("hp", b && b.hp, b && b.hp_max, 1, !!span);
  drawTrail(hp != null && !arcOff("hp") && !fullNow.has("hp") ? Math.max(0, Math.min(1, hp)) * 100 : null);
  parts.shield.style.display = span ? "" : "none";
  // The ticks look's HP notches beside the shield, in its colour.
  for (const l of ticks.querySelectorAll(".arc-tick.hp")) {
    const at = Number(l.dataset.at);
    l.classList.toggle("shielded", !!span && at >= span[0] - 0.005 && at <= span[0] + span[1] + 0.005);
  }
  if (span) {
    const [from, to] = [span[0] * 100, (span[0] + span[1]) * 100];
    // Segmented: in the cells, as the HP is. Otherwise one dash, moved along to where it
    // starts (a dash of nothing first would show as a dot with round ends).
    const segmented = setup.style === "segmented";
    parts.shield.style.strokeDasharray = segmented ? cells(from, to) : `${(to - from).toFixed(2)} 200`;
    parts.shield.style.strokeDashoffset = segmented ? "" : (-from).toFixed(2);
  }
  // Its amount after your HP's, in its colour.
  if (span && b.hp != null && setup.numbers !== "percent") {
    const t = svgEl("tspan", "arc-shield-text");
    t.textContent = ` +${Math.round(b.shield).toLocaleString("en-US")}`;
    parts.hp.value.append(t);
  }
  side("stamina", b && b.stamina, b && b.stamina_max, HUNDREDTHS);
  side("mp", b && b.mp, b && b.mp_max, 1);
  side("flight", b && b.flight, b && b.flight_max, HUNDREDTHS);
  // A full arc's ruler notches go with it.
  for (const l of ticks.querySelectorAll(".arc-tick")) l.style.display = [...fullNow].some((k) => l.classList.contains(k)) ? "none" : "";
  $("arcs").classList.toggle("low", hp != null && hp < LOW_HP);
  hpColour(hp);
  const c = b && b.cast;
  const name = c ? c.skill_name || "casting" : "";
  const left = c ? (c.remaining_ms / 1000).toFixed(1) : "";
  if (setup.cast === "bar") {
    arc("cast", c ? 1 - c.remaining_ms / Math.max(1, c.cast_ms) : null, name, "");
    parts.cast.pct.textContent = left;
    parts.castEnd.textContent = c ? (c.cast_ms / 1000).toFixed(1) : "";
  } else {
    arc("cast", c ? 1 - c.remaining_ms / Math.max(1, c.cast_ms) : null, c ? `${name} ${left}` : "", "");
    parts.cast.pct.textContent = "";
    parts.castEnd.textContent = "";
  }
  parts.castEnd.style.display = c && setup.cast === "bar" ? "" : "none";
  // The cast turned off: none of it, track included.
  if (setup.show_cast === false) {
    for (const e of [parts.cast.frame, parts.cast.track, parts.cast.fill, parts.cast.value, parts.cast.pct, parts.castEnd]) e.style.display = "none";
  }
}

// A dark plate behind each number shown (backing "plate").
function drawPlates() {
  if (!plates) return;
  if (setup.text_backing !== "plate") {
    if (plates.childElementCount) plates.replaceChildren();
    return;
  }
  const rects = [];
  for (const t of $("arcs").querySelectorAll(".arc-text")) {
    if (t.style.display === "none" || !t.textContent) continue;
    const bb = t.getBBox();
    if (!bb.width) continue;
    const r = svgEl("rect", "arc-plate");
    const padX = bb.height * 0.3;
    const padY = bb.height * 0.06;
    r.setAttribute("x", (bb.x - padX).toFixed(1));
    r.setAttribute("y", (bb.y - padY).toFixed(1));
    r.setAttribute("width", (bb.width + 2 * padX).toFixed(1));
    r.setAttribute("height", (bb.height + 2 * padY).toFixed(1));
    r.setAttribute("rx", (bb.height * 0.3).toFixed(1));
    rects.push(r);
  }
  plates.replaceChildren(...rects);
}

// ---------------------------------------------------------------- flames

// The flames effect (settings → vitals arcs → effect): each side arc burns along its fill
// in its own colour, white-hot where the flames start, its colour, then dark as they die.
// An outer arc's flames go outward, away from your character, an inner arc's (shorter)
// inward, toward it: the two never cross, so each arc's colour stays its own. Drawn on a
// canvas under the arcs, a few hundred small glows at most, and only while it shows.

// Room for the outer arcs' flames past them, in text heights (times the arcs' width).
const FLAME_ROOM = 2.4;
const FLAME_FPS = 40;
// Flames born a second along a full outer arc (an inner one's half that), and how many
// more at low HP.
const FLAME_RATE = 450;
const FLAME_LOW = 1.7;
// How long a flame lasts, in seconds, and how far it goes in that time, in text heights
// (outward, and up, as fire goes).
const FLAME_LIFE = [0.35, 0.8];
const FLAME_OUT = [0.6, 1.8];
const FLAME_UP = [0.8, 2.0];
// Points along each arc that flames start from.
const FLAME_POINTS = 64;

// Each side arc's share now (null: not shown), as drawn.
const flameShare = {};
// Each side arc's points (its fill, bottom to top) and their way out; the flames; the
// glows in each arc's colour.
let flameArcs = {};
let flameList = [];
let flameSprites = {};
let flameFs = 13;
let flameRaf = 0;
let flameLast = 0;

const rand = ([a, b]) => a + Math.random() * (b - a);

function flameCanvas() {
  let c = document.getElementById("flames");
  if (!c) {
    c = document.createElement("canvas");
    c.id = "flames";
    c.className = "flames";
    $("vitals").prepend(c);
  }
  return c;
}

function layoutFlames(arcs, layers, g) {
  const c = flameCanvas();
  const dpr = window.devicePixelRatio || 1;
  c.width = Math.round(g.w * dpr);
  c.height = Math.round(g.h * dpr);
  c.getContext("2d").setTransform(dpr, 0, 0, dpr, 0, 0);
  flameFs = g.fs;
  flameArcs = {};
  if (setup.effect !== "flames") return;
  for (const [k, layer] of Object.entries(layers)) {
    const path = parts[k].fill;
    const len = path.getTotalLength();
    if (!len) continue;
    const out = layer === "outer";
    const pts = [];
    for (let i = 0; i < FLAME_POINTS; i++) {
      const at = (len * i) / (FLAME_POINTS - 1);
      const [a, b] = [path.getPointAtLength(Math.max(0, at - 0.5)), path.getPointAtLength(Math.min(len, at + 0.5))];
      const [dx, dy] = [b.x - a.x, b.y - a.y];
      const n = Math.hypot(dx, dy) || 1;
      let [nx, ny] = [-dy / n, dx / n];
      const p = path.getPointAtLength(at);
      // Away from the middle for an outer arc, toward it for an inner one.
      const away = (p.x + nx - g.cx) ** 2 > (p.x - g.cx) ** 2;
      if (away !== out) [nx, ny] = [-nx, -ny];
      pts.push({ x: p.x, y: p.y, nx, ny });
    }
    flameArcs[k] = { pts, width: arcs[k][1], out };
  }
}

// A soft round glow in a colour: the hot one white at its middle, the dying one darker.
function flameSprite(rgb, hot) {
  const size = 64;
  const c = document.createElement("canvas");
  c.width = c.height = size;
  const x = c.getContext("2d");
  const g = x.createRadialGradient(size / 2, size / 2, 0, size / 2, size / 2, size / 2);
  const [r, gr, b] = rgb;
  const mix = (t, to) => rgb.map((v, i) => Math.round(v + (to[i] - v) * t)).join(",");
  if (hot) {
    g.addColorStop(0, `rgba(${mix(0.5, [255, 255, 255])},1)`);
    g.addColorStop(0.3, `rgba(${mix(0.15, [255, 255, 255])},0.85)`);
    g.addColorStop(0.65, `rgba(${r},${gr},${b},0.45)`);
  } else {
    g.addColorStop(0, `rgba(${r},${gr},${b},0.7)`);
    g.addColorStop(0.5, `rgba(${mix(0.45, [0, 0, 0])},0.3)`);
  }
  g.addColorStop(1, `rgba(${r},${gr},${b},0)`);
  x.fillStyle = g;
  x.fillRect(0, 0, size, size);
  return c;
}

// The arcs' colours as they are now (their own, or as built in).
function flameColours() {
  flameSprites = {};
  if (setup.effect !== "flames") return;
  const css = getComputedStyle($("arcs"));
  const probe = document.createElement("canvas").getContext("2d");
  for (const k of ["hp", "stamina", "mp", "flight"]) {
    probe.fillStyle = "#000";
    probe.fillStyle = css.getPropertyValue(`--arc-${k}`).trim() || "#ffffff";
    const hex = probe.fillStyle;
    const rgb = hex.startsWith("#") ? [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16)) : (hex.match(/\d+/g) || [255, 255, 255]).slice(0, 3).map(Number);
    flameSprites[k] = { hot: flameSprite(rgb, true), cool: flameSprite(rgb, false) };
  }
}

function runFlames() {
  if (setup.effect === "flames" && !document.hidden) {
    if (!flameRaf) {
      flameLast = 0;
      flameRaf = requestAnimationFrame(flameFrame);
    }
    return;
  }
  if (flameRaf) cancelAnimationFrame(flameRaf);
  flameRaf = 0;
  flameList = [];
  const c = document.getElementById("flames");
  if (c) c.getContext("2d").clearRect(0, 0, c.width, c.height);
}

function flameFrame(now) {
  flameRaf = requestAnimationFrame(flameFrame);
  if (flameLast && now - flameLast < 1000 / FLAME_FPS - 2) return;
  const dt = flameLast ? Math.min(0.1, (now - flameLast) / 1000) : 0;
  flameLast = now;
  const fs = flameFs;
  const k = (setup.width || 100) / 100;
  const low = $("arcs").classList.contains("low");
  // New flames along each arc's fill.
  for (const [key, a] of Object.entries(flameArcs)) {
    const share = flameShare[key];
    if (!share || !flameSprites[key]) continue;
    // As many to the arc's length whatever its thickness: thicker flames, fewer of them
    // (more would only add up to white, losing the arc's colour).
    const rate = (FLAME_RATE * share * (a.out ? 1 : 0.5) * (key === "hp" && low ? FLAME_LOW : 1)) / Math.max(1, k);
    let n = rate * dt;
    while (n > 0) {
      if (n < 1 && Math.random() > n) break;
      n -= 1;
      const p = a.pts[Math.round(Math.random() * share * (a.pts.length - 1))];
      // From the edge it burns away from (none across into the gap between two arcs).
      const across = rand([0.05, 0.5]) * a.width;
      const reach = a.out ? 1 : 0.5;
      const life = rand(FLAME_LIFE) * (a.out ? 1 : 0.8);
      flameList.push({
        key,
        x: p.x + p.nx * across,
        y: p.y + p.ny * across,
        ox: p.x + p.nx * across,
        oy: p.y + p.ny * across,
        vx: (p.nx * rand(FLAME_OUT) * fs * reach * k) / life,
        vy: (p.ny * rand(FLAME_OUT) * fs * reach * k - rand(FLAME_UP) * fs * reach) / life,
        size: (a.width * 1.4 + fs * 0.6 * (a.out ? 1 : 0.7)) * rand([0.7, 1.3]),
        age: 0,
        life,
        wob: Math.random() * 6.3,
      });
    }
  }
  const c = document.getElementById("flames");
  const x = c.getContext("2d");
  x.clearRect(0, 0, c.width, c.height);
  x.globalCompositeOperation = "lighter";
  // Embers along each arc's burning edge, flickering, so its fire is one.
  for (const [key, a] of Object.entries(flameArcs)) {
    const share = flameShare[key];
    const s = flameSprites[key];
    if (!share || !s) continue;
    const last = Math.round(share * (a.pts.length - 1));
    const r = a.width * 1.1 + fs * 0.25;
    for (let i = 0; i <= last; i++) {
      const p = a.pts[i];
      x.globalAlpha = (a.out ? 0.24 : 0.17) * (0.6 + 0.4 * Math.sin(now * 0.009 + i * 1.7));
      const [ex, ey] = [p.x + p.nx * a.width * 0.45, p.y + p.ny * a.width * 0.45];
      x.drawImage(s.hot, ex - r, ey - r, 2 * r, 2 * r);
    }
  }
  const kept = [];
  for (const f of flameList) {
    f.age += dt;
    if (f.age >= f.life || !flameSprites[f.key]) continue;
    kept.push(f);
    const t = f.age / f.life;
    // Fast at first, then slowing, with a flicker side to side.
    const slow = 1 - t * 0.6;
    f.x += (f.vx * slow + Math.sin(f.age * 14 + f.wob) * fs * 0.35) * dt;
    f.y += f.vy * slow * dt;
    const r = (f.size * (1 - t * 0.7)) / 2;
    const fade = (1 - t) ** 1.3;
    const s = flameSprites[f.key];
    // A tongue from where it started on the arc out to where it's got to, narrower as it
    // goes; late on, its base lifts off too, and it dies away from the arc.
    const lift = Math.max(0, (t - 0.45) / 0.55);
    const [bx, by] = [f.ox + (f.x - f.ox) * lift, f.oy + (f.y - f.oy) * lift];
    const reach = Math.hypot(f.x - bx, f.y - by);
    const angle = reach > 0.5 ? Math.atan2(f.y - by, f.x - bx) : Math.atan2(f.vy, f.vx);
    const [w, len] = [r * (1.1 - t * 0.4), reach + r * 2];
    x.save();
    x.translate(bx, by);
    x.rotate(angle + Math.PI / 2);
    // White-hot at first, its colour after, dark as it dies.
    const hot = Math.max(0, 1 - t / 0.4);
    if (hot > 0) {
      x.globalAlpha = fade * hot * 0.7;
      x.drawImage(s.hot, -w, -len, 2 * w, len + r * 0.4);
    }
    x.globalAlpha = fade * (1 - hot * 0.5) * 0.85;
    x.drawImage(s.cool, -w * 1.2, -len * 1.1, 2.4 * w, len * 1.1 + r * 0.4);
    x.restore();
  }
  x.globalAlpha = 1;
  flameList = kept;
}

document.addEventListener("visibilitychange", runFlames);

// ---------------------------------------------------------------- refresh

let lastBars = null;
function draw(b) {
  lastBars = b;
  $("empty").hidden = !!(b && (b.hp != null || b.mp != null));
  drawArcs(b);
  drawPlates();
}

async function refresh() {
  let b = null;
  try {
    b = await invoke("self_bars");
  } catch {
    return;
  }
  draw(b);
}

build();
new ResizeObserver(layoutArcs).observe(document.documentElement);
tauri.event.listen("appearance", () =>
  requestAnimationFrame(() => {
    layoutArcs();
    flameColours();
  }),
);
tauri.event.listen("vitals-setup", (e) => {
  if (e.payload) setup = e.payload;
  applySetup();
});
invoke("vitals_setup")
  .then((s) => {
    if (s) setup = s;
    applySetup();
  })
  .catch(() => applySetup());
applySetup();
setInterval(() => {
  if (!document.hidden) refresh();
}, POLL_MS);
refresh();
