# Units

- `game_core::unit::Unit`: id, owner, kind, `u16` x/z in world units (512 per cell), wrapping at 65536
  (plain `u16` wrapping arithmetic walks the torus), `facing` in eighths of a turn (0 = +z, 2 = +x), health
  (`game_core::health::Health`: integer current and max points), `motion` (`game_core::motion::Motion`: `velocity`, x and z
  in world units, y in height units, per tick, and `height`, absolute, above the sea, in terrain height units,
  while off the ground, None on it),
  `statuses` (`game_core::status::Statuses`: invisible, shielded, bloodlust, hypnotized, ghost; none set yet), action.
- Movement in fixed point per tick; no floats in simulation state.
- Time (`game_core::time`): one tick is one original game turn, 12 per second (`TICKS_PER_SECOND`,
  pop3-rev-analysis.md "Turns and timing"). `Tick` is a moment (`GameMap::now`), `Ticks` a length, built from the
  original's turns (`Ticks::new`) or from our guesses in seconds (`Ticks::secs`, `Ticks::millis`); a timed action
  holds a `Countdown`, which the client also reads for its animation; `Every::new(16).fires(now, phase)` is a
  power-of-two rhythm. `GameMap::run(ticks)` runs several ticks (tests, dev shots).

## Shaman (done: simulation)
`GameMap::units` holds one shaman per reincarnation site, spawned at `spawn_point()` when the map loads.
`GameMap::tick()` advances every unit; orders arrive as
`Command::Order { player, order }` through `GameMap::apply` (lockstep-safe). Casting a spell
(`Command::Cast`) also makes the caster's shaman jump.

