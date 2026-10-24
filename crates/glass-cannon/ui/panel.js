// Shared by the history window and the overlay's side widgets (cooldowns, world timers,
// target frame, party cooldowns): the app's appearance and small helpers; for the history,
// its close button and Esc. Loaded after theme.js.

// A widget's page (class "widget") is a frame in the HUD's page, which lends it the app
// (frames don't get it themselves). The settings show and hide it.
const WIDGET = document.documentElement.classList.contains("widget");
const tauri = WIDGET ? window.parent.__TAURI__ : window.__TAURI__;
const invoke = tauri.core.invoke;
const $ = (id) => document.getElementById(id);
// A page that fits its content (the target frame) tells the overlay how tall it is.
if (WIDGET && document.body.hasAttribute("data-fit")) {
  document.documentElement.classList.add("fit");
  // Its full height (scrollHeight: what it would take, not the box it has now), whenever
  // its content changes.
  let sent = 0;
  const report = () => {
    const h = Math.ceil($("panel").scrollHeight);
    if (h === sent) return;
    sent = h;
    window.parent.postMessage({ type: "gc-fit", widget: document.body.dataset.panel, height: h }, "*");
  };
  new ResizeObserver(report).observe($("panel"));
  new MutationObserver(report).observe($("panel"), { childList: true, subtree: true, characterData: true, attributes: true });
}

function el(tag, className, text) {
  const e = document.createElement(tag);
  if (className) e.className = className;
  if (text !== undefined) e.textContent = text;
  return e;
}

function fmt(n) {
  if (n >= 1e6) return (n / 1e6).toFixed(2) + "M";
  if (n >= 1e4) return (n / 1e3).toFixed(1) + "K";
  return Math.round(n).toLocaleString("en-US");
}

function clock(s) {
  const t = Math.max(0, Math.floor(s));
  const h = Math.floor(t / 3600);
  const m = Math.floor((t % 3600) / 60);
  const sec = String(t % 60).padStart(2, "0");
  return h ? `${h}:${String(m).padStart(2, "0")}:${sec}` : `${m}:${sec}`;
}

function timeOfDay(ms) {
  return new Date(ms).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}

function dayAndTime(ms) {
  const d = new Date(ms);
  const today = new Date();
  const sameDay = d.toDateString() === today.toDateString();
  return sameDay ? timeOfDay(ms) : `${d.toLocaleDateString([], { weekday: "short" })} ${timeOfDay(ms)}`;
}

// Skill icons by key, loaded once each.
const icons = new Map();
function iconFor(key, label, onLoad) {
  if (key != null && !icons.has(key)) {
    icons.set(key, null);
    invoke("skill_icon", { skill: key })
      .then((url) => {
        icons.set(key, url);
        if (url && onLoad) onLoad();
      })
      .catch(() => {});
  }
  const url = key != null ? icons.get(key) : null;
  if (url) {
    const img = el("img", "icon");
    img.src = url;
    img.alt = "";
    return img;
  }
  return el("span", "icon blank", label && !/^[#\d]/.test(label) ? label[0] : "");
}

invoke("appearance").then(applyAppearance).catch(() => {});
tauri.event.listen("appearance", (e) => applyAppearance(e.payload));

// The history window: its close button and Esc only hide it.
if (!WIDGET) {
  const hide = () => tauri.window.getCurrentWindow().hide();
  $("close").addEventListener("click", hide);
  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape") hide();
  });
}
