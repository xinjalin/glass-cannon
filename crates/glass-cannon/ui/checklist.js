// The checklist window: each character's Odyle energy and its weekly sources (the Kina buy
// at the trade guild from the game; the weekly conversions ticked by hand), how much of
// each is done this week, and when the week starts again. Character names are set with
// textContent, never innerHTML.

const POLL_MS = 3000;

let data = null;
let character = null;
let lastKey = "";

const label = (name) => name || "unknown";

// "in 3d 4h", "in 5h 10m", "in 12m".
function until(ms) {
  const m = Math.max(0, Math.round((ms - Date.now()) / 60000));
  const d = Math.floor(m / 1440);
  const h = Math.floor((m % 1440) / 60);
  if (d) return `in ${d}d ${h}h`;
  if (h) return `in ${h}h ${m % 60}m`;
  return `in ${m}m`;
}

function sourceRow(c, s) {
  const li = el("li", "ck-source" + (s.done != null && s.done >= s.max ? " done" : ""));
  const top = el("div", "ck-top");
  top.append(el("span", "ck-mark", s.done != null && s.done >= s.max ? "✓" : "○"));
  top.append(el("span", "ck-label", s.label));
  const count = el("span", "ck-count", s.done == null ? "?" : `${s.done}/${s.max}`);
  top.append(count);
  if (s.auto) {
    const tag = el("span", "ck-tag", "game");
    tag.title = s.seen_ms ? `Read from the game ${dayAndTime(s.seen_ms)}` : "Read from the game: log in, or open a shop, with the meter open";
    top.append(tag);
  } else {
    // Ticked by hand: − and +.
    for (const [text, delta, title] of [
      ["−", -1, "One fewer"],
      ["+", 1, "One more done"],
    ]) {
      const b = el("button", "ck-step", text);
      b.type = "button";
      b.title = title;
      b.disabled = delta < 0 ? !s.done : s.done >= s.max;
      b.addEventListener("click", async () => {
        await invoke("step_checklist", { character: c.name, key: s.key, delta }).catch(() => {});
        load();
      });
      top.append(b);
    }
  }
  li.append(top);
  const bar = el("div", "ck-bar");
  const fill = el("span", "ck-fill");
  fill.style.width = `${s.done == null ? 0 : Math.min(1, s.done / s.max) * 100}%`;
  bar.append(fill);
  li.append(bar);
  if (s.auto && s.done == null) li.append(el("p", "note ck-note", "not read yet: log in, or open a shop, with the meter open"));
  return li;
}

function draw(force) {
  if (!data) return;
  const key = JSON.stringify([data, character]);
  if (!force && key === lastKey) return;
  lastKey = key;
  const chars = data.characters;
  if (character == null || !chars.some((c) => c.name === character)) {
    const you = chars.find((c) => c.name === data.you);
    character = you ? you.name : chars.length ? chars[0].name : null;
  }
  $("characters").replaceChildren(
    ...chars.map((c) => {
      const b = el("button", "", `[${label(c.name)}]`);
      b.type = "button";
      b.setAttribute("role", "tab");
      b.setAttribute("aria-selected", String(c.name === character));
      if (c.name === data.you) b.title = "playing now";
      b.addEventListener("click", () => {
        character = c.name;
        draw(true);
      });
      return b;
    }),
  );
  $("characters").hidden = chars.length < 2;
  const c = chars.find((x) => x.name === character);
  $("empty").hidden = !!c;
  $("empty").textContent = "log in with the meter open: your Odyle energy and weekly buys show here";
  for (const box of document.querySelectorAll(".ck-box")) box.hidden = !c;
  $("week").textContent = `· resets ${until(data.next_reset_ms)} (Wednesday ${timeOfDay(data.next_reset_ms)})`;
  if (!c) {
    $("count").textContent = "";
    return;
  }
  const energy = $("energy");
  if (c.energy) {
    const [basic, charged] = c.energy;
    energy.replaceChildren(
      el("span", "ck-energy-basic", fmt(basic)),
      el("span", "note", " basic · "),
      el("span", "ck-energy-charged", fmt(charged)),
      el("span", "note", ` charged${c.energy_ms ? ` · read ${dayAndTime(c.energy_ms)}` : ""}`),
    );
  } else {
    energy.replaceChildren(el("span", "note", "not read yet: log in with the meter open"));
  }
  $("sources").replaceChildren(...c.sources.map((s) => sourceRow(c, s)));
  const done = c.sources.filter((s) => s.done != null && s.done >= s.max).length;
  $("count").textContent = `[${done}/${c.sources.length}]`;
  $("count").title = "Weekly sources done this week";
}

async function load() {
  try {
    data = await invoke("checklist");
    draw(false);
  } catch {
    // Next time, then.
  }
}

load();
setInterval(() => {
  if (!document.hidden) load();
}, POLL_MS);
