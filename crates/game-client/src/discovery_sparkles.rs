//! The discovery cinematic's sparkles (pop3-rev-analysis.md "Discovery cinematic: the reward flies to the panel",
//! "Sparkle burst"): a burst of 200 sparks where the reward starts, and a twinkle on its tile once it heads there.
//! On screen (UI), on the cinematic's own frames (`discovery`): they stop while paused. The burst's sparks are the
//! original's alpha sprites (hfx 1294-1299, one in 16 hfx 1288-1293) on a random frame of 6, each tinted a random
//! tribe's colour; the twinkle cycles hfx 1288-1293. Without the original files, a star of ours. No trails (sprites
//! 318-321, bank not identified).

use crate::discovery::frame_of;
use crate::hud::PANEL_WIDTH;
use crate::sim_time::{SimNow, TICK_SECS};
use crate::units::art::alpha_picture_frame;
use crate::units::PLAYER;
use crate::world::{CurrentMap, LevelList};
use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use game_core::building::Reward;
use game_core::discovery::flight;
use game_core::map::Lcg;
use game_core::time::Tick;
use game_core::tribes::TribeId;
use pop3_format::blend::AlphaTable;
use pop3_format::catalog::EFFECT_SPRITE_FILE;
use pop3_format::{Picture, SpriteBank, Theme};
use std::path::Path;

/// The sparks of a burst, their speeds (original pixels a frame), spread about the heading, and how many go a frame
/// [exe].
const SPARKS: u32 = 200;
const SPEED_MIN: u32 = 14;
const SPEED_SPAN: u32 = 30;
const SPREAD: f32 = std::f32::consts::FRAC_PI_4;
const GONE_PER_FRAME: u32 = 10;
/// The twinkle on the tile lasts this many frames [exe].
const TWINKLE_FRAMES: f32 = 100.0;
const SPARK_SPRITES: std::ops::RangeInclusive<usize> = 1294..=1299;
const TWINKLE_SPRITES: std::ops::RangeInclusive<usize> = 1288..=1293;
/// The original's screen height, for its pixel speeds and sizes.
const ORIGINAL_HEIGHT: f32 = 480.0;
/// Our star's size (pixels) without the original files.
const STAR: usize = 15;

/// One spark of a burst: where it goes (unit direction), its speed (original pixels a frame), its first sprite
/// frame, whether it is one of the twinkle's sprites, its tribe tint, and the frame it goes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spark {
    pub dir: Vec2,
    pub speed: f32,
    pub sprite: usize,
    pub twinkle: bool,
    pub tint: TribeId,
    pub gone: f32,
}

/// The burst's sparks, seeded by `seed`, about `heading` (radians, screen: y down).
pub fn sparks(seed: u32, heading: f32) -> Vec<Spark> {
    let mut rng = Lcg(seed);
    (0..SPARKS)
        .map(|k| {
            let turn = (rng.next_u32() % 1000) as f32 / 999.0 * 2.0 - 1.0;
            let speed = (SPEED_MIN + rng.next_u32() % SPEED_SPAN) as f32;
            let sprite = (rng.next_u32() % 6) as usize;
            let twinkle = rng.next_u32().is_multiple_of(16);
            let tint = TribeId::ALL[(rng.next_u32() % 4) as usize];
            Spark { dir: Vec2::from_angle(heading + turn * SPREAD), speed, sprite, twinkle, tint, gone: (k / GONE_PER_FRAME + 1) as f32 }
        })
        .collect()
}

/// Where spark `s` is `frame` frames after leaving `from` (`px`: one original pixel), None once gone.
pub fn spark_at(s: &Spark, frame: f32, from: Vec2, px: f32) -> Option<Vec2> {
    (frame >= 0.0 && frame < s.gone).then(|| from + s.dir * s.speed * frame * px)
}

/// The frames a discovery's cinematic counts, the frame its twinkle starts at, seconds a frame; None without one.
fn timing(reward: Reward) -> Option<(u32, f32, f32)> {
    let (frames, twinkle) = match reward {
        Reward::Building(_) => (crate::discovery_building::BUILDING_FRAMES, crate::discovery_building::TWINKLE_FROM),
        Reward::Spell(_) | Reward::OneShot(_) => (crate::discovery::SPELL_FRAMES, crate::discovery::TWINKLE_FROM),
        _ => return None,
    };
    Some((frames, twinkle as f32, flight(reward)?.get() as f32 * TICK_SECS / frames as f32))
}

/// The sprites (as UI images): the sparks' six frames and the twinkle's six; the same star for both without the
/// original files.
#[derive(Resource, Default)]
struct SparkImages {
    sparks: Vec<Handle<Image>>,
    twinkle: Vec<Handle<Image>>,
}

