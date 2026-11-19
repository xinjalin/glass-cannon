// The timer pins' settings (widgets.rs → timer_pins), beside them while the overlay's
// arranged: pin events and field bosses, the world boss train, and the look. Each change
// shows on the pins at once.

let view = null;
let region = "as";
let alerts = [];
let lead = 5;
let sound = true;
let bosses = [];
let tab = "pins";

function press(b, on) {
  b.setAttribute("aria-pressed", String(!!on));
}
function toggle(b, on) {
  press(b, on);
  b.textContent = on ? "[on]" : "[off]";
}

async function setLook(patch, soon) {
  view.look = { ...view.look, ...patch };
  draw();
  clearTimeout(setLook.timer);
  const go = async () => {
    setLook.timer = null;
    // What comes back is taken only if nothing's changed here since it was sent.
    const sent = view.look;
    setLook.saving = (setLook.saving || 0) + 1;
    const saved = await invoke("set_pins_look", { look: sent }).catch(() => sent);
    setLook.saving--;
    if (view.look === sent) view.look = saved;
  };
  if (soon) setLook.timer = setTimeout(go, 150);
  else await go();
}

async function pin(id, on) {
  view.pins = on ? [...view.pins.filter((p) => p !== id), id] : view.pins.filter((p) => p !== id);
  draw();
  await invoke("set_timer_pinned", { id, on }).catch(() => {});
}

async function alert(id, on) {
  alerts = on ? [...alerts.filter((a) => a !== id), id] : alerts.filter((a) => a !== id);
  draw();
  await invoke("set_alert", { id, on }).catch(() => {});
}

// One event or field boss to pin: its icon, name, and whether it's pinned; and its alert
// (the bell, as in the world timers).
function choice(id, name, meta) {
  const on = view.pins.includes(id);
  const b = el("button", `tp-item${on ? " on" : ""}`);
  b.type = "button";
  b.setAttribute("aria-pressed", String(on));
  b.title = on ? `${name} is pinned · click to unpin` : `Pin ${name}`;
  const ring = el("span", "tp-icon");
  ring.append(timerIcon(id));
  const text = el("span", "tp-text");
  text.append(el("span", "tp-name", name));
  if (meta) text.append(el("span", "tp-meta", meta));
  b.append(ring, text, el("span", "tp-state", on ? "📌" : ""));
  b.addEventListener("click", () => pin(id, !on));
  const ringing = alerts.includes(id);
  const bell = el("button", `tp-bell${ringing ? " on" : ""}`, "🔔");
  bell.type = "button";
  bell.setAttribute("aria-pressed", String(ringing));
  bell.setAttribute("aria-label", `Alert for ${name}`);
  bell.title = ringing ? `Alert ${lead} min before ${name}, and as it starts · click to stop` : `Alert ${lead} min before ${name}, and as it starts`;
  bell.addEventListener("click", () => alert(id, !ringing));
  const row = el("div", "tp-row");
  row.append(b, bell);
  return row;
}

function draw() {
  if (!view) return;
  for (const b of document.querySelectorAll(".bs-tabs [data-tab]")) b.setAttribute("aria-selected", String(b.dataset.tab === tab));
  for (const s of document.querySelectorAll(".bs-tab")) s.hidden = s.dataset.tab !== tab;
  const look = view.look;
  toggle($("tp-train"), look.train);
  $("tp-train-line").classList.toggle("off", !look.train);
  $("tp-train-min").disabled = !look.train;
  if (document.activeElement !== $("tp-train-min")) $("tp-train-min").value = String(look.train_min);
  $("tp-train-value").textContent = `${look.train_min} min`;
  if (document.activeElement !== $("tp-within")) $("tp-within").value = String(look.within_min || 0);
  $("tp-within-value").textContent = look.within_min ? withinText(look.within_min) : "always";
  $("tp-alerts-note").textContent = `🔔 alerts show on the meter ${lead} min before and as it starts, ${sound ? "with a chime" : "silently (alert sound is off)"}; the lead time and sound are in the world timers' settings.`;
  const { events } = regionEvents(region);
  $("tp-events").replaceChildren(...Object.keys(events).map((id) => choice(id, EVENTS[id] || id, "")));
  const known = [...bosses].sort((a, b) => (a.map_name || "").localeCompare(b.map_name || "") || (a.name || "").localeCompare(b.name || ""));
  $("tp-bosses").replaceChildren(
    ...(known.length
      ? known.map((b) => choice(`boss:${b.id}`, b.name || `Field boss ${b.id}`, [b.level ? `Lv ${b.level}` : "", b.place || b.map_name || ""].filter(Boolean).join(" · ")))
      : [el("span", "bs-note", "none yet: open the world map in game")]),
  );
  for (const c of document.querySelectorAll(".bs-choice[data-key]")) for (const b of c.children) press(b, look[c.dataset.key] === b.dataset.v);
  for (const t of document.querySelectorAll(".bs-toggle[data-key]")) toggle(t, look[t.dataset.key]);
  for (const r of document.querySelectorAll("input[type='range'][data-key]")) {
    if (document.activeElement !== r) r.value = String(look[r.dataset.key]);
  }
  for (const v of document.querySelectorAll(".bs-value[data-for]")) v.textContent = v.dataset.for === "icon_size" ? `${look.icon_size}px` : String(look[v.dataset.for]);
  // A setting the layout doesn't use: greyed, saying why.
  for (const line of document.querySelectorAll("[data-only]")) {
    const off = line.dataset.only !== look.layout;
    line.classList.toggle("off", off);
    for (const c of line.querySelectorAll("button, input")) c.disabled = off;
    line.title = off ? `No effect: not with the ${look.layout} layout` : "";
  }
}

async function load() {
  try {
    const [v, s, b] = await Promise.all([invoke("timer_pins"), invoke("timer_settings"), invoke("field_bosses")]);
    view = v;
    if (s) {
      region = s.region || region;
      alerts = s.alerts || alerts;
      lead = s.alert_lead_min || lead;
      sound = s.alert_sound !== false;
    }
    bosses = b || [];
  } catch {
    // Next time, then.
  }
  draw();
}

for (const b of document.querySelectorAll(".bs-tabs [data-tab]")) {
  b.addEventListener("click", () => {
    tab = b.dataset.tab;
    draw();
  });
}
$("tp-train").addEventListener("click", () => view && setLook({ train: !view.look.train }));
$("tp-train-min").addEventListener("input", () => view && setLook({ train_min: Number($("tp-train-min").value) }, true));
$("tp-within").addEventListener("input", () => view && setLook({ within_min: Number($("tp-within").value) }, true));
for (const c of document.querySelectorAll(".bs-choice[data-key]")) {
  for (const b of c.children) b.addEventListener("click", () => view && setLook({ [c.dataset.key]: b.dataset.v }));
}
for (const t of document.querySelectorAll(".bs-toggle[data-key]")) {
  t.addEventListener("click", () => view && setLook({ [t.dataset.key]: !view.look[t.dataset.key] }));
}
for (const r of document.querySelectorAll("input[type='range'][data-key]")) {
  r.addEventListener("input", () => view && setLook({ [r.dataset.key]: Number(r.value) }, true));
}
tauri.event.listen("timer-settings", () => load());
tauri.event.listen("timer-pins", (e) => {
  if (e.payload && view && !setLook.timer && !setLook.saving) {
    view = e.payload;
    draw();
  }
});
load();
setInterval(() => {
  if (!document.hidden) load();
}, 30_000);
