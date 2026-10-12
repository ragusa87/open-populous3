# Coordinators

Short-lived simulation objects that coordinate several units: a sermon, a fight, a formation, a guard post's
control. Only the infrastructure exists (`game_core::coordinator`); no real kind does. Each kind will get its own
spec and rules on top of this.

Tags: **[exe]** the original executable (pop3-rev-analysis.md), **[wiki]** the Populous Wiki (Object Data),
**[ours]** a choice made here.

## Name
The original calls them "internal" things (class 10, invisible, in the same pool as units) [exe]; pop3-rev-analysis.md
says "controller units". We say **coordinator**: "controller" already names a tribe's player type (human or
computer, pop3-rev-analysis.md "Mana rules"), and will name whoever plays an AI tribe.

## The pattern [ours]
- **A stable id**, typed (`id::Id<C>` from `GameMap::new_id_of`), never given again.
- **Created when a situation starts** (an enemy in a preacher's range, two enemies meeting), by a scan of the map
  (step a below).
- **Units link to it one way**, through their action (e.g. a future `Action::Listening { sermon }`;
  `Action::link` returns it as a `coordinator::Link`). The coordinator stores no list of its units: "the units of
  X" is computed when needed (`coordinator::participants`).
- **Ticked each turn**: it checks its own conditions, decides, and returns a `Step { finished, events }`. It reads
  the map, changes only itself, and says what it does to others as `WorldEvent`s.
- **Removed when the situation ends** (`finished`). Its units find its id gone and are released
  (`GameMap::release_orphans`); nobody notifies anybody.

## Types
| Type | Where | What |
|---|---|---|
| `Id<T>` | `id.rs` | `u32` with the kind in its type: an `Id<Fight>` does not fit where an `Id<Sermon>` is expected. `raw` / `from_raw` for the wire and tests. |
| `Registry<T>` | `id.rs` | Objects of one kind by id, a `Vec` sorted by id: `insert` (a fresh id only), `get`, `get_mut`, `contains`, `iter` in id order, `retain`. Clone and PartialEq, no HashMap. Effects use it too. |
| `Coordinator` | `coordinator.rs` | The trait of a kind: `tick(&mut self, id, now, &GameMap, &mut Lcg) -> Step`. |
| `Coordinators` | `coordinator.rs` | `GameMap::coordinators`: one `Registry` per kind. `tick` runs the kinds in one written order (`tick_kind` per kind); `exists(Link)`. |
| `Link` | `coordinator.rs` | What a unit's action points at: one variant per kind, wrapping its `Id`. |
| `participants` | `coordinator.rs` | The reverse view: the units linked to each coordinator of one kind, built once when needed (who is still in it; AI script counts later). |
| `GameMap::linked` | `map.rs` | A unit's link, only if its coordinator still exists: every action using a coordinator checks removal the same way. |

Today only a test kind exists (`Gathering`, with `Action::Gathered`, both `#[cfg(test)]`): it holds its units for
some turns, may hurt them, then ends, releasing them.

## World events
Coordinators, effects and later other emitters return `event::WorldEvent`s; `GameMap::apply_event` holds the one rule
for each variant:

| Variant | Rule |
|---|---|
| `Damage { unit, amount, kind, from }` | `GameMap::damage`, the one hurt path (`effect::hurts`: shielded units, the attacker's tribe for a blast); `from` the attacker's tribe, if any |
| `Push { unit, vel }` | `Unit::fling`, not the shielded |
| `Panic { unit }`, `Raid { building }`, `SetFire { building }` | routed, no behaviour yet |
| `SpawnEffect(Effect)` | a new effect, ticking from the next turn |
| `Terrain(TerrainChange)` | raise or flatten ground |
| `Release { unit }` | the unit leaves its coordinator: back to Idle (`Unit::release`); nothing if it is in none or gone |

Expected later, when their kinds come: `ChangeOwner { unit, tribe }` (conversion), `SpawnCoordinator(..)` (a
pre-fight starting a fight).

Rules:
- Emitters say **what happened** (`Release`, `Panic`); the map decides the resulting action. No `SetAction`.
- Events apply in the order their emitters ticked, and in the order each emitter produced them. Events caused while
  applying one go after the current batch, never in its middle (`event::run_in_order`).
- A kind-specific event is allowed only for what no other system shares; it becomes a `WorldEvent` before being
  applied. `UnitEvent` stays: its variants (`Died`, `Gone`, `Reincarnated`, `PutDown`, `Chopped`, `PickedUp`,
  `Jumped`, `Built`, `AtDoor`) are a unit's own outcome, applied with that unit's context. `Jumped` could later become
  a `Terrain` event.
- Not in the hurt path yet: the last attacker, the death cause, and death held off while tumbling. They go into
  `GameMap::damage` when added, so no emitter repeats them (TODO).

## In the tick
In `GameMap::tick`, after camp fires, vaults and totems, before units (architecture.md "The tick"):
1. `detect_coordinators`: scans that start coordinators or add units to them, one per kind (none yet);
2. `tend_coordinators`: each coordinator's turn (`mem::take` of the list, the map read as it was, the rng handed
   in turn), kind after kind in `Coordinators::tick`'s order;
3. the finished ones removed;
4. their events applied in order;
5. `release_orphans`: units whose coordinator is gone back to Idle.

Then the units tick: a coordinator that ends this turn is already gone, and its units already free.

## Ids of units and buildings
Units and buildings already have stable ids (`GameMap::new_id`): their lists stay sorted by id, lookups are binary
searches, and units leave the list during the game. Indices are only valid within a turn. Their ids are still raw
`u32`s. The first real coordinator will store units (a sermon's preacher), so they become `Id<Unit>` and
`Id<Building>` before it, in a commit of their own. That touches `Unit::id`, `UnitEvent::AtDoor`, every `unit: u32`
in `WorldEvent` and `Command` (`OrderUnit`, `QueueOrder`), the game-net codec, `work.rs`, `enter.rs`, `worship.rs`,
`occupancy.rs`, `blast.rs`, and the client's selection, stats, views and dev tools.

## The original's internal models [wiki, exe]
Class 10 things in the original. All **Unverified** for our purposes: the names come from the wiki, the roles partly
from the executable (pop3-rev-analysis.md "Combat").

| Model | Name [wiki] | What we know | Will become |
|---|---|---|---|
| 1 | Formation | `process_formation_unit`, `add_unit_to_formation` [exe] | a formation coordinator (group moves) |
| 2 | Beacon | heads a list of its own in the tribe struct [exe] | unknown |
| 3 | Thing Info Display | name only | probably nothing (display) |
| 4 | Soul Convert | soul convert [exe] | conversion (with `ChangeOwner`) |
| 5 | Soul Man | name only | unknown |
| 6 | Preacher Attract | **conflict**: the wiki says Preacher Attract, pop3-rev-analysis.md groups 10/1 and 10/6 as formations; not resolved | a sermon, or a formation |
| 7 | Object Face | name only | unknown |
| 8 | Fight | fight; people link to it (`fight_units`) [exe] | a fight coordinator |
| 9 | Pre-Fight | pre-fight (`pre_fight_units`) [exe] | a pre-fight coordinator starting fights |
| 10 | Guard Control | guard control [exe] | a guard post's control |
| 11 | Bridge Control | name only | unknown (land bridge?) |
| 12 | Soul Convert 2 | soul convert [exe] | conversion |
| 13 | DT Beacon | name only (drum tower?) | unknown |
| 14 | Player Raise | name only | unknown |

We keep the concept, not the layout: one typed registry per kind instead of one pool of things.
