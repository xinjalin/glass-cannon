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
let setup = { style: "solid", effect: "none", effect_strength: 100, effect_brightness: 100, effect_size: 100, effect_speed: 100, roundness: 50, text_size: 100, text_backing: "outline", text_at: "top", numbers: "value", cast: "arc", width: 100, always_show: false, show_cast: true, trail: false, hp_warn: false, hidden_arcs: [], arc_spots: {}, hide_full: false };

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
  svg.setAttribute("class", `arcs ${look} effect-${setup.effect || "none"}${FX[setup.effect] ? " fx" : ""} backing-${setup.text_backing}${svg.classList.contains("low") ? " low" : ""}`);
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
  fxColours();
  runFx();
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
  // The effect in it too.
  fxColours();
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
  const outer1 = thin ? Math.max(2, fs * 0.2) : Math.max(5, fs * 0.6);
  const outer = outer1 * k;
  const inner = (thin ? Math.max(2, fs * 0.16) : Math.max(3, fs * 0.38)) * k;
  const gap = thin ? Math.max(4, fs * 0.5) : inner * 0.7;
  // Room past the glow, and past the ticks look's notches (see drawTicks). None for an
  // effect: that draws past the widget's own box (see the bleed, below), so choosing one
  // never moves or resizes the arcs.
  const past = (thin ? 4 : 1) + (setup.style === "ticks" ? fs * 0.95 : 0);
  // The outer arcs' outer edges stay where they are whatever their width (they thicken
  // inward), and so does their length: `pad0` is the room at the look's own width.
  const pad = outer / 2 + past;
  const pad0 = outer1 / 2 + past;
  // Room for the numbers past the arcs' ends (the same at both, so the arcs stay centred),
  // from the overlay's text size alone: the numbers' own settings never move the arcs.
  const room = fs * 1.6;
  const len = Math.max(20, h - 2 * pad0 - 2 * room);
  const bend = Math.min(2 * SIDE_BEND, (SIDE_BEND * setup.roundness) / 50);
  return { w, h, fs, ts, outer, inner, gap, pad, pad0, len, bend, cx: w / 2, cy: h / 2 };
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
  const base = Math.min(g.w / 2 - g.pad0, g.len / (2 * Math.sin(rad(SIDE_BEND)))) * CAST_R;
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
  layoutFx(arcs, layers, g);

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
  fxShare[k] = arcOff(k) || full || share == null ? null : Math.max(0, Math.min(1, share));
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

// ---------------------------------------------------------------- effects

// The effects (settings → vitals arcs → effect): flames, lightning, holy light, darkness,
// blood, earth, nature, void or wind along each side arc's fill, in the arc's own colour:
// the element only sets how it moves and what shape it takes. An outer arc's go outward,
// away from your character, an inner arc's (smaller) inward, toward it: the two never
// cross, so each arc's colour stays its own. Drawn on one canvas under the arcs by one
// loop, only while it shows, from a fixed pool of pieces (nothing new made each frame).
// With less motion asked for (the system's reduce motion setting), each holds still: its
// glow along the arcs, nothing moving.

const FX_FPS = 40;
// The most pieces at once, all the arcs together.
const FX_MAX = 1000;
// Points along each arc that the effects follow.
const FX_POINTS = 64;

// The flames: how long one lasts, in seconds, and how far it goes in that time, in text
// heights (outward, and up, as fire goes).
const FLAME_LIFE = [0.35, 0.8];
const FLAME_OUT = [0.6, 1.8];
const FLAME_UP = [0.8, 2.0];
// Lightning: steps along a bolt.
const BOLT_STEPS = 9;
// Wind: steps along a streak.
const WIND_STEPS = 10;
// Earth: the chips' shapes, corners around their middle (x, y, x, y, ...).
const CHIPS = [
  [-1, -0.6, 0.3, -1, 1, -0.1, 0.4, 0.9, -0.8, 0.6],
  [-0.9, -0.9, 0.9, -0.5, 0.6, 0.8, -0.6, 0.5],
  [0, -1, 1, 0.4, -0.2, 1, -1, 0.1],
];

// Each side arc's share now (null: not shown), as drawn.
const fxShare = {};
// Each side arc's points (bottom to top), their way out and up, how thick and long it is;
// what each effect draws with, in each arc's colour.
let fxArcs = [];
let fxSprites = {};
let fxFs = 13;
let fxRaf = 0;
let fxLast = 0;
// The effect's own time (ms), running at its speed.
let fxClock = 0;
// The effect's settings: how much of it, how clearly it shows, how far it reaches, how
// fast.
const fxStrength = () => (setup.effect_strength || 100) / 100;
const fxBrightness = () => (setup.effect_brightness || 100) / 100;
const fxSize = () => (setup.effect_size || 100) / 100;
const fxSpeed = () => (setup.effect_speed || 100) / 100;
// A piece's opacity at the brightness chosen.
let fxGain = 1;
const lit = (v) => Math.min(1, v * fxGain);
// The effect the live pieces are of: another's start afresh.
let fxFor = "";

// The bleed: an effect reaches past the arcs, and the arcs go right to the widget's edges,
// so the overlay makes this page bigger than the widget's box by this much all round, once
// and for good (the page's address says how much: ?bleed=). The arcs' box is inset by it,
// so it's the widget's own whatever the effect, and nothing moves or resizes as effects are
// chosen; the effect's canvas reaches into it as far as the effect does.
const bleed = Math.max(0, Number(new URLSearchParams(location.search).get("bleed")) || 0);
document.documentElement.style.setProperty("--bleed", `${bleed}px`);
function fxReach(fs) {
  const fx = FX[setup.effect];
  const k = (setup.width || 100) / 100;
  return fx ? Math.min(bleed, Math.ceil(fs * fx.room * Math.max(1, k) * fxSize()) + 4) : 0;
}
// The pieces: the first `fxLive` are alive, the rest wait to be used again.
const fxPool = Array.from({ length: FX_MAX }, () => ({ arc: null, kind: 0, x: 0, y: 0, ox: 0, oy: 0, vx: 0, vy: 0, size: 0, age: 0, life: 1, wob: 0, u: 0, s: 0, ph: 0, rot: 0, spin: 0, seed: 0 }));
let fxLive = 0;
const calm = window.matchMedia("(prefers-reduced-motion: reduce)");

const rand = ([a, b]) => a + Math.random() * (b - a);
const rnd = (a, b) => a + Math.random() * (b - a);
// The same number from the same two numbers, 0 to 1: a bolt's zigzag until it changes.
const hash = (s, j) => {
  const v = Math.sin(s * 12.9898 + j * 78.233) * 43758.5453;
  return v - Math.floor(v);
};

