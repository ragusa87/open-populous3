# Architecture

```
crates/
  pop3-format   pure parsers for original files (no engine). Reusable by tools/editor.
  game-core     deterministic simulation: terrain (Heightmap), map, spell, unit, command, vault; query layers
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
