# Spells

`game_core::spell::Spell` -> `cast(&mut Heightmap) -> Option<DirtyRect>` (the terrain part). `GameMap::can_cast`
says whether a spell applies at its target; `GameMap::apply` ignores a cast that does not. Implemented:

| Spell | Effect |
|---|---|
| Land Bridge | aimed on the ground (`hud::spells::ground_spell`), within range, at land only (not open sea, `unit::is_sea`: the water on its way is raised); released like a blast 6 turns into her cast jump; starts at the caster's shaman as she stands when the command runs, or where it was aimed from on a god level (`GameMap::shaman_omni`: the ground under the bottom middle of the view). An effect (`land_bridge`, `EffectKind::LandBridge`): 4 height points wide (−1, 0, +1, +2 across the line), each moving every turn towards a straight slope between both ends' heights at cast (at least 90), `h += clamp(T − h, ±256) / (64 − turn)`, for 64 turns (`LAND_BRIDGE_MAX_CHANGE`, `LAND_BRIDGE_DURATION`): sea raised, hills in the way lowered. The editor's B brush is still the instant 1-wide line (min 32). While it builds, a tiny star twinkles at the centre of each cell with a corner not yet at its height (`sparkles::SparkleView`, from `Effect::reshaping`: every terrain effect gets them). Dev: `BRIDGE=x,z` (cells, from the player's shaman), `BRIDGE_TICKS=N` |
| Flatten | aimed on the ground at the nearest height point (`flatten::nearest_point`), released 6 turns into the cast jump like a blast. An effect (`flatten`, `EffectKind::Flatten`): the 45 height points of a disc (within √13 of the target point, about 3.5 cells: the original's count) move each turn towards the target point's height at cast, `h += (T − h) / (15 − turn)`, for 15 turns (the original's length; its curve is not traced, ours lands on the last turn): hills cut, hollows and sea filled. Sparkles on each cell it is reshaping (`sparkles`). Buildings on it are not re-levelled or destroyed yet (TODO). Free in the sandboxes. The editor's T brush is still the instant radius 4. Dev: `FLATTEN=x,z` (height point, another tribe's), `FLATTEN_TICKS=N` |
| Erode | radius 4 lowered by 120 (falloff) |
| Raise | editor brush, radius 3 +64 |
| Teleport | sandbox only (the original names a spell model 21 teleport, see below, effect unknown): the caster's living shaman does her cast jump, then is at the target (`Unit::cast_teleport`); only onto ground she can walk (`Mobility::Walk`: no sea, no cliff), checked when cast and again when she lands (she stays if it no longer is); another order during the jump cancels it; she lands falling from a cell up (units.md "Landing") |
| Blast | aimed on the ground (`hud::spells::ground_spell`): the shaman's cast jump, and `CAST_RELEASE` (6 turns) into it, when the original's shot leaves her (no shot flying there yet), a simple blast effect at the target (`blast::effect`, in `GameMap::effects`): the original's simple blast, 3 turns at radius 2, 4 then 5 cells. Each turn every living person outside buildings in the radius takes `BLAST_DAMAGE_PERSON` 50 (`GameMap::damage`: not the caster's tribe, not the shielded); each living person of any tribe but the caster's own shaman, not inside a building, within the turn's radius and 2.5 cells (`PUSH_RANGE` 1280) is flung once (`Unit::fling`): away from the centre at 140 and up at 98 (world / height units per tick), times `(1280 - d) / 1280`, then tumbles and falls (units.md "Physics"). The original pushes again on each turn the person is still within 2.5 cells, scales the push by the slope and treats allies apart (pop3-rev-analysis.md "Blast push and flight"; TODO). Dev: `BLAST=x,z` (cells, cast by another tribe), `BLAST_TICKS=N` |
| Lightning | aimed on the ground like a blast (`Spell::Lightning`, world units), released 6 turns into the cast jump, walked into range first; free in the sandboxes. An effect (`lightning`, `EffectKind::Lightning`, 12 turns, a guess): on its first turn it kills outright up to `LIGHTNING_NUM_KILLS` + 1 = 7 people in the target cell outside buildings and inside the building it hits, in id order (`DamageKind::Lightning` through `GameMap::damage`: any tribe, the caster's own followers and enemy shamans too; spared: the caster's own shaman and the shielded), sets a building covering the point on fire (`WorldEvent::SetFire`, `fire`: its survivors thrown out, one piece of wood lost), and spawns a simple blast there. Drawn (`client lightning.rs`, our own look): a jagged white-blue bolt from 8 cells up, flickering between 3 zigzags seeded by the effect id and fading over the strike, and a glow on the ground. Not yet: burning trees, the carbonized victims (anim 97), the shot from the shaman. Dev: `LIGHTNING=x,z` (cells, another tribe's), `LIGHTNING_TICKS=N` |

To do: the other original spells, as traced in pop3-rev-analysis.md "Per spell" (Swamp kills people on its cells and
does not change the terrain; Earthquake sinks a half-disc and opens a lava fissure; Volcano raises a cone with lava;
any randomness through `map::Lcg`, never `rand`). Mana cost, charges, cast range from shaman (original values
below, "Original balance"). Range: `cast_range` (`SP_W_RANGE` by the caster's altitude band, `GameMap::altitude`,
+1/3 in a finished drum tower), none on a god level. Cast beyond it (`GameMap::in_cast_range`), the shaman walks as close to the target as she can,
reachable or not (land on another island, the sea): to the point nearest it of the cell nearest it among those she
can walk to (`path::nearest_reachable`, `path::closest_point_in_cell`), and casts as soon as she is in range (`GameMap::walk_to_cast`, `cast_when_in_range`;
the original's walk-to-cast state): no charge is used before, another order on the way drops the cast, and she gives
up if she stops short; without a living shaman the spell takes effect at once; nothing is cast
while the shaman is off the ground.
Our Erode differs from the original's (pop3-rev-analysis.md "Per spell"): its erosion is a hydraulic simulation over 63
turns on 8 × 8 cells.
After every cast: update normals, walkability and destroy buildings on uneven/flooded ground.
Spells must only be cast through `Command::Cast` to stay deterministic.

## Effects (`game_core::effect`)
In the original a cast flies a shot to the target, which spawns reusable components there (pop3-rev-analysis.md
"From cast to effect", "Components"); the components do the work. Ours: `GameMap::effects`, a `Registry<Effect>` (by
`Id<Effect>`) of `Effect` (caster's tribe, position, `left` turns, `EffectKind` with only its own data), ticked once per turn before the
units (`GameMap::tend_effects`, architecture.md "The tick"). An effect reads the map and returns `WorldEvent`s (`event.rs`):
`Damage` (with the caster's tribe as `from`, through `GameMap::damage`, the one place for immunities: shielded units, the caster's own tribe for a
blast), `Push` (`Unit::fling`, not the shielded), `SpawnEffect` (a new effect, ticking from the next turn), `Terrain`
(raise, flatten). `SetFire` burns a building (`fire`, buildings.md "Damage and repair"). `Panic` sets the person running (`Action::Panicking`: out by the door first when inside, then `PANIC_SPEED` 110 at a random heading for `PANIC_TICKS` 64 turns, a quarter turn left when the way is blocked, no orders meanwhile; a guess: the original's turning not traced). `Raid` is routed but does nothing yet. An effect never holds another
object's state: a unit it panics or sets on fire counts down its own state.

## Spell book (`game_core::spell_book`)
Per player (`GameMap::spell_books`, by owner): one `SpellSlot` per `SpellKind` with `Availability` (Hidden / Discoverable / Provided { shots } /
Known / Unlimited: cast at will, used by sandbox spells), `charges` (max 4, fewer for big spells: `max_charges()` from `cost()`, to replace with `SP_1_OFF_MAX`, see
"Original balance") and `recharge` mana.
`charge(mana)` shares mana evenly between the spells taking it (known, not full, not paused: `set_paused`), a
spell filled up passing the rest of its share on, and returns what is left; `cast(kind)` consumes a charge or a
provided shot, `discover(kind)` turns "?" into Known. Integer only, deterministic.
Provided shots (a totem's or a vault's one-shot gift, `worship::one_more_shot`) never recharge and cannot be paused;
each gift adds a shot, none to a spell already known. The last shot used leaves the slot empty (Hidden), even when it
was "?" before the gift: "?" means not discovered, and the gift showed the spell, so it does not come back. The engine owns it: casts
(`Command::Cast` for a spell aimed at a spot, through `Spell::kind`; `Command::CastSelf` for the others) use a charge
and are ignored without one, `Command::PauseSpell` pauses, rewards go to their tribe's book. The terrain brushes
(`Spell::kind` None: editor and dev) use no book. The view only reads it (ui-and-editor.md).

## Mana (`game_core::mana`)
Per player (`GameMap::mana`, by owner): `ManaPool { bank, incoming, received, used }`, amounts as `Mana`. As in the
original [exe] (pop3-rev-analysis.md "Mana rules"), two entry points:
- `add(amount)`: mana gained during the turn (a reward) only goes to `incoming`;
- `tick(now, book)`, once per turn from `GameMap::tick`: the income every 16 turns (`MANA_EVERY`, a fixed `INCOME`
  guessed until income from people), then `distribute`: `incoming` charges the spells first (`SpellBook::charge`),
  the rest goes to the bank (capped at `MAX_MANA`). Only this changes the bank and the charges.

`received` is the last income, `used` what went into spells since then: with `bank`, what a mana bar shows.
Starts at `START_MANA`. Not done: income from people, the delayed reservoir, a cast's own cost (`incoming -=
cost` in the original, on top of our prepaid charges?).

`SpellBook::from_level(header, level)`: the header's `SpellsAvailable` panel spells are Known with full charges
(how charged the original starts them is not checked), the spell discoveries (`DiscoveryType 11`) not already
known are Discoverable. Their availability (permanent / this level / once) is not kept yet. Given to every
player of an original level (`GameMap::spell_books`); generated maps and levels without a header get
`SpellBook::demo` (every tile state), the sandboxes `SpellBook::sandbox` (plus Teleport, unlimited). Armageddon (bit 18) is special: the
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
  - A shaman's death loses 25 % of her tribe's mana, and her killer gains 25 % (`SHAMEN_DEAD_MANA_%_LOST/GAIN`),
    after a 1000-turn delay [exe].
- Charges: `SP_1_OFF_MAX` ("one-off max") is how many charges a spell holds, the panel's 1-4 count [exe]. Our
  `SpellKind::max_charges()` derives 4/2/1 from the cost instead, which gets tornado, hypnotism, swamp and
  flatten (3) and lightning (4) wrong.
- `_OPT_S` is in seconds per the comments. It is not a recharge or cooldown time: the original only reads it in the
  advisor's mana bookkeeping, and no per-spell cooldown exists, only a 1 s cooldown after any human cast [exe].
- Range is the cast distance from the shaman in world units (512 per cell).
  - It is scaled by the caster's altitude: `ALT_BAND_0..7_SPELL_INCR` give 80, 90, ..., 150 % from the lowest
    band to the highest.
  - Firewarriors' shots use the same scaling (`ALT_BAND_x_SUPER_INCR`).
  - The caster's band is `clamp(z >> 7, 0, 7)`, 128 height units per band, interpolated linearly to the next
    band [exe]. A shaman in a finished drum tower gets +1/3.
  - Teleport's 65 536 (128 cells) is the farthest any point can be along one axis of the 256-cell torus, so it
    reads as "no limit". Every other range is 6-16 cells. The original checks the range in the interface and the
    shaman's walk-to-cast, not again when the command runs [exe]; our sandbox teleport has no range limit.
  - Convert has the longest real range (16 cells), and blast, angel of death and volcano the shortest (6).

Effect constants:
| Constant | Value | Meaning |
|---|---|---|
| `HYPNO_COUNT_X8`, `INVISIBLE_COUNT_X8`, `SHIELD_COUNT_X8`, `BLOODLUST_COUNT_X8` | 55, 180, 180, 180 | duration in turns / 8 (440, 1440 turns) |
| `HYPNO_NUM_PEOPLE`, `INVIS_NUM_PEOPLE`, `SHIELD_NUM_PEOPLE`, `BLOODLUST_NUM_PEOPLE` | 6 each | people affected per cast |
| `LIGHTNING_NUM_KILLS` | 6 | at most this + 1 people killed in the target cell [exe] |
| `SWARM_PERSON_DAMAGE` | 100 | |
| `FIRESTORM_DURATION` | 220 | turns (18.3 s): one fireball every 4 turns [exe] |
| `LAND_BRIDGE_MAX_CHANGE`, `LAND_BRIDGE_DURATION` | 256, 64 | height change limit, build time |
| `AOD_KILL_COUNT`, `AOD_DURATION`, `LIFE_AOD`, `BLAST_DAMAGE_AOD` | 40, 2500, 10 000, 10 | the angel: kills, lifetime (turns), hit points; the exe's matching constant is `SW_BLAST_DAMAGE_AOD` (firewarrior shots against an angel, % of `SW_BLAST_DAMAGE`), so this unprefixed file name may not apply (unverified) |
| `BLOODLUST_DAMAGE_X`, `BLOODLUST_HEALTH_X`, `BLOODLUST_SW_BLAST_X` | 3 each | multipliers |
| `DME_RESTORE_TIME` | 6000 | unknown |
| `LSME_DURATION_SECS` | 120 | length of the timed landscape-shaping mode, where erosion, land bridge and flatten get 5 charges [exe] |
