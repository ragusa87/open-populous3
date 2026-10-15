//! Lava's own look: a shared ground effect for Earthquake's fissure and Volcano's flow (docs/specs/spells.md
//! "To do", docs/plans/spells.md "Earthquake": "plan a single shared lava effect module both reuse";
//! pop3-rev-analysis.md "Lava": hot cells ignite people, buildings and trees, sparing only the caster's own
//! shaman). No `EffectKind` or spell drives this yet (TODO.md): this is its procedural texture set (seamless,
//! tileable, no original art to trace: the original has no lava tile in its blend tables,
//! docs/specs/terrain-textures.md) and the ground tile it renders on, ready for whichever lands first.
//! Pure functions, no Bevy, down to `generate`, like `procedural_theme`/`terrain_texture`.

use crate::original_models::MeshData;
use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use std::f32::consts::TAU;

/// Every generated texture is this many pixels square, tiling seamlessly.
pub const SIZE: usize = 128;

/// Pattern scale (noise period, cells per tile), swirl, crust cover and crack width: the look's own tuning,
/// not a traced constant.
const PATTERN: i32 = 4;
const WARP: f32 = 0.6;
const CRUST_COVER: f32 = 0.45;
const CRACK: f32 = 0.08;

/// `mulberry32`: a small, fast, seedable PRNG, floats in `[0, 1)`.
struct Rng(u32);

impl Rng {
    fn next(&mut self) -> f32 {
        self.0 = self.0.wrapping_add(0x6D2B_79F5);
        let mut t = self.0;
        t = (t ^ (t >> 15)).wrapping_mul(t | 1);
        t ^= t.wrapping_add((t ^ (t >> 7)).wrapping_mul(t | 61));
        (t ^ (t >> 14)) as f32 / 4294967296.0
    }
}

/// Tileable Perlin noise: a permutation and a gradient per cell, each wrapped to a given period so it repeats
/// without a seam (`perlin`'s own `rem_euclid`), `fbm` layering it at doubling frequencies.
struct Noise {
    perm: [u8; 256],
    grad: Vec<(f32, f32)>,
}

fn make_noise(seed: u32) -> Noise {
    let mut rng = Rng(seed);
    let mut perm: [u8; 256] = std::array::from_fn(|i| i as u8);
    for i in (1..=255).rev() {
        let j = (rng.next() * (i as f32 + 1.0)) as usize;
        perm.swap(i, j.min(i));
    }
    let grad = (0..256).map(|_| { let a = rng.next() * TAU; (a.cos(), a.sin()) }).collect();
    Noise { perm, grad }
}

impl Noise {
    fn p(&self, i: i32) -> i32 {
        self.perm[i.rem_euclid(256) as usize] as i32
    }

    fn perlin(&self, x: f32, y: f32, period: i32) -> f32 {
        let period = period.max(1);
        let (xi, yi) = (x.floor() as i32, y.floor() as i32);
        let (fx, fy) = (x - xi as f32, y - yi as f32);
        let (i0, i1) = (xi.rem_euclid(period), (xi + 1).rem_euclid(period));
        let (j0, j1) = (yi.rem_euclid(period), (yi + 1).rem_euclid(period));
        let grad_at = |i: i32, j: i32| self.grad[self.p(self.p(i) + j) as usize];
        let dot = |i: i32, j: i32, ox: f32, oy: f32| { let (gx, gy) = grad_at(i, j); gx * ox + gy * oy };
        let fade = |t: f32| t * t * t * (t * (t * 6.0 - 15.0) + 10.0);
        let (u, v) = (fade(fx), fade(fy));
        let top = dot(i0, j0, fx, fy) + (dot(i1, j0, fx - 1.0, fy) - dot(i0, j0, fx, fy)) * u;
        let bot = dot(i0, j1, fx, fy - 1.0) + (dot(i1, j1, fx - 1.0, fy - 1.0) - dot(i0, j1, fx, fy - 1.0)) * u;
        top + (bot - top) * v
    }

