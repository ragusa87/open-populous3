# Spells

`game_core::spell::Spell` -> `cast(&mut Heightmap) -> DirtyRect`. Implemented (terrain part only):

| Spell | Effect |
|---|---|
| Land Bridge | line between two cells, heights interpolated, min height 32 |
| Flatten | radius 4 set to center height |
| Erode | radius 4 lowered by 120 (falloff) |
| Raise | editor brush, radius 3 +64 |

To do: Swamp (surface type), Earthquake (seeded noise along a line - use `map::Lcg`, never `rand`),
Volcano (cone + lava layer), Angel of Death, Firestorm... Mana cost, charges, cast range from shaman.
After every cast: update normals, walkability and destroy buildings on uneven/flooded ground.
Spells must only be cast through `Command::Cast` to stay deterministic.
