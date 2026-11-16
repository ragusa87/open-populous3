//! The splash where someone goes into the sea, or a drowned spirit rises out of it (`EffectKind::Splash`,
//! pop3-rev-analysis.md "Drowning splash"): standing on the sea's surface as drawn
//! (`ground_material::drawn_water_lift`) for the effect's 16 turns. With the original files, the original's
//! camera-facing animation (`hfx0-0.dat`, `catalog::SPLASH_SPRITES`, 16 frames over its life: the rate is ours, the
//! original steps it on its untraced draw clock). Without them (`--no-original`), our own look (`[ours]`): a patch
//! of white foam, droplets thrown up and falling back, and rings spreading on the water.

use crate::camera::CameraRig;
use crate::grounded::{render_pos, Grounded};
use crate::sim_time::{seed, SimNow, TICK_SECS};
use crate::units::art::{picture_frame, Frame};
use crate::units::{toward_eye, upload_frames, FrameAsset, PULL_TO_EYE};
use crate::views::{sync_views, WorldView};
use crate::world::{CurrentMap, LevelList};
use bevy::ecs::system::SystemParamItem;
use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use game_core::effect::{Effect, EffectKind};
use game_core::id::Id;
use game_core::map::GameMap;
use game_core::time::Tick;
use pop3_format::catalog::{EFFECT_SPRITE_FILE, SPLASH_SPRITES};
use pop3_format::{Picture, SpriteBank, Theme, WORLD_UNITS_PER_CELL};
use std::path::Path;

/// The effect's life, seconds (`game_core::splash::TICKS`).
const LIFE_SECS: f32 = game_core::splash::TICKS.get() as f32 * TICK_SECS;

/// Which of `frames` original frames shows `secs` into the splash: evenly over its life, the last held.
pub fn splash_frame(secs: f32, frames: usize) -> usize {
    (((secs / LIFE_SECS).clamp(0.0, 1.0) * frames as f32) as usize).min(frames.saturating_sub(1))
}

/// The row `sprite`'s drawn pixels are centred on, if any.
pub fn middle_row(sprite: &pop3_format::Sprite) -> Option<usize> {
    let rows: Vec<usize> = (0..sprite.height).filter(|&y| (0..sprite.width).any(|x| sprite.pixels[y * sprite.width + x].is_some())).collect();
    Some((rows.first()? + rows.last()?) / 2)
}

/// The original's frames, all anchored on the water where the first one's impact is drawn (`middle_row`: `[ours]`,
/// the original's anchor is not traced; later frames spread from there, up as streaks and down as the ripple
/// coming towards the viewer).
fn original_frames(data_dir: &Path) -> Result<Vec<Frame>, String> {
    let err = |e: pop3_format::LevelError| e.to_string();
    let bank = SpriteBank::load(data_dir, EFFECT_SPRITE_FILE).map_err(err)?;
    let palette = Theme::load(data_dir, 0).map_err(err)?.palette;
    let sprite = |i: usize| bank.sprites.get(i).ok_or(format!("no sprite {i} in {EFFECT_SPRITE_FILE}"));
    let first = sprite(*SPLASH_SPRITES.start())?;
    let impact = middle_row(first).unwrap_or(first.height / 2);
    SPLASH_SPRITES
        .map(|i| {
            let sprite = sprite(i)?;
            Ok(picture_frame(&Picture { sprite: sprite.clone(), origin: (sprite.width / 2, impact) }, &palette))
        })
        .collect()
}

const WATER: Color = Color::srgb(0.85, 0.93, 1.0);
const DROPLETS: usize = 20;
/// How long a droplet flies (seconds), how high it goes and how far out it lands (cells).
const DROP_SECS: f32 = 0.7;
const DROP_HEIGHT: f32 = 0.8;
const DROP_REACH: (f32, f32) = (0.2, 0.6);
const DROP_SIZE: f32 = 0.035;
/// A ring spreads to `RING_REACH` cells over `RING_SECS`, a new one every `RING_EVERY` seconds.
const RING_SECS: f32 = 0.9;
const RING_EVERY: f32 = 0.4;
const RING_REACH: f32 = 0.8;
const RINGS: usize = 3;
const RING_ALPHA: f32 = 0.75;
/// The foam where it went in spreads to `FOAM_REACH` cells over `FOAM_SECS`, fading.
const FOAM_SECS: f32 = 0.8;
const FOAM_REACH: f32 = 0.35;

