//! Volcano (pop3-rev-analysis.md "Per spell" 16), a multi-phase controller effect (docs/plans/spells.md "Volcano":
//! "should be structured as a multi-phase stateful effect ... same spirit as `LandBridge`/`Flatten` ... just with
//! more phases"), `TOTAL_TURNS` (160) long:
//! - `RUMBLE_TURNS` (80): rumbling only, no terrain change yet — drawn client-side as a trembling ground patch
//!   (`ground_shake.rs` [client], `ground_shake`, below, shared with Earthquake's own shake the same way `lava`
//!   is shared by their aftermaths; its magnitude and reach are our own guess, untraced).
//! - The next `RISE_TURNS` (59), ending at `SHAPING_TURNS` (139): the cone rises, one `TerrainChange::Volcano` turn
//!   at a time (`apply`/`step`, below) — a crater within `RIM_RADIUS_CELLS` (2.5), a real depression (not the
//!   trace's "plug": docs/plans/spells.md's correction), down to `CRATER_FLOOR` (0, a guess: not given a number,
//!   mirroring the rim's own absolute extreme) at the very centre, rising to the engine's absolute `MAX_HEIGHT`
//!   (1024) right at the rim; outside that, sloping back down to the ground's own height when cast by
//!   `SLOPE_RADIUS_CELLS` (5).
//! - At `DESTROY_AT` (110, 30 turns into the rise): every building in `DESTROY_AREA_CELLS` (6×6) cells is destroyed
//!   outright (`WorldEvent::Destroy`, bypassing the normal wood/wear-down model, accepted per the plan) and every
//!   person there (open or inside, but the caster's own shaman) catches fire (`WorldEvent::Ignite`, confirmed: not
//!   an outright kill either, same correction as `lava`'s own doc) — otherwise invisible until the crater finishes
//!   rising, so the area glows first (`pre_glow`, below, drawn client-side by `lava.rs` [client] reusing its own
//!   molten look at partial strength): a guess, untraced, the trace gives no such cue.
//! - Once shaping ends (`SHAPING_TURNS`), a `lava` flow is spawned over the crater's own cells (`lava::flowing`,
//!   its own growth/fade/ash lifecycle, up to 208 turns).
//! - `FIREBALLS` (28, "about 22 small and 6 big": mechanically identical here, the split is visual only and not
//!   given a damage difference) land one `panicking_blast` at a time at random points in the 6×6 area, spread
//!   evenly (ceiling division) over `FIREBALL_START`-`FIREBALL_END` (cast turns 115-150: pop3-rev-analysis.md
//!   "Volcano rocks", `ebp` in `[0x23,0x46]` + the 80-turn offset) — overlapping but not identical to the rocks'
//!   own 105-130 window below, and starting while the cone is still rising (before `SHAPING_TURNS`). A pure
//!   scheduler for its own sub-effects from here on, same spirit as Firestorm's (docs/plans/spells.md
//!   "Firestorm"); both the lava and every fireball outlive the controller itself once it ends.
//!
//! The 26 decorative rocks (pop3-rev-analysis.md "Volcano rocks"), turns 105-130, are drawn client-side only: no
//! effect spawned, no damage, nothing for the simulation to do.

use crate::effect::{DamageKind, Effect, EffectKind, TerrainChange};
use crate::event::WorldEvent;
use crate::map::{GameMap, Lcg};
use crate::spell::Cell;
use crate::terrain::{DirtyRect, Heightmap, MAX_HEIGHT};
use crate::time::{Countdown, Ticks};
use crate::unit::cell_of;
use pop3_format::WORLD_UNITS_PER_CELL;

