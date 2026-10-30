// The launcher: asks the app how things stand once a second and draws the cover, the list
// and the control bar; its buttons start and stop the overlay and open the other windows.
// Its colours are its own (launcher.css), whatever the theme. Mob names come from network
// traffic, so everything is set with textContent, never innerHTML.

const tauri = window.__TAURI__;
const invoke = tauri.core.invoke;
const win = tauri.window.getCurrentWindow();
const $ = (id) => document.getElementById(id);

const REFRESH_MS = 1000;
const PHASES = {
  idle: ["idle", ""],
  starting: ["starting", "warn"],
  waiting: ["waiting", "warn"],
  capturing: ["capturing", "ok"],
  replay: ["replay", "accent"],
  error: ["error", "bad"],
};

let state = null;

function fmt(n) {
  if (n >= 1e6) return (n / 1e6).toFixed(2) + "M";
  if (n >= 1e4) return (n / 1e3).toFixed(1) + "K";
  return Math.round(n).toLocaleString("en-US");
}

function el(tag, className, text) {
  const e = document.createElement(tag);
  if (className) e.className = className;
  if (text !== undefined) e.textContent = text;
  return e;
}

// Text too long for its place slides along instead of being cut off: how far, and slower
// the further it goes.
function fitScroll(box) {
  const over = box.firstElementChild.scrollWidth - box.clientWidth;
  box.classList.toggle("long", over > 2);
  box.style.setProperty("--shift", `-${over + 6}px`);
  box.style.setProperty("--slide-s", `${Math.max(6, 4 + over / 12).toFixed(1)}s`);
}

const ACTIONS = {
  analysis: () => invoke("open_analysis", { fight: null, run: null, player: null }).catch(() => {}),
  history: () => invoke("open_history").catch(() => {}),
  petMap: () => invoke("open_pet_map").catch(() => {}),
};

// The list's rows: "key - value" on the left, something short on the right. Each is only
// rebuilt when it changes, so sliding text keeps sliding.
function drawRows(rows) {
  const list = $("rows");
  while (list.children.length > rows.length) list.lastChild.remove();
  rows.forEach((r, i) => {
    const sig = JSON.stringify(r);
    let li = list.children[i];
    if (li && li.dataset.sig === sig) return;
    const fresh = el("li", [r.group ? "group" : "", r.current ? "current" : "", r.action ? "link" : ""].join(" ").trim());
    fresh.dataset.sig = sig;
    const what = el("span", "what scroll");
    const text = el("span", "", `${r.key} - `);
    text.append(el("span", r.cls || "", r.value));
    what.append(text);
    fresh.append(what, el("span", "right", r.right || ""));
    if (r.title) fresh.title = r.title;
    if (r.action) fresh.addEventListener("click", ACTIONS[r.action]);
    if (li) li.replaceWith(fresh);
    else list.append(fresh);
    fitScroll(what);
  });
}

// Off, on, or over the game (the game line says whether it's running).
function overlayText(s) {
  if (s.arranging) return ["arranging", "accent"];
  if (!s.overlay_on) return ["off", ""];
  // Hidden by the hotkey: say which keys bring it back.
  if (s.hidden) return [`hidden · ${s.hide_keys.replace(/Key(.)/, "$1").replace(/Digit(.)/, "$1")} shows it`, "accent"];
  if (s.step_aside === "menu") return ["hidden for a game menu (the cursor)", ""];
  if (s.world_map) return ["hidden for the world map", ""];
  return s.overlay_showing ? ["over the game", "accent"] : ["on", ""];
}

function endText(f) {
  if (f.live) return ["in combat", "bad"];
  return { kill: ["✓ kill", "ok"], wipe: ["✗ wipe", "bad"], manual: ["ended", ""], zone: ["left the map", ""], idle: ["went quiet", ""] }[f.end] || ["ended", ""];
}

