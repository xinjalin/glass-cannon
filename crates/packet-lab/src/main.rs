//! packet-lab: capture and inspect the game's network traffic for reverse-engineering.

use std::collections::{HashMap, HashSet};
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
use meter_core::recorder::{Recorder, Side};
use meter_core::session::{self, FlowMessages};
use meter_core::damage::DamageEvent;
use meter_core::entities::Registry;
use meter_core::meter::{Meter, Scope, Snapshot};
use meter_core::{damage, decode, protocol};

mod gamedata;
mod skill_details;
mod dungeons;
mod pets;
mod item_icons;
mod theostones;

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
    /// Count message types (opcodes) in the game stream, after unpacking bundles
    Opcodes {
        /// A session folder (the game connection is picked automatically) or a flow folder
        path: PathBuf,
        #[arg(long, value_enum, default_value = "in")]
        side: SideArg,
    },
    /// List damage events (opcode 0x3804) and total them per attacker
    Damage {
        /// A session folder or a flow folder
        path: PathBuf,
        /// Only your own hits (your character is detected automatically)
        #[arg(long, conflicts_with = "attacker")]
        mine: bool,
        /// Only events from this attacker entity ID
        #[arg(long)]
        attacker: Option<u64>,
        /// Only events on this target entity ID
        #[arg(long)]
        target: Option<u64>,
        #[arg(long)]
        skill: Option<u32>,
        /// Only events an entity aims at itself (left out otherwise: buffs, self-heals)
        #[arg(long)]
        self_target: bool,
        /// Maximum events to list (the per-attacker summary always covers everything)
        #[arg(long, default_value_t = 50)]
        limit: usize,
    },
    /// Live DPS meter in the terminal (Ctrl+C to stop)
    Live {
        /// Substring of the game's executable name
        #[arg(short, long, default_value = "AION2")]
        process: String,
        /// Also save the session to --out
        #[arg(long)]
        record: bool,
        #[arg(short, long, default_value = "captures")]
        out: PathBuf,
        /// Seconds without player damage that end a fight
        #[arg(long, default_value_t = 15)]
        idle: u64,
        /// Count every player fighting nearby, not just your targets
        #[arg(long)]
        everyone: bool,
        /// Count only your party or group (targets any of you hit)
        #[arg(long, conflicts_with = "everyone")]
        party: bool,
        /// Stop automatically after this many seconds
        #[arg(long)]
        seconds: Option<u64>,
    },
    /// List buff and debuff spans in a recorded session (who, what, from whom, how long)
    Buffs {
        /// A session folder or a flow folder
        path: PathBuf,
        /// Only effects on this entity
        #[arg(long)]
        on: Option<u64>,
    },
    /// Run the meter over a recorded session and print each fight
    Meter {
        /// A session folder or a flow folder
        path: PathBuf,
        #[arg(long, default_value_t = 15)]
        idle: u64,
        /// Count every player fighting nearby, not just your targets
        #[arg(long)]
        everyone: bool,
        /// Count only your party or group (targets any of you hit)
        #[arg(long, conflicts_with = "everyone")]
        party: bool,
        /// Print the final snapshot as JSON (what the overlay receives)
        #[arg(long)]
        json: bool,
        /// Print the last fight's analysis as JSON (what the analysis window receives)
        #[arg(long, conflicts_with = "json")]
        analysis: bool,
    },
    /// Download skill, mob/boss and class data for the overlay (from the GPL-3.0 NOIA2 project)
    #[command(alias = "import-skills")]
    UpdateData {
        /// Language of the names: en, ko, zh-TW or zh-CN
        #[arg(long, default_value = "en")]
        lang: String,
        /// Names only, no icons
        #[arg(long)]
        no_icons: bool,
        /// Where to save (default: a `gamedata` folder next to this program)
        #[arg(long)]
        out: Option<PathBuf>,
        /// Use the upstream commits pinned in this file (scripts/gamedata-pins.json)
        #[arg(long)]
        pins: Option<PathBuf>,
        /// Move the pins to the latest upstream commits first
        #[arg(long, requires = "pins")]
        bump: bool,
    },
    /// Build gamedata/skill-details.json (tooltips: cooldowns, costs, descriptions,
    /// specialties) from a scrape of aion2.app's skill database
    SkillDetails {
        /// The scrape: a JSON object of skills by ID, as aion2.app's pages carry them
        input: PathBuf,
        #[arg(long, default_value = "gamedata/skill-details.json")]
        out: PathBuf,
        /// When the scrape was made (recorded in the file)
        #[arg(long)]
        scraped: String,
    },
    /// Build gamedata/theostones.json (Theostone tooltips: grade, effect, cooldown) from a
    /// scraped item database
    Theostones {
        /// The scrape: a JSON object of items by ID, each with its in-game description
        items: PathBuf,
        /// Our skill names, to match Theostones to the skill IDs the game sends
        #[arg(long, default_value = "gamedata/skills.json")]
        skills: PathBuf,
        #[arg(long, default_value = "gamedata/theostones.json")]
        out: PathBuf,
        /// When the scrape was made (recorded in the file)
        #[arg(long)]
        scraped: String,
    },
    /// Build gamedata/dungeons.json (which dungeon and mode each mob type is in, for the
    /// history's dungeon runs) from a scraped monster and dungeon database
    Dungeons {
        /// The scrape's monsters: a JSON object by mob type ID
        monsters: PathBuf,
        /// The scrape's dungeons: a JSON object by dungeon ID
        dungeons: PathBuf,
        #[arg(long, default_value = "gamedata/dungeons.json")]
        out: PathBuf,
        /// When the scrape was made (recorded in the file)
        #[arg(long)]
        scraped: String,
    },
    /// Build gamedata/pets.json (pet names and genus by ID, for the pet soul ticker) from a
    /// scraped pet database
    Pets {
        /// The scrape's pets: a JSON object by pet ID
        pets: PathBuf,
        #[arg(long, default_value = "gamedata/pets.json")]
        out: PathBuf,
        /// When the scrape was made (recorded in the file)
        #[arg(long)]
        scraped: String,
    },
    /// Download icons for consumables' buffs (scrolls, potions, food) from NCSOFT's asset
    /// server into gamedata, matched to skills by name through a scraped item database
    ItemIcons {
        /// The scrape's items: a JSON object by item ID, each with its icon file name
        items: PathBuf,
        /// The game data folder (its skills.json is read; icons and item-icons.json go here)
        #[arg(long, default_value = "gamedata")]
        dir: PathBuf,
        /// When this is run (recorded in item-icons.json)
        #[arg(long)]
        fetched: String,
    },
    /// Show who was identified: your character, named players, and mob types
    Entities {
        /// A session folder or a flow folder
        path: PathBuf,
    },
    /// Find which message types contain a number (as varint, u16 or u32)
    Find {
        /// A session folder or a flow folder
        path: PathBuf,
        value: u64,
        #[arg(long, value_enum, default_value = "in")]
        side: SideArg,
    },
    /// Check HP updates (0x8d00) against the damage that lands on each entity
    Hp {
        /// A session folder or a flow folder
        path: PathBuf,
        /// List this entity's HP samples and the hits on it in order
        #[arg(long)]
        on: Option<u64>,
        /// Maximum lines to list with --on
        #[arg(long, default_value_t = 200)]
        limit: usize,
    },
    /// List every message of one type with its time and payload in hex
    Messages {
        /// A session folder or a flow folder
        path: PathBuf,
        /// Opcode in hex, e.g. 8d00, or "all" for every message (with its opcode)
        opcode: String,
        #[arg(long, value_enum, default_value = "in")]
        side: SideArg,
        /// Maximum messages to list
        #[arg(long, default_value_t = 200)]
        limit: usize,
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
        self.side().as_str()
    }

    fn side(self) -> Side {
        match self {
            SideArg::In => Side::In,
            SideArg::Out => Side::Out,
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
            record: true,
            decode: false,
        }),
        Cmd::Replay { pcap, out, local_ip } => replay(&pcap, &out, local_ip),
        Cmd::Chunks {
            flow,
            side,
            limit,
            bytes,
        } => chunks(&flow, side, limit, bytes),
        Cmd::Opcodes { path, side } => opcodes(&path, side),
        Cmd::Damage {
            path,
            mine,
            attacker,
            target,
            skill,
            self_target,
            limit,
        } => damage_cmd(
            &path,
            DamageFilter {
                mine,
                attacker,
                target,
                skill,
                self_target,
            },
            limit,
        ),
        Cmd::Entities { path } => entities_cmd(&path),
        Cmd::UpdateData {
            lang,
            no_icons,
            out,
            pins,
            bump,
        } => {
            let dir = match out {
                Some(d) => d,
                None => gamedata::default_dir()?,
            };
            gamedata::run(&dir, &lang, !no_icons, pins.as_deref(), bump)
        }
        Cmd::SkillDetails { input, out, scraped } => skill_details::run(&input, &out, &scraped),
        Cmd::ItemIcons { items, dir, fetched } => item_icons::run(&items, &dir, &fetched),
        Cmd::Dungeons { monsters, dungeons, out, scraped } => dungeons::run(&monsters, &dungeons, &out, &scraped),
        Cmd::Pets { pets, out, scraped } => pets::run(&pets, &out, &scraped),
        Cmd::Theostones { items, skills, out, scraped } => theostones::run(&items, &skills, &out, &scraped),
        Cmd::Live {
            process,
            record,
            out,
            idle,
            everyone,
            party,
            seconds,
        } => live_cmd(process, record, out, idle, scope(everyone, party), seconds),
        Cmd::Meter {
            path,
            idle,
            everyone,
            party,
            json,
            analysis,
        } => meter_cmd(&path, idle, scope(everyone, party), json, analysis),
        Cmd::Buffs { path, on } => buffs_cmd(&path, on),
        Cmd::Find { path, value, side } => find(&path, value, side),
        Cmd::Hp { path, on, limit } => hp_cmd(&path, on, limit),
        Cmd::Messages {
            path,
            opcode,
            side,
            limit,
        } => messages_cmd(&path, &opcode, side, limit),
        Cmd::Dump { flow, side, pos, len } => dump(&flow, side, pos, len),
    }
}

