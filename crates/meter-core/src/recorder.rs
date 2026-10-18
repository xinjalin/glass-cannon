//! On-disk layout of a capture session:
//!
//! ```text
//! <session>/
//!   session.json          what was captured, when, from which process and adapters
//!   raw.pcap              every matched IP packet (open in Wireshark)
//!   events.jsonl          flow opened/closed with per-direction stats
//!   flows/<id>-<proto>-<localport>-to-<remote>_<port>/
//!     out.bin / in.bin    reassembled bytes we sent / received
//!     index.jsonl         one line per chunk: time, side, kind, stream offset, file pos, length
//! ```

use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde_json::{json, Value};

use crate::pcapfile::{PcapWriter, LINKTYPE_RAW};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Side {
    /// Sent by the game client.
    Out,
    /// Received from the server.
    In,
}

impl Side {
    pub fn as_str(self) -> &'static str {
        match self {
            Side::Out => "out",
            Side::In => "in",
        }
    }
}

pub struct Recorder {
    dir: PathBuf,
    pcap: PcapWriter<BufWriter<File>>,
    events: BufWriter<File>,
}

impl Recorder {
    pub fn create(dir: &Path, meta: &Value) -> Result<Self> {
        fs::create_dir_all(dir.join("flows"))
            .with_context(|| format!("creating {}", dir.display()))?;
        fs::write(dir.join("session.json"), serde_json::to_vec_pretty(meta)?)?;
        let pcap = PcapWriter::new(
            BufWriter::new(File::create(dir.join("raw.pcap"))?),
            LINKTYPE_RAW,
        )?;
        let events = BufWriter::new(File::create(dir.join("events.jsonl"))?);
        Ok(Self {
            dir: dir.to_path_buf(),
            pcap,
            events,
        })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn packet(&mut self, ts_us: u64, ip: &[u8]) -> Result<()> {
        self.pcap.write(ts_us, ip)?;
        Ok(())
    }

    pub fn event(&mut self, v: &Value) -> Result<()> {
        write_line(&mut self.events, v)
    }

    pub fn open_flow(&mut self, id: u32, proto: &str, local: SocketAddr, remote: SocketAddr) -> Result<FlowFiles> {
        let name = format!(
            "{id:03}-{proto}-{}-to-{}_{}",
            local.port(),
            path_safe(remote.ip()),
            remote.port()
        );
        FlowFiles::create(self.dir.join("flows").join(name))
    }

    pub fn flush(&mut self) -> Result<()> {
        self.pcap.flush()?;
        self.events.flush()?;
        Ok(())
    }
}

pub struct FlowFiles {
    dir: PathBuf,
    out: BufWriter<File>,
    inn: BufWriter<File>,
    index: BufWriter<File>,
    out_pos: u64,
    in_pos: u64,
}

impl FlowFiles {
    fn create(dir: PathBuf) -> Result<Self> {
        fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
        Ok(Self {
            out: BufWriter::new(File::create(dir.join("out.bin"))?),
            inn: BufWriter::new(File::create(dir.join("in.bin"))?),
            index: BufWriter::new(File::create(dir.join("index.jsonl"))?),
            dir,
            out_pos: 0,
            in_pos: 0,
        })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Appends bytes to the side's file. `stream_off` is the TCP stream offset (None for UDP).
    pub fn data(&mut self, ts_us: u64, side: Side, kind: &str, stream_off: Option<u64>, bytes: &[u8]) -> Result<()> {
        let (w, pos) = match side {
            Side::Out => (&mut self.out, &mut self.out_pos),
            Side::In => (&mut self.inn, &mut self.in_pos),
        };
        w.write_all(bytes)?;
        let mut rec = json!({
            "t": ts_us,
            "side": side.as_str(),
            "kind": kind,
            "pos": *pos,
            "len": bytes.len(),
        });
        if let Some(off) = stream_off {
            rec["off"] = json!(off);
        }
        *pos += bytes.len() as u64;
        write_line(&mut self.index, &rec)
    }

    /// Records a non-data event (syn, fin, rst, gap) with extra fields.
    pub fn mark(&mut self, ts_us: u64, side: Side, kind: &str, extra: Value) -> Result<()> {
        let mut rec = json!({ "t": ts_us, "side": side.as_str(), "kind": kind });
        if let (Some(obj), Value::Object(extra)) = (rec.as_object_mut(), extra) {
            obj.extend(extra);
        }
        write_line(&mut self.index, &rec)
    }

    pub fn flush(&mut self) -> Result<()> {
        self.out.flush()?;
        self.inn.flush()?;
        self.index.flush()?;
        Ok(())
    }
}

fn write_line(w: &mut impl Write, v: &Value) -> Result<()> {
    serde_json::to_writer(&mut *w, v)?;
    w.write_all(b"\n")?;
    Ok(())
}

fn path_safe(ip: IpAddr) -> String {
    ip.to_string().replace(':', "-")
}
