//! Reincarnation sites: where each tribe's shaman spawns and respawns.
//! Position is fixed for the whole game: no command moves one. Whether it's currently active can be
//! toggled at runtime (`GameMap::remove_reincarnation`/`restore_reincarnation`, `DO SET_REINCARNATION
//! ON/OFF`, ai-scripts.md) - a disabled site is simply skipped by `GameMap::site_of` and everything
//! built on it (no respawn, not drawn), without losing its place in `GameMap::sites`.

use crate::event::WorldEvent;
use crate::terrain::{DirtyRect, Heightmap};
use crate::time::{Countdown, Tick, Ticks};
use crate::tribes::TribeId;
use crate::unit::{isqrt, torus_delta};
use pop3_format::{Level, WORLD_UNITS_PER_CELL};

/// Cells (around the site's own, either axis) whose wildmen join the tribe when its shaman appears.
pub const WELCOME_RANGE: i32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReincarnationSite {
    pub owner: TribeId,
    /// Centre in world units (512 per cell), wraps at 65536.
    pub x: u16,
    pub z: u16,
    /// The ground height the site is kept levelled to, every time its shaman spawns or respawns
    /// (pop3-rev-analysis.md "Reincarnation sites": `tribe+0x915`, measured 127-128 around a site at 128) - a
    /// fixed value captured once (the terrain's own height at the site's cell, before any flattening), not
    /// recomputed from whatever the surrounding ground happens to be at each respawn. `flatten_for_spawn` floors
    /// it at `MIN_SPAWN_HEIGHT` only as a last-resort safety net (a site placed on a sea cell), not as the
    /// everyday target - that was the bug (TODO.md "Terrain", 2026-10-21): flooring a *live average* at a low 32
    /// let an otherwise-dry site read as water once `row_jitter` became a genuine row-scale mover.
    /// `GameMap::restore_reincarnation` is the one exception: re-enabling a disabled site re-reads this
    /// from the live terrain then (unless it's underwater), since the ground may have changed while it
    /// was off.
    pub height: u16,
    /// Whether this tribe's shaman can currently reincarnate here (`GameMap::site_of` skips a disabled
    /// one). Always `true` until `DO SET_REINCARNATION OFF`/`GameMap::remove_reincarnation` disables it.
    pub enabled: bool,
    /// Whether (re)spawning here runs the shaman's build/rebuild ritual (`SiteBuildState`, "Build/rebuild
    /// ritual" below) instead of placing her and the 8 pillars instantly: `true` for a real level
    /// (`sites_from_level`), `false` for every sandbox/generated/dev site (`at_cell`) - so none of the dev
    /// sandboxes (`just shot SHAMAN=...` and the many tests built on them) spend their first ~40 ticks
    /// locked out of a shaman they need usable immediately.
    pub animated: bool,
    /// Tick pillar slot `n` (`SLOTS`) was last (re)built by the ritual, `None` until its shot lands and
    /// settles (`GameMap::fire_site_pillar`, `tend_site_shots`). Meaningless (ignored, `pillar_built`)
    /// when `!animated`: such a site's 8 pillars simply always stand.
    pub pillars: [Option<Tick>; 8],
}

/// The 8 pillar slots, cells from the site's cell, in slot order (pop3-rev-analysis.md "Reincarnation
/// site"): an octagon, 3 cells out on the axes, (2, 2) on the diagonals. The build/rebuild ritual fires
/// its shots in reverse of this order (slot 7 first, slot 0 last).
pub const SLOTS: [(i32, i32); 8] = [(0, 3), (2, 2), (3, 0), (2, -2), (0, -3), (-2, -2), (-3, 0), (-2, 2)];

impl ReincarnationSite {
    pub fn at_cell(owner: TribeId, (x, z): (i32, i32), height: u16) -> Self {
        let unit = |c: i32| (c.rem_euclid(128) as u32 * WORLD_UNITS_PER_CELL + WORLD_UNITS_PER_CELL / 2) as u16;
        ReincarnationSite { owner, x: unit(x), z: unit(z), height, enabled: true, animated: false, pillars: [None; 8] }
    }

    /// World-unit centre of pillar slot `slot` (`SLOTS`), the shaman's own standard shots aim at during the
    /// build/rebuild ritual.
    pub fn slot_target(&self, slot: u8) -> (u16, u16) {
        let (cx, cz) = self.cell();
        let (dx, dz) = SLOTS[slot as usize];
        let unit = |c: i32| (c.rem_euclid(128) as u32 * WORLD_UNITS_PER_CELL + WORLD_UNITS_PER_CELL / 2) as u16;
        (unit(cx + dx), unit(cz + dz))
    }