fn load_messages(path: &Path, side: SideArg) -> Result<(PathBuf, FlowMessages)> {
    let flow = session::resolve_flow(path)?;
    let m = session::read_messages(&flow, side.side())?;
    eprintln!(
        "{}: {} messages ({} gaps, {} framing resyncs, {} bad bundles)",
        flow.file_name().unwrap_or_default().to_string_lossy(),
        m.messages.len(),
        m.gaps,
        m.resyncs,
        m.bad_bundles
    );
    Ok((flow, m))
}

fn opcodes(path: &Path, side: SideArg) -> Result<()> {
    let (_, m) = load_messages(path, side)?;
    let mut by_op: HashMap<u16, (u64, u64)> = HashMap::new();
    for msg in &m.messages {
        let e = by_op.entry(msg.opcode).or_default();
        e.0 += 1;
        e.1 += msg.payload.len() as u64;
    }
    let mut rows: Vec<_> = by_op.into_iter().collect();
    rows.sort_by_key(|r| std::cmp::Reverse(r.1 .0));
    println!("{:>8} {:>8} {:>9}", "opcode", "count", "avg size");
    for (op, (count, bytes)) in rows {
        println!("{op:#08x} {count:>8} {:>9.1}", bytes as f64 / count as f64);
    }
    Ok(())
}

