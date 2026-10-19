//! Lava (pop3-rev-analysis.md "Volcano lava flow"), an effect (`effect::EffectKind::Lava`) shared between
//! Earthquake's fissure ("lava square", several short-lived single cells approximated here as one patch with a
//! shared blend clock, our own simplification since they would decay identically anyway) and Volcano's flow
//! ("lava flow", a downhill-following spread) — one `Lava` struct for both, distinguished by `Mode`.
//!
//! Every cell carries a `blend` (0-255, the original's own intensity value): **never an outright kill** — the
//! original only sets a hot cell's units on fire (`WorldEvent::Ignite`/`Unit::catch_fire`, state 31, which then
//! loses its own health over time) while `blend > IGNITE_BLEND`, and likewise sets alight any building there not
//! already burning, or its trees. `Mode::Static` (Earthquake's fissure): `blend` starts at `MATURE_BLEND`, decays
//! `STATIC_DECAY` a turn; once a cell drops below `GONE_BLEND` it's gone, scorched into the same permanent ash
//! (`WorldEvent::Scorch`, `GameMap::ashes` — docs/specs/combat.md "Ashes": "lava and earthquake ground ... share
//! the mark in the original") as a destroyed building's own, all its cells at once (they share one blend clock, so
//! they all cross `GONE_BLEND` the same turn). `Mode::Flowing` (Volcano's flow): starts as a small seed disc, each
//! turn every cell mature enough (`blend >= MATURE_BLEND`) pushes into its real downhill neighbour (source height
//! −2, target height +4, target `blend += BLEND_GAIN`) or, flat, deepens itself (height +10) — our own reading of
//! the per-cell downhill direction from the live terrain, since the original's own field source for it is not
//! traced ("Open"). An accumulator tracks how much spreading happened (+16 a push, −8 a flat step); growth ends at
//! `GROWTH_TURNS` or once the accumulator passes `ACCUMULATOR_CAP`, whichever first, then `FADE_TURNS` of fading
//! (`blend -= FADE_STEP` only, no more spread or burning), then the whole footprint becomes ash the same way, in
//! one shot, and the effect goes idle until its own (worst-case) outer lifetime ends it.
//!
//! A still-hot cell also panics anyone within `PANIC_RADIUS_CELLS` of it, not only whoever actually stands on one
//! (`panic_nearby`): our own addition, not confirmed in pop3-rev-analysis.md (only catching fire is).

use crate::effect::{DamageKind, Effect, EffectKind, TerrainChange};
use crate::event::WorldEvent;
use crate::map::GameMap;
use crate::time::{Countdown, Tick, Ticks};
use crate::unit::{cell_of, UnitKind};
use pop3_format::WORLD_UNITS_PER_CELL;
use std::collections::HashSet;

/// A cell's starting intensity ("its own 0xC4 starting value"), and the threshold a flowing cell must reach to
/// spread further itself.
pub const MATURE_BLEND: u8 = 196;
/// Above this, a cell still ignites whatever stands on it.
pub const IGNITE_BLEND: u8 = 159;
/// Below this (static mode only), a cell is gone.
pub const GONE_BLEND: u8 = 127;
/// How far a still-hot cell's heat alone scares people, beyond whoever actually stands on one (our own addition:
/// no panic-at-a-distance for lava is confirmed in pop3-rev-analysis.md "Volcano lava flow", only catching fire
/// is — a guess, untraced, like `volcano::SHAKE_RADIUS_CELLS`).
pub const PANIC_RADIUS_CELLS: f32 = 2.0;
/// A turn's push into a neighbour cell, added to its own blend.
const BLEND_GAIN: u8 = 32;
/// A static (Earthquake) cell's own decay, a turn.
const STATIC_DECAY: u8 = 2;
/// A fading (Volcano) cell's own decay, a turn.
const FADE_STEP: u8 = 8;
/// Its own worst case, generous by a few turns: `(MATURE_BLEND - GONE_BLEND) / STATIC_DECAY`, rounded up.
const STATIC_LIFETIME_TURNS: u32 = (MATURE_BLEND - GONE_BLEND) as u32 / STATIC_DECAY as u32 + 2;
/// A flowing spread's own seed, radius in cells ("radius = 3 internal units", confirmed).
pub const INITIAL_RADIUS_CELLS: f32 = 1.5;
/// Growth lasts at most this many turns (16.0 s, confirmed) before fading starts regardless.
pub const GROWTH_TURNS: u32 = 192;
/// Or ends sooner once the spread accumulator passes this (confirmed).
const ACCUMULATOR_CAP: i32 = 0x20000;
/// Fading lasts this many turns (1.3 s, confirmed) before the footprint becomes ash.
pub const FADE_TURNS: u32 = 16;
/// The whole flow's own worst case (confirmed: "up to 208 turns").
pub const TOTAL_FLOW_TURNS: u32 = GROWTH_TURNS + FADE_TURNS;
/// A spreading step's own height change: the source cell drops, the target rises.
const SPREAD_SOURCE_DROP: i32 = 2;
const SPREAD_TARGET_RISE: i32 = 4;
/// A flat (no-downhill) mature cell deepens itself instead of spreading.
const SELF_DEEPEN: i32 = 10;
const ACC_SPREAD: i32 = 16;
const ACC_FLAT: i32 = 8;

