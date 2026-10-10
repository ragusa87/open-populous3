# Multiplayer: deterministic lockstep

The original game used lockstep: peers exchange inputs only and run the same simulation.

## Rules for the simulation (game-core)
- No floats, no `HashMap` iteration order, no system time, PRNG = `map::Lcg` seeded by the host.
- Advance in fixed ticks, 12 per second, one per original turn (`game_core::time`); a tick runs only when all
  players' turn for it arrived.
- Commands issued in tick N are scheduled for N + delay (2-3) to hide latency.

## Schedule (`game_core::schedule::Schedule`, done)
Input never touches the map: the client issues commands to its `GameSchedule` (`units/mod.rs`), and
`run_ticks` steps the map (`GameMap::step`: that tick's commands, then the tick).
- `issue(command)`: a local command waits in `pending`.
- `close_local(now)`, before running tick `now`: the local turn of `now + delay` closes with the pending
  commands, and is returned to be sent to the peers (`Message::Tick`); once per tick, so a stalled tick does not
  close it twice.
- `receive(player, tick, commands)`: a peer's turn; a second one for the same tick is ignored.
- `ready(tick)`: every player closed it, or it is one of the first `delay` ticks after the start, which nobody
  could have sent anything for. Not ready: the client stops stepping until it is (the clock drops that time).
- `take(tick)`: its commands, players in order, each player's in the order issued.
- Single player: `Schedule::single`, one player, no delay: what was issued during a frame applies on the next
  tick; while paused it waits. A new map (new game, sandbox, level switch) starts a new schedule.
- A building plan and the orders sent with it go on the same tick: `GameMap::place_orders` gives the orders the
  plan will get once placed (`build_orders` needs it on the map).
- Dev shots (`dev.rs`) still apply their set-up commands to the map directly.
- Periodically exchange a checksum of the heightmap/units to detect desync.

## Wire format (game-net)
Frame: `u32 LE` length + payload. Payload tag byte:
- `0 Hello { player: u8 }`
- `1 Tick { tick: u32, n: u8, n x Command }`: a player's turn for that tick, Command = `kind u8, player u8`, then
  - kind 0 `Cast`: `spell tag u8`, cells as i16 x/z pairs (tag 4 Teleport: `u16` x, z in world units);
  - kind 1 `Order` (to the shaman): `order tag u8` (0 MoveTo + `u16` x, z in world units, 1 Pray, 2 Cast, 3 Stop, 4 Campfire + `u16` x, z of the fire's centre + `u8` ring point, 5 CutTree + `u16` x, z of the tree, 6 FetchWood, 7 PickUp + `u16` x, z of the click, 8 Build + `u16` x, z of the building's stored corner, 9 Enter + `u16` x, z of the hut's stored corner);
  - kind 2 `OrderUnit` (to one of the player's units): `u32` unit id, then the order as in kind 1;
  - kind 3 `PlaceCampfire`: `u16` x, z in world units (the fire takes that cell);
  - kind 4 `RemoveCampfire`: `u16` x, z in world units (the player's fire in that cell);
  - kind 5 `QueueOrder` (chained after the unit's current orders): `u32` unit id, then the order as in kind 1;
  - kind 6 `PlaceBuilding`: `u8` building model (`BuildingKind::model`), `u16` x, z of its corner, `u8` facing;
  - kind 7 `CancelBuilding`: `u16` x, z of a point of the plan's footprint.

Implemented: codec + TCP round trip test. To do: host/join, sending and receiving the turns, reconnect, lobby UI.
