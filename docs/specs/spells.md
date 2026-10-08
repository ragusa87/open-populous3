# Spells

`game_core::spell::Spell` -> `cast(&mut Heightmap) -> Option<DirtyRect>` (the terrain part). `GameMap::can_cast`
says whether a spell applies at its target; `GameMap::apply` ignores a cast that does not. Implemented:

| Spell | Effect |
|---|---|
| Land Bridge | line between two cells, heights interpolated, min height 32 |
| Flatten | radius 4 set to center height |
| Erode | radius 4 lowered by 120 (falloff) |
| Raise | editor brush, radius 3 +64 |
| Teleport | sandbox only (the original names a spell model 21 teleport, see below, effect unknown): the caster's living shaman does her cast jump, then is at the target (`Unit::cast_teleport`); only onto ground she can walk (`Mobility::Walk`: no sea, no cliff), checked when cast and again when she lands (she stays if it no longer is); another order during the jump cancels it |

To do: Swamp (surface type), Earthquake (seeded noise along a line - use `map::Lcg`, never `rand`),
Volcano (cone + lava layer), Angel of Death, Firestorm... Mana cost, charges, cast range from shaman (original
values below, "Original balance").
After every cast: update normals, walkability and destroy buildings on uneven/flooded ground.
Spells must only be cast through `Command::Cast` to stay deterministic.

## Spell book (`game_core::spell_book`)
Per tribe: one `SpellSlot` per `SpellKind` with `Availability` (Hidden / Discoverable / Provided { shots } /
Known / Unlimited: cast at will, used by sandbox spells), `charges` (max 4, fewer for big spells: `max_charges()` from `cost()`) and `recharge` mana.
`tick(mana)` refills recharging spells that are not paused (`paused`, `toggle_pause`), `cast(kind)` consumes a charge or a provided shot,
`discover(kind)` turns "?" into Known. Integer only, deterministic. UI: see ui-and-editor.md.