/// The foam's radius (cells) and opacity `secs` after it went in; None once gone.
pub fn foam(secs: f32) -> Option<(f32, f32)> {
    let t = secs / FOAM_SECS;
    (0.0..1.0).contains(&t).then(|| (FOAM_REACH * (0.3 + 0.7 * t.sqrt()), 0.9 * (1.0 - t)))
}

/// Where droplet `k` of a splash seeded `id` is, `secs` after it went in (cells, from its centre on the water), and
/// how opaque; None once it has fallen back.
pub fn droplet(id: u32, k: usize, secs: f32) -> Option<(Vec3, f32)> {
    let t = secs / DROP_SECS;
    if !(0.0..1.0).contains(&t) {
        return None;
    }
    let angle = (k as f32 + seed(id.wrapping_add(k as u32))) / DROPLETS as f32 * std::f32::consts::TAU;
    let reach = DROP_REACH.0 + (DROP_REACH.1 - DROP_REACH.0) * seed(id.wrapping_mul(31).wrapping_add(k as u32));
    let up = DROP_HEIGHT * (0.6 + 0.4 * seed(id.wrapping_mul(7).wrapping_add(k as u32)));
    let at = Vec3::new(angle.cos(), 0.0, angle.sin()) * reach * t + Vec3::Y * up * 4.0 * t * (1.0 - t);
    Some((at, 1.0 - t * t))
}

/// Ring `k`'s radius (cells) and opacity `secs` after it went in, while it is still `drowning`; None before it
/// starts or once faded.
pub fn ring(k: usize, secs: f32) -> Option<(f32, f32)> {
    let t = (secs - k as f32 * RING_EVERY) / RING_SECS;
    (0.0..1.0).contains(&t).then(|| (RING_REACH * t.sqrt(), (1.0 - t) * RING_ALPHA))
}

#[derive(Resource)]
pub struct SplashAssets {
    /// The original's frames; empty without the original files, then ours is drawn.
    original: Vec<FrameAsset>,
    drop: Handle<Mesh>,
    ring: Handle<Mesh>,
    foam: Handle<Mesh>,
}

/// What one splash is drawn with.
enum Look {
    Original { sprite: Entity },
    Ours { drops: Vec<(Entity, Handle<StandardMaterial>)>, rings: Vec<(Entity, Handle<StandardMaterial>)>, foam: (Entity, Handle<StandardMaterial>) },
}

/// One splash.
#[derive(Component)]
pub struct SplashView {
    look: Look,
}

