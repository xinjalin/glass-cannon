// The player bars: your HP and MP, and the cast you're making, as thin bars (the vitals
// arcs widget shows them round, around your character). Max HP is the game's; until it
// sends it, the highest HP seen stands in.

// Smooth enough for a cast bar.
const POLL_MS = 100;

function bar(cls, value, max, text) {
  const el_ = document.querySelector(`.pb.${cls}`);
  const known = value != null && max;
  el_.hidden = !known;
  if (!known) return;
  el_.querySelector(".pb-fill").style.width = `${Math.max(0, Math.min(1, value / max)) * 100}%`;
  el_.querySelector(".pb-text").textContent = text;
}

// The shield over the HP bar: on from the HP's end, or over its last part when HP and
// shield together pass the bar's end.
function shield(value, max, amount) {
  const s = document.querySelector(".pb.hp .pb-shield");
  const span = shieldSpan(value, max, amount);
  s.hidden = !span;
  if (!span) return;
  s.style.left = `${span[0] * 100}%`;
  s.style.width = `${span[1] * 100}%`;
}

async function refresh() {
  let b = null;
  try {
    b = await invoke("self_bars");
  } catch {
    return;
  }
  $("empty").hidden = !!(b && (b.hp != null || b.mp != null));
  if (!b) {
    for (const c of ["hp", "mp", "cast"]) document.querySelector(`.pb.${c}`).hidden = true;
    return;
  }
  bar("hp", b.hp, b.hp_max, b.hp != null ? `${fmt(b.hp)} / ${fmt(b.hp_max || b.hp)}` : "");
  shield(b.hp, b.hp_max, b.shield);
  // Its amount after your HP's, in its colour.
  if (b.hp != null && b.hp_max && b.shield) document.querySelector(".pb.hp .pb-text").append(el("span", "pb-shield-text", ` +${fmt(b.shield)}`));
  bar("mp", b.mp, b.mp_max, b.mp != null ? `${fmt(b.mp)} / ${fmt(b.mp_max || b.mp)}` : "");
  const c = b.cast;
  const cast = document.querySelector(".pb.cast");
  cast.hidden = !c;
  if (c) {
    // Fills as the cast goes on.
    cast.querySelector(".pb-fill").style.width = `${(1 - c.remaining_ms / Math.max(1, c.cast_ms)) * 100}%`;
    cast.querySelector(".pb-text").textContent = c.skill_name || "casting";
    cast.querySelector(".pb-time").textContent = (c.remaining_ms / 1000).toFixed(1);
  }
}

setInterval(() => {
  if (!document.hidden) refresh();
}, POLL_MS);
refresh();