    /// Tick pillar `slot` last stood up, for the client's rise animation: always `Tick::ZERO` (already
    /// standing, no animation) for a non-animated site - only an `animated` one tracks it live.
    pub fn pillar_built(&self, slot: u8) -> Option<Tick> {
        if !self.animated {
            return Some(Tick::ZERO);
        }
        self.pillars[slot as usize]
    }

    /// Cell containing the site centre.
    /// Whether a wildman standing on `cell` joins this site's tribe when its shaman appears: the
    /// levels place a tribe's first followers as wildmen in the ring of cells around its site
    /// (level 2: 8 around blue's), converted for free, no spell needed.
    pub fn welcomes(&self, cell: (i32, i32)) -> bool {
        let (sx, sz) = self.cell();
        let torus = |a: i32, b: i32| {
            let d = (a - b).rem_euclid(128);
            d.min(128 - d)
        };
        torus(sx, cell.0).max(torus(sz, cell.1)) <= WELCOME_RANGE
    }

    pub fn cell(&self) -> (i32, i32) {
        ((self.x as u32 / WORLD_UNITS_PER_CELL) as i32, (self.z as u32 / WORLD_UNITS_PER_CELL) as i32)
    }

    /// Where the shaman appears, at game start and after each death.
    pub fn spawn_point(&self) -> (u16, u16) {
        (self.x, self.z)
    }
}

/// Ground made flat around a site when its shaman spawns. The stones stand 1.3 cells out and are
/// ~0.5 wide; the triangles under them have corners up to ~2.9 cells from the centre.
///
/// Tried widening this to 5 (pop3-rev-analysis.md "Reincarnation sites": "the ground levelled to the site
/// height over a 5-cell disc"), 2026-10-21 - **reverted**: on `levl2010.dat`, the player's own site is exactly
/// 5 cells from an unrelated boat hut's shore, *only* via the map's own torus wrap (123 cells apart the other
/// way) - widening hard-levelled solid dry land over the open sea that boat hut's dock depends on
/// (`placement::shore_ok`), breaking a previously-passing, real-level-validated test. Reproducing the real
/// radius faithfully needs either confirming the exe's own effect never crosses the wrap wall this way (not
/// checked - clean room, a spec-session question) or some other difference (ours may touch cells the original's
/// own disc-growth animation never actually reaches, since ours is applied in one shot, not grown over 21
/// turns - TODO.md "Terrain" #302 tracks the full animated version this keeps approximating with a radius
/// instead). Left at the original value until one of those settles it, rather than trade one visual bug for a
/// functional regression on a real level.
pub const SPAWN_FLAT_RADIUS: i32 = 3;
/// Ring around it pulled halfway towards the flat height, so the platform has no cliff.
pub const SPAWN_BLEND_RADIUS: i32 = 4;
/// The flattened ground is never lower than this: a flooded site comes back as land.
pub const MIN_SPAWN_HEIGHT: u16 = 32;

impl ReincarnationSite {
    /// Height points (cell corners) within `radius` cells of the site centre (the middle of its cell),
    /// with their squared distance in quarter cells (integer, deterministic).
    #[cfg(test)]
    fn corners_within(&self, radius: i32) -> impl Iterator<Item = ((i32, i32), i32)> {
        let (cx, cz) = self.cell();
        let r = radius + 1;
        (-r..=r + 1).flat_map(move |dz| (-r..=r + 1).map(move |dx| (dx, dz))).filter_map(move |(dx, dz)| {
            let d2 = (2 * dx - 1).pow(2) + (2 * dz - 1).pow(2);
            (d2 <= (2 * radius).pow(2)).then_some(((cx + dx, cz + dz), d2))
        })
    }

    /// Level the ground for a spawning shaman: the inner disc takes the site's own fixed `height` (at least
    /// `MIN_SPAWN_HEIGHT`, a last-resort floor, not the everyday target - a site placed on a sea cell), the ring
    /// around is blended halfway.
    pub fn flatten_for_spawn(&self, terrain: &mut Heightmap) -> DirtyRect {
        terrain.level_around_to(self.spawn_point(), SPAWN_FLAT_RADIUS, SPAWN_BLEND_RADIUS, self.height.max(MIN_SPAWN_HEIGHT))
    }
}

