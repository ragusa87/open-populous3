//! Whirlwind (pop3-rev-analysis.md "Whirlwind mechanics"), an effect (`effect::EffectKind::Whirlwind`): an invisible
//! mover dragging 16 jittered funnel segments, active `ACTIVE` turns (homing on its target `WANDER` of them, then
//! drifting away from where the caster stood), then lifting off the ground for `DISSIPATE`. Every active turn it
//! acts on the cell under its bottom segment: people are lifted on a spiral (`Action::Whirled`, `GameMap::tend_whirled`)
//! then thrown, buildings shed wood, trees lose wood and a branch, wood pieces are carried (`Debris`), vehicles get a
//! simple blast. It drops a dust puff (`EffectKind::WhirlwindDust`) every turn.

use crate::angle::{self, Angle};
use crate::effect::{Effect, EffectKind};
use crate::event::WorldEvent;
use crate::id::Id;
use crate::map::{GameMap, Lcg};
use crate::motion::{Motion, Velocity};
use crate::time::{Countdown, Ticks};
use crate::tribes::Owner;
use crate::unit::{cell_of, torus_delta, Action, UnitKind};
use pop3_format::WORLD_UNITS_PER_CELL;

/// Turns it acts [exe].
pub const ACTIVE: u32 = 200;
/// Turns it lifts off after, acting on nothing [exe].
pub const DISSIPATE: u32 = 24;
/// The first turns, homing on its target; after, it drifts away from the caster [exe].
pub const WANDER: u32 = 100;
/// World units a turn, both phases, over land and sea [exe].
pub const SPEED: i32 = 120;
/// Funnel segments, top (0) to bottom [exe].
pub const SEGMENTS: usize = 16;
/// Height of the chain's start over the ground, each segment `SEGMENT_STEP` lower than the one above [exe].
pub const TOP: i32 = 1440;
pub const SEGMENT_STEP: i32 = 90;
/// Heading noise: `rand % NOISE - NOISE / 2` (2048ths), homing (±40°) and drifting (±75°) [exe].
const HOME_NOISE: u32 = 455;
const DRIFT_NOISE: u32 = 853;
/// Re-aim countdown masks: 1-7 turns homing, 1-31 drifting [exe].
const HOME_STEER: u32 = 7;
const DRIFT_STEER: u32 = 31;
/// A building in the cell loses one load of wood with this chance in 8 a turn [exe].
const BUILDING_CHANCE: u32 = 2;
/// A tree in the cell loses one size with this chance in 8 a turn [exe].
const TREE_CHANCE: u32 = 4;
/// A carried thing is thrown at this step of its spiral [exe].
pub const CARRY_STEPS: u8 = 19;
/// Spiral rise per step, in 256ths of a height unit (≈ 75.8) [exe].
const RISE_256: i32 = 19402;
/// Spiral radius: `RADIUS_256 × rise / TOP`, ≈ 0.18 of the rise [exe].
const RADIUS_256: i32 = 255;
/// Spiral turn per step, 2048ths (≈ 57°) [exe].
const SPIRAL_TURN: u32 = 323;
/// The throw out of the spiral: a push at a random heading and an upward speed [exe].
pub const THROW_PUSH: i32 = 192;
pub const THROW_LIFT: i16 = 230;
/// A dust puff's life [exe].
pub const DUST_TICKS: Ticks = Ticks::new(8);
/// Ground blend bytes tinting the funnel ("Colour, progressive") [exe].
const BLEND_RED: u8 = 0x7F;
const BLEND_GOLD_AGAIN: u8 = 0xC8;
const BLEND_ORANGE: u8 = 0xD4;

/// How a funnel segment or a dust puff is tinted over its gold (`data/alpha.dat`'s colour classes, Direct3D build).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tint {
    /// Classes 3, 4, 5: none (dark land, the sea).
    #[default]
    Gold,
    /// Class −2: ground blend 0x80-0xC8, × #FF2500.
    RedOrange,
    /// Class −1: ground blend above 0xD4, × #DB8707.
    Orange,
}

