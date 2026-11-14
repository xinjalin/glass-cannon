// Your skills that aren't ready, in one of two looks (settings → overlay → cooldowns):
// - list: soonest first, a row each with the time left and charges;
// - timeline: a bar from Ready to 100%, each skill's icon on it as far from Ready as the
//   share of its cooldown left, sliding to Ready as it runs down, with the time left on it;
//   a skill that comes ready flashes at the Ready end. Ready on the left or the right.
//   Icons that overlap take turns going see-through, so those under them show.
// Skills with a cooldown under 2 s (dodging, basic attacks) are left out.

const POLL_MS = 200;
const SHORTEST_MS = 2000;
// How long a skill that came ready flashes at the Ready end.
const FLASH_MS = 900;

let look = { look: "list", ready_right: false, size: 100, scale: "percent" };

function seconds(ms) {
  return ms < 10_000 ? (ms / 1000).toFixed(1) : clock(ms / 1000);
}

// A short time for on an icon: "9.5", "30", "4:49".
function short(ms) {
  if (ms < 10_000) return (ms / 1000).toFixed(1);
  if (ms < 60_000) return String(Math.round(ms / 1000));
  return clock(ms / 1000);
}

function drawList(rows) {
  const list = $("list");
  list.replaceChildren();
  for (const c of rows) {
    const label = c.name || `#${c.skill}`;
    const li = el("li", "cd" + (c.remaining_ms < 3000 ? " soon" : ""));
    const main = el("div", "cd-main");
    const name = el("span", "cd-name", label);
    if (c.charges != null) name.append(el("span", "cd-charges", ` ×${c.charges}`));
    const bar = el("div", "cd-bar");
    const fill = el("div", "cd-fill");
    fill.style.width = `${Math.min(100, (c.remaining_ms / Math.max(1, c.duration_ms)) * 100)}%`;
    bar.append(fill);
    main.append(name, bar);
    li.append(iconFor(c.icon_key, label, refresh), main, el("span", "cd-time", seconds(c.remaining_ms)));
    li.title = `${label}: ${(c.duration_ms / 1000).toFixed(1)} s cooldown` + (c.charges != null ? ` · ${c.charges} charge${c.charges === 1 ? "" : "s"} left` : "");
    list.append(li);
  }
}

// ---------------------------------------------------------------- timeline

// The icons on the timeline, by skill (kept between redraws so they glide), and the
// skills that came ready lately, flashing at the Ready end until then.
const marks = new Map();
const flashes = new Map();
let shownBefore = new Map();
// Icons that overlap take turns going see-through, each this long a turn and then solid
// again, so those under them show; clear of the others, they're solid.
const OVERLAP_CYCLE_S = 2;

// A skill's icon, or its letter until the icon has loaded.
function iconOf(c) {
  const label = c.name || `#${c.skill}`;
  return iconFor(c.icon_key, label, refresh);
}

function mark(c) {
  let m = marks.get(c.skill);
  if (m) {
    // Its icon hadn't loaded yet when it was made: put it in now that it has.
    if (m.blank) {
      const icon = iconOf(c);
      if (icon.tagName === "IMG") {
        m.icon.replaceWith(icon);
        m.icon = icon;
        m.blank = false;
      }
    }
    return m;
  }
  const label = c.name || `#${c.skill}`;
  const node = el("div", "tl-mark");
  const time = el("span", "tl-time");
  const icon = iconOf(c);
  node.append(icon, time);
  node.title = label;
  m = { node, time, icon, blank: icon.tagName !== "IMG", phase: null };
  marks.set(c.skill, m);
  $("tl-track").append(node);
  return m;
}

// Where an icon goes, from Ready (0) to the far end (1). By percent: the share of its
// cooldown left. By time: its time left, on a curve that gives the last seconds room and
// squeezes the minutes (1 s at an eighth of the way, 30 s under half, 5 min at the end);
// longer waits at the end.
const TIME_MAX_S = 300;
const TIME_CURVE = 0.35;
const TIME_MARKS = [
  [1, "1"],
  [10, "10"],
  [30, "30"],
  [60, "1m"],
  [120, "2m"],
  [300, "5m"],
];
const timeShare = (s) => Math.pow(Math.max(0, Math.min(s, TIME_MAX_S)) / TIME_MAX_S, TIME_CURVE);
const shareOf = (c) =>
  look.scale === "time" ? timeShare(c.remaining_ms / 1000) : Math.max(0, Math.min(1, c.remaining_ms / Math.max(1, c.duration_ms)));