/// Build/rebuild ritual (pop3-rev-analysis.md "Build/rebuild ritual"): what an `animated`
/// `ReincarnationSite`'s shaman is doing before she's free, carried on her own `Action::BuildingSite`.
/// Walking to the anchor (state 0) isn't a state of its own here: she always spawns exactly on it (no site
/// relocation, `ReincarnationSite`'s own doc), so that leg is always zero distance.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SiteBuildState {
    /// 12-turn settle before the ground levelling (state 1).
    Settling(Countdown),
    /// The ground levelling runs - instantly here (`flatten_for_spawn`, TODO.md "Terrain" #302 tracks the
    /// full animated growing disc separately); this just waits out its own ~21-turn length before the
    /// pillars fire (state 2).
    Levelling(Countdown),
    /// One standard shot per pillar slot, one turn apart, slot order 7..0 (state 3). `fired`: how many
    /// have gone out so far.
    Firing { fired: u8 },
}

/// 12-turn settle before the levelling starts (measured).
pub const SETTLE_TICKS: Ticks = Ticks::new(12);
/// The ground levelling's own length (measured, "Levelling" below); only its timing is reproduced here
/// (TODO.md "Terrain" #302), not its growing-disc visual.
pub const LEVEL_TICKS: Ticks = Ticks::new(21);
/// A shot's speed, world units per turn: "the standard shot other spells use" (`blast::FIREBALL_SPEED`'s
/// own doc), 1400.
pub const STANDARD_SHOT_SPEED: i32 = 1400;

impl crate::map::GameMap {
    /// Unit `i`, a shaman firing pillar `slot` of her own `Action::BuildingSite` (state 3): a standard shot
    /// from where she stands to the slot's own target cell.
    pub(crate) fn fire_site_pillar(&mut self, i: usize, slot: u8) {
        let Some(owner) = self.units[i].owner.tribe() else { return };
        let Some(site) = self.sites.iter().find(|s| s.owner == owner && s.enabled && s.animated) else { return };
        let target = site.slot_target(slot);
        let aim = (target.0, target.1, self.terrain.height_at(target.0 as u32, target.1 as u32, WORLD_UNITS_PER_CELL));
        let (at, height) = ((self.units[i].x, self.units[i].z), self.altitude(&self.units[i]) as i32);
        let id = self.new_id_of();
        self.site_shots.insert(id, SiteShot { owner, slot, at, height, aim });
    }

    /// Every site shot flies a turn towards its slot; on arrival any pillar already standing there is
    /// destroyed (a burst) and the new one appears at once (a second burst, a "pop-in" flash) - "nothing
    /// polls per turn to place a pillar" (pop3-rev-analysis.md "Build/rebuild ritual"): the whole stagger
    /// between slots is pure shot travel time on top of the one-per-turn firing order, same as here.
    pub(crate) fn tend_site_shots(&mut self) -> Vec<WorldEvent> {
        let mut shots = std::mem::take(&mut self.site_shots);
        let mut arrived = Vec::new();
        let mut events = Vec::new();
        for (id, s) in shots.iter_mut() {
            let (at, height, there) = fly(s.at, s.height, s.aim);
            (s.at, s.height) = (at, height);
            if there {
                arrived.push(id);
                let replacing = self.sites.iter().any(|site| site.owner == s.owner && site.pillars[s.slot as usize].is_some());
                if replacing {
                    events.push(WorldEvent::SpawnEffect(crate::blast::explosion_effect(s.owner, s.at)));
                }
                events.push(WorldEvent::SitePillarBuilt { owner: s.owner, slot: s.slot });
                events.push(WorldEvent::SpawnEffect(crate::blast::explosion_effect(s.owner, s.at)));
            }
        }
        shots.retain(|id, _| !arrived.contains(&id));
        self.site_shots = shots;
        events
    }
}

/// One of the build/rebuild ritual's own standard shots in flight, from the shaman to one pillar slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SiteShot {
    pub owner: TribeId,
    pub slot: u8,
    /// Where it is now: world units and height.
    pub at: (u16, u16),
    pub height: i32,
    /// Where it lands: world units and the ground height there, read once at the shot.
    pub aim: (u16, u16, i32),
}

/// The step a site shot at `at`/`h` makes towards `aim`: its next place, and whether it got there (same
/// shape as `blast::fly`, at the standard shot's own speed instead of a fireball's).
fn fly(at: (u16, u16), h: i32, aim: (u16, u16, i32)) -> ((u16, u16), i32, bool) {
    let (dx, dz, dy) = (torus_delta(at.0, aim.0), torus_delta(at.1, aim.1), aim.2 - h);
    let d2 = (dx as i64).pow(2) + (dz as i64).pow(2) + (dy as i64).pow(2);
    if d2 <= (STANDARD_SHOT_SPEED as i64).pow(2) {
        return ((aim.0, aim.1), aim.2, true);
    }
    let d = isqrt(d2.min(u32::MAX as i64) as u32).max(1) as i32;
    let step = |v: i32| v * STANDARD_SHOT_SPEED / d;
    ((at.0.wrapping_add(step(dx) as u16), at.1.wrapping_add(step(dz) as u16)), h + step(dy), false)
}

