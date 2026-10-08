# AI scripts (original `cpscrNNN.dat` / `cpatrNNN.dat`)

Computer players run a compiled script ("PopScript"). Layout from the ALACN Pop World Editor (PopRe,
`3d02fa3`, no licence: facts only, see the README references), checked on the 58 scripts of the original
`levels/` folder: a decoder built from this spec reads every 12 552-byte file from start to `SCRIPT_END` with no
word left over. Not parsed by `pop3-format` yet.

The language, how it runs and what the commands do come from the community's PopScript Wiki (Megafont, built
2006-04-30, `ts.popre.net/archive/Downloads/Docs/PopScript_Wiki_HTML_Help_File.htm`) and from popscript-upgrader
(TylerTheFox, MIT, `f2bd401`), a converter from the original language to the Lua scripts of the
Populous: Reincarnated patch.

Tags: **[files]** verified on the files, **[editor]** from the editor only, **[wiki]** from the wiki (players'
experiments, not code), **[lua]** from the converter and its dump of the Lua API, **[contradicted]** a source
disagrees with the files, the files win.

## What PopScript is
- Each computer tribe (red, yellow, green) runs one compiled script. Blue is the human player and never has one
  (header byte 99 is always 0). The community calls it PopScript or "Script2" (PopRe's v3 level header has a
  `char Script2[10][32]`, see level-format.md; the original header has no such field), and Reincarnated calls
  its Lua successor "Script4" [lua].
- A script does not move units itself. It steers the built-in computer-player AI in three ways:
  - it switches AI behaviours ("states") on and off: build, house, train, defend, preach, attack...
  - it tunes 48 AI attributes (`INT_ATTR_*`): how many of each unit to keep, attack force mix, boats...
  - it gives high-level orders: attack a target, guard between markers, cast a spell at a marker, train now.
- Campaign scripts also act as the level's event script: messages, camera flybys, the tutorial's UI locks and
  flashing buttons, the countdown timer, `TRIGGER_THING`, `TRIGGER_LEVEL_WON/LOST`.
- The source text is compiled offline. The game only reads the compiled `cpscrNNN.dat`, laid out below. The
  community decompiles and recompiles scripts with the PopRe / ALACN world editor.
- Populous: Reincarnated later added Lua scripts with full access to the engine (hooks, things, map, UI), see
  "Lua successor". Classic PopScript is still what every original level uses.

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

## Source language [wiki]
A computer player as written in the wiki's dialect (the shape every original script has, see below):

```
// line comment, /* block comment */
{
  IF ( INT_GAME_TURN == 0 )
  {
    DO STATE_CONSTRUCT_BUILDING ON
    DO STATE_AUTO_ATTACK ON
    SET INT_ATTR_MAX_ATTACKS 999
    SET $attacks 0
  }
  ELSE
  {
    EVERY 256 12
    {
      IF ( INT_MY_NUM_PEOPLE > 30 && $attacks < 3 )
      {
        DO ATTACK BLUE 10 ATTACK_BUILDING INT_NO_SPECIFIC_BUILDING 500 INT_NO_SPECIFIC_SPELL INT_NO_SPECIFIC_SPELL INT_NO_SPECIFIC_SPELL ATTACK_NORMAL 0 5 -1 -1
        INCREMENT $attacks 1
      }
      ENDIF
    }
  }
  ENDIF
}
SCRIPT_END
```

- The script is a single `{ }` block followed by `SCRIPT_END`. Anything after `SCRIPT_END` is ignored.
- Comments are `//` to the end of the line and `/* */`, which may span lines.
- User variables:
  - Written `$name`: up to 32 characters, no spaces, at most 64 per script.
  - The compiler numbers them in order of first use. The names are not stored in the file.
- Values are constants (negative ones too: markers use `-1` for "none"), `$variables`, or internal variables
  (`INT_*`). Internal variables are either game state or identifiers of a model used as an argument, e.g.
  `INT_TEMPLE`.
- Assignments, each with a user variable or an `INT_ATTR_*` as destination:
  - `SET d v`, `INCREMENT d v` and `DECREMENT d v`.
  - `MULTIPLY d a b` and `DIVIDE d a b` store `d = a * b` and `d = a / b`. Rounding and division by zero are not
    documented.