/// One funnel segment as placed this turn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Segment {
    pub x: u16,
    pub z: u16,
    /// Absolute height (terrain units) of its bottom centre.
    pub height: i32,
    /// Its fixed frame of `hfx0-0.dat` 1136-1151 (0..16).
    pub frame: u8,
    /// Its own index in the full column, 0 top: its width is `512 / (index + 1)` 256ths.
    pub index: u8,
    pub tint: Tint,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Whirlwind {
    x: u16,
    z: u16,
    /// Where the caster stood when it was cast: it drifts away from there.
    origin: (u16, u16),
    heading: Angle,
    steer: i32,
    turn: u32,
    /// Funnel segments still standing, top first.
    segments: Vec<Segment>,
    /// Where its bottom segment is this turn: the cell it acts on and drops dust on.
    bottom: (u16, u16),
}

/// One of −20..−13 or 12..19, never 0: a segment's step from the one above, per axis [exe].
fn jitter(rng: &mut Lcg) -> i32 {
    (rng.next_u32() & 0x27) as i32 - 20
}

/// Cast by tribe `owner` at `at` (world units), the caster standing at `origin`; `ground`: the height under `at`.
pub fn effect(owner: impl Into<Owner>, at: (u16, u16), origin: (u16, u16), ground: i32, rng: &mut Lcg) -> Effect {
    let steer = (rng.next_u32() & HOME_STEER) as i32;
    let heading = Angle::new(rng.next_u32() % angle::TURN);
    let phase = (rng.next_u32() & 15) as u8;
    let mut w = Whirlwind { x: at.0, z: at.1, origin, heading, steer, turn: 0, segments: Vec::with_capacity(SEGMENTS), bottom: at };
    w.segments = (0..SEGMENTS as u8).map(|i| Segment { x: at.0, z: at.1, height: 0, frame: phase.wrapping_sub(i) & 15, index: i, tint: Tint::Gold }).collect();
    w.place(ground, 0, rng);
    Effect::new(owner, at, Countdown::new(Ticks::new(ACTIVE + DISSIPATE)), EffectKind::Whirlwind(w))
}

/// A dust puff dropped at `at`, tinted `tint`.
pub fn dust(owner: Owner, at: (u16, u16), tint: Tint) -> Effect {
    Effect::new(owner, at, Countdown::new(DUST_TICKS), EffectKind::WhirlwindDust(tint))
}

/// The bottom segment's tint over ground of blend byte `blend` (sea: `None`).
pub fn funnel_tint(blend: Option<u8>) -> Tint {
    match blend {
        None => Tint::Gold,
        Some(b) if b > BLEND_ORANGE => Tint::Orange,
        Some(b) if b > BLEND_GOLD_AGAIN => Tint::Gold,
        Some(b) if b > BLEND_RED => Tint::RedOrange,
        Some(_) => Tint::Gold,
    }
}

/// A dust puff's tint: the 0xC8/0xD4 thresholds are overwritten in the original, so only red-orange or none [exe].
pub fn dust_tint(blend: Option<u8>) -> Tint {
    match blend {
        Some(b) if b > BLEND_RED => Tint::RedOrange,
        _ => Tint::Gold,
    }
}

/// Where a carried thing is on step `n` of its spiral around `centre` (ground `ground`), never lower than `was`.
pub fn spiral(centre: (u16, u16), ground: i32, n: u8, was: Option<i32>, rng: &mut Lcg) -> ((u16, u16), i32) {
    let rise = RISE_256 * n as i32 / 256;
    let noise = (rng.next_u32() % RISE_256 as u32) as i32 / 256 - 37;
    let height = (ground + rise + noise).max(was.unwrap_or(i32::MIN));
    let radius = RADIUS_256 * rise / TOP;
    let (dx, dz) = angle::rotate((radius, 0), (SPIRAL_TURN * n as u32) % angle::TURN);
    ((centre.0.wrapping_add(dx as u16), centre.1.wrapping_add(dz as u16)), height)
}

/// The throw out of the spiral: `THROW_PUSH` at a random heading, `THROW_LIFT` up.
pub fn throw(rng: &mut Lcg) -> Velocity {
    let (dx, dz) = angle::rotate((THROW_PUSH, 0), rng.next_u32() % angle::TURN);
    Velocity::new(dx as i16, THROW_LIFT, dz as i16)
}

