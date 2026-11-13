//! Splitting the game's server-to-client stream into messages.
//!
//! Worked out from our own captures (see `docs/protocol.md`):
//!
//! ```text
//! message := varint L, then (L - 4) more bytes      // so the whole message is n + L - 4 bytes,
//!                                                    // n = size of the varint itself
//! body    := u16 opcode (little-endian), payload
//! bundle  := opcode 0xffff, u32 decompressed size, LZ4 block of more messages
//! ```

use anyhow::{bail, Context, Result};

pub const OPCODE_BUNDLE: u16 = 0xffff;
pub const OPCODE_DAMAGE: u16 = 0x3804;
/// Damage over time (bleeds, burns, poisons…).
pub const OPCODE_DOT: u16 = 0x3805;

/// The length field counts 4 more bytes than follow it.
const LENGTH_BIAS: u64 = 4;
/// Opcode is 2 bytes, so a valid length is at least bias + 2.
const MIN_LENGTH: u64 = LENGTH_BIAS + 2;
const MAX_MESSAGE: u64 = 1 << 20;
const MAX_BUNDLE: usize = 16 << 20;
const MAX_BUNDLE_DEPTH: u32 = 4;
/// After framing was lost (or a stream was joined part-way), the framer is wary until this
/// many messages have framed cleanly: bytes that only look like messages don't last that
/// long.
const WARY_FOR: u32 = 64;
/// While wary, a "message" longer than this is taken for a wrong start (a length read out
/// of the middle of a message can say anything up to [`MAX_MESSAGE`], and waiting for a
/// megabyte that isn't coming stalls the stream for as long as it takes to arrive). Of the
/// 1.5 million messages in our recordings the longest is 20,369 bytes.
const WARY_MAX_MESSAGE: u64 = 64 << 10;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub opcode: u16,
    /// Bytes after the opcode.
    pub payload: Vec<u8>,
}

/// Reads an unsigned LEB128 varint at `*pos`, advancing it. `None` if the buffer ends
/// first or the value is too long to be sane.
pub fn read_varint(buf: &[u8], pos: &mut usize) -> Option<u64> {
    let mut value = 0u64;
    for i in 0..10 {
        let b = *buf.get(*pos + i)?;
        value |= ((b & 0x7f) as u64) << (7 * i);
        if b < 0x80 {
            *pos += i + 1;
            return Some(value);
        }
    }
    None
}

enum Frame {
    /// A whole message: header size and total size.
    Complete { header: usize, total: usize },
    Incomplete,
    Invalid,
}

fn frame_at(buf: &[u8]) -> Frame {
    frame_up_to(buf, MAX_MESSAGE)
}

/// As [`frame_at`], with the longest message allowed.
fn frame_up_to(buf: &[u8], max: u64) -> Frame {
    let mut pos = 0;
    let Some(len) = read_varint(buf, &mut pos) else {
        // A varint longer than 5 bytes can't be a sane length.
        return if buf.len() >= 5 { Frame::Invalid } else { Frame::Incomplete };
    };
    if !(MIN_LENGTH..=max).contains(&len) {
        return Frame::Invalid;
    }
    let total = pos + (len - LENGTH_BIAS) as usize;
    if total > buf.len() {
        Frame::Incomplete
    } else {
        Frame::Complete { header: pos, total }
    }
}

/// Splits a complete buffer (such as an unpacked bundle) into messages, unpacking any
/// nested bundles.
pub fn split_all(buf: &[u8], out: &mut Vec<Message>) -> Result<()> {
    split_all_at(buf, out, 0)
}

fn split_all_at(buf: &[u8], out: &mut Vec<Message>, depth: u32) -> Result<()> {
    let mut start = 0;
    while start < buf.len() {
        match frame_at(&buf[start..]) {
            Frame::Complete { header, total } => {
                decode_body(&buf[start + header..start + total], out, depth)?;
                start += total;
            }
            Frame::Incomplete => bail!("message cut off at byte {start} of {}", buf.len()),
            Frame::Invalid => bail!("invalid message length at byte {start}"),
        }
    }
    Ok(())
}

fn decode_body(body: &[u8], out: &mut Vec<Message>, depth: u32) -> Result<()> {
    let opcode = u16::from_le_bytes([body[0], body[1]]);
    if opcode != OPCODE_BUNDLE {
        out.push(Message {
            opcode,
            payload: body[2..].to_vec(),
        });
        return Ok(());
    }
    if depth >= MAX_BUNDLE_DEPTH {
        bail!("bundles nested too deeply");
    }
    let Some(size) = body.get(2..6) else {
        bail!("bundle too short");
    };
    let size = u32::from_le_bytes(size.try_into().unwrap()) as usize;
    if size > MAX_BUNDLE {
        bail!("bundle claims {size} bytes");
    }
    let raw = lz4_flex::block::decompress(&body[6..], size).context("bundle is not valid LZ4")?;
    if raw.len() != size {
        bail!("bundle unpacked to {} bytes, header said {size}", raw.len());
    }
    split_all_at(&raw, out, depth + 1)
}

