// Glass Cannon overlay. Polls the engine (twice a second by default; settings → update
// rate) and redraws, or on "live" redraws whenever the app says new data has arrived.
// Player and mob names come from network traffic, so everything is set with
// textContent, never innerHTML.

const tauri = window.__TAURI__;
const invoke = tauri.core.invoke;
const win = tauri.window.getCurrentWindow();

const DEFAULT_POLL_MS = 500;
// The "live" update rate: redraw when new data arrives (the app says so), and once a
// second anyway for the clock, ping and fights that end by going quiet.
const REALTIME = 100;
const HEARTBEAT_MS = 1000;
const SEGMENTS = 12; // blocks in each meter bar
const FIT_ROWS = 5; // the window's default height shows this many players
const DEFAULT_WIDTH = 420; // logical pixels, as in tauri.conf.json

// Whose damage counts; the button cycles through them in this order.
const SCOPES = ["mine", "party", "everyone"];
const SCOPE_LABELS = { mine: "mine", party: "party", everyone: "all" };
const SCOPE_TITLES = {
  mine: "Your fight: targets you hit, with everyone's damage on them",
  party: "Your party: targets any of you hit, and only your party's damage",
  everyone: "Everyone fighting nearby",
};

// What the rows show; the legend's button cycles through them in this order.
const VIEWS = ["damage", "healing", "taken"];
const VIEW_LABELS = { damage: "dmg", healing: "heal", taken: "taken" };
const VIEW_TITLES = {
  damage: "Damage dealt",
  healing: "Healing done (heals over time and self-heals)",
  taken: "Damage taken from mobs",
};
// The words for a row's amount in each view.
const VIEW_UNITS = { damage: "dmg", healing: "healed", taken: "taken" };

const $ = (id) => document.getElementById(id);
const expanded = new Set(); // player IDs whose skill list is open
let pinned = true;
let editing = false; // pause redraws while a name is being typed
let lastView = null;

// What the panel shows: the live meter, the list of past fights, or one past fight.
let mode = "live";
let fightIndex = null;
let fightView = null;

const skillIcons = new Map(); // icon key -> data URL (null while loading / missing)
const classIcons = new Map(); // class code -> data URL (null while loading / missing)

// Icons load in the background; one that arrives redraws the panel (refreshes skip
// redrawing while the fight doesn't change, so it wouldn't show up otherwise).
let iconRedraw = false;
function cached(map, key, command, args) {
  if (!map.has(key)) {
    map.set(key, null);
    invoke(command, args)
      .then((url) => {
        map.set(key, url);
        if (url && !iconRedraw) {
          iconRedraw = true;
          requestAnimationFrame(() => {
            iconRedraw = false;
            if (!editing) redraw();
          });
        }
      })
      .catch(() => {});
  }
  return map.get(key);
}

function fmt(n) {
  if (n >= 1e6) return (n / 1e6).toFixed(2) + "M";
  if (n >= 1e4) return (n / 1e3).toFixed(1) + "K";
  return Math.round(n).toLocaleString("en-US");
}

// Per-second rates: small ones (a little healing) keep a decimal instead of showing 0.
function fmtRate(n) {
  return n > 0 && n < 10 ? n.toFixed(1) : fmt(n);
}

function clock(s) {
  const t = Math.floor(s);
  return `${Math.floor(t / 60)}:${String(t % 60).padStart(2, "0")}`;
}

function timeOfDay(ms) {
  return new Date(ms).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}

// Green for a normal connection, amber when it's getting laggy, red when it's bad.
function pingClass(ms) {
  return ms < 200 ? "good" : ms < 300 ? "high" : "bad";
}

const minimal = () => document.documentElement.classList.contains("minimal");

// Meter blocks, row heights and the gaps between them are whole device pixels, so they're
// the same size at any font size or display scaling (fractional sizes rounded unevenly).
let meterKey = "";
function snapSizes() {
  const fs = parseFloat(getComputedStyle(document.documentElement).fontSize);
  const dpr = window.devicePixelRatio || 1;
  const key = `${fs}@${dpr}`;
  if (key === meterKey) return;
  meterKey = key;
  const px = (em, min) => Math.max(min, Math.round(em * fs * dpr)) / dpr;
  const seg = px(0.26, 2);
  const gap = px(0.14, 1);
  const style = document.documentElement.style;
  style.setProperty("--seg", `${seg}px`);
  style.setProperty("--seg-gap", `${gap}px`);
  style.setProperty("--seg-h", `${px(0.65, 4)}px`);
  style.setProperty("--meter-w", `${SEGMENTS * seg + (SEGMENTS - 1) * gap}px`);
  style.setProperty("--row-h", `${px(1.95, 16)}px`);
  style.setProperty("--row-gap", `${px(0.15, 1)}px`);
}

