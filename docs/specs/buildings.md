# Buildings

## Simulation (done: `game_core::building`, view only)
`GameMap::buildings`: `Building { kind, owner, x, z, facing }` from an original level's things of kind 2
(`buildings_from_level`): model = type, owner = tribe (255 = neutral), position in world units, facing in eighths
of a turn (thing byte 8: only 0, 2, 4, 6 seen, `Thing::facing`). Types by model, as numbered in the original's
scripts: 1-3 villager hut size 1-3, 4 drum tower, 5 temple (preachers), 6 spy, 7 warrior, 8 firewarrior training,
9 reconversion, 10 wall, 11 gate, 13-14 boat hut, 15-16 airship hut, 17 guard post, 18 vault of knowledge,
19 prison; others `Other(model)`. Level 19 (two tribes' villages of huts 3 and drum towers, a temple, training
huts, a boat hut) and level 1 (a neutral model 18) fit it. When the map loads, each building levels its
footprint (`BuildingKind::footprint`: a rectangle measured from its original object, half size and shift in its own
frame, world units; the boat hut's jetty left out), turned with its facing (`Building::flatten` =
`Heightmap::level_rect`): height points inside take their average height (at least 64, so never sea nor shore),
those within a cell around it are pulled halfway, except sea, which stays sea (a boat hut keeps its water on the
jetty side). Then the reincarnation sites level their disc (`level_around`, 3/4 cells, at least 32). Level 4 has a
hut on a sea-level shore: it now stands on sand; level 19's boat hut keeps the inlet its jetty points into.
No construction, health, people inside or footprint yet. Sandbox > Buildings (`GameMap::sandbox_buildings`): one of every model 1-19 for the player, a few red ones.

## On screen (client, `buildings.rs`)
Centred on the terrain (`Grounded`) and leaning with it (`grounded::Tilted`: its up follows the drawn ground
normal over 1 cell either side, slopes and the planet's curve, so no side sinks in), turned by the facing
(`facing_yaw`).
- With the original files: the original object (`building_object`: huts `catalog::villager_hut` style 0, drum
  tower, temple = prayer hut, spy/warrior/firewarrior training, boat and airship huts in the owner's colours,
  neutral ones blue; vault = pyramid of knowledge, prison), textured from the level's theme atlas (theme 0 on maps
  without one), cut-out texels see-through.
- Otherwise, and for types without an identified object (reconversion, wall, gate, guard post, unknown): a box
  1.6 x 0.8 x 1.6 cells in the tribe colour (grey when neutral) with the building's name over it on screen.
