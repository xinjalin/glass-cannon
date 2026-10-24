# Opcode map

Every message type (opcode) in our captures: 285 opcodes, 690,409 messages, from the 26 game-server connections in `glass-cannon-captures` (2026-10-18 to 2026-10-23, world and dungeon servers), server to client, after unpacking bundles. Layouts are in [protocol.md](protocol.md).

- **entity first**: how often the payload starts with an entity ID we saw spawn (yes: 60 %+). A message about one player or mob.
- **Questlog's name**: from the message list in Questlog's app.
- **other projects**: what other public projects call it, by project (see [Sources](#sources)). Their names and Questlog's are leads, not checked against our captures unless a row in the first two tables says so.

| | opcodes | messages |
| --- | ---: | ---: |
| Decoded and used by the meter | 46 | 373,337 |
| Understood, not used yet | 4 | 73 |
| Named elsewhere only | 60 | 241,280 |
| Unknown | 175 | 75,719 |
| Named elsewhere, never seen | 33 | 0 |

## Decoded and used by the meter

| opcode | count | what | Questlog's name | other projects |
| --- | ---: | --- | --- | --- |
| `0x3804` | 125,178 | damage / heal / effect hit | SkillEffect_NT |  |
| `0x3802` | 106,116 | cast: caster, skill, target, cooldown, charges | SkillStart_NT | Aion2Flow: compact control; RATmeter: skill cast; a2_protocol-research: action or cast |
| `0x382b` | 54,601 | buff / debuff active (refresh) | AbnormalChange_NT |  |
| `0x8d00` | 38,482 | HP update | CharPointUpdate_NT |  |
| `0x382a` | 12,214 | buff / debuff added | AbnormalAdd_NT |  |
| `0x3805` | 9,886 | damage and heals over time | SkillEffectByAbnormal_NT |  |
| `0x382c` | 7,225 | buffs removed | AbnormalRemoveList_NT |  |
| `0x364a` | 5,047 | your stats tick (~1/s): names your entity | CharStatsUpdate_NT | A2Tools: stats changed |
| `0xe005` | 2,999 | a mob's stagger gauge | UpdateGroggyInfo_NT | a2_protocol-research: groggy or guard |
| `0x921c` | 2,465 | a party member's update (level, gear score, combat power, position) | PartyUpdateMemberStatus_NT |  |
| `0x3641` | 1,880 | mob / NPC appears: entity, type, position, HP, max HP; summons; boss copies of players | NpcAppear_NT |  |
| `0x3642` | 1,794 | NPC leaves view: reason 3 = died | NpcDisappear_NT | A2Tools: death; a2_protocol-research: entity removed or death |
| `0x8d21` | 1,478 | in / out of combat; a boss's berserk time | CombatStateUpdate_NT | Aion2Flow: battle toggle |
| `0x3645` | 1,142 | another player appears: entity, name, character data | CharAppear_NT |  |
| `0x3847` | 909 | your cooldowns changing (reset, reduction) | SkillCooltimeUpdate_NT | Aion2Flow: cooldowns; RATmeter: skill cooldown |
| `0x3835` | 456 | what a mob is targeting | TargetUpdate_NT | Aion2Flow: sidecar; a2_protocol-research: summon or skill sequence |
| `0x3603` | 396 | the game's ping reply (client time, server time) | Ping_RS | Aion2Flow: round trip; RATmeter: server time |
| `0x3809` | 336 | a cast with a cast time starting (target frame's cast bar) | SkillCastingStart_NT | a2_protocol-research: skill cast |
| `0x8d04` | 319 | a death: who, by which skill, killer | CharDie_NT | Aion2Flow: nickname; A2Tools: summon ownership; RATmeter: entity death |
| `0x3834` | 114 | your target | SetMainTarget_RS |  |
| `0x9702` | 102 | party finder group roster (ready, inside, leader) | PartyFinderGroup_NT | A2Tools: party roster; RATmeter: party info; a2_protocol-research: party state |
| `0x922d` | 42 | a dungeon's objective progress: its stage and step | MapEventUpdateProgress_NT |  |
| `0x380c` | 27 | a cast stopped before it went off | SkillCastingStop_NT |  |
| `0x9101` | 16 | world map contents: field boss timers | WorldMapContentsInfo_RS | A2Tools: field boss map |
| `0x9200` | 13 | your party's roster | PartyInfo_NT | Aion2Flow: party member list |
| `0x610c` | 13 | one content ticket changed | ContentsTicketUpdate_NT | A2Tools: Odyle energy update |
| `0x3633` | 9 | you appear: your entity, name, character data (login, zone change) | MyCharAppear_NT |  |
| `0x4002` | 9 | you're on this map (each server connection starts with it) |  | Aion2Flow: state |
| `0x9716` | 9 | a group member readied up / stopped | PartyFinderReady_NT |  |
| `0x6100` | 6 | dungeon progress: entered, countdown over, complete | DungeonState_NT |  |
| `0x970b` | 6 | a member joined the finder group | PartyFinderJoin_NT |  |
| `0x971d` | 6 | you left the finder group | PartyFinderLeave_NT |  |
| `0x8d26` | 6 | a player came back to life | UseRebirth_NT |  |
| `0x5100` | 5 | your skills, at login and entering a dungeon | MySkillList_NT |  |
| `0x610b` | 5 | every content ticket; Odyle energy is 60000001 | ContentsTickets_NT | A2Tools: Odyle energy list |
| `0x9221` | 4 | a member left the party |  |  |
| `0x971e` | 4 | a member left the finder group |  |  |
| `0x572e` | 4 | a dungeon's middle boss was killed: its mob type (stage splits) | PartyDungeonRewardMiddleBossKill_NT |  |
| `0x922e` | 4 | joining a dungeon's objectives (map event) | MapEventJoin_NT | Aion2Flow: map |
| `0x6101` | 2 | dungeon completed | DungeonCompleteResult_NT |  |
| `0x9213` | 2 | you left the party | PartyLeave_RS |  |
| `0x9715` | 2 | you readied up / stopped | PartyFinderSelfReady_NT |  |
| `0x9211` | 1 | party leader changed | PartyChangeLeader_NT |  |
| `0x921f` | 1 | the party broke up | PartyDisband_NT |  |
| `0x972a` | 1 | the finder group removed you | PartyFinderKicked_NT |  |
| `0x931f` | 1 | you left the party (u64 party, u8 reason; with `0x971d` from a finder group) | PartyLeave_NT |  |

## Understood, not used yet

| opcode | count | what | Questlog's name | other projects |
| --- | ---: | --- | --- | --- |
| `0x3822` | 38 | about 14.5 s after a Recuperation cast: probably a skill ready again |  |  |
| `0x3656` | 15 | your combat power (u32) | CharCombatPowerUpdate_NT |  |
| `0x9701` | 11 | party finder listings (post titles, leader names) |  |  |
| `0x8a05` | 9 | carries your permanent character ID (u32) and server |  |  |

## Named elsewhere only (not checked)

| opcode | count | sessions | avg bytes | entity first | Questlog's name | other projects |
| --- | ---: | ---: | ---: | --- | --- | --- |
| `0x3600` | 79,394 | 22 | 8 |  | MapFrame_NT | a2_protocol-research: server tick |
| `0x371d` | 58,483 | 22 | 10.7 | yes |  | Aion2Flow: state; a2_protocol-research: movement |
| `0x3806` | 44,359 | 22 | 8.7 | yes | SkillEnd_NT | Aion2Flow: compact control; a2_protocol-research: cast end or defence |
| `0x371c` | 21,284 | 22 | 20.6 | yes | CharMoveUpdateSnapshot_NT | a2_protocol-research: movement |
| `0x3803` | 10,860 | 17 | 35 | yes |  | a2_protocol-research: NPC position |
| `0x371a` | 5,392 | 22 | 18.3 | yes | CharMoveStop_NT | a2_protocol-research: movement |
| `0x371b` | 5,368 | 22 | 20.2 | yes | CharMoveStart_NT | a2_protocol-research: movement |
| `0x383d` | 4,490 | 14 | 2 |  |  | a2_protocol-research: combat batch |
| `0x921b` | 2,752 | 2 | 32.4 | yes | PartyUpdateMemberHPMP_NT | Aion2Flow: party status; A2Tools: HP / MP update |
| `0x561b` | 1,624 | 5 | 47.9 |  | ItemModificationList_NT |  |
| `0x382d` | 1,280 | 19 | 8 | yes | ImmuneAbnormal_NT |  |
| `0x8a33` | 1,268 | 16 | 38 | yes | GuildDescUpdate_NT |  |
| `0x3818` | 1,136 | 14 | 9 |  | ChainSkillActiveList_NT |  |
| `0x3647` | 1,114 | 21 | 4 | yes | CharDisappear_NT |  |
| `0x373c` | 472 | 19 | 32 | yes | CharWarpLeave_NT |  |
| `0x3634` | 358 | 18 | 55.6 |  | EnvObjSpawn_NT |  |
| `0x5684` | 347 | 20 | 17.7 |  | AchievementCountUpdate_NT | Aion2Flow: wrapped envelope |
| `0x3635` | 302 | 17 | 6 |  | EnvObjDespawn_NT | a2_protocol-research: summon spawn |
| `0x561c` | 225 | 12 | 78.3 | some | ItemEquipChange_NT |  |
| `0x381c` | 180 | 11 | 32.7 | yes | SkillChargingStart_NT |  |
| `0x380e` | 111 | 3 | 45.7 | yes |  | a2_protocol-research: target selected |
| `0x3813` | 91 | 3 | 18 |  | SkillLevelUpdate_NT |  |
| `0x9220` | 79 | 2 | 1.9 | yes | PartySetTargetByBoss_NT |  |
| `0x380f` | 60 | 3 | 13 |  | SkillAdd_NT |  |
| `0x8d57` | 28 | 4 | 121 | some | RetrieveMyLatestSeasonRanking_RS |  |
| `0x3814` | 22 | 3 | 28 | yes | Abnormal_Move_Start_NT |  |
| `0x3848` | 21 | 2 | 32 | yes | SkillRangeNoticeFilterLocation_NT | decoded 2026-10-24: where a boss's cast lands and when (the meter's boss cast line) |
| `0x8d52` | 16 | 2 | 3 | yes | NpcFirstAggro_NT | decoded 2026-10-24: a boss's pull (boss timelines start there) |
| `0x4001` | 11 | 5 | 18.3 |  |  | Aion2Flow: state |
| `0x3621` | 9 | 5 | 46 |  | PrepareEnterMap_NT | Aion2Flow: state; A2Tools: map load |
| `0x3623` | 9 | 5 | 24 |  |  | Aion2Flow: map arrival; A2Tools: zone change; a2_protocol-research: zone |
| `0x3646` | 9 | 5 | 3 | yes | CharBarrierInfo_NT | Aion2Flow: state |
| `0x3649` | 9 | 5 | 858.7 |  | CharAllStats_RS | Aion2Flow: state; A2Tools: all stats; RATmeter: player stats |
| `0x920d` | 6 | 2 | 143 |  | PartyAddMember_NT | Aion2Flow: party profile; read now: a member joined your party (confirmed 2026-10-24) |
| `0x3816` | 6 | 2 | 24 | yes | Abnormal_Move_Instant_NT |  |
| `0x8d33` | 6 | 2 | 25 |  | GatherSourceCompleted_NT |  |
| `0x9000` | 5 | 3 | 2831.2 |  | RideInitData_NT |  |
| `0x5683` | 5 | 3 | 39997.8 |  | AchievementList_NT |  |
| `0x3900` | 5 | 3 | 361 |  | SpecializedSkillLoad_NT |  |
| `0x400a` | 5 | 3 | 9 |  | ArenaMatchingInfo_NT |  |
| `0xe30b` | 5 | 3 | 25 |  | MonolithList_NT |  |
| `0x5611` | 5 | 3 | 17795.6 |  | ItemDepotList_RS |  |
| `0x3620` | 5 | 3 | 2032.6 |  |  | RATmeter: session ID link |
| `0xe226` | 5 | 3 | 901.2 |  | DaevanionBoardInfo_NT |  |
| `0x8aaf` | 5 | 3 | 61 |  | GuildDecoList_NT |  |
| `0x8d44` | 5 | 3 | 9 |  | ClientEnvSetting_NT |  |
| `0xe256` | 5 | 3 | 1685 |  |  | A2Tools: Supply Requests boards |
| `0x8d5b` | 5 | 3 | 1811.8 |  | SkinList_NT |  |
| `0x5672` | 5 | 3 | 31 |  | WingList_NT |  |
| `0x5679` | 5 | 3 | 765.2 |  | TitleList_NT |  |
| `0x3611` | 4 | 2 | 276 |  |  | a2_protocol-research: server RSA handshake |
| `0x3615` | 4 | 2 | 78 |  | Login_RS | a2_protocol-research: login or server transfer |
| `0x3616` | 4 | 2 | 27.8 |  |  | a2_protocol-research: game server info |
| `0x922f` | 4 | 2 | 4 |  | MapEventLeave_NT | Aion2Flow: map |
| `0x900d` | 3 | 2 | 9 |  | RideSoulPointIncrease_NT |  |
| `0x5676` | 3 | 1 | 7 |  | WingChange_NT |  |
| `0x6109` | 2 | 2 | 4 |  | DungeonDeathLimit_NT | deaths against the dungeon's limit (Questlog's layout; not used) |
| `0x3821` | 2 | 2 | 9 | yes | SkillChargingStop_NT | decoded 2026-10-24: a charge let go; its cooldown starts |
| `0x8d2f` | 2 | 2 | 646 |  |  | a2_protocol-research: notice |
| `0x8d15` | 1 | 1 | 51 |  | EventFuncAction_NT |  |

Where a name doesn't fit our captures:

- `0x3803` isn't an NPC's position: 6,054 of its 6,832 messages are an entity, a byte, another entity, then a player skill ID.
- `0x3635` is Questlog's object despawn, not a summon spawn: 200 of its 239 remove an ID an earlier `0x3634` (object spawn) brought in.
- `0x383d` ("combat batch") is always 2 bytes, `00 00`.

## Unknown

No name anywhere. Sorted by count; the busy ones are the most worth a look.

| opcode | count | sessions | avg bytes | entity first | first bytes |
| --- | ---: | ---: | ---: | --- | --- |
| `0x372a` | 30,573 | 20 | 21.4 | yes | `2e 17 03 76 71 d2 e5 93 44 d5` |
| `0x3729` | 11,366 | 20 | 30.7 | yes | `2e 03 03 86 34 a1 c7 a5 6b 06` |
| `0x383b` | 4,491 | 14 | 2 |  | `00 00` |
| `0x372f` | 2,892 | 19 | 32.1 | yes | `ff 42 02 3b 56 92 c7 4a 43 04` |
| `0x3720` | 2,590 | 21 | 20.6 | yes | `a6 38 0b f3 cc 5e c1 b5 8b dd` |
| `0x3801` | 2,469 | 15 | 7 |  | `00 00 00 d2 15 17 01` |
| `0x920e` | 2,311 | 3 | 7 | yes | `eb 77 00` |
| `0x921a` | 2,274 | 2 | 6 | yes | `b4 48 6f ff d2 0a` |
| `0x364b` | 1,833 | 19 | 10.9 | yes | `e6 0b 01 cd 00 e2 09 00 00` |
| `0x3819` | 1,136 | 14 | 5 |  | `01 54 c4 18 01` |
| `0x371f` | 1,094 | 20 | 30 | yes | `a3 67 02 f1 11 9e c7 19 36 01` |
| `0x371e` | 1,058 | 21 | 30.6 | yes | `a6 38 14 3c aa 65 c3 fb 87 80` |
| `0x3724` | 968 | 6 | 30 | yes | `ff 42 01 de 2f 9b c7 6f 9f 01` |
| `0x3725` | 927 | 8 | 20.9 | yes | `ff 42 0f ae 41 f9 41 65 68 07` |
| `0x3728` | 925 | 19 | 30.7 | yes | `a3 67 07 03 31 ac 9d c7 2f 94` |
| `0x3832` | 831 | 19 | 5.1 | yes | `92 6b 00 00` |
| `0x3638` | 721 | 20 | 6 |  | `cc 98 01 a6 38 01` |
| `0x372e` | 707 | 19 | 53 | yes | `ff 42 73 a5 66 03 01 01 1c b8` |
| `0x363a` | 704 | 20 | 13 |  | `cc 98 01 a6 38 01 00 00 00 00` |
| `0x8d6d` | 598 | 20 | 3 |  | `cc 98 01` |
| `0x363b` | 562 | 19 | 3 |  | `a6 da 01` |
| `0x3722` | 512 | 19 | 2 | yes | `e3 7c` |
| `0x3721` | 472 | 20 | 2 | yes | `e3 7c` |
| `0x3734` | 357 | 2 | 32 | yes | `b4 48 a2 76 26 46 14 3f f7 46` |
| `0x3831` | 229 | 14 | 6 |  | `00 00 f5 a1 02 00 00` |
| `0x372d` | 216 | 10 | 11.5 | yes | `92 6b 07 0c ea 41 bc d2 00 fb` |
| `0x8d74` | 170 | 8 | 6 | yes | `ad 7c 00 05 00 01` |
| `0x8d72` | 166 | 8 | 4 | yes | `ad 7c 00 01` |
| `0x3723` | 164 | 8 | 30.4 | yes | `ff 42 02 34 4e 99 c7 b0 52 02` |
| `0x561e` | 152 | 4 | 66.2 |  | `01 4b ac 00 00 dc 5c ac c7 7d` |
| `0x561f` | 151 | 4 | 22.8 |  | `02 6a b3 00 00 a7 52 00 00 19` |
| `0x8d05` | 138 | 17 | 3 |  | `cc 98 01` |
| `0x364e` | 135 | 14 | 11.4 | yes | `f8 7d 1f 3a ed 42 5a 3a ed 42` |
| `0x9216` | 129 | 2 | 12 |  | `00 00 00 00 e4 7d 01 00 00 00` |
| `0x3732` | 114 | 8 | 9.9 | yes | `85 19 08 00 00 ac 02 01` |
| `0x8d93` | 104 | 2 | 7 |  | `8b 92 02 01 00 00 00` |
| `0x9003` | 100 | 16 | 9 | yes | `b2 74 53 05 00 00 01 00 01` |
| `0x9006` | 94 | 18 | 4 | yes | `f8 7d 01 00` |
| `0x3746` | 91 | 7 | 6 | yes | `ff 18 1e b6 e7 46` |
| `0x5626` | 59 | 9 | 6 | some | `db 6f 3c 03 00 01` |
| `0x3743` | 59 | 3 | 4 | yes | `cb 2d 00 00` |
| `0x8d03` | 45 | 5 | 6 | yes | `ff 18 00 00 00 00` |
| `0x3741` | 45 | 5 | 4 | yes | `ff 18 00 00` |
| `0x3742` | 43 | 5 | 2 | yes | `ff 18` |
| `0x3744` | 41 | 5 | 4 | yes | `ff 18 00 00` |
| `0x9508` | 39 | 2 | 12 | yes | `b2 84 02 25 04 00 00 02 00 00` |
| `0x8d34` | 39 | 4 | 7 | yes | `9e 72 00 3a 94 d6 02` |
| `0x372b` | 38 | 11 | 63.2 | yes | `92 6b 03 15 cd 94 c7 8a a3 01` |
| `0x9507` | 37 | 3 | 12 | yes | `ea a5 02 25 04 00 00 01 00 00` |
| `0x3639` | 34 | 5 | 11.1 |  | `bd f4 02 06 19 d1 3c 03 00 00` |
| `0x8d02` | 34 | 5 | 6 | yes | `ff 18 d0 07 00 00` |
| `0x5616` | 33 | 3 | 12 |  | `ef fa 1d 00 00 37 00 00 23 55` |
| `0xe324` | 31 | 3 | 76.9 |  | `02 4c 4e 00 00 a0 86 01 00 50` |
| `0x3730` | 30 | 7 | 19 | yes | `85 19 f3 2e 9f c7 af 77 05 c8` |
| `0x3815` | 21 | 2 | 16.6 | yes | `c1 e3 05 96 e9 2e 45 28 69 86` |
| `0x385c` | 18 | 2 | 15 | yes | `d5 dc 02 01 00 aa c3 01 09 01` |
| `0x8a39` | 17 | 9 | 8.2 |  | `05 57 75 6d 62 6f` |
| `0x910c` | 16 | 2 | 7 |  | `00 00 56 04 00 00 00` |
| `0x6003` | 15 | 1 | 123 |  | `00 00 b5 e9 0c 47 7d 52 0b 4e` |
| `0x3726` | 15 | 2 | 4 | yes | `fd 44 01 04` |
| `0x3637` | 13 | 3 | 14.3 |  | `00 00 00 86 89 04 00 00 00 00` |
| `0xe215` | 10 | 3 | 12 |  | `00 00 01 00 00 00 00 00 00 00` |
| `0x8d4b` | 10 | 5 | 3 | yes | `bb 42 00` |
| `0x8a03` | 9 | 5 | 304.7 |  | `00 00 05 54 49 45 52 31 0a 43` |
| `0x8a45` | 9 | 5 | 14 |  | `00 00 02 00 00 00 61 c8 e7 a2` |
| `0x4301` | 9 | 5 | 1 |  | `00` |
| `0x9002` | 8 | 2 | 11 | yes | `cb 2d 2d 3b 24 04 00 00 00 00` |
| `0x363e` | 8 | 2 | 3 |  | `13 38 00` |
| `0x564d` | 8 | 2 | 7 |  | `00 00 23 55 a7 1f 01` |
| `0xffa9` | 7 | 3 | 16 |  | `68 13 00 00 00 00 00 00 60 e3` |
| `0x3836` | 6 | 2 | 4 |  | `00 ba fd 03` |
| `0x9713` | 6 | 2 | 6 |  | `00 00 00 00 00 00` |
| `0x8d31` | 6 | 2 | 128.3 |  | `00 00 ba fd 03 01 00 00 00 01` |
| `0x3733` | 6 | 2 | 41 | yes | `b4 48 02 1f be 2d 46 ac 93 f7` |
| `0x8d3e` | 6 | 2 | 12 |  | `01 00 00 00 76 cc cf 62 a1 01` |
| `0xffa6` | 6 | 2 | 16 |  | `78 00 00 00 00 00 00 00 8d ab` |
| `0x5600` | 5 | 3 | 511.8 |  | `11 01 00 00 00 00 00 00 00 00` |
| `0x561d` | 5 | 3 | 8 |  | `e7 07 00 00 e7 07 00 00` |
| `0xe223` | 5 | 3 | 12 |  | `80 c1 f5 52 a1 01 00 00 00 00` |
| `0xe224` | 5 | 3 | 4 |  | `02 01 02 02` |
| `0xe22e` | 5 | 3 | 16 |  | `05 01 3c 01 02 00 00 03 24 00` |
| `0x9232` | 5 | 3 | 269 |  | `43 6b 69 0f 00 61 0d 10 00 62` |
| `0x5633` | 5 | 3 | 1 |  | `00` |
| `0xe237` | 5 | 3 | 128.2 |  | `02 03 02 00 00 00 01 00 00 00` |
| `0x5648` | 5 | 3 | 10 |  | `01 01 b3 14 99 1f 00 00 00 00` |
| `0x564b` | 5 | 3 | 7.4 |  | `00` |
| `0xe251` | 5 | 3 | 1 |  | `00` |
| `0xe257` | 5 | 3 | 53 |  | `34 01 02 03 04 05 06 07 08 09` |
| `0x3657` | 5 | 3 | 9 |  | `01 00 00 00 00 00 00 00 00` |
| `0xe25a` | 5 | 3 | 216 |  | `00 00 d4 01 14 00 00 00 00 00` |
| `0xe25c` | 5 | 3 | 3 |  | `01 4b 01` |
| `0xe262` | 5 | 3 | 118 |  | `03 04 02 02 00 00 00 00 00 00` |
| `0xe263` | 5 | 3 | 5 |  | `02 46 00 5a 00` |
| `0xe274` | 5 | 3 | 41 |  | `01 00 c7 09 59 d1 02 00 00 00` |
| `0x5682` | 5 | 3 | 4 |  | `00 00 23 00` |
| `0xe28c` | 5 | 3 | 1115.6 |  | `01 01 01 14 98 eb 14 00 00 09` |
| `0x56a8` | 5 | 3 | 10 |  | `00 00 00 00 00 00 00 00 00 00` |
| `0x56ac` | 5 | 3 | 43 |  | `2a 55 15 05 0e 06 07 08 49 09` |
| `0x56b6` | 5 | 3 | 1 |  | `00` |
| `0xe32a` | 5 | 3 | 43 |  | `03 01 dc 05 00 00 00 00 00 00` |
| `0x6103` | 5 | 3 | 10 |  | `01 07 a1 bb 0d 00 01 5a c0 2f` |
| `0x6000` | 5 | 3 | 64.8 |  | `09 03 fd ea 0c 02 a5 4a 3d 4e` |
| `0x8d09` | 5 | 3 | 3262 |  | `94 01 10 b4 27 00 04 01 01 00` |
| `0x570f` | 5 | 3 | 3 |  | `00 00 00` |
| `0xe200` | 5 | 3 | 12 |  | `00 80 9d 11 fe ff ff ff ff 00` |
| `0x5712` | 5 | 3 | 37 |  | `06 c2 27 09 00 01 01 cc 27 09` |
| `0x8d1c` | 5 | 3 | 1418 |  | `00 00 00 00 00 00 00 00 0f 10` |
| `0x9330` | 5 | 3 | 1 |  | `00` |
| `0x9331` | 5 | 3 | 101 |  | `02 04 02 00 00 00 01 00 00 00` |
| `0x572d` | 5 | 3 | 1 |  | `00` |
| `0x8d37` | 5 | 3 | 77 |  | `09 64 00 00 00 0a 00 00 00 2c` |
| `0xe33b` | 5 | 3 | 190 |  | `09 22 04 00 00 00 57 f7 d4 f6` |
| `0xe33d` | 5 | 3 | 4 |  | `00 00 00 00` |
| `0xe33e` | 5 | 3 | 1 |  | `00` |
| `0xe342` | 5 | 3 | 102.4 |  | `09 f7 ae 15 00 00 c8 00 00 76` |
| `0xe343` | 5 | 3 | 64 |  | `03 58 de 03 00 00 00 c7 09 01` |
| `0xe349` | 5 | 3 | 11 |  | `00 00 80 d9 86 4f a1 01 00 00` |
| `0x8d4d` | 5 | 3 | 10 |  | `00 00 b8 a9 15 00 00 cf 00 00` |
| `0xe34e` | 5 | 3 | 2 |  | `00 00` |
| `0x8d53` | 5 | 3 | 1807 |  | `22 01 10 53 65 61 73 6f 6e 5f` |
| `0x8d58` | 5 | 3 | 1 |  | `00` |
| `0x8d7e` | 5 | 3 | 1133.2 |  | `8f 01 13 83 36 07 00 00 00 00` |
| `0x8d83` | 5 | 3 | 1 |  | `00` |
| `0x8d8c` | 5 | 3 | 1070.8 |  | `80 01 81 59 b7 00 00 00 00 00` |
| `0xffa5` | 5 | 3 | 16 |  | `3d 03 00 00 00 00 00 00 11 91` |
| `0xffa7` | 5 | 3 | 2 |  | `07 01` |
| `0xffac` | 5 | 3 | 24 |  | `03 01 02 00 00 00 02 17 00 00` |
| `0x9501` | 5 | 3 | 145 |  | `24 02 00 00 00 03 00 00 00 04` |
| `0x6001` | 4 | 2 | 29 |  | `03 a9 b8 0d 01 3c 1d 29 63 a1` |
| `0x5613` | 4 | 2 | 14 |  | `04 03 f6 95 17 00 00 3d 00 00` |
| `0x5621` | 4 | 2 | 11 |  | `00 00 b0 51 00 00 bd 98 17 01` |
| `0x9237` | 4 | 2 | 6 |  | `00 00 67 0d 10 00` |
| `0x364d` | 4 | 2 | 1 |  | `00` |
| `0x3655` | 4 | 1 | 6 |  | `a6 38 00 00 00 00` |
| `0x6108` | 4 | 2 | 1 |  | `00` |
| `0x3735` | 4 | 2 | 24 | yes | `fd 44 0f d3 28 03 df f8 23 c5` |
| `0x9215` | 3 | 1 | 2 |  | `00 00` |
| `0x9235` | 3 | 2 | 13 |  | `01 e0 81 04 00 83 24 d5 00 54` |
| `0xe34c` | 3 | 3 | 1174 |  | `01 01 00 00 00 05 41 49 4f 4e` |
| `0x8d4e` | 3 | 2 | 6 | yes | `84 39 24 f4 cb 1f` |
| `0x8c07` | 2 | 1 | 19 |  | `00 00 38 1b 0f 00 00 9f 2b 2d` |
| `0x5608` | 2 | 2 | 15.5 |  | `0f 00 00 06 00 c0 44 13 01 01` |
| `0x4402` | 2 | 2 | 2 |  | `00 00` |
| `0x3817` | 2 | 2 | 4 | yes | `ff 77 90 03` |
| `0x4400` | 2 | 2 | 103 |  | `04 00 e4 05 0f 1a 04 00 00 00` |
| `0x4403` | 2 | 2 | 57.5 |  | `03 01 e4 05 0f 1a 04 00 00 00` |
| `0x3650` | 2 | 1 | 2637.5 |  | `00 00 07 03 41 6a 61 09 00 00` |
| `0xe293` | 2 | 2 | 278 |  | `01 0c 03 01 00 90 cf 12 01 04` |
| `0x5713` | 2 | 2 | 9 |  | `02 8a 1b 23 00 8b 1b 23 00` |
| `0x5715` | 2 | 2 | 212 |  | `00 00 02 1c 37 33 70 37 a0 0f` |
| `0x9719` | 2 | 1 | 9 |  | `a7 de 03 00 00 00 df 05 01` |
| `0x9319` | 2 | 2 | 0 |  | `` |
| `0x971f` | 2 | 2 | 60 |  | `1e 03 58 de 03 00 00 00 c7 09` |
| `0x9321` | 2 | 1 | 10 |  | `01 b4 48 b0 59 09 01 ec e0 06` |
| `0x3731` | 2 | 2 | 19 | yes | `ed 5b bc 56 96 c7 1c ae 03 c8` |
| `0x9339` | 2 | 1 | 2 |  | `00 00` |
| `0x8d48` | 2 | 2 | 2 |  | `00 00` |
| `0x8d92` | 2 | 2 | 3 | yes | `cb 2d 01` |
| `0xffad` | 2 | 2 | 2 |  | `02 01` |
| `0x9634` | 2 | 1 | 2 |  | `12 3e` |
| `0x8c09` | 1 | 1 | 15 |  | `00 06 38 1b 0f 00 01 b8 50 29` |
| `0x900c` | 1 | 1 | 8 | yes | `8a 1a 00 00 a2 04 00 00` |
| `0x5617` | 1 | 1 | 14 | yes | `e6 5f c1 b3 11 00 00 12 00 00` |
| `0x9226` | 1 | 1 | 2 |  | `00 00` |
| `0x9227` | 1 | 1 | 121 |  | `05 00 00 00 00 00 00 00 00 00` |
| `0x5673` | 1 | 1 | 9 |  | `e6 2f 00 00 85 fb 69 02 00` |
| `0x9706` | 1 | 1 | 7 |  | `00 00 01 00 00 00 00` |
| `0x971b` | 1 | 1 | 4 |  | `34 b0 08 00` |
| `0x8d3f` | 1 | 1 | 0 |  | `` |
| `0x8d41` | 1 | 1 | 2 |  | `00 00` |
| `0x3740` | 1 | 1 | 18 | yes | `cb 2d 06 02 06 9b d8 c6 e6 bf` |
| `0x3749` | 1 | 1 | 2 |  | `9d 73` |
| `0x372c` | 1 | 1 | 21 | yes | `9f 7c f6 ec 96 c7 cc 94 03 c8` |
| `0x373f` | 1 | 1 | 18 | yes | `b6 58 00 00 15 d1 80 46 92 67` |
| `0x5619` | 1 | 1 | 14 | yes | `e6 5f c1 b3 11 00 00 12 00 00` |

## Named elsewhere, never in our captures

Some may be older opcodes: A2Tools notes the game shifted opcodes in 2026-06.

| opcode | Questlog's name | other projects | meter |
| --- | --- | --- | --- |
| `0x3329` |  | Aion2Flow: legion metadata |  |
| `0x361a` |  | a2_protocol-research: name check |  |
| `0x3640` |  | Aion2Flow: summon; A2Tools: summon spawn; a2_protocol-research: spawn variant |  |
| `0x3644` |  | A2Tools: player spawn (variant); a2_protocol-research: player info variant |  |
| `0x4000` | CharPhaseUpdate_NT |  |  |
| `0x569e` | CustomizingPresetList_RS |  |  |
| `0x8d46` | ServerMsg_NT |  |  |
| `0x8d91` | BroadcastLevelUpdate_NT |  |  |
| `0x9009` | CharRideAcquire_NT |  |  |
| `0x900e` | RideSoulPointDecrease_NT |  |  |
| `0x9219` | PartyKickout_NT |  | kicked from the party (u8: by vote) |
| `0x9222` | PartyReconnectMember_NT |  | a party member back online: a roster entry (read like `0x920d`) |
| `0x9409` |  | Aion2Flow: nickname |  |
| `0x940b` |  | Aion2Flow: nickname |  |
| `0x9602` | ForceInfo_NT | Aion2Flow: Force roster snapshot | the Force's roster: its members count as your party (confirmed: 20 members) |
| `0x960a` | ForceAddMember_NT | Aion2Flow: Force roster profile | a Force member added |
| `0x961a` | ForceDisband_NT |  | the Force (20-player raid group) broke up |
| `0x961b` | ForceReconnectMember_NT | Aion2Flow: Force profile | a Force member back |
| `0x961d` |  | Aion2Flow: Force member | a Force member |
| `0x961e` |  | Aion2Flow: Force member list | the Force's member list |
| `0x9622` | ForceRemoveMember_NT |  | a member left the Force: u64 member, u64 party, u8 reason (confirmed) |
| `0x962b` | ForceUpdateMemberHPMP_NT | Aion2Flow: Force status |  |
| `0x962c` | ForceUpdateMemberStatus_NT |  |  |
| `0x962e` | ForceLeave_NT |  | you left the Force (20-player raid group) |
| `0xe241` | StartNpcInteraction_RS |  |  |
| `0xe243` | FinishNpcInteraction_RS |  |  |
| `0xe302` | AbyssArtifactStatus_NT |  |  |
| `0xe303` | AbyssArtifactUpdate_NT |  |  |
| `0xe305` | AbyssArtifactEventInfo_NT | A2Tools: abyss artifact entry | entering the Abyss: who holds its artifacts (untested) |
| `0xe307` | AbyssArtifactEventInfo_RS | A2Tools: abyss artifacts map | the Abyss map: who holds the artifacts (confirmed 2026-10-23) |
| `0xe308` | AbyssArtifactEventUpdate_NT |  |  |
| `0xe326` | BattleScoreUpdate_NT |  |  |
| `0xe335` | CutsceneComplete_RS |  |  |

## Sources

Only names and descriptions were taken from these, as leads to check against our own captures. No code or data was copied.

| project | license | what it names |
| --- | --- | --- |
| Questlog (questlog.gg's desktop app) | proprietary | its message list (names only) |
| [Aion2Flow](https://github.com/cloris-chan/Aion2Flow) by cloris-chan | GPL-3.0 | its opcode dispatcher: party and Force rosters, map state, nicknames |
| [A2Tools DPS Meter](https://github.com/taengu/A2Tools-DPS-Meter) by taengu | GPL-3.0 | its capture modules: zone and map loads, stats, Odyle energy, Supply Requests, abyss artifacts |
| [RATmeter](https://github.com/Kuroukihime/AIon2-Dps-Meter) by Kuroukihime | GPL-3.0 | its opcode table (`PacketOpcodes.cs`) |
| [a2_protocol-research](https://github.com/SixShoot/a2_protocol-research) by SixShoot | MIT | `docs/OPCODES.md`; mostly client to server, which a meter doesn't read |
| [NOIA2](https://github.com/ZDYoung0519/NOIA2) by ZDYoung0519 | GPL-3.0 | combat power and field boss timers (already in [protocol.md](protocol.md)) |

Searched 2026-10-23. The other meters found (Aletheia, AionCore, aion2t) publish no parser source.