- `IF ( c ) { } [ELSE { }] ENDIF`:
  - Each condition `c` is a chain of comparisons (`== != > < >= <=`) joined with `&&` or `||`.
  - Brackets inside a condition are not used. The files store at most 3 comparisons and nest left
    (see "Code").
  - `ENDIF` comes after the closing `}` and is mandatory.
- `EVERY period [offset] { }`:
  - The period is a constant power of two from 2 to 8192 game turns.
  - The optional offset staggers blocks that share a period, see "How a script runs".
  - Both operands must be constants.
- `DO COMMAND args...` runs a command, with a fixed parameter list per command (tables below). Query commands
  write their result into a user variable, always their last parameter.
- A second dialect exists, from the converter's parser [lua]:
  - It has a `COMPUTER_PLAYER n` header, `BEGIN`/`END` instead of braces and `USER_x` variables.
  - It agrees with the token names 1003 `BEGIN` and 1024 `COMPUTER_PLAYER`. Those were probably Bullfrog's
    spellings, and the wiki's braces are the editor's.
  - That parser is case-sensitive, knows only `//` comments, cannot parse `||` and has grammar conflicts: do not
    use it as a reference.

## How a script runs
Neither source describes the engine loop. What follows is what both of them and the files agree on.
- **Every game turn, the whole script is evaluated once per computer player.**
  - The original scripts' `IF (INT_GAME_TURN == 0) { setup } ELSE { EVERY ... }` shape only makes sense that
    way.
  - Reincarnated's converter puts the whole body in a per-turn `OnTurn` hook.
  - Turn 0 runs the setup block. Every later turn runs the `ELSE` branch, where the `EVERY` blocks gate the work.
- **`EVERY n m` fires on the turns where `(turn + m) % n == 0`**:
  - The wiki's example: `EVERY 256 12` first fires at turn 244, then every 256 turns.
  - `EVERY 128 28` first fires at turn 100.
  - Offsets larger than the period (21 in the files) are then harmless.
  - The stored operands are `n - 1` (a bit mask) and `m - 1`, so the engine probably tests
    `(turn + stored_m + 1) & stored_n == 0`. The exact off-by-one is unverified.
  - The converter adds the tribe index, `(turn + tribe + m) % n == 0`, so tribes sharing a script don't act on
    the same turn [lua]. Whether the original does this is unknown.
- **Turn rate**: the wiki says "about 8 game turns per second". Message timeouts and flyby times are also in
  turns, but `SET_TIMER_GOING` takes seconds. The simulation here runs 10 ticks per second (units.md), so script
  timings need a conversion.
- **User variables keep their values between turns.** Counters such as `INCREMENT $n 1` depend on it. They start
  at 0, the on-disk storage area.
- Internal variables read the live game state when evaluated.
  - Only `INT_ATTR_*` can be written. They are the computer player's attribute bytes: attribute `i` is internal
    `1000 + i`, `READ/WRITE_CP_ATTRIB(pn, i)` in Lua.
  - `M_` means this script's tribe. `INT_*_KILLED_BY_HUMAN` means killed by blue.
- A command takes effect immediately, within the same pass. The wiki's timer tutorial needs `REMOVE_TIMER` in a
  later `EVERY` block, because in the same block it wipes the timer before the message shows.
- States, attributes, marker entries and spell entries are persistent AI settings. The setup block sets them,
  and the periodic blocks adjust them and fire one-off orders.
- Debugging: there is none in the original. The community shows a value through `SET_TIMER_GOING v` plus a
  message that stops the timer [wiki].
- Some commands "don't work in the initialisation section" [wiki], meaning the turn-0 block. Which ones is not
  listed.

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
  INCREMENT/DECREMENT add/subtract; MULTIPLY/DIVIDE are `dest = a op b` [editor] [wiki].
- **EVERY stores period - 1**: periods are powers of two 2..8192, so the stored value is a mask `2^k - 1`. The
  optional second operand, the offset, is also stored minus 1 by the editor. 21 of the 331 are larger than the
  period, which is harmless under the modulo test (see "How a script runs").
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
1028-1048 and 1050-1051 are the computer player's "states", switched ON/OFF. Token `1028 + i` is internal state
`i` [editor], confirmed by Reincarnated's `CP_AT_TYPE_*` numbering [lua]. Meanings: "What the commands do".

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