// A point along an arc, `u` of the way up it (0 to 1): where it is, its way out (away from
// your character for an outer arc, toward it for an inner one) and its way up the arc.
const P = { x: 0, y: 0, nx: 0, ny: 0, tx: 0, ty: 0 };
function at(a, u) {
  const n = a.pts.length - 1;
  const f = Math.max(0, Math.min(1, u)) * n;
  const i = Math.min(n - 1, Math.floor(f));
  const r = f - i;
  const p = a.pts[i];
  const q = a.pts[i + 1];
  P.x = p.x + (q.x - p.x) * r;
  P.y = p.y + (q.y - p.y) * r;
  P.nx = p.nx + (q.nx - p.nx) * r;
  P.ny = p.ny + (q.ny - p.ny) * r;
  P.tx = p.tx + (q.tx - p.tx) * r;
  P.ty = p.ty + (q.ty - p.ty) * r;
}

function fxCanvas() {
  let c = document.getElementById("fx");
  if (!c) {
    c = document.createElement("canvas");
    c.id = "fx";
    c.className = "arc-fx";
    $("vitals").prepend(c);
  }
  return c;
}

// The canvas cleared, bleed and all.
function fxClear(c, x) {
  x.save();
  x.setTransform(1, 0, 0, 1, 0, 0);
  x.clearRect(0, 0, c.width, c.height);
  x.restore();
}

function layoutFx(arcs, layers, g) {
  const c = fxCanvas();
  const dpr = window.devicePixelRatio || 1;
  // Over the arcs' box and as far past it as the effect reaches, drawn in the arcs' own
  // coordinates. Sized anew only when that changes (sizing a canvas clears it).
  const reach = fxReach(g.fs);
  const [w, h] = [Math.round((g.w + 2 * reach) * dpr), Math.round((g.h + 2 * reach) * dpr)];
  if (c.width !== w || c.height !== h) {
    c.width = w;
    c.height = h;
  }
  Object.assign(c.style, { left: `${-reach}px`, top: `${-reach}px`, width: `${g.w + 2 * reach}px`, height: `${g.h + 2 * reach}px` });
  c.getContext("2d").setTransform(dpr, 0, 0, dpr, reach * dpr, reach * dpr);
  fxFs = g.fs;
  // The pieces carry on through a change of setting (its strength, its colours, the arcs'
  // look): only another effect's go.
  if (fxFor !== setup.effect) {
    fxFor = setup.effect;
    fxLive = 0;
  }
  const before = new Map(fxArcs.map((a) => [a.key, a]));
  fxArcs = [];
  if (!FX[setup.effect]) {
    fxLive = 0;
    return;
  }
  for (const [k, layer] of Object.entries(layers)) {
    const path = parts[k].fill;
    const len = path.getTotalLength();
    if (!len) continue;
    const out = layer === "outer";
    const pts = [];
    for (let i = 0; i < FX_POINTS; i++) {
      const at = (len * i) / (FX_POINTS - 1);
      const [a, b] = [path.getPointAtLength(Math.max(0, at - 0.5)), path.getPointAtLength(Math.min(len, at + 0.5))];
      const [dx, dy] = [b.x - a.x, b.y - a.y];
      const n = Math.hypot(dx, dy) || 1;
      let [nx, ny] = [-dy / n, dx / n];
      const p = path.getPointAtLength(at);
      // Away from the middle for an outer arc, toward it for an inner one.
      const away = (p.x + nx - g.cx) ** 2 > (p.x - g.cx) ** 2;
      if (away !== out) [nx, ny] = [-nx, -ny];
      pts.push({ x: p.x, y: p.y, nx, ny, tx: dx / n, ty: dy / n });
    }
    // The same arc as before, laid out again: the pieces on it stay on it.
    fxArcs.push(Object.assign(before.get(k) || { key: k }, { pts, width: arcs[k][1], out, len }));
  }
  // Pieces on an arc that's gone go with it.
  for (let i = 0; i < fxLive; ) {
    if (fxArcs.includes(fxPool[i].arc)) {
      i++;
      continue;
    }
    const f = fxPool[i];
    fxPool[i] = fxPool[fxLive - 1];
    fxPool[fxLive - 1] = f;
    fxLive--;
  }
  if (calm.matches) fxStill();
}

// ---- what the effects draw with

const WHITE = [255, 255, 255];
const BLACK = [0, 0, 0];
// A colour `t` of the way to another.
const tint = (rgb, to, t) => rgb.map((v, i) => Math.round(v + (to[i] - v) * t));
const rgba = (c, a) => `rgba(${c[0]},${c[1]},${c[2]},${a})`;

function sprite(w, h, paint) {
  const c = document.createElement("canvas");
  c.width = w;
  c.height = h;
  paint(c.getContext("2d"), w, h);
  return c;
}

// A soft round glow, its colours from the middle out.
function glow(stops) {
  return sprite(64, 64, (x) => {
    const g = x.createRadialGradient(32, 32, 0, 32, 32, 32);
    for (const [at, c] of stops) g.addColorStop(at, c);
    x.fillStyle = g;
    x.fillRect(0, 0, 64, 64);
  });
}

// ---- flames: each arc burns along its fill, white-hot where the flames start, its colour,
// then dark as they die.

function paintFlames(rgb) {
  return {
    hot: glow([[0, rgba(tint(rgb, WHITE, 0.5), 1)], [0.3, rgba(tint(rgb, WHITE, 0.15), 0.85)], [0.65, rgba(rgb, 0.45)], [1, rgba(rgb, 0)]]),
    cool: glow([[0, rgba(rgb, 0.7)], [0.5, rgba(tint(rgb, BLACK, 0.45), 0.3)], [1, rgba(rgb, 0)]]),
  };
}

function spawnFlame(f, a, share, fs, k) {
  // From the edge it burns away from (none across into the gap between two arcs).
  const across = rnd(0.05, 0.5) * a.width;
  const reach = a.out ? 1 : 0.5;
  const life = rand(FLAME_LIFE) * (a.out ? 1 : 0.8);
  f.x = f.ox = P.x + P.nx * across;
  f.y = f.oy = P.y + P.ny * across;
  f.vx = (P.nx * rand(FLAME_OUT) * fs * reach * k) / life;
  f.vy = (P.ny * rand(FLAME_OUT) * fs * reach * k - rand(FLAME_UP) * fs * reach) / life;
  f.size = (a.width * 1.4 + fs * 0.6 * (a.out ? 1 : 0.7)) * rnd(0.7, 1.3);
  f.life = life;
  f.wob = Math.random() * 6.3;
}

// Embers along each arc's burning edge, flickering, so its fire is one.
function flameEmbers(x, a, s, last, share, now, fs) {
  const r = a.width * 1.1 + fs * 0.25;
  for (let i = 0; i <= last; i++) {
    const p = a.pts[i];
    x.globalAlpha = lit((a.out ? 0.24 : 0.17) * (0.6 + 0.4 * Math.sin(now * 0.009 + i * 1.7)));
    const ex = p.x + p.nx * a.width * 0.45;
    const ey = p.y + p.ny * a.width * 0.45;
    x.drawImage(s.hot, ex - r, ey - r, 2 * r, 2 * r);
  }
}