fn image(width: usize, height: usize, rgba: Vec<u8>) -> Image {
    Image::new(Extent3d { width: width as u32, height: height as u32, depth_or_array_layers: 1 }, TextureDimension::D2, rgba, TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::default())
}

/// Our four-pointed star, white, fading out from its middle.
pub fn star_rgba(size: usize) -> Vec<u8> {
    let c = (size as f32 - 1.0) / 2.0;
    (0..size * size)
        .flat_map(|i| {
            let (x, y) = (((i % size) as f32 - c).abs() / c, ((i / size) as f32 - c).abs() / c);
            let a = ((1.0 - (x * y).sqrt() * 4.0 - (x + y) * 0.35).clamp(0.0, 1.0) * 255.0) as u8;
            [255, 255, 255, a]
        })
        .collect()
}

fn original_images(data_dir: &Path, images: &mut Assets<Image>) -> Result<SparkImages, String> {
    let err = |e: pop3_format::LevelError| e.to_string();
    let bank = SpriteBank::load(data_dir, EFFECT_SPRITE_FILE).map_err(err)?;
    let palette = Theme::load(data_dir, 0).map_err(err)?.palette;
    let lut = AlphaTable::load(data_dir, 0).map_err(err)?.rgba(&palette);
    let mut load = |range: std::ops::RangeInclusive<usize>| {
        range
            .map(|i| {
                let s = bank.sprites.get(i).ok_or(format!("no sprite {i} in {EFFECT_SPRITE_FILE}"))?;
                let f = alpha_picture_frame(&Picture { sprite: s.clone(), origin: (s.width / 2, s.height / 2) }, &lut);
                Ok(images.add(image(f.width, f.height, f.rgba)))
            })
            .collect::<Result<Vec<_>, String>>()
    };
    Ok(SparkImages { sparks: load(SPARK_SPRITES)?, twinkle: load(TWINKLE_SPRITES)? })
}

fn load_images(levels: Res<LevelList>, mut images: ResMut<Assets<Image>>, mut commands: Commands) {
    let original = levels.original.then(|| original_images(&levels.data_dir, &mut images).map_err(|e| warn!("discovery sparkles: {e}")).ok()).flatten();
    commands.insert_resource(original.unwrap_or_else(|| {
        let star = images.add(image(STAR, STAR, star_rgba(STAR)));
        SparkImages { sparks: vec![star.clone(); 6], twinkle: vec![star; 6] }
    }));
}

/// A discovery's sparkles on screen: its burst (from where, which way, its sparks' entities) and its twinkle's
/// entity, timed from the cinematic's start; they outlive the discovery's landing (the twinkle runs 100 frames).
#[derive(Component)]
struct Sparkles {
    granted: Tick,
    reward: Reward,
    started: Tick,
    secs_per_frame: f32,
    twinkle_from: f32,
    from: Vec2,
    sparks: Vec<(Entity, Spark)>,
    twinkle: Entity,
}

pub struct DiscoverySparklesPlugin;

impl Plugin for DiscoverySparklesPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_images).add_systems(Update, (start, draw).chain().after(crate::units::SimStep));
    }
}

fn spark_node(image: Handle<Image>, tint: Color, z: i32) -> impl Bundle {
    (ImageNode::new(image).with_color(tint), Node { position_type: PositionType::Absolute, display: Display::None, ..default() }, GlobalZIndex(z), Pickable::IGNORE)
}

fn start(mut commands: Commands, map: Res<CurrentMap>, now: Res<SimNow>, shown: Query<&Sparkles>, starts: crate::discovery::StartPoints, images: Option<Res<SparkImages>>, camera: Query<&Camera, With<crate::camera::GameCamera>>) {
    let Some(images) = images else { return };
    let Some(size) = camera.single().ok().and_then(|c| c.logical_viewport_size()) else { return };
    let middle = Vec2::new((PANEL_WIDTH + size.x) / 2.0, size.y / 2.0);
    for d in map.0.discoveries.iter().filter(|d| d.owner == PLAYER) {
        let Some((frames, twinkle_from, secs_per_frame)) = timing(d.reward) else { continue };
        if frame_of(d, *now, frames).is_none() || shown.iter().any(|s| s.granted == d.granted && s.reward == d.reward) {
            continue;
        }
        let Some(from) = starts.of(d) else { continue };
        let heading = (middle - from).to_angle();
        let sparks = sparks(d.granted.to_wire() ^ 0x5EED, if heading.is_finite() { heading } else { -std::f32::consts::FRAC_PI_2 });
        let sparks = sparks.into_iter().map(|s| {
            let set = if s.twinkle { &images.twinkle } else { &images.sparks };
            (commands.spawn(spark_node(set[s.sprite % set.len()].clone(), crate::sites::tribe_color(s.tint), 48)).id(), s)
        });
        let sparks = sparks.collect();
        let twinkle = commands.spawn(spark_node(images.twinkle[0].clone(), Color::WHITE, 51)).id();
        commands.spawn(Sparkles { granted: d.granted, reward: d.reward, started: d.started(), secs_per_frame, twinkle_from, from, sparks, twinkle });
    }
}

