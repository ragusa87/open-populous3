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
- Memory addresses, struct offsets and function names of the original executable stay in
  docs/specs/pop3-rev-analysis.md only: elsewhere cite its section ("Fight" [exe]).
- Never look at a screenshot or video of the original game running, for any purpose (not to check art, layout or
  behaviour either), and never ask for or fetch one. Renders of our own engine loading the user's original files
  (`just shot` with an install) are fine to check that they load right; they are never committed.
- Clean room: reverse engineering (disassembly, tracing, live memory reads, running the original) happens only in
  separate spec sessions, whose only output is docs/specs/pop3-rev-analysis.md and its tools. A coding session never
  reads, disassembles or runs the original executable and never traces anything itself: it implements from the docs.
  A fact the docs lack becomes a question in TODO.md for a spec session, or a guess tagged as such.
- Facts from the community tools (README "References") yes, their code no.
- Everything shippable works with `--no-original` (generated or CC0 content, our own texts).

## Before starting
Read `README.md`, `TODO.md` and `docs/` (`docs/README.md`, `architecture.md`, `roadmap.md`, then the `docs/specs/*` matching the task).
If `docs/plans/` has a plan for the task, read it too: it is the current approach and task breakdown, ahead of whatever
the matching spec still says.

## Layout
- `crates/pop3-format`: pure parsers of original game files. No Bevy, no game logic.
- `crates/game-core`: deterministic simulation (terrain, map, spell, unit, command). No Bevy, no floats in state.
- `crates/game-net`: lockstep wire codec over TCP. Depends on game-core only.
- `crates/unit-baker`: renders `assets/3d/characters` into `assets/units` atlases (`just bake-units`); `crates/unit-atlas` is their index format.
- `crates/game-client`: Bevy app, one plugin per concern (world, camera, hud, editor, dev).
- `docs/`: architecture, roadmap, `docs/specs/*`. Update the matching spec when behaviour changes.
- `docs/plans/`: WIP implementation plans, see "Plans" below. Gitignored (only `.gitignore` itself is tracked):
  local working files, never committed.
- `TODO.md`: what is left to implement. Tick/remove items in the commit that does them, add what you leave undone or discover.

## Plans
A plan (`docs/plans/<topic>.md`) is scratch, not documentation: the approach and step breakdown for a task too large
for one sitting, kept so the same working copy can pick up where a prior session left off. It is gitignored and
never committed — it carries no history and is not shared by cloning the repo.
- Start one before a multi-commit feature or refactor: what changes, in what order, open questions, which
  `docs/specs/*.md` and `TODO.md` entries it will touch when done.
- Update it as you go — ticking steps, revising the approach when reality disagrees with the plan — instead of
  letting it drift stale. A plan that no longer matches what you're doing is worse than none.
- Facts that will outlive the task (final behaviour, formats, numbers) belong in the matching `docs/specs/*.md`, not
  in the plan: move them over as each piece lands.
- Delete the plan file once its task is fully committed. Nothing enforces this (it's gitignored, so it never shows
  up as a diff) — leaving it behind just litters the working copy with stale instructions for next time.
