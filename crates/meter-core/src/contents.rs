//! Content tickets (how many entries you have left for each content, and Odyle energy)
//! and who holds the Abyss artifacts.
//!
//! Ticket layouts follow Questlog's message list and read every byte of the lists in our
//! captures (73 tickets each). Odyle energy is ticket [`ODYLE_ENERGY`]: its basic and
//! charged energy, which fell by 80 each time a dungeon was completed (490 to 410, 355 to
//! 275). The other tickets' contents aren't named in any data we have.
//!
//! The Abyss artifacts' layout is from A2Tools' notes (read against the in-game map); it
//! isn't in any of our captures yet, so a message is only read when every byte of it falls
//! into place.

use serde::{Deserialize, Serialize};

use crate::protocol::read_varint;

/// Every ticket: a varint count, then the tickets.
pub const OPCODE_TICKETS: u16 = 0x610b;
/// One ticket changed: u8 flags, the ticket, u8 reason, then with flag 1 an i32.
pub const OPCODE_TICKET_UPDATE: u16 = 0x610c;
/// Odyle energy's ticket: its count is the basic energy, its charges the charged energy.
pub const ODYLE_ENERGY: u32 = 60_000_001;

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
    // A war's "until" is there or not: whichever reading accounts for every byte.
    for with_until in 0u32..(1 << wars) {
        let mut i = start;
        let mut sieges = Vec::new();
        let mut ok = true;
        for w in 0..wars {
            let until = with_until & (1 << w) != 0;
            let Some(b) = p.get(i..i + WAR_LEN + if until { 8 } else { 0 }) else {
                ok = false;
                break;
            };
            let event = u32::from_le_bytes(b[1..5].try_into().ok()?);
            let since_ms = u64::from_le_bytes(b[7..15].try_into().ok()?);
            let until_ms = until.then(|| u64::from_le_bytes(b[15..23].try_into().expect("8 bytes")));
            sieges.push(Siege { event, since_ms, until_ms });
            i += b.len();
        }
        let Some(&n) = p.get(i).filter(|_| ok) else { continue };
        if p.len() != i + 1 + usize::from(n) * ARTIFACT_LEN {
            continue;
        }
        let artifacts = p[i + 1..]
            .chunks_exact(ARTIFACT_LEN)
            .map(|a| Artifact { id: u32::from_le_bytes(a[..4].try_into().expect("4 bytes")), owner: a[8] })
            .collect();
        return Some((sieges, artifacts));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::tests::hex;

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
        p.extend([1, 0xd1, 7, 0, 0, 0, 0]); // 2001
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
}
