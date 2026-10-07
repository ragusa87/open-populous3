# Original level format (Populous: The Beginning)

Files in `levels/`: `levlNNNN.dat` (map + objects), `levlNNNN.hdr` (616 B, settings + name),
`levlNNNN.inf`, `levlNNNN.ver` (68 B), plus `cpscr*`/`cpatr*` (AI scripts/attributes, not parsed).
Layout ("v2") from PopResourceEditor's `Level.h` (Toksisitee, MIT, `140e389`), itself taken from the world
editor ([PopRe/Pop-World-Editor](https://github.com/PopRe/Pop-World-Editor), `pop.h`); verified on the 41
shipped levels. Fields marked "unverified" are names only.

## `.dat` (192 137 bytes, little endian)

| Offset | Size | Content |
|---|---|---|
| 0 | 128*128*2 | `u16 GroundHeight`, row-major `z*128+x`. 0 = sea, observed max 1024 |
| 32 768 | 16 384 | `u8 LandBlocks` (was "layer 2", meaning unverified) |
| 49 152 | 16 384 | `u8 LandOrients` (was "layer 3", meaning unverified) |
| 65 536 | 16 384 | `u8 NoAccessSquares` (all 0 in level 1) |
| 81 920 | 4 x 16 | player start info: `i16 StartPosX, i16 StartPosY, i32 Future[3]` |
| 81 984 | 3 | sunlight: `u8 ShadeStart, ShadeRange, Inclination` |
| 81 987 | 2000 x 55 | things, `type == 0` means empty slot |
| 191 987 | 50 x 3 | access info: `u8 Model, Type, Rights` |

Observed: player start info is a placeholder in every level ((1,1), (2,2), (3,3), (4,4), futures 0);
sunlight is always ShadeStart 28, ShadeRange 15, Inclination 32 or 64 (probably the terrain lighting
input, see terrain-textures.md); access info is all zero.

### Thing record (55 bytes)

| Off | Type | Field |
|---|---|---|
| 0 | u8 | model (subtype, e.g. tree variant) |
| 1 | u8 | type: 1 person, 2 building, 3 creature, 4 vehicle, 5 scenery, 6 general, 7 effect |
| 2 | i8 | owner tribe (-1 = 255 = neutral) |
| 3 | u16 | x, world units (512 per cell, odd multiples of 256 = cell centre) |
| 5 | u16 | z |
| 7 | 48 B | union, by type (below) |

Angles are in 2048ths of a turn (seen: 0, 512, 1024, 1536).

- Building (2): `i32 Angle` at 7.
- Scenery (5): `u8 PortalStatus@7, PortalLevel@8, PortalType@9, i16 Angle@10, u8 UserId@12,
  i16 IslandAlt@13, u8 IslandNum@15, BridgeNum@16`. Angle 0 / 512 / 1024 / 1536 seen 2899 / 357 / 764 /
  536 times. Portal and island fields
  always 0, UserId 1-3 a few times.
- General (6), model 2 (discovery): `u8 DiscoveryType@7, DiscoveryModel@8, AvailabilityType@9,
  TriggerType@10, i32 ManaAmt@11` (e.g. type 11, model 8..16, availability 3).
- General (6), model 6 (trigger): `u8 TriggerType@7, CellRadius@8, RandomValue@9, i8 NumOccurences@10,
  u16 TriggerCount@11, u16 ThingIdxs[10]@13, i16 PrayTime@33, u8 StartInactive@35,
  CreatePlayerOwned@36, i16 InactiveTime@37`.
- General (6), model 9: all zero.
- Others: not decoded (kept raw in `Thing::raw`).

Types/models seen in the shipped levels: person 1-7, building 1-8, 13, 15, 18, 19, vehicle 1 and 3,
scenery 1-9 (1-6 trees, 7-8 plants?), general 2, 6, 9, effect (many models; 81, 85, 23 most common).
Person models: 1 wild, 2 brave, 3 warrior, 4 preacher, 5 spy, 6 firewarrior, 7 shaman.
Each tribe has exactly one shaman thing; its position is the tribe's reincarnation site
(the original game builds the site there, it is not a separate thing in the file).

Axis order (`x` vs `z`) is unverified; a mirrored map would still look right.

## `.hdr` (616 bytes)

| Off | Type | Field |
|---|---|---|
| 0 | u32 | SpellsAvailable (bitmask) |
| 4 | u32 | BuildingsAvailable |
| 8 | u32 | BuildingsAvailableLevel |
| 12 | u32 | BuildingsAvailableOnce |
| 16 | u32 | SpellsAvailableLevel (aka SpellsNotCharging) |
| 20 | u8[32] | SpellsAvailableOnce |
| 52 | u16 | VehiclesAvailable |
| 54 | u8 | TrainingManaOff |
| 55 | u8 | Flags |
| 56 | char[32] | level name, NUL-terminated ("Level 1") |
| 88 | u8 | number of tribes (2-4) |
| 89 | u8[3] | computer player script ids: mostly match `cpscrNNN.dat` (10, 74, 122...); 85, 100, 130 have no file; unused slots hold junk (4, 5) |
| 92 | u8[4] | default allies, bitmasks (default 1, 2, 4, 8) |
| 96 | u8 | landscape theme, 0-35 -> files `*0-X.dat` with X in `0-9a-z` (see terrain-textures.md) |
| 97 | u8 | object bank (0, 2, 6, 7 used, see objects.md) |
| 98 | u8 | level flags (0x1, 0x2, 0x10 seen) |
| 99 | u8 | pad, always 0 |
| 100 | u16[256] | markers |
| 612 | u16 | start position |
| 614 | u16 | start angle (2048ths of a turn, e.g. 1144) |

Bit meanings of the masks and flags are not mapped yet. Markers and start position look like packed cell
positions: both bytes are always even (e.g. `0x1208`, `0xa62a`), i.e. one byte per axis = cell * 2.
Which byte is x is unverified.

## `.ver` (68 bytes)
`i32 VersionNum` (11 in all 41 levels), `char CreatedBy[32]` (author, e.g. "acullum", "driley", sometimes
empty), `char CreatedOn[28]` (date, e.g. "Sep 21 1998 17:09:26"), `i32 CheckSum`.

## Code
`crates/pop3-format/src/level.rs`, example: `just level-info path/to/levl2005.dat`.
Known gaps (tracked in TODO.md, "Things and level data"):
- Things are read from 82 042, one record too late: thing 0 is non-empty in all 41 levels and is dropped
  (levl2001: a tribe-1 hut; levl2012: a wild man); the extra record read at the end is the zeroed access info.
- `Thing::facing()` reads byte 8 % 8: for buildings it matches the quarter turns only because it is the high
  byte of the `i32` angle; for scenery byte 8 is always 0, so tree turns are lost.
- `LevelHeader` decodes only the name and the theme.
