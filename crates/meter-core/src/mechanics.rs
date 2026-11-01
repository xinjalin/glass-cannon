//! Boss mechanics: hand-written warnings (`gamedata/boss-mechanics.json`) for what the
//! general rule (a long cast, or one that marks the ground) misses. Each one goes off on:
//!
//! - an effect: a buff on the boss (Lakshmi's reflect), or a debuff on you, another player
//!   or any player (a bomb to carry away, an ally to free);
//! - a cast: the boss casts a cue skill, until it casts the one that ends it;
//! - the boss's stagger check: its gauge turning breakable;
//! - a hit on you from something on the ground (a pool).
//!
//! The meter feeds it the stagger gauges and the hits on you as they come; effects and
//! casts it reads from the meter's own logs when asked.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::buffs::EffectSpan;
use crate::state::{Stagger, STAGGERED};

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Book {
    #[serde(default)]
    pub mechanics: Vec<Mechanic>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Mechanic {
    pub id: String,
    /// The boss's mob types, one per mode.
    #[serde(default)]
    pub bosses: Vec<u32>,
    pub when: When,
    /// What it is, and what to do.
    pub what: String,
    pub say: String,
    /// When it lands, from its start: a countdown.
    #[serde(default)]
    pub due_s: Option<f64>,
    /// What to do once it's over.
    #[serde(default)]
    pub then: Option<Then>,
    /// The longest it shows.
    pub max_s: f64,
    #[serde(default)]
    pub priority: u8,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Then {
    pub say: String,
    pub s: f64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum When {
    Effect { effect: u32, on: On },
    /// Over when one of `until` goes off (a cast time counts down to it; its own skill:
    /// when that cast goes off).
    Cast {
        skills: Vec<u32>,
        #[serde(default)]
        until: Vec<u32>,
    },
    Stagger,
    Hit { skills: Vec<u32> },
}

/// Whom an effect is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum On {
    Boss,
    You,
    /// Another player.
    Ally,
    /// Any player, you included.
    Player,
}

/// A mechanic going on now.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Alert {
    /// The mechanic and when it started: one chime per key.
    pub key: String,
    pub what: String,
    pub say: String,
    /// The players it's on, for an ally's.
    pub who: Vec<String>,
    pub priority: u8,
    /// Until it lands or ends, when known.
    pub left_ms: Option<u32>,
    /// It's over: this is what to do next.
    pub after: bool,
}

/// A boss cast, as the meter logged it.
#[derive(Debug, Clone, Copy)]
pub struct CastSeen {
    pub at_us: u64,
    pub fired_us: Option<u64>,
    pub caster: u64,
    pub caster_type: Option<u32>,
    pub skill: u32,
    pub cast_ms: u32,
}

/// What the meter knows right now, for [`Mechanics::alerts`].
pub struct Now<'a> {
    pub now_us: u64,
    /// Effects of the last minute or so, open and closed.
    pub effects: &'a [EffectSpan],
    pub casts: &'a [CastSeen],
    pub you: Option<u64>,
    pub mob_type: &'a dyn Fn(u64) -> Option<u32>,
    pub player_name: &'a dyn Fn(u64) -> Option<String>,
}

/// How far back [`Now`]'s effects and casts should go: the longest a mechanic and its
/// follow-up show, with room to spare.
pub const LOOK_BACK_US: u64 = 60_000_000;

#[derive(Debug, Default)]
pub struct Mechanics {
    book: Book,
    /// Stagger gauges of the bosses with a stagger check.
    gauges: HashMap<u64, Gauge>,
    /// When those bosses spawned.
    spawns: HashMap<u64, u64>,
    /// Ground hits on you: skill -> (first of the run of hits, latest).
    hits: HashMap<u32, (u64, u64)>,
}

#[derive(Debug, Clone, Copy)]
struct Gauge {
    mob_type: Option<u32>,
    /// The check going on: since when.
    open: Option<u64>,
    /// Its last message said it was idle (it can't be broken).
    idle: bool,
}

/// A gauge first seen this long after its boss spawned is a check opening; one there
/// from the start is the boss's own (Lakshmi's comes 0.2 s after her spawn, Bakarma's
/// check 54 s after his).
const CHECK_AFTER_SPAWN_US: u64 = 5_000_000;

fn us(s: f64) -> u64 {
    (s.max(0.0) * 1e6) as u64
}

fn ms_left(until_us: u64, now_us: u64) -> Option<u32> {
    (until_us > now_us).then(|| ((until_us - now_us) / 1000) as u32)
}

