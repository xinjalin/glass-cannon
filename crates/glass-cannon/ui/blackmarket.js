// The black market window: what the Black Cloud Merchant offers each character this hour
// (each page as the game last sent it), what each offer is, whether you have the pet yet,
// and how often you've seen it on offer. Item and character names are set with
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

const label = (name) => name || "unknown";
const kinah = (n) => Math.round(n).toLocaleString("en-US");

// "in 23m", "in 1h 5m".
function until(ms) {
  const m = Math.max(0, Math.round((ms - Date.now()) / 60000));
  return m >= 60 ? `in ${Math.floor(m / 60)}h ${m % 60}m` : `in ${m}m`;
}

// Pet portraits, loaded once each.
const portraits = new Map();
function portrait(pet) {
  if (!portraits.has(pet)) {
    portraits.set(pet, null);
    invoke("pet_icon", { pet })
      .then((url) => {
        portraits.set(pet, url);
        if (url) draw(true);
      })
      .catch(() => {});
  }
  const url = portraits.get(pet);
  if (!url) return el("span", "icon blank");
  const img = el("img", "icon");
  img.src = url;
  img.alt = "";
  return img;
}

// Worth a look: a pet you don't have, or something you've rarely seen offered.
function worth(o, looks) {
  return (o.pet != null && o.pet_level === 0) || (looks >= RARE_AFTER_LOOKS && o.seen <= RARE_SEEN);
}

function offerRow(o, page) {
  const li = el("li", "bm-offer" + (o.sold_out ? " sold" : "") + (worth(o, page.looks) ? " worth" : ""));
  li.append(o.pet != null ? portrait(o.pet) : el("span", "icon blank", o.kind ? o.kind[0] : "?"));
  const what = el("div", "bm-what");
  const name = el("span", "bm-name", o.name || `item ${o.item}`);
  if (!o.name) name.title = "Not in the item data";
  what.append(name);
  const sub = [o.kind, o.detail].filter(Boolean).join(" · ");
  if (o.count > 1) what.append(el("span", "bm-count", ` ×${o.count}`));
  const line = el("div", "bm-sub");
  if (sub) line.append(el("span", "note", sub));
  if (o.pet != null) {
    const have = o.pet_level > 0;
    const tag = el("span", "bm-tag " + (have ? "have" : "new"), have ? `have · Lv ${o.pet_level}` : "you don't have it");
    tag.title = have ? "Your characters' highest level with this pet, as the game last sent it" : "None of your characters has this pet yet (as the game last sent your pets)";
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
  $("count").textContent = `· new offers ${until(data.next_refresh_ms)}`;
  $("count").title = `Every character's offers change on the hour (${timeOfDay(data.next_refresh_ms)})`;
  const c = chars.find((x) => x.name === character);
  const boxes = [];
  for (const page of c ? c.pages : []) {
    const offers = page.offers.filter((o) => show === "all" || worth(o, page.looks));
    if (!offers.length && show !== "all") continue;
    const box = el("section", "ck-box bm-page" + (page.stale ? " stale" : ""));
    const head = el("h2", "ck-head", page.name);
    const when = page.stale ? ` · from ${dayAndTime(page.seen_ms)}, changed since: open it again` : ` · opened ${timeOfDay(page.seen_ms)}`;
    head.append(el("span", "note", when));
    box.append(head);
    const ul = el("ul", "bm-list");
    ul.append(...offers.map((o) => offerRow(o, page)));
    if (!offers.length) ul.append(el("li", "note", "nothing on this page"));
    box.append(ul);
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

load();
setInterval(() => {
  if (!document.hidden) load();
}, POLL_MS);
