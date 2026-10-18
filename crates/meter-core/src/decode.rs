//! Decoding captured frames down to TCP segments and UDP datagrams.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

pub mod linktype {
    pub const NULL: i32 = 0;
    pub const ETHERNET: i32 = 1;
    pub const DLT_RAW: i32 = 12;
    pub const DLT_RAW_ALT: i32 = 14;
    pub const RAW: i32 = 101;
    pub const IPV4: i32 = 228;
    pub const IPV6: i32 = 229;
}

pub mod tcp_flags {
    pub const FIN: u8 = 0x01;
    pub const SYN: u8 = 0x02;
    pub const RST: u8 = 0x04;
    pub const ACK: u8 = 0x10;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transport {
    Tcp { seq: u32, ack: u32, flags: u8 },
    Udp,
}

#[derive(Debug, Clone, Copy)]
pub struct Segment<'a> {
    pub src: SocketAddr,
    pub dst: SocketAddr,
    pub transport: Transport,
    pub payload: &'a [u8],
}

/// Strips the link-layer header and returns the IP packet, if this frame carries one.
pub fn link_payload(linktype: i32, frame: &[u8]) -> Option<&[u8]> {
    match linktype {
        linktype::ETHERNET => {
            let mut off = 12;
            let mut ethertype = be16(frame, off)?;
            // Skip 802.1Q / 802.1ad VLAN tags.
            while ethertype == 0x8100 || ethertype == 0x88a8 {
                off += 4;
                ethertype = be16(frame, off)?;
            }
            match ethertype {
                0x0800 | 0x86dd => frame.get(off + 2..),
                _ => None,
            }
        }
        // Loopback adapter: 4-byte address family in host byte order.
        linktype::NULL => {
            let family = u32::from_le_bytes(frame.get(0..4)?.try_into().ok()?);
            match family {
                2 | 24 | 28 | 30 => frame.get(4..),
                _ => None,
            }
        }
        linktype::DLT_RAW | linktype::DLT_RAW_ALT | linktype::RAW | linktype::IPV4 | linktype::IPV6 => {
            Some(frame)
        }
        _ => None,
    }
}

/// Parses an IPv4/IPv6 packet carrying TCP or UDP. Fragments are skipped.
pub fn parse_ip(ip: &[u8]) -> Option<Segment<'_>> {
    let (src, dst, proto, l4) = match ip.first()? >> 4 {
        4 => parse_v4(ip)?,
        6 => parse_v6(ip)?,
        _ => return None,
    };
    match proto {
        6 => {
            if l4.len() < 20 {
                return None;
            }
            let data_off = ((l4[12] >> 4) as usize) * 4;
            if data_off < 20 || data_off > l4.len() {
                return None;
            }
            Some(Segment {
                src: SocketAddr::new(src, be16(l4, 0)?),
                dst: SocketAddr::new(dst, be16(l4, 2)?),
                transport: Transport::Tcp {
                    seq: be32(l4, 4)?,
                    ack: be32(l4, 8)?,
                    flags: l4[13],
                },
                payload: &l4[data_off..],
            })
        }
        17 => {
            if l4.len() < 8 {
                return None;
            }
            let ulen = be16(l4, 4)? as usize;
            let end = if (8..=l4.len()).contains(&ulen) { ulen } else { l4.len() };
            Some(Segment {
                src: SocketAddr::new(src, be16(l4, 0)?),
                dst: SocketAddr::new(dst, be16(l4, 2)?),
                transport: Transport::Udp,
                payload: &l4[8..end],
            })
        }
        _ => None,
    }
}

fn parse_v4(ip: &[u8]) -> Option<(IpAddr, IpAddr, u8, &[u8])> {
    if ip.len() < 20 {
        return None;
    }
    let ihl = ((ip[0] & 0x0f) as usize) * 4;
    if ihl < 20 || ihl > ip.len() {
        return None;
    }
    let total = be16(ip, 2)? as usize;
    // Outbound packets captured before large-send offload can carry a total length of 0.
    let end = if total == 0 || total > ip.len() { ip.len() } else { total };
    if end < ihl {
        return None;
    }
    // More-fragments flag or a non-zero fragment offset.
    if be16(ip, 6)? & 0x3fff != 0 {
        return None;
    }
    let src = Ipv4Addr::new(ip[12], ip[13], ip[14], ip[15]);
    let dst = Ipv4Addr::new(ip[16], ip[17], ip[18], ip[19]);
    Some((src.into(), dst.into(), ip[9], &ip[ihl..end]))
}

