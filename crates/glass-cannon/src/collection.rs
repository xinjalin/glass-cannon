//! Your collection: every skin, wing and Pantheon decoration in the data, which your
//! characters have, and which the black market's offers would add.
//!
//! The game lists skins by skin ID (see `meter_core::collection`): its kind, the art's set
//! number and its slot. Items don't carry it, so it's worked out: the set number is in the
//! item's icon name (`Icon_GM_0063_T03_Helmet`: 63), the slot in a skin item's name ("(Skin:
//! Helm)") or a piece of gear's item ID (2103…: armour, slot 03). Icons named otherwise
//! (`Icon_GM_DebGuard_01_Torso`) can't be tied, and stay unknown. Every weapon, armour and
//! accessory skin one of our characters had (201 of them) came out named this way.

use std::collections::BTreeMap;
use std::sync::Arc;

use serde::Serialize;
use tauri::State;

use crate::gamedata::{GameData, ShopItem};
use crate::store::CollectionBook;
use crate::{lock, AppState};

/// Weapon slots, numbered as in their gear's item IDs (1101…: greatsword) and skin IDs.
pub const WEAPONS: [&str; 8] = ["Greatsword", "Longsword", "Dagger", "Bow", "Spellbook", "Orb", "Mace", "Staff"];
/// Armour slots, the same way (2101…: breastplate).
pub const ARMOUR: [&str; 7] = ["Breastplate", "Greaves", "Helm", "Pauldrons", "Gloves", "Boots", "Cloak"];
/// Accessory slots with skins, and their numbers in skin IDs (from our characters' skins;
/// the gear's IDs number them otherwise: 3101… necklace, 3102… earrings).
const ACCESSORIES: [(&str, u32); 2] = [("Earrings", 101), ("Necklace", 103)];

const WEAPON: u32 = 10_000_000;
const ARMOUR_KIND: u32 = 30_000_000;
const OUTFIT: u32 = 40_000_000;
const ACCESSORY: u32 = 50_000_000;
/// Decorations: eyewear (`Icon_DECO_EW_0007`, Pixel Glasses: 70007101, one of our
/// characters' skins).
const DECORATION: u32 = 70_000_000;
const EYEWEAR: u32 = 101;

/// The art's set number in an icon's name: four digits, before its tier ("T03") and slot
/// ("Torso"), if any.
pub fn set_of(icon: &str) -> Option<u32> {
    let mut parts = icon.rsplit('_').peekable();
    for _ in 0..2 {
        let p = parts.peek()?;
        let tier = p.len() > 1 && p.starts_with('T') && p[1..].bytes().all(|b| b.is_ascii_digit());
        let slot = p.bytes().all(|b| b.is_ascii_alphabetic());
        if !(tier || slot) {
            break;
        }
        parts.next();
    }
    let p = parts.next()?;
    (p.len() == 4 && p.bytes().all(|b| b.is_ascii_digit())).then(|| p.parse().ok()).flatten()
}

/// A skin's ID from its set and slot name ("Helm", "Greatsword", "Earrings").
pub fn skin_of_slot(set: u32, slot: &str) -> Option<u32> {
    let at = |list: &[&str], s: &str| list.iter().position(|x| *x == s).map(|i| i as u32 + 101);
    let (kind, n) = if let Some(n) = at(&WEAPONS, slot) {
        (WEAPON, n)
    } else if let Some(n) = at(&ARMOUR, slot) {
        (ARMOUR_KIND, n)
    } else if slot == "Eyewear" {
        (DECORATION, EYEWEAR)
    } else {
        (ACCESSORY, ACCESSORIES.iter().find(|(s, _)| *s == slot)?.1)
    };
    Some(kind + set * 1000 + n)
}

/// The skin a piece of gear gives, from its item ID and icon.
pub fn skin_of_gear(item: u32, icon: &str) -> Option<u32> {
    let set = set_of(icon)?;
    let prefix = item / 100_000;
    let (kind, n) = (prefix / 100, prefix % 100);
    let slot = match kind {
        11 | 15 if (1..=8).contains(&n) => WEAPONS[n as usize - 1],
        21 | 25 if (1..=7).contains(&n) => ARMOUR[n as usize - 1],
        31 if n == 1 => "Necklace",
        31 if n == 2 => "Earrings",
        _ => return None,
    };
    skin_of_slot(set, slot)
}

