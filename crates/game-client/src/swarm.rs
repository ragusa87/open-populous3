//! Swarm (insect plague, `game_core::swarm`): the cloud itself is invisible to the simulation (no terrain
//! change, no drawable body of its own — same convention as `volcano_rocks.rs`'s own "nothing for the
//! simulation to do"), so everything here is drawn from the effect's id and its live wandering position
//! alone: `BEE_COUNT` small camera-facing dots scattered around it, about 1 in 16 tinted the caster's
//! tribe colour (pop3-rev-analysis.md "Insect plague": "roughly one spawned bee-dot in sixteen"; the exact
//! neutral tint for the rest was never decoded, so that colour is our own guess, `[ours]`), trickling in
//! over the first turns of the effect's life rather than popping in all at once (`volcano_rocks.rs`'s own
//! `Visibility::Hidden`-until-born idiom, standing in for the trace's own "one new bee-dot every tick" —
//! a reasoned simplification, not a re-trace, since tracking 60 individually-timed dots buys nothing here).

use crate::camera::{CameraRig, CurveParamsRes, GameCamera};
use crate::grounded::{render_pos, Grounded};
use crate::sim_time::{seed, SimNow};
use crate::sites::tribe_color;
use crate::units::toward_eye;
use crate::views::{sync_views, WorldView};
use crate::world::CurrentMap;
use bevy::ecs::system::SystemParamItem;
use bevy::prelude::*;
use game_core::effect::{Effect, EffectKind};
use game_core::id::Id;
use game_core::map::GameMap;
use game_core::time::{Tick, TICKS_PER_SECOND};
use pop3_format::WORLD_UNITS_PER_CELL;

/// "A few bees", per the ask.
const BEE_COUNT: u32 = 8;
/// Spread over this many of the effect's own first ticks so they trickle in (our own stand-in, see module doc).
const SPAWN_SPREAD_TICKS: u32 = 30;
/// Chance (per bee) of the caster's own tribe colour rather than the shared neutral tint.
const TRIBE_CHANCE: f32 = 1.0 / 16.0;
const NEUTRAL: Color = Color::srgb(0.85, 0.82, 0.3);
/// How far out from the cloud's live centre a bee buzzes, cells.
const RADIUS: f32 = 0.6;
/// Buzzing speed, radians/sec, two phase-shifted wobbles per bee.
const BUZZ_SPEED: f32 = 5.5;
const BOB_SPEED: f32 = 8.0;
const BOB_HEIGHT: f32 = 0.08;
/// Dot size, cells, and how far it's pulled towards the camera (`units::toward_eye`, `dust.rs`'s own idiom).
const DOT_SIZE: f32 = 0.09;
const PULL_TO_EYE: f32 = 0.15;
const TEXTURE: usize = 16;

/// Soft round dot, opaque in the middle and fading to the edge (`dust.rs::dust_texture`, plain white so a
/// bee's own tint comes entirely from its material's `base_color`).
pub fn bee_texture(size: usize) -> Vec<u8> {
    let c = (size as f32 - 1.0) / 2.0;
    (0..size * size)
        .flat_map(|i| {
            let (x, y) = ((i % size) as f32 - c, (i / size) as f32 - c);
            let r = ((x * x + y * y).sqrt() / c).min(1.0);
            [255, 255, 255, ((1.0 - r) * (1.0 - r) * 255.0) as u8]
        })
        .collect()
}

/// A bee's own buzzing offset (cells, around the cloud's centre) and bob (render units) at `age` seconds
/// into the effect's life: a pure function of `(age, seed)`, matching every other world visual.
fn buzz(age: f32, seed: f32) -> (Vec2, f32) {
    let phase = seed * std::f32::consts::TAU;
    let amp = RADIUS * (0.5 + 0.5 * seed);
    let angle = age * BUZZ_SPEED + phase;
    let offset = Vec2::new(angle.cos(), angle.sin()) * amp;
    let bob = BOB_HEIGHT + (age * BOB_SPEED + phase * 2.0).sin() * BOB_HEIGHT;
    (offset, bob)
}