/// Incremental splitter for a live or recorded stream that arrives in chunks.
#[derive(Default)]
pub struct Framer {
    buf: Vec<u8>,
    bad_bundles: u64,
    /// Set after a gap or lost framing, until a message decodes cleanly again.
    resyncing: bool,
    /// Messages still to frame cleanly before the framer trusts the stream again (0: it
    /// does). See [`WARY_FOR`].
    wary: u32,
}

impl Framer {
    /// Adds bytes and appends every message they complete to `out`.
    ///
    /// Returns an error if the stream stops looking like messages (usually right after
    /// a capture gap). The buffer is then cleared, so the next chunk starts fresh.
    /// TCP chunks usually begin on a message boundary, so this resynchronises quickly.
    pub fn push(&mut self, data: &[u8], out: &mut Vec<Message>) -> Result<()> {
        if self.resyncing && !self.buf.is_empty() {
            // The buffered bytes may be the tail of a message lost in a gap that only
            // looks like a length. Keep them only if this chunk completes that message;
            // otherwise restart at this chunk's boundary.
            self.buf.extend_from_slice(data);
            if !matches!(frame_at(&self.buf), Frame::Complete { .. }) {
                self.buf.clear();
                self.buf.extend_from_slice(data);
            }
        } else {
            self.buf.extend_from_slice(data);
        }
        let produced = out.len();
        let mut start = 0;
        let mut frames = 0u32;
        let max = if self.wary > 0 { WARY_MAX_MESSAGE } else { MAX_MESSAGE };
        let result = loop {
            match frame_up_to(&self.buf[start..], max) {
                Frame::Complete { header, total } => {
                    // A broken bundle loses only that bundle; framing is still intact.
                    if decode_body(&self.buf[start + header..start + total], out, 0).is_err() {
                        self.bad_bundles += 1;
                    }
                    start += total;
                    frames += 1;
                }
                Frame::Incomplete => break Ok(()),
                Frame::Invalid => break Err(start),
            }
        };
        match result {
            Ok(()) => {
                self.buf.drain(..start);
                if out.len() > produced {
                    self.resyncing = false;
                }
                // A bundle's messages each count: one clean bundle is proof enough.
                let messages = (out.len() - produced).max(frames as usize);
                self.wary = self.wary.saturating_sub(messages.min(u32::MAX as usize) as u32);
                Ok(())
            }
            Err(at) => {
                let pending = self.buf.len() - at;
                self.lose_step();
                bail!("lost message framing ({pending} bytes dropped)")
            }
        }
    }

    /// Forgets partial data, e.g. after a gap in the capture, or before the first bytes
    /// of a stream joined part-way. The framer then looks for a message boundary, and
    /// stays wary for the next [`WARY_FOR`] messages.
    pub fn reset(&mut self) {
        self.lose_step();
    }

    /// A connection starts here (its SYN): forgets partial data from the one before, and
    /// trusts the stream from its first byte. A login's first messages can be long, and
    /// nothing before them could have put the framer out of step.
    pub fn start(&mut self) {
        self.buf.clear();
        self.resyncing = true;
        self.wary = 0;
    }

    fn lose_step(&mut self) {
        self.buf.clear();
        self.resyncing = true;
        self.wary = WARY_FOR;
    }

