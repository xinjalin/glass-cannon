// The boss cast timeline: what the boss cast this fight (newest first), and while a fight
// is on, what it cast next the last time you fought it, with how long until then.

const POLL_MS = 500;
const COMING_UP = 5;
const ATTACKS_KEY = "boss-casts-attacks";
// A skill cast this often in a fight is the boss's routine, not a mechanic.
const FREQUENT = 8;

let showAttacks = false;
try {
  showAttacks = localStorage.getItem(ATTACKS_KEY) === "1";
} catch {
  // No storage: start with plain attacks hidden.
}

// Bosses spam plain attacks and a few routine skills; hidden unless asked for. Frequent:
// named "Attack", or cast FREQUENT times or more in this fight or the last one.
let frequent = new Set();
const shown = (c) => showAttacks || !(c.skill_name === "Attack" || frequent.has(c.skill));

function countFrequent(v) {
  frequent = new Set();
  for (const list of [v.casts, v.last]) {
    const n = new Map();
    for (const c of list) n.set(c.skill, (n.get(c.skill) || 0) + 1);
    for (const [skill, count] of n) if (count >= FREQUENT) frequent.add(skill);
  }
}

const nameOf = (c) => c.skill_name || `#${c.skill}`;

function castRow(c, right, cls) {
  const li = el("li", "bc-row " + (cls || ""));
  const text = el("div", "bc-text");
  text.append(el("span", "bc-name", nameOf(c)));
  const meta = [c.cast_ms ? `${(c.cast_ms / 1000).toFixed(1)} s cast` : "instant", c.boss].filter(Boolean).join(" · ");
  text.append(el("span", "bc-meta", meta));
  li.append(el("span", "bc-at", clock(c.at_s)), text, el("span", "bc-right", right));
  li.title = `${nameOf(c)} at ${clock(c.at_s)}` + (c.stopped ? " · stopped before it went off" : "");
  return li;
}

function draw(v) {
  if (v) countFrequent(v);
  const attacks = $("attacks");
  attacks.className = showAttacks ? "on" : "";
  $("bc-boss").textContent = v ? v.boss || "boss" : "";
  $("bc-time").textContent = v ? (v.active ? `fight ${clock(v.now_s)}` : `last fight · ${clock(v.now_s)}`) : "";
  const casts = v ? v.casts.filter(shown) : [];
  const list = $("casts");
  list.replaceChildren(...casts.slice().reverse().map((c) => castRow(c, c.stopped ? "✕" : "", c.stopped ? "stopped" : "")));
  $("empty").hidden = casts.length > 0;
  // Coming up: last time's casts still ahead of where this fight is.
  const ahead = v && v.active ? v.last.filter(shown).filter((c) => c.at_s > v.now_s).slice(0, COMING_UP) : [];
  $("next-box").hidden = ahead.length === 0;
  $("next-when").textContent = v && v.last_ms ? `as on ${dayAndTime(v.last_ms)}` : "";
  $("next").replaceChildren(
    ...ahead.map((c) => {
      const left = c.at_s - v.now_s;
      return castRow(c, `in ${clock(left)}`, left <= 5 ? "soon" : "");
    }),
  );
}

async function refresh() {
  let v = null;
  try {
    v = await invoke("boss_casts");
  } catch {
    return;
  }
  draw(v);
}

$("attacks").addEventListener("click", () => {
  showAttacks = !showAttacks;
  try {
    localStorage.setItem(ATTACKS_KEY, showAttacks ? "1" : "0");
  } catch {
    // Not kept, that's all.
  }
  refresh();
});

setInterval(() => {
  if (!document.hidden) refresh();
}, POLL_MS);
refresh();