function stepFlame(x, f, a, s, t, dt, fs) {
  // Fast at first, then slowing, with a flicker side to side.
  const slow = 1 - t * 0.6;
  f.x += (f.vx * slow + Math.sin(f.age * 14 + f.wob) * fs * 0.35) * dt;
  f.y += f.vy * slow * dt;
  const r = (f.size * (1 - t * 0.7)) / 2;
  const fade = (1 - t) ** 1.3;
  // A tongue from where it started on the arc out to where it's got to, narrower as it
  // goes; late on, its base lifts off too, and it dies away from the arc.
  const lift = Math.max(0, (t - 0.45) / 0.55);
  const bx = f.ox + (f.x - f.ox) * lift;
  const by = f.oy + (f.y - f.oy) * lift;
  const reach = Math.hypot(f.x - bx, f.y - by);
  const angle = reach > 0.5 ? Math.atan2(f.y - by, f.x - bx) : Math.atan2(f.vy, f.vx);
  const w = r * (1.1 - t * 0.4);
  const len = reach + r * 2;
  x.save();
  x.translate(bx, by);
  x.rotate(angle + Math.PI / 2);
  // White-hot at first, its colour after, dark as it dies.
  const hot = Math.max(0, 1 - t / 0.4);
  if (hot > 0) {
    x.globalAlpha = lit(fade * hot * 0.7);
    x.drawImage(s.hot, -w, -len, 2 * w, len + r * 0.4);
  }
  x.globalAlpha = lit(fade * (1 - hot * 0.5) * 0.85);
  x.drawImage(s.cool, -w * 1.2, -len * 1.1, 2.4 * w, len * 1.1 + r * 0.4);
  x.restore();
}

// ---- lightning: jagged bolts crackling along each arc, flickering, a new zigzag every few
// hundredths of a second, some forking outward; the arc's edge buzzing under them.

function paintLightning(rgb) {
  return {
    glow: glow([[0, rgba(tint(rgb, WHITE, 0.4), 0.9)], [0.4, rgba(rgb, 0.4)], [1, rgba(rgb, 0)]]),
    halo: rgba(rgb, 1),
    core: rgba(tint(rgb, WHITE, 0.7), 1),
  };
}

function spawnBolt(f, a, share) {
  f.s = Math.min(share, rnd(0.12, 0.3));
  f.u = Math.random() * (share - f.s);
  f.life = rnd(0.16, 0.34);
  f.seed = Math.random() * 1000;
  f.ph = 0;
}

function boltBuzz(x, a, s, last, share, now, fs) {
  const r = a.width * 0.8 + fs * 0.2;
  for (let i = 0; i <= last; i += 2) {
    const p = a.pts[i];
    x.globalAlpha = lit((a.out ? 0.14 : 0.1) * (0.5 + Math.random()));
    const ex = p.x + p.nx * a.width * 0.5;
    const ey = p.y + p.ny * a.width * 0.5;
    x.drawImage(s.glow, ex - r, ey - r, 2 * r, 2 * r);
  }
}

function stepBolt(x, f, a, s, t, dt, fs) {
  f.ph -= dt;
  if (f.ph <= 0) {
    f.seed = Math.random() * 1000;
    f.ph = rnd(0.03, 0.07);
  }
  const reach = a.out ? 1 : 0.5;
  const amp = a.width * 0.8 + fs * 1.0 * reach;
  const fork = hash(f.seed, 3) > 0.35 ? 2 + Math.floor(hash(f.seed, 4) * (BOLT_STEPS - 4)) : -1;
  let bx = 0;
  let by = 0;
  let bnx = 0;
  let bny = 0;
  let btx = 0;
  let bty = 0;
  x.beginPath();
  for (let j = 0; j <= BOLT_STEPS; j++) {
    at(a, f.u + (f.s * j) / BOLT_STEPS);
    // Its ends on the arc's edge, the rest zigzagging out from it.
    const off = j === 0 || j === BOLT_STEPS ? a.width * 0.5 : a.width * 0.2 + hash(f.seed, j) * amp;
    const px = P.x + P.nx * off;
    const py = P.y + P.ny * off;
    if (j === 0) x.moveTo(px, py);
    else x.lineTo(px, py);
    if (j === fork) {
      bx = px;
      by = py;
      bnx = P.nx;
      bny = P.ny;
      btx = P.tx;
      bty = P.ty;
    }
  }
  if (fork >= 0) {
    // A fork off it, outward.
    x.moveTo(bx, by);
    const len = fs * 0.35 * reach;
    for (let j = 1; j <= 3; j++) {
      const side = (hash(f.seed, 10 + j) - 0.5) * 2 * len;
      bx += bnx * len + btx * side;
      by += bny * len + bty * side;
      x.lineTo(bx, by);
    }
  }
  const fade = (1 - t * t) * (hash(f.seed, 7) > 0.25 ? 1 : 0.35);
  x.lineJoin = "round";
  x.lineCap = "round";
  x.strokeStyle = s.halo;
  x.lineWidth = 3 + a.width * 0.35 + fs * 0.12;
  x.globalAlpha = lit(0.45 * fade);
  x.stroke();
  x.strokeStyle = s.core;
  x.lineWidth = Math.max(1, 0.8 + a.width * 0.08);
  x.globalAlpha = lit(0.95 * fade);
  x.stroke();
}

// ---- holy: a soft halo breathing along each arc, shafts of light fading in and out
// straight from it, and motes of light twinkling up from it.

function paintHoly(rgb) {
  const bright = tint(rgb, WHITE, 0.55);
  return {
    halo: glow([[0, rgba(bright, 0.8)], [0.45, rgba(rgb, 0.35)], [1, rgba(rgb, 0)]]),
    // A shaft of light from its base (left) narrowing and fading to its tip, soft at its
    // sides.
    ray: sprite(128, 32, (x, w, h) => {
      const g = x.createLinearGradient(0, 0, w, 0);
      g.addColorStop(0, rgba(bright, 1));
      g.addColorStop(0.35, rgba(rgb, 0.7));
      g.addColorStop(1, rgba(rgb, 0));
      x.fillStyle = g;
      x.beginPath();
      x.moveTo(0, 0);
      x.lineTo(w, h * 0.4);
      x.lineTo(w, h * 0.6);
      x.lineTo(0, h);
      x.fill();
      x.globalCompositeOperation = "destination-in";
      const side = x.createLinearGradient(0, 0, 0, h);
      side.addColorStop(0, "rgba(0,0,0,0)");
      side.addColorStop(0.5, "rgba(0,0,0,1)");
      side.addColorStop(1, "rgba(0,0,0,0)");
      x.fillStyle = side;
      x.fillRect(0, 0, w, h);
    }),
    // A glint: a small glow with a cross of light through it.
    star: sprite(64, 64, (x) => {
      const g = x.createRadialGradient(32, 32, 0, 32, 32, 14);
      g.addColorStop(0, rgba(WHITE, 1));
      g.addColorStop(0.4, rgba(bright, 0.8));
      g.addColorStop(1, rgba(rgb, 0));
      x.fillStyle = g;
      x.fillRect(0, 0, 64, 64);
      for (const across of [false, true]) {
        const l = across ? x.createLinearGradient(0, 0, 64, 0) : x.createLinearGradient(0, 0, 0, 64);
        l.addColorStop(0, rgba(rgb, 0));
        l.addColorStop(0.5, rgba(bright, 1));
        l.addColorStop(1, rgba(rgb, 0));
        x.fillStyle = l;
        if (across) x.fillRect(0, 31, 64, 2);
        else x.fillRect(31, 0, 2, 64);
      }
    }),
  };
}

