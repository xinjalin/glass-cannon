//! Rebuilds one direction of a TCP connection into an ordered byte stream.
//!
//! Handles out-of-order segments, retransmissions, overlapping data and sequence-number
//! wraparound. If a hole never fills (the capture dropped a packet), the stream skips
//! past it after a short wait and reports a gap instead of stalling forever.

use std::collections::BTreeMap;

/// Buffered out-of-order data beyond this forces a skip past the hole.
pub const MAX_PENDING_BYTES: usize = 4 << 20;
/// A hole older than this is assumed lost.
pub const MAX_PENDING_AGE_US: u64 = 2_000_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamEvent {
    Data { offset: u64, bytes: Vec<u8> },
    Gap { offset: u64, len: u64 },
}

#[derive(Debug, Default, Clone)]
pub struct HalfStats {
    pub segments: u64,
    pub delivered: u64,
    pub duplicate_bytes: u64,
    pub gap_bytes: u64,
    pub resets: u64,
}

#[derive(Debug, Default)]
pub struct HalfStream {
    /// Sequence number of stream offset 0.
    isn: Option<u32>,
    /// Stream offset of the next byte we expect.
    delivered: u64,
    /// Out-of-order data keyed by stream offset: (capture time, bytes).
    pending: BTreeMap<u64, (u64, Vec<u8>)>,
    pending_bytes: usize,
    /// True when capture started after the connection opened (no SYN seen).
    midstream: bool,
    stats: HalfStats,
}

impl HalfStream {
    pub fn delivered(&self) -> u64 {
        self.delivered
    }

    pub fn midstream(&self) -> bool {
        self.midstream
    }

    pub fn stats(&self) -> &HalfStats {
        &self.stats
    }

    pub fn push(&mut self, ts_us: u64, seq: u32, syn: bool, payload: &[u8], out: &mut Vec<StreamEvent>) {
        self.stats.segments += 1;
        let mut seq = seq;
        if syn {
            let isn = seq.wrapping_add(1);
            if self.isn != Some(isn) {
                if self.delivered > 0 || !self.pending.is_empty() {
                    // The same address/port pair is being reused by a new connection.
                    self.finish(out);
                    self.stats.resets += 1;
                }
                self.isn = Some(isn);
                self.delivered = 0;
                self.midstream = false;
            }
            seq = isn;
        }
        if payload.is_empty() {
            self.flush_stale(ts_us, out);
            return;
        }
        let isn = match self.isn {
            Some(isn) => isn,
            None => {
                self.midstream = true;
                self.isn = Some(seq);
                seq
            }
        };

        // Signed distance from the next expected sequence number handles wraparound.
        let expected = isn.wrapping_add(self.delivered as u32);
        let rel = seq.wrapping_sub(expected) as i32 as i64;
        let start = self.delivered as i64 + rel;
        let end = start + payload.len() as i64;
        let delivered = self.delivered as i64;

        if end <= delivered {
            self.stats.duplicate_bytes += payload.len() as u64;
        } else if start <= delivered {
            let skip = (delivered - start) as usize;
            self.stats.duplicate_bytes += skip as u64;
            self.emit(&payload[skip..], out);
            self.drain(out);
        } else {
            self.park(start as u64, ts_us, payload);
        }
        self.flush_stale(ts_us, out);
    }

    /// Skips holes that have waited too long or buffered too much data.
    pub fn flush_stale(&mut self, now_us: u64, out: &mut Vec<StreamEvent>) {
        loop {
            let Some((&first, _)) = self.pending.first_key_value() else {
                return;
            };
            let oldest = self.pending.values().map(|(t, _)| *t).min().unwrap_or(now_us);
            let stale = self.pending_bytes > MAX_PENDING_BYTES
                || now_us.saturating_sub(oldest) > MAX_PENDING_AGE_US;
            if !stale {
                return;
            }
            self.skip_to(first, out);
            self.drain(out);
        }
    }

    /// Delivers everything still buffered, skipping any holes. Call when the flow ends.
    pub fn finish(&mut self, out: &mut Vec<StreamEvent>) {
        while let Some((&first, _)) = self.pending.first_key_value() {
            self.skip_to(first, out);
            self.drain(out);
        }
    }

    fn emit(&mut self, bytes: &[u8], out: &mut Vec<StreamEvent>) {
        out.push(StreamEvent::Data {
            offset: self.delivered,
            bytes: bytes.to_vec(),
        });
        self.delivered += bytes.len() as u64;
        self.stats.delivered += bytes.len() as u64;
    }