    fn fbm(&self, u: f32, v: f32, period: i32, octaves: u32) -> f32 {
        let (mut sum, mut amp, mut f, mut norm) = (0.0, 0.5, 1.0, 0.0);
        for _ in 0..octaves {
            let pf = ((period as f32 * f).min(256.0)) as i32;
            sum += amp * self.perlin(u * pf as f32, v * pf as f32, pf);
            norm += amp;
            amp *= 0.5;
            f *= 2.0;
        }
        sum / norm
    }
}

/// Tileable Worley (cellular) noise: one random point per cell of a `c` x `c` grid, wrapping at its edges;
/// `sample` returns the nearest and second-nearest distances (`F1`, `F2`, in cell units).
struct Worley {
    points: Vec<(f32, f32)>,
    c: i32,
}

fn make_worley(seed: u32, c: i32) -> Worley {
    let mut rng = Rng(seed ^ 0x9e37_79b9);
    let points = (0..c * c).map(|_| (rng.next(), rng.next())).collect();
    Worley { points, c }
}

impl Worley {
    fn sample(&self, u: f32, v: f32) -> (f32, f32) {
        let (px, py) = (u * self.c as f32, v * self.c as f32);
        let (cx, cy) = (px.floor() as i32, py.floor() as i32);
        let (mut f1, mut f2) = (f32::MAX, f32::MAX);
        for dy in -1..=1 {
            for dx in -1..=1 {
                let (nx, ny) = (cx + dx, cy + dy);
                let (wx, wy) = (nx.rem_euclid(self.c), ny.rem_euclid(self.c));
                let (ox, oy) = self.points[(wy * self.c + wx) as usize];
                let d = ((px - (nx as f32 + ox)).powi(2) + (py - (ny as f32 + oy)).powi(2)).sqrt();
                if d < f1 {
                    f2 = f1;
                    f1 = d;
                } else if d < f2 {
                    f2 = d;
                }
            }
        }
        (f1, f2)
    }
}

fn clamp01(x: f32) -> f32 {
    x.clamp(0.0, 1.0)
}
fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = clamp01((x - a) / (b - a));
    t * t * (3.0 - 2.0 * t)
}
fn mix(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}
fn mix3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    std::array::from_fn(|i| mix(a[i], b[i], t))
}
fn scale3(c: [f32; 3], k: f32) -> [f32; 3] {
    c.map(|v| v * k)
}
/// `fbm` runs about `[-0.7, 0.7]`: recentred into `[0, 1]`.
fn to01(n: f32) -> f32 {
    clamp01(n / 1.1 + 0.5)
}

const CORE: [f32; 3] = [0.0, 0.0, 0.0];
const HOT: [f32; 3] = [0.6, 0.0, 0.0];
const MID: [f32; 3] = [0.5216, 0.2196, 0.2196];
const DARK: [f32; 3] = [0.3608, 0.0588, 0.0196];
const CRUST_COLOUR: [f32; 3] = [0.1686, 0.1255, 0.1098];
const ROCK: [f32; 3] = [0.2392, 0.2118, 0.2];

fn ash_colour() -> [f32; 3] {
    mix3(ROCK, [0.7765, 0.7451, 0.7137], 0.5)
}

/// The molten colour gradient: dark crust, through mid and hot, to the (here black) core.
fn ramp(t: f32) -> [f32; 3] {
    if t < 0.38 {
        mix3(DARK, MID, t / 0.38)
    } else if t < 0.68 {
        mix3(MID, HOT, (t - 0.38) / 0.30)
    } else if t < 0.92 {
        mix3(HOT, CORE, (t - 0.68) / 0.24)
    } else {
        CORE
    }
}

/// Four seamless `SIZE` x `SIZE` RGBA8 textures sharing one generation pass: the hot, flowing surface
/// (`lava`), its cooling, cracked skin (`crust`), the solid rock it leaves (`basalt`), and the ash settling
/// over it (`ash`).
pub struct LavaTextures {
    pub lava: Vec<u8>,
    pub crust: Vec<u8>,
    pub basalt: Vec<u8>,
    pub ash: Vec<u8>,
}

