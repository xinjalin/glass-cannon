// The player bars: your HP and MP, and the cast you're making, as thin bars, or (settings →
// player bars style) as arcs around your character. Max HP isn't sent: the highest HP
// seen stands in (your max whenever you've been at full HP).

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

// The arcs follow an ellipse filling the widget: HP up its left side, MP up its right
// (both filling from the bottom), the cast along the bottom (filling left to right).
// Angles in degrees, counter-clockwise from 3 o'clock.
const SIDE_FROM = 225;
const SIDE_TO = 135;
const CAST_FROM = 245;
const CAST_TO = 295;

function geometry() {
  const svg = $("arcs");
  const w = svg.clientWidth;
  const h = svg.clientHeight;
  const fs = parseFloat(getComputedStyle(document.documentElement).fontSize) || 13;
  const stroke = Math.max(4, fs * 0.45);
  const pad = stroke + fs * 0.2;
  return { w, h, fs, stroke, cx: w / 2, cy: h / 2, rx: Math.max(1, w / 2 - pad), ry: Math.max(1, h / 2 - pad) };
}

function point(g, deg) {
  const a = (deg * Math.PI) / 180;
  return [g.cx + g.rx * Math.cos(a), g.cy - g.ry * Math.sin(a)];
}

// An arc of the ellipse from one angle to another (`clockwise` on screen).
function arcPath(g, from, to, clockwise) {
  const [x0, y0] = point(g, from);
  const [x1, y1] = point(g, to);
  return `M ${x0.toFixed(1)} ${y0.toFixed(1)} A ${g.rx.toFixed(1)} ${g.ry.toFixed(1)} 0 0 ${clockwise ? 1 : 0} ${x1.toFixed(1)} ${y1.toFixed(1)}`;
}

// Lays the arcs and their labels out for the widget's size.
function layoutArcs() {
  const svg = $("arcs");
  const g = geometry();
  if (!g.w || !g.h) return;
  svg.setAttribute("viewBox", `0 0 ${g.w} ${g.h}`);
  const paths = {
    // Bottom-left up to top-left (clockwise on screen); bottom-right up to top-right.
    hp: arcPath(g, SIDE_FROM, SIDE_TO, true),
    mp: arcPath(g, 180 - SIDE_FROM, 180 - SIDE_TO, false),
    cast: arcPath(g, CAST_FROM, CAST_TO, false),
  };
  for (const [k, d] of Object.entries(paths)) {
    for (const p of svg.querySelectorAll(`.${k}:is(.arc-track, .arc-fill)`)) {
      p.setAttribute("d", d);
      p.style.strokeWidth = `${k === "cast" ? g.stroke * 0.8 : g.stroke}px`;
    }
  }
  // The numbers by the arcs' lower ends, inside; the cast's name over its arc.
  const [hx, hy] = point(g, SIDE_FROM);
  const [mx, my] = point(g, 180 - SIDE_FROM);
  const label = (cls, x, y, anchor) => {
    const t = svg.querySelector(`.arc-text.${cls}`);
    t.setAttribute("x", x.toFixed(1));
    t.setAttribute("y", y.toFixed(1));
    t.setAttribute("text-anchor", anchor);
  };
  label("hp", hx + g.stroke, hy + g.fs * 0.35, "start");
  label("mp", mx - g.stroke, my + g.fs * 0.35, "end");
  label("cast", g.cx, point(g, 270)[1] - g.stroke - g.fs * 0.3, "middle");
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

function drawArcs(b) {
  const share = (v, max) => (v != null && max ? v / max : null);
  const hp = share(b && b.hp, b && b.hp_max);
  arc("hp", hp, b && b.hp != null ? fmt(b.hp) : "");
  arc("mp", share(b && b.mp, b && b.mp_max), b && b.mp != null ? fmt(b.mp) : "");
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