function el(tag, className, text) {
  const e = document.createElement(tag);
  if (className) e.className = className;
  if (text !== undefined) e.textContent = text;
  return e;
}

function showError(err) {
  $("status").hidden = false;
  $("status").className = "error";
  $("status").textContent = String(err);
}

// ---------------------------------------------------------------- skills

function skillList(p) {
  const box = el("div", "skills");
  const stats = el("div", "stats");
  stats.append(
    el("span", "", `${fmt(p.damage)} ${VIEW_UNITS[meterView()]}`),
    el("span", "", `${p.hits} ${meterView() === "healing" ? "ticks" : "hits"}`),
    el("span", "", `max ${fmt(p.max_hit)}`),
  );
  box.append(stats);
  const head = el("div", "skill head");
  const amount = { damage: "damage", healing: "healing", taken: "taken" }[meterView()];
  head.append(el("span", "", meterView() === "taken" ? "hit by" : "skill"), el("span", "", amount), el("span", "", "%"), el("span", "", "hits"));
  box.append(head);
  for (const s of p.skills) {
    const r = el("div", "skill");
    r.append(
      skillLabel(s),
      el("span", "", fmt(s.damage)),
      el("span", "", (s.share * 100).toFixed(1)),
      el("span", "", String(s.hits)),
    );
    box.append(r);
  }
  return box;
}

// Icon + name (or the bare code when unnamed). Hover shows the code; double-click renames.
function skillLabel(s) {
  const label = el("span", "skill-label");
  const url = s.icon_key ? cached(skillIcons, s.icon_key, "skill_icon", { skill: s.icon_key }) : null;
  if (url) {
    const img = el("img", "skill-icon");
    img.src = url;
    img.alt = "";
    label.append(img);
  } else {
    label.append(el("span", "skill-icon blank"));
  }
  const name = el("span", "skill-name" + (s.name ? "" : " code"), s.name || String(s.skill));
  label.append(name);
  label.title = `skill ${s.skill} · max hit ${fmt(s.max)}\nDouble-click to ${s.name ? "rename" : "name"} it`;
  label.addEventListener("click", (e) => e.stopPropagation());
  label.addEventListener("dblclick", (e) => {
    e.stopPropagation();
    startRename(name, String(s.skill), s.name || "", (value) =>
      invoke("rename_skill", { skill: s.skill, name: value }),
    );
  });
  return label;
}

function startRename(nameEl, placeholder, current, save) {
  editing = true;
  const input = el("input", "name-input");
  input.value = current;
  input.placeholder = placeholder;
  input.maxLength = 60;
  nameEl.replaceWith(input);
  input.focus();
  input.select();
  let done = false;
  const finish = async (commit) => {
    if (done) return;
    done = true;
    if (input.isConnected) input.replaceWith(nameEl);
    if (commit && input.value.trim() !== current) {
      try {
        await save(input.value);
      } catch (err) {
        showError(err);
      }
    }
    editing = false;
    if (mode === "fight") await openFight(fightIndex);
    refresh();
  };
  input.addEventListener("keydown", (e) => {
    if (e.key === "Enter") finish(true);
    if (e.key === "Escape") finish(false);
  });
  input.addEventListener("blur", () => finish(true));
  input.addEventListener("click", (e) => e.stopPropagation());
}

// ---------------------------------------------------------------- players

function classBadge(p) {
  const url = p.class_code ? cached(classIcons, p.class_code, "class_icon", { code: p.class_code }) : null;
  if (url) {
    const img = el("img", "class-icon");
    img.src = url;
    img.alt = "";
    return img;
  }
  return el("span", "class-icon blank");
}

// The rows for the chosen view (older saved fights have no healing or taken rows).
function rowsOf(enc) {
  const view = meterView();
  return (view === "damage" ? enc.players : enc[view]) || [];
}

