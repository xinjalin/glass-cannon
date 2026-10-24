//! One analysis out of several fights (a dungeon run): the fights placed back to back on
//! one timeline, so time is time spent fighting, and every player's damage, healing and
//! damage taken added up.

use std::collections::BTreeMap;

use crate::meter::{Analysis, DeathView, EncounterView, Outcomes, PlayerRow, SkillRow};

/// Joins fights (oldest first) into one analysis. Returns it with where each fight starts
/// on the joined timeline (ms), or `None` when there are none.
pub fn join(parts: Vec<Analysis>) -> Option<(Analysis, Vec<u32>)> {
    let first_view = parts.first()?.view.clone();
    let mut starts = Vec::with_capacity(parts.len());
    let mut offset: u32 = 0;
    let mut out = Analysis {
        view: first_view,
        hits: Vec::new(),
        effects: Vec::new(),
        names: BTreeMap::new(),
        mob_types: BTreeMap::new(),
        target_hp: Vec::new(),
        heals: Vec::new(),
        taken: Vec::new(),
    };
    let mut views = Vec::with_capacity(parts.len());
    for part in parts {
        starts.push(offset);
        let shift = |t: u32| t.saturating_add(offset);
        out.hits.extend(part.hits.into_iter().map(|mut h| {
            h.t = shift(h.t);
            h
        }));
        out.heals.extend(part.heals.into_iter().map(|mut h| {
            h.t = shift(h.t);
            h
        }));
        out.taken.extend(part.taken.into_iter().map(|mut h| {
            h.t = shift(h.t);
            h
        }));
        out.effects.extend(part.effects.into_iter().map(|mut e| {
            e.start = shift(e.start);
            e.end = shift(e.end);
            e
        }));
        out.target_hp.extend(part.target_hp.into_iter().map(|mut hp| {
            for s in &mut hp.samples {
                s.t = shift(s.t);
            }
            hp
        }));
        out.names.extend(part.names);
        out.mob_types.extend(part.mob_types);
        offset = offset.saturating_add((part.view.duration_s * 1000.0).round() as u32);
        views.push(part.view);
    }
    out.view = join_views(&views);
    Some((out, starts))
}

fn join_views(views: &[EncounterView]) -> EncounterView {
    let duration_s: f64 = views.iter().map(|v| v.duration_s).sum();
    let total_damage: u64 = views.iter().map(|v| v.total_damage).sum();
    let total_healing: u64 = views.iter().map(|v| v.total_healing).sum();
    let total_taken: u64 = views.iter().map(|v| v.total_taken).sum();
    // The run is named after its biggest fight (usually its last boss).
    let main = views.iter().max_by_key(|v| v.total_damage).expect("at least one fight");
    EncounterView {
        active: false,
        duration_s,
        total_damage,
        total_dps: total_damage as f64 / duration_s.max(1.0),
        target: main.target.clone(),
        target_id: main.target_id,
        target_type: main.target_type,
        target_boss: main.target_boss,
        target_count: views.iter().map(|v| v.target_count).sum(),
        start_us: views.iter().map(|v| v.start_us).min().unwrap_or(0),
        end_us: views.iter().map(|v| v.end_us).max().unwrap_or(0),
        players: join_rows(views.iter().map(|v| &v.players), duration_s, total_damage),
        map: main.map,
        instance_us: main.instance_us,
        healing: join_rows(views.iter().map(|v| &v.healing), duration_s, total_healing),
        taken: join_rows(views.iter().map(|v| &v.taken), duration_s, total_taken),
        total_healing,
        total_taken,
        end: views.last().and_then(|v| v.end),
        // Each fight's deaths, timed from the run's start.
        deaths: views
            .iter()
            .scan(0.0, |offset, v| {
                let at = *offset;
                *offset += v.duration_s;
                Some(v.deaths.iter().map(move |d| DeathView { at_s: d.at_s + at, ..d.clone() }))
            })
            .flatten()
            .collect(),
        berserk_us: None,
        target_hp: None,
        target_max_hp: None,
        boss_casts: Vec::new(),
        boss_now_s: None,
        boss_cast: None,
    }
}

