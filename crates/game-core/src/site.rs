//! Reincarnation sites: where each tribe's shaman spawns and respawns.
//! Fixed for the whole game and indestructible: no command removes or moves them.

use crate::terrain::{DirtyRect, Heightmap};
use pop3_format::{Level, WORLD_UNITS_PER_CELL};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReincarnationSite {
    pub owner: u8,
    /// Centre in world units (512 per cell), wraps at 65536.
    pub x: u16,
    pub z: u16,
}

impl ReincarnationSite {
    pub fn at_cell(owner: u8, (x, z): (i32, i32)) -> Self {
        let unit = |c: i32| (c.rem_euclid(128) as u32 * WORLD_UNITS_PER_CELL + WORLD_UNITS_PER_CELL / 2) as u16;
        ReincarnationSite { owner, x: unit(x), z: unit(z) }
    }

    /// Cell containing the site centre.
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

    /// Level the ground for a spawning shaman: the inner disc takes its average height (at least
    /// `MIN_SPAWN_HEIGHT`, so never water), the ring around is blended halfway.
    pub fn flatten_for_spawn(&self, terrain: &mut Heightmap) -> DirtyRect {
        terrain.level_around(self.spawn_point(), SPAWN_FLAT_RADIUS, SPAWN_BLEND_RADIUS, MIN_SPAWN_HEIGHT)
    }
}

/// One site per tribe, at its shaman's start position; first shaman wins, sorted by owner.
pub fn sites_from_level(level: &Level) -> Vec<ReincarnationSite> {
    let mut sites: Vec<ReincarnationSite> = Vec::new();
    for t in level.things.iter().filter(|t| t.is_shaman()) {
        if !sites.iter().any(|s| s.owner == t.owner) {
            sites.push(ReincarnationSite { owner: t.owner, x: t.x, z: t.z });
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
    let mut sites = vec![ReincarnationSite::at_cell(0, first)];
    sites.extend(second.filter(|&c| c != first).map(|c| ReincarnationSite::at_cell(1, c)));
    sites
}

#[cfg(test)]
mod tests {
    use super::*;
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
        let sites = sites_from_level(&lvl);
        assert_eq!(
            sites,
            vec![
                ReincarnationSite { owner: 0, x: 0x1100, z: 0xd700 },
                ReincarnationSite { owner: 1, x: 0x0900, z: 0x1d00 },
            ]
        );
        assert_eq!(sites[0].cell(), (8, 107));
    }

    #[test]
    fn at_cell_is_cell_centre_and_wraps() {
        let s = ReincarnationSite::at_cell(2, (-1, 3));
        assert_eq!((s.x, s.z), (127 * 512 + 256, 3 * 512 + 256));
        assert_eq!(s.cell(), (127, 3));
        assert_eq!(s.spawn_point(), (s.x, s.z));
    }

    #[test]
    fn spawn_flattens_the_ring_to_its_average() {
        let mut t = Heightmap::new(32);
        let site = ReincarnationSite::at_cell(0, (10, 10));
        t.raise((10, 10), 6, 400);
        let before = t.clone();
        site.flatten_for_spawn(&mut t);
        let inner: Vec<_> = site.corners_within(SPAWN_FLAT_RADIUS).map(|(c, _)| c).collect();
        let h = t.get(10, 10);
        assert!(inner.iter().all(|&(x, z)| t.get(x, z) == h), "inner disc is flat");
        assert!(inner.contains(&(8, 10)) && inner.contains(&(12, 12)), "covers the stone ring");
        assert!(!inner.contains(&(7, 10)));
        let edge = before.get(7, 10);
        assert_eq!(t.get(7, 10), (edge + h) / 2, "blend ring");
        assert_eq!(t.get(2, 2), before.get(2, 2), "far ground untouched");
    }

    #[test]
    fn flooded_site_becomes_land_across_the_map_edge() {
        let mut t = Heightmap::new(32);
        let site = ReincarnationSite::at_cell(1, (0, 31));
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
