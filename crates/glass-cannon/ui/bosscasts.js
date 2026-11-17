// The boss casts window: every boss by content, dungeon and name, with the casts the meter
// has seen it make. Each cast can be renamed, and set to show or not in the boss warnings
// and the boss timers (auto: the general rule). Names are set with textContent, never
// innerHTML.

const POLL_MS = 5000;
// The content types' tab names.
const KIND_LABELS = { Sanctuary: "raids", "Field boss": "field bosses", "Dimensional Invasion": "invasions", "Sealed Dungeon": "sealed dungeons" };

let data = null;
let lastKey = "";
// A cast being renamed: its input stays put while the list refreshes.
let editing = null;

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
let kind = stored("boss-casts-kind") || "";
let show = stored("boss-casts-show") || "fought";
let query = "";

const kindLabel = (k) => (KIND_LABELS[k] || k).toLowerCase();
const secs = (ms) => `${(ms / 1000).toFixed(1)} s`;
const changed = (c) => c.renamed || c.warn != null || c.timer != null;

// "[auto] [on] [off]" for one setting of a cast; auto says what it does now.
function choice(label, value, auto, set, title) {
  const box = el("span", "bc-choice");
  box.title = title;
  box.append(el("span", "bc-label", label));
  for (const [v, text] of [[null, `auto ${auto ? "✓" : "–"}`], [true, "on"], [false, "off"]]) {
    const b = el("button", "", `[${text}]`);
    b.type = "button";
    b.setAttribute("aria-pressed", String(value === v));
    if (v === null) b.title = auto ? "Auto: shown (a long cast, or one that marks the ground)" : "Auto: not shown (short, and marks no ground)";
    b.addEventListener("click", async () => {
      await set(v).catch(() => {});
      await load(true);
    });
    box.append(b);
  }
  return box;
}

function castRow(c) {
  const li = el("li", "bc-cast" + (changed(c) ? " set" : ""));
  const what = el("div", "bm-what");
  if (editing === c.skill) {
    const input = el("input", "bc-rename");
    input.value = c.name || "";
    input.placeholder = "a name of your own (empty: the game's)";
    input.addEventListener("keydown", async (e) => {
      if (e.key === "Enter") {
        await invoke("rename_skill", { skill: c.skill, name: input.value.trim() }).catch(() => {});
        editing = null;
        await load(true);
      } else if (e.key === "Escape") {
        e.stopPropagation();
        editing = null;
        draw(true);
      }
    });
    input.addEventListener("blur", () => {
      if (editing === c.skill) {
        editing = null;
        draw(true);
      }
    });
    what.append(input);
    requestAnimationFrame(() => input.focus());
  } else {
    // Most boss moves have no name in the game's own text (it calls them "Attack").
    const name = el("button", "bc-name" + (c.name ? "" : " unnamed"), c.name || "unnamed");
    name.type = "button";
    name.title = c.renamed ? "Your name for it · click to change it" : c.name ? "Click to rename it" : 'The game only calls this move "Attack" · click to name it';
    name.addEventListener("click", () => {
      editing = c.skill;
      draw(true);
    });
    what.append(name);
  }
  const facts = [c.cast_ms ? `${secs(c.cast_ms)} cast` : "instant", c.lands_ms ? `lands ${secs(c.lands_ms)} after` : null, `seen ${c.seen}× in ${c.fights} ${c.fights === 1 ? "fight" : "fights"}`, `#${c.skill}`]
    .filter(Boolean)
    .join(" · ");
  what.append(el("div", "bm-sub note", facts));
  li.append(what);
  const sets = el("div", "bc-sets");
  sets.append(
    choice("warn", c.warn, c.auto_warn, (v) => invoke("set_boss_skill_alert", { skill: c.skill, alert: v }), "In the boss warnings: as it looks (auto), always, or never"),
    choice("timer", c.timer, c.auto_timer ?? c.warn ?? c.auto_warn, (v) => invoke("set_boss_skill_timer", { skill: c.skill, timer: v }), "In the boss timers (its countdown before it comes): as its warning goes (auto), always, or never"),
  );
  li.append(sets);
  return li;
}

function matches(boss, c) {
  if (show === "changed" && !(c && changed(c))) return false;
  if (!query) return true;
  return boss.name.toLowerCase().includes(query) || (c && (c.name || "").toLowerCase().includes(query)) || (c && String(c.skill).includes(query));
}

