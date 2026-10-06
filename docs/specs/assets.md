# Assets

The POC needs no sprites: terrain uses vertex colors banded by height.
Planned, all CC0:
- Quaternius: Ultimate Nature pack (trees, rocks), Medieval Village (huts, towers), animated characters.
- Kenney: Nature Kit, Fantasy Town Kit, UI Pack (tab buttons, icons), Game Icons (spells).
Keep them under `assets/` with a `CREDITS.md`. Convert to glTF (.glb) for Bevy.
Unit sprites: brief for artists and image models in [unit-art.md](unit-art.md).
The original game's art is copyrighted: never ship it, only read user-owned level data.
When the user's install is present, original 3D objects and their atlas are read at runtime
(`original_models.rs`, see objects.md); every use must keep a generated fallback for `--no-original`.