struct DamageFilter {
    mine: bool,
    attacker: Option<u64>,
    target: Option<u64>,
    skill: Option<u32>,
    self_target: bool,
}

struct Scan {
    registry: Registry,
    /// (capture time µs, event)
    events: Vec<(u64, DamageEvent)>,
    parsed: u64,
    partial: u64,
    unreadable: u64,
}

/// Runs every message through the entity registry in order and collects damage events.
/// Names often arrive after the first hits, so labels use what's known at the end.
fn scan_session(path: &Path) -> Result<Scan> {
    let (_, m) = load_messages(path, SideArg::In)?;
    let mut s = Scan {
        registry: Registry::default(),
        events: Vec::new(),
        parsed: 0,
        partial: 0,
        unreadable: 0,
    };
    for msg in &m.messages {
        if msg.opcode != protocol::OPCODE_DAMAGE {
            s.registry.observe(msg.opcode, &msg.payload);
            continue;
        }
        match damage::parse(&msg.payload) {
            Some(ev) => {
                s.parsed += 1;
                if !ev.fully_parsed {
                    s.partial += 1;
                }
                s.events.push((msg.ts_us, ev));
            }
            None => s.unreadable += 1,
        }
    }
    Ok(s)
}

fn describe_self(r: &Registry) -> String {
    match (r.self_id(), r.self_name()) {
        (Some(id), Some(name)) => format!("you: {name} (entity {id})"),
        (Some(id), None) => format!("you: entity {id} (name arrives on the next zone change or login)"),
        _ => "you: not identified in this capture".into(),
    }
}

/// Fits a label in a column, counting characters rather than bytes.
fn col(s: &str, width: usize) -> String {
    let n = s.chars().count();
    if n > width {
        let mut t: String = s.chars().take(width - 1).collect();
        t.push('…');
        t
    } else {
        format!("{s}{}", " ".repeat(width - n))
    }
}

