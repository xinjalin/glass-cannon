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

/// The length field counts 4 more bytes than follow it.
const LENGTH_BIAS: u64 = 4;
/// Opcode is 2 bytes, so a valid length is at least bias + 2.
const MIN_LENGTH: u64 = LENGTH_BIAS + 2;
const MAX_MESSAGE: u64 = 1 << 20;
const MAX_BUNDLE: usize = 16 << 20;
const MAX_BUNDLE_DEPTH: u32 = 4;

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
    let mut pos = 0;
    let Some(len) = read_varint(buf, &mut pos) else {
        // A varint longer than 5 bytes can't be a sane length.
        return if buf.len() >= 5 { Frame::Invalid } else { Frame::Incomplete };
    };
    if !(MIN_LENGTH..=MAX_MESSAGE).contains(&len) {
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
        let result = loop {
            match frame_at(&self.buf[start..]) {
                Frame::Complete { header, total } => {
                    // A broken bundle loses only that bundle; framing is still intact.
                    if decode_body(&self.buf[start + header..start + total], out, 0).is_err() {
                        self.bad_bundles += 1;
                    }
                    start += total;
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
                Ok(())
            }
            Err(at) => {
                let pending = self.buf.len() - at;
                self.buf.clear();
                self.resyncing = true;
                bail!("lost message framing ({pending} bytes dropped)")
            }
        }
    }

    /// Forgets partial data, e.g. after a gap in the capture.
    pub fn reset(&mut self) {
        self.buf.clear();
        self.resyncing = true;
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
    fn garbage_reports_lost_framing_then_recovers() {
        let mut f = Framer::default();
        let mut out = Vec::new();
        assert!(f.push(&[0x01, 0x02, 0x03], &mut out).is_err());
        f.push(&frame(0x3600, b"next"), &mut out).unwrap();
        assert_eq!(out.len(), 1);
    }
}
