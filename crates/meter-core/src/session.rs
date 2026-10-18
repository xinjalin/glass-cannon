//! Reading recorded sessions back as timestamped game messages.

use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde_json::Value;

use crate::protocol::{Framer, Message};
use crate::recorder::Side;

#[derive(Debug, Clone)]
pub struct TimedMessage {
    pub ts_us: u64,
    pub opcode: u16,
    pub payload: Vec<u8>,
}

#[derive(Debug, Default)]
pub struct FlowMessages {
    pub messages: Vec<TimedMessage>,
    /// Times framing was lost and picked up again at the next chunk.
    pub resyncs: u64,
    pub gaps: u64,
    pub bad_bundles: u64,
}

/// Splits one side of a recorded flow into messages. Each message gets the capture
/// time of the chunk that completed it.
pub fn read_messages(flow: &Path, side: Side) -> Result<FlowMessages> {
    let index = File::open(flow.join("index.jsonl"))
        .with_context(|| format!("{} is not a flow folder", flow.display()))?;
    let data = fs::read(flow.join(format!("{}.bin", side.as_str())))?;

    let mut framer = Framer::default();
    let mut result = FlowMessages::default();
    let mut batch: Vec<Message> = Vec::new();
    for line in BufReader::new(index).lines() {
        let rec: Value = serde_json::from_str(&line?)?;
        if rec["side"].as_str() != Some(side.as_str()) {
            continue;
        }
        let ts = rec["t"].as_u64().unwrap_or(0);
        match rec["kind"].as_str() {
            Some("data") => {
                let pos = rec["pos"].as_u64().unwrap_or(0) as usize;
                let len = rec["len"].as_u64().unwrap_or(0) as usize;
                let Some(chunk) = data.get(pos..pos + len) else {
                    bail!("index points past the end of {}.bin", side.as_str());
                };
                if framer.push(chunk, &mut batch).is_err() {
                    result.resyncs += 1;
                }
                result.messages.extend(batch.drain(..).map(|m| TimedMessage {
                    ts_us: ts,
                    opcode: m.opcode,
                    payload: m.payload,
                }));
            }
            Some("gap") => {
                result.gaps += 1;
                framer.reset();
            }
            Some("syn") | Some("rst") => framer.reset(),
            _ => {}
        }
    }
    result.bad_bundles = framer.bad_bundles();
    Ok(result)
}

/// Accepts a flow folder, or a session folder (picks the game connection).
pub fn resolve_flow(path: &Path) -> Result<PathBuf> {
    if path.join("index.jsonl").is_file() {
        return Ok(path.to_path_buf());
    }
    if path.join("flows").is_dir() {
        return find_game_flow(path);
    }
    bail!("{} is neither a session nor a flow folder", path.display())
}

/// The TCP flow whose received stream splits most cleanly into game messages.
/// TLS and other web connections lose framing almost immediately, so they score low.
pub fn find_game_flow(session: &Path) -> Result<PathBuf> {
    let mut best: Option<(i64, PathBuf)> = None;
    for entry in fs::read_dir(session.join("flows"))? {
        let dir = entry?.path();
        let is_tcp = dir
            .file_name()
            .is_some_and(|n| n.to_string_lossy().contains("-tcp-"));
        if !is_tcp {
            continue;
        }
        let Ok(m) = read_messages(&dir, Side::In) else {
            continue;
        };
        let score = m.messages.len() as i64 - 100 * m.resyncs as i64;
        if score > 0 && best.as_ref().is_none_or(|(s, _)| score > *s) {
            best = Some((score, dir));
        }
    }
    best.map(|(_, d)| d)
        .with_context(|| format!("no game connection found in {}", session.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::tests::frame;
    use crate::recorder::Recorder;
    use crate::testutil::temp_dir;
    use serde_json::json;
    use std::net::SocketAddr;

    #[test]
    fn reads_messages_with_chunk_times_and_survives_gaps() {
        let dir = temp_dir("session");
        let mut rec = Recorder::create(&dir, &json!({})).unwrap();
        let local: SocketAddr = "10.0.0.2:50000".parse().unwrap();
        let remote: SocketAddr = "1.2.3.4:13328".parse().unwrap();
        let mut flow = rec.open_flow(1, "tcp", local, remote).unwrap();

        let a = frame(0x3600, b"one");
        let b = frame(0x3804, b"two");
        let c = frame(0x371d, b"three");
        // `b` arrives in two chunks; then a gap; then a clean message.
        flow.data(100, Side::In, "data", Some(0), &a).unwrap();
        flow.data(200, Side::In, "data", Some(0), &b[..2]).unwrap();
        flow.data(300, Side::In, "data", Some(0), &b[2..]).unwrap();
        flow.mark(400, Side::In, "gap", json!({"off": 0, "len": 9})).unwrap();
        flow.data(500, Side::In, "data", Some(0), &c[3..]).unwrap(); // tail of a lost message
        flow.data(600, Side::In, "data", Some(0), &c).unwrap();
        flow.flush().unwrap();

        let m = read_messages(flow.dir(), Side::In).unwrap();
        let got: Vec<(u64, u16)> = m.messages.iter().map(|x| (x.ts_us, x.opcode)).collect();
        assert_eq!(got, vec![(100, 0x3600), (300, 0x3804), (600, 0x371d)]);
        assert_eq!(m.gaps, 1);
        // The orphaned tail is dropped quietly when the next chunk doesn't complete it.
        assert_eq!(m.resyncs, 0);
        assert_eq!(resolve_flow(&dir).unwrap(), flow.dir());
        fs::remove_dir_all(dir).ok();
    }
}
