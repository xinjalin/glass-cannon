//! Turns live reassembled TCP data into game messages.
//!
//! The game process also keeps a few TLS (HTTPS) connections open. Their encrypted
//! bytes could occasionally look like a message, so a connection's messages are held
//! back until it has framed cleanly for a while. TLS breaks framing within a few
//! records; the game connection never does once it's in step.
//!
//! Getting in step can take a few tries, though: a meter started while the game is
//! connected often sees its first packet begin in the middle of a message (anything the
//! server sends that's bigger than one packet), and a packet the capture drops does the
//! same. So a connection that loses framing before it's trusted isn't written off at once:
//! what it held is thrown away and it starts again, with a longer clean run to prove
//! itself ([`CONFIRM_AFTER_SLIP`]), up to [`MAX_SLIPS`] times. Random bytes break framing
//! about once in five messages, so they fail that every time; the game's stream passes it
//! from the first packet that begins on a message. (The pipeline says when a connection
//! was joined part-way, as a gap of no bytes, so the framer starts out wary: see
//! [`Framer::reset`].)

use std::collections::HashMap;

use crate::pipeline::{ChunkKind, StreamChunk};
use crate::protocol::{Framer, Message};
use crate::recorder::Side;
use crate::session::TimedMessage;

/// Messages that must frame cleanly before a connection is trusted.
pub const CONFIRM_AFTER: usize = 50;
/// The same, for a connection that has lost framing before: twice as many, so the extra
/// tries don't make it any likelier that random bytes pass.
pub const CONFIRM_AFTER_SLIP: usize = 100;
/// A connection that loses framing this many times without ever being trusted isn't the
/// game's. Finding the first packet that begins on a message can take a slip per packet,
/// and in a raid's traffic a few thousand packets can go by before one does (joining our
/// Force recording at 401 places, 7 took over 1,000). Each try's odds for random bytes
/// are about 1 in 10^10, so this many still leaves them 1 in 100,000.
pub const MAX_SLIPS: u32 = 100_000;
/// Web traffic; never the game.
const HTTPS_PORT: u16 = 443;

#[derive(Default)]
struct FlowState {
    framer: Framer,
    confirmed: bool,
    rejected: bool,
    /// Times framing was lost before the flow was confirmed.
    slips: u32,
    /// Held until the flow is confirmed.
    pending: Vec<TimedMessage>,
}

#[derive(Default)]
pub struct GameStreams {
    flows: HashMap<u32, FlowState>,
    resyncs: u64,
}

impl GameStreams {
    /// Feeds one chunk; appends any game messages it completes to `out`.
    pub fn push(&mut self, chunk: StreamChunk, out: &mut Vec<TimedMessage>) {
        if chunk.side != Side::In || chunk.remote.port() == HTTPS_PORT {
            return;
        }
        match chunk.kind {
            ChunkKind::Open => {
                self.flows.insert(chunk.flow, FlowState::default());
            }
            ChunkKind::Close => {
                self.flows.remove(&chunk.flow);
            }
            ChunkKind::Gap(_) => {
                if let Some(f) = self.flows.get_mut(&chunk.flow) {
                    f.framer.reset();
                }
            }
            ChunkKind::Data(bytes) => {
                let f = self.flows.entry(chunk.flow).or_default();
                if f.rejected {
                    return;
                }
                let mut batch: Vec<Message> = Vec::new();
                let framed = f.framer.push(&bytes, &mut batch);
                let timed = batch.into_iter().map(|m| TimedMessage {
                    ts_us: chunk.ts_us,
                    opcode: m.opcode,
                    payload: m.payload,
                });
                if f.confirmed {
                    if framed.is_err() {
                        self.resyncs += 1;
                    }
                    out.extend(timed);
                    return;
                }
                if framed.is_err() {
                    // Out of step: what it held may be nonsense. Start again, unless it
                    // has done this too often to be the game.
                    f.pending.clear();
                    f.slips += 1;
                    f.rejected = f.slips > MAX_SLIPS;
                    return;
                }
                f.pending.extend(timed);
                let needed = if f.slips == 0 { CONFIRM_AFTER } else { CONFIRM_AFTER_SLIP };
                if f.pending.len() >= needed {
                    f.confirmed = true;
                    out.append(&mut f.pending);
                }
            }
        }
    }

    /// Whether any connection has been confirmed as the game.
    pub fn connected(&self) -> bool {
        self.flows.values().any(|f| f.confirmed)
    }

