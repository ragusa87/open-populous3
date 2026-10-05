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

## Spell book (`game_core::spell_book`)
Per tribe: one `SpellSlot` per `SpellKind` with `Availability` (Hidden / Discoverable / Provided { shots } /
Known), `charges` (max 4, fewer for big spells: `max_charges()` from `cost()`) and `recharge` mana.
`tick(mana)` refills recharging spells, `cast(kind)` consumes a charge or a provided shot,
`discover(kind)` turns "?" into Known. Integer only, deterministic. UI: see ui-and-editor.md.
