// Party frames: a tile per member, filled by their HP in their class's colour (once
// they've fought, the meter knows their class), with a thin MP strip and their shield. HP
// and MP come from the party's own updates, and from what the game sends as it changes
// for anyone in view; both only come when they change. As set up beside it on the
// overlay (widgets.rs → PartyLook): a grid or a list, how many to a row, their size; the
// HP as a share, a value or nothing; names; the buffs you put on each and their debuffs;
// HP warning colours and a damage trail; the one your target's attacking outlined; a dead
// member greyed with a skull. In roster order, the lowest HP first, or by class.

const POLL_MS = 250;
// HP warning colours: yellow from half, red from a fifth.
const HP_WARN = [
  [0.2, "#e5383b"],
  [0.5, "#f5c542"],
];
// The damage trail: HP lost stays this long before shrinking away.
const TRAIL_HOLD_MS = 600;

let look = { layout: "grid", columns: 0, size: 100, hp_text: "percent", names: true, effects: "both", sort: "roster", hp_warn: false, trail: false, target_mark: true };

const pct = (v, max) => (v != null && max ? Math.max(0, Math.min(1, v / max)) : null);

// Each member's trail: the share it's at, and when it was last let down.
const trails = new Map();
function trailOf(name, hp) {
  const now = performance.now();
  let t = trails.get(name);
  if (!t || hp == null || hp >= t.at) {
    t = { at: hp ?? 1, hold: 0 };
    trails.set(name, t);
  } else if (!t.hold) {
    t.hold = now + TRAIL_HOLD_MS;
  } else if (now >= t.hold) {
    t.at = hp;
    t.hold = 0;
  }
  return t.at;
}

function effectIcon(x) {
  const label = x.name || `#${x.effect}`;
  const box = el("span", `fr-effect${x.debuff ? " debuff" : ""}`);
  box.append(iconFor(x.icon_key, label, () => {}));
  if (x.remaining_ms != null && x.remaining_ms < 10_000) box.append(el("span", "fr-effect-time", String(Math.ceil(x.remaining_ms / 1000))));
  if (x.stacks > 1) box.append(el("span", "fr-effect-stacks", String(x.stacks)));
  box.title = `${label}${x.debuff ? " (debuff)" : ""}${x.remaining_ms != null ? ` · ${Math.ceil(x.remaining_ms / 1000)} s left` : ""}`;
  return box;
}

function tile(f) {
  const hp = pct(f.hp, f.hp_max);
  const mp = pct(f.mp, f.mp_max);
  const dead = f.hp === 0;
  const unknown = f.hp == null;
  const li = el(
    "li",
    ["frame", f.class_code ? `c${f.class_code}` : "", f.is_self ? "self" : "", dead ? "dead" : "", unknown ? "unknown" : "", f.targeted && look.target_mark ? "targeted" : ""].filter(Boolean).join(" "),
  );
  // The trail under the fill: HP just lost, held a moment, then let go.
  if (look.trail && !dead && hp != null) {
    const trail = el("span", "fr-trail");
    trail.style.width = `${trailOf(f.name, hp) * 100}%`;
    li.append(trail);
  }
  const fill = el("span", "fr-hp");
  fill.style.width = `${(dead ? 0 : (hp ?? 1)) * 100}%`;
  const warn = look.hp_warn && hp != null && !dead ? HP_WARN.find(([at]) => hp <= at) : null;
  if (warn) fill.style.background = `color-mix(in srgb, ${warn[1]} 70%, transparent)`;
  li.append(fill);
  const span = dead ? null : shieldSpan(f.hp, f.hp_max, f.shield);
  if (span) {
    const sh = el("span", "fr-shield");
    sh.style.left = `${span[0] * 100}%`;
    sh.style.width = `${span[1] * 100}%`;
    li.append(sh);
  }
  if (look.names) li.append(el("span", "fr-name", (f.leader ? "★ " : "") + f.name));
  const value = dead ? "☠ dead" : hp == null || look.hp_text === "none" ? "" : look.hp_text === "value" ? fmt(f.hp) : `${Math.round(hp * 100)}%`;
  li.append(el("span", "fr-value", value));
  const mpBar = el("span", "fr-mp");
  mpBar.style.width = `${(mp ?? 0) * 100}%`;
  li.append(mpBar);
  if (f.effects && f.effects.length && !dead) {
    li.classList.add("has-effects");
    const row = el("span", "fr-effects");
    row.append(...f.effects.map(effectIcon));
    li.append(row);
  }
  if (f.targeted && look.target_mark) li.append(el("span", "fr-target", "⚔"));
  li.title = [
    f.name + (f.is_self ? " (you)" : ""),
    f.hp != null ? `HP ${fmt(f.hp)} / ${fmt(f.hp_max || f.hp)}` : "HP not sent yet",
    f.shield ? `shield ${fmt(f.shield)}` : "",
    f.mp != null ? `MP ${fmt(f.mp)} / ${fmt(f.mp_max || f.mp)}` : "",
    f.targeted ? "your target is attacking them" : "",
  ]
    .filter(Boolean)
    .join("\n");
  return li;
}

function sorted(frames) {
  if (look.sort === "hp") {
    const share = (f) => (f.hp === 0 ? 2 : (pct(f.hp, f.hp_max) ?? 3));
    return [...frames].sort((a, b) => share(a) - share(b));
  }
  if (look.sort === "class") return [...frames].sort((a, b) => (a.class_code || 99) - (b.class_code || 99));
  return frames;
}

function applyLook() {
  const list = $("frames");
  list.dataset.layout = look.layout;
  list.style.setProperty("--cols", look.columns ? `repeat(${look.columns}, minmax(0, 1fr))` : "repeat(auto-fill, minmax(7.5em, 1fr))");
  $("panel").style.fontSize = `${look.size / 100}em`;
}

async function refresh() {
  let frames = [];
  try {
    frames = await invoke("party_frames");
  } catch {
    return;
  }
  applyLook();
  $("frames").replaceChildren(...sorted(frames).map(tile));
  $("empty").hidden = frames.length > 0;
}

invoke("party_look")
  .then((l) => {
    if (l) look = l;
    refresh();
  })
  .catch(() => {});
tauri.event.listen("party-look", (e) => {
  if (e.payload) look = e.payload;
  refresh();
});
setInterval(() => {
  if (!document.hidden) refresh();
}, POLL_MS);
refresh();