    fn park(&mut self, offset: u64, ts_us: u64, payload: &[u8]) {
        if let Some((_, existing)) = self.pending.get(&offset) {
            if existing.len() >= payload.len() {
                self.stats.duplicate_bytes += payload.len() as u64;
                return;
            }
            self.pending_bytes -= existing.len();
        }
        self.pending_bytes += payload.len();
        self.pending.insert(offset, (ts_us, payload.to_vec()));
    }

    fn drain(&mut self, out: &mut Vec<StreamEvent>) {
        while let Some((&offset, _)) = self.pending.first_key_value() {
            if offset > self.delivered {
                break;
            }
            let (_, (_, data)) = self.pending.pop_first().expect("entry just seen");
            self.pending_bytes -= data.len();
            let end = offset + data.len() as u64;
            if end <= self.delivered {
                self.stats.duplicate_bytes += data.len() as u64;
                continue;
            }
            let skip = (self.delivered - offset) as usize;
            self.stats.duplicate_bytes += skip as u64;
            self.emit(&data[skip..], out);
        }
    }

    fn skip_to(&mut self, offset: u64, out: &mut Vec<StreamEvent>) {
        if offset > self.delivered {
            let len = offset - self.delivered;
            out.push(StreamEvent::Gap {
                offset: self.delivered,
                len,
            });
            self.stats.gap_bytes += len;
            self.delivered = offset;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data(offset: u64, bytes: &[u8]) -> StreamEvent {
        StreamEvent::Data {
            offset,
            bytes: bytes.to_vec(),
        }
    }

    fn opened(isn_minus_one: u32) -> HalfStream {
        let mut s = HalfStream::default();
        s.push(0, isn_minus_one, true, b"", &mut Vec::new());
        s
    }

    #[test]
    fn in_order() {
        let mut s = opened(1000);
        let mut ev = Vec::new();
        s.push(1, 1001, false, b"abc", &mut ev);
        s.push(2, 1004, false, b"def", &mut ev);
        assert_eq!(ev, vec![data(0, b"abc"), data(3, b"def")]);
        assert!(!s.midstream());
    }

    #[test]
    fn out_of_order() {
        let mut s = opened(1000);
        let mut ev = Vec::new();
        s.push(1, 1004, false, b"def", &mut ev);
        assert!(ev.is_empty());
        s.push(2, 1001, false, b"abc", &mut ev);
        assert_eq!(ev, vec![data(0, b"abc"), data(3, b"def")]);
    }

    #[test]
    fn retransmission_with_overlap() {
        let mut s = opened(1000);
        let mut ev = Vec::new();
        s.push(1, 1001, false, b"abcd", &mut ev);
        s.push(2, 1003, false, b"cdef", &mut ev);
        s.push(3, 1001, false, b"ab", &mut ev);
        assert_eq!(ev, vec![data(0, b"abcd"), data(4, b"ef")]);
        assert_eq!(s.stats().duplicate_bytes, 4);
    }

    #[test]
    fn sequence_wraparound() {
        let mut s = opened(u32::MAX - 1);
        let mut ev = Vec::new();
        s.push(1, u32::MAX, false, b"ab", &mut ev);
        s.push(2, 1, false, b"cd", &mut ev);
        assert_eq!(ev, vec![data(0, b"ab"), data(2, b"cd")]);
    }

    #[test]
    fn stale_hole_becomes_gap() {
        let mut s = opened(1000);
        let mut ev = Vec::new();
        s.push(0, 1001, false, b"abc", &mut ev);
        s.push(0, 1010, false, b"xyz", &mut ev);
        assert_eq!(ev, vec![data(0, b"abc")]);
        s.flush_stale(MAX_PENDING_AGE_US + 1, &mut ev);
        assert_eq!(
            ev,
            vec![
                data(0, b"abc"),
                StreamEvent::Gap { offset: 3, len: 6 },
                data(9, b"xyz")
            ]
        );
    }

    #[test]
    fn midstream_start() {
        let mut s = HalfStream::default();
        let mut ev = Vec::new();
        s.push(0, 5000, false, b"hi", &mut ev);
        s.push(0, 5002, false, b"!", &mut ev);
        assert_eq!(ev, vec![data(0, b"hi"), data(2, b"!")]);
        assert!(s.midstream());
    }

    #[test]
    fn finish_flushes_pending() {
        let mut s = opened(0);
        let mut ev = Vec::new();
        s.push(0, 5, false, b"late", &mut ev);
        s.finish(&mut ev);
        assert_eq!(
            ev,
            vec![StreamEvent::Gap { offset: 0, len: 4 }, data(4, b"late")]
        );
    }
}