function spawnHoly(f, a, share, fs, k) {
  const reach = a.out ? 1 : 0.5;
  const ex = P.x + P.nx * a.width * 0.5;
  const ey = P.y + P.ny * a.width * 0.5;
  if (Math.random() < 0.35) {
    f.kind = 1;
    f.x = ex;
    f.y = ey;
    f.rot = Math.atan2(P.ny, P.nx) + rnd(-0.15, 0.15);
    f.size = a.width * 0.9 + fs * 0.45;
    f.s = fs * rnd(1.3, 2.4) * reach * Math.sqrt(k);
    f.life = rnd(0.9, 1.6);
  } else {
    f.kind = 2;
    const out = rnd(0, fs * 0.6 * reach);
    f.x = ex + P.nx * out;
    f.y = ey + P.ny * out;
    f.vx = P.nx * fs * 0.3 * reach;
    f.vy = -fs * rnd(0.4, 0.9) * (a.out ? 1 : 0.6);
    f.size = fs * rnd(0.6, 1.0) * (a.out ? 1 : 0.8);
    f.life = rnd(1.0, 1.8);
    f.wob = Math.random() * 6.3;
  }
}

function holyHalo(x, a, s, last, share, now, fs) {
  const r = a.width * 1.3 + fs * 0.45;
  for (let i = 0; i <= last; i += 2) {
    const p = a.pts[i];
    x.globalAlpha = lit((a.out ? 0.18 : 0.12) * (0.7 + 0.3 * Math.sin(now * 0.0025 - i * 0.12)));
    const ex = p.x + p.nx * a.width * 0.5;
    const ey = p.y + p.ny * a.width * 0.5;
    x.drawImage(s.halo, ex - r, ey - r, 2 * r, 2 * r);
  }
}

function stepHoly(x, f, a, s, t, dt) {
  const rise = Math.sin(Math.PI * t);
  if (f.kind === 1) {
    x.globalAlpha = lit(rise * 0.75);
    const len = f.s * (0.75 + 0.25 * rise);
    x.save();
    x.translate(f.x, f.y);
    x.rotate(f.rot);
    x.drawImage(s.ray, 0, -f.size / 2, len, f.size);
    x.restore();
    return;
  }
  f.x += f.vx * dt;
  f.y += f.vy * dt;
  x.globalAlpha = lit(rise * (0.55 + 0.45 * Math.sin(f.age * 9 + f.wob)));
  const r = f.size / 2;
  x.drawImage(s.star, f.x - r, f.y - r, 2 * r, 2 * r);
}

// ---- darkness: smoky wisps, dark and tinted with the arc's colour, curling slowly off it,
// over a dark haze along it.

function paintDarkness(rgb) {
  const deep = tint(rgb, BLACK, 0.65);
  return {
    smoke: glow([[0, rgba(deep, 0.75)], [0.5, rgba(tint(rgb, BLACK, 0.8), 0.4)], [1, "rgba(0,0,0,0)"]]),
    wisp: glow([[0, rgba(rgb, 0.6)], [0.6, rgba(rgb, 0.12)], [1, rgba(rgb, 0)]]),
  };
}

function spawnDark(f, a, share, fs) {
  const reach = a.out ? 1 : 0.5;
  const from = rnd(0.3, 0.6) * a.width;
  f.x = P.x + P.nx * from;
  f.y = P.y + P.ny * from;
  f.vx = P.nx * fs * rnd(0.5, 1.2) * reach;
  f.vy = P.ny * fs * rnd(0.5, 1.2) * reach - fs * rnd(0.2, 0.6) * reach;
  f.size = (a.width * 1.3 + fs * 0.9) * rnd(0.8, 1.2) * (a.out ? 1 : 0.7);
  f.life = rnd(1.1, 2.0);
  f.wob = Math.random() * 6.3;
}

function darkHaze(x, a, s, last, share, now, fs) {
  const r = a.width * 1.1 + fs * 0.35;
  for (let i = 0; i <= last; i += 3) {
    const p = a.pts[i];
    x.globalAlpha = lit((a.out ? 0.4 : 0.3) * (0.7 + 0.3 * Math.sin(now * 0.002 + i * 0.5)));
    const ex = p.x + p.nx * a.width * 0.6;
    const ey = p.y + p.ny * a.width * 0.6;
    x.drawImage(s.smoke, ex - r, ey - r, 2 * r, 2 * r);
  }
}

function stepDark(x, f, a, s, t, dt, fs) {
  // Slowing as it spreads, curling side to side.
  const slow = 1 - t * 0.5;
  f.x += (f.vx * slow + Math.sin(f.age * 2.6 + f.wob) * fs * 0.6 * (a.out ? 1 : 0.5)) * dt;
  f.y += f.vy * slow * dt;
  const r = (f.size * (0.5 + t * 1.3)) / 2;
  const fade = Math.sin(Math.PI * t);
  x.save();
  x.translate(f.x, f.y);
  x.rotate(Math.atan2(f.vy, f.vx) + Math.sin(f.age * 2 + f.wob) * 0.6);
  x.globalAlpha = lit(fade * 0.6);
  x.drawImage(s.smoke, -r * 1.2, -r, 2.4 * r, 2 * r);
  // A thread of the arc's colour in it.
  x.globalCompositeOperation = "lighter";
  x.globalAlpha = lit(fade * 0.3);
  x.drawImage(s.wisp, -r * 1.2, -r * 0.45, 2.4 * r, 0.9 * r);
  x.globalCompositeOperation = "source-over";
  x.restore();
}

// ---- blood: drops beading on each arc's edge, running down it and dripping off where it
// turns under (or at its foot); the arc pulsing as a heartbeat.

