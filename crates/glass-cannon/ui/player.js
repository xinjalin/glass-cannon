// The player bars: your HP and MP, and the cast you're making, as thin bars, or (settings →
// player bars style) as round arcs around your character with your stamina and flight
// power too. Max HP is the game's; until it sends it, the highest HP seen stands in.

// Smooth enough for a cast bar.
const POLL_MS = 100;
// Below this share of HP, the HP arc glows.
const LOW_HP = 0.3;

let style = "bars";
let last = null;

function bar(cls, value, max, text) {
  const el_ = document.querySelector(`.pb.${cls}`);
  const known = value != null && max;
  el_.hidden = !known;
  if (!known) return;
  el_.querySelector(".pb-fill").style.width = `${Math.max(0, Math.min(1, value / max)) * 100}%`;
  el_.querySelector(".pb-text").textContent = text;
}

// ---------------------------------------------------------------- arcs

// Two round arcs either side of a circle around your character, each filling from the
// bottom: on the left HP (outer) with stamina inside it, on the right MP with flight power
// inside it; the cast along a smaller arc under your feet. Angles in degrees,
// counter-clockwise from 3 o'clock; each side arc spans HALF either side of 9 or 3 o'clock.
const HALF = 28;
const CAST_HALF = 32;
// The cast's circle, as a share of the side arcs' radius.
const CAST_R = 0.42;
// Stamina and flight power are sent in hundredths of what the game shows.
const HUNDREDTHS = 100;

function geometry() {
  const svg = $("arcs");
  const w = svg.clientWidth;
  const h = svg.clientHeight;
  const fs = parseFloat(getComputedStyle(document.documentElement).fontSize) || 13;
  const outer = Math.max(5, fs * 0.6);
  const inner = Math.max(3, fs * 0.38);
  const gap = inner * 0.7;
  const pad = outer / 2 + 1;
  // As big as the box allows: as wide as it is, and the arcs and their labels within its
  // height.
  const sin = Math.sin((HALF * Math.PI) / 180);
  const r = Math.max(10, Math.min(w / 2 - pad, (h / 2 - pad - fs * 1.6) / sin));
  return { w, h, fs, outer, inner, cx: w / 2, cy: h / 2, r, ri: r - outer / 2 - gap - inner / 2 };
}

function point(g, r, deg) {
  const a = (deg * Math.PI) / 180;
  return [g.cx + r * Math.cos(a), g.cy - r * Math.sin(a)];
}

// An arc of the circle of radius `r` from one angle to another (`clockwise` on screen).
function arcPath(g, r, from, to, clockwise) {
  const [x0, y0] = point(g, r, from);
  const [x1, y1] = point(g, r, to);
  return `M ${x0.toFixed(1)} ${y0.toFixed(1)} A ${r.toFixed(1)} ${r.toFixed(1)} 0 0 ${clockwise ? 1 : 0} ${x1.toFixed(1)} ${y1.toFixed(1)}`;
}

// Lays the arcs and their labels out for the widget's size.
function layoutArcs() {
  const svg = $("arcs");
  const g = geometry();
  if (!g.w || !g.h) return;
  svg.setAttribute("viewBox", `0 0 ${g.w} ${g.h}`);
  // Left: bottom up to top (clockwise on screen); right: bottom up to top the other way.
  const left = (r) => arcPath(g, r, 180 + HALF, 180 - HALF, true);
  const right = (r) => arcPath(g, r, -HALF, HALF, false);
  const arcs = {
    hp: [left(g.r), g.outer],
    stamina: [left(g.ri), g.inner],
    mp: [right(g.r), g.outer],
    flight: [right(g.ri), g.inner],
    cast: [arcPath(g, g.r * CAST_R, 270 - CAST_HALF, 270 + CAST_HALF, false), g.inner],
  };
  for (const [k, [d, width]] of Object.entries(arcs)) {
    for (const p of svg.querySelectorAll(`.${k}:is(.arc-track, .arc-fill)`)) {
      p.setAttribute("d", d);
      p.style.strokeWidth = `${width}px`;
    }
  }
  // The values by the arcs' top ends, inward: the outer one's, then the inner one's under
  // it; the cast's name and time under its arc.
  const label = (cls, x, y, anchor) => {
    const t = svg.querySelector(`.arc-text.${cls}`);
    t.setAttribute("x", x.toFixed(1));
    t.setAttribute("y", y.toFixed(1));
    t.setAttribute("text-anchor", anchor);
  };
  const [lx, ly] = point(g, g.r, 180 - HALF);
  const [rx] = point(g, g.r, HALF);
  label("hp", lx + g.outer * 0.6, ly - g.fs * 0.15, "start");
  label("stamina", lx + g.outer * 0.6, ly + g.fs * 0.85, "start");
  label("mp", rx - g.outer * 0.6, ly - g.fs * 0.15, "end");
  label("flight", rx - g.outer * 0.6, ly + g.fs * 0.85, "end");
  label("cast", g.cx, point(g, g.r * CAST_R, 270)[1] + g.inner + g.fs, "middle");
}

