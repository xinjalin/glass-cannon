// The vitals arcs widget: arcs either side of your character, as GT Companion's center HUD
// has them, with no box. On the left your HP (outer) with your stamina inside it, on the
// right your MP with your flight power inside it, each filling from the bottom; your cast
// under your feet. Drawn to the widget's size (arrange it around your character), in the
// look chosen in the settings (overlay → vitals arcs): solid, thin or segmented arcs, how
// round and thick they are, whether their tracks always show, the numbers (value, percent or both; their size,
// what's behind them, at the arcs' top or bottom ends), and the cast as an arc or a straight
// bar with its time at each end. Max HP is the game's; until it sends it, the highest HP
// seen stands in. Stamina and flight power read full until the game sends them (it sends
// them only as they change).

// Smooth enough for a cast.
const POLL_MS = 100;
// Below this share of HP, the HP arc glows.
const LOW_HP = 0.3;
// Stamina and flight power are sent in hundredths of what the game shows.
const HUNDREDTHS = 100;

// ---------------------------------------------------------------- the look

// As the settings have it (vitals.rs); these until they're read.
let setup = { style: "solid", roundness: 50, text_size: 100, text_backing: "outline", text_at: "top", numbers: "value", cast: "arc", width: 100, always_show: false, show_cast: true };

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
function build() {
  const svg = $("arcs");
  plates = svgEl("g");
  const lines = svgEl("g");
  const texts = svgEl("g");
  svg.replaceChildren(plates, lines, texts);
  for (const k of ARCS) {
    const track = svgEl("path", `arc-track ${k}`);
    const fill = svgEl("path", `arc-fill ${k}`);
    fill.setAttribute("pathLength", "100");
    track.setAttribute("pathLength", "100");
    const value = svgEl("text", `arc-text ${k}`);
    const pct = svgEl("text", `arc-text arc-pct ${k}`);
    lines.append(track, fill);
    texts.append(value, pct);
    parts[k] = { track, fill, value, pct };
  }
  parts.castEnd = svgEl("text", "arc-text cast cast-end");
  texts.append(parts.castEnd);
}

