//! The Angel of Death's own cast (`game_core::angel_of_death::Cast`, `EffectKind::AngelOfDeath`): an invisible
//! meteor falls from height 2048 to the ground over `FALL_TICKS`, trailing a shower of glowing spark particles
//! around it (pop3-rev-analysis.md "Angel of Death mechanics": 36 "orbiter" billboard sparks, the same renderer
//! family as `swarm.rs`'s own bee-dot billboard) — the meteor itself has no sprite of its own, only this trail
//! and the ground shockwave it ends in (a real terrain ripple, drawn by the terrain mesh redrawing itself, not
//! here). Sparks scatter around the height the meteor was at when each was born rather than following its
//! current position (`[ours]`, the exact particle-by-particle choreography isn't traced further than "a
//! trailing shower"), each with its own staggered lifetime so the trail reads as continuous rather than one
//! burst.

use crate::camera::{CameraRig, CurveParamsRes, GameCamera};
use crate::grounded::{render_pos, Grounded};
use crate::sim_time::{seed, SimNow};
use crate::units::toward_eye;
use crate::views::{sync_views, WorldView};
use crate::world::CurrentMap;
use bevy::ecs::system::SystemParamItem;
use bevy::prelude::*;
use game_core::angel_of_death::FALL_TICKS;
use game_core::effect::{Effect, EffectKind};
use game_core::id::Id;
use game_core::map::GameMap;
use game_core::time::{Tick, TICKS_PER_SECOND};
use pop3_format::WORLD_UNITS_PER_CELL;

/// How many sparks trail the fall (pop3-rev-analysis.md: "36 real, visible orbiter spark particles").
const SPARK_COUNT: u32 = 36;
/// How high the meteor falls from, world units (pop3-rev-analysis.md: "spawns at height 2048").
const FALL_HEIGHT: f32 = 2048.0;
/// A spark's own lifetime, seconds: short enough that many are visible at once, staggered.
const SPARK_LIFE: f32 = 0.35;
/// How far out from the meteor's own line a spark drifts, cells.
const SCATTER: f32 = 0.22;
const DOT_SIZE: f32 = 0.16;
const PULL_TO_EYE: f32 = 0.1;
const TEXTURE: usize = 16;
const COLOUR: Color = Color::srgb(1.0, 0.85, 0.45);

/// Total fall time, seconds.
pub fn fall_secs() -> f32 {
    FALL_TICKS.get() as f32 / TICKS_PER_SECOND as f32
}

/// Soft round glowing dot, white so the material's own tint colours it.
pub fn spark_texture(size: usize) -> Vec<u8> {
    let c = (size as f32 - 1.0) / 2.0;
    (0..size * size)
        .flat_map(|i| {
            let (x, y) = ((i % size) as f32 - c, (i / size) as f32 - c);
            let r = ((x * x + y * y).sqrt() / c).min(1.0);
            [255, 255, 255, ((1.0 - r * r) * 255.0) as u8]
        })
        .collect()
}

/// Where spark `k` (seeded) sits relative to the meteor's own fall line and how visible it is, `age` seconds
/// into the cast: None before it is born or once its own short life has run out. Each is born at its own point
/// along the fall (`born`, 0 to `total`) and stays where the meteor was then, drifting a little further out and
/// fading in then out over `SPARK_LIFE` — a trailing shower around an otherwise invisible falling point, not a
/// particle following it down.
pub fn spark_pose(age: f32, total: f32, seed: f32, heading: f32) -> Option<(Vec3, f32)> {
    let born = seed * total;
    let life = age - born;
    if !(0.0..=SPARK_LIFE).contains(&life) {
        return None;
    }
    let p = life / SPARK_LIFE;
    let height = FALL_HEIGHT * (1.0 - born / total).max(0.0);
    let radius = SCATTER * (0.4 + 0.6 * p);
    let (sin, cos) = heading.sin_cos();
    let offset = Vec3::new(cos * radius, height / WORLD_UNITS_PER_CELL as f32, sin * radius);
    let opacity = (1.0 - (p * 2.0 - 1.0).powi(2)).max(0.0);
    Some((offset, opacity))
}

