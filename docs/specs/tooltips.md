# Tooltips (buildings, trees and totems done: name, slot rows, clicks, bars, the dismantle toggle; other targets to do)

What the game shows about the thing under the mouse. A tooltip **renders** things: rows of slots, bars and a
toggle, not only text. Buildings, trees and totems use it (client `tooltip.rs`).

Tags: **[player]** how the game plays, as told by someone who played it, **[code]** what ours does today,
**[ours]** a choice made here, to check.

## Showing
- After resting the cursor `HOVER_SECS` (1.5 s) on a hovered thing, or at once on a right click on it, or a
  left click with nothing selected [code].
  One tooltip at a time, for one `tooltip::Target` (a building, a tree or a totem for now).
- It does not follow the mouse: it stands **above the hovered element**, centred over the top of its model, and
  stays there [player]. It stays shown while the cursor is on the element or on the tooltip itself, and hides once
  both lose the focus (after a short grace, `GRACE_SECS`, to cross the gap between them) [player; ours: the grace].
  So the cursor can reach the tooltip and click its slots.
- Not over the panel, nor while a spell or a blueprint is out (the hover halo still shows while a spell is aimed,
  ui-and-editor.md "Hover halo"). What can be hovered: `hover::Hovered` (units, wood pieces, trees, totems, buildings). While the
  cursor is over the tooltip, the map underneath is neither hovered nor clicked.

## Layout [player]
The thing's name, then its slot rows with a small separator between rows, then its toggle where there is one.
Vertical bars stand on the left side of all that, a horizontal bar across the top (see "Bars").

## Slot rows [player]
One widget for every use: one slot per unit of capacity or need.
- An empty slot shows a **placeholder**, the greyed shape of what fits there; a filled slot shows the **icon** of
  what is there. Written below as `O` and `X`: a site for 6 braves with one assigned shows `XOOOOO`.
- A long row wraps after 10 slots, except a row of 16, which wraps after 8 [player]: 16 slots show as 2 lines of
  8, 20 as 2 lines of 10, 12 as 10 then 2. Slots fill first to last, line by line.

### People rows: selecting what is inside
Clicking a filled slot adds that unit to the selection (`Selection::add`) [player]: on a hut with 3 braves,
three clicks select all three. A unit already selected stays selected. It works even for a unit inside the
building, not drawn. A selected unit's slot has a small arrow pointing down on top of its icon [player]; every
people slot keeps room for it, so the row does not move [ours].

| Target | Slots | Placeholder | Filled by |
|---|---|---|---|
| Plan, building under construction | `max_braves` | brave | the braves assigned to it |
| House (villager hut) | its room (`capacity`, 3/4/5): `XXOOO` | brave | the people inside |
| Drum tower | 1 | brave | the unit inside |
| Training unit | 1 (one trainee at a time) | brave | the trainee, with its icon as it is now (e.g. a preacher being trained into a firewarrior) |
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
One per progress, each its own colour, either vertical on the left side of the tooltip (filling upwards) or
horizontal across its top (filling left to right):
- Pyramid, totem: the prayer progress of the player's tribe (worship.md), vertical, on the left.
- House: two vertical bars on the left, its growth (towards the next size) and its birth (the next brave),
  huts-and-training.md.
- Training unit (warrior, firewarrior and spy buildings; later boat and balloon huts, which make their vehicle
  the same way): horizontal, across the top, a double fill as the original's (pop3-rev-analysis.md "The panel's
  training bar"): the overall progress, its stored mana over the cost of what it makes (yellow, on top), over the
  mana layer (blue), what the overall fill has beyond its last whole pixel, so it sweeps the whole width each time
  mana is dispatched and the overall fill moves by a pixel. No bar while it makes nothing.

