// The fight history in its own window: dungeon runs (open one for its fights), single
// fights, bosses only, PvP sessions, or training dummy runs. Clicking a fight shows it on
// the meter; right-clicking opens its analysis. Dummy runs: click two to compare them.

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
  // Your best against this boss: highest DPS, fastest kill.
  for (const [on, text, title] of [
    [f.best_dps, "best dps", "Your best DPS against this boss"],
    [f.fastest, "fastest", "Your fastest kill of this boss"],
  ]) {
    if (!on) continue;
    const tag = el("span", "best-tag", ` · ★ ${text}`);
    tag.title = title;
    meta.append(tag);
  }
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
  if (run.clear_s != null) {
    const clear = el("span", "run-clear" + (run.best_clear ? " best" : ""), `✓ ${clock(run.clear_s)}`);
    if (run.best_clear) clear.title = "Your fastest clear of this dungeon and mode";
    when.append(clear);
  }
  const bosses = run.bosses ? ` · ${run.bosses} ${run.bosses === 1 ? "boss" : "bosses"}` : "";
  const meta = el("span", "fight-meta", `${clock(run.fight_s)} · ${run.fights} fights${bosses} · ${run.players}p`);
  text.append(el("span", "run-name", run.place.name), when, meta);
  // Stage splits: each stage's time, your fastest of this dungeon and mode marked.
  if (run.stages.length) {
    const splits = el("span", "run-splits");
    run.stages.forEach((s, k) => {
      const best = run.best_stages[k];
      const split = el("span", "split" + (best ? " best" : ""), `${k + 1} ${clock(s)}`);
      split.title = `Stage ${k + 1}: ${clock(s)}` + (best ? " · your fastest" : "");
      splits.append(split);
    });
    text.append(splits);
  }
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

// ---- Dummy runs: dummy mode's settings, each label's average, the runs, and two of them
// compared (click two; the older is the baseline).
let picked = [];

function dummyControls(d) {
  const li = el("li", "dummy-controls");
  const mode = el("button", "", d.on ? "[dummy mode: on]" : "[dummy mode: off]");
  mode.type = "button";
  mode.title = "Hitting a Training Scarecrow times a run and shows your pace against your best (the meter shows your damage only)";
  mode.addEventListener("click", () => invoke("set_dummy_mode", { on: !d.on }).then(load).catch(() => {}));
  const lengths = el("span", "dummy-lengths");
  lengths.append(el("span", "note", "run"));
  for (const s of d.lengths) {
    const b = el("button", "", `[${s < 60 ? `${s}s` : `${s / 60}m`}]`);
    b.type = "button";
    b.setAttribute("aria-selected", String(s === d.run_s));
    b.title = `Runs of ${s} seconds from your first hit`;
    b.addEventListener("click", () => invoke("set_dummy_run_length", { seconds: s }).then(load).catch(() => {}));
    lengths.append(b);
  }
  const label = el("button", "", d.label ? `[label: ${d.label}]` : "[label: none]");
  label.type = "button";
  label.title = "The label new runs get (a gear set, a rotation…), to compare runs by label";
  label.addEventListener("click", () => {
    const given = window.prompt("Label for your next dummy runs (empty for none):", d.label || "");
    if (given != null) invoke("set_dummy_label", { label: given }).then(load).catch(() => {});
  });
  li.append(mode, lengths, label);
  return li;
}

// Each label's average DPS, over the clean runs of the length chosen.
function dummyLabels(d) {
  const by = new Map();
  for (const r of d.runs.filter((r) => r.run_s === d.run_s && r.clean)) {
    const k = r.label || "no label";
    const v = by.get(k) || { n: 0, sum: 0, best: 0 };
    v.n++;
    v.sum += r.dps;
    v.best = Math.max(v.best, r.dps);
    by.set(k, v);
  }
  if (by.size < 2) return null;
  const li = el("li", "dummy-labels");
  for (const [k, v] of [...by].sort((a, b) => b[1].sum / b[1].n - a[1].sum / a[1].n)) {
    const tag = el("span", "dummy-label-avg", `${k} ${fmt(v.sum / v.n)}`);
    tag.title = `${v.n} clean ${v.n === 1 ? "run" : "runs"} of ${d.run_s} s · best ${fmt(v.best)}`;
    li.append(tag);
  }
  return li;
}

const signed = (x) => `${x >= 0 ? "+" : ""}${x.toFixed(1)}%`;

function dummyComparison(c, a, b) {
  const li = el("li", "dummy-compare");
  const head = el("div", "dc-head");
  head.append(
    el("span", "", `${b.label || dayAndTime(b.ended_ms)} vs ${a.label || dayAndTime(a.ended_ms)}: `),
    el("span", c.dps_pct >= 0 ? "dc-up" : "dc-down", `${signed(c.dps_pct)} DPS`),
    el("span", "fight-meta", ` · ${signed(c.luck_free_pct)} at the same crit rate · `),
    el("span", "dc-verdict", c.verdict),
  );
  head.title = `Crit rate ${(c.crit_rate_a * 100).toFixed(1)}% then ${(c.crit_rate_b * 100).toFixed(1)}%. The second figure takes the difference in crits out: what's left is real.`;
  li.append(head);
  if (!a.clean || !b.clean) li.append(el("div", "fight-meta dc-warn", "One of them isn't clean (someone buffed you or debuffed the dummy): it doesn't compare fairly."));
  const table = el("div", "dc-skills");
  c.skills.slice(0, 8).forEach(([skill, x, y], i) => {
    const name = c.skill_names[i] || `#${skill}`;
    const diff = x ? (y / x - 1) * 100 : null;
    table.append(
      el("span", "dc-skill", name),
      el("span", "fight-meta", fmt(x)),
      el("span", "fight-meta", fmt(y)),
      el("span", diff == null ? "fight-meta" : diff >= 0 ? "dc-up" : "dc-down", diff == null ? "new" : signed(diff)),
    );
  });
  li.append(table);
  return li;
}

