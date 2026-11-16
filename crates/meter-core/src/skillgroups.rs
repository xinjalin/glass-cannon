//! One row per skill: which skill IDs are the same skill.
//!
//! A class skill's ID is its base (e.g. Judgment 12240000) plus the chosen specialties in
//! its thousands, hundreds and tens digits and a part in its last digit. One skill can land
//! under several IDs in one fight: Judgment with specialties 3, 4 and 5 hits as 12243450,
//! and 50 ms after each cast an extra hit comes as 12240347, with a cast ID of its own. Shown
//! by ID, that's two "Judgment" rows. They fold into one by base and name: variants with
//! the same base and the same name (charge levels "- Lv. 1", "- Max" aside) are one skill,
//! and a variant the game data names differently ("Smashing Blow", a spirit's attack) keeps
//! its own row.
//!
//! A triggered extra hit (last digit 7 or 8, or a digit above 5 where the specialties go:
//! 12240347, 12060057, 14010064) is part of its skill's cast, not a cast of its own: it adds
//! damage and hits, not casts.

use std::collections::HashMap;

use crate::meter::SkillRow;

/// Class skills (codes 11 to 19); each class's first ten thousand IDs (xx00xxxx) are its
/// weapon, basic attacks, dodge and spirits, which aren't made of specialties.
fn is_class_skill(id: u32) -> bool {
    (11_000_000..20_000_000).contains(&id) && id % 1_000_000 >= 10_000
}

/// A skill ID without its specialties and part.
pub fn base(id: u32) -> u32 {
    id - id % 10_000
}

/// How many specialties an ID shows were chosen.
pub fn specialty_count(id: u32) -> usize {
    if !is_class_skill(id) {
        return 0;
    }
    let tail = id % 10_000;
    [tail / 1000, tail / 100 % 10, tail / 10 % 10].iter().filter(|d| (1..=5).contains(*d)).count()
}

/// Which skill each ID is (see the module docs).
#[derive(Debug, Clone, Default)]
pub struct SkillGroups {
    /// Known IDs (from the app's skill data) to their skill's ID; others go by their base.
    known: HashMap<u32, u32>,
}

impl SkillGroups {
    /// From the app's skill data: every ID it names, with its name.
    pub fn from_names<'a>(names: impl IntoIterator<Item = (u32, &'a str)>) -> Self {
        let mut named: Vec<(u32, String)> = Vec::new();
        let mut base_name: HashMap<u32, String> = HashMap::new();
        for (id, name) in names {
            if (10_000_000..20_000_000).contains(&id) {
                let name = family_name(name);
                if id % 10_000 == 0 {
                    base_name.insert(id, name.clone());
                }
                named.push((id, name));
            }
        }
        named.sort_unstable();
        // Each (base, name) family's ID: the base when it has that name, else its first ID.
        let mut family: HashMap<(u32, String), u32> = HashMap::new();
        for (id, name) in &named {
            let b = base(*id);
            let key = (b, name.clone());
            let first = if base_name.get(&b) == Some(name) { b } else { *id };
            family.entry(key).or_insert(first);
        }
        let known = named.into_iter().map(|(id, name)| (id, family[&(base(id), name)])).collect();
        Self { known }
    }

    /// The skill `id` is part of (its row's ID).
    pub fn group(&self, id: u32) -> u32 {
        if !(10_000_000..20_000_000).contains(&id) {
            return id;
        }
        // As the names go: this ID, then without its last two digits, then its base.
        let found = [id, id - id % 100, base(id)].into_iter().find_map(|k| self.known.get(&k).copied());
        match found {
            Some(g) => g,
            None if is_class_skill(id) => base(id),
            None => id,
        }
    }

    /// One player's rows (by ID, or already by skill) as one row per skill, under the
    /// skill's ID, highest damage first. A skill's extra hits add damage and hits, not
    /// casts, unless it has no casts of its own in them. Rows already by skill come out
    /// as they went in.
    pub fn merge_rows(&self, rows: Vec<SkillRow>) -> Vec<SkillRow> {
        // Each skill's row, and the casts of the extra hits it triggered.
        let mut merged: HashMap<u32, (SkillRow, u64)> = HashMap::new();
        for r in rows {
            let group = self.group(r.skill);
            let mut o = r.outcomes;
            let triggered_casts = if self.triggered(r.skill) { std::mem::take(&mut o.casts) } else { 0 };
            match merged.get_mut(&group) {
                None => {
                    merged.insert(group, (SkillRow { skill: group, outcomes: o, ..r }, triggered_casts));
                }
                Some((m, t)) => {
                    if r.hits > 0 {
                        m.min = if m.hits == 0 { r.min } else { m.min.min(r.min) };
                    }
                    m.hits += r.hits;
                    m.damage += r.damage;
                    m.max = m.max.max(r.max);
                    m.share += r.share;
                    m.outcomes.merge(&o);
                    m.name = m.name.take().or(r.name);
                    m.icon_key = m.icon_key.or(r.icon_key);
                    m.source_type = m.source_type.or(r.source_type);
                    *t += triggered_casts;
                }
            }
        }
        let mut out: Vec<SkillRow> = merged
            .into_values()
            .map(|(mut m, triggered_casts)| {
                // Only ever triggered here: those are its casts.
                if m.outcomes.casts == 0 {
                    m.outcomes.casts = triggered_casts;
                }
                m
            })
            .collect();
        out.sort_by(|a, b| b.damage.cmp(&a.damage).then(a.skill.cmp(&b.skill)));
        out
    }

    /// Whether `id` is an extra hit its skill triggered, not a cast of its own.
    pub fn triggered(&self, id: u32) -> bool {
        if !is_class_skill(id) || self.group(id) != base(id) {
            return false;
        }
        let tail = id % 10_000;
        matches!(tail % 10, 7 | 8) || [tail / 1000, tail / 100 % 10, tail / 10 % 10].iter().any(|d| *d > 5)
    }
}