/// Whether the bee born at `born` seconds is showing yet at `age`.
fn hatched(age: f32, born: f32) -> bool {
    age >= born
}

#[derive(Component, Clone, Copy)]
struct Bee {
    effect: Id<Effect>,
    born: f32,
    seed: f32,
}

#[derive(Resource)]
pub struct SwarmAssets {
    quad: Handle<Mesh>,
    texture: Handle<Image>,
}

#[derive(Component)]
pub struct SwarmView;

impl WorldView for SwarmView {
    type Model = (Id<Effect>, Effect);
    type Params = (Res<'static, CurrentMap>, Res<'static, SwarmAssets>, ResMut<'static, Assets<StandardMaterial>>);

    fn models(map: &GameMap) -> impl Iterator<Item = &(Id<Effect>, Effect)> {
        map.effects.entries().filter(|(_, e)| matches!(e.kind, EffectKind::Swarm(_)))
    }

    fn id((id, _): &(Id<Effect>, Effect)) -> u32 {
        id.raw()
    }

    fn spawn((id, e): &(Id<Effect>, Effect), view: &mut EntityCommands, (_, assets, mats): &mut SystemParamItem<Self::Params>) -> Option<Self> {
        let raw = id.raw();
        let colour = tribe_color(e.owner);
        view.insert((Transform::default(), Visibility::Hidden));
        view.with_children(|v| {
            for k in 0..BEE_COUNT {
                let s = |salt: u32| seed(raw.wrapping_mul(97).wrapping_add(k.wrapping_mul(181)).wrapping_add(salt));
                let tribe_coloured = s(1) < TRIBE_CHANCE;
                let material = mats.add(StandardMaterial {
                    base_color_texture: Some(assets.texture.clone()),
                    base_color: if tribe_coloured { colour } else { NEUTRAL },
                    alpha_mode: AlphaMode::Blend,
                    unlit: true,
                    cull_mode: None,
                    ..default()
                });
                let bee = Bee { effect: *id, born: (s(2) * SPAWN_SPREAD_TICKS as f32) / TICKS_PER_SECOND as f32, seed: s(3) };
                v.spawn((bee, Grounded { at: Vec2::new(e.x as f32, e.z as f32) / WORLD_UNITS_PER_CELL as f32, half: 0.0 }, Transform::default(), Visibility::Hidden, crate::object_scale::Scaled(crate::object_scale::ObjectGroup::Scenery), crate::hover::NoOutline))
                    .with_child((Mesh3d(assets.quad.clone()), MeshMaterial3d(material), Transform::default()));
            }
        });
        Some(SwarmView)
    }

    /// `pose_bees` (`PostUpdate`, after the ground is placed) does the per-frame motion: it needs the
    /// camera together with a `Query` on the bees in the same pass, which this trait's `Params` does not
    /// give it alongside the view's children (`volcano_rocks.rs`'s own `update` is the same no-op).
    fn update(&self, _model: &(Id<Effect>, Effect), _now: SimNow, _params: &mut SystemParamItem<Self::Params>) {}
}

fn load_assets(mut commands: Commands, mut images: ResMut<Assets<Image>>, mut meshes: ResMut<Assets<Mesh>>) {
    use bevy::asset::RenderAssetUsages;
    use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
    let texture = Image::new(
        Extent3d { width: TEXTURE as u32, height: TEXTURE as u32, depth_or_array_layers: 1 },
        TextureDimension::D2,
        bee_texture(TEXTURE),
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    commands.insert_resource(SwarmAssets { quad: meshes.add(Rectangle::new(DOT_SIZE, DOT_SIZE)), texture: images.add(texture) });
}

/// Every bee's buzzing motion around its swarm's live wandering position, facing the camera; hidden
/// before it's born or once its swarm is gone.
fn pose_bees(
    map: Res<CurrentMap>,
    now: Res<SimNow>,
    rig: Res<CameraRig>,
    params: Res<CurveParamsRes>,
    eye: Query<&Transform, (With<GameCamera>, Without<Bee>)>,
    mut bees: Query<(&Bee, &mut Grounded, &mut Transform, &mut Visibility, &Children)>,
    mut bodies: Query<&mut Transform, (Without<Bee>, Without<GameCamera>)>,
) {
    let Ok(eye) = eye.single() else { return };
    let facing = Quat::from_rotation_y(rig.yaw) * Quat::from_rotation_x(-rig.pitch);
    for (bee, mut grounded, mut root, mut vis, children) in &mut bees {
        let Some(e) = map.0.effects.get(bee.effect) else {
            *vis = Visibility::Hidden;
            continue;
        };
        let EffectKind::Swarm(swarm) = &e.kind else {
            *vis = Visibility::Hidden;
            continue;
        };
        let start = Tick::from_wire(map.0.now.to_wire().wrapping_sub(e.left.done().get()));
        let age = now.secs_since(start);
        if !hatched(age, bee.born) {
            *vis = Visibility::Hidden;
            continue;
        }
        *vis = Visibility::Visible;
        let (x, z) = swarm.pos();
        let centre = Vec2::new(x as f32, z as f32) / WORLD_UNITS_PER_CELL as f32;
        let (offset, bob) = buzz(age - bee.born, bee.seed);
        grounded.at = centre + offset;
        let Some(feet) = render_pos(&map.0.terrain, &grounded, rig.focus, &params.0) else {
            *vis = Visibility::Hidden;
            continue;
        };
        root.translation = feet;
        let (pull, scale) = toward_eye(feet + Vec3::Y * bob, eye.translation, PULL_TO_EYE);
        for &child in children {
            if let Ok(mut t) = bodies.get_mut(child) {
                t.translation = facing.inverse() * pull;
                t.rotation = facing;
                t.scale = Vec3::splat(scale);
            }
        }
    }
}

pub struct SwarmPlugin;

impl Plugin for SwarmPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_assets)
            .add_systems(Update, sync_views::<SwarmView>.after(crate::units::run_ticks))
            .add_systems(PostUpdate, pose_bees.after(crate::grounded::place_grounded).before(TransformSystems::Propagate));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bee_dot_is_soft_and_white() {
        let tex = bee_texture(16);
        let alpha = |x: usize, y: usize| tex[(y * 16 + x) * 4 + 3];
        assert!(alpha(8, 8) > 200 && alpha(0, 0) == 0 && alpha(8, 8) > alpha(12, 8) && alpha(12, 8) > 0);
        assert_eq!(&tex[(8 * 16 + 8) * 4..(8 * 16 + 8) * 4 + 3], &[255, 255, 255]);
    }

