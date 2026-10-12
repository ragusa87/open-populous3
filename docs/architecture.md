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
                  buildings.rs    model choice, construction stages, smoke and shaking
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
4. units (`Unit::tick`), then their events (wood, work, doors) in unit order;
5. chained orders, building work, wood put down, people counted inside, the gone units removed.

Rules for every object that ticks:
- **One typed list per kind** on `GameMap` (`trees`, `units`, `effects`...), no trait objects. Each kind has its own
  `tick(&mut self, now, ...)`.
- **Each object owns its rhythm.** Periodic work runs on the original's global turn masks: a `time::Every` constant
  checked with phase 0 (`GROW_EVERY.fires(now, 0)` is the original's `turn & 15 == 0`), so it fires on the same
  turns as in the original, for every object at once. A length is a `time::Countdown` held by the object. Each
  constant cites the original's constant name or code address (pop3-rev-analysis.md), or says it is our guess.
- **An object changes only itself.** What it does to others comes out as events (`UnitEvent`, `EffectEvent`), and
  the map applies them in order, after the loop over that kind. The map's handlers are the only code that touches
  two objects at once.
- **Stable ids.** Units, buildings and effects get an id from `GameMap::new_id` (via `spawn`, `add_building`,
  `add_effect`), never given twice, so each list stays sorted by id and lookups (`unit`, `building`) are binary
  searches. Whoever holds an id finds None once that object is gone: it has ended. Indices are only valid inside
  one turn.
- **Randomness** is `GameMap::rng` (`Lcg`), handed to the objects that draw, in the loop's order. It is seeded
  per game (`GameMap::seeded`), never by game-core itself: the client draws a seed for each new game
  (`world::new_game`, from the clock, or `GAME_SEED`), and in multiplayer the host sends it to every peer
  (multiplayer.md `Start`).
- **No visuals in game-core.** Timing the client needs (a phase, a countdown) is read-only state on the object;
  smoke, rocks and shakes are drawn by the client from it.

### Spell effects
A spell is a recipe of components (pop3-rev-analysis.md "From cast to effect"): later a shot flies, then on arrival
effects are added. An `Effect` (`effect.rs`) has an id, the caster's tribe, a position, `left` (a `Countdown`) and
an `EffectKind` variant carrying only its own data (`SimpleBlast`). `tend_effects` takes the list out of the map
(`std::mem::take`), so each `Effect::tick(now, &map, &mut rng)` reads the whole map without cloning it and returns
`EffectEvent`s; finished effects are dropped (order kept), the list is put back, then the events apply in order:
- `Damage` goes through `GameMap::damage`, the one place for immunities (shield, the caster's tribe, later angels
  and friendly-fire rules per `DamageKind`), which calls `Unit::take_damage`;
- `Push` flings the unit (`Unit::fling`) unless it is shielded;
- `Spawn` adds a new effect, which first ticks on the next turn;
- `Terrain` raises or flattens ground; `Panic`, `Raid`, `SetFire` are routed, without behaviour yet.
An effect never holds another object's state: a unit it affects keeps its own countdown (panic, on fire) and
finishes it alone, even after the effect has ended.

To add a spell: add an `EffectKind` variant (or reuse one) whose `tick` follows the original's component (its
rhythm as an `Every`, its length as `left`), returning events; add a `DamageKind` if it hurts differently; add the
effect where the spell lands (`GameMap::apply`, later on a shot's arrival); test it on a small map (`effect.rs`
tests show how).

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
