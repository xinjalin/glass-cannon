# Opcode map

Every message type (opcode) in our captures: 428 opcodes, 5,337,612 messages, from the 59 game-server connections of 25 recordings (`glass-cannon-captures`, 2026-10-18 to 2026-10-25, world and dungeon servers, and a menu test), server to client, after unpacking bundles. Exported with `packet-lab export` and profiled 2026-10-25. Layouts are in [protocol.md](protocol.md).

- **entity first**: how often the payload starts with an entity ID we saw spawn (yes: 60 %+). A message about one player or mob.
- **Questlog's name**: from the message list in Questlog's app.
- **other projects**: what other public projects call it, by project (see [Sources](#sources)). Their names and Questlog's are leads, not checked against our captures unless a row in the first two tables says so.

| | opcodes | messages |
| --- | ---: | ---: |
| Decoded and used by the meter | 52 | 2,948,335 |
| Understood, not used yet | 49 | 1,707,170 |
| Named elsewhere only | 111 | 668,521 |
| Unknown | 216 | 13,586 |
| Named elsewhere, never seen | 17 | 0 |

## Decoded and used by the meter

| opcode | count | what | Questlog's name | other projects |
| --- | ---: | --- | --- | --- |
| `0x3802` | 974,598 | cast: caster, skill, target, cooldown, charges | SkillStart_NT | Aion2Flow: compact control; RATmeter: skill cast; a2_protocol-research: action or cast |
| `0x3804` | 661,697 | damage / heal / effect hit | SkillEffect_NT |  |
| `0x382b` | 626,123 | buff / debuff active (refresh) | AbnormalChange_NT |  |
| `0x8d00` | 223,624 | HP update | CharPointUpdate_NT |  |
| `0x382a` | 116,857 | buff / debuff added | AbnormalAdd_NT |  |
| `0x3805` | 112,290 | damage and heals over time | SkillEffectByAbnormal_NT |  |
| `0x382c` | 74,360 | buffs removed | AbnormalRemoveList_NT |  |
| `0x364a` | 36,857 | your stats tick (~1/s): names your entity | CharStatsUpdate_NT | A2Tools: stats changed |
| `0xe005` | 25,004 | a mob's stagger gauge | UpdateGroggyInfo_NT | a2_protocol-research: groggy or guard |
| `0x921c` | 18,434 | a party member's update (level, gear score, combat power, position) | PartyUpdateMemberStatus_NT |  |
| `0x8d21` | 17,257 | in / out of combat; a boss's berserk time | CombatStateUpdate_NT | Aion2Flow: battle toggle |
| `0x3641` | 15,931 | mob / NPC appears: entity, type, position, HP, max HP; summons; boss copies of players | NpcAppear_NT |  |
| `0x3642` | 15,531 | NPC leaves view: reason 3 = died | NpcDisappear_NT | A2Tools: death; a2_protocol-research: entity removed or death |
| `0x3645` | 8,064 | another player appears: entity, name, character data | CharAppear_NT |  |
| `0x8d04` | 4,739 | a death: who, by which skill, killer | CharDie_NT | Aion2Flow: nickname; A2Tools: summon ownership; RATmeter: entity death |
| `0x3847` | 3,992 | your cooldowns changing (reset, reduction) | SkillCooltimeUpdate_NT | Aion2Flow: cooldowns; RATmeter: skill cooldown |
| `0x3809` | 3,423 | a cast with a cast time starting (target frame's cast bar) | SkillCastingStart_NT | a2_protocol-research: skill cast |
| `0x3835` | 3,047 | what a mob is targeting | TargetUpdate_NT | Aion2Flow: sidecar; a2_protocol-research: summon or skill sequence |
| `0x3603` | 2,882 | the game's ping reply (client time, server time) | Ping_RS | Aion2Flow: round trip; RATmeter: server time |
| `0x922d` | 797 | a dungeon's objective progress: its stage and step | MapEventUpdateProgress_NT |  |
| `0x3834` | 723 | your target | SetMainTarget_RS |  |
| `0x9702` | 653 | party finder group roster (ready, inside, leader) | PartyFinderGroup_NT | A2Tools: party roster; RATmeter: party info; a2_protocol-research: party state |
| `0x9101` | 246 | world map contents: field boss timers | WorldMapContentsInfo_RS | A2Tools: field boss map |
| `0x380c` | 184 | a cast stopped before it went off | SkillCastingStop_NT |  |
| `0x8d26` | 144 | a player came back to life | UseRebirth_NT |  |
| `0x610c` | 110 | one content ticket changed | ContentsTicketUpdate_NT | A2Tools: Odyle energy update |
| `0x6100` | 85 | dungeon progress: entered, countdown over, complete | DungeonState_NT | Aion DPS Meter: instance timer (map, phase, start, end) |
| `0x9200` | 85 | your party's roster | PartyInfo_NT | Aion2Flow: party member list |
| `0x3633` | 84 | you appear: your entity, name, character data (login, zone change) | MyCharAppear_NT |  |
| `0x4002` | 84 | you're on this map (each server connection starts with it) |  | Aion2Flow: state |
| `0x970b` | 50 | a member joined the finder group | PartyFinderJoin_NT |  |
| `0x5100` | 41 | your skills, at login and entering a dungeon | MySkillList_NT |  |
| `0x922e` | 40 | joining a dungeon's objectives (map event) | MapEventJoin_NT | Aion2Flow: map |
| `0x610b` | 39 | every content ticket; Odyle energy is 60000001 | ContentsTickets_NT | A2Tools: Odyle energy list |
| `0x9716` | 34 | a group member readied up / stopped | PartyFinderReady_NT |  |
| `0x9221` | 29 | a member left the party |  |  |
| `0x572e` | 27 | a dungeon's middle boss was killed: its mob type (stage splits) | PartyDungeonRewardMiddleBossKill_NT |  |
| `0x9222` | 24 | a party member back online: a roster entry (read like `0x920d`) | PartyReconnectMember_NT |  |
| `0x9622` | 24 | a member left the Force: u64 member, u64 party, u8 reason (confirmed) | ForceRemoveMember_NT |  |
| `0x9211` | 23 | party leader changed | PartyChangeLeader_NT |  |
| `0x6101` | 19 | dungeon completed | DungeonCompleteResult_NT |  |
| `0x971e` | 15 | a member left the finder group |  |  |
| `0x971d` | 13 | you left the finder group | PartyFinderLeave_NT |  |
| `0xe307` | 13 | the Abyss map: who holds the artifacts (confirmed 2026-10-23) | AbyssArtifactEventInfo_RS | A2Tools: abyss artifacts map |
| `0x961e` | 8 | the Force's member list |  | Aion2Flow: Force member list |
| `0x9715` | 8 | you readied up / stopped | PartyFinderSelfReady_NT |  |
| `0x921f` | 7 | the party broke up | PartyDisband_NT |  |
| `0x931f` | 6 | you left the party (u64 party, u8 reason; with `0x971d` from a finder group) | PartyLeave_NT |  |
| `0x960a` | 5 | a Force member added | ForceAddMember_NT | Aion2Flow: Force roster profile |
| `0x9213` | 3 | you left the party | PartyLeave_RS |  |
| `0x9602` | 1 | the Force's roster: its members count as your party (confirmed: 20 members) | ForceInfo_NT | Aion2Flow: Force roster snapshot |
| `0x972a` | 1 | the finder group removed you | PartyFinderKicked_NT |  |

## Understood, not used yet

Checked on our captures with our own code (2026-10-25 unless the row says otherwise); the counts in a row are messages that fit, of those checked. Most came from leads in SkeeveAN's Aion DPS Meter notes (see [Sources](#sources)). The movement ones are position frames of other players and mobs (never you); layouts in [protocol.md](protocol.md#other-players-and-mobs-moving-confirmed-2026-10-25).

| opcode | count | what | Questlog's name | other projects |
| --- | ---: | --- | --- | --- |
| `0x3600` | 578,893 | server clock: u64 server time in UTC ms, several a second (2 s from the PC's clock, median) | MapFrame_NT | a2_protocol-research: server tick; Aion DPS Meter: server clock |
| `0x371d` | 502,605 | a player or mob moving, 10 a second: dx dy dz as signed bytes (cm, each optional), heading, facing; they add up to the next keyframe within 18 cm (median, 180 cm moves) |  | Aion2Flow: state; a2_protocol-research: movement; Aion DPS Meter: position change (delta) |
| `0x371c` | 114,122 | a player's or mob's position (mobs' once a second): flags, x y z, heading, facing; all 111,435 within 30 m of its last | CharMoveUpdateSnapshot_NT | a2_protocol-research: movement; Aion DPS Meter: mob keyframe once a second |
| `0x371a` | 73,298 | a player or mob stopping: flags, a byte, x y z (cm), heading (u16); 64,947 of 64,956 within 30 m of its last position | CharMoveStop_NT | a2_protocol-research: movement; Aion DPS Meter: position, heading (stop / resync) |
| `0x3729` | 68,050 | a player or mob moving: x y z, velocity (cm/s), heading; reported speed = speed from its positions (median ratio 1.00), heading = its velocity's direction (0.0°) |  | Aion DPS Meter: position, velocity, heading (flag 2 on foot, 1 gliding or falling, 3 flying) |
| `0x372a` | 66,128 | a player or mob moving: dx dy dz, velocity, heading; within 36 cm of the next keyframe (median, 300 cm moves) |  | Aion DPS Meter: position change (delta) |
| `0x371b` | 64,744 | a player or mob setting off: flags, x y z, heading, facing; all 40,759 within 30 m of its last position | CharMoveStart_NT | a2_protocol-research: movement; Aion DPS Meter: position, heading, facing |
| `0x3720` | 36,300 | a player moving: dx dy dz (cm), heading, velocity; they add up to the next keyframe within 28 cm (median, 231 cm moves) |  | Aion DPS Meter: position change (delta) |
| `0x372f` | 26,753 | a player or mob moving: x y z, direction (unit vector), facing (f32) |  | Aion DPS Meter: position, direction, facing |
| `0x920e` | 24,399 | a party member's effects: entity, n, n × u32 effect (24,098 of 24,345 were applied to it before) |  | Aion DPS Meter: target, count, buff IDs (a summary, not a removal) |
| `0x921a` | 24,136 | an effect on a party member: entity, u32 effect (applied to it before in 23,979 of 24,136); comes before its removal |  |  |
| `0x364b` | 17,957 | an entity's stats changed: entity, n, n × (u16 stat, i32 value); mostly stats 0xd6 and 0xcd |  | Aion DPS Meter: one stat of an entity (0xcd on a boss: its rage) |
| `0x371e` | 15,343 | a player moving: x y z, heading, velocity (length rule fits 15,327 of 15,343) |  | Aion DPS Meter: walking, running, rising |
| `0x3801` | 14,383 | your skill accepted or refused: 3 bytes (`00 00 00` accepted, `00 code 18` refused), skill u32 |  | Aion DPS Meter: skill acknowledgement (own player) |
| `0x371f` | 13,961 | a player moving: x y z, heading, velocity |  | Aion DPS Meter: falling |
| `0x3728` | 12,389 | as `0x3729`, at the start of a move (length rule fits 12,389 of 12,389) |  | Aion DPS Meter: start of a move segment |
| `0x3832` | 10,889 | a player's or mob's target: actor, 00, target varint (0 = none) |  | Aion DPS Meter: an actor's new target |
| `0x372e` | 9,292 | a skill used, and where: flags, skill u32, 01, x y z, direction, facing, caster u32 (the entity in 7,614 of 8,000); basic attacks and dodge rolls too |  | Aion DPS Meter: skill cast with position |
| `0x3818` | 4,893 | a chain skill opened: skill u32, 01, follow-up skill u32 | ChainSkillActiveList_NT | Aion DPS Meter: chain skill unlocked |
| `0x3819` | 4,893 | the chain skill window closed: 01, the follow-up a `0x3818` opened (4,893 of 4,893) |  | Aion DPS Meter: chain window closed |
| `0x3724` | 3,980 | a player moving: x y z, heading, velocity |  | Aion DPS Meter: gliding or falling |
| `0x3725` | 3,786 | a player moving: dx dy dz, heading, velocity |  | Aion DPS Meter: position change (delta) |
| `0x3656` | 1,770 | your combat power (u32) | CharCombatPowerUpdate_NT |  |
| `0x3638` | 1,684 | a world object used: object (from `0x3634`), who (a player or you), u8 |  | Aion DPS Meter: world object in use |
| `0x363a` | 1,630 | a world object used: object, who, 8 zero bytes |  | Aion DPS Meter: world object in use |
| `0x8d6d` | 1,203 | a world object's ID, after `0x363a` |  | Aion DPS Meter: world object (companion frame) |
| `0x561e` | 1,200 | loot on the ground: 01, the mob's entity (u32), two positions, loot ID, item, count |  | Aion DPS Meter: a drop appears |
| `0x561f` | 1,199 | loot gone: 20-byte records with the loot ID (1,189 of 1,199 from a `0x561e`); last byte 1 = its owner picked it up |  | Aion DPS Meter: a drop leaves the ground |
| `0x3831` | 979 | your target: 00, target varint (0 = none), 00 00; the same as `0x3834`'s in 439 of 463 |  | Aion DPS Meter: own target |
| `0x363b` | 956 | a world object's ID, after it's used |  | Aion DPS Meter: world object in use |
| `0x3732` | 933 | a player moving: dx dy dz, heading, facing (after `0x3730`) |  | Aion DPS Meter: position change (delta) |
| `0x3723` | 862 | a player's position: flags, x y z, …; 845 of 846 within 30 m of the player's last keyframe |  |  |
| `0x372d` | 775 | a player turning or moving slowly: dx dy dz, heading, facing (f32) |  | Aion DPS Meter: position change (delta) |
| `0x8d05` | 605 | a world object's ID |  | Aion DPS Meter: world object (companion frame) |
| `0x3822` | 419 | your skills' charges: u8 n, then skill u32, charges u8 when n = 1; comes about 14.5 s after a Recuperation cast, when its charge is back |  | Aion DPS Meter: charges (n, skill, charges) |
| `0x5626` | 413 | a player's gear enhanced: entity, kind, 03, old level, new level; kind 0x3c = +1 (398 of 398) |  | Aion DPS Meter: enhancement kind, old, new |
| `0x9507` | 305 | the NPC's idle action ends (0.15 s after `0x9508`, median) |  | Aion DPS Meter: NPC state block |
| `0x9508` | 288 | a peaceful NPC's idle action starts: entity, the 9-byte state block of its spawn |  | Aion DPS Meter: NPC state block |
| `0x3730` | 276 | a player moving: x y z, heading, facing (no flags); `0x3732` changes follow |  | Aion DPS Meter: keyframe for `0x3732` |
| `0x8a05` | 85 | carries your permanent character ID (u32) and server |  | Aion DPS Meter: legion list at login |
| `0x9701` | 82 | party finder listings (post titles, leader names) |  | Aion DPS Meter: list of open groups |
| `0x8d37` | 39 | your gathering skills: u8 n, n × (u32 skill, u32 level), u32 0 |  | Aion DPS Meter: essence extraction |
| `0x8d53` | 39 | the season table: byte-identical in every login (Season_Main_Live …) |  | Aion DPS Meter: season table |
| `0x8d7e` | 39 | your learned crafting recipes: varint n, n × (u32 recipe, u32 0) |  | Aion DPS Meter: learned crafting recipes |
| `0x8d8c` | 39 | your learned conversion recipes: varint n, n × (u32 recipe, u32 0) |  | Aion DPS Meter: learned conversion recipes |
| `0x9232` | 39 | your unlocked teleporters: varint n, n × u32 (exact length 39 of 39) |  | Aion DPS Meter: unlocked teleporters |
| `0x9501` | 39 | the server's unlocked contents: u8 36, 36 × u32 (the same everywhere) |  | Aion DPS Meter: unlocked game contents |
| `0xe32a` | 39 | your arena ratings: u8 n, n × 14 bytes (modes 1, 2, 3) |  | Aion DPS Meter: arena rating and record per mode |
| `0xe343` | 39 | your character at login: 3 identical 21-byte entries, character ID (as `0x8a05`'s, 39 of 39), server, index, time |  | Aion DPS Meter: login record, own ID |

## Named elsewhere only (not checked)

Where a row says "ours:", that much was looked at on our captures; the name itself is still a lead.

| opcode | count | sessions | avg bytes | entity first | Questlog's name | other projects |
| --- | ---: | ---: | ---: | --- | --- | --- |
| `0x3806` | 367,627 | 25 | 8.5 | some | SkillEnd_NT | Aion2Flow: compact control; a2_protocol-research: cast end or defence; Aion DPS Meter: skill end |
| `0x3803` | 99,552 | 20 | 35.0 | yes |  | a2_protocol-research: NPC position; Aion DPS Meter: a hit notice, about 0.3 s before the damage (`0x3804`), with the attacker's position |
| `0x383b` | 31,512 | 16 | 2.0 |  |  | Aion DPS Meter: comes with your skill acknowledgement `0x3801` (ours: 73 % within 0.3 s), `0000` or `0100` |
| `0x383d` | 31,383 | 16 | 2.0 |  |  | a2_protocol-research: combat batch; Aion DPS Meter: comes with your skill acknowledgement `0x3801` (ours: 73 % within 0.3 s) |
| `0x921b` | 23,249 | 4 | 32.5 | yes | PartyUpdateMemberHPMP_NT | Aion2Flow: party status; A2Tools: HP / MP update |
| `0x3848` | 22,893 | 4 | 32.0 | yes | SkillRangeNoticeFilterLocation_NT | decoded 2026-10-24: where a boss's cast lands and when (the meter's boss cast line) |
| `0x8a33` | 17,660 | 19 | 37.1 | yes | GuildDescUpdate_NT | Aion DPS Meter: a visible player's legion |
| `0x561b` | 12,497 | 8 | 49.7 |  | ItemModificationList_NT |  |
| `0x3647` | 7,890 | 24 | 4.0 | yes | CharDisappear_NT | Aion DPS Meter: a player leaves view, with a reason |
| `0x373c` | 6,606 | 22 | 31.9 | yes | CharWarpLeave_NT |  |
| `0x380e` | 5,626 | 5 | 45.2 | yes |  | a2_protocol-research: target selected |
| `0x5684` | 5,062 | 23 | 18.0 |  | AchievementCountUpdate_NT | Aion2Flow: wrapped envelope |
| `0x382d` | 4,740 | 21 | 7.9 | yes | ImmuneAbnormal_NT |  |
| `0x962b` | 4,471 | 1 | 32.7 | yes | ForceUpdateMemberHPMP_NT | Aion2Flow: Force status |
| `0x962c` | 4,127 | 1 | 67.1 |  | ForceUpdateMemberStatus_NT |  |
| `0x3722` | 3,958 | 22 | 2.0 | yes |  | Aion DPS Meter: a move ends (entity only); ours: 7.6 movement frames in the second before, 4.3 after |
| `0x3721` | 3,799 | 23 | 2.0 | yes |  | Aion DPS Meter: a move starts (entity only) |
| `0x381c` | 1,859 | 13 | 32.6 | yes | SkillChargingStart_NT |  |
| `0x3634` | 1,631 | 21 | 55.3 |  | EnvObjSpawn_NT |  |
| `0x3635` | 1,418 | 20 | 6.0 |  | EnvObjDespawn_NT | a2_protocol-research: summon spawn |
| `0x3813` | 1,115 | 6 | 18.0 |  | SkillLevelUpdate_NT |  |
| `0x9220` | 1,039 | 4 | 1.9 | yes | PartySetTargetByBoss_NT |  |
| `0x561c` | 853 | 15 | 108.4 | yes | ItemEquipChange_NT |  |
| `0x364e` | 795 | 17 | 11.8 | yes |  | Aion DPS Meter: two angles in degrees (f32); ours: 132 of 795 not in 0-360 |
| `0x8d74` | 773 | 11 | 6.0 | yes |  | Aion DPS Meter: entity, 00, 05, a varint, a type 1-5 (open) |
| `0x8d72` | 772 | 11 | 4.0 | yes |  | Aion DPS Meter: a player crafting (profession ID) |
| `0x3746` | 758 | 10 | 6.0 | yes |  | Aion DPS Meter: an entity's height (f32); ours: you in 700 of 758 |
| `0x380f` | 493 | 6 | 13.0 |  | SkillAdd_NT |  |
| `0x561d` | 337 | 6 | 8.0 |  |  | a2kit: gear score |
| `0x6003` | 300 | 2 | 322.0 |  |  | Aion DPS Meter: a list of players with permanent character IDs |
| `0x3744` | 239 | 7 | 4.0 | yes |  | Aion DPS Meter: loading state (you, `00 00`); ours: you in 237 of 239 |
| `0x3814` | 162 | 5 | 27.7 | yes | Abnormal_Move_Start_NT |  |
| `0x372b` | 151 | 14 | 61.5 | yes |  | Aion DPS Meter: a path: position, 3 or 4 waypoints, heading, facing |
| `0xe324` | 151 | 6 | 85.4 |  |  | Aion DPS Meter: Daeva pass experience |
| `0x3821` | 142 | 4 | 9.0 | yes | SkillChargingStop_NT | decoded 2026-10-24: a charge let go; its cooldown starts |
| `0x8d57` | 139 | 7 | 134.8 |  | RetrieveMyLatestSeasonRanking_RS | Aion DPS Meter: your rank per content |
| `0x3646` | 133 | 8 | 4.6 | yes | CharBarrierInfo_NT | Aion2Flow: state |
| `0x3816` | 132 | 4 | 24.0 | yes | Abnormal_Move_Instant_NT |  |
| `0x8d52` | 115 | 4 | 3.0 | yes | NpcFirstAggro_NT | decoded 2026-10-24: a boss's pull (boss timelines start there) |
| `0x5613` | 97 | 4 | 14.0 |  |  | Aion DPS Meter: an item used |
| `0x4001` | 86 | 8 | 15.4 |  |  | Aion2Flow: state |
| `0xe262` | 85 | 6 | 118.0 |  |  | Aion DPS Meter: login block, sent again during play |
| `0x3621` | 84 | 8 | 46.0 |  | PrepareEnterMap_NT | Aion2Flow: state; A2Tools: map load |
| `0x3623` | 84 | 8 | 24.0 |  |  | Aion2Flow: map arrival; A2Tools: zone change; a2_protocol-research: zone; Aion DPS Meter: your position after loading; AionMeter: teleport |
| `0x3649` | 84 | 8 | 847.4 |  | CharAllStats_RS | Aion2Flow: state; A2Tools: all stats; RATmeter: player stats |
| `0x4301` | 84 | 8 | 1.0 |  |  | Aion DPS Meter: airspace battle state at zone entry (00 = none) |
| `0x8d15` | 51 | 3 | 49.5 |  | EventFuncAction_NT |  |
| `0x6109` | 44 | 4 | 4.0 |  | DungeonDeathLimit_NT | deaths against the dungeon's limit (Questlog's layout; not used) |
| `0x56a8` | 41 | 6 | 10.0 |  |  | Aion DPS Meter: every login, 10 zero bytes |
| `0xe33e` | 41 | 6 | 15.1 |  |  | Aion DPS Meter: ascension rite record |
| `0x3749` | 40 | 3 | 2.0 | yes |  | a2kit: sit |
| `0x3620` | 39 | 6 | 2045.1 |  |  | RATmeter: session ID link; Aion DPS Meter: client settings (zlib JSON) |
| `0x3900` | 39 | 6 | 361.0 |  | SpecializedSkillLoad_NT |  |
| `0x400a` | 39 | 6 | 9.0 |  | ArenaMatchingInfo_NT |  |
| `0x5600` | 39 | 6 | 506.6 |  |  | Aion DPS Meter: hotbar contents |
| `0x5611` | 39 | 6 | 13307.4 |  | ItemDepotList_RS | Aion DPS Meter: inventory and storage at login |
| `0x5633` | 39 | 6 | 1.0 |  |  | Aion DPS Meter: godstone combinations |
| `0x5672` | 39 | 6 | 30.7 |  | WingList_NT |  |
| `0x5679` | 39 | 6 | 766.9 |  | TitleList_NT |  |
| `0x5683` | 39 | 6 | 40027.6 |  | AchievementList_NT |  |
| `0x5712` | 39 | 6 | 40.7 |  |  | Aion DPS Meter: guaranteed reward per dungeon |
| `0x8aaf` | 39 | 6 | 62.5 |  | GuildDecoList_NT |  |
| `0x8d09` | 39 | 6 | 3194.8 |  |  | Aion DPS Meter: quest log |
| `0x8d1c` | 39 | 6 | 1418.0 |  |  | Aion DPS Meter: daily mission list (journal) |
| `0x8d44` | 39 | 6 | 9.0 |  | ClientEnvSetting_NT |  |
| `0x8d5b` | 39 | 6 | 1855.6 |  | SkinList_NT | Aion DPS Meter: owned skins |
| `0x9000` | 39 | 6 | 3831.9 |  | RideInitData_NT |  |
| `0x9331` | 39 | 6 | 95.9 |  |  | Aion DPS Meter: transcendence records |
| `0xe226` | 39 | 6 | 905.5 |  | DaevanionBoardInfo_NT |  |
| `0xe237` | 39 | 6 | 147.8 |  |  | Aion DPS Meter: nightmare records per boss |
| `0xe256` | 39 | 6 | 1685.0 |  |  | A2Tools: Supply Requests boards; Aion DPS Meter: supply requests (4 tabs) |
| `0xe25a` | 39 | 6 | 216.0 |  |  | Aion DPS Meter: session token (a FlatBuffers table); ours: 216 bytes, different every login |
| `0xe274` | 39 | 6 | 72.7 |  |  | Aion DPS Meter: friends list |
| `0xe28c` | 39 | 6 | 1135.4 |  |  | Aion DPS Meter: hotbar contents |
| `0xe30b` | 39 | 6 | 25.0 |  | MonolithList_NT |  |
| `0xe33b` | 39 | 6 | 190.0 |  |  | Aion DPS Meter: membership benefits and ticket times |
| `0xffa5` | 39 | 6 | 16.0 |  |  | ours: the same two u64 as `0xffa6` at login and zone changes (the last value of that day: 829 after a session's last `0xffa6` said 829) |
| `0x922f` | 38 | 4 | 4.0 |  | MapEventLeave_NT | Aion2Flow: map |
| `0x8d33` | 35 | 4 | 25.0 |  | GatherSourceCompleted_NT |  |
| `0x3611` | 34 | 4 | 276.0 |  |  | a2_protocol-research: server RSA handshake |
| `0x3615` | 34 | 4 | 296.5 |  | Login_RS | a2_protocol-research: login or server transfer |
| `0x3616` | 33 | 4 | 27.8 |  |  | a2_protocol-research: game server info |
| `0xe261` | 30 | 3 | 2.0 |  |  | Aion DPS Meter: right after a `0xe262` |
| `0x920d` | 29 | 4 | 145.1 |  | PartyAddMember_NT | Aion2Flow: party profile; read now: a member joined your party (confirmed 2026-10-24) |
| `0x3907` | 28 | 2 | 21.4 |  |  | Aion DPS Meter: one skill's specialization slots |
| `0x3650` | 27 | 3 | 3098.7 |  |  | Aion DPS Meter: another player's character window (inspect) |
| `0x900d` | 26 | 4 | 12.1 |  | RideSoulPointIncrease_NT |  |
| `0x5676` | 24 | 3 | 7.0 | yes | WingChange_NT |  |
| `0x3810` | 23 | 2 | 4.0 |  |  | Aion DPS Meter: a skill variant removed |
| `0x4000` | 20 | 2 | 4.0 | yes | CharPhaseUpdate_NT |  |
| `0x9009` | 19 | 2 | 23.0 | yes | CharRideAcquire_NT |  |
| `0x8d8f` | 16 | 2 | 218.0 |  |  | Aion DPS Meter: conversion counters |
| `0x3817` | 15 | 4 | 3.7 | yes |  | a2kit: fall |
| `0x8c07` | 15 | 3 | 23.0 |  |  | Aion DPS Meter: coin shop purchase counters |
| `0xffa6` | 15 | 3 | 16.0 |  |  | LOXIN: daily kinah (its parser's label); ours: u64 that rises after kills (+6 to +290), then u64 server time (ms) |
| `0x8d91` | 9 | 1 | 6.0 | yes | BroadcastLevelUpdate_NT |  |
| `0xe335` | 9 | 2 | 46.0 |  | CutsceneComplete_RS |  |
| `0x8d2f` | 8 | 4 | 646.0 |  |  | a2_protocol-research: notice |
| `0xe282` | 8 | 1 | 36.0 |  |  | Aion DPS Meter: a friend or legion member online / offline |
| `0x3626` | 5 | 2 | 7891.2 |  |  | Aion DPS Meter: character list (back at character select) |
| `0x568e` | 5 | 2 | 115.0 |  |  | Aion DPS Meter: purchase counters on opening a shop |
| `0x381b` | 4 | 1 | 6.0 |  |  | Aion DPS Meter: charging starts (status, skill) |
| `0x381e` | 4 | 1 | 15.0 |  |  | Aion DPS Meter: charging let go |
| `0x8d46` | 4 | 1 | 11.0 |  | ServerMsg_NT |  |
| `0x5605` | 2 | 1 | 85.0 |  |  | Aion DPS Meter: hotbar (preset change) |
| `0x5615` | 2 | 1 | 22.0 |  |  | Aion DPS Meter: an item used (stack, count) |
| `0x900e` | 2 | 2 | 9.0 |  | RideSoulPointDecrease_NT |  |
| `0x5680` | 1 | 1 | 7.0 |  |  | Aion DPS Meter: comes with a title change |
| `0x5681` | 1 | 1 | 5.0 |  |  | Aion DPS Meter: a worn title changed (slot, title) |
| `0x8d55` | 1 | 1 | 56670.0 |  |  | Aion DPS Meter: rankings (up to 500 entries) |
| `0xe294` | 1 | 1 | 7.0 |  |  | Aion DPS Meter: the titles you wear |

Where a name doesn't fit our captures:

- `0x3803` isn't an NPC's position: 6,054 of its 6,832 messages are an entity, a byte, another entity, then a player skill ID.
- `0x3635` is Questlog's object despawn, not a summon spawn: 200 of its 239 remove an ID an earlier `0x3634` (object spawn) brought in.
- `0x383d` ("combat batch") is always 2 bytes, `00 00`.

## Unknown

No name anywhere. Sorted by count; the busy ones are the most worth a look.

| opcode | count | sessions | avg bytes | entity first | first bytes |
| --- | ---: | ---: | ---: | --- | --- |
| `0x9018` | 1,236 | 2 | 109.3 |  | `00 00 02 01 05 00 02 8d 01 0b` |
| `0x9012` | 1,235 | 2 | 6.0 |  | `01 02 14 00 00 00` |
| `0x3849` | 1,163 | 1 | 37.0 | yes | `a9 f3 02 00 a0 62 01 01 00 00` |
| `0x3734` | 1,114 | 4 | 32.0 | yes | `b4 48 a2 76 26 46 14 3f f7 46` |
| `0x9006` | 712 | 21 | 4.0 | yes | `f8 7d 01 00` |
| `0x9003` | 677 | 19 | 9.0 | yes | `b2 74 53 05 00 00 01 00 01` |
| `0x9216` | 566 | 4 | 12.0 |  | `00 00 00 00 e4 7d 01 00 00 00` |
| `0x5616` | 456 | 5 | 12.0 |  | `ef fa 1d 00 00 37 00 00 23 55` |
| `0x385c` | 389 | 4 | 14.9 | yes | `d5 dc 02 01 00 aa c3 01 09 01` |
| `0xe22e` | 302 | 6 | 16.0 |  | `05 01 3c 01 02 00 00 03 24 00` |
| `0x6001` | 222 | 5 | 24.1 |  | `03 a9 b8 0d 01 3c 1d 29 63 a1` |
| `0x8d34` | 222 | 7 | 7.0 | yes | `9e 72 00 3a 94 d6 02` |
| `0x910c` | 206 | 5 | 8.8 |  | `00 00 56 04 00 00 00` |
| `0x3743` | 196 | 5 | 4.0 | yes | `cb 2d 00 00` |
| `0x3741` | 193 | 7 | 4.0 | yes | `ff 18 00 00` |
| `0x8d03` | 190 | 7 | 6.0 | yes | `ff 18 00 00 00 00` |
| `0x3742` | 178 | 7 | 2.0 | yes | `ff 18` |
| `0x3815` | 162 | 4 | 16.3 | yes | `c1 e3 05 96 e9 2e 45 28 69 86` |
| `0x8d02` | 149 | 7 | 6.0 | yes | `ff 18 d0 07 00 00` |
| `0xe228` | 114 | 1 | 6.0 |  | `00 00 72 5c 0c 00` |
| `0x8d93` | 113 | 3 | 7.0 |  | `8b 92 02 01 00 00 00` |
| `0xe503` | 106 | 1 | 18.5 |  | `00 00 51 00 00 00 03 71 5c 0c` |
| `0x3637` | 86 | 5 | 14.3 |  | `00 00 00 86 89 04 00 00 00 00` |
| `0x8a03` | 85 | 8 | 307.6 |  | `00 00 05 54 49 45 52 31 0a 43` |
| `0x8a45` | 84 | 8 | 14.0 |  | `00 00 02 00 00 00 61 c8 e7 a2` |
| `0x8d4b` | 79 | 8 | 3.0 | yes | `bb 42 00` |
| `0xe215` | 78 | 6 | 12.0 |  | `00 00 01 00 00 00 00 00 00 00` |
| `0xe233` | 75 | 2 | 6.0 |  | `6f 13 00 00 00 00` |
| `0x8a39` | 71 | 12 | 8.2 |  | `05 57 75 6d 62 6f` |
| `0x373f` | 67 | 3 | 18.0 | yes | `b6 58 00 00 15 d1 80 46 92 67` |
| `0x3639` | 64 | 8 | 11.0 |  | `bd f4 02 06 19 d1 3c 03 00 00` |
| `0x9002` | 62 | 4 | 11.0 | yes | `cb 2d 2d 3b 24 04 00 00 00 00` |
| `0x3654` | 61 | 2 | 3.0 | yes | `f9 3d 01` |
| `0x901a` | 59 | 2 | 6.0 |  | `00 00 05 01 03 01` |
| `0x6108` | 57 | 4 | 1.0 |  | `00` |
| `0x570f` | 50 | 6 | 7.9 |  | `00 00 00` |
| `0x3726` | 45 | 4 | 4.0 | yes | `fd 44 01 04` |
| `0x3731` | 45 | 3 | 19.0 | yes | `ed 5b bc 56 96 c7 1c ae 03 c8` |
| `0xe002` | 45 | 2 | 12.0 |  | `bd 5a 00 00 00 00 00 00 4b 22` |
| `0xffa9` | 45 | 6 | 16.0 |  | `68 13 00 00 00 00 00 00 60 e3` |
| `0x8d41` | 44 | 4 | 2.0 |  | `00 00` |
| `0x9321` | 41 | 3 | 10.0 |  | `01 b4 48 b0 59 09 01 ec e0 06` |
| `0x8d4d` | 40 | 6 | 10.0 |  | `00 00 b8 a9 15 00 00 cf 00 00` |
| `0xe34e` | 40 | 6 | 2.0 |  | `00 00` |
| `0x3657` | 39 | 6 | 9.0 |  | `01 00 00 00 00 00 00 00 00` |
| `0x5648` | 39 | 6 | 10.0 |  | `01 01 b3 14 99 1f 00 00 00 00` |
| `0x564b` | 39 | 6 | 14.1 |  | `00` |
| `0x5682` | 39 | 6 | 4.0 |  | `00 00 23 00` |
| `0x56ac` | 39 | 6 | 42.9 |  | `2a 55 15 05 0e 06 07 08 49 09` |
| `0x56b6` | 39 | 6 | 1.0 |  | `00` |
| `0x572d` | 39 | 6 | 1.0 |  | `00` |
| `0x6000` | 39 | 6 | 59.7 |  | `09 03 fd ea 0c 02 a5 4a 3d 4e` |
| `0x6103` | 39 | 6 | 10.0 |  | `01 07 a1 bb 0d 00 01 5a c0 2f` |
| `0x8d58` | 39 | 6 | 1.0 |  | `00` |
| `0x8d83` | 39 | 6 | 1.0 |  | `00` |
| `0x9330` | 39 | 6 | 1.7 |  | `00` |
| `0xe200` | 39 | 6 | 12.0 |  | `00 80 9d 11 fe ff ff ff ff 00` |
| `0xe223` | 39 | 6 | 12.0 |  | `80 c1 f5 52 a1 01 00 00 00 00` |
| `0xe224` | 39 | 6 | 4.0 |  | `02 01 02 02` |
| `0xe251` | 39 | 6 | 1.0 |  | `00` |
| `0xe257` | 39 | 6 | 53.8 |  | `34 01 02 03 04 05 06 07 08 09` |
| `0xe25c` | 39 | 6 | 3.0 |  | `01 4b 01` |
| `0xe263` | 39 | 6 | 5.0 |  | `02 46 00 5a 00` |
| `0xe33d` | 39 | 6 | 4.0 |  | `00 00 00 00` |
| `0xe342` | 39 | 6 | 63.6 |  | `09 f7 ae 15 00 00 c8 00 00 76` |
| `0xe349` | 39 | 6 | 11.0 |  | `00 00 80 d9 86 4f a1 01 00 00` |
| `0xffac` | 39 | 6 | 24.0 |  | `03 01 02 00 00 00 02 17 00 00` |
| `0x8d23` | 38 | 2 | 2.0 |  | `00 00` |
| `0x971f` | 38 | 4 | 58.1 |  | `1e 03 58 de 03 00 00 00 c7 09` |
| `0x8d31` | 37 | 4 | 108.9 |  | `00 00 ba fd 03 01 00 00 00 01` |
| `0xffa7` | 37 | 6 | 2.0 |  | `07 01` |
| `0x374c` | 36 | 2 | 2.0 | yes | `98 2b` |
| `0x3836` | 36 | 4 | 4.0 |  | `00 ba fd 03` |
| `0x6006` | 36 | 1 | 11.0 |  | `8d b9 0d 71 a1 03 00 00 00 cc` |
| `0xe22b` | 36 | 1 | 6.0 |  | `00 00 59 5c 0c 00` |
| `0x8d3e` | 35 | 4 | 12.0 |  | `01 00 00 00 76 cc cf 62 a1 01` |
| `0x9506` | 34 | 2 | 19.0 | yes | `bc 3b 00 05 1b 00 00 00 01 03` |
| `0x364d` | 33 | 4 | 1.0 |  | `00` |
| `0x3655` | 33 | 2 | 6.1 | yes | `a6 38 00 00 00 00` |
| `0x9624` | 31 | 1 | 14.7 |  | `08 48 61 72 69 48 61 72 69 c7` |
| `0x363e` | 30 | 4 | 3.1 |  | `13 38 00` |
| `0x8d01` | 25 | 3 | 6.6 | yes | `dc c4 01 04 00 00 00` |
| `0x8d92` | 25 | 4 | 3.0 | yes | `cb 2d 01` |
| `0x563f` | 22 | 2 | 7.0 |  | `40 02 00 00 00 00 01` |
| `0xe34c` | 22 | 6 | 1174.0 |  | `01 01 00 00 00 05 41 49 4f 4e` |
| `0x5617` | 21 | 3 | 14.0 | yes | `e6 5f c1 b3 11 00 00 12 00 00` |
| `0x5619` | 21 | 3 | 14.0 | yes | `e6 5f c1 b3 11 00 00 12 00 00` |
| `0x9719` | 21 | 3 | 9.0 |  | `a7 de 03 00 00 00 df 05 01` |
| `0x568a` | 20 | 3 | 35.4 |  | `00 00 04 db 2a 51 0e 00 01 db` |
| `0x5105` | 19 | 2 | 4.6 |  | `01 02 41 00` |
| `0x6005` | 19 | 1 | 33.7 |  | `8d b9 0d 00 f0 80 03 00 00 00` |
| `0x9235` | 19 | 4 | 13.0 |  | `01 e0 81 04 00 83 24 d5 00 54` |
| `0x9733` | 19 | 2 | 9.0 |  | `89 b5 03 00 00 00 de 05 01` |
| `0x3738` | 18 | 1 | 19.8 | yes | `df 56 07 01 e7 10 05 6f 54 88` |
| `0x564d` | 16 | 3 | 7.0 |  | `00 00 23 55 a7 1f 01` |
| `0x5702` | 16 | 1 | 15.0 |  | `00 00 73 f2 00 00 9b 11 00 00` |
| `0x8d25` | 16 | 2 | 3.0 |  | `00 00 01` |
| `0x9237` | 16 | 4 | 6.0 |  | `00 00 67 0d 10 00` |
| `0x3840` | 15 | 2 | 6.0 |  | `00 00 90 b1 17 01` |
| `0x5621` | 15 | 4 | 11.0 |  | `00 00 b0 51 00 00 bd 98 17 01` |
| `0x562a` | 15 | 1 | 51.0 |  | `00 00 08 5f 14 00 00 94 00 00` |
| `0x5643` | 14 | 3 | 97.1 |  | `00 04 00 00 00 00 05 76 b0 05` |
| `0x9319` | 14 | 4 | 0.0 |  | `` |
| `0x9409` | 14 | 2 | 40.6 |  | `1c b0 00 00 8f 12 00 00 02 8f` |
| `0xe22d` | 14 | 1 | 6.0 |  | `00 00 51 00 00 00` |
| `0x5713` | 13 | 4 | 9.0 |  | `02 8a 1b 23 00 8b 1b 23 00` |
| `0x5715` | 13 | 4 | 188.0 |  | `00 00 02 1c 37 33 70 37 a0 0f` |
| `0xffad` | 13 | 4 | 2.0 |  | `02 01` |
| `0x4400` | 12 | 4 | 109.2 |  | `04 00 e4 05 0f 1a 04 00 00 00` |
| `0x4402` | 12 | 4 | 2.0 |  | `00 00` |
| `0x4403` | 12 | 4 | 77.1 |  | `03 01 e4 05 0f 1a 04 00 00 00` |
| `0x567d` | 12 | 2 | 4.0 |  | `e8 1a 5f 01` |
| `0x3733` | 11 | 4 | 41.0 | yes | `b4 48 02 1f be 2d 46 ac 93 f7` |
| `0x9234` | 11 | 2 | 17.3 |  | `01 c1 7d 0a 00 83 24 d5 00 01` |
| `0xe28d` | 11 | 2 | 181.5 |  | `01 01 01 14 2e be 10 00 00 c2` |
| `0xe323` | 11 | 3 | 12.0 |  | `00 00 4c 4e 00 00 68 d0 e6 30` |
| `0x383e` | 10 | 2 | 5.7 | yes | `cb 44 01 99 a2 01` |
| `0x5623` | 10 | 1 | 15.0 |  | `3c 03 08 5f 14 00 00 94 00 00` |
| `0x9227` | 10 | 3 | 121.0 |  | `05 00 00 00 00 00 00 00 00 00` |
| `0xe265` | 10 | 1 | 82.0 |  | `00 00 24 32 42 35 39 46 37 42` |
| `0x4005` | 9 | 2 | 2.0 |  | `00 00` |
| `0x5647` | 9 | 2 | 62.0 |  | `5d 03 37 0f 08 00 00 7e 01 00` |
| `0x8d3f` | 9 | 3 | 0.0 |  | `` |
| `0x900c` | 9 | 3 | 8.0 | yes | `8a 1a 00 00 a2 04 00 00` |
| `0x9706` | 9 | 3 | 7.0 |  | `00 00 01 00 00 00 00` |
| `0xe004` | 9 | 2 | 8.0 |  | `bd 5a 00 00 00 00 00 00` |
| `0xe248` | 9 | 2 | 11.0 |  | `30 04 46 16 04 00 00 b0 00 00` |
| `0x4106` | 8 | 1 | 24.6 |  | `fd bc 02 0c c7 09 08 4d 65 67` |
| `0x570a` | 8 | 1 | 749.0 |  | `00 00 73 f2 00 00 9b 11 00 00` |
| `0x8d5c` | 8 | 2 | 55.1 |  | `01 07 8f ca 01 0c 00 00 00 01` |
| `0x940b` | 8 | 2 | 32.9 |  | `e7 12 00 00 02 f0 00 00 00 e7` |
| `0x9713` | 8 | 4 | 6.0 |  | `00 00 00 00 00 00` |
| `0x971b` | 8 | 3 | 4.0 |  | `34 b0 08 00` |
| `0x9730` | 8 | 2 | 53.5 |  | `a2 7c 02 00 00 00 cb 09 14 00` |
| `0x9732` | 8 | 2 | 6.0 |  | `00 00 00 00 00 00` |
| `0x3735` | 7 | 4 | 24.0 | yes | `fd 44 0f d3 28 03 df f8 23 c5` |
| `0x373e` | 7 | 2 | 18.0 | yes | `9f 02 00 00 33 04 4d c6 f7 a8` |
| `0x5641` | 7 | 2 | 43.9 |  | `60 02 00 04 00 00 00 04 00 00` |
| `0x6104` | 7 | 1 | 5.0 |  | `f7 27 09 00 00` |
| `0x8c09` | 7 | 3 | 15.0 |  | `00 06 38 1b 0f 00 01 b8 50 29` |
| `0x9011` | 7 | 2 | 7.0 |  | `01 01 05 00 00 00 02` |
| `0xe229` | 7 | 1 | 5.0 |  | `01 81 5c 0c 00` |
| `0x5608` | 6 | 3 | 12.5 |  | `0f 00 00 06 00 c0 44 13 01 01` |
| `0x5673` | 6 | 3 | 9.0 |  | `e6 2f 00 00 85 fb 69 02 00` |
| `0x8d4e` | 6 | 3 | 6.0 | yes | `84 39 24 f4 cb 1f` |
| `0xe26e` | 6 | 1 | 51.0 |  | `24 32 42 35 39 46 37 42 32 2d` |
| `0xe270` | 6 | 1 | 41.0 |  | `00 00 24 34 31 46 46 37 37 36` |
| `0xe293` | 6 | 3 | 272.7 |  | `01 0c 03 01 00 90 cf 12 01 04` |
| `0x3811` | 5 | 2 | 3.0 |  | `02 2f 00` |
| `0x5645` | 5 | 1 | 72.0 |  | `10 04 00 00 00 00 03 bb ef 01` |
| `0x9226` | 5 | 3 | 2.0 |  | `00 00` |
| `0x9233` | 5 | 1 | 4.0 |  | `99 0d 10 00` |
| `0x923a` | 5 | 1 | 16.6 |  | `00 0e 26 18 00 00 12 e5 8f af` |
| `0x4404` | 4 | 2 | 25.0 |  | `03 01 00 00 00 0c 00 00 00 03` |
| `0x5700` | 4 | 1 | 52.0 |  | `01 73 f2 00 00 9b 11 00 00 20` |
| `0x5707` | 4 | 1 | 10.0 |  | `00 00 73 f2 00 00 9b 11 00 00` |
| `0x570b` | 4 | 1 | 8.0 |  | `73 f2 00 00 9b 11 00 00` |
| `0x6004` | 4 | 1 | 201.2 |  | `8d b9 0d 0c 03 61 30 03 00 00` |
| `0x6008` | 4 | 1 | 5.0 |  | `00 00 8d b9 0d` |
| `0x8d4a` | 4 | 2 | 2.0 |  | `00 00` |
| `0x9215` | 4 | 2 | 2.0 |  | `00 00` |
| `0x962d` | 4 | 1 | 8.0 |  | `28 34 03 00 00 00 c7 09` |
| `0x9634` | 4 | 2 | 2.0 |  | `12 3e` |
| `0x9722` | 4 | 2 | 24.8 |  | `03 39 35 6b 01 02 05 01 04 02` |
| `0x3745` | 3 | 1 | 129.7 |  | `00 00 01 00 36 3f 01 48 6f 30` |
| `0x5634` | 3 | 1 | 10.0 |  | `01 01 c2 69 30 01 02 00 00 00` |
| `0x8d48` | 3 | 3 | 2.0 |  | `00 00` |
| `0x8d69` | 3 | 3 | 2.0 |  | `00 00` |
| `0x9005` | 3 | 2 | 5.0 | yes | `a0 62 38 3b 01` |
| `0x9623` | 3 | 1 | 9.0 |  | `31 3a 0a 00 83 24 d5 00 06` |
| `0x9707` | 3 | 1 | 55.3 |  | `c6 2d 0b 00 3f 8e 03 00 00 00` |
| `0x9709` | 3 | 1 | 2.0 |  | `00 00` |
| `0xe24a` | 3 | 2 | 15.0 |  | `3f 04 7b a1 13 00 00 c6 00 00` |
| `0x372c` | 2 | 2 | 21.0 | yes | `9f 7c f6 ec 96 c7 cc 94 03 c8` |
| `0x3736` | 2 | 1 | 30.0 | yes | `df 56 01 01 cf e7 14 47 90 22` |
| `0x3844` | 2 | 2 | 17.0 |  | `00 00 02 90 b1 17 01 01 90 b1` |
| `0x5636` | 2 | 1 | 15.0 |  | `00 00 d3 78 0a 00 00 79 01 00` |
| `0x567c` | 2 | 2 | 6.0 | yes | `ac 55 87 bd 40 01` |
| `0x8d39` | 2 | 1 | 14.0 |  | `00 00 20 03 00 00 01 00 00 00` |
| `0x8d87` | 2 | 1 | 41.0 |  | `00 00 01 a5 76 ea 01 51 05 a5` |
| `0x8d8b` | 2 | 1 | 41.0 |  | `00 00 01 a5 76 ea 01 50 05 a5` |
| `0x9008` | 2 | 2 | 4.0 | yes | `e4 6b 00 00` |
| `0x9209` | 2 | 1 | 61.0 |  | `53 2a 0b 00 83 24 d5 00 fa 09` |
| `0x920b` | 2 | 1 | 2.0 |  | `00 00` |
| `0x9333` | 2 | 1 | 13.0 |  | `02 e0 05 01 90 12 c0 02 8b 01` |
| `0x9339` | 2 | 1 | 2.0 |  | `00 00` |
| `0x933b` | 2 | 1 | 0.0 |  | `` |
| `0xe26d` | 2 | 1 | 53.0 |  | `00 00 24 35 33 31 44 42 45 46` |
| `0xe290` | 2 | 1 | 6.0 |  | `00 00 02 01 00 00` |
| `0xe33f` | 2 | 1 | 14.0 |  | `00 00 ac 02 00 00 00 00 00 00` |
| `0x3737` | 1 | 1 | 30.0 | yes | `df 56 01 01 0f b4 14 47 04 1d` |
| `0x3740` | 1 | 1 | 18.0 | yes | `cb 2d 06 02 06 9b d8 c6 e6 bf` |
| `0x5637` | 1 | 1 | 12.0 | yes | `c8 17 e0 21 9b 06 01 01 c2 69` |
| `0x56b1` | 1 | 1 | 19.0 |  | `00 00 01 00 00 00 00 01 00 00` |
| `0x8ab0` | 1 | 1 | 4.0 |  | `fc 03 00 00` |
| `0x8ab1` | 1 | 1 | 11.0 |  | `02 08 04 00 00 00 fc 03 00 00` |
| `0x8ab4` | 1 | 1 | 7.0 |  | `00 00 fc 03 00 00 0e` |
| `0x8d45` | 1 | 1 | 10.0 |  | `01 1b be e4 82 69 a1 01 00 00` |
| `0x8d73` | 1 | 1 | 4.0 | yes | `a4 61 00 00` |
| `0x8d96` | 1 | 1 | 312.0 |  | `00 00 9a a2 d8 6a 04 00 00 00` |
| `0x8d99` | 1 | 1 | 167.0 |  | `03 e2 d4 02 00 00 00 c7 09 2d` |
| `0x9332` | 1 | 1 | 52.0 |  | `00 00 04 02 00 00 00 01 00 00` |
| `0x960c` | 1 | 1 | 2.0 |  | `00 00` |
| `0x9628` | 1 | 1 | 3.0 |  | `02 01 02` |
| `0x970e` | 1 | 1 | 59.0 |  | `e3 ac 0a 00 d6 27 09 00 00 fa` |
| `0x9710` | 1 | 1 | 6.0 |  | `00 00 00 00 00 00` |
| `0x9724` | 1 | 1 | 6.0 |  | `00 00 94 0f 0b 00` |
| `0xe22f` | 1 | 1 | 16.0 |  | `05 01 be 00 02 00 00 03 24 00` |
| `0xe235` | 1 | 1 | 10.0 |  | `00 00 d1 07 00 00 08 00 00 00` |
| `0xe236` | 1 | 1 | 8.0 |  | `d1 07 00 00 08 00 00 00` |
| `0xe238` | 1 | 1 | 68.0 |  | `00 00 b9 19 00 00 00 00 00 00` |
| `0xe277` | 1 | 1 | 37.0 |  | `00 c7 09 de 45 03 00 00 00 c7` |
| `0xe27d` | 1 | 1 | 10.0 |  | `00 00 de 45 03 00 00 00 c7 09` |
| `0xe27e` | 1 | 1 | 8.0 |  | `de 45 03 00 00 00 c7 09` |
| `0xe28e` | 1 | 1 | 49.0 |  | `01 01 01 05 0b ac 0c 00 00 66` |
| `0xe296` | 1 | 1 | 89.0 |  | `01 01 11 3b 00 00 00 01 3f 00` |

## Named elsewhere, never in our captures

Some may be older opcodes: A2Tools notes the game shifted opcodes in 2026-06.

| opcode | Questlog's name | other projects | meter |
| --- | --- | --- | --- |
| `0x3329` |  | Aion2Flow: legion metadata |  |
| `0x361a` |  | a2_protocol-research: name check |  |
| `0x3640` |  | Aion2Flow: summon; A2Tools: summon spawn; a2_protocol-research: spawn variant |  |
| `0x3644` |  | A2Tools: player spawn (variant); a2_protocol-research: player info variant |  |
| `0x569e` | CustomizingPresetList_RS |  |  |
| `0x9219` | PartyKickout_NT |  | kicked from the party (u8: by vote) |
| `0x961a` | ForceDisband_NT |  | the Force (20-player raid group) broke up |
| `0x961b` | ForceReconnectMember_NT | Aion2Flow: Force profile | a Force member back |
| `0x961d` |  | Aion2Flow: Force member | a Force member |
| `0x962e` | ForceLeave_NT |  | you left the Force (20-player raid group) |
| `0xe241` | StartNpcInteraction_RS |  |  |
| `0xe243` | FinishNpcInteraction_RS |  |  |
| `0xe302` | AbyssArtifactStatus_NT |  |  |
| `0xe303` | AbyssArtifactUpdate_NT |  |  |
| `0xe305` | AbyssArtifactEventInfo_NT | A2Tools: abyss artifact entry | entering the Abyss: who holds its artifacts (untested) |
| `0xe308` | AbyssArtifactEventUpdate_NT |  |  |
| `0xe326` | BattleScoreUpdate_NT |  |  |

## Sources

Only names and descriptions were taken from these, as leads to check against our own captures. No code or data was copied.

| project | license | what it names |
| --- | --- | --- |
| Questlog (questlog.gg's desktop app) | proprietary | its message list (names only) |
| [Aion2Flow](https://github.com/cloris-chan/Aion2Flow) by cloris-chan | GPL-3.0 | its opcode dispatcher: party and Force rosters, map state, nicknames |
| [A2Tools DPS Meter](https://github.com/taengu/A2Tools-DPS-Meter) by taengu | GPL-3.0 | its capture modules: zone and map loads, stats, Odyle energy, Supply Requests, abyss artifacts |
| [RATmeter](https://github.com/Kuroukihime/AIon2-Dps-Meter) by Kuroukihime | GPL-3.0 | its opcode table (`PacketOpcodes.cs`) |
| [a2_protocol-research](https://github.com/SixShoot/a2_protocol-research) by SixShoot | MIT | `docs/OPCODES.md`; mostly client to server, which a meter doesn't read |
| [Aion DPS Meter](https://github.com/SkeeveAN/Aion-DPS-Meter) by SkeeveAN | MIT | `docs/protocol/` (German): movement frames, targets, chain skills, skill acknowledgements, world objects, loot, login lists. It writes opcodes with the two bytes the other way round (its `0x2937` is our `0x3729`) |
| [a2kit](https://github.com/nuriland/a2kit) by nuriland | MIT | its opcode table (`game/opcode.go`): sit, fall, gear score |
| [AionMeter](https://github.com/cyberbadger6969/aion2-dps-meter) by cyberbadger6969 | GPL-3.0 | its opcode list (`Opcodes.cs`): teleport |
| LOXIN (loxin.gg's desktop meter) | proprietary | its installer unpacked, not run (2026-10-25): the opcodes it reads (23 in its built-in profile, about 15 more compared in its code; all in our captures) and its parser's labels. Names as leads only |
| [NOIA2](https://github.com/ZDYoung0519/NOIA2) by ZDYoung0519 | GPL-3.0 | combat power and field boss timers (already in [protocol.md](protocol.md)) |

Searched 2026-10-23 and 2026-10-25 (SearXNG and GitHub search). The other meters found (Aletheia, AionCore, aion2t) publish no parser source; [aion2-protocol-research](https://github.com/lunixxx37/aion2-protocol-research) (MIT) repeats a2_protocol-research's names.
