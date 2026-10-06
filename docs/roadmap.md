# Roadmap

## Done (POC)
- Parse original levels: heightmap, 3 byte layers, things (partially), level name from `.hdr`.
- Wrapping 128x128 heightmap with deterministic brushes (raise/lower, flatten, land bridge).
- Curved "planet" rendering, camera-centred mesh rebuilt on CPU when focus/terrain changes.
- Orbit camera, aerial view (Enter), level cycling, windowed + F11/`FULLSCREEN=1`.
- Control-tab placeholders, editor brushes, lockstep message codec + TCP round-trip test.
- Headless offscreen screenshots for CI / dev.
- Ground textured from the original theme files (palette + bigfade + disp), per level theme.
- Shaman: simulated (walk, pray, cast jump, drowning, death, reincarnation at her site), drawn as a sprite
  from the original animations or generated art, health bar, panel preview, right-click orders.

## Next
1. Terrain: move curvature to a vertex shader sampling an R16 height texture; dirty-rect re-bake;
   theme sky; check the height->colour mapping against the real game.
2. Things: decode the 55-byte record fully; spawn trees/buildings/shamans from the level.
3. Units: A*/flow field with modulo indexing (the shaman walks straight lines for now), slope limits,
   braves/warriors from the level things (animations 0-52 still to map), praying giving mana.
4. Spells: mana, cast range, building destruction on uneven/flooded ground.
5. Multiplayer: turn scheduler (input delay 2-3 turns), host/join UI, desync checksum of the heightmap.
6. Editor: mouse picking on the curved surface, save to the original `.dat` format.
7. Art: Kenney / Quaternius low-poly packs (see specs/assets.md).
