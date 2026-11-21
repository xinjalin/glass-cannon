// One action bar (barid.js says which): what it tracks as set up beside it on the overlay
// (actionbar.rs). Skills with their cooldowns (a sweep over the icon and the time left, or
// a timer bar), their charges, and a glow when ready again; buffs and debuffs with their
// time left and stacks; an effect with no set length as its icon alone. In rows and with
// gaps as laid out, at its icon size and spacing, in a row or a column, in its colours.
// Hidden out of combat if set so, and with nothing set up for your class; while you
// arrange the overlay it shows anyway, with a hint when empty. Names come from the game
// data and are set with textContent, never innerHTML.

const POLL_MS = 100;
// How long a skill glows once it's ready again.
const READY_GLOW_MS = 1200;

// Arranging the overlay (the overlay says): an empty or hidden bar shows a hint to grab.
let editing = false;
window.addEventListener("message", (e) => {
  if (e.data && e.data.type === "gc-editing") {
    editing = !!e.data.on;
    lastKey = "";
    refresh();
  }
});

let lastKey = "";
// Each skill's time left last time, to see it come off cooldown.
const before = new Map();
const glowing = new Map();

// The bar's sounds (its ⚙ → preferences): a skill ready again, an effect coming on you,
// an effect ending. Heard from one look to the next at everything on the bar (shown or
// not), and played by the overlay; one sound a look, the first of those that's set. Not
// on the first look, after a break (the overlay was hidden), when the bar became another
// (your class, what it tracks), or while it's hidden.
let watched = null;
let watchedKey = "";
let watchedAt = 0;
const WATCH_GAP_MS = 1500;
function hear(view, hidden) {
  const now = performance.now();
  const key = `${view.class}:${view.tracks}:${view.all_effects}`;
  const state = new Map((view.watch || []).map((w) => [w.id, w]));
  const before = watched;
  const follows = before && key === watchedKey && now - watchedAt < WATCH_GAP_MS && !hidden;
  watched = state;
  watchedKey = key;
  watchedAt = now;
  if (!follows || !view.sounds || view.sounds.off) return;
  // With every effect on you on the bar, one not listed isn't on you.
  const off = view.all_effects ? { cooling: false, on: false } : null;
  const heard = new Set();
  for (const [id, w] of state) {
    const was = before.get(id) || off;
    if (!was) continue;
    if (was.cooling && !w.cooling && !w.on) heard.add("ready");
    if (!was.on && w.on) heard.add("gained");
  }
  for (const [id, was] of before) {
    const w = state.get(id) || off;
    if (w && was.on && !w.on) heard.add("ended");
  }
  const what = ["ready", "ended", "gained"].find((k) => heard.has(k) && view.sounds[k] && view.sounds[k] !== "none");
  if (what) window.parent.postMessage({ type: "gc-sound", choice: view.sounds[what], chime: what }, "*");
}

// "9", "2.4" (tenths in the last 3 s, if set), "4m".
function timeText(ms, tenths) {
  if (ms >= 60_000) return `${Math.ceil(ms / 60_000)}m`;
  if (tenths && ms < 3000) return (ms / 1000).toFixed(1);
  return String(Math.ceil(ms / 1000));
}

// The countdown's colour (if set): amber in its last 10 s, red in its last 3.
const urgency = (ms) => (ms <= 3000 ? " urgent" : ms <= 10_000 ? " soon" : "");

const isEffect = (s) => s.kind === "buff" || s.kind === "debuff" || s.buff;

function tip(s) {
  const label = s.name || `#${s.id}`;
  if (s.missing) return `${label}: not on you`;
  if (isEffect(s)) return label + (s.endless ? "" : `: ${timeText(s.remaining_ms, false)} s left`) + (s.stacks > 1 ? ` · ${s.stacks} on you` : "");
  return label + (s.remaining_ms > 0 ? `: ready in ${timeText(s.remaining_ms, false)} s` : ": ready") + (s.charges != null ? ` · ${s.charges} charge${s.charges === 1 ? "" : "s"}` : "");
}

// An icon, with the sweep and time over it.
function iconSlot(s, st, glow) {
  const label = s.name || `#${s.id}`;
  const slot = el("div", `bar-slot ${s.kind}`);
  slot.append(iconFor(s.icon_key, label, () => (lastKey = "")));
  const effect = isEffect(s);
  if (s.missing) slot.classList.add("missing");
  if (effect && s.running) slot.classList.add("on");
  const timed = s.running && !s.endless && s.remaining_ms > 0;
  if (timed && !effect) {
    slot.classList.add("cooling");
    if (st.grey) slot.classList.add("grey");
  }
  if (timed && s.duration_ms > 0) {
    // A cooldown: dark over what's left; an effect: over what's gone.
    const left = Math.min(1, s.remaining_ms / s.duration_ms);
    const sweep = el("span", "bar-sweep");
    sweep.style.setProperty("--done", `${Math.round((effect ? left : 1 - left) * 360)}deg`);
    slot.append(sweep);
  }
  if (timed && st.countdown) slot.append(el("span", "bar-time" + (st.colours ? urgency(s.remaining_ms) : ""), timeText(s.remaining_ms, st.tenths)));
  const count = effect ? (s.stacks > 1 ? s.stacks : null) : s.charges;
  if (count != null) slot.append(el("span", "bar-count", String(count)));
  if (glow) slot.classList.add("glow");
  slot.title = tip(s);
  return slot;
}

