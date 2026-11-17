// An action bar's settings (barsettings.html?bar=N), beside the bar while the overlay's
// arranged (actionbar.rs). Each change is saved and shows on the bar at once.

const BAR = Math.min(6, Math.max(1, Math.round(Number(new URLSearchParams(location.search).get("bar"))) || 1));
const WIDGET_NAME = BAR === 1 ? "action-bar" : `action-bar-${BAR}`;
// The marks among a bar's skills: a gap, and a new row.
const GAP = 1;
const ROW = 2;
// Each colour, and the bars it shows on (by what they track).
const COLOURS = [
  ["ready", "ready", "A skill that's ready", ["cooldowns", "both"]],
  ["running", "buffs", "A buff that's running", ["buffs", "both"]],
  ["cooling", "cooldown", "A skill on cooldown (and a debuff)", ["cooldowns", "both", "debuffs"]],
];

// The bar as last read (its setup, what can go on it), and whether its widget shows.
let view = null;
let shown = false;
let tab = "skills";

function press(b, on) {
  b.setAttribute("aria-pressed", String(!!on));
}
function toggle(b, on) {
  press(b, on);
  b.textContent = on ? "[on]" : "[off]";
}

// ---------------------------------------------------------------- saving

let saveTimer = null;
// When it was last changed here: what's read back meanwhile may be from before.
let changedAt = 0;
async function save(setup, soon = false) {
  changedAt = Date.now();
  view.setup = setup;
  draw();
  clearTimeout(saveTimer);
  const go = async () => {
    try {
      view.setup = await invoke("set_action_bar_setup", { bar: BAR, setup: view.setup });
      view.exists = true;
    } catch (e) {
      note(String(e));
    }
  };
  // Sliders: saved as they settle, not on every step.
  if (soon) saveTimer = setTimeout(go, 120);
  else await go();
}
const change = (patch) => save({ ...view.setup, ...patch });
const changeStyle = (patch, soon) => save({ ...view.setup, style: { ...view.setup.style, ...patch } }, soon);

function note(text) {
  $("bs-note").textContent = text;
  $("bs-note").hidden = !text;
}

// ---------------------------------------------------------------- the skills tab

// A skill's or effect's icon to pick (or one on the bar).
function pick(choice, extra) {
  const b = el("button", `bs-pick${extra ? ` ${extra}` : ""}`);
  b.type = "button";
  b.append(iconFor(choice.icon_key, choice.name || "?", draw));
  b.title = choice.name || `#${choice.id}`;
  return b;
}

// The list on the bar, drawn to drag about: `ids` in order, `meta` by ID; `key` is the
// setup's list ("skills" or "effects"). Dragged with the pointer (the app's window takes
// the page's own drag and drop for files): a click takes one off, a drag puts it before or
// after the one it's dropped on.
let dragging = false;
function onBar(ids, meta, key, marks) {
  const out = ids.map((id, i) => {
    const mark = id === GAP || id === ROW;
    const b = mark ? el("button", `bs-pick bs-mark ${id === ROW ? "row" : "gap"}`, id === ROW ? "↵" : "␣") : pick(meta.get(id) || { id, name: null, icon_key: null });
    b.type = "button";
    const what = mark ? (id === ROW ? "a new row" : "a gap") : b.title;
    b.title = `${what} · drag to move it · click to take it off`;
    const takeOff = () => change({ [key]: ids.filter((_, j) => j !== i) });
    // From the keyboard (Enter or Space): off.
    b.addEventListener("click", (e) => e.detail === 0 && takeOff());
    b.addEventListener("pointerdown", (e) => {
      if (e.button !== 0) return;
      e.preventDefault();
      const start = { x: e.clientX, y: e.clientY };
      const items = [...b.parentElement.querySelectorAll(".bs-pick")];
      let moved = false;
      let to = null;
      try {
        b.setPointerCapture(e.pointerId);
      } catch {
        // Moves still come while the pointer's over it.
      }
      const clear = () => {
        for (const x of items) x.classList.remove("drop-before", "drop-after");
      };
      const move = (m) => {
        if (!moved && Math.hypot(m.clientX - start.x, m.clientY - start.y) < 5) return;
        moved = true;
        dragging = true;
        b.classList.add("dragging");
        clear();
        to = null;
        // The one under the pointer (or the nearest in its row): before it on its left
        // half, after it on its right.
        let best = null;
        for (const [j, x] of items.entries()) {
          const r = x.getBoundingClientRect();
          if (m.clientY < r.top - 4 || m.clientY > r.bottom + 4) continue;
          const d = Math.abs(m.clientX - (r.left + r.width / 2));
          if (!best || d < best.d) best = { j, x, r, d };
        }
        if (!best) return;
        const after = m.clientX > best.r.left + best.r.width / 2;
        to = best.j + (after ? 1 : 0);
        if (to === i || to === i + 1) {
          to = null;
          return;
        }
        best.x.classList.add(after ? "drop-after" : "drop-before");
      };
      const up = () => {
        b.removeEventListener("pointermove", move);
        b.removeEventListener("pointerup", up);
        b.removeEventListener("pointercancel", cancel);
        b.classList.remove("dragging");
        clear();
        dragging = false;
        if (!moved) return takeOff();
        if (to == null) return;
        const next = [...ids];
        const [one] = next.splice(i, 1);
        next.splice(to > i ? to - 1 : to, 0, one);
        change({ [key]: next });
      };
      const cancel = () => {
        moved = true;
        to = null;
        up();
      };
      b.addEventListener("pointermove", move);
      b.addEventListener("pointerup", up);
      b.addEventListener("pointercancel", cancel);
    });
    return b;
  });
  if (marks) {
    for (const [id, text, title] of [
      [GAP, "[+ gap]", "A gap, as between your hotbar's groups (drag it where you want it)"],
      [ROW, "[+ row]", "Start a new row (drag it where you want it)"],
    ]) {
      const b = el("button", "bs-add-mark", text);
      b.type = "button";
      b.title = title;
      b.addEventListener("click", () => change({ [key]: [...ids, id] }));
      out.push(b);
    }
  }
  if (!ids.length && !marks) out.push(el("span", "bs-note", "nothing yet: click one below"));
  return out;
}