fn damage_cmd(path: &Path, filter: DamageFilter, limit: usize) -> Result<()> {
    let Scan {
        registry: reg,
        events,
        parsed,
        partial,
        unreadable: failed,
    } = scan_session(path)?;
    println!("{}\n", describe_self(&reg));
    if filter.mine && reg.self_id().is_none() {
        bail!("--mine needs your character, which wasn't identified in this capture");
    }
    let t0 = events.first().map(|(t, _)| *t).unwrap_or(0);

    struct Totals {
        hits: u64,
        total: u64,
        max: u64,
        first: u64,
        last: u64,
    }
    let mut per_attacker: HashMap<u64, Totals> = HashMap::new();
    let mut listed = 0usize;

    println!(
        "{:>9}  {} -> {} {:>10} {:>4} {:>8}  flags",
        "time (s)",
        col("attacker", 18),
        col("target", 18),
        "skill",
        "hit",
        "damage"
    );
    for (ts, ev) in &events {
        let Some(dmg) = ev.damage else { continue };
        if ev.is_self() != filter.self_target
            || (filter.mine && !reg.is_self(ev.attacker))
            || filter.attacker.is_some_and(|a| a != ev.attacker)
            || filter.target.is_some_and(|t| t != ev.target)
            || filter.skill.is_some_and(|s| s != ev.skill)
        {
            continue;
        }
        let t = per_attacker.entry(ev.attacker).or_insert(Totals {
            hits: 0,
            total: 0,
            max: 0,
            first: *ts,
            last: *ts,
        });
        t.hits += 1;
        t.total += dmg;
        t.max = t.max.max(dmg);
        t.last = *ts;
        if listed < limit {
            listed += 1;
            println!(
                "{:>9.3}  {} -> {} {:>10} {:>4} {:>8}  {:#06x}{}",
                (ts - t0) as f64 / 1e6,
                col(&reg.label(ev.attacker), 18),
                col(&reg.label(ev.target), 18),
                ev.skill,
                ev.hit,
                dmg,
                ev.flags,
                if ev.fully_parsed { "" } else { " (partly decoded)" }
            );
        }
    }
    if listed == limit {
        println!("... (stopped listing at --limit {limit})");
    }

    println!("\n0x3804 messages: {parsed} parsed, {partial} only partly decoded, {failed} unreadable\n");
    let mut rows: Vec<_> = per_attacker.into_iter().collect();
    rows.sort_by_key(|r| std::cmp::Reverse(r.1.total));
    println!(
        "{} {:>8} {:>6} {:>11} {:>8} {:>9} {:>9}",
        col("attacker", 20),
        "entity",
        "hits",
        "total",
        "max",
        "active s",
        "dps"
    );
    for (id, t) in rows.iter().take(15) {
        let secs = ((t.last - t.first) as f64 / 1e6).max(1.0);
        let marker = if reg.is_self(*id) { "▶ " } else { "  " };
        println!(
            "{marker}{} {id:>8} {:>6} {:>11} {:>8} {:>9.1} {:>9.0}",
            col(&reg.label(*id), 18),
            t.hits,
            t.total,
            t.max,
            secs,
            t.total as f64 / secs
        );
    }
    Ok(())
}

fn scope(everyone: bool, party: bool) -> Scope {
    match (everyone, party) {
        (true, _) => Scope::Everyone,
        (_, true) => Scope::Party,
        _ => Scope::Mine,
    }
}

fn new_meter(idle_s: u64, scope: Scope) -> Meter {
    let mut m = Meter::new(idle_s * 1_000_000);
    m.set_scope(scope);
    m.set_boss_types(boss_types());
    m
}

/// Boss mob types from `gamedata/npcs.json` (in or above the current folder), so fights
/// end on a boss's death as they do in the app. None when it isn't there.
fn boss_types() -> Vec<u32> {
    let found = std::env::current_dir()
        .ok()
        .and_then(|d| d.ancestors().map(|a| a.join("gamedata/npcs.json")).find(|p| p.exists()));
    let Some(text) = found.and_then(|p| std::fs::read_to_string(p).ok()) else { return Vec::new() };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) else { return Vec::new() };
    json["npcs"]
        .as_object()
        .map(|npcs| {
            npcs.iter()
                // Training dummies are marked bosses but aren't fought as one (as in the app).
                .filter(|(_, n)| n["boss"].as_bool() == Some(true) && n["dummy"].as_bool() != Some(true))
                .filter_map(|(id, _)| id.parse().ok())
                .collect()
        })
        .unwrap_or_default()
}