// Rows are kept between redraws (one per player) and only their changed parts updated,
// so a live fight redrawing ten times a second doesn't rebuild the list each time.
const rowCache = new Map(); // player ID -> the row's elements and what they show

function rowFor(id) {
  let r = rowCache.get(id);
  if (r) return r;
  const row = el("li", "row");
  const line = el("div", "line");
  const rank = el("span", "rank");
  const who = el("span", "who");
  const badge = el("span", "class-icon blank");
  const name = el("span", "name");
  who.append(badge, name);
  const meter = el("span", "meter");
  const segs = Array.from({ length: SEGMENTS }, () => el("span", "seg"));
  meter.append(...segs);
  const dps = el("span", "dps");
  const share = el("span", "share");
  line.append(rank, who, meter, dps, share);
  row.append(line);
  row.addEventListener("click", () => toggleRow(id));
  // Right-click: the full analysis of this fight, on this player (again: closes it).
  row.addEventListener("contextmenu", (e) => {
    e.preventDefault();
    openAnalysis(id);
  });
  r = { row, rank, badge, name, segs, dps, share, skills: null, shown: {} };
  rowCache.set(id, r);
  return r;
}

// Sets r.shown[key] = value, calling `apply` only when it changed.
function update(r, key, value, apply) {
  if (r.shown[key] === value) return;
  r.shown[key] = value;
  apply(value);
}

function drawRows(enc) {
  const list = $("rows");
  const rows = rowsOf(enc);
  const top = rows.length ? rows[0].dps || 1 : 1;
  const ids = new Set();
  rows.forEach((p, i) => {
    ids.add(p.id);
    const r = rowFor(p.id);
    update(r, "class", "row" + (p.is_self ? " self" : "") + (p.class_code ? ` c${p.class_code}` : ""), (v) => (r.row.className = v));
    update(r, "title", [p.class_name, `entity ${p.id}`].filter(Boolean).join(" · "), (v) => (r.row.title = v));
    update(r, "rank", String(i + 1), (v) => (r.rank.textContent = v));
    const badge = classBadge(p);
    update(r, "badge", badge.src || "", () => {
      r.badge.replaceWith(badge);
      r.badge = badge;
    });
    update(r, "name", p.name, (v) => (r.name.textContent = v));
    const lit = Math.max(1, Math.round((p.dps / top) * SEGMENTS));
    update(r, "lit", lit, (v) => r.segs.forEach((seg, s) => seg.classList.toggle("on", s < v)));
    update(r, "dps", fmtRate(p.dps), (v) => (r.dps.textContent = v));
    update(r, "share", `${(p.share * 100).toFixed(0)}%`, (v) => (r.share.textContent = v));
    // The skill list, when open: rebuilt only when something in it changed.
    const open = expanded.has(p.id);
    const skillsKey = open ? `${meterView()}|${p.damage}|${p.hits}|${p.max_hit}|${JSON.stringify(p.skills)}` : "";
    update(r, "skills", skillsKey, () => {
      if (r.skills) r.skills.remove();
      r.skills = open ? skillList(p) : null;
      if (r.skills) r.row.append(r.skills);
    });
    if (list.children[i] !== r.row) list.insertBefore(r.row, list.children[i] || null);
  });
  for (const [id, r] of rowCache) {
    if (ids.has(id)) continue;
    r.row.remove();
    rowCache.delete(id);
  }
}

// Opens or closes a player's skill list. The live meter only sends the skills of open
// rows, so opening one asks for them.
function toggleRow(id) {
  if (expanded.has(id)) {
    expanded.delete(id);
    redraw();
  } else {
    expanded.add(id);
    if (mode === "live") refreshSoon();
    else redraw();
  }
}