function paintBlood(rgb) {
  const body = tint(rgb, BLACK, 0.3);
  return {
    // A drop, round at the bottom and drawn to a point at the top, with a glint; its round
    // end's middle at (16, 33).
    drop: sprite(32, 48, (x) => {
      x.beginPath();
      x.moveTo(16, 1);
      x.bezierCurveTo(19, 14, 30, 24, 30, 33);
      x.arc(16, 33, 14, 0, Math.PI);
      x.bezierCurveTo(2, 24, 13, 14, 16, 1);
      x.closePath();
      const g = x.createRadialGradient(12, 30, 1, 16, 33, 18);
      g.addColorStop(0, rgba(tint(rgb, WHITE, 0.15), 1));
      g.addColorStop(0.5, rgba(body, 1));
      g.addColorStop(1, rgba(tint(rgb, BLACK, 0.6), 1));
      x.fillStyle = g;
      x.fill();
      x.strokeStyle = "rgba(0,0,0,0.4)";
      x.lineWidth = 1.5;
      x.stroke();
      x.fillStyle = "rgba(255,255,255,0.65)";
      x.beginPath();
      x.ellipse(11, 29, 2.5, 4, -0.4, 0, Math.PI * 2);
      x.fill();
    }),
    pulse: glow([[0, rgba(rgb, 0.8)], [0.5, rgba(body, 0.3)], [1, rgba(rgb, 0)]]),
    trail: rgba(body, 1),
  };
}

function spawnBlood(f, a, share, fs) {
  f.u = f.ox = Math.random() * share;
  f.s = 0;
  f.ph = 0;
  f.size = a.width * 0.6 + fs * 0.3 * (a.out ? 1 : 0.75);
  // How long it takes to bead; it lasts until it's dripped off.
  f.wob = rnd(0.3, 0.6);
  f.life = 4;
  f.x = P.x;
  f.y = P.y;
}

// Lub-dub: two quick pulses a little over a second apart.
function bloodBeat(x, a, s, last, share, now, fs) {
  const p = ((now / 1000) % 1.1) / 1.1;
  const beat = Math.max(0, 1 - p * 7) + 0.6 * Math.max(0, 1 - Math.abs(p - 0.22) * 7);
  if (beat < 0.02) return;
  x.globalCompositeOperation = "lighter";
  const r = a.width * 1.0 + fs * 0.3;
  for (let i = 0; i <= last; i += 2) {
    const q = a.pts[i];
    x.globalAlpha = lit((a.out ? 0.3 : 0.2) * beat);
    x.drawImage(s.pulse, q.x - r, q.y - r, 2 * r, 2 * r);
  }
}

function stepBlood(x, f, a, s, t, dt, fs) {
  const r = (f.size / 2) * (f.ph === 0 ? 0.35 + 0.65 * Math.min(1, f.age / f.wob) : 1);
  let dx;
  let dy;
  let stretch;
  if (f.ph < 2) {
    if (f.ph === 0 && f.age >= f.wob) f.ph = 1;
    if (f.ph === 1) {
      // Running down the arc, faster and faster, a thin trail behind it.
      f.s += fs * 5 * dt;
      f.u = Math.max(0, f.u - (f.s * dt) / a.len);
      const from = Math.min(f.ox, f.u + 0.1);
      x.beginPath();
      for (let j = 0; j <= 4; j++) {
        at(a, from + ((f.u - from) * j) / 4);
        const off = a.width * 0.5 + r * 0.3;
        if (j === 0) x.moveTo(P.x + P.nx * off, P.y + P.ny * off);
        else x.lineTo(P.x + P.nx * off, P.y + P.ny * off);
      }
      x.strokeStyle = s.trail;
      x.lineCap = "round";
      x.lineWidth = r * 0.55;
      x.globalAlpha = lit(0.6);
      x.stroke();
    }
    at(a, f.u);
    f.x = P.x + P.nx * (a.width * 0.5 + r * 0.55);
    f.y = P.y + P.ny * (a.width * 0.5 + r * 0.55);
    dx = -P.tx;
    dy = -P.ty;
    stretch = 1 + Math.min(0.5, f.s / (fs * 8));
    // Off the arc where it turns under, or at its foot.
    if (f.ph === 1 && (P.ny > 0.3 || f.u <= 0)) {
      f.ph = 2;
      f.vx = P.nx * fs * 0.6 - P.tx * f.s * 0.3;
      f.vy = Math.max(fs * 0.5, -P.ty * f.s * 0.5);
      f.life = f.age + rnd(0.55, 0.85);
    }
  } else {
    f.vy += fs * 18 * dt;
    f.x += f.vx * dt;
    f.y += f.vy * dt;
    dx = f.vx;
    dy = f.vy;
    stretch = 1 + Math.min(0.8, Math.hypot(f.vx, f.vy) / (fs * 16));
  }
  x.globalAlpha = lit(f.ph < 2 ? 1 : Math.min(1, (f.life - f.age) / 0.3));
  const h = r * 3 * stretch;
  x.save();
  x.translate(f.x, f.y);
  // The drop's point back the way it came.
  x.rotate(Math.atan2(dy, dx) - Math.PI / 2);
  x.drawImage(s.drop, -r, (-h * 33) / 48, 2 * r, h);
  x.restore();
}

// ---- earth: each arc crusted with stone along its fill, trembling, that crumbles off in
// chips tumbling away and puffs of dust.

function paintEarth(rgb) {
  return {
    face: rgba(tint(rgb, BLACK, 0.3), 1),
    edge: rgba(tint(rgb, BLACK, 0.7), 1),
    lit: rgba(tint(rgb, WHITE, 0.35), 1),
    dust: glow([[0, rgba(tint(rgb, [128, 118, 104], 0.45), 0.6)], [1, "rgba(0,0,0,0)"]]),
  };
}

// A chip of stone, `r` across its middle, turned `rot`, its top edge lit.
function chip(x, s, shape, cx, cy, r, rot) {
  const c = Math.cos(rot) * r;
  const n = Math.sin(rot) * r;
  x.beginPath();
  for (let j = 0; j < shape.length; j += 2) {
    const px = cx + shape[j] * c - shape[j + 1] * n;
    const py = cy + shape[j] * n + shape[j + 1] * c;
    if (j === 0) x.moveTo(px, py);
    else x.lineTo(px, py);
  }
  x.closePath();
  x.fillStyle = s.face;
  x.fill();
  x.lineWidth = 1;
  x.strokeStyle = s.edge;
  x.stroke();
  x.beginPath();
  x.moveTo(cx + shape[0] * c - shape[1] * n, cy + shape[0] * n + shape[1] * c);
  x.lineTo(cx + shape[2] * c - shape[3] * n, cy + shape[2] * n + shape[3] * c);
  x.strokeStyle = s.lit;
  x.stroke();
}

