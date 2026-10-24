// The launcher: asks the app how things stand once a second and draws the tiles; its
// buttons start and stop the overlay and open the other windows. Mob names come from
// network traffic, so everything is set with textContent, never innerHTML.

const tauri = window.__TAURI__;
const invoke = tauri.core.invoke;
const win = tauri.window.getCurrentWindow();
const $ = (id) => document.getElementById(id);

const REFRESH_MS = 1000;
const PHASES = {
  idle: "idle",
  starting: "starting",
  waiting: "waiting",
  capturing: "capturing",
  replay: "replay",
  error: "error",
};

let state = null;

function fmt(n) {
  if (n >= 1e6) return (n / 1e6).toFixed(2) + "M";
  if (n >= 1e4) return (n / 1e3).toFixed(1) + "K";
  return Math.round(n).toLocaleString("en-US");
}

function clock(s) {
  const t = Math.floor(s);
  return `${Math.floor(t / 60)}:${String(t % 60).padStart(2, "0")}`;
}

// As on the meter: green for a normal connection, amber when laggy, red when bad.
const pingClass = (ms) => (ms < 200 ? "good" : ms < 300 ? "high" : "bad");

function drawCapture(s) {
  const phase = s.status.phase;
  $("capture").className = `tile ${phase}`;
  $("capture").title = s.status.message;
  $("phase").textContent = PHASES[phase] || phase;
  const ping = $("ping");
  if (s.ping_ms != null) {
    ping.textContent = `ping ${s.ping_ms} ms`;
    ping.className = `small ${pingClass(s.ping_ms)}`;
  } else {
    ping.textContent = phase === "waiting" ? "start the game" : phase === "error" ? "see the settings" : s.status.dropped ? `${s.status.dropped} packets dropped` : "";
    ping.className = "small";
  }
}

function drawFight(s) {
  const f = s.fight;
  const tile = $("fight");
  tile.disabled = !f;
  tile.classList.toggle("live", !!(f && f.live));
  $("fight-label").textContent = f && f.live ? "in combat" : "last fight";
  if (!f) {
    $("fight-target").textContent = "no fights yet";
    $("fight-dps").textContent = "";
    $("fight-time").textContent = "";
    return;
  }
  $("fight-target").textContent = (f.boss ? "★ " : "") + (f.target || "unknown target");
  const dps = f.my_dps != null ? f.my_dps : f.total_dps;
  $("fight-dps").textContent = `${fmt(dps)} dps`;
  const end = f.live ? "" : f.end === "kill" ? " · ✓ kill" : f.end === "wipe" ? " · ✗ wipe" : "";
  $("fight-time").textContent = clock(f.duration_s) + end;
  tile.title = `${s.fights} fight${s.fights === 1 ? "" : "s"} in the history · click for the analysis`;
}

function drawOverlay(s) {
  const on = s.overlay_on;
  $("overlay").classList.toggle("on", on);
  const toggle = $("toggle");
  toggle.setAttribute("aria-pressed", String(on));
  toggle.title = on ? "Stop the overlay" : "Start the overlay over the game";
  const widgets = `${s.widgets} widget${s.widgets === 1 ? "" : "s"}`;
  $("overlay-sub").textContent = !on
    ? "off"
    : s.overlay_showing
      ? `over the game · ${widgets}`
      : s.game
        ? "shows when the game is in front"
        : "waiting for the game";
  const arrange = $("arrange");
  arrange.hidden = !on;
  arrange.disabled = !s.overlay_showing;
  arrange.title = s.overlay_showing ? "Move and resize the overlay's widgets" : "The game must be in front to arrange the overlay";
}

async function refresh() {
  try {
    state = await invoke("launcher_state");
    $("version").textContent = `v${state.version}`;
    drawCapture(state);
    drawFight(state);
    drawOverlay(state);
  } catch (e) {
    $("phase").textContent = "error";
    $("capture").title = String(e);
  }
}

$("toggle").addEventListener("click", async () => {
  await invoke("set_overlay", { on: !(state && state.overlay_on) }).catch(() => {});
  refresh();
});
$("arrange").addEventListener("click", () => invoke("hud_edit", { on: true }).catch(() => {}));
const openAnalysis = () => invoke("open_analysis", { fight: null, run: null, player: null }).catch(() => {});
$("fight").addEventListener("click", openAnalysis);
$("analysis").addEventListener("click", openAnalysis);
$("history").addEventListener("click", () => invoke("open_history").catch(() => {}));
$("settings").addEventListener("click", () => invoke("toggle_settings").catch(() => {}));
$("min").addEventListener("click", () => win.minimize());
$("close").addEventListener("click", () => win.close());

invoke("appearance").then(applyAppearance).catch(() => {});
tauri.event.listen("appearance", (e) => applyAppearance(e.payload));

refresh();
setInterval(refresh, REFRESH_MS);