function drawTotals(enc) {
  $("totals").hidden = false;
  const target = $("target");
  target.replaceChildren();
  if (enc.target_boss) target.append(el("span", "boss", "BOSS"));
  const more = enc.target_count > 1 ? ` +${enc.target_count - 1}` : "";
  const name = el("span", "target-name", (enc.target || "") + more);
  target.append(name);
  target.title = enc.target_type ? `mob type ${enc.target_type} · double-click to name it` : "";
  const current = enc.target && !enc.target.startsWith("Mob ") && !enc.target.startsWith("#") ? enc.target : "";
  target.ondblclick = enc.target_type
    ? () =>
        startRename(name, `mob ${enc.target_type}`, current, (value) =>
          invoke("rename_npc", { typeId: enc.target_type, name: value }),
        )
    : null;
  const view = meterView();
  const total = view === "damage" ? enc.total_damage : (view === "healing" ? enc.total_healing : enc.total_taken) || 0;
  $("total").textContent = `${fmtRate(total / Math.max(1, enc.duration_s))}/s · ${fmt(total)}${view === "damage" ? "" : ` ${VIEW_UNITS[view]}`}`;
}

const meterView = () => (lastView && lastView.meter_view) || "damage";

function drawViewSwitch() {
  const view = meterView();
  const next = VIEWS[(VIEWS.indexOf(view) + 1) % VIEWS.length];
  for (const b of [$("view"), $("f-view")]) {
    b.textContent = VIEW_LABELS[view];
    b.classList.toggle("on", view !== "damage");
    b.title = `${VIEW_TITLES[view]}\nClick for ${VIEW_TITLES[next].split(" (")[0].toLowerCase()}`;
  }
}

// ---------------------------------------------------------------- history

async function showList() {
  mode = "list";
  fightIndex = null;
  fightView = null;
  await drawList();
}

// Dungeon runs shown open (by when they started). The newest run starts open.
const openRuns = new Set();
let runsSeen = false;

async function drawList() {
  const { fights, runs } = await invoke("history_list");
  const list = $("fights");
  list.replaceChildren();
  if (!runsSeen && runs.length) {
    runsSeen = true;
    const newest = fights.find((f) => f.run != null);
    if (newest) openRuns.add(runs[newest.run].started_ms);
  }
  // A run's fights come together (newest first): a header, then its fights if it's open.
  let shownRun = null;
  fights.forEach((f, i) => {
    if (f.run != null) {
      const run = runs[f.run];
      if (shownRun !== f.run) {
        shownRun = f.run;
        list.append(runItem(run));
      }
      if (openRuns.has(run.started_ms)) list.append(fightItem(f, i, true));
    } else {
      list.append(fightItem(f, i, false));
    }
  });
  $("legend-text").textContent = "history";
  $("clock").textContent = fights.length ? `[${fights.length}]` : "";
  $("empty").hidden = fights.length > 0;
  $("empty").textContent = "no finished fights yet";
}

function fightItem(f, i, inRun) {
  const item = el("li", "fight-item" + (inRun ? " in-run" : ""));
  const what = el("span", "fight-what");
  if (f.target_boss) what.append(el("span", "boss", "BOSS"));
  what.append(el("span", "fight-target", f.target || "unknown target"));
  const players = `${f.players} ${f.players === 1 ? "player" : "players"}`;
  // A dungeon fight on its own (not part of a run) says where it was.
  const where = !inRun && f.dungeon ? `${f.dungeon.name} · ` : "";
  const meta = el("span", "fight-meta", `${where}${timeOfDay(f.ended_ms)} · ${clock(f.duration_s)} · ${players}`);
  const text = el("span", "fight-text");
  text.append(what, meta);
  const dps = el("span", "fight-dps", f.my_dps != null ? fmt(f.my_dps) : fmt(f.total_dps));
  dps.title = f.my_dps != null ? "your DPS" : "total DPS";
  item.append(el("span", "fight-no", `${i + 1}.`), text, dps);
  item.addEventListener("click", () => openFight(f.index));
  return item;
}

// A dungeon run: where, its mode, when, how long from first pull to last kill, and its
// total damage. Click to show or hide its fights.
function runItem(run) {
  const open = openRuns.has(run.started_ms);
  const item = el("li", "run-item" + (open ? " open" : ""));
  const text = el("span", "fight-text");
  const what = el("span", "fight-what");
  what.append(el("span", "run-name", run.place.name));
  const tag = [run.place.kind, run.place.mode].filter(Boolean).join(" · ");
  if (tag) what.append(el("span", "run-tag", tag));
  const fights = `${run.fights} fights${run.bosses ? ` · ${run.bosses} ${run.bosses === 1 ? "boss" : "bosses"}` : ""}`;
  const meta = el("span", "fight-meta", `${timeOfDay(run.started_ms)} · ${clock((run.ended_ms - run.started_ms) / 1000)} · ${fights} · ${run.players}p`);
  text.append(what, meta);
  const total = el("span", "fight-dps", fmt(run.total_damage));
  total.title = run.my_dps != null ? `total damage · your DPS ${fmt(run.my_dps)}` : "total damage";
  item.append(el("span", "run-chevron", open ? "▾" : "▸"), text, total);
  item.title = open ? "Hide this run's fights" : "Show this run's fights";
  item.addEventListener("click", () => {
    if (open) openRuns.delete(run.started_ms);
    else openRuns.add(run.started_ms);
    drawList();
  });
  return item;
}

