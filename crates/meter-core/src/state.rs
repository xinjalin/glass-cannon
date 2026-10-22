//! Who's in combat, NPCs leaving, and the game's own ping.
//!
//! Checked on the Vakron Sky Island capture (2026-10-22): combat state toggles for each
//! party member as they pull and finish (353 messages), the boss's carries its berserk
//! time (5 minutes after the pull), and every NPC that died also left with reason 3.

use crate::protocol::read_varint;

/// An entity went in or out of combat: varint entity, u8 flags, u8 state (1 in combat),
/// then with flag 1 a u64 berserk time (ms since 1970: a boss's enrage).
pub const OPCODE_COMBAT_STATE: u16 = 0x8d21;
/// An NPC left: varint entity, u8 flags, u8 reason ([`REMOVED_DIED`], or left or
/// despawned), then with flag 1 a u32.
pub const OPCODE_NPC_REMOVED: u16 = 0x3642;
/// The game's ping coming back: u16 result, u64 when the client sent it (its clock: ms
/// since the PC started plus [`CLIENT_CLOCK_BASE_MS`]), u64 the server's clock.
pub const OPCODE_PING: u16 = 0x3603;

pub const REMOVED_DIED: u8 = 3;
/// What the client adds to the PC's uptime for its clock (2^24 seconds).
pub const CLIENT_CLOCK_BASE_MS: i64 = 16_777_216_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CombatState {
    pub entity: u64,
    pub in_combat: bool,
    /// When a boss goes berserk (ms since 1970).
    pub berserk_ms: Option<u64>,
}

pub fn parse_combat_state(p: &[u8]) -> Option<CombatState> {
    let mut i = 0;
    let entity = read_varint(p, &mut i)?;
    let flags = *p.get(i)?;
    let state = *p.get(i + 1)?;
    let berserk_ms = if flags & 1 != 0 {
        Some(u64::from_le_bytes(p.get(i + 2..i + 10)?.try_into().ok()?))
    } else {
        None
    };
    Some(CombatState { entity, in_combat: state != 0, berserk_ms })
}

/// The NPC and why it left.
pub fn parse_npc_removed(p: &[u8]) -> Option<(u64, u8)> {
    let mut i = 0;
    let entity = read_varint(p, &mut i)?;
    Some((entity, *p.get(i + 1)?))
}

/// When the client sent the ping, on its clock.
pub fn parse_ping(p: &[u8]) -> Option<i64> {
    if p.get(..2)? != [0, 0] {
        return None;
    }
    Some(i64::from_le_bytes(p.get(2..10)?.try_into().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(s: &str) -> Vec<u8> {
        s.split_whitespace().map(|b| u8::from_str_radix(b, 16).unwrap()).collect()
    }

    #[test]
    fn combat_state_and_berserk() {
        let s = parse_combat_state(&hex("e0 de 04 00 01")).unwrap();
        assert_eq!((s.entity, s.in_combat, s.berserk_ms), (77_664, true, None));
        // Boulder Lord Vakron's pull: berserk five minutes later.
        let boss = parse_combat_state(&hex("d1 f2 01 01 01 51 0f d1 62 a1 01 00 00")).unwrap();
        assert_eq!((boss.entity, boss.in_combat, boss.berserk_ms), (31_057, true, Some(1_792_659_230_545)));
        assert!(!parse_combat_state(&hex("e0 de 04 00 00")).unwrap().in_combat);
    }

    #[test]
    fn npcs_leaving_and_pings() {
        assert_eq!(parse_npc_removed(&hex("94 87 02 00 01")), Some((33_684, 1)));
        assert_eq!(parse_ping(&hex("00 00 b8 0b 1d e8 03 00 00 00 9c 31 c4 62 a1 01 00 00")), Some(16_779_119_544));
    }
}
