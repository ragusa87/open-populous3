# Trees

## Simulation (done: `game_core::tree`)
- `Tree { x, z, variant, size }` in `GameMap::trees`: world units at a cell centre, a model variant (0-9, the
  client picks the model), a size 0-4 = the pieces of wood it can still give.
- Growth: below size 4, one size every `GROW_TICKS` (600 ticks, a minute), ticked by `GameMap::tick`. A tree at
  size 0 stays in place, invisible, and grows back. `Tree::cut` takes one piece (size - 1, growth restarts).
- Placement (`tree::scatter`, deterministic from a seed): groves around random centres, 3-8 trees within 3 cells,
  only on ground units can walk (no sea, no cliff), never within 7 cells of a reincarnation site, one per cell,
  starting at size 1-4. Generated maps: 60 groves (seed = map seed); sandboxes: 150 attempts (most land in the
  sea around their small islands). Original levels: none until the level things are decoded.
- Trees do not block walking yet.

## On screen (client)
Drawn as 3D models standing on the terrain, like the original's 3D trees (bank 0 objects 60-71): the original
models when the original files are allowed, else the CC0 Quaternius Stylized Nature MegaKit. Scaled with the
tree's size, hidden at size 0.
