//! Classic libpcap file format, readable by Wireshark.

use std::io::{self, ErrorKind, Read, Write};

use anyhow::{bail, Result};

/// Packets start at the IP header (no link layer).
pub const LINKTYPE_RAW: u32 = 101;
const SNAPLEN: u32 = 262_144;
const MAX_RECORD: usize = 64 << 20;

pub struct PcapWriter<W: Write> {
    w: W,
}

impl<W: Write> PcapWriter<W> {
    pub fn new(mut w: W, linktype: u32) -> io::Result<Self> {
        w.write_all(&0xa1b2_c3d4u32.to_le_bytes())?;
        w.write_all(&2u16.to_le_bytes())?;
        w.write_all(&4u16.to_le_bytes())?;
        w.write_all(&0i32.to_le_bytes())?;
        w.write_all(&0u32.to_le_bytes())?;
        w.write_all(&SNAPLEN.to_le_bytes())?;
        w.write_all(&linktype.to_le_bytes())?;
        Ok(Self { w })
    }

    pub fn write(&mut self, ts_us: u64, data: &[u8]) -> io::Result<()> {
        self.w.write_all(&((ts_us / 1_000_000) as u32).to_le_bytes())?;
        self.w.write_all(&((ts_us % 1_000_000) as u32).to_le_bytes())?;
        self.w.write_all(&(data.len() as u32).to_le_bytes())?;
        self.w.write_all(&(data.len() as u32).to_le_bytes())?;
        self.w.write_all(data)
    }

    pub fn flush(&mut self) -> io::Result<()> {
        self.w.flush()
    }
}

pub struct PcapReader<R: Read> {
    r: R,
    big_endian: bool,
    nanos: bool,
    linktype: u32,
}

impl<R: Read> PcapReader<R> {
    pub fn new(mut r: R) -> Result<Self> {
        let mut h = [0u8; 24];
        r.read_exact(&mut h)?;
        let (big_endian, nanos) = match u32::from_le_bytes(h[0..4].try_into().unwrap()) {
            0xa1b2_c3d4 => (false, false),
            0xd4c3_b2a1 => (true, false),
            0xa1b2_3c4d => (false, true),
            0x4d3c_b2a1 => (true, true),
            _ => bail!("not a classic pcap file (pcapng is not supported; re-save as .pcap in Wireshark)"),
        };
        let mut reader = Self {
            r,
            big_endian,
            nanos,
            linktype: 0,
        };
        reader.linktype = reader.u32(&h[20..24]);
        Ok(reader)
    }

    pub fn linktype(&self) -> u32 {
        self.linktype
    }

    /// Next `(timestamp in µs, packet bytes)`, or `None` at end of file. A record cut
    /// short (a capture that was killed mid-write) also ends the file.
    pub fn next_packet(&mut self) -> Result<Option<(u64, Vec<u8>)>> {
        let mut h = [0u8; 16];
        match self.r.read_exact(&mut h) {
            Ok(()) => {}
            Err(e) if e.kind() == ErrorKind::UnexpectedEof => return Ok(None),
            Err(e) => return Err(e.into()),
        }
        let sec = self.u32(&h[0..4]) as u64;
        let frac = self.u32(&h[4..8]) as u64;
        let len = self.u32(&h[8..12]) as usize;
        if len > MAX_RECORD {
            bail!("corrupt pcap record length {len}");
        }
        let mut data = vec![0u8; len];
        match self.r.read_exact(&mut data) {
            Ok(()) => {}
            Err(e) if e.kind() == ErrorKind::UnexpectedEof => return Ok(None),
            Err(e) => return Err(e.into()),
        }
        let us = if self.nanos { frac / 1000 } else { frac };
        Ok(Some((sec * 1_000_000 + us, data)))
    }

    fn u32(&self, b: &[u8]) -> u32 {
        let a: [u8; 4] = b.try_into().unwrap();
        if self.big_endian {
            u32::from_be_bytes(a)
        } else {
            u32::from_le_bytes(a)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let mut buf = Vec::new();
        let mut w = PcapWriter::new(&mut buf, LINKTYPE_RAW).unwrap();
        w.write(1_700_000_000_123_456, b"first").unwrap();
        w.write(1_700_000_001_000_000, b"second!").unwrap();

        let mut r = PcapReader::new(&buf[..]).unwrap();
        assert_eq!(r.linktype(), LINKTYPE_RAW);
        assert_eq!(
            r.next_packet().unwrap(),
            Some((1_700_000_000_123_456, b"first".to_vec()))
        );
        assert_eq!(
            r.next_packet().unwrap(),
            Some((1_700_000_001_000_000, b"second!".to_vec()))
        );
        assert_eq!(r.next_packet().unwrap(), None);
    }

    #[test]
    fn truncated_record_ends_file() {
        let mut buf = Vec::new();
        let mut w = PcapWriter::new(&mut buf, LINKTYPE_RAW).unwrap();
        w.write(1, b"complete").unwrap();
        w.write(2, b"cut off").unwrap();
        buf.truncate(buf.len() - 3);

        let mut r = PcapReader::new(&buf[..]).unwrap();
        assert!(r.next_packet().unwrap().is_some());
        assert_eq!(r.next_packet().unwrap(), None);
    }
}
