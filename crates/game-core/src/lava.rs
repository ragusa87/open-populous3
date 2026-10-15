//! Lava (pop3-rev-analysis.md "Components" "Lava"), an effect (`effect::EffectKind::Lava`): no terrain change of
//! its own, no single kill cap like Swamp — a set of cells, hot for its whole lifetime. Shared between
//! Earthquake's fissure and Volcano's flow (docs/plans/spells.md "Earthquake"), each giving it its own cells and
//! lifetime at construction, rather than two separate implementations. Every turn, every living person on its
//! cells (open or inside a building) dies outright (`DamageKind::Lava`: `effect::hurts` never gates it, shield or
//! no shield, any tribe but the caster's own shaman — pop3-rev-analysis.md "Friendly fire"); every building on its
//! cells not already burning catches fire (`WorldEvent::SetFire`, the ordinary fire/evacuation path in `fire.rs`
//! then runs its course); every tree on its cells with wood left burns (`WorldEvent::BurnTrees`). Its own ground
//! look (`crates/game-client/src/lava.rs`'s `LavaView`, procedural lava/crust/basalt/ash textures draped over
//! `cells`) reads `cells` back through the accessor below, live each frame, so a growing `flowing` patch is drawn
//! correctly without the client knowing anything about it.
//!
//! pop3-rev-analysis.md's own effect-type dump (around "lava flow 33, lava square 38") names "lava flow" as a
//! *distinct* effect from this one ("lava square") and has Volcano use it, not traced any further (no shape or
//! spread speed given). `flowing` is our own invented stand-in for it, a guess tagged as such: the hot area simply
//! grows outward from a starting radius to a larger one over part of its life, rather than the fixed patch `effect`
//! gives Earthquake's fissure — to be replaced once a spec session traces the original's actual "lava flow".

use crate::effect::{DamageKind, Effect, EffectKind};
use crate::event::WorldEvent;
use crate::map::GameMap;
use crate::time::Countdown;
use crate::unit::cell_of;
use pop3_format::WORLD_UNITS_PER_CELL;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lava {
    cells: Vec<(i32, i32)>,
    flow: Option<Flow>,
}

/// `flowing`'s own growth, advanced one tick at a time (see the module doc: our own invented guess). Radii are
/// kept ×10 so no float is stored (`AGENTS.md`: deterministic state stays integers), only used transiently in
/// `radius` to recompute `cells` each turn.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Flow {
    at: (u16, u16),
    size: i32,
    start_x10: i32,
    max_x10: i32,
    grow_ticks: u32,
    elapsed: u32,
}

impl Flow {
    fn radius(&self) -> f32 {
        let t = (self.elapsed as f32 / self.grow_ticks.max(1) as f32).min(1.0);
        (self.start_x10 as f32 + (self.max_x10 - self.start_x10) as f32 * t) / 10.0
    }
}

/// Lit by tribe `owner` at `at` (world units, its nominal position for the map/effect list only: what actually
/// burns is `cells`), covering `cells`, for `lifetime`.
pub fn effect(owner: u8, at: (u16, u16), cells: Vec<(i32, i32)>, lifetime: Countdown) -> Effect {
    Effect::new(owner, at, lifetime, EffectKind::Lava(Lava { cells, flow: None }))
}

/// Our own invented "flowing out of the crater" variant (see the module doc): starts as a `start`-cell-radius
/// disc around `at` and grows to `max` cells radius over `grow_ticks`, then holds there for the rest of
/// `lifetime`.
pub fn flowing(owner: u8, at: (u16, u16), start: f32, max: f32, grow_ticks: u32, size: i32, lifetime: Countdown) -> Effect {
    let flow = Flow { at, size, start_x10: (start * 10.0).round() as i32, max_x10: (max * 10.0).round() as i32, grow_ticks, elapsed: 0 };
    let cells = disc(at, flow.radius(), size);
    Effect::new(owner, at, lifetime, EffectKind::Lava(Lava { cells, flow: Some(flow) }))
}

/// The world-unit centre of cell `(x, z)`, wrapped to `size` cells.
fn cell_centre((x, z): (i32, i32), size: i32) -> (u16, u16) {
    let at = |c: i32| (c.rem_euclid(size) as u32 * WORLD_UNITS_PER_CELL + WORLD_UNITS_PER_CELL / 2) as u16;
    (at(x), at(z))
}

impl Lava {
    /// Its current cells (growing, for `flowing`), for whatever draws them
    /// (`crates/game-client/src/lava.rs`'s `LavaView`).
    pub fn cells(&self) -> &[(i32, i32)] {
        &self.cells
    }

    /// One turn, cast by `owner`: grows `cells` first if it's a `flowing` patch, then everyone standing on them
    /// dies, every building and tree there catches fire.
    pub fn turn(&mut self, owner: u8, world: &GameMap) -> Vec<WorldEvent> {
        if let Some(flow) = &mut self.flow {
            flow.elapsed += 1;
            self.cells = disc(flow.at, flow.radius(), flow.size);
        }
        let size = world.terrain.size() as i32;
        let mut events = Vec::new();
        for &cell in &self.cells {
            for u in world.units.iter().filter(|u| u.is_alive() && u.cell() == cell) {
                events.push(WorldEvent::Damage { unit: u.id, amount: u16::MAX, kind: DamageKind::Lava, from: Some(owner) });
            }
            let at = cell_centre(cell, size);
            if let Some(b) = world.buildings.iter().find(|b| b.covers(at, 0) && b.burning.is_none()) {
                events.push(WorldEvent::SetFire { building: b.id });
            }
            if world.trees.iter().any(|t| t.cell() == cell && t.wood() > 0) {
                events.push(WorldEvent::BurnTrees { cell });
            }
        }
        events
    }
}