impl Mechanics {
    pub fn set_book(&mut self, book: Book) {
        self.book = book;
    }

    pub fn is_empty(&self) -> bool {
        self.book.mechanics.is_empty()
    }

    pub fn clear(&mut self) {
        self.gauges.clear();
        self.spawns.clear();
        self.hits.clear();
    }

    fn checks_stagger(&self, mob_type: Option<u32>) -> bool {
        self.book.mechanics.iter().any(|m| matches!(m.when, When::Stagger) && mob_type.is_some_and(|t| m.bosses.contains(&t)))
    }

    /// A mob spawning.
    pub fn on_spawn(&mut self, ts_us: u64, mob: u64, mob_type: u32) {
        if self.checks_stagger(Some(mob_type)) {
            self.spawns.insert(mob, ts_us);
            self.gauges.remove(&mob);
        }
    }

    /// A mob's stagger gauge. A check opens when it turns breakable after idling, or first
    /// shows up a while after the boss spawned; it's over once broken or idle again. A
    /// gauge breakable from the start, or again right after a break, is no check.
    pub fn on_stagger(&mut self, ts_us: u64, mob: u64, mob_type: Option<u32>, s: &Stagger) {
        if !self.checks_stagger(mob_type) {
            return;
        }
        let spawned = self.spawns.get(&mob).copied();
        let first = !self.gauges.contains_key(&mob);
        let g = self.gauges.entry(mob).or_insert(Gauge { mob_type, open: None, idle: false });
        if s.state == STAGGERED || s.breakable == Some(false) {
            g.open = None;
            g.idle = s.state != STAGGERED;
            return;
        }
        if s.breakable == Some(true) {
            let late = first && spawned.is_some_and(|at| ts_us.saturating_sub(at) >= CHECK_AFTER_SPAWN_US);
            if g.open.is_none() && (g.idle || late) {
                g.open = Some(ts_us);
            }
            g.idle = false;
        }
    }

    /// A mob's hit on you.
    pub fn on_hit_you(&mut self, ts_us: u64, skill: u32) {
        // Hits closer together than the warning lasts are one pool.
        let gap = self.book.mechanics.iter().filter(|m| matches!(&m.when, When::Hit { skills } if skills.contains(&skill))).map(|m| us(m.max_s)).max();
        let Some(gap) = gap else { return };
        let e = self.hits.entry(skill).or_insert((ts_us, ts_us));
        if ts_us.saturating_sub(e.1) > gap {
            e.0 = ts_us;
        }
        e.1 = ts_us;
    }

    /// The mechanics going on now, most important first.
    pub fn alerts(&self, n: &Now) -> Vec<Alert> {
        let mut out: Vec<Alert> = self.book.mechanics.iter().filter_map(|m| self.alert(m, n)).collect();
        out.sort_by(|a, b| b.priority.cmp(&a.priority).then(a.after.cmp(&b.after)));
        out
    }

