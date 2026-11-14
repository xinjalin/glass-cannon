//! The black market window: what the Black Cloud Merchant offers each character this hour,
//! what each offer is (a pet you don't have yet, a skin, a Theostone…), and how often you've
//! seen it on offer (`black-market.json`).
//!
//! The game sends a page of offers when you open it in the shop (H): Special, Pets or
//! Skins (see meter-core's `contents`). The offers change every hour, and are different for
//! each character, so each character keeps the pages it was last shown, until they change.
//!
//! How rare an item is comes only from what you've seen: each page of each character, each
//! hour, counts once (opening it again in the same hour doesn't), and an item counts once
//! per page it was on. The app never goes online, so there's nobody else's to add.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use meter_core::contents::{BlackMarketPage, BLACK_MARKET_PETS, BLACK_MARKET_SKINS, BLACK_MARKET_SPECIAL};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::gamedata::GameData;
use crate::store::{read_json, write_atomic, PetBook};
use crate::{lock, AppState};

/// The pages in the order the shop shows them.
pub const PAGES: [(u8, &str); 3] = [(BLACK_MARKET_SPECIAL, "special"), (BLACK_MARKET_PETS, "pets"), (BLACK_MARKET_SKINS, "skins")];
/// Hours of pages counted, kept to tell a page already counted from a new one.
const LOOKS_KEPT: usize = 2000;

/// An item's sightings.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Seen {
    /// Hourly pages it was on.
    pub times: u32,
    pub first_ms: u64,
    pub last_ms: u64,
}

/// A page as a character was last shown it, and when.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Shown {
    pub page: BlackMarketPage,
    pub seen_ms: u64,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct File {
    /// Character name -> page -> as last shown.
    #[serde(default)]
    characters: BTreeMap<String, BTreeMap<u8, Shown>>,
    /// Item -> how often it was on offer.
    #[serde(default)]
    items: BTreeMap<u32, Seen>,
    /// Page -> hourly pages looked at.
    #[serde(default)]
    looks: BTreeMap<u8, u32>,
    /// The hourly pages counted already: character, page, and when it changed next.
    #[serde(default)]
    counted: BTreeSet<(String, u8, u64)>,
}

pub struct BlackMarket {
    path: PathBuf,
    file: File,
}

impl BlackMarket {
    pub fn load(dir: &Path) -> Self {
        let path = dir.join("black-market.json");
        let file = read_json(&path).unwrap_or_default();
        Self { path, file }
    }

    fn save(&self) {
        if let Ok(s) = serde_json::to_string(&self.file) {
            write_atomic(&self.path, s);
        }
    }

    /// Takes the pages the game sent `character` (the latest of each wins).
    pub fn update(&mut self, character: Option<&str>, pages: Vec<BlackMarketPage>, now_ms: u64) {
        let Some(character) = character else { return };
        if pages.is_empty() {
            return;
        }
        for page in pages {
            let key = (character.to_string(), page.page, page.next_refresh_ms);
            if self.file.counted.insert(key) {
                *self.file.looks.entry(page.page).or_default() += 1;
                let items: BTreeSet<u32> = page.offers.iter().map(|o| o.item).collect();
                for item in items {
                    let s = self.file.items.entry(item).or_default();
                    if s.times == 0 {
                        s.first_ms = now_ms;
                    }
                    s.times += 1;
                    s.last_ms = now_ms;
                }
            }
            self.file.characters.entry(character.to_string()).or_default().insert(page.page, Shown { page, seen_ms: now_ms });
        }
        // The oldest hours go first (sorted by character, page, then hour: drop by hour).
        while self.file.counted.len() > LOOKS_KEPT {
            let oldest = self.file.counted.iter().min_by_key(|(_, _, at)| *at).cloned();
            match oldest {
                Some(k) => self.file.counted.remove(&k),
                None => break,
            };
        }
        self.save();
    }
}

