# AI scripts (original `cpscrNNN.dat` / `cpatrNNN.dat`)

Computer players run a compiled script ("PopScript"). Layout from the ALACN Pop World Editor (PopRe,
`3d02fa3`, no licence: facts only, see the README references), checked on the 58 scripts of the original
`levels/` folder: a decoder built from this spec reads every 12 552-byte file from start to `SCRIPT_END` with no
word left over. Not parsed by `pop3-format` yet.

Tags: **[files]** verified on the files, **[editor]** from the editor only, **[contradicted]** the editor disagrees,
the files win.

## Files
| File | Count | Size | Notes |
|---|---|---|---|
| `cpscrNNN.dat` | 58 | 12 552 (56 files) | NNN = 3-digit script number, lower case on disk |
| `cpscr081.dat`, `cpscr099.dat` | | 9 480, 8 328 | smaller layouts, see below; the editor refuses them |
| `cpatrNNN.dat` | 58 | 144 | computer-player attributes, same numbers as the scripts |

Numbers present: 010, 012-035, 037-041, 043, 053-055, 057-062, 064-066, 074-078, 080-084, 099, 101, 112, 120, 122.
Which script a tribe runs comes from the level header, see "Which scripts the levels use".

## `cpscr` layout (12 552 bytes, little endian, packed) [files]
| Off | Size | Type | Content |
|---|---|---|---|
| 0 | 8 192 | `u16[4096]` | code words; word 0 = format version, **12** in every file |
| 8 192 | 4 096 | `{u32 type, i32 value}[512]` | operand table ("fields") |
| 12 288 | 256 | `i32[64]` | user variable storage, 0 on disk |
| 12 544 | 8 | 2 x `u32` | runtime pointers, 0 on disk |

Field `type`: 0 constant (`value` = the number), 1 user variable (`value` = 0..63), 2 internal variable
(`value` = index, table below).

Rules seen in every file [files]:
- Fields are a dense prefix (11..237 used), each (type, value) pair once, numbered in order of first use in the
  code. User variables are also numbered by first use (cpscr057 uses all 64).
- **Unused fields are filled with `0x03` bytes**, not zeros (the editor writes zeros) [contradicted].
- Code words after `SCRIPT_END` are 0. Longest script: 2 777 words (cpscr057).
- Constants range from -100 to 600 000.

Smaller layouts (own observation, [files]): cpscr081 has `u16[2560]` code (fields at 5 120) and cpscr099
`u16[2048]` code, 32 user variables (fields at 4 096). Both have version 12 and parse with the same grammar.
cpscr099's internal variable numbers don't match the table below (an older numbering). Neither is used by an
active slot of a shipped level: a reader can accept 12 552 bytes only.

## Code
A word `< 1000` is an operand (a field index, in practice < 512); a word `>= 1000` is a token. Tokens used in
the files: 1000..1223.

Grammar (prefix encoding) [files]:

```
script := 12 block SCRIPT_END(1019) 0*
block  := BEGIN(1003) stmt* END(1004)
stmt   := DO(1006) command params                  -- parameter list fixed per command
        | EVERY(1005) F [F] block                  -- second F present iff the next word is not BEGIN
        | (SET(1007) | INCREMENT(1008) | DECREMENT(1009)) Fdest F
        | (MULTIPLY(1025) | DIVIDE(1026)) Fdest F F
        | IF(1000) cond block [ELSE(1001) block] ENDIF(1002)
cond   := (AND(1020) | OR(1021)) cond cond
        | CMP F F                                  -- CMP 1012..1017, operator first: a > b = 1012 a b
F      := operand word
```

- `ELSE` and `ENDIF` come after the closing `END` of the previous block.
- Destinations (`Fdest`) are always a user variable or an `INT_ATTR_*` (internal 1000..1047) [files].
  INCREMENT/DECREMENT add/subtract; MULTIPLY/DIVIDE are probably `dest = a op b` [editor].
- **EVERY stores period - 1**: periods are powers of two 2..8192, so the stored value is a mask `2^k - 1`. The
  optional second operand (a phase/offset?) is also stored minus 1 by the editor, but 21 of the 331 are larger
  than the period: meaning unknown.
- Conditions with more than one AND/OR are **left-nested** in the files (cpscr057: `OR AND c1 c2 c3` =
  `(c1 && c2) || c3`). The editor writes them right-nested and prints them flat, so it does not round-trip
  [contradicted]. Every other AND/OR (845) joins two comparisons. At most 3 comparisons per IF, blocks nest up
  to 13 deep.
- Never seen: 1010/1011 (expression brackets), 1018 (unassigned), 1024 (`COMPUTER_PLAYER`), 1027 (comment).

Every real script has the same shape: `IF (INT_GAME_TURN == 0) { setup: state toggles, SET INT_ATTR_*, SET $v }
ELSE { EVERY n { ... } ... } ENDIF`. Stubs (080, 082-084, 101, 112, 120, 122, used by the multiplayer levels) are
34 words: `SET $v0 0` ... `SET $v9 0`.

Example, start of cpscr010: `12, 1003, 1000 1014 0 1` = `{ IF (F0 == F1)` with F0 = (2, 0) `INT_GAME_TURN` and
F1 = (0, 0) the constant 0; then `1003, 1006 1164 1023` = `{ DO SET_REINCARNATION OFF`, `1006 1109` =
`DO DELAY_MAIN_DRUM_TOWER`, `1007 2 1` = `SET $v0 0` (F2 = (1, 0)).

### DO parameter kinds [files]
| Kind | Encoding |
|---|---|
| field | one operand |
| var_field | one operand, always a user variable (output) |
| on_off | token ON 1022 / OFF 1023 |
| team | token BLUE 1118 / RED 1119 / YELLOW 1120 / GREEN 1121, or an operand |
| team_wild | a team, or COUNT_WILD 1058 |
| target_type | ATTACK_MARKER 1070 / ATTACK_BUILDING 1071 / ATTACK_PERSON 1072 |
| attack_type | ATTACK_NORMAL 1078 / ATTACK_BY_BOAT 1079 / ATTACK_BY_BALLOON 1080 |
| guard_type | GUARD_NORMAL 1087 / GUARD_WITH_GHOSTS 1088 |
| one_shot_type | SPELL_TYPE 1099 / BUILDING_TYPE 1100 |
| vehicle_type | BOAT_TYPE 1104 / BALLOON_TYPE 1105 |

