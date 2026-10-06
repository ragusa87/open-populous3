//! Units (shaman, later braves, warriors...). Integer state, advanced one tick at a time.
//! Positions are fixed-point (1 cell = 512 units, like the original), wrapping at 65536: plain
//! `u16` wrapping arithmetic walks around the torus.

use crate::site::ReincarnationSite;
use crate::terrain::Heightmap;
use pop3_format::WORLD_UNITS_PER_CELL;

/// Simulation ticks per second (the client runs them at a fixed rate).
pub const TICKS_PER_SECOND: u32 = 10;
pub const SHAMAN_MAX_HEALTH: u16 = 100;
/// World units per tick on flat ground (1.25 cells per second).
pub const SHAMAN_SPEED: i32 = 64;
/// Walking speed factor change in 1/256 per 100 of slope (height per cell). Uphill: the sandbox
/// ramp (30 per cell) is ~22% slower, the steep hill (100 per cell) quarter speed. Downhill: the
/// steep hill is 1.5x faster.
pub const SLOPE_SLOWDOWN_UP: i32 = 192;
pub const SLOPE_SPEEDUP_DOWN: i32 = 128;
/// Walking speed factor bounds in 1/256 (steep climbs crawl, steep descents are capped).
pub const SLOPE_FACTOR_RANGE: (i32, i32) = (32, 384);
/// Health lost per tick while in the sea.
pub const DROWN_DAMAGE: u16 = 4;
/// Ticks between two health points regained on land.
pub const REGEN_EVERY: u8 = 5;
/// Length of the cast jump, the fall when dying, and the wait before reincarnation.
pub const CAST_TICKS: u16 = 12;
pub const DYING_TICKS: u16 = 8;
pub const RESPAWN_TICKS: u16 = 30;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnitKind {
    Shaman,
    Brave,
    Warrior,
    Preacher,
    Spy,
    Firewarrior,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Idle,
    Walking { to: (u16, u16) },
    Praying,
    /// Jumping with the spell in her hands, `left` ticks to go.
    Casting { left: u16 },
    /// The ground under her is sea: loses health until dead or the land comes back.
    Drowning,
    Dying { left: u16 },
    /// Waiting to reincarnate at her site.
    Dead { left: u16 },
}

impl Action {
    pub fn name(&self) -> &'static str {
        match self {
            Action::Idle => "Idle",
            Action::Walking { .. } => "Walking",
            Action::Praying => "Praying",
            Action::Casting { .. } => "Casting",
            Action::Drowning => "Drowning",
            Action::Dying { .. } => "Dying",
            Action::Dead { .. } => "Dead",
        }
    }

    /// Ticks since a timed action started (for one-shot animations), None for open-ended ones.
    pub fn elapsed(&self) -> Option<u16> {
        match *self {
            Action::Casting { left } => Some(CAST_TICKS - left.min(CAST_TICKS)),
            Action::Dying { left } => Some(DYING_TICKS - left.min(DYING_TICKS)),
            Action::Dead { left } => Some(RESPAWN_TICKS - left.min(RESPAWN_TICKS)),
            _ => None,
        }
    }

    fn can_take_orders(&self) -> bool {
        !matches!(self, Action::Drowning | Action::Dying { .. } | Action::Dead { .. })
    }
}

