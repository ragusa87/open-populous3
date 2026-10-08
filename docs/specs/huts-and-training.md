# Huts and training huts (design, simulation only)

Design for what villager huts (houses) and training huts do in `game-core`. Placing and constructing them is
in buildings.md ("Construction"); drawing occupants, bars and icons is not covered here.

Tags: **[files]** decoded `levels/constant.dat` (constants.md), **[community]** the PopRe wiki
([Hut](https://wiki.popre.net/Hut), [Warrior Training Hut](https://wiki.popre.net/Warrior_Training_Hut)) and the
[poptb.com building guide](https://ts.popre.net/websites/poptb.com/guide/basic-gameplay/building/index.html),
**[ours]** a choice made here, to check in the game.

## Original values [files]
| Constant | Value | Read as |
|---|---|---|
| `MAX_POP_VALUE__HUT_1..3` | 3, 5, 7 | population each hut size adds to the tribe's cap ("supply") |
| `HUT1..3_SPROG_TIME` | 4000, 3000, 2000 | time for a hut to make a brave; comment: "X 0 - 0.5, 1 - 1.0, 2 - 1.5, 3 - 2.0" |
| `SPROG%_POP_BAND_00_04%` .. `_95_99%` | 30, 35, 40, 50, 60, ... 190, 195, 200 | % applied to the sprog time by population band (20 bands of 5 %) |
| `MANA_F_BRAVE`, `_WARR`, `_SPY`, `_PREACH`, `_SWARR`, `_SHAMEN` | 15, 4, 4, 4, 4, 30 | mana per person per update |
| `MANA_F_HOUSED`, `_WORKING`, `_TRAINING` | 100, 100, 50 % | factor by what the person is doing |
| `MANA_F_HUT_LEVEL_1..3` | 100, 110, 120 % | factor for people in a hut of that size |
| `MANA_UPDATE_FREQ` | 15 | mana is added every 16 turns |
| `HUMAN_MANA_ADJUST`, `COMPUTER_MANA_ADJUST` | 125, 50 % | income factor by player type |
| `HUMAN_TRAIN_MANA_WARR`, `_SPY`, `_PREACH`, `_SWARR` | 3500, 4000, 3500, 4000 | mana to train one (computer players: 1000 each) |
| `TRAIN_MANA_BAND_00_03` .. `_21+` | 100, 125, 150, 175, 200, 250 % | training cost factor by number of specialists |
| `CONV_TEMPLE`, `_SPY`, `_WARRIOR`, `_SUPER` | 4000 each | probably the training time, same unit as the sprog times |
| `WOOD_BRAVE`, `WOOD_HUT_1..3`, `TREE*_WOOD_VALUE` | 100, 300, 400 | a brave carries 1 piece, a hut costs 3, a tree holds 4 (as `wood_cost`, trees.md) |

## Huts [community]
- Capacity (people inside): 3 / 4 / 5 for a small / medium / large hut. Supply (cap): 3 / 5 / 7. The tribe's
  population cap is 199: 28 large huts reach it.
- A hut makes braves on its own, empty or not: empty at half speed, faster with more people inside (the
  `SPROG_TIME` comment above). The larger the hut, the faster.
- A newly built hut starts with its breeding bar full: a brave comes out at once.
- A hut grows twice (small, medium, large). Its upgrade bar fills while people are inside; wood is brought to its
  door ("three pieces start the upgrade, only two are used"); a brave can also be ordered to drop wood there.
- People inside make mana. The guide says a brave makes the same mana in any hut size, against
  `MANA_F_HUT_LEVEL` 100/110/120 %: the files win until checked in the game.
- Firewarriors in a hut throw fire at enemies nearby, the shaman casts from inside one (later: combat, spells).

### Rules
State on `Building` (huts only): `level` 1-3 (from the kind, `villager_hut` sizes), `occupants` (unit ids, in
entry order), `breed` and `grow` progress, `wood` pieces left at its door for the upgrade.

1. **Entering** (`Order::Enter { building }` through `Command::OrderUnit`): any follower of the owner (not the
   shaman, not wildmen) walks to the door, then in (buildings.md "Doors"), and becomes `Action::Inside`: off the
   map, no standing slot, not drawn. A full hut refuses: the unit stops at the door, idle.
2. **Leaving**: an order to a unit inside (move, pray...) makes it come out by the door first. `Command::Eject
   { player, building }` empties a hut. A hut that is dismantled or destroyed puts everyone out by the door.
3. **Auto-housing** [ours]: an idle brave of the tribe with no order for `HOUSE_DELAY` (10 s) walks into the
   nearest built hut with room within 8 cells. Other kinds only enter when ordered.
4. **Population**: every living unit of the tribe except the shaman, inside or out. Cap = sum of the supply of
   its built huts, at most 199. Units over the cap (level start, conversions) stay; only breeding stops.
5. **Breeding**: each built hut, every update, adds `occupants + 1` to `breed` (half steps: 0 inside = 0.5,
   1 = 1.0, 2 = 1.5..., up to 3.0 for 5). A brave is born when `breed >= 2 * SPROG_TIME[level] * band / 100`,
   with `band` from the tribe's population as a % of its cap. The brave appears at the door, idle, and `breed`
   restarts at 0. At the cap, `breed` stays full and the brave comes as soon as there is room. A newly built hut
   starts with `breed` full.
6. **Growing** [ours, to check]: `grow` fills only while someone is inside (`occupants` per update) up to
   `GROW_TIME`. Once full, the hut waits for 2 pieces of wood at its door: braves inside it fetch them (one piece
   each, the wood dispatch rule of buildings.md), and any brave ordered to drop a carried piece there adds one.
   With 2 pieces it uses them, `level` + 1 (kind becomes the next hut size, same footprint), `grow` restarts.
   Large huts do not grow.
7. **Mana** (every `MANA_UPDATE` = 16 turns), per tribe: sum over its people of `MANA_F[kind]` x activity %
   (inside a hut: `HOUSED` x `HUT_LEVEL[level]`; working on a building or fetching wood: `WORKING`; inside a
   training hut: `TRAINING`; otherwise [ours]: 0), then x `HUMAN_` or `COMPUTER_MANA_ADJUST`, added to the
   tribe's mana, capped at `MAX_MANA`. Praying at a totem is its own gauge (TODO "Praying"), not mana.

## Training huts
| Building | Trains | Cost (human) |
|---|---|---|
| Warrior training | warrior | 3500 |
| Firewarrior training | firewarrior | 4000 (`SWARR`, "super warrior") |
| Temple | preacher | 3500 |
| Spy training | spy | 4000 |

A training hut makes no mana [community]. As in the game (checked by someone who knows it):

1. **Assigning**: any follower (not the shaman, not wildmen) ordered onto a built training hut of its tribe
   (`Order::Enter { building }`) joins its queue, in order of arrival, however many there are (200 is fine). A
   unit already of the hut's kind is trained all the same and comes out unchanged.
2. **Queue**: one standing spot per unit (`game_core::slots`), starting at the front door and going on around
   the building; when a ring is full it goes on next to it, folding back like a snake. Queued units stand on the
   map, selectable and orderable as usual. When the head goes in, or a queued unit is given another order and
   leaves, everyone behind it moves up one spot.
3. **One at a time**: the head of the queue walks in by the door and is trained for a while. The hut's tooltip
   shows the training progress, and the unit inside can be selected.
4. **Mana** [ours, to check]: taken along the training, a share every update (cost / duration). Without enough
   mana for the share, training pauses. Mana spent is lost if the training is cancelled. Later: the training speed
   may depend on the mana available.
5. **Done**: the unit is transformed into the hut's kind (same unit, new kind: a brave goes in, a firewarrior
   comes out). A small room by the door lets it come out while the next in the queue goes in. Out of the door it
   is idle, on the nearest free standing spot (never one taken by another unit, `GameMap::taken_spots`).
6. **Cancelling**: ordering the unit inside (move...) makes it come out by the door, its kind unchanged, then
   carry out the order; its training is cancelled and the queue goes on.
7. **Damage**: when the hut is damaged, the whole queue is cancelled (its units idle where they stand) and the
   unit inside comes out unchanged. Later, with combat and spells: in a fight the unit inside comes out to fight,
   hit by a spell it is handled like any unit.
8. While inside, a unit counts as `TRAINING` (50 %) for mana.

## Balance and game turns
The original counts in game turns (about 8 per second, unverified, ai-scripts.md); we tick 10 times per second.
Every number of these rules lives in one place, `game_core::balance`:
- `Turns(u32)`: a duration in original game turns, as the constants store it. `Balance::ticks(Turns) -> u32`
  converts with an integer ratio (`turns_per_second`, 10 until the original rate is measured, so 1 turn = 1 tick
  today). Simulation state only holds ticks.
- `Balance`: plain integer fields grouped by topic (mana, huts, training, wood), the durations as `Turns`.
  `Balance::ours()` is our own defaults, written by hand (the original values for now, to tune), used in generated
  mode and the sandboxes; `Balance::from_constants` will fill it from `constant.dat` once `pop3_format::constants`
  exists, when an install is present.
- Stored in `GameMap` and read by the rules, never a global: tests build their own. Lockstep peers must run with
  the same `Balance` (part of the game setup and the desync checksum).

With the default balance, a small hut with 1 brave inside at 0-4 % population breeds every 4000 x 30 % = 1200 ticks
(2 min); 3 inside (gain 2.0), 1 min. To measure in the game: time between two braves of a small hut with 1 inside,
time to train one warrior and its mana, a medium hut's time to grow.

## Simulation (to build)
- `Building::id` (stable, given in load order then creation order): units point at it; the view needs it too
  (TODO "Site visuals").
- `game_core::tribe::Tribe { mana, human }` in `GameMap::tribes`, one per tribe in owner order.
- Pure functions with tests: `breed_gain(occupants)`, `breed_threshold(level, population, cap)`,
  `population_band(population, cap)`, `train_cost(kind, specialists, human)`, `train_share(cost, duration)`,
  `mana_income(people)`, `queue_spots(building, count)` (the snake around the footprint).
- `GameMap::tick`: huts and training huts in id order, then mana; no `HashMap`, no randomness.
- `Order::Enter { building }`, `Command::Eject { player, building }`; wire codec kinds in multiplayer.md.

## Open questions
- Huts: what the population band is a percentage of (the tribe's cap assumed, or 199); whether people outside
  huts make mana (assumed not); what fills the upgrade bar and who brings the wood; whether idle braves house
  themselves, from how far, after how long.
- Whether spell recharge and training take from the same mana pool (TODO "Mana").
- Training: how mana is taken (assumed along the training), the real `CONV_*` meaning and the cost by specialist
  count (`TRAIN_MANA_BAND`).