    fn alert(&self, m: &Mechanic, n: &Now) -> Option<Alert> {
        let max = us(m.max_s);
        let boss = |t: Option<u32>| m.bosses.is_empty() || t.is_some_and(|t| m.bosses.contains(&t));
        let alert = |start: u64, left_ms: Option<u32>, who: Vec<String>| Alert {
            key: format!("{}:{start}", m.id),
            what: m.what.clone(),
            say: m.say.clone(),
            who,
            priority: m.priority,
            left_ms: m.due_s.and_then(|d| ms_left(start + us(d), n.now_us)).or(left_ms),
            after: false,
        };
        // Its follow-up, for a while after it ended.
        let after = |start: u64, end: u64| {
            let t = m.then.as_ref()?;
            (n.now_us >= end && n.now_us - end <= us(t.s)).then(|| Alert {
                key: format!("{}:{start}:then", m.id),
                what: m.what.clone(),
                say: t.say.clone(),
                who: Vec::new(),
                priority: m.priority,
                left_ms: ms_left(end + us(t.s), n.now_us),
                after: true,
            })
        };
        match &m.when {
            When::Effect { effect, on } => {
                let on_it = |s: &EffectSpan| match on {
                    On::Boss => boss((n.mob_type)(s.target)),
                    On::You => n.you == Some(s.target),
                    On::Ally => n.you != Some(s.target) && (n.player_name)(s.target).is_some(),
                    On::Player => n.you == Some(s.target) || (n.player_name)(s.target).is_some(),
                };
                let spans: Vec<&EffectSpan> = n.effects.iter().filter(|s| s.effect == *effect && on_it(s)).collect();
                let live: Vec<&&EffectSpan> = spans.iter().filter(|s| s.end_us.is_none() && n.now_us.saturating_sub(s.start_us) <= max).collect();
                if let Some(first) = live.iter().min_by_key(|s| s.start_us) {
                    let mut who: Vec<String> = Vec::new();
                    if *on == On::Ally {
                        for s in &live {
                            if let Some(name) = (n.player_name)(s.target).filter(|x| !who.contains(x)) {
                                who.push(name);
                            }
                        }
                    }
                    let ends = (first.duration_ms > 0).then(|| first.start_us + u64::from(first.duration_ms) * 1000);
                    return Some(alert(first.start_us, ends.and_then(|e| ms_left(e.min(first.start_us + max), n.now_us)), who));
                }
                spans.iter().rev().find_map(|s| after(s.start_us, s.end_us?))
            }
            When::Cast { skills, until } => {
                let c = n.casts.iter().rev().find(|c| skills.contains(&c.skill) && boss(c.caster_type) && n.now_us.saturating_sub(c.at_us) <= max)?;
                // When a cast goes off: a cast time's end (Vakron's prison lasts until its
                // 4.5 s explosion goes off, not until it starts), else at once.
                let goes_off = |o: &CastSeen| match o.cast_ms {
                    0 => o.at_us,
                    ms => o.fired_us.unwrap_or(o.at_us + u64::from(ms) * 1000),
                };
                // Over once the cast that ends it goes off.
                let ends = if until.contains(&c.skill) && c.cast_ms > 0 {
                    Some(goes_off(c))
                } else {
                    n.casts.iter().find(|o| o.caster == c.caster && o.at_us > c.at_us && until.contains(&o.skill)).map(goes_off)
                };
                match ends {
                    Some(end) if end <= n.now_us => after(c.at_us, end),
                    _ => {
                        let casting = (c.cast_ms > 0 && c.fired_us.is_none()).then(|| c.at_us + u64::from(c.cast_ms) * 1000);
                        Some(alert(c.at_us, ends.or(casting).and_then(|e| ms_left(e, n.now_us)), Vec::new()))
                    }
                }
            }
            When::Stagger => {
                let since = self.gauges.values().filter(|g| boss(g.mob_type)).filter_map(|g| g.open).filter(|since| n.now_us.saturating_sub(*since) <= max).min()?;
                Some(alert(since, None, Vec::new()))
            }
            When::Hit { skills } => {
                let (first, _) = skills
                    .iter()
                    .filter_map(|s| self.hits.get(s))
                    .filter(|(_, last)| n.now_us.saturating_sub(*last) <= max)
                    .min_by_key(|(first, _)| *first)?;
                Some(alert(*first, None, Vec::new()))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const S: u64 = 1_000_000;
    const YOU: u64 = 10;
    const ALLY: u64 = 11;
    const BOSS: u64 = 99;
    const LAKSHMI: u32 = 2_320_401;

    fn book(json: &str) -> Mechanics {
        let mut m = Mechanics::default();
        m.set_book(serde_json::from_str(json).unwrap());
        m
    }

    fn span(target: u64, effect: u32, start: u64, end: Option<u64>, duration_ms: u32) -> EffectSpan {
        EffectSpan { target, instance: start, effect, skill: None, source: BOSS, duration_ms, start_us: start, end_us: end }
    }

    fn alerts(m: &Mechanics, now_us: u64, effects: &[EffectSpan], casts: &[CastSeen]) -> Vec<Alert> {
        let mob_type = |e: u64| (e == BOSS).then_some(LAKSHMI);
        let player_name = |e: u64| match e {
            YOU => Some("Me".to_string()),
            ALLY => Some("Ally".to_string()),
            _ => None,
        };
        m.alerts(&Now { now_us, effects, casts, you: Some(YOU), mob_type: &mob_type, player_name: &player_name })
    }

    // Lakshmi's reflect as our own Draupnir runs show it: 8 s on herself, twice a fight.
    const REFLECT: &str = r#"{"mechanics":[{"id":"reflect","bosses":[2320401],"when":{"kind":"effect","effect":16059101,"on":"boss"},
        "what":"Lakshmi · reflect","say":"stop attacking","max_s":9,"priority":100}]}"#;