/// What an item unlocks.
#[derive(Debug, Clone, PartialEq)]
pub enum Look {
    Skin(u32),
    /// A whole outfit ("Set"), by its set number: its pieces' skins are 4 0SSS ….
    Outfit(u32),
    /// Wings: any of these (one per faction, or grade).
    Wings(Vec<u32>),
}

pub fn look_of(data: &GameData, item: &ShopItem) -> Option<Look> {
    match item.kind.as_str() {
        "skin" => {
            let set = set_of(item.icon.as_deref()?)?;
            let slot = item.slot.as_deref()?;
            if slot == "Set" {
                return Some(Look::Outfit(set));
            }
            skin_of_slot(set, slot).map(Look::Skin)
        }
        "wings" => Some(Look::Wings(data.wings_named(&item.name))).filter(|l| !matches!(l, Look::Wings(w) if w.is_empty())),
        _ => None,
    }
}

/// Whether one of your characters has it: unknown until the game has sent a list.
pub fn owned(book: &CollectionBook, look: &Look) -> Option<bool> {
    match look {
        Look::Skin(id) => book.skins_known().then(|| book.skin(*id).is_some()),
        Look::Outfit(set) => {
            let from = OUTFIT + set * 1000;
            book.skins_known().then(|| book.skins().range(from..from + 1000).next().is_some())
        }
        Look::Wings(ids) => book.wings_known().then(|| ids.iter().any(|w| book.wing(*w).is_some())),
    }
}

/// One thing to collect.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Entry {
    /// weapon, armour, accessory, outfit or wings. Skins you have that no item in the data
    /// looks like (a handful: 2…, 8…, some 7…) aren't listed.
    pub kind: &'static str,
    pub slot: Option<String>,
    /// Its skin ID (an outfit's: its set number), or wing ID.
    pub id: u32,
    pub name: Option<String>,
    /// Gear that looks like it (a skin): other names it goes by.
    pub also: Vec<String>,
    pub grade: Option<String>,
    pub icon: Option<String>,
    pub faction: Option<String>,
    pub owned: bool,
    /// The game's count of a skin, a wing's enchant level, or the Pantheon slot a
    /// decoration is placed in (0: not placed).
    pub count: u16,
    /// What else there is to say: a decoration's stats ("Destiny +3 · Life +3").
    pub detail: Option<String>,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct CollectionView {
    pub skins_known: bool,
    pub wings_known: bool,
    pub decorations_known: bool,
    /// The characters whose collection is known.
    pub characters: Vec<String>,
    pub entries: Vec<Entry>,
}

/// A skin's entry while the list is built: names from skin items first, then gear.
#[derive(Default)]
struct Draft {
    kind: &'static str,
    slot: String,
    names: Vec<String>,
    grade: Option<String>,
    icon: Option<String>,
}

impl Draft {
    fn add(&mut self, name: &str, grade: Option<&String>, icon: Option<&String>) {
        if !self.names.iter().any(|n| n == name) {
            self.names.push(name.to_string());
        }
        if self.grade.is_none() {
            self.grade = grade.cloned();
        }
        if self.icon.is_none() {
            self.icon = icon.cloned();
        }
    }
}

fn kind_of(skin: u32) -> &'static str {
    match skin / 10_000_000 * 10_000_000 {
        WEAPON => "weapon",
        ARMOUR_KIND => "armour",
        ACCESSORY | DECORATION => "accessory",
        _ => "other",
    }
}