fn render_meter(snap: &Snapshot) -> String {
    let mut out = String::new();
    let you = match (&snap.self_name, snap.self_id) {
        (Some(n), _) => n.clone(),
        (None, Some(id)) => format!("entity {id}"),
        _ => "not identified yet".into(),
    };
    out.push_str(&format!("you: {you}\n"));
    let Some(e) = &snap.encounter else {
        out.push_str("\nno fight yet — hit something\n");
        return out;
    };
    out.push_str(&format!(
        "\n{} {:>5.0}s  {}{}  total {}  ({}/s)\n\n",
        if e.active { "● fighting".to_string() } else { format!("■ ended ({})", e.end.map_or("?".into(), |x| format!("{x:?}").to_lowercase())) },
        e.duration_s,
        e.target.as_deref().unwrap_or("?"),
        if e.target_count > 1 {
            format!(" +{} more", e.target_count - 1)
        } else {
            String::new()
        },
        human_num(e.total_damage),
        human_num(e.total_dps as u64)
    ));
    for d in &e.deaths {
        out.push_str(&format!(
            "☠ {} died at {:.0}s (killed by {} with {}): {:.1}s to death, {} direct, {} over time, {} healed
",
            d.name,
            d.at_s,
            d.killer.as_deref().or(d.killer_type.map(|_| "a mob")).unwrap_or("?"),
            d.skill,
            d.time_to_death_s,
            human_num(d.direct),
            human_num(d.over_time),
            human_num(d.healed),
        ));
    }
    out.push_str(&format!(
        "   {} {:>5} {:>9} {:>9} {:>6} {:>5} {:>8}\n",
        col("player", 18),
        "class",
        "dps",
        "damage",
        "share",
        "hits",
        "max hit"
    ));
    for (i, p) in e.players.iter().take(12).enumerate() {
        let class = p.class_code.map(|c| c.to_string()).unwrap_or_default();
        out.push_str(&format!(
            "{:>2}{}{} {:>5} {:>9} {:>9} {:>5.1}% {:>5} {:>8}\n",
            i + 1,
            if p.is_self { "▶" } else { " " },
            col(&p.name, 18),
            class,
            human_num(p.dps as u64),
            human_num(p.damage),
            p.share * 100.0,
            p.hits,
            human_num(p.max_hit)
        ));
    }
    if let Some(me) = e.players.iter().find(|p| p.is_self) {
        let o = &me.outcomes;
        out.push_str(&format!(
            "\nyou: {} casts, {} hits ({} counting multi-hit parts)\n",
            o.casts, me.hits, o.multi_hits
        ));
        out.push_str(&format!("   {}\n", outcome_line(o, me.hits)));
        out.push_str(&format!(
            "\n   {:>10}  {:>9} {:>6} {:>5} {:>5} {:>6} {:>6}  {:>6} {:>6} {:>6} {:>6} {:>6}  {:>7} {:>7}\n",
            "skill", "damage", "share", "casts", "hits", "multi", "crit", "double", "perf", "back", "front", "", "min", "max"
        ));
        for s in me.skills.iter() {
            let o = &s.outcomes;
            let pct = |n: u64| format!("{:5.1}%", n as f64 * 100.0 / s.hits.max(1) as f64);
            out.push_str(&format!(
                "   {:>10}  {:>9} {:>5.1}% {:>5} {:>5} {:>6} {:>6}  {:>6} {:>6} {:>6} {:>6} {:>6}  {:>7} {:>7}\n",
                s.skill,
                human_num(s.damage),
                s.share * 100.0,
                o.casts,
                s.hits,
                pct(o.multi),
                pct(o.crits),
                pct(o.double),
                pct(o.perfect),
                pct(o.back),
                pct(o.front),
                "",
                human_num(s.min),
                human_num(s.max)
            ));
        }
    }
    out
}

/// "crit 46.0%  back 37.7%  front 25.7%  perfect 19.2%  double 4.9%", out of `hits`.
fn outcome_line(o: &meter_core::meter::Outcomes, hits: u64) -> String {
    let pct = |n: u64| n as f64 * 100.0 / hits.max(1) as f64;
    format!(
        "crit {:.1}%  back {:.1}%  front {:.1}%  perfect {:.1}%  double {:.1}%  multi {:.1}%",
        pct(o.crits),
        pct(o.back),
        pct(o.front),
        pct(o.perfect),
        pct(o.double),
        pct(o.multi)
    )
}

fn human_num(n: u64) -> String {
    match n {
        0..10_000 => n.to_string(),
        10_000..1_000_000 => format!("{:.1}K", n as f64 / 1e3),
        _ => format!("{:.2}M", n as f64 / 1e6),
    }
}

fn live_cmd(process: String, record: bool, out: PathBuf, idle: u64, scope: Scope, seconds: Option<u64>) -> Result<()> {
    let stop = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&stop);
    ctrlc::set_handler(move || flag.store(true, Ordering::Relaxed)).context("installing Ctrl+C handler")?;

    let opts = LiveOptions {
        process,
        out_dir: out,
        devices: Vec::new(),
        duration: seconds.map(Duration::from_secs),
        record,
        decode: true,
    };
    let mut meter = new_meter(idle, scope);
    let mut last_render = std::time::Instant::now();
    let mut status = String::from("starting…");
    let mut last_wait = String::new();
    live::run(&opts, &stop, |ev| {
        match ev {
            LiveEvent::Waiting(msg) => {
                if msg != last_wait {
                    println!("{msg}... (Ctrl+C to cancel)");
                    last_wait = msg;
                }
                return;
            }
            LiveEvent::Started { devices, .. } => status = format!("capturing on {}", devices.join(", ")),
            LiveEvent::Warning(msg) => status = format!("warning: {msg}"),
            LiveEvent::Status(s) if s.dropped > 0 => {
                status = format!("capturing — {} packets dropped (numbers may be low)", s.dropped)
            }
            LiveEvent::Status(_) => {}
            LiveEvent::Messages(msgs) => {
                for m in &msgs {
                    meter.feed(m.ts_us, m.opcode, &m.payload);
                }
            }
            LiveEvent::Finished { .. } => return,
        }
        meter.tick(live::now_us());
        if last_render.elapsed() >= Duration::from_millis(500) {
            last_render = std::time::Instant::now();
            // Clear the screen and draw from the top.
            print!("\x1b[2J\x1b[H{}\n{status}  (Ctrl+C to stop)\n", render_meter(&meter.snapshot()));
        }
    })?;
    println!("\n{}", render_meter(&meter.snapshot()));
    Ok(())
}