fn parse_v6(ip: &[u8]) -> Option<(IpAddr, IpAddr, u8, &[u8])> {
    if ip.len() < 40 {
        return None;
    }
    let payload_len = be16(ip, 4)? as usize;
    let end = if payload_len == 0 || 40 + payload_len > ip.len() {
        ip.len()
    } else {
        40 + payload_len
    };
    let src: [u8; 16] = ip[8..24].try_into().ok()?;
    let dst: [u8; 16] = ip[24..40].try_into().ok()?;
    let mut next = ip[6];
    let mut off = 40;
    loop {
        match next {
            // Hop-by-hop, routing and destination-options headers.
            0 | 43 | 60 => {
                let h = ip.get(off..off + 2)?;
                next = h[0];
                off += (h[1] as usize + 1) * 8;
                if off > end {
                    return None;
                }
            }
            44 => return None, // fragment
            _ => break,
        }
    }
    Some((Ipv6Addr::from(src).into(), Ipv6Addr::from(dst).into(), next, &ip[off..end]))
}

fn be16(b: &[u8], off: usize) -> Option<u16> {
    Some(u16::from_be_bytes([*b.get(off)?, *b.get(off + 1)?]))
}

fn be32(b: &[u8], off: usize) -> Option<u32> {
    Some(u32::from_be_bytes(b.get(off..off + 4)?.try_into().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{ipv4_tcp, ipv4_udp};

    #[test]
    fn decodes_tcp_over_ethernet() {
        let ip = ipv4_tcp([10, 0, 0, 2], 50000, [1, 2, 3, 4], 7777, 1000, tcp_flags::ACK, b"hello");
        let mut frame = vec![0u8; 12];
        frame.extend_from_slice(&[0x08, 0x00]);
        frame.extend_from_slice(&ip);

        let ip = link_payload(linktype::ETHERNET, &frame).unwrap();
        let seg = parse_ip(ip).unwrap();
        assert_eq!(seg.src, "10.0.0.2:50000".parse().unwrap());
        assert_eq!(seg.dst, "1.2.3.4:7777".parse().unwrap());
        assert_eq!(
            seg.transport,
            Transport::Tcp { seq: 1000, ack: 0, flags: tcp_flags::ACK }
        );
        assert_eq!(seg.payload, b"hello");
    }

    #[test]
    fn decodes_udp_and_trims_ethernet_padding() {
        let mut ip = ipv4_udp([10, 0, 0, 2], 6000, [1, 2, 3, 4], 6001, b"hi");
        ip.extend_from_slice(&[0u8; 10]); // trailing padding past the IP total length
        let seg = parse_ip(&ip).unwrap();
        assert_eq!(seg.transport, Transport::Udp);
        assert_eq!(seg.payload, b"hi");
    }

    #[test]
    fn zero_total_length_uses_captured_length() {
        let mut ip = ipv4_tcp([10, 0, 0, 2], 1, [1, 2, 3, 4], 2, 5, 0, b"lso");
        ip[2] = 0;
        ip[3] = 0;
        assert_eq!(parse_ip(&ip).unwrap().payload, b"lso");
    }

    #[test]
    fn skips_fragments() {
        let mut ip = ipv4_tcp([10, 0, 0, 2], 1, [1, 2, 3, 4], 2, 5, 0, b"x");
        ip[6] = 0x20; // more fragments
        assert!(parse_ip(&ip).is_none());
    }

    #[test]
    fn loopback_family_header() {
        let ip = ipv4_tcp([127, 0, 0, 1], 1, [127, 0, 0, 1], 2, 5, 0, b"x");
        let mut frame = 2u32.to_le_bytes().to_vec();
        frame.extend_from_slice(&ip);
        assert_eq!(link_payload(linktype::NULL, &frame).unwrap(), &ip[..]);
    }
}
