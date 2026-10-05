# AGENTS.md

Guidance for coding agents working on this repo (Rust + Bevy 0.19 Populous-like POC).

## Layout
- `crates/pop3-format`: pure parsers of original game files. No Bevy, no game logic.
- `crates/game-core`: deterministic simulation (terrain, map, spell, unit, command). No Bevy, no floats in state.
- `crates/game-net`: lockstep wire codec over TCP. Depends on game-core only.
- `crates/game-client`: Bevy app, one plugin per concern (world, camera, hud, editor, dev).
- `docs/`: architecture, roadmap, `docs/specs/*`. Update the matching spec when behaviour changes.

## Commands
- `just test` (or `cargo test --workspace`): must pass before committing.
- `just run [levl.dat|dir]`: windowed game. Avoid it as an agent: it opens a window and steals focus.
- `just shot out.png [level]` / `AERIAL=1 just shot out.png`: headless offscreen render, then exits. Use this to check visuals.
- `just run-generated` / `--no-original`: no original files read at all (use for anything shippable).
- `just level-info file.dat`: dump parsed level.
- Original levels: `$POP3_LEVELS` or the Wine install path in `world.rs` (`DEFAULT_LEVELS_DIR`). Read-only, never modify or ship them.

## Rules
- Keep logic in small pure functions with unit tests next to them; Bevy systems stay thin.
- Simulation must stay deterministic: integers, `map::Lcg` for randomness, no time/HashMap-order dependence.
  State changes go through `Command` so lockstep replays match.
- All grid access wraps (`rem_euclid`): the map is a torus.
- Bevy APIs change between versions: check `~/.cargo/registry/src/*/bevy_*-0.19.*` before guessing.
- Never `rm` paths built from shell variables that may be empty. Write temp files to a scratch dir and overwrite.
- Commits: one logical step per commit, short message, no body, no co-author footer. Do not push.
