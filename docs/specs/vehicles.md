# Vehicles (boats and balloons)

Boats sail the water cells, balloons (airships) fly anywhere; each carries a few people. The original's
rules are in pop3-rev-analysis.md "Vehicles" [exe]; this page says what `game_core::vehicle` does, and which
numbers are guesses.

## The object (`Vehicle`, done)
- Kept in `GameMap::vehicles`, a `Registry<Vehicle>` (typed `VehicleId`, the map's id counter).
- `VehicleKind`: Boat or Balloon. Seats 5 / 2, a hard limit (vehicle table +0x8). Mobility `Sail` (water cells only,
  all four corners at sea level: a shore cell with land in it is land for a boat) / `Fly` (anywhere, walls ignored). Speed 96 / 80 world
  units per turn [ours, a guess].
- State: `Parked`, `Moving { to }`, `Sinking`, `Rising`, `Stranded` (the exe's states 1-8, without the drift to a
  resting place, state 7, and the whirlwind's spin, state 9: TODO.md).
- `heading` (an `angle::Angle`, from +x towards +z) follows the way it moves; the sinking pitch and the stranded
  roll live in their states.
- `altitude`: the ground under a boat (the sea's surface is 0); a balloon's `float`, which follows the terrain
  and what stands on it ("Terrain changes" [exe]): each turn it eases towards the highest of, under it and while it
  moves at `LOOK_AHEAD` 256 to 2048 ahead, the ground plus what it
  floats over there (`hover_over`): `HOVER` 200 over bare ground, `HOVER_OVER_BUILDING` 560 over a building (the
  pyramid included; the exe's value), `HOVER_OVER_TREE` / `_TOTEM` 450 over a tree or a totem [guesses]. So it
  rises before a mountain, a building or a tree and comes down after it, never bumping into them; never nearer the
  ground under it than `CLEARANCE` 100 [ours]. Easing (`vehicle::ease`): a quarter of what is left each turn
  (`EASE`), at most `CLIMB_STEP` 32, at least `MIN_STEP` 4, so it speeds up and slows down on a curve, never past the
  height it heads for. The step and the distances are guesses. Drawn, its height glides between turns from where its
  last turn started (`Vehicle::was_at`), its people with it.

## Parking and drifting (done, improvised)
- One vehicle a cell: a vehicle sent where another stands or is going stops in the nearest free cell it reaches
  (`GameMap::move_vehicle`, `berths_taken`).
- Resting places (`vehicle::resting`) [ours, after "Parking and the shore"]: a boat on a water cell next to
  land (`by_the_shore`: where people board it), a balloon anywhere but over the water. Every 64 turns (`DRIFT_EVERY`) an empty parked
  vehicle away from one drifts at speed 8 (`DRIFT_SPEED`, `State::Drifting`) to the nearest free one it reaches
  (`path::nearest_matching`): abandoned boats go back to a shore. The exe's landing flag and its time limits
  on the drift are not followed.

## Health is also an abandon timer (done)
`health` (a `Health` of max `LIFE` 5000, `VEHICLE_LIFE_*` of constant.dat) loses 1 every turn and is full again
every turn while the vehicle moves or carries anyone. So an empty, still vehicle falls apart after 5000 turns
(417 s). At 0:
- a boat on the water sinks: 8 deeper and 8/2048 more bow-up a turn (capped at 398), gone at depth 192 (24 turns);
- a boat on land (an all-land cell) explodes: a simple blast of its tribe where it stood, and it is gone;
- a balloon rises 80 a turn and explodes the same way at altitude 1024.
Damage (`GameMap::damage_vehicle`, `WorldEvent::DamageVehicle`) comes only from another tribe: 50
(`blast::DAMAGE_PERSON`) each turn of a simple blast whose radius holds it, 100 (`firebolt::CELL_DAMAGE`) from a
bolt landing in its cell. As the exe, it only destroys an empty, still vehicle: a moving or manned one is whole
again every turn.
Whoever is aboard is put out where it is (`GameMap::put_out`): out of a balloon they fall, at sea they drown
(`Unit::tick`).

## Stranded boat (done)
Every 16 turns (`STRAND_EVERY`) a boat whose cell is no longer all water (Raise Land, Land Bridge...) is
stranded: everyone is put off, its health becomes 160 + a draw below 128 turns, and it keels over 8/2048 a turn to
a random side, up to 90° (`KEEL_MAX`). Water back in its cell: parked again. Otherwise it explodes when its health
runs out (13-24 s).

## Passengers (done: carried)
`Unit::aboard` names the vehicle carrying a unit, `Vehicle::passengers` its seats in boarding order (the first
drives). Each turn the map drops from the seats those dead, gone or teleported off (`sync_passengers`), and moves
the passengers to the vehicle's position. A unit aboard goes nowhere on its own and never drowns; only a cast goes
on.

## Boarding, driving, unloading (done in game-core)
- `Order::Board { vehicle }` (`GameMap::go_board`): any living person but wildmen walks to the walkable cell
  nearest the vehicle (`path::nearest_reachable`: the land next to a boat on the water by the shore) and, within `BOARD_REACH` (1024
  world units per axis, "within 1-2 cells"), gets on (`board`, `UnitEvent::AtVehicle`). Refused when every seat is
  taken (a hard limit), when it is wrecked or stranded, and when someone of another tribe is aboard. Whoever gets
  on makes it their tribe's: an empty enemy or neutral vehicle becomes theirs. A brave carrying wood puts it down
  where he stood before getting on. It then waits `BOARD_WAIT` (7 turns)
  before moving off. Getting on is a jump (`Action::Boarding`, `BOARD_JUMP` = the vehicle's 7-turn wait): seated at
  once, it is drawn on an arc from where it stood to its seat. Getting off too (`Action::Alighting`): on the ground
  at once, drawn jumping down from the vehicle.
- Anyone aboard drives: a move order to a passenger (or to a shaman aboard) sends its vehicle there
  (`order_aboard`), a stop stops it, other orders are ignored until they get off. `Command::MoveVehicle` does the
  same for a manned vehicle of the player; `GameMap::move_orders` turns a move of a selection into one per vehicle
  plus the walkers' `dispatch`. A point the vehicle cannot reach sends it to the nearest it can
  (`nearest_reachable`, `closest_point_in_cell`): a boat to the shore nearest a point inland.
- `Order::GetOff` (`GameMap::get_off`): that passenger gets off; `Command::Unload` (`GameMap::unload`, the tooltip's
  button, which also selects everyone aboard): everyone aboard. They get off onto free spots of the walkable cell nearest the
  vehicle within `UNLOAD_REACH` (1024 per axis), standing idle; nobody when there is none (`can_unload` false: a
  boat away from the shore, a balloon over the sea). Those left without a spot stay aboard.
- A click with passengers selected on walkable ground right next to their stopped vehicle (parked: a boat by the
  shore; within `UNLOAD_REACH`) gets those selected off (an `Order::GetOff` each; the others stay aboard) instead of
  moving it (`GameMap::gets_off_at`, through
  `move_orders`); anywhere else, or while it moves, the click sends the vehicle there.
- A passenger is out of reach of blasts, lightning, bolts, spotting and challenges (`Unit::in_the_open`); a
  balloon's people stand at its altitude (`GameMap::altitude`), so a shaman casts farther from one. The headcount
  counts them In boat / In balloon.
- A shaman aboard casting out of range is carried there (`GameMap::walk_to_cast`, `Unit::ride_to_cast`): her
  vehicle goes towards the target (a balloon anywhere, a boat as near as the sea lets it), and she casts as soon
  as it is within her range (from a balloon's altitude, farther); a vehicle stopped short gives it up.
- Aboard, the shaman cannot cast a land bridge (it starts at her feet; `GameMap::can_cast`, the cursor shows it
  invalid), unless the player is a god (`shaman_omni`: it starts at the view). Other spells: all allowed for now.

## Vehicle huts (done in game-core)
A boat hut makes boats, an airship hut balloons (`VehicleKind::made_by`). Built with wood like any building; then
[ours, from the player's description, the original's way is not traced]:
- Braves sent into a built vehicle hut (`Order::Enter`, `GameMap::join_crew`) join its crew (`work`), up to `CREW`
  4 [a guess]; others walk to its door. The crew fetches wood to its pile (`Building::stock`) like for a site, until
  the pile and the wood on the way reach the vehicle's cost: 4 pieces for a boat, 3 for a balloon
  (`WOOD_VEHICLE_BOAT1` / `_AIRSHIP_1` of constant.dat, 400 / 300, a piece being 100). Otherwise they work inside.
- With the wood there, each crew member inside adds a turn of work (`Building::launch`, a `Launch`) up to
  `LAUNCH_WORK` 1200 [a guess: 100 s alone, 25 s with 4]. Done: the wood is spent and the vehicle comes out, its
  owner the hut's, heading away from the hut, with the first of the crew inside (lowest id) aboard, out of the crew.
  A boat goes on the open sea nearest the jetty side (local +z) within `LAUNCH_SEARCH` 4 cells, and waits, done,
  while there is none; a balloon just past that side.
- The hut's tooltip: its crew in the people row, the vehicle's wood in the wood row (`occupancy`), the work in the
  bar (`GameMap::launch_progress`). Mana: none (unknown in the original).

## In the turn
`GameMap::tend_vehicles`, after the coordinators and before healing and the units (architecture.md "The tick"):
seats synced, each vehicle's turn in id order (`Vehicle::tick`, its events `PutOut`, `Exploded`, `Sunk` applied
after the loop), passengers carried.

## Client (done: drawn)
`vehicles.rs`, a `WorldView` per vehicle (`VehicleView`): the original boat or airship object (`catalog::BOAT`,
`AIRSHIP`, recoloured for its tribe and remade when the tribe changes) with the theme's material, or with
`--no-original` our stand-in (a wooden hull with a deck in the tribe's colour, a balloon in the tribe's colour over
a basket). Turned by its heading (the original boat's bow is along its own +z), tilted by a sinking boat's pitch
and a stranded one's roll, lifted to its altitude (a balloon's hover, a sinking boat's depth below the surface),
gliding between ticks like the units. People aboard are drawn in it (`units::riding`): on their seat
(`vehicles::seat_offset`: along a boat, side by side in a basket) at its floor, the model's origin (the original
boat's waterline, its keel going below; the airship's basket floor), sitting and pedalling (`Pose::Pedal`, the
original's anim 51; the end of `SitDown` in our sheets) while it is under way (`vehicles::under_way`; still, parked or
waiting after a boarding, the pedalling holds its first frame), hoverable and clickable. Unlike a tower's lookout they are
not pulled towards the camera: the depth test lets the near side of the hull or basket hide their lower body, so
they are seen inside, not in front. Dev shots:
`VEHICLE=boat@x,z[:n]` (`;` between several), `VEHICLE_TO=x,z`, `VEHICLE_TICKS=N` (`dev.rs`).

## Client (done: orders and tooltip)
- Hover: a vehicle is under the cursor anywhere in the box around its model (`hover::HoverTarget::Vehicle`, after
  units and before wood piles), and gets the halo.
- A left click on a vehicle with people selected sends those not aboard to board it (`selection::vehicle_click`);
  with nothing selected, it selects the people aboard the player's vehicle (`crew_click`). Passengers stay
  selected; a click on the ground with passengers selected moves their vehicle (`ground_click` through
  `GameMap::move_orders`).
- Tooltip (`tooltip::vehicle_model`): its name; for the player's own its seats in a people row (a click selects
  one) and the unload button (tooltips.md "Unload button"). A vehicle hut's tooltip shows its crew, the vehicle's
  wood and its work as the training bar, blinking while wood is missing.
