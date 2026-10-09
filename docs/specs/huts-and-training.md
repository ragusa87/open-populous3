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
  population cap is min(supply of its huts, 200) [player; 199 per the community]; 29 large huts reach it. It is not
  in `constant.dat` (only `MAX_POP_VALUE__HUT_1..3` and the breeding bands): probably one of the "extended"
  constants of the executable (constants.md). We use 200.
- A hut makes braves on its own, empty or not: empty at half speed, faster with more people inside (the
  `SPROG_TIME` comment above). The larger the hut, the faster.
- A newly built hut starts with its breeding bar full: a brave comes out at once.
- A hut grows twice (small, medium, large) with wood brought to its door ("three pieces start the upgrade, only
  two are used"); a brave can also be ordered to drop wood there.
- People inside make mana. The guide says a brave makes the same mana in any hut size, against
  `MANA_F_HUT_LEVEL` 100/110/120 %: the files win until checked in the game.
- Firewarriors in a hut throw fire at enemies nearby, the shaman casts from inside one (later: combat, spells).

### Done
- Going inside (`game_core::enter`, reused for every holder): `Order::Enter { site }` (left click on one of
  the player's built buildings with room, `GameMap::shelter_at` / `enter_orders`, `selection::building_click`;
  a click on a building wins over a tree): followers (not the shaman, not wildmen) walk to the door and in
  (`Unit::enter`), and stay inside, idle, not drawn. `BuildingKind::capacity`: huts 3 / 4 / 5 by size (they
  rest), a drum tower 1 (buildings.md "Towers"); the others stay at the door, idle. Any walk takes them out by
  the door. Builders go in and out the same way (buildings.md "Construction").
- Vehicles (boats, balloons) will hold people the same way: `Unit::inside` will name its holder (a building's
  stored corner today; a holder enum with vehicle ids once vehicles exist), each with its room and its way in
  (a boat's side, a balloon's basket) instead of a door.
- `Building::inside` is counted each tick from the units inside (`GameMap::count_inside`): a built hut or
  drum tower with people inside smokes from the top of its roof. The one in a drum tower stands on its lookout,
  in view (buildings.md "On screen"). The tooltip shows `Braves: people inside/room` (planned: a people row, tooltips.md).
- Trees under a building (plans included) do not grow back.

### Rules
State on `Building` (huts only): `level` 1-3 (from the kind, `villager_hut` sizes), `occupants` (unit ids, in
entry order), `breed` (green bar) and `grow` (red bar) progress. The hut's tooltip shows both bars (tooltips.md "Bars", blinking while blocked); a large
hut (max size) hides the red bar, it does not show it at 0.

1. **Entering** (`Order::Enter { building }` through `Command::OrderUnit`): any follower of the owner (not the
   shaman, not wildmen) walks to the door, then in (buildings.md "Doors"), and becomes `Action::Inside`: off the
   map, no standing slot, not drawn. A full hut refuses: the unit stops at the door, idle.
2. **Leaving**: an order to a unit inside (move, pray...) makes it come out by the door first. `Command::Eject
   { player, building }` empties a hut. A hut that is dismantled or destroyed puts everyone out by the door.
3. **Auto-housing** [ours]: an idle brave of the tribe with no order for `HOUSE_DELAY` (10 s) walks into the
   nearest built hut with room within 8 cells. Other kinds only enter when ordered.
4. **Population**: every living unit of the tribe except the shaman, inside or out. Cap = sum of the supply of
   its built huts at their current size (a hut being grown counts its old size), at most 200. Units over the cap (level start, conversions) stay; only breeding stops.
5. **Breeding** (green bar): each built hut, every update, adds `occupants + 1` to `breed` (half steps: 0 inside =
   0.5, 1 = 1.0, 2 = 1.5..., up to 3.0 for 5). At 100 % (`breed >= 2 * SPROG_TIME[level] * band / 100`, with
   `band` from the tribe's population as a % of its cap) a brave is born at the front door, idle, and `breed`
   restarts at 0. At the cap (population full), `breed` stops at 100 % and the brave comes as soon as there is room. A newly built hut
   starts with `breed` full. Later: a small star animation where the brave appears (client).
6. **Growing** (red bar, the need to grow): `grow` rises with time, even when the hut is empty, a little faster
   with people inside: every update `GROW_STEP` x (100 + `GROW_BONUS` x occupants) / 100 [ours: no constant for it
   in `constant.dat`, maybe one of the "extended" constants of the executable, constants.md]. At 100 %, if the
   tribe's population has room (below its cap), the hut grows, as in the game:
   1. some braves of the hut fetch 3 pieces of wood and put them inside: 1 is the cost of growing (consumed), 2 go
      into the hut (given back when it is dismantled);
   2. the hut becomes a construction site of the next size (if there is one: large huts do not grow), with the
      wood of its current size already in it;
   3. a few braves work on it (as a construction, buildings.md) while the others inside keep resting: the hut stays
      usable all along (people inside, breeding);
   4. once built, it is the bigger hut: more wood in it and more room for people; `grow` restarts at 0.
   If the population is saturated (population = cap), the red bar stays at 100 % and blinks (client) until there
   is room.

   Damage while growing [ours, unsure]: the growing part takes the damage first, like extra health on top of the
   hut; once it is gone, the hut falls back to its size before growing (the growth is lost) and then takes damage
   like any building.

   Wood held by size [ours, to check]: grows with the size, about +2 per size: small 3, medium 5, large 7
   (`BuildingKind::wood_cost` is 3 for every hut size today).
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
time to train one warrior and its mana, how fast the red bar fills.

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
- Huts: what the population band is a percentage of (the tribe's cap assumed, or 200); whether people outside
  huts make mana (assumed not); the red bar's speed and occupant bonus; the wood held per hut size; how many braves fetch and build when a hut grows (only those inside?); what damage really does to a growing hut; whether idle braves house
  themselves, from how far, after how long.
- Whether spell recharge and training take from the same mana pool (TODO "Mana").
- Training: how mana is taken (assumed along the training), the real `CONV_*` meaning and the cost by specialist
  count (`TRAIN_MANA_BAND`).