async function openFight(index) {
  fightView = await invoke("history_fight", { index });
  if (!fightView) return showList();
  mode = "fight";
  fightIndex = index;
  redraw();
}

// ---------------------------------------------------------------- drawing

function drawScope(meter) {
  const scope = meter.scope;
  const next = SCOPES[(SCOPES.indexOf(scope) + 1) % SCOPES.length];
  const b = $("scope");
  b.textContent = SCOPE_LABELS[scope] || scope;
  b.classList.toggle("on", scope !== "mine");
  let title = SCOPE_TITLES[scope] || "";
  if (scope === "party") {
    title += meter.party_size > 1 ? ` (${meter.party_size} people)` : " (not in a party: just you)";
  }
  b.title = `${title}\nClick for ${SCOPE_LABELS[next]}`;
}

// How tall the window must be to show `rows` players with the current font and layout:
// lays out a hidden copy of the panel at its natural height.
function heightFor(rows) {
  const panel = $("panel");
  const copy = panel.cloneNode(true);
  Object.assign(copy.style, {
    position: "absolute",
    visibility: "hidden",
    left: "0",
    top: "0",
    width: `${panel.offsetWidth}px`,
    height: "auto",
  });
  const part = (id) => copy.querySelector(`#${id}`);
  part("box").style.flex = "none";
  const list = part("rows");
  Object.assign(list.style, { flex: "none", overflow: "visible" });
  list.hidden = false;
  list.replaceChildren(
    ...Array.from({ length: rows }, () => {
      const row = el("li", "row");
      const line = el("div", "line");
      line.append(el("span", "rank", "0"));
      row.append(line);
      return row;
    }),
  );
  part("totals").hidden = false;
  part("target").textContent = "x";
  part("total").textContent = "x";
  for (const id of ["fights", "empty", "status"]) part(id).hidden = true;
  document.body.append(copy);
  const height = Math.ceil(copy.getBoundingClientRect().height);
  copy.remove();
  return height;
}

// Fits the window to FIT_ROWS players: on start (only ever growing a window you made
// taller), and exactly whenever the font size, time bar or minimal mode changes.
let fitFor = null;
let resetWidth = null; // set by "reset appearance": the width goes back to the default too
function fitHeight(a) {
  const key = `${a.font_size}|${a.font}|${a.info_bar}|${a.minimal}`;
  if (key === fitFor) return;
  const exact = fitFor !== null;
  fitFor = key;
  const width = resetWidth;
  resetWidth = null;
  requestAnimationFrame(() =>
    invoke("fit_overlay", { height: heightFor(FIT_ROWS), exact, width }).catch(() => {}),
  );
}

function drawStatus(view) {
  const { status, meter } = view;
  const enc = meter.encounter;
  drawScope(meter);
  $("dot").className = "dot " + (enc && enc.active ? "fighting" : status.phase);
  $("dot").title = status.message;

  const ms = status.ping_ms;
  const ping = $("ping");
  ping.hidden = ms == null;
  ping.textContent = ms == null ? "" : `${ms}ms`;
  ping.className = ms == null ? "" : pingClass(ms);
  const infoPing = $("info-ping");
  infoPing.textContent = ms == null ? "ping –" : `ping ${ms}ms`;
  infoPing.className = ms == null ? "" : pingClass(ms);

  // The bottom line only appears when something needs attention.
  let problem = "";
  let kind = "";
  if (status.phase === "error") {
    problem = status.message;
    kind = "error";
  } else if (view.skills_loaded === 0) {
    problem = "no game data: extract the whole release zip (the gamedata folder goes next to the exe)";
  } else if (status.dropped > 0) {
    problem = `${status.dropped} packets dropped: numbers may be low`;
  }
  const line = $("status");
  line.hidden = !problem;
  line.className = kind;
  line.textContent = problem;
  line.title = problem;
}