pub fn generate(seed: u32) -> LavaTextures {
    let noise = make_noise(seed);
    let plates = make_worley(seed, PATTERN * 2);
    let vesicle = make_worley(seed.wrapping_add(101), PATTERN * 4);
    let mut grain = Rng(seed.wrapping_add(55));
    let ash = ash_colour();

    let th_lava = 0.85 - CRUST_COVER * 0.7;
    let th_crust = 0.85 - (CRUST_COVER + 0.55).min(1.0) * 0.7;
    let crack_c = CRACK * 0.55;

    let mut out = LavaTextures {
        lava: vec![0u8; SIZE * SIZE * 4],
        crust: vec![0u8; SIZE * SIZE * 4],
        basalt: vec![0u8; SIZE * SIZE * 4],
        ash: vec![0u8; SIZE * SIZE * 4],
    };

    for y in 0..SIZE {
        for x in 0..SIZE {
            let (u, v) = ((x as f32 + 0.5) / SIZE as f32, (y as f32 + 0.5) / SIZE as f32);

            let wu = u + WARP * 0.14 * noise.fbm(u + 0.13, v + 0.71, PATTERN, 3);
            let wv = v + WARP * 0.14 * noise.fbm(u + 0.57, v + 0.29, PATTERN, 3);

            let n = to01(noise.fbm(wu, wv, PATTERN, 5));
            let ridge = clamp01(1.0 - noise.fbm(wu + 0.37, wv + 0.11, PATTERN * 2, 4).abs() * 2.4).powi(3);
            let pn = to01(noise.fbm(u + 0.91, v + 0.43, PATTERN, 4));
            let fine = to01(noise.fbm(u + 0.2, v + 0.8, (PATTERN * 8).min(128), 2));

            let (f1, f2) = plates.sample(wu, wv);
            let e = f2 - f1;

            let heat = clamp01(0.1 + n * 0.8 + ridge * 0.45);
            let molten = ramp(heat);
            let plate_colour = scale3(mix3(CRUST_COLOUR, ROCK, 0.2 + 0.5 * fine), 0.8 + 0.4 * smoothstep(0.0, 0.5, e));

            let rim = 1.0 - smoothstep(CRACK, CRACK + 0.12, e);
            let mask = smoothstep(th_lava - 0.07, th_lava + 0.07, pn) * smoothstep(CRACK, CRACK + 0.05, e);
            let lava_px = mix3(molten, mix3(plate_colour, MID, rim * 0.5), mask);

            let rim_c = 1.0 - smoothstep(crack_c, crack_c + 0.10, e);
            let mask_c = smoothstep(th_crust - 0.07, th_crust + 0.07, pn) * smoothstep(crack_c, crack_c + 0.05, e);
            let crust_px = mix3(ramp(heat * 0.72), mix3(plate_colour, DARK, rim_c * 0.6), mask_c);

            let (b1, b2) = vesicle.sample(u, v);
            let eb = b2 - b1;
            let mut k = 0.7 + to01(noise.fbm(u + 0.33, v + 0.66, PATTERN * 2, 4)) * 0.6 + (fine - 0.5) * 0.15;
            if b1 < 0.13 {
                k *= 0.55 + b1 * 3.0;
            }
            if eb < 0.04 {
                k *= 0.55 + eb * 8.0;
            }
            let basalt_px = scale3(ROCK, k);

            let ash_k = 0.82 + fine * 0.3 + (grain.next() - 0.5) * 0.12 - if eb < 0.03 { 0.1 } else { 0.0 };
            let ash_px = scale3(ash, ash_k);

            let i = (y * SIZE + x) * 4;
            write(&mut out.lava, i, lava_px);
            write(&mut out.crust, i, crust_px);
            write(&mut out.basalt, i, basalt_px);
            write(&mut out.ash, i, ash_px);
        }
    }
    out
}

fn write(buf: &mut [u8], i: usize, c: [f32; 3]) {
    for (k, v) in c.into_iter().enumerate() {
        buf[i + k] = (clamp01(v) * 255.0).round() as u8;
    }
    buf[i + 3] = 255;
}

