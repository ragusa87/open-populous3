# Trees

## Simulation (done: `game_core::tree`)
- `Tree { x, z, variant, size }` in `GameMap::trees`: world units at a cell centre, a tree type (0-17, see
  below; the client picks the model), a size 0-4 = the pieces of wood it can still give.
- Original levels: their scenery things of models 1-6 (`Thing::tree_type`, type = model - 1), at full size.
  Level 19 has 50 of model 1 and 14 of model 2: cone pines and weeping trees, as in the game.
  Each tree's angle is the scenery `i16@10` (quarter turns in the files, see level-format.md). It is not read
  yet: `Thing::facing` reads byte 8, which is always 0 for scenery.
- Growth: below size 4, one size every `GROW_TICKS` (600 ticks, a minute), ticked by `GameMap::tick`. A tree at
  size 0 stays in place, invisible, and grows back. `Tree::cut` takes one piece (size - 1, growth restarts).
- Placement (`tree::scatter`, deterministic from a seed): groves around random centres, 3-8 trees within 3 cells,
  only on ground units can walk (no sea, no cliff), never within 7 cells of a reincarnation site, one per cell,
  starting at size 1-4, of any type. Generated maps: 60 groves (seed = map seed); sandboxes: 150 attempts (most
  land in the sea around their small islands).
- Trees do not block walking yet.

## Wood on the ground (`game_core::wood`, client `wood.rs`)
`GameMap::wood`: `WoodPiece { x, z }` (world units), one piece of wood each, dropped by braves or by a dismantled
building, to be picked up by constructions (see buildings.md). None in original levels (no level has the editor's
"wood pile" scenery, model 11, nor fire 10, pillar 12 or rock 13); Sandbox > Buildings has a few between the site
and the buildings.
Drawn like a unit: a camera-facing sprite standing on the ground (`Grounded`), pulled 0.3 cell towards the camera:
the original pile of logs (`hfx0-0.dat` 23, see sprites.md, Scale2x-upscaled as unit sprites) when allowed, else the
bundled `assets/sprites/wood_pile.png`: three logs with light cut ends in the same 17 x 11 pixel size, upscaled
the same way.

## On screen (client, `nature.rs`)
Drawn as 3D models standing on the terrain (`Grounded`, trunk footprint 0.15 cell), turned by a fixed per-tree
angle (`tree_yaw`), scaled with the size (`size_factor`: 40% at size 1 to full at size 4), hidden at size 0.
- With the original files: the original tree objects (`catalog::tree_object`: types 0-5 = scenery models 1-6 =
  objects 13-18, types 6-17 = objects 60-71; 0.9-1.8 cells tall). The tree look actually depends on the level's
  object bank: in banks 2-7, trees 4-6 are copies of 1-3, and bank 3/4/5/6 levels use bank 0's 60-62 / 63-65 /
  66-68 / 69-71 (see objects.md). Not done yet: bank 6 levels (3, 5, 16, 22, 2120) show the wrong trees,
  and level 2127 (bank 2) draws its 4 model-4 trees as bank 0's pine 16 instead of a cone pine. Textured from the map's theme atlas (theme 0 on maps without one: generated, sandboxes), drawn at their own size.
- Otherwise: the CC0 Quaternius Stylized Nature MegaKit (`assets/models/nature`, type modulo 10: 0-4 common
  trees, 5-9 pines), 1.6 cells tall at full size. Models load through Bevy's asset server from the repository's
  `assets/` (`dev::asset_plugin`).
Resting the cursor on a visible tree for 1.5 s (`HOVER_SECS`), or right-clicking it, shows a tooltip by the
cursor: "Tree: 3/4 wood" (pieces left / most it can hold), until the cursor leaves the tree (`HoveredTree`). A right
click on a tree keeps the unit selection (elsewhere it clears it). The tree under the cursor is the one whose screen box (trunk base to top,
30% of its height either side) holds it, the nearest one when several overlap (`tree_at`). Not over the panel,
nor while a spell is aimed.