Every keyword parameter in the files is one of the allowed tokens for its slot. `ATTACK` takes 13 parameters:
team, people count, target type, target model, damage, spells 1-3, attack type, bring the vehicles back, markers
1-3 (meanings [editor]). Marker parameters are indices into the level header's `Markers[256]`
(see level-format.md).

## Token tables
Names are the editor's script spellings. Uses = total occurrences / files, over the 58 scripts.

### Control tokens and operators
| Value | Text | Uses (total / files) |
|---|---|---|
| 1000 | `IF` | 2248 / 50 |
| 1001 | `ELSE` | 806 / 50 |
| 1002 | `ENDIF` | 2248 / 50 |
| 1003 | `{` | 3582 / 58 |
| 1004 | `}` | 3582 / 58 |
| 1005 | `EVERY` | 470 / 50 |
| 1006 | `DO` | 5239 / 50 |
| 1007 | `SET` | 5756 / 58 |
| 1008 | `INCREMENT` | 377 / 46 |
| 1009 | `DECREMENT` | 64 / 23 |
| 1010 | `(none)` | 0 / 0 |
| 1011 | `(none)` | 0 / 0 |
| 1012 | `>` | 1782 / 50 |
| 1013 | `<` | 403 / 50 |
| 1014 | `==` | 862 / 50 |
| 1015 | `!=` | 44 / 15 |
| 1016 | `>=` | 1 / 1 |
| 1017 | `<=` | 3 / 3 |
| 1019 | `SCRIPT_END` | 58 / 58 |
| 1020 | `&&` | 845 / 49 |
| 1021 | `\|\|` | 2 / 2 |
| 1022 | `ON` | 890 / 50 |
| 1023 | `OFF` | 113 / 36 |
| 1024 | `COMPUTER_PLAYER` | 0 / 0 |
| 1025 | `MULTIPLY` | 20 / 12 |
| 1026 | `DIVIDE` | 16 / 11 |

### DO commands
1028-1048 and 1050-1051 are the computer player's "states", switched ON/OFF; their order follows the game's
internal state list [editor].