#[derive(Component, Clone, Copy)]
struct Spark {
    effect: Id<Effect>,
    seed: f32,
    heading: f32,
}

#[derive(Resource)]
pub struct MeteorAssets {
    quad: Handle<Mesh>,
    texture: Handle<Image>,
}

/// One spark's own material (not shared: each fades on its own, `pose_sparks` animates its opacity alone).
fn spark_material(assets: &MeteorAssets, mats: &mut Assets<StandardMaterial>) -> Handle<StandardMaterial> {
    mats.add(StandardMaterial {
        base_color_texture: Some(assets.texture.clone()),
        base_color: COLOUR,
        emissive: LinearRgba::from(COLOUR) * 2.0,
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        cull_mode: None,
        ..default()
    })
}

#[derive(Component)]
pub struct AngelMeteorView;

impl WorldView for AngelMeteorView {
    type Model = (Id<Effect>, Effect);
    type Params = (Res<'static, CurrentMap>, Res<'static, MeteorAssets>, ResMut<'static, Assets<StandardMaterial>>);

    fn models(map: &GameMap) -> impl Iterator<Item = &(Id<Effect>, Effect)> {
        map.effects.entries().filter(|(_, e)| matches!(e.kind, EffectKind::AngelOfDeath(_)))
    }

    fn id((id, _): &(Id<Effect>, Effect)) -> u32 {
        id.raw()
    }

    fn spawn((id, e): &(Id<Effect>, Effect), view: &mut EntityCommands, (_, assets, mats): &mut SystemParamItem<Self::Params>) -> Option<Self> {
        let raw = id.raw();
        view.insert((Transform::default(), Visibility::Hidden));
        view.with_children(|v| {
            for k in 0..SPARK_COUNT {
                let s = |salt: u32| seed(raw.wrapping_mul(131).wrapping_add(k.wrapping_mul(197)).wrapping_add(salt));
                let spark = Spark { effect: *id, seed: s(1), heading: s(2) * std::f32::consts::TAU };
                v.spawn((spark, Grounded { at: Vec2::new(e.x as f32, e.z as f32) / WORLD_UNITS_PER_CELL as f32, half: 0.0 }, Transform::default(), Visibility::Hidden, crate::object_scale::Scaled(crate::object_scale::ObjectGroup::Scenery), crate::hover::NoOutline))
                    .with_child((Mesh3d(assets.quad.clone()), MeshMaterial3d(spark_material(assets, mats)), Transform::default()));
            }
        });
        Some(AngelMeteorView)
    }

    /// `pose_sparks` (`PostUpdate`, after the ground is placed) does the per-frame motion, same reason as
    /// `swarm.rs`'s own no-op `update`: it needs the camera alongside a `Query` on the sparks' own children.
    fn update(&self, _model: &(Id<Effect>, Effect), _now: SimNow, _params: &mut SystemParamItem<Self::Params>) {}
}

fn load_assets(mut commands: Commands, mut images: ResMut<Assets<Image>>, mut meshes: ResMut<Assets<Mesh>>) {
    use bevy::asset::RenderAssetUsages;
    use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
    let image = Image::new(
        Extent3d { width: TEXTURE as u32, height: TEXTURE as u32, depth_or_array_layers: 1 },
        TextureDimension::D2,
        spark_texture(TEXTURE),
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    commands.insert_resource(MeteorAssets { quad: meshes.add(Rectangle::new(DOT_SIZE, DOT_SIZE)), texture: images.add(image) });
}

/// Every spark's own drift and fade around its meteor's fall line, facing the camera; hidden before it is
/// born, once its own life ends, or once its cast is gone.
fn pose_sparks(
    map: Res<CurrentMap>,
    now: Res<SimNow>,
    rig: Res<CameraRig>,
    params: Res<CurveParamsRes>,
    eye: Query<&Transform, (With<GameCamera>, Without<Spark>)>,
    mut sparks: Query<(&Spark, &mut Grounded, &mut Transform, &mut Visibility, &Children)>,
    mut bodies: Query<(&mut Transform, &mut MeshMaterial3d<StandardMaterial>), (Without<Spark>, Without<GameCamera>)>,
    mut mats: ResMut<Assets<StandardMaterial>>,
) {
    let Ok(eye) = eye.single() else { return };
    let facing = Quat::from_rotation_y(rig.yaw) * Quat::from_rotation_x(-rig.pitch);
    let total = fall_secs();
    for (spark, mut grounded, mut root, mut vis, children) in &mut sparks {
        let Some(e) = map.0.effects.get(spark.effect) else {
            *vis = Visibility::Hidden;
            continue;
        };
        let start = Tick::from_wire(map.0.now.to_wire().wrapping_sub(e.left.done().get()));
        let age = now.secs_since(start);
        let Some((offset, opacity)) = spark_pose(age, total, spark.seed, spark.heading) else {
            *vis = Visibility::Hidden;
            continue;
        };
        *vis = Visibility::Visible;
        grounded.at = Vec2::new(e.x as f32, e.z as f32) / WORLD_UNITS_PER_CELL as f32;
        let Some(feet) = render_pos(&map.0.terrain, &grounded, rig.focus, &params.0) else {
            *vis = Visibility::Hidden;
            continue;
        };
        root.translation = feet;
        let (pull, scale) = toward_eye(feet + offset, eye.translation, PULL_TO_EYE);
        for &child in children {
            if let Ok((mut t, mat)) = bodies.get_mut(child) {
                t.translation = facing.inverse() * pull;
                t.rotation = facing;
                t.scale = Vec3::splat(scale);
                if let Some(mut m) = mats.get_mut(&mat.0).filter(|m| (m.base_color.alpha() - opacity).abs() > 1e-3) {
                    m.base_color = COLOUR.with_alpha(opacity);
                }
            }
        }
    }
}

pub struct AngelMeteorPlugin;

impl Plugin for AngelMeteorPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_assets)
            .add_systems(Update, sync_views::<AngelMeteorView>.after(crate::units::run_ticks))
            .add_systems(PostUpdate, pose_sparks.after(crate::grounded::place_grounded).before(TransformSystems::Propagate));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_spark_is_born_along_the_fall_and_fades_in_then_out_over_its_own_short_life() {
        let total = fall_secs();
        assert!(spark_pose(-0.01, total, 0.5, 0.0).is_none(), "before it's born");
        assert!(spark_pose(total * 0.5 + SPARK_LIFE + 0.01, total, 0.5, 0.0).is_none(), "well after its own life ended");
        let (_, mid_opacity) = spark_pose(total * 0.5 + SPARK_LIFE / 2.0, total, 0.5, 0.0).unwrap();
        let (_, start_opacity) = spark_pose(total * 0.5, total, 0.5, 0.0).unwrap();
        assert!(mid_opacity > start_opacity, "brightest in the middle of its life: {mid_opacity} vs {start_opacity}");
    }

    #[test]
    fn a_later_born_spark_sits_lower_along_the_fall() {
        let total = fall_secs();
        let (early, _) = spark_pose(0.0, total, 0.0, 0.0).unwrap();
        let (late, _) = spark_pose(total * 0.9, total, 0.9, 0.0).unwrap();
        assert!(late.y < early.y, "born later: the meteor had already fallen further, {late:?} vs {early:?}");
    }

    #[test]
    fn soft_glow_dot() {
        let tex = spark_texture(16);
        let alpha = |x: usize, y: usize| tex[(y * 16 + x) * 4 + 3];
        assert!(alpha(8, 8) > 200 && alpha(0, 0) == 0);
    }
}