    #[test]
    fn a_buff_on_the_boss_warns_while_it_lasts() {
        let m = book(REFLECT);
        let reflect = span(BOSS, 16_059_101, 23 * S, None, 8000);
        let a = alerts(&m, 25 * S, std::slice::from_ref(&reflect), &[]);
        assert_eq!(a.len(), 1);
        assert_eq!((a[0].say.as_str(), a[0].left_ms), ("stop attacking", Some(6000)));
        // Gone once removed, and on anyone but the boss it's not hers.
        assert!(alerts(&m, 32 * S, &[span(BOSS, 16_059_101, 23 * S, Some(31 * S), 8000)], &[]).is_empty());
        assert!(alerts(&m, 25 * S, &[span(ALLY, 16_059_101, 23 * S, None, 8000)], &[]).is_empty());
        // A remove we missed doesn't keep it up.
        assert!(alerts(&m, 33 * S, &[reflect], &[]).is_empty());
    }

    #[test]
    fn a_debuff_on_you_or_an_ally_and_its_follow_up() {
        let m = book(
            r#"{"mechanics":[
            {"id":"bomb","when":{"kind":"effect","effect":7,"on":"you"},"what":"bomb on you","say":"away from the party",
             "then":{"say":"out of the fire","s":2},"max_s":6,"priority":90},
            {"id":"jail","when":{"kind":"effect","effect":8,"on":"ally"},"what":"jailed","say":"free them","max_s":15,"priority":80}]}"#,
        );
        let a = alerts(&m, 2 * S, &[span(YOU, 7, S, None, 0), span(ALLY, 8, S, None, 0), span(YOU, 8, S, None, 0)], &[]);
        assert_eq!(a.iter().map(|x| x.say.as_str()).collect::<Vec<_>>(), ["away from the party", "free them"]);
        assert_eq!(a[1].who, ["Ally"]);
        // On you only: the ally's isn't yours, yours isn't an ally's.
        assert!(alerts(&m, 2 * S, &[span(ALLY, 7, S, None, 0)], &[]).is_empty());
        assert!(alerts(&m, 2 * S, &[span(YOU, 8, S, None, 0)], &[]).is_empty());
        // Over: what to do next, for 2 s.
        let done = [span(YOU, 7, S, Some(3 * S), 0)];
        let a = alerts(&m, 4 * S, &done, &[]);
        assert_eq!((a[0].say.as_str(), a[0].after, a[0].left_ms), ("out of the fire", true, Some(1000)));
        assert!(alerts(&m, 6 * S, &done, &[]).is_empty());
    }

    #[test]
    fn a_due_time_counts_down_from_the_start() {
        let m = book(
            r#"{"mechanics":[{"id":"stack","when":{"kind":"effect","effect":5,"on":"player"},"what":"x","say":"stack together",
             "due_s":10.75,"max_s":12}]}"#,
        );
        let a = alerts(&m, 4 * S, &[span(ALLY, 5, S, None, 10770)], &[]);
        assert_eq!(a[0].left_ms, Some(7750));
    }

    fn cast(at: u64, skill: u32, cast_ms: u32, fired: Option<u64>) -> CastSeen {
        CastSeen { at_us: at, fired_us: fired, caster: BOSS, caster_type: Some(LAKSHMI), skill, cast_ms }
    }

    #[test]
    fn a_cue_cast_warns_until_the_cast_that_ends_it() {
        let m = book(
            r#"{"mechanics":[
            {"id":"flower","bosses":[2320401],"when":{"kind":"cast","skills":[1],"until":[2]},"what":"x","say":"kill the flower","max_s":7},
            {"id":"rings","bosses":[2320401],"when":{"kind":"cast","skills":[3],"until":[3]},"what":"x","say":"jump","max_s":15}]}"#,
        );
        assert_eq!(alerts(&m, 2 * S, &[], &[cast(S, 1, 0, Some(S))])[0].say, "kill the flower");
        assert!(alerts(&m, 4 * S, &[], &[cast(S, 1, 0, Some(S)), cast(3 * S, 2, 0, Some(3 * S))]).is_empty());
        assert!(alerts(&m, 9 * S, &[], &[cast(S, 1, 0, Some(S))]).is_empty());
        // Its own skill ends it when the cast goes off; until then, the cast's time left.
        let a = alerts(&m, 2 * S, &[], &[cast(S, 3, 4000, None)]);
        assert_eq!((a[0].say.as_str(), a[0].left_ms), ("jump", Some(3000)));
        assert!(alerts(&m, 6 * S, &[], &[cast(S, 3, 4000, Some(5 * S))]).is_empty());
        // A cast time on the one that ends it: up, counting down, until it goes off.
        let prison = book(r#"{"mechanics":[{"id":"prison","when":{"kind":"cast","skills":[4],"until":[5]},"what":"x","say":"break out","max_s":15}]}"#);
        let a = alerts(&prison, 4 * S, &[], &[cast(S, 4, 0, Some(S)), cast(3 * S, 5, 4500, None)]);
        assert_eq!((a[0].say.as_str(), a[0].left_ms), ("break out", Some(3500)));
        assert!(alerts(&prison, 8 * S, &[], &[cast(S, 4, 0, Some(S)), cast(3 * S, 5, 4500, Some(7_500_000))]).is_empty());
        // Another boss's cast of the same skill isn't this one.
        let other = CastSeen { caster_type: Some(1), ..cast(S, 1, 0, Some(S)) };
        assert!(alerts(&m, 2 * S, &[], &[other]).is_empty());
    }

    #[test]
    fn the_stagger_gauge_and_ground_hits() {
        let mut m = book(
            r#"{"mechanics":[
            {"id":"stagger","bosses":[2320401],"when":{"kind":"stagger"},"what":"x","say":"stagger it now","max_s":25},
            {"id":"pool","when":{"kind":"hit","skills":[1605144]},"what":"x","say":"move out","max_s":1.5}]}"#,
        );
        let gauge = |left: u32, breakable: bool| Stagger { state: 1, left, size: 557, breakable: Some(breakable) };
        let broken = Stagger { state: STAGGERED, left: 0, size: 0, breakable: Some(true) };
        // As Bakarma's in our Draupnir run: no gauge until the check, 54 s after he spawned.
        m.on_spawn(0, BOSS, LAKSHMI);
        m.on_stagger(54 * S, BOSS, Some(LAKSHMI), &gauge(557, true));
        m.on_stagger(60 * S, BOSS, Some(LAKSHMI), &gauge(300, true));
        let a = alerts(&m, 66 * S, &[], &[]);
        assert_eq!((a[0].say.as_str(), a[0].key.as_str()), ("stagger it now", "stagger:54000000"));
        // Broken: over, and the gauge filling up idle again is no check.
        m.on_stagger(68 * S, BOSS, Some(LAKSHMI), &broken);
        assert!(alerts(&m, 68 * S, &[], &[]).is_empty());
        m.on_stagger(73 * S, BOSS, Some(LAKSHMI), &gauge(557, false));
        assert!(alerts(&m, 74 * S, &[], &[]).is_empty());
        // The next check: breakable after idling; idle again (not broken in time) ends it.
        m.on_stagger(109 * S, BOSS, Some(LAKSHMI), &gauge(557, true));
        assert_eq!(alerts(&m, 110 * S, &[], &[])[0].key, "stagger:109000000");
        m.on_stagger(119 * S, BOSS, Some(LAKSHMI), &gauge(240, false));
        assert!(alerts(&m, 119 * S, &[], &[]).is_empty());
        // Idle from its spawn (Atiel's in Trans Deus): nothing until the check.
        m.on_spawn(200 * S, BOSS, LAKSHMI);
        m.on_stagger(200 * S, BOSS, Some(LAKSHMI), &gauge(480, false));
        assert!(alerts(&m, 201 * S, &[], &[]).is_empty());
        // A gauge breakable from the spawn on, and again after each break, is the boss's
        // own (Lakshmi's), never a check.
        m.on_spawn(300 * S, BOSS, LAKSHMI);
        m.on_stagger(300 * S, BOSS, Some(LAKSHMI), &gauge(557, true));
        m.on_stagger(320 * S, BOSS, Some(LAKSHMI), &broken);
        m.on_stagger(325 * S, BOSS, Some(LAKSHMI), &gauge(557, true));
        assert!(alerts(&m, 326 * S, &[], &[]).is_empty());
        // A pool's ticks keep one warning up; a later pool is a new one.
        m.on_hit_you(10 * S, 1_605_144);
        m.on_hit_you(11 * S, 1_605_144);
        m.on_hit_you(12 * S, 1_600_000);
        assert_eq!(alerts(&m, 12 * S, &[], &[])[0].key, "pool:10000000");
        assert!(alerts(&m, 13 * S, &[], &[]).is_empty());
        m.on_hit_you(20 * S, 1_605_144);
        assert_eq!(alerts(&m, 20 * S, &[], &[])[0].key, "pool:20000000");
    }

    #[test]
    fn the_shipped_book_reads() {
        let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../gamedata/boss-mechanics.json")).unwrap();
        let b: Book = serde_json::from_str(&text).unwrap();
        assert!(b.mechanics.len() >= 30);
        let mut ids: Vec<&str> = b.mechanics.iter().map(|m| m.id.as_str()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), b.mechanics.len(), "ids are unique");
    }
}

