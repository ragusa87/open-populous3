# AGENTS.md

Guidance for coding agents working on this repo (Rust + Bevy 0.19 Populous-like POC).

## Layout
- `crates/pop3-format`: pure parsers of original game files. No Bevy, no game logic.
- `crates/game-core`: deterministic simulation (terrain, map, spell, unit, command). No Bevy, no floats in state.
- `crates/game-net`: lockstep wire codec over TCP. Depends on game-core only.
- `crates/unit-baker`: renders `assets/3d/characters` into `assets/units` atlases (`just bake-units`); `crates/unit-atlas` is their index format.
- `crates/game-client`: Bevy app, one plugin per concern (world, camera, hud, editor, dev).
- `docs/`: architecture, roadmap, `docs/specs/*`. Update the matching spec when behaviour changes.
- `TODO.md`: what is left to implement. Tick/remove items in the commit that does them, add what you leave undone or discover.

## Commands
- `just test` (or `cargo test --workspace`): must pass before committing.
- `just run [levl.dat|dir]`: windowed game. Avoid it as an agent: it opens a window and steals focus.
- `just shot out.png [level]` / `AERIAL=1 just shot out.png`: headless offscreen render, then exits. Use this to check visuals.
  `FOCUS=x,z` (cells), `DISTANCE`, `PITCH`, `YAW` (degrees), `SHOT_FRAME`, `SHAMAN=walk|teleport|...`, `BRAVES=cut|carry`, `HOVER=unit:N|wood:N|tree:N|building:N`, `TAB=spells|build|stats`, `BLUEPRINT=temple@64,70` set up the shot.
  Level paths with spaces or quotes break `just shot`: run `HEADLESS=1 SCREENSHOT=out.png cargo run -p game-client -- "<level>"`.
- `POP3_START=menu|game|sandbox-walk|sandbox-units|sandbox-buildings`: skip the main menu or open it (`just shot` starts in the game by default).
- `just run-generated` / `--no-original`: no original files read at all (use for anything shippable).
- `just level-info file.dat`: dump parsed level.
- Original files: `$POP3_INSTALL`, else auto-detected (Wine prefixes, `C:\Program Files*\Bullfrog\*`) by `pop3_format::install`; `--no-original` skips it. Read-only, never modify or ship them.

## Rules
- Keep logic in small pure functions with unit tests next to them; Bevy systems stay thin.
- Simulation must stay deterministic: integers, `map::Lcg` for randomness, no time/HashMap-order dependence.
  State changes go through `Command` so lockstep replays match.
- All grid access wraps (`rem_euclid`): the map is a torus.
- Bevy APIs change between versions: check `~/.cargo/registry/src/*/bevy_*-0.19.*` before guessing.
- Never `rm` paths built from shell variables that may be empty. Write temp files to a scratch dir and overwrite.
- Commits: one logical step per commit, short message, no body, no co-author footer. Do not push.
