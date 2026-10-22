// World timers: the events' schedule (schedule.js) for your region, field bosses from the
// world map, and which of them to be alerted about (the bell; the meter shows the alert).

let region = "as";
let alerts = new Set();
let lead = 5;
let bosses = [];
const LEADS = [1, 3, 5, 10, 15];
// A field boss "up" as of a world map read longer ago than this may have been killed.
const STALE_MS = 3 * 3_600_000;

function every_text(e) {
  if (e.times.length > 3 && !e.days) {
    const step = e.times[1] - e.times[0];
    return step % 60 === 0 ? `every ${step / 60} h` : `every ${step} min`;
  }
  const days = e.days ? e.days.map((d) => DAY_NAMES[d]).join(", ") : "daily";
  return days;
}

function drawRegions() {
  const nav = $("regions");
  nav.replaceChildren();
  for (const [key, r] of Object.entries(REGIONS)) {
    const b = el("button", "", `[${r.label}]`);
    b.type = "button";
    b.setAttribute("role", "tab");
    b.setAttribute("aria-selected", String(key === region));
    b.title = r.timeZone.replace("_", " ");
    b.addEventListener("click", () => {
      region = key;
      invoke("set_timer_region", { region }).catch(() => {});
      drawRegions();
      draw();
    });
    nav.append(b);
  }
  const leadBox = el("span", "lead");
  leadBox.append(el("span", "note", "alert"));
  for (const m of LEADS) {
    const b = el("button", "", `[${m}m]`);
    b.type = "button";
    b.setAttribute("aria-selected", String(m === lead));
    b.title = `Alerts ${m} minute${m === 1 ? "" : "s"} before`;
    b.addEventListener("click", () => {
      lead = m;
      invoke("set_alert_lead", { minutes: m }).catch(() => {});
      drawRegions();
    });
    leadBox.append(b);
  }
  nav.append(leadBox);
}

function bell(id, name) {
  const on = alerts.has(id);
  const b = el("button", "bell" + (on ? " on" : ""), on ? "🔔" : "🔕");
  b.type = "button";
  b.title = on ? `Alerting before ${name} · click to stop` : `Alert me before ${name}`;
  b.addEventListener("click", (e) => {
    e.stopPropagation();
    if (on) alerts.delete(id);
    else alerts.add(id);
    invoke("set_alert", { id, on: !on }).catch(() => {});
    draw();
  });
  return b;
}

function row(name, meta, when, cls, id, title) {
  const li = el("li", "timer " + cls);
  const text = el("div", "timer-text");
  text.append(el("span", "timer-name", name), el("span", "timer-meta", meta));
  const w = el("span", "timer-when", when);
  if (title) w.title = title;
  li.append(bell(id, name), text, w);
  return li;
}

function draw() {
  const now = Date.now();
  const list = $("list");
  list.replaceChildren();
  for (const { id, name, e, start } of upcoming(region, now, Infinity)) {
    const on = start <= now;
    const left = on ? start + e.minutes * 60_000 - now : start - now;
    const how = [every_text(e), e.minutes ? `${e.minutes} min` : ""].filter(Boolean).join(" · ");
    list.append(
      row(
        name,
        `${on ? "started" : dayAndTime(start)} · ${how}`,
        on ? `on · ${clock(left / 1000)}` : clock(left / 1000),
        on ? "on" : left <= SOON_MS ? "soon" : "",
        id,
        on ? "Time left" : `Starts ${new Date(start).toLocaleString()}`,
      ),
    );
  }
  drawBosses(now);
}

// Field bosses: from the world map (open it in game to read them). Up ones first, then by
// when they spawn. Double-click a name to name one.
function drawBosses(now) {
  const box = $("bosses");
  box.replaceChildren();
  $("bosses-empty").hidden = bosses.length > 0;
  const sorted = [...bosses].sort((a, b) => (a.time_ms <= now) - (b.time_ms <= now) || a.time_ms - b.time_ms);
  sorted.sort((a, b) => (b.time_ms <= now) - (a.time_ms <= now));
  for (const b of sorted) {
    const up = b.time_ms <= now;
    const name = b.name || `Field boss ${b.id}`;
    // Up as of the last world map; after a few hours it may well have been killed.
    const seen = b.seen_ms || b.time_ms;
    const stale = up && now - seen > STALE_MS;
    const li = row(
      name,
      `map ${b.map} · ${up ? `up since ${dayAndTime(b.time_ms)}` : `spawns ${dayAndTime(b.time_ms)}`}${stale ? ` · as of ${dayAndTime(seen)}` : ""}`,
      up ? (stale ? "up?" : "up") : clock((b.time_ms - now) / 1000),
      up ? (stale ? "stale" : "on") : b.time_ms - now <= SOON_MS ? "soon" : "",
      `boss:${b.id}`,
      up ? `Up when you last opened the world map (${new Date(seen).toLocaleString()}); open it again to update` : `Spawns ${new Date(b.time_ms).toLocaleString()}`,
    );
    li.querySelector(".timer-name").title = "Double-click to name it";
    li.querySelector(".timer-name").addEventListener("dblclick", () => {
      const given = window.prompt(`Name for field boss ${b.id} (as the world map shows it):`, b.name || "");
      if (given == null) return;
      invoke("rename_field_boss", { id: b.id, name: given }).then(loadBosses).catch(() => {});
    });
    box.append(li);
  }
}

async function loadBosses() {
  try {
    bosses = await invoke("field_bosses");
  } catch {
    bosses = [];
  }
  draw();
}

invoke("timer_settings")
  .then((s) => {
    if (REGIONS[s.region]) region = s.region;
    alerts = new Set(s.alerts);
    lead = s.alert_lead_min;
  })
  .catch(() => {})
  .finally(() => {
    drawRegions();
    loadBosses();
  });
setInterval(() => {
  if (!document.hidden) draw();
}, 1000);
setInterval(() => {
  if (!document.hidden) loadBosses();
}, 15_000);
