# Roadmap

## Done (POC)
- Parse original levels: heightmap, 3 byte layers, things (partially), level name from `.hdr`.
- Wrapping 128x128 heightmap with deterministic brushes (raise/lower, flatten, land bridge).
- Curved "planet" rendering, camera-centred mesh rebuilt on CPU when focus/terrain changes.
- Orbit camera, aerial view (Enter), level cycling, windowed + F11/`FULLSCREEN=1`.
- Control-tab placeholders, editor brushes, lockstep message codec + TCP round-trip test.
- Headless offscreen screenshots for CI / dev.
- Original-data-free asset audit; 16 generated low-poly building GLBs from a reproducible, AI-written Python script,
  optional Blender contact sheet, tribe accents and timber construction frames. All named building kinds covered.
- Reproducible generated building surface textures (clay, wood, thatch, stone, cloth), one atlas linked from every GLB;
  shared mipmapped atlas in the client, including partially constructed buildings and scaffolds.
- Ground textured from the original theme files (palette + bigfade + disp), per level theme.
- Shaman: simulated (walk, pray, cast jump, drowning, death, reincarnation at her site), drawn as a sprite
  from the original animations or generated art, health bar, panel preview, right-click orders.

## Next
See [TODO.md](../TODO.md).