function spawnEarth(f, a, share, fs) {
  const reach = a.out ? 1 : 0.5;
  const ex = P.x + P.nx * a.width * 0.5;
  const ey = P.y + P.ny * a.width * 0.5;
  if (Math.random() < 0.4) {
    f.kind = 1;
    f.x = ex;
    f.y = ey;
    f.vx = P.nx * fs * rnd(0.8, 2.0) * reach;
    f.vy = P.ny * fs * rnd(0.8, 2.0) * reach - fs * rnd(0.4, 1.4) * reach;
    f.rot = Math.random() * 6.3;
    f.spin = rnd(-9, 9);
    f.size = (a.width * 0.3 + fs * 0.17) * rnd(0.7, 1.4) * (a.out ? 1 : 0.8);
    f.seed = Math.floor(Math.random() * CHIPS.length);
    f.life = rnd(0.6, 1.0);
  } else {
    f.kind = 2;
    const out = rnd(0, 0.5) * a.width;
    f.x = ex + P.nx * out;
    f.y = ey + P.ny * out;
    f.vx = P.nx * fs * rnd(0.2, 0.7) * reach;
    f.vy = fs * rnd(-0.3, 0.3);
    f.size = (a.width * 0.9 + fs * 0.5) * rnd(0.7, 1.3) * (a.out ? 1 : 0.7);
    f.life = rnd(0.8, 1.5);
  }
}

function earthCrust(x, a, s, last, share, now, fs) {
  x.globalAlpha = lit(1);
  for (let i = 0; i <= last; i += 2) {
    const p = a.pts[i];
    // A little tremble.
    const off = a.width * 0.4 + Math.sin(now * 0.03 + i * 2.1) * 0.5;
    const r = (a.width * 0.22 + fs * 0.1) * (0.6 + hash(i, 2) * 0.7) * (a.out ? 1 : 0.8);
    chip(x, s, CHIPS[i % CHIPS.length], p.x + p.nx * off, p.y + p.ny * off, r, hash(i, 1) * 6.3);
  }
}

function stepEarth(x, f, a, s, t, dt, fs) {
  if (f.kind === 1) {
    f.vy += fs * 14 * dt;
    f.x += f.vx * dt;
    f.y += f.vy * dt;
    f.rot += f.spin * dt;
    x.globalAlpha = lit(Math.min(1, (1 - t) * 4));
    chip(x, s, CHIPS[f.seed], f.x, f.y, f.size, f.rot);
    return;
  }
  f.x += f.vx * (1 - t * 0.5) * dt;
  f.y += f.vy * dt;
  const r = (f.size * (0.5 + t)) / 2;
  x.globalAlpha = lit(Math.sin(Math.PI * t) * 0.4);
  x.drawImage(s.dust, f.x - r, f.y - r, 2 * r, 2 * r);
}

// ---- nature: two vines winding along each arc's fill, swaying, with leaves on them
// (budding at the fill's end), now and then one coming loose and fluttering away.

function paintNature(rgb) {
  return {
    vine: rgba(tint(rgb, BLACK, 0.35), 0.95),
    tendril: rgba(tint(rgb, WHITE, 0.25), 0.85),
    // A leaf from its stalk (left) to its tip, a vein down its middle.
    leaf: sprite(48, 24, (x) => {
      x.beginPath();
      x.moveTo(2, 12);
      x.quadraticCurveTo(20, -2, 46, 12);
      x.quadraticCurveTo(20, 26, 2, 12);
      const g = x.createLinearGradient(0, 0, 0, 24);
      g.addColorStop(0, rgba(tint(rgb, WHITE, 0.3), 1));
      g.addColorStop(0.5, rgba(rgb, 1));
      g.addColorStop(1, rgba(tint(rgb, BLACK, 0.4), 1));
      x.fillStyle = g;
      x.fill();
      x.strokeStyle = "rgba(0,0,0,0.35)";
      x.lineWidth = 1;
      x.stroke();
      x.beginPath();
      x.moveTo(3, 12);
      x.lineTo(42, 12);
      x.strokeStyle = rgba(tint(rgb, BLACK, 0.5), 0.9);
      x.lineWidth = 1.2;
      x.stroke();
    }),
  };
}

function vines(x, a, s, last, share, now, fs) {
  const reach = a.out ? 1 : 0.6;
  // Mostly on the arc's outer side (an inner arc's inner side), never across into the gap
  // beside it.
  const mid = a.width * 0.3;
  const amp = a.width * 0.45 + fs * 0.15 * reach;
  const sway = Math.sin(now * 0.0011 + (a.out ? 0 : 1.3)) * 0.6;
  x.globalAlpha = lit(1);
  x.lineCap = "round";
  x.lineJoin = "round";
  for (let strand = 0; strand < 2; strand++) {
    x.beginPath();
    for (let i = 0; i <= last; i++) {
      const p = a.pts[i];
      const off = mid + Math.sin(i * 0.55 + sway + strand * Math.PI) * amp;
      if (i === 0) x.moveTo(p.x + p.nx * off, p.y + p.ny * off);
      else x.lineTo(p.x + p.nx * off, p.y + p.ny * off);
    }
    x.strokeStyle = strand ? s.tendril : s.vine;
    x.lineWidth = strand ? Math.max(1, a.width * 0.1 + 0.5) : Math.max(1.4, a.width * 0.18 + 0.8);
    x.stroke();
  }
  const big = (fs * 0.75 + a.width * 0.45) * (a.out ? 1 : 0.75);
  for (let i = 3; i <= last; i += 6) {
    const p = a.pts[i];
    const grow = Math.min(1, (last - i + 1) / 5);
    const len = big * grow;
    if (len < 2) continue;
    // Out from the arc, leaning up or down the arc by turns.
    const lean = (i / 6) % 2 < 1 ? 0.8 : -0.8;
    const ex = p.x + p.nx * a.width * 0.5;
    const ey = p.y + p.ny * a.width * 0.5;
    x.save();
    x.translate(ex, ey);
    x.rotate(Math.atan2(p.ny + p.ty * lean, p.nx + p.tx * lean) + Math.sin(now * 0.002 + i) * 0.15);
    x.drawImage(s.leaf, 0, -len / 4, len, len / 2);
    x.restore();
  }
}

function spawnLeaf(f, a, share, fs) {
  const reach = a.out ? 1 : 0.5;
  f.x = P.x + P.nx * a.width * 0.5;
  f.y = P.y + P.ny * a.width * 0.5;
  f.vx = P.nx * fs * rnd(0.5, 1.1) * reach;
  f.vy = -fs * rnd(0, 0.4);
  f.rot = Math.atan2(P.ny, P.nx);
  f.size = (fs * rnd(0.6, 0.8) + a.width * 0.35) * (a.out ? 1 : 0.75);
  f.life = rnd(1.8, 2.8);
  f.wob = Math.random() * 6.3;
}

function stepLeaf(x, f, a, s, t, dt, fs) {
  // Drifting down slowly, rocking side to side.
  f.vy = Math.min(fs * 1.1, f.vy + fs * 1.2 * dt);
  f.vx *= 1 - dt * 0.8;
  f.x += (f.vx + Math.sin(f.age * 3 + f.wob) * fs * 0.9 * (a.out ? 1 : 0.5)) * dt;
  f.y += f.vy * dt;
  x.globalAlpha = lit(Math.min(1, (1 - t) * 3, t * 6));
  x.save();
  x.translate(f.x, f.y);
  x.rotate(f.rot + Math.sin(f.age * 3 + f.wob) * 0.8);
  x.drawImage(s.leaf, -f.size / 2, -f.size / 4, f.size, f.size / 2);
  x.restore();
}