| Action | Entered by | Behaviour |
|---|---|---|
| Idle | default, arrival, Stop | heals `HEALTH_STEP` (20) HP every `REGEN_EVERY` (0.5 s) |
| Walking { to } | `Order::MoveTo` | follows a path (see Pathfinding) at 53 units/tick on flat ground (1.24 cells/s) (slope over the next step: `slope_speed`, 1/256 factor `256 - grade*k/100` with k = 192 uphill and 128 downhill, clamped to 32..384, grade = height per cell: ~78% speed up the sandbox ramp, quarter speed up the steep hill, 1.5x down it, ground height bilinear `Heightmap::height_at`); target unreachable: the shaman stays Idle, other units are Stranded |
| Stranded { to } | target unreachable (not the shaman) | does not move, arms up, -20 HP every `STRANDED_HURT_EVERY` (0.3 s) until the terrain opens a path (walks again) or it dies |
| Worshipping | `Order::Worship { site }` (a click on a vault of knowledge with her selected) | prays at the vault's door; heals; see worship.md |
| Casting { left } | `Order::Cast`, any spell cast | `CAST_TICKS` (10, the original's cast state) jump, then Idle (Teleport: then at the target, Landing) |
| Landing { left } | arriving from a teleport | `LANDING_TICKS` (0.6 s), then Idle; drawn in the idle pose floating 0.2 cell up and settling down (`landing_lift`, eases out); a puff of dust at touchdown (`units/dust.rs`) |
| Chopping { tree, left } | `Order::CutTree`, `Order::FetchWood` (braves only) | walks to a free spot next to the tree, then `CHOP_TICKS` (20, 1.67 s, the original's countdown) of chopping, then the tree loses one size and the brave carries the piece |
| Flattening { at, left } | assigned to a plan (`Order::Build`) | `JUMP_TICKS` jump on a footprint height point, which then moves towards the site's level (buildings.md "Construction"); drawn with the jump pose (original anim 12; the CC0 sheets: arms up) |
| Entering { to } | `Unit::enter` at a building's door | straight to `to` inside, through its walls, then Idle inside (`Unit::inside`) |
| Building { left } | inside a site with wood on its pile | `BUILD_TICKS` building one piece in (hammer pose: the axe swing, anim 11, until the original hammering anim is found) |
| Hammering | inside a site under construction with nothing to build yet | until wood comes or it is built; counts as free (`Unit::is_free`) |
| Holding { left } | idle with a piece of wood | stands holding it for `HOLD_TICKS` (3 s), then puts it down where he stands; any order (chained or direct) takes over and keeps the piece |
| Drowning | ground under her becomes open sea (on the ground) | -60 HP per tick, no orders; back to Idle if land returns |
| Tumbling | flung or rolling from a spell (nothing sets it yet) | locked (no orders, not hovered, dropped from the selection, see "Locked units"), no healing, no drowning while off the ground; moved by the physics step (below) while off the ground, back to Idle once on the ground and still. Drawn flung in the air (off the ground: followers 19, wildmen 31, shaman 61) or falling down a slope on the ground (followers 37, wildmen 47, shaman 73); the CC0 sheets show their fall meanwhile. `TUMBLE=air\|ground` in a shot |
| Dying { left } | health reaches 0 | `DYING_TICKS` (0.8 s) |
| Dead { left } | after dying | `RESPAWN_TICKS` (3 s), then reincarnates at her site at full health; the site levels its ground again |

Health: 2000. Orders are ignored while drowning, dying or dead.

## From the levels (done)
`GameMap::from_level` spawns every person thing (kind 1) where it is placed, model = kind
(`UnitKind::from_person_model`: 1 wildman, 2 brave, 3 warrior, 4 preacher, 5 spy, 6 firewarrior, 7 shaman; 8 is the
angel of death, in no level; only 2 spies in all 41 levels), owner =
tribe (255 = none: wildmen). Shamans spawn at their reincarnation site instead (one per site, ids first). Level 10:
5 braves, 4 warriors, 4 preachers for the player, 5 warriors, 2 preachers, 4 firewarriors for green, 20 wildmen.
In the files, persons stand at a cell centre (4978 of 5271) or anywhere, and carry no extra data (no angle).
Wildmen placed in the ring of cells around a reincarnation site (`ReincarnationSite::welcomes`, 1 cell either
axis) are that tribe's first followers: they start as its braves. In the original they are converted for free when
the shaman appears, no Convert spell needed (level 2: the 8 around blue's site; most levels place 6 or 8 per
tribe this way, level 3 has 12). The conversion animation is not drawn.
Wildmen are neutral (not selectable), drawn with their original anims (0 walk, 1 stand, 3 sit, 31 flung, 47 down)
or else a generated figure in a hide with a mane, or the brave's rendered sheets in a hide colour.

## Unit kinds (simulation: walking only)
`UnitKind::ALL`: Shaman, Brave, Warrior, Preacher, Spy, Firewarrior, built with `Unit::new(id, owner, kind, pos)`.
`max_health` is the original's `LIFE_<P>` (constants.md, fixed values until the file is parsed); wildmen have no
constant and take a brave's. Flat-ground `speed` in world units per tick is a placeholder:

| Kind | Health | Speed |
|---|---|---|
| Shaman | 2000 | 53 |
| Brave, Wildman | 1000 | 53 |
| Warrior | 1800 | 47 |
| Preacher | 1100 | 47 |
| Spy | 600 | 60 |
| Firewarrior | 700 | 50 |

Every kind walks, prays, heals, drowns and dies like the shaman; only the shaman reincarnates (the others stay
`Dead`, lying where they fell). `Command::Order` and spells go to the player's shaman (`GameMap::shaman_of`),
`Command::OrderUnit` to any unit. Sandbox > Units (`GameMap::sandbox_units`): flat island, the player's site and
shaman, three of each other kind in columns to the west, one of each for tribe 1 (red) in a row to the east (all in
view of the starting camera), a pond to the north.

## Chained orders (done: `Unit::queue`, `Command::QueueOrder`)
Any order to any unit can be chained after its current action and the orders already chained (Ctrl + click on the
ground or a camp fire, Ctrl + P / X; later the game itself, for tasks). The queue advances when the unit is idle:
each tick, an idle living unit starts its next chained order still valid (`GameMap::still_valid`: every order
today; task orders will check their task). A unit idle with nothing chained starts a chained order at once. A
direct order (`Command::OrderUnit`) replaces the chain; dying clears it. Open-ended actions end with their
target: going round a camp fire is endless (a tended fire never goes out) until the player puts the fire out,
then the chain goes on; praying will be at a totem and end when it is gone (today it never ends on its own).
Internal moves (stepping off a taken spot on arrival, a fire put out) use `Unit::start`, which keeps the chain.

## Wood (done: braves only)
- A brave carries at most one piece of wood (`Unit::carrying`, 0 or 1).
- `Order::CutTree { tree }` (left click on a tree with braves selected, `GameMap::cut_orders`; other kinds
  sent along walk next to it): each tree takes as many braves as it has wood (`GameMap::tree_claims`: braves
  chopping it or walking to it); one with no wood to spare sends the brave to the nearest tree that has within
  `REFIND_CELLS` (8). The brave stands on the free spot by the tree nearest to him, chops, takes the piece.
- `Order::PickUp { at }` (left click on a wood pile, `GameMap::wood_at` / `pick_orders`): each brave with empty
  hands walks onto the nearest piece within `PICK_RADIUS` (3/4 cell) of the click that nobody else is fetching
  and takes it; the others walk next to the pile.
- `Order::FetchWood`: the nearest wood, a piece on the floor nobody is fetching (walk onto it, pick it up) or a
  tree with wood to spare; the floor wins ties. Tasks will chain it before reassigning the brave to themselves
  (huts-and-training.md, buildings.md); he then works with the piece he carries.
- Wood is put down only when the brave is idle, where he stands, after holding it 3 s (`Action::Holding`, the
  standing-with-wood pose): with nothing chained, a cut piece lands by its tree; chained after a move, at its end. Any order keeps the piece (no drop on a new order). A dead brave drops
  it where he fell; carried into the sea it is lost. Already carrying, a brave ignores `CutTree` / `FetchWood`.

## Inside buildings (done: `Unit::inside`)
A unit is inside a building when it walked in by the door (`Unit::enter`) or stood where the building's walls went
up. Inside, it is not drawn while it stands there; any walk first takes it straight to the door, then on its
route (units.md "Pathfinding", buildings.md "Walking around buildings"). A teleport takes it out.

## Worshipping the shaman (decoration, done: `units/worship.rs`)
Idle followers (braves, warriors, firewarriors, spies, preachers) whose shaman (same tribe) is within 3 cells on each axis (torus)
and idle or walking pray facing her: the Pray pose (original anim 8, kneeling, arms going up and down; the CC0
pray sheet), turned towards her (`octant`), following her as she walks by. Drawn only: in the simulation they stay
Idle, selectable and orderable, nothing goes through `Command`. Wildmen and the shaman never do it; casting, praying, drowning or dying, she gets no worship.

## Standing slots (done: `game_core::slots`)
- Each cell holds 3 x 3 standing spots (`PER_CELL`, about a third of a cell apart: a unit's width).
- A unit never stops on a spot taken by another living unit (where it stands, or where it is walking to) nor
  in a visible tree's cell (a tree takes all its spots): `GameMap::taken_spots`.
- Moving one unit or a group (`GameMap::dispatch`, used for left clicks on the ground): the free spots around the
  target that a walker reaches from it (no crossing water or cliffs, within 12 cells) are taken nearest first,
  each by the closest unit not yet placed (ties by id); the unit on the target spot goes exactly where clicked,
  the others to their spot's centre: a group stands packed shoulder to shoulder. One `Command::OrderUnit` per
  unit, so lockstep peers get the same orders.
- Arriving (end of a walk, or landing from a teleport) on a spot taken meanwhile: the unit walks on to the
  nearest free spot (`GameMap::tick`).

## Pathfinding (done: `game_core::path`)
- Walls: walkers never cross a building's walled cells (`path::Ground`, buildings.md "Walking around buildings").
- `path::Mobility`: Walk (land, not open sea = cell with 4 water corners, not a cliff = a cell edge
  rising more than `MAX_CLIMB` 300; ~5.5% of the original levels' land), Sail (open sea only, boats),
  Fly (anywhere, balloons). Every person walks. A* on the 128² torus, 8 neighbours, no diagonal past an
  impassable side cell, ties broken by cell index (deterministic).
- Fastest, not shortest: a step costs its walking time (100 straight, 141 diagonal on flat ground, divided
  by `slope_factor` of the rise between the two cell centres, the same rule as walking), so a walker goes
  around a hill or along its flank when climbing is slower (sandbox hill: around beats over). Vehicles
  ignore slopes. Heuristic: distance at the top downhill speed (never overestimates).
- The cell path is straightened into legs (start, turning points, exact target): a leg is kept when every
  cell it crosses is passable (exact grid traversal, both side cells where it passes through a corner),
  it is at most 24 cells long, and walking it (slope sampled every 64 units) is no slower than the cells
  it skips (+5%).
- `Heightmap::revision` is bumped on every write; a walking (or stranded) unit replans on the next tick
  when it changed (spells, editor, site levelling). A step that would still end in the sea replans cell
  by cell through cell centres.
- The start cell may be impassable (ground raised into a cliff under her): she can step off it.
- `path::nearest_reachable`: the cell a mobility reaches closest to a goal (walkers boarding a boat,
  boats unloading at a shore; vehicles to come).

### Locked units
No order of the player reaches a unit (`GameMap::locked`, `shaman_locked`: unit orders, chained orders, the
shaman's own orders, spells and Teleport are ignored) while it is:
- flung (`Action::Tumbling`) or off the ground (`motion.height` set), `Unit::locked`: it is not hovered either, and
  it leaves the selection. Picking only takes a `PickableUnit`, which only `Unit::pickable` makes, None while
  locked: hover and selection (`selection::OnScreen::new`) cannot reach such a unit;
- inside a vault of knowledge, from the moment its full gauge sends her in until she is out by the door
  (done, worship.md), so nothing breaks its sequence;
- inside a prison, held there until freed (to do: levels where she starts imprisoned; the prison holds her).

## Physics (done: `game_core::physics`)
A unit off the ground (`Motion::height` set) moves freely, every tick before its action (`Unit::tick`):
- `physics::step`: the position moves by the velocity (x, z in world units, wrapping), the height by its vertical
  speed, then gravity takes `GRAVITY` (32, the original's per turn) off the vertical speed. All integers.
- At or below the ground under its new position (`Heightmap::height_at`), it lands: `height` None, velocity zero,
  a `Touchdown` with its vertical speed (for the fall damage to come). Higher ground in its way stops it there too.
- On the ground nothing moves it yet (rolling and friction to come). A tumbling unit still on the ground goes back to
  Idle.
- Example: pushed up at 98 it rises 3 ticks to about 200 above where it left, and lands 8 ticks (0.67 s) later.

## Shaman on screen (client, `units/`)
- `SimClock` runs `GameMap::tick` at a fixed 10 Hz; positions glide between the last two ticks (moves over a cell in one tick, teleport or reincarnation, are not glided).
- Each unit is a `Grounded` sprite quad (1 px = 1/88 cell: standing ~0.39 cell, half a site stone; feet at the
  anchor) turned to face the camera, uploaded Scale2x-upscaled x4 and filtered linearly (no blocky pixels),
  with a health bar over the head (green -> yellow -> red) shown while the unit is alive and selected, or under the
  mouse for the player's own units (`hover::Hoverable::health`).
  Feet on the ground right under the unit (`Grounded` with no footprint), sprite and bar raised to its `motion.height`
  off the ground (terrain height units at the terrain's scale, glided between ticks, `drawn_lift`; the shadow stays
  on the ground). Sprite and bar are drawn pulled
  0.6 cell towards the camera along the eye-feet line and shrunk to match (`toward_eye`): same picture on
  screen, but slopes and bumps around the feet no longer cut the legs; real hills in front still hide her.
  The original has no health bar: low-health units get a spinning star/crown over the head (to do). The panel
  preview always shows the shaman's health.
- Dust (`units/dust.rs`, cosmetic, real-time): when a unit stops landing, 10 soft generated motes spread 0.4 cell
  around its feet, rise 0.1, grow and fade over 0.8 s, facing the camera and pulled towards it like the sprites.
- Pose from the action: Idle, Walk, Pray (kneeling, original anim 93), Cast (jump), Drown (original anim 40 for followers: the body lying, its spirit rising; tumbling otherwise), Fall (dying, then lies still while dead), Chop, CarryWalk, CarryIdle (braves cutting and carrying wood: original anims 11, 9, 10; other kinds, and the CC0 sheets and generated figures, which have no wood poses yet, show idle / walk instead, `Pose::fallback`), Stranded (arms up: frame 1 of original anim 12 for every follower, a stand-in, the real anim is unknown; wildmen and the shaman show their idle; the CC0 sheets loop their own arms-up `stranded` sheet).
  Timed actions (cast, dying) play once in step with the simulation, others loop.
- View direction: `facing * 45deg - camera yaw`, rounded to the 8 drawn directions (0 front, 2 screen right, 4 back).
- Art: with the original files, the shaman animations of `VSTART/VFRA/VELE` + `HSPR0-0.DAT` (see animations.md),
  per tribe; otherwise (or `--no-original`) the open-source witch (Quaternius, CC0) rendered into
  `assets/units/shaman.png` atlas, loaded by `units/sheets.rs` (see unit-art.md); `units/procedural.rs` can still draw a ~34 px pixel-art shaman in the tribe colour
  (feather headdress, staff) for every pose and direction (front / side / back, left ones mirrored). With the original files the other kinds
  use their original animations too (`art::Originals`, `original_anim`: tribesman body + outfit layer, the preacher too, see
  animations.md); otherwise they use rendered CC0 sheets (unit-art.md), else generated figures (`UnitSprites`, per kind and tribe), told apart by headgear, held item and clothes: brave
  (bare chest, hair tuft, empty hands), warrior (horned helmet, club), preacher (pointed hood, long robe, book), spy
  (dark cloak and cowl, dagger), firewarrior (red cone hat, flame in hand).
- Selection (`units/selection.rs`, player 0's living units): left click on a unit selects it, Ctrl+click adds or
  removes it; a left drag (over 6 px) draws a whitish box and selects the units whose middle is inside (Ctrl adds);
  right click clears. On the map the shaman is selected like any unit (click, Ctrl, box); clicking her panel
  preview selects her alone (and looks at her). Selecting a vehicle or building will not select the people inside.
  The cursor shows the selected count when more than one. Dead units leave the selection.
- Orders go to each selected unit as `Command::OrderUnit`: left click on the ground walks there
  (`grounded::pick_ground`), X stops. Units never pray anywhere: only at a totem or a vault of knowledge
  (`Order::Worship`, worship.md). C (cast selected spell) makes the player's shaman jump,
  Space looks at her.
- Dev: `SHAMAN=walk|cast|drown|teleport|worship [SHOT_FRAME=n] just shot out.png` orders her at start to check a pose.

## Reincarnation site (done: data + rendering)
- `game_core::site::ReincarnationSite { owner, x, z }`, stored in `GameMap::sites` (one per tribe, sorted by owner).
- Original levels: built at the tribe's shaman thing (person model 7). Tribes without a shaman get no site
  (some campaign AI tribes; levl2025 has none for the player).
- Generated maps: tribe 0 on low inland ground, tribe 1 on the land cell farthest away on the torus.
- Spawn ground: when a shaman spawns (map load = first spawn; respawns once units exist),
  `flatten_for_spawn` levels the terrain: height points within 3 cells of the centre take their
  average height (at least `MIN_SPAWN_HEIGHT` = 32, so a flooded site becomes land again), a ring out
  to 4 cells is blended halfway. Integer-only, applied in owner order (deterministic).
- Fixed and indestructible: no `Command` moves or removes it. `spawn_point()` is where the shaman
  appears at start and after death.
- Rendered as 8 pillars on the original's octagon of cells (`sites::SLOTS`: 3 cells out on the axes, (2, 2) on the
  diagonals, each on its cell's centre, turned to the centre, pop3-rev-analysis.md "Reincarnation sites"); the
  camera starts on the player's (tribe 0) site.
  With the original files: the reincarnation stone (object 30) in the owner's tribe colour, textured from the
  level theme's `bl320` atlas (see objects.md), theme 0 on maps without one (sandboxes). Without them or with `--no-original`: plain blocks
  tinted with the tribe colour (no totem: the shaman stands at the centre). Stones face the centre (glyph side inward, as in the game).

## Standing on the ground (client)
Anything placed on the map (site stones, buildings, trees, units) is made of `grounded::Grounded`
parts: each part has its own cell position and footprint, and is set on the terrain exactly as the
mesh draws it (same triangle split, same `drop_at` curve), resting on its lowest footprint corner so
it never floats. Multi-part objects (a building's walls/corners) follow slopes instead of staying flat.
