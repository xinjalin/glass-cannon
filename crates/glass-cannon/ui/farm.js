// The farming widget: a row an item, with what you've gained since the last reset and the
// rate, and as tall as its rows. No title; one small button, in the top right corner,
// resets the run (two clicks: the first asks). Which items are listed, which parts show
// (the run's time, the reset, the amounts, the rates) and what the rates go by are set
// in the panel beside it while the overlay's arranged (farmsettings.js).
//
// Clicks go through the widget to the game everywhere but on the button: the page tells
// the overlay where it is (gc-hot).

const POLL_MS = 1000;
// How long the reset waits for its second click.
const CONFIRM_MS = 3000;
let state = null;
let confirming = null;

// 12,345 below a million (a kill's worth shows), 1.23M and up above.
function amount(n) {
  if (n >= 1e9) return `${(n / 1e9).toFixed(2)}B`;
  if (n >= 1e6) return `${(n / 1e6).toFixed(2)}M`;
  return Math.round(n).toLocaleString("en-US");
}

// A rate: whole numbers from 100 up, a decimal below (0.4 souls a minute).
function rate(r) {
  if (r >= 1e6) return `${(r / 1e6).toFixed(2)}M`;
  if (r >= 100) return Math.round(r).toLocaleString("en-US");
  return r >= 10 ? r.toFixed(0) : r.toFixed(1);
}

const perLabel = (per) => (per === "minute" ? "/min" : "/h");

// What a row shows (a row is only redrawn when that changes).
const rowKey = (r, s) => [r.item, r.name, r.pinned, r.gained, r.rate == null ? "" : rate(r.rate), s.per, s.show.gained, s.show.rate].join("|");

function row(r, s) {
  const li = el("li", "farm-row" + (r.pinned ? "" : " other") + (r.gained ? "" : " none"));
  li.append(el("span", "farm-name" + (r.grade ? ` grade-${r.grade.toLowerCase()}` : ""), r.name));
  if (s.show.gained) li.append(el("span", "farm-gained", r.gained ? `+${amount(r.gained)}` : "0"));
  // No rate for what you haven't gained, or before there's enough time to go by.
  if (s.show.rate) li.append(el("span", "farm-rate", r.rate == null || !r.gained ? "–" : `${rate(r.rate)}${perLabel(s.per)}`));
  return li;
}

function draw() {
  if (!state) return;
  // The line over the rows: the run's time on the left, the reset on the right; gone
  // with both hidden.
  $("farm-head").hidden = !state.show.time && !state.show.reset;
  const time = $("farm-time");
  time.hidden = !state.show.time;
  const reset = $("farm-reset");
  reset.hidden = !state.show.reset;
  reset.classList.toggle("asking", !!confirming);
  setText(reset, confirming ? "reset?" : "↺");
  // The run's time, and what the rates go by when it isn't the whole run.
  const over = state.window_min ? ` · last ${state.window_min === 60 ? "hour" : `${state.window_min} min`}` : "";
  setText(time, state.elapsed_s == null ? "no gains yet" : `${clock(state.elapsed_s)}${state.show.rate ? over : ""}`);
  const list = $("list");
  // A column each for the name and whichever of the amount and the rate show.
  list.className = `list farm-list cols-${1 + state.show.gained + state.show.rate}`;
  const old = new Map([...list.children].map((li) => [li.dataset.key, li]));
  const items = state.rows.map((r) => {
    const key = rowKey(r, state);
    const li = old.get(key) || row(r, state);
    li.dataset.key = key;
    return li;
  });
  if (items.length !== list.children.length || items.some((li, k) => list.children[k] !== li)) list.replaceChildren(...items);
  // Past what's listed with everything gained on.
  const more = $("more");
  more.hidden = !(state.all && state.more);
  setText(more, `and ${state.more} more`);
  tellHot();
}

// Where the reset button is, for the overlay to take the mouse there and nowhere else
// on the widget (a little past its edges, to be easy to hit).
let hotSent = "";
function tellHot() {
  const b = $("farm-reset");
  let rect = null;
  if (state && state.show.reset && b.getClientRects().length) {
    const r = b.getBoundingClientRect();
    rect = { x: Math.floor(r.left) - 3, y: Math.floor(r.top) - 3, width: Math.ceil(r.width) + 6, height: Math.ceil(r.height) + 6 };
  }
  const key = JSON.stringify(rect);
  if (key === hotSent) return;
  hotSent = key;
  window.parent.postMessage({ type: "gc-hot", widget: "farm", rect }, "*");
}
new ResizeObserver(tellHot).observe($("panel"));

// The first click asks, the second (within three seconds) resets: a run isn't lost to a
// stray click in a fight.
$("farm-reset").addEventListener("click", async () => {
  clearTimeout(confirming);
  if (confirming) {
    confirming = null;
    await invoke("farm_reset").catch(() => {});
    await refresh();
    // Back to the arrow even if the fresh numbers didn't come.
    return draw();
  }
  confirming = setTimeout(() => {
    confirming = null;
    draw();
  }, CONFIRM_MS);
  draw();
});

async function refresh() {
  try {
    state = await invoke("farm_view");
  } catch {
    return;
  }
  draw();
}

onTick(POLL_MS, () => {
  if (!document.hidden) refresh();
});
// A change in its settings panel shows at once.
tauri.event.listen("farm-changed", refresh);
refresh();
