//! Content tickets (how many entries you have left for each content, and Odyle energy)
//! and who holds the Abyss artifacts.
//!
//! Ticket layouts follow Questlog's message list and read every byte of the lists in our
//! captures (73 tickets each). Odyle energy is ticket [`ODYLE_ENERGY`]: its basic and
//! charged energy, which fell by 80 each time a dungeon was completed (490 to 410, 355 to
//! 275). The other tickets' contents aren't named in any data we have.
//!
//! Shop purchase counters (`0x568e`): u16 0, u8 n, then n × (u32 counter, u32 bought,
//! u64 reset time in ms since 1970), the whole list after login and when a shop opens.
//! Counter [`ODYLE_PURCHASE`] is the weekly buy of Odyle Energy for Kina at the trade guild
//! (20 a week per character, as SkeeveAN's Aion DPS Meter has it): 20 bought in our
//! 2026-10-24 capture, 0 in the next week's, each resetting Wednesday 07:00 UTC (16:00 in
//! Korea). The other six counters there aren't named.
//!
//! The Abyss artifacts' layout is from A2Tools' notes (read against the in-game map); it
//! isn't in any of our captures yet, so a message is only read when every byte of it falls
//! into place.
//!
//! The Black Cloud Merchant (`0x5691`, Questlog's RandomShopInfo_NT): the game's black
//! market, a shop in the H menu whose offers are different for each character and change
//! every hour (on the hour UTC). Opening one of its three pages sends that page:
//!
//! ```text
//! u8 page (201 Special, 204 Pets, 203 Skins), u8 n, n × offer, i64 next refresh (ms since
//! 1970), i32 refreshes bought this hour
//! offer = u32 slot, u32 item, i64 count, u8 discount step (0-3: 10, 20, 30, 50 % off),
//!         u8 currency (93: Kinah), i64 price, i64 discounted price, flag discounted,
//!         flag sold out
//! ```
//!
//! The flags are bits: the first one takes a byte where it falls, and the next seven go in
//! its bits (as in Questlog's reader). Confirmed on our capture of 2026-11-14: all three
//! pages, 10 offers each, read to the last byte, every item named in the item data (pets,
//! skins), and the prices as the game showed them (100,000 Kinah at 30 % off: 70,000).

use serde::{Deserialize, Serialize};

use crate::protocol::read_varint;

/// Every ticket: a varint count, then the tickets.
pub const OPCODE_TICKETS: u16 = 0x610b;
/// One ticket changed: u8 flags, the ticket, u8 reason, then with flag 1 an i32.
pub const OPCODE_TICKET_UPDATE: u16 = 0x610c;
/// Odyle energy's ticket: its count is the basic energy, its charges the charged energy.
pub const ODYLE_ENERGY: u32 = 60_000_001;

/// Shop purchase counters: see the module's notes.
pub const OPCODE_SHOP_COUNTERS: u16 = 0x568e;
/// The weekly Odyle Energy buy at the trade guild, and how many a week.
pub const ODYLE_PURCHASE: u32 = 100_134;
pub const ODYLE_PURCHASE_MAX: u32 = 20;

/// The Black Cloud Merchant's page of offers: see the module's notes.
pub const OPCODE_BLACK_MARKET: u16 = 0x5691;

/// Its pages, as the game numbers them.
pub const BLACK_MARKET_SPECIAL: u8 = 201;
pub const BLACK_MARKET_SKINS: u8 = 203;
pub const BLACK_MARKET_PETS: u8 = 204;

/// One of the Black Cloud Merchant's offers.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct BlackMarketOffer {
    /// Which of the merchant's offers this is (each page numbers its own: 21xxxx, 22xxxx…).
    pub slot: u32,
    pub item: u32,
    pub count: u64,
    /// How far it's discounted: 0 to 3 (10, 20, 30 or 50 % off).
    pub step: u8,
    /// What it costs in (93: Kinah).
    pub currency: u8,
    pub price: u64,
    pub discounted_price: u64,
    pub discounted: bool,
    pub sold_out: bool,
}

