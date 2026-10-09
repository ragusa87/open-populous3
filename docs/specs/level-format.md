# Original level format (Populous: The Beginning)

Files in `levels/`: `levlNNNN.dat` (map + objects), `levlNNNN.hdr` (616 B, settings + name), `levlNNNN.inf`,
`levlNNNN.ver` (68 B), plus `cpscrNNN.dat` / `cpatrNNN.dat` (AI scripts and attributes, see ai-scripts.md).

Sources:
- The "v2" structures of the ALACN Pop World Editor (`pop.h`, PopRe `3d02fa3`, no licence: facts only), also used
  by PopResourceEditor (`Level.h`, MIT, `140e389`).
- Field meanings from the editor's dialogs.

Everything is checked on the 41 shipped levels. Tags:
- **[files]**: verified on the files.
- **[editor]**: from the editor only, can't be checked without the game.
- **[contradicted]**: the editor disagrees with the files, which win.

## `.dat` (192 137 bytes, little endian, packed)

| Offset | Size | Content |
|---|---|---|
| 0 | 128*128*2 | `u16 GroundHeight`, index `z*128 + x` (x = thing `PosX`). 0 = sea, max 1024 |
| 32 768 | 16 384 | `u8 LandBlocks` |
| 49 152 | 16 384 | `u8 LandOrients` |
| 65 536 | 16 384 | `u8 NoAccessSquares` |
| 81 920 | 4 x 16 | player start info: `i16 StartPosX, i16 StartPosY, i32 Future[3]` |
| 81 984 | 3 | sunlight: `u8 ShadeStart, ShadeRange, Inclination` |
| 81 987 | 2000 x 55 | things |
| 191 987 | 50 x 3 | access info: `u8 Model, Type, Rights` |

[files]:
- **Axis order**: 95% of persons and buildings stand on land with `z*128 + x` and only 43% with the axes swapped.
  Markers agree (see `.hdr`).
- **Heights are per vertex**: one height per grid point, a cell is the quad (x, z)..(x+1, z+1). Max 1024 (27
  levels reach it exactly). The editor allows up to 1792 [editor].
- **LandBlocks and LandOrients are byte-identical to each other in all 41 levels.** Their non-zero cells are mostly
  in the sea and often on even indices, so they look like stale data. Meaning unknown: ignore them.
- NoAccessSquares: 0/1, non-zero only in levl2002 (40 cells) and levl2079 (174 cells).
- Player start info is a placeholder in every level ((1,1), (2,2), (3,3), (4,4), futures 0). Access info is all
  zero.
- Sunlight is always ShadeStart 28, ShadeRange 15, Inclination 64 (29 levels) or 32 (12). Probably the terrain
  lighting input (see terrain-textures.md); the editor doesn't use it.

The editor rewrites LandBlocks, LandOrients, NoAccessSquares, start info, sunlight and access info as zeros when
it saves [editor]. So the game probably doesn't need them, but keep the sunlight when writing.

### Things
[files]:
- **Things are packed from slot 0 with no gaps; every empty slot is 55 zero bytes.**
- `model == 0` exactly when `type == 0`.
- Trigger links point at slots by 1-based index, so keep the slot order.

| Off | Type | Field |
|---|---|---|
| 0 | u8 | model (subtype, per type below) |
| 1 | u8 | type: 1 person, 2 building, 3 creature, 4 vehicle, 5 scenery, 6 general, 7 effect, 8 shot, 11 spell |
| 2 | i8 | owner: -1 (255) neutral, 0 blue, 1 red, 2 yellow, 3 green (the editor also has 4 "hostbot", and writes -2 for a neutral prison) |
| 3 | u16 | x, world units (512 per cell) |
| 5 | u16 | z |
| 7 | 48 B | union, by type (below) |

**Positions [files]:**
- **Buildings sit exactly on a cell corner** (`x % 512 == 0`, all 598).
- Scenery, generals and effects sit at a cell centre (`x % 512 == 256`).
- Persons: centre (4978) or anywhere (293).

