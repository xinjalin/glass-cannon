//! Core of the meter: finding the game's connections, capturing them with Npcap,
//! reassembling TCP streams and recording sessions to disk.
//!
//! Everything here runs as a normal user. Nothing is sent anywhere.

pub mod conns;
pub mod decode;
pub mod live;
pub mod npcap;
pub mod pcapfile;
pub mod pipeline;
pub mod reassembly;
pub mod recorder;
mod win;

#[cfg(test)]
mod testutil;