    /// Times framing was lost on a confirmed game connection.
    pub fn resyncs(&self) -> u64 {
        self.resyncs
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::tests::frame;
    use std::net::SocketAddr;

    fn chunk(flow: u32, port: u16, kind: ChunkKind) -> StreamChunk {
        let remote: SocketAddr = format!("1.2.3.4:{port}").parse().unwrap();
        StreamChunk {
            flow,
            remote,
            side: Side::In,
            ts_us: flow as u64,
            kind,
        }
    }

    #[test]
    fn game_messages_are_released_once_confirmed() {
        let mut g = GameStreams::default();
        let mut out = Vec::new();
        g.push(chunk(1, 13328, ChunkKind::Open), &mut out);
        for i in 0..CONFIRM_AFTER - 1 {
            g.push(chunk(1, 13328, ChunkKind::Data(frame(0x3600, &[i as u8]))), &mut out);
        }
        assert!(out.is_empty());
        assert!(!g.connected());
        g.push(chunk(1, 13328, ChunkKind::Data(frame(0x364a, b"x"))), &mut out);
        assert_eq!(out.len(), CONFIRM_AFTER);
        assert_eq!(out[0].payload, vec![0]);
        assert!(g.connected());
        // After confirmation, messages flow straight through.
        g.push(chunk(1, 13328, ChunkKind::Data(frame(0x3804, b"hit"))), &mut out);
        assert_eq!(out.last().unwrap().opcode, 0x3804);
    }

    /// Bytes that look random, as encrypted ones do.
    fn noise(seed: &mut u32, n: usize) -> Vec<u8> {
        (0..n)
            .map(|_| {
                *seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                (*seed >> 16) as u8
            })
            .collect()
    }

    #[test]
    fn tls_like_stream_is_rejected() {
        let mut g = GameStreams::default();
        let mut out = Vec::new();
        let mut seed = 7;
        // TLS records: content type, version, length, then encrypted bytes. 3 MB of them.
        let mut record = |g: &mut GameStreams, out: &mut Vec<TimedMessage>| {
            let mut tls = vec![0x17, 0x03, 0x03, 0x05, 0x64];
            tls.extend(noise(&mut seed, 1380));
            g.push(chunk(2, 8443, ChunkKind::Data(tls)), out);
        };
        for _ in 0..2000 {
            record(&mut g, &mut out);
        }
        assert!(out.is_empty(), "random bytes never frame cleanly for long enough");
        assert!(!g.connected());
        let slips = g.flows[&2].slips;
        assert!(slips > 100, "it keeps losing framing: {slips} times in 2,000 packets");
        // After enough of that (a few hundred megabytes) it's written off...
        g.flows.get_mut(&2).unwrap().slips = MAX_SLIPS;
        while !g.flows[&2].rejected {
            record(&mut g, &mut out);
        }
        // ...and nothing it sends counts, however well it frames.
        for _ in 0..CONFIRM_AFTER_SLIP + 1 {
            g.push(chunk(2, 8443, ChunkKind::Data(frame(0x3600, b"ok"))), &mut out);
        }
        assert!(out.is_empty());
    }

    #[test]
    fn a_connection_joined_in_the_middle_of_a_message_is_still_the_game() {
        // 2,000 messages of 20 to 120 random bytes (about 150 KB).
        let mut seed = 0x1234_5678;
        let messages: Vec<Vec<u8>> = (0..2000)
            .map(|_| {
                let n = 20 + noise(&mut seed, 1)[0] as usize % 100;
                frame(0x3804, &noise(&mut seed, n))
            })
            .collect();
        // The meter starts `cut` bytes into the first one, wherever that is.
        for cut in 1..messages[0].len() {
            let mut g = GameStreams::default();
            let mut out = Vec::new();
            g.push(chunk(1, 13328, ChunkKind::Open), &mut out);
            // Joined part-way: the pipeline says so.
            g.push(chunk(1, 13328, ChunkKind::Gap(0)), &mut out);
            g.push(chunk(1, 13328, ChunkKind::Data(messages[0][cut..].to_vec())), &mut out);
            for m in &messages[1..] {
                g.push(chunk(1, 13328, ChunkKind::Data(m.clone())), &mut out);
            }
            // A wrong start can wait on a "message" of up to 64 KB before it's found out
            // (and then on another): at worst about half of these go by first.
            assert!(g.connected(), "cut at {cut}");
            assert!(out.len() >= 500, "cut at {cut}: {} of 1999 messages", out.len());
        }
    }

    #[test]
    fn a_gap_before_a_connection_is_trusted_does_not_write_it_off() {
        let mut g = GameStreams::default();
        let mut out = Vec::new();
        g.push(chunk(1, 13328, ChunkKind::Open), &mut out);
        for i in 0..10u8 {
            g.push(chunk(1, 13328, ChunkKind::Data(frame(0x3600, &[i]))), &mut out);
        }
        // The capture drops a packet; the next one begins in the middle of a message.
        g.push(chunk(1, 13328, ChunkKind::Gap(700)), &mut out);
        g.push(chunk(1, 13328, ChunkKind::Data(vec![0x00, 0x01, 0x02])), &mut out);
        for i in 0..CONFIRM_AFTER_SLIP as u32 {
            g.push(chunk(1, 13328, ChunkKind::Data(frame(0x3600, &i.to_le_bytes()))), &mut out);
        }
        assert!(g.connected());
        assert_eq!(out.len(), CONFIRM_AFTER_SLIP, "what it held before the slip is gone");
    }

    #[test]
    fn https_port_is_ignored() {
        let mut g = GameStreams::default();
        let mut out = Vec::new();
        for _ in 0..CONFIRM_AFTER {
            g.push(chunk(3, 443, ChunkKind::Data(frame(0x3600, b"ok"))), &mut out);
        }
        assert!(out.is_empty());
    }
}