function drawSkills() {
  const s = view.setup;
  const effects = s.tracks === "buffs" || s.tracks === "debuffs";
  $("bs-skills").hidden = effects;
  $("bs-effects").hidden = !effects;
  for (const b of $("bs-tracks").children) press(b, b.dataset.v === s.tracks);
  toggle($("bs-shown"), shown);
  if (!effects) {
    const meta = new Map([...view.active, ...view.stigma].map((c) => [c.id, c]));
    $("bs-on").replaceChildren(...onBar(s.skills, meta, "skills", true));
    const choices = (list) =>
      list.length
        ? list.map((c) => {
            const b = pick(c, `${s.skills.includes(c.id) ? "chosen" : ""}${c.learned ? "" : " unlearned"}`);
            b.title = `${c.name || `#${c.id}`}${c.learned ? "" : " · not in your skill list"}${s.skills.includes(c.id) ? " · on the bar" : " · click to add"}`;
            b.addEventListener("click", () => !s.skills.includes(c.id) && change({ skills: [...s.skills, c.id] }));
            return b;
          })
        : [el("span", "bs-note", view.class == null ? "use a skill in game first" : "none known")];
    $("bs-active").replaceChildren(...choices(view.active));
    // Hide unequipped stigmas: only those you have equipped to pick from too (once known).
    const hiding = s.hide_unequipped && view.stigmas_known;
    const stigmas = hiding ? view.stigma.filter((c) => c.learned) : view.stigma;
    $("bs-stigma").replaceChildren(...choices(stigmas));
    const left = view.stigma.length - stigmas.length;
    $("bs-stigma-head").textContent = hiding ? `stigma · equipped · click to add${left ? ` (${left} not equipped, hidden)` : ""}` : "stigma · click to add";
    toggle($("bs-unequipped"), s.hide_unequipped);
    $("bs-stigmas-unknown").hidden = !s.hide_unequipped || view.stigmas_known;
  } else {
    const debuffs = s.tracks === "debuffs";
    const seen = debuffs ? view.debuffs : view.buffs;
    $("bs-all-label").textContent = debuffs ? "every debuff on you" : "every buff on you";
    toggle($("bs-all"), s.all_effects);
    $("bs-picked-effects").hidden = s.all_effects;
    $("bs-effects-head").textContent = `${debuffs ? "debuffs" : "buffs"} on you in the last hour · click to add`;
    const meta = new Map(seen.map((c) => [c.id, c]));
    $("bs-effects-on").replaceChildren(...onBar(s.effects, meta, "effects", false));
    const left = seen.filter((c) => !s.effects.includes(c.id));
    $("bs-effects-seen").replaceChildren(
      ...(left.length
        ? left.map((c) => {
            const b = pick(c);
            b.title = `${c.name || `#${c.id}`} · click to add`;
            b.addEventListener("click", () => change({ effects: [...s.effects, c.id] }));
            return b;
          })
        : [el("span", "bs-note", `none on you in the last hour yet (only named ones show)`)]),
    );
  }
  $("bs-add").hidden = view.bars.length >= 6;
}