// ---- void: dark motes, ringed in the arc's colour, pulled into it from around it, faster
// and faster and swirling, over a dark rift along it.

function paintVoid(rgb) {
  return {
    mote: glow([[0, "rgba(0,0,0,0.95)"], [0.4, rgba(tint(rgb, BLACK, 0.85), 0.9)], [0.58, rgba(tint(rgb, WHITE, 0.3), 0.95)], [0.75, rgba(rgb, 0.35)], [1, rgba(rgb, 0)]]),
    rift: glow([[0, rgba(tint(rgb, BLACK, 0.85), 0.7)], [0.6, rgba(tint(rgb, BLACK, 0.6), 0.25)], [1, "rgba(0,0,0,0)"]]),
    line: rgba(rgb, 1),
  };
}

function spawnVoid(f, a, share, fs, k) {
  const reach = a.out ? 1 : 0.5;
  // Where it ends, on the arc's edge, and (in vx, vy) where it starts, out from it.
  f.ox = P.x + P.nx * a.width * 0.5;
  f.oy = P.y + P.ny * a.width * 0.5;
  const d = fs * rnd(0.9, 2.0) * reach * Math.sqrt(k);
  const side = rnd(-0.9, 0.9) * fs * reach;
  f.vx = f.ox + P.nx * d + P.tx * side;
  f.vy = f.oy + P.ny * d + P.ty * side;
  f.spin = rnd(-1, 1) * fs * 0.7 * reach;
  f.size = fs * rnd(0.4, 0.65) * (a.out ? 1 : 0.8) + a.width * 0.15;
  f.life = rnd(0.8, 1.4);
}

// Where a mote is `t` of its way in: pulled in faster and faster, swirling.
const Q = { x: 0, y: 0 };
function voidAt(f, t) {
  const e = t ** 2.2;
  const dx = f.ox - f.vx;
  const dy = f.oy - f.vy;
  const len = Math.hypot(dx, dy) || 1;
  const swirl = f.spin * Math.sin(Math.PI * e);
  Q.x = f.vx + dx * e - (dy / len) * swirl;
  Q.y = f.vy + dy * e + (dx / len) * swirl;
}

function voidRift(x, a, s, last, share, now, fs) {
  const r = a.width * 0.9 + fs * 0.3;
  for (let i = 0; i <= last; i += 2) {
    const p = a.pts[i];
    x.globalAlpha = lit((a.out ? 0.65 : 0.5) * (0.55 + 0.45 * Math.sin(i * 0.4 + now * 0.005)));
    const ex = p.x + p.nx * a.width * 0.55;
    const ey = p.y + p.ny * a.width * 0.55;
    x.drawImage(s.rift, ex - r, ey - r, 2 * r, 2 * r);
  }
}

function stepVoid(x, f, a, s, t) {
  const fade = Math.min(1, t * 4);
  voidAt(f, Math.max(0, t - 0.25));
  const tx = Q.x;
  const ty = Q.y;
  voidAt(f, t);
  const r = (f.size * (1 - 0.65 * t * t)) / 2;
  x.globalAlpha = lit(fade * 0.55);
  x.strokeStyle = s.line;
  x.lineCap = "round";
  x.lineWidth = Math.max(0.8, r * 0.5);
  x.beginPath();
  x.moveTo(tx, ty);
  x.lineTo(Q.x, Q.y);
  x.stroke();
  x.globalAlpha = lit(fade);
  x.drawImage(s.mote, Q.x - r, Q.y - r, 2 * r, 2 * r);
}

// ---- wind: streaks rushing up along each arc, just out from it, wavering, bright at their
// heads and fading behind.

function paintWind(rgb) {
  return {
    line: rgba(tint(rgb, WHITE, 0.25), 1),
    air: glow([[0, rgba(tint(rgb, WHITE, 0.3), 0.6)], [1, rgba(rgb, 0)]]),
  };
}

function spawnWind(f, a, share, fs) {
  const reach = a.out ? 1 : 0.5;
  // Where its head is, how fast it goes and how long it is (in shares of the arc), how
  // far out from the arc.
  f.u = Math.random() * share * 0.9;
  f.s = rnd(0.45, 0.85);
  f.size = rnd(0.12, 0.24);
  f.ox = a.width * 0.5 + rnd(0.1, 1) * fs * 0.9 * reach;
  f.wob = Math.random() * 6.3;
  f.life = rnd(0.45, 0.85);
}

function windAir(x, a, s, last, share, now, fs) {
  const r = a.width * 0.9 + fs * 0.3;
  for (let i = 0; i <= last; i += 3) {
    const p = a.pts[i];
    x.globalAlpha = lit((a.out ? 0.08 : 0.06) * (0.7 + 0.3 * Math.sin(now * 0.004 - i * 0.3)));
    const ex = p.x + p.nx * a.width * 0.6;
    const ey = p.y + p.ny * a.width * 0.6;
    x.drawImage(s.air, ex - r, ey - r, 2 * r, 2 * r);
  }
}

// A streak along the arc from `from` of its length back from its head to the head.
function windPath(x, a, f, fs, from) {
  x.beginPath();
  for (let j = 0; j <= WIND_STEPS; j++) {
    const u = f.u - f.size * from * (1 - j / WIND_STEPS);
    at(a, u);
    const off = f.ox + Math.sin(u * 18 + f.wob) * fs * 0.08;
    if (j === 0) x.moveTo(P.x + P.nx * off, P.y + P.ny * off);
    else x.lineTo(P.x + P.nx * off, P.y + P.ny * off);
  }
}

function stepWind(x, f, a, s, t, dt, fs) {
  f.u += f.s * dt;
  // Gone past the fill's end.
  const share = fxShare[a.key] || 0;
  const fade = Math.sin(Math.PI * t) * Math.max(0, Math.min(1, 1 - (f.u - share) / 0.04));
  if (fade < 0.01) return;
  x.strokeStyle = s.line;
  x.lineCap = "round";
  x.lineJoin = "round";
  const lw = Math.max(1, fs * 0.13) * (a.out ? 1 : 0.75);
  // Faint and thin all along, brighter and thicker toward its head.
  windPath(x, a, f, fs, 1);
  x.globalAlpha = lit(fade * 0.3);
  x.lineWidth = lw * 0.6;
  x.stroke();
  windPath(x, a, f, fs, 0.45);
  x.globalAlpha = lit(fade * 0.75);
  x.lineWidth = lw;
  x.stroke();
}

// ---- the effects