## What the commands do [wiki]
Shared conventions:
- Positions are `x z` map cells (0..255), and radii are in cells.
- Directions: the wiki gives 0 north, 500 east, 1000 south, 1500 west. Those look like approximate quarters of
  the engine's 2048 per turn (level-format.md), but the scale is unverified.
- Markers are indices into the level header's `Markers[256]`, and `-1` means none.
- A team is a keyword or a number with blue 0, red 1, yellow 2, green 3. "Blue" always means the human player.
- Text arguments are string indices into `language/lang00.dat`.
- `?` marks what the wiki lists as unknown ("Incomplete Knowledge").

### States (`DO STATE_x ON|OFF`)
Internal names are Reincarnated's `CP_AT_TYPE_*` [lua]. States marked "-" have no description in the wiki, so
their meaning only comes from their name.

| Token | Internal | Effect when ON |
|---|---|---|
| 1028 | 0 `CONSTRUCT_BUILDING` | - (build the base: hut share `INT_ATTR_HOUSE_PERCENTAGE`, at most `MAX_BUILDINGS_ON_GO` at once) |
| 1029 | 1 `FETCH_WOOD` | - |
| 1030 | 2 `MED_MAN_GET_WILD_PEEPS` | shaman converts wildmen within `SET_BASE_RADIUS` of `SET_BASE_MARKER` (default: the reincarnation site, "we think") |
| 1031 | 3 `HOUSE_A_PERSON` | - |
| 1032 | 4 `SEND_GHOSTS` | - |
| 1033 | 5 `BRING_NEW_PEOPLE_BACK` | - |
| 1034 | 6 `TRAIN_PEOPLE` | - (train up to the `INT_ATTR_PREF_*_PEOPLE` counts, replacing losses) |
| 1035 | 7 `POPULATE_DRUM_TOWER` | - |
| 1036 | 8 `DEFEND` | every follower not at a marker or in a hut defends the shaman |
| 1037 | 9 `DEFEND_BASE` | 8 warriors and firewarriors circle the reincarnation site, losses not replaced; needs 1043 ON and `INT_ATTR_USE_PREACHER_FOR_DEFENCE` != 0 |
| 1038 | 10 `SPELL_DEFENCE` | `x z ON`: the shaman's home position, which she defends |
| 1039 | 11 `PREACH` | needed by `PREACH_AT_MARKER` |
| 1040-1042 | 12-14 `BUILD_WALLS`, `SABOTAGE`, `SPELL_OFFENSIVE` | unused or one use |
| 1043 | 15 `SUPER_DEFEND` | spelled `FIREWARRIOR_DEFEND` in scripts, see 1037 |
| 1044 | 16 `BUILD_VEHICLE` | braves build boats or balloons at their huts |
| 1045-1047 | 17-19 `FETCH_LOST_PEOPLE`, `FETCH_LOST_VEHICLE`, `FETCH_FAR_VEHICLE` | - |
| 1048 | 20 `AUTO_ATTACK` | `ATTACK` does nothing while this is OFF |
| 1049 | 21 `MED_MAN_DEFEND` | spelled `STATE_SHAMAN_DEFEND`, refused by the compiler |
| 1050, 1051 | 22 `FLATTEN_BASE`, 23 `BUILD_OUTER_DEFENCES` | - |

Internal states 24-28 (`GUARD_AT_MARKER`, `SEND_ALL_TO_MARKER`, `PRAY_AT_HEAD`, `BOAT_PATROL`, `DEFEND_SHAMEN`)
have no token. They are probably set by the DO commands of the same name (inference).

Other ON/OFF switches:
- `SET_REINCARNATION`: this AI gets a reincarnation site at the start. `SET_NO_BLUE_REINC` is the human
  equivalent, so the shaman's death ends the game.