fn meter_cmd(path: &Path, idle: u64, scope: Scope, json: bool, analysis: bool) -> Result<()> {
    let (_, m) = load_messages(path, SideArg::In)?;
    let mut meter = new_meter(idle, scope);
    let mut seen = 0;
    if analysis {
        for msg in &m.messages {
            meter.tick(msg.ts_us);
            meter.feed(msg.ts_us, msg.opcode, &msg.payload);
        }
        let now = m.messages.last().map_or(0, |msg| msg.ts_us);
        println!("{}", serde_json::to_string_pretty(&meter.analysis(now))?);
        return Ok(());
    }
    if json {
        for msg in &m.messages {
            meter.feed(msg.ts_us, msg.opcode, &msg.payload);
        }
        println!("{}", serde_json::to_string_pretty(&meter.snapshot())?);
        return Ok(());
    }
    let mut cubes = Vec::new();
    for msg in &m.messages {
        meter.tick(msg.ts_us);
        if meter.encounters_completed() > seen {
            seen = meter.encounters_completed();
            println!("── fight {seen} ──\n{}", render_meter(&meter.snapshot()));
        }
        meter.feed(msg.ts_us, msg.opcode, &msg.payload);
        for i in meter.take_instances() {
            println!("{}", render_stay(&i));
        }
        for g in meter.take_soul_gains() {
            let next = match g.next_level_souls {
                _ if !g.known => "progress not known yet".to_string(),
                Some(n) => format!("{}/{n} to level {}", g.souls, g.level + 1),
                None => "top level".to_string(),
            };
            println!("◆ pet {} soul +{} · {next}", g.pet, g.gained);
        }
        if matches!(msg.opcode, meter_core::cubes::OPCODE_OBJECT_SPAWN | meter_core::cubes::OPCODE_OBJECT_DESPAWN) {
            let now = meter.cubes(msg.ts_us);
            if now != cubes {
                for c in &now {
                    let place = match (c.distance_m, c.bearing, c.rise_m) {
                        (Some(d), Some(b), Some(r)) => format!("{d} m at {b}°, {r:+} m"),
                        _ => "where you are not known".into(),
                    };
                    println!("◇ cube {:?}{} · {place}", c.look, if c.opened { " (opened)" } else { "" });
                }
                if now.is_empty() {
                    println!("◇ no cubes in view");
                }
                cubes = now;
            }
        }
    }
    let snap = meter.snapshot();
    if snap.encounter.as_ref().is_some_and(|e| e.active) {
        println!("── fight {} (still going when the capture ended) ──", seen + 1);
    }
    println!("{}", render_meter(&snap));
    Ok(())
}

/// A map stay as it changed: when it started and completed, and each stage's start (seconds
/// after you entered).
fn render_stay(i: &meter_core::dungeon::Instance) -> String {
    let after = |t: u64| format!("+{:.0}s", t.saturating_sub(i.entered_us) as f64 / 1e6);
    let stages: Vec<String> = i.stages.iter().take_while(|t| **t != 0).map(|t| after(*t)).collect();
    format!(
        "◆ map {} entered {}  started {}  stages [{}]  completed {}",
        i.map,
        i.entered_us,
        i.started_us.map_or("-".into(), after),
        stages.join(" "),
        i.completed_us.map_or("-".into(), after),
    )
}

fn buffs_cmd(path: &Path, on: Option<u64>) -> Result<()> {
    let (_, m) = load_messages(path, SideArg::In)?;
    let mut effects = meter_core::buffs::EffectTracker::default();
    for msg in &m.messages {
        effects.feed(msg.ts_us, msg.opcode, &msg.payload);
    }
    let Some(t0) = m.messages.first().map(|msg| msg.ts_us) else {
        bail!("no messages in this session");
    };
    let t1 = m.messages.last().map_or(t0, |msg| msg.ts_us);
    let spans: Vec<_> = effects
        .spans(t0, t1)
        .into_iter()
        .filter(|s| on.is_none_or(|id| s.target == id))
        .collect();
    println!("{} effect spans{}", spans.len(), on.map(|id| format!(" on {id}")).unwrap_or_default());
    println!("{:>8} {:>8} {:>7}  {:>8} {:>10} {:>10} {:>8}", "start", "end", "length", "target", "skill", "effect", "source");
    let secs = |us: u64| format!("{:.2}s", us.saturating_sub(t0) as f64 / 1e6);
    for s in spans {
        let (end, len) = match s.end_us {
            Some(e) => (secs(e), format!("{:.2}s", (e - s.start_us) as f64 / 1e6)),
            None => ("(open)".into(), String::new()),
        };
        println!(
            "{:>8} {:>8} {:>7}  {:>8} {:>10} {:>10} {:>8}",
            secs(s.start_us),
            end,
            len,
            s.target,
            s.skill.map(|k| k.to_string()).unwrap_or_else(|| "-".into()),
            s.effect,
            s.source
        );
    }
    Ok(())
}

