//! Live capture of one game process: find it, pick the adapter it talks through,
//! capture, and keep the socket list up to date while it runs.

use std::collections::HashSet;
use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, SyncSender};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{bail, Context, Result};
use serde_json::json;

use crate::conns::{self, ProcessInfo};
use crate::decode;
use crate::game::GameStreams;
use crate::npcap::{Capture, Device, Npcap};
use crate::pipeline::{LiveMatcher, Matcher, Pipeline, PipelineStats};
use crate::recorder::Recorder;
use crate::session::TimedMessage;

/// Only TCP/UDP reaches us; the pipeline narrows it to the game's own sockets.
const CAPTURE_FILTER: &str = "tcp or udp";
const REFRESH_EVERY: Duration = Duration::from_millis(500);
const STATUS_EVERY: Duration = Duration::from_secs(1);
/// The game seen only on other adapters this many looks in a row (2 s): it has moved.
const ADAPTER_CHECKS: u32 = 4;

pub struct LiveOptions {
    /// Case-insensitive substring of the game's executable name.
    pub process: String,
    /// Parent folder for session folders.
    pub out_dir: PathBuf,
    /// Adapter selectors (index, or substring of name/description). Empty = automatic.
    pub devices: Vec<String>,
    pub duration: Option<Duration>,
    /// Save the session to `out_dir`.
    pub record: bool,
    /// Decode the game stream and emit [`LiveEvent::Messages`].
    pub decode: bool,
    /// Return (with `Ok`) when the game is found talking through another network adapter
    /// than the ones being captured (a VPN turned on or off, Wi-Fi to cable), so the
    /// caller can call [`run`] again and pick it up. Only when the adapters were chosen
    /// automatically, by the game's own address.
    pub follow_adapter: bool,
}

#[derive(Debug, Clone)]
pub struct LiveStatus {
    pub elapsed: Duration,
    pub pids: Vec<u32>,
    pub game_tcp: usize,
    pub game_udp: usize,
    pub received: u64,
    pub dropped: u64,
    pub stats: PipelineStats,
}

pub enum LiveEvent {
    Waiting(String),
    Started {
        /// None when not recording.
        session_dir: Option<PathBuf>,
        devices: Vec<String>,
        processes: Vec<ProcessInfo>,
    },
    Warning(String),
    Status(LiveStatus),
    /// Decoded game messages, in order (only with `decode`).
    Messages(Vec<TimedMessage>),
    Finished {
        session_dir: Option<PathBuf>,
        stats: PipelineStats,
    },
}

struct DeviceCounters {
    received: AtomicU64,
    dropped: AtomicU64,
}

pub fn now_us() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_micros() as u64)
        .unwrap_or(0)
}

/// Selects devices by index or by case-insensitive substring of name/description.
pub fn select_devices(all: &[Device], selectors: &[String]) -> Vec<Device> {
    all.iter()
        .enumerate()
        .filter(|(i, d)| {
            selectors.iter().any(|s| {
                s.parse::<usize>().ok() == Some(*i)
                    || d.name.to_lowercase().contains(&s.to_lowercase())
                    || d.description.to_lowercase().contains(&s.to_lowercase())
            })
        })
        .map(|(_, d)| d.clone())
        .collect()
}

