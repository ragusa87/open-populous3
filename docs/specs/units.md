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
| Walking { to } | `Order::MoveTo` | 64 units/tick straight to the target, shortest way around the torus; stops at open sea (cell with 4 water corners) |
| Praying | `Order::Pray` | until another order; heals |
| Casting { left } | `Order::Cast`, any spell cast | 12-tick jump, then Idle |
| Drowning | ground under her becomes open sea | -4 HP per tick, no orders; back to Idle if land returns |
| Dying { left } | health reaches 0 | 8 ticks |
| Dead { left } | after dying | 30 ticks, then reincarnates at her site at full health; the site levels its ground again |

Health: 100. Orders are ignored while drowning, dying or dead.

## Shaman on screen (client, `units/`)
- `SimClock` runs `GameMap::tick` at a fixed 10 Hz; positions glide between the last two ticks.
- Each unit is a `Grounded` sprite quad (1 px = 1/30 cell, feet at the anchor) turned to face the camera,
  with a health bar over the head (green -> yellow -> red, hidden once dead).
- Pose from the action: Idle, Walk, Pray, Cast (jump), Drown (tumbling), Fall (dying, then lies still while dead).
  Timed actions (cast, dying) play once in step with the simulation, others loop.
- View direction: `facing * 45deg - camera yaw`, rounded to the 8 drawn directions (0 front, 2 screen right, 4 back).
- Art: with the original files, the shaman animations of `VSTART/VFRA/VELE` + `HSPR0-0.DAT` (see animations.md),
  per tribe; otherwise (or `--no-original`) `units/procedural.rs` draws a ~34 px pixel-art figure in the tribe colour
  (feather headdress, staff) for every pose and direction (front / side / back, left ones mirrored).
- Orders (player 0): right click on the ground walks there (`grounded::pick_ground`), P prays, X stops,
  C (cast selected spell) makes her jump, Space or a click on the panel preview looks at her.
- Dev: `SHAMAN=walk|pray|cast|drown [SHOT_FRAME=n] just shot out.png` orders her at start to check a pose.
- Pathfinding: A* or flow fields on the 128² grid with modulo neighbours; blocked by water and slope
  above a threshold. Recompute only regions touched by a `DirtyRect`.
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
