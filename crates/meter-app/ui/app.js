// Aion Meter overlay. Polls the engine twice a second and redraws.
// Player names come from network traffic, so everything is set with textContent,
// never innerHTML.

const tauri = window.__TAURI__;
const invoke = tauri.core.invoke;
const win = tauri.window.getCurrentWindow();

const POLL_MS = 500;

// Colours per class code (first two digits of a player's skill IDs).
const CLASS_COLOURS = {
  11: "#e0a458",
  12: "#e06c62",
  13: "#a58be0",
  14: "#5aa9e6",
  15: "#5fc9a8",
  16: "#e57fb0",
  17: "#c9c46a",
  18: "#6fd08c",
};

const $ = (id) => document.getElementById(id);
const expanded = new Set(); // player IDs whose skill list is open
let everyone = false;
let pinned = true;
const icons = new Map(); // icon key -> data URL, or null while loading / when missing
let editing = false; // pause redraws while a skill name is being typed

function fmt(n) {
  if (n >= 1e6) return (n / 1e6).toFixed(2) + "M";
  if (n >= 1e4) return (n / 1e3).toFixed(1) + "K";
  return Math.round(n).toLocaleString("en-US");
}

function clock(s) {
  const t = Math.floor(s);
  return `${Math.floor(t / 60)}:${String(t % 60).padStart(2, "0")}`;
}

function el(tag, className, text) {
  const e = document.createElement(tag);
  if (className) e.className = className;
  if (text !== undefined) e.textContent = text;
  return e;
}

