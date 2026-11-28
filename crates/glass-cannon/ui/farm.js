// The farming widget: what you've gained since the last [reset] and at what rate. Abyss
// Points and both kinds of Kina are pinned to begin with; [all] lists everything gained
// this run, and a right-click pins an item (always listed) or unpins it. The rates go by
// what you gain: what you spend is in the row's tooltip, and takes nothing off.

const POLL_MS = 1000;
const WINDOWS = [0, 5, 15, 60];
const WINDOW_LABEL = { 0: "run", 5: "5m", 15: "15m", 60: "1h" };
let state = null;

// 12,345 below a million (a kill's worth shows), 1.23M and up above.
function amount(n) {
  if (n >= 1e9) return `${(n / 1e9).toFixed(2)}B`;
  if (n >= 1e6) return `${(n / 1e6).toFixed(2)}M`;
  return Math.round(n).toLocaleString("en-US");
}

// A rate: whole numbers from 100 up, a decimal below (0.4 souls a minute).
function rate(r) {
  if (r == null) return "–";
  if (r >= 1e6) return `${(r / 1e6).toFixed(2)}M`;
  if (r >= 100) return Math.round(r).toLocaleString("en-US");
  return r >= 10 ? r.toFixed(0) : r.toFixed(1);
}

const perLabel = (per) => (per === "minute" ? "/min" : "/h");

const rowKey = (r, per) => [r.item, r.name, r.pinned, r.gained, r.spent, r.rate == null ? "" : rate(r.rate), r.have, per].join("|");

function row(r, per) {
  const li = el("li", "farm-row" + (r.pinned ? " pinned" : "") + (r.gained ? "" : " none"));
  const name = el("span", "farm-name" + (r.grade ? ` grade-${r.grade.toLowerCase()}` : ""), r.name);
  const gained = el("span", "farm-gained", r.gained ? `+${amount(r.gained)}` : "0");
  // No rate for what you haven't gained, or before there's enough time to go by.
  const per_ = el("span", "farm-rate", r.rate == null || !r.gained ? "–" : `${rate(r.rate)}${perLabel(per)}`);
  const lines = [r.name];
  if (r.have != null) lines.push(`you have ${r.have.toLocaleString("en-US")}`);
  lines.push(`gained ${r.gained.toLocaleString("en-US")} this run`);
  if (r.spent) lines.push(`spent or used ${r.spent.toLocaleString("en-US")}`);
  lines.push(r.pinned ? "right-click: unpin" : "right-click: pin (always listed)");
  li.title = lines.join("\n");
  li.addEventListener("contextmenu", async (e) => {
    e.preventDefault();
    await invoke("set_farm_pinned", { item: r.item, on: !r.pinned }).catch(() => {});
    refresh();
  });
  li.append(name, gained, per_);
  return li;
}

function draw() {
  if (!state) return;
  $("farm-time").textContent = state.elapsed_s == null ? "· waiting for a first gain" : `· ${clock(state.elapsed_s)}`;
  $("farm-window").textContent = `[${WINDOW_LABEL[state.window_min] || "run"}]`;
  $("farm-per").textContent = `[${perLabel(state.per)}]`;
  $("farm-all").setAttribute("aria-pressed", String(state.all));
  const list = $("list");
  const old = new Map([...list.children].map((li) => [li.dataset.key, li]));
  const items = state.rows.map((r) => {
    const key = rowKey(r, state.per);
    const li = old.get(key) || row(r, state.per);
    li.dataset.key = key;
    return li;
  });
  // Only touch the list when it changed (moving a row would drop its tooltip).
  if (items.length !== list.children.length || items.some((li, k) => list.children[k] !== li)) list.replaceChildren(...items);
  const more = $("more");
  more.hidden = !state.more;
  more.textContent = state.all ? `and ${state.more} more` : `+ ${state.more} other ${state.more === 1 ? "item" : "items"} gained: [all]`;
}

async function refresh() {
  try {
    state = await invoke("farm_view");
  } catch {
    return;
  }
  draw();
}

$("farm-window").addEventListener("click", async () => {
  const at = WINDOWS.indexOf(state ? state.window_min : 0);
  await invoke("set_farm_window", { minutes: WINDOWS[(at + 1) % WINDOWS.length] }).catch(() => {});
  refresh();
});
$("farm-per").addEventListener("click", async () => {
  await invoke("set_farm_per", { per: state && state.per === "hour" ? "minute" : "hour" }).catch(() => {});
  refresh();
});
$("farm-all").addEventListener("click", async () => {
  await invoke("set_farm_all", { on: !(state && state.all) }).catch(() => {});
  refresh();
});
$("farm-reset").addEventListener("click", async () => {
  await invoke("farm_reset").catch(() => {});
  refresh();
});

onTick(POLL_MS, () => {
  if (!document.hidden) refresh();
});
refresh();
