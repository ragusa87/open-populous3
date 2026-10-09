# Architecture

```
crates/
  pop3-format   pure parsers for original files (no engine). Reusable by tools/editor.
  game-core     deterministic simulation: terrain (Heightmap), map, spell, unit, command, vault.
                Integers only in state. No Bevy.
  game-net      lockstep wire protocol (length-prefixed, hand-written binary) over std TCP.
  unit-atlas    text index of baked unit frames (rectangle and feet per pose, direction, frame).
  unit-baker    Bevy tool: renders the CC0 characters into assets/units atlases (just bake-units).
  game-client   Bevy app. One plugin per concern:
                  world.rs        CurrentMap, LevelList, terrain entity rebuild
                  terrain_mesh.rs pure mesh builder (curvature, colors, normals), unit-tested
                  grounded.rs     parts set on the curved terrain under them (sites, buildings...)
                  original_models.rs original 3D objects -> meshes + theme atlas (optional)
                  generated_buildings.rs embedded generated GLBs -> cell-space coloured meshes + construction frames
                  buildings.rs    model choice, construction stages, smoke and shaking
                  vault.rs        pyramid of knowledge door and top, posed from its phase
                  sites.rs        reincarnation site markers on the curved surface
                  units/          sim clock, unit sprites (original or generated art), health bars, orders
                  virtual_cursor.rs in-game cursor (system cursor locked), drives edge scroll + UI picking
                  camera.rs       CameraRig (focus wraps on the torus), input, aerial toggle
                  hud/            info text, shaman preview, control tabs (Spells/Build/Stats), view menu
                  editor.rs       edit mode: spells as brushes at camera focus
                  dev.rs          HEADLESS / SCREENSHOT / AERIAL env helpers
```

Rules:
- `pop3-format` and `game-core` never depend on Bevy: they must run in tests, servers and tools.
- Rendering reads simulation state, never the opposite.
- Simulation changes only through `Command`s, so the same inputs replay identically on every peer.
- Logic lives in pure functions (`terrain_mesh::build`, `spell_for_key`, `CameraRig::move_by`) with tests;
  Bevy systems stay thin.