The cell is `x >> 9` either way. The editor writes every thing at the corner [contradicted for non-buildings].

**Angles**: 2048 = a full turn (0x100 = 45 degrees), rotation about the vertical axis. Only quarter turns are seen.
The editor rotates buildings by quarter turns and scenery by eighths [editor].

**Unions**:
- **Building (2)**: `i32 Angle` at 7, seen 0, 512, 1024, 1536. Bytes 11-54 are 0: no construction progress,
  damage or people inside. Every building in a level is stored as a finished one; a building that looks unfinished in
  the game (level 10's raised island) gets that state while the game runs (buildings.md, "Damage and repair").
- **Scenery (5)**: `u8 PortalStatus@7, PortalLevel@8, PortalType@9, i16 Angle@10, u8 UserId@12,
  i16 IslandAlt@13, u8 IslandNum@15, BridgeNum@16`, rest 0.
  - Angles 0 / 512 / 1024 / 1536 seen 2899 / 357 / 764 / 536 times.
  - Portal and island fields are always 0; UserId is 1-3 four times.
  - The editor's scenery 18/19 ("top/sub level scenery") store the object to draw in `IslandNum@15` (39-44)
    [editor]. No level has them.
- **Person (1)** and **vehicle (4)**: bytes 7-54 always 0, no angle.
- **General (6), model 2, discovery**:

  | Off | Type | Field |
  |---|---|---|
  | 7 | u8 | DiscoveryType: a thing type, 11 spell, 2 building, 6 mana |
  | 8 | u8 | DiscoveryModel: the spell or building model; 3 for mana |
  | 9 | u8 | AvailabilityType: 1 permanent, 2 this level, 3 once |
  | 10 | u8 | TriggerType: 0 normal, 1 immediate; always 1 in the files |
  | 11 | i32 | ManaAmt |

  Mana discoveries in the files: type 6, model 3, 50 000 mana. The editor writes model 5 [contradicted], and caps
  ManaAmt at 1 000 000.
- **General (6), model 6, trigger**:

  | Off | Type | Field | Files (editor range) |
  |---|---|---|---|
  | 7 | u8 | TriggerType: 0 proximity, 1 timed, 2 player death, 3 shaman proximity, 4 library, 5 shaman + angel of death | 0 x124, 4 x23, 3 x21, 2, 5 |
  | 8 | u8 | CellRadius | 1 (0, 2 rare) (0-4) |
  | 9 | u8 | RandomValue | always 0 |
  | 10 | i8 | NumOccurences | 0-5 (-1..120) |
  | 11 | u16 | TriggerCount | 1-31 (0-32000) |
  | 13 | u16[10] | ThingIdxs: things to activate, **1-based slot index**, 0 = none | |
  | 33 | i16 | PrayTime | 0-1000 (0-1000) |
  | 35 | u8 | StartInactive | always 0 |
  | 36 | u8 | CreatePlayerOwned | 1 three times |
  | 37 | i16 | InactiveTime | 1 or 768 (0-1000) |

  ThingIdxs land on the discoveries, effects and things a trigger reveals (164 of 170 hit a non-empty slot; read
  0-based they make no sense). levl2020 has 6 dangling indices (1708-1713, it has 470 things).
- **Vault of knowledge (building 18) and its reward**: the building record has no link to what it gives; the link
  is by position. Each of the 23 vaults in the levels has exactly one library trigger (type 4) on its own cell, and
  no library trigger stands elsewhere. That trigger targets exactly one discovery, always permanent: a spell
  (kind 11, 15 vaults) or a building (kind 2, 8 vaults; level 3: model 5, the temple). Its `PrayTime` is 20-200
  (level 3: 100). So a vault's reward is: trigger type 4 on the vault's cell -> its target -> `Discovery`.
- **What triggers stand on** (all levels, 170 triggers; a thing on the trigger's cell): 23 library triggers
  (type 4) on vaults; 98 on scenery 9 (the "stone head" here, the totems players pray at): type 0 (followers
  pray, `TriggerCount` 1-8 of them, `PrayTime` 15-1000, `NumOccurences` 0-4 times) or type 3 (shaman only,
  `PrayTime` 5-35), one type 5 (level 5); 19 on effects alone (17 lightning, 15, 26, 85: repeating hazards in
  level 21); 30 on nothing (walk-in triggers). Scenery 9 rewards: spells with `Once` availability (one-shot, never
  `Permanent` like a vault's), mana (discovery kind 6), effects (24 land bridge, 17 lightning, 23, 26, 81...), a
  vehicle (level 5: boat), revealed scenery 9 and triggers (chains), sometimes several targets at once.
- **General (6), model 9** ("building add-on"): all zero. Always within about 2 cells of a medium or large hut of
  the same owner (91 of 91).
- **Effect (7)**: rest zero, except:
  - model 24 (land bridge) and 17 (lightning bolt) store a **target**: `i32@7` = target x, `i32@11` = target z,
    each a u16 world coordinate sign-extended to 32 bits (`0xffffcb00` = 0xcb00). Land bridges point 8 cells away
    along one axis; bolts often target their own position.

### Types and models
Names from the editor. Bracketed names exist in the game but the editor can't place them. Counts are things in
the 41 levels.

| Type | Models (count) |
|---|---|
| 1 person | 1 wild (3829), 2 brave (595), 3 warrior (290), 4 preacher (195), 5 spy (2), 6 firewarrior (242), 7 shaman (118), 8 angel of death (0) |
| 2 building | 1-3 hut small/medium/large (11/75/204), 4 drum tower (164), 5 temple (29), 6 spy (17), 7 warrior (31), 8 firewarrior training (28), [9 reconversion, 10 wall piece, 11 gate, 12 curr OE slot], 13/14 boat hut (11/0), 15/16 airship hut (4/0), [17 guard post], 18 vault of knowledge (23), 19 prison (1) |
| 3 creature | [1 bear, 2 buffalo, 3 wolf], 4 eagle, [5 rabbit, 6 beaver, 7 fish]: none in levels |
| 4 vehicle | 1/2 boat (3/0), 3/4 airship (1/0) |
| 5 scenery | 1-6 trees (2244/775/286/317/391/208), 7 plant 1 (187), 8 plant 2 (50), 9 stone head (98), 10 fire, 11 wood pile, 12 reincarnation-site pillar, 13 rock, [14 portal, 15 island, 16 bridge, 17 dormant tree], 18 top-level / 19 sub-level scenery |
| 6 general | 1 light, 2 discovery (101), [3 debug static, 4 debug flying], 5 debug flag, 6 trigger (170), [7 vehicle construction, 8 mapwho thing], 9 building add-on (91), [10 discovery marker] |
| 7 effect | 1-94, see below |
| 8 shot | 1-3 standard, 4 fireball, 5 lightning, 6 super warrior, 7/8 volcano fireball: none in levels |
| 11 spell | the spell models (spells.md): none in levels |

The editor also lists, without being able to place them: type 9 shape and type 10 internal (models 1-19).

Effects seen in the levels, mostly trigger targets: 5, 15, 17, 18, 19, 22, 23 (79), 24, 26, 30, 31, 39, 65, 75, 79,
81 (204), 83, 85 (103), 88-92.
- Level 10 is the only one with 83 (boat hut repair), 89 (atlantis set) and 90 (atlantis invoke):
  - 89 (slot 144) stands in the middle of a low island (heights 15-145) around cell (64, 6) holding the player's
    huts, drum tower, warrior and firewarrior huts, temple and 20 wildmen.
  - 90 (slot 145) is fired by trigger 120 on the stone head at cell (18, 29), with pray time 64.
  - 83 (slot 159) is on the same spot as boat hut 160.
  - Likely meaning: "atlantis set" sinks the island at the start and "atlantis invoke" raises it back. Not checked
    in the game. All effect names by model:
- 1-10: simple blast, sprite circles, smoke, lightning element, burn cell obstacles, flatten land, move pillar,
  prepare site land, sphere explode, fireball
- 11-20: fire cloud, ghost army, invisibility, explode building partial, volcano, hypnotism, lightning bolt, swamp,
  angel of death, whirlwind
- 21-29: insect plague, firestorm, erosion, land bridge, wrath of god, earthquake, fly thingummy, sphere explode
  and fire, big fire
- 30-40: lightning, flatten, general, shape sparkle, lava flow, volcano explosions, purify land, unpurify land,
  explosion 1, explosion 2, lava square
- 41-50: whirlwind element, lightning strand, whirlwind dust, raise land, lower land, hill, valley, place tree,
  rise, dip
- 51-60: rock debris, clear mapwho, place shaman, place wild, building smoke, much simpler blast, tumbling branch,
  conversion flash, hypnosis flash, sparkle
- 61-70: small sparkle, explosion 3, rock explosion, lava gloop, splash, smoke cloud, smoke cloud constant,
  fireball 2, ground shockwave, orbiter
- 71-80: big sparkle, meteor, convert wild, building smoke full / partial / damaged, delete pillars, spell blast,
  firestorm smoke, player dead
- 81-90: reveal fog area, shield, boat hut repair, reedy grass, swamp mist, armageddon, bloodlust, teleport,
  atlantis set, atlantis invoke
- 91-94: statue to angel of death, fill one shots, fire roll element, armageddon arena

Each tribe has exactly one shaman thing; its position is the tribe's reincarnation site (the game builds the site
there, it is not a separate thing in the file).

## `.hdr` (616 bytes)

| Off | Type | Field |
|---|---|---|
| 0 | u32 | SpellsAvailable: bit N = spell model N |
| 4 | u32 | BuildingsAvailable: bit N = building model N |
| 8 | u32 | BuildingsAvailableLevel (0 everywhere) |
| 12 | u32 | BuildingsAvailableOnce (0 everywhere) |
| 16 | u32 | SpellsAvailableLevel, aka SpellsNotCharging, same bits (0 everywhere) |
| 20 | u8[32] | SpellsAvailableOnce (0 in the campaign, junk text in some multiplayer levels) |
| 52 | u16 | VehiclesAvailable: bit N = vehicle model N |
| 54 | u8 | TrainingManaOff (0) |
| 55 | u8 | Flags (0) |
| 56 | char[32] | level name, NUL-terminated ("Level 1") |
| 88 | u8 | number of tribes (2-4; editor 1-4) |
| 89 | u8[3] | AI script number of red, yellow, green (`cpscrNNN.dat`, see ai-scripts.md), 0 = none; junk beyond the number of tribes |
| 92 | u8[4] | DefaultAllies: per tribe, bitmask of allied tribes (1 blue, 2 red, 4 yellow, 8 green) |
| 96 | u8 | landscape theme, 0-35 -> files `*0-X.dat` with X in `0-9a-z` (see terrain-textures.md) |
| 97 | u8 | object bank (0, 2, 6, 7 used; the editor offers 0, 2-7; see objects.md) |
| 98 | u8 | level flags |
| 99 | u8 | AI script number of blue (0 in every level) |
| 100 | u16[256] | markers |
| 612 | u16 | start position |
| 614 | u16 | start angle (2048ths of a turn, e.g. 1144) |

[files]:
- **Masks**: bit N = model N, confirmed by the campaign. A spell or building discovered in a level (discovery
  things) is missing from that level's mask and present in later levels (e.g. level 4 discovers spells 3 and 17;
  level 5 has 3, level 7 has 17). Bits that are not models are always set: bit 0 and 22-31 for spells, bit 0 and
  17-31 for buildings, bit 0 and 5-15 for vehicles. So:
  - vehicles 0xffe1 = none, 0xffe7 = both boats, 0xffeb = boat + airship 1, 0xffff = all;
  - levels with boat or airship huts allow the matching vehicles.
- **Allies**: own bit set by default. Level 14 `[1, e, e, e]` = red, yellow and green allied against blue. Level
  19 `[5, 2, 5, 8]` = blue and yellow. Multiplayer levels: all 0.
- **Level flags**:

  | Bit | Meaning | Seen in |
  |---|---|---|
  | 0x01 | fog of war | levels 9 and 18 |
  | 0x02 | "shaman omni", shown as "God mode" in the editor | level 25 |
  | 0x04, 0x08 | force 640x480, level edit [editor] | |
  | 0x10 | no guest spells | 2111, 2120, 2128, 2133 |
  | 0x20 | no reincarnation time [editor] | |

- **Markers and start position**: one byte per axis, **low byte = x * 2, high byte = z * 2** (both bytes always
  even). An unused marker is 0 (cell 0, 0). Read lo = x, markers lie a median 3 cells from the nearest thing,
  against 6.4 swapped. Level 3's start position `0xa62a` = (21, 83) is exactly the blue shaman's cell. Markers are
  what AI scripts refer to by index.

## `.ver` (68 bytes)
`i32 VersionNum`, `char CreatedBy[32]`, `char CreatedOn[28]`, `i32 CheckSum`. [files]:
- VersionNum is 11 in all 41 levels; the editor writes 11 too.
- CreatedBy is the author, e.g. "acullum", "driley", sometimes empty.
- CreatedOn is a date, e.g. "Sep 21 1998 17:09:26".
- **CheckSum is 0 in every level**, so the game can't require it.

## v3 (PopRe extension, not read by the original game) [editor]
The editor's own format, starting with the magic `LEVL3`, with no `.hdr` or `.ver`:

| Off | Size | Content |
|---|---|---|
| 0 | 5 | `"LEVL3"` |
| 5 | 953 | the 616-byte header, then `u8 ComputerPlayerIndex[4]` (blue, red, yellow, green), `u8 Version` (1), `u32 MaxAltPoints` (16384), `u32 MaxNumObjects` (thing count), `u32 MaxNumPlayers` (4), `char Script2[10][32]` |
| 958 | 32 768 | heights |
| 33 726 | 16 384 | NoAccessSquares (no LandBlocks / LandOrients) |
| 50 110 | 67 | start info, sunlight |
| 50 177 | 55 x count | things, up to 66 535 |

## Writing levels (for a future editor)
Write v2 and:
- pack things from slot 0 and zero-fill the rest;
- renumber trigger ThingIdxs (1-based);
- put buildings on corners and everything else on cell centres;
- write `.hdr` and `.ver` (CheckSum 0).

LandBlocks, LandOrients, start info and access info can be zero (the editor does it); keep the sunlight bytes.

## Code
`crates/pop3-format/src/level.rs` and `header.rs`, example: `just level-info path/to/levl2005.dat [all]`.
- Things are read from 81 987 with their 0-based `Thing::slot`; `Level::slot` resolves a 1-based trigger target.
- Start info, sunlight and access info: `Level::start_info`, `sunlight`, `access`.
- `Thing::angle()`: buildings `i32@7`, scenery `i16@10` (trees keep it, `Tree::angle`); `facing()` rounds it to
  eighths.
- `Thing::data()`: discovery, trigger and effect target unions.
- `LevelHeader`: masks, tribes, AI scripts in owner order (blue from 99), allies, theme, bank, flags, markers,
  start position and angle. `LevelVersion` for `.ver`.

Not used by the game yet: header masks, flags, allies, bank, start camera, discoveries and triggers (TODO.md).
