# Tooltips (design; text-only tooltips done for buildings and trees)

What the game shows about the thing under the mouse. A tooltip **renders** things: rows of slots, bars and a
toggle, not only text. Today buildings (`buildings::building_label`) and trees (`nature::tooltip`) show text.

Tags: **[player]** how the game plays, as told by someone who played it, **[code]** what ours does today,
**[ours]** a choice made here, to check.

## Showing [code]
- After resting the cursor `HOVER_SECS` (1.5 s) on a hovered thing, or at once on a right click (trees today,
  buildings to come). One tooltip at a time, by the cursor (16 px right, 18 px down), hidden when the target goes.
- Not over the panel, nor while a spell or a blueprint is out, like the hover halo (ui-and-editor.md "Hover
  halo"). What can be hovered: `hover::Hovered` (units, wood pieces, trees, buildings; totems to add).

## Layout [player]
The thing's name, then its slot rows with a small separator between rows, then its toggle where there is one.
Bars stand on the left side.

## Slot rows [player]
One widget for every use: one slot per unit of capacity or need.
- An empty slot shows a **placeholder**, the greyed shape of what fits there; a filled slot shows the **icon** of
  what is there. Written below as `O` and `X`: a site for 6 braves with one assigned shows `XOOOOO`.
- A long row wraps after 10 slots, except a row of 16, which wraps after 8 [player]: 16 slots show as 2 lines of
  8, 20 as 2 lines of 10, 12 as 10 then 2. Slots fill first to last, line by line.

### People rows: selecting what is inside
Clicking a filled slot selects that unit [player].

| Target | Slots | Placeholder | Filled by |
|---|---|---|---|
| Plan, building under construction | `max_braves` | brave | the braves assigned to it |
| House (villager hut) | its room (`capacity`, 3/4/5): `XXOOO` | brave | the people inside |
| Drum tower | 1 | brave | the unit inside |
| Training hut | 1 (one trainee at a time) | brave | the trainee, with its icon as it is now (e.g. a preacher being trained into a firewarrior) |
| Pyramid of knowledge | 1 | shaman | the shaman assigned to it, praying at the door or inside |
| Totem | `TriggerCount` (`Totem::prayers`), e.g. 8: `XOOOOOOO` | brave (shaman for shaman-only totems) | the counted prayers, each with the icon of its own kind; assigned units beyond the count do not show |
| Vehicle | its seats | brave | the people aboard |

### Wood rows: display only
Nothing to select [player].

| Target | Slots | Filled by |
|---|---|---|
| Plan, building under construction | its wood cost: `XOOO` | the wood provided (pile by the door + built in) |
| Built building | the wood used | all filled |
| Growing house | the wood it has plus the pieces still needed: `XXXXOO` | the wood it has |
| Tree | its current wood only, no placeholders (its original capacity is not known): `XXX` for 3 | all filled |

A tree with no wood left is not drawn and has no tooltip (trees.md).

## Bars [player]
Horizontal, on the left of the tooltip, one per progress, each its own colour:
- Pyramid, totem: the prayer progress of the player's tribe (worship.md).
- House: two bars, its growth (towards the next size) and its birth (the next brave), huts-and-training.md.
- Training hut: the training.

A bar **blinks while blocked**: a house's birth bar when the tribe is at its population cap; its growth bar when
the next size cannot be reached; a training bar without mana [player: blinking; ours: which causes count as
blocked].

## Dismantle toggle [player]
On the player's buildings that take wood. On a built building it marks it for dismantling: it stops working, the
people inside walk out, braves take it apart, and its wood can be used again (buildings.md "Dismantling").
Toggled back while dismantling, it is built again.

## Per target
| Target | People row | Wood row | Bars | Toggle |
|---|---|---|---|---|
| Plan | assigned braves | provided / cost | | |
| Under construction | assigned braves | provided / cost | | dismantle |
| Built building | people inside (when it holds any) | used | | dismantle |
| House | people inside | used (+ needed while growing) | growth, birth | dismantle |
| Drum tower | the unit inside | used | | dismantle |
| Training hut | the trainee | used | training | dismantle |
| Pyramid of knowledge | the shaman; the name of its reward under its own name | | prayer | |
| Totem | counted prayers | | prayer | |
| Vehicle | people aboard | | | |
| Tree | | current wood | | |
| Wood piece | name only ("Wood") | | | |
| Camp fire | who is round it [ours: to define] | | | |
| Unit | no box: its health bar is its tooltip (units.md) | | | |
| Spy | a spy indicator instead of its disguise (units.md) | | | |
| Reincarnation site | none | | | |

## Model [ours]
Done in the simulation: `game_core::occupancy`, `GameMap::people_slots(holder)` (`People { capacity, filled }`,
unit ids in id order, at most `capacity`) and `GameMap::wood_slots(holder)` (`Wood { capacity, filled }`), for a
`Holder::Building | Totem | Tree`; `occupancy::room` is a built building's people row (huts' room, 1 for a drum
tower, a training hut and an unspent pyramid). Who counts is decided there, next to the rules; the text tooltips
read them already. To do in the client: one `SlotRow { kind: People | Wood, slots, filled }` model, `filled` holding each filled slot's icon (and, for
people, the unit id its click selects), built from the simulation each frame by pure, tested functions; one Bevy
UI widget draws every row. Bars and the toggle the same way: a model from the simulation, one widget.

A pyramid's tooltip names its reward (the spell or building it teaches) [player]; once granted, like the icon
on its top, not any more [ours].

## Icons [ours]
Generated in code for now, no art asset: a small silhouette per unit kind (head and body shapes, a staff for the
shaman), in the tribe's colour when filled and grey when a placeholder; a log for wood. Better art can replace
them later, the slot rows only ask for an icon by what it shows.
