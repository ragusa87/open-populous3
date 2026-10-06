# Units

- `game_core::unit::Unit`: id, owner, kind, `u16` x/z in world units (512 per cell), wrapping at 65536
  (plain `u16` wrapping arithmetic walks the torus), `facing` in eighths of a turn (0 = +z, 2 = +x), health, action.
- Movement in fixed point per tick; no floats in simulation state.

## Shaman (done: simulation)
`GameMap::units` holds one shaman per reincarnation site, spawned at `spawn_point()` when the map loads.
`GameMap::tick()` advances every unit (10 ticks per second, `TICKS_PER_SECOND`); orders arrive as
`Command::Order { player, order }` through `GameMap::apply` (lockstep-safe). Casting a spell
(`Command::Cast`) also makes the caster's shaman jump.

| Action | Entered by | Behaviour |
|---|---|---|
| Idle | default, arrival, Stop | heals 1 HP every 5 ticks |
| Walking { to } | `Order::MoveTo` | follows a path (see Pathfinding) at 64 units/tick on flat ground (slope over the next step: `slope_speed`, 1/256 factor `256 - grade*k/100` with k = 192 uphill and 128 downhill, clamped to 32..384, grade = height per cell: ~78% speed up the sandbox ramp, quarter speed up the steep hill, 1.5x down it, ground height bilinear `Heightmap::height_at`); target unreachable: the shaman stays Idle, other units are Stranded |
| Stranded { to } | target unreachable (not the shaman) | does not move, arms up, -1 HP every 3 ticks until the terrain opens a path (walks again) or it dies |
| Praying | `Order::Pray` | until another order; heals |
| Casting { left } | `Order::Cast`, any spell cast | 12-tick jump, then Idle (Teleport: then at the target, Landing) |
| Landing { left } | arriving from a teleport | 6 ticks, then Idle; drawn in the idle pose floating 0.2 cell up and settling down (`landing_lift`, eases out); a puff of dust at touchdown (`units/dust.rs`) |
| Drowning | ground under her becomes open sea | -4 HP per tick, no orders; back to Idle if land returns |
| Dying { left } | health reaches 0 | 8 ticks |
| Dead { left } | after dying | 30 ticks, then reincarnates at her site at full health; the site levels its ground again |

Health: 100. Orders are ignored while drowning, dying or dead.

## Unit kinds (simulation: walking only)
`UnitKind::ALL`: Shaman, Brave, Warrior, Preacher, Spy, Firewarrior, built with `Unit::new(id, owner, kind, pos)`.
Placeholder balance until combat (`max_health`, flat-ground `speed` in world units per tick):

| Kind | Health | Speed |
|---|---|---|
| Shaman | 100 | 64 |
| Brave | 60 | 64 |
| Warrior | 120 | 56 |
| Preacher | 70 | 56 |
| Spy | 60 | 72 |
| Firewarrior | 80 | 60 |

Every kind walks, prays, heals, drowns and dies like the shaman; only the shaman reincarnates (the others stay
`Dead`, lying where they fell). `Command::Order` and spells go to the player's shaman (`GameMap::shaman_of`),
`Command::OrderUnit` to any unit. Sandbox > Units (`GameMap::sandbox_units`): flat island, the player's site and
shaman, three of each other kind in columns to the west, one of each for tribe 1 (red) in a row to the east (all in
view of the starting camera), a pond to the north.

