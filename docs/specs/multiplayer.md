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
  - kind 0 `Cast`: `spell tag u8`, cells as i16 x/z pairs;
  - kind 1 `Order` (to the shaman): `order tag u8` (0 MoveTo + `u16` x, z in world units, 1 Pray, 2 Cast, 3 Stop).

Implemented: codec + TCP round trip test. To do: host/join, turn scheduler, reconnect, lobby UI.
