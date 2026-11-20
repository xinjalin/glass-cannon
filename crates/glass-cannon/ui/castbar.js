// The cast bar widget: the cast you're making on a bar of its own, with no box. A charged
// skill you're holding (the game sends no other cast of yours) fills the bar to its top
// level, with a notch at each level, and stays full ("max") until you let go. Beside it the
// skill's icon; on, over or under it the skill's name and the time. Drawn to the widget's
// size (as wide as its box, in the middle of its height), in the look chosen in its
// settings (overlay → cast bar): solid, thin, segmented, gradient or framed, how thick, the
// text's size and place, a colour of its own, and an effect on it: the vitals arcs' own
// (whatever that's set to) or one picked here. The effect runs along the bar's top rim, as
// big as on a vitals arc however thick the bar (blood along its bottom rim, dripping off),
// drawn over the bar and under its texts; between casts too, along the whole empty bar,
// if that's chosen. While the overlay's being arranged, a
// sample cast runs on it, so its look shows as it's changed.

// How often the cast is read; the bar glides between readings.
const POLL_MS = 100;
// The segmented look's cells, in hundredths of the bar: about this long (a whole number of
// them to each of a charge's levels, so a level ends where a cell does), this far apart.
const CELL = 5;
const CELL_GAP = 1.2;
const cellFor = (c) => {
  const levels = c && c.charge && c.charge.levels > 1 ? c.charge.levels : 1;
  return 100 / (levels * Math.max(1, Math.round(100 / CELL / levels)));
};
// The sample cast (while arranging): to its top level, held, then a gap, in ms.
const SAMPLE = { full: 1500, held: 2300, every: 3300 };
const SVG = "http://www.w3.org/2000/svg";

// As the settings have it (castbar.rs); these until they're read.
let cb = { style: "solid", effect: "match", effect_strength: 100, effect_brightness: 100, effect_size: 100, effect_speed: 100, thickness: 100, text_size: 100, text_at: "inside", time: "left", ends: "round", icon: true, name: true, marks: true, always_show: false, effect_always: false, colour: null };
// The vitals arcs' settings: their effect is the bar's while its own is "match".
let vitals = null;
// What the effects go by (arcfx.js): the effect, its settings and the bar's thickness
// (in percent of a vitals arc's).
let setup = { effect: "none", effect_strength: 100, effect_brightness: 100, effect_size: 100, effect_speed: 100, width: 100 };
fxKeys = ["cast"];

const parts = {};
let editing = false;
// The cast as last read, and when (the bar glides on from there).
let cast = null;
let castAt = 0;
// The bar as laid out: its ends, thickness and where the texts go.
let box = null;

function svgEl(tag, cls) {
  const e = document.createElementNS(SVG, tag);
  if (cls) e.setAttribute("class", cls);
  return e;
}

function build() {
  const svg = $("arcs");
  const defs = svgEl("defs");
  const grad = svgEl("linearGradient", "arc-grad cast");
  grad.id = "grad-cast";
  grad.setAttribute("gradientUnits", "userSpaceOnUse");
  grad.append(svgEl("stop", "g0"), svgEl("stop", "g1"));
  grad.lastChild.setAttribute("offset", "1");
  defs.append(grad);
  const frame = svgEl("path", "arc-frame cast");
  const track = svgEl("path", "arc-track cast");
  const marks = svgEl("path", "arc-cast-marks");
  const fill = svgEl("path", "arc-fill cast");
  const marksOn = svgEl("path", "arc-cast-marks on");
  for (const p of [frame, track, marks, fill, marksOn]) p.setAttribute("pathLength", "100");
  // The line the effect follows (the bar's rim), never drawn.
  const fxPath = svgEl("path", "cb-fx-line");
  const name = svgEl("text", "cb-text cb-name");
  const time = svgEl("text", "cb-text cb-time");
  svg.append(defs, frame, track, marks, fill, marksOn, fxPath);
  // The texts over the effect's canvas, which is over the bar.
  $("cb-texts").append(name, time);
  parts.cast = { frame, track, marks, fill, marksOn, grad, name, time, fxPath, fxSide: "up" };
}

// The theme's accent, as the cast's built-in colour.
function themeColour() {
  const css = getComputedStyle(document.documentElement);
  return css.getPropertyValue("--accent-fill").trim() || css.getPropertyValue("--accent").trim() || "#f2a8a4";
}