- `GIVE_UP_AND_SULK`: everyone guards the shaman, who is sent to blue's reincarnation site.
- `FLYBY_ALLOW_INTERRUPT`: the player can skip flybys.
- No meaning in the wiki: `SET_BUCKET_USAGE`, `SET_AUTO_BUILD`, `SET_AUTO_HOUSE`, `AUTO_MESSAGES`,
  `EXTRA_WOOD_COLLECTION` and `TURN_PUSH`.

### Attacks and spells
- `ATTACK team num target_type model damage spell1 spell2 spell3 attack_type bring_back m1 m2 m3`. Requires
  `STATE_AUTO_ATTACK ON` and `INT_ATTR_MAX_ATTACKS` > 0 (0 ignores every attack; the tutorial uses 999).
  - Who goes: up to `num` idle or housed followers (never guards or patrols), weighted by the
    `INT_ATTR_AWAY_*` shares. The shaman joins only if `INT_ATTR_AWAY_SHAMAN` > 0. `num` 0 sends the shaman
    alone.
  - Sequence: the force gathers at the reincarnation site, or at the main drum tower if
    `SET_DRUM_TOWER_POS` was used, unless `INT_ATTR_DONT_GROUP_AT_DT` is set. It leaves, regroups at `m1`
    because units walk at different speeds, then attacks.
  - `target_type`:
    - `ATTACK_MARKER`: `model` is a marker index, and the force fights whatever is there.
    - `ATTACK_BUILDING`: `model` is a building, or `INT_NO_SPECIFIC_BUILDING` for the nearest one. The force
      fights the occupants, then dismantles the building.
    - `ATTACK_PERSON`: in practice always the enemy shaman (`INT_TARGET_SHAMAN`).
  - `damage` is 0..999: how much damage to deal before withdrawing. 999 means to the death.
  - Spells: `spell1` is cast at `m1` toward `m2` when both are set, otherwise at the target. `spell2` and
    `spell3` are cast at the target, each once. Use `INT_NO_SPECIFIC_SPELL` when the shaman is not involved.
    Without charged spells she turns back at `m1`.
  - `attack_type` is `ATTACK_NORMAL`, `ATTACK_BY_BOAT` or `ATTACK_BY_BALLOON` (check that the vehicles exist
    first). `bring_back` (0/1) returns the vehicles. `m3` has no visible effect (?).
  - In Lua, `ATTACK` returns a value [lua]. In scripts, `SET_ATTACK_VARIABLE $v` names a variable the AI may
    update with the attack's outcome (?).
- `NAV_CHECK team target_type model remember $v`: 1 if attackers can reach the target. `remember` is unknown
  (?), and 0 in the examples. The tutorial recommends calling it before `ATTACK`.
- `SPELL_ATTACK spell marker direction`: the shaman walks to the marker and casts.
- `SET_SPELL_ENTRY entry spell mana frequency min_people base`: slot `entry` (from 0) of the shaman's
  automatic spells.
  - `mana` is normally `INT_M_SPELL_x_COST`.
  - She casts when at least `min_people` enemies, shaman included, are in range.
  - `base` 1 means only inside the base, 0 only outside. Lua names it `base_spell`.
  - `frequency` is unknown (?).
- `SET_BUCKET_COUNT_FOR_SPELL spell n` (1502 uses): unknown (?). Probably weights the mana spent per spell.
- `IS_SHAMAN_AVAILABLE_FOR_ATTACK $v`: 1 if the shaman can cast.
- `I_HAVE_ONE_SHOT SPELL_TYPE spell $v`: 1 if she holds a one-shot charge of the spell. The `BUILDING_TYPE` form
  is unknown.
- `GET_SPELLS_CAST team spell $v` and `GET_NUM_ONE_OFF_SPELLS team spell $v`: casts so far, and one-shot
  charges held.
- `DEFEND_SHAMEN n` sends `n` followers to guard the shaman, like the player's G key.
  `SEND_SHAMEN_DEFENDERS_HOME` releases them.
- The six `[DONT_]TARGET_*` commands showed no effect in testing (?).

