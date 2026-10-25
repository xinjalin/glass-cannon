// The game's recurring events on a fixed schedule in each region's time, shared by the
// timers window and the meter's alerts. The schedule follows Questlog's list (2026-10);
// times are minutes after midnight in the region's time zone, days 0 = Sunday. `minutes`
// is how long it's on: one length for every start, or one per start.

// Arena of Tactics (10v10) opens 11:00-14:00 and 19:00-21:00 in the region's time (as
// LOXIN's timers have it, 2026-10); its Asian times aren't known.
const ARENA = { "arena-tactics": { times: [11 * 60, 19 * 60], minutes: [180, 120] } };

const REGIONS = {
  as: { label: "Asia", schedule: "global", timeZone: "Asia/Tokyo" },
  naw: { label: "NA", schedule: "global", timeZone: "America/Los_Angeles", extra: ARENA },
  eu: { label: "EU", schedule: "global", timeZone: "Europe/Berlin", extra: ARENA },
  la: { label: "LA", schedule: "global", timeZone: "America/Sao_Paulo", extra: ARENA },
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
  "arena-tactics": "Arena of Tactics (10v10)",
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

// How long an event's `i`th start of the day is on (minutes).
const lastsMin = (event, i) => (Array.isArray(event.minutes) ? event.minutes[i] : event.minutes) || 0;

// An event's starts from today on for a week, as { start (ms since 1970), minutes }, in order.
function starts(event, timeZone, now) {
  const z = zoned(now, timeZone);
  const out = [];
  for (let k = 0; k <= 7; k++) {
    if (event.days && !event.days.includes((z.weekday + k) % 7)) continue;
    event.times.forEach((t, i) => {
      out.push({ start: Date.UTC(z.y, z.m, z.d + k) + t * 60_000 - z.offsetMin * 60_000, minutes: lastsMin(event, i) });
    });
  }
  return out.sort((a, b) => a.start - b.start);
}

// The next start of an event (or the one going on): { start, minutes }, or null.
function next(event, timeZone, now) {
  return starts(event, timeZone, now).find((s) => s.start + s.minutes * 60_000 > now) || null;
}

// The next `n` starts after now (not the one going on), for the timers' hover.
function nextStarts(event, timeZone, now, n) {
  return starts(event, timeZone, now)
    .filter((s) => s.start > now)
    .slice(0, n);
}

// A region's events: its schedule and its own extras.
function regionEvents(regionKey) {
  const r = REGIONS[regionKey] || REGIONS.as;
  return { r, events: { ...SCHEDULES[r.schedule], ...(r.extra || {}) } };
}

// Upcoming starts in a region within `withinMs`: [{ id, name, e, start, minutes, timeZone }],
// soonest first.
function upcoming(regionKey, now, withinMs) {
  const { r, events } = regionEvents(regionKey);
  return Object.entries(events)
    .map(([id, e]) => {
      const timeZone = e.timeZone || r.timeZone;
      const n = next(e, timeZone, now);
      return { id, name: EVENTS[id] || id, e, timeZone, start: n && n.start, minutes: n ? n.minutes : 0 };
    })
    .filter((x) => x.start != null && x.start - now <= withinMs)
    .sort((a, b) => a.start - b.start);
}