pub fn view(data: &GameData, book: &CollectionBook) -> CollectionView {
    let mut skins: BTreeMap<u32, Draft> = BTreeMap::new();
    let mut outfits: BTreeMap<u32, Draft> = BTreeMap::new();
    let draft = |slot: &str, id: u32| Draft { kind: kind_of(id), slot: slot.to_string(), ..Draft::default() };
    // Skin items first: their names are the looks' own.
    for (_, item) in data.shop_items().filter(|(_, i)| i.kind == "skin") {
        let (Some(set), Some(slot)) = (item.icon.as_deref().and_then(set_of), item.slot.as_deref()) else { continue };
        if slot == "Set" {
            outfits.entry(set).or_insert_with(|| Draft { kind: "outfit", ..Draft::default() }).add(&item.name, item.grade.as_ref(), item.icon.as_ref());
        } else if let Some(id) = skin_of_slot(set, slot) {
            skins.entry(id).or_insert_with(|| draft(slot, id)).add(&item.name, item.grade.as_ref(), item.icon.as_ref());
        }
    }
    for (id, g) in data.all_gear() {
        let Some(skin) = g.icon.as_deref().and_then(|icon| skin_of_gear(id, icon)) else { continue };
        let slot = slot_name(skin);
        skins.entry(skin).or_insert_with(|| draft(&slot, skin)).add(&g.name, g.grade.as_ref(), g.icon.as_ref());
    }
    let owned_skins = book.skins();
    let mut entries: Vec<Entry> = skins
        .into_iter()
        .map(|(id, d)| {
            let count = owned_skins.get(&id).copied();
            let mut names = d.names.into_iter();
            Entry {
                kind: d.kind,
                slot: Some(d.slot),
                id,
                name: names.next(),
                also: names.collect(),
                grade: d.grade,
                icon: d.icon,
                faction: None,
                owned: count.is_some(),
                count: count.unwrap_or(0),
                detail: None,
            }
        })
        .collect();
    for (set, d) in outfits {
        let from = OUTFIT + set * 1000;
        let have = owned_skins.range(from..from + 1000).next().is_some();
        let mut names = d.names.into_iter();
        entries.push(Entry {
            kind: "outfit",
            slot: None,
            id: set,
            name: names.next(),
            also: names.collect(),
            grade: d.grade,
            icon: d.icon,
            faction: None,
            owned: have,
            count: 0,
            detail: None,
        });
    }
    for (id, w) in data.wings() {
        let level = book.wing(id);
        entries.push(Entry {
            kind: "wings",
            slot: None,
            id,
            name: Some(w.name.clone()),
            also: Vec::new(),
            grade: w.grade.clone(),
            icon: w.icon.clone(),
            faction: w.faction.clone(),
            owned: level.is_some(),
            count: level.unwrap_or(0).into(),
            detail: None,
        });
    }
    for (id, d) in data.decorations() {
        let slot = book.decoration(id);
        let (kind, prefix) = match d.kind.as_str() {
            "statue" => ("Statue", "Statue: "),
            "colossus" => ("Colossus", "Colossus: "),
            _ => ("Artwork", "Artwork: "),
        };
        let effects: Vec<String> = d.effects.iter().map(|(stat, n)| format!("{stat} {n:+}")).collect();
        entries.push(Entry {
            kind: "pantheon",
            slot: Some(kind.to_string()),
            id,
            name: Some(d.name.strip_prefix(prefix).unwrap_or(&d.name).to_string()),
            also: Vec::new(),
            grade: None,
            icon: d.icon.clone(),
            faction: d.side.clone(),
            owned: slot.is_some(),
            count: slot.unwrap_or(0).into(),
            detail: (!effects.is_empty()).then(|| effects.join(" · ")),
        });
    }
    CollectionView {
        skins_known: book.skins_known(),
        wings_known: book.wings_known(),
        decorations_known: book.decorations_known(),
        characters: book.characters(),
        entries,
    }
}

/// A skin's slot name from its ID.
fn slot_name(skin: u32) -> String {
    let n = (skin % 1000) as usize;
    let list: &[&str] = match skin / 10_000_000 * 10_000_000 {
        WEAPON => &WEAPONS,
        ARMOUR_KIND => &ARMOUR,
        ACCESSORY => return ACCESSORIES.iter().find(|(_, k)| *k as usize == n).map(|(s, _)| s.to_string()).unwrap_or_default(),
        DECORATION if n as u32 == EYEWEAR => return "Eyewear".into(),
        _ => return String::new(),
    };
    n.checked_sub(101).and_then(|i| list.get(i)).map(|s| s.to_string()).unwrap_or_default()
}

#[tauri::command]
pub fn collection(app: State<'_, Arc<AppState>>) -> CollectionView {
    view(&lock(&app.data), &lock(&app.collection))
}