function phaseText(view) {
  if (!view) return "";
  const enc = view.meter.encounter;
  if (enc && enc.active) return "in combat";
  return {
    idle: "idle",
    starting: "starting",
    waiting: "waiting for the game",
    capturing: enc ? "last fight" : "ready",
    replay: "replay",
    error: "error",
  }[view.status.phase] || "";
}

// An open analysis window shows the fight shown here: a saved one, or the live one.
let followed = null;
function followFight() {
  const fight = mode === "fight" ? fightIndex : mode === "live" ? null : undefined;
  if (fight === undefined || fight === followed) return;
  followed = fight;
  invoke("follow_fight", { fight }).catch(() => {});
}

function redraw() {
  followFight();
  drawViewSwitch();
  const live = mode === "live";
  $("back").hidden = live;
  $("f-back").hidden = live;
  $("fight").textContent = phaseText(lastView);
  $("rows").hidden = mode === "list";
  $("fights").hidden = mode !== "list";
  $("history").classList.toggle("on", !live);
  if (lastView) drawStatus(lastView);

  if (mode === "list") {
    $("totals").hidden = true;
    return;
  }
  const enc = live ? lastView && lastView.meter.encounter : fightView;
  if (!enc) {
    $("legend-text").textContent = "fight";
    $("clock").textContent = "";
    $("totals").hidden = true;
    $("rows").replaceChildren();
    rowCache.clear();
    $("empty").hidden = false;
    const phase = lastView ? lastView.status.phase : "starting";
    $("empty").textContent =
      phase === "capturing" || phase === "replay"
        ? "waiting for a fight…"
        : phase === "error"
          ? "not capturing (see below)"
          : "starting…";
    return;
  }
  $("legend-text").textContent = live ? "fight" : `fight · ${timeOfDay(enc.end_us / 1000)}`;
  $("clock").textContent = clock(enc.duration_s);
  drawTotals(enc);
  drawRows(enc);
  const none = rowsOf(enc).length === 0;
  $("empty").hidden = !none;
  if (none) $("empty").textContent = meterView() === "healing" ? "no healing in this fight" : "no damage taken in this fight";
}

// The last snapshot as text: when a new one is the same (no fight going on), the panel
// isn't rebuilt.
let lastKey = "";
let drawnMode = "";

async function refresh() {
  if (editing) return;
  try {
    const view = await invoke("snapshot", { expanded: [...expanded] });
    const key = JSON.stringify(view);
    const same = key === lastKey && drawnMode === `${mode}/${fightIndex}`;
    lastKey = key;
    drawnMode = `${mode}/${fightIndex}`;
    lastView = view;
    if (same) return;
    applyAppearance(lastView.appearance);
    snapSizes();
    fitHeight(lastView.appearance);
    drawLock(lastView.locked);
    if (mode === "list") await drawList();
    redraw();
  } catch (e) {
    showError(e);
  }
}

// One refresh at a time: a request while one is running runs once it's done.
let refreshing = false;
let refreshAgain = false;
async function refreshSoon() {
  if (refreshing) {
    refreshAgain = true;
    return;
  }
  refreshing = true;
  await refresh();
  refreshing = false;
  if (refreshAgain) {
    refreshAgain = false;
    refreshSoon();
  }
}

// Polls at the rate chosen in the settings, read from each snapshot.
async function poll() {
  await refreshSoon();
  const ms = (lastView && lastView.refresh_ms) || DEFAULT_POLL_MS;
  setTimeout(poll, ms === REALTIME ? HEARTBEAT_MS : ms);
}

// "live": the app says when new data has arrived.
tauri.event.listen("meter-changed", () => {
  if (lastView && lastView.refresh_ms === REALTIME) refreshSoon();
});

// A theme or font change in the settings shows straight away, not on the next refresh.
tauri.event.listen("appearance", (e) => {
  if (lastView) lastView.appearance = e.payload;
  applyAppearance(e.payload);
  snapSizes();
  fitHeight(e.payload);
});

// ---------------------------------------------------------------- controls

async function goBack() {
  if (mode === "fight") await showList();
  else mode = "live";
  redraw();
}
$("back").addEventListener("click", goBack);
$("f-back").addEventListener("click", goBack);

