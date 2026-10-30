// The player bars: your HP and MP, and the cast you're making, as thin bars. Max HP isn't
// sent: the highest HP seen stands in (your max whenever you've been at full HP).

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