fn entities_cmd(path: &Path) -> Result<()> {
    let Scan {
        registry: reg,
        events,
        ..
    } = scan_session(path)?;
    println!("{}\n", describe_self(&reg));

    let mut damage_by: HashMap<u64, u64> = HashMap::new();
    for (_, ev) in &events {
        if let (Some(d), false) = (ev.damage, ev.is_self()) {
            *damage_by.entry(ev.attacker).or_default() += d;
        }
    }

    let mut players: Vec<_> = reg.players().collect();
    players.sort_by_cached_key(|p| p.1.to_lowercase());
    println!("players named: {}", players.len());
    for (id, name) in &players {
        let dealt = damage_by.get(id).copied().unwrap_or(0);
        println!("  {} entity {id:>6}   damage dealt {dealt:>10}", col(name, 20));
    }

    let mut by_type: HashMap<u32, u32> = HashMap::new();
    for (_, t) in reg.mobs() {
        *by_type.entry(t).or_default() += 1;
    }
    let mut types: Vec<_> = by_type.into_iter().collect();
    types.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    println!("\nmobs seen: {} ({} types)", reg.mobs().count(), types.len());
    for (t, n) in types.iter().take(20) {
        println!("  type {t:>9}  ×{n}");
    }

    let unnamed = damage_by
        .keys()
        .filter(|id| !reg.is_self(**id) && reg.player_name(**id).is_none() && reg.mob_type(**id).is_none())
        .count();
    println!("\nattackers with no spawn seen (were already nearby when capture started): {unnamed}");
    Ok(())
}

fn find(path: &Path, value: u64, side: SideArg) -> Result<()> {
    let (_, m) = load_messages(path, side)?;
    let mut encodings: Vec<(&str, Vec<u8>)> = vec![("varint", encode_varint(value))];
    if value <= u16::MAX as u64 {
        encodings.push(("u16", (value as u16).to_le_bytes().to_vec()));
    }
    if value <= u32::MAX as u64 {
        encodings.push(("u32", (value as u32).to_le_bytes().to_vec()));
    }

    let mut hits: HashMap<(u16, &str, usize), u64> = HashMap::new();
    for msg in &m.messages {
        for (name, pat) in &encodings {
            let mut from = 0;
            while let Some(i) = find_bytes(&msg.payload[from..], pat) {
                *hits.entry((msg.opcode, *name, from + i)).or_default() += 1;
                from += i + 1;
            }
        }
    }
    if hits.is_empty() {
        println!("{value} not found in any message");
        return Ok(());
    }
    let mut rows: Vec<_> = hits.into_iter().collect();
    rows.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    println!("{:>8} {:>7} {:>8} {:>6}", "opcode", "as", "offset", "count");
    for ((op, name, off), count) in rows.into_iter().take(25) {
        println!("{op:#08x} {name:>7} {off:>8} {count:>6}");
    }
    println!("(offset = bytes after the opcode)");
    Ok(())
}

fn encode_varint(mut v: u64) -> Vec<u8> {
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

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
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
            if let Some(dir) = session_dir {
                println!("saving to {}", dir.display());
            }
            println!("press Ctrl+C to stop\n");
        }
        LiveEvent::Warning(msg) => eprintln!("warning: {msg}"),
        LiveEvent::Messages(_) => {}
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
            if let Some(dir) = session_dir {
                println!("session: {}", dir.display());
                println!("next: packet-lab damage \"{}\" --mine", dir.display());
            }
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
        Some(rec),
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

