// The party frames' settings (widgets.rs → PartyLook), beside them while the overlay's
// arranged. Each change shows on the frames at once.

let look = null;
let timer = null;
// Saves on their way: what the app says meanwhile may be from before them.
let saving = 0;

function press(b, on) {
  b.setAttribute("aria-pressed", String(!!on));
}

async function set(patch, soon) {
  look = { ...look, ...patch };
  draw();
  clearTimeout(timer);
  const go = async () => {
    timer = null;
    // What comes back is taken only if nothing's changed here since it was sent.
    const sent = look;
    saving++;
    const saved = await invoke("set_party_look", { look: sent }).catch(() => sent);
    saving--;
    if (look === sent) look = saved;
  };
  if (soon) timer = setTimeout(go, 150);
  else await go();
}

function draw() {
  if (!look) return;
  for (const c of document.querySelectorAll(".bs-choice[data-key]")) for (const b of c.children) press(b, look[c.dataset.key] === b.dataset.v);
  for (const t of document.querySelectorAll(".bs-toggle[data-key]")) {
    press(t, look[t.dataset.key]);
    t.textContent = look[t.dataset.key] ? "[on]" : "[off]";
  }
  for (const r of document.querySelectorAll("input[type='range'][data-key]")) {
    if (document.activeElement !== r) r.value = String(look[r.dataset.key]);
  }
  for (const v of document.querySelectorAll(".bs-value[data-for]")) {
    const k = v.dataset.for;
    v.textContent = k === "size" ? `${look.size}%` : look[k] ? String(look[k]) : "fit";
  }
  // A setting the layout doesn't use: greyed, saying why.
  for (const line of document.querySelectorAll("[data-only]")) {
    const off = line.dataset.only !== look.layout;
    line.classList.toggle("off", off);
    for (const c of line.querySelectorAll("button, input")) c.disabled = off;
    line.title = off ? `No effect: not with the ${look.layout} layout` : "";
  }
}

for (const c of document.querySelectorAll(".bs-choice[data-key]")) {
  for (const b of c.children) b.addEventListener("click", () => look && set({ [c.dataset.key]: b.dataset.v }));
}
for (const t of document.querySelectorAll(".bs-toggle[data-key]")) {
  t.addEventListener("click", () => look && set({ [t.dataset.key]: !look[t.dataset.key] }));
}
for (const r of document.querySelectorAll("input[type='range'][data-key]")) {
  r.addEventListener("input", () => look && set({ [r.dataset.key]: Number(r.value) }, true));
}
invoke("party_look")
  .then((l) => {
    look = l;
    draw();
  })
  .catch(() => {});
tauri.event.listen("party-look", (e) => {
  if (e.payload && !timer && !saving) {
    look = e.payload;
    draw();
  }
});