// A timer bar with its icon, name and time left: a skill full in the ready colour when
// ready, else in the cooldown colour as far as its cooldown has left; an effect in the
// running colour (a debuff: the cooldown colour) as far as it has left, full when endless.
function barSlot(s, st, glow) {
  const label = s.name || `#${s.id}`;
  const effect = isEffect(s);
  const slot = el("div", `bar-line ${s.kind}`);
  if (s.missing) slot.classList.add("missing");
  slot.append(iconFor(s.icon_key, label, () => (lastKey = "")));
  const track = el("div", "bar-track");
  const fill = el("span", "bar-fill");
  const timed = s.running && !s.endless && s.remaining_ms > 0 && s.duration_ms > 0;
  const share = timed ? s.remaining_ms / s.duration_ms : s.missing ? 0 : 1;
  fill.style.width = `${Math.round(Math.min(1, share) * 200) / 2}%`;
  slot.classList.add(effect ? (s.kind === "debuff" ? "debuffed" : "on") : s.running ? "cooling" : "ready");
  const time = s.missing ? "" : timed || (!effect && s.remaining_ms > 0) ? (st.countdown ? timeText(s.remaining_ms, st.tenths) : "") : effect ? "" : "ready";
  const count = effect ? (s.stacks > 1 ? ` ×${s.stacks}` : "") : s.charges != null ? ` ×${s.charges}` : "";
  track.append(fill, el("span", "bar-name", label + count), el("span", "bar-left" + (st.colours && timed ? urgency(s.remaining_ms) : ""), time));
  slot.append(track);
  if (glow) slot.classList.add("glow");
  slot.title = tip(s);
  return slot;
}

function applyStyle(st) {
  const p = $("panel");
  p.style.setProperty("--icon", `${st.icon_size}px`);
  p.style.setProperty("--space", `${st.spacing}px`);
  p.style.setProperty("--bar-w", `${st.bar_width}px`);
  p.style.setProperty("--ab-ready", barColour(st.ready));
  p.style.setProperty("--ab-running", barColour(st.running));
  p.style.setProperty("--ab-cooling", barColour(st.cooling));
  p.style.opacity = String(st.opacity / 100);
  p.dataset.look = st.look;
  p.classList.toggle("vertical", st.vertical);
  p.classList.toggle("round", st.shape === "round");
  p.classList.toggle("no-track", !st.background);
}

async function refresh() {
  let view;
  try {
    view = await invoke("action_bar", { bar: BAR });
  } catch {
    return;
  }
  const now = performance.now();
  const hidden = !view.exists || (view.hide_idle && !view.in_combat);
  hear(view, hidden);
  // Skills that just came off cooldown glow a moment.
  for (const s of view.slots) {
    if (s.kind !== "skill" || s.buff) continue;
    const was = before.get(s.id);
    if (was > 0 && s.remaining_ms === 0) glowing.set(s.id, now + READY_GLOW_MS);
    before.set(s.id, s.remaining_ms);
  }
  const glow = view.style.glow ? [...glowing].filter(([, until]) => until > now).map(([k]) => k) : [];
  const key = JSON.stringify([view, glow, editing]);
  if (key === lastKey) return;
  lastKey = key;
  applyStyle(view.style);
  const p = $("panel");
  const content = view.slots.some((s) => s.kind !== "gap" && s.kind !== "row");
  if ((hidden || !content) && !editing) {
    p.replaceChildren();
    return;
  }
  if (!content) {
    const hint = view.class == null ? "use a skill in game first" : view.exists ? "⚙ to choose what it shows" : "not set up for this class · ⚙";
    p.replaceChildren(el("div", "bar-hint", `action bar ${BAR} · ${hint}`));
    return;
  }
  // A row each up to a row mark (a column each, for a vertical bar), gaps where put.
  const rows = [[]];
  for (const s of view.slots) {
    if (s.kind === "row") rows.push([]);
    else rows[rows.length - 1].push(s);
  }
  const bars = view.style.look === "bars";
  morph(
    p,
    rows
      .filter((r) => r.length)
      .map((r) => {
        const row = el("div", "bar-row");
        row.append(
          ...r.map((s) => {
            if (s.kind === "gap") return el("div", "bar-gap");
            const g = glow.includes(s.id) && s.kind === "skill";
            return bars ? barSlot(s, view.style, g) : iconSlot(s, view.style, g);
          }),
        );
        return row;
      }),
  );
  if (editing && hidden) p.append(el("div", "bar-hint", view.exists ? "hidden out of combat" : "not set up for this class"));
}

onTick(POLL_MS, () => {
  if (!document.hidden) refresh();
});
refresh();
