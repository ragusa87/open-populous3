//! A puff of dust where a unit touches down after floating (teleport landing): the original
//! blended dust cloud (`hfx0-0.dat` 1225-1239, through the alpha table) when allowed, else soft
//! motes spreading out around the feet, rising a little, growing and fading. Cosmetic only, on the
//! real-time clock.

use super::art::alpha_picture_frame;
use super::{toward_eye, upload_frame, FrameAsset, UnitView, PULL_TO_EYE};
use crate::camera::{CameraRig, CurveParamsRes, GameCamera};
use crate::grounded::{render_pos, Grounded};
use crate::world::{CurrentMap, LevelList};
use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use game_core::unit::Action;
use pop3_format::blend::AlphaTable;
use pop3_format::catalog::EFFECT_SPRITE_FILE;
use pop3_format::{Picture, SpriteBank, Theme};
use std::f32::consts::TAU;
use std::path::Path;

/// Seconds a puff lasts.
pub const LIFE: f32 = 0.8;
const MOTES: usize = 10;
/// How far the motes spread from the feet, and how high they rise (cells).
const SPREAD: f32 = 0.4;
const RISE: f32 = 0.1;
/// Mote size (cells) at birth and at the end.
const SIZE: (f32, f32) = (0.12, 0.3);
const START_ALPHA: f32 = 0.9;
const DUST: [u8; 3] = [222, 206, 172];
const TEXTURE: usize = 32;
/// The original dust cloud, growing then fading (`hfx0-0.dat`, blended).
const ORIGINAL_CLOUD: std::ops::RangeInclusive<usize> = 1225..=1239;

/// Which of `frames` frames of the original cloud shows `age` seconds after touchdown.
pub fn cloud_frame(age: f32, frames: usize) -> usize {
    (((age / LIFE).clamp(0.0, 1.0) * frames as f32) as usize).min(frames.saturating_sub(1))
}

/// The original cloud's frames, around the feet (two thirds down), blended over the background.
fn original_cloud(data_dir: &Path) -> Result<Vec<super::art::Frame>, String> {
    let err = |e: pop3_format::LevelError| e.to_string();
    let bank = SpriteBank::load(data_dir, EFFECT_SPRITE_FILE).map_err(err)?;
    let rgba = AlphaTable::load(data_dir, 0).map_err(err)?.rgba(&Theme::load(data_dir, 0).map_err(err)?.palette);
    ORIGINAL_CLOUD
        .map(|i| {
            let sprite = bank.sprites.get(i).ok_or(format!("no sprite {i} in {EFFECT_SPRITE_FILE}"))?;
            let origin = (sprite.width / 2, sprite.height * 2 / 3);
            Ok(alpha_picture_frame(&Picture { sprite: sprite.clone(), origin }, &rgba))
        })
        .collect()
}

/// One mote `k` of `n`, `age` seconds after touchdown: offset from the feet (world axes, cells),
/// size (cells) and opacity. Motes spread evenly around, fast at first then slowing down.
pub fn mote(k: usize, n: usize, age: f32) -> (Vec3, f32, f32) {
    let t = (age / LIFE).clamp(0.0, 1.0);
    let out = 1.0 - (1.0 - t) * (1.0 - t);
    let angle = k as f32 / n as f32 * TAU + 0.3;
    let r = 0.05 + (SPREAD - 0.05) * out;
    let offset = Vec3::new(angle.cos() * r, 0.02 + RISE * out, angle.sin() * r);
    (offset, SIZE.0 + (SIZE.1 - SIZE.0) * out, START_ALPHA * (1.0 - t))
}

/// Soft round dot, opaque in the middle and fading to the edge (RGBA, `size` x `size`).
pub fn dust_texture(size: usize) -> Vec<u8> {
    let c = (size as f32 - 1.0) / 2.0;
    (0..size * size)
        .flat_map(|i| {
            let (x, y) = ((i % size) as f32 - c, (i / size) as f32 - c);
            let r = ((x * x + y * y).sqrt() / c).min(1.0);
            [DUST[0], DUST[1], DUST[2], ((1.0 - r) * (1.0 - r) * 255.0) as u8]
        })
        .collect()
}

