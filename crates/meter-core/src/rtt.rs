//! Ping: the TCP round trip between your PC and the game server, measured passively.
//!
//! When the game sends data, we note when; when the server's acknowledgement for it comes
//! back, the difference is one round-trip sample. Retransmitted data is skipped (its ACK
//! can't be matched to a send time). The reported ping is the median of the last few
//! seconds of samples, which ignores one-off spikes. No packets of our own are sent.

use std::collections::VecDeque;

/// Samples older than this no longer count.
const WINDOW_US: u64 = 10_000_000;
/// Sends still waiting for an ACK beyond this are dropped (the ACK was missed).
const MAX_PENDING: usize = 512;
/// Anything slower than this is a stall, not a ping.
const MAX_RTT_US: u64 = 5_000_000;

#[derive(Debug, Default)]
pub struct RttTracker {
    /// (sequence number just past the sent data, send time, was retransmitted)
    pending: VecDeque<(u32, u64, bool)>,
    /// (time the sample was taken, round trip in µs)
    samples: VecDeque<(u64, u64)>,
}

impl RttTracker {
    /// The game sent `len` bytes starting at `seq`.
    pub fn on_send(&mut self, ts_us: u64, seq: u32, len: usize) {
        if len == 0 {
            return;
        }
        let end = seq.wrapping_add(len as u32);
        if let Some(p) = self.pending.iter_mut().find(|p| p.0 == end) {
            p.2 = true; // sent again: its ACK can't be timed
            return;
        }
        self.pending.push_back((end, ts_us, false));
        if self.pending.len() > MAX_PENDING {
            self.pending.pop_front();
        }
    }

    /// The server acknowledged everything before `ack`.
    pub fn on_ack(&mut self, ts_us: u64, ack: u32) {
        while let Some(&(end, sent, retransmitted)) = self.pending.front() {
            // `ack` covers `end` if it's at or past it (with sequence wraparound).
            if ack.wrapping_sub(end) >= 0x8000_0000 {
                break;
            }
            self.pending.pop_front();
            let rtt = ts_us.saturating_sub(sent);
            if !retransmitted && rtt <= MAX_RTT_US {
                self.samples.push_back((ts_us, rtt));
            }
        }
        while self.samples.front().is_some_and(|(t, _)| ts_us.saturating_sub(*t) > WINDOW_US) {
            self.samples.pop_front();
        }
    }

    /// Median round trip over the last few seconds, in milliseconds.
    pub fn ping_ms(&self, now_us: u64) -> Option<u32> {
        let mut recent: Vec<u64> = self
            .samples
            .iter()
            .filter(|(t, _)| now_us.saturating_sub(*t) <= WINDOW_US)
            .map(|(_, r)| *r)
            .collect();
        if recent.is_empty() {
            return None;
        }
        recent.sort_unstable();
        Some((recent[recent.len() / 2] / 1000) as u32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MS: u64 = 1000;

    #[test]
    fn median_of_round_trips() {
        let mut r = RttTracker::default();
        for (i, rtt) in [140u64, 150, 400, 145, 160].into_iter().enumerate() {
            let t = i as u64 * 1000 * MS;
            r.on_send(t, 1000 + i as u32 * 10, 10);
            r.on_ack(t + rtt * MS, 1000 + (i as u32 + 1) * 10);
        }
        // The 400 ms spike doesn't move the median.
        assert_eq!(r.ping_ms(5000 * MS), Some(150));
    }

    #[test]
    fn one_ack_can_cover_several_sends() {
        let mut r = RttTracker::default();
        r.on_send(0, 0, 10);
        r.on_send(20 * MS, 10, 10);
        r.on_ack(150 * MS, 20);
        // Samples 150 and 130; with an even count the upper middle value is reported.
        assert_eq!(r.ping_ms(150 * MS), Some(150));
    }

    #[test]
    fn retransmissions_are_not_timed() {
        let mut r = RttTracker::default();
        r.on_send(0, 0, 10);
        r.on_send(300 * MS, 0, 10); // sent again
        r.on_ack(450 * MS, 10);
        assert_eq!(r.ping_ms(450 * MS), None);
    }

    #[test]
    fn sequence_wraparound() {
        let mut r = RttTracker::default();
        r.on_send(0, u32::MAX - 4, 10); // ends at 5 after wrapping
        r.on_ack(100 * MS, u32::MAX); // not yet
        assert_eq!(r.ping_ms(100 * MS), None);
        r.on_ack(120 * MS, 5);
        assert_eq!(r.ping_ms(120 * MS), Some(120));
    }

    #[test]
    fn old_samples_expire() {
        let mut r = RttTracker::default();
        r.on_send(0, 0, 10);
        r.on_ack(100 * MS, 10);
        assert_eq!(r.ping_ms(100 * MS), Some(100));
        assert_eq!(r.ping_ms(20_000 * MS), None);
    }
}
