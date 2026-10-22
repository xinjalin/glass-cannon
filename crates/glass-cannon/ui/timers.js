// World timers: the game's recurring events on a fixed schedule in each region's time.
// The schedule follows Questlog's list (2026-10); times are minutes after midnight in the
// region's time zone, days 0 = Sunday.

const REGIONS = {
  as: { label: "Asia", schedule: "global", timeZone: "Asia/Tokyo" },
  naw: { label: "NA", schedule: "global", timeZone: "America/Los_Angeles" },
  eu: { label: "EU", schedule: "global", timeZone: "Europe/Berlin" },
  la: { label: "LA", schedule: "global", timeZone: "America/Sao_Paulo" },
  kr: { label: "Korea", schedule: "kr", timeZone: "Asia/Seoul" },
  tw: { label: "Taiwan", schedule: "tw", timeZone: "Asia/Taipei" },
};

const at = (hhmm) => {
  const [h, m] = hhmm.split(":").map(Number);
  return h * 60 + m;
};
// From `first`, every `step` minutes until midnight.
const every = (first, step) => {
  const out = [];
  for (let t = at(first); t < 1440; t += step) out.push(t);
  return out;
};
const MON_THU_SAT = [1, 4, 6];
const WED_SAT = [3, 6];
const SUN_FRI = [0, 5];

const EVENTS = {
  "spacetime-rift": "Spacetime Rift",
  "shugo-festival": "Shugo Festival",
  "dimensional-invasion": "Dimensional Invasion",
  "watcher-kaira": "Watcher Kaira",
  "artifact-siege": "Artifact Siege",
  "siege-bosses": "Siege Bosses",
  "guardian-lord-nahma": "Guardian Lord Nahma",
  "daily-reset": "Daily reset",
  "weekly-reset": "Weekly reset",
};

const shared = {
  "spacetime-rift": { times: every("02:00", 180), minutes: 60 },
  "shugo-festival": { times: every("00:00", 60), minutes: 10 },
  "dimensional-invasion": { times: every("00:30", 60), minutes: 13 },
};
const SCHEDULES = {
  global: {
    ...shared,
    "watcher-kaira": { times: every("01:00", 180) },
    "artifact-siege": { days: MON_THU_SAT, times: [at("21:00")], minutes: 30 },
    "siege-bosses": { days: MON_THU_SAT, times: [at("21:30")], minutes: 30 },
    "guardian-lord-nahma": { days: SUN_FRI, times: [at("21:00")], minutes: 30 },
    "daily-reset": { times: [at("16:00")], timeZone: "Asia/Seoul" },
    "weekly-reset": { days: [3], times: [at("16:00")], timeZone: "Asia/Seoul" },
  },
  kr: {
    ...shared,
    "watcher-kaira": { times: every("01:00", 240) },
    "artifact-siege": { days: WED_SAT, times: [at("21:20"), at("21:50"), at("22:20")], minutes: 30 },
    "siege-bosses": { days: WED_SAT, times: [at("21:45"), at("22:15"), at("22:45")], minutes: 30 },
    "guardian-lord-nahma": { days: SUN_FRI, times: [at("22:00"), at("22:30"), at("23:00")], minutes: 30 },
    "daily-reset": { times: [at("05:00")] },
    "weekly-reset": { days: [3], times: [at("05:00")] },
  },
  tw: {
    ...shared,
    "watcher-kaira": { times: every("01:00", 240) },
    "artifact-siege": { days: WED_SAT, times: [at("21:30")], minutes: 30 },
    "siege-bosses": { days: WED_SAT, times: [at("21:45")], minutes: 30 },
    "guardian-lord-nahma": { days: SUN_FRI, times: [at("22:00")], minutes: 30 },
    "daily-reset": { times: [at("05:00")] },
    "weekly-reset": { days: [3], times: [at("05:00")] },
  },
};

const DAY_NAMES = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
// Soon: highlighted in the list.
const SOON_MS = 15 * 60_000;

let region = "as";

// The wall clock in a time zone: its date, minutes after midnight, weekday, and how far
// ahead of UTC it is (minutes).
const zoneFormats = new Map();
function zoned(now, timeZone) {
  if (!zoneFormats.has(timeZone)) {
    zoneFormats.set(
      timeZone,
      new Intl.DateTimeFormat("en-US", {
        timeZone,
        hourCycle: "h23",
        year: "numeric",
        month: "numeric",
        day: "numeric",
        hour: "numeric",
        minute: "numeric",
        second: "numeric",
        weekday: "short",
      }),
    );
  }
  const p = Object.fromEntries(zoneFormats.get(timeZone).formatToParts(now).map((x) => [x.type, x.value]));
  const asUtc = Date.UTC(+p.year, +p.month - 1, +p.day, +p.hour, +p.minute, +p.second);
  return {
    y: +p.year,
    m: +p.month - 1,
    d: +p.day,
    weekday: DAY_NAMES.indexOf(p.weekday),
    offsetMin: Math.round((asUtc - now) / 60_000),
  };
}

// The next start of an event (or the one going on), as ms since 1970.
function next(event, timeZone, now) {
  const z = zoned(now, timeZone);
  const lasts = (event.minutes || 0) * 60_000;
  let best = null;
  for (let k = 0; k <= 7; k++) {
    if (event.days && !event.days.includes((z.weekday + k) % 7)) continue;
    for (const t of event.times) {
      const start = Date.UTC(z.y, z.m, z.d + k) + t * 60_000 - z.offsetMin * 60_000;
      if (start + lasts <= now) continue;
      if (best == null || start < best) best = start;
    }
    if (best != null) break;
  }
  return best;
}

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
}

function draw() {
  const r = REGIONS[region];
  const now = Date.now();
  const rows = Object.entries(SCHEDULES[r.schedule])
    .map(([id, e]) => ({ id, e, start: next(e, e.timeZone || r.timeZone, now) }))
    .filter((x) => x.start != null)
    .sort((a, b) => a.start - b.start);
  const list = $("list");
  list.replaceChildren();
  for (const { id, e, start } of rows) {
    const on = start <= now;
    const left = on ? start + e.minutes * 60_000 - now : start - now;
    const li = el("li", "timer" + (on ? " on" : left <= SOON_MS ? " soon" : ""));
    const text = el("div", "timer-text");
    text.append(el("span", "timer-name", EVENTS[id] || id));
    const how = [every_text(e), e.minutes ? `${e.minutes} min` : ""].filter(Boolean).join(" · ");
    text.append(el("span", "timer-meta", `${on ? "started" : dayAndTime(start)} · ${how}`));
    const when = el("span", "timer-when", on ? `on · ${clock(left / 1000)}` : clock(left / 1000));
    when.title = on ? "Time left" : `Starts ${new Date(start).toLocaleString()}`;
    li.append(text, when);
    list.append(li);
  }
}

invoke("timer_region")
  .then((r) => {
    if (REGIONS[r]) region = r;
  })
  .catch(() => {})
  .finally(() => {
    drawRegions();
    draw();
  });
setInterval(() => {
  if (!document.hidden) draw();
}, 1000);