/// A page of the Black Cloud Merchant, as it was sent.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BlackMarketPage {
    pub page: u8,
    pub offers: Vec<BlackMarketOffer>,
    /// When the offers change (ms since 1970).
    pub next_refresh_ms: u64,
    /// Refreshes bought with Kinah this hour.
    pub refreshes: u32,
}

/// Reads fields one after another, with flags packed eight to a byte.
struct Fields<'a> {
    p: &'a [u8],
    at: usize,
    /// The byte the flags are in now, and the next bit.
    flags: Option<(usize, u8)>,
}

impl Fields<'_> {
    fn bytes<const N: usize>(&mut self) -> Option<[u8; N]> {
        let b = self.p.get(self.at..self.at + N)?.try_into().ok()?;
        self.at += N;
        Some(b)
    }
    fn u8(&mut self) -> Option<u8> {
        Some(self.bytes::<1>()?[0])
    }
    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.bytes()?))
    }
    fn i64(&mut self) -> Option<i64> {
        Some(i64::from_le_bytes(self.bytes()?))
    }
    fn flag(&mut self) -> Option<bool> {
        let (byte, bit) = match self.flags {
            Some(f) => f,
            None => {
                let at = self.at;
                self.u8()?;
                (at, 0)
            }
        };
        self.flags = (bit < 7).then_some((byte, bit + 1));
        Some(self.p[byte] >> bit & 1 != 0)
    }
}

/// A page of the Black Cloud Merchant, when the message reads to its last byte.
pub fn parse_black_market(p: &[u8]) -> Option<BlackMarketPage> {
    let mut f = Fields { p, at: 0, flags: None };
    let page = f.u8()?;
    let n = f.u8()?;
    let offers = (0..n)
        .map(|_| {
            let (slot, item, count, step, currency) = (f.u32()?, f.u32()?, f.i64()?, f.u8()?, f.u8()?);
            let (price, discounted_price) = (f.i64()?, f.i64()?);
            let (discounted, sold_out) = (f.flag()?, f.flag()?);
            Some(BlackMarketOffer {
                slot,
                item,
                count: u64::try_from(count).ok()?,
                step,
                currency,
                price: u64::try_from(price).ok()?,
                discounted_price: u64::try_from(discounted_price).ok()?,
                discounted,
                sold_out,
            })
        })
        .collect::<Option<Vec<_>>>()?;
    let next_refresh_ms = u64::try_from(f.i64()?).ok()?;
    let refreshes = f.u32()?;
    (f.at == p.len()).then_some(BlackMarketPage { page, offers, next_refresh_ms, refreshes })
}

/// A shop purchase limit: how many were bought, and when the count resets.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct ShopCounter {
    pub id: u32,
    pub bought: u32,
    pub reset_ms: u64,
}

/// Every counter, when the message reads exactly to its end.
pub fn parse_shop_counters(p: &[u8]) -> Option<Vec<ShopCounter>> {
    if p.get(..2)? != [0, 0] {
        return None;
    }
    let n = usize::from(*p.get(2)?);
    if p.len() != 3 + 16 * n {
        return None;
    }
    Some(
        p[3..]
            .chunks_exact(16)
            .map(|c| ShopCounter {
                id: u32::from_le_bytes(c[..4].try_into().expect("4 bytes")),
                bought: u32::from_le_bytes(c[4..8].try_into().expect("4 bytes")),
                reset_ms: u64::from_le_bytes(c[8..].try_into().expect("8 bytes")),
            })
            .collect(),
    )
}

/// The Abyss map was opened: who holds each artifact, both layers.
pub const OPCODE_ABYSS_MAP: u16 = 0xe307;
/// You entered the Abyss: who holds each artifact of that layer.
pub const OPCODE_ABYSS_ENTERED: u16 = 0xe305;
pub const ELYOS: u8 = 1;
pub const ASMODIANS: u8 = 2;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct Ticket {
    pub id: u32,
    /// Entries (or energy) left, and charged ones, when the game says.
    pub count: Option<u32>,
    pub charged: Option<u32>,
}

