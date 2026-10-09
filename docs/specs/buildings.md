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
Construction state (`Building`): `used` pieces of wood, `flat`, `dismantling`, `inside` (units in), `shaking`
(ticks left after a hit), `stock` (wood on the pile by the door, not built in yet), `level` (the height a plan's
footprint is flattened to); `Building::stage` works out Blueprint (not flat) / UnderConstruction / Built /
Dismantling from them, and `BuildingKind::wood_cost` / `max_braves` hold the cost table below. Level buildings
load as Built. A vault of knowledge also loads its `reward` (`Reward::Spell` / `Reward::Building`,
`building::vault_reward`): the discovery of the library trigger on its cell (level-format.md "Vault of knowledge");
all 23 vaults of the levels have one, none of the other buildings; with the gauge and phase of `game_core::vault`
(worship.md), its door and top drawn from them. Not prayed at nor granted yet. Braves build plans placed in the game (see "Construction"); nothing else changes the state over
time yet, and there is no health. Sandbox > Buildings (`GameMap::sandbox_buildings`, an island of radius 48 cells): south, one of
every model 1-19 for the player and a few red ones; north, one row per buildable kind in each `showcase_states`
column (blueprint, under construction at 0, 1/3, 2/3 and all but one piece, built, dismantling at half, attacked,
people inside); east, free ground with 8 braves, a pile of 12 wood pieces and 6 trees, to try construction on.

## On screen (client, `buildings.rs`)
Centred on the terrain (`Grounded`) and leaning with it (`grounded::Tilted`: its up follows the drawn ground
normal over 1 cell either side, slopes and the planet's curve, so no side sinks in), turned by the facing
(`facing_yaw`).
- With the original files: the original object (`building_object`: huts `catalog::villager_hut` style 0, drum
  tower, temple = prayer hut, spy/warrior/firewarrior training, boat and airship huts in the owner's colours,
  neutral ones blue; vault = pyramid of knowledge, prison), textured from the level's theme atlas (theme 0 on maps
  without one), cut-out texels see-through, faces drawn from their front only (back faces culled, objects.md
  "facs").
- Otherwise, and for named types without an identified original object: the generated kit in
  `assets/3d/buildings/` (16 models: all named kinds, including hut sizes 1-3). Procedurally textured clay,
  timber, thatch and stone with only the `Tribe` material recoloured (grey when neutral).
  `generated_buildings.rs` reads embedded GLB Body/Scaffold meshes into the same `MeshData` stage path;
  material colours become linear vertex colours, multiplied by a shared 512 x 512 grayscale surface atlas.
  `TEXCOORD_0` supplies face-projected UVs; padded tiles and five mip levels keep surfaces readable at distance.
  Each GLB links the atlas next to it, `surfaces.png`, for Blender. Geometry is already in cell units, entry local -z,
  boat piers +z. See [asset inventory](assets.md) and [authoring contract](../../assets/3d/buildings/README.md).
- Only unknown model IDs (e.g. 12) or an invalid bundled model keep the labelled diagnostic box,
  1.6 x 0.8 x 1.6 cells. Generated maps still show buildings only in Sandbox > Buildings.

By stage (`Building::stage`, client `construction.rs`):
- Blueprint: the same white mark as the Build tab's blueprint (`blueprint::mark_mesh`), draped on the ground and
  redrawn every frame, no model.
- Under construction and dismantling: the wooden structure, the model's face edges as square brown beams
  (`object_edges` + `beams` for original objects; the generated kit has a separate authored timber scaffold;
  the unknown-ID box: corner posts and a ring per piece of wood), with the built part of
  the real model inside it (`built_part`: its triangles sorted by height, the lowest `used / wood_cost` of them;
  the unknown-ID box is stacked in one layer per piece). So it fills from the ground up, one band per piece.
  Generated material colours and texture UVs survive the triangle selection; timber frames share the atlas.
  Its faces show their outer side as usual and their inner side in the plain tribe colour, so the inside of the
  building is not seen through the structure. The inner side is drawn from a copy pushed 0.006 cells along each
  face's normal (`pushed`): where the model has two faces back to back on one plane, it stays behind the textured
  one instead of flickering with it. Part-by-part assembly is still future work.
