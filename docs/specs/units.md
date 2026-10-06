# Units (to do)

- `game_core::unit::Unit`: id, owner, kind, `u16` x/z in world units (512 per cell), wrapping at 65536.
- Movement in fixed point per tick; no floats in simulation state.
- Pathfinding: A* or flow fields on the 128² grid with modulo neighbours; blocked by water and slope
  above a threshold. Recompute only regions touched by a `DirtyRect`.
- Spawn from level things (`kind` 1 = person, model = brave/warrior/...) once the record is decoded.
- Rendering: Bevy entities mirroring sim units, positioned on the curved surface using the same
  `drop_at` formula as the terrain.

## Reincarnation site (done: data + rendering)
- `game_core::site::ReincarnationSite { owner, x, z }`, stored in `GameMap::sites` (one per tribe, sorted by owner).
- Original levels: built at the tribe's shaman thing (person model 7). Tribes without a shaman get no site
  (some campaign AI tribes; levl2025 has none for the player).
- Generated maps: tribe 0 on low inland ground, tribe 1 on the land cell farthest away on the torus.
- Fixed and indestructible: no `Command` moves or removes it. `spawn_point()` is where the shaman
  appears at start and after death (to wire once units are simulated).
- Rendered as a ring of stones with a tribe-coloured totem; the camera starts on the player's (tribe 0) site.
  Stones are the original capped pillar (object 76, bank 0, textured from the level theme's `bl320` atlas,
  see objects.md) when the original files are present; plain generated blocks otherwise or with `--no-original`.

## Standing on the ground (client)
Anything placed on the map (site stones, buildings, trees, units) is made of `grounded::Grounded`
parts: each part has its own cell position and footprint, and is set on the terrain exactly as the
mesh draws it (same triangle split, same `drop_at` curve), resting on its lowest footprint corner so
it never floats. Multi-part objects (a building's walls/corners) follow slopes instead of staying flat.
