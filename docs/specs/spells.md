# Spells

`game_core::spell::Spell` -> `cast(&mut Heightmap) -> Option<DirtyRect>` (the terrain part). `GameMap::can_cast`
says whether a spell applies at its target; `GameMap::apply` ignores a cast that does not. Implemented:

| Spell | Effect |
|---|---|
| Land Bridge | line between two cells, heights interpolated, min height 32 |
| Flatten | radius 4 set to center height |
| Erode | radius 4 lowered by 120 (falloff) |
| Raise | editor brush, radius 3 +64 |
| Teleport | sandbox only (the original names a spell model 21 teleport, see below, effect unknown): the caster's living shaman does her cast jump, then is at the target (`Unit::cast_teleport`); only onto ground she can walk (`Mobility::Walk`: no sea, no cliff), checked when cast and again when she lands (she stays if it no longer is); another order during the jump cancels it |

To do: Swamp (surface type), Earthquake (seeded noise along a line - use `map::Lcg`, never `rand`),
Volcano (cone + lava layer), Angel of Death, Firestorm... Mana cost, charges, cast range from shaman.
After every cast: update normals, walkability and destroy buildings on uneven/flooded ground.
Spells must only be cast through `Command::Cast` to stay deterministic.

## Spell book (`game_core::spell_book`)
Per tribe: one `SpellSlot` per `SpellKind` with `Availability` (Hidden / Discoverable / Provided { shots } /
Known / Unlimited: cast at will, used by sandbox spells), `charges` (max 4, fewer for big spells: `max_charges()` from `cost()`) and `recharge` mana.
`tick(mana)` refills recharging spells, `cast(kind)` consumes a charge or a provided shot,
`discover(kind)` turns "?" into Known. Integer only, deterministic. UI: see ui-and-editor.md.

`SpellBook::from_level(header, level)`: the header's `SpellsAvailable` panel spells are Known with full charges
(how charged the original starts them is not checked), the spell discoveries (`DiscoveryType 11`) not already
known are Discoverable. Their availability (permanent / this level / once) is not kept yet. Stored as
`GameMap::spell_book` for original levels, the same for every tribe. Armageddon (bit 18) is special: the
campaign's last spell, discovered in levels 17 and 18 and in the masks from level 19 on, always with Convert (17).
Levels 1 and 2 also set bit 18, among leftover bits (1 burn, 20 bloodlust, 21 teleport) and without Convert:
`known_in_header` only takes Armageddon next to Convert. The panel keeps a tile for it.

## Original spells (levels and scripts)
Spell model numbers, as used by discovery things, the level header masks and AI scripts (names from the ALACN
world editor, checked against the campaign's discoveries, see level-format.md):

| Model | Spell | Model | Spell | Model | Spell |
|---|---|---|---|---|---|
| 1 | burn | 8 | firestorm | 15 | flatten |
| 2 | blast | 9 | ghost army | 16 | volcano |
| 3 | lightning | 10 | erode | 17 | convert |
| 4 | tornado (whirlwind) | 11 | swamp | 18 | armageddon |
| 5 | swarm (insect plague) | 12 | land bridge | 19 | magical shield |
| 6 | invisibility | 13 | angel of death | 20 | bloodlust |
| 7 | hypnotism | 14 | earthquake | 21 | teleport |

`SpellKind::model()` / `SpellKind::from_model()` map them; `from_model` only returns the 18 spells of the
original panel (2-19): burn, bloodlust and teleport are never on it (our `Teleport` is a sandbox spell that keeps
model 21 for reference). Ghost Army and Magical Shield have placeholder costs.

The editor also names models 23-30 (hill, rise, valley, dip, place tree, clear mapwho, place shaman, place
wild) but can't place them: unverified.

How a level grants spells:
- **Level header** `SpellsAvailable`: bit N = spell N, known from the start. `SpellsNotCharging` uses the same bits.
- **Discovery things** (general model 2, `DiscoveryType 11`, `DiscoveryModel` = spell): availability 1 permanent,
  2 this level, 3 once ("one shot"). A spell discovered in a level shows up in the start mask of later levels.
- **Mana discoveries**: `DiscoveryType 6`, model 3, 50 000 mana.
- **AI scripts** can give spells or one-shots and set spell costs (see ai-scripts.md).

Land bridge effects in the levels store their target cell (see level-format.md).