impl WorldView for SplashView {
    type Model = (Id<Effect>, Effect);
    type Params = (
        Res<'static, CurrentMap>,
        Res<'static, SplashAssets>,
        ResMut<'static, Assets<StandardMaterial>>,
        Query<'static, 'static, (&'static mut Transform, &'static mut Visibility, Option<&'static mut Mesh3d>, Option<&'static mut MeshMaterial3d<StandardMaterial>>), Without<crate::camera::GameCamera>>,
        (Res<'static, crate::world::TerrainMaterial>, Res<'static, Assets<crate::ground_material::GroundMaterial>>, Res<'static, Assets<Image>>, Res<'static, crate::camera::CurveParamsRes>, Res<'static, CameraRig>),
        Query<'static, 'static, &'static Transform, With<crate::camera::GameCamera>>,
    );

    fn models(map: &GameMap) -> impl Iterator<Item = &(Id<Effect>, Effect)> {
        map.effects.entries().filter(|(_, e)| e.kind == EffectKind::Splash)
    }

    fn id((id, _): &(Id<Effect>, Effect)) -> u32 {
        id.raw()
    }

    fn spawn((_, e): &(Id<Effect>, Effect), view: &mut EntityCommands, (_, assets, mats, ..): &mut SystemParamItem<Self::Params>) -> Option<Self> {
        let cell = WORLD_UNITS_PER_CELL as f32;
        view.insert((Grounded { at: Vec2::new(e.x as f32, e.z as f32) / cell, half: 0.0 }, Transform::default(), Visibility::Visible));
        if let Some(first) = assets.original.first() {
            let mut sprite = Entity::PLACEHOLDER;
            view.with_children(|v| {
                sprite = v.spawn((Mesh3d(first.mesh.clone()), MeshMaterial3d(first.material.clone()), NotShadowCaster, crate::hover::NoOutline, Transform::default(), Visibility::Hidden)).id();
            });
            return Some(SplashView { look: Look::Original { sprite } });
        }
        let mut part = |v: &mut ChildSpawnerCommands, mesh: &Handle<Mesh>| {
            let m = mats.add(StandardMaterial { base_color: WATER, alpha_mode: AlphaMode::Blend, unlit: true, ..default() });
            (v.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(m.clone()), NotShadowCaster, crate::hover::NoOutline, Transform::default(), Visibility::Hidden)).id(), m)
        };
        let (mut drops, mut rings, mut foam) = (Vec::new(), Vec::new(), None);
        view.with_children(|v| {
            drops = (0..DROPLETS).map(|_| part(v, &assets.drop)).collect();
            rings = (0..RINGS).map(|_| part(v, &assets.ring)).collect();
            foam = Some(part(v, &assets.foam));
        });
        Some(SplashView { look: Look::Ours { drops, rings, foam: foam? } })
    }

    fn update(&self, (id, e): &(Id<Effect>, Effect), now: SimNow, (map, assets, mats, parts, (ground, grounds, images, curve, rig), eye): &mut SystemParamItem<Self::Params>) {
        let start = Tick::from_wire(map.0.now.to_wire().wrapping_sub(e.left.done().get()));
        let secs = now.secs_since(start).max(0.0);
        let at = Vec2::new(e.x as f32, e.z as f32) / WORLD_UNITS_PER_CELL as f32;
        let surface = Vec3::Y * crate::ground_material::drawn_water_lift(grounds, images, &ground.0, &map.0.terrain, at) * curve.0.height_scale;
        let (drops, rings, foam_part) = match &self.look {
            Look::Original { sprite } => {
                // Pulled towards the eye like the units it splashes around (`units::toward_eye`).
                let feet = render_pos(&map.0.terrain, &Grounded { at, half: 0.0 }, rig.focus, &curve.0).map(|f| f + surface);
                let (pull, scale) = feet.zip(eye.single().ok()).map_or((Vec3::ZERO, 1.0), |(f, eye)| toward_eye(f, eye.translation, PULL_TO_EYE));
                if let Ok((mut t, mut vis, mesh, material)) = parts.get_mut(*sprite) {
                    let f = &assets.original[splash_frame(secs, assets.original.len())];
                    (t.translation, t.rotation, t.scale, *vis) = (surface + pull, Quat::from_rotation_y(rig.yaw) * Quat::from_rotation_x(-rig.pitch), Vec3::splat(scale), Visibility::Inherited);
                    if let (Some(mut mesh), Some(mut material)) = (mesh, material) {
                        (mesh.0, material.0) = (f.mesh.clone(), f.material.clone());
                    }
                }
                return;
            }
            Look::Ours { drops, rings, foam } => (drops, rings, foam),
        };
        let mut show = |(e, m): &(Entity, Handle<StandardMaterial>), pose: Option<(Transform, f32)>| {
            if let Ok((mut t, mut vis, ..)) = parts.get_mut(*e) {
                match pose {
                    Some((pose, alpha)) => {
                        (*t, *vis) = (pose, Visibility::Inherited);
                        if let Some(mut mat) = mats.get_mut(m) {
                            mat.base_color = WATER.with_alpha(alpha);
                        }
                    }
                    None => *vis = Visibility::Hidden,
                }
            }
        };
        for (k, part) in drops.iter().enumerate() {
            show(part, droplet(id.raw(), k, secs).map(|(at, a)| (Transform::from_translation(surface + at).with_scale(Vec3::splat(DROP_SIZE)), a)));
        }
        for (k, part) in rings.iter().enumerate() {
            show(part, ring(k, secs).map(|(r, a)| (Transform::from_translation(surface + Vec3::Y * 0.01).with_scale(Vec3::new(r, 0.15, r)), a)));
        }
        show(foam_part, foam(secs).map(|(r, a)| (Transform::from_translation(surface + Vec3::Y * 0.005).with_scale(Vec3::new(r, 1.0, r)), a)));
    }
}

