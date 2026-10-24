// World timers: the events' schedule (schedule.js) for your region, field bosses from the
// world map, and which of them to be alerted about (the bell; the meter shows the alert).

let region = "as";
let alerts = new Set();
// Starred events and field bosses, and whether only they show ([★ only]).
let favourites = new Set();
let favouritesOnly = false;
const shown = (id) => !favouritesOnly || favourites.has(id);
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
  const only = el("button", "fav-filter", "[★ only]");
  only.type = "button";
  only.setAttribute("aria-selected", String(favouritesOnly));
  only.title = favouritesOnly ? "Showing your starred events and bosses · click for all" : "Show only the events and bosses you starred";
  only.addEventListener("click", () => {
    favouritesOnly = !favouritesOnly;
    invoke("set_timer_favourites_only", { on: favouritesOnly }).catch(() => {});
    drawRegions();
    draw();
  });
  nav.append(only);
}

function star(id, name) {
  const on = favourites.has(id);
  const b = el("button", "star" + (on ? " on" : ""), on ? "★" : "☆");
  b.type = "button";
  b.title = on ? `${name} is starred · click to unstar` : `Star ${name} ([★ only] shows just the starred)`;
  b.addEventListener("click", (e) => {
    e.stopPropagation();
    if (on) favourites.delete(id);
    else favourites.add(id);
    invoke("set_timer_favourite", { id, on: !on }).catch(() => {});
    draw();
  });
  return b;
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
  li.append(star(id, name), bell(id, name), text, w);
  return li;
}

function draw() {
  const now = Date.now();
  const list = $("list");
  list.replaceChildren();
  const events = upcoming(region, now, Infinity).filter((x) => shown(x.id));
  $("favourites-empty").hidden = !favouritesOnly || favourites.size > 0;
  for (const { id, name, e, start } of events) {
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
  const sorted = bosses.filter((b) => shown(`boss:${b.id}`)).sort((a, b) => (a.time_ms <= now) - (b.time_ms <= now) || a.time_ms - b.time_ms);
  sorted.sort((a, b) => (b.time_ms <= now) - (a.time_ms <= now));
  for (const b of sorted) {
    const up = b.time_ms <= now;
    const name = (b.name || `Field boss ${b.id}`) + (b.level ? ` · Lv ${b.level}` : "");
    // Up as of the last world map; after a few hours it may well have been killed.
    const seen = b.seen_ms || b.time_ms;
    const stale = up && now - seen > STALE_MS;
    const li = row(
      name,
      `${b.place || b.map_name || `map ${b.map}`} · ${up ? `up since ${dayAndTime(b.time_ms)}` : `spawns ${dayAndTime(b.time_ms)}`}${stale ? ` · as of ${dayAndTime(seen)}` : ""}`,
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

// Odyle energy, the other content tickets (double-click one to name it: the game's data
// doesn't), and who holds the Abyss artifacts, as the game last sent them. Only named
// tickets show until you ask for all of them (to name some).
let contents = null;
let allTickets = false;

function plainRow(name, meta, when, cls, title) {
  const li = el("li", "timer " + cls);
  const text = el("div", "timer-text");
  text.append(el("span", "timer-name", name), el("span", "timer-meta", meta));
  const w = el("span", "timer-when", when);
  if (title) w.title = title;
  li.append(el("span", "bell-space"), el("span", "bell-space"), text, w);
  return li;
}

function drawContents() {
  const c = contents;
  const abyss = $("abyss");
  abyss.replaceChildren();
  $("abyss-empty").hidden = !!(c && c.abyss.length);
  for (const a of (c && c.abyss) || []) {
    const until = a.until_ms ? ` · resets ${dayAndTime(a.until_ms)}` : "";
    abyss.append(
      plainRow(a.name, `layer ${a.layer}${until} · as of ${dayAndTime(c.abyss_ms)}`, a.owner || "nobody", a.owner ? "held" : "", "Who held it when the game last said"),
    );
  }
  const list = $("tickets");
  list.replaceChildren();
  const any = c && (c.odyle || c.tickets.length);
  $("tickets-empty").hidden = !!any;
  if (!any) return;
  $("tickets-when").textContent = `as of ${dayAndTime(c.tickets_ms)}`;
  if (c.odyle) {
    const [basic, charged] = c.odyle;
    list.append(plainRow("Odyle energy", `charged ${fmt(charged)}`, fmt(basic), "odyle", "Basic energy (charged energy below)"));
  }
  const unnamed = c.tickets.filter((t) => !t.name).length;
  const more = $("all-tickets");
  more.hidden = unnamed === 0;
  more.textContent = allTickets ? "[hide unnamed tickets]" : `[show all ${c.tickets.length} tickets to name them]`;
  for (const t of c.tickets.filter((t) => allTickets || t.name)) {
    const left = [t.count, t.charged].filter((v) => v != null).map(fmt).join(" + ");
    const li = plainRow(t.name || `ticket #${t.id}`, t.name ? `#${t.id}` : "double-click to name it", left, "", "Entries left (+ charged)");
    const name = li.querySelector(".timer-name");
    name.title = "Double-click to name it";
    name.addEventListener("dblclick", () => {
      const given = window.prompt(`Name for ticket #${t.id} (what it's for in game):`, t.name || "");
      if (given == null) return;
      invoke("rename_ticket", { id: t.id, name: given }).then(loadContents).catch(() => {});
    });
    list.append(li);
  }
}

$("all-tickets").addEventListener("click", () => {
  allTickets = !allTickets;
  drawContents();
});

async function loadContents() {
  try {
    contents = await invoke("contents");
  } catch {
    contents = null;
  }
  drawContents();
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
    favourites = new Set(s.favourites || []);
    favouritesOnly = !!s.favourites_only;
  })
  .catch(() => {})
  .finally(() => {
    drawRegions();
    loadBosses();
    loadContents();
  });
setInterval(() => {
  if (!document.hidden) draw();
}, 1000);
setInterval(() => {
  if (!document.hidden) {
    loadBosses();
    loadContents();
  }
}, 15_000);