pub const RUMBLE_TURNS: u32 = 80;
pub const RISE_TURNS: u32 = 59;
pub const SHAPING_TURNS: u32 = RUMBLE_TURNS + RISE_TURNS;
pub const TOTAL_TURNS: u32 = 160;
/// 30 turns into the rise (confirmed: "falls *inside* that rise phase", docs/plans/spells.md).
pub const DESTROY_AT: u32 = 110;
/// Half the side of the 6 × 6 destroyed area.
pub const DESTROY_AREA_CELLS: i32 = 3;
pub const RIM_RADIUS_CELLS: f32 = 2.5;
pub const SLOPE_RADIUS_CELLS: f32 = 5.0;
/// The crater's own floor (a guess, not given a number: mirrors `MAX_HEIGHT`, the rim's own absolute extreme).
pub const CRATER_FLOOR: u16 = 0;
/// "about 22 small and 6 big fireballs": 28 total, not mechanically distinguished (see module doc).
pub const FIREBALLS: u32 = 22 + 6;
/// Cast turns 115-150 (`ebp` 35-70 + 80, pop3-rev-analysis.md "Volcano rocks"): overlaps but does not match the
/// rocks' own 105-130 window (`volcano_rocks.rs` [client]).
pub const FIREBALL_START: u32 = 115;
pub const FIREBALL_END: u32 = 150;
const _: () = assert!(FIREBALL_START < SHAPING_TURNS, "fireballs begin while the cone is still rising");
/// How far the pre-eruption rumble (`ground_shake`) is felt (a guess, untraced: roughly twice `DESTROY_AREA_CELLS`).
pub const SHAKE_RADIUS_CELLS: f32 = 6.0;
/// How long the pre-eruption glow (`pre_glow`) ramps up before `DESTROY_AT`, and fades out after it (a guess,
/// untraced: a visual cue for the otherwise invisible instant ignite there, the trace gives no such cue).
const GLOW_BEFORE: u32 = 10;
const GLOW_AFTER: u32 = 20;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Volcano {
    /// The ground's own height at the centre when cast: what the outer slope blends back to.
    ground: u16,
    /// Turns done.
    turn: u32,
    /// Whether the turn-110 destruction has already happened.
    destroyed: bool,
    /// Fireballs spawned so far, towards `FIREBALLS`.
    fireballs: u32,
}

/// Cast by tribe `owner` at `at` (world units).
pub fn effect(owner: u8, at: (u16, u16), terrain: &Heightmap) -> Effect {
    let cell = (cell_of(at.0), cell_of(at.1));
    let ground = terrain.get(cell.0, cell.1);
    let volcano = Volcano { ground, turn: 0, destroyed: false, fireballs: 0 };
    Effect::new(owner, at, Countdown::new(Ticks::new(TOTAL_TURNS)), EffectKind::Volcano(volcano))
}

impl Volcano {
    /// One turn at `at`, cast by `owner`.
    pub fn turn(&mut self, at: (u16, u16), owner: u8, world: &GameMap, rng: &mut Lcg) -> Vec<WorldEvent> {
        let cell = (cell_of(at.0), cell_of(at.1));
        let size = world.terrain.size() as i32;
        let mut events = Vec::new();

        if (RUMBLE_TURNS..SHAPING_TURNS).contains(&self.turn) {
            events.push(WorldEvent::Terrain(TerrainChange::Volcano { at: cell, ground: self.ground, turn: self.turn - RUMBLE_TURNS }));
        }
        if self.turn == DESTROY_AT && !self.destroyed {
            self.destroyed = true;
            events.extend(destroy_and_ignite(cell, owner, world, size));
        }
        if self.turn == SHAPING_TURNS {
            events.push(WorldEvent::SpawnEffect(crate::lava::flowing(owner, at, size)));
        }
        if (FIREBALL_START..=FIREBALL_END).contains(&self.turn) {
            let ticks_left = (FIREBALL_END - self.turn + 1).max(1);
            let remaining = FIREBALLS - self.fireballs;
            let now = remaining.div_ceil(ticks_left).min(remaining);
            for _ in 0..now {
                let landing = random_point_in_area(cell, DESTROY_AREA_CELLS, size, rng);
                events.push(WorldEvent::SpawnEffect(crate::panicking_blast::effect(owner, landing)));
                self.fireballs += 1;
            }
        }
        self.turn += 1;
        events
    }

