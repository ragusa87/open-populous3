//! Authoritative, deterministic terrain: a wrapping (torus) heightmap of integers.

pub const SEA_LEVEL: u16 = 0;
pub const MAX_HEIGHT: u16 = 1024;

/// Wrapping square heightmap. Integer-only so lockstep peers stay in sync.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Heightmap {
    size: usize,
    heights: Vec<u16>,
}

/// Inclusive cell rectangle touched by an edit, in unwrapped coordinates
/// (may extend past the edges; consumers wrap with `rem_euclid`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DirtyRect {
    pub min: (i32, i32),
    pub max: (i32, i32),
}

impl Heightmap {
    pub fn new(size: usize) -> Self {
        Heightmap { size, heights: vec![SEA_LEVEL; size * size] }
    }

    pub fn from_heights(size: usize, heights: Vec<u16>) -> Self {
        assert_eq!(heights.len(), size * size, "heightmap must be size*size");
        Heightmap { size, heights }
    }

    pub fn size(&self) -> usize {
        self.size
    }

    pub fn heights(&self) -> &[u16] {
        &self.heights
    }

    fn index(&self, x: i32, z: i32) -> usize {
        let s = self.size as i32;
        (z.rem_euclid(s) * s + x.rem_euclid(s)) as usize
    }

    /// Height at any integer cell; coordinates wrap on both axes.
    pub fn get(&self, x: i32, z: i32) -> u16 {
        self.heights[self.index(x, z)]
    }

    pub fn set(&mut self, x: i32, z: i32, h: u16) {
        let i = self.index(x, z);
        self.heights[i] = h.min(MAX_HEIGHT);
    }

    pub fn is_water(&self, x: i32, z: i32) -> bool {
        self.get(x, z) == SEA_LEVEL
    }

    /// Highest cell (first one in row-major order on ties); handy to frame a level.
    pub fn highest_cell(&self) -> (i32, i32) {
        let (i, _) = self.heights.iter().enumerate().fold((0, 0), |best, (i, &h)| if h > best.1 { (i, h) } else { best });
        ((i % self.size) as i32, (i / self.size) as i32)
    }

    /// Bilinear height for rendering (float use is fine outside the simulation).
    pub fn sample(&self, x: f32, z: f32) -> f32 {
        let (x0, z0) = (x.floor(), z.floor());
        let (fx, fz) = (x - x0, z - z0);
        let (x0, z0) = (x0 as i32, z0 as i32);
        let h = |dx, dz| self.get(x0 + dx, z0 + dz) as f32;
        let top = h(0, 0) * (1.0 - fx) + h(1, 0) * fx;
        let bot = h(0, 1) * (1.0 - fx) + h(1, 1) * fx;
        top * (1.0 - fz) + bot * fz
    }

    /// Generic circular brush. `op(current, falloff_0_to_256) -> new height`.
    pub fn apply_brush(
        &mut self,
        center: (i32, i32),
        radius: i32,
        mut op: impl FnMut(u16, i32) -> u16,
    ) -> DirtyRect {
        let r2 = radius * radius;
        for dz in -radius..=radius {
            for dx in -radius..=radius {
                let d2 = dx * dx + dz * dz;
                if d2 > r2 {
                    continue;
                }
                let falloff = 256 - d2 * 256 / (r2 + 1);
                let (x, z) = (center.0 + dx, center.1 + dz);
                let new = op(self.get(x, z), falloff);
                self.set(x, z, new);
            }
        }
        DirtyRect {
            min: (center.0 - radius, center.1 - radius),
            max: (center.0 + radius, center.1 + radius),
        }
    }

    pub fn raise(&mut self, center: (i32, i32), radius: i32, amount: i32) -> DirtyRect {
        self.apply_brush(center, radius, |h, f| {
            (h as i32 + amount * f / 256).clamp(0, MAX_HEIGHT as i32) as u16
        })
    }

    /// Set every cell in the radius to the center height (Flatten spell).
    pub fn flatten(&mut self, center: (i32, i32), radius: i32) -> DirtyRect {
        let target = self.get(center.0, center.1);
        self.apply_brush(center, radius, |_, _| target)
    }

    /// Raise a walkable strip between two cells, interpolating heights (Land Bridge).
    /// Walks the shortest way around the torus.
    pub fn land_bridge(&mut self, a: (i32, i32), b: (i32, i32), min_height: u16) -> DirtyRect {
        let s = self.size as i32;
        let wrap_delta = |d: i32| (d + s / 2).rem_euclid(s) - s / 2;
        let (dx, dz) = (wrap_delta(b.0 - a.0), wrap_delta(b.1 - a.1));
        let steps = dx.abs().max(dz.abs()).max(1);
        let (ha, hb) = (self.get(a.0, a.1) as i32, self.get(b.0, b.1) as i32);
        for i in 0..=steps {
            let (x, z) = (a.0 + dx * i / steps, a.1 + dz * i / steps);
            let h = (ha + (hb - ha) * i / steps).max(min_height as i32) as u16;
            for (ox, oz) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                if self.get(x + ox, z + oz) < h {
                    self.set(x + ox, z + oz, h);
                }
            }
        }
        DirtyRect {
            min: (a.0.min(a.0 + dx), a.1.min(a.1 + dz)),
            max: (a.0.max(a.0 + dx) + 1, a.1.max(a.1 + dz) + 1),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_both_axes() {
        let mut m = Heightmap::new(8);
        m.set(-1, -1, 50);
        assert_eq!(m.get(7, 7), 50);
        assert_eq!(m.get(15, 15), 50);
    }

    #[test]
    fn raise_is_strongest_at_center_and_clamped() {
        let mut m = Heightmap::new(16);
        m.raise((4, 4), 3, 100);
        assert_eq!(m.get(4, 4), 100);
        assert!(m.get(6, 4) < m.get(4, 4));
        assert_eq!(m.get(8, 4), 0);
        m.raise((4, 4), 3, 5000);
        assert_eq!(m.get(4, 4), MAX_HEIGHT);
    }

    #[test]
    fn flatten_lowers_toward_center() {
        let mut m = Heightmap::new(16);
        m.set(5, 5, 100);
        m.set(6, 5, 300);
        m.flatten((5, 5), 2);
        assert_eq!(m.get(6, 5), 100);
    }

    #[test]
    fn land_bridge_crosses_water_across_edge() {
        let mut m = Heightmap::new(16);
        m.set(14, 2, 200);
        m.set(2, 2, 200);
        m.land_bridge((14, 2), (2, 2), 10);
        for x in [14, 15, 0, 1, 2] {
            assert!(!m.is_water(x, 2), "cell {x} should be land");
        }
        assert!(m.is_water(8, 2), "bridge goes the short way round");
    }

    #[test]
    fn highest_cell_finds_peak() {
        let mut m = Heightmap::new(8);
        m.set(3, 6, 9);
        assert_eq!(m.highest_cell(), (3, 6));
    }

    #[test]
    fn sample_interpolates() {
        let mut m = Heightmap::new(4);
        m.set(1, 0, 100);
        assert_eq!(m.sample(0.5, 0.0), 50.0);
    }
}
