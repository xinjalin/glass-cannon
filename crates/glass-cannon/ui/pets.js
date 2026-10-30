// The pet tracker: the pets you track (settings → pet tracker; where each is found shows
// there), with their level and souls toward the next one, and their mobs in view now: how
// far and which way to the nearest, once the meter knows where you are (from when you log
// in, teleport, change zone or are hit; "about" when it's from a mob you hit, or a minute
// old). With [nearest], every pet you still need whose mobs are in view, nearest first.

// Mobs move in small steps about ten times a second: this keeps the arrow live. Rows are
// updated in place, so a tooltip being read stays put.
const POLL_MS = 150;
let state = null;

// Portraits, by pet (asked for once each).
const portraits = new Map();
function portrait(p) {
  if (!portraits.has(p.id)) {
    portraits.set(p.id, null);
    invoke("pet_icon", { pet: p.id })
      .then((url) => {
        portraits.set(p.id, url);
        if (url) draw();
      })
      .catch(() => {});
  }
  const url = portraits.get(p.id);
  if (url) {
    const img = el("img", "pet-icon");
    img.src = url;
    img.alt = "";
    return img;
  }
  return el("span", "pet-icon blank", p.name[0]);
}

// Three pips, one per level reached.
function pips(level) {
  const box = el("span", "pet-pips");
  for (let i = 1; i <= 3; i++) box.append(el("span", "pip" + (level != null && level >= i ? " on" : ""), ""));
  box.title = level == null ? "Level not known yet" : level === 0 ? "Not summoned yet" : `Level ${level}`;
  return box;
}

const needs = (p) => p.level == null || p.need != null;

// Its mobs in view: how many, and how far and which way when known.
function near(p) {
  const box = el("span", "pet-near");
  if (p.bearing != null) {
    const arrow = el("span", "pet-arrow", "➤");
    // The arrow points east at 0°; bearings turn clockwise from north.
    arrow.style.transform = `rotate(${p.bearing - 90}deg)`;
    const rough = state.me_exact === false;
    box.append(arrow, el("span", "", ` ${rough ? "≈" : ""}${p.distance_m} m ×${p.in_view}`));
    box.title =
      `${p.in_view} of its mobs in view; the nearest ${p.distance_m} m away (the arrow: north is up)` +
      (rough ? "\nAbout: from the last mob you hit, or where you were a while ago (being hit, or a teleport, makes it exact)" : "");
  } else {
    box.append(el("span", "", `×${p.in_view}`));
    box.title = `${p.in_view} of its mobs in view (which way shows once the meter knows where you are: when you log in, teleport, change zone, are hit or hit a mob)`;
  }
  return box;
}

// What a row shows, apart from where its mobs are (that changes in place).
const rowKey = (p) => [p.id, p.name, p.level, p.souls, p.need, p.in_view > 0, needs(p)].join("|");

function row(p) {
  const li = el("li", "pet" + (p.in_view ? " here" : "") + (needs(p) ? "" : " done"));
  const top = el("div", "pet-top");
  const name = el("span", "pet-name", p.name);
  if (p.genus) name.title = p.genus;
  const progress = el("span", "pet-souls", p.level == null ? "?" : p.need == null ? "max" : `${p.souls}/${p.need}`);
  progress.title = p.level == null ? "Your pets come when you log in or enter a dungeon" : p.need == null ? "Top level" : `Souls toward level ${p.level + 1}`;
  top.append(name, pips(p.level), progress);
  if (p.in_view) top.append(near(p));
  li.append(portrait(p), top);
  return li;
}

function draw() {
  if (!state) return;
  const nearest = state.nearest;
  $("nearest").setAttribute("aria-pressed", String(nearest));
  $("pets-sync").textContent = state.synced ? "" : "· levels after login";
  const tracked = state.pets.filter((p) => p.tracked);
  const dist = (p) => (p.distance_m != null ? p.distance_m : Infinity);
  const shown = nearest
    ? state.pets.filter((p) => needs(p) && p.in_view).sort((a, b) => dist(a) - dist(b) || b.in_view - a.in_view || a.name.localeCompare(b.name))
    : tracked.sort((a, b) => needs(b) - needs(a) || (b.in_view > 0) - (a.in_view > 0) || a.name.localeCompare(b.name));
  const list = $("list");
  const old = new Map([...list.children].map((li) => [li.dataset.key, li]));
  const items = shown.map((p) => {
    const key = rowKey(p);
    const li = old.get(key) || row(p);
    li.dataset.key = key;
    const was = li.querySelector(".pet-near");
    if (p.in_view && was) {
      const now = near(p);
      if (now.textContent !== was.textContent || now.title !== was.title) {
        was.replaceChildren(...now.childNodes);
        was.title = now.title;
      }
      const arrow = was.querySelector(".pet-arrow");
      if (arrow) arrow.style.transform = `rotate(${p.bearing - 90}deg)`;
    }
    return li;
  });
  // Only re-order when the list changed (moving a row would drop its tooltip).
  if (items.length !== list.children.length || items.some((li, k) => list.children[k] !== li)) list.replaceChildren(...items);
  const empty = $("empty");
  empty.hidden = shown.length > 0;
  empty.textContent = nearest
    ? "none of the mobs in view give souls you still need"
    : !state.character
      ? "log in to a character to track its pets"
      : !tracked.length
        ? "pick pets to track in settings → pet tracker, or [nearest] for the pets around you"
        : "every pet you track is maxed ✓";
}

async function refresh() {
  try {
    state = await invoke("pet_tracker");
  } catch {
    return;
  }
  draw();
}

$("nearest").addEventListener("click", async () => {
  await invoke("set_pet_tracker_nearest", { on: !(state && state.nearest) }).catch(() => {});
  refresh();
});

setInterval(() => {
  if (!document.hidden) refresh();
}, POLL_MS);
refresh();
