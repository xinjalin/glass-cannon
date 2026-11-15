// The black market window: what the Black Cloud Merchant offers each character this hour
// (each page as the game last sent it), what each offer is (with its icon), how far along
// each pet is (maxed in the theme's colour, still to get in red), whether you have each
// skin, outfit and wings (the same colours), a Theostone's effect on hover, and how often
// you've seen it on offer. Item and character names are set with
// textContent, never innerHTML.

const POLL_MS = 3000;
// Seen this many times or fewer (all your characters' pages): rare, as far as you know.
const RARE_SEEN = 2;
// A page needs this many looks before "rare" means anything.
const RARE_AFTER_LOOKS = 10;

let data = null;
let character = null;
let show = "all";
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
show = stored("black-market-show") || "all";
let hideMaxed = stored("black-market-hide-maxed") === "1";

const label = (name) => name || "unknown";
const kinah = (n) => Math.round(n).toLocaleString("en-US");

// "in 23m", "in 1h 5m".
function until(ms) {
  const m = Math.max(0, Math.round((ms - Date.now()) / 60000));
  return m >= 60 ? `in ${Math.floor(m / 60)}h ${m % 60}m` : `in ${m}m`;
}

// Each offer's icon (a pet's portrait, a skin's or Theostone's icon), loaded once each;
// a letter for its kind until it comes, or when the data has none.
const offerIcons = new Map();
function itemIcon(o) {
  if (!offerIcons.has(o.item)) {
    offerIcons.set(o.item, null);
    invoke("shop_icon", { item: o.item })
      .then((url) => {
        offerIcons.set(o.item, url);
        if (url) draw(true);
      })
      .catch(() => {});
  }
  const url = offerIcons.get(o.item);
  // Framed in its grade's colour, as the game colours them.
  const grade = o.grade ? ` grade-${o.grade.toLowerCase()}` : "";
  if (!url) return el("span", "icon blank" + grade, o.kind ? o.kind[0] : "?");
  const img = el("img", "icon" + grade);
  img.src = url;
  img.alt = "";
  if (o.grade) img.title = o.grade;
  return img;
}

// A pet at its highest level: nothing more to get from it.
const maxed = (o) => o.pet != null && o.pet_level > 0 && o.pet_next == null;
// Nothing to get: a maxed pet, or a skin, outfit or wings one of your characters has.
const done = (o) => maxed(o) || o.owned === true;

// Worth a look: a pet, skin or wings you don't have, or something you've rarely seen offered.
function worth(o, looks) {
  return (o.pet != null && o.pet_level === 0) || o.owned === false || (looks >= RARE_AFTER_LOOKS && o.seen <= RARE_SEEN);
}

function offerRow(o, page) {
  // A pet: maxed in the theme's colour, still to get (or to level) in red. A skin, outfit
  // or wings: had in the theme's colour, missing in red.
  const state = o.pet != null ? (maxed(o) ? " maxed" : " need") : o.owned === true ? " maxed" : o.owned === false ? " need" : "";
  const li = el("li", "bm-offer" + (o.sold_out ? " sold" : "") + (worth(o, page.looks) ? " worth" : "") + state);
  li.append(itemIcon(o));
  const what = el("div", "bm-what");
  const name = el("span", "bm-name", o.name || `item ${o.item}`);
  if (!o.name) name.title = "Not in the item data";
  // A Theostone: what it does.
  if (o.effect) li.title = o.effect;
  what.append(name);
  const sub = [o.kind, o.detail].filter(Boolean).join(" · ");
  if (o.count > 1) what.append(el("span", "bm-count", ` ×${o.count}`));
  const line = el("div", "bm-sub");
  if (sub) line.append(el("span", "note", sub));
  if (o.pet != null) {
    const have = o.pet_level > 0;
    // Its level and souls toward the next ("Lv 2 · 25/75"), with a bar; or maxed.
    const progress = el("span", "bm-pet");
    if (maxed(o)) {
      progress.textContent = `Lv ${o.pet_level} · maxed`;
      progress.title = "At its highest level on one of your characters (as the game last sent your pets)";
    } else {
      const souls = o.pet_souls || 0;
      const next = o.pet_next || 0;
      progress.append(`${have ? `Lv ${o.pet_level}` : "not yet"} · ${souls}/${next}`);
      const bar = el("span", "bm-pet-bar");
      const fill = el("span", "bm-pet-fill");
      fill.style.width = `${next ? Math.min(100, (souls / next) * 100) : 0}%`;
      bar.append(fill);
      progress.append(bar);
      progress.title = have
        ? `Level ${o.pet_level}: ${souls} of the ${next} souls the next level takes (your characters' highest, as the game last sent your pets)`
        : `None of your characters has it yet: ${souls} of the ${next} souls it takes to summon (as the game last sent your pets)`;
    }
    line.append(progress);
  }
  if (o.owned != null) {
    const tag = el("span", "bm-have", o.owned ? "have" : "missing");
    tag.title = o.owned
      ? "One of your characters has it (as the game last sent your collection)"
      : "None of your characters has it yet (as the game last sent your collection)";
    line.append(tag);
  }
  const seen = el("span", "bm-seen note", o.seen <= 1 ? "first time seen" : `seen ${o.seen}× in ${page.looks} looks`);
  seen.title = `On offer in ${o.seen} of the ${page.looks} hourly ${page.name} pages you've opened (all characters), first ${dayAndTime(o.first_ms)}`;
  line.append(seen);
  what.append(line);
  li.append(what);
  const cost = el("div", "bm-cost");
  cost.append(el("span", "bm-price", kinah(o.discounted_price)));
  if (o.percent_off) {
    const off = el("span", "bm-off", `−${o.percent_off}%`);
    off.title = `${kinah(o.price)} before the discount`;
    cost.append(off);
  }
  if (o.sold_out) cost.append(el("span", "bm-tag sold", "bought"));
  li.append(cost);
  return li;
}