/// Walks the stream in order. Between two HP samples of an entity, its HP should fall by
/// the damage (0x3804 and 0x3805, heals excluded) that landed on it in between.
fn hp_cmd(path: &Path, on: Option<u64>, limit: usize) -> Result<()> {
    let (_, m) = load_messages(path, SideArg::In)?;
    let start = m.messages.first().map_or(0, |x| x.ts_us);
    #[derive(Default)]
    struct Track {
        samples: u64,
        last: Option<u64>,
        since: u64,
        hits_since: u64,
        exact: u64,
        rises: u64,
        other: u64,
        min: u64,
        max: u64,
        fell: u64,
        fell_damage: u64,
        at_ratio: u64,
        spawn_max: Option<u64>,
        zero_at: Option<u64>,
    }
    let mut tracks: HashMap<u64, Track> = HashMap::new();
    let mut listed = 0usize;
    for msg in &m.messages {
        let t = msg.ts_us.saturating_sub(start) as f64 / 1e6;
        let hit = match msg.opcode {
            protocol::OPCODE_DAMAGE => damage::parse(&msg.payload),
            protocol::OPCODE_DOT => damage::parse_dot(&msg.payload),
            meter_core::hp::OPCODE_MOB_SPAWN => {
                let Some((e, hp, max)) = meter_core::hp::parse_spawn_hp(&msg.payload) else { continue };
                if on == Some(e) && listed < limit {
                    println!("{t:9.3}  HP {hp:>9}  of {max} (spawn)");
                    listed += 1;
                }
                let tr = tracks.entry(e).or_insert(Track { min: u64::MAX, ..Default::default() });
                tr.spawn_max = Some(max);
                tr.last = Some(hp);
                tr.since = 0;
                tr.hits_since = 0;
                continue;
            }
            meter_core::hp::OPCODE_HP => {
                let Some(u) = meter_core::hp::parse_hp(&msg.payload) else { continue };
                let tr = tracks.entry(u.entity).or_insert(Track { min: u64::MAX, ..Default::default() });
                let verdict = match tr.last {
                    None => "first",
                    Some(prev) if prev.checked_sub(u.hp) == Some(tr.since) && tr.hits_since > 0 => {
                        tr.exact += 1;
                        "= damage"
                    }
                    Some(prev) if u.hp > prev => {
                        tr.rises += 1;
                        "rise"
                    }
                    Some(prev) if prev == u.hp && tr.hits_since == 0 => "same",
                    Some(prev) if u.hp > 1 && tr.hits_since > 0 => {
                        tr.fell += prev - u.hp;
                        tr.fell_damage += tr.since;
                        // Mobs and dummies lose 4.5 HP per point of damage shown.
                        let scaled = tr.since as f64 * 4.5;
                        if ((prev - u.hp) as f64 - scaled).abs() <= scaled * 0.005 {
                            tr.at_ratio += 1;
                        }
                        tr.other += 1;
                        "differs"
                    }
                    Some(_) => {
                        tr.other += 1;
                        "differs"
                    }
                };
                if on == Some(u.entity) && listed < limit {
                    let drop = tr.last.map_or(0, |p| p as i64 - u.hp as i64);
                    println!(
                        "{t:9.3}  HP {:>9}  fell {drop:>8}  damage since {:>8} ({} hits)  {verdict}",
                        u.hp, tr.since, tr.hits_since
                    );
                    listed += 1;
                }
                tr.samples += 1;
                tr.min = tr.min.min(u.hp);
                tr.max = tr.max.max(u.hp);
                if u.hp == 0 && tr.zero_at.is_none() {
                    tr.zero_at = Some(msg.ts_us.saturating_sub(start) / 1000);
                }
                tr.last = Some(u.hp);
                tr.since = 0;
                tr.hits_since = 0;
                continue;
            }
            _ => continue,
        };
        let Some(ev) = hit else { continue };
        let Some(dmg) = ev.damage else { continue };
        if ev.is_heal() {
            continue;
        }
        if let Some(tr) = tracks.get_mut(&ev.target) {
            tr.since += dmg;
            tr.hits_since += 1;
        }
        if on == Some(ev.target) && listed < limit {
            println!("{t:9.3}      {:>9}  hit by #{} skill {} for {dmg}", "", ev.attacker, ev.skill);
            listed += 1;
        }
    }
    if on.is_some() {
        return Ok(());
    }
    let mut rows: Vec<_> = tracks.into_iter().filter(|(_, t)| t.samples > 1).collect();
    rows.sort_by_key(|(_, t)| std::cmp::Reverse(t.samples));
    println!(
        "{:>8} {:>7} {:>9} {:>9} {:>9} {:>8} {:>6} {:>8} {:>6} {:>6}  died",
        "entity", "samples", "min", "max", "spawn max", "=damage", "rises", "differs", "×4.5", "ratio"
    );
    for (e, t) in rows {
        let died = t.zero_at.map_or(String::new(), |ms| format!("at {:.1}s", ms as f64 / 1000.0));
        let ratio = if t.fell_damage > 0 {
            format!("{:.3}", t.fell as f64 / t.fell_damage as f64)
        } else {
            String::new()
        };
        println!(
            "{e:>8} {:>7} {:>9} {:>9} {:>9} {:>8} {:>6} {:>8} {:>6} {ratio:>6}  {died}",
            t.samples,
            t.min,
            t.max,
            t.spawn_max.map_or(String::new(), |m| m.to_string()),
            t.exact, t.rises, t.other, t.at_ratio
        );
    }
    Ok(())
}

fn messages_cmd(path: &Path, opcode: &str, side: SideArg, limit: usize) -> Result<()> {
    let op = match opcode {
        "all" => None,
        _ => Some(
            u16::from_str_radix(opcode.trim_start_matches("0x"), 16)
                .with_context(|| format!("{opcode} is not a hex opcode"))?,
        ),
    };
    let (_, m) = load_messages(path, side)?;
    let start = m.messages.first().map_or(0, |x| x.ts_us);
    for (shown, msg) in m.messages.iter().filter(|x| op.is_none_or(|op| x.opcode == op)).enumerate() {
        if shown == limit {
            println!("…");
            break;
        }
        let t = msg.ts_us.saturating_sub(start) as f64 / 1e6;
        let tag = if op.is_none() { format!("{:04x}  ", msg.opcode) } else { String::new() };
        println!("{t:9.3}  {tag}{:>4}  {}", msg.payload.len(), hex(&msg.payload));
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