pub struct SplashPlugin;

impl Plugin for SplashPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_assets).add_systems(Update, sync_views::<SplashView>.after(crate::units::run_ticks));
    }
}

fn load_assets(levels: Res<LevelList>, mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut images: ResMut<Assets<Image>>, mut mats: ResMut<Assets<StandardMaterial>>) {
    let frames = levels.original.then(|| original_frames(&levels.data_dir).map_err(|e| warn!("original splash: {e}")).ok()).flatten().unwrap_or_default();
    let original = upload_frames(&frames, &mut images, &mut meshes, &mut mats);
    commands.insert_resource(SplashAssets { original, drop: meshes.add(Sphere::new(1.0).mesh().ico(1).unwrap()), ring: meshes.add(Torus::new(0.88, 1.0)), foam: meshes.add(Cylinder::new(1.0, 0.01)) });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn droplets_fly_up_and_out_then_fall_back_and_fade() {
        let (start, a0) = droplet(7, 3, 0.0).unwrap();
        assert_eq!((start, a0), (Vec3::ZERO, 1.0), "from its centre on the water");
        let (mid, a) = droplet(7, 3, DROP_SECS / 2.0).unwrap();
        assert!(mid.y > 0.0 && mid.xz().length() > 0.0 && a < 1.0);
        assert!(droplet(7, 3, DROP_SECS).is_none(), "fallen back");
        assert_ne!(droplet(7, 3, 0.2), droplet(7, 4, 0.2), "each its own way");
    }

    #[test]
    fn rings_spread_and_fade_one_after_another() {
        assert_eq!(ring(0, 0.0), Some((0.0, RING_ALPHA)));
        assert!(ring(1, 0.1).is_none(), "the next one not yet");
        let (r, a) = ring(1, RING_EVERY + RING_SECS / 2.0).unwrap();
        assert!(r > 0.0 && r < RING_REACH && a < RING_ALPHA);
        assert!(ring(0, RING_SECS).is_none(), "faded");
    }

    #[test]
    fn the_foam_spreads_and_fades_where_it_went_in() {
        let (r0, a0) = foam(0.0).unwrap();
        let (r1, a1) = foam(FOAM_SECS / 2.0).unwrap();
        assert!(r1 > r0 && a1 < a0 && r1 < FOAM_REACH);
        assert!(foam(FOAM_SECS).is_none());
    }

    #[test]
    fn a_frame_is_anchored_on_the_middle_of_what_it_draws() {
        let mut pixels = vec![None; 4 * 10];
        pixels[4 * 5 + 1] = Some(3);
        pixels[4 * 7 + 2] = Some(3);
        let sprite = pop3_format::Sprite { width: 4, height: 10, pixels };
        assert_eq!(middle_row(&sprite), Some(6));
        assert_eq!(middle_row(&pop3_format::Sprite { width: 2, height: 2, pixels: vec![None; 4] }), None);
    }

    #[test]
    fn the_original_frames_play_once_over_its_life() {
        assert_eq!(splash_frame(0.0, 16), 0);
        assert_eq!(splash_frame(LIFE_SECS / 2.0, 16), 8);
        assert_eq!(splash_frame(LIFE_SECS - 0.001, 16), 15);
        assert_eq!(splash_frame(LIFE_SECS * 2.0, 16), 15, "the last held");
        assert_eq!(splash_frame(0.5, 0), 0, "none loaded");
    }
}