function draw(force) {
  if (!data) return;
  const key = JSON.stringify([data, kind, show, query, editing]);
  if (!force && key === lastKey) return;
  lastKey = key;
  const kinds = data.kinds;
  if (!kinds.some((k) => k.kind === kind)) kind = kinds.length ? kinds[0].kind : "";
  const fought = (b) => b.casts.length > 0;
  $("kinds").replaceChildren(
    ...kinds.map((k) => {
      const n = k.places.reduce((s, p) => s + p.bosses.filter(fought).length, 0);
      const b = el("button", "", `[${kindLabel(k.kind)}${n ? ` ${n}` : ""}]`);
      b.type = "button";
      b.setAttribute("role", "tab");
      b.setAttribute("aria-selected", String(k.kind === kind));
      b.title = n ? `${n} ${n === 1 ? "boss" : "bosses"} with casts seen` : "No casts seen yet";
      b.addEventListener("click", () => {
        kind = k.kind;
        store("boss-casts-kind", kind);
        editing = null;
        draw(true);
      });
      return b;
    }),
  );
  for (const b of document.querySelectorAll("#show [data-show]")) b.setAttribute("aria-selected", String(b.dataset.show === show));
  const all = kinds.flatMap((k) => k.places.flatMap((p) => p.bosses));
  const casts = all.flatMap((b) => b.casts);
  $("count").textContent = `· ${all.filter(fought).length} bosses · ${casts.length} casts · ${casts.filter(changed).length} set`;
  const k = kinds.find((x) => x.kind === kind);
  const boxes = [];
  for (const place of k ? k.places : []) {
    const bosses = [];
    for (const boss of place.bosses) {
      if (show !== "all" && !fought(boss)) continue;
      // The likeliest mechanics first: the longest casts (or marking the ground), then the
      // most seen.
      const list = boss.casts
        .filter((c) => matches(boss, c))
        .sort((a, b) => Math.max(b.cast_ms, b.lands_ms ? 2500 : 0) - Math.max(a.cast_ms, a.lands_ms ? 2500 : 0) || b.seen - a.seen);
      if (!list.length && !(show === "all" && matches(boss, null))) continue;
      const sec = el("div", "bc-boss");
      const head = el("div", "bc-boss-head");
      head.append(el("span", "bc-boss-name", boss.name));
      if (boss.modes.length) head.append(el("span", "note", boss.modes.join(" · ")));
      sec.append(head);
      if (list.length) {
        const ul = el("ul", "bm-list");
        ul.append(...list.map(castRow));
        sec.append(ul);
      } else {
        sec.append(el("p", "note bc-none", "no casts seen yet: fight it with the meter running"));
      }
      bosses.push(sec);
    }
    if (!bosses.length) continue;
    const box = el("section", "ck-box bm-page");
    box.append(el("h2", "ck-head", place.name));
    box.append(...bosses);
    boxes.push(box);
  }
  $("list").replaceChildren(...boxes);
  $("empty").hidden = boxes.length > 0;
  $("empty").textContent =
    query || show === "changed" ? "nothing matches" : show === "fought" ? `no ${kindLabel(kind)} bosses fought with the meter running yet · [all bosses] lists them` : "no bosses here";
}

async function load(force) {
  try {
    data = await invoke("boss_casts_setup");
    // Not while a name is being typed: the input would be redrawn under you.
    if (editing === null || force) draw(force);
  } catch {
    // Next time, then.
  }
}

// Sharing a mapping: your names and choices to a file, or someone's file over yours.
function shareNote(text) {
  $("share-note").textContent = text;
}
$("export").addEventListener("click", async () => {
  try {
    const path = await invoke("export_boss_casts");
    shareNote(`saved to ${path}`);
  } catch (e) {
    shareNote(String(e));
  }
});
$("import").addEventListener("click", () => $("import-file").click());
$("import-file").addEventListener("change", async () => {
  const file = $("import-file").files[0];
  $("import-file").value = "";
  if (!file) return;
  try {
    const got = await invoke("import_boss_casts", { json: await file.text() });
    shareNote(`took in ${got.names} name${got.names === 1 ? "" : "s"}, ${got.warns} warning and ${got.timers} timer choice${got.timers === 1 ? "" : "s"}`);
    await load(true);
  } catch (e) {
    shareNote(String(e));
  }
});

for (const b of document.querySelectorAll("#show [data-show]")) {
  b.addEventListener("click", () => {
    show = b.dataset.show;
    store("boss-casts-show", show);
    draw(true);
  });
}
$("search").addEventListener("input", () => {
  query = $("search").value.trim().toLowerCase();
  draw(true);
});

load();
setInterval(() => {
  if (!document.hidden && editing === null) load();
}, POLL_MS);
