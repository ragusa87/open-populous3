//! The ambience (sound.md "Sound id table", ambience bed): a cross-mix of up to three looping layers weighted by
//! the ground around the camera (low land, shore, high land, sea, by the level's theme), seagulls over open sea and
//! wildlife where trees are. Pure choices, thin systems.

use crate::audio::{LoopKey, LoopKind, PlayLoop, PlaySound, SoundCue, StopLoop};
use crate::audio_world::{cells, looping, roll, Kept};
use crate::world::CurrentMap;
use bevy::prelude::*;
use game_core::terrain::Heightmap;

/// A layer of the bed, each the id of its theme.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Layer {
    LowLand,
    Shore,
    HighLand,
    Sea,
    /// Theme 16's sea.
    SeaWind,
    HellLowLand,
    HellShore,
    HellHighLand,
    HellSea,
}

impl Layer {
    pub const ALL: [Layer; 9] = [Layer::LowLand, Layer::Shore, Layer::HighLand, Layer::Sea, Layer::SeaWind, Layer::HellLowLand, Layer::HellShore, Layer::HellHighLand, Layer::HellSea];

    pub fn id(self) -> u8 {
        match self {
            Layer::LowLand => 0x1D,
            Layer::Shore => 0x1E,
            Layer::HighLand => 0x1F,
            Layer::Sea => 0x20,
            Layer::SeaWind => 0xD5,
            Layer::HellLowLand => 0xC7,
            Layer::HellShore => 0xC8,
            Layer::HellHighLand => 0xC9,
            Layer::HellSea => 0xCA,
        }
    }
}

/// Wildlife calls, by theme.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Wilds {
    Default,
    /// Theme 22.
    Theme22,
    /// Theme 30, hell.
    Hell,
}

impl Wilds {
    pub const ALL: [Wilds; 3] = [Wilds::Default, Wilds::Theme22, Wilds::Hell];

    pub fn id(self) -> u8 {
        match self {
            Wilds::Default => 0x1C,
            Wilds::Theme22 => 0xC2,
            Wilds::Hell => 0xC6,
        }
    }

    pub fn of(theme: Option<u8>) -> Wilds {
        match theme {
            Some(22) => Wilds::Theme22,
            Some(HELL) => Wilds::Hell,
            _ => Wilds::Default,
        }
    }
}

const HELL: u8 = 30;
const SEA_WIND: u8 = 16;

/// The four layers of a theme: low land, shore, high land, sea.
pub fn layers(theme: Option<u8>) -> [Layer; 4] {
    match theme {
        Some(HELL) => [Layer::HellLowLand, Layer::HellShore, Layer::HellHighLand, Layer::HellSea],
        Some(SEA_WIND) => [Layer::LowLand, Layer::Shore, Layer::HighLand, Layer::SeaWind],
        _ => [Layer::LowLand, Layer::Shore, Layer::HighLand, Layer::Sea],
    }
}

/// Heights over which land is high (wind).
pub const HIGH_LAND: u16 = 0x200;

/// Half the side of the square of cells around the camera's look-at point that counts as "drawn near the camera".
/// Ours: the original counts what its renderer just drew near the camera.
pub const NEAR: i32 = 12;

/// The ground near the camera, cells counted.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Ground {
    pub low: u32,
    pub high: u32,
    pub water: u32,
}

impl Ground {
    pub fn near(&self) -> u32 {
        self.low + self.high + self.water
    }
}

/// The cells within `NEAR` of `at` (cells).
pub fn ground(terrain: &Heightmap, at: Vec2) -> Ground {
    let (cx, cz) = (at.x.floor() as i32, at.y.floor() as i32);
    let mut g = Ground::default();
    for z in cz - NEAR..=cz + NEAR {
        for x in cx - NEAR..=cx + NEAR {
            match terrain.get(x, z) {
                h if h == game_core::terrain::SEA_LEVEL => g.water += 1,
                h if h > HIGH_LAND => g.high += 1,
                _ => g.low += 1,
            }
        }
    }
    g
}

/// Each layer's weight, 0..=255: low land, high land and sea by their share of the near cells, the shore twice the
/// lesser of low land and sea (ours: as a share too, the original's counts are its renderer's).
pub fn weights(g: Ground, theme: Option<u8>) -> [(Layer, u32); 4] {
    let near = g.near().max(1);
    let share = |n: u32| (n * 256 / near).min(255);
    let [low, shore, high, sea] = layers(theme);
    [(low, share(g.low)), (shore, share(2 * g.low.min(g.water))), (high, share(g.high)), (sea, share(g.water))]
}

/// The bed: the three heaviest layers with any weight, heaviest first.
pub fn bed(weights: [(Layer, u32); 4]) -> Vec<(Layer, u32)> {
    let mut w: Vec<(Layer, u32)> = weights.into_iter().filter(|&(_, w)| w > 0).collect();
    w.sort_by_key(|&(l, w)| (std::cmp::Reverse(w), l));
    w.truncate(3);
    w
}