/// One ticket: u8 flags, u32 id, then by flag 1 an i64 time, 2 an i64 time, 4 a varint
/// count, 8 a varint charged count.
fn ticket(p: &[u8], i: &mut usize) -> Option<Ticket> {
    let flags = *p.get(*i)?;
    let id = u32::from_le_bytes(p.get(*i + 1..*i + 5)?.try_into().ok()?);
    *i += 5;
    *i += 8 * usize::from(flags & 1 != 0) + 8 * usize::from(flags & 2 != 0);
    if *i > p.len() {
        return None;
    }
    let count = |bit: u8, i: &mut usize| -> Option<Option<u32>> {
        if flags & bit == 0 {
            return Some(None);
        }
        Some(Some(u32::try_from(read_varint(p, i)?).ok()?))
    };
    let count_left = count(4, i)?;
    let charged = count(8, i)?;
    Some(Ticket { id, count: count_left, charged })
}

/// Every ticket, when the list reads exactly to its end.
pub fn parse_tickets(p: &[u8]) -> Option<Vec<Ticket>> {
    let mut i = 0;
    let n = read_varint(p, &mut i)?;
    let mut out = Vec::new();
    for _ in 0..n.min(512) {
        out.push(ticket(p, &mut i)?);
    }
    (i == p.len()).then_some(out)
}

pub fn parse_ticket_update(p: &[u8]) -> Option<Ticket> {
    let mut i = 1;
    let t = ticket(p, &mut i)?;
    p.get(i)?; // the reason
    Some(t)
}

/// One layer's Artifact Siege.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct Siege {
    /// The game's AbyssArtifactEvent: 1001 the first layer, 2001 the second.
    pub event: u32,
    /// When the current holding began and when it resets (ms since 1970); 0 if none.
    pub since_ms: u64,
    pub until_ms: Option<u64>,
}

/// An artifact and who holds it ([`ELYOS`], [`ASMODIANS`], or 0 nobody).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct Artifact {
    /// 1001-1003 on the first layer, 2001-2003 on the second.
    pub id: u32,
    pub owner: u8,
}

const ARTIFACT_LEN: usize = 17;
const WAR_LEN: usize = 15;

