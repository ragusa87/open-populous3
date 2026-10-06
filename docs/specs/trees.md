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

## On screen (client, `nature.rs`)
Drawn as 3D models standing on the terrain (`Grounded`, trunk footprint 0.15 cell), like the original's 3D trees
(bank 0 objects 60-71): the CC0 Quaternius Stylized Nature MegaKit (`assets/models/nature`, variants 0-4 common
trees, 5-9 pines), turned by a fixed per-tree angle (`tree_yaw`), scaled with the size (`tree_scale`: 40% at
size 1 to 1.6 cells tall at size 4), hidden at size 0. Models load through Bevy's asset server from the
repository's `assets/` (`dev::asset_plugin`). The original tree objects need a theme atlas: to use once trees
come from original levels.
Resting the cursor on a visible tree for 1.5 s (`HOVER_SECS`) shows a tooltip by the cursor: "Tree: 3/4 wood"
(pieces left / most it can hold). The tree under the cursor is the one whose screen box (trunk base to top,
30% of its height either side) holds it, the nearest one when several overlap (`tree_at`). Not over the panel,
nor while a spell is aimed.