function draw(force) {
  if (!data) return;
  const key = JSON.stringify([data, character, show]);
  if (!force && key === lastKey) return;
  lastKey = key;
  const chars = data.characters;
  if (character == null || !chars.some((c) => c.name === character)) {
    const you = chars.find((c) => c.name === data.you);
    character = you ? you.name : chars.length ? chars[0].name : null;
  }
  $("characters").replaceChildren(
    ...chars.map((c) => {
      const b = el("button", "", `[${label(c.name)}]`);
      b.type = "button";
      b.setAttribute("role", "tab");
      b.setAttribute("aria-selected", String(c.name === character));
      if (c.name === data.you) b.title = "playing now";
      b.addEventListener("click", () => {
        character = c.name;
        draw(true);
      });
      return b;
    }),
  );
  $("characters").hidden = chars.length < 2;
  for (const b of document.querySelectorAll("#show [data-show]")) b.setAttribute("aria-selected", String(b.dataset.show === show));
  $("hide-maxed").setAttribute("aria-pressed", String(hideMaxed));
  $("count").textContent = `· new offers ${until(data.next_refresh_ms)}`;
  $("count").title = `Every character's offers change on the hour (${timeOfDay(data.next_refresh_ms)})`;
  const c = chars.find((x) => x.name === character);
  const boxes = [];
  for (const page of c ? c.pages : []) {
    const shown = page.offers.filter((o) => show === "all" || worth(o, page.looks));
    const offers = hideMaxed ? shown.filter((o) => !done(o)) : shown;
    const hidden = shown.length - offers.length;
    if (!offers.length && !hidden && show !== "all") continue;
    const box = el("section", "ck-box bm-page" + (page.stale ? " stale" : ""));
    const head = el("h2", "ck-head", page.name);
    const when = page.stale ? ` · from ${dayAndTime(page.seen_ms)}, changed since: open it again` : ` · opened ${timeOfDay(page.seen_ms)}`;
    head.append(el("span", "note", when));
    box.append(head);
    const ul = el("ul", "bm-list");
    ul.append(...offers.map((o) => offerRow(o, page)));
    if (!offers.length && !hidden) ul.append(el("li", "note", "nothing on this page"));
    box.append(ul);
    if (hidden) box.append(el("p", "note bm-hidden", `${hidden} you have hidden`));
    boxes.push(box);
  }
  $("pages").replaceChildren(...boxes);
  $("empty").hidden = boxes.length > 0;
  $("empty").textContent = !c
    ? "open the Black Cloud Merchant in the shop (H) with the meter open: its offers show here"
    : show === "worth"
      ? "nothing worth a look on the pages you've opened this hour"
      : "no pages yet for this character";
}

async function load() {
  try {
    data = await invoke("black_market");
    draw(false);
  } catch {
    // Next time, then.
  }
}

for (const b of document.querySelectorAll("#show [data-show]")) {
  b.addEventListener("click", () => {
    show = b.dataset.show;
    store("black-market-show", show);
    draw(true);
  });
}

$("hide-maxed").addEventListener("click", () => {
  hideMaxed = !hideMaxed;
  store("black-market-hide-maxed", hideMaxed ? "1" : "0");
  draw(true);
});

load();
setInterval(() => {
  if (!document.hidden) load();
}, POLL_MS);