    pub fn bad_bundles(&self) -> u64 {
        self.bad_bundles
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn encode_varint(mut v: u64) -> Vec<u8> {
        let mut out = Vec::new();
        loop {
            let b = (v & 0x7f) as u8;
            v >>= 7;
            if v == 0 {
                out.push(b);
                return out;
            }
            out.push(b | 0x80);
        }
    }

    /// Encodes one message the way the server does.
    pub fn frame(opcode: u16, payload: &[u8]) -> Vec<u8> {
        let mut body = opcode.to_le_bytes().to_vec();
        body.extend_from_slice(payload);
        let mut out = encode_varint(body.len() as u64 + LENGTH_BIAS);
        out.extend_from_slice(&body);
        out
    }

    fn bundle(inner: &[u8]) -> Vec<u8> {
        let mut payload = (inner.len() as u32).to_le_bytes().to_vec();
        payload.extend_from_slice(&lz4_flex::block::compress(inner));
        frame(OPCODE_BUNDLE, &payload)
    }

    #[test]
    fn varint_round_trip() {
        for v in [0, 1, 127, 128, 3199, 305_252, u32::MAX as u64] {
            let enc = encode_varint(v);
            let mut pos = 0;
            assert_eq!(read_varint(&enc, &mut pos), Some(v));
            assert_eq!(pos, enc.len());
        }
        let mut pos = 0;
        assert_eq!(read_varint(&[0x80, 0x80], &mut pos), None);
    }

    #[test]
    fn real_message_from_capture() {
        // 11-byte message seen in the server stream: length byte 0x0e, opcode 0x3600.
        let raw = [0x0e, 0x00, 0x36, 0xb4, 0x49, 0x0b, 0x4e, 0xa1, 0x01, 0x00, 0x00];
        let mut out = Vec::new();
        split_all(&raw, &mut out).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].opcode, 0x3600);
        assert_eq!(out[0].payload.len(), 8);
    }

    #[test]
    fn framer_handles_messages_split_across_chunks() {
        let mut stream = frame(0x3600, b"first");
        stream.extend(frame(OPCODE_DAMAGE, &[7u8; 300])); // 2-byte length varint
        stream.extend(frame(0x371d, b"third"));

        let mut f = Framer::default();
        let mut out = Vec::new();
        for chunk in stream.chunks(7) {
            f.push(chunk, &mut out).unwrap();
        }
        let ops: Vec<u16> = out.iter().map(|m| m.opcode).collect();
        assert_eq!(ops, vec![0x3600, OPCODE_DAMAGE, 0x371d]);
        assert_eq!(out[1].payload.len(), 300);
    }

    #[test]
    fn bundles_are_unpacked_in_order() {
        let mut inner = frame(0x3802, b"a");
        inner.extend(frame(OPCODE_DAMAGE, b"bb"));
        let mut stream = frame(0x3600, b"before");
        stream.extend(bundle(&inner));
        stream.extend(frame(0x3600, b"after"));

        let mut out = Vec::new();
        split_all(&stream, &mut out).unwrap();
        let ops: Vec<u16> = out.iter().map(|m| m.opcode).collect();
        assert_eq!(ops, vec![0x3600, 0x3802, OPCODE_DAMAGE, 0x3600]);
    }

    #[test]
    fn corrupt_bundle_is_skipped_without_losing_framing() {
        let mut payload = 100u32.to_le_bytes().to_vec();
        payload.extend_from_slice(&[0xff; 10]);
        let mut stream = frame(OPCODE_BUNDLE, &payload);
        stream.extend(frame(0x3600, b"ok"));

        let mut f = Framer::default();
        let mut out = Vec::new();
        f.push(&stream, &mut out).unwrap();
        assert_eq!(f.bad_bundles(), 1);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].payload, b"ok");
    }

    #[test]
    fn a_long_message_is_waited_on_in_a_stream_that_is_in_step() {
        // From the start of a connection: a 20 KB message over 14 packets, as at login.
        let mut stream = frame(0x3600, b"hello");
        stream.extend(frame(0x9a00, &[5u8; 20_000]));
        stream.extend(frame(0x3600, b"after"));
        let mut f = Framer::default();
        let mut out = Vec::new();
        for chunk in stream.chunks(1460) {
            f.push(chunk, &mut out).unwrap();
        }
        assert_eq!(out.iter().map(|m| m.payload.len()).collect::<Vec<_>>(), [5, 20_000, 5]);
    }

    #[test]
    fn a_long_message_at_the_start_of_a_connection_is_waited_on() {
        // A recording's SYN, then a 20 KB login list as the second message.
        let mut stream = frame(0x3600, b"hello");
        stream.extend(frame(0x9a00, &[5u8; 20_000]));
        let mut f = Framer::default();
        let mut out = Vec::new();
        f.start();
        for chunk in stream.chunks(1460) {
            f.push(chunk, &mut out).unwrap();
        }
        assert_eq!(out.iter().map(|m| m.payload.len()).collect::<Vec<_>>(), [5, 20_000]);
    }

    #[test]
    fn a_wrong_start_after_a_resync_is_not_waited_on() {
        let mut f = Framer::default();
        let mut out = Vec::new();
        f.reset();
        // Bytes from the middle of a message that read as one small message, then the
        // start of a 100 KB one that will never come.
        let mut wrong = frame(0x1111, b"xx");
        wrong.extend([0xa0, 0x8d, 0x06, 0x01, 0x02, 0x03]);
        assert!(f.push(&wrong, &mut out).is_err(), "100 KB, just after a resync: not a message");
        // The real stream goes on, read as it is from the next packet.
        for i in 0..8u8 {
            f.push(&frame(0x3600, &[i]), &mut out).unwrap();
        }
        assert_eq!(out.iter().skip(1).map(|m| m.payload[0]).collect::<Vec<_>>(), [0, 1, 2, 3, 4, 5, 6, 7]);
        // Still wary, but in step: the longest real messages are waited on (20 KB, over
        // 14 packets).
        let n = out.len();
        for chunk in frame(0x9a00, &[5u8; 20_000]).chunks(1460) {
            f.push(chunk, &mut out).unwrap();
        }
        assert_eq!((out.len(), out.last().unwrap().payload.len()), (n + 1, 20_000));
        // Trusted again after enough clean messages, the megabyte limit is back.
        for i in 0..WARY_FOR as u8 {
            f.push(&frame(0x3600, &[i]), &mut out).unwrap();
        }
        let n = out.len();
        for chunk in frame(0x9a00, &[5u8; 200_000]).chunks(1460) {
            f.push(chunk, &mut out).unwrap();
        }
        assert_eq!((out.len(), out.last().unwrap().payload.len()), (n + 1, 200_000));
    }

    #[test]
    fn garbage_reports_lost_framing_then_recovers() {
        let mut f = Framer::default();
        let mut out = Vec::new();
        assert!(f.push(&[0x01, 0x02, 0x03], &mut out).is_err());
        f.push(&frame(0x3600, b"next"), &mut out).unwrap();
        assert_eq!(out.len(), 1);
    }
}
