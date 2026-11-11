//! Routes decoded packets to per-connection streams and records them.
//!
//! Packets from connections not (yet) known to belong to the game are held briefly
//! ("limbo"). The connection table is polled, so a brand-new connection is usually
//! identified a fraction of a second after its first packets. Holding them lets us keep
//! the handshake and first messages, which tend to be the most informative.
//!
//! Only packets nobody is known to own wait there. The same tables say which sockets the
//! other programs have, and their packets (a download, a stream, a browser) are dropped as
//! they come: holding them cost a copy of everything the PC sent and received, and filled
//! limbo in seconds.

use std::collections::{HashMap, HashSet, VecDeque};
use std::net::{IpAddr, SocketAddr};
use std::path::Path;

use anyhow::Result;
use serde_json::json;

use crate::conns::{TcpConn, UdpEndpoint};
use crate::decode::{self, tcp_flags, Segment, Transport};
use crate::reassembly::{HalfStream, StreamEvent};
use crate::recorder::{FlowFiles, Recorder, Side};

const LIMBO_MAX_AGE_US: u64 = 3_000_000;
const LIMBO_MAX_PACKETS_PER_FLOW: usize = 256;
const LIMBO_MAX_BYTES: usize = 32 << 20;

/// Sockets that belong to the game, from the OS connection tables.
#[derive(Debug, Default, Clone)]
pub struct LiveMatcher {
    /// (local port, remote address) of each game TCP connection.
    tcp: HashSet<(u16, SocketAddr)>,
    /// Local ports of the game's UDP sockets.
    udp: HashSet<u16>,
    /// The same for every other program's sockets, as the tables last had them (never
    /// kept from an earlier look: a port let go can be the game's next).
    other_tcp: HashSet<(u16, SocketAddr)>,
    other_udp: HashSet<u16>,
}

impl LiveMatcher {
    pub fn from_tables(pids: &HashSet<u32>, tcp: &[TcpConn], udp: &[UdpEndpoint]) -> Self {
        Self {
            tcp: tcp
                .iter()
                .filter(|c| pids.contains(&c.pid) && c.state != TcpConn::LISTEN && c.remote.port() != 0)
                .map(|c| (c.local.port(), c.remote))
                .collect(),
            udp: udp
                .iter()
                .filter(|u| pids.contains(&u.pid))
                .map(|u| u.local.port())
                .collect(),
            other_tcp: tcp
                .iter()
                .filter(|c| !pids.contains(&c.pid) && c.state != TcpConn::LISTEN && c.remote.port() != 0)
                .map(|c| (c.local.port(), c.remote))
                .collect(),
            other_udp: udp
                .iter()
                .filter(|u| !pids.contains(&u.pid))
                .map(|u| u.local.port())
                .collect(),
        }
    }

    /// Keeps sockets seen earlier, so packets that arrive after a connection leaves the
    /// OS table (its final FIN/RST) are still recorded.
    pub fn merge(&mut self, other: &LiveMatcher) {
        self.tcp.extend(other.tcp.iter().copied());
        self.udp.extend(other.udp.iter().copied());
        self.other_tcp.clone_from(&other.other_tcp);
        self.other_udp.clone_from(&other.other_udp);
    }

    /// The packet is on a socket another program is known to own (and none of the game's).
    fn is_others(&self, s: &Segment) -> bool {
        match s.transport {
            Transport::Tcp { .. } => self.other_tcp.contains(&(s.src.port(), s.dst)) || self.other_tcp.contains(&(s.dst.port(), s.src)),
            Transport::Udp => self.other_udp.contains(&s.src.port()) || self.other_udp.contains(&s.dst.port()),
        }
    }

    pub fn tcp_count(&self) -> usize {
        self.tcp.len()
    }

    pub fn udp_count(&self) -> usize {
        self.udp.len()
    }
}

pub enum Matcher {
    /// Only sockets owned by the game process.
    Live(LiveMatcher),
    /// Every packet (used when replaying a capture). The side whose IP is in
    /// `local_ips` is us; without that, the higher (ephemeral) port is assumed to be us.
    Any { local_ips: HashSet<IpAddr> },
}