// The analysis window, on the fight shown here (a saved one in history mode).
function openAnalysis(player) {
  const fight = mode === "fight" ? fightIndex : null;
  invoke("open_analysis", { fight, player }).catch(showError);
}
$("analysis").addEventListener("click", () => openAnalysis(null));

$("history").addEventListener("click", async () => {
  if (mode === "live") await showList();
  else mode = "live";
  redraw();
});

// Damage → healing → taken.
async function cycleView() {
  const next = VIEWS[(VIEWS.indexOf(meterView()) + 1) % VIEWS.length];
  await invoke("set_meter_view", { view: next });
  if (lastView) lastView.meter_view = next;
  expanded.clear();
  redraw();
}
$("view").addEventListener("click", cycleView);
$("f-view").addEventListener("click", cycleView);

// Ends the live fight now instead of waiting for it to go quiet.
async function endFight() {
  expanded.clear();
  mode = "live";
  await invoke("end_fight");
  refresh();
}
$("end-fight").addEventListener("click", endFight);
$("f-end-fight").addEventListener("click", endFight);

$("reset").addEventListener("click", async () => {
  expanded.clear();
  mode = "live";
  await invoke("reset");
  refresh();
});

$("scope").addEventListener("click", async () => {
  const now = lastView ? lastView.meter.scope : "mine";
  await invoke("set_scope", { scope: SCOPES[(SCOPES.indexOf(now) + 1) % SCOPES.length] });
  refresh();
});

$("settings").addEventListener("click", () => invoke("toggle_settings"));
$("f-settings").addEventListener("click", () => invoke("toggle_settings"));

// Minimal mode has no title bar, so the meter drags from anywhere. Dragging only starts
// once the pointer moves, so clicks (open a row) and double-clicks (rename) still work.
let press = null;
document.addEventListener("mousedown", (e) => {
  const draggable = e.button === 0 && minimal() && !e.target.closest("button, input");
  press = draggable ? { x: e.screenX, y: e.screenY } : null;
});
document.addEventListener("mousemove", (e) => {
  if (!press) return;
  if (!(e.buttons & 1)) {
    press = null;
  } else if (Math.hypot(e.screenX - press.x, e.screenY - press.y) >= 4) {
    press = null;
    win.startDragging();
  }
});
document.addEventListener("mouseup", () => (press = null));

function tick() {
  $("info-time").textContent = new Date().toLocaleTimeString([], {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });
}
tick();
setInterval(tick, 1000);

$("pin").addEventListener("click", async () => {
  pinned = !pinned;
  $("pin").classList.toggle("on", pinned);
  await win.setAlwaysOnTop(pinned);
});

// ---------------------------------------------------------------- lock (click-through)

let locked = false;
function drawLock(on) {
  locked = on;
  document.documentElement.classList.toggle("locked", on);
  $("lock").title = on
    ? "Locked: clicks go through to the game. Click to unlock"
    : "Lock: let clicks through to the game (this button stays clickable)";
  reportLockRect();
}

// The app keeps the lock button clickable while the rest lets clicks through, so it
// needs to know where the button is (in device pixels from the page's top left).
let lockRectKey = "";
function reportLockRect() {
  const r = $("lock").getBoundingClientRect();
  const dpr = window.devicePixelRatio || 1;
  const rect = { x: r.left * dpr, y: r.top * dpr, width: r.width * dpr, height: r.height * dpr };
  const key = Object.values(rect).map(Math.round).join(",");
  if (key === lockRectKey) return;
  lockRectKey = key;
  invoke("lock_button_rect", { rect }).catch(() => {});
}
new ResizeObserver(reportLockRect).observe(document.documentElement);

$("lock").addEventListener("click", async () => {
  await invoke("set_locked", { locked: !locked });
  refresh();
});

$("min").addEventListener("click", () => win.minimize());
$("close").addEventListener("click", () => win.close());

// "Reset appearance" in the settings: refit at the default size on the next refresh.
tauri.event.listen("reset-size", () => {
  resetWidth = DEFAULT_WIDTH;
  fitFor = "reset";
});

// The settings window's "history" opens this window straight on the list.
tauri.event.listen("show-history", async () => {
  await showList();
  redraw();
});

poll();
