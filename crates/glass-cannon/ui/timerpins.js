// Timer pins (widgets.rs → timer_pins): the events and field bosses you pinned, each
// counting down to its start; while on, its time left; a field boss up, "up". With the
// world boss train, every field boss up or spawning within its time is added. Their
// rings: green while on or up, the accent when soon, pulsing when its alert is due (the
// bell in the world timers). As a grid of icons or a slim row each, sorted by time or as
// pinned. Field bosses come from the world map (open it in game to read them).

// A field boss "up" as of a world map read longer ago than this may have been killed.
const STALE_MS = 3 * 3_600_000;

let view = { pins: [], look: { layout: "icons", icon_size: 40, columns: 6, names: true, sort: "time", train: false, train_min: 30 } };
let settings = { region: "as", alerts: [], alert_lead_min: 5 };
let bosses = [];
let editing = false;
window.addEventListener("message", (e) => {
  if (e.data && e.data.type === "gc-editing") {
    editing = !!e.data.on;
    draw();
  }
});

// The time on a pin: minutes and seconds, or hours and minutes past an hour.
function pinTime(ms) {
  const s = Math.max(0, Math.round(ms / 1000));
  if (s >= 3600) return `${Math.floor(s / 3600)}h ${String(Math.floor((s % 3600) / 60)).padStart(2, "0")}`;
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
}

// What each pin is now: { id, name, short, meta, at (for sorting), state, time, train }.
function items(now) {
  const { r, events } = regionEvents(settings.region);
  const lead = settings.alert_lead_min * 60_000;
  const out = [];
  const add = (id, train) => {
    if (id.startsWith("boss:")) {
      const b = bosses.find((x) => `boss:${x.id}` === id);
      if (!b) return;
      const up = b.time_ms <= now;
      const stale = up && now - (b.seen_ms || b.time_ms) > STALE_MS;
      const left = b.time_ms - now;
      out.push({
        id,
        name: b.name || `Field boss ${b.id}`,
        short: b.name || `Boss ${b.id}`,
        meta: b.place || b.map_name || "",
        at: up ? now - 1 : b.time_ms,
        state: up ? (stale ? "stale" : "up") : left <= SOON_MS ? "soon" : "later",
        time: up ? (stale ? "up?" : "Up") : pinTime(left),
        alert: !up && settings.alerts.includes(id) && left <= lead,
        train,
      });
      return;
    }
    const e = events[id];
    if (!e) return;
    const n = next(e, e.timeZone || r.timeZone, now);
    if (!n) return;
    const on = n.start <= now;
    const left = on ? n.start + n.minutes * 60_000 - now : n.start - now;
    out.push({
      id,
      name: EVENTS[id] || id,
      short: TIMER_SHORT[id] || EVENTS[id] || id,
      meta: on ? "on now" : dayAndTime(n.start),
      at: on ? now - 1 : n.start,
      state: on ? "on" : left <= SOON_MS ? "soon" : "later",
      time: pinTime(left),
      alert: !on && settings.alerts.includes(id) && left <= lead,
      train: false,
    });
  };
  for (const id of view.pins) add(id, false);
  // The world boss train: every field boss up (and not stale), or spawning soon.
  if (view.look.train) {
    for (const b of bosses) {
      const id = `boss:${b.id}`;
      if (view.pins.includes(id)) continue;
      const up = b.time_ms <= now && now - (b.seen_ms || b.time_ms) <= STALE_MS;
      if (up || (b.time_ms > now && b.time_ms - now <= view.look.train_min * 60_000)) add(id, true);
    }
  }
  if (view.look.sort === "time") out.sort((a, b) => a.at - b.at);
  return out;
}

function pinIcon(it) {
  const ring = el("div", "pin-ring");
  ring.append(timerIcon(it.id));
  return ring;
}

function draw() {
  const p = $("panel");
  const look = view.look;
  p.style.setProperty("--pin", `${look.icon_size}px`);
  p.style.setProperty("--cols", String(look.columns));
  p.dataset.layout = look.layout;
  const now = Date.now();
  const list = items(now);
  if (!list.length) {
    p.replaceChildren(...(editing ? [el("div", "bar-hint", "timer pins · ⚙ to pin events and field bosses")] : []));
    return;
  }
  p.replaceChildren(
    ...list.map((it) => {
      const cls = `pin ${it.state}${it.alert ? " alert" : ""}${it.train ? " train" : ""}`;
      const box = el("div", look.layout === "list" ? `${cls} pin-row` : cls);
      box.title = `${it.name}${it.meta ? ` · ${it.meta}` : ""}${it.train ? " · world boss train" : ""}`;
      if (look.layout === "list") {
        const text = el("div", "pin-text");
        text.append(el("span", "pin-name", it.name), el("span", "pin-meta", it.meta));
        box.append(pinIcon(it), text, el("span", "pin-time", it.time));
      } else {
        box.append(pinIcon(it), el("div", "pin-time", it.time));
        if (look.names) box.append(el("div", "pin-name", it.short));
      }
      return box;
    }),
  );
}

async function loadAll() {
  try {
    const [v, s] = await Promise.all([invoke("timer_pins"), invoke("timer_settings")]);
    if (v) view = v;
    if (s) settings = s;
  } catch {
    // Next time, then.
  }
  draw();
}
async function loadBosses() {
  bosses = await invoke("field_bosses").catch(() => bosses);
  draw();
}

tauri.event.listen("timer-pins", (e) => {
  if (e.payload) view = e.payload;
  draw();
});
tauri.event.listen("timer-settings", () => loadAll());
loadAll();
loadBosses();
setInterval(() => {
  if (!document.hidden) draw();
}, 1000);
// The world map is read when opened in game; and alerts and region can change.
setInterval(() => {
  if (!document.hidden) {
    loadBosses();
    loadAll();
  }
}, 30_000);