/// Odds, 1 in this many original frames, of seagulls when every near cell is sea (not in hell), and of a wildlife
/// call while trees are near.
pub const GULL_ODDS: u64 = 256;
pub const WILDLIFE_ODDS: u64 = 128;
/// The original's frame rate the odds above are counted in. Ours: its frame rate is not in the docs.
pub const ORIGINAL_FPS: f32 = 30.0;

/// Whether a 1-in-`odds`-per-original-frame roll comes up over `secs` of ours, from a uniform `draw`.
pub fn rolled(odds: u64, secs: f32, draw: u64) -> bool {
    let chance = 1.0 - (1.0 - 1.0 / odds as f64).powf((secs * ORIGINAL_FPS) as f64);
    (draw as f64 / u64::MAX as f64) < chance
}

fn key(layer: Layer) -> LoopKey {
    LoopKey(7 << 32 | layer.id() as u64)
}

fn ambience(map: Res<CurrentMap>, listener: Res<crate::audio::ListenerState>, time: Res<Time>, mut kept: Local<Kept>, mut draws: Local<u64>, mut plays: MessageWriter<PlayLoop>, mut stops: MessageWriter<StopLoop>, mut sounds: MessageWriter<PlaySound>) {
    let map = &map.0;
    let focus = listener.0.focus;
    let g = ground(&map.terrain, focus);
    let loops: Vec<PlayLoop> = bed(weights(g, map.theme)).into_iter().map(|(layer, w)| looping(key(layer), SoundCue::Bed(layer), LoopKind::Variety, None, w as f32 / 255.0)).collect();
    kept.update(loops, &mut plays, &mut stops);
    let secs = time.delta_secs();
    let mut draw = || {
        *draws += 1;
        roll(*draws, 0xA3B1)
    };
    if g.water == g.near() && map.theme != Some(HELL) && rolled(GULL_ODDS, secs, draw()) {
        sounds.write(PlaySound::flat(SoundCue::Gulls));
    }
    let trees = map.trees.iter().any(|t| {
        let d = cells(t.x, t.z) - focus;
        let size = map.terrain.size() as f32;
        let wrap = |v: f32| (v + size / 2.0).rem_euclid(size) - size / 2.0;
        wrap(d.x).abs() <= NEAR as f32 && wrap(d.y).abs() <= NEAR as f32
    });
    if trees && rolled(WILDLIFE_ODDS, secs, draw()) {
        sounds.write(PlaySound::flat(SoundCue::Wildlife(Wilds::of(map.theme))));
    }
}

pub struct AudioAmbiencePlugin;

impl Plugin for AudioAmbiencePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, ambience.in_set(crate::menu::Gameplay));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_theme_picks_the_layers() {
        assert_eq!(layers(None)[3], Layer::Sea);
        assert_eq!(layers(Some(16))[3], Layer::SeaWind);
        assert_eq!(layers(Some(16))[0], Layer::LowLand);
        assert_eq!(layers(Some(30)), [Layer::HellLowLand, Layer::HellShore, Layer::HellHighLand, Layer::HellSea]);
        assert_eq!(Wilds::of(Some(22)).id(), 0xC2);
        assert_eq!(Wilds::of(Some(3)).id(), 0x1C);
    }

    #[test]
    fn the_ground_is_counted_around_the_camera_wrapping() {
        let mut terrain = Heightmap::new(64);
        terrain.set(0, 0, 100);
        terrain.set(63, 0, 900);
        let g = ground(&terrain, Vec2::new(0.5, 0.5));
        assert_eq!(g, Ground { low: 1, high: 1, water: 25 * 25 - 2 });
    }

    #[test]
    fn the_bed_is_the_three_heaviest_layers() {
        let g = Ground { low: 50, high: 10, water: 40 };
        let w = weights(g, None);
        assert_eq!(w, [(Layer::LowLand, 128), (Layer::Shore, 204), (Layer::HighLand, 25), (Layer::Sea, 102)]);
        assert_eq!(bed(w), vec![(Layer::Shore, 204), (Layer::LowLand, 128), (Layer::Sea, 102)]);
        let open_sea = weights(Ground { low: 0, high: 0, water: 9 }, None);
        assert_eq!(bed(open_sea), vec![(Layer::Sea, 255)], "no weight, no layer");
    }

    #[test]
    fn a_roll_comes_up_at_its_odds() {
        let hits = (0..100_000u64).filter(|&i| rolled(128, 1.0 / ORIGINAL_FPS, roll(i, 1))).count();
        assert!((600..960).contains(&hits), "{hits} in 100000, about 781");
        assert!(!rolled(128, 0.0, 0));
    }
}
