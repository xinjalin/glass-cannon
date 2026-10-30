//! Core of the meter: finding the game's connections, capturing them with Npcap,
//! reassembling TCP streams and recording sessions to disk.
//!
//! Everything here runs as a normal user. Nothing is sent anywhere.

pub mod buffs;
pub mod cast;
pub mod classes;
pub mod conns;
pub mod contents;
pub mod cooldowns;
pub mod cubes;
pub mod damage;
pub mod dungeon;
pub mod entities;
pub mod fieldboss;
pub mod game;
pub mod gear;
pub mod hp;
pub mod join;
pub mod decode;
pub mod live;
pub mod meter;
pub mod npcap;
pub mod pcapfile;
pub mod pets;
pub mod pipeline;
pub mod positions;
pub mod protocol;
pub mod pvp;
pub mod state;
pub mod reassembly;
pub mod recorder;
pub mod session;
mod win;

#[cfg(test)]
mod testutil;
