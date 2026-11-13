// The action bar: the skills chosen in the settings, each with its cooldown (a sweep over
// the icon and the time left, or a bar), its charges, and a glow when it's ready again;
// then the buffs on you that you follow, with their time left. Names come from the game
// data and are set with textContent, never innerHTML.

const POLL_MS = 100;
// How long a skill glows once it's ready again.
const READY_GLOW_MS = 1200;

let lastKey = "";
// Each skill's time left last time, to see it come off cooldown.
const before = new Map();
const glowing = new Map();

function seconds(ms) {
  if (ms >= 60_000) return `${Math.ceil(ms / 60_000)}m`;
  return ms < 10_000 ? (ms / 1000).toFixed(1) : String(Math.ceil(ms / 1000));
}

// An icon, with the sweep and time over it (square and round looks).
function iconSlot(label, iconKey, remaining, duration, now) {
  const slot = el("li", "ab-slot");
  slot.append(iconFor(iconKey, label, () => (lastKey = "")));
  if (remaining > 0 && duration > 0) {
    slot.classList.add("cooling");
    const sweep = el("span", "ab-sweep");
    sweep.style.setProperty("--done", `${(1 - remaining / duration) * 360}deg`);
    slot.append(sweep, el("span", "ab-time", seconds(remaining)));
  }
  slot.title = label;
  return slot;
}

// A bar with the name and time left (bars look).
function barSlot(label, iconKey, remaining, duration) {
  const slot = el("li", "ab-bar-slot");
  slot.append(iconFor(iconKey, label, () => (lastKey = "")));
  const bar = el("div", "ab-bar");
  const fill = el("span", "ab-fill");
  fill.style.width = `${remaining > 0 && duration > 0 ? (remaining / duration) * 100 : 0}%`;
  bar.append(fill, el("span", "ab-bar-name", label), el("span", "ab-bar-time", remaining > 0 ? seconds(remaining) : "ready"));
  slot.append(bar);
  if (remaining <= 0) slot.classList.add("ready");
  return slot;
}

async function refresh() {
  let view;
  try {
    view = await invoke("action_bar");
  } catch {
    return;
  }
  const now = performance.now();
  // Skills that just came off cooldown glow a moment.
  for (const s of view.skills) {
    const was = before.get(s.skill);
    if (was > 0 && s.remaining_ms === 0) glowing.set(s.skill, now + READY_GLOW_MS);
    before.set(s.skill, s.remaining_ms);
  }
  const glow = [...glowing].filter(([, until]) => until > now).map(([k]) => k);
  const key = JSON.stringify([view, glow]);
  if (key === lastKey) return;
  lastKey = key;
  const bars = view.style === "bars";
  document.body.dataset.look = view.style;
  $("ab-slots").replaceChildren(
    ...view.skills.map((s) => {
      const label = s.name || `#${s.skill}`;
      // Bars: the charges by the name; icons: in the corner.
      const shown = bars && s.charges != null ? `${label} ×${s.charges}` : label;
      const slot = bars ? barSlot(shown, s.icon_key, s.remaining_ms, s.duration_ms) : iconSlot(label, s.icon_key, s.remaining_ms, s.duration_ms, now);
      if (!bars && s.charges != null) slot.append(el("span", "ab-charges", String(s.charges)));
      if (glow.includes(s.skill)) slot.classList.add("glow");
      slot.title = `${label}` + (s.remaining_ms > 0 ? `: ready in ${seconds(s.remaining_ms)} s` : ": ready") + (s.charges != null ? ` · ${s.charges} charge${s.charges === 1 ? "" : "s"}` : "");
      return slot;
    }),
  );
  $("ab-buffs").replaceChildren(
    ...view.buffs.map((b) => {
      const label = b.name || `#${b.effect}`;
      const left = b.remaining_ms ?? 0;
      const shown = bars && b.stacks > 1 ? `${label} ×${b.stacks}` : label;
      const slot = bars ? barSlot(shown, b.icon_key, left, b.duration_ms) : iconSlot(label, b.icon_key, left, b.duration_ms, now);
      slot.classList.add("buff");
      if (!bars && b.stacks > 1) slot.append(el("span", "ab-charges", String(b.stacks)));
      slot.title = label + (b.remaining_ms != null ? `: ${seconds(left)} s left` : "") + (b.stacks > 1 ? ` · ${b.stacks} on you` : "");
      return slot;
    }),
  );
  $("ab-buffs").hidden = view.buffs.length === 0;
  const empty = view.unset;
  $("empty").hidden = !empty;
  $("empty").textContent = view.class == null ? "use a skill in game: the bar is chosen per class" : "choose its skills and buffs in the settings (overlay → action bar)";
}

setInterval(() => {
  if (!document.hidden) refresh();
}, POLL_MS);
refresh();