### Markers, guards and the base
- `SET_MARKER_ENTRY entry m1 m2 braves warriors firewarriors preachers` defines a patrol group: it guards
  between `m1` and `m2`, or circles `m1` when `m2` is -1.
  - `MARKER_ENTRIES e1 e2 e3 e4` activates up to 4 groups, and `CLEAR_GUARDING_FROM e1..e4` deactivates them
    (-1 = unused slot).
  - `ONLY_STAND_AT_MARKERS` makes the groups stand still instead of circling.
  - The wiki calls this pair Bullfrog's preferred method.
- `GUARD_AT_MARKER m b w f p GUARD_NORMAL` and `GUARD_BETWEEN_MARKERS m1 m2 b w f p GUARD_NORMAL` are one-off
  guard orders.
- `VEHICLE_PATROL num m1 m2 m3 m4 BOAT_TYPE|BALLOON_TYPE`: a patrol in vehicles. How the markers are used is
  unknown (?); the examples alternate two of them.
- `SET_BASE_MARKER m` sets the centre of the base, and `SET_BASE_RADIUS r` its radius (for wildmen
  conversion). Bullfrog always calls `RESET_BASE_MARKER` (?) just before `SET_BASE_MARKER`.
  `SET_DEFENCE_RADIUS r` is unknown (?).
- `COUNT_PEOPLE_IN_MARKER team|COUNT_WILD marker radius $v` (144 uses) and `IS_SHAMAN_IN_AREA team marker
  radius $v`.
- `GET_HEIGHT_AT_POS marker $v` gives the ground height. It is used to notice that the water at a marker has
  been land-bridged.

### People, buildings and orders
- `TRAIN_PEOPLE_NOW n model` sends `n` braves to train, e.g. `5 INT_RELIGIOUS`.
- `PRAY_AT_HEAD n marker`: `n` followers pray at the stone head on the marker until it fires. The level needs a
  head, a trigger linked to the effect, and a marker on the same spot.
- `PREACH_AT_MARKER m` sends one preacher. `CONVERT_AT_MARKER m` sends the shaman to convert wildmen.
  `SEND_ALL_PEOPLE_TO_MARKER m` sends everyone, shaman included.
- `SET_DRUM_TOWER_POS x z` sets the main drum tower, which is also the attack gathering point. If the spot is
  unreachable, the nearest reachable one is used. The tower is built automatically unless
  `DELAY_MAIN_DRUM_TOWER` was called (lifted by `BUILD_MAIN_DRUM_TOWER`).
- `BUILD_DRUM_TOWER x z` places a tower plan. `BUILD_AT x z model ?` places any plan; the last parameter may be
  the facing.
- `PUT_PERSON_IN_DT model x z` puts a follower of that model in the tower at x z.
- `SET_BUILDING_DIRECTION d` sets the facing of new buildings, random otherwise (?).
- `IS_BUILDING_NEAR model x z team radius $v` and `IS_PRISON_ON_LEVEL $v`.
- `PARTIAL_BUILDING_COUNT`: the `INT_x_BUILDING_*` counts then include unfinished and damaged buildings.
- `KILL_TEAM_IN_AREA x z radius`: every follower and shaman in the area vanishes, with no death animation.
  `FIX_WILD_IN_AREA`, `CLEAR_STANDING_PEOPLE`, `DELETE_SMOKE_STUFF`, `MARVELLOUS_HOUSE_DEATH` and
  `SET_WOOD_COLLECTION_RADII` are unknown (?).

### Level events (campaign scripts)
- Gifts:
  - `GIVE_MANA_TO_PLAYER team n`.
  - `GIVE_ONE_SHOT spell team` (spell first).
  - `GIVE_PLAYER_SPELL team spell`.
  - `REMOVE_PLAYER_THING team spell|building`.
  - Bloodlust, teleport and armageddon are only granted if the level can obtain them some other way.
- `TRIGGER_THING marker` fires the trigger thing on the marker and everything linked to it (see objects.md).
  `GET_HEAD_TRIGGER_COUNT x z $v` counts a head's activations, and `REMOVE_HEAD_AT_POS x z` sinks it.
- `TRIGGER_LEVEL_WON` and `TRIGGER_LEVEL_LOST` end the level.
- Timer:
  - `SET_TIMER_GOING seconds` shows a countdown at the top right, starting one second short.
  - `HAS_TIMER_REACHED_ZERO $v` and `REMOVE_TIMER`.