/// Runs until `stop` is set or `opts.duration` passes. Returns the session folder,
/// if recording.
pub fn run(opts: &LiveOptions, stop: &AtomicBool, mut on_event: impl FnMut(LiveEvent)) -> Result<Option<PathBuf>> {
    let np = Arc::new(Npcap::load()?);

    // 1. Wait for the game and for it to open a connection.
    let (procs, local_ips) = loop {
        if stop.load(Ordering::Relaxed) {
            bail!("stopped before the game was found");
        }
        let procs = conns::find_processes(&opts.process)?;
        if procs.is_empty() {
            on_event(LiveEvent::Waiting(format!(
                "waiting for a process whose name contains \"{}\"",
                opts.process
            )));
            sleep_unless_stopped(stop, Duration::from_secs(1));
            continue;
        }
        let pids: HashSet<u32> = procs.iter().map(|p| p.pid).collect();
        let ips = conns::local_ips_for(&pids, &conns::tcp_connections()?);
        if ips.is_empty() && opts.devices.is_empty() {
            on_event(LiveEvent::Waiting(
                "game found, waiting for it to connect to a server".into(),
            ));
            sleep_unless_stopped(stop, Duration::from_secs(1));
            continue;
        }
        break (procs, ips);
    };

    // 2. Capture only on the adapter(s) the game actually uses: loopback too, when it goes
    // through a relay on this PC (ExitLag and other ping reducers).
    let all = np.devices()?;
    let (chosen, matched) = pick_devices(&all, &opts.devices, &local_ips, &mut on_event)?;
    let chosen_names: HashSet<String> = chosen.iter().map(|d| d.name.clone()).collect();
    let follows = opts.follow_adapter && opts.devices.is_empty() && matched;
    // The game's local addresses last looked at, and whether they wanted an adapter we
    // aren't on (looked up again only when they change).
    let mut seen_ips = local_ips.clone();
    let mut moved = false;

    // 3. Session folder, if recording.
    let session_dir = opts
        .record
        .then(|| opts.out_dir.join(chrono::Local::now().format("%Y%m%d-%H%M%S").to_string()));
    let meta = json!({
        "tool": concat!("packet-lab ", env!("CARGO_PKG_VERSION")),
        "started_us": now_us(),
        "process_filter": opts.process,
        "processes": procs.iter().map(|p| json!({"pid": p.pid, "name": p.name})).collect::<Vec<_>>(),
        "local_ips": local_ips.iter().map(|ip| ip.to_string()).collect::<Vec<_>>(),
        "devices": chosen.iter().map(|d| json!({"name": d.name, "description": d.description})).collect::<Vec<_>>(),
        "npcap": np.version(),
    });
    let rec = match &session_dir {
        Some(dir) => Some(Recorder::create(dir, &meta)?),
        None => None,
    };

    let mut pids: HashSet<u32> = procs.iter().map(|p| p.pid).collect();
    let mut matcher = snapshot_matcher(&pids)?;
    let mut current = matcher.clone();
    let mut pipeline = Pipeline::new(Matcher::Live(matcher.clone()), rec);
    let mut game = GameStreams::default();
    if opts.decode {
        pipeline.collect_streams();
    }

    // 4. One capture thread per adapter.
    let (tx, rx) = mpsc::sync_channel::<(u64, Vec<u8>)>(65_536);
    let threads_stop = Arc::new(AtomicBool::new(false));
    let mut counters = Vec::new();
    let mut handles = Vec::new();
    for dev in &chosen {
        let mut cap = np
            .open(&dev.name, CAPTURE_FILTER)
            .with_context(|| format!("opening capture on {}", dev.label()))?;
        let c = Arc::new(DeviceCounters {
            received: AtomicU64::new(0),
            dropped: AtomicU64::new(0),
        });
        counters.push(Arc::clone(&c));
        let tx = tx.clone();
        let stop_flag = Arc::clone(&threads_stop);
        handles.push(
            thread::Builder::new()
                .name(format!("capture {}", dev.label()))
                .spawn(move || capture_loop(&mut cap, &tx, &stop_flag, &c))?,
        );
    }
    drop(tx);

    on_event(LiveEvent::Started {
        session_dir: session_dir.clone(),
        devices: chosen.iter().map(|d| d.label().to_string()).collect(),
        processes: procs,
    });

    // 5. Main loop: record packets, poll the socket table, report status.
    let started = Instant::now();
    let mut last_refresh = started;
    let mut last_status = started;
    let mut game_gone = false;
    let mut failure: Option<anyhow::Error> = None;
    // Looks at the tables in a row that found the game only on adapters we aren't on.
    let mut elsewhere = 0u32;
    loop {
        if stop.load(Ordering::Relaxed) || opts.duration.is_some_and(|d| started.elapsed() >= d) {
            break;
        }
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok((ts, pkt)) => {
                if let Err(e) = pipeline.ingest(ts, &pkt) {
                    failure = Some(e);
                    break;
                }
                emit_messages(&mut pipeline, &mut game, &mut on_event);
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                on_event(LiveEvent::Warning("all capture threads stopped".into()));
                break;
            }
        }
        if last_refresh.elapsed() >= REFRESH_EVERY {
            last_refresh = Instant::now();
            let refreshed = (|| -> Result<()> {
                let procs = conns::find_processes(&opts.process)?;
                if procs.is_empty() {
                    if !game_gone {
                        on_event(LiveEvent::Warning(
                            "the game exited; still capturing in case it restarts".into(),
                        ));
                        game_gone = true;
                    }
                    return pipeline.tick(now_us());
                }
                game_gone = false;
                pids = procs.iter().map(|p| p.pid).collect();
                let (tcp, udp) = (conns::tcp_connections()?, conns::udp_endpoints()?);
                current = LiveMatcher::from_tables(&pids, &tcp, &udp);
                matcher.merge(&current);
                let ips = conns::local_ips_for(&pids, &tcp);
                if follows && !ips.is_empty() && ips != seen_ips {
                    // The game's connections moved (a new server, a VPN or ping reducer
                    // turned on or off): is it on an adapter we aren't capturing?
                    let (wanted, _) = devices_for(&np.devices()?, &ips, &conns::interface_guids());
                    moved = wanted.iter().any(|d| !chosen_names.contains(&d.name));
                    seen_ips = ips;
                }
                elsewhere = if follows && moved { elsewhere + 1 } else { 0 };
                pipeline.set_matcher(Matcher::Live(matcher.clone()), now_us())
            })();
            if let Err(e) = refreshed {
                failure = Some(e);
                break;
            }
            if elsewhere >= ADAPTER_CHECKS {
                on_event(LiveEvent::Warning("the game's connection moved to another network adapter: starting the capture again".into()));
                break;
            }
            // Released held packets and skipped holes can produce data too.
            emit_messages(&mut pipeline, &mut game, &mut on_event);
        }
        if last_status.elapsed() >= STATUS_EVERY {
            last_status = Instant::now();
            on_event(LiveEvent::Status(LiveStatus {
                elapsed: started.elapsed(),
                pids: pids.iter().copied().collect(),
                game_tcp: current.tcp_count(),
                game_udp: current.udp_count(),
                received: counters.iter().map(|c| c.received.load(Ordering::Relaxed)).sum(),
                dropped: counters.iter().map(|c| c.dropped.load(Ordering::Relaxed)).sum(),
                stats: pipeline.stats(),
            }));
        }
    }

    // 6. Stop the threads, keep whatever they already captured, close the session.
    threads_stop.store(true, Ordering::Relaxed);
    if failure.is_none() {
        loop {
            match rx.recv_timeout(Duration::from_millis(500)) {
                Ok((ts, pkt)) => {
                    if let Err(e) = pipeline.ingest(ts, &pkt) {
                        failure = Some(e);
                        break;
                    }
                    emit_messages(&mut pipeline, &mut game, &mut on_event);
                }
                Err(RecvTimeoutError::Timeout) => continue,
                Err(RecvTimeoutError::Disconnected) => break,
            }
        }
    }
    drop(rx);
    for h in handles {
        match h.join() {
            Ok(Ok(())) => {}
            Ok(Err(e)) => on_event(LiveEvent::Warning(format!("capture thread failed: {e:#}"))),
            Err(_) => on_event(LiveEvent::Warning("a capture thread panicked".into())),
        }
    }
    let stats = pipeline.finish(now_us())?;
    if let Some(e) = failure {
        return Err(match &session_dir {
            Some(dir) => e.context(format!("capture stopped early; partial session in {}", dir.display())),
            None => e.context("capture stopped early"),
        });
    }
    on_event(LiveEvent::Finished {
        session_dir: session_dir.clone(),
        stats,
    });
    Ok(session_dir)
}