// ---------------------------------------------------------------- the style tab

const NAMED = { green: "#79dcaa", blue: "#6aa8f0", red: "#f87070", amber: "#f1c073", lavender: "#c8b4f7", rose: "#f2a8a4", mono: "#dcdcdc" };

let coloursDrawn = "";
function drawColours(st) {
  const key = JSON.stringify([st.ready, st.running, st.cooling]);
  // Drawn again only when they change (it would close a picker that's open).
  if (key === coloursDrawn) return;
  coloursDrawn = key;
  $("bs-colours").replaceChildren(
    ...COLOURS.map(([field, label, what, on]) => {
      const line = el("div", "bs-line");
      line.dataset.tracks = on.join(" ");
      line.append(el("span", "", `${label} colour`));
      const sw = el("span", "bs-swatches");
      const theme = el("button", "", "[theme]");
      theme.type = "button";
      theme.title = "The bar colour (appearance)";
      press(theme, st[field] === "theme");
      theme.addEventListener("click", () => changeStyle({ [field]: "theme" }));
      sw.append(theme);
      for (const [name, hex] of Object.entries(NAMED)) {
        const b = el("button", "bs-swatch");
        b.type = "button";
        b.title = `${what}: ${name}`;
        b.style.setProperty("--swatch", hex);
        press(b, st[field] === name);
        b.addEventListener("click", () => changeStyle({ [field]: name }));
        sw.append(b);
      }
      const picked = /^#[0-9a-f]{6}$/i.test(st[field]);
      const custom = el("button", `bs-swatch custom${picked ? " picked" : ""}`);
      custom.type = "button";
      if (picked) custom.style.setProperty("--swatch", st[field]);
      custom.title = picked ? `${st[field]} · click to pick another` : "Pick any colour";
      press(custom, picked);
      const input = el("input");
      input.type = "color";
      input.tabIndex = -1;
      input.value = picked ? st[field] : "#79dcaa";
      input.addEventListener("change", () => changeStyle({ [field]: input.value }));
      custom.append(input);
      custom.addEventListener("click", () => {
        try {
          input.showPicker();
        } catch {
          input.click();
        }
      });
      sw.append(custom);
      line.append(sw);
      return line;
    }),
  );
}

function drawStyle() {
  const st = view.setup.style;
  for (const c of document.querySelectorAll(".bs-tab[data-tab='style'] .bs-choice[data-key]")) for (const b of c.children) press(b, st[c.dataset.key] === b.dataset.v);
  for (const t of document.querySelectorAll(".bs-tab[data-tab='style'] .bs-toggle[data-key]")) toggle(t, st[t.dataset.key]);
  for (const r of document.querySelectorAll("input[type='range'][data-key]")) {
    if (document.activeElement !== r) r.value = String(st[r.dataset.key]);
  }
  for (const v of document.querySelectorAll(".bs-value[data-for]")) {
    const k = v.dataset.for;
    v.textContent = k === "opacity" ? `${st[k]}%` : `${st[k]}px`;
  }
  drawColours(st);
  // A setting with no effect as the bar is (another look's, the countdown's with it off, a
  // skill's on an effects bar): greyed and still, saying why.
  const skills = view.setup.tracks === "cooldowns" || view.setup.tracks === "both";
  for (const line of document.querySelectorAll(".bs-tab[data-tab='style'] .bs-line")) {
    let why = "";
    if (line.dataset.only && line.dataset.only !== st.look) why = `not with the ${st.look} look`;
    else if (line.dataset.needs === "countdown" && !st.countdown) why = "the countdown is off";
    else if (line.dataset.skills !== undefined && !skills) why = "only for skills, and this bar tracks effects";
    else if (line.dataset.tracks && !line.dataset.tracks.split(" ").includes(view.setup.tracks)) why = `nothing on a ${view.setup.tracks} bar takes it`;
    line.classList.toggle("off", !!why);
    for (const c of line.querySelectorAll("button, input")) c.disabled = !!why;
    line.title = why ? `No effect: ${why}` : "";
  }
}

// ---------------------------------------------------------------- the rest

function drawPrefs() {
  for (const b of $("bs-show").children) press(b, b.dataset.v === view.setup.show);
  toggle($("bs-idle"), view.setup.hide_idle);
}