/// The molten `lava` texture as a repeating, unlit ground material (it reads as glowing regardless of scene
/// light, like `flame`'s boards): no original art to trace, generated instead.
#[allow(dead_code)] // awaits Earthquake/Volcano (docs/plans/spells.md), the first callers
pub fn lava_material(seed: u32, images: &mut Assets<Image>, materials: &mut Assets<StandardMaterial>) -> Handle<StandardMaterial> {
    let mut image = Image::new(
        Extent3d { width: SIZE as u32, height: SIZE as u32, depth_or_array_layers: 1 },
        TextureDimension::D2,
        generate(seed).lava,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        ..default()
    });
    materials.add(StandardMaterial { base_color_texture: Some(images.add(image)), unlit: true, ..default() })
}

/// A flat, ground-level quad `cells` wide and deep, UVs spanning `0..cells` so a repeating material
/// (`lava_material`) tiles once per cell instead of stretching across the whole tile.
pub fn tile_mesh(cells: f32) -> MeshData {
    let h = cells / 2.0;
    let corners = [[-h, 0.0, -h], [h, 0.0, -h], [h, 0.0, h], [-h, 0.0, h]];
    let uvs = [[0.0, 0.0], [cells, 0.0], [cells, cells], [0.0, cells]];
    let mut m = MeshData::default();
    for k in [0, 1, 2, 0, 2, 3] {
        m.indices.push(m.positions.len() as u32);
        m.positions.push(corners[k]);
        m.normals.push([0.0, 1.0, 0.0]);
        m.uvs.push(uvs[k]);
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rng_is_deterministic_and_in_range() {
        let (mut a, mut b) = (Rng(7), Rng(7));
        let (va, vb) = (a.next(), b.next());
        assert_eq!(va, vb);
        assert!((0.0..1.0).contains(&va));
        assert_ne!(Rng(7).next(), Rng(8).next());
    }

    #[test]
    fn perlin_wraps_seamlessly_on_its_period() {
        let noise = make_noise(962337);
        for (x, y) in [(0.3, 0.6), (2.7, 1.1), (3.99, 0.0)] {
            assert!((noise.perlin(x, y, 4) - noise.perlin(x + 4.0, y, 4)).abs() < 1e-5, "wraps on x");
            assert!((noise.perlin(x, y, 4) - noise.perlin(x, y + 4.0, 4)).abs() < 1e-5, "wraps on y");
        }
    }

    #[test]
    fn worley_wraps_seamlessly_on_its_grid() {
        let w = make_worley(1, 8);
        for (u, v) in [(0.1, 0.2), (0.45, 0.9)] {
            let a = w.sample(u, v);
            let b = w.sample(u + 1.0, v);
            assert!((a.0 - b.0).abs() < 1e-4 && (a.1 - b.1).abs() < 1e-4);
        }
    }

    #[test]
    fn ramp_runs_dark_to_the_core_and_stays_in_range() {
        assert_eq!(ramp(0.0), DARK);
        assert_eq!(ramp(1.0), CORE);
        for i in 0..=100 {
            let c = ramp(i as f32 / 100.0);
            assert!(c.iter().all(|&v| (0.0..=1.0).contains(&v)));
        }
    }

    #[test]
    fn generate_is_deterministic_and_the_right_shape() {
        let a = generate(962337);
        let b = generate(962337);
        assert_eq!(a.lava, b.lava);
        for buf in [&a.lava, &a.crust, &a.basalt, &a.ash] {
            assert_eq!(buf.len(), SIZE * SIZE * 4);
            assert!(buf.chunks_exact(4).all(|p| p[3] == 255), "opaque");
        }
        assert_ne!(a.lava, generate(1).lava, "a different seed, a different flow");
        assert_ne!(a.lava, a.crust, "distinct textures");
    }

    #[test]
    fn tile_mesh_spans_its_cells_with_matching_uvs() {
        let m = tile_mesh(2.0);
        assert_eq!(m.indices.len(), 6);
        assert!(m.positions.iter().all(|p| p[1] == 0.0 && p[0].abs() <= 1.0 && p[2].abs() <= 1.0));
        assert!(m.uvs.iter().any(|uv| uv[0] == 2.0 || uv[1] == 2.0), "tiles twice across 2 cells");
    }
}
