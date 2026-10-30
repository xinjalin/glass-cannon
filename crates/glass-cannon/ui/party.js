// Party frames: a tile per member in roster order, filled by their HP in their class's
// colour (once they've fought, the meter knows their class), with a thin MP strip. HP
// and MP come from the party's own updates, and from what the game sends as it changes
// for anyone in view. Both only come when they change: no update just means no change.

const POLL_MS = 250;

const pct = (v, max) => (v != null && max ? Math.max(0, Math.min(1, v / max)) : null);

function tile(f) {
  const hp = pct(f.hp, f.hp_max);
  const mp = pct(f.mp, f.mp_max);
  const dead = f.hp === 0;
  const unknown = f.hp == null;
  const li = el("li", ["frame", f.class_code ? `c${f.class_code}` : "", f.is_self ? "self" : "", dead ? "dead" : "", unknown ? "unknown" : ""].filter(Boolean).join(" "));
  const fill = el("span", "fr-hp");
  fill.style.width = `${(hp ?? 1) * 100}%`;
  const name = el("span", "fr-name", (f.leader ? "★ " : "") + f.name);
  const value = el("span", "fr-value", dead ? "dead" : hp != null ? `${Math.round(hp * 100)}%` : "");
  const mpBar = el("span", "fr-mp");
  mpBar.style.width = `${(mp ?? 0) * 100}%`;
  li.append(fill, name, value, mpBar);
  li.title = [
    f.name + (f.is_self ? " (you)" : ""),
    f.hp != null ? `HP ${fmt(f.hp)} / ${fmt(f.hp_max || f.hp)}` : "HP not sent yet",
    f.mp != null ? `MP ${fmt(f.mp)} / ${fmt(f.mp_max || f.mp)}` : "",
  ]
    .filter(Boolean)
    .join("\n");
  return li;
}

async function refresh() {
  let frames = [];
  try {
    frames = await invoke("party_frames");
  } catch {
    return;
  }
  $("frames").replaceChildren(...frames.map(tile));
  $("empty").hidden = frames.length > 0;
}

setInterval(() => {
  if (!document.hidden) refresh();
}, POLL_MS);
refresh();