fn emit_messages(pipeline: &mut Pipeline, game: &mut GameStreams, on_event: &mut impl FnMut(LiveEvent)) {
    let chunks = pipeline.take_streams();
    if chunks.is_empty() {
        return;
    }
    let mut messages = Vec::new();
    for chunk in chunks {
        game.push(chunk, &mut messages);
    }
    if !messages.is_empty() {
        on_event(LiveEvent::Messages(messages));
    }
}

/// What to say when the game goes through a relay on this PC and Npcap can't capture
/// this PC's own (loopback) traffic.
const NO_LOOPBACK: &str = "the game's connection goes through a relay on this PC (ExitLag or another ping reducer), and Npcap can't see this PC's own traffic: reinstall Npcap (npcap.com) with \"Support loopback traffic\" ticked";

/// The adapters the game's local addresses are on, each once, and the addresses none is
/// found for. A loopback address (a relay on this PC) is Npcap's loopback adapter; any
/// other is the adapter Npcap lists it on, else the one Windows says it's on (by its GUID,
/// in the adapter's name: Npcap lists some adapters without their addresses).
pub fn devices_for(all: &[Device], ips: &HashSet<IpAddr>, guids: &[(IpAddr, String)]) -> (Vec<Device>, Vec<IpAddr>) {
    let mut found: Vec<Device> = Vec::new();
    let mut missing = Vec::new();
    let mut ips: Vec<&IpAddr> = ips.iter().collect();
    ips.sort();
    for ip in ips {
        let device = if ip.is_loopback() {
            all.iter().find(|d| d.loopback)
        } else {
            all.iter().find(|d| !d.loopback && d.addrs.contains(ip)).or_else(|| {
                let guid = guids.iter().find(|(a, _)| a == ip).map(|(_, g)| g.to_ascii_uppercase())?;
                all.iter().find(|d| d.name.to_ascii_uppercase().ends_with(&guid))
            })
        };
        match device {
            Some(d) if !found.iter().any(|f| f.name == d.name) => found.push(d.clone()),
            Some(_) => {}
            None => missing.push(*ip),
        }
    }
    (found, missing)
}