/// An icon for the collection window, by its name.
#[tauri::command]
pub fn collection_icon(app: State<'_, Arc<AppState>>, icon: String) -> Option<String> {
    lock(&app.data).icon_named(&icon)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pantheon_decorations_owned_and_placed() {
        let dir = std::env::temp_dir().join(format!("gc-pantheon-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("skills.json"), r#"{ "skills": { "1": { "name": "Glide" } } }"#).unwrap();
        std::fs::write(
            dir.join("pantheon.json"),
            r#"{ "decorations": { "58": { "kind": "frame", "name": "Artwork: Artisan Blacksmith", "side": "Light", "effects": [["Destiny", 3], ["Life", 3]] },
                                  "1023": { "kind": "statue", "name": "Statue: Asahr's Mirror" },
                                  "5014": { "kind": "colossus", "name": "Colossus: Warden Santras" } } }"#,
        )
        .unwrap();
        let data = GameData::load_for_tests(dir.clone());
        let mut book = CollectionBook::load(&dir);
        let mut c = meter_core::collection::Collection::default();
        // The statue in slot 13, the artwork not placed.
        c.feed(meter_core::collection::OPCODE_DECORATION_LIST, &[2, 0xff, 3, 0, 0, 13, 58, 0, 0, 0, 0]);
        book.sync("Alpha", &mut c);
        let v = view(&data, &book);
        assert!(v.decorations_known);
        let decos: Vec<_> = v.entries.iter().filter(|e| e.kind == "pantheon").map(|e| (e.id, e.slot.as_deref(), e.name.as_deref(), e.owned, e.count, e.detail.as_deref())).collect();
        assert_eq!(
            decos,
            [
                (58, Some("Artwork"), Some("Artisan Blacksmith"), true, 0, Some("Destiny +3 · Life +3")),
                (1023, Some("Statue"), Some("Asahr's Mirror"), true, 13, None),
                (5014, Some("Colossus"), Some("Warden Santras"), false, 0, None),
            ]
        );
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn set_numbers_come_from_icon_names() {
        assert_eq!(set_of("Icon_GM_0067_T05_Torso"), Some(67));
        assert_eq!(set_of("Icon_Equip_AR_L_0001_T04_Cape"), Some(1));
        assert_eq!(set_of("Icon_WP_SW_0001_T03"), Some(1));
        assert_eq!(set_of("Icon_Equip_WP_L_GS_0108_T06"), Some(108));
        assert_eq!(set_of("Icon_ACC_ER_0005"), Some(5));
        // Named, not numbered: unknown.
        assert_eq!(set_of("Icon_GM_DebGuard_01_Torso"), None);
        assert_eq!(set_of("Icon_AR_Part_GM_ZikelS1_01"), None);
        assert_eq!(set_of("Icon_Item_Currency_Box_Weapon"), None);
    }

    #[test]
    fn skins_as_the_game_numbered_them_when_looted() {
        // The Strange Karma Bow and Helm, looted 2026-10-26: skins 10014104 and 30063103.
        assert_eq!(skin_of_gear(110_450_035, "Icon_WP_BW_0014_T03"), Some(10_014_104));
        assert_eq!(skin_of_gear(210_360_027, "Icon_GM_0063_T03_Helmet"), Some(30_063_103));
        // A skin item for the same looks.
        assert_eq!(skin_of_slot(14, "Bow"), Some(10_014_104));
        assert_eq!(skin_of_slot(63, "Helm"), Some(30_063_103));
        // Verdant Leaf: its necklace (103), its earrings (101).
        assert_eq!(skin_of_slot(5, "Necklace"), Some(50_005_103));
        assert_eq!(skin_of_slot(5, "Earrings"), Some(50_005_101));
        assert_eq!(skin_of_gear(310_100_001, "Icon_ACC_NL_0005"), Some(50_005_103));
        // A ring has no skin; nor a shield.
        assert_eq!(skin_of_gear(310_300_001, "Icon_ACC_Ring_0005"), None);
        assert_eq!(skin_of_slot(7, "Eyewear"), Some(70_007_101));
        assert_eq!(skin_of_slot(5, "Shoulder"), None);
        assert_eq!((slot_name(10_014_104), slot_name(30_063_103), slot_name(50_005_101)), ("Bow".into(), "Helm".into(), "Earrings".into()));
        assert_eq!(slot_name(70_007_101), "Eyewear");
    }
}