/// An offer, as the window shows it.
#[derive(Debug, Serialize, PartialEq)]
pub struct OfferView {
    pub item: u32,
    /// Its name ("Congealed Drana", "Blue Dawn"), when the item data has it.
    pub name: Option<String>,
    /// "pet", "skin", "theostone", or none when the data doesn't know the item.
    pub kind: Option<String>,
    /// What a skin is for ("Greatsword"), a pet's genus.
    pub detail: Option<String>,
    /// A pet: its ID, and its level as your characters last had it (0: you don't have it).
    pub pet: Option<u32>,
    pub pet_level: Option<u32>,
    pub count: u64,
    pub price: u64,
    pub discounted_price: u64,
    /// 10, 20, 30 or 50.
    pub percent_off: u32,
    pub currency: u8,
    pub sold_out: bool,
    /// Hourly pages it was on, and the first time.
    pub seen: u32,
    pub first_ms: u64,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct PageView {
    pub page: u8,
    pub name: &'static str,
    pub offers: Vec<OfferView>,
    pub next_refresh_ms: u64,
    pub seen_ms: u64,
    /// The offers have changed since (the hour turned): these are last hour's.
    pub stale: bool,
    pub refreshes: u32,
    /// Hourly pages of this kind looked at, all characters.
    pub looks: u32,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct CharacterView {
    pub name: String,
    pub pages: Vec<PageView>,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct BlackMarketView {
    pub you: Option<String>,
    pub characters: Vec<CharacterView>,
    /// The next hour (when every character's offers change).
    pub next_refresh_ms: u64,
}

/// A discount step's percentage off.
fn percent_off(step: u8, price: u64, discounted: u64) -> u32 {
    match step {
        0 => 10,
        1 => 20,
        2 => 30,
        3 => 50,
        // A step we haven't seen: from the prices.
        _ if price > 0 => (100 - discounted * 100 / price) as u32,
        _ => 0,
    }
}

pub fn view(m: &BlackMarket, data: &GameData, pets: &PetBook, you: Option<String>, now_ms: u64) -> BlackMarketView {
    let mut characters: Vec<CharacterView> = m
        .file
        .characters
        .iter()
        .map(|(name, pages)| CharacterView {
            name: name.clone(),
            pages: PAGES
                .iter()
                .filter_map(|(id, page_name)| {
                    let shown = pages.get(id)?;
                    let offers = shown
                        .page
                        .offers
                        .iter()
                        .map(|o| {
                            let info = data.shop_item(o.item);
                            let pet = info.and_then(|i| i.pet);
                            let seen = m.file.items.get(&o.item).cloned().unwrap_or_default();
                            OfferView {
                                item: o.item,
                                name: info.map(|i| i.name.clone()),
                                kind: info.map(|i| i.kind.clone()),
                                detail: info.and_then(|i| i.slot.clone()).or_else(|| pet.and_then(|p| data.pet_genus(p))),
                                pet,
                                pet_level: pet.map(|p| pets.level(p)),
                                count: o.count,
                                price: o.price,
                                discounted_price: o.discounted_price,
                                percent_off: if o.discounted { percent_off(o.step, o.price, o.discounted_price) } else { 0 },
                                currency: o.currency,
                                sold_out: o.sold_out,
                                seen: seen.times,
                                first_ms: seen.first_ms,
                            }
                        })
                        .collect();
                    Some(PageView {
                        page: *id,
                        name: page_name,
                        offers,
                        next_refresh_ms: shown.page.next_refresh_ms,
                        seen_ms: shown.seen_ms,
                        stale: shown.page.next_refresh_ms <= now_ms,
                        refreshes: shown.page.refreshes,
                        looks: m.file.looks.get(id).copied().unwrap_or(0),
                    })
                })
                .collect(),
        })
        .collect();
    // You first.
    if let Some(me) = &you {
        characters.sort_by_key(|c| &c.name != me);
    }
    BlackMarketView { you, characters, next_refresh_ms: now_ms - now_ms % 3_600_000 + 3_600_000 }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

#[tauri::command]
pub fn black_market(app: State<'_, Arc<AppState>>) -> BlackMarketView {
    let you = lock(&app.profile).name.clone();
    view(&lock(&app.black_market), &lock(&app.data), &lock(&app.pets), you, now_ms())
}

/// Called with the pages the meter read since its last snapshot.
pub fn take(app: &AppState, pages: Vec<BlackMarketPage>) {
    if pages.is_empty() {
        return;
    }
    let me = lock(&app.profile).name.clone();
    lock(&app.black_market).update(me.as_deref(), pages, now_ms());
    // The merchant was opened in game: its window comes up (settings), soon after.
    app.black_market_opened.store(true, std::sync::atomic::Ordering::SeqCst);
}

#[cfg(test)]
mod tests {
    use super::*;
    use meter_core::contents::BlackMarketOffer;
    use std::fs;

    const HOUR: u64 = 3_600_000;
    const T: u64 = 1_794_571_200_000; // an hour, UTC

    fn offer(item: u32, step: u8) -> BlackMarketOffer {
        BlackMarketOffer { slot: item, item, count: 1, step, currency: 93, price: 100_000, discounted_price: 70_000, discounted: true, sold_out: false }
    }

    fn page(page: u8, next: u64, items: &[u32]) -> BlackMarketPage {
        BlackMarketPage { page, offers: items.iter().map(|i| offer(*i, 2)).collect(), next_refresh_ms: next, refreshes: 0 }
    }

    fn temp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("gc-bm-{tag}-{}-{}", std::process::id(), now_ms()));
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn an_hour_of_a_page_counts_once() {
        let dir = temp_dir("count");
        let mut m = BlackMarket::load(&dir);
        // Opened twice in one hour: one look. The next hour: another.
        m.update(Some("Nene"), vec![page(BLACK_MARKET_PETS, T + HOUR, &[1, 2])], T + 10);
        m.update(Some("Nene"), vec![page(BLACK_MARKET_PETS, T + HOUR, &[1, 2])], T + 20);
        m.update(Some("Nene"), vec![page(BLACK_MARKET_PETS, T + 2 * HOUR, &[2, 3])], T + HOUR + 10);
        // Another character's page in the same hour: its own look.
        m.update(Some("Alt"), vec![page(BLACK_MARKET_PETS, T + 2 * HOUR, &[2])], T + HOUR + 20);
        let m = BlackMarket::load(&dir);
        assert_eq!(m.file.looks[&BLACK_MARKET_PETS], 3);
        assert_eq!([1, 2, 3].map(|i| m.file.items[&i].times), [1, 3, 1]);
        assert_eq!((m.file.items[&2].first_ms, m.file.items[&2].last_ms), (T + 10, T + HOUR + 20));
        // Each keeps its last page.
        assert_eq!(m.file.characters["Nene"][&BLACK_MARKET_PETS].page.next_refresh_ms, T + 2 * HOUR);
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn without_a_character_nothing_is_kept() {
        let dir = temp_dir("none");
        let mut m = BlackMarket::load(&dir);
        m.update(None, vec![page(BLACK_MARKET_PETS, T, &[1])], T);
        assert!(m.file.characters.is_empty() && m.file.items.is_empty());
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn the_view_names_offers_and_says_when_they_changed() {
        let dir = temp_dir("view");
        fs::write(dir.join("skills.json"), r#"{ "skills": { "18010000": { "name": "Onslaught" } } }"#).unwrap();
        fs::write(dir.join("pets.json"), r#"{ "pets": { "1217": { "name": "Congealed Drana", "genus": "Fera" } } }"#).unwrap();
        fs::write(
            dir.join("shop-items.json"),
            r#"{ "items": { "534760187": { "kind": "pet", "name": "Congealed Drana", "pet": 1217 },
                            "535710057": { "kind": "skin", "name": "Blue Dawn", "slot": "Greatsword" } } }"#,
        )
        .unwrap();
        let data = GameData::load_for_tests(dir.clone());
        let pets = PetBook::load(&dir);
        let mut m = BlackMarket::load(&dir);
        m.update(Some("Nene"), vec![page(BLACK_MARKET_SPECIAL, T + HOUR, &[534_760_187, 535_710_057, 999])], T + 5);
        let v = view(&m, &data, &pets, Some("Nene".into()), T + 60_000);
        let offers = &v.characters[0].pages[0].offers;
        assert_eq!(v.characters[0].pages[0].name, "special");
        assert_eq!((offers[0].name.as_deref(), offers[0].kind.as_deref(), offers[0].detail.as_deref()), (Some("Congealed Drana"), Some("pet"), Some("Fera")));
        assert_eq!((offers[0].pet, offers[0].pet_level, offers[0].percent_off, offers[0].seen), (Some(1217), Some(0), 30, 1));
        assert_eq!((offers[1].name.as_deref(), offers[1].detail.as_deref(), offers[1].pet), (Some("Blue Dawn"), Some("Greatsword"), None));
        assert_eq!((offers[2].name.as_deref(), offers[2].kind.as_deref()), (None, None), "an item the data doesn't know");
        assert!(!v.characters[0].pages[0].stale);
        assert_eq!(v.next_refresh_ms, T + HOUR);
        // Past the hour: last hour's offers.
        assert!(view(&m, &data, &pets, None, T + HOUR + 1).characters[0].pages[0].stale);
        fs::remove_dir_all(dir).ok();
    }
}