/// The adapters to capture on, and whether they were found from the game's own addresses
/// (else chosen by hand, or every adapter as a fallback).
fn pick_devices(
    all: &[Device],
    selectors: &[String],
    local_ips: &HashSet<IpAddr>,
    on_event: &mut impl FnMut(LiveEvent),
) -> Result<(Vec<Device>, bool)> {
    if !selectors.is_empty() {
        let chosen = select_devices(all, selectors);
        if chosen.is_empty() {
            bail!("no capture device matches {selectors:?} (run `packet-lab devices`)");
        }
        return Ok((chosen, false));
    }
    let (found, missing) = devices_for(all, local_ips, &conns::interface_guids());
    let relay_unseen = missing.iter().any(|ip| ip.is_loopback());
    if !found.is_empty() {
        if relay_unseen {
            on_event(LiveEvent::Warning(NO_LOOPBACK.into()));
        } else if !missing.is_empty() {
            on_event(LiveEvent::Warning(format!("couldn't find the adapter for {}: capturing on the others", join(&missing))));
        }
        return Ok((found, true));
    }
    // Only through a relay we can't see: no adapter would show the game's messages.
    if relay_unseen && missing.iter().all(|ip| ip.is_loopback()) {
        bail!("{NO_LOOPBACK}");
    }
    on_event(LiveEvent::Warning(
        "couldn't match the game's local address to an adapter (VPN?); capturing on all adapters".into(),
    ));
    let fallback: Vec<Device> = all
        .iter()
        .filter(|d| !d.loopback && !d.addrs.is_empty())
        .cloned()
        .collect();
    if fallback.is_empty() {
        bail!("no usable capture devices found");
    }
    Ok((fallback, false))
}

fn join(ips: &[IpAddr]) -> String {
    ips.iter().map(|ip| ip.to_string()).collect::<Vec<_>>().join(", ")
}