/// When and which way a flowing cell arrived (`Lava::cells_lit`, client-side only): the tick it was first pushed
/// into, and the direction back toward the neighbour it spread from (`-1..=1` each, never both zero). `None` for
/// a cell already present at a tick's start: the initial seed disc (`flowing`) and `Mode::Static`'s fixed fissure
/// (`effect`) both start at `MATURE_BLEND` with nothing to pour from; only an actual spread step (`grow`) sets it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CellArrival {
    pub born: Tick,
    pub from: (i32, i32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FlowCell {
    pos: (i32, i32),
    blend: u8,
    arrival: Option<CellArrival>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Growing { accumulator: i32, left: Countdown },
    Fading { left: Countdown },
    /// Already turned to ash: nothing left to do until the effect's own outer lifetime ends it.
    Done,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Mode {
    Static,
    Flowing(Phase),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lava {
    cells: Vec<FlowCell>,
    mode: Mode,
}

/// Earthquake's own fissure, lit by tribe `owner` at `at` (world units, its nominal position for the map/effect
/// list only), covering `cells` ("lava square", several approximated as one shared-blend patch: see module doc).
pub fn effect(owner: u8, at: (u16, u16), cells: Vec<(i32, i32)>) -> Effect {
    let cells = cells.into_iter().map(|pos| FlowCell { pos, blend: MATURE_BLEND, arrival: None }).collect();
    Effect::new(owner, at, Countdown::new(Ticks::new(STATIC_LIFETIME_TURNS)), EffectKind::Lava(Lava { cells, mode: Mode::Static }))
}

/// Volcano's own flow, lit by tribe `owner` once the cone finishes rising, seeded over `INITIAL_RADIUS_CELLS`
/// around `at` (world units) on a `size`-cell torus.
pub fn flowing(owner: u8, at: (u16, u16), size: i32) -> Effect {
    let cells = disc(at, INITIAL_RADIUS_CELLS, size).into_iter().map(|pos| FlowCell { pos, blend: MATURE_BLEND, arrival: None }).collect();
    let phase = Phase::Growing { accumulator: 0, left: Countdown::new(Ticks::new(GROWTH_TURNS)) };
    Effect::new(owner, at, Countdown::new(Ticks::new(TOTAL_FLOW_TURNS)), EffectKind::Lava(Lava { cells, mode: Mode::Flowing(phase) }))
}

/// The world-unit centre of cell `(x, z)`, wrapped to `size` cells.
fn cell_centre((x, z): (i32, i32), size: i32) -> (u16, u16) {
    let at = |c: i32| (c.rem_euclid(size) as u32 * WORLD_UNITS_PER_CELL + WORLD_UNITS_PER_CELL / 2) as u16;
    (at(x), at(z))
}

/// Our own reading of which way lava at `pos` flows (the original's own per-cell downhill field is not traced:
/// "Open" in pop3-rev-analysis.md "Volcano lava flow"): the lowest of its 8 neighbours on the real terrain, if
/// lower than `pos` itself (the target cell, and the pre-wrap `(dx, dz)` offset used to reach it — the caller's
/// own `CellArrival::from` is derived from this offset directly, not by re-subtracting two wrapped positions,
/// which would get the sign wrong right at the torus seam); None when `pos` is a local low point (flat, by the
/// trace's own terms).
fn downhill(world: &GameMap, pos: (i32, i32), size: i32) -> Option<((i32, i32), (i32, i32))> {
    let h = world.terrain.get(pos.0, pos.1);
    (-1..=1)
        .flat_map(|dz| (-1..=1).map(move |dx| (dx, dz)))
        .filter(|&d| d != (0, 0))
        .map(|(dx, dz)| ((dx, dz), ((pos.0 + dx).rem_euclid(size), (pos.1 + dz).rem_euclid(size))))
        .min_by_key(|&(_, (x, z))| world.terrain.get(x, z))
        .filter(|&(_, (x, z))| world.terrain.get(x, z) < h)
        .map(|(d, target)| (target, d))
}

impl Lava {
    /// Its current cells, for whatever draws them (`crates/game-client/src/lava.rs`'s `LavaView`).
    pub fn cells(&self) -> Vec<(i32, i32)> {
        self.cells.iter().map(|c| c.pos).collect()
    }

    /// The same cells with how hot each one still is (0-255, `MATURE_BLEND` full strength) and, for a cell an
    /// actual spread step added, when and which way it arrived (`CellArrival`, client-side only: `LavaView`'s own
    /// "pour-in" flourish, `pour_in_alpha`) — a freshly seeded or cooling edge cell is dimmer than an established
    /// one, for `LavaView`'s own alpha fade (no more popping a cell to full strength the instant it's added, or
    /// off the instant it's removed).
    pub fn cells_lit(&self) -> Vec<((i32, i32), u8, Option<CellArrival>)> {
        self.cells.iter().map(|c| (c.pos, c.blend, c.arrival)).collect()
    }

    /// One turn at `now`, cast by `owner`: grows or fades, then ignites whoever (or whatever) stands on a
    /// still-hot cell.
    pub fn turn(&mut self, now: Tick, owner: u8, world: &GameMap) -> Vec<WorldEvent> {
        let size = world.terrain.size() as i32;
        let mut events = match &mut self.mode {
            Mode::Static => {
                for c in &mut self.cells {
                    c.blend = c.blend.saturating_sub(STATIC_DECAY);
                }
                let gone: Vec<(i32, i32)> = self.cells.iter().filter(|c| c.blend < GONE_BLEND).map(|c| c.pos).collect();
                self.cells.retain(|c| c.blend >= GONE_BLEND);
                if gone.is_empty() { Vec::new() } else { vec![WorldEvent::Scorch { cells: gone }] }
            }
            Mode::Flowing(Phase::Growing { accumulator, left }) => {
                if left.tick() || *accumulator >= ACCUMULATOR_CAP {
                    self.mode = Mode::Flowing(Phase::Fading { left: Countdown::new(Ticks::new(FADE_TURNS)) });
                    Vec::new()
                } else {
                    self.grow(now, world, size)
                }
            }
            Mode::Flowing(Phase::Fading { left }) => {
                for c in &mut self.cells {
                    c.blend = c.blend.saturating_sub(FADE_STEP);
                }
                if left.tick() {
                    let cells = self.cells.drain(..).map(|c| c.pos).collect();
                    self.mode = Mode::Flowing(Phase::Done);
                    vec![WorldEvent::Scorch { cells }]
                } else {
                    Vec::new()
                }
            }
            Mode::Flowing(Phase::Done) => Vec::new(),
        };
        events.extend(self.ignite(owner, world, size));
        events
    }

    /// One growth step: every mature cell spreads downhill or deepens itself, flat.
    fn grow(&mut self, now: Tick, world: &GameMap, size: i32) -> Vec<WorldEvent> {
        let mut events = Vec::new();
        let (mut gained, mut spent) = (0i32, 0i32);
        let mature: Vec<(i32, i32)> = self.cells.iter().filter(|c| c.blend >= MATURE_BLEND).map(|c| c.pos).collect();
        for pos in mature {
            match downhill(world, pos, size) {
                Some((target, dir)) => {
                    events.push(WorldEvent::Terrain(TerrainChange::Raise { at: pos, radius: 0, by: -SPREAD_SOURCE_DROP }));
                    events.push(WorldEvent::Terrain(TerrainChange::Raise { at: target, radius: 0, by: SPREAD_TARGET_RISE }));
                    match self.cells.iter_mut().find(|c| c.pos == target) {
                        Some(c) => c.blend = c.blend.saturating_add(BLEND_GAIN),
                        None => self.cells.push(FlowCell { pos: target, blend: BLEND_GAIN, arrival: Some(CellArrival { born: now, from: (-dir.0, -dir.1) }) }),
                    }
                    gained += ACC_SPREAD;
                }
                None => {
                    events.push(WorldEvent::Terrain(TerrainChange::Raise { at: pos, radius: 0, by: SELF_DEEPEN }));
                    spent += ACC_FLAT;
                }
            }
        }
        if let Mode::Flowing(Phase::Growing { accumulator, .. }) = &mut self.mode {
            *accumulator += gained - spent;
        }
        events
    }

    /// Every cell still hot enough (`IGNITE_BLEND`) sets fire to whoever and whatever is on it, and scares anyone
    /// within `PANIC_RADIUS_CELLS` of it too.
    fn ignite(&self, owner: u8, world: &GameMap, size: i32) -> Vec<WorldEvent> {
        let mut events = Vec::new();
        let hot: Vec<(i32, i32)> = self.cells.iter().filter(|c| c.blend > IGNITE_BLEND).map(|c| c.pos).collect();
        for &pos in &hot {
            for u in world.units.iter().filter(|u| u.cell() == pos && crate::effect::hurts(u, DamageKind::Lava, Some(owner))) {
                events.push(WorldEvent::Ignite { unit: u.id });
            }
            let at = cell_centre(pos, size);
            if let Some(b) = world.buildings.iter().find(|b| b.covers(at, 0) && b.burning.is_none()) {
                events.push(WorldEvent::SetFire { building: b.id });
            }
            if world.trees.iter().any(|t| t.cell() == pos && t.wood() > 0) {
                events.push(WorldEvent::BurnTrees { cell: pos });
            }
        }
        events.extend(self.panic_nearby(owner, world, &hot, size));
        events
    }

    /// Anyone within `PANIC_RADIUS_CELLS` of a still-hot cell panics, not only whoever actually stands on one: a
    /// shield doesn't block it, same as lava's own ignite doesn't (like Swamp); only the caster's own shaman is
    /// spared. A unit already caught by `ignite` above is included here too (the halo covers the hot cells
    /// themselves) but that's harmless: `Unit::panic` backs off whenever `takes_orders` is false, which `OnFire`
    /// already excludes, so it never overrides the burn. A unit that panics *out of* range and wanders back onto a
    /// hot cell catches fire there the normal way, next turn's `ignite` pass.
    fn panic_nearby(&self, owner: u8, world: &GameMap, hot: &[(i32, i32)], size: i32) -> Vec<WorldEvent> {
        let halo: HashSet<(i32, i32)> = hot.iter().flat_map(|&pos| disc_cells(pos, PANIC_RADIUS_CELLS, size)).collect();
        world
            .units
            .iter()
            .filter(|u| u.is_alive() && u.in_the_open() && !(u.owner == owner && u.kind == UnitKind::Shaman))
            .filter(|u| halo.contains(&u.cell()))
            .map(|u| WorldEvent::Panic { unit: u.id })
            .collect()
    }
}

/// The cells within `radius_cells` of `centre` (wrapped on a `size`-cell torus): `disc`'s own core, also
/// `panic_nearby`'s halo around each hot cell.
fn disc_cells((cx, cz): (i32, i32), radius_cells: f32, size: i32) -> Vec<(i32, i32)> {
    let r = radius_cells.ceil() as i32;
    (-r..=r)
        .flat_map(|dz| (-r..=r).map(move |dx| (dx, dz)))
        .filter(|&(dx, dz)| ((dx * dx + dz * dz) as f32).sqrt() <= radius_cells)
        .map(|(dx, dz)| ((cx + dx).rem_euclid(size), (cz + dz).rem_euclid(size)))
        .collect()
}

/// The cells of a disc of radius `cells` (world units) around `at`: Volcano's own seed, and the test helpers
/// below.
pub fn disc(at: (u16, u16), radius_cells: f32, size: i32) -> Vec<(i32, i32)> {
    disc_cells((cell_of(at.0), cell_of(at.1)), radius_cells, size)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::building::{Building, BuildingKind};
    use crate::id::Id;
    use crate::status::{Timed, SHIELD_STEPS};
    use crate::terrain::Heightmap;
    use crate::time::Ticks;
    use crate::tree::Tree;
    use crate::unit::{Action, UnitKind};

    const AT: (u16, u16) = (10 * WORLD_UNITS_PER_CELL as u16 + 256, 20 * WORLD_UNITS_PER_CELL as u16 + 256);

    fn bare() -> GameMap {
        // Dry land, not the default all-zero (sea) heightmap: a unit standing on sea is forced into Drowning every
        // tick regardless of its own action, which would mask `OnFire` entirely.
        let mut terrain = Heightmap::new(128);
        for z in 0..128 {
            for x in 0..128 {
                terrain.set(x, z, 500);
            }
        }
        GameMap::bare("lava-test", terrain, Vec::new())
    }

    #[test]
    fn disc_covers_a_round_area_wrapped_at_the_edge() {
        let cells = disc((0, 0), 1.5, 128);
        assert!(cells.contains(&(0, 0)) && cells.contains(&(1, 0)) && cells.contains(&(0, 1)));
        assert!(!cells.contains(&(2, 0)), "outside the radius");
        assert!(cells.contains(&(127, 0)), "wraps past the edge");
    }

    #[test]
    fn ignites_everyone_on_its_cells_any_tribe_shielded_or_inside_a_building_but_spares_the_casters_shaman() {
        let mut map = bare();
        map.add_effect(effect(0, AT, vec![(10, 20)]));
        let own = map.spawn(0, UnitKind::Brave, AT).id;
        let foe = map.spawn(1, UnitKind::Brave, AT).id;
        let shielded = map.spawn(1, UnitKind::Brave, AT).id;
        map.unit_mut(shielded).unwrap().statuses.shield = Timed::steps(SHIELD_STEPS);
        let inside = map.spawn(1, UnitKind::Brave, AT).id;
        map.unit_mut(inside).unwrap().inside = Some(crate::unit::Inside { building: Id::from_raw(999), door: AT });
        let shaman = map.spawn(0, UnitKind::Shaman, AT).id;
        map.tick();
        for id in [own, foe, shielded, inside] {
            assert!(matches!(map.unit(id).unwrap().action, Action::OnFire { .. }), "none spared, caught fire not killed outright: {id:?}");
        }
        assert!(!matches!(map.unit(shaman).unwrap().action, Action::OnFire { .. }), "the caster's own shaman is spared");
    }

    fn cell_pos(cell: (i32, i32)) -> (u16, u16) {
        (cell.0 as u16 * WORLD_UNITS_PER_CELL as u16 + 256, cell.1 as u16 * WORLD_UNITS_PER_CELL as u16 + 256)
    }

    #[test]
    fn panics_anyone_within_panic_radius_cells_of_a_hot_cell_not_only_whoever_stands_on_it() {
        let mut map = bare();
        map.add_effect(effect(0, AT, vec![(10, 20)]));
        let near = map.spawn(1, UnitKind::Brave, cell_pos((11, 20))).id;
        let far = map.spawn(1, UnitKind::Brave, cell_pos((14, 20))).id;
        map.tick();
        assert!(matches!(map.unit(near).unwrap().action, Action::Panicking { .. }), "scared by the heat alone, one cell away");
        assert!(!matches!(map.unit(near).unwrap().action, Action::OnFire { .. }), "scared, not burned: never stood on the hot cell");
        assert!(!matches!(map.unit(far).unwrap().action, Action::Panicking { .. }), "too far to be scared");
    }

    #[test]
    fn a_shield_does_not_block_the_panic_halo_but_the_casters_own_shaman_is_still_spared() {
        let mut map = bare();
        map.add_effect(effect(0, AT, vec![(10, 20)]));
        let near = cell_pos((11, 20));
        let shielded = map.spawn(1, UnitKind::Brave, near).id;
        map.unit_mut(shielded).unwrap().statuses.shield = Timed::steps(SHIELD_STEPS);
        let shaman = map.spawn(0, UnitKind::Shaman, near).id;
        map.tick();
        assert!(matches!(map.unit(shielded).unwrap().action, Action::Panicking { .. }), "a shield doesn't block it, like lava's own ignite");
        assert!(!matches!(map.unit(shaman).unwrap().action, Action::Panicking { .. }), "the caster's own shaman is spared");
    }

    #[test]
    fn never_kills_outright_only_catches_fire() {
        let mut map = bare();
        map.add_effect(effect(0, AT, vec![(10, 20)]));
        let unit = map.spawn(1, UnitKind::Brave, AT).id;
        map.tick();
        assert!(map.unit(unit).unwrap().is_alive(), "set alight, not struck dead");
    }

    #[test]
    fn sets_a_building_and_a_tree_on_its_cells_alight() {
        let mut map = bare();
        let hut = map.add_building(Building::new(BuildingKind::Hut { size: 1 }, 1, AT.0 - 256, AT.1 - 256, 0));
        map.trees.push(Tree::new((10, 20), 1, 2));
        map.add_effect(effect(0, AT, vec![(10, 20)]));
        map.tick();
        assert!(map.building(hut).unwrap().burning.is_some(), "the building catches fire");
        assert_eq!(map.trees[0].wood(), 0, "the tree is burning: no wood while it does");
    }

    #[test]
    fn a_static_patch_decays_and_is_gone_once_below_gone_blend() {
        let mut map = bare();
        let id = map.add_effect(effect(0, AT, vec![(10, 20)]));
        map.run(Ticks::new(STATIC_LIFETIME_TURNS));
        assert!(!map.effects.contains(id), "decayed away");
    }

    #[test]
    fn a_static_patch_leaves_the_same_permanent_ash_scorch_once_its_cells_cool() {
        let mut map = bare();
        let cell = (cell_of(AT.0), cell_of(AT.1));
        map.add_effect(effect(0, AT, vec![cell]));
        let turns_to_cool = (MATURE_BLEND - GONE_BLEND) as u32 / STATIC_DECAY as u32 + 1;
        map.run(Ticks::new(turns_to_cool));
        assert!(!map.ashes.buildable(cell), "ash left behind, same as a flowing patch's own");
    }

    #[test]
    fn cells_lit_reports_each_cell_s_own_blend_starting_at_mature_with_no_arrival() {
        let mut map = bare();
        let id = map.add_effect(effect(0, AT, vec![(10, 20)]));
        let lit = { let EffectKind::Lava(lava) = &map.effects.get(id).unwrap().kind else { panic!("not lava") }; lava.cells_lit() };
        assert_eq!(lit, vec![((10, 20), MATURE_BLEND, None)]);
    }

    #[test]
    fn the_initial_seed_disc_carries_no_arrival_data() {
        let mut map = bare();
        let id = map.add_effect(flowing(0, AT, 128));
        let lit = { let EffectKind::Lava(lava) = &map.effects.get(id).unwrap().kind else { panic!("not lava") }; lava.cells_lit() };
        assert!(!lit.is_empty());
        assert!(lit.iter().all(|&(_, _, arrival)| arrival.is_none()), "nothing to pour from yet: the seed disc itself");
    }

    #[test]
    fn a_cell_added_by_an_actual_spread_step_records_its_own_born_tick_and_entry_direction() {
        let mut map = bare();
        let (cx, cz) = (cell_of(AT.0), cell_of(AT.1));
        // `INITIAL_RADIUS_CELLS` (1.5) seeds the whole 3x3 block around the centre, so only a target reachable
        // from exactly one of its cells gives an unambiguous entry direction: the seed's own SE corner,
        // (cx+1, cz+1), is the only seeded cell with (cx+2, cz+2) among its 8 neighbours.
        map.terrain.set(cx + 2, cz + 2, 300);
        let id = map.add_effect(flowing(0, AT, 128));
        map.tick();
        let lit = { let EffectKind::Lava(lava) = &map.effects.get(id).unwrap().kind else { panic!("not lava") }; lava.cells_lit() };
        let (_, _, arrival) = lit.iter().find(|&&(pos, ..)| pos == (cx + 2, cz + 2)).expect("spread there");
        assert_eq!(*arrival, Some(CellArrival { born: map.now, from: (-1, -1) }), "arrived from the seed's own SE corner, the tick it was pushed into");
    }

    #[test]
    fn a_static_patch_stops_igniting_once_below_ignite_blend_but_is_not_gone_yet() {
        let mut map = bare();
        map.add_effect(effect(0, AT, vec![(10, 20)]));
        let turns_to_cool = (MATURE_BLEND - IGNITE_BLEND) as u32 / STATIC_DECAY as u32 + 1;
        map.run(Ticks::new(turns_to_cool));
        let unit = map.spawn(1, UnitKind::Brave, AT).id;
        map.tick();
        assert_eq!(map.unit(unit).unwrap().action, Action::Idle, "too cool to ignite by now");
    }

    #[test]
    fn flowing_spreads_downhill_not_uphill() {
        let mut map = bare();
        let (cx, cz) = (cell_of(AT.0), cell_of(AT.1));
        // Outside the seed disc's own 3x3 block (`INITIAL_RADIUS_CELLS` reaches the diagonals too), so these only
        // get added by an actual spread step, from the seed's own edge cells.
        map.terrain.set(cx + 2, cz, 300); // lower, two cells east: within reach of the seed's own eastern edge
        map.terrain.set(cx - 2, cz, 700); // higher, two cells west: never reached
        let id = map.add_effect(flowing(0, AT, 128));
        map.tick();
        let cells = lava_cells(&map, id);
        assert!(cells.contains(&(cx + 2, cz)), "spread downhill, from the seed's own edge");
        assert!(!cells.contains(&(cx - 2, cz)), "never spread uphill");
        assert!(map.terrain.get(cx + 2, cz) > 300, "the target rose");
    }

    fn lava_cells(map: &GameMap, id: crate::id::Id<Effect>) -> Vec<(i32, i32)> {
        let EffectKind::Lava(lava) = &map.effects.get(id).unwrap().kind else { panic!("not lava") };
        lava.cells()
    }

    #[test]
    fn flowing_keeps_growing_even_on_flat_ground() {
        // Self-deepening breaks the tie: a flat mature cell that deepens itself becomes relatively higher than
        // its still-flat neighbours the very next turn, so it starts spreading onto them too — flat ground does
        // not stay inert for long, confirmed by this growing (never shrinking, during growth) cell count.
        let mut map = bare();
        let id = map.add_effect(flowing(0, AT, 128));
        let seeded = lava_cells(&map, id).len();
        map.run(Ticks::new(10));
        assert!(lava_cells(&map, id).len() > seeded, "grew past its own seed");
    }

    #[test]
    fn flowing_leaves_a_permanent_ash_scorch_once_it_ends() {
        let mut map = bare();
        map.add_effect(flowing(0, AT, 128));
        let cell = (cell_of(AT.0), cell_of(AT.1));
        map.run(Ticks::new(TOTAL_FLOW_TURNS));
        assert!(!map.ashes.buildable(cell), "ash left behind, nothing builds there");
    }
}
