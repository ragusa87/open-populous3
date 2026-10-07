# Buildings

## Simulation (done: `game_core::building`, view only)
`GameMap::buildings`: `Building { kind, owner, x, z, facing }` from an original level's things of kind 2
(`buildings_from_level`): model = type, owner = tribe (255 = neutral), position in world units, facing in eighths
of a turn (`Thing::facing`, from the `i32` angle at byte 7, 2048 = a full turn; only quarter turns in the files).
In the files a building always sits **exactly on a cell corner** (x, z multiples of 512), unlike other things (cell
centres). The levels are already flattened around them (equal heights over the vertices around that corner):

| Buildings | Flat area |
|---|---|
| huts 1-3, temple, warrior, firewarrior training, airship hut | vertices -1..+2, i.e. 3 x 3 cells |
| drum tower, boat hut | only the cell at the corner (vertices 0..+1) |
| spy hut | vertices 0..+2, 2 x 2 cells (17 of 17, every facing) |
| vault, prison | vertices -2..+3, 5 x 5 cells (23 of 24) |

Whatever the facing, the centre is half a cell off the stored corner (x + 256, z + 256), a whole cell for the spy
hut (`BuildingKind::centre_shift`, `Building::centre`). `Building::flatten` and the view centre the building
there.

The other bits seen in the files and the editor:
- The editor rotates buildings by quarter turns.
- Each of the 91 general things of model 9 ("building add-on", no data) sits within about 2 cells of a medium or
  large hut of the same owner: probably a hut extension (unverified).
- The level header's `BuildingsAvailable` has bit N = building model N (see level-format.md).

Types by model, as numbered in the original's
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

Done (no icons, no blueprint yet): `game_core::build_book::BuildBook`, the original panel's 8 kinds (`BUILDABLE`: hut,
drum tower, temple, spy, warrior and firewarrior training, boat hut, airship hut). `BuildBook::from_level`: the
header's `BuildingsAvailable` (hut = model 1, boat hut 13, airship hut 15) are Available, building discoveries
(`DiscoveryType 2`, any hut size = the hut) not yet available are "?". Stored as `GameMap::build_book` (original
levels; generated maps and sandboxes: all available). The campaign is consistent: each level discovers one kind that
the next level's header has (warrior hut 1 -> 2, temple 3 -> 4, drum tower 4 -> 5, firewarrior 8 -> 9, boat hut
9 -> 10, spy 12 -> 13, airship hut 13 -> 14). Models 12 ("curr OE slot") and 17 (guard post) are set in many
masks but are not on the panel; 18 and 19 (vault, prison) are always set and never built. Client: `hud/build.rs`,
tiles named (`panel_name`), "?" for discoverable, empty slot for hidden, hover describes.

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
- A site needs at least one assigned brave to progress (none: it just waits); each kind has only a maximum. More
  braves work faster: each flattens, fetches and builds on its own, so the work is shared between them.
- More braves than the site's maximum: the first ones (by unit id, deterministic) up to the maximum are assigned,
  the others walk to the site and stand idle, not assigned.

Done (UX only, nothing is placed yet):
- `game_core::placement`: `blocked_at(map, point)` (Sea: drawn height < 1; Building: `Building::covers` its turned
  footprint; Site: within `SPAWN_FLAT_RADIUS` of a reincarnation site; Tree: a cell with a tree that has wood),
  `too_steep` (height points under the footprint spread more than `STEEP_SPREAD` = 200, placeholder), `can_place`.
- Client `blueprint.rs`: a click on an available Build tab tile picks it (white border; picking a spell puts it
  away and the other way round). Over the map it snaps to the nearest cell corner like the levels' buildings and is
  drawn centred as they are, a 12 x 12 grid draped on the drawn ground with a door arrow out of the local +z side
  (door side unverified); vertices red where `blocked_at`, all red when `too_steep`, arrow red when any part is.
  Space turns it (and no longer looks at the shaman meanwhile), right click on the map puts it away, left click
  does nothing yet; units are not selected or ordered while it is out. Leaving the game or changing level puts it
  away.

