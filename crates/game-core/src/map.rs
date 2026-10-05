//! A playable map: terrain + metadata, built from an original level or generated.

use crate::terrain::{Heightmap, MAX_HEIGHT};
use pop3_format::{Level, LevelHeader, MAP_SIZE};
use std::path::Path;

#[derive(Clone, Debug)]
pub struct GameMap {
    pub name: String,
    pub terrain: Heightmap,
}

impl GameMap {
    pub fn from_level(level: &Level, name: impl Into<String>) -> Self {
        GameMap {
            name: name.into(),
            terrain: Heightmap::from_heights(MAP_SIZE, level.heights.clone()),
        }
    }

    /// Load `levlXXXX.dat`, picking the name from the sibling `.hdr` if present.
    pub fn load_original(dat: &Path) -> Result<Self, pop3_format::LevelError> {
        let level = Level::load(dat)?;
        let name = LevelHeader::load(dat.with_extension("hdr"))
            .map(|h| h.name)
            .ok()
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| dat.file_stem().unwrap_or_default().to_string_lossy().into_owned());
        Ok(Self::from_level(&level, name))
    }

    /// Deterministic island map from a seed (fallback when no original data exists).
    pub fn generate(seed: u32) -> Self {
        let mut rng = Lcg(seed.max(1));
        let mut terrain = Heightmap::new(MAP_SIZE);
        for _ in 0..40 {
            let c = ((rng.next() % MAP_SIZE as u32) as i32, (rng.next() % MAP_SIZE as u32) as i32);
            let radius = 4 + (rng.next() % 14) as i32;
            let amount = 150 + (rng.next() % 350) as i32;
            terrain.raise(c, radius, amount);
        }
        for h in 0..MAP_SIZE * MAP_SIZE {
            let (x, z) = ((h % MAP_SIZE) as i32, (h / MAP_SIZE) as i32);
            let v = terrain.get(x, z).saturating_sub(120).min(MAX_HEIGHT);
            terrain.set(x, z, v);
        }
        GameMap { name: format!("Generated #{seed}"), terrain }
    }
}

/// Tiny deterministic PRNG, same sequence on every platform.
pub struct Lcg(pub u32);

impl Lcg {
    pub fn next(&mut self) -> u32 {
        self.0 = self.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        self.0 >> 8
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generation_is_deterministic_and_has_land_and_sea() {
        let a = GameMap::generate(42);
        assert_eq!(a.terrain, GameMap::generate(42).terrain);
        let hs = a.terrain.heights();
        assert!(hs.iter().any(|&h| h == 0));
        assert!(hs.iter().any(|&h| h > 100));
    }
}
