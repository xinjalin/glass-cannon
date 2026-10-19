// Glass Cannon player detail: full skill list, rotation, buffs-per-skill and buff timeline.
// Everything is set with textContent (names come from network traffic), never innerHTML.

const tauri = window.__TAURI__;
const invoke = tauri.core.invoke;
const win = tauri.window.getCurrentWindow();

const $ = (id) => document.getElementById(id);

function el(tag, className, text) {
  const e = document.createElement(tag);
  if (className) e.className = className;
  if (text !== undefined) e.textContent = text;
  return e;
}

function fmt(n) {
  if (n >= 1e6) return (n / 1e6).toFixed(2) + "M";
  if (n >= 1e4) return (n / 1e3).toFixed(1) + "K";
  return Math.round(n).toLocaleString("en-US");
}

function clock(us) {
  const s = Math.max(0, Math.floor(us / 1e6));
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
}

// Skill code -> display name, from the annotated full list.
function namesBySkill(detail) {
  const map = new Map();
  for (const s of detail.skills) map.set(s.skill, s.name || String(s.skill));
  return map;
}

function label(names, code) {
  return names.get(code) || String(code);
}

function drawSkills(detail) {
  const names = namesBySkill(detail);
  const table = $("skills-table");
  table.replaceChildren();
  if (!detail.skills.length) {
    table.append(el("tr", null, el("td", "empty", "no skills yet")));
    return;
  }
  const head = el("tr");
  head.append(el("th", null, "skill"), el("th", "num", "damage"), el("th", "num", "%"), el("th", "num", "hits"), el("th", "num", "max"));
  table.append(head);
  for (const s of detail.skills) {
    const r = el("tr");
    r.append(
      el("td", null, label(names, s.skill)),
      el("td", "num", fmt(s.damage)),
      el("td", "num", (s.share * 100).toFixed(1)),
      el("td", "num", String(s.hits)),
      el("td", "num", fmt(s.max)),
    );
    table.append(r);
  }
}

function drawRotation(detail) {
  const names = namesBySkill(detail);
  const box = $("rotation-strip");
  box.replaceChildren();
  if (!detail.rotation.length) {
    box.append(el("div", "empty", "no casts yet"));
    return;
  }
  const t0 = detail.rotation[0].time_us;
  for (const e of detail.rotation) {
    const r = el("div", "cast");
    const suffix = e.is_dot ? " · dot" : e.hit > 1 ? ` · hit ${e.hit}` : "";
    r.append(
      el("span", "t", clock(e.time_us - t0)),
      el("span", "skill", label(names, e.skill) + suffix),
      el("span", "dmg", fmt(e.damage)),
    );
    box.append(r);
  }
}

function drawBuffs(detail) {
  const names = namesBySkill(detail);
  const box = $("buffs-list");
  box.replaceChildren();
  const skills = detail.buffs_per_skill.skills;
  if (!skills.length) {
    box.append(el("div", "empty", "no casts yet"));
    return;
  }
  for (const sb of skills) {
    box.append(el("div", "skill-head", `${label(names, sb.skill)} · ${sb.casts} casts · ${fmt(sb.damage)} dmg`));
    const uptime = (b) => `${label(names, b.id.skill)} ${b.percent}%`;
    if (sb.always.length) box.append(el("div", "buffs", `always · ${sb.always.map(uptime).join(" · ")}`));
    if (sb.buffs.length) box.append(el("div", "buffs", `buffs · ${sb.buffs.map(uptime).join(" · ")}`));
    if (sb.debuffs.length) box.append(el("div", "debuffs", `debuffs · ${sb.debuffs.map(uptime).join(" · ")}`));
    if (!sb.always.length && !sb.buffs.length && !sb.debuffs.length) {
      box.append(el("div", "faint", "no buffs decoded yet"));
    }
  }
}

function drawBuffTimeline(detail) {
  const names = namesBySkill(detail);
  const box = $("bufftimeline-list");
  box.replaceChildren();
  if (!detail.buff_spans.length) {
    box.append(el("div", "empty", "buffs aren't decoded yet (see docs/protocol.md)"));
    return;
  }
  const t0 = detail.rotation[0] ? detail.rotation[0].time_us : detail.buff_spans[0].start_us;
  for (const s of detail.buff_spans) {
    box.append(el("div", "span", `${clock(s.start_us - t0)}–${clock(s.end_us - t0)} · ${label(names, s.skill)}`));
  }
}

async function refresh() {
  try {
    const d = await invoke("detail_state");
    if (!d) return;
    if (typeof applyAppearance === "function" && d.appearance) applyAppearance(d.appearance);
    $("title").textContent = d.name;
    drawSkills(d);
    drawRotation(d);
    drawBuffs(d);
    drawBuffTimeline(d);
  } catch (e) {
    $("title").textContent = String(e);
  }
}

$("close").addEventListener("click", () => win.hide());
document.addEventListener("keydown", (e) => {
  if (e.key === "Escape") win.hide();
});

refresh();
setInterval(refresh, 500);
