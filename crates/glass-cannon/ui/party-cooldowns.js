// Your party's long cooldowns (you included): each member's skills of a minute or more,
// and party buffs of 30 s or more, that they've used this session. Ready ones say so.

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

async function refresh() {
  let members = [];
  try {
    members = await invoke("party_cooldowns");
  } catch {
    return;
  }
  const box = $("members");
  box.replaceChildren();
  for (const m of members) {
    const block = el("section", "member" + (m.is_self ? " self" : ""));
    const head = el("div", "member-head");
    head.append(classIcon(m.class_code), el("span", "member-name", m.name));
    const ready = m.skills.filter((s) => s.remaining_ms === 0).length;
    head.append(el("span", "note", `${ready}/${m.skills.length} ready`));
    block.append(head);
    for (const s of m.skills) {
      const label = s.name || `#${s.skill}`;
      const on = s.remaining_ms > 0;
      const row = el("div", "cd" + (on ? "" : " ready"));
      const main = el("div", "cd-main");
      const name = el("span", "cd-name", label);
      if (s.party) name.append(el("span", "party-mark", " ◆"));
      const bar = el("div", "cd-bar");
      const fill = el("div", "cd-fill");
      fill.style.width = on ? `${Math.min(100, (s.remaining_ms / Math.max(1, s.duration_ms)) * 100)}%` : "0%";
      bar.append(fill);
      main.append(name, bar);
      row.append(iconFor(s.icon_key, label, refresh), main, el("span", "cd-time", on ? seconds(s.remaining_ms) : "ready"));
      row.title = `${label}: ${clock(s.duration_ms / 1000)} cooldown` + (s.party ? " · affects the party" : "");
      block.append(row);
    }
    box.append(block);
  }
  $("empty").hidden = members.length > 0;
}

setInterval(() => {
  if (!document.hidden) refresh();
}, POLL_MS);
refresh();
