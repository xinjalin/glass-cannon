# Opcode map

Every message type (opcode) in our captures: 280 opcodes, 527,465 messages, from the 23 game-server connections in `glass-cannon-captures` (2026-10-18 to 2026-10-22, world and dungeon servers), server to client, after unpacking bundles. Layouts are in [protocol.md](protocol.md).

- **entity first**: how often the payload starts with an entity ID we saw spawn (yes: 60 %+). A message about one player or mob.
- **Questlog's name**: from the message list in Questlog's app.
- **other projects**: what other public projects call it, by project (see [Sources](#sources)). Their names and Questlog's are leads, not checked against our captures unless a row in the first two tables says so.

| | opcodes | messages |
| --- | ---: | ---: |
| Decoded and used by the meter | 36 | 287,957 |
| Understood, not used yet | 4 | 48 |
| Named elsewhere only | 69 | 186,533 |
| Unknown | 171 | 52,927 |
| Named elsewhere, never seen | 34 | 0 |

## Decoded and used by the meter

| opcode | count | what | Questlog's name | other projects |
| --- | ---: | --- | --- | --- |
| `0x3804` | 96,310 | damage / heal / effect hit | SkillEffect_NT |  |
| `0x3802` | 81,208 | cast: caster, skill, target, cooldown, charges | SkillStart_NT | Aion2Flow: compact control; RATmeter: skill cast; a2_protocol-research: action or cast |
| `0x382b` | 44,535 | buff / debuff active (refresh) | AbnormalChange_NT |  |
| `0x8d00` | 29,365 | HP update | CharPointUpdate_NT |  |
| `0x382a` | 9,811 | buff / debuff added | AbnormalAdd_NT |  |
| `0x3805` | 7,160 | damage and heals over time | SkillEffectByAbnormal_NT |  |
| `0x382c` | 5,755 | buffs removed | AbnormalRemoveList_NT |  |
| `0x364a` | 4,172 | your stats tick (~1/s): names your entity | CharStatsUpdate_NT | A2Tools: stats changed |
| `0xe005` | 1,569 | a mob's stagger gauge | UpdateGroggyInfo_NT | a2_protocol-research: groggy or guard |
| `0x3641` | 1,371 | mob / NPC appears: entity, type, position, HP, max HP; summons; boss copies of players | NpcAppear_NT |  |
| `0x921c` | 1,349 | a party member's update (level, gear score, combat power, position) | PartyUpdateMemberStatus_NT |  |
| `0x3642` | 1,341 | NPC leaves view: reason 3 = died | NpcDisappear_NT | A2Tools: death; a2_protocol-research: entity removed or death |
| `0x8d21` | 1,174 | in / out of combat; a boss's berserk time | CombatStateUpdate_NT | Aion2Flow: battle toggle |
| `0x3645` | 1,060 | another player appears: entity, name, character data | CharAppear_NT |  |
| `0x3847` | 690 | your cooldowns changing (reset, reduction) | SkillCooltimeUpdate_NT | Aion2Flow: cooldowns; RATmeter: skill cooldown |
| `0x3603` | 320 | the game's ping reply (client time, server time) | Ping_RS | Aion2Flow: round trip; RATmeter: server time |
| `0x3835` | 283 | what a mob is targeting | TargetUpdate_NT | Aion2Flow: sidecar; a2_protocol-research: summon or skill sequence |
| `0x8d04` | 255 | a death: who, by which skill, killer | CharDie_NT | Aion2Flow: nickname; A2Tools: summon ownership; RATmeter: entity death |
| `0x9702` | 81 | party finder group roster (ready, inside, leader) | PartyFinderGroup_NT | A2Tools: party roster; RATmeter: party info; a2_protocol-research: party state |
| `0x3834` | 73 | your target | SetMainTarget_RS |  |
| `0x9101` | 16 | world map contents: field boss timers | WorldMapContentsInfo_RS | A2Tools: field boss map |
| `0x9200` | 10 | your party's roster | PartyInfo_NT | Aion2Flow: party member list |
| `0x3633` | 7 | you appear: your entity, name, character data (login, zone change) | MyCharAppear_NT |  |
| `0x4002` | 7 | you're on this map (each server connection starts with it) |  | Aion2Flow: state |
| `0x9716` | 7 | a group member readied up / stopped | PartyFinderReady_NT |  |
| `0x970b` | 5 | a member joined the finder group | PartyFinderJoin_NT |  |
| `0x971d` | 5 | you left the finder group | PartyFinderLeave_NT |  |
| `0x9221` | 4 | a member left the party |  |  |
| `0x971e` | 4 | a member left the finder group |  |  |
| `0x6100` | 3 | dungeon progress: entered, countdown over, complete | DungeonState_NT |  |
| `0x9213` | 2 | you left the party | PartyLeave_RS |  |
| `0x6101` | 1 | dungeon completed | DungeonCompleteResult_NT |  |
| `0x9211` | 1 | party leader changed | PartyChangeLeader_NT |  |
| `0x921f` | 1 | the party broke up | PartyDisband_NT |  |
| `0x9715` | 1 | you readied up / stopped | PartyFinderSelfReady_NT |  |
| `0x972a` | 1 | the finder group removed you | PartyFinderKicked_NT |  |

## Understood, not used yet

| opcode | count | what | Questlog's name | other projects |
| --- | ---: | --- | --- | --- |
| `0x3822` | 22 | about 14.5 s after a Recuperation cast: probably a skill ready again |  |  |
| `0x9701` | 10 | party finder listings (post titles, leader names) |  |  |
| `0x3656` | 9 | your combat power (u32) | CharCombatPowerUpdate_NT |  |
| `0x8a05` | 7 | carries your permanent character ID (u32) and server |  |  |

## Named elsewhere only (not checked)

| opcode | count | sessions | avg bytes | entity first | Questlog's name | other projects |
| --- | ---: | ---: | ---: | --- | --- | --- |
| `0x3600` | 63,952 | 21 | 8 |  | MapFrame_NT | a2_protocol-research: server tick |
| `0x371d` | 45,436 | 21 | 10.7 | yes |  | Aion2Flow: state; a2_protocol-research: movement |
| `0x3806` | 31,313 | 21 | 8.7 | yes | SkillEnd_NT | Aion2Flow: compact control; a2_protocol-research: cast end or defence |
| `0x371c` | 18,628 | 21 | 20.7 | yes | CharMoveUpdateSnapshot_NT | a2_protocol-research: movement |
| `0x3803` | 6,832 | 16 | 35 | yes |  | a2_protocol-research: NPC position |
| `0x371a` | 4,156 | 21 | 18.4 | yes | CharMoveStop_NT | a2_protocol-research: movement |
| `0x371b` | 4,058 | 21 | 20.2 | yes | CharMoveStart_NT | a2_protocol-research: movement |
| `0x383d` | 3,158 | 13 | 2 |  |  | a2_protocol-research: combat batch |
| `0x921b` | 1,602 | 1 | 32.3 | yes | PartyUpdateMemberHPMP_NT | Aion2Flow: party status; A2Tools: HP / MP update |
| `0x8a33` | 1,216 | 15 | 38.1 | yes | GuildDescUpdate_NT |  |
| `0x382d` | 1,090 | 18 | 8 | yes | ImmuneAbnormal_NT |  |
| `0x3647` | 1,056 | 20 | 4 | yes | CharDisappear_NT |  |
| `0x561b` | 1,052 | 4 | 46.8 |  | ItemModificationList_NT |  |
| `0x3818` | 881 | 13 | 9 |  | ChainSkillActiveList_NT |  |
| `0x373c` | 396 | 18 | 32 | yes | CharWarpLeave_NT |  |
| `0x3634` | 281 | 17 | 55.5 |  | EnvObjSpawn_NT |  |
| `0x5684` | 240 | 19 | 17.8 |  | AchievementCountUpdate_NT | Aion2Flow: wrapped envelope |
| `0x3635` | 239 | 16 | 6 |  | EnvObjDespawn_NT | a2_protocol-research: summon spawn |
| `0x3809` | 211 | 4 | 33.6 | yes | SkillCastingStart_NT | a2_protocol-research: skill cast |
| `0x561c` | 193 | 11 | 78.8 | yes | ItemEquipChange_NT |  |
| `0x381c` | 122 | 10 | 32.6 | yes | SkillChargingStart_NT |  |
| `0x380e` | 61 | 2 | 45.7 | yes |  | a2_protocol-research: target selected |
| `0x3813` | 55 | 2 | 18 |  | SkillLevelUpdate_NT |  |
| `0x9220` | 40 | 1 | 1.9 | yes | PartySetTargetByBoss_NT |  |
| `0x380f` | 36 | 2 | 13 |  | SkillAdd_NT |  |
| `0x8d57` | 26 | 3 | 120.1 | some | RetrieveMyLatestSeasonRanking_RS |  |
| `0x380c` | 23 | 3 | 10.4 | yes | SkillCastingStop_NT |  |
| `0x922d` | 21 | 1 | 27.8 |  | MapEventUpdateProgress_NT |  |
| `0x3814` | 11 | 2 | 28.1 | yes | Abnormal_Move_Start_NT |  |
| `0x610c` | 10 | 5 | 8.3 |  | ContentsTicketUpdate_NT | A2Tools: Odyle energy update |
| `0x8d52` | 10 | 1 | 3 | yes | NpcFirstAggro_NT |  |
| `0x4001` | 8 | 4 | 17.2 |  |  | Aion2Flow: state |
| `0x3621` | 7 | 4 | 46 |  | PrepareEnterMap_NT | Aion2Flow: state; A2Tools: map load |
| `0x3623` | 7 | 4 | 24 |  |  | Aion2Flow: map arrival; A2Tools: zone change; a2_protocol-research: zone |
| `0x3646` | 7 | 4 | 3 | yes | CharBarrierInfo_NT | Aion2Flow: state |
| `0x3649` | 7 | 4 | 857.1 |  | CharAllStats_RS | Aion2Flow: state; A2Tools: all stats; RATmeter: player stats |
| `0x3848` | 6 | 1 | 32 | yes | SkillRangeNoticeFilterLocation_NT |  |
| `0x3816` | 5 | 1 | 24 | yes | Abnormal_Move_Instant_NT |  |
| `0x8d26` | 4 | 1 | 2 | yes | UseRebirth_NT |  |
| `0x9000` | 3 | 2 | 2825.3 |  | RideInitData_NT |  |
| `0x5683` | 3 | 2 | 39975 |  | AchievementList_NT |  |
| `0x3900` | 3 | 2 | 361 |  | SpecializedSkillLoad_NT |  |
| `0x5100` | 3 | 2 | 1075 |  | MySkillList_NT |  |
| `0x400a` | 3 | 2 | 9 |  | ArenaMatchingInfo_NT |  |
| `0x610b` | 3 | 2 | 491.3 |  | ContentsTickets_NT | A2Tools: Odyle energy list |
| `0x900d` | 3 | 2 | 9 |  | RideSoulPointIncrease_NT |  |
| `0x920d` | 3 | 1 | 143.3 |  | PartyAddMember_NT | Aion2Flow: party profile |
| `0x5611` | 3 | 2 | 17391.7 |  | ItemDepotList_RS |  |
| `0x3620` | 3 | 2 | 2023 |  |  | RATmeter: session ID link |
| `0xe226` | 3 | 2 | 900 |  | DaevanionBoardInfo_NT |  |
| `0x8aaf` | 3 | 2 | 61 |  | GuildDecoList_NT |  |
| `0x8d33` | 3 | 1 | 25 |  | GatherSourceCompleted_NT |  |
| `0x8d44` | 3 | 2 | 9 |  | ClientEnvSetting_NT |  |
| `0xe30b` | 3 | 2 | 25 |  | MonolithList_NT |  |
| `0xe256` | 3 | 2 | 1685 |  |  | A2Tools: Supply Requests boards |
| `0x8d5b` | 3 | 2 | 1804.3 |  | SkinList_NT |  |
| `0x5672` | 3 | 2 | 31 |  | WingList_NT |  |
| `0x5676` | 3 | 1 | 7 |  | WingChange_NT |  |
| `0x5679` | 3 | 2 | 762 |  | TitleList_NT |  |
| `0x3611` | 2 | 1 | 276 |  |  | a2_protocol-research: server RSA handshake |
| `0x3615` | 2 | 1 | 78 |  | Login_RS | a2_protocol-research: login or server transfer |
| `0x3616` | 2 | 1 | 27.5 |  |  | a2_protocol-research: game server info |
| `0x3821` | 2 | 2 | 9 | yes | SkillChargingStop_NT |  |
| `0x572e` | 2 | 1 | 4 |  | PartyDungeonRewardMiddleBossKill_NT |  |
| `0x922e` | 2 | 1 | 68 |  | MapEventJoin_NT | Aion2Flow: map |
| `0x922f` | 2 | 1 | 4 |  | MapEventLeave_NT | Aion2Flow: map |
| `0x8d15` | 1 | 1 | 51 |  | EventFuncAction_NT |  |
| `0x8d2f` | 1 | 1 | 646 |  |  | a2_protocol-research: notice |
| `0x6109` | 1 | 1 | 4 |  | DungeonDeathLimit_NT |  |

Where a name doesn't fit our captures:

- `0x3803` isn't an NPC's position: 6,054 of its 6,832 messages are an entity, a byte, another entity, then a player skill ID.
- `0x3635` is Questlog's object despawn, not a summon spawn: 200 of its 239 remove an ID an earlier `0x3634` (object spawn) brought in.
- `0x383d` ("combat batch") is always 2 bytes, `00 00`.

## Unknown

No name anywhere. Sorted by count; the busy ones are the most worth a look.

| opcode | count | sessions | avg bytes | entity first | first bytes |
| --- | ---: | ---: | ---: | --- | --- |
| `0x372a` | 20,131 | 19 | 21.4 | yes | `2e 17 03 76 71 d2 e5 93 44 d5` |
| `0x3729` | 8,234 | 19 | 30.6 | yes | `2e 03 03 86 34 a1 c7 a5 6b 06` |
| `0x383b` | 3,159 | 13 | 2 |  | `00 00` |
| `0x372f` | 1,957 | 18 | 32.1 | yes | `ff 42 02 3b 56 92 c7 4a 43 04` |
| `0x3720` | 1,865 | 20 | 20.7 | yes | `a6 38 0b f3 cc 5e c1 b5 8b dd` |
| `0x3801` | 1,781 | 14 | 7 |  | `00 00 00 d2 15 17 01` |
| `0x920e` | 1,531 | 2 | 7 | yes | `eb 77 00` |
| `0x921a` | 1,510 | 1 | 6 | yes | `b4 48 6f ff d2 0a` |
| `0x364b` | 1,474 | 18 | 10.8 | yes | `e6 0b 01 cd 00 e2 09 00 00` |
| `0x3819` | 881 | 13 | 5 |  | `01 54 c4 18 01` |
| `0x3728` | 800 | 18 | 30.7 | yes | `a3 67 07 03 31 ac 9d c7 2f 94` |
| `0x371e` | 762 | 20 | 30.6 | yes | `a6 38 14 3c aa 65 c3 fb 87 80` |
| `0x371f` | 740 | 19 | 30 | yes | `a3 67 02 f1 11 9e c7 19 36 01` |
| `0x3832` | 686 | 18 | 5.1 | yes | `92 6b 00 00` |
| `0x3638` | 671 | 19 | 6 |  | `cc 98 01 a6 38 01` |
| `0x363a` | 657 | 19 | 13 |  | `cc 98 01 a6 38 01 00 00 00 00` |
| `0x8d6d` | 559 | 19 | 3 |  | `cc 98 01` |
| `0x363b` | 547 | 18 | 3 |  | `a6 da 01` |
| `0x372e` | 490 | 18 | 53 | yes | `ff 42 73 a5 66 03 01 01 1c b8` |
| `0x3724` | 480 | 5 | 30 | yes | `ff 42 01 de 2f 9b c7 6f 9f 01` |
| `0x3725` | 475 | 7 | 20.9 | yes | `ff 42 0f ae 41 f9 41 65 68 07` |
| `0x3722` | 398 | 18 | 2 | yes | `e3 7c` |
| `0x3721` | 374 | 19 | 2 | yes | `e3 7c` |
| `0x3734` | 204 | 1 | 32 | yes | `b4 48 a2 76 26 46 14 3f f7 46` |
| `0x372d` | 203 | 9 | 11.5 | yes | `92 6b 07 0c ea 41 bc d2 00 fb` |
| `0x3831` | 189 | 13 | 6 |  | `00 00 f5 a1 02 00 00` |
| `0x8d74` | 169 | 7 | 6 | yes | `ad 7c 00 05 00 01` |
| `0x8d72` | 165 | 7 | 4 | yes | `ad 7c 00 01` |
| `0x561e` | 152 | 4 | 66.2 |  | `01 4b ac 00 00 dc 5c ac c7 7d` |
| `0x561f` | 151 | 4 | 22.8 |  | `02 6a b3 00 00 a7 52 00 00 19` |
| `0x364e` | 101 | 13 | 11.3 | yes | `f8 7d 1f 3a ed 42 5a 3a ed 42` |
| `0x8d05` | 97 | 16 | 3 |  | `cc 98 01` |
| `0x8d93` | 86 | 1 | 7 |  | `8b 92 02 01 00 00 00` |
| `0x3723` | 82 | 7 | 30.3 | yes | `ff 42 02 34 4e 99 c7 b0 52 02` |
| `0x3732` | 82 | 7 | 10 | yes | `85 19 08 00 00 ac 02 01` |
| `0x9003` | 80 | 15 | 9 | yes | `b2 74 53 05 00 00 01 00 01` |
| `0x9006` | 72 | 17 | 3.9 | yes | `f8 7d 01 00` |
| `0x9216` | 63 | 1 | 12 |  | `00 00 00 00 e4 7d 01 00 00 00` |
| `0x5626` | 59 | 9 | 6 | some | `db 6f 3c 03 00 01` |
| `0x3746` | 46 | 6 | 6 | yes | `ff 18 1e b6 e7 46` |
| `0x9508` | 39 | 2 | 12 | yes | `b2 84 02 25 04 00 00 02 00 00` |
| `0x9507` | 37 | 3 | 12 | yes | `ea a5 02 25 04 00 00 01 00 00` |
| `0x372b` | 35 | 10 | 63.2 | yes | `92 6b 03 15 cd 94 c7 8a a3 01` |
| `0x3639` | 28 | 4 | 11.1 |  | `bd f4 02 06 19 d1 3c 03 00 00` |
| `0x3743` | 28 | 2 | 4 | yes | `cb 2d 00 00` |
| `0x3744` | 27 | 4 | 4 | yes | `ff 18 00 00` |
| `0x8d03` | 26 | 4 | 6 | yes | `ff 18 00 00 00 00` |
| `0x3741` | 26 | 4 | 4 | yes | `ff 18 00 00` |
| `0x3742` | 25 | 4 | 2 | yes | `ff 18` |
| `0x8d34` | 24 | 3 | 7 | yes | `9e 72 00 3a 94 d6 02` |
| `0x3730` | 21 | 6 | 19 | yes | `85 19 f3 2e 9f c7 af 77 05 c8` |
| `0x8d02` | 20 | 4 | 6 | yes | `ff 18 d0 07 00 00` |
| `0x5616` | 19 | 2 | 12 |  | `ef fa 1d 00 00 37 00 00 23 55` |
| `0x385c` | 18 | 2 | 15 | yes | `d5 dc 02 01 00 aa c3 01 09 01` |
| `0xe324` | 17 | 2 | 77.9 |  | `02 4c 4e 00 00 a0 86 01 00 50` |
| `0x8a39` | 16 | 8 | 8.1 |  | `05 57 75 6d 62 6f` |
| `0x910c` | 16 | 2 | 7 |  | `00 00 56 04 00 00 00` |
| `0x6003` | 15 | 1 | 123 |  | `00 00 b5 e9 0c 47 7d 52 0b 4e` |
| `0x3815` | 10 | 1 | 16.5 | yes | `c1 e3 05 96 e9 2e 45 28 69 86` |
| `0x8d4b` | 8 | 4 | 3 | yes | `bb 42 00` |
| `0x8a03` | 7 | 4 | 303.7 |  | `00 00 05 54 49 45 52 31 0a 43` |
| `0x3637` | 7 | 2 | 14.6 |  | `00 00 00 86 89 04 00 00 00 00` |
| `0x8a45` | 7 | 4 | 14 |  | `00 00 02 00 00 00 61 c8 e7 a2` |
| `0x4301` | 7 | 4 | 1 |  | `00` |
| `0x3726` | 7 | 1 | 4 | yes | `fd 44 01 04` |
| `0xe215` | 6 | 2 | 12 |  | `00 00 01 00 00 00 00 00 00 00` |
| `0xffa6` | 6 | 2 | 16 |  | `78 00 00 00 00 00 00 00 8d ab` |
| `0x363e` | 5 | 1 | 3 |  | `13 38 00` |
| `0x9713` | 5 | 1 | 6 |  | `00 00 00 00 00 00` |
| `0xffa9` | 5 | 2 | 16 |  | `68 13 00 00 00 00 00 00 60 e3` |
| `0x9002` | 4 | 1 | 11 | yes | `cb 2d 2d 3b 24 04 00 00 00 00` |
| `0x5621` | 4 | 2 | 11 |  | `00 00 b0 51 00 00 bd 98 17 01` |
| `0x9237` | 4 | 2 | 6 |  | `00 00 67 0d 10 00` |
| `0x564d` | 4 | 1 | 7 |  | `00 00 23 55 a7 1f 01` |
| `0x3655` | 4 | 1 | 6 |  | `a6 38 00 00 00 00` |
| `0x5600` | 3 | 2 | 523 |  | `11 01 00 00 00 00 00 00 00 00` |
| `0x9215` | 3 | 1 | 2 |  | `00 00` |
| `0x561d` | 3 | 2 | 8 |  | `e7 07 00 00 e7 07 00 00` |
| `0xe223` | 3 | 2 | 12 |  | `80 c1 f5 52 a1 01 00 00 00 00` |
| `0xe224` | 3 | 2 | 4 |  | `02 01 02 02` |
| `0xe22e` | 3 | 2 | 16 |  | `05 01 3c 01 02 00 00 03 24 00` |
| `0x9232` | 3 | 2 | 269 |  | `43 6b 69 0f 00 61 0d 10 00 62` |
| `0x5633` | 3 | 2 | 1 |  | `00` |
| `0x3836` | 3 | 1 | 4 |  | `00 ba fd 03` |
| `0xe237` | 3 | 2 | 107 |  | `02 03 02 00 00 00 01 00 00 00` |
| `0x5648` | 3 | 2 | 10 |  | `01 01 b3 14 99 1f 00 00 00 00` |
| `0x564b` | 3 | 2 | 6.3 |  | `00` |
| `0xe251` | 3 | 2 | 1 |  | `00` |
| `0xe257` | 3 | 2 | 53 |  | `34 01 02 03 04 05 06 07 08 09` |
| `0x3657` | 3 | 2 | 9 |  | `01 00 00 00 00 00 00 00 00` |
| `0xe25a` | 3 | 2 | 216 |  | `00 00 d4 01 14 00 00 00 00 00` |
| `0xe25c` | 3 | 2 | 3 |  | `01 4b 01` |
| `0xe262` | 3 | 2 | 118 |  | `03 04 02 02 00 00 00 00 00 00` |
| `0xe263` | 3 | 2 | 5 |  | `02 46 00 5a 00` |
| `0xe274` | 3 | 2 | 41 |  | `01 00 c7 09 59 d1 02 00 00 00` |
| `0x5682` | 3 | 2 | 4 |  | `00 00 23 00` |
| `0xe28c` | 3 | 2 | 1116.7 |  | `01 01 01 14 98 eb 14 00 00 09` |
| `0x56a8` | 3 | 2 | 10 |  | `00 00 00 00 00 00 00 00 00 00` |
| `0x56ac` | 3 | 2 | 43 |  | `2a 55 15 05 0e 06 07 08 49 09` |
| `0x56b6` | 3 | 2 | 1 |  | `00` |
| `0xe32a` | 3 | 2 | 43 |  | `03 01 dc 05 00 00 00 00 00 00` |
| `0x6103` | 3 | 2 | 10 |  | `01 07 a1 bb 0d 00 01 5a c0 2f` |
| `0x6000` | 3 | 2 | 88 |  | `09 03 fd ea 0c 02 a5 4a 3d 4e` |
| `0x8d09` | 3 | 2 | 3262 |  | `94 01 10 b4 27 00 04 01 01 00` |
| `0x570f` | 3 | 2 | 3 |  | `00 00 00` |
| `0xe200` | 3 | 2 | 12 |  | `00 80 9d 11 fe ff ff ff ff 00` |
| `0x5712` | 3 | 2 | 37 |  | `06 c2 27 09 00 01 01 cc 27 09` |
| `0x8d1c` | 3 | 2 | 1418 |  | `00 00 00 00 00 00 00 00 0f 10` |
| `0x8d31` | 3 | 1 | 152.7 |  | `00 00 ba fd 03 01 00 00 00 01` |
| `0x9330` | 3 | 2 | 1 |  | `00` |
| `0x9331` | 3 | 2 | 101 |  | `02 04 02 00 00 00 01 00 00 00` |
| `0x572d` | 3 | 2 | 1 |  | `00` |
| `0x8d37` | 3 | 2 | 77 |  | `09 64 00 00 00 0a 00 00 00 2c` |
| `0x3733` | 3 | 1 | 41 | yes | `b4 48 02 1f be 2d 46 ac 93 f7` |
| `0xe33b` | 3 | 2 | 190 |  | `09 22 04 00 00 00 57 f7 d4 f6` |
| `0xe33d` | 3 | 2 | 4 |  | `00 00 00 00` |
| `0x8d3e` | 3 | 1 | 12 |  | `01 00 00 00 76 cc cf 62 a1 01` |
| `0xe33e` | 3 | 2 | 1 |  | `00` |
| `0xe342` | 3 | 2 | 144 |  | `09 f7 ae 15 00 00 c8 00 00 76` |
| `0xe343` | 3 | 2 | 64 |  | `03 58 de 03 00 00 00 c7 09 01` |
| `0xe349` | 3 | 2 | 11 |  | `00 00 80 d9 86 4f a1 01 00 00` |
| `0x8d4d` | 3 | 2 | 10 |  | `00 00 b8 a9 15 00 00 cf 00 00` |
| `0xe34e` | 3 | 2 | 2 |  | `00 00` |
| `0x8d4e` | 3 | 2 | 6 | yes | `84 39 24 f4 cb 1f` |
| `0x8d53` | 3 | 2 | 1807 |  | `22 01 10 53 65 61 73 6f 6e 5f` |
| `0x8d58` | 3 | 2 | 1 |  | `00` |
| `0x8d7e` | 3 | 2 | 1135.3 |  | `8f 01 13 83 36 07 00 00 00 00` |
| `0x8d83` | 3 | 2 | 1 |  | `00` |
| `0x8d8c` | 3 | 2 | 1063.3 |  | `80 01 81 59 b7 00 00 00 00 00` |
| `0xffa5` | 3 | 2 | 16 |  | `3d 03 00 00 00 00 00 00 11 91` |
| `0xffa7` | 3 | 2 | 2 |  | `07 01` |
| `0xffac` | 3 | 2 | 24 |  | `03 01 02 00 00 00 02 17 00 00` |
| `0x9501` | 3 | 2 | 145 |  | `24 02 00 00 00 03 00 00 00 04` |
| `0x6001` | 2 | 1 | 29 |  | `03 a9 b8 0d 01 3c 1d 29 63 a1` |
| `0x8c07` | 2 | 1 | 19 |  | `00 00 38 1b 0f 00 00 9f 2b 2d` |
| `0x5608` | 2 | 2 | 15.5 |  | `0f 00 00 06 00 c0 44 13 01 01` |
| `0x5613` | 2 | 1 | 14 |  | `04 03 f6 95 17 00 00 3d 00 00` |
| `0x9235` | 2 | 1 | 13 |  | `01 e0 81 04 00 83 24 d5 00 54` |
| `0x364d` | 2 | 1 | 1 |  | `00` |
| `0x3650` | 2 | 1 | 2637.5 |  | `00 00 07 03 41 6a 61 09 00 00` |
| `0xe293` | 2 | 2 | 278 |  | `01 0c 03 01 00 90 cf 12 01 04` |
| `0x6108` | 2 | 1 | 1 |  | `00` |
| `0x9719` | 2 | 1 | 9 |  | `a7 de 03 00 00 00 df 05 01` |
| `0x9321` | 2 | 1 | 10 |  | `01 b4 48 b0 59 09 01 ec e0 06` |
| `0x9339` | 2 | 1 | 2 |  | `00 00` |
| `0xe34c` | 2 | 2 | 1174 |  | `01 01 00 00 00 05 41 49 4f 4e` |
| `0x9634` | 2 | 1 | 2 |  | `12 3e` |
| `0x3735` | 2 | 1 | 24 | yes | `fd 44 0f d3 28 03 df f8 23 c5` |
| `0x8c09` | 1 | 1 | 15 |  | `00 06 38 1b 0f 00 01 b8 50 29` |
| `0x4402` | 1 | 1 | 2 |  | `00 00` |
| `0x4403` | 1 | 1 | 84 |  | `03 01 e4 05 0f 1a 04 00 00 00` |
| `0x3817` | 1 | 1 | 4 | yes | `ff 77 90 03` |
| `0x5617` | 1 | 1 | 14 | yes | `e6 5f c1 b3 11 00 00 12 00 00` |
| `0x4400` | 1 | 1 | 98 |  | `04 00 e4 05 0f 1a 04 00 00 00` |
| `0x9226` | 1 | 1 | 2 |  | `00 00` |
| `0x9227` | 1 | 1 | 121 |  | `05 00 00 00 00 00 00 00 00 00` |
| `0x5673` | 1 | 1 | 9 |  | `e6 2f 00 00 85 fb 69 02 00` |
| `0x9706` | 1 | 1 | 7 |  | `00 00 01 00 00 00 00` |
| `0x5713` | 1 | 1 | 9 |  | `02 8a 1b 23 00 8b 1b 23 00` |
| `0x5715` | 1 | 1 | 228 |  | `00 00 02 1c 37 33 70 37 a0 0f` |
| `0x9319` | 1 | 1 | 0 |  | `` |
| `0x971f` | 1 | 1 | 60 |  | `1e 03 58 de 03 00 00 00 c7 09` |
| `0x3731` | 1 | 1 | 19 | yes | `ed 5b bc 56 96 c7 1c ae 03 c8` |
| `0x3740` | 1 | 1 | 18 | yes | `cb 2d 06 02 06 9b d8 c6 e6 bf` |
| `0x8d41` | 1 | 1 | 2 |  | `00 00` |
| `0x3749` | 1 | 1 | 2 |  | `9d 73` |
| `0x8d48` | 1 | 1 | 2 |  | `00 00` |
| `0x8d92` | 1 | 1 | 3 | yes | `cb 2d 01` |
| `0x372c` | 1 | 1 | 21 | yes | `9f 7c f6 ec 96 c7 cc 94 03 c8` |
| `0xffad` | 1 | 1 | 2 |  | `02 01` |
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
| `0x9222` | PartyReconnectMember_NT |  |  |
| `0x931f` | PartyLeave_NT |  | you left the party (notice) |
| `0x9409` |  | Aion2Flow: nickname |  |
| `0x940b` |  | Aion2Flow: nickname |  |
| `0x9602` | ForceInfo_NT | Aion2Flow: Force roster snapshot |  |
| `0x960a` | ForceAddMember_NT | Aion2Flow: Force roster profile |  |
| `0x961a` | ForceDisband_NT |  | the Force (20-player raid group) broke up |
| `0x961b` | ForceReconnectMember_NT | Aion2Flow: Force profile |  |
| `0x961d` |  | Aion2Flow: Force member |  |
| `0x961e` |  | Aion2Flow: Force member list |  |
| `0x9622` | ForceRemoveMember_NT |  |  |
| `0x962b` | ForceUpdateMemberHPMP_NT | Aion2Flow: Force status |  |
| `0x962c` | ForceUpdateMemberStatus_NT |  |  |
| `0x962e` | ForceLeave_NT |  | you left the Force (20-player raid group) |
| `0xe241` | StartNpcInteraction_RS |  |  |
| `0xe243` | FinishNpcInteraction_RS |  |  |
| `0xe302` | AbyssArtifactStatus_NT |  |  |
| `0xe303` | AbyssArtifactUpdate_NT |  |  |
| `0xe305` | AbyssArtifactEventInfo_NT | A2Tools: abyss artifact entry |  |
| `0xe307` | AbyssArtifactEventInfo_RS | A2Tools: abyss artifacts map |  |
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