/// Whether each unit touched down this frame: it was landing and no longer is (alive).
pub fn touchdowns(was_landing: &[bool], actions: &[Action]) -> Vec<bool> {
    actions
        .iter()
        .enumerate()
        .map(|(i, a)| was_landing.get(i).copied().unwrap_or(false) && !matches!(a, Action::Landing { .. } | Action::Dying { .. } | Action::Dead { .. }))
        .collect()
}

#[derive(Resource)]
struct DustAssets {
    quad: Handle<Mesh>,
    texture: Handle<Image>,
    /// The original cloud's frames, when allowed and found.
    cloud: Vec<FrameAsset>,
}

#[derive(Component)]
struct Puff {
    born: f32,
}

#[derive(Component)]
struct Mote(usize);

/// The original cloud sprite of a puff.
#[derive(Component)]
struct Cloud;

pub struct DustPlugin;

impl Plugin for DustPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_assets)
            .add_systems(Update, spawn_puffs.after(super::run_ticks).in_set(crate::menu::Gameplay))
            .add_systems(PostUpdate, animate_puffs.before(TransformSystems::Propagate));
    }
}

fn load_assets(levels: Res<LevelList>, mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut images: ResMut<Assets<Image>>, mut mats: ResMut<Assets<StandardMaterial>>) {
    let frames = levels.original.then(|| original_cloud(&levels.data_dir).map_err(|e| warn!("original dust cloud: {e}")).ok()).flatten().unwrap_or_default();
    let cloud = frames.iter().map(|f| upload_frame(f, &mut images, &mut meshes, &mut mats)).collect();
    let texture = Image::new(
        Extent3d { width: TEXTURE as u32, height: TEXTURE as u32, depth_or_array_layers: 1 },
        TextureDimension::D2,
        dust_texture(TEXTURE),
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    commands.insert_resource(DustAssets { quad: meshes.add(Rectangle::new(1.0, 1.0)), texture: images.add(texture), cloud });
}

/// A puff at each unit that touched down since the last frame.
fn spawn_puffs(
    mut commands: Commands,
    map: Res<CurrentMap>,
    time: Res<Time>,
    clock: Res<super::SimClock>,
    assets: Res<DustAssets>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut was_landing: Local<Vec<bool>>,
) {
    let actions: Vec<Action> = map.0.units.iter().map(|u| u.action).collect();
    for (i, down) in touchdowns(&was_landing, &actions).into_iter().enumerate() {
        if !down {
            continue;
        }
        let at = clock.cell_pos(i, &map.0.units[i]);
        let puff = (Puff { born: time.elapsed_secs() }, Grounded { at, half: 0.0 }, Transform::default(), Visibility::Hidden);
        if let Some(first) = assets.cloud.first() {
            commands.spawn(puff).with_child((Cloud, Mesh3d(first.mesh.clone()), MeshMaterial3d(first.material.clone()), Transform::default()));
            continue;
        }
        let material = mats.add(StandardMaterial {
            base_color_texture: Some(assets.texture.clone()),
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            cull_mode: None,
            ..default()
        });
        commands.spawn(puff).with_children(|p| {
            for k in 0..MOTES {
                p.spawn((Mote(k), Mesh3d(assets.quad.clone()), MeshMaterial3d(material.clone()), Transform::default()));
            }
        });
    }
    *was_landing = actions.iter().map(|a| matches!(a, Action::Landing { .. })).collect();
}

/// Moves, grows and fades the motes, facing the camera and pulled towards it like the units
/// (`toward_eye`); a puff is removed once faded.
#[allow(clippy::too_many_arguments)]
fn animate_puffs(
    mut commands: Commands,
    time: Res<Time>,
    map: Res<CurrentMap>,
    rig: Res<CameraRig>,
    params: Res<CurveParamsRes>,
    eye: Query<&Transform, (With<GameCamera>, Without<Puff>, Without<Mote>, Without<Cloud>)>,
    mut puffs: Query<(Entity, &Puff, &Grounded, &mut Transform, &Children), (Without<Mote>, Without<Cloud>, Without<UnitView>)>,
    mut motes: Query<(&Mote, &mut Transform, &MeshMaterial3d<StandardMaterial>), (Without<Puff>, Without<Cloud>)>,
    mut clouds: Query<(&mut Transform, &mut Mesh3d, &mut MeshMaterial3d<StandardMaterial>), (With<Cloud>, Without<Puff>)>,
    assets: Res<DustAssets>,
    mut mats: ResMut<Assets<StandardMaterial>>,
) {
    let Ok(eye) = eye.single() else { return };
    let facing = Quat::from_rotation_y(rig.yaw) * Quat::from_rotation_x(-rig.pitch);
    for (entity, puff, ground, mut t, children) in &mut puffs {
        let age = time.elapsed_secs() - puff.born;
        if age > LIFE {
            commands.entity(entity).despawn();
            continue;
        }
        t.rotation = facing;
        let Some(feet) = render_pos(&map.0.terrain, ground, rig.focus, &params.0) else { continue };
        let (pull, scale) = toward_eye(feet, eye.translation, PULL_TO_EYE);
        for &child in children {
            if let Ok((mut ct, mut mesh, mut material)) = clouds.get_mut(child) {
                let f = &assets.cloud[cloud_frame(age, assets.cloud.len())];
                mesh.0 = f.mesh.clone();
                material.0 = f.material.clone();
                ct.translation = facing.inverse() * pull;
                ct.scale = Vec3::splat(scale);
                continue;
            }
            let Ok((m, mut mt, material)) = motes.get_mut(child) else { continue };
            let (offset, size, alpha) = mote(m.0, MOTES, age);
            mt.translation = facing.inverse() * (pull + offset * scale);
            mt.scale = Vec3::splat(size * scale);
            if let Some(mut mat) = mats.get_mut(&material.0) {
                mat.base_color = Color::srgba(1.0, 1.0, 1.0, alpha);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn motes_spread_rise_grow_and_fade() {
        let (start, s0, a0) = mote(0, 8, 0.0);
        let (mid, s1, a1) = mote(0, 8, LIFE / 2.0);
        let (end, s2, a2) = mote(0, 8, LIFE);
        let flat = |v: Vec3| v.xz().length();
        assert!(flat(start) < flat(mid) && flat(mid) < flat(end) && (flat(end) - SPREAD).abs() < 1e-5);
        assert!(start.y < mid.y && mid.y <= end.y);
        assert!(s0 < s1 && s1 < s2);
        assert!(a0 > a1 && a1 > a2 && a2 == 0.0);
        assert!(flat(mid) - flat(start) > flat(end) - flat(mid), "fast first, then slowing");
        let around: Vec3 = (0..8).map(|k| mote(k, 8, LIFE).0).sum();
        assert!(around.xz().length() < 1e-4, "evenly spread around the feet");
    }

    #[test]
    fn cloud_plays_once_over_the_puff() {
        assert_eq!(cloud_frame(0.0, 15), 0);
        assert_eq!(cloud_frame(LIFE / 2.0, 15), 7);
        assert_eq!(cloud_frame(LIFE, 15), 14);
        assert_eq!(cloud_frame(LIFE * 2.0, 15), 14);
        assert_eq!(cloud_frame(0.3, 0), 0);
    }

    #[test]
    fn dust_dot_is_soft() {
        let tex = dust_texture(32);
        let alpha = |x: usize, y: usize| tex[(y * 32 + x) * 4 + 3];
        assert!(alpha(16, 16) > 200 && alpha(0, 0) == 0 && alpha(16, 16) > alpha(24, 16) && alpha(24, 16) > 0);
    }

    #[test]
    fn puffs_where_landing_ends() {
        let landing = Action::Landing { left: 1 };
        let was = [true, true, false, true];
        let now = [Action::Idle, landing, Action::Idle, Action::Dying { left: 8 }];
        assert_eq!(touchdowns(&was, &now), vec![true, false, false, false]);
        assert_eq!(touchdowns(&[], &[Action::Idle]), vec![false], "new units");
    }
}
