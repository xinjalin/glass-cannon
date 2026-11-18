// Icons for the timers (timer pins, world timers): a simple glyph for each kind of event,
// drawn as SVG in the current colour. Our own drawings, not the game's art.

const TIMER_SVG = "http://www.w3.org/2000/svg";

// Each kind's glyph: SVG elements as [tag, attributes], in a 24 × 24 box.
const TIMER_GLYPHS = {
  // A swirl: a rift opening.
  rift: [
    ["circle", { cx: 12, cy: 12, r: 8.5 }],
    ["path", { d: "M12 6.5a5.5 5.5 0 1 1-5.2 7.3" }],
    ["path", { d: "M12 9.5a2.5 2.5 0 1 1-2.4 3.2" }],
  ],
  // A star: the festival.
  festival: [["path", { d: "M12 3.5l2.4 5.3 5.6.6-4.2 3.8 1.2 5.6L12 16l-5 2.8 1.2-5.6L4 9.4l5.6-.6z" }]],
  // A gate with an arrow through it: an invasion.
  invasion: [
    ["path", { d: "M12 3l8.5 9L12 21l-8.5-9z" }],
    ["path", { d: "M8 12h8M13 9l3 3-3 3" }],
  ],
  // Horns over a skull: a boss.
  boss: [
    ["path", { d: "M5 4c0 3.5 1.5 5.5 3.5 6M19 4c0 3.5-1.5 5.5-3.5 6" }],
    ["path", { d: "M7 13a5 5 0 0 1 10 0v3l-1.5 1.5v2h-7v-2L7 16z" }],
    ["circle", { cx: 10, cy: 14, r: 0.9, fill: "currentColor" }],
    ["circle", { cx: 14, cy: 14, r: 0.9, fill: "currentColor" }],
  ],
  // A tower: a siege.
  siege: [["path", { d: "M5 21V10h2.5V7H10v3h4V7h2.5v3H19v11zM10 21v-4h4v4" }]],
  // Crossed swords: the arena.
  arena: [
    ["path", { d: "M5 5l10 10M19 5L9 15" }],
    ["path", { d: "M13.5 17l3.5 3.5M10.5 17L7 20.5M15 15l2-2M9 15l-2-2" }],
  ],
  // A clock: a reset.
  reset: [
    ["circle", { cx: 12, cy: 12, r: 8.5 }],
    ["path", { d: "M12 7.5V12l3 2" }],
  ],
};

// Which glyph an event or field boss takes.
const TIMER_KINDS = {
  "spacetime-rift": "rift",
  "shugo-festival": "festival",
  "dimensional-invasion": "invasion",
  "watcher-kaira": "boss",
  "guardian-lord-nahma": "boss",
  "siege-bosses": "boss",
  "artifact-siege": "siege",
  "arena-tactics": "arena",
  "daily-reset": "reset",
  "weekly-reset": "reset",
};
const timerKind = (id) => (id.startsWith("boss:") ? "boss" : TIMER_KINDS[id] || "reset");

function timerIcon(id) {
  const svg = document.createElementNS(TIMER_SVG, "svg");
  svg.setAttribute("viewBox", "0 0 24 24");
  svg.setAttribute("class", "timer-icon");
  svg.setAttribute("aria-hidden", "true");
  for (const [tag, attrs] of TIMER_GLYPHS[timerKind(id)]) {
    const e = document.createElementNS(TIMER_SVG, tag);
    for (const [k, v] of Object.entries(attrs)) e.setAttribute(k, String(v));
    svg.append(e);
  }
  return svg;
}

// A span of minutes: "45 min", "2h", "1h 30".
function withinText(min) {
  if (min < 60) return `${min} min`;
  const h = Math.floor(min / 60);
  const m = min % 60;
  return m ? `${h}h ${String(m).padStart(2, "0")}` : `${h}h`;
}

// Short names for the icons' labels.
const TIMER_SHORT = {
  "spacetime-rift": "Rift",
  "shugo-festival": "Shugo",
  "dimensional-invasion": "Invasion",
  "watcher-kaira": "Kaira",
  "artifact-siege": "Siege",
  "siege-bosses": "Siege bosses",
  "guardian-lord-nahma": "Nahma",
  "arena-tactics": "Arena",
  "daily-reset": "Daily",
  "weekly-reset": "Weekly",
};