impl Whirlwind {
    /// Its centre, world units.
    pub fn pos(&self) -> (u16, u16) {
        (self.x, self.z)
    }

    /// Its funnel segments still standing, top first.
    pub fn segments(&self) -> &[Segment] {
        &self.segments
    }

    /// Still catching (not lifting off yet).
    pub fn active(&self) -> bool {
        self.turn < ACTIVE
    }

    /// One turn, `id` its own effect id, `home` its target, cast by `owner`.
    pub fn turn(&mut self, id: Id<Effect>, home: (u16, u16), owner: Owner, world: &GameMap, rng: &mut Lcg) -> Vec<WorldEvent> {
        self.turn += 1;
        let active = self.turn <= ACTIVE;
        let (mx, mz) = angle::rotate((SPEED, 0), self.heading.steps());
        (self.x, self.z) = (self.x.wrapping_add(mx as u16), self.z.wrapping_add(mz as u16));
        let ground = world.terrain.height_at(self.x as u32, self.z as u32, WORLD_UNITS_PER_CELL);
        let lift = if active { 0 } else { (ACTIVE + DISSIPATE - self.turn) as i32 - 8 } * SEGMENT_STEP;
        self.place(ground, lift, rng);
        let blend = blend_under(world, self.bottom);
        for i in 0..self.segments.len() {
            self.segments[i].tint = self.segments.get(i + 1).map_or(funnel_tint(blend), |below| below.tint);
        }
        let mut events = if active { self.act(id, owner, world, rng) } else { Vec::new() };
        if !active && !self.segments.is_empty() {
            self.segments.remove(0);
        }
        events.push(WorldEvent::SpawnEffect(dust(owner, self.bottom, dust_tint(blend))));
        self.steer(home, rng);
        events
    }

    /// Re-places the segments from `TOP + lift` over `ground`, each one a random step from the one above.
    fn place(&mut self, ground: i32, lift: i32, rng: &mut Lcg) {
        let (mut x, mut z, mut h) = (self.x, self.z, ground + TOP + lift);
        for s in &mut self.segments {
            x = x.wrapping_add(jitter(rng) as u16);
            z = z.wrapping_add(jitter(rng) as u16);
            h -= SEGMENT_STEP;
            (s.x, s.z, s.height) = (x, z, h);
        }
        self.bottom = (x, z);
    }

    fn steer(&mut self, home: (u16, u16), rng: &mut Lcg) {
        self.steer -= 1;
        if self.steer > 0 {
            return;
        }
        let homing = self.turn < WANDER;
        let (mask, noise, (fx, fz), (tx, tz)) = if homing { (HOME_STEER, HOME_NOISE, (self.x, self.z), home) } else { (DRIFT_STEER, DRIFT_NOISE, self.origin, (self.x, self.z)) };
        self.steer = (rng.next_u32() & mask) as i32;
        let toward = angle::atan2(torus_delta(fz, tz), torus_delta(fx, tx));
        self.heading = toward.turned((rng.next_u32() % noise) as i32 - (noise / 2) as i32);
    }

    /// Everything in the cell under its bottom segment.
    fn act(&self, id: Id<Effect>, owner: Owner, world: &GameMap, rng: &mut Lcg) -> Vec<WorldEvent> {
        let cell = (cell_of(self.bottom.0), cell_of(self.bottom.1));
        let in_cell = |p: (u16, u16)| (cell_of(p.0), cell_of(p.1)) == cell;
        let mut events = Vec::new();
        for u in world.units.iter().filter(|u| in_cell((u.x, u.z)) && catchable(u)) {
            events.push(WorldEvent::Whirl { unit: u.id, by: id, step: (rng.next_u32() & 7) as u8 });
        }
        for b in world.buildings.iter().filter(|b| in_cell(b.centre()) && !b.kind.fireproof()) {
            if rng.next_u32() % 8 < BUILDING_CHANCE {
                events.push(WorldEvent::StripWood { building: b.id });
            }
        }
        if world.trees.iter().any(|t| t.is_visible() && t.cell() == cell) && rng.next_u32() % 8 < TREE_CHANCE {
            events.push(WorldEvent::TearTree { cell, by: id });
        }
        for w in world.wood.iter().filter(|w| in_cell((w.x, w.z))) {
            events.push(WorldEvent::LiftWood { at: (w.x, w.z), by: id });
        }
        for (_, v) in world.vehicles.iter().filter(|(_, v)| in_cell((v.x, v.z))) {
            events.push(WorldEvent::SpawnEffect(crate::blast::effect(owner, (v.x, v.z))));
        }
        events
    }
}