function arc(cls, share, text) {
  const svg = $("arcs");
  const known = share != null;
  for (const p of svg.querySelectorAll(`.${cls}`)) p.style.display = known ? "" : "none";
  if (!known) return;
  const pct = Math.max(0, Math.min(1, share)) * 100;
  svg.querySelector(`.arc-fill.${cls}`).style.strokeDasharray = `${pct.toFixed(2)} 100`;
  svg.querySelector(`.arc-text.${cls}`).textContent = text;
}

// "value/max", in the game's units.
const pair = (v, max, scale = 1) => `${Math.round(v / scale).toLocaleString("en-US")}/${Math.round(max / scale).toLocaleString("en-US")}`;

function drawArcs(b) {
  const share = (v, max) => (v != null && max ? v / max : null);
  const side = (cls, v, max, scale) => arc(cls, share(v, max), v != null && max ? pair(v, max, scale) : "");
  const hp = share(b && b.hp, b && b.hp_max);
  side("hp", b && b.hp, b && b.hp_max, 1);
  side("stamina", b && b.stamina, b && b.stamina_max, HUNDREDTHS);
  side("mp", b && b.mp, b && b.mp_max, 1);
  side("flight", b && b.flight, b && b.flight_max, HUNDREDTHS);
  $("arcs").classList.toggle("low", hp != null && hp < LOW_HP);
  const c = b && b.cast;
  arc("cast", c ? 1 - c.remaining_ms / Math.max(1, c.cast_ms) : null, c ? `${c.skill_name || "casting"} ${(c.remaining_ms / 1000).toFixed(1)}` : "");
}

// ---------------------------------------------------------------- both

function applyStyle(s) {
  style = s === "arcs" ? "arcs" : "bars";
  const arcs = style === "arcs";
  $("panel").classList.toggle("arcs-on", arcs);
  // (An SVG element has no .hidden: the attribute.)
  $("arcs").toggleAttribute("hidden", !arcs);
  for (const p of document.querySelectorAll(".pb")) p.style.display = arcs ? "none" : "";
  if (arcs) layoutArcs();
  draw(last);
}

function draw(b) {
  $("empty").hidden = !!(b && (b.hp != null || b.mp != null));
  if (style === "arcs") {
    drawArcs(b);
    return;
  }
  if (!b) {
    for (const c of ["hp", "mp", "cast"]) document.querySelector(`.pb.${c}`).hidden = true;
    return;
  }
  bar("hp", b.hp, b.hp_max, b.hp != null ? `${fmt(b.hp)} / ${fmt(b.hp_max || b.hp)}` : "");
  bar("mp", b.mp, b.mp_max, b.mp != null ? `${fmt(b.mp)} / ${fmt(b.mp_max || b.mp)}` : "");
  const c = b.cast;
  const cast = document.querySelector(".pb.cast");
  cast.hidden = !c;
  if (c) {
    // Fills as the cast goes on.
    cast.querySelector(".pb-fill").style.width = `${(1 - c.remaining_ms / Math.max(1, c.cast_ms)) * 100}%`;
    cast.querySelector(".pb-text").textContent = c.skill_name || "casting";
    cast.querySelector(".pb-time").textContent = (c.remaining_ms / 1000).toFixed(1);
  }
}

async function refresh() {
  try {
    last = await invoke("self_bars");
  } catch {
    return;
  }
  draw(last);
}

new ResizeObserver(() => {
  if (style === "arcs") layoutArcs();
}).observe(document.documentElement);
invoke("player_bars_style").then(applyStyle).catch(() => {});
tauri.event.listen("player-bars-style", (e) => applyStyle(e.payload));
tauri.event.listen("appearance", () => {
  if (style === "arcs") requestAnimationFrame(layoutArcs);
});

setInterval(() => {
  if (!document.hidden) refresh();
}, POLL_MS);
refresh();
