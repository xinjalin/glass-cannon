// Your skills that aren't ready: soonest first, with the time left and charges. Skills
// with a cooldown under 2 s (dodging, basic attacks) are left out.

const POLL_MS = 200;
const SHORTEST_MS = 2000;

function seconds(ms) {
  return ms < 10_000 ? (ms / 1000).toFixed(1) : clock(ms / 1000);
}

async function refresh() {
  let rows = [];
  try {
    rows = (await invoke("cooldowns")).filter((c) => c.duration_ms >= SHORTEST_MS);
  } catch {
    return;
  }
  const list = $("list");
  list.replaceChildren();
  for (const c of rows) {
    const label = c.name || `#${c.skill}`;
    const li = el("li", "cd" + (c.remaining_ms < 3000 ? " soon" : ""));
    const main = el("div", "cd-main");
    const name = el("span", "cd-name", label);
    if (c.charges != null) name.append(el("span", "cd-charges", ` ×${c.charges}`));
    const bar = el("div", "cd-bar");
    const fill = el("div", "cd-fill");
    fill.style.width = `${Math.min(100, (c.remaining_ms / Math.max(1, c.duration_ms)) * 100)}%`;
    bar.append(fill);
    main.append(name, bar);
    li.append(iconFor(c.icon_key, label, refresh), main, el("span", "cd-time", seconds(c.remaining_ms)));
    li.title = `${label}: ${(c.duration_ms / 1000).toFixed(1)} s cooldown` + (c.charges != null ? ` · ${c.charges} charge${c.charges === 1 ? "" : "s"} left` : "");
    list.append(li);
  }
  $("empty").hidden = rows.length > 0;
}

setInterval(() => {
  if (!document.hidden) refresh();
}, POLL_MS);
refresh();