### Tooltip
Hovering or right-clicking a site (as for trees): kind, braves assigned / maximum, wood delivered / needed. On a
building of a buildable kind with at least one piece of wood used (under construction or built): a "Dismantle"
toggle (back to "Build" while dismantling). It also lists the
assigned braves, one small icon each; clicking an icon selects that brave alone (one at a time), so it can be
given an order.

### Orders and selection
- Any new order to an assigned brave (`Command::OrderUnit`: move, pray, stop...) unassigns it from the site,
  inside the simulation, so replays match. It can be assigned again by hand (`Command::Assign`). A piece of wood it
  was carrying is dropped where it stands, as a wood piece on the ground; its claim is freed for another brave.
- Braves away from the site (gathering, watching, fetching or carrying wood) can be selected as usual (click, box).
- Braves working on the footprint (flattening, building, dismantling) cannot be selected on the map: clicks and
  boxes skip them, a click there hits the site (tooltip). The tooltip icons are the only way to pick one of them.
- Shift + click on a blueprint (a site with no wood used yet) cancels it (`Command::Cancel { player, site }`): the
  site is removed, its braves are unassigned and stand idle, wood already piled by the door stays there as free
  wood pieces, the ground keeps whatever flattening was done. Once a piece of wood is used it is a building under
  construction: Shift + click does nothing, it can only be dismantled.

### Building it (deterministic, integer state)
1. Gather: assigned braves walk to the site edge and stand looking at it for a short while.
2. Flatten: each brave walks onto a footprint point, jumps, which moves that point's height toward the target
   height (average of the footprint, at least 64, as `Building::flatten`), then moves to the next point not yet
   flat. Done when every point is at the target.
3. Wood: each brave fetches one piece at a time: the nearest wood piece lying on the ground within range, else the
   nearest tree with size > 0, where it cuts one piece (`Tree::cut`, cutting takes a while). It drops the piece on
   the ground next to the door. First dispatch rule: a brave only goes for wood if delivered + claimed < needed, where
   claimed = pieces being fetched or carried by other braves; so at most needed - delivered braves are out at once
   (a hut needing 3 sends at most 3, one piece each). The others wait at the site and build from the pile. When a
   piece is used, nothing changes (it counts as delivered). To revisit (nearest brave per piece, gatherers vs
   builders).
4. Build: a brave picks a piece up from the door pile, works on the site for a while, and progress grows by one
   piece. The tooltip shows wood provided (pile + used) and needed.
5. Done when all the wood is used: the stage becomes Built, assigned braves are released (idle).

On screen: the white blueprint until the first piece is used; then a wooden frame of the building's shape (the
same model in a plain wood material, or a scaffold sized from the footprint), whose visible part grows with the
progress; the full building once built.

### Dismantling
With Dismantle on (on a built building or one under construction), braves assigned to it take it apart: after a while it loses one piece of wood, which
is dropped on the ground near the door as a wood piece (a pile of logs on the ground, `GameMap::wood`, see trees.md,
usable by any construction). Once all its wood is out, the building is removed and its ground is free again (a
covered tree grows back). Switching back to Build makes braves rebuild it with wood again.

### Walking around buildings
Buildings and construction sites block walking over their footprint (`BuildingKind::footprint` turned with the
facing, the cells it covers): `path::Mobility::Walk` gets a blocked-cell mask from `GameMap` besides the terrain,
so routes go around them, and a unit never stands inside one. Exceptions:
- Flying units and vehicles (`Mobility::Fly`) ignore it.
- The reincarnation site is walked on (it is not in the mask).
- Braves assigned to a site or to a building being dismantled may walk on that one footprint (their own path query
  leaves it out of the mask), to flatten, build or take it apart.
- A boat hut's jetty side stays open to boats (`Mobility::Sail`), to check with vehicles.
Placing a site or finishing/removing a building changes the mask: walkers whose route crosses it replan (as for a
terrain write). A unit already standing on a new site's footprint is moved to the nearest free cell.

### Cost (placeholders to tune, then match the original)
| Kind | Wood | Max braves (min is always 1) |
|---|---|---|
| Villager hut | 3 | 3 |
| Drum tower | 4 | 3 |
| Training huts | 5 | 4 |
| Boat / airship hut | 6 | 5 |