/// The sieges and artifacts in a `0xe307` or `0xe305` message, when every byte reads.
pub fn parse_abyss(opcode: u16, p: &[u8]) -> Option<(Vec<Siege>, Vec<Artifact>)> {
    let (wars, start) = match opcode {
        OPCODE_ABYSS_MAP if p.get(..2)? == [0, 0] => (usize::from(*p.get(2)?), 3),
        OPCODE_ABYSS_ENTERED => (1, 0),
        _ => return None,
    };
    if wars > 8 {
        return None;
    }
    let mut i = start;
    let mut sieges = Vec::new();
    for _ in 0..wars {
        // Its first byte says whether an "until" follows (1 when it does).
        let until = *p.get(i)? & 1 != 0;
        let b = p.get(i..i + WAR_LEN + if until { 8 } else { 0 })?;
        let event = u32::from_le_bytes(b[1..5].try_into().ok()?);
        let since_ms = u64::from_le_bytes(b[7..15].try_into().ok()?);
        let until_ms = until.then(|| u64::from_le_bytes(b[15..23].try_into().expect("8 bytes")));
        sieges.push(Siege { event, since_ms, until_ms });
        i += b.len();
    }
    let n = usize::from(*p.get(i)?);
    if p.len() != i + 1 + n * ARTIFACT_LEN {
        return None;
    }
    let artifacts = p[i + 1..]
        .chunks_exact(ARTIFACT_LEN)
        .map(|a| Artifact { id: u32::from_le_bytes(a[..4].try_into().expect("4 bytes")), owner: a[8] })
        .collect();
    Some((sieges, artifacts))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::tests::hex;

    #[test]
    fn shop_counters_and_the_weekly_odyle_buy() {
        // From our 2026-10-24 capture: seven counters, the Odyle buy's 20 of 20.
        let p = hex(
            "00 00 07 91 04 00 00 00 00 00 00 80 fd 4e 81 a1 01 00 00 e6 04 00 00 00 00 00 00 80 75 d5 2a bb 03 00 00              ee 04 00 00 01 00 00 00 80 6d e8 95 a1 01 00 00 04 87 01 00 1e 00 00 00 80 fd 4e 81 a1 01 00 00              26 87 01 00 14 00 00 00 80 fd 4e 81 a1 01 00 00 27 87 01 00 00 00 00 00 80 fd 4e 81 a1 01 00 00              2d 87 01 00 00 00 00 00 80 fd 4e 81 a1 01 00 00",
        );
        let list = parse_shop_counters(&p).unwrap();
        assert_eq!(list.len(), 7);
        let odyle = list.iter().find(|c| c.id == ODYLE_PURCHASE).unwrap();
        // Wednesday 2026-10-28 07:00 UTC.
        assert_eq!((odyle.bought, odyle.reset_ms), (20, 1_793_170_800_000));
        assert!(parse_shop_counters(&p[..p.len() - 1]).is_none());
    }

    #[test]
    fn the_black_cloud_merchants_special_page() {
        // Opening its Special page (2026-11-14): 10 offers, every byte read.
        let p = hex(include_str!("../testdata/black-market.hex"));
        let page = parse_black_market(&p).unwrap();
        assert_eq!((page.page, page.offers.len(), page.refreshes), (BLACK_MARKET_SPECIAL, 10, 0));
        // 2026-11-14 01:30 in Adelaide: the next hour, UTC.
        assert_eq!(page.next_refresh_ms % 3_600_000, 0);
        // Pet: Congealed Drana, 100,000 Kinah at 30 % off (as the game showed it).
        let first = page.offers[0];
        assert_eq!((first.slot, first.item, first.count), (211_085, 534_760_187, 1));
        assert_eq!((first.step, first.currency, first.price, first.discounted_price), (2, 93, 100_000, 70_000));
        assert!(first.discounted && !first.sold_out);
        // Eternal Crest (a dagger skin), the last: the flags for offers 9 and 10 are in a third
        // byte of flags, after offer 9.
        let last = page.offers[9];
        assert_eq!((last.item, last.step, last.price, last.discounted_price), (535_710_131, 0, 960_000, 864_000));
        assert!(page.offers.iter().all(|o| o.discounted && !o.sold_out));
        // A byte short, or one over: not read.
        assert!(parse_black_market(&p[..p.len() - 1]).is_none());
        let mut more = p.clone();
        more.push(0);
        assert!(parse_black_market(&more).is_none());
    }

    #[test]
    fn flags_eight_to_a_byte_where_the_first_falls() {
        // Two offers: the flags' byte comes after the first, and holds both offers' flags.
        let offer = |slot: u32| {
            let mut o = slot.to_le_bytes().to_vec();
            o.extend(534_760_001u32.to_le_bytes());
            o.extend(1i64.to_le_bytes());
            o.extend([3, 93]);
            o.extend(100_800i64.to_le_bytes());
            o.extend(50_400i64.to_le_bytes());
            o
        };
        let mut p = vec![BLACK_MARKET_SKINS, 2];
        p.extend(offer(1));
        p.push(0b0000_1101); // first: discounted, not sold out; second: discounted, sold out
        p.extend(offer(2));
        p.extend(1_794_574_800_000i64.to_le_bytes());
        p.extend(3u32.to_le_bytes());
        let page = parse_black_market(&p).unwrap();
        assert_eq!(page.offers.iter().map(|o| (o.slot, o.discounted, o.sold_out)).collect::<Vec<_>>(), [(1, true, false), (2, true, true)]);
        assert_eq!((page.next_refresh_ms, page.refreshes), (1_794_574_800_000, 3));
    }

    #[test]
    fn the_ticket_list_and_odyle_energy() {
        // Entering Vakron Sky Island (2026-10-23): 73 tickets, every byte read.
        let list = parse_tickets(&hex(include_str!("../testdata/tickets.hex"))).unwrap();
        assert_eq!(list.len(), 73);
        let odyle = list.iter().find(|t| t.id == ODYLE_ENERGY).unwrap();
        assert_eq!((odyle.count, odyle.charged), (Some(355), Some(1345)));
        assert_eq!(list.iter().find(|t| t.id == 10).unwrap().count, Some(14));
        // Completing it: 80 energy gone.
        let spent = parse_ticket_update(&hex("00 0c 01 87 93 03 93 02 c1 0a 03")).unwrap();
        assert_eq!((spent.id, spent.count, spent.charged), (ODYLE_ENERGY, Some(275), Some(1345)));
        assert_eq!(parse_ticket_update(&hex("00 04 0a 00 00 00 0e 02")).unwrap().count, Some(14));
        // A list with a byte left over isn't one.
        assert!(parse_tickets(&hex("01 04 0a 00 00 00 0e 02")).is_none());
    }

    #[test]
    fn who_holds_the_abyss_artifacts() {
        // Built from the documented layout: two layers, the first held since a time until
        // its reset, the second by nobody; three artifacts each.
        let mut p = vec![0, 0, 2];
        p.extend([1, 0xe9, 3, 0, 0, 0, 0]); // 1001
        p.extend(1_792_000_000_000u64.to_le_bytes());
        p.extend(1_792_100_000_000u64.to_le_bytes());
        p.extend([0, 0xd1, 7, 0, 0, 0, 0]); // 2001, no "until"
        p.extend(0u64.to_le_bytes());
        p.push(6);
        for (id, owner) in [(1001u32, 1u8), (1002, 2), (1003, 1), (2001, 0), (2002, 0), (2003, 2)] {
            p.extend(id.to_le_bytes());
            p.extend([0, 0, 0, 0, owner]);
            p.extend([0; 8]);
        }
        let (sieges, artifacts) = parse_abyss(OPCODE_ABYSS_MAP, &p).unwrap();
        assert_eq!(sieges[0], Siege { event: 1001, since_ms: 1_792_000_000_000, until_ms: Some(1_792_100_000_000) });
        assert_eq!(sieges[1], Siege { event: 2001, since_ms: 0, until_ms: None });
        assert_eq!(artifacts.iter().map(|a| (a.id, a.owner)).collect::<Vec<_>>()[..3], [(1001, 1), (1002, 2), (1003, 1)]);
        // One byte off: not read at all.
        p.push(0);
        assert!(parse_abyss(OPCODE_ABYSS_MAP, &p).is_none());
    }

    #[test]
    fn the_abyss_map_in_game() {
        // Opening the Abyss map (2026-10-23): the second layer's war first, with nobody
        // holding its artifacts; the Asmodians hold the first layer's until its reset.
        let (sieges, artifacts) = parse_abyss(OPCODE_ABYSS_MAP, &hex(include_str!("../testdata/abyss-map.hex"))).unwrap();
        assert_eq!(sieges[0], Siege { event: 2001, since_ms: 0, until_ms: None });
        assert_eq!(sieges[1], Siege { event: 1001, since_ms: 1_792_672_095_000, until_ms: Some(1_792_843_800_000) });
        let held: Vec<(u32, u8)> = artifacts.iter().map(|a| (a.id, a.owner)).collect();
        assert_eq!(held, [(2001, 0), (2002, 0), (2003, 0), (1001, ASMODIANS), (1002, ASMODIANS), (1003, ASMODIANS)]);
    }
}
