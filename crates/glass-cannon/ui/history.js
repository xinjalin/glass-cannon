// The fight history in its own window: dungeon runs (open one for its fights), single
// fights, bosses only, or PvP sessions. Clicking a fight shows it on the meter;
// right-clicking opens its analysis.

const POLL_MS = 2000;

let tab = "all";
let selected = null;
let lastKey = "";
// Dungeon runs shown open (by when they started). The newest starts open.
const openRuns = new Set();
let runsSeen = false;
// PvP sessions shown open (by when they started).
const openPvp = new Set();

function endTag(end, deaths) {
  const parts = [];
  if (end === "kill") parts.push("✓ kill");
  if (end === "wipe") parts.push("✗ wipe");
  if (deaths) parts.push(`☠${deaths > 1 ? deaths : ""}`);
  if (!parts.length) return null;
  return el("span", `end-tag ${end || ""}`, ` · ${parts.join(" ")}`);
}

function fightItem(f, i, inRun) {
  const item = el("li", "fight-item" + (inRun ? " in-run" : "") + (selected === f.index ? " selected" : ""));
  const what = el("span", "fight-what");
  if (f.target_boss) what.append(el("span", "boss", "BOSS"));
  what.append(el("span", "fight-target", f.target || "unknown target"));
  const players = `${f.players} ${f.players === 1 ? "player" : "players"}`;
  const where = !inRun && f.dungeon ? `${f.dungeon.name} · ` : "";
  const meta = el("span", "fight-meta", `${where}${dayAndTime(f.ended_ms)} · ${clock(f.duration_s)} · ${players}`);
  const end = endTag(f.end, f.deaths);
  if (end) meta.append(end);
  const text = el("span", "fight-text");
  text.append(what, meta);
  const dps = el("span", "fight-dps", f.my_dps != null ? fmt(f.my_dps) : fmt(f.total_dps));
  dps.title = f.my_dps != null ? "your DPS" : "total DPS";
  item.append(el("span", "fight-no", `${i + 1}.`), text, dps);
  item.addEventListener("click", () => {
    selected = f.index;
    invoke("show_fight", { index: f.index }).catch(() => {});
    draw(true);
  });
  item.addEventListener("contextmenu", (e) => {
    e.preventDefault();
    invoke("open_analysis", { fight: f.index, run: null, player: null }).catch(() => {});
  });
  return item;
}

function runItem(run) {
  const open = openRuns.has(run.started_ms);
  const item = el("li", "run-item" + (open ? " open" : ""));
  const text = el("span", "fight-text");
  const when = el("span", "fight-what");
  const tag = [run.place.kind, run.place.mode].filter(Boolean).join(" · ");
  if (tag) when.append(el("span", "run-tag", tag));
  when.append(el("span", "fight-meta", dayAndTime(run.started_ms)));
  if (run.clear_s != null) when.append(el("span", "run-clear", `✓ ${clock(run.clear_s)}`));
  const bosses = run.bosses ? ` · ${run.bosses} ${run.bosses === 1 ? "boss" : "bosses"}` : "";
  const meta = el("span", "fight-meta", `${clock(run.fight_s)} · ${run.fights} fights${bosses} · ${run.players}p`);
  text.append(el("span", "run-name", run.place.name), when, meta);
  item.append(el("span", "run-chevron", open ? "▾" : "▸"), text, el("span", "fight-dps", fmt(run.total_damage)));
  item.title = "Click to show or hide its fights · right-click for the run's analysis";
  item.addEventListener("click", () => {
    if (open) openRuns.delete(run.started_ms);
    else openRuns.add(run.started_ms);
    draw(true);
  });
  item.addEventListener("contextmenu", (e) => {
    e.preventDefault();
    invoke("open_analysis", { fight: null, run: [run.first, run.last], player: null }).catch(() => {});
  });
  return item;
}

