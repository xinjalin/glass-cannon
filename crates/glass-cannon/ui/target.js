// The target frame: what you've picked (or what you're fighting), its HP, how long until
// it dies at the current rate, what it's casting, when it enrages, who it's attacking and
// its stagger gauge.

const POLL_MS = 250;

async function refresh() {
  let t = null;
  try {
    t = await invoke("target");
  } catch {
    return;
  }
  $("frame").hidden = !t;
  $("empty").hidden = !!t;
  if (!t) return;
  $("tf-boss").hidden = !t.boss;
  $("tf-name").textContent = t.name;
  $("tf-name").title = (t.selected ? "Your target" : "What you're fighting") + (t.mob_type ? ` · mob ${t.mob_type}` : "");
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
  att.className = t.attacking_you ? "you" : "";
  att.title = "Who it's attacking";
  const left = t.berserk_us ? t.berserk_us / 1000 - Date.now() : null;
  const enrage = $("tf-enrage");
  enrage.hidden = left == null || left < -60_000 || left > 3_600_000;
  enrage.textContent = left > 0 ? `enrage ${clock(left / 1000)}` : "enraged";
  enrage.className = left != null && left < 30_000 ? "soon" : "";
  const s = t.stagger;
  $("tf-stagger-box").hidden = !s || !s.size;
  if (s && s.size) {
    const broken = s.state === 3;
    $("tf-stagger-fill").style.width = `${broken ? 100 : Math.max(0, Math.min(100, (s.left / s.size) * 100))}%`;
    $("tf-stagger-fill").className = broken ? "broken" : "";
    $("tf-stagger-text").textContent = broken ? "staggered!" : `stagger ${fmt(s.left)} / ${fmt(s.size)}`;
  }
}

setInterval(() => {
  if (!document.hidden) refresh();
}, POLL_MS);
refresh();