`SpellBook::from_level(header, level)`: the header's `SpellsAvailable` panel spells are Known with full charges
(how charged the original starts them is not checked), the spell discoveries (`DiscoveryType 11`) not already
known are Discoverable. Their availability (permanent / this level / once) is not kept yet. Stored as
`GameMap::spell_book` for original levels, the same for every tribe. Armageddon (bit 18) is special: the
campaign's last spell, discovered in levels 17 and 18 and in the masks from level 19 on, always with Convert (17).
Levels 1 and 2 also set bit 18, among leftover bits (1 burn, 20 bloodlust, 21 teleport) and without Convert:
`known_in_header` only takes Armageddon next to Convert. The panel keeps a tile for it.
Convert follows the same campaign path: absent from levels 1-3 (level 1 has no wildmen; levels 2 and 3 only have
the wildmen around the player's site, converted for free, see units.md), single-shot discoveries
in levels 4 and 5, a permanent one in level 6, in every mask from level 7 (except level 22, the shaman alone).

## Original spells (levels and scripts)
Spell model numbers, as used by discovery things, the level header masks and AI scripts (names from the ALACN
world editor, checked against the campaign's discoveries, see level-format.md):

| Model | Spell | Model | Spell | Model | Spell |
|---|---|---|---|---|---|
| 1 | burn | 8 | firestorm | 15 | flatten |
| 2 | blast | 9 | ghost army | 16 | volcano |
| 3 | lightning | 10 | erode | 17 | convert |
| 4 | tornado (whirlwind) | 11 | swamp | 18 | armageddon |
| 5 | swarm (insect plague) | 12 | land bridge | 19 | magical shield |
| 6 | invisibility | 13 | angel of death | 20 | bloodlust |
| 7 | hypnotism | 14 | earthquake | 21 | teleport |

`SpellKind::model()` / `SpellKind::from_model()` map them; `from_model` only returns the 18 spells of the
original panel (2-19): burn, bloodlust and teleport are never on it (our `Teleport` is a sandbox spell that keeps
model 21 for reference). Our costs are placeholders on another scale (blast 40); the original ones are under
"Original balance".

The editor also names models 23-30 (hill, rise, valley, dip, place tree, clear mapwho, place shaman, place
wild) but can't place them: unverified.

How a level grants spells:
- **Level header** `SpellsAvailable`: bit N = spell N, known from the start. `SpellsNotCharging` uses the same bits.
- **Discovery things** (general model 2, `DiscoveryType 11`, `DiscoveryModel` = spell): availability 1 permanent,
  2 this level, 3 once ("one shot"). A spell discovered in a level shows up in the start mask of later levels.
- **Mana discoveries**: `DiscoveryType 6`, model 3, 50 000 mana.
- **AI scripts** can give spells or one-shots and set spell costs (see ai-scripts.md).

Land bridge effects in the levels store their target cell (see level-format.md).

## Original balance (`constant.dat`) [files]
Decoded from `levels/constant.dat`, see constants.md for the file. Names are `P3CONST_` + the column's prefix and
the spell's suffix.

| Model | Spell | Suffix | Cost `SPELL_` | Charges `SP_1_OFF_MAX_` | `SPELL_x_OPT_S` | Range `SP_W_RANGE_` (cells) |
|---|---|---|---|---|---|---|
| 2 | blast | `BLAST` | 10 000 | 4 | 30 s | 3072 (6) |
| 3 | lightning | `BOLT` | 80 000 | 4 | 180 s | 6144 (12) |
| 4 | tornado | `WWIND` | 90 000 | 3 | 180 s | 4096 (8) |
| 5 | swarm | `PLAGUE` | 40 000 | 4 | 45 s | 6144 (12) |
| 6 | invisibility | `INVIS` | 50 000 | 4 | 120 s | 4096 (8) |
| 7 | hypnotism | `HYPNO` | 85 000 | 3 | 240 s | 4096 (8) |
| 8 | firestorm | `FIREST` | 400 000 | 2 | 180 s | 4096 (8) |
| 9 | ghost army | `GARMY` | 18 000 | 4 | 60 s | 4096 (8) |
| 10 | erode | `EROSION` | 210 000 | 2 | 240 s | 4096 (8) |
| 11 | swamp | `SWAMP` | 100 000 | 3 | 180 s | 4096 (8) |
| 12 | land bridge | `LBRIDGE` | 70 000 | 4 | 180 s | 5120 (10) |
| 13 | angel of death | `AOD` | 510 000 | 1 | 300 s | 3072 (6) |
| 14 | earthquake | `QUAKE` | 175 000 | 2 | 240 s | 4096 (8) |
| 15 | flatten | `FLATTEN` | 125 000 | 3 | 240 s | 4096 (8) |
| 16 | volcano | `VOLCANO` | 800 000 | 1 | 300 s | 3072 (6) |
| 17 | convert | `CONVERT_WILD` (`CONVERT` for charges) | 10 000 | 4 | 300 s | 8192 (16) |
| 18 | armageddon | `ARMAGEDDON` | - | 1 | 60 s | - |
| 19 | magical shield | `SHIELD` | 60 000 | 4 | 120 s | 4096 (8) |
| 20 | bloodlust | `BLOODLUST` | - | 4 | 120 s | 4096 (8) |
| 21 | teleport | `TELEPORT` | - | 4 | 120 s | 65 536 (128) |

Reading the columns:
- Cost is the mana of one cast ("Mana cost of firing spell" in the file's comments). The AI scripts read it as
  `INT_M_SPELL_x_COST`.
  - Armageddon, bloodlust and teleport have no cost entry.
  - Mana: start 30 000, cap 1 000 000, updated every `MANA_UPDATE_FREQ` + 1 = 16 turns. Income is scaled by
    `HUMAN_MANA_ADJUST` 125 % for humans and `COMPUTER_MANA_ADJUST` 50 % for computer players.
  - A shaman's death loses 25 % of her tribe's mana, and her killer gains 25 % (`SHAMEN_DEAD_MANA_%_LOST/GAIN`).
- Charges: `SP_1_OFF_MAX` ("one-off max") is read as how many charges a spell holds, the panel's 1-4 count.
  This fits the game but is unverified. Our `SpellKind::max_charges()` derives 4/2/1 from the cost instead, which
  gets tornado, hypnotism, swamp and flatten (3) and lightning (4) wrong.
- `_OPT_S` is in seconds per the comments. Its meaning is unverified: probably the charge time of one charge at
  a reference income ("optimum").
- Range is the cast distance from the shaman in world units (512 per cell).
  - It is scaled by the caster's altitude: `ALT_BAND_0..7_SPELL_INCR` give 80, 90, ..., 150 % from the lowest
    band to the highest.
  - Firewarriors' shots use the same scaling (`ALT_BAND_x_SUPER_INCR`).
  - How heights map to the 8 bands is unknown. A guess is the height divided into 8 equal bands up to the map's
    maximum.
  - Teleport's 65 536 (128 cells) is half the 256-cell torus. Along an axis it reaches anywhere; the diagonal
    corners are further (181 cells) if the distance is Euclidean.
  - Convert has the longest real range (16 cells), and blast, angel of death and volcano the shortest (6).

Effect constants:
| Constant | Value | Meaning |
|---|---|---|
| `HYPNO_COUNT_X8`, `INVISIBLE_COUNT_X8`, `SHIELD_COUNT_X8`, `BLOODLUST_COUNT_X8` | 55, 180, 180, 180 | duration in turns / 8 (440, 1440 turns) |
| `HYPNO_NUM_PEOPLE`, `INVIS_NUM_PEOPLE`, `SHIELD_NUM_PEOPLE`, `BLOODLUST_NUM_PEOPLE` | 6 each | people affected per cast |
| `LIGHTNING_NUM_KILLS` | 6 | |
| `SWARM_PERSON_DAMAGE` | 100 | |
| `FIRESTORM_DURATION` | 220 | turns? |
| `LAND_BRIDGE_MAX_CHANGE`, `LAND_BRIDGE_DURATION` | 256, 64 | height change limit, build time |
| `AOD_KILL_COUNT`, `AOD_DURATION`, `LIFE_AOD`, `BLAST_DAMAGE_AOD` | 40, 2500, 10 000, 10 | the angel: kills, lifetime, hit points, damage it takes from blast |
| `BLOODLUST_DAMAGE_X`, `BLOODLUST_HEALTH_X`, `BLOODLUST_SW_BLAST_X` | 3 each | multipliers |
| `DME_RESTORE_TIME` | 6000 | unknown |
| `LSME_DURATION_SECS` | 120 | unknown |