/// The cells of a disc of radius `cells` (world units) around `at`: Earthquake's fissure and Volcano's crater each
/// build their own shape; this is the simple round one both start from.
pub fn disc(at: (u16, u16), radius_cells: f32, size: i32) -> Vec<(i32, i32)> {
    let (cx, cz) = (cell_of(at.0), cell_of(at.1));
    let r = radius_cells.ceil() as i32;
    (-r..=r)
        .flat_map(|dz| (-r..=r).map(move |dx| (dx, dz)))
        .filter(|&(dx, dz)| ((dx * dx + dz * dz) as f32).sqrt() <= radius_cells)
        .map(|(dx, dz)| ((cx + dx).rem_euclid(size), (cz + dz).rem_euclid(size)))
        .collect()
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
    use crate::unit::UnitKind;

    const AT: (u16, u16) = (10 * WORLD_UNITS_PER_CELL as u16 + 256, 20 * WORLD_UNITS_PER_CELL as u16 + 256);

    fn bare() -> GameMap {
        GameMap::bare("lava-test", Heightmap::new(128), Vec::new())
    }

    #[test]
    fn disc_covers_a_round_area_wrapped_at_the_edge() {
        let cells = disc((0, 0), 1.5, 128);
        assert!(cells.contains(&(0, 0)) && cells.contains(&(1, 0)) && cells.contains(&(0, 1)));
        assert!(!cells.contains(&(2, 0)), "outside the radius");
        assert!(cells.contains(&(127, 0)), "wraps past the edge");
    }

    #[test]
    fn kills_everyone_on_its_cells_any_tribe_shielded_or_inside_a_building_but_spares_the_casters_shaman() {
        let mut map = bare();
        map.add_effect(effect(0, AT, vec![(10, 20)], Countdown::new(Ticks::new(4))));
        let own = map.spawn(0, UnitKind::Brave, AT).id;
        let foe = map.spawn(1, UnitKind::Brave, AT).id;
        let shielded = map.spawn(1, UnitKind::Brave, AT).id;
        map.unit_mut(shielded).unwrap().statuses.shield = Timed::steps(SHIELD_STEPS);
        let inside = map.spawn(1, UnitKind::Brave, AT).id;
        map.unit_mut(inside).unwrap().inside = Some(crate::unit::Inside { building: Id::from_raw(999), door: AT });
        let shaman = map.spawn(0, UnitKind::Shaman, AT).id;
        map.tick();
        for id in [own, foe, shielded, inside] {
            assert!(!map.unit(id).unwrap().is_alive(), "none spared: {id:?}");
        }
        assert!(map.unit(shaman).unwrap().is_alive(), "the caster's own shaman is spared");
    }

    #[test]
    fn sets_a_building_and_a_tree_on_its_cells_alight() {
        let mut map = bare();
        let hut = map.add_building(Building::new(BuildingKind::Hut { size: 1 }, 1, AT.0 - 256, AT.1 - 256, 0));
        map.trees.push(Tree::new((10, 20), 1, 2));
        map.add_effect(effect(0, AT, vec![(10, 20)], Countdown::new(Ticks::new(4))));
        map.tick();
        assert!(map.building(hut).unwrap().burning.is_some(), "the building catches fire");
        assert_eq!(map.trees[0].wood(), 0, "the tree is burning: no wood while it does");
    }

    fn lava_cells(map: &GameMap, id: Id<Effect>) -> Vec<(i32, i32)> {
        let EffectKind::Lava(lava) = &map.effects.get(id).unwrap().kind else { panic!("not lava") };
        lava.cells().to_vec()
    }

    #[test]
    fn flowing_starts_at_the_start_radius_and_grows_to_the_max_one() {
        let mut map = bare();
        let id = map.add_effect(flowing(0, AT, 1.0, 3.0, 10, 128, Countdown::new(Ticks::new(20))));
        assert_eq!(lava_cells(&map, id), disc(AT, 1.0, 128), "starts small, before any turn runs");
        for _ in 0..10 {
            map.tick();
        }
        assert_eq!(lava_cells(&map, id), disc(AT, 3.0, 128), "fully grown after grow_ticks turns");
    }

    #[test]
    fn flowing_holds_at_the_max_radius_once_grown() {
        let mut map = bare();
        let id = map.add_effect(flowing(0, AT, 1.0, 3.0, 4, 128, Countdown::new(Ticks::new(20))));
        for _ in 0..10 {
            map.tick();
        }
        assert_eq!(lava_cells(&map, id), disc(AT, 3.0, 128), "stays at the max once reached, not beyond");
    }

    #[test]
    fn a_fixed_patch_from_effect_never_grows() {
        let mut map = bare();
        let id = map.add_effect(effect(0, AT, disc(AT, 1.0, 128), Countdown::new(Ticks::new(20))));
        for _ in 0..10 {
            map.tick();
        }
        assert_eq!(lava_cells(&map, id), disc(AT, 1.0, 128), "effect() gives a fixed patch, unlike flowing()");
    }
}