    /// The cone's height points (cells, unwrapped) not yet at their target on `terrain`, while it's still rising
    /// (`sparkles::SparkleView`, the terrain spells' twinkle, same as `LandBridge`/`Flatten`).
    pub fn reshaping(&self, at: (u16, u16), terrain: &Heightmap) -> Vec<Cell> {
        if self.turn >= SHAPING_TURNS {
            return Vec::new();
        }
        let cell = (cell_of(at.0), cell_of(at.1));
        let r = SLOPE_RADIUS_CELLS.ceil() as i32;
        (-r..=r)
            .flat_map(|dz| (-r..=r).map(move |dx| (dx, dz)))
            .map(|(dx, dz)| (((dx * dx + dz * dz) as f32).sqrt(), (cell.0 + dx, cell.1 + dz)))
            .filter(|&(d, _)| d <= SLOPE_RADIUS_CELLS)
            .filter(|&(d, (x, z))| terrain.get(x, z) != target_height(d, self.ground))
            .map(|(_, p)| p)
            .collect()
    }

    /// The pre-eruption rumble (`effect::GroundShake`), building towards the rise, while there's still ground to
    /// shake before it (a guess, untraced: the trace only says "rumbling").
    pub fn ground_shake(&self) -> Option<crate::effect::GroundShake> {
        (self.turn < RUMBLE_TURNS).then(|| {
            let p = self.turn as f32 / RUMBLE_TURNS as f32;
            crate::effect::GroundShake { radius_cells: SHAKE_RADIUS_CELLS, intensity: p.powf(1.6) }
        })
    }

    /// How strongly the `DESTROY_AREA_CELLS` square glows right now, 0 (nothing) to 1 (peak): ramping up over
    /// `GLOW_BEFORE` turns before `DESTROY_AT`, then fading out over `GLOW_AFTER` after it — a visual cue for the
    /// turn-110 destruction, otherwise invisible until the crater finishes rising and the real lava appears.
    pub fn pre_glow(&self) -> Option<f32> {
        let since = self.turn as i64 - DESTROY_AT as i64;
        if since < -(GLOW_BEFORE as i64) || since > GLOW_AFTER as i64 {
            return None;
        }
        let intensity = if since < 0 { 1.0 - (-since as f32 / GLOW_BEFORE as f32) } else { 1.0 - (since as f32 / GLOW_AFTER as f32) };
        Some(intensity.clamp(0.0, 1.0))
    }
}

/// Everyone in the `DESTROY_AREA_CELLS` square around `cell` (open or inside) catches fire, but the caster's own
/// shaman (`effect::hurts`, `DamageKind::Lava`'s own shaman-sparing rule, reused here too); every building there
/// is destroyed outright.
fn destroy_and_ignite(cell: Cell, owner: u8, world: &GameMap, size: i32) -> Vec<WorldEvent> {
    let area: Vec<Cell> = square(cell, DESTROY_AREA_CELLS, size).collect();
    let mut events = Vec::new();
    for u in world.units.iter().filter(|u| area.contains(&u.cell()) && crate::effect::hurts(u, DamageKind::Lava, Some(owner))) {
        events.push(WorldEvent::Ignite { unit: u.id });
    }
    for b in world.buildings.iter().filter(|b| area.iter().any(|&c| b.covers(cell_centre(c, size), 0))) {
        events.push(WorldEvent::Destroy { building: b.id });
    }
    events
}