// The effect now: the vitals arcs' with its settings, or the bar's own.
function effectNow() {
  const from = cb.effect === "match" ? vitals || {} : cb;
  const effect = FX[from.effect] ? from.effect : "none";
  return { effect, effect_strength: from.effect_strength || 100, effect_brightness: from.effect_brightness || 100, effect_size: from.effect_size || 100, effect_speed: from.effect_speed || 100 };
}

// Round ends, unless the bar's cut into cells (round ones would run into each other).
const roundEnds = () => cb.ends === "round" && cb.style !== "segmented";

function applySetup() {
  Object.assign(setup, effectNow());
  const svg = $("arcs");
  svg.setAttribute("class", `arcs castbar look-${cb.style} ends-${roundEnds() ? "round" : "square"} effect-${setup.effect}${FX[setup.effect] ? " fx" : ""}`);
  $("castbar").style.setProperty("--vt", String(cb.text_size / 100));
  // Its colour set outright (yours, or the theme's), so the effect is in it too.
  svg.style.setProperty("--arc-cast", isHexColor(cb.colour) ? cb.colour : themeColour());
  layout();
  draw();
  fxColours();
  runFx();
}

// Lays the bar, its icon and its texts out for the widget's size and the look.
function layout() {
  const svg = $("arcs");
  const host = $("castbar");
  const [w, h] = [host.clientWidth, host.clientHeight];
  if (!w || !h) return;
  for (const s of [svg, $("cb-texts")]) s.setAttribute("viewBox", `0 0 ${w} ${h}`);
  const fs = parseFloat(getComputedStyle(document.documentElement).fontSize) || 13;
  const ts = (fs * cb.text_size) / 100;
  const k = cb.thickness / 100;
  const thin = cb.style === "thin";
  const texts = cb.name || cb.time !== "none";
  // The texts on the bar where it's thick enough for them (never on the thin look's line);
  // otherwise over it.
  const wanted = thin ? Math.max(2, fs * 0.22) * k : (cb.text_at === "inside" ? fs * 1.6 : fs * 0.75) * k;
  const inside = texts && cb.text_at === "inside" && !thin && wanted >= ts * 1.1;
  const row = texts && !inside ? ts * 1.35 : 0;
  const t = Math.max(2, Math.min(wanted, h - row));
  const below = !inside && cb.text_at === "below";
  const top = Math.max(0, (h - t - row) / 2);
  const barTop = below ? top : top + row;
  const textY = inside ? barTop + t / 2 : below ? barTop + t + row / 2 : top + row / 2;
  // The icon: a square as tall as the bar and its texts together.
  const size = cb.icon ? Math.min(h, Math.max(t + row, fs * 1.6)) : 0;
  const gap = cb.icon ? fs * 0.4 : 0;
  const icon = $("cb-icon");
  Object.assign(icon.style, { left: "0px", top: `${Math.max(0, top + (t + row - size) / 2)}px`, width: `${size}px`, height: `${size}px` });
  const round = roundEnds();
  const x0 = size + gap;
  // Round ends reach half the bar's thickness past the line's.
  const cap = round ? t / 2 : 0;
  const [left, right] = [x0 + cap, Math.max(x0 + cap + 1, w - cap)];
  const y = barTop + t / 2;
  const p = parts.cast;
  const d = `M ${left.toFixed(1)} ${y.toFixed(1)} L ${right.toFixed(1)} ${y.toFixed(1)}`;
  for (const e of [p.frame, p.track, p.marks, p.fill, p.marksOn]) e.setAttribute("d", d);
  // Framed: a light frame, the track just inside it, the fill inset in that.
  const framed = cb.style === "framed";
  const trackW = framed ? Math.max(2, t - 3) : t;
  const fillW = framed ? Math.max(1.5, trackW - Math.max(2, trackW * 0.3)) : t;
  p.frame.style.strokeWidth = `${t}px`;
  p.frame.style.display = framed ? "" : "none";
  p.track.style.strokeWidth = `${trackW}px`;
  for (const e of [p.fill, p.marks, p.marksOn]) e.style.strokeWidth = `${fillW}px`;
  for (const [x, v] of [["x1", left], ["x2", right], ["y1", y], ["y2", y]]) p.grad.setAttribute(x, v.toFixed(1));
  // The name from the bar's start, the time at its end.
  const inset = inside ? Math.max(fs * 0.45, t * 0.3) : 0;
  for (const [e, x, anchor] of [[p.name, x0 + inset, "start"], [p.time, w - inset, "end"]]) {
    e.setAttribute("x", x.toFixed(1));
    e.setAttribute("y", textY.toFixed(1));
    e.setAttribute("text-anchor", anchor);
  }
  box = { w, h, fs, t, left, right, y, x0, inset, textW: w - x0 - 2 * inset };
  // The effect along the bar's rim, as if the rim were a vitals arc (as thick as one at
  // most: an effect sizes itself by its arc's thickness), going up from the top rim; blood
  // along the bottom one, beading under the bar and dripping off.
  const rim = Math.min(t, Math.max(3, fs * 0.6));
  p.fxSide = setup.effect === "blood" ? "down" : "up";
  const fy = p.fxSide === "down" ? barTop + t - rim / 2 : barTop + rim / 2;
  p.fxPath.setAttribute("d", `M ${left.toFixed(1)} ${fy.toFixed(1)} L ${right.toFixed(1)} ${fy.toFixed(1)}`);
  setup.width = 100;
  layoutFx({ cast: [{ d }, rim] }, { cast: "outer" }, { w, h, fs, cx: w / 2, cy: h / 2 });
  named = null;
}

