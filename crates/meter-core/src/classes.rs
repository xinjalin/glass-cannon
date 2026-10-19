//! Player classes, identified by the first two digits of their skill IDs.
//!
//! Worked out from the skills each prefix uses (e.g. 18 has "Barrier Spell" and
//! "Chanter_equip weapon"; 14 has "Arrow Storm" and "Deadshot").

/// The class for a class code (a skill ID / 1,000,000), if it's a player class.
pub fn class_name(code: u32) -> Option<&'static str> {
    Some(match code {
        11 => "Gladiator",
        12 => "Templar",
        13 => "Assassin",
        14 => "Ranger",
        15 => "Sorcerer",
        16 => "Elementalist",
        17 => "Cleric",
        18 => "Chanter",
        19 => "Fighter",
        _ => return None,
    })
}

/// The class code for a skill ID, if the skill belongs to a player class.
pub fn class_code_of_skill(skill: u32) -> Option<u32> {
    let code = skill / 1_000_000;
    class_name(code).map(|_| code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_and_skills() {
        assert_eq!(class_name(18), Some("Chanter"));
        assert_eq!(class_code_of_skill(18_010_340), Some(18));
        assert_eq!(class_code_of_skill(30_000_071), None); // Theostone, not a class
        assert_eq!(class_code_of_skill(1_216_760), None); // a mob skill
    }
}