function skillList(p) {
  const box = el("div", "skills");
  const stats = el("div", "stats");
  stats.append(
    el("span", "", `${fmt(p.damage)} dmg`),
    el("span", "", `${p.hits} hits`),
    el("span", "", `max ${fmt(p.max_hit)}`),
  );
  box.append(stats);
  const head = el("div", "skill head");
  head.append(el("span", "", "skill"), el("span", "", "damage"), el("span", "", "%"), el("span", "", "hits"));
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

function iconFor(key) {
  if (!icons.has(key)) {
    icons.set(key, null);
    invoke("skill_icon", { skill: key })
      .then((url) => icons.set(key, url))
      .catch(() => {});
  }
  return icons.get(key);
}

// Icon + name (or the bare code when unnamed). Hover shows the code; double-click renames.
// Names and icon keys arrive with each snapshot.
function skillLabel(s) {
  const named = s.name;
  const label = el("span", "skill-label");
  const url = s.icon_key ? iconFor(s.icon_key) : null;
  if (url) {
    const img = el("img", "skill-icon");
    img.src = url;
    img.alt = "";
    label.append(img);
  } else {
    label.append(el("span", "skill-icon blank"));
  }
  const name = el("span", "skill-name" + (named ? "" : " code"), named ? s.name : String(s.skill));
  label.append(name);
  label.title = `skill ${s.skill} · max hit ${fmt(s.max)}\nDouble-click to ${named ? "rename" : "name"} it`;
  label.addEventListener("click", (e) => e.stopPropagation());
  label.addEventListener("dblclick", (e) => {
    e.stopPropagation();
    startRename(name, s.skill, named ? s.name : "");
  });
  return label;
}

function startRename(nameEl, skill, current) {
  editing = true;
  const input = el("input", "skill-input");
  input.value = current;
  input.placeholder = String(skill);
  input.maxLength = 60;
  nameEl.replaceWith(input);
  input.focus();
  input.select();
  let done = false;
  const finish = async (save) => {
    if (done) return;
    done = true;
    if (save && input.value.trim() !== current) {
      try {
        await invoke("rename_skill", { skill, name: input.value });
      } catch (err) {
        $("status").className = "error";
        $("status").textContent = String(err);
      }
    }
    editing = false;
    refresh();
  };
  input.addEventListener("keydown", (e) => {
    if (e.key === "Enter") finish(true);
    if (e.key === "Escape") finish(false);
  });
  input.addEventListener("blur", () => finish(true));
  input.addEventListener("click", (e) => e.stopPropagation());
}

function drawRows(enc) {
  const list = $("rows");
  list.replaceChildren();
  const top = enc.players.length ? enc.players[0].dps : 1;
  enc.players.forEach((p, i) => {
    const row = el("li", "row" + (p.is_self ? " self" : ""));
    row.style.setProperty("--class", CLASS_COLOURS[p.class_code] || "#6b7280");
    row.title = p.class_code ? `class ${p.class_code} · entity ${p.id}` : `entity ${p.id}`;

    const line = el("div", "line");
    const fill = el("div", "fill");
    fill.style.width = `${Math.max(2, (p.dps / top) * 100)}%`;
    line.append(
      fill,
      el("span", "rank", String(i + 1)),
      el("span", "name", p.name),
      el("span", "dps", fmt(p.dps)),
      el("span", "share", `${(p.share * 100).toFixed(1)}%`),
    );
    row.append(line);
    if (expanded.has(p.id)) row.append(skillList(p));
    row.addEventListener("click", () => {
      expanded.has(p.id) ? expanded.delete(p.id) : expanded.add(p.id);
      refresh();
    });
    list.append(row);
  });
}

function drawStatus(view) {
  const { status, meter } = view;
  const enc = meter.encounter;
  const dot = $("dot");
  dot.className = "dot " + (enc && enc.active ? "fighting" : status.phase);

  const footer = $("status");
  footer.className = status.phase === "error" ? "error" : status.dropped > 0 ? "warn" : "";
  let text = status.message;
  if (status.dropped > 0) text += ` · ${status.dropped} packets dropped`;
  const you = meter.self_name || (meter.self_id ? `entity ${meter.self_id}` : null);
  if (you && status.phase !== "error") text = `${you} · ${text}`;
  if (view.skills_loaded === 0) {
    text += ` · no skill names: ${view.skills_problem || "run packet-lab import-skills"}`;
    footer.className = "warn";
  }
  footer.textContent = text;
  footer.title = text;
}

function draw(view) {
  drawStatus(view);
  const enc = view.meter.encounter;
  if (!enc) {
    $("fight").textContent = "Aion Meter";
    $("clock").textContent = "";
    $("totals").hidden = true;
    $("rows").replaceChildren();
    $("empty").hidden = false;
    $("empty").textContent =
      view.status.phase === "capturing" || view.status.phase === "replay"
        ? "Waiting for a fight…"
        : "Not capturing yet";
    return;
  }
  $("empty").hidden = true;
  $("fight").textContent = enc.active ? "In combat" : "Last fight";
  $("clock").textContent = clock(enc.duration_s);
  $("totals").hidden = false;
  const more = enc.target_count > 1 ? ` +${enc.target_count - 1}` : "";
  $("target").textContent = (enc.target || "") + more;
  $("total").textContent = `${fmt(enc.total_dps)}/s · ${fmt(enc.total_damage)}`;
  drawRows(enc);
}

async function refresh() {
  if (editing) return;
  try {
    draw(await invoke("snapshot"));
  } catch (e) {
    $("status").className = "error";
    $("status").textContent = String(e);
  }
}

$("reset").addEventListener("click", async () => {
  expanded.clear();
  await invoke("reset");
  refresh();
});

$("scope").addEventListener("click", async () => {
  everyone = !everyone;
  $("scope").textContent = everyone ? "All" : "Mine";
  $("scope").classList.toggle("on", everyone);
  await invoke("set_everyone", { everyone });
  refresh();
});

$("pin").addEventListener("click", async () => {
  pinned = !pinned;
  $("pin").classList.toggle("on", pinned);
  await win.setAlwaysOnTop(pinned);
});

$("min").addEventListener("click", () => win.minimize());
$("close").addEventListener("click", () => win.close());

refresh();
setInterval(refresh, POLL_MS);