// The sample cast, by the clock: charging, held at its top level, then nothing for a bit.
function sample(now) {
  const at = now % SAMPLE.every;
  if (at > SAMPLE.held) return null;
  return { skill: 0, skill_name: "Sample skill", icon_key: null, cast_ms: SAMPLE.full, remaining_ms: Math.max(0, SAMPLE.full - at), charge: { levels: 3, elapsed_ms: at }, sample: true };
}

// The cast to show now, moved on from when it was read: how far along it is (0 to 1).
function castNow(now) {
  if (!cast) return editing ? sample(now) : null;
  const since = Math.min(400, now - castAt);
  const elapsed = (cast.charge ? cast.charge.elapsed_ms : cast.cast_ms - cast.remaining_ms) + since;
  return { ...cast, remaining_ms: Math.max(0, cast.cast_ms - elapsed) };
}

// The name as it fits beside the time: cut short with an ellipsis.
let named = null;
function setName(text, room) {
  const e = parts.cast.name;
  const key = `${text}|${Math.round(room)}`;
  if (named === key) return;
  named = key;
  e.textContent = text;
  let cut = text;
  while (cut.length > 1 && e.getComputedTextLength() > room) {
    cut = cut.slice(0, -1);
    e.textContent = `${cut.trimEnd()}…`;
  }
}

let iconFor_ = null;
function setIcon(c) {
  const box_ = $("cb-icon");
  const show = cb.icon && !!c;
  box_.hidden = !show;
  if (!show) {
    iconFor_ = null;
    return;
  }
  const key = c.sample ? "sample" : `${c.icon_key}|${c.skill_name}`;
  if (iconFor_ === key) return;
  iconFor_ = key;
  box_.replaceChildren(
    iconFor(c.icon_key, c.skill_name || "", () => {
      iconFor_ = null;
      draw();
    }),
  );
}

