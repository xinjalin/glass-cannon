// Your party's long cooldowns (you included): each member's skills of a minute or more,
// and party buffs of 30 s or more, that they've used this session. Ready ones say so.
// Grouped by player, or (a setting) by class, then skill, then player.

const POLL_MS = 250;

const classIcons = new Map();
function classIcon(code) {
  if (code != null && !classIcons.has(code)) {
    classIcons.set(code, null);
    invoke("class_icon", { code })
      .then((url) => classIcons.set(code, url))
      .catch(() => {});
  }
  const url = code != null ? classIcons.get(code) : null;
  if (url) {
    const img = el("img", "icon class");
    img.src = url;
    img.alt = "";
    return img;
  }
  return el("span", "icon blank");
}

function seconds(ms) {
  return ms < 10_000 ? (ms / 1000).toFixed(1) : clock(ms / 1000);
}

// A cooldown's bar and time: `row` gets "ready" when it's up.
function timer(row, s) {
  const on = s.remaining_ms > 0;
  if (!on) row.classList.add("ready");
  const bar = el("div", "cd-bar");
  const fill = el("div", "cd-fill");
  fill.style.width = on ? `${Math.min(100, (s.remaining_ms / Math.max(1, s.duration_ms)) * 100)}%` : "0%";
  bar.append(fill);
  return [bar, el("span", "cd-time", on ? seconds(s.remaining_ms) : "ready")];
}

function skillName(s) {
  const label = s.name || `#${s.skill}`;
  const name = el("span", "cd-name", label);
  if (s.party) name.append(el("span", "party-mark", " ◆"));
  return [label, name];
}

function tip(label, s) {
  return `${label}: ${clock(s.duration_ms / 1000)} cooldown` + (s.party ? " · affects the party" : "");
}

// Each player, then their skills.
function byPlayer(members) {
  return members.map((m) => {
    const block = el("section", "member" + (m.is_self ? " self" : ""));
    const head = el("div", "member-head");
    head.append(classIcon(m.class_code), el("span", "member-name", m.name));
    const ready = m.skills.filter((s) => s.remaining_ms === 0).length;
    head.append(el("span", "note", `${ready}/${m.skills.length} ready`));
    block.append(head);
    for (const s of m.skills) {
      const [label, name] = skillName(s);
      const row = el("div", "cd");
      const main = el("div", "cd-main");
      const [bar, time] = timer(row, s);
      main.append(name, bar);
      row.append(iconFor(s.icon_key, label, refresh), main, time);
      row.title = tip(label, s);
      block.append(row);
    }
    return block;
  });
}

// Each class, then its skills, then who has them: ready first, then soonest back.
function byClass(members) {
  const classes = new Map();
  for (const m of members) {
    const key = m.class_code ?? -1;
    if (!classes.has(key)) classes.set(key, { code: m.class_code, name: m.class_name || "other", skills: new Map(), players: 0 });
    const c = classes.get(key);
    c.players++;
    for (const s of m.skills) {
      const base = s.skill - (s.skill % 10_000);
      if (!c.skills.has(base)) c.skills.set(base, { skill: s, who: [] });
      const g = c.skills.get(base);
      g.skill = s.duration_ms > g.skill.duration_ms ? s : g.skill;
      g.who.push({ ...s, player: m.name, is_self: m.is_self });
    }
  }
  return [...classes.values()]
    .sort((a, b) => a.name.localeCompare(b.name))
    .map((c) => {
      const block = el("section", "member");
      const head = el("div", "member-head");
      head.append(classIcon(c.code), el("span", "member-name", c.name));
      head.append(el("span", "note", c.players === 1 ? "1 player" : `${c.players} players`));
      block.append(head);
      const skills = [...c.skills.values()].sort((a, b) => b.skill.duration_ms - a.skill.duration_ms || a.skill.skill - b.skill.skill);
      for (const { skill, who } of skills) {
        const [label, name] = skillName(skill);
        const row = el("div", "cd skill-head");
        const ready = who.filter((s) => s.remaining_ms === 0).length;
        row.append(iconFor(skill.icon_key, label, refresh), name, el("span", "note", `${ready}/${who.length} ready`));
        row.title = tip(label, skill);
        block.append(row);
        who.sort((a, b) => a.remaining_ms - b.remaining_ms || a.player.localeCompare(b.player));
        for (const s of who) {
          const line = el("div", "cd who" + (s.is_self ? " self" : ""));
          const [bar, time] = timer(line, s);
          const main = el("div", "cd-main");
          main.append(el("span", "cd-name", s.player), bar);
          line.append(main, time);
          line.title = `${s.player} · ${tip(label, s)}`;
          block.append(line);
        }
      }
      return block;
    });
}

async function refresh() {
  let party = { by_class: false, members: [] };
  try {
    party = await invoke("party_cooldowns");
  } catch {
    return;
  }
  const { members } = party;
  $("members").replaceChildren(...(party.by_class ? byClass(members) : byPlayer(members)));
  $("empty").hidden = members.length > 0;
}

setInterval(() => {
  if (!document.hidden) refresh();
}, POLL_MS);
refresh();
