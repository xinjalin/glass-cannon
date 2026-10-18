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
const STATUS_EVERY: Duration = Duration::from_secs(2);

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

    // 2. Capture only on the adapter(s) the game actually uses.
    let all = np.devices()?;
    let chosen = pick_devices(&all, &opts.devices, &local_ips, &mut on_event)?;

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
                current = snapshot_matcher(&pids)?;
                matcher.merge(&current);
                pipeline.set_matcher(Matcher::Live(matcher.clone()), now_us())
            })();
            if let Err(e) = refreshed {
                failure = Some(e);
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

fn pick_devices(
    all: &[Device],
    selectors: &[String],
    local_ips: &HashSet<IpAddr>,
    on_event: &mut impl FnMut(LiveEvent),
) -> Result<Vec<Device>> {
    if !selectors.is_empty() {
        let chosen = select_devices(all, selectors);
        if chosen.is_empty() {
            bail!("no capture device matches {selectors:?} (run `packet-lab devices`)");
        }
        return Ok(chosen);
    }
    let matched: Vec<Device> = all
        .iter()
        .filter(|d| !d.loopback && d.addrs.iter().any(|a| local_ips.contains(a)))
        .cloned()
        .collect();
    if !matched.is_empty() {
        return Ok(matched);
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
    Ok(fallback)
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