/// A random world point in a random cell of the `radius_cells` square around `cell` (wrapped). Shared with
/// Firestorm (`crate::firestorm`), whose fireballs land the same way over its own target area.
pub(crate) fn random_point_in_area(cell: Cell, radius_cells: i32, size: i32, rng: &mut Lcg) -> (u16, u16) {
    let side = (2 * radius_cells) as u32;
    let (dx, dz) = ((rng.next_u32() % side) as i32 - radius_cells, (rng.next_u32() % side) as i32 - radius_cells);
    let (x, z) = ((cell.0 + dx).rem_euclid(size), (cell.1 + dz).rem_euclid(size));
    let in_cell = |c: i32, r: u32| (c as u32 * WORLD_UNITS_PER_CELL + r % WORLD_UNITS_PER_CELL) as u16;
    (in_cell(x, rng.next_u32()), in_cell(z, rng.next_u32()))
}

/// The `2 * radius` × `2 * radius` cells around `centre` (wrapped), `radius` either side. `pub`: also the
/// footprint `lava.rs` [client]'s pre-eruption glow (`pre_glow`, below) drapes over `DESTROY_AREA_CELLS` with.
pub fn square(centre: Cell, radius: i32, size: i32) -> impl Iterator<Item = Cell> {
    (-radius..radius).flat_map(move |dz| (-radius..radius).map(move |dx| ((centre.0 + dx).rem_euclid(size), (centre.1 + dz).rem_euclid(size))))
}

/// The world-unit centre of cell `(x, z)`.
fn cell_centre((x, z): Cell, size: i32) -> (u16, u16) {
    let at = |c: i32| (c.rem_euclid(size) as u32 * WORLD_UNITS_PER_CELL + WORLD_UNITS_PER_CELL / 2) as u16;
    (at(x), at(z))
}

/// The crater+rim's target height (absolute, not a delta) at distance `d` cells from centre, blending back to
/// `ground` (the terrain's own height at the centre when cast) by `SLOPE_RADIUS_CELLS`.
fn target_height(d: f32, ground: u16) -> u16 {
    if d <= RIM_RADIUS_CELLS {
        let t = d / RIM_RADIUS_CELLS;
        (CRATER_FLOOR as f32 + (MAX_HEIGHT - CRATER_FLOOR) as f32 * t) as u16
    } else {
        let t = ((d - RIM_RADIUS_CELLS) / (SLOPE_RADIUS_CELLS - RIM_RADIUS_CELLS)).min(1.0);
        (MAX_HEIGHT as f32 + (ground as f32 - MAX_HEIGHT as f32) * t) as u16
    }
}

/// A height `h` one turn closer to its target at distance `d` cells from the centre, `turn` turns into the rise
/// (the original's curve is not traced: a guess, like the other terrain effects', landing on the target the last
/// turn).
pub fn step(h: u16, ground: u16, d: f32, turn: u32) -> u16 {
    let target = target_height(d, ground) as i32;
    let left = RISE_TURNS.saturating_sub(turn).max(1) as i32;
    (h as i32 + (target - h as i32) / left).clamp(0, MAX_HEIGHT as i32) as u16
}