/// One site per tribe, at its shaman's start position; first shaman wins, sorted by owner. `height` is
/// captured once from `terrain`'s own value at the site's cell - the *original*, unmodified level terrain, not
/// recomputed later (`ReincarnationSite::height`'s own doc comment).
pub fn sites_from_level(level: &Level, terrain: &Heightmap) -> Vec<ReincarnationSite> {
    let mut sites: Vec<ReincarnationSite> = Vec::new();
    for t in level.things.iter().filter(|t| t.is_shaman()) {
        let Some(owner) = TribeId::from_u8(t.owner) else { continue };
        if !sites.iter().any(|s| s.owner == owner) {
            let height = terrain.get((t.x as u32 / WORLD_UNITS_PER_CELL) as i32, (t.z as u32 / WORLD_UNITS_PER_CELL) as i32);
            sites.push(ReincarnationSite { owner, x: t.x, z: t.z, height, enabled: true, animated: true, pillars: [None; 8] });
        }
    }
    sites.sort_by_key(|s| s.owner);
    sites
}

/// Two sites for a generated map: tribe 0 on low inland ground, tribe 1 on the land
/// cell farthest from it on the torus.
pub fn generated_sites(terrain: &Heightmap) -> Vec<ReincarnationSite> {
    let first = terrain.lowland_cell();
    let s = terrain.size() as i32;
    let torus = |a: i32, b: i32| {
        let d = (a - b).rem_euclid(s);
        d.min(s - d)
    };
    let second = (0..s * s)
        .map(|i| (i % s, i / s))
        .filter(|&(x, z)| !terrain.is_water(x, z))
        .max_by_key(|&(x, z)| (torus(x, first.0).pow(2) + torus(z, first.1).pow(2), -(z * s + x)));
    let mut sites = vec![ReincarnationSite::at_cell(TribeId::Blue, first, terrain.get(first.0, first.1))];
    sites.extend(second.filter(|&c| c != first).map(|c| ReincarnationSite::at_cell(TribeId::Red, c, terrain.get(c.0, c.1))));
    sites
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn welcomes_the_ring_around_it_across_the_edge() {
        let site = ReincarnationSite::at_cell(TribeId::Blue, (0, 46), 0);
        assert!(site.welcomes((0, 46)) && site.welcomes((127, 45)) && site.welcomes((1, 47)));
        assert!(!site.welcomes((2, 46)) && !site.welcomes((0, 48)));
    }
    use pop3_format::level::{DAT_SIZE, KIND_PERSON, PERSON_SHAMAN};

    fn level_with(things: &[[u8; 7]]) -> Level {
        let mut d = vec![0u8; DAT_SIZE];
        let base = d.len() - 95 - 2000 * 55;
        for (i, t) in things.iter().enumerate() {
            d[base + i * 55..base + i * 55 + 7].copy_from_slice(t);
        }
        Level::parse(&d).unwrap()
    }

    #[test]
    fn one_site_per_tribe_from_shamans() {
        let lvl = level_with(&[
            [PERSON_SHAMAN, KIND_PERSON, 1, 0x00, 0x09, 0x00, 0x1d],
            [2, KIND_PERSON, 0, 0x00, 0x01, 0x00, 0x01],
            [PERSON_SHAMAN, KIND_PERSON, 0, 0x00, 0x11, 0x00, 0xd7],
            [PERSON_SHAMAN, KIND_PERSON, 1, 0x00, 0x20, 0x00, 0x20],
        ]);
        let terrain = Heightmap::from_heights(pop3_format::MAP_SIZE, lvl.heights.clone());
        let sites = sites_from_level(&lvl, &terrain);
        assert_eq!(
            sites,
            vec![
                ReincarnationSite { owner: TribeId::Blue, x: 0x1100, z: 0xd700, height: 0, enabled: true, animated: true, pillars: [None; 8] },
                ReincarnationSite { owner: TribeId::Red, x: 0x0900, z: 0x1d00, height: 0, enabled: true, animated: true, pillars: [None; 8] },
            ]
        );
        assert_eq!(sites[0].cell(), (8, 107));
    }

    #[test]
    fn sites_from_level_captures_the_original_terrain_height() {
        let lvl = level_with(&[[PERSON_SHAMAN, KIND_PERSON, 0, 0x00, 0x11, 0x00, 0xd7]]);
        let mut terrain = Heightmap::from_heights(pop3_format::MAP_SIZE, lvl.heights.clone());
        terrain.set(8, 107, 200);
        let sites = sites_from_level(&lvl, &terrain);
        assert_eq!(sites[0].height, 200);
    }

    #[test]
    fn at_cell_is_cell_centre_and_wraps() {
        let s = ReincarnationSite::at_cell(TribeId::Yellow, (-1, 3), 0);
        assert_eq!((s.x, s.z), (127 * 512 + 256, 3 * 512 + 256));
        assert_eq!(s.cell(), (127, 3));
        assert_eq!(s.spawn_point(), (s.x, s.z));
    }

    #[test]
    fn spawn_flattens_the_ring_to_the_sites_own_stored_height() {
        // Not a live average of the surrounding ground (the bug, TODO.md "Terrain", 2026-10-21: a flooded site
        // or an unlucky `row_jitter` could then read as water) - the site's own fixed `height`, same every time.
        let mut t = Heightmap::new(32);
        let site = ReincarnationSite::at_cell(TribeId::Blue, (10, 10), 200);
        t.raise((10, 10), 6, 400); // the surrounding ground is quite different from the site's own height
        let before = t.clone();
        site.flatten_for_spawn(&mut t);
        let inner: Vec<_> = site.corners_within(SPAWN_FLAT_RADIUS).map(|(c, _)| c).collect();
        assert!(inner.iter().all(|&(x, z)| t.get(x, z) == 200), "inner disc is flat, at the site's own height");
        assert!(inner.contains(&(8, 10)) && inner.contains(&(12, 12)), "covers the stone ring");
        assert!(!inner.contains(&(7, 10)));
        let edge = before.get(7, 10);
        assert_eq!(t.get(7, 10), (edge + 200) / 2, "blend ring");
        assert_eq!(t.get(2, 2), before.get(2, 2), "far ground untouched");
    }

    #[test]
    fn flooded_site_becomes_land_across_the_map_edge() {
        let mut t = Heightmap::new(32);
        let site = ReincarnationSite::at_cell(TribeId::Red, (0, 31), 0); // height 0: MIN_SPAWN_HEIGHT's own floor kicks in
        site.flatten_for_spawn(&mut t);
        for (x, z) in [(0, 31), (1, 0), (31, 30), (2, 1)] {
            assert_eq!(t.get(x, z), MIN_SPAWN_HEIGHT, "({x},{z}) wraps and is land");
        }
        assert!(!t.is_water(0, 0));
    }

    #[test]
    fn generated_sites_are_on_land_and_far_apart() {
        let mut t = Heightmap::new(128);
        t.raise((10, 10), 4, 300);
        t.raise((74, 74), 4, 300);
        let sites = generated_sites(&t);
        assert_eq!(sites.len(), 2);
        for s in &sites {
            assert!(!t.is_water(s.cell().0, s.cell().1));
        }
        let (a, b) = (sites[0].cell(), sites[1].cell());
        assert!((a.0 - b.0).abs() > 40 && (a.1 - b.1).abs() > 40, "{a:?} {b:?}");
    }

    #[test]
    fn slot_targets_sit_on_the_octagon_around_the_site() {
        let site = ReincarnationSite::at_cell(TribeId::Blue, (10, 20), 0);
        let centre = (10, 20);
        for (slot, &(dx, dz)) in SLOTS.iter().enumerate() {
            assert_eq!(site.slot_target(slot as u8), ReincarnationSite::at_cell(TribeId::Blue, (centre.0 + dx, centre.1 + dz), 0).spawn_point());
        }
    }

    #[test]
    fn pillar_built_is_always_up_without_animation_only_tracked_with_it() {
        let plain = ReincarnationSite::at_cell(TribeId::Blue, (10, 10), 0);
        assert_eq!(plain.pillar_built(3), Some(Tick::ZERO));
        let mut animated = ReincarnationSite { animated: true, ..plain };
        assert_eq!(animated.pillar_built(3), None, "not built yet");
        animated.pillars[3] = Some(Tick::from_wire(50));
        assert_eq!(animated.pillar_built(3), Some(Tick::from_wire(50)));
    }

    #[test]
    fn a_site_shot_reaches_its_aim_within_one_turn_at_short_range() {
        assert_eq!(fly((0, 0), 0, (100, 0, 0)), ((100, 0), 0, true));
    }

    #[test]
    fn a_site_shot_steps_at_its_own_speed_towards_a_far_aim() {
        let (at, height, there) = fly((0, 0), 0, (10_000, 0, 0));
        assert_eq!((at, height, there), ((STANDARD_SHOT_SPEED as u16, 0), 0, false));
    }
}