| Value | Command | Parameters | Uses |
|---|---|---|---|
| 1028 | `STATE_CONSTRUCT_BUILDING` | on_off | 51 / 50 |
| 1029 | `STATE_FETCH_WOOD` | on_off | 39 / 39 |
| 1030 | `STATE_SHAMAN_GET_WILDS` | on_off | 92 / 50 |
| 1031 | `STATE_HOUSE_A_PERSON` | on_off | 51 / 50 |
| 1032 | `STATE_SEND_GHOSTS` | on_off | 4 / 4 |
| 1033 | `STATE_STATE_BRING_NEW_PEOPLE_BACK` | on_off | 50 / 50 |
| 1034 | `STATE_TRAIN_PEOPLE` | on_off | 53 / 50 |
| 1035 | `STATE_POPULATE_DRUM_TOWER` | on_off | 49 / 49 |
| 1036 | `STATE_DEFEND` | on_off | 50 / 50 |
| 1037 | `STATE_DEFEND_BASE` | on_off | 50 / 50 |
| 1038 | `STATE_SPELL_DEFENCE` | field, field, on_off | 47 / 40 |
| 1039 | `STATE_PREACH` | on_off | 47 / 45 |
| 1040 | `STATE_BUILD_WALLS` | on_off | 0 / 0 |
| 1041 | `STATE_SABOTAGE` | on_off | 1 / 1 |
| 1042 | `STATE_SPELL_OFFENSIVE` | on_off | 0 / 0 |
| 1043 | `STATE_FIREWARRIOR_DEFEND` | on_off | 40 / 38 |
| 1044 | `STATE_BUILD_VEHICLE` | on_off | 35 / 35 |
| 1045 | `STATE_FETCH_LOST_PEOPLE` | on_off | 41 / 41 |
| 1046 | `STATE_FETCH_LOST_VEHICLE` | on_off | 31 / 31 |
| 1047 | `STATE_FETCH_FAR_VEHICLE` | on_off | 41 / 41 |
| 1048 | `STATE_AUTO_ATTACK` | on_off | 50 / 50 |
| 1050 | `STATE_FLATTEN_BASE` | on_off | 2 / 1 |
| 1051 | `STATE_BUILD_OUTER_DEFENCES` | on_off | 2 / 1 |
| 1059 | `ATTACK` | team, field, target_type, field, field, field, field, field, attack_type, field, field, field, field | 334 / 44 |
| 1064 | `SPELL_ATTACK` | field, field, field | 19 / 14 |
| 1065 | `RESET_BASE_MARKER` | - | 0 / 0 |
| 1066 | `SET_BASE_MARKER` | field | 7 / 6 |
| 1067 | `SET_BASE_RADIUS` | field | 5 / 5 |
| 1068 | `COUNT_PEOPLE_IN_MARKER` | team_wild, field, field, var_field | 144 / 40 |
| 1069 | `SET_DRUM_TOWER_POS` | field, field | 42 / 40 |
| 1073 | `CONVERT_AT_MARKER` | field | 12 / 6 |
| 1074 | `PREACH_AT_MARKER` | field | 47 / 12 |
| 1075 | `SEND_GHOST_PEOPLE` | field | 0 / 0 |
| 1076 | `GET_SPELLS_CAST` | team, field, var_field | 21 / 15 |
| 1077 | `GET_NUM_ONE_OFF_SPELLS` | team, field, var_field | 13 / 10 |
| 1081 | `SET_ATTACK_VARIABLE` | var_field | 62 / 50 |
| 1082 | `BUILD_DRUM_TOWER` | field, field | 73 / 20 |
| 1083 | `GUARD_AT_MARKER` | field, field, field, field, field, guard_type | 0 / 0 |
| 1084 | `GUARD_BETWEEN_MARKERS` | field, field, field, field, field, field, guard_type | 6 / 3 |
| 1085 | `GET_HEIGHT_AT_POS` | field, var_field | 55 / 16 |
| 1086 | `SEND_ALL_PEOPLE_TO_MARKER` | field | 15 / 15 |
| 1089 | `RESET_CONVERT_MARKER` | - | 0 / 0 |
| 1090 | `SET_CONVERT_MARKER` | field | 0 / 0 |
| 1091 | `SET_MARKER_ENTRY` | field, field, field, field, field, field, field | 212 / 32 |
| 1092 | `MARKER_ENTRIES` | field, field, field, field | 115 / 32 |
| 1093 | `CLEAR_GUARDING_FROM` | field, field, field, field | 7 / 3 |
| 1094 | `SET_BUILDING_DIRECTION` | field | 1 / 1 |
| 1095 | `TRAIN_PEOPLE_NOW` | field, field | 56 / 23 |
| 1096 | `PRAY_AT_HEAD` | field, field | 29 / 15 |
| 1097 | `PUT_PERSON_IN_DT` | field, field, field | 147 / 30 |
| 1098 | `I_HAVE_ONE_SHOT` | one_shot_type, field, var_field | 11 / 9 |
| 1101 | `VEHICLE_PATROL` | field, field, field, field, field, vehicle_type | 20 / 7 |
| 1102 | `DEFEND_SHAMEN` | field | 36 / 21 |
| 1103 | `SEND_SHAMEN_DEFENDERS_HOME` | - | 22 / 18 |
| 1106 | `IS_BUILDING_NEAR` | field, field, field, team, field, var_field | 4 / 4 |
| 1107 | `BUILD_AT` | field, field, field, field | 0 / 0 |
| 1108 | `SET_SPELL_ENTRY` | field, field, field, field, field, field | 108 / 35 |
| 1109 | `DELAY_MAIN_DRUM_TOWER` | - | 7 / 7 |
| 1110 | `BUILD_MAIN_DRUM_TOWER` | - | 3 / 3 |
| 1111 | `ZOOM_TO` | field, field, field | 9 / 3 |
| 1112 | `DISABLE_USER_INPUTS` | - | 6 / 5 |
| 1113 | `ENABLE_USER_INPUTS` | - | 4 / 4 |
| 1114 | `OPEN_DIALOG` | field | 0 / 0 |
| 1115 | `GIVE_ONE_SHOT` | field, team | 57 / 15 |
| 1116 | `CLEAR_STANDING_PEOPLE` | - | 0 / 0 |
| 1117 | `ONLY_STAND_AT_MARKERS` | - | 3 / 3 |
| 1122 | `NAV_CHECK` | team, target_type, field, field, var_field | 92 / 11 |
| 1123 | `TARGET_FIREWARRIORS` | - | 10 / 10 |
| 1124 | `DONT_TARGET_FIREWARRIORS` | - | 0 / 0 |
| 1125 | `TARGET_BLUE_SHAMAN` | - | 10 / 10 |
| 1126 | `DONT_TARGET_BLUE_SHAMAN` | - | 0 / 0 |
| 1127 | `TARGET_BLUE_DRUM_TOWERS` | - | 7 / 7 |
| 1128 | `DONT_TARGET_BLUE_DRUM_TOWERS` | - | 0 / 0 |
| 1129 | `HAS_BLUE_KILLED_A_GHOST` | var_field | 0 / 0 |
| 1130 | `COUNT_GUARD_FIRES` | field, field, field, var_field | 0 / 0 |
| 1131 | `GET_HEAD_TRIGGER_COUNT` | field, field, var_field | 59 / 19 |
| 1132 | `MOVE_SHAMAN_TO_MARKER` | field | 2 / 1 |
| 1133 | `TRACK_SHAMAN_TO_ANGLE` | field | 0 / 0 |
| 1134 | `TRACK_SHAMAN_EXTRA_BOLLOCKS` | field | 0 / 0 |
| 1135 | `IS_SHAMAN_AVAILABLE_FOR_ATTACK` | var_field | 2 / 2 |
| 1136 | `PARTIAL_BUILDING_COUNT` | - | 16 / 6 |
| 1137 | `SEND_BLUE_PEOPLE_TO_MARKER` | field | 0 / 0 |
| 1138 | `GIVE_MANA_TO_PLAYER` | team, field | 14 / 11 |
| 1139 | `IS_PLAYER_IN_WORLD_VIEW` | var_field | 1 / 1 |
| 1140 | `SET_AUTO_BUILD` | on_off | 2 / 1 |
| 1141 | `DESELECT_ALL_BLUE_PEOPLE` | - | 5 / 1 |
| 1142 | `FLASH_BUTTON` | field, on_off | 5 / 1 |
| 1143 | `TURN_PANEL_ON` | field | 6 / 1 |
| 1144 | `GIVE_PLAYER_SPELL` | team, field | 4 / 1 |
| 1145 | `HAS_PLAYER_BEEN_IN_ENCYC` | var_field | 0 / 0 |
| 1146 | `IS_BLUE_SHAMAN_SELECTED` | var_field | 0 / 0 |
| 1147 | `CLEAR_SHAMAN_LEFT_CLICK` | - | 0 / 0 |
| 1148 | `CLEAR_SHAMAN_RIGHT_CLICK` | - | 0 / 0 |
| 1149 | `IS_SHAMAN_ICON_LEFT_CLICKED` | var_field | 0 / 0 |
| 1150 | `IS_SHAMAN_ICON_RIGHT_CLICKED` | var_field | 0 / 0 |
| 1151 | `TRIGGER_THING` | field | 42 / 8 |
| 1152 | `TRACK_TO_MARKER` | field | 0 / 0 |
| 1153 | `CAMERA_ROTATION` | field | 0 / 0 |
| 1154 | `STOP_CAMERA_ROTATION` | - | 0 / 0 |
| 1155 | `COUNT_BLUE_SHAPES` | var_field | 1 / 1 |
| 1156 | `COUNT_BLUE_IN_HOUSES` | var_field | 1 / 1 |
| 1157 | `HAS_HOUSE_INFO_BEEN_SHOWN` | var_field | 0 / 0 |
| 1158 | `CLEAR_HOUSE_INFO_FLAG` | - | 0 / 0 |
| 1159 | `SET_AUTO_HOUSE` | on_off | 1 / 1 |
| 1160 | `COUNT_BLUE_WITH_BUILD_COMMAND` | var_field | 1 / 1 |
| 1161 | `DONT_HOUSE_SPECIALISTS` | on_off | 0 / 0 |
| 1162 | `TARGET_PLAYER_DT_AND_S` | team | 0 / 0 |
| 1163 | `REMOVE_PLAYER_THING` | team, field | 4 / 1 |
| 1164 | `SET_REINCARNATION` | on_off | 5 / 5 |
| 1165 | `EXTRA_WOOD_COLLECTION` | on_off | 1 / 1 |
| 1166 | `SET_WOOD_COLLECTION_RADII` | field, field, field, field | 1 / 1 |
| 1167 | `GET_NUM_PEOPLE_CONVERTED` | team, var_field | 0 / 0 |
| 1168 | `GET_NUM_PEOPLE_BEING_PREACHED` | team, var_field | 1 / 1 |
| 1169 | `TRIGGER_LEVEL_LOST` | - | 4 / 3 |
| 1170 | `TRIGGER_LEVEL_WON` | - | 3 / 3 |
| 1171 | `REMOVE_HEAD_AT_POS` | field, field | 4 / 3 |
| 1172 | `SET_BUCKET_USAGE` | on_off | 94 / 47 |
| 1173 | `SET_BUCKET_COUNT_FOR_SPELL` | field, field | 1502 / 47 |
| 1174 | `CREATE_MSG_NARRATIVE` | field | 25 / 25 |
| 1175 | `CREATE_MSG_OBJECTIVE` | field | 0 / 0 |
| 1176 | `CREATE_MSG_INFORMATION` | field | 67 / 12 |
| 1177 | `CREATE_MSG_INFORMATION_ZOOM` | field, field, field, field | 17 / 8 |
| 1178 | `SET_MSG_ZOOM` | field, field, field | 0 / 0 |
| 1179 | `SET_MSG_TIMEOUT` | field | 18 / 8 |
| 1180 | `SET_MSG_DELETE_ON_OK` | - | 46 / 14 |
| 1181 | `SET_MSG_RETURN_ON_OK` | - | 0 / 0 |
| 1182 | `SET_MSG_DELETE_ON_RMB_ZOOM` | - | 0 / 0 |
| 1183 | `SET_MSG_OPEN_DLG_ON_RMB_ZOOM` | - | 0 / 0 |
| 1184 | `SET_MSG_CREATE_RETURN_MSG_ON_RMB_ZOOM` | - | 0 / 0 |
| 1185 | `SET_MSG_OPEN_DLG_ON_RMB_DELETE` | - | 0 / 0 |
| 1186 | `SET_MSG_ZOOM_ON_LMB_OPEN_DLG` | - | 0 / 0 |
| 1187 | `SET_MSG_AUTO_OPEN_DLG` | - | 68 / 26 |
| 1188 | `SET_SPECIAL_NO_BLDG_PANEL` | on_off | 0 / 0 |
| 1189 | `SET_MSG_OK_SAVE_EXIT_DLG` | - | 0 / 0 |
| 1190 | `FIX_WILD_IN_AREA` | field, field, field | 2 / 2 |
| 1191 | `CHECK_IF_PERSON_PREACHED_TO` | var_field, var_field, var_field | 0 / 0 |
| 1192 | `COUNT_ANGELS` | team, var_field | 3 / 1 |
| 1193 | `SET_NO_BLUE_REINC` | - | 4 / 4 |
| 1194 | `IS_SHAMAN_IN_AREA` | team, field, field, var_field | 0 / 0 |
| 1195 | `FORCE_TOOLTIP` | field, field, field, field | 0 / 0 |
| 1196 | `SET_DEFENCE_RADIUS` | field | 24 / 24 |
| 1197 | `MARVELLOUS_HOUSE_DEATH` | - | 1 / 1 |
| 1198 | `CALL_TO_ARMS` | - | 1 / 1 |
| 1199 | `DELETE_SMOKE_STUFF` | field, field, field | 6 / 1 |
| 1200 | `SET_TIMER_GOING` | field | 3 / 3 |
| 1201 | `REMOVE_TIMER` | - | 3 / 3 |
| 1202 | `HAS_TIMER_REACHED_ZERO` | var_field | 3 / 3 |
| 1203 | `START_REINC_NOW` | - | 0 / 0 |
| 1204 | `TURN_PUSH` | on_off | 1 / 1 |
| 1205 | `FLYBY_CREATE_NEW` | - | 30 / 14 |
| 1206 | `FLYBY_START` | - | 33 / 17 |
| 1207 | `FLYBY_STOP` | - | 0 / 0 |
| 1208 | `FLYBY_ALLOW_INTERRUPT` | on_off | 22 / 17 |
| 1209 | `FLYBY_SET_EVENT_POS` | field, field, field, field | 105 / 17 |
| 1210 | `FLYBY_SET_EVENT_ANGLE` | field, field, field | 119 / 17 |
| 1211 | `FLYBY_SET_EVENT_ZOOM` | field, field, field | 110 / 17 |
| 1212 | `FLYBY_SET_EVENT_INT_POINT` | field, field, field, field | 0 / 0 |
| 1213 | `FLYBY_SET_EVENT_TOOLTIP` | field, field, field, field, field | 15 / 5 |
| 1214 | `FLYBY_SET_END_TARGET` | field, field, field, field | 21 / 17 |
| 1215 | `FLYBY_SET_MESSAGE` | field, field | 0 / 0 |
| 1216 | `KILL_TEAM_IN_AREA` | field, field, field | 1 / 1 |
| 1217 | `CLEAR_ALL_MSG` | - | 6 / 1 |
| 1218 | `SET_MSG_ID` | field | 0 / 0 |
| 1219 | `GET_MSG_ID` | var_field | 0 / 0 |
| 1220 | `KILL_ALL_MSG_ID` | field | 0 / 0 |
| 1221 | `GIVE_UP_AND_SULK` | on_off | 44 / 44 |
| 1222 | `AUTO_MESSAGES` | on_off | 2 / 2 |
| 1223 | `IS_PRISON_ON_LEVEL` | field | 1 / 1 |