// Each: how far it reaches past the outer arcs (the bleed), in text heights (times the
// arcs' width and the effect's size); pieces
// born a second along a full outer arc (an inner one's half that), and how many more at low
// HP; how it's laid over what's under it ("lighter" adds light, glowing); what it draws
// with; and how it starts a piece, draws along the arc and draws a piece.
const FX = {
  flames: { room: 2.4, rate: 450, low: 1.7, blend: "lighter", paint: paintFlames, spawn: spawnFlame, ambient: flameEmbers, step: stepFlame },
  lightning: { room: 1.6, rate: 12, low: 1.8, blend: "lighter", paint: paintLightning, spawn: spawnBolt, ambient: boltBuzz, step: stepBolt },
  holy: { room: 2.4, rate: 14, low: 1, blend: "lighter", paint: paintHoly, spawn: spawnHoly, ambient: holyHalo, step: stepHoly },
  darkness: { room: 2.2, rate: 40, low: 1.5, blend: "source-over", paint: paintDarkness, spawn: spawnDark, ambient: darkHaze, step: stepDark },
  blood: { room: 1.2, rate: 6, low: 2.2, blend: "source-over", paint: paintBlood, spawn: spawnBlood, ambient: bloodBeat, step: stepBlood },
  earth: { room: 1.4, rate: 30, low: 1.5, blend: "source-over", paint: paintEarth, spawn: spawnEarth, ambient: earthCrust, step: stepEarth },
  nature: { room: 1.4, rate: 3, low: 1, blend: "source-over", paint: paintNature, spawn: spawnLeaf, ambient: vines, step: stepLeaf },
  void: { room: 2.0, rate: 45, low: 1.5, blend: "source-over", paint: paintVoid, spawn: spawnVoid, ambient: voidRift, step: stepVoid },
  wind: { room: 1.2, rate: 26, low: 1.5, blend: "lighter", paint: paintWind, spawn: spawnWind, ambient: windAir, step: stepWind },
};

// The arcs' colours as they are now (their own, or as built in), and what the effect draws
// with in each.
function fxColours() {
  fxSprites = {};
  const fx = FX[setup.effect];
  if (!fx) return;
  const css = getComputedStyle($("arcs"));
  const probe = document.createElement("canvas").getContext("2d");
  for (const k of ["hp", "stamina", "mp", "flight"]) {
    probe.fillStyle = "#000";
    probe.fillStyle = css.getPropertyValue(`--arc-${k}`).trim() || "#ffffff";
    const hex = probe.fillStyle;
    const rgb = hex.startsWith("#") ? [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16)) : (hex.match(/\d+/g) || [255, 255, 255]).slice(0, 3).map(Number);
    fxSprites[k] = fx.paint(rgb);
  }
  if (calm.matches) fxStill();
}

function runFx() {
  const on = !!FX[setup.effect];
  if (on && !document.hidden && !calm.matches) {
    if (!fxRaf) {
      fxLast = 0;
      fxRaf = requestAnimationFrame(fxFrame);
    }
    return;
  }
  if (fxRaf) cancelAnimationFrame(fxRaf);
  fxRaf = 0;
  fxLive = 0;
  if (on && calm.matches) fxStill();
  else {
    const c = document.getElementById("fx");
    if (c) fxClear(c, c.getContext("2d"));
  }
}

// Each arc's glow along it, still (reduce motion).
function fxStill() {
  const c = document.getElementById("fx");
  const fx = FX[setup.effect];
  if (!c) return;
  const x = c.getContext("2d");
  fxClear(c, x);
  if (!fx) return;
  fxGain = fxBrightness();
  for (const a of fxArcs) {
    const share = fxShare[a.key];
    const s = fxSprites[a.key];
    if (!share || !s) continue;
    x.globalCompositeOperation = fx.blend;
    fx.ambient(x, a, s, Math.round(share * (a.pts.length - 1)), share, 0, fxFs * fxSize());
  }
  x.globalCompositeOperation = "source-over";
  x.globalAlpha = 1;
}

function fxFrame(now) {
  fxRaf = requestAnimationFrame(fxFrame);
  if (fxLast && now - fxLast < 1000 / FX_FPS - 2) return;
  // The effect's time runs at its speed; its pieces are as big as its size has them.
  const dt = (fxLast ? Math.min(0.1, (now - fxLast) / 1000) : 0) * fxSpeed();
  fxLast = now;
  fxClock += dt * 1000;
  const fx = FX[setup.effect];
  const c = document.getElementById("fx");
  if (!fx || !c) return;
  const fs = fxFs * fxSize();
  const strength = fxStrength();
  fxGain = fxBrightness();
  const k = (setup.width || 100) / 100;
  const low = $("arcs").classList.contains("low");
  // New pieces along each arc's fill: as many to the arc's length whatever its thickness
  // (thicker pieces, fewer of them; more would only add up to white, losing its colour).
  for (const a of fxArcs) {
    const share = fxShare[a.key];
    if (!share || !fxSprites[a.key]) continue;
    let n = (fx.rate * strength * share * (a.out ? 1 : 0.5) * (a.key === "hp" && low ? fx.low : 1) * dt) / Math.max(1, k);
    while (n > 0 && fxLive < FX_MAX) {
      if (n < 1 && Math.random() > n) break;
      n -= 1;
      const f = fxPool[fxLive++];
      f.arc = a;
      f.kind = 0;
      f.age = 0;
      at(a, Math.random() * share);
      fx.spawn(f, a, share, fs, k);
    }
  }
  const x = c.getContext("2d");
  fxClear(c, x);
  for (const a of fxArcs) {
    const share = fxShare[a.key];
    const s = fxSprites[a.key];
    if (!share || !s) continue;
    x.globalCompositeOperation = fx.blend;
    fx.ambient(x, a, s, Math.round(share * (a.pts.length - 1)), share, fxClock, fs);
  }
  x.globalCompositeOperation = fx.blend;
  for (let i = 0; i < fxLive; ) {
    const f = fxPool[i];
    f.age += dt;
    const s = fxSprites[f.arc.key];
    if (f.age >= f.life || !s) {
      // Done: swapped with the last one alive, to be used again.
      fxPool[i] = fxPool[fxLive - 1];
      fxPool[fxLive - 1] = f;
      fxLive--;
      continue;
    }
    fx.step(x, f, f.arc, s, f.age / f.life, dt, fs);
    i++;
  }
  x.globalCompositeOperation = "source-over";
  x.globalAlpha = 1;
}

document.addEventListener("visibilitychange", runFx);
calm.addEventListener("change", runFx);

// ---------------------------------------------------------------- refresh

let lastBars = null;
function draw(b) {
  lastBars = b;
  $("empty").hidden = !!(b && (b.hp != null || b.mp != null));
  drawArcs(b);
  drawPlates();
  // Still effects follow the fills here (moving ones each frame).
  if (calm.matches && FX[setup.effect]) fxStill();
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
    fxColours();
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
