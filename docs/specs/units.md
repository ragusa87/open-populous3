# Units (to do)

- `game_core::unit::Unit`: id, owner, kind, `u16` x/z in world units (512 per cell), wrapping at 65536.
- Movement in fixed point per tick; no floats in simulation state.
- Pathfinding: A* or flow fields on the 128² grid with modulo neighbours; blocked by water and slope
  above a threshold. Recompute only regions touched by a `DirtyRect`.
- Spawn from level things (`kind` 1 = person, model = brave/warrior/...) once the record is decoded.
- Rendering: Bevy entities mirroring sim units, positioned on the curved surface using the same
  `drop_at` formula as the terrain.