### Parameter keywords
| Value | Text | Uses |
|---|---|---|
| 1058 | `COUNT_WILD` | 9 / 5 |
| 1070 | `ATTACK_MARKER` | 105 / 28 |
| 1071 | `ATTACK_BUILDING` | 289 / 42 |
| 1072 | `ATTACK_PERSON` | 32 / 14 |
| 1078 | `ATTACK_NORMAL` | 225 / 43 |
| 1079 | `ATTACK_BY_BOAT` | 87 / 22 |
| 1080 | `ATTACK_BY_BALLOON` | 22 / 7 |
| 1087 | `GUARD_NORMAL` | 6 / 3 |
| 1088 | `GUARD_WITH_GHOSTS` | 0 / 0 |
| 1099 | `SPELL_TYPE` | 11 / 9 |
| 1100 | `BUILDING_TYPE` | 0 / 0 |
| 1104 | `BOAT_TYPE` | 20 / 7 |
| 1105 | `BALLOON_TYPE` | 0 / 0 |
| 1118 | `BLUE` | 448 / 47 |
| 1119 | `RED` | 73 / 25 |
| 1120 | `YELLOW` | 54 / 19 |
| 1121 | `GREEN` | 45 / 16 |

### Defined, never used (and refused by the editor's compiler)
| Value | Text |
|---|---|
| 1049 | `STATE_SHAMAN_DEFEND` |
| 1052 | `SPARE5` |
| 1053 | `SPARE6` |
| 1054 | `SPARE7` |
| 1055 | `SPARE8` |
| 1056 | `SPARE9` |
| 1057 | `SPARE10` |
| 1060 | `ATTACK_BLUE` |
| 1061 | `ATTACK_RED` |
| 1062 | `ATTACK_YELLOW` |
| 1063 | `ATTACK_GREEN` |