- If the task took real time or iteration (a multi-session investigation, several false starts, a bug hunt worth
  remembering how it was found) — worth keeping as a record, not just deleting. Move it to `docs/archived-plans/
  <topic>.md` instead: that directory is tracked (committed, not gitignored, unlike `docs/plans/`), so commit it
  as its own small step. Before moving it:
  - Add a short header noting it's archived (resolved, read as history not current state) and pointing at the
    final outcome (commits, the spec/TODO entries it fed) so a reader doesn't stop at a stale "not yet decided".
  - Make it standalone: it must read correctly with no access to `docs/plans/` (gitignored, and the original file
    will be gone) — fix any internal cross-reference to the plan's own old `docs/plans/<topic>.md` path, and
    correct (don't silently rewrite) anything the plan said that later turned out wrong, the same way
    `pop3-rev-analysis.md` appends corrections rather than editing history away.
  - Update every other tracked file that pointed at `docs/plans/<topic>.md` (`grep -rl` for the path) to the new
    `docs/archived-plans/<topic>.md` location, so nothing tracked references a path that no longer exists.
  A plan that was just scratch bookkeeping for a routine task (most of them) still gets deleted outright — archive
  the ones worth a reader finding again, not every plan by default.

## Commands
- `just test` (or `cargo test --workspace`): must pass before committing.
- `just run [levl.dat|dir]`: windowed game. Avoid it as an agent: it opens a window and steals focus.
- `just shot out.png [level]` / `AERIAL=1 just shot out.png`: headless offscreen render, then exits. Use this to check visuals.
  `FOCUS=x,z` (cells), `DISTANCE`, `PITCH`, `YAW` (degrees), `SHOT_FRAME` (frames after loading, 10 by default; `SHOT_BOOT=1`: from launch, to shoot the loading screen), `SHAMAN=walk|teleport|worship|...`, `BRAVES=cut|carry|totem:N`, `HOVER=unit:N|wood:N|tree:N|totem:N|building:N`, `VAULT=progress:N|granted:T|spent`, `TAB=spells|build|stats`, `BLUEPRINT=temple@64,70`, `AIM=blast` (a spell selected as if picked: shows its range ring), `BUILD=hut@90,62 BUILD_TICKS=150` (plan built by all the player's braves), `TUMBLE=air|ground` (the player's units tumble; add `GAME_SPEED=0` to hold them in the air), `BLAST=x,z BLAST_TICKS=N` (another tribe's blast, N ticks run; with `GAME_SPEED=0`), `BRIDGE=x,z BRIDGE_TICKS=N` (the player's land bridge from the shaman, N ticks run; with `GAME_SPEED=0`: its sparkles), `FLATTEN=x,z FLATTEN_TICKS=N` (a flatten on that height point), `ERODE=x,z ERODE_TICKS=N` (an erosion centred on that height point, N ticks run), `LIGHTNING=x,z LIGHTNING_TICKS=N` (another tribe's lightning there), `SWAMP=x,z SWAMP_TICKS=N` (another tribe's swamp there, N ticks run), `CONVERT=x,z CONVERT_TICKS=N` (the player's Convert effect there, with a wildman placed next to it), `HYPNOTISM=x,z HYPNOTISM_TICKS=N` (the player's Hypnotism effect there, with an enemy placed next to it), `VOLCANO=x,z VOLCANO_TICKS=N` (the player's volcano there), `FIRESTORM=x,z FIRESTORM_TICKS=N` (another tribe's firestorm there), `EARTHQUAKE=x,z EARTHQUAKE_TICKS=N` (another tribe's earthquake there), `SWARM=x,z SWARM_TICKS=N` (another tribe's swarm there, N ticks run), `EXPLODE=building:N EXPLODE_TICKS=N` (building N blows up, as if N ticks ago; with `GAME_SPEED=0`), `TRAIN=stored/cost[:short]` (a brave of the player inside each of their training units, making something with that much mana stored), `SELECT=own` (all the player's people selected), `CREW=building:N CREW_TICKS=N` (the player's braves on foot sent into building N, N ticks run), `VEHICLE=boat@x,z[:n]` (or `balloon`, `;` between several: the player's vehicle there with n braves aboard; `VEHICLE_TO=x,z VEHICLE_TICKS=N` sends them there and runs N ticks), `ATTACK=nearest` (the player's fighters attack the nearest enemy), `MENU=shortcuts` (paused on the pause menu's Shortcuts page), `SHOT_RESOLUTION=WxH` (overrides the default 1280x720),
  `DEBUG_OVERLAY=1` (dev overlay: fight reaches on the ground; F4 in dev mode) set up the shot.
  Level paths with spaces or quotes break `just shot`: run `HEADLESS=1 SCREENSHOT=out.png cargo run -p game-client -- "<level>"`.
- `just video [level]`: the same headless setup recorded as a video, `VIDEO=secs` (5 by default) from `SHOT_FRAME`,
  always to `target/video.mp4` (git-ignored; frames piped raw into `ffmpeg`, which must be installed). Use it for animations.
- `just profile [level]`: the same headless run built with Bevy's `trace_chrome` (cargo feature `profile`, off in normal
  builds), `VIDEO=secs` (2 by default): `target/trace.json` (ui.perfetto.dev) and its most expensive spans printed
  (`tools/trace_top.py target/trace.json 40 game_client` for our systems only). Slow: minutes, and a large file.
- `POP3_START=menu|game|sandbox-walk|sandbox-units|sandbox-buildings|sandbox-worship|sandbox-spells|sandbox-vehicles|sandbox-bridge`: skip the main menu or open it (`just shot` starts in the game by default).
- `GAME_SPEED=0|1|2|4|8`: start paused or sped up (P and Shift +/- in the game).
- `GAME_SEED=N`: the game's random seed (drawn per game otherwise), to replay a game the same way.
- `POP3_DEV=0`: start with dev mode off (original keymap, no info line or camera readout); on by default.
- `just run-generated` / `--no-original`: no original files read at all (use for anything shippable).
- `just level-info file.dat`: dump parsed level.
- Original files: `$POP3_INSTALL`, else auto-detected (Wine prefixes, `C:\Program Files*\Bullfrog\*`) by `pop3_format::install`; `--no-original` skips it. Read-only, never modify or ship them.

## Rules
- Keep logic in small pure functions with unit tests next to them; Bevy systems stay thin.
- Simulation must stay deterministic: integers, `map::Lcg` for randomness, no time/HashMap-order dependence.
  State changes go through `Command` so lockstep replays match.
- The tick (docs/architecture.md "The tick"): a new ticking object is a typed list on `GameMap` with its own
  `tick(&mut self, now, ...)`, its rhythm an `Every` checked with phase 0 and its lengths `Countdown`s (each constant
  cites the original's constant name or its pop3-rev-analysis.md section, or says it is a guess), events out for
  anything it does to others, and one line in `GameMap::tick` at its place in the order. Removable objects get ids from `GameMap::new_id` (new kinds:
  typed `Id<T>` in a `Registry<T>`, `GameMap::new_id_of`).
- An object coordinating several units (a fight, a sermon, a formation) is a coordinator (docs/specs/coordinators.md):
  units link to it one way, it ticks before them and is removed when its situation ends.
- A new spell is a recipe of effects: an `EffectKind` variant reading the map and returning `WorldEvent`s; units
  are hurt only through `GameMap::damage`.
- A game-balance number that exists in `levels/constant.dat` (docs/specs/constants.md) is read through
  `pop3_format::constants::Constants` (loaded from the player's own install, e.g. `GameMap::load_original`;
  never shipped), not hardcoded a second time as a new placeholder. Keep a code-level default for
  generated/no-original mode (written by us, not copied from the real numbers, constants.md "Use here") —
  except where the value has no "our own economy" to protect (ranges, charge counts, timings: just facts),
  in which case the default can be the real number outright. `game_core::spell_book::SpellStats` is the
  precedent to follow for a new group (units life/speed/damage, training costs, wood, hut breeding, ...:
  TODO.md still has these on their own hardcoded numbers).
- World visuals follow simulation time (`sim_time::SimNow`), computed from a start tick and never accumulated per
  frame, seeded by stable ids; UI, camera and mouse feedback stay on `Res<Time>`. A new kind of visual for a
  game-core object is a `views::WorldView` with its own plugin (docs/architecture.md "Client: world visuals").
- A `Mesh` without vertices reaching the renderer, even hidden, makes Bevy 0.19 log "Use-after-free ... unallocated key": make the mesh when there is data, or fill it the frame it is added (terrain, site marks).
- All grid access wraps (`rem_euclid`): the map is a torus.
- Bevy APIs change between versions: check `~/.cargo/registry/src/*/bevy_*-0.19.*` before guessing.
- Never `rm` paths built from shell variables that may be empty. Write temp files to a scratch dir and overwrite.
- Commits: one logical step per commit, short message, no body, no co-author footer. Do not push.
