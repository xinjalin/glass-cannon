// The farming widget's settings (farm.rs → farm_setup), beside it while the overlay's
// arranged: which parts show, what the rates go by, the run's reset, and which items are
// listed. Each change shows on the widget at once.

const POLL_MS = 2000;
let view = null;
let tab = "show";
let found = [];
let query = "";

function press(b, on) {
  b.setAttribute("aria-pressed", String(!!on));
}
function toggle(b, on) {
  press(b, on);
  b.textContent = on ? "[on]" : "[off]";
}

// Saves a change (the app tells the widget), and takes the settings as they are now.
async function change(cmd, args) {
  await invoke(cmd, args).catch(() => {});
  await load();
}

function numbers(r) {
  const parts = [];
  if (r.gained) parts.push(`+${r.gained.toLocaleString("en-US")} this run`);
  if (r.have != null) parts.push(`you have ${r.have.toLocaleString("en-US")}`);
  if (r.spent) parts.push(`spent or used ${r.spent.toLocaleString("en-US")}`);
  return parts.join(" · ");
}

// An item to list or hide: its name, what the run has of it, and whether it's listed.
function choice(r) {
  const b = el("button", `tp-item${r.pinned ? " on" : ""}`);
  b.type = "button";
  b.setAttribute("aria-pressed", String(r.pinned));
  b.title = r.pinned ? `${r.name} is listed · click to hide it` : `Always list ${r.name}`;
  const text = el("span", "tp-text");
  text.append(el("span", "tp-name" + (r.grade ? ` grade-${r.grade.toLowerCase()} fs-grade` : ""), r.name));
  const meta = numbers(r);
  if (meta) text.append(el("span", "tp-meta", meta));
  b.append(text, el("span", "tp-state", r.pinned ? "✓" : "+"));
  b.addEventListener("click", async () => {
    await change("set_farm_pinned", { item: r.item, on: !r.pinned });
    if (query) search();
  });
  return b;
}

const none = (text) => el("span", "bs-note", text);

function draw() {
  if (!view) return;
  for (const b of document.querySelectorAll(".bs-tabs [data-tab]")) b.setAttribute("aria-selected", String(b.dataset.tab === tab));
  for (const s of document.querySelectorAll(".bs-tab")) s.hidden = s.dataset.tab !== tab;
  for (const t of document.querySelectorAll(".bs-toggle[data-show]")) toggle(t, view.show[t.dataset.show]);
  toggle($("fs-all"), view.all);
  for (const b of $("fs-per").children) press(b, view.per === b.dataset.v);
  for (const b of $("fs-window").children) press(b, String(view.window_min) === b.dataset.v);
  // With the rate hidden, what it goes by changes nothing.
  for (const id of ["fs-per", "fs-window"]) $(id).closest(".bs-line").classList.toggle("off", !view.show.rate);
  setText($("fs-run"), view.elapsed_s == null ? "no gains yet" : `going for ${clock(view.elapsed_s)}`);
  $("fs-pinned").replaceChildren(...(view.pinned.length ? view.pinned.map(choice) : [none("nothing listed: pick items below")]));
  $("fs-others").replaceChildren(...(view.others.length ? view.others.map(choice) : [none(view.elapsed_s == null ? "nothing yet" : "nothing but the items listed")]));
  $("fs-found").replaceChildren(...(query.trim().length < 2 ? [] : found.length ? found.map(choice) : [none("no item by that name")]));
}

async function load() {
  try {
    view = await invoke("farm_setup");
  } catch {
    // Next time, then.
  }
  draw();
}

async function search() {
  const q = query;
  const got = q.trim().length < 2 ? [] : await invoke("farm_search", { query: q }).catch(() => []);
  // Typed on since: that search's answer is the one to show.
  if (q !== query) return;
  found = got || [];
  draw();
}

for (const b of document.querySelectorAll(".bs-tabs [data-tab]")) {
  b.addEventListener("click", () => {
    tab = b.dataset.tab;
    draw();
  });
}
for (const t of document.querySelectorAll(".bs-toggle[data-show]")) {
  t.addEventListener("click", () => view && change("set_farm_show", { part: t.dataset.show, on: !view.show[t.dataset.show] }));
}
$("fs-all").addEventListener("click", () => view && change("set_farm_all", { on: !view.all }));
for (const b of $("fs-per").children) b.addEventListener("click", () => change("set_farm_per", { per: b.dataset.v }));
for (const b of $("fs-window").children) b.addEventListener("click", () => change("set_farm_window", { minutes: Number(b.dataset.v) }));
$("fs-reset").addEventListener("click", () => change("farm_reset", {}));
$("fs-search").addEventListener("input", () => {
  query = $("fs-search").value;
  clearTimeout(search.timer);
  search.timer = setTimeout(search, 150);
});

// What the run has gained moves on while the panel's open.
onTick(POLL_MS, () => {
  if (!document.hidden) load();
});
load();
