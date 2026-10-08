# Multiplayer: deterministic lockstep

The original game used lockstep: peers exchange inputs only and run the same simulation.

## Rules for the simulation (game-core)
- No floats, no `HashMap` iteration order, no system time, PRNG = `map::Lcg` seeded by the host.
- Advance in fixed turns (e.g. 10 Hz); a turn runs only when all players' `Message::Turn` arrived.
- Commands issued in turn N are scheduled for N + delay (2-3) to hide latency.
- Periodically exchange a checksum of the heightmap/units to detect desync.

## Wire format (game-net)
Frame: `u32 LE` length + payload. Payload tag byte:
- `0 Hello { player: u8 }`
- `1 Turn { turn: u32, n: u8, n x Command }`, Command = `kind u8, player u8`, then
  - kind 0 `Cast`: `spell tag u8`, cells as i16 x/z pairs (tag 4 Teleport: `u16` x, z in world units);
  - kind 1 `Order` (to the shaman): `order tag u8` (0 MoveTo + `u16` x, z in world units, 1 Pray, 2 Cast, 3 Stop, 4 Campfire + `u16` x, z of the fire's centre + `u8` ring point, 5 CutTree + `u16` x, z of the tree, 6 FetchWood, 7 PickUp + `u16` x, z of the click);
  - kind 2 `OrderUnit` (to one of the player's units): `u32` unit id, then the order as in kind 1;
  - kind 3 `PlaceCampfire`: `u16` x, z in world units (the fire takes that cell);
  - kind 4 `RemoveCampfire`: `u16` x, z in world units (the player's fire in that cell);
  - kind 5 `QueueOrder` (chained after the unit's current orders): `u32` unit id, then the order as in kind 1.

Implemented: codec + TCP round trip test. To do: host/join, turn scheduler, reconnect, lobby UI.
