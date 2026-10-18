//! Turns live reassembled TCP data into game messages.
//!
//! The game process also keeps a few TLS (HTTPS) connections open. Their encrypted
//! bytes could occasionally look like a message, so a connection's messages are held
//! back until it has framed cleanly for a while. TLS breaks framing within a few
//! records; the game connection never does.

use std::collections::HashMap;

use crate::pipeline::{ChunkKind, StreamChunk};
use crate::protocol::{Framer, Message};
use crate::recorder::Side;
use crate::session::TimedMessage;

/// Messages that must frame cleanly before a connection is trusted.
pub const CONFIRM_AFTER: usize = 50;
/// Web traffic; never the game.
const HTTPS_PORT: u16 = 443;

#[derive(Default)]
struct FlowState {
    framer: Framer,
    confirmed: bool,
    rejected: bool,
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
                    f.rejected = true;
                    f.pending.clear();
                    return;
                }
                f.pending.extend(timed);
                if f.pending.len() >= CONFIRM_AFTER {
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

    #[test]
    fn tls_like_stream_is_rejected() {
        let mut g = GameStreams::default();
        let mut out = Vec::new();
        // A TLS record header: content type, version, length, then encrypted bytes.
        let mut tls = vec![0x17, 0x03, 0x03, 0x01, 0x00];
        tls.extend((0..256u32).map(|i| (i * 167 % 251) as u8));
        g.push(chunk(2, 8443, ChunkKind::Data(tls)), &mut out);
        for _ in 0..CONFIRM_AFTER {
            g.push(chunk(2, 8443, ChunkKind::Data(frame(0x3600, b"ok"))), &mut out);
        }
        assert!(out.is_empty());
        assert!(!g.connected());
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
