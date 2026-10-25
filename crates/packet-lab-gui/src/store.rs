//! The messages kept (the latest [`KEEP`]), their counts by opcode, and the page's queries:
//! the stream through a filter, one message's bytes, and finding a number or bytes.

use std::collections::{BTreeMap, HashSet, VecDeque};

use serde::{Deserialize, Serialize};

use meter_core::session::TimedMessage;

/// Messages kept; older ones are dropped (their counts stay).
pub const KEEP: usize = 200_000;
/// The stream's rate is over this much of its latest time.
const RATE_WINDOW_US: u64 = 5_000_000;
/// An opcode's rate is over this much.
const OPCODE_RATE_WINDOW_US: u64 = 10_000_000;
/// Bytes shown in a row's preview.
const HEAD_BYTES: usize = 24;
/// Rows sent at most per query.
const MAX_ROWS: usize = 2_000;

struct Row {
    seq: u64,
    ts_us: u64,
    opcode: u16,
    payload: Vec<u8>,
}

#[derive(Default)]
struct Stat {
    count: u64,
    bytes: u64,
    last_us: u64,
}

#[derive(Default)]
pub struct Store {
    rows: VecDeque<Row>,
    next_seq: u64,
    stats: BTreeMap<u16, Stat>,
    first_us: Option<u64>,
    total: u64,
}

/// What the page asks for: messages of these opcodes (all if none) holding `find`, newer
/// than `after` or older than `before`, at most `limit` (the newest of them).
#[derive(Debug, Default, Deserialize)]
pub struct Query {
    pub opcodes: Option<Vec<u16>>,
    pub find: Option<String>,
    pub after: Option<u64>,
    pub before: Option<u64>,
    pub limit: Option<usize>,
}

#[derive(Serialize)]
pub struct RowView {
    seq: u64,
    /// Seconds from the source's first message.
    t: f64,
    opcode: u16,
    len: usize,
    /// The first bytes, in hex.
    head: String,
}

#[derive(Serialize)]
pub struct RowsView {
    /// Oldest first.
    rows: Vec<RowView>,
    /// More matching messages are older than the first row (and newer than `after`).
    more: bool,
}

#[derive(Serialize)]
pub struct StatView {
    opcode: u16,
    /// Its family, by high byte (for opcodes the notes don't list too).
    family: &'static str,
    count: u64,
    bytes: u64,
    /// When it was last seen (seconds from the first message).
    last_s: f64,
    /// A second, over the latest stretch of the stream.
    rate: f64,
}

#[derive(Serialize)]
pub struct MessageView {
    seq: u64,
    t: f64,
    opcode: u16,
    payload: Vec<u8>,
    /// Where the find matches: (offset, length).
    marks: Vec<(usize, usize)>,
}

impl Store {
    /// Empties the store (message numbers keep counting up, so the page never mixes old
    /// and new).
    pub fn clear(&mut self) {
        let next_seq = self.next_seq;
        *self = Self { next_seq, ..Self::default() };
    }

    pub fn push(&mut self, messages: Vec<TimedMessage>) {
        for m in messages {
            self.first_us.get_or_insert(m.ts_us);
            let s = self.stats.entry(m.opcode).or_default();
            s.count += 1;
            s.bytes += m.payload.len() as u64;
            s.last_us = m.ts_us;
            self.total += 1;
            self.rows.push_back(Row { seq: self.next_seq, ts_us: m.ts_us, opcode: m.opcode, payload: m.payload });
            self.next_seq += 1;
        }
        while self.rows.len() > KEEP {
            self.rows.pop_front();
        }
    }

    pub fn total(&self) -> u64 {
        self.total
    }

    pub fn kept(&self) -> usize {
        self.rows.len()
    }

    fn seconds(&self, ts_us: u64) -> f64 {
        ts_us.saturating_sub(self.first_us.unwrap_or(ts_us)) as f64 / 1e6
    }

