// The target frame: what you've picked (or what you're fighting), its HP, how long until
// it dies at the current rate, what it's casting, when it enrages, who it's attacking, your
// best against it and its stagger gauge. (While it shows, the meter leaves out its own
// boss HP, enrage and cast lines.)

const POLL_MS = 250;

// How it looks (settings → overlay → target frame; targetframe.rs): these until read.
let setup = { class_look: "name", rank_style: "title", hidden: [] };

// The game's ranks, 1 the highest; 19 is a player without one (not on their class's
// board), the most common by far.
const RANKS = [
  "Supreme Commander",
  "Commander",
  "Marshal",
  "General",
  "5-Star Officer",
  "4-Star Officer",
  "3-Star Officer",
  "2-Star Officer",
  "1-Star Officer",
  "First-Class Soldier",
  "Second-Class Soldier",
  "Third-Class Soldier",
  "Fourth-Class Soldier",
  "Fifth-Class Soldier",
  "Sixth-Class Soldier",
  "Seventh-Class Soldier",
  "Eighth-Class Soldier",
  "Ninth-Class Soldier",
];
const UNRANKED = 19;

function rankText(rank) {
  if (rank == null) return "";
  if (setup.rank_style === "number") return rank >= UNRANKED ? "unranked" : `rank ${rank}`;
  return RANKS[rank - 1] || "unranked";
}

// The class look on the page, and the parts turned off.
function applySetup() {
  document.body.dataset.classLook = setup.class_look;
  const hidden = new Set(setup.hidden || []);
  for (const e of document.querySelectorAll("[data-part]")) e.classList.toggle("off", hidden.has(e.dataset.part));
}

// Class icons, loaded once each (null while loading or missing).
const classIcons = new Map();
function classIconUrl(code) {
  if (code == null) return null;
  if (!classIcons.has(code)) {
    classIcons.set(code, null);
    invoke("class_icon", { code })
      .then((url) => {
        classIcons.set(code, url);
        lastSeen.clear();
      })
      .catch(() => {});
  }
  return classIcons.get(code);
}

// A player you target: their class (in its colour), rank and combat power.
function drawPlayer(p) {
  $("tf-player").hidden = !p;
  // Their class's colour on the frame (as the settings have it): set on the panel, so a
  // whole-frame tint can reach its background.
  const cls = p && p.class_code ? p.class_code : null;
  $("panel").className = cls ? `has-class c${cls}` : "";
  if (!p) return;
  const icon = classIconUrl(p.class_code);
  $("tf-class-icon").hidden = !icon;
  if (icon && $("tf-class-icon").src !== icon) $("tf-class-icon").src = icon;
  $("tf-class").textContent = p.class_name || "class not known yet";
  $("tf-rank").textContent = rankText(p.rank);
  $("tf-rank").title = p.rank != null ? (p.rank >= UNRANKED ? "Not on their class's rank board" : `${RANKS[p.rank - 1]} (rank ${p.rank} of 18)`) : "";
  $("tf-cp").textContent = p.combat_power != null ? `CP ${fmt(p.combat_power)}` : "";
}

async function refresh() {
  let t = null;
  try {
    t = await invoke("target");
  } catch {
    return;
  }
  // Nothing new (the time to its enrage counts down by itself): the frame stays as it is.
  const enrageIn = t && t.berserk_us ? Math.round((t.berserk_us / 1000 - Date.now()) / 500) : null;
  if (unchanged("target", [t, enrageIn, setup])) return;
  $("frame").hidden = !t;
  $("empty").hidden = !!t;
  if (!t) return;
  $("tf-boss").hidden = !t.boss;
  $("tf-name").textContent = t.name;
  $("tf-name").title = (t.selected ? "Your target" : "What you're fighting") + (t.mob_type ? ` · mob ${t.mob_type}` : "");
  drawPlayer(t.player);
  const known = t.max_hp && t.hp != null;
  const pct = known ? Math.max(0, Math.min(1, t.hp / t.max_hp)) : null;
  $("tf-pct").textContent = pct == null ? "" : `${(pct * 100).toFixed(1)}%`;
  $("tf-hp-fill").style.width = `${pct == null ? 0 : pct * 100}%`;
  $("tf-hp-text").textContent = known ? `${fmt(t.hp)} / ${fmt(t.max_hp)}` : "HP not seen yet";
  $("tf-ttk").textContent = t.time_to_kill_s != null ? `dies in ~${clock(t.time_to_kill_s)}` : "";
  $("tf-ttk").title = "At the damage it took in the last 10 seconds";
  const c = t.cast;
  $("tf-cast-box").hidden = !c;
  if (c) {
    $("tf-cast-name").textContent = c.skill_name ? `casting ${c.skill_name}` : `casting (${(c.cast_ms / 1000).toFixed(1)} s)`;
    $("tf-cast-time").textContent = `${(c.remaining_ms / 1000).toFixed(1)} s`;
    $("tf-cast-fill").style.width = `${Math.max(0, Math.min(100, (1 - c.remaining_ms / c.cast_ms) * 100))}%`;
  }
  const att = $("tf-attacking");
  att.textContent = t.attacking ? `→ ${t.attacking_you ? "you" : t.attacking}` : "";
  att.classList.toggle("you", !!t.attacking_you);
  att.title = "Who it's attacking";
  const left = t.berserk_us ? t.berserk_us / 1000 - Date.now() : null;
  const enrage = $("tf-enrage");
  enrage.hidden = left == null || left < -60_000 || left > 3_600_000;
  enrage.textContent = left > 0 ? `enrage ${clock(left / 1000)}` : "enraged";
  enrage.classList.toggle("soon", left != null && left < 30_000);
  // Your best against this boss: green while you're doing better.
  const b = t.best;
  $("tf-best-line").hidden = !b;
  if (b) {
    $("tf-best").textContent = `best ${fmt(b.dps)} dps · fastest ${clock(b.kill_s)}`;
    $("tf-best").className = t.my_dps != null && t.my_dps > b.dps ? "beating" : "";
    $("tf-best").title = `Your best DPS against it, and its fastest kill (${b.kills} ${b.kills === 1 ? "kill" : "kills"})`;
  }
  const s = t.stagger;
  $("tf-stagger-box").hidden = !s || !s.size;
  if (s && s.size) {
    const broken = s.state === 3;
    $("tf-stagger-fill").style.width = `${broken ? 100 : Math.max(0, Math.min(100, (s.left / s.size) * 100))}%`;
    $("tf-stagger-fill").className = broken ? "broken" : "";
    $("tf-stagger-text").textContent = broken ? "staggered!" : `stagger ${fmt(s.left)} / ${fmt(s.size)}`;
  }
}

onTick(POLL_MS, () => {
  if (!document.hidden) refresh();
});
tauri.event.listen("target-setup", (e) => {
  if (e.payload) setup = e.payload;
  applySetup();
  refresh();
});
invoke("target_setup")
  .then((s) => {
    if (s) setup = s;
    applySetup();
    refresh();
  })
  .catch(() => {});
applySetup();
refresh();