function applySetup() {
  const svg = $("arcs");
  svg.setAttribute("class", `arcs look-${setup.style} backing-${setup.text_backing}${svg.classList.contains("low") ? " low" : ""}`);
  svg.style.setProperty("--vt", String(setup.text_size / 100));
  layoutArcs();
  draw(lastBars);
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
  const thin = setup.style === "thin";
  // The arcs' thickness, as the look has it times the width chosen.
  const k = (setup.width || 100) / 100;
  const outer = (thin ? Math.max(2, fs * 0.2) : Math.max(5, fs * 0.6)) * k;
  const inner = (thin ? Math.max(2, fs * 0.16) : Math.max(3, fs * 0.38)) * k;
  const gap = thin ? Math.max(4, fs * 0.5) : inner * 0.7;
  // Room past the glow.
  const pad = outer / 2 + (thin ? 4 : 1);
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
  const arcs = {
    hp: [sideArc(g, true, 0), g.outer],
    stamina: [sideArc(g, true, step), g.inner],
    mp: [sideArc(g, false, 0), g.outer],
    flight: [sideArc(g, false, step), g.inner],
  };
  const cast = castShape(g);
  arcs.cast = [cast, cast.bar ? Math.max(2, g.inner * 0.6) : g.inner];
  for (const [k, [shape, width]] of Object.entries(arcs)) {
    for (const p of [parts[k].track, parts[k].fill]) {
      p.setAttribute("d", shape.d);
      p.style.strokeWidth = `${width}px`;
    }
    parts[k].track.style.strokeDasharray = setup.style === "segmented" && k !== "cast" ? `${CELL - CELL_GAP} ${CELL_GAP}` : "";
  }

  // The numbers, the outer arc's over the inner's, beside the chosen ends and inward of
  // both arcs' ends: the arcs bend outward from their ends, so nothing of them is further
  // in, and the numbers never cross them. With both, the percents go past the ends (over
  // the top ones, under the bottom ones), either side of the gap between the two arcs,
  // and the values a line further along the arcs, clear of them.
  const ts = g.ts;
  const both = setup.numbers === "both";
  for (const [outerKey, innerKey, left] of [
    ["hp", "stamina", true],
    ["mp", "flight", false],
  ]) {
    const o = arcs[outerKey][0];
    const i = arcs[innerKey][0];
    const inward = left ? 1 : -1;
    const anchor = left ? "start" : "end";
    const away = left ? "end" : "start";
    const end = setup.text_at === "bottom" ? "bottom" : "top";
    // Between the two arcs' ends, and a little either side of it.
    const mid = (o[end][0] + i[end][0]) / 2;
    const apart = ts * 0.15;
    const x = i[end][0] + inward * (g.inner / 2 + ts * 0.35);
    if (end === "bottom") {
      // Upward from the outer arc's bottom end, as the top's mirrored: the outer arc's
      // value lowest, by its end (the lower one), the inner arc's over it, by its own.
      const y = o.bottom[1] + ts * 0.75 - (both ? ts : 0);
      place(parts[outerKey].value, [x, y], anchor);
      place(parts[innerKey].value, [x, y - ts], anchor);
      const py = Math.max(o.bottom[1], i.bottom[1]) + g.outer / 2 + ts * 0.9;
      place(parts[outerKey].pct, [mid - inward * apart, py], away);
      place(parts[innerKey].pct, [mid + inward * apart, py], anchor);
    } else {
      // Downward from the outer arc's top end.
      const y = o.top[1] - ts * 0.15 + (both ? ts : 0);
      place(parts[outerKey].value, [x, y], anchor);
      place(parts[innerKey].value, [x, y + ts], anchor);
      const py = Math.min(o.top[1], i.top[1]) - g.outer / 2 - ts * 0.45;
      place(parts[outerKey].pct, [mid - inward * apart, py], away);
      place(parts[innerKey].pct, [mid + inward * apart, py], anchor);
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

// ---------------------------------------------------------------- drawing

// A segmented arc's fill: whole cells up to the share, then the part of the next.
function cells(pct) {
  const dashes = [];
  let at = 0;
  while (at + CELL <= pct + 0.001) {
    dashes.push(CELL - CELL_GAP, CELL_GAP);
    at += CELL;
  }
  const part = Math.min(pct - at, CELL - CELL_GAP);
  if (part > 0.05) dashes.push(part.toFixed(2), CELL - part);
  dashes.push(0, 100);
  return dashes.join(" ");
}

function arc(k, share, value, percent) {
  const p = parts[k];
  const known = share != null;
  // Always shown: the empty track stays up with nothing to fill it.
  p.track.style.display = known || setup.always_show ? "" : "none";
  for (const e of [p.fill, p.value, p.pct]) e.style.display = known ? "" : "none";
  if (!known) return;
  const pct = Math.max(0, Math.min(1, share)) * 100;
  p.fill.style.strokeDasharray = setup.style === "segmented" && k !== "cast" ? cells(pct) : `${pct.toFixed(2)} 100`;
  p.value.textContent = setup.numbers === "percent" ? percent : value;
  p.pct.textContent = setup.numbers === "both" ? percent : "";
}

// "value/max", in the game's units.
const pair = (v, max, scale = 1) => `${Math.round(v / scale).toLocaleString("en-US")}/${Math.round(max / scale).toLocaleString("en-US")}`;

function drawArcs(b) {
  const share = (v, max) => (v != null && max ? v / max : null);
  const side = (cls, v, max, scale) => {
    const s = share(v, max);
    arc(cls, s, s != null ? pair(v, max, scale) : "", s != null ? `${Math.round(Math.min(1, s) * 100)}` : "");
  };
  const hp = share(b && b.hp, b && b.hp_max);
  side("hp", b && b.hp, b && b.hp_max, 1);
  side("stamina", b && b.stamina, b && b.stamina_max, HUNDREDTHS);
  side("mp", b && b.mp, b && b.mp_max, 1);
  side("flight", b && b.flight, b && b.flight_max, HUNDREDTHS);
  $("arcs").classList.toggle("low", hp != null && hp < LOW_HP);
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
    for (const e of [parts.cast.track, parts.cast.fill, parts.cast.value, parts.cast.pct, parts.castEnd]) e.style.display = "none";
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
tauri.event.listen("appearance", () => requestAnimationFrame(layoutArcs));
tauri.event.listen("vitals-setup", (e) => {
  setup = e.payload;
  applySetup();
});
invoke("vitals_setup")
  .then((s) => {
    setup = s;
    applySetup();
  })
  .catch(() => applySetup());
applySetup();
setInterval(() => {
  if (!document.hidden) refresh();
}, POLL_MS);
refresh();