- Built: the whole model.
- Attacked (`shaking` > 0, someone inside damaging it): the model (not the structure) rocks about its base, the
  walls move and the base stays: a blow every 0.7 s tilts it by up to 0.012 rad, a quick wobble dying out before
  the next, in a new direction each time (`blow_tilt`).
- A built building people stay in (huts, drum towers: `BuildingKind::capacity`) with people inside (`inside` >
  0): grey puffs rise from the middle of its highest points
  (`chimney`, the top of the roof; the generated hut has a central raised smoke vent), growing and drifting, then shrinking away (`puff_at`). Other kinds show
  nothing for busy yet.
- Torches: a built building's flames burn, animated like the camp fire's (`flame.rs`, see "Camp fire"; each
  building starts at its own frame), and cast no shadow. Original objects: their blended faces (tile 92, see
  [objects.md](objects.md) "Blended faces") are left out of the model, its structure and its built part, and drawn
  as the flame instead: the firewarrior training hut's 2 torches, the prayer hut's (temple) 4. Under construction
  or dismantling, no flame. Generated kit: two crossed flame boards, 0.7 cell, standing on each bowl of the
  firewarrior hut's braziers (`generated_buildings::flame_bases`), around the kit's static ember cone.
Kit meshes are made the first time one shows and shared per kind and owner: the built model and the timber frame
(a built building never paints its frame); the built part under construction is made per building.
Views are rebuilt only when the map resource changes; once the simulation changes buildings during ticks they
need stable ids and updates per building.

What huts and training huts do once built: [huts-and-training.md](huts-and-training.md).

## Construction
Only these are built and taken apart by braves: villager hut (placed at size 1, it grows later), drum tower, the
training huts (warrior, firewarrior, temple = preachers, spy), boat hut and airship hut. The reincarnation site,
prison, vault/pyramid of knowledge and totems can never be built nor dismantled.

### Build tab (HUD)
Like the Spells tab: one tile per buildable kind, per tribe `Availability` (Hidden / Discoverable "?" / Available),
taken from the level (`.hdr`) and unlocked by triggers. Clicking an available tile picks a blueprint.

Done (no icons; a click picks the blueprint, see below): `game_core::build_book::BuildBook`, the original panel's 8 kinds (`BUILDABLE`: hut,
drum tower, temple, spy, warrior and firewarrior training, boat hut, airship hut). `BuildBook::from_level`: the
header's `BuildingsAvailable` (hut = model 1, boat hut 13, airship hut 15) are Available, building discoveries
(`DiscoveryType 2`, any hut size = the hut) not yet available are "?". Stored as `GameMap::build_book` (original
levels; generated maps and sandboxes: all available). The campaign is consistent: each level discovers one kind that
the next level's header has (warrior hut 1 -> 2, temple 3 -> 4, drum tower 4 -> 5, firewarrior 8 -> 9, boat hut
9 -> 10, spy 12 -> 13, airship hut 13 -> 14). Models 12 ("curr OE slot") and 17 (guard post) are set in many
masks but are not on the panel; 18 and 19 (vault, prison) are always set and never built. Client: `hud/build.rs`,
tiles named (`panel_name`), "?" for discoverable, empty slot for hidden, hover describes. A last tile, always
available, is the camp fire (see "Camp fire").

### Blueprint
- Follows the cursor as a white mark drawn on the ground, the building's footprint draped over the terrain
  heights, with an arrow showing the door side (the facing).
- Space turns it a quarter turn (facing + 2).
- Parts of the footprint that cannot be built on are drawn red: sea, ground too steep (height spread inside the
  footprint over a threshold to tune; smaller unevenness is fine, the braves flatten it), another building or
  construction site, a tree that still has wood (size > 0). Any red part blocks placement. A boat hut is also all
  red unless its jetty side is over the sea and its door side on land; its blueprint turns itself to fit.
- A left click on a valid spot places it (`Command::PlaceBuilding { player, kind, x, z, facing, braves }`): the
  blueprint stays drawn on the ground as a construction site, a `Building` with a `stage` (Blueprint / Under
  construction / Built / Dismantling) and its wood counts. A size-0 tree under it stays invisible and does not grow back while covered.
