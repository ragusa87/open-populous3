# Architecture

```
crates/
  pop3-format   pure parsers for original files (no engine). Reusable by tools/editor.
  game-core     deterministic simulation: terrain (Heightmap), map, spell, effect, blast, unit, physics, command, vault; query layers
                over the map (occupancy: who fills a building; headcount: units by state and kind, housing).
                Integers only in state. No Bevy.
  game-net      lockstep wire protocol (length-prefixed, hand-written binary) over std TCP.
  unit-atlas    text index of baked unit frames (rectangle and feet per pose, direction, frame).
  unit-baker    Bevy tool: renders the CC0 characters into assets/units atlases (just bake-units).
  game-client   Bevy app. One plugin per concern:
                  world.rs        CurrentMap, LevelList, terrain entity rebuild
                  terrain_mesh.rs pure mesh builder (curvature, colors, normals), unit-tested
                  grounded.rs     parts set on the curved terrain under them (sites, buildings...)
                  game_frame.rs   the game's left-handed frame and its mirror into Bevy (GamePos, GameYaw)
                  original_models.rs original 3D objects -> meshes + theme atlas (optional)
                  generated_buildings.rs embedded generated GLBs -> cell-space coloured meshes + construction frames
                  sim_time.rs     SimNow (the drawn moment, from the map's tick), seeds from ids
                  views.rs        WorldView: one view object per model, keyed by stable id (sync_views)
                  buildings.rs    model choice, construction stages, shaking
                  chimney.rs      a busy hut's smoke, the first WorldView
                  vault.rs        pyramid of knowledge door and top, posed from its phase
                  tooltip.rs      rendered tooltips above buildings: slot rows of generated icons
                  effects.rs      puffs of smoke and dust: one pattern each (chimney, landing dust, sinking totem)
                  sites.rs        reincarnation site markers on the curved surface
                  units/          sim clock, unit sprites (original or generated art), health bars, orders
                  virtual_cursor.rs in-game cursor (system cursor locked), drives edge scroll + UI picking
                  camera.rs       CameraRig (focus wraps on the torus), input, aerial toggle
                  hud/            info text, shaman preview, control tabs (Spells/Build/Stats), view menu
                  editor.rs       edit mode: spells as brushes at camera focus
                  dev.rs          HEADLESS / SCREENSHOT / AERIAL env helpers
```

## The tick
`GameMap::tick` is the one main loop: one call is one turn of the original (12 per second, `time.rs`), and it ticks
each kind of object in a fixed, written order. The original's order inside a turn is not known yet (TODO.md); ours:
1. walls (`update_walls`), then trees (`Tree::tick`);
2. effects (`tend_effects`): before anything they act on, so units react the same turn;
3. camp fires, vaults, totems;
4. coordinators (docs/specs/coordinators.md): scans that start them, their turns, the finished ones removed, their
   events, then the units whose coordinator is gone released, so a coordinator ending now is gone when its units tick;
5. units (`Unit::tick`), then their events (wood, work, doors) in unit order;
6. chained orders, building work, wood put down, people counted inside, mana (`ManaPool::tick`, each player's with
   its spell book), the gone units removed.

Rules for every object that ticks:
- **One typed list per kind** on `GameMap` (`trees`, `units`, `effects`...), no trait objects. Each kind has its own
  `tick(&mut self, now, ...)`.
