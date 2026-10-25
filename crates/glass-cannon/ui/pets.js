// The pet tracker: the pets you track (settings → pet tracker), or with [nearest] every pet
// you still need, with their level and souls toward the next one, where they're found, and
// their mobs in view now (how far and which way while the meter knows where you are: for a
// minute after you log in, teleport or change zone).

const POLL_MS = 1000;
// Rows shown with [nearest] (the list is every pet you still need).
const NEAREST_SHOWN = 12;
let state = null;

// Three pips, one per level reached.
function pips(level) {
  const box = el("span", "pet-pips");
  for (let i = 1; i <= 3; i++) box.append(el("span", "pip" + (level != null && level >= i ? " on" : ""), ""));
  box.title = level == null ? "Level not known yet" : level === 0 ? "Not summoned yet" : `Level ${level}`;
  return box;
}

const needs = (p) => p.level == null || p.need != null;

function where(p) {
  if (p.in_view) {
    const box = el("span", "pet-near");
    if (p.bearing != null) {
      const arrow = el("span", "pet-arrow", "➤");
      // The arrow points east at 0°; bearings turn clockwise from north.
      arrow.style.transform = `rotate(${p.bearing - 90}deg)`;
      box.append(arrow, el("span", "", ` ${p.distance_m} m`));
    }
    box.append(el("span", "pet-count", p.bearing != null ? ` ×${p.in_view}` : `${p.in_view} in view`));
    box.title = p.bearing != null
      ? `${p.in_view} of its mobs in view; the nearest ${p.distance_m} m away (the arrow: north is up)`
      : `${p.in_view} of its mobs in view (how far isn't known: the meter knows where you are for a minute after you log in, teleport or change zone)`;
    return box;
  }
  const box = el("span", "pet-where", p.found_in[0] || "");
  box.title = p.found_in.length ? `Found in:\n${p.found_in.join("\n")}` : "Where it's found isn't known (an item, an event, or a dungeon)";
  return box;
}

function row(p) {
  const li = el("li", "pet" + (p.in_view ? " here" : "") + (needs(p) ? "" : " done"));
  const main = el("div", "pet-main");
  const name = el("span", "pet-name", p.name);
  if (p.genus) name.title = p.genus;
  const progress = el("span", "pet-souls", p.level == null ? "?" : p.need == null ? "max" : `${p.souls}/${p.need}`);
  progress.title = p.level == null ? "Your pets come when you log in or enter a dungeon" : p.need == null ? "Top level" : `Souls toward level ${p.level + 1}`;
  main.append(name, pips(p.level), progress);
  li.append(main, where(p));
  return li;
}

function draw() {
  if (!state) return;
  const nearest = state.nearest;
  $("nearest").setAttribute("aria-pressed", String(nearest));
  $("pets-sync").textContent = state.synced ? "" : "· levels after login";
  let shown;
  if (nearest) {
    const dist = (p) => (p.distance_m != null ? p.distance_m : Infinity);
    shown = state.pets
      .filter((p) => needs(p) && (p.in_view || p.found_in.length))
      .sort((a, b) => (b.in_view > 0) - (a.in_view > 0) || dist(a) - dist(b) || b.in_view - a.in_view || a.name.localeCompare(b.name))
      .slice(0, NEAREST_SHOWN);
  } else {
    shown = state.pets.filter((p) => p.tracked).sort((a, b) => needs(b) - needs(a) || (b.in_view > 0) - (a.in_view > 0) || a.name.localeCompare(b.name));
  }
  const list = $("list");
  list.replaceChildren(...shown.map(row));
  const empty = $("empty");
  empty.hidden = shown.length > 0;
  empty.textContent = nearest
    ? "every pet you can farm is maxed ✓"
    : state.character
      ? "pick pets to track in settings → pet tracker, or [nearest] for all you still need"
      : "log in to a character to track its pets";
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