## Internal variables
Field type 2 values: 0..14 and 1000..1246. 1000..1047 are the 48 computer-player attributes (`INT_ATTR_*`), the
only writable ones. Prefixes: `M_` this computer player, `B_ / R_ / Y_ / G_` blue / red / yellow / green. Every
internal index used by the 12 552-byte files (140 distinct) is in this table, and the names fit how they are used
(e.g. `DO SET_SPELL_ENTRY 0 INT_BLAST INT_M_SPELL_BLAST_COST 512 6 0`). Editor quirks: index 1107 is named
`INT_M_BUILDING_WALL_PIECE` but is the red one, and its decompiler prints 1176 with 1177's name.

| Index | Name | Fields using it |
|---|---|---|
| 0 | `INT_GAME_TURN` | 50 |
| 1 | `INT_MY_NUM_PEOPLE` | 49 |
| 2 | `INT_BLUE_PEOPLE` | 29 |
| 3 | `INT_RED_PEOPLE` | 11 |
| 4 | `INT_YELLOW_PEOPLE` | 15 |
| 5 | `INT_GREEN_PEOPLE` | 14 |
| 6 | `INT_MY_NUM_KILLED_BY_HUMAN` | 25 |
| 7 | `INT_RED_KILLED_BY_HUMAN` | 0 |
| 8 | `INT_YELLOW_KILLED_BY_HUMAN` | 0 |
| 9 | `INT_GREEN_KILLED_BY_HUMAN` | 0 |
| 10 | `INT_WILD_PEOPLE` | 0 |
| 11 | `INT_BLUE_MANA` | 0 |
| 12 | `INT_RED_MANA` | 0 |
| 13 | `INT_YELLOW_MANA` | 0 |
| 14 | `INT_GREEN_MANA` | 0 |
| 1000 | `INT_ATTR_EXPANSION` | 50 |
| 1001 | `INT_ATTR_PREF_SPY_TRAINS` | 50 |
| 1002 | `INT_ATTR_PREF_RELIGIOUS_TRAINS` | 50 |
| 1003 | `INT_ATTR_PREF_WARRIOR_TRAINS` | 50 |
| 1004 | `INT_ATTR_PREF_FIREWARRIOR_TRAINS` | 50 |
| 1005 | `INT_ATTR_PREF_SPY_PEOPLE` | 41 |
| 1006 | `INT_ATTR_PREF_RELIGIOUS_PEOPLE` | 46 |
| 1007 | `INT_ATTR_PREF_WARRIOR_PEOPLE` | 46 |
| 1008 | `INT_ATTR_PREF_FIREWARRIOR_PEOPLE` | 46 |
| 1009 | `INT_ATTR_MAX_BUILDINGS_ON_GO` | 50 |
| 1010 | `INT_ATTR_HOUSE_PERCENTAGE` | 50 |
| 1011 | `INT_ATTR_AWAY_BRAVE` | 50 |
| 1012 | `INT_ATTR_AWAY_WARRIOR` | 50 |
| 1013 | `INT_ATTR_AWAY_RELIGIOUS` | 50 |
| 1014 | `INT_ATTR_DEFENSE_RAD_INCR` | 49 |
| 1015 | `INT_ATTR_MAX_DEFENSIVE_ACTIONS` | 50 |
| 1016 | `INT_ATTR_AWAY_SPY` | 50 |
| 1017 | `INT_ATTR_AWAY_FIREWARRIOR` | 50 |
| 1018 | `INT_ATTR_ATTACK_PERCENTAGE` | 49 |
| 1019 | `INT_ATTR_AWAY_SHAMAN` | 50 |
| 1020 | `INT_ATTR_PEOPLE_PER_BOAT` | 28 |
| 1021 | `INT_ATTR_PEOPLE_PER_BALLOON` | 10 |
| 1022 | `INT_ATTR_DONT_USE_BOATS` | 3 |
| 1023 | `INT_ATTR_MAX_SPY_ATTACKS` | 3 |
| 1024 | `INT_ATTR_ENEMY_SPY_MAX_STAND` | 42 |
| 1025 | `INT_ATTR_MAX_ATTACKS` | 50 |
| 1026 | `INT_ATTR_EMPTY_AT_WAYPOINT` | 1 |
| 1027 | `INT_ATTR_SPY_CHECK_FREQUENCY` | 36 |
| 1028 | `INT_ATTR_RETREAT_VALUE` | 49 |
| 1029 | `INT_ATTR_BASE_UNDER_ATTACK_RETREAT` | 48 |
| 1030 | `INT_ATTR_RANDOM_BUILD_SIDE` | 44 |
| 1031 | `INT_ATTR_USE_PREACHER_FOR_DEFENCE` | 45 |
| 1032 | `INT_ATTR_SHAMEN_BLAST` | 49 |
| 1033 | `INT_ATTR_MAX_TRAIN_AT_ONCE` | 49 |
| 1034 | `INT_ATTR_GROUP_OPTION` | 49 |
| 1035 | `INT_ATTR_PREF_BOAT_HUTS` | 50 |
| 1036 | `INT_ATTR_PREF_BALLOON_HUTS` | 50 |
| 1037 | `INT_ATTR_PREF_BOAT_DRIVERS` | 32 |
| 1038 | `INT_ATTR_PREF_BALLOON_DRIVERS` | 10 |
| 1039 | `INT_ATTR_FIGHT_STOP_DISTANCE` | 8 |
| 1040 | `INT_ATTR_SPY_DISCOVER_CHANCE` | 42 |
| 1041 | `INT_ATTR_COUNT_PREACH_DAMAGE` | 27 |
| 1042 | `INT_ATTR_DONT_GROUP_AT_DT` | 4 |
| 1043 | `INT_ATTR_SPELL_DELAY` | 0 |
| 1044 | `INT_ATTR_DONT_DELETE_USELESS_BOAT_HOUSE` | 0 |
| 1045 | `INT_ATTR_BOAT_HOUSE_BROKEN` | 0 |
| 1046 | `INT_ATTR_DONT_AUTO_TRAIN_PREACHERS` | 2 |
| 1047 | `INT_ATTR_SPARE` | 0 |
| 1048 | `INT_MY_MANA` | 37 |
| 1049 | `INT_M_SPELL_BURN_COST` | 0 |
| 1050 | `INT_M_SPELL_BLAST_COST` | 14 |
| 1051 | `INT_M_SPELL_LIGHTNING_COST` | 27 |
| 1052 | `INT_M_SPELL_TORNADO_COST` | 23 |
| 1053 | `INT_M_SPELL_SWARM_COST` | 20 |
| 1054 | `INT_M_SPELL_INVISIBILITY_COST` | 22 |
| 1055 | `INT_M_SPELL_HYPNOTISM_COST` | 17 |
| 1056 | `INT_M_SPELL_FIRESTORM_COST` | 13 |
| 1057 | `INT_M_SPELL_GHOST_ARMY_COST` | 0 |
| 1058 | `INT_M_SPELL_EROSION_COST` | 6 |
| 1059 | `INT_M_SPELL_SWAMP_COST` | 2 |
| 1060 | `INT_M_SPELL_LAND_BRIDGE_COST` | 21 |
| 1061 | `INT_M_SPELL_ANGEL_OF_DEATH_COST` | 4 |
| 1062 | `INT_M_SPELL_EARTHQUAKE_COST` | 15 |
| 1063 | `INT_M_SPELL_FLATTEN_COST` | 2 |
| 1064 | `INT_M_SPELL_VOLCANO_COST` | 9 |
| 1065 | `INT_M_SPELL_WRATH_OF_GOD_COST` | 0 |
| 1066 | `INT_M_BUILDING_SMALL_HUT` | 42 |
| 1067 | `INT_M_BUILDING_MEDIUM_HUT` | 42 |
| 1068 | `INT_M_BUILDING_LARGE_HUT` | 42 |
| 1069 | `INT_M_BUILDING_DRUM_TOWER` | 1 |
| 1070 | `INT_M_BUILDING_TEMPLE` | 14 |
| 1071 | `INT_M_BUILDING_SPY_TRAIN` | 3 |
| 1072 | `INT_M_BUILDING_WARRIOR_TRAIN` | 15 |
| 1073 | `INT_M_BUILDING_FIREWARRIOR_TRAIN` | 13 |
| 1074 | `INT_M_BUILDING_RECONVERSION` | 0 |
| 1075 | `INT_M_BUILDING_WALL_PIECE` | 0 |
| 1076 | `INT_M_BUILDING_GATE` | 0 |
| 1077 | `INT_M_BUILDING_CURR_OE_SLOT` | 0 |
| 1078 | `INT_M_BUILDING_BOAT_HUT` | 5 |
| 1079 | `INT_M_BUILDING_BOAT_HUT_2` | 0 |
| 1080 | `INT_M_BUILDING_AIRSHIP_HUT` | 2 |
| 1081 | `INT_M_BUILDING_AIRSHIP_HUT_2` | 0 |
| 1082 | `INT_B_BUILDING_SMALL_HUT` | 4 |
| 1083 | `INT_B_BUILDING_MEDIUM_HUT` | 4 |
| 1084 | `INT_B_BUILDING_LARGE_HUT` | 4 |
| 1085 | `INT_B_BUILDING_DRUM_TOWER` | 1 |
| 1086 | `INT_B_BUILDING_TEMPLE` | 11 |
| 1087 | `INT_B_BUILDING_SPY_TRAIN` | 1 |
| 1088 | `INT_B_BUILDING_WARRIOR_TRAIN` | 7 |
| 1089 | `INT_B_BUILDING_FIREWARRIOR_TRAIN` | 0 |
| 1090 | `INT_B_BUILDING_RECONVERSION` | 0 |
| 1091 | `INT_B_BUILDING_WALL_PIECE` | 0 |
| 1092 | `INT_B_BUILDING_GATE` | 0 |
| 1093 | `INT_B_BUILDING_CURR_OE_SLOT` | 0 |
| 1094 | `INT_B_BUILDING_BOAT_HUT` | 0 |
| 1095 | `INT_B_BUILDING_BOAT_HUT_2` | 0 |
| 1096 | `INT_B_BUILDING_AIRSHIP_HUT` | 0 |
| 1097 | `INT_B_BUILDING_AIRSHIP_HUT_2` | 0 |
| 1098 | `INT_R_BUILDING_SMALL_HUT` | 0 |
| 1099 | `INT_R_BUILDING_MEDIUM_HUT` | 0 |
| 1100 | `INT_R_BUILDING_LARGE_HUT` | 0 |
| 1101 | `INT_R_BUILDING_DRUM_TOWER` | 0 |
| 1102 | `INT_R_BUILDING_TEMPLE` | 0 |
| 1103 | `INT_R_BUILDING_SPY_TRAIN` | 0 |
| 1104 | `INT_R_BUILDING_WARRIOR_TRAIN` | 0 |
| 1105 | `INT_R_BUILDING_FIREWARRIOR_TRAIN` | 0 |
| 1106 | `INT_R_BUILDING_RECONVERSION` | 0 |
| 1107 | `INT_M_BUILDING_WALL_PIECE` | 0 |
| 1108 | `INT_R_BUILDING_GATE` | 0 |
| 1109 | `INT_R_BUILDING_CURR_OE_SLOT` | 0 |
| 1110 | `INT_R_BUILDING_BOAT_HUT` | 0 |
| 1111 | `INT_R_BUILDING_BOAT_HUT_2` | 0 |
| 1112 | `INT_R_BUILDING_AIRSHIP_HUT` | 0 |
| 1113 | `INT_R_BUILDING_AIRSHIP_HUT_2` | 0 |
| 1114 | `INT_Y_BUILDING_SMALL_HUT` | 0 |
| 1115 | `INT_Y_BUILDING_MEDIUM_HUT` | 0 |
| 1116 | `INT_Y_BUILDING_LARGE_HUT` | 1 |
| 1117 | `INT_Y_BUILDING_DRUM_TOWER` | 0 |
| 1118 | `INT_Y_BUILDING_TEMPLE` | 0 |
| 1119 | `INT_Y_BUILDING_SPY_TRAIN` | 0 |
| 1120 | `INT_Y_BUILDING_WARRIOR_TRAIN` | 0 |
| 1121 | `INT_Y_BUILDING_FIREWARRIOR_TRAIN` | 1 |
| 1122 | `INT_Y_BUILDING_RECONVERSION` | 0 |
| 1123 | `INT_Y_BUILDING_WALL_PIECE` | 0 |
| 1124 | `INT_Y_BUILDING_GATE` | 0 |
| 1125 | `INT_Y_BUILDING_CURR_OE_SLOT` | 0 |
| 1126 | `INT_Y_BUILDING_BOAT_HUT` | 0 |
| 1127 | `INT_Y_BUILDING_BOAT_HUT_2` | 0 |
| 1128 | `INT_Y_BUILDING_AIRSHIP_HUT` | 0 |
| 1129 | `INT_Y_BUILDING_AIRSHIP_HUT_2` | 0 |
| 1130 | `INT_G_BUILDING_SMALL_HUT` | 0 |
| 1131 | `INT_G_BUILDING_MEDIUM_HUT` | 0 |
| 1132 | `INT_G_BUILDING_LARGE_HUT` | 0 |
| 1133 | `INT_G_BUILDING_DRUM_TOWER` | 0 |
| 1134 | `INT_G_BUILDING_TEMPLE` | 0 |
| 1135 | `INT_G_BUILDING_SPY_TRAIN` | 0 |
| 1136 | `INT_G_BUILDING_WARRIOR_TRAIN` | 0 |
| 1137 | `INT_G_BUILDING_FIREWARRIOR_TRAIN` | 0 |
| 1138 | `INT_G_BUILDING_RECONVERSION` | 0 |
| 1139 | `INT_G_BUILDING_WALL_PIECE` | 0 |
| 1140 | `INT_G_BUILDING_GATE` | 0 |
| 1141 | `INT_G_BUILDING_CURR_OE_SLOT` | 0 |
| 1142 | `INT_G_BUILDING_BOAT_HUT` | 0 |
| 1143 | `INT_G_BUILDING_BOAT_HUT_2` | 0 |
| 1144 | `INT_G_BUILDING_AIRSHIP_HUT` | 0 |
| 1145 | `INT_G_BUILDING_AIRSHIP_HUT_2` | 0 |
| 1146 | `INT_M_PERSON_BRAVE` | 3 |
| 1147 | `INT_M_PERSON_WARRIOR` | 35 |
| 1148 | `INT_M_PERSON_RELIGIOUS` | 30 |
| 1149 | `INT_M_PERSON_SPY` | 11 |
| 1150 | `INT_M_PERSON_FIREWARRIOR` | 27 |
| 1151 | `INT_M_PERSON_SHAMAN` | 3 |
| 1152 | `INT_B_PERSON_BRAVE` | 1 |
| 1153 | `INT_B_PERSON_WARRIOR` | 5 |
| 1154 | `INT_B_PERSON_RELIGIOUS` | 4 |
| 1155 | `INT_B_PERSON_SPY` | 0 |
| 1156 | `INT_B_PERSON_FIREWARRIOR` | 0 |
| 1157 | `INT_B_PERSON_SHAMAN` | 2 |
| 1158 | `INT_R_PERSON_BRAVE` | 0 |
| 1159 | `INT_R_PERSON_WARRIOR` | 0 |
| 1160 | `INT_R_PERSON_RELIGIOUS` | 0 |
| 1161 | `INT_R_PERSON_SPY` | 0 |
| 1162 | `INT_R_PERSON_FIREWARRIOR` | 0 |
| 1163 | `INT_R_PERSON_SHAMAN` | 0 |
| 1164 | `INT_Y_PERSON_BRAVE` | 0 |
| 1165 | `INT_Y_PERSON_WARRIOR` | 0 |
| 1166 | `INT_Y_PERSON_RELIGIOUS` | 0 |
| 1167 | `INT_Y_PERSON_SPY` | 0 |
| 1168 | `INT_Y_PERSON_FIREWARRIOR` | 1 |
| 1169 | `INT_Y_PERSON_SHAMAN` | 0 |
| 1170 | `INT_G_PERSON_BRAVE` | 0 |
| 1171 | `INT_G_PERSON_WARRIOR` | 0 |
| 1172 | `INT_G_PERSON_RELIGIOUS` | 0 |
| 1173 | `INT_G_PERSON_SPY` | 0 |
| 1174 | `INT_G_PERSON_FIREWARRIOR` | 1 |
| 1175 | `INT_G_PERSON_SHAMAN` | 0 |
| 1176 | `INT_BLUE_KILLED_BY_ME` | 1 |
| 1177 | `INT_RED_KILLED_BY_ME` | 0 |
| 1178 | `INT_YELLOW_KILLED_BY_ME` | 1 |
| 1179 | `INT_GREEN_KILLED_BY_ME` | 0 |
| 1180 | `INT_MY_NUM_KILLED_BY_BLUE` | 24 |
| 1181 | `INT_MY_NUM_KILLED_BY_RED` | 0 |
| 1182 | `INT_MY_NUM_KILLED_BY_YELLOW` | 0 |
| 1183 | `INT_MY_NUM_KILLED_BY_GREEN` | 0 |
| 1184 | `INT_BURN` | 0 |
| 1185 | `INT_BLAST` | 48 |
| 1186 | `INT_LIGHTNING` | 48 |
| 1187 | `INT_TORNADO` | 47 |
| 1188 | `INT_SWARM` | 47 |
| 1189 | `INT_INVISIBILITY` | 47 |
| 1190 | `INT_HYPNOTISM` | 47 |
| 1191 | `INT_FIRESTORM` | 47 |
| 1192 | `INT_GHOST_ARMY` | 0 |
| 1193 | `INT_EROSION` | 48 |
| 1194 | `INT_SWAMP` | 47 |
| 1195 | `INT_LAND_BRIDGE` | 47 |
| 1196 | `INT_ANGEL_OF_DEATH` | 47 |
| 1197 | `INT_EARTHQUAKE` | 47 |
| 1198 | `INT_FLATTEN` | 47 |
| 1199 | `INT_VOLCANO` | 47 |
| 1200 | `INT_WRATH_OF_GOD` | 5 |
| 1201 | `INT_BRAVE` | 0 |
| 1202 | `INT_WARRIOR` | 20 |
| 1203 | `INT_RELIGIOUS` | 26 |
| 1204 | `INT_SPY` | 1 |
| 1205 | `INT_FIREWARRIOR` | 23 |
| 1206 | `INT_SHAMAN` | 2 |
| 1207 | `INT_SMALL_HUT` | 0 |
| 1208 | `INT_MEDIUM_HUT` | 0 |
| 1209 | `INT_LARGE_HUT` | 5 |
| 1210 | `INT_DRUM_TOWER` | 20 |
| 1211 | `INT_TEMPLE` | 7 |
| 1212 | `INT_SPY_TRAIN` | 1 |
| 1213 | `INT_WARRIOR_TRAIN` | 13 |
| 1214 | `INT_FIREWARRIOR_TRAIN` | 7 |
| 1215 | `INT_RECONVERSION` | 0 |
| 1216 | `INT_WALL_PIECE` | 0 |
| 1217 | `INT_GATE` | 0 |
| 1218 | `INT_BOAT_HUT` | 3 |
| 1219 | `INT_BOAT_HUT_2` | 0 |
| 1220 | `INT_AIRSHIP_HUT` | 0 |
| 1221 | `INT_AIRSHIP_HUT_2` | 0 |
| 1222 | `INT_NO_SPECIFIC_PERSON` | 0 |
| 1223 | `INT_NO_SPECIFIC_BUILDING` | 37 |
| 1224 | `INT_NO_SPECIFIC_SPELL` | 29 |
| 1225 | `INT_TARGET_SHAMAN` | 12 |
| 1226 | `INT_M_VEHICLE_BOAT` | 22 |
| 1227 | `INT_M_VEHICLE_AIRSHIP` | 7 |
| 1228 | `INT_B_VEHICLE_BOAT` | 0 |
| 1229 | `INT_B_VEHICLE_AIRSHIP` | 0 |
| 1230 | `INT_R_VEHICLE_BOAT` | 0 |
| 1231 | `INT_R_VEHICLE_AIRSHIP` | 0 |
| 1232 | `INT_Y_VEHICLE_BOAT` | 1 |
| 1233 | `INT_Y_VEHICLE_AIRSHIP` | 0 |
| 1234 | `INT_G_VEHICLE_BOAT` | 1 |
| 1235 | `INT_G_VEHICLE_AIRSHIP` | 0 |
| 1236 | `INT_CP_FREE_ENTRIES` | 26 |
| 1237 | `INT_RANDOM_100` | 0 |
| 1238 | `INT_NUM_SHAMEN_DEFENDERS` | 0 |
| 1239 | `INT_CAMERA_ANGLE` | 0 |
| 1240 | `INT_CAMERA_X` | 0 |
| 1241 | `INT_CAMERA_Z` | 0 |
| 1242 | `INT_M_SPELL_SHIELD_COST` | 15 |
| 1243 | `INT_SHIELD` | 47 |
| 1244 | `INT_CONVERT` | 47 |
| 1245 | `INT_TELEPORT` | 1 |
| 1246 | `INT_BLOODLUST` | 1 |

