# Huts and training huts (design, simulation only)

Design for what villager huts (houses) and training huts do in `game-core`. Placing and constructing them is
in buildings.md ("Construction"); drawing occupants, bars and icons is not covered here.

Tags: **[files]** decoded `levels/constant.dat` (constants.md), **[community]** the PopRe wiki
([Hut](https://wiki.popre.net/Hut), [Warrior Training Hut](https://wiki.popre.net/Warrior_Training_Hut)) and the
[poptb.com building guide](https://ts.popre.net/websites/poptb.com/guide/basic-gameplay/building/index.html),
**[exe]** the original executable (pop3-rev-analysis.md), **[ours]** a choice made here, to check in the game.

## Original values [files]
| Constant | Value | Read as |
|---|---|---|
| `MAX_POP_VALUE__HUT_1..3` | 3, 5, 7 | population each hut size adds to the tribe's cap ("supply") |
| `HUT1..3_SPROG_TIME` | 4000, 3000, 2000 | time for a hut to make a brave; comment: "X 0 - 0.5, 1 - 1.0, 2 - 1.5, 3 - 2.0" |
| `SPROG%_POP_BAND_00_04%` .. `_95_99%` | 30, 35, 40, 50, 60, ... 190, 195, 200 | % applied to the sprog time by population band (20 bands of 5 %) |
| `MANA_F_BRAVE`, `_WARR`, `_SPY`, `_PREACH`, `_SWARR`, `_SHAMEN` | 15, 4, 4, 4, 4, 30 | mana per person; only `_SHAMEN` is read by the income [exe] |
| `MANA_F_HOUSED`, `_WORKING`, `_TRAINING` | 100, 100, 50 % | factor by what the person is doing; not read by the income [exe] |
| `MANA_F_HUT_LEVEL_1..3` | 100, 110, 120 % | factor for people in a hut of that size; not read by the income [exe] |
| `MANA_UPDATE_FREQ` | 15 | mana is added every 16 turns |
| `HUMAN_MANA_ADJUST`, `COMPUTER_MANA_ADJUST` | 125, 50 % | income factor by player type |
| `HUMAN_TRAIN_MANA_WARR`, `_SPY`, `_PREACH`, `_SWARR` | 3500, 4000, 3500, 4000 | mana to train one (computer players: 1000 each) [exe] |
| `TRAIN_MANA_BAND_00_03` .. `_21+` | 100, 125, 150, 175, 200, 250 % | cost factor by the tribe's number of that specialist (0-3, 4-7, 8-11, 12-15, 16-20, 21+) [exe] |

Not in the file [exe]: the income per person is `MANA_BUSY_BRAVES` 15 / `MANA_IDLE_BRAVES` 4 and
`MANA_BUSY_SPECIALS` 5 / `MANA_IDLE_SPECIALS` 4, exe defaults that `constant.dat` cannot change.
| `CONV_TEMPLE`, `_SPY`, `_WARRIOR`, `_SUPER` | 4000 each | match no exe constant name, probably ignored; not the training time [exe] |
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
- People inside make mana. The guide says a brave makes the same mana in any hut size, which fits the
  income routine [exe]: it does not read `MANA_F_HUT_LEVEL`.
- Firewarriors in a hut throw fire at enemies nearby, the shaman casts from inside one (later: combat, spells).

### Done
- Going inside (`game_core::enter`, reused for every holder): `Order::Enter { site }` (left click on one of
  the player's built buildings with room, `GameMap::shelter_at` / `enter_orders`, `selection::building_click`;
  a click on a building wins over a tree): followers (not the shaman, not wildmen) walk to the door and in
  (`Unit::enter`), and stay inside, idle, not drawn. `BuildingKind::capacity`: huts 3 / 4 / 5 by size (they
  rest), a drum tower 1 (buildings.md "Towers"); the others stay at the door, idle. Any walk takes them out by
  the door. Builders go in and out the same way (buildings.md "Construction").
- Sending a selection into buildings of a kind (the Build tab's shift click, `game_core::send_in`): only the
  player's own people into their own built buildings of that kind (huts of any size, a tower, a training unit, a
  vehicle hut: braves only) or round their camp fires. Room: a hut's or tower's free room, a vehicle hut's free crew
  places, a training unit and a camp fire everyone (they queue, go round). `send_in::assign`: the units spread
  evenly over the places with room, nearest pairs first, then the rest to the nearest place still with room; those
  that fit nowhere stay.
- Vehicles hold people their own way (`Unit::aboard`, vehicles.md); vehicle huts take braves as a crew that
  fetches wood and builds vehicles (vehicles.md "Vehicle huts").
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
   its built huts (`headcount::Housing::room`), at most 200 - a hut mid-growth (below) counts neither its old
   nor its new size for the brief window it is a construction site rather than `Stage::Built`, `[ours]`,
   simpler than the trace's own per-tribe base offset (`tribe+0x921`, not modelled) or tracking a shadow
   "previous size" through that window. Units over the cap (level start, conversions) stay; only breeding stops.
5. **Breeding** (green bar, done - `hut_growth.rs`): every `BREED_EVERY` (4 turns [exe]), each built hut adds
   `BREED_STEP` (2) x (occupants + 1) to `breed` [exe, confirmed - not the half-steps this entry's own earlier
   draft guessed]. At its target (`HUT<n>_SPROG_TIME` x `SPROG%[band]` / 100 - `SPROG_TIME` 4000/3000/2000 by
   level [files]; `band` is `(population + 1) / 10` clamped to 0-19, **flat bands of 10 people, not a % of the
   tribe's cap** as this entry's own earlier draft guessed [exe, confirmed]; `SPROG%` 30 at band 0 to 200 at
   band 19 [files] - only those two ends are individually confirmed this session, the 18 between are `[ours]`,
   linearly interpolated, not the real (close but not exactly linear) stairstep) a brave is born at the front
   door, idle, and `breed` restarts at 0. Without room (population + 1 at or past the cap) breeding does not
   even accumulate: `breed` resets to 0 every such turn instead of holding at its target [exe, confirmed - not
   "stops at 100 %" as this entry's own earlier draft guessed]. A freshly completed hut (construction finishing,
   `work::work_event`'s own `UnitEvent::Built`) starts with a head start, not a cold 0 nor an instantly full bar:
   `target - 54` [exe, confirmed "the first brave comes quickly"] - computed at band 0 (the fastest) as a small,
   documented simplification, since the real population at that exact moment isn't threaded through to there;
   the very next breeding tick recomputes the real target anyway. Later: a small star animation where the brave
   appears (client).
6. **Growing** (red bar, done - `hut_growth.rs`): every `GROW_EVERY` (16 turns [exe]), a small or medium hut (a
   large one has no next size) with nobody inside is left alone (kept, not reset [exe, confirmed]); otherwise
   `grow` gains `GROW_STEP` (8) x occupants [exe, confirmed - not this entry's own earlier "a little faster with
   people inside" guessed formula]. At `GROW_TARGET` (2400, exe-only, no `constant.dat` entry [exe, confirmed]),
   if the next size's footprint fits where the hut already stands (`placement::can_place`, excluding the hut's
   own current footprint) it becomes that size outright:
   1. `kind` changes in place, same id - whoever is inside just stays "inside" that same id, so "move the people
      inside into it" falls out for free, matching the original's own "the hut stays usable all along (people
      inside, breeding)";
   2. seeded with 1 of its `wood_cost` pieces already in (the trace's own "100 wood" - **not** this entry's
      earlier guess of braves fetching and consuming 3 pieces up front; the real mechanism seeds 1 piece
      directly and lets the ordinary construction system fetch the rest, same as any other building under
      construction);
   3. `grow` back to 0.
   If the footprint does not fit, `grow` stays clamped at `GROW_TARGET` and is retried every future growth tick
   - simpler than the trace's own "retried every 128 turns" detail, `[ours]`, since nothing is lost by checking
   more often. Every hut size shares one footprint today (`BuildingKind::footprint`), so in practice this almost
   never blocks growth - a real per-size footprint is a separate, unimplemented gap (buildings.md).

   Damage while growing [ours, unsure, not traced]: the growing part takes the damage first, like extra health
   on top of the hut; once it is gone, the hut falls back to its size before growing (the growth is lost) and
   then takes damage like any building. Not implemented.

   Wood held by size: uniform, not "+2 per size" as this entry's own earlier draft guessed to check - the
   trace's own "WOOD_HUT_2/_3 300" confirms every size costs the same 3 pieces / 300 wood
   (`BuildingKind::wood_cost`, already 3 for every hut size).
7. **Mana** [exe] (`GameMap::mana_income`, headcount.rs; every `MANA_UPDATE_FREQ` + 1 = 16 turns), per tribe: sum over its people of busy or idle
   mana (brave 15 / 4, warrior, spy, firewarrior and preacher 5 / 4, shaman 30 always, wildmen and angel 0), then
   x `HUMAN_` or `COMPUTER_MANA_ADJUST` (8.8 fixed point) - ours always uses `HUMAN_` (×1.25): no AI/controller
   type exists yet to pick `COMPUTER_` instead (same gap `training.rs`'s `train_cost` already has). "Busy" is
   probably inside a building or working: [ours] inside a hut, working on a building or fetching wood
   (`headcount::State::Housed`/`Working`). The income charges the spells first, the rest goes to the tribe's mana,
   capped at `MAX_MANA` (pop3-rev-analysis.md "Mana rules"). Praying at a totem is its own gauge (TODO "Praying"),
   not mana.

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
   shows the training progress, and the unit inside can be selected. In the original [exe] the bar is the training
   building's stored mana over its cost, blinking while it is short of mana (pop3-rev-analysis.md "The panel's
   training bar"). The exe disagrees on "one" (pop3-rev-analysis.md "The queue at the door"): the head goes in
   every 16 turns while fewer than 5 are inside, and they train together; not yet checked in the game.
4. **Mana** [exe]: no duration: the training building stores mana taken from each income before the spells (at
   most `cost / 32` per income), never from the bank, and trains when the store reaches the cost; so the more
   income, the faster. Cancelled, the store goes back to the tribe at 100 per turn (pop3-rev-analysis.md
   "Training buildings").
5. **Done** [exe]: the trainee is deleted and a new person of the hut's kind made at the door (new id, full life),
   keeping only its queued orders and the ghost flag. A small room by the door lets it come out while the next in
   the queue goes in. Out of the door it walks to the nearest free standing spot (never one taken by another
   unit, `GameMap::taken_spots`).
6. **Cancelling**: ordering the unit inside (move...) makes it come out by the door, its kind unchanged, then
   carry out the order; its training is cancelled and the queue goes on.
7. **Damage**: when the hut is damaged, the whole queue is cancelled (its units idle where they stand) and the
   unit inside comes out unchanged. Later, with combat and spells: in a fight the unit inside comes out to fight,
   hit by a spell it is handled like any unit.
8. While inside, a unit counts as busy for mana (rule 7) [ours]. In the people counts (headcount, ui-and-editor.md)
   the trainee and every queued unit are Working, never Idle [player].

### Done
`game_core::training`, `GameMap::tend_training` after the totems: a training unit holds one person
(`BuildingKind::capacity`); while someone is inside its cost is `train_cost(kind, count)` (`HUMAN_` costs for
every tribe, the band from the tribe's people of that kind), recomputed every 16 turns (`RECOST_EVERY`), and 0
with nobody inside (the store then given back). The mana distribution fills the store (`crate::production`); at
the cost the trainee is replaced (rule 5, ghost flag kept): the new person appears inside where the trainee was and
walks out by the door to a free spot, so the one going in and the one coming out are seen apart.

The queue (`training::tend_queues`, before `tend_training`; "The queue at the door" [exe]): sent into a training
unit (`Order::Enter`), a follower walks to its door; coming within `QUEUE_REACH` (2.6 cells per axis) of it, it goes
straight in when nobody queues, nobody else is let in and walking (`Building::admitted`) and there is room, else
joins the end of the queue (`Building::queue`) as `Action::Queuing`: the queue builds up in the order people arrive,
chained from the door. Each rank has its spot (`queue_spot`): a quarter cell apart (`QUEUE_STEP`) along the
walls at the door's distance, from beside the door (the doorway stays free) round the building; a full lap goes on
a row further out, turning the other way [ours: past one lap is not traced]. A spot on ground nobody can walk on
(another building, the sea) or already given gives way to the nearest free one within 2 cells, failing that one near
the door [user's account of the game]; someone with no way to its spot is stranded (arms up, losing health). The
spots are kept on the building (`queue_spots`, `spots_on`): extended as the queue grows, found again when the
ground changes. At its spot each one faces the one ahead (the head
faces the door) [ours]. Anyone given another order leaves the
queue and those behind move up at once [ours: the original ripples, 2 turns per rank]. The head goes in when there
is room and the admission gate is open: `ADMIT_GAP` (16) turns after one went in, `LEFT_GAP` (12) after one came
out [exe]. Queued people count as working (`headcount`). Not yet: queued orders carried over, those already of the
building's kind letting others pass, five inside training together (the exe's count), the computer players' cost,
damage sending the trainee out.

## Balance and game turns
The original counts in game turns, 12 per second (pop3-rev-analysis.md "Turns and timing"); one of our ticks is one turn.
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
- Mana per player is done (`game_core::mana::ManaPool` in `GameMap::mana`, spells.md "Mana"); `mana_income(people)`
  replaces its fixed `INCOME`. Later a tribe model holds it with `human` (TODO "Tribes").
- Pure functions with tests: `breed_gain(occupants)`, `breed_threshold(level, population, cap)`,
  `population_band(population, cap)`, `train_cost(kind, specialists, human)`, `train_share(cost, duration)`,
  `mana_income(people)`, `queue_spots(building, count)` (the snake around the footprint).
- `GameMap::tick`: huts and training huts in id order, then mana (income on update turns, then spell charges,
  then the bank); no `HashMap`, no randomness.
- `Order::Enter { building }`, `Command::Eject { player, building }`; wire codec kinds in multiplayer.md.

## Open questions
- Huts: what the population band is a percentage of (the tribe's cap assumed, or 200); what exactly counts as busy
  (idle people make 4 [exe]); the red bar's speed and occupant bonus; the wood held per hut size; how many braves fetch and build when a hut grows (only those inside?); what damage really does to a growing hut; whether idle braves house
  themselves, from how far, after how long.
- Training: the measured training time.
