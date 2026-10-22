// Shared by the side windows (history, cooldowns, timers): the app's appearance, the close
// button and Esc, and small helpers. Loaded after theme.js.

const tauri = window.__TAURI__;
const invoke = tauri.core.invoke;
const $ = (id) => document.getElementById(id);

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

const panelName = document.body.dataset.panel;
function closePanel() {
  invoke("toggle_panel", { name: panelName }).catch(() => {});
}
$("close").addEventListener("click", closePanel);
document.addEventListener("keydown", (e) => {
  if (e.key === "Escape") closePanel();
});