function draw() {
  if (!box) return;
  const p = parts.cast;
  const c = castNow(performance.now());
  const shown = !!c || cb.always_show || editing;
  $("castbar").classList.toggle("idle", !c);
  p.track.style.display = shown ? "" : "none";
  if (cb.style === "framed") p.frame.style.display = shown ? "" : "none";
  const share = c ? Math.max(0, Math.min(1, 1 - c.remaining_ms / Math.max(1, c.cast_ms))) : 0;
  const pct = share * 100;
  // The effect along the fill; between casts along the whole bar, where that's chosen (and
  // the empty bar shows).
  fxShare.cast = c ? share : cb.always_show && cb.effect_always ? 1 : null;
  // (A round end with nothing behind it would show as a dot.)
  p.fill.style.display = c && pct > 0.4 ? "" : "none";
  const cell = cellFor(c);
  p.track.style.strokeDasharray = cb.style === "segmented" ? `${(cell - CELL_GAP).toFixed(3)} ${CELL_GAP}` : "";
  p.fill.style.strokeDasharray = cb.style === "segmented" ? cells(pct, cell) : `${pct.toFixed(2)} 100`;
  if (cb.style === "gradient") p.grad.setAttribute("x2", (box.left + (box.right - box.left) * Math.max(0.02, share)).toFixed(1));
  // A charge's levels: light on the track, dark on the fill where it has passed them.
  // (Cut into cells, a level ends at a gap between two: the mark is the gap, drawn wider.)
  const marked = !!c && cb.marks && c.charge && c.charge.levels > 1;
  for (const m of [p.marks, p.marksOn]) m.style.display = marked ? "" : "none";
  if (marked) {
    // As wide in pixels whatever the bar's length.
    const wide = Math.max(0.3, (100 * Math.max(2, box.fs * 0.2)) / Math.max(1, box.right - box.left));
    p.marks.style.strokeDasharray = levelMarks(c.charge.levels, 100, wide);
    p.marksOn.style.strokeDasharray = levelMarks(c.charge.levels, pct, wide);
  }
  // The time first: the name gets the room it leaves.
  const full = c && c.charge && c.charge.levels > 1 && c.remaining_ms <= 0;
  const left = c ? (full ? "max" : (c.remaining_ms / 1000).toFixed(1)) : "";
  p.time.textContent = !c || cb.time === "none" ? "" : cb.time === "both" && !full ? `${left} / ${(c.cast_ms / 1000).toFixed(1)}` : left;
  p.name.style.display = c && cb.name ? "" : "none";
  if (c && cb.name) setName(c.skill_name || (c.charge ? "charging" : "casting"), box.textW - (p.time.textContent ? p.time.getComputedTextLength() + box.fs * 0.6 : 0));
  setIcon(c);
  // Still effects follow the fill here (moving ones each frame).
  if (calm.matches && FX[setup.effect]) fxStill();
}

// The segmented look's fill: cells `cell` long up to `pct`, the last one as far as it goes.
function cells(pct, cell) {
  const n = Math.floor((pct + 0.01) / cell);
  const part = Math.max(0, pct - n * cell);
  let dashes = "";
  for (let i = 0; i < n; i += 1) dashes += `${(cell - CELL_GAP).toFixed(3)} ${CELL_GAP} `;
  return `${dashes}${Math.min(part, cell - CELL_GAP).toFixed(2)} 200`;
}

// Redrawn each frame while there's a cast (or its sample) to move.
// (At most sixty times a second: on a faster screen every other frame, which the overlay,
// one page with all the widgets in it, then needn't be drawn again for.)
let frame = 0;
let drawnAt = 0;
function tick(now) {
  frame = 0;
  if (now - drawnAt >= 15) {
    drawnAt = now;
    draw();
  }
  if ((cast || editing) && !document.hidden) frame = requestAnimationFrame(tick);
}
function run() {
  if (!frame && (cast || editing) && !document.hidden) frame = requestAnimationFrame(tick);
}

async function refresh() {
  let b = null;
  try {
    b = await invoke("self_bars");
  } catch {
    return;
  }
  const had = !!cast;
  cast = (b && b.cast) || null;
  castAt = performance.now();
  // Drawn with each reading too: the frames between only smooth it.
  if (cast || had) draw();
  run();
}

function take(v) {
  if (!v) return;
  cb = { ...cb, ...v };
}

window.addEventListener("message", (e) => {
  if (e.data && e.data.type === "gc-editing") {
    editing = !!e.data.on;
    draw();
    run();
  }
});
document.addEventListener("visibilitychange", run);

build();
new ResizeObserver(() => {
  layout();
  draw();
}).observe(document.documentElement);
tauri.event.listen("appearance", () => requestAnimationFrame(applySetup));
tauri.event.listen("cast-bar-setup", (e) => {
  take(e.payload);
  applySetup();
});
tauri.event.listen("vitals-setup", (e) => {
  if (e.payload) vitals = e.payload;
  applySetup();
});
Promise.all([invoke("cast_bar_setup").catch(() => null), invoke("vitals_setup").catch(() => null)]).then(([c, v]) => {
  take(c);
  if (v) vitals = v;
  applySetup();
});
applySetup();
onTick(POLL_MS, () => {
  if (!document.hidden) refresh();
});
refresh();