## Pathfinding (done: `game_core::path`)
- `path::Mobility`: Walk (land, not open sea = cell with 4 water corners, not a cliff = a cell edge
  rising more than `MAX_CLIMB` 300; ~5.5% of the original levels' land), Sail (open sea only, boats),
  Fly (anywhere, balloons). Every person walks. A* on the 128² torus, 8 neighbours, no diagonal past an
  impassable side cell, ties broken by cell index (deterministic).
- Fastest, not shortest: a step costs its walking time (100 straight, 141 diagonal on flat ground, divided
  by `slope_factor` of the rise between the two cell centres, the same rule as walking), so a walker goes
  around a hill or along its flank when climbing is slower (sandbox hill: around beats over). Vehicles
  ignore slopes. Heuristic: distance at the top downhill speed (never overestimates).
- The cell path is straightened into legs (start, turning points, exact target): a leg is kept when every
  cell it crosses is passable (exact grid traversal, both side cells where it passes through a corner),
  it is at most 24 cells long, and walking it (slope sampled every 64 units) is no slower than the cells
  it skips (+5%).
- `Heightmap::revision` is bumped on every write; a walking (or stranded) unit replans on the next tick
  when it changed (spells, editor, site levelling). A step that would still end in the sea replans cell
  by cell through cell centres.
- The start cell may be impassable (ground raised into a cliff under her): she can step off it.
- `path::nearest_reachable`: the cell a mobility reaches closest to a goal (walkers boarding a boat,
  boats unloading at a shore; vehicles to come).

## Shaman on screen (client, `units/`)
- `SimClock` runs `GameMap::tick` at a fixed 10 Hz; positions glide between the last two ticks (moves over a cell in one tick, teleport or reincarnation, are not glided).
- Each unit is a `Grounded` sprite quad (1 px = 1/88 cell: standing ~0.39 cell, half a site stone; feet at the
  anchor) turned to face the camera, uploaded Scale2x-upscaled x4 and filtered linearly (no blocky pixels),
  with a health bar over the head (green -> yellow -> red) shown only while the unit is selected and alive.
  Feet on the ground right under the unit (`Grounded` with no footprint). Sprite and bar are drawn pulled
  0.6 cell towards the camera along the eye-feet line and shrunk to match (`toward_eye`): same picture on
  screen, but slopes and bumps around the feet no longer cut the legs; real hills in front still hide her.
  The original has no health bar: low-health units get a spinning star/crown over the head (to do). The panel
  preview always shows the shaman's health.
- Dust (`units/dust.rs`, cosmetic, real-time): when a unit stops landing, 10 soft generated motes spread 0.4 cell
  around its feet, rise 0.1, grow and fade over 0.8 s, facing the camera and pulled towards it like the sprites.
- Pose from the action: Idle, Walk, Pray (kneeling, original anim 93), Cast (jump), Drown (tumbling), Fall (dying, then lies still while dead).
  Timed actions (cast, dying) play once in step with the simulation, others loop.
- View direction: `facing * 45deg - camera yaw`, rounded to the 8 drawn directions (0 front, 2 screen right, 4 back).
- Art: with the original files, the shaman animations of `VSTART/VFRA/VELE` + `HSPR0-0.DAT` (see animations.md),
  per tribe; otherwise (or `--no-original`) the open-source witch (Quaternius, CC0) rendered into
  `assets/units/shaman/*.png` sheets, loaded by `units/sheets.rs` (see unit-art.md); `units/procedural.rs` can still draw a ~34 px pixel-art shaman in the tribe colour
  (feather headdress, staff) for every pose and direction (front / side / back, left ones mirrored). The other kinds
  are always generated (`UnitSprites`, per kind and tribe), told apart by headgear, held item and clothes: brave
  (bare chest, hair tuft, empty hands), warrior (horned helmet, club), preacher (pointed hood, long robe, book), spy
  (dark cloak and cowl, dagger), firewarrior (red cone hat, flame in hand).
- Selection (`units/selection.rs`, player 0's living units): left click on a unit selects it, Ctrl+click adds or
  removes it; a left drag (over 6 px) draws a whitish box and selects the units whose middle is inside (Ctrl adds);
  right click clears. On the map the shaman is selected like any unit (click, Ctrl, box); clicking her panel
  preview selects her alone (and looks at her). Selecting a vehicle or building will not select the people inside.
  The cursor shows the selected count when more than one. Dead units leave the selection.
- Orders go to each selected unit as `Command::OrderUnit`: left click on the ground walks there
  (`grounded::pick_ground`), P prays, X stops. C (cast selected spell) makes the player's shaman jump,
  Space looks at her.
- Dev: `SHAMAN=walk|pray|cast|drown|teleport [SHOT_FRAME=n] just shot out.png` orders her at start to check a pose.
- Spawn from level things (`kind` 1 = person, model = brave/warrior/...) once the record is decoded.

## Reincarnation site (done: data + rendering)
- `game_core::site::ReincarnationSite { owner, x, z }`, stored in `GameMap::sites` (one per tribe, sorted by owner).
- Original levels: built at the tribe's shaman thing (person model 7). Tribes without a shaman get no site
  (some campaign AI tribes; levl2025 has none for the player).
- Generated maps: tribe 0 on low inland ground, tribe 1 on the land cell farthest away on the torus.
- Spawn ground: when a shaman spawns (map load = first spawn; respawns once units exist),
  `flatten_for_spawn` levels the terrain: height points within 3 cells of the centre take their
  average height (at least `MIN_SPAWN_HEIGHT` = 32, so a flooded site becomes land again), a ring out
  to 4 cells is blended halfway. Integer-only, applied in owner order (deterministic).
- Fixed and indestructible: no `Command` moves or removes it. `spawn_point()` is where the shaman
  appears at start and after death.
- Rendered as a ring of 8 stones; the camera starts on the player's (tribe 0) site.
  With the original files: the reincarnation stone (object 30) in the owner's tribe colour, textured from the
  level theme's `bl320` atlas (see objects.md). Without them or with `--no-original`: plain blocks
  tinted with the tribe colour (no totem: the shaman stands at the centre). Stones face the centre (glyph side inward, as in the game); count and radius are ours.

## Standing on the ground (client)
Anything placed on the map (site stones, buildings, trees, units) is made of `grounded::Grounded`
parts: each part has its own cell position and footprint, and is set on the terrain exactly as the
mesh draws it (same triangle split, same `drop_at` curve), resting on its lowest footprint corner so
it never floats. Multi-part objects (a building's walls/corners) follow slopes instead of staying flat.