/// Whether a whirlwind lifts `u`: any tribe, not an angel of death, shielded, already lifted or held by an angel.
pub fn catchable(u: &crate::unit::Unit) -> bool {
    u.is_alive() && u.kind != UnitKind::AngelOfDeath && !u.statuses.shield.is_on() && !matches!(u.action, Action::Whirled { .. } | Action::Grabbed { .. })
}

/// The ground blend byte under `at`, None over the sea.
fn blend_under(world: &GameMap, at: (u16, u16)) -> Option<u8> {
    let cell = (cell_of(at.0), cell_of(at.1));
    (!crate::unit::is_sea(&world.terrain, cell)).then(|| world.shading.blend(cell.0, cell.1))
}

/// A branch torn off a tree or a wood piece, carried by a whirlwind then thrown.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Debris {
    pub kind: DebrisKind,
    pub x: u16,
    pub z: u16,
    pub motion: Motion,
    /// The whirlwind carrying it and its spiral step; None once thrown.
    pub held: Option<(Id<Effect>, u8)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DebrisKind {
    /// Gone on landing.
    Branch,
    /// A wood piece again where it lands, unless that is the sea (`[ours]`: not traced).
    Wood,
}

/// What became of a piece of debris this turn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Landed {
    Flying,
    /// On the ground at this point: gone, leaving a wood piece if `wood`.
    At { at: (u16, u16), wood: bool },
}

impl Debris {
    pub fn new(kind: DebrisKind, at: (u16, u16), height: i32, by: Id<Effect>) -> Self {
        Debris { kind, x: at.0, z: at.1, motion: Motion::flying(Velocity::ZERO, height), held: Some((by, 0)) }
    }

    /// One turn: on its holder's spiral (`holder`: its centre and ground, if it still exists), else thrown and flying.
    pub fn tick(&mut self, holder: Option<((u16, u16), i32)>, terrain: &crate::terrain::Heightmap, rng: &mut Lcg) -> Landed {
        if let Some((by, n)) = self.held {
            let n = n + 1;
            match holder {
                Some((centre, ground)) if n < CARRY_STEPS => {
                    let (at, height) = spiral(centre, ground, n, self.motion.height, rng);
                    (self.x, self.z) = at;
                    self.motion = Motion::flying(Velocity::ZERO, height);
                    self.held = Some((by, n));
                    return Landed::Flying;
                }
                _ => {
                    self.held = None;
                    self.motion = Motion::flying(throw(rng), self.motion.height.unwrap_or(0));
                }
            }
        }
        let below = |(x, z): (u16, u16)| terrain.height_at(x as u32, z as u32, WORLD_UNITS_PER_CELL);
        let ((x, z), motion, down) = crate::physics::step((self.x, self.z), self.motion, below);
        (self.x, self.z, self.motion) = (x, z, motion);
        match down {
            None => Landed::Flying,
            Some(_) => Landed::At { at: (x, z), wood: self.kind == DebrisKind::Wood && !crate::unit::is_sea(terrain, (cell_of(x), cell_of(z))) },
        }
    }
}

impl GameMap {
    /// The whirlwind effect `id`'s centre and the ground under it, if it still exists; `active`: only while it still
    /// catches.
    pub(crate) fn whirlwind_at(&self, id: Id<Effect>, active: bool) -> Option<((u16, u16), i32)> {
        let EffectKind::Whirlwind(w) = &self.effects.get(id)?.kind else { return None };
        if active && !w.active() {
            return None;
        }
        let at = w.pos();
        Some((at, self.terrain.height_at(at.0 as u32, at.1 as u32, WORLD_UNITS_PER_CELL)))
    }

