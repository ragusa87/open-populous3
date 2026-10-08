# Balance constants (original `levels/constant.dat`)

The game's balance values (mana, spell costs, unit life and speed, wood, training, hut breeding, spell ranges...)
live in one obfuscated text file. Decoded here [files]; names and the plain-text form are confirmed by the
community: PopRe wiki page "Constant" and Brandan Lasley's `New_Constants.dat` (2012), whose 231 integer
values all match ours. Not parsed by `pop3-format` yet.

## Encoding [files]
- `levels/constant.dat`, 11 712 bytes in the install checked here (French release, 1998).
- Byte `i` is XORed with `!(1 << ((i + 5) % 8))`: the key cycles `DF BF 7F FE FD FB F7 EF`. The same formula
  decodes and encodes, over the whole file, from byte 0.
- After decoding, bytes 0-1 are `9F C1`, then plain text to the end of the file. The rest of the text has no
  byte outside printable ASCII, tab and CR/LF. Those two bytes are probably a marker or checksum: they are not
  the byte sum, a CRC-32's low half or the length. Unverified.
- The community edits a plain-text `constant.dat` that the game also accepts (wiki), so the loader probably
  tests the marker and decodes only when it is present. Unverified here.

## Text format [files]
```
##########################
#	POP3 BALANCE FILE
##########################

P3CONST_START_MANA		=	30000		# Players mana at start of level
P3CONST_SPROG%_POP_BAND_00_04%	=	30
P3CONST_TRAIN_MANA_BAND_21+	=	250	# % of mana cost used for this number of specialists
```
- Lines end in CRLF, and the last line has no line ending.
- `#` starts a comment, both on its own line and after a value.
- A line is `NAME = integer`, separated by tabs or spaces. Every value is a non-negative decimal.
- Names start with `P3CONST_` and may contain `%` and `+`.
- There are 255 assignments. `P3CONST_CONV_SPY` appears twice: first in the unit group (1), then in the
  building group (4000), so the game probably reads them in order, by position rather than by name. Unverified.

## Contents
Units:
- Speeds are in engine units, and life in hit points.
- Spell ranges (`SP_W_RANGE_*`) are world units, 1 cell = 512.
- `*_COUNT_X8` durations are in turns / 8.

| Group | Constants | Example values |
|---|---|---|
| Mana | `START_MANA`, `MAX_MANA`, `MANA_F_{BRAVE,WARR,...}` per person, `MANA_F_{TRAINING,HOUSED,WORKING}` %, `MANA_F_HUT_LEVEL_1..3` %, `MANA_UPDATE_FREQ` (2^n - 1), `HUMAN/COMPUTER_MANA_ADJUST` %, `SHAMEN_DEAD_MANA_%_LOST/GAIN` | 30 000, 1 000 000; brave 15, specialists 4, shaman 30; update every 15 + 1 turns; human 125 %, computer 50 % |
| Spell costs | `SPELL_<X>` | blast 10 000 ... volcano 800 000 |
| Spell charges | `SP_1_OFF_MAX_<X>` (max one-shots), `SPELL_<X>_OPT_S` (recharge time, seconds) | blast 4 and 30 s, volcano 1 and 300 s |
| Spell ranges | `SP_W_RANGE_<X>`, `ALT_BAND_0..7_SPELL_INCR` / `_SUPER_INCR` (% of the range per altitude band, 80..150) | blast 3072, convert 8192, teleport 65 536 |
| Spell effects | `HYPNO/INVISIBLE/SHIELD/BLOODLUST_COUNT_X8`, `*_NUM_PEOPLE`, `LIGHTNING_NUM_KILLS`, `SWARM_PERSON_DAMAGE`, `FIRESTORM_DURATION`, `LAND_BRIDGE_*`, `AOD_*`, `BLOODLUST_*_X` multipliers, `LSME_DURATION_SECS` | shield 180 x 8 turns, 6 people |
| Units | `LIFE_<P>`, `FIGHT_DAMAGE_<P>`, `<P>_SPEED`, `<P>_DT_RADIUS` (detection radius in a tower) | brave life 1000 speed 70, warrior life 1800 damage 360 |
| Firewarrior | `SW_BLAST_DAMAGE`, `SW_FIRE_RATE`, `SW_*_TOWER`, `SW_BLDG_DAMAGE_DELAY` | 100, 25 |
| Conversion | `CONV_<P>` (preacher), `CONV_<BUILDING>` (training time?), `PREACHEE_CONV_FREQ/CHANCE` | |
| Training | `HUMAN/CP_TRAIN_MANA_<P>`, `TRAIN_MANA_BAND_*` (% of the cost by specialist count) | human warrior 3500, computer 1000 |
| Wood | `TREE1..6_WOOD_VALUE` and `_GROW` (added every 16 frames), `WOOD_<P>` (carried), `WOOD_<BUILDING>`, `WOOD_VEHICLE_*` | tree 400, grow 2, hut 300, temple 800 |
| Huts | `HUT1..3_SPROG_TIME`, `SPROG%_POP_BAND_*` (breeding speed by population %, 30..200), `MAX_POP_VALUE__HUT_1..3`, `NEAR_BLDG_CELLS` | 4000 / 3000 / 2000, 3 / 5 / 7 |
| Terrain | `WALK_ALT_DIFF2`, `BUILD_ALT_DIFF2` (max height step to walk on or build) | 384, 160 |
| Misc | `MULTIPLE_SELECT_NUM`, `DME_RESTORE_TIME`, `TRIGGER_REACTIVATE_TIME`, `SPY_DISGUISE_DELAY`, `HUMAN_REINC_START_DELAY`, `VEHICLE_LIFE_*` | |

## Community findings (not from the files)
- The "extended" or "forbidden" constants are about 100 more values (builders per building, the tree models...).
  Xandra and Brandan found them in the executable's memory in 2012, and they are not in `constant.dat`.
- Value Studio 2012 is a constant editor. The wiki recommends raising `WALK_ALT_DIFF2` to 600-900 and
  `BUILD_ALT_DIFF2` to 200-300 to fix pathfinding and placement on bumpy ground.

## Use here
- Read it from the original install when one is present, and never ship it. Generated mode needs its own
  defaults, written by us, not copied.
- Our spell and unit placeholders (spells.md, units.md, buildings.md) can then be checked against the real
  values.
- The spell costs here are the same numbers the AI scripts read through `INT_M_SPELL_*_COST` (ai-scripts.md).
  Whether those are adjusted, e.g. by `COMPUTER_MANA_ADJUST`, is unknown.