#[allow(clippy::too_many_arguments)]
fn draw(
    mut commands: Commands,
    now: Res<SimNow>,
    shown: Query<(Entity, &Sparkles)>,
    mut nodes: Query<(&mut Node, &mut ImageNode)>,
    images: Option<Res<SparkImages>>,
    spells: crate::hud::spells::SpellTiles,
    buildings: crate::hud::build::BuildTiles,
    camera: Query<&Camera, With<crate::camera::GameCamera>>,
) {
    let Some(images) = images else { return };
    let Some(size) = camera.single().ok().and_then(|c| c.logical_viewport_size()) else { return };
    let px = size.y / ORIGINAL_HEIGHT;
    for (e, s) in &shown {
        let frame = now.secs_since(s.started) / s.secs_per_frame;
        let end = s.twinkle_from + TWINKLE_FRAMES;
        if !(0.0..end).contains(&frame) {
            for &(spark, _) in &s.sparks {
                commands.entity(spark).despawn();
            }
            commands.entity(s.twinkle).despawn();
            commands.entity(e).despawn();
            continue;
        }
        for &(spark, ref sp) in &s.sparks {
            let Ok((mut node, mut img)) = nodes.get_mut(spark) else { continue };
            match spark_at(sp, frame, s.from, px) {
                Some(at) => {
                    let set = if sp.twinkle { &images.twinkle } else { &images.sparks };
                    img.image = set[(sp.sprite + frame as usize) % set.len()].clone();
                    place(&mut node, at, px);
                }
                None => node.display = Display::None,
            }
        }
        let tile = match s.reward {
            Reward::Building(kind) => buildings.centre(kind),
            Reward::Spell(kind) | Reward::OneShot(kind) => spells.centre(kind),
            _ => None,
        };
        if let Ok((mut node, mut img)) = nodes.get_mut(s.twinkle) {
            match tile.filter(|_| frame >= s.twinkle_from) {
                Some(at) => {
                    img.image = images.twinkle[((frame - s.twinkle_from) as usize) % images.twinkle.len()].clone();
                    place(&mut node, at, px);
                }
                None => node.display = Display::None,
            }
        }
    }
}

/// The size sparks are drawn at, original pixels (ours: the sprites' own sizes are not used).
const SIDE: f32 = 16.0;

/// Shows `node` centred on `at`.
fn place(node: &mut Node, at: Vec2, px: f32) {
    let side = SIDE * px;
    node.display = Display::Flex;
    node.width = Val::Px(side);
    node.height = Val::Px(side);
    node.left = Val::Px(at.x - side / 2.0);
    node.top = Val::Px(at.y - side / 2.0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_hundred_sparks_within_45_degrees_ten_gone_a_frame() {
        let all = sparks(3, 0.0);
        assert_eq!(all.len(), 200);
        assert!(all.iter().all(|s| s.dir.to_angle().abs() <= SPREAD + 1e-4 && (14.0..44.0).contains(&s.speed)));
        assert_eq!(all.iter().filter(|s| spark_at(s, 0.5, Vec2::ZERO, 1.0).is_some()).count(), 200);
        assert_eq!(all.iter().filter(|s| spark_at(s, 1.0, Vec2::ZERO, 1.0).is_some()).count(), 190);
        assert_eq!(all.iter().filter(|s| spark_at(s, 20.0, Vec2::ZERO, 1.0).is_some()).count(), 0, "all gone after 20 frames");
        let twinkles = all.iter().filter(|s| s.twinkle).count();
        assert!((3..30).contains(&twinkles), "about one in 16: {twinkles}");
    }

    #[test]
    fn a_spark_flies_straight_at_its_speed() {
        let s = Spark { dir: Vec2::X, speed: 20.0, sprite: 0, twinkle: false, tint: TribeId::Red, gone: 5.0 };
        assert_eq!(spark_at(&s, 2.0, Vec2::new(10.0, 10.0), 1.5), Some(Vec2::new(70.0, 10.0)));
        assert_eq!(spark_at(&s, 5.0, Vec2::ZERO, 1.0), None);
    }

    #[test]
    fn our_star_is_bright_in_the_middle_clear_at_the_corners() {
        let px = star_rgba(15);
        assert_eq!(px.len(), 15 * 15 * 4);
        assert_eq!(px[(7 * 15 + 7) * 4 + 3], 255);
        assert_eq!(px[3], 0);
    }
}
