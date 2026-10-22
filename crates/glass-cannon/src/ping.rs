//! The game's own ping, as it measures it.
//!
//! Every 10 seconds the client sends the server its clock (milliseconds since the PC
//! started, plus 2^24 seconds) and the server sends it straight back (`0x3603`). The ping is
//! how long that took: when the reply arrived, on the same clock, minus the time it carries.
//! The TCP round trip can't show this: the game's connection ends at a relay close by.

use std::time::{SystemTime, UNIX_EPOCH};

use meter_core::meter::Meter;
use meter_core::state::CLIENT_CLOCK_BASE_MS;

/// Replies older than this don't count (the game stopped sending).
const FRESH_US: u64 = 35_000_000;

#[link(name = "kernel32")]
extern "system" {
    fn QueryPerformanceCounter(count: *mut i64) -> i32;
    fn QueryPerformanceFrequency(freq: *mut i64) -> i32;
}

/// Milliseconds since the PC started, on the clock the game uses.
fn uptime_ms() -> Option<i64> {
    let (mut count, mut freq) = (0i64, 0i64);
    // SAFETY: both write one i64 through a valid pointer.
    let ok = unsafe { QueryPerformanceCounter(&mut count) != 0 && QueryPerformanceFrequency(&mut freq) != 0 };
    (ok && freq > 0).then(|| (i128::from(count) * 1000 / i128::from(freq)) as i64)
}

/// The median of the game's recent pings, in milliseconds. Only for live capture: a
/// recording's replies were stamped on another session's clock.
pub fn game_ping_ms(meter: &Meter) -> Option<u32> {
    let wall_us = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_micros() as u64;
    let client_now = uptime_ms()? + CLIENT_CLOCK_BASE_MS;
    let mut pings: Vec<i64> = meter
        .pings()
        .filter(|(arrived, _)| wall_us.saturating_sub(*arrived) < FRESH_US)
        .map(|(arrived, sent)| {
            // When it arrived, on the client's clock.
            let ago_ms = (wall_us.saturating_sub(*arrived) / 1000) as i64;
            client_now - ago_ms - sent
        })
        .filter(|ms| (0..5000).contains(ms))
        .collect();
    if pings.is_empty() {
        return None;
    }
    pings.sort_unstable();
    Some(pings[pings.len() / 2] as u32)
}