function dummyItem(r, i) {
  const on = picked.includes(r.ended_ms);
  const item = el("li", "fight-item" + (on ? " selected" : ""));
  const what = el("span", "fight-what");
  if (r.label) what.append(el("span", "run-tag", r.label));
  const helpers = r.helpers.slice(0, 2).join(", ") + (r.helpers.length > 2 ? "…" : "");
  what.append(el("span", r.clean ? "clean-tag" : "buffed-tag", r.clean ? "clean" : `buffed by ${helpers}`));
  const crit = r.hits ? Math.round((r.crits / r.hits) * 100) : 0;
  const back = r.hits ? Math.round((r.back / r.hits) * 100) : 0;
  const meta = el("span", "fight-meta", `${dayAndTime(r.ended_ms)} · ${r.run_s} s · crit ${crit}% · back ${back}% · ${r.casts} casts`);
  const text = el("span", "fight-text");
  text.append(what, meta);
  const dps = el("span", "fight-dps", fmt(r.dps));
  dps.title = `${fmt(r.damage)} damage in ${r.run_s} s`;
  item.append(el("span", "fight-no", `${i + 1}.`), text, dps);
  item.title = "Click two runs to compare them · double-click to label it · right-click to delete it";
  item.addEventListener("click", () => {
    picked = on ? picked.filter((x) => x !== r.ended_ms) : [...picked, r.ended_ms].slice(-2);
    draw(true);
  });
  item.addEventListener("dblclick", () => {
    const given = window.prompt("Label for this run (empty for none):", r.label || "");
    if (given != null) invoke("label_dummy_run", { id: r.ended_ms, label: given }).then(load).catch(() => {});
  });
  item.addEventListener("contextmenu", (e) => {
    e.preventDefault();
    if (window.confirm(`Delete the run of ${dayAndTime(r.ended_ms)} (${fmt(r.dps)} DPS)?`)) {
      picked = picked.filter((x) => x !== r.ended_ms);
      invoke("delete_dummy_run", { id: r.ended_ms }).then(load).catch(() => {});
    }
  });
  return item;
}

let comparing = null;
function drawDummy(list) {
  const d = data.dummy;
  picked = picked.filter((id) => d.runs.some((r) => r.ended_ms === id));
  list.append(dummyControls(d));
  const labels = dummyLabels(d);
  if (labels) list.append(labels);
  if (picked.length === 2) {
    const [a, b] = picked.map((id) => d.runs.find((r) => r.ended_ms === id)).sort((x, y) => x.ended_ms - y.ended_ms);
    const key = `${a.ended_ms}|${b.ended_ms}`;
    if (comparing && comparing.key === key) list.append(dummyComparison(comparing.c, a, b));
    else
      invoke("compare_dummy_runs", { a: a.ended_ms, b: b.ended_ms })
        .then((c) => {
          comparing = { key, c };
          draw(true);
        })
        .catch(() => {});
  }
  d.runs.forEach((r, i) => list.append(dummyItem(r, i)));
  return d.runs.length;
}

async function load() {
  try {
    const [h, p, d] = await Promise.all([invoke("history_list"), invoke("pvp_list"), invoke("dummy_runs")]);
    data = { fights: h.fights, runs: h.runs, pvp: p, dummy: d };
  } catch {
    return;
  }
  draw(false);
}

function draw(force) {
  if (!data) return;
  const key = JSON.stringify([tab, data.fights.length, data.runs.length, data.pvp.sessions.length, data.pvp.current, selected, [...openRuns], [...openPvp], data.dummy, picked]);
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
  if (tab === "dummy") {
    shown = drawDummy(list);
    $("count").textContent = data.dummy.runs.length ? `[${data.dummy.runs.length}]` : "";
  } else if (tab === "pvp") {
    if (pvp.current) list.append(...pvpItem(pvp.current, true));
    for (const s of pvp.sessions) list.append(...pvpItem(s, false));
    shown = pvp.sessions.length + (pvp.current ? 1 : 0);
    // No count over an empty list (it says there are none).
    $("count").textContent = pvp.sessions.length ? `[${pvp.sessions.length}]` : "";
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
    $("count").textContent = fights.length ? `[${fights.length}]` : "";
  }
  const empty = {
    all: "no finished fights yet",
    runs: "no dungeon runs yet",
    bosses: "no boss fights yet",
    pvp: "no PvP yet: fights with players outside your party show here",
    dummy: "no dummy runs yet: hit a Training Scarecrow for a whole run",
  };
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