/// Each player's rows added up, highest first, with rates over `duration_s` and shares of
/// `total`.
fn join_rows<'a>(lists: impl Iterator<Item = &'a Vec<PlayerRow>>, duration_s: f64, total: u64) -> Vec<PlayerRow> {
    let mut rows: BTreeMap<u64, PlayerRow> = BTreeMap::new();
    let mut skills: BTreeMap<u64, BTreeMap<u32, SkillRow>> = BTreeMap::new();
    for row in lists.flatten() {
        let r = rows.entry(row.id).or_insert_with(|| PlayerRow {
            damage: 0,
            hits: 0,
            max_hit: 0,
            outcomes: Outcomes::default(),
            skills: Vec::new(),
            ..row.clone()
        });
        // A real name beats a bare "#ID" from a fight where the game hadn't named them.
        if r.name.starts_with('#') && !row.name.starts_with('#') {
            r.name.clone_from(&row.name);
        }
        r.is_self |= row.is_self;
        r.class_code = r.class_code.or(row.class_code);
        if r.class_name.is_none() {
            r.class_name.clone_from(&row.class_name);
        }
        r.damage += row.damage;
        r.hits += row.hits;
        r.max_hit = r.max_hit.max(row.max_hit);
        add_outcomes(&mut r.outcomes, &row.outcomes);
        let theirs = skills.entry(row.id).or_default();
        for s in &row.skills {
            let k = theirs.entry(s.skill).or_insert_with(|| SkillRow {
                damage: 0,
                hits: 0,
                max: 0,
                min: 0,
                outcomes: Outcomes::default(),
                ..s.clone()
            });
            k.damage += s.damage;
            k.hits += s.hits;
            k.max = k.max.max(s.max);
            k.min = match (k.min, s.min) {
                (0, m) | (m, 0) => m,
                (a, b) => a.min(b),
            };
            add_outcomes(&mut k.outcomes, &s.outcomes);
        }
    }
    let mut out: Vec<PlayerRow> = rows
        .into_iter()
        .map(|(id, mut r)| {
            r.dps = r.damage as f64 / duration_s.max(1.0);
            r.share = if total > 0 { r.damage as f64 / total as f64 } else { 0.0 };
            let mut list: Vec<SkillRow> = skills.remove(&id).unwrap_or_default().into_values().collect();
            for s in &mut list {
                s.share = if r.damage > 0 { s.damage as f64 / r.damage as f64 } else { 0.0 };
            }
            list.sort_by(|a, b| b.damage.cmp(&a.damage));
            r.skills = list;
            r
        })
        .collect();
    out.sort_by(|a, b| b.damage.cmp(&a.damage));
    out
}

fn add_outcomes(a: &mut Outcomes, b: &Outcomes) {
    a.casts += b.casts;
    a.crits += b.crits;
    a.back += b.back;
    a.front += b.front;
    a.perfect += b.perfect;
    a.double += b.double;
    a.multi += b.multi;
    a.multi_hits += b.multi_hits;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::meter::TimelineHit;

    fn row(id: u64, name: &str, damage: u64, skill: u32) -> PlayerRow {
        PlayerRow {
            id,
            name: name.into(),
            is_self: id == 1,
            class_code: Some(18),
            class_name: None,
            damage,
            dps: 0.0,
            share: 0.0,
            hits: 2,
            max_hit: damage / 2,
            outcomes: Outcomes { crits: 1, ..Outcomes::default() },
            skills: vec![SkillRow {
                skill,
                name: None,
                icon_key: None,
                source_type: None,
                damage,
                hits: 2,
                max: damage / 2,
                min: damage / 4,
                share: 1.0,
                outcomes: Outcomes::default(),
            }],
        }
    }

    fn fight(secs: f64, rows: Vec<PlayerRow>, hit_t: u32) -> Analysis {
        let total = rows.iter().map(|r| r.damage).sum();
        Analysis {
            view: EncounterView {
                active: false,
                duration_s: secs,
                total_damage: total,
                total_dps: 0.0,
                target: Some("Mob".into()),
                target_id: Some(9),
                target_type: None,
                target_boss: false,
                target_count: 1,
                start_us: 0,
                end_us: 0,
                players: rows,
                map: None,
                instance_us: None,
                healing: Vec::new(),
                taken: Vec::new(),
                total_healing: 0,
                total_taken: 0,
                end: None,
                deaths: Vec::new(),
                berserk_us: None,
                target_hp: None,
                target_max_hp: None,
                boss_casts: Vec::new(),
                boss_now_s: None,
                boss_cast: None,
            },
            hits: vec![TimelineHit { t: hit_t, player: 1, target: 9, skill: 1, cast: 0, damage: 1, flags: 0, n: 1 }],
            effects: Vec::new(),
            names: BTreeMap::new(),
            mob_types: BTreeMap::new(),
            target_hp: Vec::new(),
            heals: Vec::new(),
            taken: Vec::new(),
        }
    }

    #[test]
    fn fights_go_back_to_back_and_add_up() {
        let a = fight(10.0, vec![row(1, "#1", 100, 5), row(2, "Bob", 300, 7)], 4000);
        let b = fight(20.0, vec![row(1, "Me", 500, 5)], 1000);
        let (j, starts) = join(vec![a, b]).unwrap();
        assert_eq!(starts, vec![0, 10_000]);
        // The second fight's hit moves along by the first fight's length.
        assert_eq!(j.hits.iter().map(|h| h.t).collect::<Vec<_>>(), vec![4000, 11_000]);
        assert_eq!((j.view.duration_s, j.view.total_damage), (30.0, 900));
        let me = &j.view.players[0];
        assert_eq!((me.id, me.name.as_str(), me.damage, me.hits, me.outcomes.crits), (1, "Me", 600, 4, 2));
        assert_eq!(me.dps, 20.0);
        assert!((me.share - 600.0 / 900.0).abs() < 1e-9);
        assert_eq!((me.skills.len(), me.skills[0].damage, me.skills[0].min), (1, 600, 25));
        assert_eq!(j.view.players[1].name, "Bob");
        assert!(join(Vec::new()).is_none());
    }
}