function drawList(s) {
  const f = s.fight;
  const [phase, phaseCls] = PHASES[s.status.phase] || [s.status.phase, ""];
  const [overlay, overlayCls] = overlayText(s);
  // Grouped: status, the overlay, the fight, the history.
  const rows = [
    { key: "status", value: phase, right: s.ping_ms != null ? `${s.ping_ms} ms` : "", cls: phaseCls, title: s.status.message },
    { key: "game", value: s.game ? "running" : "not running", cls: s.game ? "" : "warn" },
    {
      key: "overlay",
      value: overlay,
      right: s.overlay_on ? `${s.widgets} widget${s.widgets === 1 ? "" : "s"}` : "",
      cls: overlayCls,
      current: !(f && f.live),
      group: true,
      title: "The play button below starts and stops it",
    },
  ];
  if (f) {
    const [end, endCls] = endText(f);
    // How it's going or how it ended (its name, time and the boss's HP are on the bar below,
    // so they're not said twice).
    rows.push({
      key: f.live ? "fight" : "last fight",
      value: end,
      cls: endCls,
      current: f.live,
      group: true,
      action: "analysis",
      title: "Click for its analysis",
    });
    if (f.my_dps != null) {
      const share = f.total_dps > 0 ? `${Math.round((f.my_dps / f.total_dps) * 100)}%` : "";
      rows.push({ key: "you", value: `${fmt(f.my_dps)} dps`, right: share, cls: "accent" });
    }
    rows.push({ key: f.players > 1 ? "party" : "total", value: `${fmt(f.total_dps)} dps`, right: `${f.players} player${f.players === 1 ? "" : "s"}` });
  } else {
    rows.push({ key: "last fight", value: "none yet", group: true });
  }
  rows.push({ key: "history", value: `${s.fights} fight${s.fights === 1 ? "" : "s"}`, right: "›", group: true, action: "history" });
  drawRows(rows);
}

// The cover: its dot beats during a fight (the list says so in words), and the version.
function drawCover(s) {
  const f = s.fight;
  $("cover-dot").classList.toggle("live", !!(f && f.live));
  $("cover-status").textContent = `v${s.version}`;
}

// The bar: buttons only (the fight is in the list above).
function drawBar(s) {
  const toggle = $("toggle");
  // Hidden by the hotkey, the button brings the widgets back (as if off).
  toggle.setAttribute("aria-pressed", String(s.overlay_on && !s.hidden));
  toggle.title = s.hidden ? "Show the overlay's widgets again" : s.overlay_on ? "Stop the overlay" : "Start the overlay over the game";
  // Arrange: on while the overlay is over the game; again to save and finish.
  const arrange = $("arrange");
  arrange.disabled = !s.overlay_showing && !s.arranging;
  arrange.setAttribute("aria-pressed", String(s.arranging));
  arrange.title = s.arranging
    ? "Save the layout and stop arranging"
    : s.overlay_showing
      ? "Arrange the overlay's widgets"
      : "Arrange the overlay's widgets (the game must be in front)";
}

async function refresh() {
  try {
    state = await invoke("launcher_state");
    drawPin(state.on_top);
    drawList(state);
    drawCover(state);
    drawBar(state);
  } catch (e) {
    $("cover-status").textContent = "error";
    $("cover-status").title = String(e);
  }
}

$("toggle").addEventListener("click", async () => {
  await invoke("set_overlay", { on: !(state && state.overlay_on && !state.hidden) }).catch(() => {});
  refresh();
});
$("cover").addEventListener("click", ACTIONS.analysis);
$("analysis").addEventListener("click", ACTIONS.analysis);
$("history").addEventListener("click", ACTIONS.history);
$("pet-map").addEventListener("click", ACTIONS.petMap);
$("arrange").addEventListener("click", async () => {
  await invoke("hud_edit", { on: !(state && state.arranging) }).catch(() => {});
  refresh();
});
$("settings").addEventListener("click", () => invoke("toggle_settings").catch(() => {}));
$("close").addEventListener("click", () => win.close());
$("min").addEventListener("click", () => win.minimize());
// The green light: keep the launcher on top (remembered for next time).
function drawPin(on) {
  $("pin").setAttribute("aria-pressed", String(on));
  $("pin").title = on ? "Kept on top · click to let other windows cover it" : "Keep on top";
}
$("pin").addEventListener("click", async () => {
  const on = !(state && state.on_top);
  await invoke("set_launcher_on_top", { on }).catch(() => {});
  if (state) state.on_top = on;
  drawPin(on);
});

refresh();
setInterval(refresh, REFRESH_MS);
