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

## Construction (planned)
Only these are built and taken apart by braves: villager hut (placed at size 1, it grows later), drum tower, the
training huts (warrior, firewarrior, temple = preachers, spy), boat hut and airship hut. The reincarnation site,
prison, vault/pyramid of knowledge and totems can never be built nor dismantled.

### Build tab (HUD)
Like the Spells tab: one tile per buildable kind, per tribe `Availability` (Hidden / Discoverable "?" / Available),
taken from the level (`.hdr`) and unlocked by triggers. Clicking an available tile picks a blueprint.

### Blueprint
- Follows the cursor as a white mark drawn on the ground, the building's footprint draped over the terrain
  heights, with an arrow showing the door side (the facing).
- Space turns it a quarter turn (facing + 2).
- Parts of the footprint that cannot be built on are drawn red: sea, ground too steep (height spread inside the
  footprint over a threshold to tune; smaller unevenness is fine, the braves flatten it), another building or
  construction site, a tree that still has wood (size > 0). Any red part blocks placement.
- A left click on a valid spot places it (`Command::PlaceBuilding { player, kind, x, z, facing, braves }`): the
  blueprint stays drawn on the ground as a construction site, a `Building` with a `stage` (Site / Built /
  Dismantling) and its wood counts. A size-0 tree under it stays invisible and does not grow back while covered.
- The braves selected when placing are assigned to it. Later, selecting braves and clicking the site assigns them
  (`Command::Assign { player, site, units }`).
- More braves than the site's maximum: the first ones (by unit id, deterministic) up to the maximum are assigned,
  the others walk to the site and stand idle, not assigned.

### Tooltip
Hovering or right-clicking a site (as for trees): kind, braves assigned / maximum, wood delivered / needed. On a
built building of a buildable kind: a "Dismantle" toggle (back to "Build" while dismantling).

### Building it (deterministic, integer state)
1. Gather: assigned braves walk to the site edge and stand looking at it for a short while.
2. Flatten: each brave walks onto a footprint point, jumps, which moves that point's height toward the target
   height (average of the footprint, at least 64, as `Building::flatten`), then moves to the next point not yet
   flat. Done when every point is at the target.
3. Wood: each brave fetches one piece at a time: the nearest wood piece lying on the ground within range, else the
   nearest tree with size > 0, where it cuts one piece (`Tree::cut`, cutting takes a while). It drops the piece on
   the ground next to the door. First dispatch rule: every assigned brave fetches wood while delivered + carried <
   needed; to revisit (split gatherers/builders, nearest brave per piece).
4. Build: a brave picks a piece up from the door pile, works on the site for a while, and progress grows by one
   piece. The tooltip shows wood provided (pile + used) and needed.
5. Done when all the wood is used: the stage becomes Built, assigned braves are released (idle).

On screen: the white blueprint until the first piece is used; then a wooden frame of the building's shape (the
same model in a plain wood material, or a scaffold sized from the footprint), whose visible part grows with the
progress; the full building once built.

### Dismantling
With Dismantle on, braves assigned to the building take it apart: after a while it loses one piece of wood, which
is dropped on the ground near the door as a wood piece (drawn as a small circle on the ground, `GameMap::wood`,
usable by any construction). Once all its wood is out, the building is removed and its ground is free again (a
covered tree grows back). Switching back to Build makes braves rebuild it with wood again.

### Cost (placeholders to tune, then match the original)
| Kind | Wood | Max braves |
|---|---|---|
| Villager hut | 3 | 3 |
| Drum tower | 4 | 3 |
| Training huts | 5 | 4 |
| Boat / airship hut | 6 | 5 |
