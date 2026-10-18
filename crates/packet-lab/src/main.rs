//! packet-lab: capture and inspect the game's network traffic for reverse-engineering.

use std::collections::HashSet;
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use serde_json::{json, Value};

use meter_core::conns::{self, TcpConn};
use meter_core::live::{self, LiveEvent, LiveOptions};
use meter_core::npcap::Npcap;
use meter_core::pcapfile::PcapReader;
use meter_core::pipeline::{Matcher, Pipeline};
use meter_core::recorder::Recorder;
use meter_core::decode;

#[derive(Parser)]
#[command(name = "packet-lab", version, about = "Capture and inspect a game's network traffic (no admin needed)")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// List Npcap capture adapters
    Devices,
    /// List processes with network connections (use this to find the game's exe name)
    Procs {
        /// Only processes whose name contains this (case-insensitive); shows each socket
        name: Option<String>,
    },
    /// Capture a game process's traffic into a session folder (Ctrl+C to stop)
    Capture {
        /// Substring of the game's executable name, e.g. "aion"
        #[arg(short, long)]
        process: String,
        /// Folder that session folders are created in
        #[arg(short, long, default_value = "captures")]
        out: PathBuf,
        /// Adapter index or name substring (repeatable). Default: the adapter the game uses
        #[arg(short, long)]
        device: Vec<String>,
        /// Stop automatically after this many seconds
        #[arg(long)]
        seconds: Option<u64>,
    },
    /// Re-run reassembly on a .pcap file (e.g. a session's raw.pcap)
    Replay {
        pcap: PathBuf,
        #[arg(short, long, default_value = "captures")]
        out: PathBuf,
        /// Our own IP address(es), to tell sent from received. Read from session.json if omitted
        #[arg(long)]
        local_ip: Vec<IpAddr>,
    },
    /// Show a flow's chunks in time order with their first bytes
    Chunks {
        /// A flow folder inside a session's flows/ folder
        flow: PathBuf,
        #[arg(long, value_enum)]
        side: Option<SideArg>,
        /// Maximum lines to print
        #[arg(long, default_value_t = 200)]
        limit: usize,
        /// Bytes of each chunk to show
        #[arg(long, default_value_t = 32)]
        bytes: usize,
    },
    /// Hex dump part of a flow's reassembled stream
    Dump {
        flow: PathBuf,
        #[arg(long, value_enum)]
        side: SideArg,
        /// File position to start at (the `pos` column from `chunks`)
        #[arg(long, default_value_t = 0)]
        pos: u64,
        #[arg(long, default_value_t = 256)]
        len: usize,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum SideArg {
    In,
    Out,
}

impl SideArg {
    fn as_str(self) -> &'static str {
        match self {
            SideArg::In => "in",
            SideArg::Out => "out",
        }
    }
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<()> {
    match cli.cmd {
        Cmd::Devices => devices(),
        Cmd::Procs { name } => procs(name.as_deref()),
        Cmd::Capture {
            process,
            out,
            device,
            seconds,
        } => capture(LiveOptions {
            process,
            out_dir: out,
            devices: device,
            duration: seconds.map(Duration::from_secs),
        }),
        Cmd::Replay { pcap, out, local_ip } => replay(&pcap, &out, local_ip),
        Cmd::Chunks {
            flow,
            side,
            limit,
            bytes,
        } => chunks(&flow, side, limit, bytes),
        Cmd::Dump { flow, side, pos, len } => dump(&flow, side, pos, len),
    }
}

fn devices() -> Result<()> {
    let np = Npcap::load()?;
    println!("{}\n", np.version());
    for (i, d) in np.devices()?.iter().enumerate() {
        let tag = if d.loopback { "  (loopback)" } else { "" };
        println!("[{i}] {}{tag}", d.label());
        println!("     {}", d.name);
        if !d.addrs.is_empty() {
            let addrs: Vec<String> = d.addrs.iter().map(|a| a.to_string()).collect();
            println!("     {}", addrs.join(", "));
        }
    }
    Ok(())
}

fn procs(name: Option<&str>) -> Result<()> {
    let all = conns::processes()?;
    let tcp = conns::tcp_connections()?;
    let udp = conns::udp_endpoints()?;
    let internet = |c: &TcpConn| c.state == TcpConn::ESTABLISHED && !c.remote.ip().is_loopback();

    let mut rows: Vec<_> = all
        .iter()
        .filter(|p| p.pid != 0)
        .filter(|p| name.is_none_or(|n| p.name.to_lowercase().contains(&n.to_lowercase())))
        .map(|p| {
            let t: Vec<&TcpConn> = tcp.iter().filter(|c| c.pid == p.pid).collect();
            let u: Vec<_> = udp.iter().filter(|e| e.pid == p.pid).collect();
            let live = t.iter().filter(|c| internet(c)).count();
            (p, t, u, live)
        })
        .filter(|(_, _, _, live)| name.is_some() || *live > 0)
        .collect();
    rows.sort_by(|a, b| b.3.cmp(&a.3).then(a.0.name.cmp(&b.0.name)));

    if rows.is_empty() {
        match name {
            Some(n) => println!("no running process name contains \"{n}\""),
            None => println!("no process has an internet connection right now"),
        }
        return Ok(());
    }
    for (p, t, u, live) in rows {
        println!(
            "{:>7}  {}  ({live} internet TCP, {} TCP total, {} UDP)",
            p.pid,
            p.name,
            t.len(),
            u.len()
        );
        if name.is_some() {
            for c in t {
                println!("           tcp {:<22} -> {:<22} {}", c.local, c.remote, c.state_name());
            }
            for e in u {
                println!("           udp {}", e.local);
            }
        }
    }
    Ok(())
}

fn capture(opts: LiveOptions) -> Result<()> {
    let stop = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&stop);
    ctrlc::set_handler(move || flag.store(true, Ordering::Relaxed)).context("installing Ctrl+C handler")?;

    let mut last_wait = String::new();
    live::run(&opts, &stop, |ev| match ev {
        LiveEvent::Waiting(msg) => {
            if msg != last_wait {
                println!("{msg}... (Ctrl+C to cancel)");
                last_wait = msg;
            }
        }
        LiveEvent::Started {
            session_dir,
            devices,
            processes,
        } => {
            let names: Vec<String> = processes.iter().map(|p| format!("{} ({})", p.name, p.pid)).collect();
            println!("capturing {}", names.join(", "));
            println!("adapter(s): {}", devices.join(", "));
            println!("saving to {}", session_dir.display());
            println!("press Ctrl+C to stop\n");
        }
        LiveEvent::Warning(msg) => eprintln!("warning: {msg}"),
        LiveEvent::Status(s) => {
            let drop_note = if s.dropped > 0 {
                format!("  DROPPED {} (expect gaps)", s.dropped)
            } else {
                String::new()
            };
            println!(
                "[{:>5}s] sockets {} tcp/{} udp | flows {} | in {} | out {} | gaps {}{drop_note}",
                s.elapsed.as_secs(),
                s.game_tcp,
                s.game_udp,
                s.stats.flows_opened,
                human(s.stats.bytes_in),
                human(s.stats.bytes_out),
                human(s.stats.gap_bytes),
            );
        }
        LiveEvent::Finished { session_dir, stats } => {
            println!(
                "\nstopped. {} packets, {} flows, {} in, {} out, {} lost to gaps",
                stats.packets_matched,
                stats.flows_opened,
                human(stats.bytes_in),
                human(stats.bytes_out),
                human(stats.gap_bytes)
            );
            println!("session: {}", session_dir.display());
            println!("next: packet-lab chunks \"{}\"", session_dir.join("flows").join("<flow>").display());
        }
    })?;
    Ok(())
}

