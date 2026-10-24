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
  waiting: ["waiting for the game", "warn"],
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

function clock(s) {
  const t = Math.floor(s);
  return `${String(Math.floor(t / 60)).padStart(2, "0")}:${String(t % 60).padStart(2, "0")}`;
}

function el(tag, className, text) {
  const e = document.createElement(tag);
  if (className) e.className = className;
  if (text !== undefined) e.textContent = text;
  return e;
}

// One line of the list: "key - value" on the left, something short on the right.
function row(key, value, right = "", opts = {}) {
  const li = el("li", [opts.current ? "current" : "", opts.onClick ? "link" : ""].join(" ").trim());
  const what = el("span", "what", `${key} - `);
  what.append(el("span", opts.cls || "", value));
  li.append(what, el("span", "right", right));
  if (opts.title) li.title = opts.title;
  if (opts.onClick) li.addEventListener("click", opts.onClick);
  return li;
}

function overlayText(s) {
  if (!s.overlay_on) return ["off", ""];
  if (s.overlay_showing) return ["over the game", "accent"];
  return s.game ? ["shows with the game in front", ""] : ["waiting for the game", "warn"];
}

function endText(f) {
  if (f.live) return ["in combat", "bad"];
  return { kill: ["✓ kill", "ok"], wipe: ["✗ wipe", "bad"], manual: ["ended", ""], zone: ["left the map", ""], idle: ["went quiet", ""] }[f.end] || ["ended", ""];
}

function drawList(s) {
  const f = s.fight;
  const [phase, phaseCls] = PHASES[s.status.phase] || [s.status.phase, ""];
  const [overlay, overlayCls] = overlayText(s);
  const rows = [
    row("capture", phase, s.ping_ms != null ? `${s.ping_ms} ms` : "", { cls: phaseCls, title: s.status.message }),
    row("game", s.game ? "running" : "not running", "", { cls: s.game ? "" : "warn" }),
    row("overlay", overlay, s.overlay_on ? `${s.widgets} widget${s.widgets === 1 ? "" : "s"}` : "", {
      cls: overlayCls,
      current: !(f && f.live),
      title: "The play button below starts and stops it",
    }),
  ];
  if (f) {
    const openAnalysis = () => invoke("open_analysis", { fight: null, run: null, player: null }).catch(() => {});
    const [end, endCls] = endText(f);
    rows.push(
      row(f.live ? "fight" : "last fight", (f.boss ? "★ " : "") + (f.target || "unknown target"), clock(f.duration_s), {
        current: f.live,
        onClick: openAnalysis,
        title: "Click for its analysis",
      }),
      row("result", end, f.progress != null ? `${Math.round(f.progress * 100)}%` : "", { cls: endCls }),
    );
    if (f.my_dps != null) {
      const share = f.total_dps > 0 ? `${Math.round((f.my_dps / f.total_dps) * 100)}%` : "";
      rows.push(row("you", `${fmt(f.my_dps)} dps`, share, { cls: "accent" }));
    }
    rows.push(row(f.players > 1 ? "party" : "total", `${fmt(f.total_dps)} dps`, `${f.players} player${f.players === 1 ? "" : "s"}`));
  } else {
    rows.push(row("last fight", "none yet", ""));
  }
  rows.push(row("history", `${s.fights} fight${s.fights === 1 ? "" : "s"}`, "›", { onClick: () => invoke("open_history").catch(() => {}) }));
  $("rows").replaceChildren(...rows);
}

function drawCover(s) {
  const f = s.fight;
  $("cover-sub").textContent = f ? (f.boss ? "★ " : "") + (f.target || "unknown target") : "aion 2 dps meter";
  $("cover-dot").classList.toggle("live", !!(f && f.live));
  const bars = f && f.bars.length ? f.bars : [0.35, 0.6, 0.25, 0.8, 0.45, 0.3, 0.55];
  $("skyline").replaceChildren(
    ...bars.map((b) => {
      const span = el("span");
      span.style.height = `${Math.max(5, Math.round(b * 100))}%`;
      return span;
    }),
  );
}

function drawBar(s) {
  const f = s.fight;
  $("progress-fill").style.width = `${f && f.progress != null ? f.progress * 100 : 0}%`;
  const on = s.overlay_on;
  const toggle = $("toggle");
  toggle.setAttribute("aria-pressed", String(on));
  toggle.title = on ? "Stop the overlay" : "Start the overlay over the game";
  $("arrange").disabled = !s.overlay_showing;
  $("arrange").title = s.overlay_showing ? "Arrange the overlay's widgets" : "Arrange the overlay's widgets (the game must be in front)";
  $("now").textContent = `GLASS CANNON - ${f ? (f.live ? "" : "last: ") + (f.target || "unknown target") : "waiting for a fight"}`;
  $("clock").textContent = f ? clock(f.duration_s) : "--:--";
  $("sub").textContent = s.ping_ms != null ? `/ ${s.ping_ms} ms` : `/ v${s.version}`;
}

async function refresh() {
  try {
    state = await invoke("launcher_state");
    drawList(state);
    drawCover(state);
    drawBar(state);
  } catch (e) {
    $("now").textContent = `GLASS CANNON - ${e}`;
  }
}

$("toggle").addEventListener("click", async () => {
  await invoke("set_overlay", { on: !(state && state.overlay_on) }).catch(() => {});
  refresh();
});
const openAnalysis = () => invoke("open_analysis", { fight: null, run: null, player: null }).catch(() => {});
$("cover").addEventListener("click", openAnalysis);
$("analysis").addEventListener("click", openAnalysis);
$("history").addEventListener("click", () => invoke("open_history").catch(() => {}));
$("arrange").addEventListener("click", () => invoke("hud_edit", { on: true }).catch(() => {}));
$("settings").addEventListener("click", () => invoke("toggle_settings").catch(() => {}));
$("close").addEventListener("click", () => win.close());
$("min").addEventListener("click", () => win.minimize());
$("pin").addEventListener("click", async () => {
  const on = $("pin").getAttribute("aria-pressed") !== "true";
  await win.setAlwaysOnTop(on).catch(() => {});
  $("pin").setAttribute("aria-pressed", String(on));
  $("pin").title = on ? "Kept on top · click to let other windows cover it" : "Keep on top";
});

refresh();
setInterval(refresh, REFRESH_MS);