- Messages:
  - The tags queue on the left of the screen. `CREATE_MSG_INFORMATION idx` (the "i" tag) and
    `CREATE_MSG_NARRATIVE idx` (the book tag) add one.
  - `CREATE_MSG_INFORMATION_ZOOM idx x z angle` also zooms the camera while the message is open.
  - `OPEN_DIALOG idx` shows a message without queueing it.
  - The `SET_MSG_*` commands change the last message queued: `AUTO_OPEN_DLG` opens it and pauses,
    `DELETE_ON_OK` deletes it when closed, `TIMEOUT turns` sets its expiry. `CLEAR_ALL_MSG` empties the queue.
- Camera:
  - `ZOOM_TO x z angle`, `TRACK_TO_MARKER m`, `TRACK_SHAMAN_TO_ANGLE a`.
  - `CAMERA_ROTATION speed` rotates until `STOP_CAMERA_ROTATION`.
- Flybys:
  - `FLYBY_CREATE_NEW` starts one, then events timed in turns from the start:
    - `FLYBY_SET_EVENT_POS x z start duration`
    - `FLYBY_SET_EVENT_ANGLE angle start duration` (shortest way)
    - `FLYBY_SET_EVENT_ZOOM z start duration`, with z from -100 (out) to 100 (in)
    - `FLYBY_SET_EVENT_TOOLTIP x z idx start duration`
  - `FLYBY_SET_END_TARGET` is unknown (?).
  - `FLYBY_START` and `FLYBY_STOP`.
- Tutorial UI:
  - `DISABLE_USER_INPUTS` and `ENABLE_USER_INPUTS`.
  - `TURN_PANEL_ON` 0 followers, 1 spells, 2 buildings.
  - `FLASH_BUTTON idx ON|OFF` highlights a panel button:
    - buildings 0-8: hut, tower, temple, spy, warrior, firewarrior, boat, balloon, guard post
    - 13-16: the spells, buildings and followers tabs, then the shaman
    - 18-35: the spells
    - 38: the map
  - The `BLUE`-named queries (`COUNT_BLUE_SHAPES`, `COUNT_BLUE_IN_HOUSES`, `IS_BLUE_SHAMAN_SELECTED`...) watch
    what the human does. `MOVE_SHAMAN_TO_MARKER`, `SEND_BLUE_PEOPLE_TO_MARKER` and `DESELECT_ALL_BLUE_PEOPLE`
    act on the human's units.

### Attributes (`INT_ATTR_*`)
| Attribute | Meaning, usual values |
|---|---|
| `EXPANSION` | ? |
| `PREF_{SPY,RELIGIOUS,WARRIOR,FIREWARRIOR}_TRAINS` | number of each training building to have |
| `PREF_{SPY,RELIGIOUS,WARRIOR,FIREWARRIOR}_PEOPLE` | number of each unit to keep, trained and replaced |
| `MAX_BUILDINGS_ON_GO` | buildings under construction at once |
| `HOUSE_PERCENTAGE` | how many huts the base gets |
| `AWAY_{BRAVE,WARRIOR,RELIGIOUS,SPY,FIREWARRIOR}` | 0..100 relative share of the type in an attack force (>100 can exceed `num`) |
| `AWAY_SHAMAN` | > 0: the shaman joins attacks |
| `MAX_ATTACKS` | attack cap; 0 ignores `ATTACK` |
| `DEFENSE_RAD_INCR`, `MAX_DEFENSIVE_ACTIONS`, `RETREAT_VALUE` | ? (0..15 for the first) |
| `BASE_UNDER_ATTACK_RETREAT`, `DONT_USE_BOATS`, `RANDOM_BUILD_SIDE` | 0/1 |
| `PEOPLE_PER_BOAT`, `PEOPLE_PER_BALLOON`, `PREF_BOAT_HUTS`, `PREF_BALLOON_HUTS`, `PREF_BOAT_DRIVERS`, `PREF_BALLOON_DRIVERS` | vehicle use |
| `EMPTY_AT_WAYPOINT` | balloon attackers get out at the waypoint and walk |
| `SHAMEN_BLAST` | the shaman's blast damage: 0, 32, 64, 128 seen; at 0 a shaman dies in 6-8 blasts, at 256 in 2-3 |
| `USE_PREACHER_FOR_DEFENCE` | != 0 needed by `STATE_DEFEND_BASE` |
| `DONT_GROUP_AT_DT` | > 0: no gathering before an attack |
| `ENEMY_SPY_MAX_STAND`, `SPY_CHECK_FREQUENCY`, `SPY_DISCOVER_CHANCE`, `MAX_SPY_ATTACKS` | spy handling (128 or 255 for the first) |
| `FIGHT_STOP_DISTANCE`, `GROUP_OPTION`, `COUNT_PREACH_DAMAGE`, `MAX_TRAIN_AT_ONCE` | ? (0/24/26, 0/2/3, 0/1, ?) |
| `SPELL_DELAY`, `DONT_DELETE_USELESS_BOAT_HOUSE`, `BOAT_HOUSE_BROKEN`, `DONT_AUTO_TRAIN_PREACHERS`, `SPARE` | unused or rare |