fn replay(pcap: &Path, out: &Path, mut local_ips: Vec<IpAddr>) -> Result<()> {
    if local_ips.is_empty() {
        local_ips = session_local_ips(pcap);
    }
    let file = File::open(pcap).with_context(|| format!("opening {}", pcap.display()))?;
    let mut reader = PcapReader::new(BufReader::new(file))?;
    let linktype = reader.linktype() as i32;

    let stem = pcap
        .parent()
        .and_then(|p| p.file_name())
        .or(pcap.file_stem())
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "pcap".into());
    let dir = out.join(format!(
        "replay-{stem}-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs()
    ));
    let rec = Recorder::create(
        &dir,
        &json!({ "replay_of": pcap.display().to_string(), "local_ips": local_ips.iter().map(|i| i.to_string()).collect::<Vec<_>>() }),
    )?;
    let mut pipeline = Pipeline::new(
        Matcher::Any {
            local_ips: local_ips.into_iter().collect::<HashSet<_>>(),
        },
        rec,
    );

    let mut last_ts = 0;
    let mut skipped = 0u64;
    while let Some((ts, frame)) = reader.next_packet()? {
        last_ts = ts;
        match decode::link_payload(linktype, &frame) {
            Some(ip) => pipeline.ingest(ts, ip)?,
            None => skipped += 1,
        }
    }
    let stats = pipeline.finish(last_ts + 1)?;
    println!(
        "{} packets ({} non-IP skipped), {} flows, {} in, {} out, {} lost to gaps",
        stats.packets_seen,
        skipped,
        stats.flows_opened,
        human(stats.bytes_in),
        human(stats.bytes_out),
        human(stats.gap_bytes)
    );
    println!("session: {}", dir.display());
    Ok(())
}

