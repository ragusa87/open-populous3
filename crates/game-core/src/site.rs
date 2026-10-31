//! Reincarnation sites: where each tribe's shaman spawns and respawns.
//! Position is fixed for the whole game: no command moves one. Whether it's currently active can be
//! toggled at runtime (`GameMap::remove_reincarnation`/`restore_reincarnation`, `DO SET_REINCARNATION
//! ON/OFF`, ai-scripts.md) - a disabled site is simply skipped by `GameMap::site_of` and everything
//! built on it (no respawn, not drawn), without losing its place in `GameMap::sites`.

use crate::terrain::{DirtyRect, Heightmap};
use crate::tribes::TribeId;
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
}

impl ReincarnationSite {
    pub fn at_cell(owner: TribeId, (x, z): (i32, i32), height: u16) -> Self {
        let unit = |c: i32| (c.rem_euclid(128) as u32 * WORLD_UNITS_PER_CELL + WORLD_UNITS_PER_CELL / 2) as u16;
        ReincarnationSite { owner, x: unit(x), z: unit(z), height, enabled: true }
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

/// One site per tribe, at its shaman's start position; first shaman wins, sorted by owner. `height` is
/// captured once from `terrain`'s own value at the site's cell - the *original*, unmodified level terrain, not
/// recomputed later (`ReincarnationSite::height`'s own doc comment).
pub fn sites_from_level(level: &Level, terrain: &Heightmap) -> Vec<ReincarnationSite> {
    let mut sites: Vec<ReincarnationSite> = Vec::new();
    for t in level.things.iter().filter(|t| t.is_shaman()) {
        let Some(owner) = TribeId::from_u8(t.owner) else { continue };
        if !sites.iter().any(|s| s.owner == owner) {
            let height = terrain.get((t.x as u32 / WORLD_UNITS_PER_CELL) as i32, (t.z as u32 / WORLD_UNITS_PER_CELL) as i32);
            sites.push(ReincarnationSite { owner, x: t.x, z: t.z, height, enabled: true });
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
                ReincarnationSite { owner: TribeId::Blue, x: 0x1100, z: 0xd700, height: 0, enabled: true },
                ReincarnationSite { owner: TribeId::Red, x: 0x0900, z: 0x1d00, height: 0, enabled: true },
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
}