/// One rise turn of the cone at `at`, cast when the ground there was `ground`.
pub fn apply(terrain: &mut Heightmap, at: Cell, ground: u16, turn: u32) -> DirtyRect {
    let r = SLOPE_RADIUS_CELLS.ceil() as i32;
    for dz in -r..=r {
        for dx in -r..=r {
            let d = ((dx * dx + dz * dz) as f32).sqrt();
            if d > SLOPE_RADIUS_CELLS {
                continue;
            }
            let (x, z) = (at.0 + dx, at.1 + dz);
            let h = terrain.get(x, z);
            terrain.set(x, z, step(h, ground, d, turn));
        }
    }
    DirtyRect { min: (at.0 - r, at.1 - r), max: (at.0 + r, at.1 + r) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::building::{Building, BuildingKind};
    use crate::command::Command;
    use crate::spell::Spell;
    use crate::spell_book::{Availability, SpellKind};
    use crate::status::{Timed, SHIELD_STEPS};
    use crate::terrain::Heightmap;
    use crate::unit::UnitKind;

    #[test]
    fn target_height_peaks_at_max_height_on_the_rim_and_blends_to_ground_by_the_slope_radius() {
        assert_eq!(target_height(0.0, 500), CRATER_FLOOR);
        assert_eq!(target_height(RIM_RADIUS_CELLS, 500), MAX_HEIGHT);
        assert_eq!(target_height(SLOPE_RADIUS_CELLS, 500), 500);
        assert_eq!(target_height(SLOPE_RADIUS_CELLS + 1.0, 500), 500, "flat beyond the slope");
    }

    #[test]
    fn apply_eases_towards_the_target_and_lands_on_it_the_last_turn() {
        let mut t = Heightmap::new(32);
        for z in 0..32 {
            for x in 0..32 {
                t.set(x, z, 500);
            }
        }
        for turn in 0..RISE_TURNS {
            apply(&mut t, (16, 16), 500, turn);
        }
        assert_eq!(t.get(16, 16), CRATER_FLOOR, "the centre reaches the crater floor");
        // No grid cell sits exactly on the rim's own 2.5-cell radius; (2, 0) is the closest integer point still
        // inside it, so it should land exactly on what `target_height` predicts there (already close to the peak).
        assert_eq!(t.get(18, 16), target_height(2.0, 500), "rising towards the rim's own peak");
        assert_eq!(t.get(16 + 5, 16), 500, "back to the original ground by the slope radius");
    }

    fn bare() -> GameMap {
        let mut terrain = Heightmap::new(128);
        for z in 0..128 {
            for x in 0..128 {
                terrain.set(x, z, 500);
            }
        }
        GameMap::bare("volcano-test", terrain, Vec::new())
    }

    const AT: (u16, u16) = (64 * WORLD_UNITS_PER_CELL as u16 + 256, 64 * WORLD_UNITS_PER_CELL as u16 + 256);

    #[test]
    fn ground_shake_builds_during_the_rumble_then_stops_once_the_rise_starts() {
        let mut volcano = Volcano { ground: 500, turn: 0, destroyed: false, fireballs: 0 };
        let first = volcano.ground_shake().unwrap().intensity;
        volcano.turn = RUMBLE_TURNS - 1;
        let last = volcano.ground_shake().unwrap().intensity;
        assert!(last > first, "builds towards the rise");
        volcano.turn = RUMBLE_TURNS;
        assert!(volcano.ground_shake().is_none(), "no more rumble once it starts rising");
    }

    #[test]
    fn pre_glow_peaks_right_at_the_destruction_and_fades_out_either_side() {
        let mut volcano = Volcano { ground: 500, turn: DESTROY_AT, destroyed: false, fireballs: 0 };
        assert_eq!(volcano.pre_glow(), Some(1.0), "full strength the moment it destroys everything");
        volcano.turn = DESTROY_AT - GLOW_BEFORE;
        assert_eq!(volcano.pre_glow(), Some(0.0), "just starting to ramp up");
        volcano.turn = DESTROY_AT - GLOW_BEFORE - 1;
        assert_eq!(volcano.pre_glow(), None, "too early");
        volcano.turn = DESTROY_AT + GLOW_AFTER;
        assert_eq!(volcano.pre_glow(), Some(0.0), "just finished fading");
        volcano.turn = DESTROY_AT + GLOW_AFTER + 1;
        assert_eq!(volcano.pre_glow(), None, "too late");
    }

    #[test]
    fn rumbles_without_touching_the_terrain_then_rises_over_the_next_59_turns() {
        let mut map = bare();
        map.add_effect(effect(0, AT, &map.terrain.clone()));
        map.run(Ticks::new(RUMBLE_TURNS));
        assert_eq!(map.terrain.get(64, 64), 500, "no change yet: still rumbling");
        map.run(Ticks::new(RISE_TURNS));
        assert_eq!(map.terrain.get(64, 64), CRATER_FLOOR, "the cone has fully risen");
    }

    #[test]
    fn destroys_every_building_and_ignites_everyone_in_the_6x6_area_at_turn_110_but_spares_the_casters_shaman() {
        let mut map = bare();
        map.add_effect(effect(0, AT, &map.terrain.clone()));
        let hut = map.add_building(Building::new(BuildingKind::Hut { size: 1 }, 1, AT.0, AT.1, 0));
        let victim = map.spawn(1, UnitKind::Brave, AT).id;
        let shielded = map.spawn(1, UnitKind::Brave, AT).id;
        map.unit_mut(shielded).unwrap().statuses.shield = Timed::steps(SHIELD_STEPS);
        let shaman = map.spawn(0, UnitKind::Shaman, AT).id;
        let inside = map.spawn(1, UnitKind::Brave, AT).id;
        map.unit_mut(inside).unwrap().inside = Some(crate::unit::Inside { building: hut, door: AT });
        map.run(Ticks::new(DESTROY_AT + 1));
        assert!(map.building(hut).is_none(), "destroyed outright");
        for id in [victim, shielded, inside] {
            assert!(matches!(map.unit(id).unwrap().action, crate::unit::Action::OnFire { .. }), "caught fire, not killed outright: {id:?}");
        }
        assert!(!matches!(map.unit(shaman).unwrap().action, crate::unit::Action::OnFire { .. }), "the caster's own shaman is spared");
    }

    #[test]
    fn spawns_lava_over_the_crater_once_shaping_ends() {
        let mut map = bare();
        map.add_effect(effect(0, AT, &map.terrain.clone()));
        map.run(Ticks::new(SHAPING_TURNS + 1));
        assert!(map.effects.entries().any(|(_, e)| matches!(e.kind, EffectKind::Lava(_))), "lava spawned once shaping ends");
    }

    #[test]
    fn schedules_all_28_fireballs_over_the_115_150_window() {
        // Each fireball is its own short-lived `PanickingBlast` (3 turns), so counting what's still alive in
        // `map.effects` at one moment undercounts them; drive the controller directly and sum every spawn instead.
        let map = bare();
        let mut volcano = Volcano { ground: 500, turn: FIREBALL_START, destroyed: true, fireballs: 0 };
        let mut rng = map.rng;
        let mut spawned = 0;
        for _ in FIREBALL_START..=FIREBALL_END {
            let events = volcano.turn(AT, 0, &map, &mut rng);
            spawned += events.iter().filter(|e| matches!(e, WorldEvent::SpawnEffect(eff) if matches!(eff.kind, EffectKind::PanickingBlast(_)))).count();
        }
        assert_eq!(spawned as u32, FIREBALLS);
    }

    #[test]
    fn fireballs_start_before_shaping_ends_overlapping_but_not_matching_the_rocks_window() {
        let map = bare();
        let mut volcano = Volcano { ground: 500, turn: FIREBALL_START, destroyed: true, fireballs: 0 };
        let mut rng = map.rng;
        let events = volcano.turn(AT, 0, &map, &mut rng);
        assert!(events.iter().any(|e| matches!(e, WorldEvent::SpawnEffect(eff) if matches!(eff.kind, EffectKind::PanickingBlast(_)))), "the first fireball lands right at FIREBALL_START");
    }

    #[test]
    fn casting_it_through_the_spell_book_adds_the_controller_effect() {
        let mut map = GameMap::sandbox_walk();
        map.spell_books[0].set(SpellKind::Volcano, Availability::Unlimited);
        let at = (map.units[0].x, map.units[0].z);
        map.apply(&Command::Cast { player: 0, spell: Spell::Volcano { at } });
        map.run(crate::unit::CAST_RELEASE);
        assert!(map.effects.entries().any(|(_, e)| matches!(e.kind, EffectKind::Volcano(_))));
    }
}