fn snapshot_matcher(pids: &HashSet<u32>) -> Result<LiveMatcher> {
    Ok(LiveMatcher::from_tables(
        pids,
        &conns::tcp_connections()?,
        &conns::udp_endpoints()?,
    ))
}

fn capture_loop(
    cap: &mut Capture,
    tx: &SyncSender<(u64, Vec<u8>)>,
    stop: &AtomicBool,
    counters: &DeviceCounters,
) -> Result<()> {
    let linktype = cap.linktype();
    let mut last_stats = Instant::now();
    while !stop.load(Ordering::Relaxed) {
        if let Some(pkt) = cap.next_packet()? {
            if let Some(ip) = decode::link_payload(linktype, pkt.data) {
                if tx.send((pkt.ts_us, ip.to_vec())).is_err() {
                    break;
                }
            }
        }
        if last_stats.elapsed() >= Duration::from_secs(1) {
            last_stats = Instant::now();
            if let Ok(s) = cap.stats() {
                counters.received.store(s.received, Ordering::Relaxed);
                counters.dropped.store(s.dropped, Ordering::Relaxed);
            }
        }
    }
    Ok(())
}

fn sleep_unless_stopped(stop: &AtomicBool, total: Duration) {
    let step = Duration::from_millis(100);
    let mut waited = Duration::ZERO;
    while waited < total && !stop.load(Ordering::Relaxed) {
        thread::sleep(step);
        waited += step;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conns::TcpConn;
    use crate::decode::{self, linktype, tcp_flags};
    use crate::game::GameStreams;
    use crate::pipeline::{LiveMatcher, Matcher, Pipeline};
    use crate::protocol::tests::frame;
    use crate::testutil::ipv4_tcp;

    fn device(name: &str, addrs: &[&str], loopback: bool) -> Device {
        Device { name: name.into(), description: String::new(), addrs: addrs.iter().map(|a| a.parse().unwrap()).collect(), loopback }
    }

    fn ips(list: &[&str]) -> HashSet<IpAddr> {
        list.iter().map(|a| a.parse().unwrap()).collect()
    }

    /// This PC's adapters, as Npcap lists them (2026-11-14).
    fn adapters() -> Vec<Device> {
        vec![
            device(r"\Device\NPF_{E2965313-B9C2-4EAC-82E6-1534D115B86E}", &["172.21.192.1"], false),
            device(r"\Device\NPF_{BB17D9E9-3D2D-4CF5-8166-07498586EFAB}", &["192.168.0.248"], false),
            // A VPN's adapter, listed without its address.
            device(r"\Device\NPF_{0A1B2C3D-4E5F-6071-8293-A4B5C6D7E8F9}", &[], false),
            device(r"\Device\NPF_Loopback", &[], true),
        ]
    }

    #[test]
    fn the_game_straight_to_its_server_is_captured_on_its_adapter() {
        let (found, missing) = devices_for(&adapters(), &ips(&["192.168.0.248"]), &[]);
        assert_eq!(found.iter().map(|d| d.name.as_str()).collect::<Vec<_>>(), [r"\Device\NPF_{BB17D9E9-3D2D-4CF5-8166-07498586EFAB}"]);
        assert!(missing.is_empty());
    }

    #[test]
    fn the_game_through_a_relay_on_this_pc_is_captured_on_loopback() {
        // ExitLag: the game talks to its relay on 127.0.0.1, and keeps a direct connection
        // or two (the launcher's, a web one).
        let (found, missing) = devices_for(&adapters(), &ips(&["127.0.0.1", "192.168.0.248"]), &[]);
        let names: Vec<&str> = found.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(names, [r"\Device\NPF_Loopback", r"\Device\NPF_{BB17D9E9-3D2D-4CF5-8166-07498586EFAB}"]);
        assert!(missing.is_empty());
        // Npcap installed without loopback support: the relay can't be seen.
        let without: Vec<Device> = adapters().into_iter().filter(|d| !d.loopback).collect();
        let (found, missing) = devices_for(&without, &ips(&["127.0.0.1"]), &[]);
        assert!(found.is_empty());
        assert_eq!(missing, ["127.0.0.1".parse::<IpAddr>().unwrap()]);
    }

    #[test]
    fn an_adapter_listed_without_its_address_is_found_by_its_guid() {
        let guids = [("10.8.0.2".parse().unwrap(), "{0a1b2c3d-4e5f-6071-8293-a4b5c6d7e8f9}".to_string())];
        let (found, missing) = devices_for(&adapters(), &ips(&["10.8.0.2"]), &guids);
        assert_eq!(found.iter().map(|d| d.name.as_str()).collect::<Vec<_>>(), [r"\Device\NPF_{0A1B2C3D-4E5F-6071-8293-A4B5C6D7E8F9}"]);
        assert!(missing.is_empty());
        // Nothing has it: said so.
        let (found, missing) = devices_for(&adapters(), &ips(&["10.9.9.9"]), &guids);
        assert!(found.is_empty() && missing.len() == 1);
    }

    #[test]
    fn game_messages_read_off_loopback_through_a_relay() {
        // The game (pid 7) talks to ExitLag's relay (pid 99) on this PC; the relay has its
        // own end of that connection, and a tunnel out of its own.
        let (game, relay) = (7, 99);
        let conn = |pid, local: &str, remote: &str| TcpConn { pid, local: local.parse().unwrap(), remote: remote.parse().unwrap(), state: TcpConn::ESTABLISHED };
        let tcp = [
            conn(game, "127.0.0.1:50001", "127.0.0.1:61000"),
            conn(relay, "127.0.0.1:61000", "127.0.0.1:50001"),
            conn(relay, "192.168.0.248:52000", "45.60.1.2:443"),
        ];
        let pids = HashSet::from([game]);
        assert_eq!(conns::local_ips_for(&pids, &tcp), ips(&["127.0.0.1"]));
        let mut p = Pipeline::new(Matcher::Live(LiveMatcher::from_tables(&pids, &tcp, &[])), None);
        p.collect_streams();
        // Npcap's loopback adapter: DLT_NULL, a 4-byte address family before each packet.
        let looped = |ip: Vec<u8>| {
            let mut f = 2u32.to_le_bytes().to_vec();
            f.extend(ip);
            f
        };
        let (lo, game_port, relay_port) = ([127, 0, 0, 1], 50001, 61000);
        let mut stream = frame(0x3600, b"hello");
        stream.extend(frame(0x3804, &[7u8; 40]));
        let mut seq = 1000;
        let mut packets = vec![looped(ipv4_tcp(lo, game_port, lo, relay_port, 1, tcp_flags::ACK, b"login"))];
        for chunk in stream.chunks(20) {
            packets.push(looped(ipv4_tcp(lo, relay_port, lo, game_port, seq, tcp_flags::ACK, chunk)));
            seq += chunk.len() as u32;
        }
        for (i, f) in packets.iter().enumerate() {
            let ip = decode::link_payload(linktype::NULL, f).unwrap();
            p.ingest(i as u64 + 1, ip).unwrap();
        }
        assert_eq!(p.stats().packets_matched, packets.len() as u64);
        // Into game messages, as the meter reads them (the connection proves itself first).
        let mut game_streams = GameStreams::default();
        let mut out = Vec::new();
        for c in p.take_streams() {
            game_streams.push(c, &mut out);
        }
        let small = frame(0x3600, b"x");
        for _ in 0..crate::game::CONFIRM_AFTER_SLIP {
            let f = looped(ipv4_tcp(lo, relay_port, lo, game_port, seq, tcp_flags::ACK, &small));
            p.ingest(99, decode::link_payload(linktype::NULL, &f).unwrap()).unwrap();
            seq += small.len() as u32;
        }
        for c in p.take_streams() {
            game_streams.push(c, &mut out);
        }
        assert!(game_streams.connected());
        assert_eq!(out.iter().take(2).map(|m| m.opcode).collect::<Vec<_>>(), [0x3600, 0x3804]);
    }
}