/// Our IPs from a session.json next to the pcap, if there is one.
fn session_local_ips(pcap: &Path) -> Vec<IpAddr> {
    let Some(meta) = pcap
        .parent()
        .map(|d| d.join("session.json"))
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
    else {
        return Vec::new();
    };
    meta["local_ips"]
        .as_array()
        .map(|a| a.iter().filter_map(|v| v.as_str()?.parse().ok()).collect())
        .unwrap_or_default()
}

fn chunks(flow: &Path, side: Option<SideArg>, limit: usize, bytes: usize) -> Result<()> {
    let index = BufReader::new(
        File::open(flow.join("index.jsonl")).with_context(|| format!("{} is not a flow folder", flow.display()))?,
    );
    let mut files = [
        File::open(flow.join("in.bin"))?,
        File::open(flow.join("out.bin"))?,
    ];
    let mut t0: Option<u64> = None;
    let mut printed = 0;
    println!("{:>11}  {:<4} {:<5} {:>9} {:>7}  first bytes", "time (s)", "side", "kind", "pos", "len");
    for line in index.lines() {
        let rec: Value = serde_json::from_str(&line?)?;
        let rec_side = rec["side"].as_str().unwrap_or("");
        if side.is_some_and(|s| s.as_str() != rec_side) {
            continue;
        }
        let t = rec["t"].as_u64().unwrap_or(0);
        let rel = (t - *t0.get_or_insert(t)) as f64 / 1e6;
        let kind = rec["kind"].as_str().unwrap_or("?");
        match kind {
            "data" | "dgram" => {
                let pos = rec["pos"].as_u64().unwrap_or(0);
                let len = rec["len"].as_u64().unwrap_or(0) as usize;
                let f = &mut files[if rec_side == "in" { 0 } else { 1 }];
                let mut buf = vec![0u8; len.min(bytes)];
                f.seek(SeekFrom::Start(pos))?;
                f.read_exact(&mut buf)?;
                let more = if len > bytes { " …" } else { "" };
                println!(
                    "{rel:>11.3}  {rec_side:<4} {kind:<5} {pos:>9} {len:>7}  {}{more}  |{}|",
                    hex(&buf),
                    ascii(&buf)
                );
            }
            "gap" => println!(
                "{rel:>11.3}  {rec_side:<4} GAP   {} bytes missing at stream offset {}",
                rec["len"],
                rec["off"]
            ),
            other => println!("{rel:>11.3}  {rec_side:<4} {}", other.to_uppercase()),
        }
        printed += 1;
        if printed >= limit {
            println!("... (stopped at --limit {limit})");
            break;
        }
    }
    Ok(())
}

fn dump(flow: &Path, side: SideArg, pos: u64, len: usize) -> Result<()> {
    let path = flow.join(format!("{}.bin", side.as_str()));
    let mut f = File::open(&path).with_context(|| format!("opening {}", path.display()))?;
    let size = f.metadata()?.len();
    if pos >= size {
        bail!("position {pos} is past the end of {} ({size} bytes)", path.display());
    }
    f.seek(SeekFrom::Start(pos))?;
    let mut buf = vec![0u8; len.min((size - pos) as usize)];
    f.read_exact(&mut buf)?;
    for (i, row) in buf.chunks(16).enumerate() {
        println!("{:08x}  {:<48}  |{}|", pos + i as u64 * 16, hex(row), ascii(row));
    }
    Ok(())
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect::<Vec<_>>().join(" ")
}

fn ascii(b: &[u8]) -> String {
    b.iter()
        .map(|&c| if c.is_ascii_graphic() || c == b' ' { c as char } else { '.' })
        .collect()
}

fn human(n: u64) -> String {
    match n {
        0..1024 => format!("{n} B"),
        1024..1_048_576 => format!("{:.1} KB", n as f64 / 1024.0),
        _ => format!("{:.1} MB", n as f64 / 1_048_576.0),
    }
}