// The scale's marks: 50% and 100%, or the times.
function drawScale() {
  const time = look.scale === "time";
  for (const s of document.querySelectorAll(".tl-half, .tl-full")) s.hidden = time;
  $("tl-ticks").replaceChildren(
    ...(time ? TIME_MARKS : []).map(([s, label]) => {
      const t = el("span", "tl-tick");
      t.style.setProperty("--at", timeShare(s).toFixed(4));
      t.append(el("span", "tl-tick-text", label));
      return t;
    }),
  );
}

function timeText(c) {
  return short(c.remaining_ms) + (c.charges != null ? ` ×${c.charges}` : "");
}

// Each icon in its place, the soonest on top. Icons that overlap make a run (each over the
// next): they take turns going see-through, spread evenly over the cycle.
function drawSpread(rows) {
  const track = $("tl-track");
  const fs = parseFloat(getComputedStyle(track).fontSize) || 13;
  const icon = fs * 1.9;
  const usable = Math.max(1, track.clientWidth - icon - 2 * fs * 0.3);
  const byPlace = [...rows].sort((a, b) => shareOf(a) - shareOf(b));
  const runs = [];
  for (const c of byPlace) {
    const run = runs[runs.length - 1];
    const x = shareOf(c) * usable;
    if (run && x - run.last < icon) {
      run.members.push(c);
      run.last = x;
    } else runs.push({ last: x, members: [c] });
  }
  const phaseOf = new Map();
  for (const run of runs) {
    run.members.forEach((c, i) => phaseOf.set(c.skill, run.members.length > 1 ? i / run.members.length : null));
  }
  const order = [...rows].sort((a, b) => b.remaining_ms - a.remaining_ms);
  order.forEach((c, i) => {
    const m = mark(c);
    m.node.style.setProperty("--at", shareOf(c).toFixed(4));
    m.node.style.zIndex = String(10 + i);
    m.node.classList.remove("flash");
    m.node.classList.toggle("soon", c.remaining_ms < 3000);
    m.time.textContent = timeText(c);
    // Its turn in the run it overlaps, changed only when the run does (so it doesn't jump).
    const phase = phaseOf.get(c.skill);
    if (phase !== m.phase) {
      m.phase = phase;
      m.node.classList.toggle("overlap", phase != null);
      m.node.style.animationDelay = phase != null ? `${(-phase * OVERLAP_CYCLE_S).toFixed(2)}s` : "";
    }
  });
}

function drawTimeline(rows) {
  const track = $("tl-track");
  const now = performance.now();
  $("timeline").classList.toggle("ready-right", look.ready_right);
  const here = new Set(rows.map((c) => c.skill));
  // Gone since the last look: it came ready.
  for (const [skill, c] of shownBefore) if (!here.has(skill)) flashes.set(skill, { c, until: now + FLASH_MS });
  shownBefore = new Map(rows.map((c) => [c.skill, c]));
  drawSpread(rows);
  // Flashing at Ready, then gone.
  for (const [skill, f] of flashes) {
    if (here.has(skill)) {
      flashes.delete(skill);
      continue;
    }
    const m = mark(f.c);
    if (now >= f.until) {
      m.node.remove();
      marks.delete(skill);
      flashes.delete(skill);
      continue;
    }
    m.node.style.setProperty("--at", "0");
    m.node.style.zIndex = "100";
    m.node.classList.remove("overlap");
    m.node.style.animationDelay = "";
    m.phase = null;
    m.node.classList.add("flash");
    m.time.textContent = "";
  }
  for (const [skill, m] of marks) {
    if (!here.has(skill) && !flashes.has(skill)) {
      m.node.remove();
      marks.delete(skill);
    }
  }
  track.classList.toggle("idle", rows.length === 0 && flashes.size === 0);
}

// ---------------------------------------------------------------- refresh

async function refresh() {
  let rows = [];
  try {
    rows = (await invoke("cooldowns")).filter((c) => c.duration_ms >= SHORTEST_MS);
  } catch {
    return;
  }
  const timeline = look.look === "timeline";
  $("timeline").hidden = !timeline;
  $("list-view").hidden = timeline;
  if (timeline) {
    drawTimeline(rows);
  } else {
    drawList(rows);
    $("empty").hidden = rows.length > 0;
  }
}

function setLook(l) {
  if (!l) return;
  look = l;
  // The timeline's size: its icons, times and labels all go by its text size.
  $("timeline").style.fontSize = `${(look.size || 100) / 100}em`;
  drawScale();
  if (look.look !== "timeline") {
    for (const m of marks.values()) m.node.remove();
    marks.clear();
    flashes.clear();
    shownBefore = new Map();
  }
  refresh();
}

invoke("cooldowns_look").then(setLook).catch(() => {});
tauri.event.listen("cooldowns-look", (e) => setLook(e.payload));
setInterval(() => {
  if (!document.hidden) refresh();
}, POLL_MS);
refresh();
