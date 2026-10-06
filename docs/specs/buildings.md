# Buildings

## Simulation (done: `game_core::building`, view only)
`GameMap::buildings`: `Building { kind, owner, x, z, facing }` from an original level's things of kind 2
(`buildings_from_level`): model = type, owner = tribe (255 = neutral), position in world units, facing in eighths
of a turn (thing byte 8: only 0, 2, 4, 6 seen, `Thing::facing`). Types by model, as numbered in the original's
scripts: 1-3 villager hut size 1-3, 4 drum tower, 5 temple (preachers), 6 spy, 7 warrior, 8 firewarrior training,
9 reconversion, 10 wall, 11 gate, 13-14 boat hut, 15-16 airship hut, 17 guard post, 18 vault of knowledge,
19 prison; others `Other(model)`. Level 19 (two tribes' villages of huts 3 and drum towers, a temple, training
huts, a boat hut) and level 1 (a neutral model 18) fit it. When the map loads, each building levels its
ground (`Building::flatten` = `Heightmap::level_around`: height points within 2 cells take their average height,
at least 64 so never sea nor shore, the ring out to 3 cells is pulled halfway), before the reincarnation sites
level theirs (same function, 3/4 cells, at least 32). Level 4 has a hut on a sea-level shore: it now stands on
sand. No construction, health, people inside or footprint yet. Sandbox > Buildings (`GameMap::sandbox_buildings`): one of every model 1-19 for the player, a few red ones.

## On screen (client, `buildings.rs`)
Standing on the terrain (`Grounded`, footprint half 0.8 cell), turned by the facing (`facing_yaw`).
- With the original files: the original object (`building_object`: huts `catalog::villager_hut` style 0, drum
  tower, temple = prayer hut, spy/warrior/firewarrior training, boat and airship huts in the owner's colours,
  neutral ones blue; vault = pyramid of knowledge, prison), textured from the level's theme atlas (theme 0 on maps
  without one), cut-out texels see-through.
- Otherwise, and for types without an identified object (reconversion, wall, gate, guard post, unknown): a box
  1.6 x 0.8 x 1.6 cells in the tribe colour (grey when neutral) with the building's name over it on screen.
