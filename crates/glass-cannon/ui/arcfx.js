// The arcs' effects (flames, lightning, frost and the rest), shared by the vitals arcs widget
// and the cast bar widget: loaded before the page's own script, which has `setup` (the
// effect and its strength, brightness, size and speed, and the arcs' width), `parts` (each
// arc's fill, a path), an svg #arcs with each arc's colour (--arc-<key>), and a box of class
// "fx-host" for the canvas; it calls layoutFx as it lays its arcs out, sets fxShare as it
// draws them, and fxColours and runFx as its look changes. An arc's effect follows its fill
// unless its part has an `fxPath` (another path to follow: the cast bar's rim) and an
// `fxSide` ("up" or "down": which way is out from a level line).

// Notches across the cast at each of a charge's levels but the last (the bar's end), those
// up to `filled` (0-100) only when given; each `wide` hundredths of the cast wide.
const MARK = 1.4;
function levelMarks(levels, filled = 100, wide = MARK) {
  let dashes = "0";
  let at = 0;
  for (let i = 1; i < levels; i += 1) {
    const mark = (100 * i) / levels;
    if (mark + wide / 2 > filled) break;
    dashes += ` ${(mark - wide / 2 - at).toFixed(2)} ${wide.toFixed(2)}`;
    at = mark + wide / 2;
  }
  return `${dashes} 200`;
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
// The arcs that can have an effect, by key (their colours are --arc-<key> on #arcs): the
// vitals arcs' four; the cast bar widget's is its bar.
let fxKeys = ["hp", "stamina", "mp", "flight"];
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
// Whether your HP is low now (the HP arc's pulse quickens).
let fxLow = false;
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
    document.querySelector(".fx-host").prepend(c);
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
    const path = parts[k].fxPath || parts[k].fill;
    const side = parts[k].fxSide;
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
      if (side ? ny > 0 !== (side === "down") : away !== out) [nx, ny] = [-nx, -ny];
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
    // Under the arc (where it turns under; a level bar's bottom rim), it hangs: its point
    // up at the arc. Elsewhere its point back the way it runs.
    const under = P.ny > 0.3;
    dx = under ? P.nx : -P.tx;
    dy = under ? P.ny : -P.ty;
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

// How many of an arc's points apart things `px` apart along it are (at least one): so
// what's set along an arc is as close together whatever the arc's length.
const every = (a, px) => Math.max(1, Math.round((px / a.len) * (a.pts.length - 1)));

// ---- frost: each arc rimed with shards of ice along its fill, glinting, in a cold mist,
// with snowflakes drifting down from it and glints of light on it.

// A snowflake's arms, drawn `w` wide in `colour`.
function flakeArms(x, colour, w) {
  x.strokeStyle = colour;
  x.lineWidth = w;
  x.lineCap = "round";
  for (let arm = 0; arm < 6; arm++) {
    const t = (arm * Math.PI) / 3;
    const [c, n] = [Math.cos(t), Math.sin(t)];
    x.beginPath();
    x.moveTo(24, 24);
    x.lineTo(24 + c * 20, 24 + n * 20);
    // Two barbs on each arm.
    for (const side of [1, -1]) {
      const [bc, bn] = [Math.cos(t + side * 0.9), Math.sin(t + side * 0.9)];
      x.moveTo(24 + c * 11, 24 + n * 11);
      x.lineTo(24 + c * 11 + bc * 7, 24 + n * 11 + bn * 7);
    }
    x.stroke();
  }
}

function paintFrost(rgb) {
  const ice = tint(rgb, WHITE, 0.55);
  const pale = tint(rgb, WHITE, 0.85);
  const deep = tint(rgb, BLACK, 0.35);
  return {
    mist: glow([[0, rgba(pale, 0.5)], [0.5, rgba(ice, 0.22)], [1, rgba(ice, 0)]]),
    // A shard of ice from its base (left) to its point: pale, its lower face in the arc's
    // colour.
    shard: sprite(64, 20, (x) => {
      const whole = () => {
        x.beginPath();
        x.moveTo(0, 10);
        x.lineTo(14, 2);
        x.lineTo(63, 10);
        x.lineTo(14, 18);
        x.closePath();
      };
      whole();
      const g = x.createLinearGradient(0, 0, 64, 0);
      g.addColorStop(0, rgba(ice, 0.95));
      g.addColorStop(0.6, rgba(pale, 0.95));
      g.addColorStop(1, rgba(WHITE, 1));
      x.fillStyle = g;
      x.fill();
      x.beginPath();
      x.moveTo(0, 10);
      x.lineTo(63, 10);
      x.lineTo(14, 18);
      x.closePath();
      x.fillStyle = rgba(rgb, 0.45);
      x.fill();
      whole();
      x.strokeStyle = rgba(deep, 0.6);
      x.lineWidth = 1;
      x.stroke();
    }),
    // A snowflake: pale, with a darker edge so it shows over anything.
    flake: sprite(48, 48, (x) => {
      flakeArms(x, rgba(deep, 0.45), 4.5);
      flakeArms(x, rgba(pale, 1), 2.2);
    }),
    // A glint: a cross of light.
    glint: sprite(32, 32, (x) => {
      const g = x.createRadialGradient(16, 16, 0, 16, 16, 7);
      g.addColorStop(0, rgba(WHITE, 1));
      g.addColorStop(1, rgba(pale, 0));
      x.fillStyle = g;
      x.fillRect(0, 0, 32, 32);
      for (const across of [false, true]) {
        const l = across ? x.createLinearGradient(0, 0, 32, 0) : x.createLinearGradient(0, 0, 0, 32);
        l.addColorStop(0, rgba(pale, 0));
        l.addColorStop(0.5, rgba(WHITE, 1));
        l.addColorStop(1, rgba(pale, 0));
        x.fillStyle = l;
        if (across) x.fillRect(0, 15, 32, 2);
        else x.fillRect(15, 0, 2, 32);
      }
    }),
  };
}

function spawnFrost(f, a, share, fs) {
  const reach = a.out ? 1 : 0.5;
  const out = a.width * 0.5 + rnd(0, fs * 1.2 * reach);
  f.x = P.x + P.nx * out;
  f.y = P.y + P.ny * out;
  f.rot = Math.random() * 6.3;
  if (Math.random() < 0.3) {
    f.kind = 2;
    f.x = P.x + P.nx * a.width * rnd(-0.3, 0.6);
    f.y = P.y + P.ny * a.width * rnd(-0.3, 0.6);
    f.size = fs * rnd(0.6, 1.1);
    f.life = rnd(0.4, 0.9);
    return;
  }
  f.kind = 1;
  f.vx = P.nx * fs * rnd(0.1, 0.5) * reach;
  f.vy = fs * rnd(0.5, 1.1);
  f.size = fs * rnd(0.7, 1.2) * (a.out ? 1 : 0.8);
  f.spin = rnd(-1.5, 1.5);
  f.life = rnd(1.6, 2.8);
  f.wob = Math.random() * 6.3;
}

function frostRime(x, a, s, last, share, now, fs) {
  const reach = a.out ? 1 : 0.6;
  const r = a.width * 1.0 + fs * 0.4;
  x.globalAlpha = lit(a.out ? 0.2 : 0.12);
  for (let i = 0; i <= last; i += 3) {
    const p = a.pts[i];
    x.drawImage(s.mist, p.x + p.nx * a.width * 0.5 - r, p.y + p.ny * a.width * 0.5 - r, 2 * r, 2 * r);
  }
  // Shards out from the arc, each its own length and lean (the same every time), growing
  // in at the fill's end and glinting by turns.
  const long = (fs * 0.85 + a.width * 0.6) * reach;
  const step = every(a, fs * 0.5);
  for (let i = 1; i <= last; i += step) {
    const p = a.pts[i];
    const len = long * (0.35 + hash(i, 3) * 0.65) * Math.min(1, (last - i + 1) / (2 * step));
    if (len < 2) continue;
    const lean = (hash(i, 5) - 0.5) * 1.4;
    x.globalAlpha = lit(0.7 + 0.3 * Math.sin(now * 0.004 + i * 1.7));
    x.save();
    x.translate(p.x + p.nx * a.width * 0.4, p.y + p.ny * a.width * 0.4);
    x.rotate(Math.atan2(p.ny + p.ty * lean, p.nx + p.tx * lean));
    x.drawImage(s.shard, 0, -len * 0.16, len, len * 0.32);
    x.restore();
  }
}

function stepFrost(x, f, a, s, t, dt, fs) {
  if (f.kind === 2) {
    x.globalAlpha = lit(Math.sin(Math.PI * t));
    const r = (f.size * (0.6 + 0.4 * Math.sin(Math.PI * t))) / 2;
    x.drawImage(s.glint, f.x - r, f.y - r, 2 * r, 2 * r);
    return;
  }
  // Drifting down, swaying, turning slowly.
  f.x += (f.vx + Math.sin(f.age * 2 + f.wob) * fs * 0.5 * (a.out ? 1 : 0.5)) * dt;
  f.y += f.vy * dt;
  f.rot += f.spin * dt;
  x.globalAlpha = lit(Math.min(1, (1 - t) * 3, t * 5) * 0.95);
  x.save();
  x.translate(f.x, f.y);
  x.rotate(f.rot);
  x.drawImage(s.flake, -f.size / 2, -f.size / 2, f.size, f.size);
  x.restore();
}

// ---- feathers: each arc plumed like a wing along its fill, its feathers swept back and
// stirring, with one coming loose now and then and floating down, swinging side to side
// (more of them at low HP).

function paintFeathers(rgb) {
  const pale = tint(rgb, WHITE, 0.8);
  return {
    // A feather from its quill (left) to its tip: white at the quill, the arc's colour at
    // its tip, its shaft down the middle, barbs slanting off it, a split or two in its vane.
    feather: sprite(96, 32, (x) => {
      const vane = () => {
        x.beginPath();
        x.moveTo(10, 16);
        x.bezierCurveTo(28, -2, 70, 0, 94, 14);
        x.bezierCurveTo(72, 30, 30, 34, 10, 16);
        x.closePath();
      };
      vane();
      const g = x.createLinearGradient(0, 0, 96, 0);
      g.addColorStop(0, rgba(WHITE, 1));
      g.addColorStop(0.5, rgba(pale, 1));
      g.addColorStop(1, rgba(rgb, 1));
      x.fillStyle = g;
      x.fill();
      x.save();
      vane();
      x.clip();
      x.strokeStyle = rgba(tint(rgb, BLACK, 0.25), 0.3);
      x.lineWidth = 0.8;
      for (let i = 14; i < 92; i += 5) {
        x.beginPath();
        x.moveTo(i, 15.5);
        x.lineTo(i + 9, 0);
        x.moveTo(i, 15.5);
        x.lineTo(i + 9, 32);
        x.stroke();
      }
      x.restore();
      // Splits in the vane.
      x.globalCompositeOperation = "destination-out";
      for (const [at, up] of [[58, true], [40, false], [74, false]]) {
        x.beginPath();
        x.moveTo(at, 15.5);
        x.lineTo(at + 9, up ? -1 : 33);
        x.lineTo(at + 12, up ? -1 : 33);
        x.closePath();
        x.fill();
      }
      x.globalCompositeOperation = "source-over";
      vane();
      x.strokeStyle = rgba(tint(rgb, BLACK, 0.45), 0.45);
      x.lineWidth = 1;
      x.stroke();
      x.beginPath();
      x.moveTo(0, 17);
      x.quadraticCurveTo(50, 14, 92, 14);
      x.strokeStyle = rgba(tint(rgb, BLACK, 0.3), 0.85);
      x.lineWidth = 1.4;
      x.stroke();
    }),
  };
}

function spawnFeather(f, a, share, fs) {
  const reach = a.out ? 1 : 0.5;
  f.x = P.x + P.nx * a.width * 0.5;
  f.y = P.y + P.ny * a.width * 0.5;
  f.vx = P.nx * fs * rnd(0.6, 1.4) * reach;
  f.vy = -fs * rnd(0.1, 0.6);
  f.rot = Math.atan2(P.ny - P.ty * 0.9, P.nx - P.tx * 0.9);
  f.spin = rnd(-0.5, 0.5);
  f.size = (fs * rnd(1.3, 1.7) + a.width * 0.5) * (a.out ? 1 : 0.75);
  f.life = rnd(2.6, 4.0);
  f.wob = Math.random() * 6.3;
}

function plumage(x, a, s, last, share, now, fs) {
  const reach = a.out ? 1 : 0.6;
  const long = (fs * 1.6 + a.width * 0.8) * reach;
  if (last < 2) return;
  x.globalAlpha = lit(0.95);
  // Close enough to overlap; from the top down, so each feather lies over the one above
  // it, as a wing's do.
  const step = every(a, long * 0.4);
  for (let i = last - ((last - 2) % step); i >= 2; i -= step) {
    const p = a.pts[i];
    const len = long * (0.75 + hash(i, 7) * 0.4) * Math.min(1, (last - i + 1) / step);
    if (len < 3) continue;
    x.save();
    x.translate(p.x + p.nx * a.width * 0.35, p.y + p.ny * a.width * 0.35);
    // Out from the arc and swept back down it, stirring a little.
    x.rotate(Math.atan2(p.ny - p.ty * 0.9, p.nx - p.tx * 0.9) + Math.sin(now * 0.0017 + i * 0.9) * 0.08);
    x.drawImage(s.feather, 0, -len * 0.17, len, len * 0.34);
    x.restore();
  }
}

function stepFeather(x, f, a, s, t, dt, fs) {
  // Floating down slowly, swinging wide from side to side and tilting as it swings.
  const swing = Math.sin(f.age * 2.1 + f.wob);
  f.vy = Math.min(fs * 0.8, f.vy + fs * 0.7 * dt);
  f.vx *= 1 - dt * 0.9;
  f.x += (f.vx + swing * fs * 1.1 * (a.out ? 1 : 0.5)) * dt;
  f.y += f.vy * (0.6 + 0.4 * Math.abs(Math.cos(f.age * 2.1 + f.wob))) * dt;
  f.rot += f.spin * dt;
  x.globalAlpha = lit(Math.min(1, (1 - t) * 2.5, t * 6));
  x.save();
  x.translate(f.x, f.y);
  x.rotate(f.rot + swing * 0.6);
  x.drawImage(s.feather, -f.size / 2, -f.size * 0.17, f.size, f.size * 0.34);
  x.restore();
}

// ---- pulse: no pieces, only light: each arc's glow breathing along its fill, and a wave of
// light running up it (as many waves as the strength has: one at 100%). The HP arc's
// quickens at low HP.

function paintPulse(rgb) {
  return { glow: glow([[0, rgba(tint(rgb, WHITE, 0.6), 0.9)], [0.4, rgba(rgb, 0.45)], [1, rgba(rgb, 0)]]) };
}

function pulseGlow(x, a, s, last, share, now, fs) {
  const quick = a.key === "hp" && fxLow ? 2.2 : 1;
  const breath = 0.5 + 0.5 * Math.sin(now * 0.0028 * quick + (a.out ? 0 : 0.9));
  const r = a.width * 1.2 + fs * 0.55;
  const waves = Math.round(fxStrength());
  const n = a.pts.length - 1;
  for (let i = 0; i <= last; i += 2) {
    const p = a.pts[i];
    // How much of a wave is here: each runs up the whole arc, off its top, and comes round.
    let wave = 0;
    for (let w = 0; w < waves; w++) {
      const d = (i / n - (((now * 0.00035 * quick + w / waves) % 1.25) - 0.125)) / 0.07;
      wave = Math.max(wave, Math.exp(-d * d));
    }
    x.globalAlpha = lit((a.out ? 0.16 : 0.1) * (0.35 + 0.65 * breath) + wave * (a.out ? 0.5 : 0.35));
    const k = r * (1 + wave * 0.5);
    x.drawImage(s.glow, p.x - k, p.y - k, 2 * k, 2 * k);
  }
}

// Pulse has no pieces to start or draw.
function noPiece() {}

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
  frost: { room: 1.8, rate: 18, low: 1, blend: "source-over", paint: paintFrost, spawn: spawnFrost, ambient: frostRime, step: stepFrost },
  feathers: { room: 2.4, rate: 2.2, low: 2, blend: "source-over", paint: paintFeathers, spawn: spawnFeather, ambient: plumage, step: stepFeather },
  pulse: { room: 1.6, rate: 0, low: 1, blend: "lighter", paint: paintPulse, spawn: noPiece, ambient: pulseGlow, step: noPiece },
};

// The arcs' colours as they are now (their own, or as built in), and what the effect draws
// with in each.
function fxColours() {
  fxSprites = {};
  const fx = FX[setup.effect];
  if (!fx) return;
  const css = getComputedStyle($("arcs"));
  const probe = document.createElement("canvas").getContext("2d");
  for (const k of fxKeys) {
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
  fxLow = low;
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
