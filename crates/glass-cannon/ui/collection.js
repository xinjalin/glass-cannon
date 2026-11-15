// The collection window: every skin, outfit and wing in the data, by kind and slot, which
// your characters have (in the theme's colour, with the game's count, or a wing's enchant)
// and which they're missing (in red), with how far along each slot is. The game sends your
// collection at login; the app keeps it per character, and they count together. Names
// are set with textContent, never innerHTML.

const POLL_MS = 5000;
const KINDS = [
  ["weapon", "weapons"],
  ["armour", "armour"],
  ["accessory", "accessories"],
  ["outfit", "outfits"],
  ["wings", "wings"],
];

let data = null;
let lastKey = "";

function stored(key) {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}
function store(key, value) {
  try {
    localStorage.setItem(key, value);
  } catch {
    // Not remembered, then.
  }
}
let kind = stored("collection-kind") || "weapon";
if (kind === "other") kind = "weapon";
let slot = stored("collection-slot") || "";
let show = stored("collection-show") || "all";
let query = "";

// Icons by name, loaded when their row scrolls into view.
const iconUrls = new Map();
const pending = new Map();
const seen = new IntersectionObserver((entries) => {
  for (const e of entries) {
    if (!e.isIntersecting) continue;
    seen.unobserve(e.target);
    loadIcon(e.target.dataset.icon);
  }
});
function loadIcon(name) {
  if (iconUrls.has(name) || pending.has(name)) return;
  pending.set(name, true);
  invoke("collection_icon", { icon: name })
    .then((url) => {
      iconUrls.set(name, url || null);
      if (!url) return;
      for (const ph of document.querySelectorAll(`.icon.blank[data-icon="${CSS.escape(name)}"]`)) {
        const img = el("img", ph.className.replace("blank", "").trim());
        img.src = url;
        img.alt = "";
        img.title = ph.title;
        ph.replaceWith(img);
      }
    })
    .catch(() => iconUrls.set(name, null))
    .finally(() => pending.delete(name));
}

function icon(e) {
  const grade = e.grade ? ` grade-${e.grade.toLowerCase()}` : "";
  const url = e.icon ? iconUrls.get(e.icon) : null;
  if (url) {
    const img = el("img", "icon" + grade);
    img.src = url;
    img.alt = "";
    if (e.grade) img.title = e.grade;
    return img;
  }
  const ph = el("span", "icon blank" + grade, (e.slot || e.kind || "?")[0]);
  if (e.grade) ph.title = e.grade;
  if (e.icon && !iconUrls.has(e.icon)) {
    ph.dataset.icon = e.icon;
    seen.observe(ph);
  }
  return ph;
}

const known = () => (kind === "wings" ? data.wings_known : data.skins_known);

function row(e) {
  const state = !known() ? "" : e.owned ? " maxed" : " need";
  const li = el("li", "bm-offer" + state);
  li.append(icon(e));
  const what = el("div", "bm-what");
  what.append(el("span", "bm-name", e.name || `skin ${e.id}`));
  const line = el("div", "bm-sub");
  const sub = [e.slot, e.faction, e.grade].filter(Boolean).join(" · ");
  if (sub) line.append(el("span", "note", sub));
  if (e.also.length) {
    const also = el("span", "note col-also", `looks like ${e.also.slice(0, 2).join(", ")}${e.also.length > 2 ? ` +${e.also.length - 2}` : ""}`);
    also.title = e.also.join("\n");
    line.append(also);
  }
  what.append(line);
  li.append(what);
  if (known()) {
    const have = el("span", "bm-have", !e.owned ? "missing" : e.kind === "wings" ? (e.count ? `have · +${e.count}` : "have") : e.count > 1 ? `have ×${e.count}` : "have");
    have.title = e.owned
      ? e.kind === "wings"
        ? "One of your characters has it (its highest enchant)"
        : "One of your characters has it (the game's count of it)"
      : "None of your characters has it yet";
    li.append(have);
  }
  return li;
}