- **Each object owns its rhythm.** Periodic work runs on the original's global turn masks: a `time::Every` constant
  checked with phase 0 (`GROW_EVERY.fires(now, 0)` is the original's `turn & 15 == 0`), so it fires on the same
  turns as in the original, for every object at once. A length is a `time::Countdown` held by the object. Each
  constant cites the original's constant name or code address (pop3-rev-analysis.md), or says it is our guess.
- **An object changes only itself.** What it does to others comes out as events, and the map applies them in order,
  after the loop over that kind. The map's handlers are the only code that touches two objects at once. A change to
  the world (damage, a push, terrain, a new effect...) is a `event::WorldEvent`, shared by every kind that makes it,
  each variant with one rule (`GameMap::apply_event`); they run in emission order, and the events applying one
  causes run after the whole batch, never in its middle (`event::run_in_order`). `UnitEvent` stays for what only
  a unit's own outcome means (wood, work, doors, death).
- **Stable ids.** Units, buildings and effects get an id from `GameMap::new_id` (via `spawn`, `add_building`,
  `add_effect`), never given twice, so each list stays sorted by id and lookups (`unit`, `building`) are binary
  searches. New kinds use typed ids (`id::Id<T>`, from `GameMap::new_id_of`, the same counter): an `Id<Effect>` does
  not fit where another kind's id is expected. They are kept in an `id::Registry<T>` (sorted by id, iterated in id
  order); effects are. Units and buildings still hold a raw `u32` (TODO). Whoever holds an id finds None once that object is gone: it has ended. Indices are only valid inside
  one turn.
- **Coordinators** (docs/specs/coordinators.md): an object that coordinates several units (a fight, a sermon) is a
  coordinator, linked one way by the units' actions and ticked before them.
- **Randomness** is `GameMap::rng` (`Lcg`), handed to the objects that draw, in the loop's order. It is seeded
  per game (`GameMap::seeded`), never by game-core itself: the client draws a seed for each new game
  (`world::new_game`, from the clock, or `GAME_SEED`), and in multiplayer the host sends it to every peer
  (multiplayer.md `Start`).
- **No visuals in game-core.** Timing the client needs (a phase, a countdown) is read-only state on the object;
  smoke, rocks and shakes are drawn by the client from it.

### Spell effects
A spell is a recipe of components (pop3-rev-analysis.md "From cast to effect"): later a shot flies, then on arrival
effects are added. An `Effect` (`effect.rs`, kept in `GameMap::effects`, a `Registry<Effect>` that holds its id) has the caster's tribe, a position, `left` (a `Countdown`) and
an `EffectKind` variant carrying only its own data (`SimpleBlast`). `tend_effects` takes the list out of the map
(`std::mem::take`), so each `Effect::tick(now, &map, &mut rng)` reads the whole map without cloning it and returns
`WorldEvent`s; finished effects are dropped (order kept), the list is put back, then the events apply in order:
- `Damage` (with the attacker's tribe, `from`) goes through `GameMap::damage`, the one place for immunities (shield,
  the caster's tribe, later angels and friendly-fire rules per `DamageKind`), which calls `Unit::take_damage`;
- `Push` flings the unit (`Unit::fling`) unless it is shielded;
- `SpawnEffect` adds a new effect, which first ticks on the next turn;
- `Terrain` raises or flattens ground; `Panic` makes the unit run off (`Unit::panic`); `SetFire` burns a building; `BurnTrees` the trees of a cell;
  `Raid` is routed, without behaviour yet.
An effect never holds another object's state: a unit it affects keeps its own countdown (panic, on fire) and
finishes it alone, even after the effect has ended.

To add a spell: add an `EffectKind` variant (or reuse one) whose `tick` follows the original's component (its
rhythm as an `Every`, its length as `left`), returning events; add a `DamageKind` if it hurts differently; add the
effect where the spell lands (`GameMap::apply`, later on a shot's arrival); test it on a small map (`effect.rs`
tests show how).

## Client: boot jobs
Heavy loading never holds the first frames: it is a `boot::BootJob` (`boot/mod.rs`), whose `run` does the CPU work on
a worker thread (`AsyncComputeTaskPool`) from an `Input` copied out of the world, reporting `Progress` steps, and
whose `install` puts the `Output` in the world on the main thread (GPU uploads, resources). `BootJobPlugin::<J>`
registers one: started on entering `AppState::Loading`, its task held in its own typed slot, installed when done;
`BootStatus` is what the loading screen shows. A job's CPU half takes plain data: for the unit sprites,
`units::prepare_frame` makes a `PreparedFrame` (the only way to get one) and `units::upload_prepared` uploads it.
Assets Bevy loads itself (glTF scenes) are pushed to `boot::TrackedAssets` and waited for too.