Done (`tooltip::Bar { kind, fill, blocked }`, `BarKind` giving the colour and the side, `spawn_bar`,
`blink_bars`): the pyramid's prayer bar from its progress (gone once granted), the totem's from the player's gauge
(gone once exhausted); the training bar from `GameMap::production` (`game_core::production`, both fills,
`tooltip::fine_fill`), waiting for the mana distribution to fill it; the house bars wait for that feature.

A bar **blinks while blocked**: a house's birth bar when the tribe is at its population cap; its growth bar when
the next size cannot be reached; a training bar while its last mana share was 32 or less (both fills blink).

## Dismantle toggle [player]
On the player's buildings that take wood. On a built building it marks it for dismantling: it stops working, the
people inside walk out, braves take it apart, and its wood can be used again (buildings.md "Dismantling").
Toggled back while dismantling, it is built again.
At the top right of the tooltip, on the name's line (`tooltip::Toggle`, on the player's flat buildings that take
wood; a plan not flat yet is cancelled instead). Pressed, it sends `Command::Dismantle`. With the original files
(`hfx0-0.dat`, sprites.md, `catalog::DISMANTLE_BUTTON`, `REBUILD_BUTTON`): dismantle 49, 51 hovered, 50 pressed;
undo (build it again) 46, 48 hovered, 47 pressed. Without them, a small "Dismantle" / "Rebuild" text button.
Today dismantling empties the building (people walk out) and its braves stop building it; taking it apart piece
by piece is still to do (buildings.md "Dismantling").

## Unload button [player]
A vehicle has no toggle but a button in the same place: pressed, everyone aboard gets off. It is disabled while
there is no ground to get off on: a balloon over water, a boat not next to the shore, a balloon above ground too
steep to walk on (where they would land is not walkable, `path` walkable cells).
With the original files: 60, 61 hovered, 62 pressed.
Done (`tooltip::Toggle::Unload`, the same widget as the dismantle toggle): shown on the player's manned vehicles,
greyed and doing nothing while `GameMap::can_unload` is false (no walkable ground within reach, vehicles.md);
pressed, it sends `Command::Unload`. Without the original files, an "Unload" text button.

## Per target
| Target | People row | Wood row | Bars | Toggle |
|---|---|---|---|---|
| Plan | assigned braves | provided / cost | | |
| Under construction | assigned braves | provided / cost | | dismantle |
| Built building | people inside (when it holds any) | used | | dismantle |
| House | people inside | used (+ needed while growing) | growth, birth | dismantle |
| Drum tower | the unit inside | used | | dismantle |
| Training unit | the trainee | used | training | dismantle |
| Pyramid of knowledge | the shaman; the name of its reward under its own name | | prayer | |
| Totem | counted prayers | | prayer | |
| Vehicle | people aboard | | | unload button |
| Tree | | current wood | | |
| Wood piece | name only ("Wood") | | | |
| Another tribe's building | its name only; a drum tower clicked (left with nothing selected, or right), its people row too: who is in, not selectable [player] | | | |
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
UI widget draws every row. Done for buildings, trees and totems: `tooltip::building_model`, `tree_model`, `totem_model` (lines and rows of `Slot { icon, unit }`),
`line_lengths`, `sticky` (what stays shown), `icon_pixels` (the generated icons); the box is rebuilt only when its
model changes. Bars and the toggle the same way: a model from the simulation, one widget.

A pyramid's tooltip names its reward (the spell or building it teaches) [player]; once granted, like the icon
on its top, not any more [ours].

## Icons [ours]
With the original files a person is its kind's teal figure (`hfx0-0.dat` 75-80, sprites.md, drawn 1:1 in an
18 x 24 slot), greyed and half seen through when a placeholder, whatever the tribe (`tooltip::sprite_pixels`, `IconImages`).
Otherwise (`--no-original`, wildmen) generated in code: a small silhouette per unit kind (head and body shapes, a
staff for the shaman), in the tribe's colour when filled and grey when a placeholder; a log for wood, always.
The slot rows only ask for an icon by what it shows.
