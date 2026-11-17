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
  const wide = document.body.hasAttribute("data-fit-width");
  // Its full height (scrollHeight: what it would take, not the box it has now), whenever
  // its content changes.
  let sent = "";
  const report = () => {
    // With the box shorter than the content, the list scrolls inside it: count what's
    // scrolled away too, or the box could never grow back.
    const scroll = $("panel").querySelector(".scroll");
    const hidden = scroll ? scroll.scrollHeight - scroll.clientHeight : 0;
    // (A pixel spare: heights in fractions of one round down.)
    const h = Math.ceil($("panel").scrollHeight + hidden) + 1;
    // One that fits its width too (an action bar): as wide as what it shows.
    const w = wide ? Math.ceil($("panel").scrollWidth) + 1 : null;
    if (`${h}/${w}` === sent) return;
    sent = `${h}/${w}`;
    window.parent.postMessage({ type: "gc-fit", widget: document.body.dataset.panel, height: h, width: w }, "*");
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

// Where a shield sits on an HP bar, as shares of the bar: [start, length]. On from the
// HP's end; when HP and shield together pass the end, over the bar's last part instead,
// so it stays as long as it is. None without HP, a max, or a shield.
function shieldSpan(hp, max, shield) {
  if (hp == null || !max || !shield) return null;
  const len = Math.min(1, shield / max);
  const at = Math.max(0, Math.min(1, hp / max));
  return [Math.min(at, 1 - len), len];
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

// The action bar's bar colours (settings → action bar): a name, or a colour picked.
const BAR_COLOURS = { green: "#79dcaa", blue: "#6aa8f0", red: "#f87070", amber: "#f1c073", lavender: "#c8b4f7", rose: "#f2a8a4", mono: "#dcdcdc" };
const barColour = (c) => (c === "theme" ? "var(--bar, var(--accent-fill))" : BAR_COLOURS[c] || c);

// The cooldown widgets' bars in the action bar's colours and background (settings →
// cooldowns → action bar's colours), or, with none, their own: cooling in its cooling
// colour, ready full in its ready colour.
function applyCooldownBars(l) {
  const s = document.body.style;
  for (const [k, v] of [["--cd-cooling", l && l.cooling], ["--cd-ready", l && l.ready]]) {
    if (v) s.setProperty(k, barColour(v));
    else s.removeProperty(k);
  }
  document.body.classList.toggle("cd-bar-look", !!l);
  document.body.classList.toggle("cd-no-track", !!l && !l.background);
}

// A widget's note (#empty: "not in a party", "all your skills are ready", "your HP and MP
// show once you're in game"…): with settings → hide empty notes, the note's text never
// shows (panel.css). The widget itself goes only when that note is all it has to show (no
// empty box left on the screen): what else it has showing is anything drawn (an icon, a
// canvas, a shape) or text, outside the note. Told to the overlay, which hides it then.
if (WIDGET) {
  const note = document.getElementById("empty");
  if (note) {
    let sent = null;
    let timer = null;
    const shows = (e) => e.getClientRects().length > 0 && getComputedStyle(e).visibility !== "hidden";
    // The note's up, as the widget has it (whether hidden by the setting or not).
    const noteUp = () => !note.hidden && !!note.textContent.trim() && (!note.parentElement || shows(note.parentElement));
    const DRAWN = new Set(["IMG", "CANVAS", "path", "circle", "rect", "line", "polyline", "polygon", "ellipse", "image"]);
    const somethingElse = () => {
      for (const e of document.body.querySelectorAll("*")) {
        if (e === note || note.contains(e) || e.tagName === "SCRIPT" || e.firstElementChild) continue;
        const drawn = DRAWN.has(e.tagName) || !!e.textContent.trim();
        if (!drawn || !shows(e)) continue;
        const r = e.getBoundingClientRect();
        if (r.width > 0 && r.height > 0) return true;
      }
      return false;
    };
    const tell = () => {
      timer = null;
      const empty = noteUp() && !somethingElse();
      if (empty === sent) return;
      sent = empty;
      window.parent.postMessage({ type: "gc-empty", widget: document.body.dataset.panel || "meter", empty }, "*");
    };
    // At most every quarter second (a widget redraws often).
    const soon = () => {
      if (!timer) timer = setTimeout(tell, 250);
    };
    new MutationObserver(soon).observe(document.body, { subtree: true, childList: true, characterData: true, attributes: true, attributeFilter: ["hidden", "class", "style", "d"] });
    window.addEventListener("load", soon);
    soon();
  }
}