function pvpItem(s, live) {
  const open = openPvp.has(s.started_us);
  const item = el("li", "run-item pvp" + (open ? " open" : ""));
  const text = el("span", "fight-text");
  const secs = (s.ended_us - s.started_us) / 1e6;
  const who = s.opponents.slice(0, 3).map((o) => o.name).join(", ") + (s.opponents.length > 3 ? ` +${s.opponents.length - 3}` : "");
  const head = el("span", "fight-what");
  head.append(el("span", "run-tag", live ? "PvP · now" : "PvP"), el("span", "fight-target", who || "?"));
  const kd = el("span", "kd", `${s.kills} / ${s.deaths}`);
  kd.title = "Kills / deaths";
  const meta = el("span", "fight-meta", `${dayAndTime(s.started_us / 1000)} · ${clock(secs)} · dealt ${fmt(s.dealt)} · taken ${fmt(s.taken)}`);
  text.append(head, meta);
  item.append(el("span", "run-chevron", open ? "▾" : "▸"), text, kd);
  if (s.last_killer) item.title = `Last killed by ${s.last_killer}`;
  item.addEventListener("click", () => {
    if (open) openPvp.delete(s.started_us);
    else openPvp.add(s.started_us);
    draw(true);
  });
  const out = [item];
  if (open) {
    for (const o of s.opponents) {
      const row = el("li", "fight-item in-run opponent");
      const t = el("span", "fight-text");
      const name = el("span", "fight-what");
      name.append(el("span", "fight-target", o.name));
      if (o.deaths) name.append(el("span", "end-tag kill", ` · ☠${o.deaths > 1 ? o.deaths : ""}${o.kills ? ` (${o.kills} yours)` : ""}`));
      t.append(name, el("span", "fight-meta", `dealt ${fmt(o.dealt)} · taken ${fmt(o.taken)} · burst ${fmt(o.burst)}`));
      row.append(el("span", "fight-no", ""), t, el("span", "fight-dps", fmt(o.dealt)));
      out.push(row);
    }
  }
  return out;
}

let data = null;

async function load() {
  try {
    const [h, p] = await Promise.all([invoke("history_list"), invoke("pvp_list")]);
    data = { fights: h.fights, runs: h.runs, pvp: p };
  } catch {
    return;
  }
  draw(false);
}

function draw(force) {
  if (!data) return;
  const key = JSON.stringify([tab, data.fights.length, data.runs.length, data.pvp.sessions.length, data.pvp.current, selected, [...openRuns], [...openPvp]]);
  if (!force && key === lastKey) return;
  lastKey = key;
  for (const b of document.querySelectorAll("#tabs [data-tab]")) b.setAttribute("aria-selected", String(b.dataset.tab === tab));
  const { fights, runs, pvp } = data;
  if (!runsSeen && runs.length) {
    runsSeen = true;
    const newest = fights.find((f) => f.run != null);
    if (newest) openRuns.add(runs[newest.run].started_ms);
  }
  const list = $("list");
  list.replaceChildren();
  let shown = 0;
  if (tab === "pvp") {
    if (pvp.current) list.append(...pvpItem(pvp.current, true));
    for (const s of pvp.sessions) list.append(...pvpItem(s, false));
    shown = pvp.sessions.length + (pvp.current ? 1 : 0);
    $("count").textContent = `[${pvp.sessions.length}]`;
  } else {
    let shownRun = null;
    fights.forEach((f, i) => {
      if (tab === "bosses") {
        if (f.target_boss) {
          list.append(fightItem(f, i, false));
          shown++;
        }
        return;
      }
      if (f.run != null) {
        const run = runs[f.run];
        if (shownRun !== f.run) {
          shownRun = f.run;
          list.append(runItem(run));
          shown++;
        }
        if (openRuns.has(run.started_ms)) list.append(fightItem(f, i, true));
      } else if (tab === "all") {
        list.append(fightItem(f, i, false));
        shown++;
      }
    });
    $("count").textContent = `[${fights.length}]`;
  }
  const empty = { all: "no finished fights yet", runs: "no dungeon runs yet", bosses: "no boss fights yet", pvp: "no PvP yet: fights with players outside your party show here" };
  $("empty").hidden = shown > 0;
  $("empty").textContent = empty[tab];
}

// Clearing asks first (it can't be undone), and stops asking after a few seconds.
const CLEAR_ASK_MS = 6000;
let clearAskTimer = null;
function askClear(on) {
  clearTimeout(clearAskTimer);
  $("clear").hidden = on;
  $("hint").hidden = on;
  $("clear-ask").hidden = !on;
  if (on) clearAskTimer = setTimeout(() => askClear(false), CLEAR_ASK_MS);
}
$("clear").addEventListener("click", () => askClear(true));
$("clear-no").addEventListener("click", () => askClear(false));
$("clear-yes").addEventListener("click", async () => {
  askClear(false);
  await invoke("clear_history").catch(() => {});
  openRuns.clear();
  openPvp.clear();
  selected = null;
  load();
});

for (const b of document.querySelectorAll("#tabs [data-tab]")) {
  b.addEventListener("click", () => {
    tab = b.dataset.tab;
    draw(true);
  });
}

load();
setInterval(() => {
  if (!document.hidden) load();
}, POLL_MS);