## Client: world visuals
Two clocks (`sim_time.rs`):
- **Simulation time** for everything drawn in the world: units, smoke, dust, flames, nods, shakes, later spell
  effects. The client draws the world at `SimNow`, set each frame by `units::run_ticks` from the map's tick counter
  (`GameMap::now`), the ticks per step of the last step and `SimClock::alpha` (one step behind the map, like the unit
  glides). So it stops while paused, scales with the game speed, jumps when several ticks run in one frame, and
  a client that resyncs, rejoins or replays draws the same picture from the simulation state alone. No visual
  state goes over the network.
- **Real time** (`Res<Time>`) for UI, camera and mouse feedback, and debug: blinking (population, tooltip bars),
  tooltip timing, the virtual cursor, camera flights and movement, a hovered tree's reaction, the debug logs. They
  work while paused. `SimClock::steps_due` also reads it: it is what runs the simulation.

Rules for a world visual:
- **Computed, never accumulated.** A pose is a pure function of when it started and the drawn moment
  (`SimNow::secs_since`), never `x += v * dt`, which drifts on pause and cannot be rebuilt after a jump. Ages are
  whole ticks first, then a fraction; loops are taken modulo on whole ticks (`SimNow::secs_in`, a period of whole
  ticks holding whole loops), so long games keep their precision.
- **Seeded by stable ids.** A visual's randomness or phase comes from its model's id (`sim_time::seed`), never from
  an entity index or a list position, so the same game always looks the same.
- **One view object per model** (`views.rs`). A model in game-core is drawn by one view type implementing
  `WorldView`: which models it draws (`models`), their stable id (`id`), building the view (`spawn`) and posing it
  at the drawn moment (`update`). Everything about drawing that kind lives in its view's file. The generic
  `sync_views::<V>` spawns a view when a model appears, updates it every frame, and despawns it once its id is
  gone (or the game changed, `SimClock::game`: ids restart on a new map). Views are keyed by id, never by list
  index. Each view type has its own plugin, so adding or removing a kind of visual is one line in `main.rs`. First
  view: `chimney::ChimneyView` (a busy hut's smoke); `sparkles::SparkleView` draws the effects reshaping the ground. Older visuals still index their models by position (units,
  building views, totems): they move over as their models get ids or are removed.

For later, spell effects: each will be a game-core `Effect` with its id, each kind drawn by its own view
(`VolcanoView`, `SwarmView`...) through the same trait and `sync_views`. Timing rules stay in game-core, read by
the views, never hard-coded in the client: e.g. a future `Effect::volcano_phase()` (rumbling, rising, erupting,
flowing) that `VolcanoView` reads to start the camera shake and the rock spray.

## Handedness
The original renders with Direct3D (pop3-rev reverses `D3DPopTB.exe`): its world is left-handed, x right, y up,
z away from the viewer (the game's `XPos`/`ZPos` on the ground, `YPos` height). The files, `game-core` and the
original 3D objects keep that frame; Bevy is right-handed (z towards the viewer), and drawing the same numbers
there gives the mirror image (on level 3 the totem stood left of the hut seen from the site instead of right).
The client mirrors z at the boundary only, in `game_frame.rs`: a `GamePos` (position or offset in the game's frame)
becomes a render `Vec3` with z negated, a `GameYaw` (a turn in the game's sense) a `Quat` turning the other way, and
mirrored meshes (original objects, flames, our building kit, authored in the game's frame) reverse their triangles
(`mirrored`) to keep their fronts. The simulation never sees it. Camera yaw, eye and billboards are render-space.

Rules:
- `pop3-format` and `game-core` never depend on Bevy: they must run in tests, servers and tools.
- Rendering reads simulation state, never the opposite.
- Simulation changes only through `Command`s, so the same inputs replay identically on every peer.
- Logic lives in pure functions (`terrain_mesh::build`, `spell_for_key`, `CameraRig::move_by`) with tests;
  Bevy systems stay thin.