    pub fn span_s(&self) -> f64 {
        self.rows.back().map_or(0.0, |r| self.seconds(r.ts_us))
    }

    /// The latest timestamp, and the rows within `window` of it.
    fn recent(&self, window: u64) -> impl Iterator<Item = &Row> {
        let last = self.rows.back().map_or(0, |r| r.ts_us);
        self.rows.iter().rev().take_while(move |r| r.ts_us + window >= last)
    }

    pub fn rate(&self) -> f64 {
        self.recent(RATE_WINDOW_US).count() as f64 / (RATE_WINDOW_US as f64 / 1e6)
    }

    pub fn stats(&self) -> Vec<StatView> {
        let mut recent: BTreeMap<u16, u64> = BTreeMap::new();
        for r in self.recent(OPCODE_RATE_WINDOW_US) {
            *recent.entry(r.opcode).or_default() += 1;
        }
        let window_s = OPCODE_RATE_WINDOW_US as f64 / 1e6;
        self.stats
            .iter()
            .map(|(&opcode, s)| StatView {
                opcode,
                family: crate::catalog::family(opcode),
                count: s.count,
                bytes: s.bytes,
                last_s: self.seconds(s.last_us),
                rate: recent.get(&opcode).copied().unwrap_or(0) as f64 / window_s,
            })
            .collect()
    }

    pub fn rows(&self, q: &Query, find: Option<&Find>) -> RowsView {
        let limit = q.limit.unwrap_or(500).clamp(1, MAX_ROWS);
        let opcodes: Option<HashSet<u16>> = q.opcodes.as_ref().map(|o| o.iter().copied().collect());
        let mut rows = Vec::new();
        let mut more = false;
        for r in self.rows.iter().rev() {
            if q.before.is_some_and(|b| r.seq >= b) {
                continue;
            }
            if q.after.is_some_and(|a| r.seq <= a) {
                break;
            }
            if opcodes.as_ref().is_some_and(|o| !o.contains(&r.opcode)) || find.is_some_and(|f| !f.matches(&r.payload)) {
                continue;
            }
            if rows.len() == limit {
                more = true;
                break;
            }
            rows.push(RowView { seq: r.seq, t: self.seconds(r.ts_us), opcode: r.opcode, len: r.payload.len(), head: hex(&r.payload[..r.payload.len().min(HEAD_BYTES)]) });
        }
        rows.reverse();
        RowsView { rows, more }
    }

    pub fn message(&self, seq: u64, find: Option<&Find>) -> Option<MessageView> {
        // Rows are in seq order with no gaps: the one wanted is at a known distance from the front.
        let first = self.rows.front()?.seq;
        let r = self.rows.get(usize::try_from(seq.checked_sub(first)?).ok()?)?;
        Some(MessageView {
            seq: r.seq,
            t: self.seconds(r.ts_us),
            opcode: r.opcode,
            payload: r.payload.clone(),
            marks: find.map(|f| f.marks(&r.payload)).unwrap_or_default(),
        })
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" ")
}

/// What to find in payloads: a number (decimal, or hex with 0x), as a varint and as
/// little-endian u16, u32 and u64 where it fits (as `packet-lab find` does); or bytes
/// written in hex ("8d 00 2e").
pub struct Find {
    patterns: Vec<Vec<u8>>,
}

