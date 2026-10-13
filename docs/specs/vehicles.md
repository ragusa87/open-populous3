# Vehicles (boats and balloons)

Boats sail the sea and its coast, balloons (airships) fly anywhere; each carries a few people. The original's
rules are in pop3-rev-analysis.md "Vehicles" [exe]; this page says what `game_core::vehicle` does, and which
numbers are guesses.

## The object (`Vehicle`, done)
- Kept in `GameMap::vehicles`, a `Registry<Vehicle>` (typed `VehicleId`, the map's id counter).
- `VehicleKind`: Boat or Balloon. Seats 5 / 2, a hard limit (vehicle table +0x8). Mobility `Sail` (cells touching
  the sea: open sea and the coast, `path::touches_sea`) / `Fly` (anywhere, walls ignored). Speed 96 / 80 world
  units per turn [ours, a guess].
- State: `Parked`, `Moving { to }`, `Sinking`, `Rising`, `Stranded` (the exe's states 1-8, without the drift to a
  resting place, state 7, and the whirlwind's spin, state 9: TODO.md).
- `heading` (an `angle::Angle`, from +x towards +z) follows the way it moves; the sinking pitch and the stranded
  roll live in their states.
- `altitude`: the ground under a boat (the sea's surface is 0), `HOVER` 200 over the ground for a balloon (the
  exe's parked height; the moving hover over the highest ground ahead is not done).

## Health is also an abandon timer (done)
`health` (a `Health` of max `LIFE` 5000, `VEHICLE_LIFE_*` of constant.dat) loses 1 every turn and is full again
every turn while the vehicle moves or carries anyone. So an empty, still vehicle falls apart after 5000 turns
(417 s). At 0:
- a boat on the water sinks: 8 deeper and 8/2048 more bow-up a turn (capped at 398), gone at depth 192 (24 turns);
- a boat on land explodes: a simple blast of its tribe where it stood, and it is gone;
- a balloon rises 80 a turn and explodes the same way at altitude 1024.
Whoever is aboard is put out where it is (`GameMap::put_out`): out of a balloon they fall, at sea they drown
(`Unit::tick`).

## Stranded boat (done)
Every 16 turns (`STRAND_EVERY`) a boat whose cell no longer touches the sea (Raise Land, Land Bridge...) is
stranded: everyone is put off, its health becomes 160 + a draw below 128 turns, and it keels over 8/2048 a turn to
a random side, up to 90° (`KEEL_MAX`). Water back in its cell: parked again. Otherwise it explodes when its health
runs out (13-24 s).

## Passengers (done: carried)
`Unit::aboard` names the vehicle carrying a unit, `Vehicle::passengers` its seats in boarding order (the first
drives). Each turn the map drops from the seats those dead, gone or teleported off (`sync_passengers`), and moves
the passengers to the vehicle's position. A unit aboard goes nowhere on its own and never drowns; only a cast goes
on.

## In the turn
`GameMap::tend_vehicles`, after the coordinators and before healing and the units (architecture.md "The tick"):
seats synced, each vehicle's turn in id order (`Vehicle::tick`, its events `PutOut`, `Exploded`, `Sunk` applied
after the loop), passengers carried.
