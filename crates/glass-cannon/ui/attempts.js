// The attempts window: each character's tries at every dungeon and raid boss (kills,
// wipes, the wipe that got furthest, the fastest kill), from every fight the meter saw.
// One tab per character; the dungeons and raids latest tried first. Text from the game
// and character names are set with textContent, never innerHTML.

const POLL_MS = 3000;
const RAID_KIND = "Sanctuary";
// Out in the world, though the game puts you in a copy of it: with the field bosses.
const FIELD_KINDS = ["Dimensional Invasion"];
// Cleared once each, solo: a tab of their own.
const SEALED_KINDS = ["Sealed Dungeon", "Stronghold"];

let data = null;
// The character shown ("" for fights whose character wasn't known), and which places.
let character = null;
let kind = "all";
let lastKey = "";

const pct = (x) => `${(x * 100).toFixed(1)}%`;
const label = (name) => name || "unknown";
const plural = (n, one, many) => `${n} ${n === 1 ? one : many}`;

function stored(key) {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}
function store(key, value) {
  try {
    localStorage.setItem(key, value);
  } catch {
    // Not remembered, then.
  }
}
kind = stored("attempts-kind") || "all";

function placeKind(p) {
  if (!p.kind || FIELD_KINDS.includes(p.kind)) return "other";
  if (SEALED_KINDS.includes(p.kind)) return "sealed";
  return p.kind === RAID_KIND ? "raids" : "dungeons";
}

function bossRow(b) {
  const row = el("div", "att-boss");
  const name = el("span", "att-name", b.name);
  name.title = `${b.name} · first tried ${dayAndTime(b.first_ms)} · last ${dayAndTime(b.last_ms)} · ${clock(b.fight_s)} in its fights`;
  const tries = el("span", "att-tries", String(b.attempts));
  tries.title = plural(b.attempts, "try", "tries");
  const record = el("span", "att-record");
  if (b.kills) record.append(el("span", "end-tag kill", `✓${b.kills}`));
  if (b.wipes) record.append(el("span", "end-tag wipe", `✗${b.wipes}`));
  // The best of it: the fastest kill, else the wipe that got the boss lowest.
  const best = el("span", "att-best");
  if (b.fastest_kill_s != null) {
    best.append(el("span", "best-tag", `★ ${clock(b.fastest_kill_s)}`));
    best.title = `Your fastest kill${b.first_kill_ms ? ` · first kill ${dayAndTime(b.first_kill_ms)}` : ""}`;
  } else if (b.best_wipe != null) {
    best.append(el("span", "att-wipe", `best ${pct(b.best_wipe)}`));
    best.title = `Your best wipe: the boss got down to ${pct(b.best_wipe)} (${dayAndTime(b.best_wipe_ms)})`;
  }
  row.append(name, tries, record, best);
  return row;
}

function placeBox(p) {
  const box = el("section", "att-place" + (p.kind === RAID_KIND ? " raid" : ""));
  const head = el("div", "att-head");
  head.append(el("span", "run-name", p.name));
  const tag = [p.kind === RAID_KIND ? "Raid" : p.kind, p.mode].filter(Boolean).join(" · ");
  if (tag) head.append(el("span", "run-tag", tag));
  const sum = el("span", "att-sum", plural(p.attempts, "try", "tries"));
  sum.title = `${plural(p.kills, "kill", "kills")} · ${plural(p.wipes, "wipe", "wipes")} · ${clock(p.fight_s)} fighting · last ${dayAndTime(p.last_ms)}`;
  head.append(sum);
  box.append(head);
  const cols = el("div", "att-boss att-cols");
  cols.append(el("span", "", "boss"), el("span", "", "tries"), el("span", "", "✓ ✗"), el("span", "", "best"));
  box.append(cols, ...p.bosses.map(bossRow));
  return box;
}

function draw(force) {
  if (!data) return;
  const key = JSON.stringify([data, character, kind]);
  if (!force && key === lastKey) return;
  lastKey = key;
  const chars = data.characters;
  // You first; else the one with the most tries (the list is sorted that way).
  if (character == null || !chars.some((c) => c.name === character)) {
    const you = chars.find((c) => c.name === data.you);
    character = you ? you.name : chars.length ? chars[0].name : null;
  }
  const tabs = $("characters");
  tabs.replaceChildren(
    ...chars.map((c) => {
      const b = el("button", "", `[${label(c.name)}]`);
      b.type = "button";
      b.setAttribute("role", "tab");
      b.setAttribute("aria-selected", String(c.name === character));
      b.title = `${plural(c.attempts, "try", "tries")}${c.name === data.you ? " · playing now" : ""}${c.name ? "" : " · fights before the meter knew your character"}`;
      b.addEventListener("click", () => {
        character = c.name;
        askClear(false);
        draw(true);
      });
      return b;
    }),
  );
  tabs.hidden = chars.length < 2 && !(chars[0] && chars[0].name === "");
  for (const b of document.querySelectorAll("#kinds [data-kind]")) b.setAttribute("aria-selected", String(b.dataset.kind === kind));

  const c = chars.find((x) => x.name === character);
  const places = c ? c.places.filter((p) => kind === "all" || placeKind(p) === kind) : [];
  $("count").textContent = c ? `[${c.attempts}]` : "";
  const t = places.reduce((a, p) => ({ attempts: a.attempts + p.attempts, kills: a.kills + p.kills, wipes: a.wipes + p.wipes }), { attempts: 0, kills: 0, wipes: 0 });
  $("totals").textContent = places.length
    ? `${label(character)} · ${plural(t.attempts, "try", "tries")} · ${plural(t.kills, "kill", "kills")} · ${plural(t.wipes, "wipe", "wipes")}`
    : "";
  $("list").replaceChildren(...places.map(placeBox));
  $("empty").hidden = places.length > 0;
  $("empty").textContent = !chars.length
    ? "no boss kills or wipes yet: every try at a boss counts here"
    : { raids: "no raid tries yet", dungeons: "no dungeon boss tries yet", sealed: "no sealed dungeon or stronghold boss tries yet", other: "no field boss tries yet", all: "nothing yet" }[kind];
  $("clear").hidden = !c || !$("clear-ask").hidden;
  $("clear").title = c ? `Forget ${label(c.name)}'s attempts (the history keeps its fights)` : "";
}

async function load() {
  try {
    data = await invoke("attempts_list");
    draw(false);
  } catch {
    // Next time, then.
  }
}

for (const b of document.querySelectorAll("#kinds [data-kind]")) {
  b.addEventListener("click", () => {
    kind = b.dataset.kind;
    store("attempts-kind", kind);
    draw(true);
  });
}

// Clearing asks first (it can't be undone), and stops asking after a few seconds.
const CLEAR_ASK_MS = 6000;
let clearAskTimer = null;
function askClear(on) {
  clearTimeout(clearAskTimer);
  $("clear").hidden = on;
  $("hint").hidden = on;
  $("clear-ask").hidden = !on;
  if (on) {
    $("clear-what").textContent = `forget every try ${label(character)} made?`;
    clearAskTimer = setTimeout(() => askClear(false), CLEAR_ASK_MS);
  }
}
$("clear").addEventListener("click", () => askClear(true));
$("clear-no").addEventListener("click", () => askClear(false));
$("clear-yes").addEventListener("click", async () => {
  const who = character;
  askClear(false);
  await invoke("clear_attempts", { character: who }).catch(() => {});
  character = null;
  load();
});

load();
setInterval(() => {
  if (!document.hidden) load();
}, POLL_MS);