    #[test]
    fn hatching_respects_born_time() {
        assert!(!hatched(0.0, 1.0));
        assert!(hatched(1.0, 1.0));
        assert!(hatched(2.0, 1.0));
    }

    #[test]
    fn buzz_is_a_pure_function_of_age_and_seed() {
        assert_eq!(buzz(1.5, 0.3), buzz(1.5, 0.3));
        assert_ne!(buzz(1.5, 0.3), buzz(1.5, 0.7), "differs by seed");
        assert_ne!(buzz(1.5, 0.3), buzz(2.0, 0.3), "differs by age");
    }

    #[test]
    fn buzz_stays_within_its_radius() {
        for i in 0..200 {
            let (offset, _) = buzz(i as f32 * 0.37, (i as f32 * 0.123).fract());
            assert!(offset.length() <= RADIUS + 1e-3, "{offset} too far out");
        }
    }

    #[test]
    fn tribe_colour_roll_lands_around_one_in_sixteen() {
        let hits = (0..4096u32).filter(|&raw| seed(raw.wrapping_mul(97).wrapping_add(3u32.wrapping_mul(181)).wrapping_add(1)) < TRIBE_CHANCE).count();
        let rate = hits as f32 / 4096.0;
        assert!((rate - TRIBE_CHANCE).abs() < 0.03, "rate {rate} should be close to 1/16");
    }
}
