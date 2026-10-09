# Terrain

## Model
- `Heightmap` (game-core): `size x size` `u16`, 128 for original maps, 0 = water, max 1024.
- Wraps on both axes (torus): every access goes through `rem_euclid`. No edges anywhere.
- Integer only, so lockstep peers stay identical. `sample()` (float bilinear) is render-only.

## Planet illusion
The world is flat; vertices are bent down with distance from the camera focus:
`y = h * height_scale - k * (dx² + dz²)` (`terrain_mesh::build`, k = 0.012, scale = 1/384, max height 1024 = 2.7 cells, slightly exaggerated vs the original 2 cells).
The mesh is a (2R+1)² grid (R = 64, clipped to a disc) centred on the focus; the focus is the render
origin, so wrapping is free. Aerial view = the same mesh seen from far away -> a globe. The grid is drawn with the
game's z mirrored (architecture.md "Handedness", `game_frame`): map cell (x, z) at render (dx, -dz).

Current implementation rebuilds the mesh on CPU when focus or terrain changes (~16k vertices, cheap).
To scale: static grid + vertex shader sampling an R16 height texture with wrapped UVs.

## Brushes
`apply_brush(center, radius, op)` with integer falloff 0..256, returns a `DirtyRect`.
`raise`, `flatten`, `land_bridge` (shortest path around the torus) are built on it.
After an edit, recompute only the dirty rect: normals, walkability, building validity (to do).