impl Find {
    pub fn parse(s: &str) -> Result<Self, String> {
        let s = s.trim();
        let number = match s.strip_prefix("0x") {
            Some(h) => Some(u64::from_str_radix(h, 16).map_err(|_| format!("{s} isn't a number"))?),
            None if !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()) => Some(s.parse::<u64>().map_err(|e| e.to_string())?),
            None => None,
        };
        let mut patterns: Vec<Vec<u8>> = match number {
            Some(n) => {
                let mut p = vec![varint(n)];
                if let Ok(v) = u16::try_from(n) {
                    p.push(v.to_le_bytes().to_vec());
                }
                if let Ok(v) = u32::try_from(n) {
                    p.push(v.to_le_bytes().to_vec());
                }
                p.push(n.to_le_bytes().to_vec());
                p
            }
            None => {
                let digits: String = s.chars().filter(|c| !c.is_whitespace()).collect();
                if !digits.len().is_multiple_of(2) || !digits.chars().all(|c| c.is_ascii_hexdigit()) {
                    return Err(format!("find a number (123 or 0x7b) or bytes in hex (7b 00); not \"{s}\""));
                }
                let bytes = (0..digits.len()).step_by(2).map(|i| u8::from_str_radix(&digits[i..i + 2], 16).expect("checked hex")).collect();
                vec![bytes]
            }
        };
        patterns.retain(|p| !p.is_empty());
        patterns.dedup();
        Ok(Self { patterns })
    }

    pub fn matches(&self, payload: &[u8]) -> bool {
        self.patterns.is_empty() || self.patterns.iter().any(|p| payload.windows(p.len()).any(|w| w == p.as_slice()))
    }

    pub fn marks(&self, payload: &[u8]) -> Vec<(usize, usize)> {
        let mut out = Vec::new();
        for p in &self.patterns {
            for (i, w) in payload.windows(p.len()).enumerate() {
                if w == p.as_slice() && !out.iter().any(|&(s, l): &(usize, usize)| i < s + l && s < i + p.len()) {
                    out.push((i, p.len()));
                }
            }
        }
        out.sort_unstable();
        out
    }
}

fn varint(mut n: u64) -> Vec<u8> {
    let mut out = Vec::new();
    loop {
        let b = (n & 0x7f) as u8;
        n >>= 7;
        if n == 0 {
            out.push(b);
            return out;
        }
        out.push(b | 0x80);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(ts_s: u64, opcode: u16, payload: &[u8]) -> TimedMessage {
        TimedMessage { ts_us: ts_s * 1_000_000, opcode, payload: payload.to_vec() }
    }

    #[test]
    fn rows_filter_page_and_find() {
        let mut s = Store::default();
        s.push(vec![msg(10, 0x3804, &[0x2e, 0x17, 0x03]), msg(11, 0x8d00, &[0xd2, 0x09]), msg(12, 0x3804, &[0x05, 0xd2, 0x09, 0x00])]);
        let all = s.rows(&Query::default(), None);
        assert_eq!(all.rows.iter().map(|r| r.seq).collect::<Vec<_>>(), [0, 1, 2]);
        assert_eq!(all.rows[2].t, 2.0);
        let hits = s.rows(&Query { opcodes: Some(vec![0x3804]), ..Query::default() }, None);
        assert_eq!(hits.rows.len(), 2);
        // 1234 is d2 09 as a u16 and as a varint (d2 09).
        let f = Find::parse("1234").unwrap();
        assert_eq!(s.rows(&Query::default(), Some(&f)).rows.iter().map(|r| r.seq).collect::<Vec<_>>(), [1, 2]);
        assert_eq!(s.message(2, Some(&f)).unwrap().marks, [(1, 2)]);
        // Newer than 0, at most one: the newest, with more left.
        let page = s.rows(&Query { after: Some(0), limit: Some(1), ..Query::default() }, None);
        assert_eq!((page.rows[0].seq, page.more), (2, true));
        assert_eq!(s.stats().iter().find(|x| x.opcode == 0x3804).unwrap().count, 2);
        // Cleared: message numbers carry on.
        s.clear();
        s.push(vec![msg(20, 0x3600, &[1])]);
        assert_eq!(s.rows(&Query::default(), None).rows[0].seq, 3);
        assert!(Find::parse("zz").is_err());
        assert_eq!(Find::parse("2e 17").unwrap().patterns, vec![vec![0x2e, 0x17]]);
    }
}
