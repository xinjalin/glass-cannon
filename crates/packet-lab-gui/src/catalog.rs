//! What we know about each opcode, read from `docs/opcodes.md` (built into the program, so
//! the notes stay the one place it's written down): its status (which table it's in), what
//! it is, its name in Questlog's list and in other projects, and its family (by high byte).

use std::collections::BTreeMap;

use serde::Serialize;

const OPCODES_MD: &str = include_str!("../../../docs/opcodes.md");

/// How well an opcode is understood: the table it's in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    /// Decoded and used by the meter.
    Decoded,
    /// Understood, not used yet.
    Understood,
    /// Named by Questlog or another project, not checked.
    Named,
    /// In our captures, no name anywhere.
    Unknown,
    /// Named elsewhere, never seen in our captures.
    Unseen,
}

#[derive(Debug, Clone, Serialize)]
pub struct Entry {
    pub opcode: u16,
    pub status: Status,
    /// Our description (decoded and understood ones).
    pub what: String,
    /// Questlog's name for it.
    pub name: String,
    /// What other projects call it.
    pub other: String,
    pub family: &'static str,
}

/// Families by the opcode's high byte, as the known ones group (spawns in 0x36, movement in
/// 0x37, combat in 0x38…).
pub fn family(opcode: u16) -> &'static str {
    match opcode >> 8 {
        0x36 => "entities",
        0x37 => "movement",
        0x38 => "combat",
        0x39 | 0x51 => "skills",
        0x40 => "map",
        0x56 | 0x57 => "items",
        0x61 => "dungeons",
        0x8a => "guild",
        0x8d => "character",
        0x90 => "pets",
        0x91 => "world map",
        0x92 | 0x93 => "party",
        0x96 => "force",
        0x97 => "party finder",
        0xe0 => "stagger",
        0xe2 => "daevanion & npcs",
        0xe3 => "abyss",
        _ => "other",
    }
}

/// Every opcode the notes list, by opcode.
pub fn load() -> BTreeMap<u16, Entry> {
    parse(OPCODES_MD)
}

fn parse(md: &str) -> BTreeMap<u16, Entry> {
    let mut out = BTreeMap::new();
    let mut status = None;
    let mut columns: Vec<String> = Vec::new();
    for line in md.lines() {
        if let Some(title) = line.strip_prefix("## ") {
            status = match title {
                t if t.starts_with("Decoded") => Some(Status::Decoded),
                t if t.starts_with("Understood") => Some(Status::Understood),
                t if t.starts_with("Named elsewhere only") => Some(Status::Named),
                t if t.starts_with("Unknown") => Some(Status::Unknown),
                t if t.starts_with("Named elsewhere, never") => Some(Status::Unseen),
                _ => None,
            };
            columns.clear();
            continue;
        }
        let Some(status) = status else { continue };
        let cells: Vec<&str> = line.trim().trim_matches('|').split('|').map(str::trim).collect();
        if cells.first() == Some(&"opcode") {
            columns = cells.iter().map(|c| c.to_string()).collect();
            continue;
        }
        let Some(hex) = cells.first().and_then(|c| c.strip_prefix("`0x")).and_then(|c| c.strip_suffix('`')) else {
            continue;
        };
        let Ok(opcode) = u16::from_str_radix(hex, 16) else { continue };
        let cell = |name: &str| {
            columns
                .iter()
                .position(|c| c == name)
                .and_then(|i| cells.get(i))
                .map(|c| c.trim_matches('`').to_string())
                .unwrap_or_default()
        };
        out.insert(
            opcode,
            Entry {
                opcode,
                status,
                what: cell("what"),
                name: cell("Questlog's name"),
                other: cell("other projects"),
                family: family(opcode),
            },
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_notes_are_read() {
        let c = load();
        let hit = &c[&0x3804];
        assert_eq!((hit.status, hit.name.as_str(), hit.family), (Status::Decoded, "SkillEffect_NT", "combat"));
        assert!(hit.what.contains("damage"));
        assert_eq!(c[&0x372a].status, Status::Unknown);
        assert_eq!(c[&0x3600].status, Status::Named);
        assert_eq!(c[&0x3600].name, "MapFrame_NT");
        assert!(c.values().filter(|e| e.status == Status::Decoded).count() >= 40);
    }
}
