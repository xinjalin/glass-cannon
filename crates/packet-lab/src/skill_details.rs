//! `packet-lab skill-details`: turns a scrape of aion2.app's skill database (one JSON object
//! per skill, as its pages carry it) into `gamedata/skill-details.json`, the compact table
//! the analysis window's skill tooltips use.
//!
//! Kept per skill: what it is (class, type, weapon, range, properties), its cost and
//! cooldown at level 1 and at its top level, its description at both levels, and each
//! specialty with its description. Descriptions are templates with `{token}` values that
//! change with the skill's level; they're filled in from that level's values.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use anyhow::{bail, Context, Result};
use serde_json::{json, Map, Value};

pub fn run(input: &Path, out: &Path, scraped: &str) -> Result<()> {
    let bytes = fs::read(input).with_context(|| format!("can't read {}", input.display()))?;
    let all: Map<String, Value> = match serde_json::from_slice(&bytes)? {
        Value::Object(m) => m,
        _ => bail!("{} isn't a JSON object of skills", input.display()),
    };
    let mut skills: BTreeMap<u32, Value> = BTreeMap::new();
    for (id, s) in &all {
        let Ok(num) = id.parse::<u32>() else { continue };
        if let Some(entry) = skill(s) {
            skills.insert(num, entry);
        }
    }
    let count = skills.len();
    let doc = json!({
        "source": {
            "site": "https://aion2.app/db/skills",
            "scraped": scraped,
            "note": "Aion 2 game data as compiled by aion2.app, which sources it from NCSOFT. Not covered by Glass Cannon's MIT licence; game text and numbers belong to NCSOFT. Made with packet-lab skill-details.",
        },
        "skills": skills,
    });
    if let Some(dir) = out.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::write(out, serde_json::to_string_pretty(&doc)? + "\n").with_context(|| format!("writing {}", out.display()))?;
    println!("done: {count} skills saved to {}", out.display());
    Ok(())
}

/// One skill's tooltip data, or `None` for entries with nothing to show.
fn skill(s: &Value) -> Option<Value> {
    let name = s["name"].as_str().filter(|n| !n.is_empty())?;
    let levels = s["levels"].as_array().filter(|l| !l.is_empty())?;
    let first = &levels[0];
    let last = &levels[levels.len() - 1];
    let mut e = Map::new();
    e.insert("name".into(), json!(name));
    let text = |v: &Value| v.as_str().filter(|t| !t.is_empty()).map(str::to_string);
    let details = &s["details"];
    for (key, value) in [
        ("class", text(&s["classClean"])),
        ("type", text(&s["typeClean"])),
        ("subtype", text(&s["subtype"])),
        ("weapon", text(&details["Weapon"])),
        ("damage_type", text(&details["Damage type"])),
        ("range", text(&s["range"])),
        ("properties", text(&s["properties"])),
    ] {
        if let Some(v) = value {
            e.insert(key.into(), json!(v));
        }
    }
    let number = |v: &Value| v.as_str().and_then(|t| t.parse::<u32>().ok()).or_else(|| v.as_u64().map(|n| n as u32));
    if let Some(n) = number(&s["maxLevel"]) {
        e.insert("max_level".into(), json!(n));
    }
    if let Some(n) = number(&s["requiredLevel"]) {
        e.insert("required_level".into(), json!(n));
    }
    // Numbers that may change with the skill's level: one value, or [level 1, top level].
    for (key, field) in [("cooldown_s", "cooldown"), ("mp", "cost_mp"), ("cast_s", "casting_time")] {
        let (a, b) = (first[field].as_f64().unwrap_or(0.0), last[field].as_f64().unwrap_or(0.0));
        if a > 0.0 || b > 0.0 {
            e.insert(key.into(), if a == b { json!(a) } else { json!([a, b]) });
        }
    }
    if let Some(template) = text(&s["description"]) {
        let (d1, dn) = (fill(&template, first), fill(&template, last));
        e.insert("desc".into(), json!(d1));
        if dn != d1 {
            e.insert("desc_max".into(), json!(dn));
        }
    }
    let specs: Vec<Value> = s["specs"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|sp| {
            let id = sp["id"].as_u64()?;
            let template = sp["description"].as_str().unwrap_or("");
            let mut v = json!({ "id": id, "desc": fill(template, first) });
            if let Some(unlock) = sp["unlock_level"].as_u64() {
                v["unlock"] = json!(unlock);
            }
            let dn = fill(template, last);
            if dn != v["desc"] {
                v["desc_max"] = json!(dn);
            }
            Some(v)
        })
        .collect();
    if !specs.is_empty() {
        e.insert("specs".into(), json!(specs));
    }
    Some(Value::Object(e))
}

/// Fills a description's `{token}`s with one level's values; unknown tokens become "?".
/// A token ending in `:time` is a duration in seconds.
fn fill(template: &str, level: &Value) -> String {
    let values = &level["token_values"];
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let Some(close) = rest[open..].find('}') else {
            out.push_str(&rest[open..]);
            return out;
        };
        let token = &rest[open + 1..open + close];
        match &values[token] {
            Value::String(v) => out.push_str(v),
            Value::Number(v) => out.push_str(&v.to_string()),
            _ => out.push('?'),
        }
        // Durations are in seconds ("every 2s for 8s").
        if token.ends_with(":time") {
            out.push('s');
        }
        rest = &rest[open + close + 1..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descriptions_fill_in_each_levels_values() {
        let level = |dmg: &str| json!({ "cooldown": 0, "cost_mp": 0, "casting_time": 0, "token_values": {
            "se_dmg:1801000011:SkillUIMinDmgSum": dmg, "se:1801002011:effect_value14:divide100": "0.7" } });
        let s = json!({
            "name": "Onslaught", "classClean": "Chanter", "typeClean": "Active", "subtype": "Mastery",
            "maxLevel": "40", "requiredLevel": "1", "range": "4m", "details": { "Weapon": "Staff" },
            "description": "Deals {se_dmg:1801000011:SkillUIMinDmgSum} damage.",
            "specs": [{ "id": 18010020, "unlock_level": 8, "description": "Absorbs {se:1801002011:effect_value14:divide100}% HP" }],
            "levels": [level("74"), level("2960")],
        });
        let e = skill(&s).unwrap();
        assert_eq!(e["desc"], "Deals 74 damage.");
        assert_eq!(e["desc_max"], "Deals 2960 damage.");
        assert_eq!(e["specs"][0]["desc"], "Absorbs 0.7% HP");
        assert!(e["specs"][0].get("desc_max").is_none());
        assert_eq!(e["specs"][0]["unlock"], 8);
        assert_eq!((e["class"].as_str(), e["max_level"].as_u64()), (Some("Chanter"), Some(40)));
        assert!(e.get("cooldown_s").is_none(), "no cooldown, no field");
    }

    #[test]
    fn cooldowns_keep_both_ends_when_they_change() {
        let s = json!({ "name": "Defiance", "levels": [
            { "cooldown": 60, "cost_mp": 0, "casting_time": 0 },
            { "cooldown": 21, "cost_mp": 0, "casting_time": 0 } ] });
        assert_eq!(skill(&s).unwrap()["cooldown_s"], json!([60.0, 21.0]));
        assert_eq!(fill("{missing} left", &json!({})), "? left");
        let tick = json!({ "token_values": { "abe:1812000011:value01:time": "2" } });
        assert_eq!(fill("every {abe:1812000011:value01:time} for", &tick), "every 2s for");
    }
}
