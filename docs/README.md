# Documentation

- [architecture.md](architecture.md): crates, boundaries, data flow.
- [roadmap.md](roadmap.md): what is done; what is left is in [TODO.md](../TODO.md).
- specs/
  - [level-format.md](specs/level-format.md): original `.dat`/`.hdr` layout (what we know).
  - [ai-scripts.md](specs/ai-scripts.md): PopScript, the original computer-player scripts (`cpscr`/`cpatr`): language, execution, commands, Lua successor.
  - [terrain.md](specs/terrain.md): heightmap, wrapping, curvature rendering, brushes.
  - [terrain-textures.md](specs/terrain-textures.md): original theme files and the texture bake.
  - [spells.md](specs/spells.md): terrain spells and what they must recompute.
  - [units.md](specs/units.md): units, pathfinding (to do).
  - [multiplayer.md](specs/multiplayer.md): deterministic lockstep over TCP.
  - [ui-and-editor.md](specs/ui-and-editor.md): camera, control tabs, world editor.
  - [assets.md](specs/assets.md): original-data-free path, missing-asset inventory and building kit brief.
  - [buildings.md](specs/buildings.md): building models, stages and planned construction.
  - [building kit](../assets/models/buildings/README.md): reproducible glTF generation, Blender preview and licence.
  - [sound.md](specs/sound.md): original `.SDT` sound, drum and music banks.