- The braves selected when placing are assigned to it. Later, selecting braves and clicking the site assigns them
  (`Command::Assign { player, site, units }`).
- A site needs at least one assigned brave to progress (none: it just waits); each kind has only a maximum. More
  braves work faster: each flattens, fetches and builds on its own, so the work is shared between them.
- More braves than the site's maximum: the first ones (by unit id, deterministic) up to the maximum are assigned,
  the others walk to the site and stand idle, not assigned.

Done:
- `game_core::placement`: `blocked_at(map, point)` (Sea: drawn height < 1; Building: `Building::covers` its turned
  footprint; Site: within `SPAWN_FLAT_RADIUS` of a reincarnation site; Tree: a cell with a tree that has wood),
  `too_steep` (height points under the footprint spread more than `STEEP_SPREAD` = 200, placeholder), `shore_ok`,
  `can_place`.
- Boat hut (`shore_ok`): its jetty is local +z, its door local -z (all 11 boat huts of the levels: only sea within
  2.5 cells past +z, land past -z). A cell past the jetty side, the middle and at least one corner must be sea (a
  loaded level's levelling lifts one corner of level 10's hut off the sea); a cell past the door side, no sea.
  `best_facing` turns a boat hut blueprint to the first quarter turn from the player's that fits. Tests cover each
  coast of an island, inland / at sea / a thin spit, other blockers, and every boat hut of the original levels
  when an install is found.
