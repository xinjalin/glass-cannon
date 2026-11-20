// The player bars: your HP and MP, and the cast you're making, as thin bars (the vitals
// arcs widget shows them round, around your character). Max HP is the game's; until it
// sends it, the highest HP seen stands in. The bars share the widget's height; the cast's
// can be turned off (the cast bar's settings), for the cast bar widget to have it alone.

// Smooth enough for a cast bar.
const POLL_MS = 100;

// Whether the cast shows here too (castbar.rs).
let showCast = true;

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
  if (unchanged("bars", [b, showCast])) return;
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
  const c = showCast ? b.cast : null;
  const cast = document.querySelector(".pb.cast");
  // Its row stays between casts (the HP and MP bars keep their height); none with it off.
  cast.hidden = !showCast;
  cast.classList.toggle("idle", !c);
  if (c) {
    // Fills as the cast goes on.
    cast.querySelector(".pb-fill").style.width = `${(1 - c.remaining_ms / Math.max(1, c.cast_ms)) * 100}%`;
    // A charged skill held: to its top level (a notch at each), then full until let go.
    cast.querySelector(".pb-text").textContent = c.skill_name || (c.charge ? "charging" : "casting");
    cast.querySelector(".pb-time").textContent = c.charge && c.charge.levels > 1 && c.remaining_ms <= 0 ? "max" : (c.remaining_ms / 1000).toFixed(1);
  }
  // Glides between a charge's readings (not into its first).
  cast.classList.toggle("charging", !!(c && c.charge) && cast.classList.contains("charge"));
  cast.classList.toggle("charge", !!(c && c.charge));
  cast.classList.toggle("levels", !!(c && c.charge && c.charge.levels > 1));
}

tauri.event.listen("cast-bar-setup", (e) => {
  if (e.payload) showCast = e.payload.player_cast !== false;
  refresh();
});
invoke("cast_bar_setup")
  .then((v) => {
    showCast = !v || v.player_cast !== false;
    refresh();
  })
  .catch(() => {});
onTick(POLL_MS, () => {
  if (!document.hidden) refresh();
});
refresh();