    /// Right after the effects: everyone lifted takes its next spiral step, or is thrown once at `CARRY_STEPS` or as
    /// soon as its whirlwind stops catching; then the carried and thrown debris.
    pub(crate) fn tend_whirled(&mut self) {
        let mut rng = self.rng;
        for i in 0..self.units.len() {
            let Action::Whirled { by, step } = self.units[i].action else { continue };
            let n = step + 1;
            let holder = self.whirlwind_at(by, true);
            let owner = self.effects.get(by).map(|e| e.owner);
            let u = &mut self.units[i];
            match holder {
                Some((centre, ground)) if n < CARRY_STEPS => {
                    let ((x, z), height) = spiral(centre, ground, n, u.motion.height, &mut rng);
                    (u.x, u.z) = (x, z);
                    u.motion = Motion::flying(Velocity::ZERO, height);
                    u.action = Action::Whirled { by, step: n };
                }
                _ => u.thrown_by(throw(&mut rng), owner),
            }
        }
        let mut debris = std::mem::take(&mut self.debris);
        let mut landed = Vec::new();
        for (id, d) in debris.iter_mut() {
            let holder = d.held.and_then(|(by, _)| self.whirlwind_at(by, false));
            if let Landed::At { at, wood } = d.tick(holder, &self.terrain, &mut rng) {
                landed.push(id);
                if wood {
                    self.wood.push(crate::wood::WoodPiece::new(at.0, at.1));
                }
            }
        }
        debris.retain(|id, _| !landed.contains(&id));
        (self.debris, self.rng) = (debris, rng);
    }

    /// `unit` caught by the whirlwind `by`, its spiral starting at `step`: out of its building or vehicle, its plans
    /// dropped, held in the air where it is.
    pub(crate) fn whirl(&mut self, unit: crate::unit::UnitId, by: Id<Effect>, step: u8) {
        if !self.unit(unit).is_some_and(catchable) {
            return;
        }
        if let Some(vehicle) = self.unit(unit).and_then(|u| u.aboard) {
            self.evict(vehicle, &[unit]);
        }
        let Some(i) = self.unit_index(unit) else { return };
        let ground = self.terrain.height_at(self.units[i].x as u32, self.units[i].z as u32, WORLD_UNITS_PER_CELL);
        self.units[i].whirled(by, step, ground);
    }

    /// The building loses one load of wood, dropped as a wood piece at it; destroyed with none left.
    pub(crate) fn strip_wood(&mut self, building: crate::building::BuildingId) -> (Option<crate::terrain::DirtyRect>, Vec<WorldEvent>) {
        let Some(i) = self.building_index(building) else { return (None, Vec::new()) };
        let destroyed = self.buildings[i].lose_a_piece();
        let at = self.buildings[i].centre();
        self.wood.push(crate::wood::WoodPiece::new(at.0, at.1));
        if !destroyed {
            return (None, Vec::new());
        }
        let out = self.throw_out(building);
        (self.burn_down(building).pop(), out)
    }

    /// A tree of `cell` loses one size; a branch flies off it, carried by the whirlwind `by`.
    pub(crate) fn tear_tree(&mut self, cell: (i32, i32), by: Id<Effect>) {
        let Some(t) = self.trees.iter_mut().find(|t| t.is_visible() && t.cell() == cell) else { return };
        let at = (t.x, t.z);
        t.size -= 1;
        let ground = self.terrain.height_at(at.0 as u32, at.1 as u32, WORLD_UNITS_PER_CELL);
        let id = self.new_id_of();
        self.debris.insert(id, Debris::new(DebrisKind::Branch, at, ground, by));
    }