/// What a player tells their shaman to do (carried by `Command::Order`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Order {
    MoveTo { x: u16, z: u16 },
    Pray,
    Cast,
    Stop,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnitEvent {
    Died,
    /// Back at her site: the caller levels its ground.
    Reincarnated,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unit {
    pub id: u32,
    pub owner: u8,
    pub kind: UnitKind,
    pub x: u16,
    pub z: u16,
    /// Heading in eighths of a turn: 0 = +z, 2 = +x, 4 = -z, 6 = -x.
    pub facing: u8,
    pub health: u16,
    pub action: Action,
    regen: u8,
}

impl Unit {
    pub fn shaman(id: u32, site: &ReincarnationSite) -> Self {
        let (x, z) = site.spawn_point();
        Unit {
            id,
            owner: site.owner,
            kind: UnitKind::Shaman,
            x,
            z,
            facing: 0,
            health: SHAMAN_MAX_HEALTH,
            action: Action::Idle,
            regen: 0,
        }
    }

    pub fn max_health(&self) -> u16 {
        SHAMAN_MAX_HEALTH
    }

    pub fn is_alive(&self) -> bool {
        !matches!(self.action, Action::Dying { .. } | Action::Dead { .. })
    }

    /// Cell holding the unit.
    pub fn cell(&self) -> (i32, i32) {
        (cell_of(self.x), cell_of(self.z))
    }

    /// Ignored while drowning, dying or dead.
    pub fn order(&mut self, order: Order) {
        if !self.action.can_take_orders() {
            return;
        }
        self.action = match order {
            Order::MoveTo { x, z } => Action::Walking { to: (x, z) },
            Order::Pray => Action::Praying,
            Order::Cast => Action::Casting { left: CAST_TICKS },
            Order::Stop => Action::Idle,
        };
    }

    /// One simulation step. `site` is where the unit reincarnates.
    pub fn tick(&mut self, terrain: &Heightmap, site: Option<&ReincarnationSite>) -> Option<UnitEvent> {
        if self.is_alive() && is_sea(terrain, self.cell()) {
            self.action = Action::Drowning;
        }
        match self.action {
            Action::Idle | Action::Praying => self.heal(),
            Action::Walking { to } => {
                self.heal();
                self.step_towards(to, terrain);
            }
            Action::Casting { left } => {
                self.action = if left > 1 { Action::Casting { left: left - 1 } } else { Action::Idle };
            }
            Action::Drowning => {
                self.health = self.health.saturating_sub(DROWN_DAMAGE);
                if self.health == 0 {
                    self.action = Action::Dying { left: DYING_TICKS };
                    return Some(UnitEvent::Died);
                }
                if !is_sea(terrain, self.cell()) {
                    self.action = Action::Idle;
                }
            }
            Action::Dying { left } => {
                self.action = if left > 1 { Action::Dying { left: left - 1 } } else { Action::Dead { left: RESPAWN_TICKS } };
            }
            Action::Dead { left } if left > 1 => self.action = Action::Dead { left: left - 1 },
            Action::Dead { .. } => {
                let site = site?;
                *self = Unit { facing: self.facing, ..Unit::shaman(self.id, site) };
                return Some(UnitEvent::Reincarnated);
            }
        }
        None
    }

    fn heal(&mut self) {
        if self.health >= self.max_health() {
            self.regen = 0;
            return;
        }
        self.regen += 1;
        if self.regen >= REGEN_EVERY {
            self.regen = 0;
            self.health += 1;
        }
    }

    /// Straight line, shortest way around the torus; stops at the shore or on arrival.
    fn step_towards(&mut self, to: (u16, u16), terrain: &Heightmap) {
        let (dx, dz) = (torus_delta(self.x, to.0), torus_delta(self.z, to.1));
        if dx == 0 && dz == 0 {
            self.action = Action::Idle;
            return;
        }
        self.facing = octant(dx, dz);
        let dist = isqrt((dx * dx + dz * dz) as u32) as i32;
        let speed = slope_speed(SHAMAN_SPEED, self.grade_ahead((dx, dz), dist, terrain));
        let (sx, sz) = if dist <= speed { (dx, dz) } else { (dx * speed / dist, dz * speed / dist) };
        let (nx, nz) = (self.x.wrapping_add(sx as u16), self.z.wrapping_add(sz as u16));
        if is_sea(terrain, (cell_of(nx), cell_of(nz))) {
            self.action = Action::Idle;
            return;
        }
        (self.x, self.z) = (nx, nz);
        if (nx, nz) == to {
            self.action = Action::Idle;
        }
    }

    /// Slope (height per cell, positive uphill) over one flat-ground step towards `(dx, dz)`.
    fn grade_ahead(&self, (dx, dz): (i32, i32), dist: i32, terrain: &Heightmap) -> i32 {
        let (px, pz) = (self.x.wrapping_add((dx * SHAMAN_SPEED / dist) as u16), self.z.wrapping_add((dz * SHAMAN_SPEED / dist) as u16));
        let h = |x: u16, z: u16| terrain.height_at(x as u32, z as u32, WORLD_UNITS_PER_CELL);
        (h(px, pz) - h(self.x, self.z)) * WORLD_UNITS_PER_CELL as i32 / SHAMAN_SPEED
    }
}

/// Walking speed on a slope (height per cell, positive uphill): slower up, faster down, clamped.
pub fn slope_speed(flat: i32, grade: i32) -> i32 {
    let per_100 = if grade > 0 { SLOPE_SLOWDOWN_UP } else { SLOPE_SPEEDUP_DOWN };
    let factor = (256 - grade * per_100 / 100).clamp(SLOPE_FACTOR_RANGE.0, SLOPE_FACTOR_RANGE.1);
    (flat * factor / 256).max(1)
}

fn cell_of(v: u16) -> i32 {
    (v as u32 / WORLD_UNITS_PER_CELL) as i32
}

/// Open sea: all four corners of the cell are at sea level (shore cells are still walkable).
pub fn is_sea(terrain: &Heightmap, (x, z): (i32, i32)) -> bool {
    [(0, 0), (1, 0), (0, 1), (1, 1)].iter().all(|&(dx, dz)| terrain.is_water(x + dx, z + dz))
}

/// Signed shortest offset from `a` to `b` on the 65536-wide torus.
pub fn torus_delta(a: u16, b: u16) -> i32 {
    b.wrapping_sub(a) as i16 as i32
}

/// Nearest of 8 headings for a direction (0 = +z, 2 = +x, 4 = -z, 6 = -x), integers only.
pub fn octant(dx: i32, dz: i32) -> u8 {
    // tan(22.5 deg) ~ 29/70.
    let (ax, az) = (dx.abs() as i64, dz.abs() as i64);
    if ax * 70 < az * 29 {
        if dz >= 0 { 0 } else { 4 }
    } else if az * 70 < ax * 29 {
        if dx > 0 { 2 } else { 6 }
    } else {
        match (dx > 0, dz > 0) {
            (true, true) => 1,
            (true, false) => 3,
            (false, false) => 5,
            (false, true) => 7,
        }
    }
}

pub fn isqrt(n: u32) -> u32 {
    if n < 2 {
        return n;
    }
    let mut x = n;
    let mut y = x.div_ceil(2);
    while y < x {
        x = y;
        y = (x + n / x) / 2;
    }
    x
}

#[cfg(test)]
mod tests {
    use super::*;

    fn land() -> Heightmap {
        let mut t = Heightmap::new(128);
        for z in 0..128 {
            for x in 0..128 {
                t.set(x, z, 100);
            }
        }
        t
    }

    fn shaman_at(cell: (i32, i32)) -> (Unit, ReincarnationSite) {
        let site = ReincarnationSite::at_cell(0, cell);
        (Unit::shaman(1, &site), site)
    }

    fn run(u: &mut Unit, t: &Heightmap, site: &ReincarnationSite, ticks: usize) -> Vec<UnitEvent> {
        (0..ticks).filter_map(|_| u.tick(t, Some(site))).collect()
    }

    #[test]
    fn walks_to_the_target_then_idles() {
        let t = land();
        let (mut u, site) = shaman_at((10, 10));
        let to = (u.x + 3 * 512, u.z);
        u.order(Order::MoveTo { x: to.0, z: to.1 });
        run(&mut u, &t, &site, 23);
        assert_eq!(u.action, Action::Walking { to });
        assert_eq!(u.facing, 2, "heading +x");
        run(&mut u, &t, &site, 2);
        assert_eq!((u.x, u.z, u.action), (to.0, to.1, Action::Idle));
    }

    #[test]
    fn walks_the_short_way_across_the_map_edge() {
        let t = land();
        let (mut u, site) = shaman_at((127, 0));
        u.order(Order::MoveTo { x: 256, z: u.z });
        u.tick(&t, Some(&site));
        assert_eq!(u.x, 127 * 512 + 256 + SHAMAN_SPEED as u16);
        assert_eq!(u.facing, 2);
    }

    #[test]
    fn slower_uphill_faster_downhill() {
        assert_eq!(slope_speed(SHAMAN_SPEED, 0), SHAMAN_SPEED);
        assert_eq!(slope_speed(SHAMAN_SPEED, 100), SHAMAN_SPEED / 4, "steep hill: quarter speed");
        assert_eq!(slope_speed(SHAMAN_SPEED, 30), 49, "gentle ramp: ~22% slower");
        assert_eq!(slope_speed(SHAMAN_SPEED, -100), SHAMAN_SPEED * 3 / 2, "steep descent: 1.5x");
        assert!(slope_speed(SHAMAN_SPEED, 30) < SHAMAN_SPEED && slope_speed(SHAMAN_SPEED, -30) > SHAMAN_SPEED, "gentle ramp");
        assert_eq!(slope_speed(SHAMAN_SPEED, 1000), SHAMAN_SPEED / 8, "cliffs: clamped, never stuck");
        assert_eq!(slope_speed(SHAMAN_SPEED, -1000), SHAMAN_SPEED * 3 / 2);
    }

    #[test]
    fn climbing_a_ramp_takes_longer_than_coming_down() {
        let mut t = land();
        for z in 0..128 {
            for x in 10..=20 {
                t.set(x, z, 100 + (x as u16 - 10) * 50);
            }
            for x in 21..30 {
                t.set(x, z, 600);
            }
        }
        let ticks_to = |from: (i32, i32), to_cell: i32| {
            let (mut u, site) = shaman_at(from);
            let to = (to_cell as u16 * 512 + 256, u.z);
            u.order(Order::MoveTo { x: to.0, z: to.1 });
            (1..500).find(|_| {
                u.tick(&t, Some(&site));
                u.action == Action::Idle
            })
        };
        let (up, down, flat) = (ticks_to((10, 5), 20).unwrap(), ticks_to((20, 5), 10).unwrap(), ticks_to((40, 5), 50).unwrap());
        assert!(down < flat && flat < up, "down {down} < flat {flat} < up {up}");
    }

    #[test]
    fn stops_at_the_shore() {
        let mut t = land();
        for z in 0..128 {
            for x in 13..20 {
                t.set(x, z, 0);
            }
        }
        let (mut u, site) = shaman_at((10, 10));
        u.order(Order::MoveTo { x: 16 * 512, z: u.z });
        run(&mut u, &t, &site, 100);
        assert_eq!(u.action, Action::Idle);
        assert_eq!(u.cell().0, 12, "last land cell");
    }

    #[test]
    fn cast_jump_ends_back_idle() {
        let t = land();
        let (mut u, site) = shaman_at((10, 10));
        u.order(Order::Cast);
        assert_eq!(u.action.elapsed(), Some(0));
        run(&mut u, &t, &site, CAST_TICKS as usize - 1);
        assert_eq!(u.action, Action::Casting { left: 1 });
        run(&mut u, &t, &site, 1);
        assert_eq!(u.action, Action::Idle);
    }

    #[test]
    fn drowns_dies_and_reincarnates_at_the_site() {
        let mut t = land();
        let (mut u, site) = shaman_at((10, 10));
        u.order(Order::Pray);
        u.x += 20 * 512;
        let (cx, cz) = u.cell();
        for (dx, dz) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
            t.set(cx + dx, cz + dz, 0);
        }
        u.tick(&t, Some(&site));
        assert_eq!((u.action, u.health), (Action::Drowning, SHAMAN_MAX_HEALTH - DROWN_DAMAGE));
        u.order(Order::MoveTo { x: 0, z: 0 });
        assert_eq!(u.action, Action::Drowning, "no orders while drowning");
        let events = run(&mut u, &t, &site, 24);
        assert_eq!((events, u.health, u.is_alive()), (vec![UnitEvent::Died], 0, false));
        let events = run(&mut u, &t, &site, (DYING_TICKS + RESPAWN_TICKS) as usize);
        assert_eq!(events, vec![UnitEvent::Reincarnated]);
        assert_eq!((u.x, u.z, u.health, u.action), (site.x, site.z, SHAMAN_MAX_HEALTH, Action::Idle));
    }

    #[test]
    fn heals_slowly_on_land() {
        let t = land();
        let (mut u, site) = shaman_at((10, 10));
        u.health = 50;
        run(&mut u, &t, &site, 5 * REGEN_EVERY as usize);
        assert_eq!(u.health, 55);
    }

    #[test]
    fn octants_and_sqrt() {
        assert_eq!([octant(0, 5), octant(5, 5), octant(5, 0), octant(5, -5)], [0, 1, 2, 3]);
        assert_eq!([octant(0, -5), octant(-5, -5), octant(-5, 0), octant(-5, 5)], [4, 5, 6, 7]);
        assert_eq!(octant(10, 3), 2);
        assert_eq!((isqrt(0), isqrt(15), isqrt(16), isqrt(u32::MAX)), (0, 3, 4, 65535));
        assert_eq!(torus_delta(65000, 100), 636);
    }
}