/// A name without its charge level ("Punishment - Level 2", "Deadshot - Max").
fn family_name(name: &str) -> String {
    let name = name.trim();
    // ASCII lower-casing keeps every byte where it was, so cuts in it fit `name`.
    let lower = name.to_ascii_lowercase();
    let no_digits = lower.trim_end_matches(|c: char| c.is_ascii_digit());
    let marker = if no_digits.len() < lower.len() {
        let t = no_digits.trim_end_matches([' ', '.']);
        ["level", "lv"].into_iter().find(|w| t.ends_with(w)).map(|w| t.len() - w.len())
    } else {
        lower.ends_with("max").then(|| lower.len() - 3)
    };
    match marker {
        // Only a word of its own ("Bolt MAX", "Deadshot - Lv.1"), after a name.
        Some(at) if at > 0 && matches!(lower.as_bytes()[at - 1], b' ' | b'-') => {
            let head = name[..at].trim_end().trim_end_matches('-').trim_end();
            if head.is_empty() { name.to_string() } else { head.to_string() }
        }
        _ => name.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn groups() -> SkillGroups {
        SkillGroups::from_names([
            (12_240_000, "Judgment"),
            (12_240_340, "Judgment"),
            (12_243_450, "Judgment"),
            (12_090_000, "Punishment"),
            (12_090_001, "Punishment - Level 1"),
            (12_090_003, "Punishment - Max"),
            (11_010_000, "Rending Blow"),
            (11_010_040, "Rending Blow"),
            (11_010_047, "Smashing Blow"),
            (16_100_000, "Summon: Fire Spirit"),
            (16_100_001, "Fire Spirit: Leaping Slam"),
            (16_100_007, "Fire Spirit: Leaping Slam"),
            (18_000_101, "Basic Attack"),
            (18_000_102, "Basic Attack"),
            (18_000_100, "Dodge"),
            (14_010_000, "Deadshot"),
            (14_010_003, "Deadshot - Max"),
        ])
    }

    #[test]
    fn variants_of_a_skill_are_one() {
        let g = groups();
        // Specialties 3·4·5, its extra hit 12240347, and IDs the data doesn't list.
        for id in [12_243_450, 12_240_340, 12_240_347, 12_243_451, 12_240_000] {
            assert_eq!(g.group(id), 12_240_000, "{id}");
        }
        // Charge levels too.
        for id in [12_090_001, 12_090_003, 12_093_453] {
            assert_eq!(g.group(id), 12_090_000, "{id}");
        }
        assert_eq!(g.group(14_012_353), 14_010_000);
    }

    #[test]
    fn differently_named_parts_keep_their_own_row() {
        let g = groups();
        assert_eq!(g.group(11_010_047), 11_010_047);
        assert_eq!(g.group(11_010_040), 11_010_000);
        assert_eq!(g.group(16_100_007), 16_100_001);
        assert_eq!(g.group(16_100_000), 16_100_000);
        // Basic attack combo hits are one row; dodge its own.
        assert_eq!(g.group(18_000_102), 18_000_101);
        assert_eq!(g.group(18_000_100), 18_000_100);
    }

    #[test]
    fn without_data_variants_go_by_their_base() {
        let g = SkillGroups::default();
        assert_eq!(g.group(12_243_450), 12_240_000);
        assert_eq!(g.group(12_240_347), 12_240_000);
        // Not class skills: as they are.
        assert_eq!(g.group(18_000_102), 18_000_102);
        assert_eq!(g.group(3_001_015), 3_001_015);
        assert_eq!(g.group(1_802_510), 1_802_510);
    }

    #[test]
    fn extra_hits_are_not_casts() {
        let g = groups();
        assert!(g.triggered(12_240_347));
        assert!(g.triggered(14_010_064));
        assert!(!g.triggered(12_243_450));
        assert!(!g.triggered(12_090_003), "a charge level is a cast");
        assert!(!g.triggered(11_010_047), "a named part is its own skill");
        assert!(!g.triggered(16_100_007), "spirits' attacks aren't specialties");
    }

    #[test]
    fn specialties_and_names() {
        assert_eq!(specialty_count(12_243_450), 3);
        assert_eq!(specialty_count(12_240_347), 2);
        assert_eq!(specialty_count(12_240_000), 0);
        assert_eq!(family_name("Punishment - Level 2"), "Punishment");
        assert_eq!(family_name("Deadshot - Lv.1"), "Deadshot");
        assert_eq!(family_name("Ruinous Blow - MAX"), "Ruinous Blow");
        assert_eq!(family_name("Bolt Level 1"), "Bolt");
        assert_eq!(family_name("Bolt MAX"), "Bolt");
        assert_eq!(family_name("Rush Strike - Max"), "Rush Strike");
        assert_eq!(family_name("Maximum Effort"), "Maximum Effort");
        assert_eq!(family_name("Heal Level"), "Heal Level");
    }
}