    /// The wood piece at `at` is carried off by the whirlwind `by`.
    pub(crate) fn lift_wood(&mut self, at: (u16, u16), by: Id<Effect>) {
        let Some(k) = self.wood.iter().position(|w| (w.x, w.z) == at) else { return };
        self.wood.remove(k);
        let ground = self.terrain.height_at(at.0 as u32, at.1 as u32, WORLD_UNITS_PER_CELL);
        let id = self.new_id_of();
        self.debris.insert(id, Debris::new(DebrisKind::Wood, at, ground, by));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::building::{Building, BuildingKind};
    use crate::command::Command;
    use crate::spell::Spell;
    use crate::terrain::Heightmap;
    use crate::tree::Tree;
    use crate::tribes::TribeId;

    const CELL: u16 = WORLD_UNITS_PER_CELL as u16;
    const AT: (u16, u16) = (64 * CELL + CELL / 2, 64 * CELL + CELL / 2);

    fn land() -> GameMap {
        let mut t = Heightmap::new(128);
        for z in 0..128 {
            for x in 0..128 {
                t.set(x, z, 300);
            }
        }
        GameMap::bare("whirlwind-test", t, Vec::new())
    }

    fn whirlwind_of(map: &GameMap) -> (Id<Effect>, &Whirlwind) {
        map.effects.iter().find_map(|(id, e)| if let EffectKind::Whirlwind(w) = &e.kind { Some((id, w)) } else { None }).expect("a whirlwind")
    }

    fn cast(map: &mut GameMap, origin: (u16, u16)) -> Id<Effect> {
        let mut rng = Lcg(5);
        map.add_effect(effect(TribeId::Blue, AT, origin, 300, &mut rng))
    }

    #[test]
    fn jitter_is_never_zero_and_stays_within_20() {
        let mut rng = Lcg(1);
        let steps: Vec<i32> = (0..1000).map(|_| jitter(&mut rng)).collect();
        assert!(steps.iter().all(|&j| (-20..=-13).contains(&j) || (12..=19).contains(&j)));
    }

    #[test]
    fn a_fresh_funnel_stands_16_segments_from_1350_down_to_the_ground() {
        let map = land();
        let mut rng = Lcg(3);
        let Effect { kind: EffectKind::Whirlwind(w), left, .. } = effect(TribeId::Blue, AT, AT, 300, &mut rng) else { unreachable!() };
        assert_eq!(left.left(), Ticks::new(224));
        let heights: Vec<i32> = w.segments().iter().map(|s| s.height - 300).collect();
        assert_eq!(heights, (0..16).map(|i| 1350 - 90 * i).collect::<Vec<_>>());
        assert!(w.segments().windows(2).all(|p| p[1].frame == p[0].frame.wrapping_sub(1) & 15), "staggered frames");
        let _ = map;
    }

    #[test]
    fn homes_near_its_target_then_drifts_away_from_the_caster() {
        let mut map = land();
        let origin = (AT.0.wrapping_sub(10 * CELL), AT.1);
        cast(&mut map, origin);
        let dist = |m: &GameMap, to: (u16, u16)| {
            let p = whirlwind_of(m).1.pos();
            crate::unit::isqrt(crate::map::torus_dist2(p, to) as u32) as i32
        };
        let mut far = 0;
        for _ in 0..WANDER {
            map.tick();
            far = far.max(dist(&map, AT));
        }
        assert!(far < 3 * CELL as i32, "circles around its target: {far}");
        let before = dist(&map, origin);
        map.run(Ticks::new(ACTIVE - WANDER));
        assert!(dist(&map, origin) > before + 4 * CELL as i32, "drifted away from the caster");
    }

    #[test]
    fn lifts_off_shortening_from_the_top_and_ends_after_224_turns() {
        let mut map = land();
        cast(&mut map, AT);
        map.run(Ticks::new(ACTIVE + 1));
        let w = whirlwind_of(&map).1;
        assert!(!w.active());
        assert_eq!(w.segments().len(), 15);
        let bottom = w.segments().last().unwrap();
        assert_eq!(bottom.index, 15);
        let ground = map.terrain.height_at(w.pos().0 as u32, w.pos().1 as u32, WORLD_UNITS_PER_CELL);
        assert_eq!(bottom.height - ground, 1350, "the bottom leaves the ground at once");
        map.run(Ticks::new(15));
        assert!(whirlwind_of(&map).1.segments().is_empty());
        map.run(Ticks::new(DISSIPATE - 16));
        assert!(!map.effects.iter().any(|(_, e)| matches!(e.kind, EffectKind::Whirlwind(_))), "gone");
    }

    #[test]
    fn drops_one_dust_puff_a_turn() {
        let mut map = land();
        cast(&mut map, AT);
        map.run(Ticks::new(20));
        let dust = map.effects.iter().filter(|(_, e)| matches!(e.kind, EffectKind::WhirlwindDust(_))).count();
        assert_eq!(dust as u32, DUST_TICKS.get());
    }

    #[test]
    fn the_ground_tint_climbs_one_segment_a_turn() {
        let mut rng = Lcg(1);
        let Effect { kind: EffectKind::Whirlwind(mut w), .. } = effect(TribeId::Blue, AT, AT, 300, &mut rng) else { unreachable!() };
        for turn in 1..=3 {
            let below: Vec<Tint> = w.segments.iter().skip(1).map(|s| s.tint).chain([Tint::RedOrange]).collect();
            w.segments.iter_mut().zip(below).for_each(|(s, t)| s.tint = t);
            let red = w.segments.iter().filter(|s| s.tint == Tint::RedOrange).count();
            assert_eq!(red, turn);
            assert_eq!(w.segments.last().unwrap().tint, Tint::RedOrange);
        }
        assert_eq!(funnel_tint(None), Tint::Gold);
        assert_eq!(funnel_tint(Some(0x90)), Tint::RedOrange);
        assert_eq!(funnel_tint(Some(0xD0)), Tint::Gold);
        assert_eq!(funnel_tint(Some(0xE0)), Tint::Orange);
        assert_eq!(dust_tint(Some(0xE0)), Tint::RedOrange, "dust: only red-orange or none");
    }

    #[test]
    fn the_spiral_rises_widens_and_never_goes_down() {
        let mut rng = Lcg(9);
        let (p1, h1) = spiral(AT, 300, 1, None, &mut rng);
        let (p18, h18) = spiral(AT, 300, 18, None, &mut rng);
        assert!((300 + 75 - 37..=300 + 75 + 38).contains(&h1));
        assert!((300 + 1364 - 37..=300 + 1364 + 38).contains(&h18));
        let r = |p: (u16, u16)| crate::unit::isqrt(crate::map::torus_dist2(p, AT) as u32);
        assert!(r(p1) < 20 && (235..=245).contains(&r(p18)), "{} {}", r(p1), r(p18));
        let (_, kept) = spiral(AT, 300, 1, Some(2000), &mut rng);
        assert_eq!(kept, 2000);
    }

    #[test]
    fn a_person_caught_circles_up_then_is_thrown_and_lands_hard() {
        let mut map = land();
        let brave = map.spawn(TribeId::Red, UnitKind::Brave, AT).id;
        let id = cast(&mut map, AT);
        map.whirl(brave, id, 0);
        assert!(matches!(map.unit(brave).unwrap().action, Action::Whirled { by, .. } if by == id), "caught");
        let max = map.unit(brave).unwrap().health.max();
        let (mut highest, mut lowest_life, mut thrown) = (0, max, false);
        for _ in 0..ACTIVE + DISSIPATE + 40 {
            map.tick();
            let u = map.unit(brave).unwrap();
            highest = highest.max(u.motion.height.unwrap_or(0));
            lowest_life = lowest_life.min(u.health.current());
            thrown |= u.action == Action::Tumbling;
        }
        assert!(thrown && highest > 300 + 1000, "carried high then thrown: {highest}");
        assert_eq!(lowest_life, max - crate::unit::FALL_DAMAGE, "a hard landing");
        assert_eq!(map.unit(brave).unwrap().last_attacker(), Some(Owner::Tribe(TribeId::Blue)));
    }

    #[test]
    fn whoever_stands_under_its_bottom_segment_is_caught() {
        let mut map = land();
        let mut rng = Lcg(5);
        let mut e = effect(TribeId::Blue, AT, AT, 300, &mut rng);
        let EffectKind::Whirlwind(ref w) = e.kind else { unreachable!() };
        let (mx, mz) = angle::rotate((SPEED, 0), w.heading.steps());
        let bottom_next = w.segments.iter().fold((w.x.wrapping_add(mx as u16), w.z.wrapping_add(mz as u16)), |p, _| p);
        let brave = map.spawn(TribeId::Blue, UnitKind::Brave, bottom_next).id;
        let events = e.tick(Id::from_raw(7), map.now, &map, &mut rng);
        let EffectKind::Whirlwind(ref w) = e.kind else { unreachable!() };
        let caught = events.iter().any(|ev| matches!(ev, WorldEvent::Whirl { unit, .. } if *unit == brave));
        let under = (cell_of(w.bottom.0), cell_of(w.bottom.1)) == map.unit(brave).unwrap().cell();
        assert_eq!(caught, under, "its own tribe too, when under the bottom segment");
    }

    #[test]
    fn spares_angels_the_shielded_and_whoever_it_already_holds() {
        let mut map = land();
        let angel = map.spawn(TribeId::Red, UnitKind::AngelOfDeath, AT).id;
        let shielded = map.spawn(TribeId::Red, UnitKind::Brave, AT).id;
        map.unit_mut(shielded).unwrap().statuses.shield = crate::status::Timed::steps(crate::status::SHIELD_STEPS);
        let id = cast(&mut map, AT);
        map.whirl(angel, id, 0);
        map.whirl(shielded, id, 0);
        assert!(!matches!(map.unit(angel).unwrap().action, Action::Whirled { .. }));
        assert!(!matches!(map.unit(shielded).unwrap().action, Action::Whirled { .. }));
    }

    #[test]
    fn a_building_sheds_wood_pieces_until_it_is_destroyed() {
        let mut map = land();
        let b = map.add_building(Building::new(BuildingKind::Hut { size: 1 }, TribeId::Red, 60 * CELL, 60 * CELL, 0));
        let pieces = map.building(b).unwrap().used;
        for k in 1..=pieces {
            map.strip_wood(b);
            assert_eq!(map.wood.len(), k as usize);
        }
        assert!(map.building(b).is_none(), "no wood left: destroyed");
    }

    #[test]
    fn a_torn_tree_shrinks_and_its_branch_flies_off_then_vanishes() {
        let mut map = land();
        map.trees.push(Tree::new((64, 64), 0, 3));
        let id = cast(&mut map, AT);
        map.tear_tree((64, 64), id);
        assert_eq!(map.trees[0].size, 2);
        assert_eq!(map.debris.len(), 1);
        map.run(Ticks::new(80));
        assert!(map.debris.is_empty(), "landed and gone");
        assert!(map.wood.is_empty(), "a branch leaves nothing");
    }

    #[test]
    fn a_lifted_wood_piece_lands_as_a_wood_piece_elsewhere() {
        let mut map = land();
        map.wood.push(crate::wood::WoodPiece::new(AT.0, AT.1));
        let id = cast(&mut map, AT);
        map.lift_wood(AT, id);
        assert!(map.wood.is_empty() && map.debris.len() == 1);
        map.run(Ticks::new(80));
        assert!(map.debris.is_empty());
        assert_eq!(map.wood.len(), 1);
        assert_ne!((map.wood[0].x, map.wood[0].z), AT, "thrown away");
    }

    #[test]
    fn cast_from_the_book_it_starts_at_the_target() {
        let mut map = GameMap::sandbox_spells();
        let s = map.shaman_of(TribeId::Blue).unwrap();
        let at = (s.x.wrapping_add(3 * CELL), s.z);
        map.spell_books[0].set(crate::spell_book::SpellKind::Whirlwind, crate::spell_book::Availability::Unlimited);
        map.apply(&Command::Cast { player: TribeId::Blue, spell: Spell::Whirlwind { at } });
        map.run(Ticks::new(30));
        assert!(map.effects.iter().any(|(_, e)| matches!(e.kind, EffectKind::Whirlwind(_)) && (e.x, e.z) == at));
    }

    #[test]
    fn the_same_seed_gives_the_same_storm() {
        let run = || {
            let mut map = land();
            for k in 0..6 {
                map.spawn(TribeId::Red, UnitKind::Brave, (AT.0.wrapping_add(k * 40), AT.1));
            }
            cast(&mut map, AT);
            map.run(Ticks::new(240));
            (map.units, map.wood, map.rng)
        };
        assert_eq!(run(), run());
    }
}