impl Matcher {
    fn classify(&self, s: &Segment) -> Option<Side> {
        match self {
            Matcher::Live(m) => match s.transport {
                Transport::Tcp { .. } => {
                    if m.tcp.contains(&(s.src.port(), s.dst)) {
                        Some(Side::Out)
                    } else if m.tcp.contains(&(s.dst.port(), s.src)) {
                        Some(Side::In)
                    } else {
                        None
                    }
                }
                Transport::Udp => {
                    if m.udp.contains(&s.src.port()) {
                        Some(Side::Out)
                    } else if m.udp.contains(&s.dst.port()) {
                        Some(Side::In)
                    } else {
                        None
                    }
                }
            },
            Matcher::Any { local_ips } => Some(if local_ips.contains(&s.src.ip()) {
                Side::Out
            } else if local_ips.contains(&s.dst.ip()) || s.src.port() < s.dst.port() {
                Side::In
            } else {
                Side::Out
            }),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum Proto {
    Tcp,
    Udp,
}

impl Proto {
    fn as_str(self) -> &'static str {
        match self {
            Proto::Tcp => "tcp",
            Proto::Udp => "udp",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct FlowKey {
    proto: Proto,
    local: SocketAddr,
    remote: SocketAddr,
}

/// Direction-independent key for packets we can't attribute yet.
type PairKey = (Proto, SocketAddr, SocketAddr);

struct Flow {
    id: u32,
    proto: Proto,
    remote: SocketAddr,
    /// None when not recording to disk.
    files: Option<FlowFiles>,
    out: HalfStream,
    inn: HalfStream,
    packets: u64,
}

#[derive(Debug, Default, Clone)]
pub struct PipelineStats {
    pub packets_seen: u64,
    pub packets_matched: u64,
    /// Other programs' packets, dropped as they came.
    pub packets_others: u64,
    pub undecodable: u64,
    pub bytes_in: u64,
    pub bytes_out: u64,
    pub gap_bytes: u64,
    pub flows_opened: u32,
    pub flows_open: usize,
}

/// Reassembled TCP data handed to the live engine, in arrival order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamChunk {
    pub flow: u32,
    pub remote: SocketAddr,
    pub side: Side,
    pub ts_us: u64,
    pub kind: ChunkKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChunkKind {
    /// A new TCP connection (or a capture that started mid-connection).
    Open,
    Data(Vec<u8>),
    /// Bytes the capture missed. `Gap(0)` before a connection's first bytes: it was
    /// joined part-way through, so they may begin inside a message.
    Gap(u64),
    Close,
}

pub struct Pipeline {
    matcher: Matcher,
    rec: Option<Recorder>,
    /// Collected for the live engine when enabled.
    streams: Option<Vec<StreamChunk>>,
    flows: HashMap<FlowKey, Flow>,
    limbo: HashMap<PairKey, VecDeque<(u64, Vec<u8>)>>,
    limbo_bytes: usize,
    next_id: u32,
    stats: PipelineStats,
}

impl Pipeline {
    /// `rec` is where to save the session; `None` keeps everything in memory only.
    pub fn new(matcher: Matcher, rec: Option<Recorder>) -> Self {
        Self {
            matcher,
            rec,
            streams: None,
            flows: HashMap::new(),
            limbo: HashMap::new(),
            limbo_bytes: 0,
            next_id: 0,
            stats: PipelineStats::default(),
        }
    }

    pub fn session_dir(&self) -> Option<&Path> {
        self.rec.as_ref().map(Recorder::dir)
    }

    /// Starts collecting reassembled TCP data for [`Pipeline::take_streams`].
    pub fn collect_streams(&mut self) {
        self.streams.get_or_insert_with(Vec::new);
    }

    /// Everything collected since the last call.
    pub fn take_streams(&mut self) -> Vec<StreamChunk> {
        self.streams.as_mut().map(std::mem::take).unwrap_or_default()
    }

    pub fn stats(&self) -> PipelineStats {
        let mut s = self.stats.clone();
        s.flows_open = self.flows.len();
        s
    }

    /// Feeds one captured IP packet.
    pub fn ingest(&mut self, ts_us: u64, ip: &[u8]) -> Result<()> {
        self.stats.packets_seen += 1;
        self.route(ts_us, ip)
    }

    /// Swaps in a fresh view of the game's sockets and releases any held packets that
    /// now match. Also skips stale holes and flushes files; call it every ~500 ms.
    pub fn set_matcher(&mut self, matcher: Matcher, now_us: u64) -> Result<()> {
        self.matcher = matcher;
        // Release in first-seen order so flow numbers follow the order connections opened.
        let mut parked: Vec<_> = std::mem::take(&mut self.limbo).into_iter().collect();
        parked.sort_by_key(|(_, q)| q.front().map(|(ts, _)| *ts));
        self.limbo_bytes = 0;
        for (key, queue) in parked {
            let matches = queue
                .front()
                .and_then(|(_, p)| decode::parse_ip(p))
                .and_then(|s| self.matcher.classify(&s))
                .is_some();
            if matches {
                for (ts, p) in queue {
                    self.route(ts, &p)?;
                }
            } else {
                let young: VecDeque<_> = queue
                    .into_iter()
                    .filter(|(ts, _)| now_us.saturating_sub(*ts) <= LIMBO_MAX_AGE_US)
                    .collect();
                if !young.is_empty() {
                    self.limbo_bytes += young.iter().map(|(_, p)| p.len()).sum::<usize>();
                    self.limbo.insert(key, young);
                }
            }
        }
        self.tick(now_us)
    }

    pub fn tick(&mut self, now_us: u64) -> Result<()> {
        for flow in self.flows.values_mut() {
            for side in [Side::Out, Side::In] {
                let half = match side {
                    Side::Out => &mut flow.out,
                    Side::In => &mut flow.inn,
                };
                let mut events = Vec::new();
                half.flush_stale(now_us, &mut events);
                let out = Outputs {
                    files: flow.files.as_mut(),
                    streams: self.streams.as_mut(),
                    flow: flow.id,
                    remote: flow.remote,
                };
                write_events(out, &mut self.stats, now_us, side, events)?;
            }
            if let Some(f) = flow.files.as_mut() {
                f.flush()?;
            }
        }
        match self.rec.as_mut() {
            Some(rec) => rec.flush(),
            None => Ok(()),
        }
    }

    /// Closes every flow and returns the final counters.
    pub fn finish(mut self, now_us: u64) -> Result<PipelineStats> {
        let mut flows: Vec<Flow> = self.flows.drain().map(|(_, f)| f).collect();
        flows.sort_by_key(|f| f.id);
        for flow in flows {
            self.close_flow(now_us, flow)?;
        }
        let stats = self.stats();
        let Some(rec) = self.rec.as_mut() else {
            return Ok(stats);
        };
        rec.event(&json!({
            "t": now_us,
            "event": "session_end",
            "packets_seen": stats.packets_seen,
            "packets_matched": stats.packets_matched,
            "bytes_in": stats.bytes_in,
            "bytes_out": stats.bytes_out,
            "gap_bytes": stats.gap_bytes,
            "flows": stats.flows_opened,
        }))?;
        rec.flush()?;
        Ok(stats)
    }

    fn route(&mut self, ts_us: u64, ip: &[u8]) -> Result<()> {
        let Some(seg) = decode::parse_ip(ip) else {
            self.stats.undecodable += 1;
            return Ok(());
        };
        match self.matcher.classify(&seg) {
            Some(side) => self.accept(ts_us, ip, &seg, side),
            None => {
                if let Matcher::Live(m) = &self.matcher {
                    if m.is_others(&seg) {
                        self.stats.packets_others += 1;
                    } else {
                        self.park(ts_us, ip, &seg);
                    }
                }
                Ok(())
            }
        }
    }

    fn accept(&mut self, ts_us: u64, ip: &[u8], seg: &Segment, side: Side) -> Result<()> {
        if let Some(rec) = self.rec.as_mut() {
            rec.packet(ts_us, ip)?;
        }
        self.stats.packets_matched += 1;

        let (local, remote) = match side {
            Side::Out => (seg.src, seg.dst),
            Side::In => (seg.dst, seg.src),
        };
        let proto = match seg.transport {
            Transport::Tcp { .. } => Proto::Tcp,
            Transport::Udp => Proto::Udp,
        };
        let key = FlowKey { proto, local, remote };

        // A fresh SYN on an address pair that already carried data is a new connection.
        if let Transport::Tcp { flags, .. } = seg.transport {
            let fresh_syn = flags & tcp_flags::SYN != 0 && flags & tcp_flags::ACK == 0;
            let had_data = self
                .flows
                .get(&key)
                .is_some_and(|f| f.out.delivered() > 0 || f.inn.delivered() > 0);
            if fresh_syn && had_data {
                if let Some(old) = self.flows.remove(&key) {
                    self.close_flow(ts_us, old)?;
                }
            }
        }

        if !self.flows.contains_key(&key) {
            self.next_id += 1;
            let id = self.next_id;
            let files = match self.rec.as_mut() {
                Some(rec) => {
                    let files = rec.open_flow(id, proto.as_str(), local, remote)?;
                    rec.event(&json!({
                        "t": ts_us,
                        "event": "flow_open",
                        "flow": id,
                        "proto": proto.as_str(),
                        "local": local.to_string(),
                        "remote": remote.to_string(),
                        "dir": files.dir().file_name().map(|n| n.to_string_lossy().into_owned()),
                    }))?;
                    Some(files)
                }
                None => None,
            };
            if let (Proto::Tcp, Some(streams)) = (proto, self.streams.as_mut()) {
                streams.push(StreamChunk {
                    flow: id,
                    remote,
                    side: Side::In,
                    ts_us,
                    kind: ChunkKind::Open,
                });
            }
            self.stats.flows_opened += 1;
            self.flows.insert(
                key,
                Flow {
                    id,
                    proto,
                    remote,
                    files,
                    out: HalfStream::default(),
                    inn: HalfStream::default(),
                    packets: 0,
                },
            );
        }

        let flow = self.flows.get_mut(&key).expect("flow exists");
        flow.packets += 1;
        match seg.transport {
            Transport::Udp => {
                if !seg.payload.is_empty() {
                    if let Some(f) = flow.files.as_mut() {
                        f.data(ts_us, side, "dgram", None, seg.payload)?;
                    }
                    add_bytes(&mut self.stats, side, seg.payload.len() as u64);
                }
            }
            Transport::Tcp { seq, flags, .. } => {
                let half = match side {
                    Side::Out => &mut flow.out,
                    Side::In => &mut flow.inn,
                };
                let mut events = Vec::new();
                let was_midstream = half.midstream();
                half.push(ts_us, seq, flags & tcp_flags::SYN != 0, seg.payload, &mut events);
                // Joined part-way through (no SYN seen): said before its first bytes, so
                // whoever splits them into messages knows they may begin inside one.
                if !was_midstream && half.midstream() {
                    if let Some(f) = flow.files.as_mut() {
                        f.mark(ts_us, side, "midstream", json!({}))?;
                    }
                    if let Some(streams) = self.streams.as_mut() {
                        streams.push(StreamChunk { flow: flow.id, remote: flow.remote, side, ts_us, kind: ChunkKind::Gap(0) });
                    }
                }
                let out = Outputs {
                    files: flow.files.as_mut(),
                    streams: self.streams.as_mut(),
                    flow: flow.id,
                    remote: flow.remote,
                };
                write_events(out, &mut self.stats, ts_us, side, events)?;
                if let Some(f) = flow.files.as_mut() {
                    for (bit, name) in [
                        (tcp_flags::SYN, "syn"),
                        (tcp_flags::FIN, "fin"),
                        (tcp_flags::RST, "rst"),
                    ] {
                        if flags & bit != 0 {
                            f.mark(ts_us, side, name, json!({ "seq": seq }))?;
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn park(&mut self, ts_us: u64, ip: &[u8], seg: &Segment) {
        let proto = match seg.transport {
            Transport::Tcp { .. } => Proto::Tcp,
            Transport::Udp => Proto::Udp,
        };
        let (a, b) = if seg.src <= seg.dst {
            (seg.src, seg.dst)
        } else {
            (seg.dst, seg.src)
        };
        let queue = self.limbo.entry((proto, a, b)).or_default();
        queue.push_back((ts_us, ip.to_vec()));
        self.limbo_bytes += ip.len();
        if queue.len() > LIMBO_MAX_PACKETS_PER_FLOW {
            if let Some((_, old)) = queue.pop_front() {
                self.limbo_bytes -= old.len();
            }
        }
        // Too much held: the connections that have waited longest go first.
        while self.limbo_bytes > LIMBO_MAX_BYTES {
            let oldest = self.limbo.iter().min_by_key(|(_, q)| q.front().map(|(ts, _)| *ts)).map(|(k, _)| *k);
            let Some(queue) = oldest.and_then(|k| self.limbo.remove(&k)) else { break };
            self.limbo_bytes -= queue.iter().map(|(_, p)| p.len()).sum::<usize>();
        }
    }

    fn close_flow(&mut self, ts_us: u64, mut flow: Flow) -> Result<()> {
        for side in [Side::Out, Side::In] {
            let half = match side {
                Side::Out => &mut flow.out,
                Side::In => &mut flow.inn,
            };
            let mut events = Vec::new();
            half.finish(&mut events);
            let out = Outputs {
                files: flow.files.as_mut(),
                streams: self.streams.as_mut(),
                flow: flow.id,
                remote: flow.remote,
            };
            write_events(out, &mut self.stats, ts_us, side, events)?;
        }
        if let (Proto::Tcp, Some(streams)) = (flow.proto, self.streams.as_mut()) {
            streams.push(StreamChunk {
                flow: flow.id,
                remote: flow.remote,
                side: Side::In,
                ts_us,
                kind: ChunkKind::Close,
            });
        }
        let Some(files) = flow.files.as_mut() else {
            return Ok(());
        };
        files.flush()?;
        let half_json = |h: &HalfStream| {
            let s = h.stats();
            json!({
                "segments": s.segments,
                "bytes": s.delivered,
                "duplicate_bytes": s.duplicate_bytes,
                "gap_bytes": s.gap_bytes,
                "midstream": h.midstream(),
            })
        };
        let Some(rec) = self.rec.as_mut() else {
            return Ok(());
        };
        rec.event(&json!({
            "t": ts_us,
            "event": "flow_close",
            "flow": flow.id,
            "proto": flow.proto.as_str(),
            "packets": flow.packets,
            "out": half_json(&flow.out),
            "in": half_json(&flow.inn),
        }))
    }
}

fn add_bytes(stats: &mut PipelineStats, side: Side, n: u64) {
    match side {
        Side::Out => stats.bytes_out += n,
        Side::In => stats.bytes_in += n,
    }
}

/// Where one flow's reassembled data goes: its files, the live outbox, or both.
struct Outputs<'a> {
    files: Option<&'a mut FlowFiles>,
    streams: Option<&'a mut Vec<StreamChunk>>,
    flow: u32,
    remote: SocketAddr,
}

fn write_events(
    mut out: Outputs,
    stats: &mut PipelineStats,
    ts_us: u64,
    side: Side,
    events: Vec<StreamEvent>,
) -> Result<()> {
    for ev in events {
        let kind = match ev {
            StreamEvent::Data { offset, bytes } => {
                if let Some(f) = out.files.as_deref_mut() {
                    f.data(ts_us, side, "data", Some(offset), &bytes)?;
                }
                add_bytes(stats, side, bytes.len() as u64);
                ChunkKind::Data(bytes)
            }
            StreamEvent::Gap { offset, len } => {
                if let Some(f) = out.files.as_deref_mut() {
                    f.mark(ts_us, side, "gap", json!({ "off": offset, "len": len }))?;
                }
                stats.gap_bytes += len;
                ChunkKind::Gap(len)
            }
        };
        if let Some(streams) = out.streams.as_deref_mut() {
            streams.push(StreamChunk {
                flow: out.flow,
                remote: out.remote,
                side,
                ts_us,
                kind,
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pcapfile::PcapReader;
    use crate::testutil::{ipv4_tcp, ipv4_udp, temp_dir};
    use std::fs;

    const ME: [u8; 4] = [10, 0, 0, 2];
    const SERVER: [u8; 4] = [1, 2, 3, 4];

    fn flow_dirs(session: &Path) -> Vec<std::path::PathBuf> {
        let mut dirs: Vec<_> = fs::read_dir(session.join("flows"))
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect();
        dirs.sort();
        dirs
    }

    #[test]
    fn records_both_directions_of_a_tcp_flow() {
        let dir = temp_dir("pipeline-any");
        let rec = Recorder::create(&dir, &json!({})).unwrap();
        let local_ips = HashSet::from([IpAddr::from(ME)]);
        let mut p = Pipeline::new(Matcher::Any { local_ips }, Some(rec));

        use tcp_flags::*;
        p.ingest(1, &ipv4_tcp(ME, 50000, SERVER, 7777, 100, SYN, b"")).unwrap();
        p.ingest(2, &ipv4_tcp(SERVER, 7777, ME, 50000, 500, SYN | ACK, b"")).unwrap();
        p.ingest(3, &ipv4_tcp(ME, 50000, SERVER, 7777, 101, ACK, b"hello")).unwrap();
        // Server data arrives out of order.
        p.ingest(4, &ipv4_tcp(SERVER, 7777, ME, 50000, 506, ACK, b"!!")).unwrap();
        p.ingest(5, &ipv4_tcp(SERVER, 7777, ME, 50000, 501, ACK, b"world")).unwrap();
        let stats = p.finish(6).unwrap();

        assert_eq!(stats.packets_matched, 5);
        assert_eq!(stats.bytes_out, 5);
        assert_eq!(stats.bytes_in, 7);
        let flows = flow_dirs(&dir);
        assert_eq!(flows.len(), 1);
        assert!(flows[0].ends_with("001-tcp-50000-to-1.2.3.4_7777"));
        assert_eq!(fs::read(flows[0].join("out.bin")).unwrap(), b"hello");
        assert_eq!(fs::read(flows[0].join("in.bin")).unwrap(), b"world!!");

        let mut r = PcapReader::new(fs::File::open(dir.join("raw.pcap")).unwrap()).unwrap();
        let mut n = 0;
        while r.next_packet().unwrap().is_some() {
            n += 1;
        }
        assert_eq!(n, 5);
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn streams_outbox_without_recording() {
        let local_ips = HashSet::from([IpAddr::from(ME)]);
        let mut p = Pipeline::new(Matcher::Any { local_ips }, None);
        p.collect_streams();

        use tcp_flags::*;
        p.ingest(1, &ipv4_tcp(ME, 50000, SERVER, 7777, 100, SYN, b"")).unwrap();
        p.ingest(2, &ipv4_tcp(SERVER, 7777, ME, 50000, 500, SYN | ACK, b"")).unwrap();
        // Out of order: reassembly hands the data out in stream order.
        p.ingest(3, &ipv4_tcp(SERVER, 7777, ME, 50000, 506, ACK, b"!!")).unwrap();
        p.ingest(4, &ipv4_tcp(SERVER, 7777, ME, 50000, 501, ACK, b"world")).unwrap();
        p.ingest(5, &ipv4_tcp(ME, 50000, SERVER, 7777, 101, ACK, b"hi")).unwrap();

        let got: Vec<(Side, ChunkKind)> = p.take_streams().into_iter().map(|c| (c.side, c.kind)).collect();
        assert_eq!(
            got,
            vec![
                (Side::In, ChunkKind::Open),
                (Side::In, ChunkKind::Data(b"world".to_vec())),
                (Side::In, ChunkKind::Data(b"!!".to_vec())),
                (Side::Out, ChunkKind::Data(b"hi".to_vec())),
            ]
        );
        assert!(p.take_streams().is_empty());
        assert!(p.session_dir().is_none());
        p.finish(6).unwrap();
    }

    #[test]
    fn early_packets_are_kept_once_the_connection_is_identified() {
        let dir = temp_dir("pipeline-limbo");
        let rec = Recorder::create(&dir, &json!({})).unwrap();
        let mut p = Pipeline::new(Matcher::Live(LiveMatcher::default()), Some(rec));

        use tcp_flags::*;
        let game = 4242;
        let other = 7;
        // The game's handshake and first message arrive before the table poll sees it.
        p.ingest(1, &ipv4_tcp(ME, 50000, SERVER, 7777, 100, SYN, b"")).unwrap();
        p.ingest(2, &ipv4_tcp(SERVER, 7777, ME, 50000, 500, SYN | ACK, b"")).unwrap();
        p.ingest(3, &ipv4_tcp(SERVER, 7777, ME, 50000, 501, ACK, b"welcome")).unwrap();
        // Unrelated traffic from another app.
        p.ingest(4, &ipv4_tcp(ME, 50001, SERVER, 443, 9, ACK, b"browser")).unwrap();
        p.ingest(5, &ipv4_udp(ME, 6000, SERVER, 6001, b"voice")).unwrap();

        let tcp = vec![
            TcpConn {
                pid: game,
                local: "10.0.0.2:50000".parse().unwrap(),
                remote: "1.2.3.4:7777".parse().unwrap(),
                state: TcpConn::ESTABLISHED,
            },
            TcpConn {
                pid: other,
                local: "10.0.0.2:50001".parse().unwrap(),
                remote: "1.2.3.4:443".parse().unwrap(),
                state: TcpConn::ESTABLISHED,
            },
        ];
        let udp = vec![UdpEndpoint {
            pid: game,
            local: "10.0.0.2:6000".parse().unwrap(),
        }];
        let m = LiveMatcher::from_tables(&HashSet::from([game]), &tcp, &udp);
        assert_eq!((m.tcp_count(), m.udp_count()), (1, 1));
        p.set_matcher(Matcher::Live(m), 10).unwrap();
        p.ingest(11, &ipv4_tcp(SERVER, 7777, ME, 50000, 508, ACK, b"!")).unwrap();
        let stats = p.finish(12).unwrap();

        assert_eq!(stats.flows_opened, 2);
        let flows = flow_dirs(&dir);
        assert_eq!(fs::read(flows[0].join("in.bin")).unwrap(), b"welcome!");
        assert!(flows[1].ends_with("002-udp-6000-to-1.2.3.4_6001"));
        assert_eq!(fs::read(flows[1].join("out.bin")).unwrap(), b"voice");
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn other_programs_packets_are_dropped_not_held() {
        let (game, browser) = (4242, 7);
        let tcp = vec![
            TcpConn { pid: game, local: "10.0.0.2:50000".parse().unwrap(), remote: "1.2.3.4:7777".parse().unwrap(), state: TcpConn::ESTABLISHED },
            TcpConn { pid: browser, local: "10.0.0.2:50001".parse().unwrap(), remote: "1.2.3.4:443".parse().unwrap(), state: TcpConn::ESTABLISHED },
        ];
        let udp = vec![UdpEndpoint { pid: browser, local: "10.0.0.2:6000".parse().unwrap() }];
        let m = LiveMatcher::from_tables(&HashSet::from([game]), &tcp, &udp);
        let mut p = Pipeline::new(Matcher::Live(m), None);

        use tcp_flags::*;
        // The browser's download, both ways, and its voice chat: known to be someone else's.
        p.ingest(1, &ipv4_tcp(SERVER, 443, ME, 50001, 9, ACK, &[0; 1000])).unwrap();
        p.ingest(2, &ipv4_tcp(ME, 50001, SERVER, 443, 9, ACK, b"ack")).unwrap();
        p.ingest(3, &ipv4_udp(ME, 6000, SERVER, 6001, b"voice")).unwrap();
        // A connection nobody has yet (it may turn out to be the game's): held.
        p.ingest(4, &ipv4_tcp(ME, 50002, SERVER, 7778, 100, SYN, b"")).unwrap();
        assert_eq!((p.stats().packets_others, p.limbo.len()), (3, 1));
        assert!(p.limbo_bytes < 100, "only the unknown connection's packet is held");
        // The game's own still counts.
        p.ingest(5, &ipv4_tcp(SERVER, 7777, ME, 50000, 501, ACK, b"hit")).unwrap();
        assert_eq!(p.stats().packets_matched, 1);
    }

    #[test]
    fn when_too_much_is_held_the_oldest_connection_goes_first() {
        let mut p = Pipeline::new(Matcher::Live(LiveMatcher::default()), None);
        use tcp_flags::*;
        let big = vec![0u8; 60_000];
        // Unknown connections, one packet each, until they're more than limbo holds.
        let connections = (LIMBO_MAX_BYTES / big.len()) as u64 + 2;
        for i in 0..connections {
            p.ingest(1 + i, &ipv4_tcp(ME, 50_001 + i as u16, SERVER, 9001, 1, ACK, &big)).unwrap();
        }
        assert!(p.limbo_bytes <= LIMBO_MAX_BYTES);
        let first: PairKey = (Proto::Tcp, "1.2.3.4:9001".parse().unwrap(), "10.0.0.2:50001".parse().unwrap());
        assert!(!p.limbo.contains_key(&first), "the one that waited longest was let go");
        assert!(!p.limbo.is_empty(), "the rest are still held");
    }

    #[test]
    fn a_connection_joined_part_way_says_so_before_its_first_bytes() {
        let local_ips = HashSet::from([IpAddr::from(ME)]);
        let mut p = Pipeline::new(Matcher::Any { local_ips }, None);
        p.collect_streams();
        use tcp_flags::*;
        // No SYN seen: the capture began after the connection did.
        p.ingest(1, &ipv4_tcp(SERVER, 7777, ME, 50000, 9000, ACK, b"tail")).unwrap();
        p.ingest(2, &ipv4_tcp(SERVER, 7777, ME, 50000, 9004, ACK, b"next")).unwrap();
        let got: Vec<ChunkKind> = p.take_streams().into_iter().map(|c| c.kind).collect();
        assert_eq!(got, [ChunkKind::Open, ChunkKind::Gap(0), ChunkKind::Data(b"tail".to_vec()), ChunkKind::Data(b"next".to_vec())]);
    }
}