Other internal variables worth noting:
- `INT_RANDOM_100` is a random 0..99. Here it must come from `map::Lcg`; Reincarnated separates a synced
  `G_RANDOM` from a local `L_RANDOM` [lua].
- `INT_CP_FREE_ENTRIES` is unknown (`FREE_ENTRIES(pn)` in Lua).
- `INT_NUM_SHAMEN_DEFENDERS` counts the shaman's guards. `INT_CAMERA_*` is the human's camera.
- Name pairs between the scripts and the engine [lua]:
  - `SMALL_HUT` is `TEPEE`.
  - `SHAMAN` is `MEDICINE_MAN`.
  - firewarrior is `S_WARRIOR` (the `firewarriors` parameter of `SET_MARKER_ENTRY` is `s_warriors`).
  - `WRATH_OF_GOD` is armageddon.

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
- **Every campaign level is scripted, with AI and level events mixed in the same script.** Decompiling the
  scripts of 2001-2025 and 2079 shows:
  - all 26 queue messages: `CREATE_MSG_NARRATIVE` in all but 2001, `CREATE_MSG_INFORMATION` in 12;
  - 23 switch on `GIVE_UP_AND_SULK` (all but 2001, 2002 and 2079);
  - 17 play a flyby;
  - 16 watch stone heads (`GET_HEAD_TRIGGER_COUNT`), and 7 fire `TRIGGER_THING`;
  - 4 end the level themselves: 2005 and 2079 are won, 2010 and 2015 are lost (2019 does both), the others end
    by the normal victory rules;
  - the tutorial 2079 (1 069 lines) drives the UI: flashing buttons, panels, the human's shaman.
  When several tribes share a script, its level events run once per tribe. Level 2002, for instance, runs
  cpscr074 for all three tribes: whether the messages repeat or the game only runs events for one tribe is
  unknown.
- The other files in `levels/` hold no scripts:
  - `levlNNNN.inf` (26 B) is a designer note, e.g. "island 1 Access Level".
  - `constant.dat` (11 712 B) is obfuscated.
  - `objectiv.dat` (768 B, 16-byte records) and `levlspc2.dat` (924 B) are small binary tables. Their meaning is
    unknown.
- The wiki's level list gives the same numbers: 2100 uses 100, 2110 uses 085 and 2131 uses 130. It also
  shows the Undiscovered Worlds levels (`levluw/`) using 001, 002, 079, 085, 087, 098, 119, 124, 130 and 138. So
  the missing scripts probably ship with that add-on (no `levluw/` here to check). Unused slots of 2- and
  3-tribe campaign levels often hold 004/005 [wiki].

## Writing scripts the way the originals look
Fields in first-use order, unique; user variables in first-use order; zero words after `SCRIPT_END`; unused fields
filled with `0x03`; user variables and pointers 0; AND/OR chains left-nested; EVERY operands stored minus 1.

## Lua successor (Populous: Reincarnated "Script4") [lua]
The Reincarnated patch runs Lua scripts against the engine. popscript-upgrader turns a classic script into
one. Its output, simplified, shows how the two models line up:

```lua
MY_TRIBE = TRIBE_RED                       -- the converter's --tribe; the script number is dropped
SC2_USR_FLAG = 0                           -- user variables become globals, saved in OnSave/OnLoad
computer_init_player(getPlayer(MY_TRIBE))
function OnTurn()
    if getTurn() == 0 then                 -- IF (INT_GAME_TURN == 0)
        STATE_SET(MY_TRIBE, CP_AT_TYPE_TRAIN_PEOPLE, ON)
        WRITE_CP_ATTRIB(MY_TRIBE, ATTR_EXPANSION, 24)
    end
    if ((getTurn() + MY_TRIBE + 7) % 256 == 0) then   -- EVERY 256 7
        if MANA(MY_TRIBE) >= PLAYERS_SPELL_COST(MY_TRIBE, M_SPELL_BLAST) then
            ATTK_RST = ATTACK(MY_TRIBE, TRIBE_BLUE, 10, ATTACK_BUILDING, M_BUILDING_HUT, 0,
                              M_SPELL_BLAST, M_SPELL_NONE, M_SPELL_NONE, ATTACK_NORMAL, 0, -1, -1, -1)
        end
        SC2_USR_CNT = COUNT_PEOPLE_IN_MARKER(TRIBE_BLUE, 3, 4)   -- query output becomes a return value
    end
end
```

- The `Script4_Popscript` module exposes every classic command as a function taking the player first:
  `ATTACK`, `SET_SPELL_ENTRY`, `SET_MARKER_ENTRY`, `MARKER_ENTRIES`, `STATE_SET`, `READ_CP_ATTRIB` and
  `WRITE_CP_ATTRIB`, the message and flyby functions...
  - Queries return their value instead of writing a variable.
  - Its enums give the runtime values of the keywords: `ATTACK_MARKER/BUILDING/PERSON` = 0/1/2,
    `ATTACK_NORMAL/BY_BOAT/BY_BALLOON` = 0/1/2, `GUARD_NORMAL/WITH_GHOSTS` = 0/1, tribes blue..green = 0..3.
  - They also give the internal-variable numbers, which match the "Internal variables" table one for one
    (`INT_GAME_TURN` 0 ... `INT_BLOODLUST` 1246, attributes 0..47).
- Game state is read straight from the engine structures: `_gsi.Players[t].NumPeople`, `NumPeopleOfType[model]`,
  `PLAYERS_BUILDING_OF_TYPE(t, model)`, `MANA(t)`, `getTurn()`.
- Beyond PopScript, the API has about 30 modules (things, persons, map cells, spells, flybys, drawing, ImGui,
  network, save data). It also has engine hooks: `OnTurn`, `OnCreateThing`, `OnDeleteThing`, `OnTrigger`,
  `OnSpellCast`, `OnPlayerDeath`, `OnLevelInit`, `OnSave`/`OnLoad`, `OnKeyDown`, `OnChat`...
  - The community uses those hooks for level logic that PopScript could not express.
  - Lockstep safety is left to the script author: synced `G_RANDOM` versus local `L_RANDOM`.
- The converter is best effort; trust the original scripts, not its output:
  - Several argument orders differ from its own API dump: `STATE_SET`, `PLAYERS_SPELL_COST`, and spell versus
    building in `GIVE_PLAYER_SPELL`.
  - It does not handle `||` or arithmetic expressions.
  - Its single-file mode is broken.

## What this means here
- Parse `cpscr` into the statement tree above, then interpret it in `game-core` once per turn and per computer
  tribe.
  - Use integers only. `INT_RANDOM_100` comes from `map::Lcg`.
  - Effects go through `Command`, so lockstep replays match.
- Keep the AI behaviours (states, attributes, marker and spell entries, attacks) as plain simulation data that
  the script sets. A future native or Lua-like scripting layer can then drive the same data, the way Script4
  wraps the original functions.
- Open before implementing:
  - the original turn rate (~8 per second?) against our 10 ticks per second
  - the `EVERY` off-by-one and whether there is a per-tribe phase
  - what `SET_ATTACK_VARIABLE` receives
  - the unknown states and commands marked "-" or "?" above