function draw() {
  if (!view) return;
  if (view.class == null) {
    note("use a skill in game first: each class has its own bars");
    $("bs-body").hidden = true;
    return;
  }
  $("bs-body").hidden = false;
  note(view.exists ? "" : `bar ${BAR} isn't set up for this class yet: pick what it shows to start it`);
  // Your class's bars, to go between.
  const nums = [...new Set([...view.bars, BAR])].sort();
  $("bs-bars").replaceChildren(
    ...nums.map((n) => {
      const b = el("button", "", `[bar ${n}]`);
      b.type = "button";
      press(b, n === BAR);
      b.addEventListener("click", () => n !== BAR && window.parent.postMessage({ type: "gc-open-settings", widget: n === 1 ? "action-bar" : `action-bar-${n}` }, "*"));
      return b;
    }),
  );
  for (const b of document.querySelectorAll(".bs-tabs [data-tab]")) b.setAttribute("aria-selected", String(b.dataset.tab === tab));
  for (const s of document.querySelectorAll(".bs-tab")) s.hidden = s.dataset.tab !== tab;
  drawSkills();
  drawStyle();
  drawPrefs();
}

async function load() {
  try {
    const [v, widgets] = await Promise.all([invoke("action_bar_setup", { bar: BAR }), invoke("widgets_shown")]);
    shown = (widgets || []).includes(WIDGET_NAME);
    // Not while a list's being dragged, or just after a change.
    if (dragging || Date.now() - changedAt < 1500) return;
    const key = JSON.stringify([v, shown]);
    if (key === lastLoaded) return;
    lastLoaded = key;
    view = v;
    draw();
  } catch (e) {
    note(String(e));
  }
}
let lastLoaded = "";

// ---------------------------------------------------------------- wiring

for (const b of document.querySelectorAll(".bs-tabs [data-tab]")) {
  b.addEventListener("click", () => {
    tab = b.dataset.tab;
    draw();
  });
}
for (const b of $("bs-tracks").children) b.addEventListener("click", () => view && change({ tracks: b.dataset.v }));
for (const b of $("bs-show").children) b.addEventListener("click", () => view && change({ show: b.dataset.v }));
$("bs-idle").addEventListener("click", () => view && change({ hide_idle: !view.setup.hide_idle }));
$("bs-unequipped").addEventListener("click", () => view && change({ hide_unequipped: !view.setup.hide_unequipped }));
$("bs-all").addEventListener("click", () => view && change({ all_effects: !view.setup.all_effects }));
$("bs-shown").addEventListener("click", async () => {
  if (!view) return;
  // Shown with nothing set up: set up as it is, so it has something to be.
  if (!view.exists) await save(view.setup);
  await invoke("toggle_widget", { name: WIDGET_NAME }).catch(() => {});
  lastLoaded = "";
  load();
});
for (const c of document.querySelectorAll(".bs-tab[data-tab='style'] .bs-choice[data-key]")) {
  for (const b of c.children) b.addEventListener("click", () => view && changeStyle({ [c.dataset.key]: b.dataset.v }));
}
for (const t of document.querySelectorAll(".bs-tab[data-tab='style'] .bs-toggle[data-key]")) {
  t.addEventListener("click", () => view && changeStyle({ [t.dataset.key]: !view.setup.style[t.dataset.key] }));
}
for (const r of document.querySelectorAll("input[type='range'][data-key]")) {
  r.addEventListener("input", () => view && changeStyle({ [r.dataset.key]: Number(r.value) }, true));
}
$("bs-add").addEventListener("click", async () => {
  try {
    const n = await invoke("add_action_bar");
    window.parent.postMessage({ type: "gc-open-settings", widget: n === 1 ? "action-bar" : `action-bar-${n}` }, "*");
  } catch (e) {
    note(String(e));
  }
});
// Removing takes a second click.
let removeArmed = null;
$("bs-remove").addEventListener("click", async () => {
  if (!removeArmed) {
    $("bs-remove").textContent = "[click again to remove it]";
    removeArmed = setTimeout(() => {
      removeArmed = null;
      $("bs-remove").textContent = "[remove this bar]";
    }, 3000);
    return;
  }
  clearTimeout(removeArmed);
  removeArmed = null;
  await invoke("remove_action_bar", { bar: BAR }).catch((e) => note(String(e)));
  window.parent.postMessage({ type: "gc-close-settings" }, "*");
});

load();
// What's been on you lately changes the choices.
setInterval(() => {
  if (!document.hidden) load();
}, 2000);