function matches(e) {
  if (show === "have" && !e.owned) return false;
  if (show === "missing" && e.owned) return false;
  if (!query) return true;
  return [e.name, ...e.also].some((n) => n && n.toLowerCase().includes(query));
}

// "12/56".
const tally = (list) => `${list.filter((e) => e.owned).length}/${list.length}`;

function draw(force) {
  if (!data) return;
  const key = JSON.stringify([data, kind, slot, show, query]);
  if (!force && key === lastKey) return;
  lastKey = key;
  const of = (k) => data.entries.filter((e) => e.kind === k);
  // Kinds, each with how many you have; "other" only when there's some.
  $("kinds").replaceChildren(
    ...KINDS.map(([k, label]) => {
      const list = of(k);
      const b = el("button", "", `[${label} ${tally(list)}]`);
      b.type = "button";
      b.setAttribute("role", "tab");
      b.setAttribute("aria-selected", String(k === kind));
      b.addEventListener("click", () => {
        kind = k;
        slot = "";
        store("collection-kind", kind);
        store("collection-slot", slot);
        draw(true);
      });
      return b;
    }),
  );
  const all = of(kind);
  // Slots of this kind, in the data's order.
  const slots = [...new Set(all.map((e) => e.slot).filter(Boolean))];
  if (slot && !slots.includes(slot)) slot = "";
  $("slots").replaceChildren(
    ...(slots.length > 1 ? ["", ...slots] : []).map((s) => {
      const b = el("button", "", `[${s || "all"}]`);
      b.type = "button";
      b.setAttribute("role", "tab");
      b.setAttribute("aria-selected", String(s === slot));
      if (s) b.title = `${tally(all.filter((e) => e.slot === s))} of the ${s.toLowerCase()} skins`;
      b.addEventListener("click", () => {
        slot = s;
        store("collection-slot", slot);
        draw(true);
      });
      return b;
    }),
  );
  $("slots").hidden = slots.length < 2;
  for (const b of document.querySelectorAll("#show [data-show]")) b.setAttribute("aria-selected", String(b.dataset.show === show));
  const skins = data.entries.filter((e) => e.kind !== "wings");
  const wings = of("wings");
  $("count").textContent = data.skins_known || data.wings_known ? `· skins ${tally(skins)} · wings ${tally(wings)}` : "";
  // Grouped by slot (wings by faction), owned last within each so what's left stands out.
  const shown = all.filter((e) => (!slot || e.slot === slot) && matches(e));
  const groups = new Map();
  for (const e of shown) {
    const g = (kind === "wings" ? e.faction : e.slot) || "";
    if (!groups.has(g)) groups.set(g, []);
    groups.get(g).push(e);
  }
  const boxes = [];
  for (const [g, list] of groups) {
    const box = el("section", "ck-box bm-page");
    if (g) {
      const head = el("h2", "ck-head", g.toLowerCase());
      const whole = all.filter((e) => ((kind === "wings" ? e.faction : e.slot) || "") === g);
      if (known()) head.append(el("span", "note", ` · ${tally(whole)}`));
      box.append(head);
    }
    const ul = el("ul", "bm-list");
    ul.append(...list.sort((a, b) => (known() ? a.owned - b.owned : 0) || (a.name || "").localeCompare(b.name || "")).map(row));
    box.append(ul);
    boxes.push(box);
  }
  $("list").replaceChildren(...boxes);
  $("empty").hidden = boxes.length > 0;
  $("empty").textContent = query || show !== "all" ? "nothing matches" : "nothing of this kind in the data";
  $("hint").hidden = known();
}

async function load() {
  try {
    data = await invoke("collection");
    draw(false);
  } catch {
    // Next time, then.
  }
}

for (const b of document.querySelectorAll("#show [data-show]")) {
  b.addEventListener("click", () => {
    show = b.dataset.show;
    store("collection-show", show);
    draw(true);
  });
}
$("search").addEventListener("input", () => {
  query = $("search").value.trim().toLowerCase();
  draw(true);
});

load();
setInterval(() => {
  if (!document.hidden) load();
}, POLL_MS);