## `cpatr` layout (144 bytes)
| Off | Size | Type | Content | Status |
|---|---|---|---|---|
| 0 | 48 | `u8[48]` | attributes; bytes 33-47 always 0 | [files] |
| 48 | 32 | `char[32]` | name, NUL-padded, e.g. "Doc 010" (19 files say "Doc 047") | [files] |
| 80 | 4 | `u32` | starting spells, bit N = spell model N (e.g. 0x20404 = blast, erode, convert) | [editor] bits |
| 84 | 4 | `u32` | starting buildings, bit N = building model N (e.g. 146 = small hut, tower, warrior training) | [editor] bits |
| 88..131 | | | BuildingsAvailableLevel/Once, SpellsNotCharging, SpellsAvailableOnce[32]: all 0 | [files] |
| 132 | 2 | `u16` | vehicles, bit N = vehicle model N (0, 2, 6, 10, 16, 30 seen) | [files] |
| 134 | 2 | `u8, u8` | TrainingManaOff, Flags: 0 | [files] |
| 136 | 4 | `u32` | Spare1: the file number in 49 files (10 in the stubs, 99 in cpatr081) | [files] |
| 140 | 4 | `u32` | Spare2: 0 | [files] |

Bytes 80-135 have the same layout as the level header's first 56 bytes (`PLAYERTHINGS`).
The editor names attribute byte i after internal `1000 + i` (`INT_ATTR_EXPANSION`, ...), but the values fit poorly
(byte 0 is 36 in every file, byte 10 always 64, byte 21 always 182): **unverified**. Every real script sets its
attributes in its turn-0 block anyway, so whether the game reads `cpatr` at all is unknown.

## Which scripts the levels use [files]
Level header bytes 89, 90, 91 = script number for red, yellow, green, byte 99 = blue (always 0); 0 = none
(see level-format.md). Slots beyond NumPlayers (byte 88) hold junk (4, 5, 64, 99).
- The campaign (2001-2025, 2079) uses 10-78. All scripts it references in active slots exist.
- Multiplayer levels use the stubs (80, 82-84, 101, 112, 120, 122).
- **Missing scripts in active slots**: levl2100 (100), levl2110 (85), levl2131 (130).
- cpscr081 is never referenced; cpscr099 only by an inactive slot of levl2112.

## Writing scripts the way the originals look
Fields in first-use order, unique; user variables in first-use order; zero words after `SCRIPT_END`; unused fields
filled with `0x03`; user variables and pointers 0; AND/OR chains left-nested; EVERY operands stored minus 1.
