# AGENTS.md

Guidance for coding agents working on this repo (Rust + Bevy 0.19 Populous-like POC).

## Licence and original data
The project must stay MIT, open-sourcable as a whole. *Populous: The Beginning* belongs to Bullfrog / Electronic
Arts: its files serve only to understand how the game works (formats, rules, numbers) and to be loaded at runtime
from the user's own install.
- Never commit original data or anything reproducing it: models, textures, palettes, sprites, sounds, level
  dumps, renders or screenshots made with the original files, exported viewers. Keep such outputs in a scratch dir.
- Never quote original texts (level names, briefings, messages, UI strings) in code, tests, docs, commits or
  TODO: refer to them by their text number (docs/specs/language.md).
- Never look at a screenshot or video of the original game running, for any purpose (not to check art, layout or
  behaviour either), and never ask for or fetch one. Renders of our own engine loading the user's original files
  (`just shot` with an install) are fine to check that they load right; they are never committed.
- Facts from the community tools (README "References") yes, their code no.
- Everything shippable works with `--no-original` (generated or CC0 content, our own texts).

## Before starting
Read `README.md`, `TODO.md` and `docs/` (`docs/README.md`, `architecture.md`, `roadmap.md`, then the `docs/specs/*` matching the task).

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
  `FOCUS=x,z` (cells), `DISTANCE`, `PITCH`, `YAW` (degrees), `SHOT_FRAME`, `SHAMAN=walk|teleport|worship|...`, `BRAVES=cut|carry|totem:N`, `HOVER=unit:N|wood:N|tree:N|totem:N|building:N`, `VAULT=progress:N|granted:T|spent`, `TAB=spells|build|stats`, `BLUEPRINT=temple@64,70`, `BUILD=hut@90,62 BUILD_TICKS=150` (plan built by all the player's braves), `TUMBLE=air|ground` (the player's units tumble; add `GAME_SPEED=0` to hold them in the air) set up the shot.
  Level paths with spaces or quotes break `just shot`: run `HEADLESS=1 SCREENSHOT=out.png cargo run -p game-client -- "<level>"`.
- `POP3_START=menu|game|sandbox-walk|sandbox-units|sandbox-buildings|sandbox-worship`: skip the main menu or open it (`just shot` starts in the game by default).
- `GAME_SPEED=0|1|2|4|8`: start paused or sped up (P and Shift +/- in the game).
- `POP3_DEV=0`: start with dev mode off (original keymap, no info line or camera readout); on by default.
- `just run-generated` / `--no-original`: no original files read at all (use for anything shippable).
- `just level-info file.dat`: dump parsed level.
- Original files: `$POP3_INSTALL`, else auto-detected (Wine prefixes, `C:\Program Files*\Bullfrog\*`) by `pop3_format::install`; `--no-original` skips it. Read-only, never modify or ship them.

## Rules
- Keep logic in small pure functions with unit tests next to them; Bevy systems stay thin.
- Simulation must stay deterministic: integers, `map::Lcg` for randomness, no time/HashMap-order dependence.
  State changes go through `Command` so lockstep replays match.
- A `Mesh` without vertices reaching the renderer, even hidden, makes Bevy 0.19 log "Use-after-free ... unallocated key": make the mesh when there is data, or fill it the frame it is added (terrain, site marks).
- All grid access wraps (`rem_euclid`): the map is a torus.
- Bevy APIs change between versions: check `~/.cargo/registry/src/*/bevy_*-0.19.*` before guessing.
- Never `rm` paths built from shell variables that may be empty. Write temp files to a scratch dir and overwrite.
- Commits: one logical step per commit, short message, no body, no co-author footer. Do not push.
