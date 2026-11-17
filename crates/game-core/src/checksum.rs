//! The desync check (multiplayer.md "Schedule"): a hash of the state every peer must agree on after the same
//! tick — the tick, the random draw, the heightmap and each unit — for peers to compare now and then. FNV-1a
//! 64, not `DefaultHasher`, whose output may change between Rust releases and so between two peers' builds.

use crate::map::GameMap;

/// FNV-1a, 64 bits.
#[derive(Clone, Copy, Debug)]
pub struct Fnv(u64);

impl Default for Fnv {
    fn default() -> Self {
        Fnv(0xcbf2_9ce4_8422_2325)
    }
}

impl Fnv {
    pub fn bytes(&mut self, bytes: &[u8]) -> &mut Self {
        for &b in bytes {
            self.0 = (self.0 ^ b as u64).wrapping_mul(0x0000_0100_0000_01b3);
        }
        self
    }

    pub fn finish(&self) -> u64 {
        self.0
    }
}

impl GameMap {
    /// This tick's desync checksum (module doc).
    pub fn checksum(&self) -> u64 {
        let mut h = Fnv::default();
        h.bytes(&self.now.to_wire().to_le_bytes()).bytes(&self.rng.0.to_le_bytes());
        for &height in self.terrain.heights() {
            h.bytes(&height.to_le_bytes());
        }
        for u in &self.units {
            let owner = u.owner.tribe().map_or(u8::MAX, |t| t.as_usize() as u8);
            h.bytes(&u.id.raw().to_le_bytes()).bytes(&[owner, u.kind as u8, u.facing]);
            h.bytes(&u.x.to_le_bytes()).bytes(&u.z.to_le_bytes()).bytes(&u.health.current().to_le_bytes());
            h.bytes(u.action.name().as_bytes());
        }
        h.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::Ticks;

    #[test]
    fn fnv_matches_its_reference_values() {
        assert_eq!(Fnv::default().finish(), 0xcbf2_9ce4_8422_2325);
        assert_eq!(Fnv::default().bytes(b"a").finish(), 0xaf63_dc4c_8601_ec8c);
    }

    #[test]
    fn the_same_game_gives_the_same_checksum_tick_after_tick() {
        let (mut a, mut b) = (GameMap::sandbox_units().seeded(7), GameMap::sandbox_units().seeded(7));
        for _ in 0..20 {
            a.run(Ticks::new(1));
            b.run(Ticks::new(1));
            assert_eq!(a.checksum(), b.checksum());
        }
    }

    #[test]
    fn a_moved_unit_or_a_changed_height_shows() {
        let map = GameMap::sandbox_units();
        let base = map.checksum();
        let mut moved = map.clone();
        moved.units[0].x = moved.units[0].x.wrapping_add(1);
        assert_ne!(moved.checksum(), base);
        let mut raised = map.clone();
        raised.terrain.set(3, 4, raised.terrain.get(3, 4) + 1);
        assert_ne!(raised.checksum(), base);
    }
}