- Client `blueprint.rs`: a click on an available Build tab tile picks it (white border; picking a spell puts it
  away and the other way round). Over the map it snaps to the nearest cell corner like the levels' buildings and is
  drawn centred as they are, on a grid through the terrain's cell lines every quarter cell, split along the cells'
  own diagonal, so it lies exactly on the drawn triangles (no terrain poking through on hills), with a door arrow
  out of the local -z side (the boat huts' land side; other kinds unverified); vertices red where `blocked_at`, all
  red when `too_steep` or a boat hut is off the shore, arrow red when any part is. Boat huts turn themselves
  (`best_facing`).
  Space turns it (and no longer looks at the shaman meanwhile), right click on the map puts it away, left click
  where it can stand places it (`blueprint::place_command`, `Command::PlaceBuilding`, `GameMap::place_building`
  checks `can_place` again), sends the selected braves to it (`GameMap::build_orders`) and puts the blueprint
  away; units are not selected or ordered while it is out. Leaving the game or changing level puts it
  away.

### Tooltip
Done (`buildings::building_label`, after resting the cursor `HOVER_SECS` on a building, plans included): the
kind's name; for the player's buildings that take wood, `Braves: assigned/max` and `Wood: delivered/cost`, while
a plan and under construction; once built, `Braves: inside/room` (huts only) and `Wood: n`, the wood in it
(what dismantling gives back, raised when a hut grows). One `Braves` line either way. Other tribes' buildings show their name only. To do:
Hovering or right-clicking a site (as for trees): kind, braves assigned / maximum, wood delivered / needed. On a
building of a buildable kind with at least one piece of wood used (under construction or built): a "Dismantle"
toggle (back to "Build" while dismantling). It also lists the
assigned braves, one small icon each; clicking an icon selects that brave alone (one at a time), a quick way to
pick one besides selecting it on the map.

### Orders and selection
- Any new order to an assigned brave (`Command::OrderUnit`: move, pray, stop...) unassigns it from the site,
  inside the simulation, so replays match. It can be assigned again by hand (`Command::Assign`). A piece of wood it
  carries stays with it until it is idle, then it is put down where it stands (units.md "Wood").
- Assigned braves can be selected all along, as usual (click, box), wherever they are: away fetching wood, around
  the site or inside the building.
- A brave inside a building under construction given an order (move...) leaves through the door first (see "Doors" below), then goes on.
- Shift + click on a blueprint (footprint not flat yet) cancels it (`Command::Cancel { player, site }`): the site
  is removed, its braves are unassigned and stand idle, the ground keeps whatever flattening was done. Wood already
  brought to it is lost (cancelling never gives wood back). Once the footprint is flat it is a building under
  construction: Shift + click does nothing, it can only be dismantled, which gives back its wood one piece at a time.

### Done (`game_core::work`)
- `Command::PlaceBuilding { player, kind, at, facing }` (`at`: the stored corner) adds a plan
  (`Building::placed`: `level` = average height of its footprint points, at least `MIN_GROUND`). Building kinds
  with no wood cost (vault, prison...) are refused.
- Braves are assigned with `Order::Build { site }` (`site`: the stored corner) through `Command::OrderUnit` /
  `QueueOrder`, as `Unit::work`. `GameMap::build_orders` makes them for a click: braves in id order (the first
  ones up to `max_braves` are assigned, the others walk to the door unassigned), other kinds walk to the door.
  Any other order to an assigned brave (direct or chained, when it starts) unassigns it; a piece it was building
  goes back on the pile. Dying unassigns too.
- Each tick, every assigned brave that is free (idle or holding wood, nothing chained) goes on (`GameMap::work`):
  1. Carrying wood: walks to the door (`Building::door`, `DOOR_GAP` past the middle of the local -z side) and
     puts it on the pile (`stock`) once within a cell of it.
  2. Plan (not flat): while wood is wanted (`wood_wanted`: 1 piece before flat, as in the game, then the rest of
     the cost) and fewer braves bring wood than missing pieces, fetches wood (`Order::FetchWood`'s rule). Else
     walks to the nearest footprint height point not done (each needs one jump at least, `Building::jumped`, and
     must be at the site's level) that no other brave is on or going to, and jumps on it (`Action::Flattening`, `JUMP_TICKS` 8, the jump pose: original anim 12): the point moves `JUMP_STEP` (32)
     towards the level. With no point left, the plan is flat: the ring around is blended
     (`Building::flatten`) and it is under construction: walled (see "Walking around buildings"). Wood lying in
     its walls is moved out in front of its door (`clear_wood_under`); braves never go for wood or trees
     behind walls (`GameMap::behind_walls`). Walls going up stop anyone walking to a spot behind them (they go
     on with their next order or task); a walk whose target is behind walls ends idle, never stranded. Ground
     already level still takes one jump per point: the flattening step always shows.
  3. Under construction: with wood on the pile, walks to the door, in (`Unit::enter`, `Action::Entering`,
     straight to a work point a third of a cell apart per brave around the centre) and builds the piece from
     inside (`Action::Building`, `BUILD_TICKS` 50, the hammer pose); with no pile, fetches wood under the
     dispatch rule below (out by the door first); else walks in by the door and hammers (`Action::Hammering`,
     open-ended, free for the next step) until there is wood or the building is done. No brave stands idle.
  4. Built: the moment the last piece is in, everyone inside walks out by the door to a free spot around it
     (`slots::dispatch` from the door) and stands idle; assigned braves are released (`work` cleared).
  Braves bringing wood (`wood_on_the_way`): carrying, fetching or cutting, or building a piece taken off the
  pile.
- `Command::CancelBuilding { player, at }`: the player's plan (not flat) whose footprint holds `at` is removed,
  its braves stop, the wood brought is lost. Once flat it can only be dismantled (not done).
- A hut with 3 braves and wood around (Sandbox > Buildings) takes about 20 s.
- Client: a left click on one of the player's sites (within `AROUND`) with units selected sends braves to it
  (`selection::ground_click`); Shift + right click on a plan cancels it (`shift_right_click`, after the camp
  fires). Wood on a pile is drawn as wood pieces around the door (`Building::pile_point`, `wood::PileView`).
  Building views are redone whenever a building changes. Units standing inside a built building are not
  drawn nor picked (`units::hidden_inside`), except up a lookout (`buildings::lookout`: a drum tower's
  platform, at 0.56 of the drawn model's height, `ModelHeights`), where they stand in its middle, pulled
  `PERCH_PULL` (0.7 cell) towards the camera so the drum or the walls never hide them; inside a building under construction (open frame), and walking in
  or out, they are. Dev: `BUILD=hut@90,62 BUILD_TICKS=150` places a plan
  with all the player's braves and runs that many ticks before the shot.

Not done yet: the gathering look, dismantling, the tooltip's brave icons and Dismantle toggle, wood claimed by
braves of another site, and a jump per point being the game's rule (to check).

### Building it (design, deterministic, integer state)
1. Gather: assigned braves walk to the site edge and stand looking at it for a short while.
2. Flatten (stage Blueprint): each brave walks onto a footprint point, jumps, which moves that point's height
   toward the target height (average of the footprint, at least 64, as `Building::flatten`), then moves to the next
   point not yet flat. Done when every point is at the target. Meanwhile the first piece of wood is brought to the
   site (as in the game); it is lost if the blueprint is cancelled.
3. Flat: the braves leave the footprint and spread around the building; the stage becomes Under construction
   (no cancel any more) and it is drawn as a wooden structure. From here they work around it and can also go
   inside it to work.
4. Wood: each brave fetches one piece at a time: the nearest wood piece lying on the ground within range, else the
   nearest tree with size > 0, where it cuts one piece (`Tree::cut`, cutting takes a while). It drops the piece on
   the ground next to the door. First dispatch rule: a brave only goes for wood if delivered + claimed < needed, where
   claimed = pieces being fetched or carried by other braves; so at most needed - delivered braves are out at once
   (a hut needing 3 sends at most 3, one piece each). The others wait at the site and build from the pile. When a
   piece is used, nothing changes (it counts as delivered). To revisit (nearest brave per piece, gatherers vs
   builders).
5. Build: a brave picks a piece up from the door pile, works on the building (around or inside it) for a while, and
   progress grows by one piece. The tooltip shows wood provided (pile + used) and needed.
6. Done when all the wood is used: the stage becomes Built, assigned braves are released (idle).

On screen (as in the game):
- Blueprint: the white mark on the ground while the footprint is flattened, even once wood is brought.
- Under construction: from the moment the footprint is flat, a wooden structure of the building's shape. As pieces
  are used it turns, part after part, into the final building: more and more parts of the real model show and
  replace the wood, until the whole building stands (Built). Pieces used / wood needed picks how many parts.
- Dismantling and damage go the other way (parts back to wood, then removed).

### Dismantling
With Dismantle on (on a built building or one under construction), braves assigned to it take it apart: after a while it loses one piece of wood, which
is dropped on the ground near the door as a wood piece (a pile of logs on the ground, `GameMap::wood`, see trees.md,
usable by any construction). Once all its wood is out, the building is removed and its ground is free again (a
covered tree grows back). Switching back to Build makes braves rebuild it with wood again.

### Damage and repair
The level files store every building as a finished one (level-format.md, building union), but in level 10 the
island that praying at the stone head brings back (effects 89 atlantis set / 90 atlantis invoke) shows its
buildings as under construction. That state comes from the game itself: most likely the buildings are damaged when
their ground sinks and rises, and a damaged building looks and works like one under construction, which braves
repair with wood. Level 10 also places effect 83 "boat hut repair" on a boat hut, and the effect list has
"building smoke full / partial / damaged" (74-76) and "explode building partial" (14), so damage is a state that
exists while the game runs.

Seen in the game once the island is up (wood in the building / wood needed): every hut 1/3, warrior training
6/8, firewarrior training 7/8, temple 5/8, drum tower 4/5, boat hut 2/5. So they lose 1 to 3 pieces each, with no
common percentage and no link with the island heights stored under them (huts at heights 30 to 90 all show 1/3):
the rule is unknown (fixed per kind, random, or from how far the ground moved).

Planned model: damage takes wood out of a Built building (its used count drops below the kind's wood cost, the
pieces are lost, not dropped), its view goes back to the wooden frame, and assigned braves bring it back up like a
construction. To check in the game: what sets the damage, and whether repairs need fetched wood.

### Doors
Units enter and leave a building only through its door (the facing's side, local -z), the same way in and out:
- Entering: walk to the door, then from the door to the building's centre.
- Leaving: walk from where they are to the building's centre, then to the door, then on along their route.
A blueprint is not a building yet: braves walk on and off its footprint freely to flatten it. As soon as it turns
into a wooden structure (footprint flat, Under construction) it is a building: from then on, including while
dismantling, braves go in and out by the door. Later the same holds for any building units go into (houses,
training huts, towers, boat and airship huts).

### Walking around buildings
Done (`path::Walls`, `GameMap::update_walls`, each tick): every building past its plan stage walls the cells whose
centre its footprint covers (`Building::walled_cells`). `path::Ground` is what movers cross: bare terrain, or
`Walled` (terrain + walls, as one unit sees it: the building it is `inside` does not stop it). Walkers never
cross a wall (A*, straight legs, steps, standing spots, teleport landing), boats and balloons ignore walls, plans
and the reincarnation site are not walled. A wall change bumps the walls' revision and walkers replan. A unit
standing in a newly walled cell is inside that building (`Unit::inside`, with its door). A unit inside going
anywhere first walks straight to the door, then plans its route; if the door is blocked it stays inside, idle. Into a building only `Unit::enter`: straight from
the door. Design notes:
Buildings and construction sites block walking over their footprint (`BuildingKind::footprint` turned with the
facing, the cells it covers): `path::Mobility::Walk` gets a blocked-cell mask from `GameMap` besides the terrain,
so routes go around them, and a unit never stands inside one. Exceptions:
- Flying units and vehicles (`Mobility::Fly`) ignore it.
- The reincarnation site is walked on (it is not in the mask).
- Braves assigned to a site or a building being dismantled may walk on its footprint (their own path query leaves
  it out of the mask): anywhere on a blueprint to flatten it; once it is under construction, only in and out
  through the door (Doors).
- A boat hut's jetty side stays open to boats (`Mobility::Sail`), to check with vehicles.
Placing a site or finishing/removing a building changes the mask: walkers whose route crosses it replan (as for a
terrain write). A unit already standing on a new site's footprint is moved to the nearest free cell.

### Cost
From the original game (tooltips on level 10's atlantis island, see "Damage and repair", and the game's
construction limits).

| Kind | Wood | Max braves (min is always 1) |
|---|---|---|
| Villager hut | 3 (large huts on the island show x/3) | 6 |
| Drum tower | 5 | 12 |
| Temple (preachers) | 8 | 20 |
| Warrior training | 8 | 16 |
| Firewarrior training | 8 | 16 |
| Spy training | 8 | 16 |
| Boat hut | 5 | 16 |
| Airship hut | 11 | 16 |

## Camp fire (done: `game_core::campfire`)
Not a building: always on the Build tab (after the panel's buildings), lit in one click, no wood nor braves.
Its blueprint is the cell under the mouse (white, red where it cannot be lit) with cursor icon 66; a left click
lights it and puts the blueprint away. Clicking one of the player's fires with units selected sends them round
it (instead of walking there).
- Placed at a cell's centre (`Command::PlaceCampfire`, `GameMap::place_campfire`), only on flat free land
  (`campfire::can_place`): no corner of the cell in the sea, corners within `FLAT_SPREAD` of each other, and no
  building, site platform, tree with wood or other camp fire on it. It blocks buildings on its cell
  (`placement::Blocked::Campfire`).
- Units sent to it (`GameMap::gather`, `Order::Campfire`) walk to a point of its ring (`RING` world units out,
  `RING_POINTS` points), spread evenly from the point nearest the first one, then go round it
  (`Action::AroundFire`, half their walking speed, the walk animation).
- A camp fire with nobody of its tribe going to it or round it for `ABANDON_TICKS` (60 s) goes out (removed).
- Shift + right click on one of the player's fires (mouse over its cell, on the map) puts it out
  (`Command::RemoveCampfire`, with or without a selection, which is kept): the units going to it or round it
  stop, idle. The same gesture will remove a placed building plan.
- Drawn (`campfire.rs`) with original object 0: its logs from the theme atlas, its flame boards (blended faces,
  objects.md) through the theme's alpha table with the faint board around the flame left out, animated by
  warping the picture (`flame.rs`: 8 frames at 10 fps, the tip sways with a wave climbing up, the flame stretches
  and flickers; each fire starts at its own frame). Without the original files: four generated logs, two crossed
  boards and a generated flame tile through the same steps.
