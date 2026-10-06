# Trees

## Simulation (done: `game_core::tree`)
- `Tree { x, z, variant, size }` in `GameMap::trees`: world units at a cell centre, a model variant (0-11, the
  original tree objects; the client picks the model), a size 0-4 = the pieces of wood it can still give.
- Growth: below size 4, one size every `GROW_TICKS` (600 ticks, a minute), ticked by `GameMap::tick`. A tree at
  size 0 stays in place, invisible, and grows back. `Tree::cut` takes one piece (size - 1, growth restarts).
- Placement (`tree::scatter`, deterministic from a seed): groves around random centres, 3-8 trees within 3 cells,
  only on ground units can walk (no sea, no cliff), never within 7 cells of a reincarnation site, one per cell,
  starting at size 1-4. Generated maps: 60 groves (seed = map seed); sandboxes: 150 attempts (most land in the
  sea around their small islands). Original levels: none until the level things are decoded.
- Trees do not block walking yet.

## On screen (client, `nature.rs`)
Drawn as 3D models standing on the terrain (`Grounded`, trunk footprint 0.15 cell), turned by a fixed per-tree
angle (`tree_yaw`), scaled with the size (`size_factor`: 40% at size 1 to full at size 4), hidden at size 0.
- With the original files: the original tree objects (bank 0, `60 + variant`, 0.9-1.8 cells tall), textured from
  the map's theme atlas (theme 0 on maps without one: generated, sandboxes), drawn at their own size.
- Otherwise: the CC0 Quaternius Stylized Nature MegaKit (`assets/models/nature`, variant modulo 10: 0-4 common
  trees, 5-9 pines), 1.6 cells tall at full size. Models load through Bevy's asset server from the repository's
  `assets/` (`dev::asset_plugin`).
Resting the cursor on a visible tree for 1.5 s (`HOVER_SECS`) shows a tooltip by the cursor: "Tree: 3/4 wood"
(pieces left / most it can hold). The tree under the cursor is the one whose screen box (trunk base to top,
30% of its height either side) holds it, the nearest one when several overlap (`tree_at`). Not over the panel,
nor while a spell is aimed.
